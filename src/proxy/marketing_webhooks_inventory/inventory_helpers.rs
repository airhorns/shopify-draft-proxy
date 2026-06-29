use super::*;

pub(in crate::proxy) struct InventoryLevelViewState<'a> {
    pub inventory_level_ids: &'a BTreeMap<(String, String), String>,
    pub inactive_levels: &'a BTreeSet<(String, String)>,
    pub quantity_updated_at: &'a BTreeMap<(String, String, String), String>,
    pub locations: Option<&'a BTreeMap<String, Value>>,
}

pub(in crate::proxy) fn inventory_levels_connection_selected_json(
    inventory_item_id: &str,
    levels: &[(String, BTreeMap<String, i64>)],
    view_state: &InventoryLevelViewState<'_>,
    arguments: &BTreeMap<String, ResolvedValue>,
    selections: &[SelectedField],
) -> Value {
    let include_inactive = matches!(
        arguments.get("includeInactive"),
        Some(ResolvedValue::Bool(true))
    );
    let visible_levels = levels
        .iter()
        .filter(|(location_id, _)| {
            include_inactive
                || !view_state
                    .inactive_levels
                    .contains(&(inventory_item_id.to_string(), location_id.clone()))
        })
        .collect::<Vec<_>>();
    let first = resolved_int_field(arguments, "first")
        .and_then(|value| usize::try_from(value).ok())
        .unwrap_or(visible_levels.len());
    let mut fields = serde_json::Map::new();
    for selection in selections {
        let value = match selection.name.as_str() {
            "nodes" => Some(Value::Array(
                visible_levels
                    .iter()
                    .take(first)
                    .map(|(location_id, quantities)| {
                        inventory_level_selected_json(
                            inventory_item_id,
                            location_id,
                            quantities,
                            view_state,
                            &selection.selection,
                        )
                    })
                    .collect(),
            )),
            "pageInfo" => Some(selected_json(&empty_page_info(), &selection.selection)),
            _ => None,
        };
        if let Some(value) = value {
            fields.insert(selection.response_key.clone(), value);
        }
    }
    Value::Object(fields)
}

pub(in crate::proxy) fn inventory_level_selected_json(
    inventory_item_id: &str,
    location_id: &str,
    quantities: &BTreeMap<String, i64>,
    view_state: &InventoryLevelViewState<'_>,
    selections: &[SelectedField],
) -> Value {
    let is_active = !view_state
        .inactive_levels
        .contains(&(inventory_item_id.to_string(), location_id.to_string()));
    let mut fields = serde_json::Map::new();
    for selection in selections {
        let value = match selection.name.as_str() {
            "id" => Some(json!(view_state
                .inventory_level_ids
                .get(&(inventory_item_id.to_string(), location_id.to_string()))
                .cloned()
                .unwrap_or_else(|| inventory_level_id(
                    inventory_item_id,
                    location_id
                )))),
            "isActive" => Some(json!(is_active)),
            "item" => Some(selected_json(
                &json!({ "id": inventory_item_id }),
                &selection.selection,
            )),
            "location" => Some(
                view_state
                    .locations
                    .and_then(|locations| locations.get(location_id))
                    .map(|location| selected_json(location, &selection.selection))
                    .unwrap_or_else(|| {
                        selected_json(
                            &json!({
                                "id": location_id,
                                "name": inventory_location_name(location_id)
                            }),
                            &selection.selection,
                        )
                    }),
            ),
            "quantities" => Some(Value::Array(
                inventory_quantity_names(&selection.arguments)
                    .into_iter()
                    .map(|name| {
                        let updated_at = view_state
                            .quantity_updated_at
                            .get(&(
                                inventory_item_id.to_string(),
                                location_id.to_string(),
                                name.clone(),
                            ))
                            .map_or(Value::Null, |value| json!(value));
                        selected_json(
                            &json!({
                                "name": name,
                                "quantity": quantities.get(&name).copied().unwrap_or(0),
                                "updatedAt": updated_at
                            }),
                            &selection.selection,
                        )
                    })
                    .collect(),
            )),
            _ => None,
        };
        if let Some(value) = value {
            fields.insert(selection.response_key.clone(), value);
        }
    }
    Value::Object(fields)
}

fn inventory_quantity_names(arguments: &BTreeMap<String, ResolvedValue>) -> Vec<String> {
    match arguments.get("names") {
        Some(ResolvedValue::List(values)) => values
            .iter()
            .filter_map(|value| match value {
                ResolvedValue::String(name) => Some(name.clone()),
                _ => None,
            })
            .collect(),
        _ => vec![
            "available".to_string(),
            "on_hand".to_string(),
            "damaged".to_string(),
        ],
    }
}

pub(in crate::proxy) fn inventory_level_id(inventory_item_id: &str, location_id: &str) -> String {
    let level_tail = format!(
        "{}-{}",
        resource_id_tail(inventory_item_id),
        resource_id_tail(location_id)
    );
    format!(
        "{}?inventory_item_id={}",
        shopify_gid("InventoryLevel", level_tail),
        inventory_item_id
    )
}

pub(in crate::proxy) fn inventory_level_id_tail(id: &str) -> Option<&str> {
    shopify_gid_tail_for_type(id, "InventoryLevel")
        .map(|rest| rest.split('?').next().unwrap_or_default())
}

pub(in crate::proxy) fn inventory_level_id_tail_and_query(id: &str) -> Option<(&str, &str)> {
    let rest = shopify_gid_tail_for_type(id, "InventoryLevel")?;
    rest.split_once("?inventory_item_id=")
}

pub(in crate::proxy) fn inventory_level_parts_from_id(id: &str) -> Option<(String, String)> {
    let (level_tail, query) = inventory_level_id_tail_and_query(id)?;
    let (item_tail, location_tail) = level_tail.rsplit_once('-')?;
    let item_id = if query.starts_with("gid://shopify/InventoryItem/") {
        query.to_string()
    } else {
        shopify_gid("InventoryItem", item_tail)
    };
    Some((item_id, shopify_gid("Location", location_tail)))
}

pub(in crate::proxy) fn inventory_properties_json() -> Value {
    json!({
        "quantityNames": [
            {"name": "available", "displayName": "Available", "isInUse": true, "belongsTo": ["on_hand"], "comprises": []},
            {"name": "committed", "displayName": "Committed", "isInUse": true, "belongsTo": ["on_hand"], "comprises": []},
            {"name": "damaged", "displayName": "Damaged", "isInUse": false, "belongsTo": ["on_hand"], "comprises": []},
            {"name": "incoming", "displayName": "Incoming", "isInUse": false, "belongsTo": [], "comprises": []},
            {"name": "on_hand", "displayName": "On hand", "isInUse": true, "belongsTo": [], "comprises": ["available", "committed", "damaged", "quality_control", "reserved", "safety_stock"]},
            {"name": "quality_control", "displayName": "Quality control", "isInUse": false, "belongsTo": ["on_hand"], "comprises": []},
            {"name": "reserved", "displayName": "Reserved", "isInUse": true, "belongsTo": ["on_hand"], "comprises": []},
            {"name": "safety_stock", "displayName": "Safety stock", "isInUse": false, "belongsTo": ["on_hand"], "comprises": []}
        ]
    })
}

pub(in crate::proxy) fn inventory_change_json(
    item_id: &str,
    name: &str,
    delta: i64,
    ledger: Option<&str>,
    location_id: &str,
    location_name: &str,
) -> Value {
    // Real Shopify returns `quantityAfterChange: null` for changes read back
    // from inventoryAdjust/Set/MoveQuantities mutation responses (the field is
    // only populated in certain ledger contexts). Match that to stay faithful to
    // the recorded live captures rather than the staging engine's running total.
    json!({
        "name": name,
        "delta": delta,
        "quantityAfterChange": Value::Null,
        "ledgerDocumentUri": ledger,
        "item": {
            "id": item_id
        },
        "location": {
            "id": location_id,
            "name": location_name
        }
    })
}

fn inventory_set_on_hand_change_json(
    item_id: &str,
    name: &str,
    delta: i64,
    ledger: Option<&str>,
    location_id: &str,
    location_name: &str,
) -> Value {
    json!({
        "name": name,
        "delta": delta,
        "quantityAfterChange": Value::Null,
        "ledgerDocumentUri": ledger
            .map(|value| Value::String(value.to_string()))
            .unwrap_or(Value::Null),
        "item": { "id": item_id },
        "location": {
            "id": location_id,
            "name": location_name
        }
    })
}

pub(in crate::proxy) fn inventory_location_name(_location_id: &str) -> &'static str {
    "Location"
}

const INVENTORY_VALID_REASONS: &[&str] = &[
    "correction",
    "cycle_count_available",
    "damaged",
    "movement_canceled",
    "movement_created",
    "movement_received",
    "movement_updated",
    "other",
    "promotion",
    "quality_control",
    "received",
    "reservation_created",
    "reservation_deleted",
    "reservation_updated",
    "restock",
    "safety_stock",
    "shrinkage",
];
const INVENTORY_PUBLIC_ADJUST_QUANTITY_NAMES: &[&str] = &[
    "available",
    "damaged",
    "incoming",
    "quality_control",
    "reserved",
    "safety_stock",
];
const INVENTORY_SET_QUANTITY_NAMES: &[&str] = &["available", "on_hand"];
const INVENTORY_INVALID_PUBLIC_QUANTITY_NAME_MESSAGE: &str = "The specified quantity name is invalid. Valid values are: available, damaged, incoming, quality_control, reserved, safety_stock.";
const INVENTORY_INVALID_SET_QUANTITY_NAME_MESSAGE: &str =
    "The quantity name must be either 'available' or 'on_hand'.";
const INVENTORY_SET_QUANTITY_MAX: i64 = 1_000_000_000;
const INVENTORY_SET_QUANTITY_MIN: i64 = -1_000_000_000;
const INVENTORY_MAX_ACTIVE_LEVELS: usize = 200;
const INVENTORY_ITEM_WEIGHT_UNITS: &[&str] = &["KILOGRAMS", "GRAMS", "POUNDS", "OUNCES"];
const COMMON_MISSING_INVENTORY_ID_TAILS: &[&str] = &["999999999999", "missing", "unknown"];
const INVENTORY_ITEM_EXTRA_MISSING_ID_TAILS: &[&str] = &["999999999998", "999999999999999"];
const INVENTORY_VALID_COUNTRY_CODES: &[&str] = &[
    "AC", "AD", "AE", "AF", "AG", "AI", "AL", "AM", "AN", "AO", "AR", "AT", "AU", "AW", "AX", "AZ",
    "BA", "BB", "BD", "BE", "BF", "BG", "BH", "BI", "BJ", "BL", "BM", "BN", "BO", "BQ", "BR", "BS",
    "BT", "BV", "BW", "BY", "BZ", "CA", "CC", "CD", "CF", "CG", "CH", "CI", "CK", "CL", "CM", "CN",
    "CO", "CR", "CU", "CV", "CW", "CX", "CY", "CZ", "DE", "DJ", "DK", "DM", "DO", "DZ", "EC", "EE",
    "EG", "EH", "ER", "ES", "ET", "FI", "FJ", "FK", "FO", "FR", "GA", "GB", "GD", "GE", "GF", "GG",
    "GH", "GI", "GL", "GM", "GN", "GP", "GQ", "GR", "GS", "GT", "GW", "GY", "HK", "HM", "HN", "HR",
    "HT", "HU", "ID", "IE", "IL", "IM", "IN", "IO", "IQ", "IR", "IS", "IT", "JE", "JM", "JO", "JP",
    "KE", "KG", "KH", "KI", "KM", "KN", "KP", "KR", "KW", "KY", "KZ", "LA", "LB", "LC", "LI", "LK",
    "LR", "LS", "LT", "LU", "LV", "LY", "MA", "MC", "MD", "ME", "MF", "MG", "MK", "ML", "MM", "MN",
    "MO", "MQ", "MR", "MS", "MT", "MU", "MV", "MW", "MX", "MY", "MZ", "NA", "NC", "NE", "NF", "NG",
    "NI", "NL", "NO", "NP", "NR", "NU", "NZ", "OM", "PA", "PE", "PF", "PG", "PH", "PK", "PL", "PM",
    "PN", "PS", "PT", "PY", "QA", "RE", "RO", "RS", "RU", "RW", "SA", "SB", "SC", "SD", "SE", "SG",
    "SH", "SI", "SJ", "SK", "SL", "SM", "SN", "SO", "SR", "SS", "ST", "SV", "SX", "SY", "SZ", "TA",
    "TC", "TD", "TF", "TG", "TH", "TJ", "TK", "TL", "TM", "TN", "TO", "TR", "TT", "TV", "TW", "TZ",
    "UA", "UG", "UM", "US", "UY", "UZ", "VA", "VC", "VE", "VG", "VN", "VU", "WF", "WS", "XK", "YE",
    "YT", "ZA", "ZM", "ZW",
];
const INVENTORY_TRANSFER_HYDRATE_NODES_QUERY: &str = r#"#graphql
  query ProductsHydrateNodes($ids: [ID!]!) {
    nodes(ids: $ids) {
      __typename
      id
      ... on InventoryItem {
        tracked
        requiresShipping
        measurement { weight { unit value } }
        variant {
          id
          title
          inventoryQuantity
          selectedOptions { name value }
          product {
            id
            title
            handle
            status
            totalInventory
            tracksInventory
          }
        }
        inventoryLevels(first: 50) {
          nodes {
            id
            location { id name }
            quantities(names: ["available", "on_hand", "committed", "incoming", "reserved", "damaged", "quality_control", "safety_stock"]) {
              name
              quantity
              updatedAt
            }
          }
        }
      }
      ... on Location {
        id
        name
        isActive
      }
    }
  }
"#;

impl DraftProxy {
    fn inventory_level_view_state(&self) -> InventoryLevelViewState<'_> {
        InventoryLevelViewState {
            inventory_level_ids: &self.store.staged.inventory_level_ids,
            inactive_levels: &self.store.staged.inactive_inventory_levels,
            quantity_updated_at: &self.store.staged.inventory_quantity_updated_at,
            locations: Some(&self.store.staged.locations.records),
        }
    }
    pub(in crate::proxy) fn inventory_query_data(
        &self,
        fields: &[RootFieldSelection],
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        root_payload_json(fields, |field| {
            Some(match field.name.as_str() {
                "inventoryItems" => self.inventory_items_connection_selected_json(
                    &field.arguments,
                    variables,
                    &field.selection,
                ),
                "inventoryProperties" => {
                    selected_json(&inventory_properties_json(), &field.selection)
                }
                "inventoryItem" => {
                    let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
                    if self.inventory_item_id_is_missing(&id)
                        && !self.inventory_item_has_local_state(&id)
                    {
                        Value::Null
                    } else {
                        self.inventory_item_selected_json(&id, variables, &field.selection)
                    }
                }
                "inventoryLevel" => {
                    let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
                    self.inventory_level_by_id_selected_json(&id, &field.selection)
                }
                "inventoryTransfer" => {
                    let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
                    self.inventory_transfer_by_id_selected_json(&id, &field.selection)
                }
                "inventoryTransfers" => self.inventory_transfers_connection_selected_json(
                    self.store.staged.inventory_transfers.values().collect(),
                    &field.selection,
                ),
                "inventoryShipment" => {
                    let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
                    self.inventory_shipment_by_id_selected_json(&id, &field.selection)
                }
                "product" => {
                    let id = resolved_string_field(&field.arguments, "id")
                        .or_else(|| resolved_string_field(variables, "productId"))
                        .unwrap_or_default();
                    self.inventory_product_selected_json(&id, &field.selection)
                }
                _ => Value::Null,
            })
        })
    }

    fn inventory_items_connection_selected_json(
        &self,
        arguments: &BTreeMap<String, ResolvedValue>,
        variables: &BTreeMap<String, ResolvedValue>,
        selections: &[SelectedField],
    ) -> Value {
        let item_ids = self.inventory_item_ids_for_connection(arguments);
        selected_typed_connection_with_args(
            &item_ids,
            arguments,
            selections,
            |inventory_item_id, node_selection| {
                self.inventory_item_selected_json(inventory_item_id, variables, node_selection)
            },
            |inventory_item_id| inventory_item_id.clone(),
        )
    }

    fn inventory_item_ids_for_connection(
        &self,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) -> Vec<String> {
        let query = resolved_string_field(arguments, "query").unwrap_or_default();
        let mut seen = BTreeSet::new();
        let mut item_ids = Vec::new();

        for variant in effective_records(
            &self.store.base.product_variants,
            &self.store.staged.product_variants,
        ) {
            let inventory_item_id = variant.inventory_item.id;
            if seen.insert(inventory_item_id.clone())
                && self.inventory_item_matches_query(&inventory_item_id, &query)
            {
                item_ids.push(inventory_item_id);
            }
        }

        for (inventory_item_id, _) in &self.store.staged.inventory_level_order {
            if seen.insert(inventory_item_id.clone())
                && self.inventory_item_matches_query(inventory_item_id, &query)
            {
                item_ids.push(inventory_item_id.clone());
            }
        }
        for (inventory_item_id, _) in self.store.staged.inventory_levels.keys() {
            if seen.insert(inventory_item_id.clone())
                && self.inventory_item_matches_query(inventory_item_id, &query)
            {
                item_ids.push(inventory_item_id.clone());
            }
        }

        item_ids
    }

    fn inventory_item_matches_query(&self, inventory_item_id: &str, query: &str) -> bool {
        let query = query.trim();
        if query.is_empty() {
            return true;
        }
        query.split_whitespace().all(|term| {
            let term = term.trim();
            let Some((field, raw_value)) = term.split_once(':') else {
                return true;
            };
            let value = raw_value.trim_matches('"');
            match field {
                "id" => {
                    inventory_item_id == value
                        || resource_id_tail(inventory_item_id).eq_ignore_ascii_case(value)
                }
                "sku" => self
                    .store
                    .product_variant_by_inventory_item_id(inventory_item_id)
                    .map(|variant| variant.sku.eq_ignore_ascii_case(value))
                    .unwrap_or(false),
                "tracked" => match value {
                    "true" => self.inventory_item_tracked(inventory_item_id),
                    "false" => !self.inventory_item_tracked(inventory_item_id),
                    _ => true,
                },
                _ => true,
            }
        })
    }

    fn inventory_item_tracked(&self, inventory_item_id: &str) -> bool {
        self.store
            .product_variant_by_inventory_item_id(inventory_item_id)
            .map(|variant| variant.inventory_item.tracked)
            .unwrap_or(true)
    }

    pub(in crate::proxy) fn inventory_mutation_data(
        &mut self,
        request: &Request,
        fields: &[RootFieldSelection],
    ) -> MutationOutcome {
        let mut log_drafts = Vec::new();
        let mut top_level_response = None;
        let data = root_payload_json(fields, |field| {
            if top_level_response.is_some() {
                return None;
            }
            let outcome = match field.name.as_str() {
                "inventoryAdjustQuantities" => self.inventory_adjust_quantities(request, field),
                "inventorySetQuantities" => self.inventory_set_quantities(request, field),
                "inventorySetOnHandQuantities" => {
                    self.inventory_set_on_hand_quantities(request, field)
                }
                "inventoryMoveQuantities" => self.inventory_move_quantities(request, field),
                "inventoryActivate" => self.inventory_activate(field),
                "inventoryDeactivate" => self.inventory_deactivate(field),
                "inventoryBulkToggleActivation" => self.inventory_bulk_toggle_activation(field),
                "inventoryItemUpdate" => self.inventory_item_update(request, field),
                "inventoryTransferCreate" => self.inventory_transfer_create(field, false),
                "inventoryTransferCreateAsReadyToShip" => {
                    self.inventory_transfer_create(field, true)
                }
                "inventoryTransferMarkAsReadyToShip" => self.inventory_transfer_mark_ready(field),
                "inventoryTransferEdit" => self.inventory_transfer_edit(field),
                "inventoryTransferSetItems" => self.inventory_transfer_set_items(field),
                "inventoryTransferRemoveItems" => self.inventory_transfer_remove_items(field),
                "inventoryTransferDuplicate" => self.inventory_transfer_duplicate(field),
                "inventoryTransferCancel" => self.inventory_transfer_cancel(field),
                "inventoryTransferDelete" => self.inventory_transfer_delete(field),
                "inventoryShipmentCreate" => self.inventory_shipment_create(field, false),
                "inventoryShipmentCreateInTransit" => self.inventory_shipment_create(field, true),
                "inventoryShipmentAddItems" => self.inventory_shipment_add_items(field),
                "inventoryShipmentRemoveItems" => self.inventory_shipment_remove_items(field),
                "inventoryShipmentUpdateItemQuantities" => {
                    self.inventory_shipment_update_item_quantities(field)
                }
                "inventoryShipmentSetTracking" => self.inventory_shipment_set_tracking(field),
                "inventoryShipmentMarkInTransit" => self.inventory_shipment_mark_in_transit(field),
                "inventoryShipmentReceive" => self.inventory_shipment_receive(field),
                "inventoryShipmentDelete" => self.inventory_shipment_delete(field),
                _ => MutationFieldOutcome::unlogged(Value::Null),
            };
            if let Some(errors) = outcome.value.get("__topLevelErrors") {
                top_level_response = Some(MutationOutcome::response(ok_json(json!({
                    "errors": errors,
                    "data": { field.response_key.clone(): Value::Null }
                }))));
                return None;
            }
            if let Some(log_draft) = outcome.log_draft {
                log_drafts.push(log_draft);
            }
            Some(outcome.value)
        });
        if let Some(response) = top_level_response {
            return response;
        }
        MutationOutcome::with_log_drafts(ok_json(json!({ "data": data })), log_drafts)
    }

    fn inventory_item_selected_json(
        &self,
        inventory_item_id: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        selections: &[SelectedField],
    ) -> Value {
        let item_levels = self.inventory_levels_for_item(inventory_item_id);
        let variant = self
            .store
            .product_variant_by_inventory_item_id(inventory_item_id);
        let inventory_quantity = if item_levels.is_empty() {
            variant
                .map(|variant| variant.inventory_quantity)
                .unwrap_or_default()
        } else {
            self.inventory_total(inventory_item_id, "available")
        };
        let variant_for_payload = variant.cloned().map(|mut variant| {
            variant.inventory_quantity = inventory_quantity;
            variant
        });
        let product_id = resolved_string_field(variables, "productId").unwrap_or_default();
        let variant_id = resolved_string_field(variables, "variantId")
            .or_else(|| variant.map(|variant| variant.id.clone()))
            .unwrap_or_else(|| {
                format!(
                    "gid://shopify/ProductVariant/{}",
                    resource_id_tail(inventory_item_id)
                )
            });
        let mut fields = serde_json::Map::new();
        for selection in selections {
            let value = match selection.name.as_str() {
                "id" => Some(json!(inventory_item_id)),
                "tracked" => Some(json!(variant
                    .map(|variant| variant.inventory_item.tracked)
                    .unwrap_or(true))),
                "requiresShipping" => Some(json!(variant
                    .map(|variant| variant.inventory_item.requires_shipping)
                    .unwrap_or(true))),
                "variant" => Some(match variant_for_payload.as_ref() {
                    Some(variant) => product_variant_json(variant, None, &selection.selection),
                    None => selected_json(
                        &json!({
                            "id": variant_id,
                            "inventoryQuantity": inventory_quantity,
                            "product": {
                                "id": product_id,
                                "totalInventory": self.inventory_total_all("available")
                            }
                        }),
                        &selection.selection,
                    ),
                }),
                "locationsCount" => Some(selected_json(
                    &count_object(item_levels.len()),
                    &selection.selection,
                )),
                "inventoryLevel" => {
                    let location_id = resolved_string_field(&selection.arguments, "locationId");
                    let level = location_id.and_then(|location_id| {
                        item_levels.iter().find(|(candidate_location_id, _)| {
                            *candidate_location_id == location_id
                        })
                    });
                    Some(level.map_or(Value::Null, |(location_id, quantities)| {
                        self.inventory_level_json_with_item(
                            inventory_item_id,
                            location_id,
                            quantities,
                            &selection.selection,
                        )
                    }))
                }
                "inventoryLevels" => Some(inventory_levels_connection_selected_json(
                    inventory_item_id,
                    &item_levels,
                    &self.inventory_level_view_state(),
                    &selection.arguments,
                    &selection.selection,
                )),
                _ => variant.and_then(|variant| {
                    variant
                        .inventory_item
                        .extra_fields
                        .get(&selection.name)
                        .map(|value| product_variant_extra_field_json(value, &selection.selection))
                }),
            };
            if let Some(value) = value {
                fields.insert(selection.response_key.clone(), value);
            }
        }
        Value::Object(fields)
    }

    /// Fill `inventory_level_cursors` from real Shopify when a product/variant overlay
    /// read selects `inventoryLevels` edge or pageInfo cursors and none have been
    /// observed yet. The cursor is an opaque, server-assigned token that cannot be
    /// synthesized; the only honest source is the upstream read itself. Forwards the
    /// client's exact request once (LiveHybrid only) and observes the returned edge
    /// cursors. A no-op in Snapshot mode, once cursors are staged, or when the query
    /// does not select level cursors.
    pub(in crate::proxy) fn hydrate_inventory_level_cursors_for_read(
        &mut self,
        request: &Request,
        query: &str,
    ) {
        if self.config.read_mode != ReadMode::LiveHybrid {
            return;
        }
        if !self.store.staged.inventory_level_cursors.is_empty() {
            return;
        }
        if !(query.contains("inventoryLevels") && query.contains("cursor")) {
            return;
        }
        let response = (self.upstream_transport)(request.clone());
        if response.status < 400 {
            self.observe_inventory_level_cursors(&response.body);
        }
    }

    /// Walk an upstream response for every `inventoryLevels { edges { cursor node { id } } }`
    /// connection and stage each level's opaque cursor keyed by its level id, so a later
    /// overlay read of the same connection reproduces the real pagination cursors.
    pub(in crate::proxy) fn observe_inventory_level_cursors(&mut self, body: &Value) {
        fn walk(value: &Value, sink: &mut Vec<(String, String)>) {
            match value {
                Value::Object(map) => {
                    if let Some(edges) = map
                        .get("inventoryLevels")
                        .and_then(|connection| connection.get("edges"))
                        .and_then(Value::as_array)
                    {
                        for edge in edges {
                            let cursor = edge.get("cursor").and_then(Value::as_str);
                            let id = edge
                                .get("node")
                                .and_then(|node| node.get("id"))
                                .and_then(Value::as_str);
                            if let (Some(cursor), Some(id)) = (cursor, id) {
                                sink.push((id.to_string(), cursor.to_string()));
                            }
                        }
                    }
                    for child in map.values() {
                        walk(child, sink);
                    }
                }
                Value::Array(items) => {
                    for item in items {
                        walk(item, sink);
                    }
                }
                _ => {}
            }
        }
        let mut pairs = Vec::new();
        walk(body, &mut pairs);
        for (level_id, cursor) in pairs {
            self.store
                .staged
                .inventory_level_cursors
                .insert(level_id, cursor);
        }
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
                self.observe_inventory_level_node(level);
            }
        }
    }

    pub(in crate::proxy) fn observe_inventory_level_node(&mut self, node: &Value) {
        let Some(level_id) = node.get("id").and_then(Value::as_str) else {
            return;
        };
        let Some((inventory_item_id, parsed_location_id)) =
            self.inventory_level_parts_from_id_or_fallback(level_id)
        else {
            return;
        };
        let location_id = node
            .get("location")
            .and_then(|location| location.get("id"))
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or(parsed_location_id);
        let key = (inventory_item_id.clone(), location_id.clone());
        let quantities = node
            .get("quantities")
            .and_then(Value::as_array)
            .map(|rows| inventory_quantities_from_observed_rows(rows))
            .unwrap_or_else(empty_inventory_quantities);
        self.store
            .staged
            .inventory_levels
            .insert(key.clone(), quantities);
        self.store
            .staged
            .inventory_level_ids
            .insert(key.clone(), level_id.to_string());
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
                        .staged
                        .inventory_quantity_updated_at
                        .insert(timestamp_key, updated_at.to_string());
                } else {
                    self.store
                        .staged
                        .inventory_quantity_updated_at
                        .remove(&timestamp_key);
                }
            }
        }
        if node.get("isActive").and_then(Value::as_bool) == Some(false) {
            self.store.staged.inactive_inventory_levels.insert(key);
        } else {
            self.store.staged.inactive_inventory_levels.remove(&key);
        }
        if let Some(location) = node.get("location") {
            self.stage_observed_inventory_location(location);
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
                    self.observe_inventory_level_node(nested_level);
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
            self.store.stage_observed_product(product);
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
        let variant_record = ProductVariantRecord {
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
        self.store.stage_product_variant(variant_record);
    }

    fn stage_observed_inventory_location(&mut self, location: &Value) {
        self.merge_staged_location(
            location,
            &[("__typename", json!("Location")), ("isActive", json!(true))],
        );
    }

    fn merge_staged_location(&mut self, location: &Value, defaults: &[(&str, Value)]) {
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

    fn inventory_level_by_id_selected_json(&self, id: &str, selections: &[SelectedField]) -> Value {
        let Some((inventory_item_id, location_id)) =
            self.inventory_level_parts_from_id_or_fallback(id)
        else {
            return Value::Null;
        };
        let Some(quantities) = self
            .store
            .staged
            .inventory_levels
            .get(&(inventory_item_id.clone(), location_id.clone()))
        else {
            return Value::Null;
        };
        self.inventory_level_json_with_item(
            &inventory_item_id,
            &location_id,
            quantities,
            selections,
        )
    }

    fn inventory_levels_for_item(
        &self,
        inventory_item_id: &str,
    ) -> Vec<(String, BTreeMap<String, i64>)> {
        // Levels created via local mutations (e.g. inventoryActivate) are surfaced in
        // their creation order, tracked by `inventory_level_order`. Any remaining
        // levels (observed/hydrated from upstream) fall back to the BTreeMap's stable
        // sorted-by-location-id order, which the inventory lifecycle specs depend on.
        let mut levels = Vec::new();
        let mut seen = BTreeSet::new();
        for (item_id, location_id) in &self.store.staged.inventory_level_order {
            if item_id != inventory_item_id || seen.contains(location_id) {
                continue;
            }
            if let Some(quantities) = self
                .store
                .staged
                .inventory_levels
                .get(&(item_id.clone(), location_id.clone()))
            {
                seen.insert(location_id.clone());
                levels.push((location_id.clone(), quantities.clone()));
            }
        }
        levels.extend(
            self.store
                .staged
                .inventory_levels
                .iter()
                .filter(|((item_id, _), _)| item_id == inventory_item_id)
                .filter(|((_, location_id), _)| !seen.contains(location_id))
                .map(|((_, location_id), quantities)| (location_id.clone(), quantities.clone())),
        );
        levels
    }

    /// Build a fully-materialized `inventoryLevels` connection value for an inventory
    /// item from staged level state (ids, locations, quantities, updatedAt timestamps,
    /// and the opaque seeded edge cursors). The result carries `edges`, `nodes`, and
    /// `pageInfo` with every canonical quantity name, so the generic selection
    /// projector can render whatever shape an `inventoryItem.inventoryLevels(...)`
    /// selection asks for. Returns `None` when the item has no staged levels, leaving
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
                .inventory_level_ids
                .get(&key)
                .cloned()
                .unwrap_or_else(|| inventory_level_id(inventory_item_id, location_id));
            let is_active = !view.inactive_levels.contains(&key);
            let location = view
                .locations
                .and_then(|locations| locations.get(location_id))
                .cloned()
                .unwrap_or_else(|| {
                    json!({
                        "id": location_id,
                        "name": inventory_location_name(location_id)
                    })
                });
            let quantities_value: Vec<Value> = CANONICAL
                .iter()
                .map(|name| {
                    let updated_at = view
                        .quantity_updated_at
                        .get(&(
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

    fn active_inventory_levels_for_item(
        &self,
        inventory_item_id: &str,
    ) -> Vec<(String, BTreeMap<String, i64>)> {
        self.inventory_levels_for_item(inventory_item_id)
            .into_iter()
            .filter(|(location_id, _)| {
                !self
                    .store
                    .staged
                    .inactive_inventory_levels
                    .contains(&(inventory_item_id.to_string(), location_id.clone()))
            })
            .collect()
    }

    pub(in crate::proxy) fn inventory_total(&self, inventory_item_id: &str, name: &str) -> i64 {
        self.store
            .staged
            .inventory_levels
            .iter()
            .filter(|((item_id, _), _)| item_id == inventory_item_id)
            .filter(|((item_id, location_id), _)| {
                !self
                    .store
                    .staged
                    .inactive_inventory_levels
                    .contains(&(item_id.clone(), location_id.clone()))
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
        format!("2024-01-01T00:00:{sequence:02}.000Z")
    }

    fn stamp_inventory_quantity(
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
        {
            let level = self
                .store
                .staged
                .inventory_levels
                .entry((inventory_item_id.to_string(), location_id.clone()))
                .or_default();
            *level.entry("available".to_string()).or_insert(0) -= quantity;
            level.entry("on_hand".to_string()).or_insert(0);
            level.entry("damaged".to_string()).or_insert(0);
        }
        self.stamp_inventory_quantity(inventory_item_id, &location_id, "available", &updated_at);
    }

    fn inventory_total_all(&self, name: &str) -> i64 {
        self.store
            .staged
            .inventory_levels
            .iter()
            .filter(|((item_id, location_id), _)| {
                !self
                    .store
                    .staged
                    .inactive_inventory_levels
                    .contains(&(item_id.clone(), location_id.clone()))
            })
            .map(|(_, quantities)| quantities.get(name).copied().unwrap_or(0))
            .sum()
    }

    fn inventory_product_selected_json(
        &self,
        product_id: &str,
        selections: &[SelectedField],
    ) -> Value {
        selected_json(
            &json!({
                "id": product_id,
                "totalInventory": self.inventory_total_all("available"),
                "tracksInventory": true
            }),
            selections,
        )
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
        let ids = ids
            .into_iter()
            .filter(|id| {
                if id.starts_with("gid://shopify/InventoryItem/") {
                    !self.inventory_item_exists(id)
                } else if id.starts_with("gid://shopify/Location/") {
                    !self.inventory_location_exists(id)
                } else {
                    false
                }
            })
            .collect::<Vec<_>>();
        if ids.is_empty() {
            return;
        }
        let response = self.upstream_post(
            request,
            json!({
                "query": INVENTORY_TRANSFER_HYDRATE_NODES_QUERY,
                "variables": { "ids": ids }
            }),
        );
        if response.status >= 400 {
            return;
        }
        self.observe_inventory_transfer_hydration_response(&response.body);
    }

    pub(in crate::proxy) fn inventory_set_quantities(
        &mut self,
        request: &Request,
        field: &RootFieldSelection,
    ) -> MutationFieldOutcome {
        let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
        let ignore_compare = matches!(
            input.get("ignoreCompareQuantity"),
            Some(ResolvedValue::Bool(true))
        );
        let quantities = resolved_object_list_field(&input, "quantities");
        if inventory_set_requires_change_from(request, field) && !ignore_compare {
            if let Some(error_payload) = inventory_quantity_missing_change_from_payload(
                field,
                "inventorySetQuantities",
                "InventoryQuantityInput",
                &quantities,
                "quantity",
            ) {
                return MutationFieldOutcome::unlogged(error_payload);
            }
        }
        if let Some(error_payload) = inventory_invalid_reason_payload(field, &input) {
            return MutationFieldOutcome::unlogged(error_payload);
        }
        if !ignore_compare
            && quantities.iter().any(|quantity| {
                !quantity.contains_key("compareQuantity")
                    && !quantity.contains_key("changeFromQuantity")
            })
        {
            return MutationFieldOutcome::unlogged(selected_json(
                &json!({
                    "inventoryAdjustmentGroup": null,
                    "userErrors": [user_error_omit_code(["input", "ignoreCompareQuantity"], "The compareQuantity argument must be given to each quantity or ignored using ignoreCompareQuantity.", None)]
                }),
                &field.selection,
            ));
        }
        let name = resolved_string_field(&input, "name").unwrap_or_else(|| "available".to_string());
        if let Some(error_payload) = inventory_invalid_set_quantity_name_payload(field, &name) {
            return MutationFieldOutcome::unlogged(error_payload);
        }
        if let Some(error_payload) =
            inventory_invalid_set_quantities_payload(field, &quantities, &name)
        {
            return MutationFieldOutcome::unlogged(error_payload);
        }
        self.hydrate_inventory_quantity_rows(request, &quantities, "inventoryItemId", "locationId");
        if let Some(error_payload) =
            self.inventory_existence_payload(field, &quantities, "quantities")
        {
            return MutationFieldOutcome::unlogged(error_payload);
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
            let existed_before = self.store.staged.inventory_levels.contains_key(&key);
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
                on_hand_changes.push(inventory_change_json(
                    &item_id,
                    "on_hand",
                    delta,
                    None,
                    &location_id,
                    &location_name,
                ));
            }
            if !existed_before {
                self.store.staged.inventory_level_order.push(key);
            }
            self.stamp_inventory_quantity(&item_id, &location_id, &name, &updated_at);
            self.sync_variant_available_quantity(&item_id, &name, true);
            changes.push(inventory_change_json(
                &item_id,
                &name,
                delta,
                None,
                &location_id,
                &location_name,
            ));
        }
        changes.extend(on_hand_changes);
        MutationFieldOutcome::staged(
            selected_json(
                &json!({
                    "inventoryAdjustmentGroup": {
                        "id": self.next_proxy_synthetic_gid("InventoryAdjustmentGroup"),
                        "createdAt": updated_at,
                        "reason": reason,
                        "referenceDocumentUri": reference,
                        "changes": changes
                    },
                    "userErrors": []
                }),
                &field.selection,
            ),
            LogDraft::staged("inventorySetQuantities", "products", Vec::new()),
        )
    }

    pub(in crate::proxy) fn inventory_set_on_hand_quantities(
        &mut self,
        request: &Request,
        field: &RootFieldSelection,
    ) -> MutationFieldOutcome {
        if inventory_requires_idempotency(request) && !inventory_field_has_idempotent(field) {
            return MutationFieldOutcome::unlogged(inventory_idempotency_required_payload(
                field,
                "inventorySetOnHandQuantities",
            ));
        }

        let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
        let set_quantities = resolved_object_list_field(&input, "setQuantities");
        if inventory_set_requires_change_from(request, field) {
            if let Some(error_payload) = inventory_quantity_missing_change_from_payload(
                field,
                "inventorySetOnHandQuantities",
                "InventorySetQuantityInput",
                &set_quantities,
                "quantity",
            ) {
                return MutationFieldOutcome::unlogged(error_payload);
            }
        }
        if let Some(error_payload) = inventory_invalid_reason_payload(field, &input) {
            return MutationFieldOutcome::unlogged(error_payload);
        }
        if let Some(error_payload) =
            inventory_invalid_set_on_hand_quantities_payload(field, &set_quantities)
        {
            return MutationFieldOutcome::unlogged(error_payload);
        }
        self.hydrate_inventory_quantity_rows(
            request,
            &set_quantities,
            "inventoryItemId",
            "locationId",
        );
        if let Some(error_payload) =
            self.inventory_existence_payload(field, &set_quantities, "setQuantities")
        {
            return MutationFieldOutcome::unlogged(error_payload);
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
            let existed_before = self.store.staged.inventory_levels.contains_key(&key);
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
                self.store.staged.inventory_level_order.push(key);
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

        MutationFieldOutcome::staged(
            selected_json(
                &json!({
                    "inventoryAdjustmentGroup": {
                        "id": self.next_proxy_synthetic_gid("InventoryAdjustmentGroup"),
                        "createdAt": updated_at,
                        "reason": reason,
                        "referenceDocumentUri": reference,
                        "changes": changes
                    },
                    "userErrors": []
                }),
                &field.selection,
            ),
            LogDraft::staged("inventorySetOnHandQuantities", "products", Vec::new()),
        )
    }

    pub(in crate::proxy) fn inventory_adjust_quantities(
        &mut self,
        request: &Request,
        field: &RootFieldSelection,
    ) -> MutationFieldOutcome {
        let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
        let changes_input = resolved_object_list_field(&input, "changes");
        if inventory_adjust_requires_change_from(request) {
            if let Some(error_payload) = inventory_quantity_missing_change_from_payload(
                field,
                "inventoryAdjustQuantities",
                "InventoryChangeInput",
                &changes_input,
                "delta",
            ) {
                return MutationFieldOutcome::unlogged(error_payload);
            }
        }
        if let Some(error_payload) = inventory_invalid_reason_payload(field, &input) {
            return MutationFieldOutcome::unlogged(error_payload);
        }
        let name = resolved_string_field(&input, "name").unwrap_or_else(|| "available".to_string());
        if let Some(error_payload) =
            inventory_invalid_public_quantity_name_payload(field, &name, json!(["input", "name"]))
        {
            return MutationFieldOutcome::unlogged(error_payload);
        }
        self.hydrate_inventory_quantity_rows(
            request,
            &changes_input,
            "inventoryItemId",
            "locationId",
        );
        if let Some(error_payload) =
            self.inventory_existence_payload(field, &changes_input, "changes")
        {
            return MutationFieldOutcome::unlogged(error_payload);
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
            if name == "available" {
                {
                    let on_hand = level.entry("on_hand".to_string()).or_insert(0);
                    *on_hand += delta;
                }
                level.entry("damaged".to_string()).or_insert(0);
                self.stamp_inventory_quantity(&item_id, &location_id, "on_hand", &updated_at);
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
        MutationFieldOutcome::staged(
            selected_json(
                &json!({
                    "inventoryAdjustmentGroup": {
                        "id": self.next_proxy_synthetic_gid("InventoryAdjustmentGroup"),
                        "createdAt": updated_at,
                        "reason": reason,
                        "referenceDocumentUri": reference,
                        "changes": changes
                    },
                    "userErrors": []
                }),
                &field.selection,
            ),
            LogDraft::staged("inventoryAdjustQuantities", "products", Vec::new()),
        )
    }

    pub(in crate::proxy) fn inventory_move_quantities(
        &mut self,
        request: &Request,
        field: &RootFieldSelection,
    ) -> MutationFieldOutcome {
        let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
        let changes_input = resolved_object_list_field(&input, "changes");
        if let Some(error_payload) = inventory_invalid_reason_payload(field, &input) {
            return MutationFieldOutcome::unlogged(error_payload);
        }
        for (index, change) in changes_input.iter().enumerate() {
            let from = resolved_object_field(change, "from").unwrap_or_default();
            let to = resolved_object_field(change, "to").unwrap_or_default();
            let from_name = resolved_string_field(&from, "name").unwrap_or_default();
            if let Some(error_payload) = inventory_invalid_public_quantity_name_payload(
                field,
                &from_name,
                json!(["input", "changes", index.to_string(), "from", "name"]),
            ) {
                return MutationFieldOutcome::unlogged(error_payload);
            }
            let to_name = resolved_string_field(&to, "name").unwrap_or_default();
            if let Some(error_payload) = inventory_invalid_public_quantity_name_payload(
                field,
                &to_name,
                json!(["input", "changes", index.to_string(), "to", "name"]),
            ) {
                return MutationFieldOutcome::unlogged(error_payload);
            }
        }
        self.hydrate_inventory_move_rows(request, &changes_input);
        if let Some(error_payload) = self.inventory_move_existence_payload(field, &changes_input) {
            return MutationFieldOutcome::unlogged(error_payload);
        }
        for (index, change) in changes_input.iter().enumerate() {
            let from = resolved_object_field(change, "from").unwrap_or_default();
            let to = resolved_object_field(change, "to").unwrap_or_default();
            if resolved_string_field(&from, "locationId")
                != resolved_string_field(&to, "locationId")
            {
                return MutationFieldOutcome::unlogged(selected_json(
                    &json!({
                        "inventoryAdjustmentGroup": null,
                        "userErrors": [user_error_omit_code(json!(["input", "changes", index.to_string()]), "The quantities can't be moved between different locations.", None)]
                    }),
                    &field.selection,
                ));
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
        MutationFieldOutcome::staged(
            selected_json(
                &json!({
                    "inventoryAdjustmentGroup": {
                        "id": self.next_proxy_synthetic_gid("InventoryAdjustmentGroup"),
                        "createdAt": created_at,
                        "reason": reason,
                        "referenceDocumentUri": reference,
                        "changes": changes
                    },
                    "userErrors": []
                }),
                &field.selection,
            ),
            LogDraft::staged("inventoryMoveQuantities", "products", Vec::new()),
        )
    }

    fn inventory_existence_payload(
        &self,
        field: &RootFieldSelection,
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
        inventory_existence_error_payload(field, errors)
    }

    fn inventory_move_existence_payload(
        &self,
        field: &RootFieldSelection,
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
        inventory_existence_error_payload(field, errors)
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

    pub(in crate::proxy) fn inventory_activate(
        &mut self,
        field: &RootFieldSelection,
    ) -> MutationFieldOutcome {
        let inventory_item_id =
            resolved_string_field(&field.arguments, "inventoryItemId").unwrap_or_default();
        let location_id = resolved_string_field(&field.arguments, "locationId").unwrap_or_default();
        let has_available = field.arguments.contains_key("available");
        let available = resolved_int_field(&field.arguments, "available");
        let has_on_hand = field.arguments.contains_key("onHand");
        let on_hand = resolved_int_field(&field.arguments, "onHand");
        let inventory_level_selection =
            selected_child_selection(&field.selection, "inventoryLevel").unwrap_or_default();
        let mut user_errors = Vec::new();

        if !self.inventory_item_exists(&inventory_item_id) {
            user_errors.push(user_error_omit_code(
                vec!["inventoryItemId"],
                "The product couldn't be stocked because it wasn't found.",
                None,
            ));
            return MutationFieldOutcome::unlogged(self.inventory_activate_payload(
                None,
                &field.selection,
                user_errors,
            ));
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
            return MutationFieldOutcome::unlogged(self.inventory_activate_payload(
                None,
                &field.selection,
                user_errors,
            ));
        }
        if !self.inventory_location_is_active(&location_id) {
            user_errors.push(user_error_omit_code(
                vec!["locationId"],
                "The product couldn't be stocked because the location is not active.",
                None,
            ));
            return MutationFieldOutcome::unlogged(self.inventory_activate_payload(
                None,
                &field.selection,
                user_errors,
            ));
        }
        let location_name = self.inventory_location_display_name(&location_id);
        if has_available && has_on_hand {
            let message = format!(
                "The product couldn't be stocked at {location_name} because not allowed to set available and on_hand quantities at the same time."
            );
            user_errors.push(inventory_activate_user_error(vec!["available"], &message));
            user_errors.push(inventory_activate_user_error(vec!["onHand"], &message));
            return MutationFieldOutcome::unlogged(self.inventory_activate_payload(
                None,
                &field.selection,
                user_errors,
            ));
        }
        if on_hand_out_of_range {
            let message = format!(
                "The product couldn't be stocked at {location_name} because the quantity needs to be between -1 billion and 1 billion."
            );
            user_errors.push(inventory_activate_user_error(vec!["onHand"], &message));
            return MutationFieldOutcome::unlogged(self.inventory_activate_payload(
                None,
                &field.selection,
                user_errors,
            ));
        }

        let key = (inventory_item_id.clone(), location_id.clone());
        // The "already active" decision must be based on the level's state *before*
        // this call. A fresh activation (a brand-new level, or reactivating an
        // inactive one) is allowed to seed `available`; only a level that was
        // already active rejects it. Computing this up-front avoids the earlier bug
        // where pre-creating a default level flipped the flag and spuriously errored.
        let existed_before = self.store.staged.inventory_levels.contains_key(&key);
        let was_active =
            existed_before && !self.store.staged.inactive_inventory_levels.contains(&key);
        if was_active && has_available {
            user_errors.push(user_error_omit_code(
                vec!["available"],
                "Not allowed to set available quantity when the item is already active at the location.",
                None,
            ));
            let level = self.inventory_level_for_payload(
                &inventory_item_id,
                &location_id,
                &inventory_level_selection,
            );
            return MutationFieldOutcome::unlogged(self.inventory_activate_payload(
                level,
                &field.selection,
                user_errors,
            ));
        }
        if was_active && has_on_hand {
            user_errors.push(inventory_activate_user_error(
                vec!["onHand"],
                "Not allowed to set an on_hand quantity when the item is already active at the location.",
            ));
            let level = self.inventory_level_for_payload(
                &inventory_item_id,
                &location_id,
                &inventory_level_selection,
            );
            return MutationFieldOutcome::unlogged(self.inventory_activate_payload(
                level,
                &field.selection,
                user_errors,
            ));
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
            return MutationFieldOutcome::unlogged(self.inventory_activate_payload(
                None,
                &field.selection,
                user_errors,
            ));
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
        let level = self.inventory_level_for_payload(
            &inventory_item_id,
            &location_id,
            &inventory_level_selection,
        );
        MutationFieldOutcome::staged(
            self.inventory_activate_payload(level, &field.selection, user_errors),
            LogDraft::staged("inventoryActivate", "products", vec![inventory_item_id]),
        )
    }

    pub(in crate::proxy) fn inventory_deactivate(
        &mut self,
        field: &RootFieldSelection,
    ) -> MutationFieldOutcome {
        let inventory_level_id =
            resolved_string_field(&field.arguments, "inventoryLevelId").unwrap_or_default();
        let mut user_errors = Vec::new();
        let Some((inventory_item_id, location_id)) =
            self.inventory_level_parts_from_id_or_fallback(&inventory_level_id)
        else {
            user_errors.push(inventory_deactivate_user_error(
                "The product couldn't be unstocked because the product was deleted.",
            ));
            return MutationFieldOutcome::unlogged(
                self.inventory_deactivate_payload(&field.selection, user_errors),
            );
        };
        let key = (inventory_item_id.clone(), location_id.clone());
        if !self.inventory_item_exists(&inventory_item_id) {
            user_errors.push(inventory_deactivate_user_error(
                "The product couldn't be unstocked because the product was deleted.",
            ));
        } else if self.inventory_level_id_is_missing(&inventory_level_id) {
            user_errors.push(inventory_deactivate_user_error(
                "The product couldn't be unstocked because the location was deleted.",
            ));
        } else if !self.store.staged.inventory_levels.contains_key(&key) {
            self.ensure_default_inventory_level(&inventory_item_id, &location_id);
        }
        if user_errors.is_empty()
            && self
                .active_inventory_levels_for_item(&inventory_item_id)
                .len()
                <= 1
            && !self.store.staged.inactive_inventory_levels.contains(&key)
        {
            user_errors.push(inventory_deactivate_user_error(
                &format!(
                    "The product couldn't be unstocked from {} because products need to be stocked at a minimum of 1 location.",
                    self.inventory_location_display_name(&location_id)
                ),
            ));
        }
        if !user_errors.is_empty() {
            return MutationFieldOutcome::unlogged(
                self.inventory_deactivate_payload(&field.selection, user_errors),
            );
        }

        self.store.staged.inactive_inventory_levels.insert(key);
        MutationFieldOutcome::staged(
            self.inventory_deactivate_payload(&field.selection, user_errors),
            LogDraft::staged("inventoryDeactivate", "products", vec![inventory_level_id]),
        )
    }

    pub(in crate::proxy) fn inventory_bulk_toggle_activation(
        &mut self,
        field: &RootFieldSelection,
    ) -> MutationFieldOutcome {
        let inventory_item_id =
            resolved_string_field(&field.arguments, "inventoryItemId").unwrap_or_default();
        let updates = resolved_object_list_field(&field.arguments, "inventoryItemUpdates");
        let changed_level_selection =
            selected_child_selection(&field.selection, "inventoryLevels").unwrap_or_default();
        let mut changed_levels = Vec::new();
        let mut user_errors = Vec::new();

        if !self.inventory_item_exists(&inventory_item_id) {
            user_errors.push(user_error_omit_code(
                vec!["inventoryItemId".to_string()],
                "The inventory item couldn't be found.",
                Some("INVENTORY_ITEM_NOT_FOUND"),
            ));
            return MutationFieldOutcome::unlogged(self.inventory_bulk_toggle_payload(
                None,
                None,
                &field.selection,
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
                return MutationFieldOutcome::unlogged(self.inventory_bulk_toggle_payload(
                    None,
                    None,
                    &field.selection,
                    user_errors,
                ));
            }
            let key = (inventory_item_id.clone(), location_id.clone());
            let is_active = self.store.staged.inventory_levels.contains_key(&key)
                && !self.store.staged.inactive_inventory_levels.contains(&key);
            if !is_active
                && self
                    .active_inventory_levels_for_item(&inventory_item_id)
                    .is_empty()
            {
                self.ensure_default_inventory_level(&inventory_item_id, &location_id);
            }
            let is_active = self.store.staged.inventory_levels.contains_key(&key)
                && !self.store.staged.inactive_inventory_levels.contains(&key);
            if activate {
                if !is_active {
                    self.activate_inventory_level(&inventory_item_id, &location_id);
                }
                if let Some(level) = self.inventory_level_for_payload(
                    &inventory_item_id,
                    &location_id,
                    &changed_level_selection,
                ) {
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
                    return MutationFieldOutcome::unlogged(self.inventory_bulk_toggle_payload(
                        None,
                        None,
                        &field.selection,
                        user_errors,
                    ));
                }
                if is_active {
                    self.store.staged.inactive_inventory_levels.insert(key);
                }
            }
        }

        let item = Some(
            self.inventory_item_selected_json(
                &inventory_item_id,
                &BTreeMap::new(),
                selected_child_selection(&field.selection, "inventoryItem")
                    .as_deref()
                    .unwrap_or(&[]),
            ),
        );
        MutationFieldOutcome::staged(
            self.inventory_bulk_toggle_payload(
                item,
                Some(changed_levels),
                &field.selection,
                user_errors,
            ),
            LogDraft::staged(
                "inventoryBulkToggleActivation",
                "products",
                vec![inventory_item_id],
            ),
        )
    }

    pub(in crate::proxy) fn inventory_item_update(
        &mut self,
        request: &Request,
        field: &RootFieldSelection,
    ) -> MutationFieldOutcome {
        let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
        let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
        if let Some(errors) = inventory_item_update_variable_errors(field, &input) {
            return MutationFieldOutcome::unlogged(json!({ "__topLevelErrors": errors }));
        }
        let user_errors = inventory_item_update_user_errors(&input);
        if !user_errors.is_empty() {
            return MutationFieldOutcome::unlogged(self.inventory_item_update_payload(
                None,
                &field.selection,
                user_errors,
            ));
        }
        self.hydrate_inventory_reference_ids(request, vec![id.clone()]);
        let Some(mut variant) = self
            .store
            .product_variant_by_inventory_item_id(&id)
            .cloned()
        else {
            return MutationFieldOutcome::unlogged(self.inventory_item_update_payload(
                None,
                &field.selection,
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
        let inventory_item = self.inventory_item_selected_json(
            &inventory_item_id,
            &BTreeMap::new(),
            selected_child_selection(&field.selection, "inventoryItem")
                .as_deref()
                .unwrap_or(&[]),
        );
        MutationFieldOutcome::staged(
            self.inventory_item_update_payload(Some(inventory_item), &field.selection, Vec::new()),
            LogDraft::staged("inventoryItemUpdate", "products", vec![product_id]),
        )
    }

    fn inventory_item_exists(&self, inventory_item_id: &str) -> bool {
        if inventory_item_id.is_empty()
            || !inventory_item_id.starts_with("gid://shopify/InventoryItem/")
        {
            return false;
        }
        if self.inventory_item_id_is_missing(inventory_item_id) {
            return false;
        }
        self.store
            .product_variant_by_inventory_item_id(inventory_item_id)
            .is_some()
            || self
                .store
                .staged
                .inventory_levels
                .keys()
                .any(|(item_id, _)| item_id == inventory_item_id)
    }

    fn inventory_item_has_local_state(&self, inventory_item_id: &str) -> bool {
        self.store
            .product_variant_by_inventory_item_id(inventory_item_id)
            .is_some()
            || self
                .store
                .staged
                .inventory_levels
                .keys()
                .any(|(item_id, _)| item_id == inventory_item_id)
    }

    fn inventory_location_exists(&self, location_id: &str) -> bool {
        if location_id.is_empty() || !location_id.starts_with("gid://shopify/Location/") {
            return false;
        }
        if self.inventory_location_id_is_missing(location_id) {
            return false;
        }
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
                .any(|(_, staged_location_id)| staged_location_id == location_id)
    }

    fn inventory_location_is_active(&self, location_id: &str) -> bool {
        self.inventory_location_record(location_id)
            .and_then(|location| location.get("isActive"))
            .and_then(Value::as_bool)
            .unwrap_or(true)
    }

    fn inventory_location_display_name(&self, location_id: &str) -> String {
        self.inventory_location_record(location_id)
            .and_then(|location| location.get("name"))
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| inventory_location_name(location_id).to_string())
    }

    fn inventory_location_record(&self, location_id: &str) -> Option<&Value> {
        self.store
            .staged
            .locations
            .get(location_id)
            .or_else(|| {
                self.store
                    .staged
                    .observed_shipping_locations
                    .get(location_id)
            })
            .or_else(|| {
                self.store
                    .staged
                    .fulfillment_service_locations
                    .get(location_id)
            })
    }

    fn inventory_item_id_is_missing(&self, inventory_item_id: &str) -> bool {
        let tail = resource_id_tail(inventory_item_id);
        inventory_id_tail_is_missing(tail)
            || INVENTORY_ITEM_EXTRA_MISSING_ID_TAILS
                .iter()
                .any(|sentinel| tail.eq_ignore_ascii_case(sentinel))
    }

    fn inventory_level_id_is_missing(&self, inventory_level_id: &str) -> bool {
        let tail = inventory_level_id_tail(inventory_level_id).unwrap_or_default();
        inventory_id_tail_is_missing(tail)
    }

    fn inventory_location_id_is_missing(&self, location_id: &str) -> bool {
        inventory_id_tail_is_missing(resource_id_tail(location_id))
    }

    fn inventory_level_parts_from_id_or_fallback(&self, id: &str) -> Option<(String, String)> {
        let (_, query) = inventory_level_id_tail_and_query(id)?;
        let inventory_item_id = if query.starts_with("gid://shopify/InventoryItem/") {
            query.to_string()
        } else {
            shopify_gid("InventoryItem", query)
        };
        if let Some(((item_id, location_id), _)) = self
            .store
            .staged
            .inventory_level_ids
            .iter()
            .find(|(_, observed_id)| observed_id.as_str() == id)
        {
            return Some((item_id.clone(), location_id.clone()));
        }
        if let Some((item_id, location_id)) = self
            .store
            .staged
            .inventory_levels
            .keys()
            .find(|(item_id, location_id)| inventory_level_id(item_id, location_id) == id)
        {
            return Some((item_id.clone(), location_id.clone()));
        }
        if let Some((_, location_id)) = inventory_level_parts_from_id(id) {
            return Some((inventory_item_id, location_id));
        }
        let location_id = self
            .active_inventory_levels_for_item(&inventory_item_id)
            .first()
            .map(|(location_id, _)| location_id.clone())
            .or_else(|| self.default_inventory_location_id())?;
        Some((inventory_item_id, location_id))
    }

    fn default_inventory_location_id(&self) -> Option<String> {
        self.first_active_location_from_order(
            &self.store.staged.locations.order,
            &self.store.staged.locations.records,
        )
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
                .map(|(_, location_id)| location_id)
                .find(|location_id| self.inventory_location_exists(location_id))
                .cloned()
        })
        .or_else(|| {
            self.store
                .staged
                .inventory_levels
                .keys()
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
            && requested_location_id.starts_with("gid://shopify/Location/")
            && requested_location_id != "gid://shopify/Location/999999999999"
        {
            requested_location_id.to_string()
        } else {
            let Some(location_id) = self.default_inventory_location_id() else {
                return;
            };
            location_id
        };
        let key = (inventory_item_id.to_string(), location_id);
        self.store
            .staged
            .inventory_levels
            .entry(key)
            .or_insert_with(empty_inventory_quantities);
    }

    fn activate_inventory_level(&mut self, inventory_item_id: &str, location_id: &str) {
        let key = (inventory_item_id.to_string(), location_id.to_string());
        self.store.staged.inactive_inventory_levels.remove(&key);
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
        selections: &[SelectedField],
    ) -> Option<Value> {
        let quantities = self
            .store
            .staged
            .inventory_levels
            .get(&(inventory_item_id.to_string(), location_id.to_string()))?;
        Some(self.inventory_level_json_with_item(
            inventory_item_id,
            location_id,
            quantities,
            selections,
        ))
    }

    /// Render an inventory level, overriding the `item` sub-selection with the
    /// store-backed item payload (so `tracked`/`variant` resolve correctly).
    /// The free `inventory_level_selected_json` only knows the item id; reads of
    /// `inventoryLevel { item { tracked } }` need this `&self` override.
    fn inventory_level_json_with_item(
        &self,
        inventory_item_id: &str,
        location_id: &str,
        quantities: &BTreeMap<String, i64>,
        selections: &[SelectedField],
    ) -> Value {
        let mut value = inventory_level_selected_json(
            inventory_item_id,
            location_id,
            quantities,
            &self.inventory_level_view_state(),
            selections,
        );
        if let Some(item_selection) = selections.iter().find(|selection| selection.name == "item") {
            if let Some(object) = value.as_object_mut() {
                object.insert(
                    item_selection.response_key.clone(),
                    self.inventory_level_item_payload(inventory_item_id, &item_selection.selection),
                );
            }
        }
        value
    }

    fn inventory_level_item_payload(
        &self,
        inventory_item_id: &str,
        selections: &[SelectedField],
    ) -> Value {
        let variant = self
            .store
            .product_variant_by_inventory_item_id(inventory_item_id);
        let product = variant.and_then(|variant| self.store.product_by_id(&variant.product_id));
        let variant_for_payload = variant.cloned().map(|mut variant| {
            variant.inventory_quantity = self.inventory_total(inventory_item_id, "available");
            variant
        });
        selected_payload_json(selections, |selection| match selection.name.as_str() {
            "id" => Some(json!(inventory_item_id)),
            "tracked" => Some(json!(variant
                .map(|variant| variant.inventory_item.tracked)
                .unwrap_or(true))),
            "variant" => variant_for_payload
                .as_ref()
                .map(|variant| product_variant_json(variant, product, &selection.selection)),
            _ => None,
        })
    }

    fn inventory_activate_payload(
        &self,
        inventory_level: Option<Value>,
        selections: &[SelectedField],
        user_errors: Vec<Value>,
    ) -> Value {
        selected_payload_json(selections, |selection| match selection.name.as_str() {
            "inventoryLevel" => Some(inventory_level.clone().unwrap_or(Value::Null)),
            "userErrors" => selected_user_errors_field(user_errors.as_slice(), selection),
            _ => None,
        })
    }

    fn inventory_deactivate_payload(
        &self,
        selections: &[SelectedField],
        user_errors: Vec<Value>,
    ) -> Value {
        selected_payload_json(selections, |selection| match selection.name.as_str() {
            "userErrors" => selected_user_errors_field(user_errors.as_slice(), selection),
            _ => None,
        })
    }

    fn inventory_bulk_toggle_payload(
        &self,
        inventory_item: Option<Value>,
        inventory_levels: Option<Vec<Value>>,
        selections: &[SelectedField],
        user_errors: Vec<Value>,
    ) -> Value {
        selected_payload_json(selections, |selection| match selection.name.as_str() {
            "inventoryItem" => Some(nullable_selected_json(
                inventory_item.as_ref().unwrap_or(&Value::Null),
                &selection.selection,
            )),
            "inventoryLevels" => Some(
                inventory_levels
                    .as_ref()
                    .map_or(Value::Null, |levels| Value::Array(levels.clone())),
            ),
            "userErrors" => selected_user_errors_field(user_errors.as_slice(), selection),
            _ => None,
        })
    }

    fn inventory_item_update_payload(
        &self,
        inventory_item: Option<Value>,
        selections: &[SelectedField],
        user_errors: Vec<Value>,
    ) -> Value {
        selected_payload_json(selections, |selection| match selection.name.as_str() {
            "inventoryItem" => Some(nullable_selected_json(
                inventory_item.as_ref().unwrap_or(&Value::Null),
                &selection.selection,
            )),
            "userErrors" => selected_user_errors_field(user_errors.as_slice(), selection),
            _ => None,
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

    pub(in crate::proxy) fn inventory_shipment_create(
        &mut self,
        field: &RootFieldSelection,
        in_transit: bool,
    ) -> MutationFieldOutcome {
        let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
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
            return MutationFieldOutcome::unlogged(self.inventory_shipment_payload_with_errors(
                field,
                "inventoryShipment",
                errors,
            ));
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
        let payload =
            self.inventory_shipment_payload_json(&record, &field.selection, "inventoryShipment");
        self.store
            .staged
            .inventory_shipments
            .insert(id.clone(), record);
        MutationFieldOutcome::staged(
            payload,
            LogDraft::staged(field.name.clone(), "products", vec![id]),
        )
    }

    pub(in crate::proxy) fn inventory_shipment_add_items(
        &mut self,
        field: &RootFieldSelection,
    ) -> MutationFieldOutcome {
        let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
        let Some(mut record) = self.store.staged.inventory_shipments.get(&id).cloned() else {
            return MutationFieldOutcome::unlogged(
                self.inventory_shipment_missing_mutation_payload(
                    field,
                    "inventoryShipment",
                    &[("addedItems", json!([]))],
                ),
            );
        };
        let line_inputs = resolved_object_list_field(&field.arguments, "lineItems");
        if let Some(errors) =
            self.inventory_shipment_line_validation_errors(&record, &line_inputs, "lineItems")
        {
            return MutationFieldOutcome::unlogged(
                self.inventory_shipment_payload_with_errors_and_extra(
                    field,
                    "inventoryShipment",
                    errors,
                    &[("addedItems", json!([]))],
                ),
            );
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
        let payload = selected_json(
            &json!({
                "inventoryShipment": self.inventory_shipment_full_json(&record),
                "addedItems": added_items,
                "userErrors": []
            }),
            &field.selection,
        );
        self.store
            .staged
            .inventory_shipments
            .insert(id.clone(), record);
        MutationFieldOutcome::staged(
            payload,
            LogDraft::staged("inventoryShipmentAddItems", "products", vec![id]),
        )
    }

    pub(in crate::proxy) fn inventory_shipment_remove_items(
        &mut self,
        field: &RootFieldSelection,
    ) -> MutationFieldOutcome {
        let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
        let Some(mut record) = self.store.staged.inventory_shipments.get(&id).cloned() else {
            return MutationFieldOutcome::unlogged(
                self.inventory_shipment_missing_mutation_payload(
                    field,
                    "inventoryShipment",
                    &[("removedLineItemIds", json!([]))],
                ),
            );
        };
        let remove_ids = resolved_string_list_arg(&field.arguments, "shipmentLineItemIds");
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
        let payload = selected_json(
            &json!({
                "inventoryShipment": self.inventory_shipment_full_json(&record),
                "removedLineItemIds": removed_ids,
                "userErrors": []
            }),
            &field.selection,
        );
        self.store
            .staged
            .inventory_shipments
            .insert(id.clone(), record);
        MutationFieldOutcome::staged(
            payload,
            LogDraft::staged("inventoryShipmentRemoveItems", "products", vec![id]),
        )
    }

    pub(in crate::proxy) fn inventory_shipment_update_item_quantities(
        &mut self,
        field: &RootFieldSelection,
    ) -> MutationFieldOutcome {
        let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
        let Some(mut record) = self.store.staged.inventory_shipments.get(&id).cloned() else {
            return MutationFieldOutcome::unlogged(
                self.inventory_shipment_missing_mutation_payload(
                    field,
                    "shipment",
                    &[("updatedLineItems", json!([]))],
                ),
            );
        };
        let items = resolved_object_list_field(&field.arguments, "items");
        let mut proposed_quantities_by_line_id = BTreeMap::new();
        for (index, item) in items.iter().enumerate() {
            let line_item_id =
                resolved_string_field(item, "shipmentLineItemId").unwrap_or_default();
            let Some(line_item) = record
                .line_items
                .iter()
                .find(|line_item| line_item.id == line_item_id)
            else {
                return MutationFieldOutcome::unlogged(
                    self.inventory_shipment_payload_with_errors_and_extra(
                        field,
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
                    return MutationFieldOutcome::unlogged(
                        self.inventory_shipment_payload_with_errors_and_extra(
                            field,
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
        let payload = selected_json(
            &json!({
                "shipment": self.inventory_shipment_full_json(&record),
                "updatedLineItems": updated,
                "userErrors": []
            }),
            &field.selection,
        );
        self.store
            .staged
            .inventory_shipments
            .insert(id.clone(), record);
        MutationFieldOutcome::staged(
            payload,
            LogDraft::staged(
                "inventoryShipmentUpdateItemQuantities",
                "products",
                vec![id],
            ),
        )
    }

    pub(in crate::proxy) fn inventory_shipment_set_tracking(
        &mut self,
        field: &RootFieldSelection,
    ) -> MutationFieldOutcome {
        let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
        let Some(mut record) = self.store.staged.inventory_shipments.get(&id).cloned() else {
            return MutationFieldOutcome::unlogged(
                self.inventory_shipment_missing_mutation_payload(field, "inventoryShipment", &[]),
            );
        };
        let input = resolved_object_field(&field.arguments, "trackingInput")
            .or_else(|| resolved_object_field(&field.arguments, "tracking"))
            .unwrap_or_default();
        let errors = inventory_shipment_tracking_errors(&input);
        if !errors.is_empty() {
            return MutationFieldOutcome::unlogged(self.inventory_shipment_payload_with_errors(
                field,
                "inventoryShipment",
                errors,
            ));
        }
        record.tracking = inventory_shipment_tracking_from_input(&input);
        let payload =
            self.inventory_shipment_payload_json(&record, &field.selection, "inventoryShipment");
        self.store
            .staged
            .inventory_shipments
            .insert(id.clone(), record);
        MutationFieldOutcome::staged(
            payload,
            LogDraft::staged("inventoryShipmentSetTracking", "products", vec![id]),
        )
    }

    pub(in crate::proxy) fn inventory_shipment_mark_in_transit(
        &mut self,
        field: &RootFieldSelection,
    ) -> MutationFieldOutcome {
        let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
        let Some(mut record) = self.store.staged.inventory_shipments.get(&id).cloned() else {
            return MutationFieldOutcome::unlogged(
                self.inventory_shipment_missing_mutation_payload(field, "inventoryShipment", &[]),
            );
        };
        if record.status != "DRAFT" {
            return MutationFieldOutcome::unlogged(self.inventory_shipment_payload_with_errors(
                field,
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
        let payload =
            self.inventory_shipment_payload_json(&record, &field.selection, "inventoryShipment");
        self.store
            .staged
            .inventory_shipments
            .insert(id.clone(), record);
        MutationFieldOutcome::staged(
            payload,
            LogDraft::staged("inventoryShipmentMarkInTransit", "products", vec![id]),
        )
    }

    pub(in crate::proxy) fn inventory_shipment_receive(
        &mut self,
        field: &RootFieldSelection,
    ) -> MutationFieldOutcome {
        let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
        let Some(mut record) = self.store.staged.inventory_shipments.get(&id).cloned() else {
            return MutationFieldOutcome::unlogged(
                self.inventory_shipment_missing_mutation_payload(field, "inventoryShipment", &[]),
            );
        };
        if !matches!(record.status.as_str(), "IN_TRANSIT" | "PARTIALLY_RECEIVED") {
            return MutationFieldOutcome::unlogged(self.inventory_shipment_payload_with_errors(
                field,
                "inventoryShipment",
                vec![inventory_shipment_user_error(
                    vec!["id"],
                    "Only in-transit shipments can be received.",
                    "INVALID_STATE",
                )],
            ));
        }
        let receive_items = resolved_object_list_field(&field.arguments, "lineItems");
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
        let payload =
            self.inventory_shipment_payload_json(&record, &field.selection, "inventoryShipment");
        self.store
            .staged
            .inventory_shipments
            .insert(id.clone(), record);
        MutationFieldOutcome::staged(
            payload,
            LogDraft::staged("inventoryShipmentReceive", "products", vec![id]),
        )
    }

    pub(in crate::proxy) fn inventory_shipment_delete(
        &mut self,
        field: &RootFieldSelection,
    ) -> MutationFieldOutcome {
        let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
        let Some(record) = self.store.staged.inventory_shipments.remove(&id) else {
            return MutationFieldOutcome::unlogged(selected_json(
                &json!({
                    "id": Value::Null,
                    "userErrors": [inventory_shipment_user_error(
                        vec!["id"],
                        "The specified inventory shipment could not be found.",
                        "NOT_FOUND",
                    )]
                }),
                &field.selection,
            ));
        };
        if inventory_shipment_has_incoming(&record) {
            self.apply_shipment_incoming_delta(&record, -record.unreceived_quantity());
        }
        let deleted_id = record.id.clone();
        MutationFieldOutcome::staged(
            selected_json(
                &json!({
                    "id": id,
                    "userErrors": []
                }),
                &field.selection,
            ),
            LogDraft::staged("inventoryShipmentDelete", "products", vec![deleted_id]),
        )
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

    fn remaining_transfer_line_quantity(
        &self,
        transfer_id: &str,
        transfer_line_item_id: &str,
        excluding_shipment_id: Option<&str>,
    ) -> i64 {
        let total = self
            .store
            .staged
            .inventory_transfers
            .get(transfer_id)
            .and_then(|transfer| {
                transfer
                    .line_items
                    .iter()
                    .find(|line_item| line_item.id == transfer_line_item_id)
                    .map(|line_item| line_item.quantity)
            })
            .unwrap_or(0);
        let staged = self
            .store
            .staged
            .inventory_shipments
            .values()
            .filter(|shipment| excluding_shipment_id != Some(shipment.id.as_str()))
            .flat_map(|shipment| shipment.line_items.iter())
            .filter(|line_item| {
                line_item.transfer_line_item_id.as_deref() == Some(transfer_line_item_id)
            })
            .map(|line_item| line_item.quantity)
            .sum::<i64>();
        total - staged
    }

    fn inventory_shipment_payload_json(
        &self,
        record: &InventoryShipmentRecord,
        selection: &[SelectedField],
        shipment_field: &str,
    ) -> Value {
        selected_json(
            &json!({
                shipment_field: self.inventory_shipment_full_json(record),
                "userErrors": []
            }),
            selection,
        )
    }

    fn inventory_shipment_payload_with_errors(
        &self,
        field: &RootFieldSelection,
        shipment_field: &str,
        errors: Vec<Value>,
    ) -> Value {
        self.inventory_shipment_payload_with_errors_and_extra(field, shipment_field, errors, &[])
    }

    fn inventory_shipment_payload_with_errors_and_extra(
        &self,
        field: &RootFieldSelection,
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
        selected_json(&Value::Object(payload), &field.selection)
    }

    fn inventory_shipment_missing_mutation_payload(
        &self,
        field: &RootFieldSelection,
        shipment_field: &str,
        extra: &[(&str, Value)],
    ) -> Value {
        self.inventory_shipment_payload_with_errors_and_extra(
            field,
            shipment_field,
            vec![inventory_shipment_user_error(
                vec!["id"],
                "The specified inventory shipment could not be found.",
                "NOT_FOUND",
            )],
            extra,
        )
    }

    fn inventory_shipment_by_id_selected_json(
        &self,
        id: &str,
        selection: &[SelectedField],
    ) -> Value {
        self.store
            .staged
            .inventory_shipments
            .get(id)
            .map(|record| selected_json(&self.inventory_shipment_full_json(record), selection))
            .unwrap_or(Value::Null)
    }

    fn inventory_shipment_full_json(&self, record: &InventoryShipmentRecord) -> Value {
        let line_items = record
            .line_items
            .iter()
            .map(|line_item| self.inventory_shipment_line_item_full_json(line_item))
            .collect::<Vec<_>>();
        json!({
            "id": record.id,
            "name": record.name,
            "movementId": record.movement_id,
            "status": record.status,
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
            "lineItems": {
                "nodes": line_items,
                "pageInfo": empty_page_info()
            }
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
            if self.store.staged.inventory_levels.contains_key(&key) {
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
        let level = self
            .store
            .staged
            .inventory_levels
            .entry((inventory_item_id.to_string(), location_id.to_string()))
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

    pub(in crate::proxy) fn inventory_transfer_create(
        &mut self,
        field: &RootFieldSelection,
        ready_to_ship: bool,
    ) -> MutationFieldOutcome {
        let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
        let origin_location_id =
            resolved_string_field(&input, "originLocationId").unwrap_or_default();
        let destination_location_id =
            resolved_string_field(&input, "destinationLocationId").unwrap_or_default();
        let line_item_inputs = resolved_object_list_field(&input, "lineItems");
        self.hydrate_inventory_transfer_references(
            [&origin_location_id, &destination_location_id],
            &line_item_inputs,
        );
        let user_errors = self.inventory_transfer_validate(
            &origin_location_id,
            &destination_location_id,
            &line_item_inputs,
        );
        if !user_errors.is_empty() {
            return MutationFieldOutcome::unlogged(self.inventory_transfer_user_error_payload(
                &field.selection,
                "inventoryTransfer",
                &[],
                user_errors,
            ));
        }
        let id = self.next_proxy_synthetic_gid("InventoryTransfer");
        let name = format!(
            "#T{:04}",
            self.store
                .staged
                .inventory_transfers
                .len()
                .saturating_add(1)
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
            status: if ready_to_ship {
                "READY_TO_SHIP".to_string()
            } else {
                "DRAFT".to_string()
            },
            origin_location_id,
            destination_location_id,
            line_items,
        };
        self.ensure_transfer_inventory_levels(&record);
        if ready_to_ship {
            self.apply_transfer_reservations(&record, 1);
        }
        let payload =
            self.inventory_transfer_payload_json(&record, &field.selection, "inventoryTransfer");
        self.store
            .staged
            .inventory_transfers
            .insert(id.clone(), record);
        MutationFieldOutcome::staged(
            payload,
            LogDraft::staged(field.name.clone(), "products", vec![id]),
        )
    }

    pub(in crate::proxy) fn inventory_transfer_mark_ready(
        &mut self,
        field: &RootFieldSelection,
    ) -> MutationFieldOutcome {
        let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
        let Some(existing) = self.store.staged.inventory_transfers.get(&id).cloned() else {
            return MutationFieldOutcome::unlogged(
                self.inventory_transfer_missing_payload(&field.selection, "inventoryTransfer"),
            );
        };
        let mut record = existing;
        if record.status == "DRAFT" {
            self.apply_transfer_reservations(&record, 1);
        }
        record.status = "READY_TO_SHIP".to_string();
        let payload =
            self.inventory_transfer_payload_json(&record, &field.selection, "inventoryTransfer");
        self.store
            .staged
            .inventory_transfers
            .insert(id.clone(), record);
        MutationFieldOutcome::staged(
            payload,
            LogDraft::staged("inventoryTransferMarkAsReadyToShip", "products", vec![id]),
        )
    }

    pub(in crate::proxy) fn inventory_transfer_set_items(
        &mut self,
        field: &RootFieldSelection,
    ) -> MutationFieldOutcome {
        let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
        let id = resolved_string_field(&input, "id").unwrap_or_default();
        let Some(existing) = self.store.staged.inventory_transfers.get(&id).cloned() else {
            return MutationFieldOutcome::unlogged(
                self.inventory_transfer_missing_payload(&field.selection, "inventoryTransfer"),
            );
        };
        let mut record = existing;
        let line_item_inputs = resolved_object_list_field(&input, "lineItems");
        self.hydrate_inventory_transfer_references(
            [&record.origin_location_id, &record.destination_location_id],
            &line_item_inputs,
        );
        let user_errors = self.inventory_transfer_validate(
            &record.origin_location_id,
            &record.destination_location_id,
            &line_item_inputs,
        );
        if !user_errors.is_empty() {
            return MutationFieldOutcome::unlogged(self.inventory_transfer_user_error_payload(
                &field.selection,
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
        let payload = selected_json(
            &json!({
                "inventoryTransfer": self.inventory_transfer_full_json(&record),
                "updatedLineItems": updated,
                "userErrors": []
            }),
            &field.selection,
        );
        self.store
            .staged
            .inventory_transfers
            .insert(id.clone(), record);
        MutationFieldOutcome::staged(
            payload,
            LogDraft::staged("inventoryTransferSetItems", "products", vec![id]),
        )
    }

    pub(in crate::proxy) fn inventory_transfer_edit(
        &mut self,
        field: &RootFieldSelection,
    ) -> MutationFieldOutcome {
        let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
        let Some(existing) = self.store.staged.inventory_transfers.get(&id).cloned() else {
            return MutationFieldOutcome::unlogged(
                self.inventory_transfer_missing_payload(&field.selection, "inventoryTransfer"),
            );
        };
        let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
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
        let user_errors = self.inventory_transfer_validate(
            &origin_location_id,
            &destination_location_id,
            &line_item_inputs,
        );
        if !user_errors.is_empty() {
            return MutationFieldOutcome::unlogged(self.inventory_transfer_user_error_payload(
                &field.selection,
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
        self.ensure_transfer_inventory_levels(&record);
        if was_ready {
            self.apply_transfer_reservations(&record, 1);
        }

        let payload =
            self.inventory_transfer_payload_json(&record, &field.selection, "inventoryTransfer");
        self.store
            .staged
            .inventory_transfers
            .insert(id.clone(), record);
        MutationFieldOutcome::staged(
            payload,
            LogDraft::staged("inventoryTransferEdit", "products", vec![id]),
        )
    }

    pub(in crate::proxy) fn inventory_transfer_duplicate(
        &mut self,
        field: &RootFieldSelection,
    ) -> MutationFieldOutcome {
        let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
        let Some(existing) = self.store.staged.inventory_transfers.get(&id).cloned() else {
            return MutationFieldOutcome::unlogged(
                self.inventory_transfer_missing_payload(&field.selection, "inventoryTransfer"),
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
            return MutationFieldOutcome::unlogged(self.inventory_transfer_user_error_payload(
                &field.selection,
                "inventoryTransfer",
                &[],
                user_errors,
            ));
        }

        let new_id = self.next_proxy_synthetic_gid("InventoryTransfer");
        let name = format!(
            "#T{:04}",
            self.store
                .staged
                .inventory_transfers
                .len()
                .saturating_add(1)
        );
        let record = InventoryTransferRecord {
            id: new_id.clone(),
            name,
            status: "DRAFT".to_string(),
            origin_location_id: existing.origin_location_id,
            destination_location_id: existing.destination_location_id,
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
        let payload =
            self.inventory_transfer_payload_json(&record, &field.selection, "inventoryTransfer");
        self.store
            .staged
            .inventory_transfers
            .insert(new_id.clone(), record);
        MutationFieldOutcome::staged(
            payload,
            LogDraft::staged("inventoryTransferDuplicate", "products", vec![new_id]),
        )
    }

    pub(in crate::proxy) fn inventory_transfer_remove_items(
        &mut self,
        field: &RootFieldSelection,
    ) -> MutationFieldOutcome {
        let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
        let id = resolved_string_field(&input, "id").unwrap_or_default();
        let Some(existing) = self.store.staged.inventory_transfers.get(&id).cloned() else {
            return MutationFieldOutcome::unlogged(
                self.inventory_transfer_missing_payload(&field.selection, "inventoryTransfer"),
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
        let payload = selected_json(
            &json!({
                "inventoryTransfer": self.inventory_transfer_full_json(&record),
                "removedQuantities": removed,
                "userErrors": []
            }),
            &field.selection,
        );
        self.store
            .staged
            .inventory_transfers
            .insert(id.clone(), record);
        MutationFieldOutcome::staged(
            payload,
            LogDraft::staged("inventoryTransferRemoveItems", "products", vec![id]),
        )
    }

    pub(in crate::proxy) fn inventory_transfer_cancel(
        &mut self,
        field: &RootFieldSelection,
    ) -> MutationFieldOutcome {
        let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
        let Some(existing) = self.store.staged.inventory_transfers.get(&id).cloned() else {
            return MutationFieldOutcome::unlogged(
                self.inventory_transfer_missing_payload(&field.selection, "inventoryTransfer"),
            );
        };
        let mut record = existing;
        if record.status == "READY_TO_SHIP" {
            self.apply_transfer_reservations(&record, -1);
        }
        record.status = "CANCELED".to_string();
        let payload =
            self.inventory_transfer_payload_json(&record, &field.selection, "inventoryTransfer");
        self.store
            .staged
            .inventory_transfers
            .insert(id.clone(), record);
        MutationFieldOutcome::staged(
            payload,
            LogDraft::staged("inventoryTransferCancel", "products", vec![id]),
        )
    }

    pub(in crate::proxy) fn inventory_transfer_delete(
        &mut self,
        field: &RootFieldSelection,
    ) -> MutationFieldOutcome {
        let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
        let Some(record) = self.store.staged.inventory_transfers.get(&id).cloned() else {
            return MutationFieldOutcome::unlogged(selected_json(
                &json!({
                    "deletedId": Value::Null,
                    "userErrors": [user_error_omit_code(["id"], "Inventory transfer not found.", None)]
                }),
                &field.selection,
            ));
        };
        if record.status != "DRAFT" {
            return MutationFieldOutcome::unlogged(selected_json(
                &json!({
                    "deletedId": Value::Null,
                    "userErrors": [user_error_omit_code(["id"], "Can't delete the transfer if it's not in the draft status.", None)]
                }),
                &field.selection,
            ));
        }
        self.store.staged.inventory_transfers.remove(&id);
        MutationFieldOutcome::staged(
            selected_json(
                &json!({ "deletedId": id, "userErrors": [] }),
                &field.selection,
            ),
            LogDraft::staged("inventoryTransferDelete", "products", Vec::new()),
        )
    }

    fn inventory_transfer_payload_json(
        &self,
        record: &InventoryTransferRecord,
        selection: &[SelectedField],
        transfer_field: &str,
    ) -> Value {
        selected_json(
            &json!({
                transfer_field: self.inventory_transfer_full_json(record),
                "userErrors": []
            }),
            selection,
        )
    }

    fn inventory_transfer_user_error_payload(
        &self,
        selection: &[SelectedField],
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
        selected_json(&Value::Object(payload), selection)
    }

    fn inventory_transfer_missing_payload(
        &self,
        selection: &[SelectedField],
        transfer_field: &str,
    ) -> Value {
        selected_json(
            &json!({
                transfer_field: Value::Null,
                "userErrors": [user_error_omit_code(["id"], "Inventory transfer not found.", None)]
            }),
            selection,
        )
    }

    fn inventory_transfer_by_id_selected_json(
        &self,
        id: &str,
        selection: &[SelectedField],
    ) -> Value {
        self.store
            .staged
            .inventory_transfers
            .get(id)
            .map(|record| selected_json(&self.inventory_transfer_full_json(record), selection))
            .unwrap_or(Value::Null)
    }

    fn inventory_transfers_connection_selected_json(
        &self,
        transfers: Vec<&InventoryTransferRecord>,
        selection: &[SelectedField],
    ) -> Value {
        let nodes = transfers
            .into_iter()
            .map(|record| self.inventory_transfer_full_json(record))
            .collect::<Vec<_>>();
        selected_json(
            &json!({
                "nodes": nodes,
                "pageInfo": empty_page_info()
            }),
            selection,
        )
    }

    fn inventory_transfer_full_json(&self, record: &InventoryTransferRecord) -> Value {
        let nodes = record
            .line_items
            .iter()
            .map(|line_item| {
                let shippable = if record.status == "READY_TO_SHIP" {
                    line_item.quantity
                } else {
                    0
                };
                json!({
                    "id": line_item.id,
                    "inventoryItem": { "id": line_item.inventory_item_id },
                    "totalQuantity": line_item.quantity,
                    "shippableQuantity": shippable,
                    "shippedQuantity": 0,
                    "processableQuantity": line_item.quantity,
                    "pickedForShipmentQuantity": 0
                })
            })
            .collect::<Vec<_>>();
        json!({
            "id": record.id,
            "name": record.name,
            "status": record.status,
            "totalQuantity": record.line_items.iter().map(|line_item| line_item.quantity).sum::<i64>(),
            "lineItems": {
                "nodes": nodes,
                "pageInfo": empty_page_info()
            }
        })
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
                self.store
                    .staged
                    .inventory_levels
                    .entry((
                        line_item.inventory_item_id.clone(),
                        default_location_id.to_string(),
                    ))
                    .or_insert_with(empty_inventory_quantities);
            }
            let origin = self
                .store
                .staged
                .inventory_levels
                .entry((
                    line_item.inventory_item_id.clone(),
                    record.origin_location_id.clone(),
                ))
                .or_insert_with(default_transfer_inventory_quantities);
            if origin.is_empty() {
                *origin = default_transfer_inventory_quantities();
            }
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
        // A transfer endpoint must be a real, active location. Each scenario seeds its
        // origin/destination via `locationAdd` (isActive: true), so this resolves the
        // status from the live staged location registry rather than a hardcoded
        // allow-list of capture-specific location ids.
        self.store
            .staged
            .locations
            .get(location_id)
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
        if self.store.staged.inventory_levels.contains_key(&(
            inventory_item_id.to_string(),
            origin_location_id.to_string(),
        )) {
            return true;
        }
        false
    }

    fn hydrate_inventory_transfer_references<'a>(
        &mut self,
        location_ids: impl IntoIterator<Item = &'a String>,
        line_item_inputs: &[BTreeMap<String, ResolvedValue>],
    ) {
        if self.config.read_mode == ReadMode::Snapshot {
            return;
        }
        let mut ids = Vec::new();
        for location_id in location_ids {
            if !location_id.is_empty() && !ids.iter().any(|id| id == location_id) {
                ids.push(location_id.clone());
            }
        }
        for item_input in line_item_inputs {
            let item_id = resolved_string_field(item_input, "inventoryItemId").unwrap_or_default();
            if !item_id.is_empty() && !ids.iter().any(|id| id == &item_id) {
                ids.push(item_id);
            }
        }
        if ids.is_empty() {
            return;
        }
        let request = Request {
            method: "POST".to_string(),
            path: "/admin/api/2025-01/graphql.json".to_string(),
            headers: BTreeMap::new(),
            body: json!({
                "query": INVENTORY_TRANSFER_HYDRATE_NODES_QUERY,
                "variables": { "ids": ids }
            })
            .to_string(),
        };
        let response = (self.upstream_transport)(request);
        if response.status >= 400 {
            return;
        }
        self.observe_inventory_transfer_hydration_response(&response.body);
    }

    fn observe_inventory_transfer_hydration_response(&mut self, body: &Value) {
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
                Some("Location") => self.merge_staged_location(&node, &[]),
                Some("InventoryItem") => self.stage_inventory_transfer_inventory_item(node),
                _ => {}
            }
        }
    }

    fn stage_inventory_transfer_inventory_item(&mut self, item: Value) {
        let Some(item_id) = item.get("id").and_then(Value::as_str).map(str::to_string) else {
            return;
        };
        let Some(variant) = item.get("variant") else {
            return;
        };
        let product = variant.get("product").cloned().unwrap_or_else(|| {
            json!({
                "id": shopify_gid("Product", resource_id_tail(&item_id)),
                "title": "",
                "handle": "",
                "status": "ACTIVE",
                "totalInventory": 0,
                "tracksInventory": item.get("tracked").and_then(Value::as_bool).unwrap_or(true)
            })
        });
        if let Some(product) = product_state_from_json(&product) {
            self.store.stage_observed_product(product);
        }
        let variant_id = variant
            .get("id")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| shopify_gid("ProductVariant", resource_id_tail(&item_id)));
        let product_id = variant
            .get("product")
            .and_then(|product| product.get("id"))
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| shopify_gid("Product", resource_id_tail(&item_id)));
        let mut variant_value = variant.clone();
        if let Some(fields) = variant_value.as_object_mut() {
            fields.insert("id".to_string(), json!(variant_id));
            fields.insert("productId".to_string(), json!(product_id));
            fields.insert(
                "inventoryItem".to_string(),
                json!({
                    "id": item_id,
                    "tracked": item.get("tracked").and_then(Value::as_bool).unwrap_or(true),
                    "requiresShipping": item.get("requiresShipping").and_then(Value::as_bool).unwrap_or(true)
                }),
            );
        }
        if let Some(variant) = product_variant_state_from_json(&variant_value) {
            self.store.stage_product_variant(variant);
        }
        for level in item
            .get("inventoryLevels")
            .and_then(|connection| connection.get("nodes"))
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let Some(location_id) = level
                .get("location")
                .and_then(|location| location.get("id"))
                .and_then(Value::as_str)
                .map(str::to_string)
            else {
                continue;
            };
            let mut quantities = BTreeMap::new();
            for quantity in level
                .get("quantities")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                let Some(name) = quantity.get("name").and_then(Value::as_str) else {
                    continue;
                };
                quantities.insert(
                    name.to_string(),
                    quantity
                        .get("quantity")
                        .and_then(Value::as_i64)
                        .unwrap_or_default(),
                );
            }
            self.store
                .staged
                .inventory_levels
                .insert((item_id.clone(), location_id.clone()), quantities);
            if let Some(location) = level.get("location") {
                self.merge_staged_location(location, &[]);
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
        let level = self
            .store
            .staged
            .inventory_levels
            .entry((inventory_item_id.to_string(), location_id.to_string()))
            .or_insert_with(default_transfer_inventory_quantities);
        *level.entry("available".to_string()).or_insert(0) -= reserved_delta;
        *level.entry("reserved".to_string()).or_insert(0) += reserved_delta;
        let available = level.get("available").copied().unwrap_or(0);
        let reserved = level.get("reserved").copied().unwrap_or(0);
        level
            .entry("on_hand".to_string())
            .or_insert(available + reserved);
    }
}

fn inventory_quantity_missing_change_from_payload(
    field: &RootFieldSelection,
    root_field: &str,
    input_type: &str,
    rows: &[BTreeMap<String, ResolvedValue>],
    quantity_field: &str,
) -> Option<Value> {
    if rows
        .iter()
        .any(|row| row.contains_key("changeFromQuantity"))
        || rows.iter().any(|row| row.contains_key("compareQuantity"))
    {
        return None;
    }
    if rows.iter().any(|row| row.contains_key(quantity_field)) {
        return Some(json!({
            "__topLevelErrors": [{
                "message": format!("{input_type} must include the following argument: changeFromQuantity."),
                "locations": [
                    { "line": field.location.line, "column": field.location.column },
                    { "line": field.location.line.saturating_sub(1).max(1), "column": 1 }
                ],
                "extensions": { "code": "INVALID_FIELD_ARGUMENTS" },
                "path": [root_field]
            }]
        }));
    }
    None
}

fn inventory_adjust_requires_change_from(request: &Request) -> bool {
    admin_graphql_version(&request.path).is_some_and(|version| version_at_least(version, 2026, 4))
}

fn inventory_set_requires_change_from(request: &Request, field: &RootFieldSelection) -> bool {
    admin_graphql_version(&request.path).is_some_and(|version| version_at_least(version, 2026, 4))
        && inventory_field_has_idempotent(field)
}

fn inventory_requires_idempotency(request: &Request) -> bool {
    admin_graphql_version(&request.path).is_some_and(|version| version_at_least(version, 2026, 4))
}

fn inventory_field_has_idempotent(field: &RootFieldSelection) -> bool {
    field
        .directives
        .iter()
        .any(|directive| directive == "idempotent")
}

fn inventory_idempotency_required_payload(field: &RootFieldSelection, root_field: &str) -> Value {
    json!({
        "__topLevelErrors": [{
            "message": "The @idempotent directive is required for this mutation but was not provided.",
            "locations": [{ "line": field.location.line, "column": field.location.column }],
            "extensions": { "code": "BAD_REQUEST" },
            "path": [root_field]
        }]
    })
}

fn empty_inventory_quantities() -> BTreeMap<String, i64> {
    BTreeMap::from([
        ("available".to_string(), 0),
        ("reserved".to_string(), 0),
        ("on_hand".to_string(), 0),
        ("incoming".to_string(), 0),
    ])
}

fn default_transfer_inventory_quantities() -> BTreeMap<String, i64> {
    BTreeMap::from([
        ("available".to_string(), 5),
        ("reserved".to_string(), 0),
        ("on_hand".to_string(), 5),
    ])
}

fn inventory_id_tail_is_missing(tail: &str) -> bool {
    tail.is_empty()
        || COMMON_MISSING_INVENTORY_ID_TAILS
            .iter()
            .any(|sentinel| tail.eq_ignore_ascii_case(sentinel))
}

fn inventory_shipment_tracking_from_input(
    input: &BTreeMap<String, ResolvedValue>,
) -> Option<InventoryShipmentTrackingRecord> {
    let tracking = resolved_object_field(input, "trackingInput").unwrap_or_else(|| input.clone());
    let record = InventoryShipmentTrackingRecord {
        tracking_number: resolved_string_field(&tracking, "trackingNumber"),
        company: resolved_string_field(&tracking, "company")
            .or_else(|| resolved_string_field(&tracking, "carrier")),
        tracking_url: resolved_string_field(&tracking, "trackingUrl")
            .or_else(|| resolved_string_field(&tracking, "url")),
        arrives_at: resolved_string_field(&tracking, "arrivesAt"),
    };
    (record.tracking_number.is_some()
        || record.company.is_some()
        || record.tracking_url.is_some()
        || record.arrives_at.is_some())
    .then_some(record)
}

fn inventory_shipment_tracking_errors(input: &BTreeMap<String, ResolvedValue>) -> Vec<Value> {
    let mut errors = Vec::new();
    let carrier = resolved_string_field(input, "carrier");
    if carrier
        .as_deref()
        .is_some_and(|value| !is_valid_tracking_carrier(value))
    {
        errors.push(inventory_shipment_user_error(
            vec!["input", "trackingInput", "carrier"],
            "Carrier is not included in the list.",
            "INVALID",
        ));
    }
    let tracking_url =
        resolved_string_field(input, "url").or_else(|| resolved_string_field(input, "trackingUrl"));
    if tracking_url
        .as_deref()
        .is_some_and(|url| !(url.starts_with("https://") || url.starts_with("http://")))
    {
        errors.push(inventory_shipment_user_error(
            vec!["input", "trackingInput", "url"],
            "Tracking URL is invalid.",
            "INVALID",
        ));
    }
    errors
}

fn is_valid_tracking_carrier(carrier: &str) -> bool {
    !carrier.trim().is_empty()
}

fn inventory_shipment_user_error(field_path: Vec<&str>, message: &str, code: &str) -> Value {
    user_error(field_path, message, Some(code))
}

fn inventory_shipment_has_incoming(record: &InventoryShipmentRecord) -> bool {
    matches!(record.status.as_str(), "IN_TRANSIT" | "PARTIALLY_RECEIVED")
}

impl InventoryShipmentRecord {
    fn line_item_total_quantity(&self) -> i64 {
        self.line_items
            .iter()
            .map(|line_item| line_item.quantity)
            .sum()
    }

    fn total_accepted_quantity(&self) -> i64 {
        self.line_items
            .iter()
            .map(|line_item| line_item.accepted_quantity)
            .sum()
    }

    fn total_rejected_quantity(&self) -> i64 {
        self.line_items
            .iter()
            .map(|line_item| line_item.rejected_quantity)
            .sum()
    }

    fn total_received_quantity(&self) -> i64 {
        self.line_items
            .iter()
            .map(|line_item| line_item.received_quantity())
            .sum()
    }

    fn unreceived_quantity(&self) -> i64 {
        self.line_items
            .iter()
            .map(|line_item| line_item.unreceived_quantity())
            .sum()
    }
}

impl InventoryShipmentLineItemRecord {
    fn received_quantity(&self) -> i64 {
        self.accepted_quantity + self.rejected_quantity
    }

    fn unreceived_quantity(&self) -> i64 {
        (self.quantity - self.received_quantity()).max(0)
    }
}

fn inventory_quantities_from_observed_rows(rows: &[Value]) -> BTreeMap<String, i64> {
    let mut quantities = empty_inventory_quantities();
    for row in rows {
        let Some(name) = row.get("name").and_then(Value::as_str) else {
            continue;
        };
        let Some(quantity) = row.get("quantity").and_then(Value::as_i64) else {
            continue;
        };
        quantities.insert(name.to_string(), quantity);
    }
    quantities
}

fn inventory_deactivate_user_error(message: &str) -> Value {
    user_error_omit_code(Value::Null, message, None)
}

fn inventory_activate_user_error(field: impl Into<UserErrorField>, message: &str) -> Value {
    user_error_omit_code(field, message, None)
}

fn inventory_item_update_variable_errors(
    field: &RootFieldSelection,
    input: &BTreeMap<String, ResolvedValue>,
) -> Option<Vec<Value>> {
    let measurement = resolved_object_field(input, "measurement")?;
    let weight = resolved_object_field(&measurement, "weight")?;
    let unit = resolved_string_field(&weight, "unit")?;
    if INVENTORY_ITEM_WEIGHT_UNITS
        .iter()
        .any(|candidate| *candidate == unit)
    {
        return None;
    }
    Some(vec![json!({
        "message": format!("Variable $input of type InventoryItemInput! was provided invalid value for measurement.weight.unit (Expected \"{}\" to be one of: {})", unit, INVENTORY_ITEM_WEIGHT_UNITS.join(", ")),
        "locations": [{ "line": field.location.line.saturating_sub(1).max(1), "column": 52 }],
        "extensions": {
            "code": "INVALID_VARIABLE",
            "value": resolved_value_json(&ResolvedValue::Object(input.clone())),
            "problems": [{
                "path": ["measurement", "weight", "unit"],
                "explanation": format!("Expected \"{}\" to be one of: {}", unit, INVENTORY_ITEM_WEIGHT_UNITS.join(", "))
            }]
        }
    })])
}

fn inventory_item_update_user_errors(input: &BTreeMap<String, ResolvedValue>) -> Vec<Value> {
    let mut errors = Vec::new();
    if resolved_f64_path(input, &["cost"]).is_some_and(|cost| cost < 0.0) {
        errors.push(user_error_omit_code(
            inventory_item_update_field_path(&["input", "cost"]),
            "Cost must be greater than or equal to 0",
            Some("INVALID"),
        ));
    }
    if let Some(weight) = resolved_object_field(input, "measurement")
        .and_then(|measurement| resolved_object_field(&measurement, "weight"))
    {
        if let Some(value) = resolved_f64_path(&weight, &["value"]) {
            if value < 0.0 {
                errors.push(user_error_omit_code(
                    inventory_item_update_field_path(&["input", "measurement", "weight"]),
                    &format!(
                        "Measurement weight value {} kg must be >= 0 kg",
                        shopify_number_text(value)
                    ),
                    Some("INVALID"),
                ));
            }
        }
    }
    if let Some(country_code) = resolved_string_field(input, "countryCodeOfOrigin") {
        if !is_valid_country_code(&country_code) {
            errors.push(user_error_omit_code(
                inventory_item_update_field_path(&["input", "countryCodeOfOrigin"]),
                "Country code of origin is invalid",
                Some("INVALID"),
            ));
        }
    }
    if let Some(province_code) = resolved_string_field(input, "provinceCodeOfOrigin") {
        if province_code.len() > 3 || !province_code.chars().all(|ch| ch.is_ascii_alphabetic()) {
            errors.push(user_error_omit_code(
                inventory_item_update_field_path(&["input", "provinceCodeOfOrigin"]),
                "Province code of origin is invalid",
                Some("INVALID"),
            ));
        }
    }
    if let Some(hs_code) = resolved_string_field(input, "harmonizedSystemCode") {
        if !valid_harmonized_system_code(&hs_code) {
            errors.push(user_error_omit_code(
                inventory_item_update_field_path(&["input", "harmonizedSystemCode"]),
                "Harmonized system code must be a number between six and thirteen digits",
                Some("INVALID"),
            ));
        }
    }
    let mut seen_country_codes = BTreeSet::new();
    for (index, row) in resolved_object_list_field(input, "countryHarmonizedSystemCodes")
        .iter()
        .enumerate()
    {
        if let Some(country_code) = resolved_string_field(row, "countryCode") {
            if !is_valid_country_code(&country_code) {
                errors.push(user_error_omit_code(
                    inventory_item_update_field_path(&["input", "countryHarmonizedSystemCodes"]),
                    "Country code is invalid",
                    Some("INVALID"),
                ));
            } else if !seen_country_codes.insert(country_code) {
                errors.push(user_error_omit_code(
                    vec![
                        "input".to_string(),
                        "countryHarmonizedSystemCodes".to_string(),
                        index.to_string(),
                        "countryCode".to_string(),
                    ],
                    "Country code has already been taken",
                    Some("TAKEN"),
                ));
            }
        }
        if let Some(hs_code) = resolved_string_field(row, "harmonizedSystemCode") {
            if !valid_harmonized_system_code(&hs_code) {
                errors.push(user_error_omit_code(
                    inventory_item_update_field_path(&["input", "countryHarmonizedSystemCodes"]),
                    "Harmonized system code must be a number between six and thirteen digits",
                    Some("INVALID"),
                ));
            }
        }
    }
    errors
}

fn inventory_item_update_field_path(parts: &[&str]) -> Vec<String> {
    parts.iter().map(|part| (*part).to_string()).collect()
}

fn is_valid_country_code(country_code: &str) -> bool {
    INVENTORY_VALID_COUNTRY_CODES.contains(&country_code)
}

fn valid_harmonized_system_code(value: &str) -> bool {
    let normalized = normalized_harmonized_system_code(value);
    (6..=13).contains(&normalized.len()) && normalized.chars().all(|ch| ch.is_ascii_digit())
}

fn resolved_harmonized_system_code_json(value: &ResolvedValue) -> Value {
    match value {
        ResolvedValue::String(value) => json!(normalized_harmonized_system_code(value)),
        _ => resolved_value_json(value),
    }
}

fn normalized_harmonized_system_code(value: &str) -> String {
    value.chars().filter(char::is_ascii_alphanumeric).collect()
}

fn shopify_number_text(value: f64) -> String {
    if value.fract() == 0.0 {
        format!("{}", value as i64)
    } else {
        value.to_string()
    }
}

fn inventory_invalid_reason_payload(
    field: &RootFieldSelection,
    input: &BTreeMap<String, ResolvedValue>,
) -> Option<Value> {
    let reason = resolved_string_field(input, "reason").unwrap_or_else(|| "correction".to_string());
    if INVENTORY_VALID_REASONS.iter().any(|valid| *valid == reason) {
        return None;
    }
    Some(inventory_invalid_adjustment_payload(
        field,
        vec![user_error(
            ["input", "reason"],
            &format!(
                "The specified reason is invalid. Valid values are: {}.",
                INVENTORY_VALID_REASONS.join(", ")
            ),
            Some("INVALID_REASON"),
        )],
    ))
}

fn inventory_invalid_public_quantity_name_payload(
    field: &RootFieldSelection,
    name: &str,
    path: Value,
) -> Option<Value> {
    if INVENTORY_PUBLIC_ADJUST_QUANTITY_NAMES.contains(&name) {
        return None;
    }
    Some(inventory_invalid_adjustment_payload(
        field,
        vec![user_error(
            json!(path),
            INVENTORY_INVALID_PUBLIC_QUANTITY_NAME_MESSAGE,
            Some("INVALID_QUANTITY_NAME"),
        )],
    ))
}

fn inventory_invalid_set_quantity_name_payload(
    field: &RootFieldSelection,
    name: &str,
) -> Option<Value> {
    if INVENTORY_SET_QUANTITY_NAMES.contains(&name) {
        return None;
    }
    Some(inventory_invalid_adjustment_payload(
        field,
        vec![user_error(
            ["input", "name"],
            INVENTORY_INVALID_SET_QUANTITY_NAME_MESSAGE,
            Some("INVALID_NAME"),
        )],
    ))
}

fn inventory_invalid_set_quantities_payload(
    field: &RootFieldSelection,
    quantities: &[BTreeMap<String, ResolvedValue>],
    name: &str,
) -> Option<Value> {
    let mut errors = Vec::new();
    for (index, quantity) in quantities.iter().enumerate() {
        if let Some(value) = resolved_int_field(quantity, "quantity") {
            if value < INVENTORY_SET_QUANTITY_MIN {
                errors.push(user_error(
                    json!(["input", "quantities", index.to_string(), "quantity"]),
                    "The quantity can't be lower than -1,000,000,000.",
                    Some("INVALID_QUANTITY_TOO_LOW"),
                ));
            } else if name != "available" && value < 0 {
                errors.push(user_error(
                    json!(["input", "quantities", index.to_string(), "quantity"]),
                    "The quantity can't be negative.",
                    Some("INVALID_QUANTITY_NEGATIVE"),
                ));
            } else if value > INVENTORY_SET_QUANTITY_MAX {
                errors.push(user_error(
                    json!(["input", "quantities", index.to_string(), "quantity"]),
                    "The quantity can't be higher than 1,000,000,000.",
                    Some("INVALID_QUANTITY_TOO_HIGH"),
                ));
            }
        }
    }

    let mut indexes_by_pair: BTreeMap<(String, String), Vec<usize>> = BTreeMap::new();
    for (index, quantity) in quantities.iter().enumerate() {
        let item_id = resolved_string_field(quantity, "inventoryItemId").unwrap_or_default();
        let location_id = resolved_string_field(quantity, "locationId").unwrap_or_default();
        indexes_by_pair
            .entry((item_id, location_id))
            .or_default()
            .push(index);
    }
    let duplicate_indexes: BTreeSet<usize> = indexes_by_pair
        .values()
        .filter(|indexes| indexes.len() > 1)
        .flat_map(|indexes| indexes.iter().copied())
        .collect();
    for index in duplicate_indexes {
        errors.push(user_error(
            json!(["input", "quantities", index.to_string(), "locationId"]),
            "The combination of inventoryItemId and locationId must be unique.",
            Some("NO_DUPLICATE_INVENTORY_ITEM_ID_GROUP_ID_PAIR"),
        ));
    }

    if errors.is_empty() {
        return None;
    }
    Some(inventory_invalid_adjustment_payload(field, errors))
}

fn inventory_invalid_set_on_hand_quantities_payload(
    field: &RootFieldSelection,
    set_quantities: &[BTreeMap<String, ResolvedValue>],
) -> Option<Value> {
    let mut errors = Vec::new();
    for (index, quantity) in set_quantities.iter().enumerate() {
        if resolved_int_field(quantity, "quantity").is_some_and(|value| value < 0) {
            errors.push(json!({
                "field": ["input", "setQuantities", index.to_string(), "quantity"],
                "message": "The quantity can't be negative.",
                "code": "INVALID_QUANTITY_NEGATIVE"
            }));
        }
        if resolved_int_field(quantity, "quantity")
            .is_some_and(|value| value > INVENTORY_SET_QUANTITY_MAX)
        {
            errors.push(json!({
                "field": ["input", "setQuantities", index.to_string(), "quantity"],
                "message": "The quantity can't be higher than 1,000,000,000.",
                "code": "INVALID_QUANTITY_TOO_HIGH"
            }));
        }
    }

    if errors.is_empty() {
        return None;
    }
    Some(inventory_invalid_adjustment_payload(field, errors))
}

fn inventory_invalid_adjustment_payload(
    field: &RootFieldSelection,
    user_errors: Vec<Value>,
) -> Value {
    selected_json(
        &json!({
            "inventoryAdjustmentGroup": null,
            "userErrors": user_errors
        }),
        &field.selection,
    )
}

fn inventory_existence_error_payload(
    field: &RootFieldSelection,
    user_errors: Vec<Value>,
) -> Option<Value> {
    (!user_errors.is_empty()).then(|| inventory_invalid_adjustment_payload(field, user_errors))
}

fn inventory_input_path(list_key: &str, index: usize, field_path: &[&str]) -> Vec<String> {
    let mut path = vec!["input".to_string(), list_key.to_string(), index.to_string()];
    path.extend(field_path.iter().map(|segment| (*segment).to_string()));
    path
}

fn inventory_unknown_inventory_item_error(field: Vec<String>) -> Value {
    user_error(
        field,
        "The specified inventory item could not be found.",
        Some("INVALID_INVENTORY_ITEM"),
    )
}

fn inventory_unknown_location_error(field: Vec<String>) -> Value {
    user_error(
        field,
        "The specified location could not be found.",
        Some("INVALID_LOCATION"),
    )
}

use super::*;

mod items;
mod shipments;
mod transfers;

pub(in crate::proxy) struct InventoryLevelViewState<'a> {
    pub inventory_level_ids: &'a BTreeMap<(String, String), String>,
    pub inactive_levels: &'a BTreeSet<(String, String)>,
    pub quantity_updated_at: &'a BTreeMap<(String, String, String), String>,
    pub staged_locations: &'a BTreeMap<String, Value>,
    pub observed_shipping_locations: &'a BTreeMap<String, Value>,
    pub fulfillment_service_locations: &'a BTreeMap<String, Value>,
}

fn inventory_level_location_for_view(
    location_id: &str,
    view_state: &InventoryLevelViewState<'_>,
) -> Value {
    view_state
        .staged_locations
        .get(location_id)
        .or_else(|| view_state.observed_shipping_locations.get(location_id))
        .or_else(|| view_state.fulfillment_service_locations.get(location_id))
        .cloned()
        .unwrap_or_else(|| json!({ "id": location_id }))
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
            "location" => Some(selected_json(
                &inventory_level_location_for_view(location_id, view_state),
                &selection.selection,
            )),
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

pub(in crate::proxy) fn inventory_level_id_tail_and_query(id: &str) -> Option<(&str, &str)> {
    let rest = shopify_gid_tail_for_type(id, "InventoryLevel")?;
    rest.split_once("?inventory_item_id=")
}

pub(in crate::proxy) fn inventory_level_parts_from_id(id: &str) -> Option<(String, String)> {
    let (level_tail, query) = inventory_level_id_tail_and_query(id)?;
    let (item_tail, location_tail) = level_tail.rsplit_once('-')?;
    let item_id = if is_shopify_gid_of_type(query, "InventoryItem") {
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

fn inventory_adjust_name_mirrors_on_hand(name: &str) -> bool {
    matches!(
        name,
        "available" | "damaged" | "quality_control" | "reserved" | "safety_stock"
    )
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
            staged_locations: &self.store.staged.locations.records,
            observed_shipping_locations: &self.store.staged.observed_shipping_locations,
            fulfillment_service_locations: &self.store.staged.fulfillment_service_locations.records,
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
                    if !self.inventory_item_exists(&id) {
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
                    self.store
                        .staged
                        .inventory_transfers
                        .values()
                        .cloned()
                        .collect(),
                    &field.arguments,
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
        errors.push(user_error(
            vec!["input", "trackingInput", "carrier"],
            "Carrier is not included in the list.",
            Some("INVALID"),
        ));
    }
    let tracking_url =
        resolved_string_field(input, "url").or_else(|| resolved_string_field(input, "trackingUrl"));
    if tracking_url
        .as_deref()
        .is_some_and(|url| !(url.starts_with("https://") || url.starts_with("http://")))
    {
        errors.push(user_error(
            vec!["input", "trackingInput", "url"],
            "Tracking URL is invalid.",
            Some("INVALID"),
        ));
    }
    errors
}

fn is_valid_tracking_carrier(carrier: &str) -> bool {
    !carrier.trim().is_empty()
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
            ["input", "cost"],
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
                    ["input", "measurement", "weight"],
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
                ["input", "countryCodeOfOrigin"],
                "Country code of origin is invalid",
                Some("INVALID"),
            ));
        }
    }
    if let Some(province_code) = resolved_string_field(input, "provinceCodeOfOrigin") {
        if province_code.len() > 3 || !province_code.chars().all(|ch| ch.is_ascii_alphabetic()) {
            errors.push(user_error_omit_code(
                ["input", "provinceCodeOfOrigin"],
                "Province code of origin is invalid",
                Some("INVALID"),
            ));
        }
    }
    if let Some(hs_code) = resolved_string_field(input, "harmonizedSystemCode") {
        if !valid_harmonized_system_code(&hs_code) {
            errors.push(user_error_omit_code(
                ["input", "harmonizedSystemCode"],
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
                    ["input", "countryHarmonizedSystemCodes"],
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
                    ["input", "countryHarmonizedSystemCodes"],
                    "Harmonized system code must be a number between six and thirteen digits",
                    Some("INVALID"),
                ));
            }
        }
    }
    errors
}

fn inventory_shipment_user_error(field_path: Vec<&str>, message: &str, code: &str) -> Value {
    user_error(field_path, message, Some(code))
}

fn inventory_deactivate_user_error(message: &str) -> Value {
    user_error_omit_code(Value::Null, message, None)
}

fn inventory_activate_user_error(field: impl Into<UserErrorField>, message: &str) -> Value {
    user_error_omit_code(field, message, None)
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

fn inventory_invalid_adjust_ledger_document_payload(
    field: &RootFieldSelection,
    changes: &[BTreeMap<String, ResolvedValue>],
    name: &str,
) -> Option<Value> {
    let distinct_ledgers = changes
        .iter()
        .filter_map(|change| resolved_string_field(change, "ledgerDocumentUri"))
        .collect::<BTreeSet<_>>();
    if distinct_ledgers.len() > 1 {
        return Some(inventory_invalid_adjustment_payload(
            field,
            vec![user_error(
                ["input", "changes"],
                "All changes must have the same ledger document URI or, in the case of adjusting available, no ledger document URI.",
                Some("MAX_ONE_LEDGER_DOCUMENT"),
            )],
        ));
    }

    for (index, change) in changes.iter().enumerate() {
        let ledger = resolved_string_field(change, "ledgerDocumentUri");
        let field_path = json!(["input", "changes", index.to_string(), "ledgerDocumentUri"]);
        match (name == "available", ledger.as_deref()) {
            (true, Some(_)) => {
                return Some(inventory_invalid_adjustment_payload(
                    field,
                    vec![user_error(
                        field_path,
                        "A ledger document URI is not allowed when adjusting available.",
                        Some("INVALID_AVAILABLE_DOCUMENT"),
                    )],
                ));
            }
            (false, None) => {
                return Some(inventory_invalid_adjustment_payload(
                    field,
                    vec![user_error(
                        field_path,
                        "A ledger document URI is required except when adjusting available.",
                        Some("INVALID_QUANTITY_DOCUMENT"),
                    )],
                ));
            }
            (_, Some(ledger)) if has_shopify_gid_prefix(ledger) => {
                return Some(inventory_invalid_adjustment_payload(
                    field,
                    vec![user_error(
                        field_path,
                        "Internal (gid://shopify/) ledger documents are not allowed to be adjusted via API.",
                        Some("INTERNAL_LEDGER_DOCUMENT"),
                    )],
                ));
            }
            _ => {}
        }
    }

    None
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
            errors.push(user_error(
                vec![
                    "input".to_string(),
                    "setQuantities".to_string(),
                    index.to_string(),
                    "quantity".to_string(),
                ],
                "The quantity can't be negative.",
                Some("INVALID_QUANTITY_NEGATIVE"),
            ));
        }
        if resolved_int_field(quantity, "quantity")
            .is_some_and(|value| value > INVENTORY_SET_QUANTITY_MAX)
        {
            errors.push(user_error(
                vec![
                    "input".to_string(),
                    "setQuantities".to_string(),
                    index.to_string(),
                    "quantity".to_string(),
                ],
                "The quantity can't be higher than 1,000,000,000.",
                Some("INVALID_QUANTITY_TOO_HIGH"),
            ));
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

fn inventory_search_terms(query: &str) -> Vec<String> {
    query
        .split_whitespace()
        .map(str::trim)
        .filter(|term| !term.is_empty())
        .map(str::to_string)
        .collect()
}

fn inventory_unquoted_query_value(raw: &str) -> String {
    let value = raw.trim();
    if let Some(inner) = value
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
    {
        return inner.to_string();
    }
    if let Some(inner) = value
        .strip_prefix('\'')
        .and_then(|rest| rest.strip_suffix('\''))
    {
        return inner.to_string();
    }
    value.to_string()
}

fn inventory_search_comparator(value: &str) -> (&str, &str) {
    let value = value.trim();
    comparison_operator_prefix(value, &[">=", "<=", ">", "<", "="]).unwrap_or(("=", value))
}

fn inventory_search_string_matches(actual: &str, expected: &str) -> bool {
    let actual = actual.to_ascii_lowercase();
    let expected = expected.to_ascii_lowercase();
    !expected.is_empty() && actual.contains(&expected)
}

fn inventory_id_matches_query(id: &str, raw_value: &str) -> bool {
    let (operator, expected) = inventory_search_comparator(raw_value);
    let expected = inventory_unquoted_query_value(expected);
    if expected.is_empty() {
        return false;
    }
    let actual_tail = resource_id_tail(id);
    let expected_tail = if has_shopify_gid_prefix(&expected) {
        resource_id_tail(&expected).to_string()
    } else {
        expected.clone()
    };
    if operator == "=" {
        return id.eq_ignore_ascii_case(&expected)
            || actual_tail.eq_ignore_ascii_case(&expected_tail);
    }
    match (actual_tail.parse::<i64>(), expected_tail.parse::<i64>()) {
        (Ok(actual), Ok(expected)) => inventory_compare_ordering(actual.cmp(&expected), operator),
        _ => inventory_compare_ordering(
            actual_tail
                .to_ascii_lowercase()
                .cmp(&expected_tail.to_ascii_lowercase()),
            operator,
        ),
    }
}

fn inventory_compare_ordering(ordering: std::cmp::Ordering, operator: &str) -> bool {
    match operator {
        "<" => ordering.is_lt(),
        "<=" => ordering.is_lt() || ordering.is_eq(),
        ">" => ordering.is_gt(),
        ">=" => ordering.is_gt() || ordering.is_eq(),
        _ => ordering.is_eq(),
    }
}

fn inventory_datetime_matches_query(actual: Option<&str>, raw_value: &str) -> bool {
    let Some(actual) = actual.filter(|value| !value.is_empty()) else {
        return false;
    };
    let (operator, expected) = inventory_search_comparator(raw_value);
    let expected = inventory_unquoted_query_value(expected);
    if expected.is_empty() {
        return false;
    }
    let actual = if expected.contains('T') {
        actual
    } else {
        actual
            .split_once('T')
            .map(|(date, _)| date)
            .unwrap_or(actual)
    };
    inventory_compare_ordering(actual.cmp(expected.as_str()), operator)
        || (operator == "=" && actual.starts_with(&expected))
}

fn inventory_gid_sort_key(id: &str) -> StagedSortKey {
    vec![resource_id_tail_sort_value(Some(id))]
}

fn inventory_item_sort_key(inventory_item_id: &str, _sort_key: Option<&str>) -> StagedSortKey {
    inventory_gid_sort_key(inventory_item_id)
}

fn inventory_transfer_default_created_at(existing_count: usize) -> String {
    format!(
        "2024-01-01T00:00:{:02}.000Z",
        existing_count.saturating_add(1) % 60
    )
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

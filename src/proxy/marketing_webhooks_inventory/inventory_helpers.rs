use super::*;

pub(in crate::proxy) fn inventory_empty_connection(selection: &[SelectedField]) -> Value {
    selected_empty_connection_json(selection)
}

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

pub(in crate::proxy) fn inventory_location_name(location_id: &str) -> &'static str {
    match location_id {
        "gid://shopify/Location/1" => "Source location",
        "gid://shopify/Location/2" => "Destination location",
        "gid://shopify/Location/106318430514" => "Shop location",
        "gid://shopify/Location/106318463282" => "My Custom Location",
        _ => "Shop location",
    }
}

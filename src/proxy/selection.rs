use super::*;

pub(in crate::proxy) fn selected_json(record: &Value, selections: &[SelectedField]) -> Value {
    if record.is_null() {
        return Value::Null;
    }
    let typename = record.get("__typename").and_then(Value::as_str);
    let mut fields = serde_json::Map::new();
    for selection in selections {
        if !selected_field_applies_to_record(record, typename, selection) {
            continue;
        }
        let Some(value) = record.get(&selection.name) else {
            continue;
        };
        let value = if selection.selection.is_empty() {
            value.clone()
        } else if value.is_null() {
            Value::Null
        } else if let Some(values) = value.as_array() {
            quantities_selection_by_names(selection, values).unwrap_or_else(|| {
                Value::Array(
                    values
                        .iter()
                        .map(|item| nullable_selected_json(item, &selection.selection))
                        .collect(),
                )
            })
        } else {
            selected_json(value, &selection.selection)
        };
        fields.insert(selection.response_key.clone(), value);
    }
    if let Some(typename) = typename {
        fields
            .entry("__typename".to_string())
            .or_insert_with(|| json!(typename));
    }
    Value::Object(fields)
}

/// Normalize a value returned by an upstream GraphQL request back to schema
/// field names. This is the inverse boundary operation to output projection:
/// aliases belong to the caller-facing response, while domain/store code owns
/// canonical entity values. Keeping this at the transport boundary lets local
/// resolvers consume upstream evidence without learning about selections.
pub(in crate::proxy) fn canonicalize_upstream_value(
    value: &Value,
    selections: &[SelectedField],
) -> Value {
    if value.is_null() || selections.is_empty() {
        return value.clone();
    }
    if let Some(values) = value.as_array() {
        return Value::Array(
            values
                .iter()
                .map(|value| canonicalize_upstream_value(value, selections))
                .collect(),
        );
    }
    let Some(object) = value.as_object() else {
        return value.clone();
    };
    let mut canonical = serde_json::Map::new();
    for selection in selections {
        let Some(selected) = object
            .get(&selection.response_key)
            .or_else(|| object.get(&selection.name))
        else {
            continue;
        };
        let selected = if selection.selection.is_empty() || selected.is_null() {
            selected.clone()
        } else if let Some(values) = selected.as_array() {
            Value::Array(
                values
                    .iter()
                    .map(|value| canonicalize_upstream_value(value, &selection.selection))
                    .collect(),
            )
        } else {
            canonicalize_upstream_value(selected, &selection.selection)
        };
        canonical.insert(selection.name.clone(), selected);
    }
    Value::Object(canonical)
}

pub(in crate::proxy) fn selected_field_applies_to_record(
    record: &Value,
    typename: Option<&str>,
    selection: &SelectedField,
) -> bool {
    let Some(record_type) = record_type_name(record, typename) else {
        return true;
    };
    selected_field_applies_to_type(record_type, selection)
}

pub(in crate::proxy) fn selected_field_applies_to_type(
    type_name: &str,
    selection: &SelectedField,
) -> bool {
    selection.type_condition.as_deref().is_none_or(|condition| {
        condition == type_name
            || crate::admin_graphql::output_type_condition_applies(type_name, condition)
    })
}

pub(in crate::proxy) fn selection_contains_any(
    selections: &[SelectedField],
    names: &[&str],
) -> bool {
    selections.iter().any(|selection| {
        names.contains(&selection.name.as_str())
            || selection_contains_any(&selection.selection, names)
    })
}

pub(in crate::proxy) trait DispatchField {
    fn response_key(&self) -> &str;
}

impl DispatchField for SelectedField {
    fn response_key(&self) -> &str {
        self.response_key.as_str()
    }
}

impl DispatchField for RootFieldSelection {
    fn response_key(&self) -> &str {
        self.response_key.as_str()
    }
}

/// Honor the `quantities(names: [...])` argument on an `InventoryLevel` selection when
/// projecting a materialized quantity array. Shopify returns exactly one entry per
/// requested name, in request order, synthesizing a zero/`null` row for any name with
/// no recorded quantity. The generic projector is otherwise selection-only and would
/// echo every materialized row in storage order, so this filters/reorders to match the
/// argument. Returns `None` for any non-`quantities` field, an absent/empty `names`
/// list, or an array that is not shaped like quantity rows, leaving such arrays to the
/// default element-wise projection.
fn quantities_selection_by_names(selection: &SelectedField, values: &[Value]) -> Option<Value> {
    if selection.name != "quantities" {
        return None;
    }
    let ResolvedValue::List(name_values) = selection.arguments.get("names")? else {
        return None;
    };
    let names: Vec<&str> = name_values
        .iter()
        .filter_map(|value| match value {
            ResolvedValue::String(name) => Some(name.as_str()),
            _ => None,
        })
        .collect();
    if names.is_empty() {
        return None;
    }
    // Only intervene when the array looks like inventory quantity rows (each carries a
    // `name`), so unrelated `quantities` arrays fall through to the default projection.
    if !values.iter().all(|value| value.get("name").is_some()) {
        return None;
    }
    let rows = names
        .into_iter()
        .map(|name| {
            let row = values
                .iter()
                .find(|value| value.get("name").and_then(Value::as_str) == Some(name))
                .cloned()
                .unwrap_or_else(|| json!({ "name": name, "quantity": 0, "updatedAt": null }));
            nullable_selected_json(&row, &selection.selection)
        })
        .collect();
    Some(Value::Array(rows))
}

fn record_type_name<'a>(record: &'a Value, typename: Option<&'a str>) -> Option<&'a str> {
    typename.or_else(|| {
        record
            .get("id")
            .and_then(Value::as_str)
            .and_then(shopify_gid_resource_type)
    })
}

pub(in crate::proxy) fn nullable_selected_json(
    value: &Value,
    selection: &[SelectedField],
) -> Value {
    if value.is_null() {
        Value::Null
    } else if selection.is_empty() {
        value.clone()
    } else {
        selected_json(value, selection)
    }
}

pub(in crate::proxy) fn selected_payload_json<Field, ValueFor>(
    selections: &[Field],
    mut value_for: ValueFor,
) -> Value
where
    Field: DispatchField,
    ValueFor: FnMut(&Field) -> Option<Value>,
{
    let mut fields = serde_json::Map::new();
    for selection in selections {
        if let Some(value) = value_for(selection) {
            fields.insert(selection.response_key().to_string(), value);
        }
    }
    Value::Object(fields)
}

pub(in crate::proxy) fn root_payload_json<ValueFor>(
    selections: &[RootFieldSelection],
    value_for: ValueFor,
) -> Value
where
    ValueFor: FnMut(&RootFieldSelection) -> Option<Value>,
{
    selected_payload_json(selections, value_for)
}

pub(in crate::proxy) fn nested_selected_fields(
    selections: &[SelectedField],
    path: &[&str],
) -> Vec<SelectedField> {
    let Some((next, remaining)) = path.split_first() else {
        return selections.to_vec();
    };
    selections
        .iter()
        .find(|selection| selection.name == *next)
        .map(|selection| nested_selected_fields(&selection.selection, remaining))
        .unwrap_or_default()
}

pub(in crate::proxy) fn selected_child_selection(
    selections: &[SelectedField],
    name: &str,
) -> Option<Vec<SelectedField>> {
    selections
        .iter()
        .find(|selection| selection.name == name)
        .map(|selection| selection.selection.clone())
}

#[cfg(test)]
fn selected_fields_named(selections: &[SelectedField], names: &[&str]) -> Vec<SelectedField> {
    selections
        .iter()
        .filter(|selection| names.iter().any(|name| selection.name == *name))
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field(name: &str, response_key: &str, selection: Vec<SelectedField>) -> SelectedField {
        SelectedField {
            name: name.to_string(),
            response_key: response_key.to_string(),
            location: SourceLocation { line: 1, column: 1 },
            arguments: Default::default(),
            selection,
            type_condition: None,
        }
    }

    fn typed_field(
        name: &str,
        response_key: &str,
        type_condition: &str,
        selection: Vec<SelectedField>,
    ) -> SelectedField {
        SelectedField {
            type_condition: Some(type_condition.to_string()),
            ..field(name, response_key, selection)
        }
    }

    fn root_field(name: &str, response_key: &str) -> RootFieldSelection {
        RootFieldSelection {
            name: name.to_string(),
            response_key: response_key.to_string(),
            location: SourceLocation { line: 1, column: 1 },
            directives: Vec::new(),
            raw_directives: Vec::new(),
            raw_arguments: Default::default(),
            arguments: Default::default(),
            selection: Vec::new(),
        }
    }

    #[test]
    fn selected_json_preserves_aliases_and_nested_array_selection() {
        let record = json!({
            "id": "gid://shopify/Product/1",
            "title": "Hat",
            "variants": [
                { "id": "gid://shopify/ProductVariant/1", "title": "Red", "sku": "RED" }
            ],
            "seo": null,
            "ignored": true
        });
        let selections = vec![
            field("id", "legacyId", vec![]),
            field(
                "variants",
                "variants",
                vec![field("title", "variantTitle", vec![])],
            ),
            field("seo", "seo", vec![field("title", "title", vec![])]),
            field("missing", "missingAlias", vec![]),
        ];

        assert_eq!(
            selected_json(&record, &selections),
            json!({
                "legacyId": "gid://shopify/Product/1",
                "variants": [{ "variantTitle": "Red" }],
                "seo": null
            })
        );

        let record_without_typename = json!({
            "id": "gid://shopify/GenericFile/1",
            "url": "https://cdn.example.com/spec.pdf"
        });
        assert_eq!(
            selected_json(
                &record_without_typename,
                &[typed_field("url", "url", "GenericFile", vec![])]
            ),
            json!({"url": "https://cdn.example.com/spec.pdf"})
        );

        let nested_record_without_discriminator = json!({"content": "hello"});
        assert_eq!(
            selected_json(
                &nested_record_without_discriminator,
                &[typed_field(
                    "content",
                    "content",
                    "OnlineStoreThemeFileBodyText",
                    vec![]
                )]
            ),
            json!({"content": "hello"})
        );
    }

    #[test]
    fn canonicalize_upstream_value_removes_aliases_recursively() {
        let selections = vec![field(
            "nodes",
            "cards",
            vec![
                field("id", "identifier", vec![]),
                field(
                    "balance",
                    "remaining",
                    vec![field("amount", "value", vec![])],
                ),
            ],
        )];
        let upstream = json!({
            "cards": [{
                "identifier": "gid://shopify/GiftCard/1",
                "remaining": { "value": "12.50" }
            }]
        });

        assert_eq!(
            canonicalize_upstream_value(&upstream, &selections),
            json!({
                "nodes": [{
                    "id": "gid://shopify/GiftCard/1",
                    "balance": { "amount": "12.50" }
                }]
            })
        );
    }

    #[test]
    fn selected_json_filters_type_conditions_and_allows_file_interfaces() {
        let record = json!({
            "__typename": "GenericFile",
            "id": "gid://shopify/GenericFile/1",
            "url": "https://cdn.example.com/spec.pdf",
            "image": {"url": "https://cdn.example.com/spec.pdf"}
        });
        let selections = vec![
            field("__typename", "__typename", vec![]),
            typed_field("id", "id", "File", vec![]),
            typed_field("url", "url", "GenericFile", vec![]),
            typed_field("image", "image", "MediaImage", vec![]),
        ];

        assert_eq!(
            selected_json(&record, &selections),
            json!({
                "__typename": "GenericFile",
                "id": "gid://shopify/GenericFile/1",
                "url": "https://cdn.example.com/spec.pdf"
            })
        );
    }

    #[test]
    fn selected_json_allows_catalog_interface_fragments_for_catalog_types() {
        let record = json!({
            "__typename": "MarketCatalog",
            "id": "gid://shopify/MarketCatalog/1",
            "title": "Wholesale",
            "status": "ACTIVE",
            "name": "not a catalog field"
        });
        let selections = vec![
            typed_field("__typename", "__typename", "Catalog", vec![]),
            typed_field("title", "title", "Catalog", vec![]),
            typed_field("status", "status", "Catalog", vec![]),
            typed_field("name", "name", "Market", vec![]),
        ];

        assert_eq!(
            selected_json(&record, &selections),
            json!({
                "__typename": "MarketCatalog",
                "title": "Wholesale",
                "status": "ACTIVE"
            })
        );
    }

    #[test]
    fn selected_json_applies_store_credit_interface_fields_to_concrete_transactions() {
        let record = json!({
            "__typename": "StoreCreditAccountCreditTransaction",
            "event": "ADJUSTMENT",
            "remainingAmount": { "amount": "12.0", "currencyCode": "CAD" }
        });
        let selections = vec![
            typed_field("event", "event", "StoreCreditAccountTransaction", vec![]),
            typed_field(
                "remainingAmount",
                "remainingAmount",
                "StoreCreditAccountCreditTransaction",
                vec![field("amount", "amount", vec![])],
            ),
        ];

        assert_eq!(
            selected_json(&record, &selections),
            json!({
                "__typename": "StoreCreditAccountCreditTransaction",
                "event": "ADJUSTMENT",
                "remainingAmount": { "amount": "12.0" }
            })
        );
    }

    #[test]
    fn selected_payload_json_preserves_aliases_and_skips_missing_values() {
        let selections = vec![
            field("id", "legacyId", vec![]),
            field("title", "title", vec![]),
            field("missing", "missingAlias", vec![]),
        ];

        assert_eq!(
            selected_payload_json(&selections, |selection| match selection.name.as_str() {
                "id" => Some(json!("gid://shopify/Product/1")),
                "title" => Some(json!("Hat")),
                _ => None,
            }),
            json!({
                "legacyId": "gid://shopify/Product/1",
                "title": "Hat"
            })
        );
    }

    #[test]
    fn selected_payload_json_accepts_root_fields_and_keeps_projection_semantics() {
        let roots = vec![
            root_field("shop", "shopAlias"),
            root_field("unknownNull", "unknownNull"),
            root_field("unknownSkip", "unknownSkip"),
        ];

        assert_eq!(
            selected_payload_json(&roots, |field| match field.name.as_str() {
                "shop" => Some(json!({ "name": "Example Shop" })),
                "unknownNull" => Some(Value::Null),
                _ => None,
            }),
            json!({
                "shopAlias": { "name": "Example Shop" },
                "unknownNull": null
            })
        );
    }

    #[test]
    fn selection_lookup_helpers_return_requested_children_in_order() {
        let selections = vec![
            field("id", "id", vec![]),
            field(
                "connection",
                "connectionAlias",
                vec![
                    field(
                        "edges",
                        "edges",
                        vec![field("node", "node", vec![field("title", "title", vec![])])],
                    ),
                    field("nodes", "nodes", vec![field("id", "id", vec![])]),
                ],
            ),
            field("title", "name", vec![]),
        ];

        assert_eq!(
            nested_selected_fields(&selections, &["connection", "edges", "node"]),
            vec![field("title", "title", vec![])]
        );
        assert_eq!(
            selected_child_selection(&selections[1].selection, "nodes"),
            Some(vec![field("id", "id", vec![])])
        );
        assert_eq!(
            selected_fields_named(&selections, &["title", "id"]),
            vec![field("id", "id", vec![]), field("title", "name", vec![])]
        );
    }

    #[test]
    fn nullable_selected_json_keeps_shopify_null_behavior() {
        let null_value = Value::Null;
        assert_eq!(
            nullable_selected_json(&null_value, &[field("id", "id", vec![])]),
            Value::Null
        );

        let value = json!({ "id": "gid://shopify/App/1", "title": "Local app" });
        assert_eq!(nullable_selected_json(&value, &[]), value);
    }
}

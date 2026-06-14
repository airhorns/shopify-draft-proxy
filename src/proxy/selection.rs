use super::*;

pub(in crate::proxy) fn selected_json(record: &Value, selections: &[SelectedField]) -> Value {
    if record.is_null() {
        return Value::Null;
    }
    let mut fields = serde_json::Map::new();
    for selection in selections {
        let Some(value) = record.get(&selection.name) else {
            continue;
        };
        let value = if selection.selection.is_empty() {
            value.clone()
        } else if value.is_null() {
            Value::Null
        } else if let Some(values) = value.as_array() {
            Value::Array(
                values
                    .iter()
                    .map(|item| selected_json(item, &selection.selection))
                    .collect(),
            )
        } else {
            selected_json(value, &selection.selection)
        };
        fields.insert(selection.response_key.clone(), value);
    }
    Value::Object(fields)
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

pub(in crate::proxy) fn selected_payload_json<ValueFor>(
    selections: &[SelectedField],
    mut value_for: ValueFor,
) -> Value
where
    ValueFor: FnMut(&SelectedField) -> Option<Value>,
{
    let mut fields = serde_json::Map::new();
    for selection in selections {
        if let Some(value) = value_for(selection) {
            fields.insert(selection.response_key.clone(), value);
        }
    }
    Value::Object(fields)
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

pub(in crate::proxy) fn selected_fields_named(
    selections: &[SelectedField],
    names: &[&str],
) -> Vec<SelectedField> {
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
            arguments: Default::default(),
            selection,
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

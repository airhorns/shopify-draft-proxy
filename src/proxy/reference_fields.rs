use super::*;

fn reference_base_type(type_name: &str) -> Option<&str> {
    type_name
        .strip_prefix("list.")
        .unwrap_or(type_name)
        .strip_suffix("_reference")
}

fn is_reference_type(type_name: &str) -> bool {
    reference_base_type(type_name).is_some()
}

fn is_list_reference_type(type_name: &str) -> bool {
    type_name
        .strip_prefix("list.")
        .is_some_and(is_reference_type)
}

fn reference_record_type(record: &Value) -> Option<&str> {
    record.get("type").and_then(Value::as_str)
}

fn reference_record_scalar_id(record: &Value) -> Option<String> {
    record
        .get("value")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

pub(in crate::proxy) fn reference_record_list_ids(record: &Value) -> Vec<String> {
    if let Some(items) = record.get("jsonValue").and_then(Value::as_array) {
        return items
            .iter()
            .filter_map(|item| {
                item.as_str()
                    .filter(|id| !id.is_empty())
                    .map(str::to_string)
            })
            .collect();
    }
    record
        .get("value")
        .and_then(Value::as_str)
        .and_then(|value| serde_json::from_str::<Value>(value).ok())
        .and_then(|value| match value {
            Value::Array(items) => Some(
                items
                    .into_iter()
                    .filter_map(|item| match item {
                        Value::String(id) if !id.is_empty() => Some(id),
                        _ => None,
                    })
                    .collect(),
            ),
            _ => None,
        })
        .unwrap_or_default()
}

pub(in crate::proxy) fn reference_record_targets_id(record: &Value, target_id: &str) -> bool {
    let Some(type_name) = reference_record_type(record) else {
        return false;
    };
    if is_list_reference_type(type_name) {
        reference_record_list_ids(record)
            .iter()
            .any(|id| id == target_id)
    } else if is_reference_type(type_name) {
        reference_record_scalar_id(record).as_deref() == Some(target_id)
    } else {
        false
    }
}

impl DraftProxy {
    pub(in crate::proxy) fn selected_reference_field_record(
        &self,
        record: &Value,
        selections: &[SelectedField],
    ) -> Value {
        selected_payload_json(selections, |field| match field.name.as_str() {
            "reference" => Some(self.selected_reference_record_target(record, &field.selection)),
            "references" => Some(self.selected_reference_record_targets(record, field)),
            _ => record
                .get(&field.name)
                .map(|value| nullable_selected_json(value, &field.selection)),
        })
    }

    fn selected_reference_record_target(
        &self,
        record: &Value,
        selections: &[SelectedField],
    ) -> Value {
        let Some(type_name) = reference_record_type(record) else {
            return Value::Null;
        };
        if !is_reference_type(type_name) || is_list_reference_type(type_name) {
            return Value::Null;
        }
        let Some(id) = reference_record_scalar_id(record) else {
            return Value::Null;
        };
        let target_selection = reference_target_selection(&id, selections);
        self.local_node_value_by_id(&id, &target_selection)
            .map(|value| selected_reference_target_with_typename(value, &id, selections))
            .filter(|value| !value.is_null())
            .unwrap_or(Value::Null)
    }

    fn selected_reference_record_targets(&self, record: &Value, field: &SelectedField) -> Value {
        let Some(type_name) = reference_record_type(record) else {
            return Value::Null;
        };
        if !is_list_reference_type(type_name) {
            return Value::Null;
        }
        let targets = reference_record_list_ids(record)
            .into_iter()
            .filter(|id| {
                self.local_node_value_by_id(id, &[])
                    .is_some_and(|value| !value.is_null())
            })
            .collect::<Vec<_>>();
        selected_typed_connection_with_args(
            &targets,
            &field.arguments,
            &field.selection,
            |id, selection| {
                let target_selection = reference_target_selection(id, selection);
                self.local_node_value_by_id(id, &target_selection)
                    .map(|value| selected_reference_target_with_typename(value, id, selection))
                    .unwrap_or(Value::Null)
            },
            |id| id.clone(),
        )
    }
}

fn reference_target_selection(id: &str, selections: &[SelectedField]) -> Vec<SelectedField> {
    let Some(typename) = shopify_gid_resource_type(id) else {
        return selections.to_vec();
    };
    selections
        .iter()
        .filter(|field| {
            field
                .type_condition
                .as_deref()
                .is_none_or(|condition| reference_type_condition_matches(typename, condition))
        })
        .cloned()
        .map(|mut field| {
            field.type_condition = None;
            field
        })
        .collect()
}

fn reference_type_condition_matches(typename: &str, condition: &str) -> bool {
    condition == typename
        || condition == "Node"
        || (condition == "File"
            && matches!(
                typename,
                "MediaImage" | "Video" | "GenericFile" | "Model3d" | "ExternalVideo"
            ))
        || (condition == "Media"
            && matches!(
                typename,
                "MediaImage" | "Video" | "Model3d" | "ExternalVideo"
            ))
}

fn selected_reference_target_with_typename(
    mut value: Value,
    id: &str,
    selections: &[SelectedField],
) -> Value {
    let Some(typename) = shopify_gid_resource_type(id) else {
        return value;
    };
    let Some(object) = value.as_object_mut() else {
        return value;
    };
    for field in selections {
        if field.name == "__typename" {
            object
                .entry(field.response_key.clone())
                .or_insert_with(|| json!(typename));
        }
    }
    value
}

use super::*;

pub(in crate::proxy) fn event_empty_read_data(fields: &[RootFieldSelection]) -> Value {
    root_payload_json(fields, |field| match field.name.as_str() {
        "event" => Some(Value::Null),
        "events" => Some(selected_json(
            &json!({
                "nodes": [],
                "edges": [],
                "pageInfo": empty_page_info()
            }),
            &field.selection,
        )),
        "eventsCount" => Some(event_count_empty_json(&field.selection)),
        _ => Some(Value::Null),
    })
}

pub(in crate::proxy) fn event_count_empty_json(selections: &[SelectedField]) -> Value {
    let mut fields = serde_json::Map::new();
    for selection in selections {
        let value = match selection.name.as_str() {
            "count" => json!(0),
            "precision" => json!("EXACT"),
            _ => Value::Null,
        };
        fields.insert(selection.response_key.clone(), value);
    }
    Value::Object(fields)
}

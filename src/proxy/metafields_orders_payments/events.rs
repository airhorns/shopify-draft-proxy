use super::*;

pub(in crate::proxy) fn event_empty_read_data(fields: &[RootFieldSelection]) -> Value {
    root_payload_json(fields, |field| match field.name.as_str() {
        "event" => Some(Value::Null),
        "events" => Some(selected_json(
            &connection_json(Vec::new()),
            &field.selection,
        )),
        "eventsCount" => Some(selected_count_json(0, &field.selection)),
        _ => Some(Value::Null),
    })
}

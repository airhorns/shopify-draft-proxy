use super::*;

pub(in crate::proxy) fn delivery_settings_read_data(fields: &[RootFieldSelection]) -> Value {
    root_payload_json(fields, |field| match field.name.as_str() {
        "deliverySettings" => Some(selected_json(
            &json!({
                "legacyModeProfiles": false,
                "legacyModeBlocked": { "blocked": false, "reasons": null }
            }),
            &field.selection,
        )),
        "deliveryPromiseSettings" => Some(selected_json(
            &json!({ "deliveryDatesEnabled": false, "processingTime": null }),
            &field.selection,
        )),
        _ => None,
    })
}

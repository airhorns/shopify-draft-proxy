use super::*;

impl DraftProxy {
    pub(in crate::proxy) fn delivery_settings_read_response(
        &mut self,
        request: &Request,
        fields: &[RootFieldSelection],
    ) -> Response {
        if self.config.read_mode != ReadMode::Snapshot {
            return (self.upstream_transport)(request.clone());
        }
        ok_json(json!({ "data": delivery_settings_read_data(fields) }))
    }
}

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

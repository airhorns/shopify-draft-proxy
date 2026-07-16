use super::*;

impl DraftProxy {
    pub(in crate::proxy) fn delivery_settings_read_outcome(
        &mut self,
        request: &Request,
        fields: &[RootFieldSelection],
        response_key: &str,
    ) -> ResolverOutcome<Value> {
        if self.config.read_mode != ReadMode::Snapshot {
            return self.cached_or_forward_upstream_root_outcome(request, response_key);
        }
        let data = delivery_settings_read_data(fields);
        ResolverOutcome::value(data.get(response_key).cloned().unwrap_or(Value::Null))
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

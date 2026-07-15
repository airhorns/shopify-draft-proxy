use super::*;

impl DraftProxy {
    pub(in crate::proxy) fn resolve_events_graphql(
        &mut self,
        context: RootResolverContext<'_>,
    ) -> Response {
        let RootResolverContext {
            request,
            query,
            variables,
            root_name,
            mode,
            ..
        } = context;
        match mode {
            LocalResolverMode::OverlayRead => {
                if self.config.read_mode == ReadMode::LiveHybrid {
                    return (self.upstream_transport)(request.clone());
                }
                let fields = match self.root_fields_or_error(query, variables) {
                    Ok(fields) => fields,
                    Err(response) => return response,
                };
                ok_json(json!({ "data": event_empty_read_data(&fields) }))
            }
            LocalResolverMode::StageLocally => {
                Self::unimplemented_resolver_response(mode, root_name)
            }
        }
    }
}

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

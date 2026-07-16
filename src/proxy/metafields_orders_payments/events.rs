use super::*;

pub(in crate::proxy) fn event_field_resolver_registrations() -> Vec<FieldResolverRegistration> {
    [
        (
            "Event",
            &[
                "action",
                "appTitle",
                "attributeToApp",
                "attributeToUser",
                "createdAt",
                "criticalAlert",
                "id",
                "message",
            ][..],
        ),
        ("EventConnection", &["edges", "nodes", "pageInfo"]),
        ("EventEdge", &["cursor", "node"]),
        ("Count", &["count", "precision"]),
    ]
    .into_iter()
    .flat_map(|(parent_type, fields)| {
        fields.iter().map(move |field| {
            FieldResolverRegistration::property(ApiSurface::Admin, parent_type, field)
        })
    })
    .collect()
}

impl DraftProxy {
    pub(crate) fn resolve_events_graphql(
        &mut self,
        invocation: RootInvocation<'_>,
    ) -> ResolverOutcome<Value> {
        let RootInvocation {
            response_key,
            request,
            root_name,
            mode,
            ..
        } = invocation;
        match mode {
            LocalResolverMode::OverlayRead => {
                if self.config.read_mode == ReadMode::LiveHybrid {
                    return resolver_outcome_from_response(
                        (self.upstream_transport)(request.clone()),
                        response_key,
                    );
                }
                ResolverOutcome::value(match root_name {
                    "event" => Value::Null,
                    "events" => connection_json(Vec::new()),
                    "eventsCount" => count_object(0),
                    _ => Value::Null,
                })
            }
            LocalResolverMode::StageLocally => ResolverOutcome::error(format!(
                "Events resolver `{root_name}` cannot execute in {} mode",
                mode.registry_name(),
            )),
        }
    }
}

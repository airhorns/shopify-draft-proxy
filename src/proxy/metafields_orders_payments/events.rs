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

pub(in crate::proxy) fn event_field_resolver_type_policies() -> Vec<FieldResolverTypePolicy> {
    [
        "BasicEvent",
        "CommentEvent",
        "CommentEventAttachment",
        "CommentEventSubject",
        "CustomerVisit",
        "HasEvents",
    ]
    .into_iter()
    .map(|parent_type| {
        FieldResolverTypePolicy::property_backed_ordinary_fields(
            ApiSurface::Admin,
            parent_type,
            "argument-bearing event field has no explicit canonical resolver",
        )
    })
    .collect()
}

impl DraftProxy {
    pub(crate) fn event_query_root(
        &mut self,
        invocation: RootInvocation<'_>,
    ) -> ResolverOutcome<Value> {
        let RootInvocation {
            response_key,
            request,
            root_name,
            ..
        } = invocation;
        if self.config.read_mode == ReadMode::LiveHybrid {
            return self.cached_or_forward_upstream_root_outcome(request, response_key);
        }
        ResolverOutcome::value(match root_name {
            "event" => Value::Null,
            "events" => connection_json(Vec::new()),
            "eventsCount" => count_object(0),
            root => return ResolverOutcome::error(format!("Unknown event query root `{root}`")),
        })
    }
}

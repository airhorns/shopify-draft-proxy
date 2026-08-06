use super::*;
use crate::proxy::request_context::AdminOperationContext;

impl DraftProxy {
    pub(in crate::proxy) fn delivery_settings_query_is_upstream_authoritative(
        &self,
        context: &AdminOperationContext<'_>,
    ) -> bool {
        self.config.read_mode == ReadMode::LiveHybrid
            && context.operation_type == OperationType::Query
            && !context.roots.is_empty()
            && context.roots.iter().all(|root| {
                matches!(
                    root.name.as_str(),
                    "deliverySettings" | "deliveryPromiseSettings"
                )
            })
    }

    pub(crate) fn delivery_settings_query_root(
        &mut self,
        invocation: RootInvocation<'_>,
    ) -> ResolverOutcome<Value> {
        if self.config.read_mode != ReadMode::Snapshot {
            return self.cached_or_forward_upstream_root_outcome(
                invocation.request,
                invocation.response_key,
            );
        }
        ResolverOutcome::value(delivery_settings_value(invocation.root_name))
    }
}

fn delivery_settings_value(root_name: &str) -> Value {
    match root_name {
        "deliverySettings" => json!({
            "legacyModeProfiles": false,
            "legacyModeBlocked": { "blocked": false, "reasons": null }
        }),
        "deliveryPromiseSettings" => {
            json!({ "deliveryDatesEnabled": false, "processingTime": null })
        }
        _ => Value::Null,
    }
}

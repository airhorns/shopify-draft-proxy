use super::*;
use crate::proxy::request_planner::{
    RequestExecutionPlan, RequestPlanningInvocation, RootReadAuthority,
};

impl DraftProxy {
    pub(crate) fn plan_delivery_settings_query(
        &self,
        invocation: &RequestPlanningInvocation<'_>,
        plan: &mut RequestExecutionPlan,
    ) {
        if self.config.read_mode == ReadMode::LiveHybrid
            && invocation.operation_type == OperationType::Query
            && !invocation.roots.is_empty()
            && invocation.roots.iter().all(|root| {
                matches!(
                    root.name.as_str(),
                    "deliverySettings" | "deliveryPromiseSettings"
                )
            })
        {
            plan.set_named_roots_authority(
                &["deliverySettings", "deliveryPromiseSettings"],
                invocation.roots,
                RootReadAuthority::Upstream,
            );
        }
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

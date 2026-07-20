use super::*;
use crate::graphql::RawArgumentValue;
use std::collections::{BTreeMap, BTreeSet};

mod inventory_helpers;
mod marketing_helpers;
mod webhook_helpers;

pub(in crate::proxy) use self::inventory_helpers::*;

struct MarketingRootInput {
    name: String,
    response_key: String,
    arguments: BTreeMap<String, ResolvedValue>,
    raw_arguments: BTreeMap<String, RawArgumentValue>,
    selection: Vec<SelectedField>,
    variable_definitions: BTreeMap<String, crate::graphql::VariableDefinitionInfo>,
    variables: BTreeMap<String, ResolvedValue>,
    operation_has_local_boundary: bool,
}

pub(in crate::proxy) fn marketing_field_resolver_type_policies() -> Vec<FieldResolverTypePolicy> {
    [
        "MarketingActivity",
        "MarketingEngagement",
        "MarketingEvent",
        "WebhookEventBridgeEndpoint",
        "WebhookHttpEndpoint",
        "WebhookPubSubEndpoint",
        "WebhookSubscription",
    ]
    .into_iter()
    .map(|parent_type| {
        FieldResolverTypePolicy::property_backed_ordinary_fields(
            ApiSurface::Admin,
            parent_type,
            "argument-bearing marketing or webhook field has no explicit canonical resolver",
        )
    })
    .collect()
}

impl DraftProxy {
    pub(crate) fn marketing_query_root(
        &mut self,
        invocation: RootInvocation<'_>,
    ) -> ResolverOutcome<Value> {
        let operation_has_local_boundary =
            operation_has_local_connection_boundary(&invocation.operation_roots);
        let RootInvocation {
            response_key,
            arguments,
            raw_arguments,
            selection,
            variable_definitions,
            variables,
            request,
            root_name,
            ..
        } = invocation;
        let field = MarketingRootInput {
            name: root_name.to_string(),
            response_key: response_key.to_string(),
            arguments: resolved_arguments_from_json(&arguments),
            raw_arguments,
            selection,
            variable_definitions: variable_definitions.clone(),
            variables: variables.clone(),
            operation_has_local_boundary,
        };
        self.marketing_query_outcome(request, &field)
    }

    pub(crate) fn marketing_mutation_root(
        &mut self,
        invocation: RootInvocation<'_>,
    ) -> ResolverOutcome<Value> {
        let RootInvocation {
            response_key,
            arguments,
            raw_arguments,
            selection,
            variable_definitions,
            variables,
            request,
            root_name,
            ..
        } = invocation;
        let field = MarketingRootInput {
            name: root_name.to_string(),
            response_key: response_key.to_string(),
            arguments: resolved_arguments_from_json(&arguments),
            raw_arguments,
            selection,
            variable_definitions: variable_definitions.clone(),
            variables: variables.clone(),
            operation_has_local_boundary: false,
        };
        let (outcome, staged_ids) = self.marketing_mutation_outcome(&field, request);
        if staged_ids.is_empty() {
            outcome
        } else {
            outcome.with_log_draft(LogDraft::staged(root_name, "marketing", staged_ids))
        }
    }
}

fn comparison_operator_prefix<'a>(
    value: &'a str,
    operators: &[&'static str],
) -> Option<(&'static str, &'a str)> {
    operators
        .iter()
        .find_map(|&operator| value.strip_prefix(operator).map(|tail| (operator, tail)))
}

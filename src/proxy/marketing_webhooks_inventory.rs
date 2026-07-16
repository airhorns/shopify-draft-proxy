use super::*;
use crate::graphql::{parsed_document, ParsedDocument, RawArgumentValue};
use std::collections::{BTreeMap, BTreeSet};

mod inventory_helpers;
mod marketing_helpers;
mod webhook_helpers;

pub(in crate::proxy) use self::inventory_helpers::*;

impl DraftProxy {
    pub(crate) fn resolve_marketing_graphql(
        &mut self,
        invocation: RootInvocation<'_>,
    ) -> ResolverOutcome<Value> {
        let RootInvocation {
            response_key,
            arguments,
            request,
            query,
            variables,
            root_name,
            mode,
            ..
        } = invocation;
        let mut fields = match self.root_fields_or_error(query, variables) {
            Ok(fields) => fields,
            Err(_) => return resolver_http_error_outcome(400, "Could not parse GraphQL operation"),
        };
        fields.retain(|field| field.response_key == response_key);
        if let Some(field) = fields.first_mut() {
            field.arguments = arguments
                .iter()
                .map(|(name, value)| (name.clone(), resolved_value_from_json(value)))
                .collect();
        }
        match mode {
            LocalResolverMode::OverlayRead => {
                self.marketing_query_outcome(request, &fields, response_key)
            }
            LocalResolverMode::StageLocally => {
                let (outcome, staged_ids) =
                    self.marketing_mutation_outcome(&fields, request, response_key);
                if staged_ids.is_empty() {
                    outcome
                } else {
                    outcome.with_log_draft(LogDraft::staged(root_name, "marketing", staged_ids))
                }
            }
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

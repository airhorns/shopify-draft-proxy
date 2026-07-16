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
            request,
            query,
            variables,
            root_name,
            mode,
            ..
        } = invocation;
        let response = (|| -> Response {
            let fields = match self.root_fields_or_error(query, variables) {
                Ok(fields) => fields,
                Err(response) => return response,
            };
            match mode {
                LocalResolverMode::OverlayRead => self.marketing_query_response(request, &fields),
                LocalResolverMode::StageLocally => {
                    let response = self.marketing_mutation(&fields, request);
                    let staged_ids: Vec<String> = fields
                        .iter()
                        .filter_map(|field| {
                            response.body["data"][field.response_key.as_str()]["marketingActivity"]
                                ["id"]
                                .as_str()
                                .map(ToString::to_string)
                        })
                        .collect();
                    if !staged_ids.is_empty() {
                        self.record_mutation_log_entry(
                            request, query, variables, root_name, staged_ids,
                        );
                    }
                    response
                }
            }
        })();
        resolver_outcome_from_response(response, response_key)
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

use super::*;

mod addresses;
mod companies;
mod consent;
mod customers;
mod merge_erasure;

use self::addresses::{
    customer_address_contains_url, customer_address_field_path, customer_address_string,
    customer_country_from_input, customer_mailing_addresses, customer_update_mailing_address,
    selected_customer_addresses_connection,
};
pub(in crate::proxy) use self::addresses::{
    customer_address_cursor, customer_address_dedup_key, customer_address_input_node,
    customer_address_nodes, customer_rebuild_addresses,
};
pub(in crate::proxy) use self::companies::*;
pub(in crate::proxy) use self::consent::{
    b2b_tax_settings_invalid_enum_error, customer_sms_consent_invalid_enum_error,
    customer_tax_exemptions_invalid_enum_error,
};
use self::consent::{customer_update_inline_consent_errors, resolved_inline_consent_state};
use self::customers::apply_customer_marketing_consent;
pub(in crate::proxy) use self::customers::customer_field_resolver_registrations;
use self::merge_erasure::{
    connection_has_nodes, customer_merge_extract_order_records, customer_merge_job_from_request,
    nodes_connection, order_connection_cursor,
};

fn upstream_count_field(
    field: &RootFieldSelection,
    upstream_data: Option<&Value>,
) -> Option<(u64, String)> {
    let value = upstream_data?.get(field.response_key.as_str())?;
    let count_key = field
        .selection
        .iter()
        .find(|selection| selection.name == "count")
        .map(|selection| selection.response_key.as_str())
        .unwrap_or("count");
    let precision_key = field
        .selection
        .iter()
        .find(|selection| selection.name == "precision")
        .map(|selection| selection.response_key.as_str())
        .unwrap_or("precision");
    let count = value
        .get(count_key)
        .or_else(|| value.get("count"))
        .and_then(Value::as_u64)?;
    let precision = value
        .get(precision_key)
        .or_else(|| value.get("precision"))
        .and_then(Value::as_str)
        .unwrap_or("EXACT")
        .to_string();
    Some((count, precision))
}

impl DraftProxy {
    pub(crate) fn resolve_b2b_graphql(
        &mut self,
        invocation: RootInvocation<'_>,
    ) -> ResolverOutcome<Value> {
        let RootInvocation {
            response_key,
            request,
            query,
            variables,
            operation,
            root_name: root_field,
            mode,
            ..
        } = invocation;
        match mode {
            LocalResolverMode::StageLocally
                if operation.operation_type == OperationType::Mutation =>
            {
                let outcome = match root_field {
                    "companyLocationUpdate" => self.b2b_location_buyer_experience_outcome(
                        request,
                        query,
                        variables,
                        operation.operation_type,
                        response_key,
                    ),
                    "companyLocationTaxSettingsUpdate" => {
                        self.b2b_tax_settings_outcome(request, query, variables, response_key)
                    }
                    "companyAssignCustomerAsContact" => self
                        .b2b_assign_customer_as_contact_outcome(query, variables, response_key)
                        .or_else(|| {
                            self.order_customer_error_paths_outcome(
                                request,
                                query,
                                variables,
                                response_key,
                            )
                        }),
                    _ => self.b2b_company_outcome(
                        request,
                        query,
                        variables,
                        operation.operation_type,
                        response_key,
                    ),
                };
                outcome.unwrap_or_else(|| {
                    resolver_http_error_outcome(
                        501,
                        format!("No Rust b2b resolver implemented for root field: {root_field}"),
                    )
                })
            }
            LocalResolverMode::OverlayRead if operation.operation_type == OperationType::Query => {
                if self.should_route_owner_metafields_read(query, variables) {
                    return self.owner_metafields_read(request, query, variables, response_key);
                }
                self.b2b_location_buyer_experience_outcome(
                    request,
                    query,
                    variables,
                    operation.operation_type,
                    response_key,
                )
                .or_else(|| {
                    self.b2b_company_outcome(
                        request,
                        query,
                        variables,
                        operation.operation_type,
                        response_key,
                    )
                })
                .unwrap_or_else(|| {
                    if self.config.read_mode != ReadMode::Snapshot {
                        self.cached_or_forward_upstream_root_outcome(request, response_key)
                    } else {
                        resolver_http_error_outcome(
                            501,
                            format!(
                                "No Rust b2b overlay-read resolver implemented for root field: {root_field}"
                            ),
                        )
                    }
                })
            }
            LocalResolverMode::OverlayRead | LocalResolverMode::StageLocally => {
                resolver_http_error_outcome(
                    501,
                    format!(
                        "No Rust {} resolver implemented for root field: {root_field}",
                        mode.registry_name()
                    ),
                )
            }
        }
    }
}

fn customer_payment_methods_only_read(fields: &[RootFieldSelection]) -> bool {
    !fields.is_empty()
        && fields.iter().all(|field| {
            field.name == "customer"
                && field
                    .selection
                    .iter()
                    .any(|selection| selection.name == "paymentMethods")
                && field
                    .selection
                    .iter()
                    .all(|selection| matches!(selection.name.as_str(), "id" | "paymentMethods"))
        })
}

impl DraftProxy {
    pub(crate) fn resolve_customers_graphql(
        &mut self,
        invocation: RootInvocation<'_>,
    ) -> ResolverOutcome<Value> {
        if invocation.mode == LocalResolverMode::StageLocally
            && matches!(
                invocation.root_name,
                "storeCreditAccountCredit" | "storeCreditAccountDebit"
            )
        {
            return self.store_credit_account_mutation(
                invocation.root_name,
                invocation.request,
                invocation.query,
                invocation.variables,
                invocation.response_key,
            );
        }
        let RootInvocation {
            response_key,
            request,
            query,
            variables,
            operation,
            root_name: root_field,
            mode,
            ..
        } = invocation;
        match mode {
            LocalResolverMode::OverlayRead
                if operation.operation_type == OperationType::Query =>
            {
                let Some(fields) = self.execution_root_fields(query, variables) else {
                    return resolver_http_error_outcome(400, "Could not parse GraphQL operation");
                };
                let Some(field) = fields
                    .iter()
                    .find(|field| field.response_key == response_key)
                else {
                    return ResolverOutcome::value(Value::Null);
                };
                let current_fields = std::slice::from_ref(field);
                if customer_payment_methods_only_read(current_fields) {
                    if let Some(outcome) = self.customer_payment_method_local_outcome(
                        request,
                        query,
                        variables,
                        response_key,
                    )
                    {
                        return outcome;
                    }
                }
                let handle_customers = self.should_handle_customer_overlay_read(current_fields);
                if !handle_customers && self.should_route_owner_metafields_read(query, variables) {
                    return self.owner_metafields_read(request, query, variables, response_key);
                }
                let handle_store_credit = field.name == "storeCreditAccount";
                if !handle_customers && !handle_store_credit {
                    return self.cached_or_forward_upstream_root_outcome(request, response_key);
                }
                if handle_customers
                    && field.name == "customersCount"
                    && self.request_upstream_query_response.as_ref().is_none_or(|response| {
                        response.body["data"].get(response_key).is_none()
                    })
                {
                    self.hydrate_customers_count_for_overlay_read(request);
                }
                if handle_customers && self.customer_read_selects_amount_spent(current_fields) {
                    self.hydrate_shop_pricing_state_if_missing(request, true, false);
                }
                let customer_upstream_data = self
                    .request_upstream_query_response
                    .as_ref()
                    .filter(|response| (200..300).contains(&response.status))
                    .and_then(|response| response.body.get("data"))
                    .cloned()
                    .or_else(|| {
                        (handle_customers
                            && self.customer_overlay_needs_upstream_data(current_fields))
                        .then(|| self.customer_overlay_upstream_data(request))
                        .flatten()
                    });
                let value = if handle_customers {
                    self.customer_overlay_read_fields(
                        request,
                        current_fields,
                        customer_upstream_data.as_ref(),
                    )
                    .get(response_key)
                    .cloned()
                    .unwrap_or(Value::Null)
                } else {
                    self.store_credit_account_read_fields(current_fields)
                        .get(response_key)
                        .cloned()
                        .unwrap_or(Value::Null)
                };
                ResolverOutcome::value(value)
            }
            LocalResolverMode::StageLocally
                if operation.operation_type == OperationType::Mutation =>
            {
                match root_field {
                    "customerCreate" | "customerUpdate" | "customerDelete" | "customerSet" => {
                        self.customer_mutation_outcome(request, query, variables, response_key)
                    }
                    "customerGenerateAccountActivationUrl"
                    | "customerSendAccountInviteEmail" => self
                        .customer_outbound_lifecycle_outcome(
                            request,
                            query,
                            variables,
                            response_key,
                        ),
                    "customerMerge" => {
                        self.customer_merge_outcome(query, variables, request, response_key)
                    }
                    "customerRequestDataErasure" | "customerCancelDataErasure" => self
                        .customer_data_erasure_outcome(
                            query,
                            variables,
                            request,
                            root_field,
                            root_field == "customerRequestDataErasure",
                            response_key,
                        ),
                    "customerAddressCreate"
                    | "customerAddressUpdate"
                    | "customerAddressDelete"
                    | "customerUpdateDefaultAddress" => self
                        .customer_address_mutation_outcome(
                            query,
                            variables,
                            response_key,
                        ),
                    "customerAddTaxExemptions"
                    | "customerRemoveTaxExemptions"
                    | "customerReplaceTaxExemptions" => {
                        let Some(fields) = self.execution_root_fields(query, variables) else {
                            return resolver_http_error_outcome(
                                400,
                                "Could not parse GraphQL operation",
                            );
                        };
                        let current = fields
                            .iter()
                            .filter(|field| field.response_key == response_key)
                            .cloned()
                            .collect::<Vec<_>>();
                        if let Some(error) =
                            customer_tax_exemptions_invalid_enum_error(query, &current)
                        {
                            return graphql_error_outcome(vec![error], response_key);
                        }
                        self.customer_tax_exemptions_mutation_outcome(
                            &current,
                            request,
                            response_key,
                        )
                    }
                    "customerEmailMarketingConsentUpdate"
                    | "customerSmsMarketingConsentUpdate" => {
                        let Some(fields) = self.execution_root_fields(query, variables) else {
                            return resolver_http_error_outcome(
                                400,
                                "Could not parse GraphQL operation",
                            );
                        };
                        let current = fields
                            .iter()
                            .filter(|field| field.response_key == response_key)
                            .cloned()
                            .collect::<Vec<_>>();
                        if let Some(error) =
                            customer_sms_consent_invalid_enum_error(query, &current)
                        {
                            return graphql_error_outcome(vec![error], response_key);
                        }
                        self.customer_marketing_consent_update_outcome(
                            query,
                            variables,
                            request,
                            response_key,
                        )
                    }
                    _ => resolver_http_error_outcome(
                        501,
                        format!(
                            "No Rust stage-locally resolver implemented for root field: {root_field}"
                        ),
                    ),
                }
            }
            LocalResolverMode::OverlayRead | LocalResolverMode::StageLocally => {
                resolver_http_error_outcome(
                    501,
                    format!(
                        "No Rust {} resolver implemented for root field: {root_field}",
                        mode.registry_name()
                    ),
                )
            }
        }
    }
}

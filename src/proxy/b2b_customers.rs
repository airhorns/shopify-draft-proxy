use super::*;

mod addresses;
mod companies;
mod consent;
mod customers;
mod merge_erasure;

use self::addresses::{
    customer_address_contains_url, customer_address_field_path, customer_address_string,
    customer_country_from_input, customer_mailing_addresses, customer_update_mailing_address,
};
pub(in crate::proxy) use self::addresses::{
    customer_address_cursor, customer_address_dedup_key, customer_address_input_node,
    customer_address_nodes, customer_rebuild_addresses,
};
pub(in crate::proxy) use self::companies::*;
pub(in crate::proxy) use self::consent::b2b_tax_settings_invalid_enum_error;
use self::consent::{customer_update_inline_consent_errors, resolved_inline_consent_state};
use self::customers::apply_customer_marketing_consent;
pub(in crate::proxy) use self::customers::{
    customer_count_baseline_key, customer_field_resolver_registrations,
    customer_field_resolver_type_policies,
};
use self::merge_erasure::{
    connection_has_nodes, customer_merge_extract_order_records, customer_merge_job_from_request,
    nodes_connection, order_connection_cursor,
};

pub(in crate::proxy) struct B2bRootInput {
    name: String,
    response_key: String,
    location: SourceLocation,
    raw_arguments: BTreeMap<String, RawArgumentValue>,
    arguments: BTreeMap<String, ResolvedValue>,
}

fn b2b_root_input(invocation: &RootInvocation<'_>) -> B2bRootInput {
    B2bRootInput {
        name: invocation.root_name.to_string(),
        response_key: invocation.response_key.to_string(),
        location: invocation.root_location,
        raw_arguments: invocation.raw_arguments.clone(),
        arguments: resolved_arguments_from_json(&invocation.arguments),
    }
}

impl DraftProxy {
    pub(crate) fn b2b_mutation_root(
        &mut self,
        invocation: RootInvocation<'_>,
    ) -> ResolverOutcome<Value> {
        let arguments = resolved_arguments_from_json(&invocation.arguments);
        let field = b2b_root_input(&invocation);
        self.hydrate_b2b_mutation_prerequisites(invocation.request, &field);
        let outcome = match invocation.root_name {
            "companyLocationUpdate" => self.b2b_location_buyer_experience_outcome(
                invocation.request,
                &field,
                OperationType::Mutation,
            ),
            "companyLocationTaxSettingsUpdate" => self.b2b_tax_settings_outcome(
                invocation.request,
                invocation.query,
                &field,
                invocation.response_key,
            ),
            "companyAssignCustomerAsContact" => self
                .b2b_assign_customer_as_contact_outcome(&field)
                .or_else(|| {
                    self.order_customer_error_paths_outcome(
                        invocation.request,
                        invocation.root_name,
                        &arguments,
                        invocation.query,
                        invocation.variables,
                    )
                }),
            _ => self.b2b_company_outcome(
                invocation.request,
                &field,
                OperationType::Mutation,
                invocation.response_key,
            ),
        };
        outcome.unwrap_or_else(|| {
            resolver_http_error_outcome(
                501,
                format!(
                    "No Rust b2b mutation resolver implemented for root field: {}",
                    invocation.root_name
                ),
            )
        })
    }

    pub(crate) fn b2b_query_root(
        &mut self,
        invocation: RootInvocation<'_>,
    ) -> ResolverOutcome<Value> {
        let field = b2b_root_input(&invocation);
        self.b2b_location_buyer_experience_outcome(invocation.request, &field, OperationType::Query)
            .or_else(|| {
                self.b2b_company_outcome(
                    invocation.request,
                    &field,
                    OperationType::Query,
                    invocation.response_key,
                )
            })
            .unwrap_or_else(|| {
                if self.config.read_mode != ReadMode::Snapshot {
                    self.cached_or_forward_upstream_root_outcome(
                        invocation.request,
                        invocation.response_key,
                    )
                } else {
                    resolver_http_error_outcome(
                        501,
                        format!(
                            "No Rust b2b overlay-read resolver implemented for root field: {}",
                            invocation.root_name
                        ),
                    )
                }
            })
    }
}

impl DraftProxy {
    pub(crate) fn customer_query_root(
        &mut self,
        invocation: RootInvocation<'_>,
    ) -> ResolverOutcome<Value> {
        let operation_has_local_boundary =
            operation_has_local_connection_boundary(&invocation.operation_roots);
        let arguments = resolved_arguments_from_json(&invocation.arguments);
        let requests_payment_methods = invocation.requests_field_path(&["paymentMethods"]);
        if requests_payment_methods {
            self.ensure_customer_payment_method_seed_state();
        }
        let handle_customers = self.should_handle_customer_overlay_read(
            invocation.root_name,
            &arguments,
            requests_payment_methods,
        );
        let handle_store_credit = invocation.root_name == "storeCreditAccount";
        if !handle_customers && !handle_store_credit {
            return self.cached_or_forward_upstream_root_outcome(
                invocation.request,
                invocation.response_key,
            );
        }
        if handle_customers
            && invocation.root_name == "customersCount"
            && self
                .execution_session
                .upstream_query_response
                .as_ref()
                .is_none_or(|response| response.body["data"].get(invocation.response_key).is_none())
        {
            self.hydrate_customers_count_for_overlay_read(invocation.request, &arguments);
        }
        if handle_customers
            && invocation
                .requested_field_paths
                .iter()
                .any(|path| path.iter().any(|segment| segment == "amountSpent"))
        {
            self.hydrate_shop_pricing_state_if_missing(invocation.request, true, false);
        }
        let mut upstream_value =
            if invocation.root_name == "customers" && operation_has_local_boundary {
                Some(Value::Null)
            } else {
                invocation.upstream_value.clone()
            };
        if upstream_value.is_none()
            && handle_customers
            && self.customer_overlay_needs_upstream_data(invocation.root_name, &arguments)
            && !operation_has_local_boundary
        {
            let upstream = self.cached_or_forward_upstream_root_outcome(
                invocation.request,
                invocation.response_key,
            );
            // This is best-effort overlay hydration, not a passthrough root.
            // Preserve the staged customer result when the live catalog cannot
            // be fetched; the legacy overlay path likewise treated an
            // unsuccessful hydrate as an empty baseline rather than replacing
            // authoritative local state with an upstream transport error.
            if upstream.errors.is_empty() {
                upstream_value = Some(upstream.value);
            }
        }
        let value = if handle_customers {
            self.customer_overlay_read_value(
                invocation.request,
                invocation.root_name,
                &arguments,
                upstream_value.as_ref(),
                (invocation.root_name == "customers").then_some(ConnectionOverlayRequest {
                    root_name: invocation.root_name,
                    arguments: &arguments,
                    raw_arguments: &invocation.raw_arguments,
                    selection: &invocation.selection,
                    variable_definitions: invocation.variable_definitions,
                    variables: invocation.variables,
                    required_node_selection: "id firstName lastName displayName email phone locale note canDelete verifiedEmail dataSaleOptOut taxExempt taxExemptions state tags createdAt updatedAt defaultEmailAddress { emailAddress } defaultPhoneNumber { phoneNumber } defaultAddress { id firstName lastName address1 address2 city company province provinceCode country countryCodeV2 zip phone name formattedArea }",
                }),
            )
        } else {
            self.store_credit_account_read_value(&arguments)
        };
        ResolverOutcome::value(value)
    }
}

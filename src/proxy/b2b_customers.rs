use super::*;

mod addresses;
mod companies;
mod consent;
mod customers;
mod merge_erasure;

use self::addresses::{
    customer_address_contains_url, customer_address_cursor, customer_address_dedup_key,
    customer_address_field_path, customer_address_nodes, customer_address_string,
    customer_country_from_input, customer_mailing_addresses, customer_rebuild_addresses,
    customer_update_mailing_address, selected_customer_addresses_connection,
};
pub(in crate::proxy) use self::companies::*;
pub(in crate::proxy) use self::consent::{
    b2b_tax_settings_invalid_enum_response, customer_sms_consent_invalid_enum_response,
    customer_tax_exemptions_invalid_enum_response,
};
use self::consent::{customer_update_inline_consent_errors, resolved_inline_consent_state};
use self::customers::apply_customer_marketing_consent;
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
    pub(in crate::proxy) fn resolve_b2b_graphql(
        &mut self,
        context: RootResolverContext<'_>,
    ) -> Response {
        let RootResolverContext {
            request,
            query,
            variables,
            operation,
            root_name: root_field,
            mode,
        } = context;
        match mode {
            LocalResolverMode::StageLocally
                if operation.operation_type == OperationType::Mutation
                    && matches!(
                        root_field,
                        "companyLocationUpdate"
                            | "companyLocationTaxSettingsUpdate"
                            | "companyAssignCustomerAsContact"
                    ) =>
            {
                match root_field {
                    "companyLocationUpdate" => self
                        .b2b_location_buyer_experience_tail_helper_response(
                            request,
                            query,
                            variables,
                            operation.operation_type,
                            &operation.root_fields,
                        )
                        .unwrap_or_else(|| unimplemented_root_response("b2b", root_field)),
                    "companyLocationTaxSettingsUpdate" => self
                        .b2b_tax_settings_tail_helper_response(
                            request,
                            query,
                            variables,
                            operation.operation_type,
                            &operation.root_fields,
                        )
                        .unwrap_or_else(|| unimplemented_root_response("b2b", root_field)),
                    "companyAssignCustomerAsContact" => {
                        if let Some(response) =
                            self.b2b_assign_customer_as_contact_response(request, query, variables)
                        {
                            response
                        } else if let Some(data) =
                            self.order_customer_error_paths_data(request, query, variables)
                        {
                            ok_json(data)
                        } else {
                            unimplemented_root_response("b2b", root_field)
                        }
                    }
                    _ => unimplemented_root_response("b2b", root_field),
                }
            }
            LocalResolverMode::StageLocally
                if operation.operation_type == OperationType::Mutation
                    && self.config.read_mode == ReadMode::Snapshot =>
            {
                // Snapshot mode (unit tests) has no upstream to forward to, so every
                // remaining B2B mutations stage locally through the company tail
                // helper.
                self.b2b_company_tail_helper_response(
                    request,
                    query,
                    variables,
                    operation.operation_type,
                    &operation.root_fields,
                )
                .unwrap_or_else(|| unimplemented_root_response("b2b", root_field))
            }
            LocalResolverMode::StageLocally
                if operation.operation_type == OperationType::Mutation =>
            {
                // LiveHybrid still stages B2B mutations locally. Cold existing
                // resources may need fuller hydration in future work, but the
                // caller's mutation must never be forwarded as the fallback.
                self.b2b_company_tail_helper_response(
                    request,
                    query,
                    variables,
                    operation.operation_type,
                    &operation.root_fields,
                )
                .unwrap_or_else(|| unimplemented_root_response("b2b", root_field))
            }
            LocalResolverMode::OverlayRead if operation.operation_type == OperationType::Query => {
                if self.should_route_owner_metafields_read(query, variables) {
                    return self.owner_metafields_read(request, query, variables);
                }
                self.b2b_location_buyer_experience_tail_helper_response(
                    request,
                    query,
                    variables,
                    operation.operation_type,
                    &operation.root_fields,
                )
                .or_else(|| {
                    self.b2b_company_tail_helper_response(
                        request,
                        query,
                        variables,
                        operation.operation_type,
                        &operation.root_fields,
                    )
                })
                .unwrap_or_else(|| {
                    // Cold read: the query touches no locally-staged B2B graph
                    // (e.g. a pure read of a pre-existing company catalog, or a
                    // multi-root read whose roots the local serializer does not
                    // cover). Forward verbatim upstream as a read-through so the
                    // real recorded Shopify response is replayed. Staged
                    // read-after-write reads short-circuit above by returning
                    // Some, so this never masks local overlay state. Snapshot
                    // mode has no upstream, so it keeps the explicit 501.
                    if self.config.read_mode != ReadMode::Snapshot {
                        (self.upstream_transport)(request.clone())
                    } else {
                        unimplemented_root_response("b2b overlay-read", root_field)
                    }
                })
            }
            LocalResolverMode::OverlayRead | LocalResolverMode::StageLocally => {
                Self::unimplemented_resolver_response(mode, root_field)
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
    pub(in crate::proxy) fn resolve_customers_graphql(
        &mut self,
        context: RootResolverContext<'_>,
    ) -> Response {
        let RootResolverContext {
            request,
            query,
            variables,
            operation,
            root_name: root_field,
            mode,
        } = context;
        match mode {
            LocalResolverMode::OverlayRead if operation.operation_type == OperationType::Query => {
                let fields = match self.root_fields_or_error(query, variables) {
                    Ok(fields) => fields,
                    Err(response) => return response,
                };
                if customer_payment_methods_only_read(&fields) {
                    if let Some(data) =
                        self.customer_payment_method_local_data(request, query, variables)
                    {
                        return ok_json(data);
                    }
                }
                // A query may combine `customer*` reads with a standalone
                // `storeCreditAccount(id:)` read (or carry only the latter).
                // Each is served from its own staged overlay and the two field
                // maps are merged into one `data` object.
                let handle_customers = self.should_handle_customer_overlay_read(&fields);
                if !handle_customers && self.should_route_owner_metafields_read(query, variables) {
                    return self.owner_metafields_read(request, query, variables);
                }
                let handle_store_credit = fields
                    .iter()
                    .any(|field| field.name == "storeCreditAccount");
                if handle_customers || handle_store_credit {
                    // A `customersCount` read served from the staged overlay
                    // needs the live store-wide baseline; hydrate it once in
                    // LiveHybrid mode before projecting.
                    if handle_customers && fields.iter().any(|field| field.name == "customersCount")
                    {
                        self.hydrate_customers_count_for_overlay_read(request);
                    }
                    if handle_customers && self.customer_read_selects_amount_spent(&fields) {
                        self.hydrate_shop_pricing_state_if_missing(request, true, false);
                    }
                    let customer_upstream_data = (handle_customers
                        && self.customer_overlay_needs_upstream_data(&fields))
                    .then(|| self.customer_overlay_upstream_data(request))
                    .flatten();
                    let data = root_payload_json(&fields, |field| {
                        if handle_customers {
                            if let Value::Object(object) = self.customer_overlay_read_fields(
                                request,
                                std::slice::from_ref(field),
                                customer_upstream_data.as_ref(),
                            ) {
                                if let Some(value) = object.get(field.response_key.as_str()) {
                                    return Some(value.clone());
                                }
                            }
                        }
                        if handle_store_credit {
                            if let Value::Object(object) =
                                self.store_credit_account_read_fields(std::slice::from_ref(field))
                            {
                                if let Some(value) = object.get(field.response_key.as_str()) {
                                    return Some(value.clone());
                                }
                            }
                        }
                        None
                    });
                    ok_json(json!({ "data": data }))
                } else {
                    (self.upstream_transport)(request.clone())
                }
            }
            LocalResolverMode::StageLocally
                if operation.operation_type == OperationType::Mutation
                    && matches!(
                        root_field,
                        "customerCreate" | "customerUpdate" | "customerDelete" | "customerSet"
                    ) =>
            {
                self.customer_mutation_response(request, query, variables)
            }
            LocalResolverMode::StageLocally
                if operation.operation_type == OperationType::Mutation
                    && matches!(
                        root_field,
                        "customerGenerateAccountActivationUrl" | "customerSendAccountInviteEmail"
                    ) =>
            {
                self.customer_outbound_lifecycle_response(request, query, variables)
            }
            LocalResolverMode::StageLocally
                if operation.operation_type == OperationType::Mutation
                    && root_field == "customerMerge" =>
            {
                self.customer_merge(query, variables, request)
            }
            LocalResolverMode::StageLocally
                if operation.operation_type == OperationType::Mutation
                    && matches!(
                        root_field,
                        "customerRequestDataErasure" | "customerCancelDataErasure"
                    ) =>
            {
                self.customer_data_erasure(
                    query,
                    variables,
                    request,
                    root_field,
                    root_field == "customerRequestDataErasure",
                )
            }
            LocalResolverMode::StageLocally
                if operation.operation_type == OperationType::Mutation
                    && matches!(
                        root_field,
                        "customerAddressCreate"
                            | "customerAddressUpdate"
                            | "customerAddressDelete"
                            | "customerUpdateDefaultAddress"
                    ) =>
            {
                self.customer_address_mutation(request, query, variables)
            }
            LocalResolverMode::StageLocally
                if operation.operation_type == OperationType::Mutation
                    && matches!(
                        root_field,
                        "storeCreditAccountCredit" | "storeCreditAccountDebit"
                    ) =>
            {
                let outcome =
                    self.store_credit_account_mutation(root_field, request, query, variables);
                self.finalize_mutation_outcome(request, query, variables, outcome)
            }
            LocalResolverMode::StageLocally
                if operation.operation_type == OperationType::Mutation
                    && matches!(
                        root_field,
                        "customerAddTaxExemptions"
                            | "customerRemoveTaxExemptions"
                            | "customerReplaceTaxExemptions"
                    ) =>
            {
                let fields = match self.root_fields_or_error(query, variables) {
                    Ok(fields) => fields,
                    Err(response) => return response,
                };
                // Enum coercion errors (invalid `taxExemptions`) are raised before
                // any staging, matching Shopify's request-validation ordering.
                if let Some(response) =
                    customer_tax_exemptions_invalid_enum_response(query, &fields)
                {
                    return response;
                }
                self.customer_tax_exemptions_mutation_response(&fields, request, query, variables)
            }
            LocalResolverMode::StageLocally
                if operation.operation_type == OperationType::Mutation
                    && matches!(
                        root_field,
                        "customerEmailMarketingConsentUpdate" | "customerSmsMarketingConsentUpdate"
                    ) =>
            {
                let fields = match self.root_fields_or_error(query, variables) {
                    Ok(fields) => fields,
                    Err(response) => return response,
                };
                // SMS marketingState values outside `CustomerSmsMarketingState` fail
                // enum coercion before any staging, matching Shopify's ordering.
                if let Some(response) = customer_sms_consent_invalid_enum_response(query, &fields) {
                    return response;
                }
                self.customer_marketing_consent_update(query, variables, request)
            }
            LocalResolverMode::OverlayRead | LocalResolverMode::StageLocally => {
                Self::unimplemented_resolver_response(mode, root_field)
            }
        }
    }
}

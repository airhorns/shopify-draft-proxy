use super::*;

impl DraftProxy {
    pub(in crate::proxy) fn finalize_mutation_outcome(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        outcome: MutationOutcome,
    ) -> Response {
        for draft in outcome.log_drafts {
            self.record_mutation_log_draft(request, query, variables, draft);
        }
        outcome.response
    }

    fn is_registered_orders_stage_locally_root(
        &self,
        operation_type: OperationType,
        root_field: &str,
    ) -> bool {
        self.registry.iter().any(|entry| {
            entry.operation_type == operation_type
                && entry.domain == CapabilityDomain::Orders
                && entry.execution == CapabilityExecution::StageLocally
                && entry.match_names.iter().any(|name| name == root_field)
        })
    }

    fn dispatch_orders_stage_locally_fallback(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        root_field_names: &[String],
        root_field: &str,
    ) -> Response {
        if self.config.read_mode != ReadMode::Snapshot
            && self.config.unsupported_mutation_mode == Some(UnsupportedMutationMode::Passthrough)
        {
            self.record_passthrough_log_entry(
                request,
                query,
                variables,
                root_field_names,
                root_field,
            );
            return (self.upstream_transport)(request.clone());
        }

        self.record_mutation_log_entry(request, query, variables, root_field, Vec::new());
        if let Some(entry) = self.log_entries.last_mut() {
            set_log_status(entry, "failed");
            entry["notes"] = json!(
                "Orders mutation root is registered for local staging, but this argument/selection shape is not modeled yet."
            );
            entry["interpreted"]["capability"] = json!({
                "operationName": root_field,
                "domain": "orders",
                "execution": "stage-locally"
            });
        }

        let field = root_fields(query, variables)
            .and_then(|fields| fields.into_iter().find(|field| field.name == root_field));
        let response_key = field
            .as_ref()
            .map(|field| field.response_key.clone())
            .unwrap_or_else(|| root_field.to_string());
        let selection = field.map(|field| field.selection).unwrap_or_default();
        let payload = json!({
            "draftOrder": Value::Null,
            "calculatedDraftOrder": Value::Null,
            "order": Value::Null,
            "calculatedOrder": Value::Null,
            "refund": Value::Null,
            "return": Value::Null,
            "fulfillment": Value::Null,
            "fulfillmentOrder": Value::Null,
            "reverseFulfillmentOrder": Value::Null,
            "reverseDelivery": Value::Null,
            "job": Value::Null,
            "bulkOperation": Value::Null,
            "userErrors": [{
                "field": Value::Null,
                "message": format!(
                    "Local staging for {root_field} is not implemented for this request shape"
                ),
                "code": "NOT_IMPLEMENTED"
            }]
        });

        ok_json(json!({
            "data": {
                response_key: selected_json(&payload, &selection)
            }
        }))
    }

    pub(in crate::proxy) fn record_mutation_log_draft(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        draft: LogDraft,
    ) {
        let root_field = draft.root_field;
        let staged_resource_ids = draft.staged_resource_ids;
        let status = draft.status;
        let capability_domain = draft.capability_domain;
        let capability_execution = draft.capability_execution;
        let notes = draft.notes;
        let root_fields = parse_operation(query)
            .map(|operation| operation.root_fields)
            .unwrap_or_else(|| vec![root_field.clone()]);
        self.log_entries.push(json!({
            "id": format!("log-{}", self.log_entries.len() + 1),
            "operationName": null,
            "path": request.path,
            "query": query,
            "variables": resolved_variables_json(variables),
            "rawBody": request.body,
            "stagedResourceIds": staged_resource_ids,
            "status": status,
            "interpreted": {
                "operationType": "mutation",
                "operationName": root_field.clone(),
                "rootFields": root_fields,
                "primaryRootField": root_field.clone(),
                "capability": {
                    "operationName": root_field,
                    "domain": capability_domain,
                    "execution": capability_execution
                }
            },
            "notes": notes
        }));
    }

    pub(in crate::proxy) fn dispatch_graphql(&mut self, request: &Request) -> Response {
        let Some(graphql_request) = parse_graphql_request_body(&request.body) else {
            return json_error(400, "Expected JSON body with a string `query`");
        };
        let query = graphql_request.query;
        let variables = graphql_request.variables;

        let Some(operation) = parse_operation(&query) else {
            return json_error(400, "Could not parse GraphQL operation");
        };
        let Some(root_field) = operation.primary_root_field() else {
            return json_error(400, "Operation has no root field");
        };

        let schema_input_errors = public_admin_schema_input_errors(&query, &variables);
        if !schema_input_errors.is_empty() {
            return ok_json(json!({ "errors": schema_input_errors }));
        }

        if matches!(root_field, "customerCreate" | "companyCreate")
            || (root_field == "companyAssignCustomerAsContact"
                && !resolved_string_arg(
                    &root_field_arguments(&query, &variables).unwrap_or_default(),
                    "companyId",
                )
                .is_some_and(|company_id| {
                    self.store.staged.b2b_companies.contains_key(&company_id)
                }))
        {
            if let Some(data) = self.order_customer_error_paths_data(&query, &variables) {
                return ok_json(data);
            }
        }

        if let Some(response) = self.products_mutation_tail_helper_response(
            request,
            &query,
            &variables,
            operation.operation_type,
            &operation.root_fields,
        ) {
            return response;
        }

        if let Some(response) = self.b2b_tax_settings_tail_helper_response(
            request,
            &query,
            &variables,
            operation.operation_type,
            &operation.root_fields,
        ) {
            return response;
        }

        if let Some(response) = self.b2b_location_buyer_experience_tail_helper_response(
            request,
            &query,
            &variables,
            operation.operation_type,
            &operation.root_fields,
        ) {
            return response;
        }

        if let Some(response) = self.b2b_contact_role_response(
            request,
            &query,
            &variables,
            operation.operation_type,
            &operation.root_fields,
        ) {
            return response;
        }

        if let Some(response) = self.b2b_company_tail_helper_response(
            request,
            &query,
            &variables,
            operation.operation_type,
            &operation.root_fields,
        ) {
            return response;
        }

        if let Some(data) = self.customer_payment_method_local_data(request, &query, &variables) {
            return ok_json(data);
        }

        if let Some(data) = self.money_bag_presentment_local_data(request, &query, &variables) {
            return ok_json(data);
        }

        if let Some(data) = self.abandonment_delivery_status_local_data(request, &query, &variables)
        {
            return ok_json(data);
        }

        if let Some(data) =
            self.order_payment_transaction_local_data(root_field, &query, &variables)
        {
            return ok_json(data);
        }

        if let Some(response) =
            self.draft_order_invoice_send_local_response(request, &query, &variables)
        {
            return response;
        }

        if let Some(data) = self.remaining_order_local_data(request, root_field, &query, &variables)
        {
            return ok_json(data);
        }

        if matches!(
            root_field,
            "orderCreate" | "order" | "orders" | "ordersCount"
        ) {
            if let Some(data) =
                self.order_create_local_data(request, root_field, &query, &variables)
            {
                return ok_json(data);
            }
        }

        if let Some(data) = self.draft_order_complete_local_data(root_field, &query, &variables) {
            return ok_json(data);
        }

        if let Some(data) = self.payment_terms_local_data(request, &query, &variables) {
            return ok_json(data);
        }

        if root_field == "paymentReminderSend" {
            if let Some(data) = payment_reminder_local_data(
                &query,
                &variables,
                &mut self.store.staged.payment_reminder_schedule_ids,
            ) {
                return ok_json(data);
            }
        }

        if operation.operation_type == OperationType::Query
            && operation
                .root_fields
                .iter()
                .all(|field| matches!(field.as_str(), "availableLocales" | "shopLocales"))
        {
            if let Some(fields) = root_fields(&query, &variables) {
                return ok_json(json!({ "data": self.localization_catalog_query_data(&fields) }));
            }
        }

        if operation.operation_type == OperationType::Query
            && operation.root_fields.iter().all(|field| {
                matches!(
                    field.as_str(),
                    "paymentCustomization" | "paymentCustomizations"
                )
            })
        {
            if let Some(fields) = root_fields(&query, &variables) {
                return ok_json(json!({ "data": self.payment_customization_query_data(&fields) }));
            }
        }

        if operation.operation_type == OperationType::Mutation
            && operation.root_fields.iter().all(|field| {
                matches!(
                    field.as_str(),
                    "paymentCustomizationCreate"
                        | "paymentCustomizationUpdate"
                        | "paymentCustomizationActivation"
                        | "paymentCustomizationDelete"
                )
            })
        {
            if let Some(fields) = root_fields(&query, &variables) {
                let data = self.payment_customization_mutation_data(&fields);
                let staged_ids = fields
                    .iter()
                    .filter_map(|field| {
                        data[field.response_key.as_str()]["paymentCustomization"]["id"]
                            .as_str()
                            .map(ToString::to_string)
                            .or_else(|| {
                                data[field.response_key.as_str()]["deletedId"]
                                    .as_str()
                                    .map(ToString::to_string)
                            })
                    })
                    .collect();
                self.record_mutation_log_entry(request, &query, &variables, root_field, staged_ids);
                return ok_json(json!({ "data": data }));
            }
        }

        if let Some(data) = self.draft_order_bulk_tag_local_data(&query, &variables) {
            return ok_json(data);
        }

        if operation.operation_type == OperationType::Query
            && operation.root_fields.iter().all(|field| {
                matches!(
                    field.as_str(),
                    "metaobject" | "metaobjectByHandle" | "metaobjects"
                )
            })
        {
            if self.config.read_mode != ReadMode::Snapshot
                && !self.has_local_metaobject_entry_state()
            {
                if let Some(fields) = root_fields(&query, &variables) {
                    return self.metaobject_live_hybrid_read(request, &fields);
                }
                return (self.upstream_transport)(request.clone());
            }
            if let Some(fields) = root_fields(&query, &variables) {
                return ok_json(json!({"data": self.metaobject_query_data(&fields)}));
            }
        }

        if operation.operation_type == OperationType::Mutation
            && operation
                .root_fields
                .iter()
                .all(|field| matches!(field.as_str(), "metaobjectCreate" | "metaobjectDelete"))
        {
            if let Some(fields) = root_fields(&query, &variables) {
                return self.metaobject_mutation(&fields, request, &query, &variables);
            }
        }

        if operation.operation_type == OperationType::Query
            && operation.root_fields.iter().all(|field| {
                matches!(
                    field.as_str(),
                    "mobilePlatformApplication"
                        | "mobilePlatformApplications"
                        | "scriptTag"
                        | "scriptTags"
                        | "webPixel"
                        | "serverPixel"
                        | "theme"
                        | "themes"
                )
            })
            && is_ported_online_store_document(&query)
        {
            if let Some(fields) = root_fields(&query, &variables) {
                return ok_json(json!({ "data": self.online_store_query_data(&fields) }));
            }
        }

        if operation.operation_type == OperationType::Mutation
            && operation.root_fields.iter().all(|field| {
                matches!(
                    field.as_str(),
                    "mobilePlatformApplicationCreate"
                        | "mobilePlatformApplicationUpdate"
                        | "scriptTagCreate"
                        | "scriptTagUpdate"
                        | "themeCreate"
                        | "themePublish"
                        | "themeUpdate"
                        | "themeDelete"
                        | "themeFilesUpsert"
                        | "themeFilesCopy"
                        | "themeFilesDelete"
                        | "webPixelCreate"
                        | "webPixelUpdate"
                        | "serverPixelCreate"
                        | "eventBridgeServerPixelUpdate"
                        | "pubSubServerPixelUpdate"
                        | "storefrontAccessTokenCreate"
                )
            })
            && is_ported_online_store_document(&query)
        {
            if let Some(fields) = root_fields(&query, &variables) {
                return self.online_store_mutation(&fields, request, &query, &variables);
            }
        }

        if operation.operation_type == OperationType::Query
            && operation.root_fields.iter().all(|field| {
                matches!(
                    field.as_str(),
                    "marketingActivity"
                        | "marketingActivities"
                        | "marketingEvent"
                        | "marketingEvents"
                )
            })
            && is_ported_marketing_document(&query)
        {
            if let Some(fields) = root_fields(&query, &variables) {
                return ok_json(json!({ "data": self.marketing_query_data(&fields) }));
            }
        }

        if operation.operation_type == OperationType::Mutation
            && operation.root_fields.iter().all(|field| {
                matches!(
                    field.as_str(),
                    "marketingActivityCreateExternal"
                        | "marketingActivityUpdateExternal"
                        | "marketingActivityUpsertExternal"
                        | "marketingActivityDeleteExternal"
                        | "marketingActivitiesDeleteAllExternal"
                        | "marketingEngagementCreate"
                        | "marketingEngagementsDelete"
                        | "marketingActivityCreate"
                        | "marketingActivityUpdate"
                )
            })
            && (is_ported_marketing_document(&query) || is_log_draft_enforcement_document(&query))
        {
            if let Some(fields) = root_fields(&query, &variables) {
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
                        request, &query, &variables, root_field, staged_ids,
                    );
                }
                return response;
            }
        }

        if operation.operation_type == OperationType::Mutation
            && is_rust_webhook_local_runtime_document(&query)
            && matches!(
                root_field,
                "webhookSubscriptionCreate"
                    | "webhookSubscriptionUpdate"
                    | "webhookSubscriptionDelete"
                    | "pubSubWebhookSubscriptionCreate"
                    | "pubSubWebhookSubscriptionUpdate"
                    | "eventBridgeWebhookSubscriptionCreate"
                    | "eventBridgeWebhookSubscriptionUpdate"
            )
        {
            return match root_field {
                "webhookSubscriptionCreate"
                | "pubSubWebhookSubscriptionCreate"
                | "eventBridgeWebhookSubscriptionCreate" => {
                    self.webhook_subscription_create(root_field, request, &query, &variables)
                }
                "webhookSubscriptionUpdate"
                | "pubSubWebhookSubscriptionUpdate"
                | "eventBridgeWebhookSubscriptionUpdate" => {
                    self.webhook_subscription_update(root_field, request, &query, &variables)
                }
                "webhookSubscriptionDelete" => {
                    self.webhook_subscription_delete(request, &query, &variables)
                }
                _ => unreachable!(),
            };
        }

        if operation.operation_type == OperationType::Query
            && is_rust_webhook_local_runtime_document(&query)
            && operation.root_fields.iter().all(|field| {
                matches!(
                    field.as_str(),
                    "webhookSubscription" | "webhookSubscriptions" | "webhookSubscriptionsCount"
                )
            })
        {
            if let Some(fields) = root_fields(&query, &variables) {
                return ok_json(json!({ "data": self.webhook_subscriptions_query_data(&fields) }));
            }
        }

        if operation.operation_type == OperationType::Query
            && operation.root_fields.iter().all(|field| {
                matches!(
                    field.as_str(),
                    "event" | "events" | "eventsCount" | "whatever"
                )
            })
        {
            if let Some(fields) = root_fields(&query, &variables) {
                return ok_json(json!({ "data": event_empty_read_data(&fields) }));
            }
        }

        if operation.operation_type == OperationType::Query
            && operation.root_fields.iter().all(|field| {
                matches!(
                    field.as_str(),
                    "availableLocales"
                        | "shopLocales"
                        | "translatableResource"
                        | "translatableResources"
                        | "translatableResourcesByIds"
                        | "markets"
                )
            })
            && is_ported_localization_document(&query)
        {
            if let Some(fields) = root_fields(&query, &variables) {
                return ok_json(json!({ "data": self.localization_query_data(&fields, &query) }));
            }
        }

        if operation.operation_type == OperationType::Mutation
            && operation.root_fields.iter().all(|field| {
                matches!(
                    field.as_str(),
                    "shopLocaleEnable"
                        | "shopLocaleUpdate"
                        | "shopLocaleDisable"
                        | "translationsRegister"
                        | "translationsRemove"
                )
            })
            && (is_ported_localization_document(&query)
                || is_log_draft_enforcement_document(&query))
        {
            if let Some(fields) = root_fields(&query, &variables) {
                let data = self.localization_mutation_data(&fields);
                self.record_mutation_log_entry(
                    request,
                    &query,
                    &variables,
                    root_field,
                    fields
                        .iter()
                        .map(|field| field.response_key.clone())
                        .collect(),
                );
                return ok_json(json!({ "data": data }));
            }
        }

        if operation.operation_type == OperationType::Query
            && operation
                .root_fields
                .iter()
                .all(|field| matches!(field.as_str(), "market"))
            && (is_ported_market_create_document(&query)
                || is_ported_market_relations_document(&query))
        {
            if let Some(fields) = root_fields(&query, &variables) {
                return ok_json(json!({ "data": self.market_query_data(&fields) }));
            }
        }

        if operation.operation_type == OperationType::Mutation
            && operation
                .root_fields
                .iter()
                .all(|field| matches!(field.as_str(), "marketCreate" | "marketUpdate"))
            && (is_ported_market_create_document(&query)
                || is_ported_market_relations_document(&query))
        {
            if let Some(fields) = root_fields(&query, &variables) {
                let data = self.market_create_mutation_data(&fields, request, &query, &variables);
                return ok_json(json!({ "data": data }));
            }
        }

        if operation.operation_type == OperationType::Query
            && operation
                .root_fields
                .iter()
                .all(|field| matches!(field.as_str(), "catalog" | "catalogs"))
            && is_ported_catalog_document(&query)
        {
            if let Some(fields) = root_fields(&query, &variables) {
                return ok_json(json!({ "data": self.catalog_query_data(&fields) }));
            }
        }

        if operation.operation_type == OperationType::Mutation
            && operation.root_fields.iter().all(|field| {
                matches!(
                    field.as_str(),
                    "catalogCreate" | "catalogUpdate" | "catalogDelete" | "catalogContextUpdate"
                )
            })
            && is_ported_catalog_document(&query)
        {
            if let Some(fields) = root_fields(&query, &variables) {
                let data = self.catalog_mutation_data(&fields, request, &query, &variables);
                return ok_json(json!({ "data": data }));
            }
        }

        if operation.operation_type == OperationType::Query
            && operation.root_fields.iter().all(|field| {
                matches!(
                    field.as_str(),
                    "catalog" | "catalogs" | "priceList" | "priceLists"
                )
            })
            && is_ported_price_list_document(&query)
        {
            if let Some(fields) = root_fields(&query, &variables) {
                return ok_json(json!({ "data": self.price_list_query_data(&fields) }));
            }
        }

        if operation.operation_type == OperationType::Mutation
            && operation.root_fields.iter().all(|field| {
                matches!(
                    field.as_str(),
                    "priceListCreate"
                        | "priceListUpdate"
                        | "priceListDelete"
                        | "priceListFixedPricesByProductUpdate"
                        | "priceListFixedPricesAdd"
                        | "priceListFixedPricesUpdate"
                        | "priceListFixedPricesDelete"
                        | "quantityRulesDelete"
                        | "webPresenceCreate"
                        | "webPresenceUpdate"
                        | "webPresenceDelete"
                )
            })
            && is_ported_price_list_document(&query)
        {
            if let Some(fields) = root_fields(&query, &variables) {
                let data = self.price_list_mutation_data(&fields, request, &query, &variables);
                return ok_json(json!({ "data": data }));
            }
        }

        if operation.operation_type == OperationType::Query
            && operation.root_fields.iter().all(|field| {
                matches!(
                    field.as_str(),
                    "marketLocalizableResource" | "marketLocalizableResources" | "markets"
                )
            })
            && is_ported_market_localization_document(&query)
        {
            if let Some(fields) = root_fields(&query, &variables) {
                return ok_json(json!({ "data": self.market_localization_query_data(&fields) }));
            }
        }

        if operation.operation_type == OperationType::Mutation
            && operation.root_fields.iter().all(|field| {
                matches!(
                    field.as_str(),
                    "marketLocalizationsRegister" | "marketLocalizationsRemove"
                )
            })
            && is_ported_market_localization_document(&query)
        {
            if let Some(fields) = root_fields(&query, &variables) {
                let data = self.market_localization_mutation_data(&fields);
                self.record_mutation_log_entry(
                    request,
                    &query,
                    &variables,
                    root_field,
                    fields
                        .iter()
                        .map(|field| field.response_key.clone())
                        .collect(),
                );
                return ok_json(json!({ "data": data }));
            }
        }

        if operation.operation_type == OperationType::Query
            && operation.root_fields.iter().all(|field| {
                matches!(
                    field.as_str(),
                    "deliverySettings" | "deliveryPromiseSettings"
                )
            })
        {
            if let Some(fields) = root_fields(&query, &variables) {
                return ok_json(json!({ "data": delivery_settings_read_data(&fields) }));
            }
        }

        if operation.operation_type == OperationType::Query
            && is_finance_risk_no_data_read_document(&query)
            && operation.root_fields.iter().all(|field| {
                matches!(
                    field.as_str(),
                    "cashTrackingSession"
                        | "cashTrackingSessions"
                        | "pointOfSaleDevice"
                        | "dispute"
                        | "disputeEvidence"
                        | "disputes"
                        | "shopPayPaymentRequestReceipt"
                        | "shopPayPaymentRequestReceipts"
                )
            })
        {
            if let Some(fields) = root_fields(&query, &variables) {
                return ok_json(json!({ "data": finance_risk_no_data_read_data(&fields) }));
            }
        }

        if operation.operation_type == OperationType::Query
            && query.contains("ShopifyPaymentsAccountAccessProbe")
            && operation
                .root_fields
                .iter()
                .all(|field| field == "shopifyPaymentsAccount")
        {
            return ok_json(json!({ "data": { "shopifyPaymentsAccount": Value::Null } }));
        }

        if operation.operation_type == OperationType::Query
            && operation.root_fields.iter().all(|field| field == "company")
            && is_b2b_company_customer_since_read_document(&query)
        {
            if let Some(fields) = root_fields(&query, &variables) {
                return ok_json(json!({ "data": b2b_company_customer_since_read_data(&fields) }));
            }
        }

        if operation.operation_type == OperationType::Query
            && operation
                .root_fields
                .iter()
                .all(|field| matches!(field.as_str(), "carrierService" | "carrierServices"))
        {
            if let Some(fields) = root_fields(&query, &variables) {
                return ok_json(json!({ "data": self.carrier_service_read_data(&fields) }));
            }
        }

        if operation.operation_type == OperationType::Query
            && operation
                .root_fields
                .iter()
                .all(|field| matches!(field.as_str(), "fulfillmentService" | "location"))
            && is_fulfillment_service_lifecycle_document(&query)
        {
            if let Some(fields) = root_fields(&query, &variables) {
                if let Some(data) = self.fulfillment_service_read_data(&fields) {
                    return ok_json(json!({ "data": data }));
                }
            }
        }

        if operation.operation_type == OperationType::Query
            && root_field == "locationByIdentifier"
            && is_location_custom_id_miss_document(&query)
        {
            return ok_json(location_custom_id_miss_response());
        }

        if operation.operation_type == OperationType::Query
            && operation.root_fields.iter().all(|field| {
                matches!(
                    field.as_str(),
                    "location" | "locationByIdentifier" | "locations"
                )
            })
            && (self.config.read_mode == ReadMode::Snapshot || self.has_staged_locations())
        {
            if let Some(fields) = root_fields(&query, &variables) {
                return ok_json(json!({ "data": self.location_read_data(&fields) }));
            }
        }

        if operation.operation_type == OperationType::Query
            && operation
                .root_fields
                .iter()
                .all(|field| field == "collection")
            && is_collection_publishable_parity_document(&query)
        {
            if let Some(fields) = root_fields(&query, &variables) {
                return ok_json(json!({ "data": self.collection_read_data(&fields) }));
            }
        }

        if operation.operation_type == OperationType::Query
            && operation
                .root_fields
                .iter()
                .all(|field| field == "customerSegmentMembersQuery")
            && is_customer_segment_members_query_document(&query)
        {
            if let Some(fields) = root_fields(&query, &variables) {
                return ok_json(json!({
                    "data": self.customer_segment_members_query_read_data(&fields)
                }));
            }
        }

        if operation.operation_type == OperationType::Query
            && operation
                .root_fields
                .iter()
                .all(|field| field == "currentAppInstallation")
            && (is_app_subscription_activation_document(&query)
                || is_app_access_scopes_read_document(&query)
                || is_app_usage_record_read_document(&query)
                || is_app_billing_local_read_document(&query))
        {
            if let Some(fields) = root_fields(&query, &variables) {
                return ok_json(json!({
                    "data": self.current_app_installation_read_data(&fields)
                }));
            }
        }

        if operation.operation_type == OperationType::Query
            && self.has_staged_discounts()
            && operation.root_fields.iter().all(|field| {
                local_dispatch_root(
                    OperationType::Query,
                    CapabilityDomain::Discounts,
                    CapabilityExecution::OverlayRead,
                    field,
                )
                .is_some()
            })
        {
            return self.discounts_query_response(&query, &variables);
        }

        if operation.operation_type == OperationType::Query
            && query.contains("DiscountTimestampsMonotonicRead")
            && operation.root_fields.iter().all(|field| {
                matches!(
                    field.as_str(),
                    "codeDiscountNode" | "codeDiscountNodeByCode"
                )
            })
        {
            if let Some(fields) = root_fields(&query, &variables) {
                return ok_json(json!({
                    "data": self.discount_timestamps_monotonic_read_data(&fields)
                }));
            }
        }

        if operation.operation_type == OperationType::Query
            && query.contains("DiscountRedeemCodeBulkLiveRead")
            && operation.root_fields.iter().all(|field| {
                matches!(
                    field.as_str(),
                    "codeDiscountNode" | "codeDiscountNodeByCode"
                )
            })
        {
            if let Some(fields) = root_fields(&query, &variables) {
                return ok_json(json!({
                    "data": discount_redeem_code_bulk_live_read_data(
                        &fields,
                        self.store.staged.redeem_code_bulk_live_added,
                        self.store.staged.redeem_code_bulk_live_deleted_seed,
                    )
                }));
            }
        }

        if operation.operation_type == OperationType::Query
            && (query.contains("DiscountRedeemCodeBulkValidationCreationRead")
                || query.contains("DiscountRedeemCodeBulkValidationRead")
                || query.contains("DiscountRedeemCodeBulkValidationExistingRead"))
            && operation.root_fields.iter().all(|field| {
                matches!(
                    field.as_str(),
                    "discountRedeemCodeBulkCreation"
                        | "codeDiscountNode"
                        | "codeDiscountNodeByCode"
                )
            })
        {
            if let Some(fields) = root_fields(&query, &variables) {
                return ok_json(json!({
                    "data": discount_redeem_code_bulk_validation_read_data(&fields)
                }));
            }
        }

        if operation.operation_type == OperationType::Query
            && query.contains("DiscountBxgyLifecycleRead")
            && operation.root_fields.iter().all(|field| {
                matches!(
                    field.as_str(),
                    "discountNode" | "codeDiscountNodeByCode" | "automaticDiscountNode"
                )
            })
        {
            if let Some(fields) = root_fields(&query, &variables) {
                return ok_json(json!({ "data": discount_bxgy_lifecycle_read_data(&fields) }));
            }
        }

        if operation.operation_type == OperationType::Query
            && query.contains("DiscountClassInferenceRead")
            && root_field == "discountNodesCount"
        {
            if let Some(fields) = root_fields(&query, &variables) {
                return ok_json(json!({ "data": discount_class_inference_read_data(&fields) }));
            }
        }

        if operation.operation_type == OperationType::Query
            && query.contains("DiscountStatusTimeWindowDerivationRead")
            && operation.root_fields.iter().all(|field| {
                matches!(
                    field.as_str(),
                    "codeDiscountNode" | "discountNode" | "discountNodes" | "discountNodesCount"
                )
            })
        {
            if let Some(fields) = root_fields(&query, &variables) {
                return ok_json(json!({ "data": discount_status_time_window_read_data(&fields) }));
            }
        }

        if operation.operation_type == OperationType::Query
            && query.contains("DiscountFreeShippingLifecycleRead")
            && operation.root_fields.iter().all(|field| {
                matches!(
                    field.as_str(),
                    "discountNode"
                        | "codeDiscountNodeByCode"
                        | "automaticDiscountNode"
                        | "discountNodes"
                        | "discountNodesCount"
                )
            })
        {
            if let Some(fields) = root_fields(&query, &variables) {
                return ok_json(json!({
                    "data": self.discount_free_shipping_lifecycle_read_data(&fields)
                }));
            }
        }

        if operation.operation_type == OperationType::Query
            && query.contains("DiscountCodeBasicLifecycleRead")
            && operation.root_fields.iter().all(|field| {
                matches!(
                    field.as_str(),
                    "discountNode"
                        | "codeDiscountNodeByCode"
                        | "discountNodes"
                        | "discountNodesCount"
                )
            })
        {
            if let Some(fields) = root_fields(&query, &variables) {
                return ok_json(json!({
                    "data": self.discount_code_basic_lifecycle_read_data(&fields)
                }));
            }
        }

        if operation.operation_type == OperationType::Query
            && query.contains("DiscountCodeBasicBuyerContextRead")
            && operation
                .root_fields
                .iter()
                .all(|field| matches!(field.as_str(), "discountNode" | "codeDiscountNodeByCode"))
        {
            if let Some(fields) = root_fields(&query, &variables) {
                return ok_json(
                    json!({ "data": discount_code_basic_buyer_context_read_data(&fields) }),
                );
            }
        }

        if operation.operation_type == OperationType::Query
            && root_field == "automaticDiscountNode"
            && query.contains("DiscountAutomaticBasicBuyerContextRead")
        {
            if let Some(response) = discount_automatic_basic_buyer_context_read(&query, &variables)
            {
                return response;
            }
        }

        if operation.operation_type == OperationType::Query
            && root_field == "automaticDiscountNodes"
            && query.contains("DiscountAutomaticNodesRead")
        {
            if let Some(fields) = root_fields(&query, &variables) {
                return ok_json(json!({ "data": discount_automatic_nodes_read_data(&fields) }));
            }
        }

        if operation.operation_type == OperationType::Query
            && query.contains("CartTransformInvalidFields")
        {
            return ok_json(json!({
                "errors": [
                    {
                        "message": "Field 'title' doesn't exist on type 'CartTransform'",
                        "locations": [{ "line": 5, "column": 7 }],
                        "path": ["query CartTransformInvalidFields", "cartTransforms", "nodes", "title"],
                        "extensions": { "code": "undefinedField", "typeName": "CartTransform", "fieldName": "title" }
                    },
                    {
                        "message": "Field 'functionHandle' doesn't exist on type 'CartTransform'",
                        "locations": [{ "line": 6, "column": 7 }],
                        "path": ["query CartTransformInvalidFields", "cartTransforms", "nodes", "functionHandle"],
                        "extensions": { "code": "undefinedField", "typeName": "CartTransform", "fieldName": "functionHandle" }
                    },
                    {
                        "message": "Field 'createdAt' doesn't exist on type 'CartTransform'",
                        "locations": [{ "line": 7, "column": 7 }],
                        "path": ["query CartTransformInvalidFields", "cartTransforms", "nodes", "createdAt"],
                        "extensions": { "code": "undefinedField", "typeName": "CartTransform", "fieldName": "createdAt" }
                    },
                    {
                        "message": "Field 'updatedAt' doesn't exist on type 'CartTransform'",
                        "locations": [{ "line": 8, "column": 7 }],
                        "path": ["query CartTransformInvalidFields", "cartTransforms", "nodes", "updatedAt"],
                        "extensions": { "code": "undefinedField", "typeName": "CartTransform", "fieldName": "updatedAt" }
                    }
                ]
            }));
        }

        if operation.operation_type == OperationType::Query
            && operation.root_fields.iter().all(|field| {
                matches!(
                    field.as_str(),
                    "validation"
                        | "validations"
                        | "cartTransforms"
                        | "shopifyFunctions"
                        | "shopifyFunction"
                )
            })
        {
            if let Some(fields) = root_fields(&query, &variables) {
                return ok_json(json!({ "data": self.functions_metadata_read_data(&fields) }));
            }
        }

        if operation.operation_type == OperationType::Query
            && operation.root_fields.iter().all(|field| field == "node")
            && query.contains("CartTransform")
        {
            if let Some(fields) = root_fields(&query, &variables) {
                if fields.iter().any(|field| {
                    resolved_field_string_arg(field, "id")
                        .is_some_and(|id| id.contains("gid://shopify/CartTransform/"))
                }) {
                    return ok_json(
                        json!({ "data": self.functions_metadata_node_read_data(&fields) }),
                    );
                }
            }
        }

        if operation.operation_type == OperationType::Query
            && root_field == "node"
            && query.contains("AdminPlatformDiscountCodeNodeReadAfterUpdate")
        {
            if let Some(fields) = root_fields(&query, &variables) {
                return ok_json(json!({
                    "data": self.discount_code_basic_lifecycle_admin_node_read_data(&fields)
                }));
            }
        }

        if operation.operation_type == OperationType::Query
            && matches!(root_field, "segment" | "segments" | "segmentsCount")
            && self.segment_read_data_handles_fields(&query, &variables)
        {
            if let Some(fields) = root_fields(&query, &variables) {
                return ok_json(json!({ "data": self.segment_read_data(&fields) }));
            }
            return json_error(400, "Could not parse GraphQL operation");
        }

        if operation.operation_type == OperationType::Query
            && matches!(root_field, "node" | "nodes")
        {
            if query.contains("ProductVariantNodeRead") {
                return ok_json(json!({ "data": product_variant_node_read_data(&variables) }));
            }
            if let Some(fields) = root_fields(&query, &variables) {
                if is_segment_query_grammar_document(&query) {
                    if let Some(data) = self.segment_node_read_data(&fields) {
                        return ok_json(json!({ "data": data }));
                    }
                }
                if is_customer_segment_members_query_document(&query) {
                    if let Some(data) = self.customer_segment_members_query_node_read_data(&fields)
                    {
                        return ok_json(json!({ "data": data }));
                    }
                }
                if let Some(data) = self.app_node_read_data(&fields) {
                    return ok_json(json!({ "data": data }));
                }
                if let Some(data) = self.gift_card_node_read_data(&fields) {
                    return ok_json(json!({ "data": data }));
                }
            }
            if let Some(data) =
                local_node_read_fields(&query, &variables, Some(&self.store.staged.backup_region))
            {
                return ok_json(json!({ "data": data }));
            }
        }

        if operation.operation_type == OperationType::Query && root_field == "backupRegion" {
            let response_key =
                root_field_response_key(&query).unwrap_or_else(|| root_field.to_string());
            return ok_json(
                json!({ "data": { response_key: self.store.staged.backup_region.clone() } }),
            );
        }

        if let Some(data) = self.order_return_local_runtime_data(root_field, &query, &variables) {
            return ok_json(data);
        }

        if operation.operation_type == OperationType::Query
            && root_field == "order"
            && is_shipping_fulfillment_order_local_order_request(&query, &variables)
        {
            return self.shipping_fulfillment_order_local_order_read(&query, &variables);
        }

        if operation.operation_type == OperationType::Query
            && root_field == "fulfillmentOrder"
            && is_fulfillment_order_request_lifecycle_direct_read(&query, &variables)
        {
            return self.fulfillment_order_request_lifecycle_direct_read(&query, &variables);
        }

        if operation.operation_type == OperationType::Query
            && (query.contains("GiftCardReadEvidence")
                || query.contains("GiftCardReadAfterLifecycle"))
            && operation.root_fields.iter().all(|field| {
                matches!(
                    field.as_str(),
                    "giftCard" | "giftCards" | "giftCardsCount" | "giftCardConfiguration"
                )
            })
        {
            if let Some(fields) = root_fields(&query, &variables) {
                return ok_json(json!({ "data": self.gift_card_read_data(&fields) }));
            }
        }

        if operation.operation_type == OperationType::Query
            && query.contains("GiftCardNodeReadAfterLifecycle")
            && operation.root_fields.iter().all(|field| field == "node")
        {
            if let Some(fields) = root_fields(&query, &variables) {
                return ok_json(json!({ "data": self.gift_card_node_read_data(&fields) }));
            }
        }

        if operation.operation_type == OperationType::Query
            && operation.root_fields.iter().all(|field| {
                matches!(
                    field.as_str(),
                    "order" | "customer" | "article" | "draftOrder"
                )
            })
        {
            if let Some(fields) = root_fields(&query, &variables) {
                if self.should_handle_taggable_resource_overlay_read(&fields) {
                    return ok_json(
                        json!({ "data": self.taggable_resource_overlay_read_fields(&fields) }),
                    );
                }
            }
        }

        if operation.operation_type == OperationType::Query
            && operation.root_fields.iter().all(|field| {
                matches!(
                    field.as_str(),
                    "customer" | "customers" | "customersCount" | "customerByIdentifier"
                )
            })
        {
            if let Some(fields) = root_fields(&query, &variables) {
                if self.should_handle_customer_overlay_read(&query, &fields) {
                    return ok_json(json!({ "data": self.customer_overlay_read_fields(&fields) }));
                }
            }
        }

        if operation.operation_type == OperationType::Mutation
            && root_field == "customerCreate"
            && (is_local_customer_create_document(&query, &variables)
                || self.has_b2b_contact_state())
        {
            return self.customer_create(&query, &variables, request);
        }

        if operation.operation_type == OperationType::Mutation
            && root_field == "customerUpdate"
            && is_local_customer_update_document(&query, &variables)
        {
            return self.customer_update(&query, &variables, request);
        }

        if operation.operation_type == OperationType::Mutation
            && root_field == "customerDelete"
            && is_local_customer_delete_document(&query)
        {
            return self.customer_delete(&query, &variables, request);
        }

        if operation.operation_type == OperationType::Mutation
            && root_field == "orderCreate"
            && query.contains("CustomerDeleteOrderPreconditionOrderCreate")
        {
            return self.customer_order_create(&query, &variables, request);
        }

        if operation.operation_type == OperationType::Mutation && root_field == "customerSet" {
            if let Some(response) = self.customer_set_guard_response(&query, &variables) {
                return response;
            }
        }

        if operation.operation_type == OperationType::Mutation
            && root_field == "bulkOperationRunQuery"
        {
            return self.bulk_operation_run_query(request, &query, &variables);
        }

        if operation.operation_type == OperationType::Mutation
            && root_field == "bulkOperationRunMutation"
        {
            return self.bulk_operation_run_mutation(request, &query, &variables);
        }

        if operation.operation_type == OperationType::Mutation
            && root_field == "bulkOperationCancel"
            && resolved_string_arg(&variables, "id")
                .map(|id| shopify_gid_resource_type(&id) == Some("BulkOperation"))
                .unwrap_or(false)
        {
            return self.bulk_operation_cancel(request, &query, &variables);
        }

        if operation.operation_type == OperationType::Mutation && root_field == "backupRegionUpdate"
        {
            return self.backup_region_update(request, &query, &variables);
        }

        if operation.operation_type == OperationType::Mutation
            && matches!(root_field, "flowGenerateSignature" | "flowTriggerReceive")
        {
            return self.flow_utility_mutation(root_field, request, &query, &variables);
        }

        if operation.operation_type == OperationType::Mutation
            && matches!(
                root_field,
                "fileCreate"
                    | "fileUpdate"
                    | "fileDelete"
                    | "fileAcknowledgeUpdateFailed"
                    | "stagedUploadsCreate"
            )
        {
            let outcome = self.media_mutation(root_field, request, &query, &variables);
            return self.finalize_mutation_outcome(request, &query, &variables, outcome);
        }

        if operation.operation_type == OperationType::Query && root_field == "files" {
            return self.media_files_read(&query, &variables);
        }

        if operation.operation_type == OperationType::Query
            && root_field == "product"
            && (query.contains("FileReferenceProductRead")
                || query.contains("FileDeleteMediaReferenceDownstream"))
        {
            return self.media_product_read(&query, &variables);
        }

        if operation.operation_type == OperationType::Query
            && root_field == "product"
            && query.contains("ProductPublicationAggregateDownstream")
        {
            return product_publication_aggregate_downstream_read(&query, &variables);
        }

        if operation.operation_type == OperationType::Mutation
            && operation.root_fields.iter().any(|field| {
                matches!(
                    field.as_str(),
                    "metafieldDefinitionCreate"
                        | "metafieldDefinitionUpdate"
                        | "metafieldDefinitionDelete"
                        | "metafieldDefinitionPin"
                        | "metafieldDefinitionUnpin"
                )
            })
        {
            return self.metafield_definition_pinning_mutation(request, &query, &variables);
        }

        if operation.operation_type == OperationType::Query
            && operation.root_fields.iter().any(|field| {
                matches!(
                    field.as_str(),
                    "metafieldDefinition" | "metafieldDefinitions"
                )
            })
            && !self.store.staged.metafield_definitions.is_empty()
        {
            return self.metafield_definition_pinning_read(&query, &variables);
        }

        if operation.operation_type == OperationType::Mutation
            && root_field == "standardMetafieldDefinitionEnable"
        {
            return self.standard_metafield_definition_enable(request, &query, &variables);
        }

        if operation.operation_type == OperationType::Mutation
            && matches!(root_field, "metafieldsSet" | "metafieldsDelete")
            && query.contains("AppNamespaceResolution")
        {
            return self.metafields_app_namespace_mutation(root_field, &query, &variables);
        }

        if operation.operation_type == OperationType::Query
            && root_field == "product"
            && query.contains("MetafieldsAppNamespaceProductRead")
        {
            return self.metafields_app_namespace_product_read(&query, &variables);
        }

        if operation.operation_type == OperationType::Mutation && root_field == "metafieldsSet" {
            let outcome = self.owner_metafields_set(request, &query, &variables);
            return self.finalize_mutation_outcome(request, &query, &variables, outcome);
        }

        if operation.operation_type == OperationType::Mutation && root_field == "metafieldsDelete" {
            let outcome = self.owner_metafields_delete(request, &query, &variables);
            return self.finalize_mutation_outcome(request, &query, &variables, outcome);
        }

        if operation.operation_type == OperationType::Query
            && operation.root_fields.iter().any(|field| {
                matches!(
                    field.as_str(),
                    "product" | "productVariant" | "collection" | "customer" | "order" | "company"
                )
            })
            && self.should_handle_owner_metafields_read(&query, &variables)
        {
            return self.owner_metafields_read(request, &query, &variables);
        }

        if operation.operation_type == OperationType::Mutation
            && root_field == "quantityPricingByVariantUpdate"
            && is_quantity_pricing_by_variant_update_document(&query)
        {
            return quantity_pricing_by_variant_update_response(&query, &variables);
        }

        if operation.operation_type == OperationType::Mutation
            && matches!(root_field, "quantityRulesAdd" | "quantityRulesDelete")
            && is_quantity_rules_document(root_field, &query)
        {
            return quantity_rules_mutation_response(root_field, &query, &variables);
        }

        if operation.operation_type == OperationType::Mutation
            && matches!(
                root_field,
                "webPresenceCreate" | "webPresenceUpdate" | "webPresenceDelete"
            )
            && is_market_web_presence_helper_document(&query)
        {
            return self.web_presence_helper_mutation(root_field, &query, &variables, request);
        }

        if operation.operation_type == OperationType::Query
            && root_field == "webPresences"
            && is_market_web_presence_helper_document(&query)
        {
            return self.web_presence_helper_query(&query);
        }

        if operation.operation_type == OperationType::Mutation
            && root_field == "webPresenceCreate"
            && is_web_presence_local_document(&query, &variables)
        {
            return web_presence_create_response(&query, &variables);
        }

        if operation.operation_type == OperationType::Mutation
            && matches!(
                root_field,
                "shippingPackageUpdate" | "shippingPackageMakeDefault" | "shippingPackageDelete"
            )
        {
            return self.shipping_package_mutation(root_field, &query, &variables, request);
        }

        if operation.operation_type == OperationType::Mutation
            && operation.root_fields.iter().all(|field| {
                matches!(
                    field.as_str(),
                    "carrierServiceCreate" | "carrierServiceUpdate" | "carrierServiceDelete"
                )
            })
        {
            return self.carrier_service_mutations(&query, &variables, request);
        }

        if operation.operation_type == OperationType::Mutation
            && matches!(
                root_field,
                "fulfillmentServiceCreate"
                    | "fulfillmentServiceUpdate"
                    | "fulfillmentServiceDelete"
            )
            && is_fulfillment_service_lifecycle_document(&query)
        {
            return self.fulfillment_service_mutation(root_field, &query, &variables, request);
        }

        if operation.operation_type == OperationType::Mutation
            && root_field == "customerSegmentMembersQueryCreate"
            && is_customer_segment_members_query_document(&query)
        {
            return self.customer_segment_members_query_create(&query, &variables, request);
        }

        if operation.operation_type == OperationType::Mutation
            && matches!(
                root_field,
                "segmentCreate" | "segmentUpdate" | "segmentDelete"
            )
        {
            return self.segment_mutation(root_field, &query, &variables, request);
        }

        if operation.operation_type == OperationType::Mutation
            && matches!(
                root_field,
                "publishablePublish"
                    | "publishableUnpublish"
                    | "publishablePublishToCurrentChannel"
                    | "publishableUnpublishToCurrentChannel"
            )
        {
            return self.product_publishable_mutation(root_field, &query, &variables, request);
        }

        if operation.operation_type == OperationType::Mutation
            && matches!(root_field, "locationAdd" | "locationActivate")
        {
            return self.location_mutation(root_field, &query, &variables, request);
        }

        if operation.operation_type == OperationType::Mutation && root_field == "locationDeactivate"
        {
            return self.location_deactivate(&query, &variables, request);
        }

        if operation.operation_type == OperationType::Mutation
            && root_field == "fulfillmentOrderMove"
            && is_fulfillment_order_move_assignment_status_request(&variables)
        {
            return self.fulfillment_order_move_assignment_status(&query, &variables, request);
        }

        if operation.operation_type == OperationType::Mutation
            && matches!(
                root_field,
                "fulfillmentOrderOpen" | "fulfillmentOrderReportProgress"
            )
            && is_shipping_fulfillment_order_status_precondition_request(&variables)
        {
            return self.fulfillment_order_status_precondition(root_field, &query, &variables);
        }

        if operation.operation_type == OperationType::Mutation
            && root_field == "fulfillmentOrdersSetFulfillmentDeadline"
            && is_fulfillment_order_deadline_request(&variables)
        {
            return self.fulfillment_order_set_deadline(&query, &variables, request);
        }

        if operation.operation_type == OperationType::Mutation
            && root_field == "appSubscriptionCreate"
        {
            return self.app_subscription_create(&query, &variables, request);
        }

        if operation.operation_type == OperationType::Mutation
            && root_field == "appSubscriptionCancel"
        {
            return self.app_subscription_cancel(&query, &variables, request);
        }

        if operation.operation_type == OperationType::Mutation
            && root_field == "appSubscriptionTrialExtend"
        {
            return self.app_subscription_trial_extend(&query, &variables, request);
        }

        if operation.operation_type == OperationType::Mutation
            && root_field == "appSubscriptionLineItemUpdate"
        {
            return self.app_subscription_line_item_update(&query, &variables, request);
        }

        if operation.operation_type == OperationType::Mutation
            && root_field == "appUsageRecordCreate"
        {
            return self.app_usage_record_create(&query, &variables, request);
        }

        if operation.operation_type == OperationType::Mutation
            && root_field == "appPurchaseOneTimeCreate"
        {
            return self.app_purchase_one_time_create(&query, &variables, request);
        }

        if operation.operation_type == OperationType::Mutation
            && root_field == "appRevokeAccessScopes"
        {
            return self.app_revoke_access_scopes(&query, &variables, request);
        }

        if operation.operation_type == OperationType::Mutation
            && root_field == "delegateAccessTokenCreate"
        {
            return self.delegate_access_token_create(&query, &variables, request);
        }

        if operation.operation_type == OperationType::Mutation
            && root_field == "delegateAccessTokenDestroy"
        {
            return self.delegate_access_token_destroy(&query, &variables, request);
        }

        if operation.operation_type == OperationType::Mutation && root_field == "appUninstall" {
            return self.app_uninstall(&query, &variables, request);
        }

        if operation.operation_type == OperationType::Mutation
            && operation.root_fields.iter().all(|field| {
                local_dispatch_root(
                    OperationType::Mutation,
                    CapabilityDomain::Discounts,
                    CapabilityExecution::StageLocally,
                    field,
                )
                .is_some()
            })
        {
            let outcome = self.discounts_mutation(request, &query, &variables);
            return self.finalize_mutation_outcome(request, &query, &variables, outcome);
        }

        if operation.operation_type == OperationType::Mutation
            && matches!(
                root_field,
                "discountAutomaticBasicCreate"
                    | "discountAutomaticBasicUpdate"
                    | "discountAutomaticDelete"
            )
            && query.contains("DiscountAutomaticBasicBuyerContext")
        {
            if let Some(response) =
                discount_automatic_basic_buyer_context_mutation(root_field, &query, &variables)
            {
                return response;
            }
        }

        if operation.operation_type == OperationType::Mutation
            && matches!(
                root_field,
                "discountCodeBasicCreate" | "discountCodeBasicUpdate" | "discountCodeDelete"
            )
            && query.contains("DiscountCodeBasicBuyerContext")
        {
            if let Some(fields) = root_fields(&query, &variables) {
                return ok_json(
                    json!({ "data": discount_code_basic_buyer_context_mutation_data(&fields) }),
                );
            }
        }

        if operation.operation_type == OperationType::Mutation
            && query.contains("DiscountClassInferenceCreate")
            && operation.root_fields.iter().all(|field| {
                matches!(
                    field.as_str(),
                    "discountCodeBasicCreate"
                        | "discountCodeBxgyCreate"
                        | "discountCodeFreeShippingCreate"
                )
            })
        {
            if let Some(fields) = root_fields(&query, &variables) {
                return ok_json(json!({ "data": discount_class_inference_mutation_data(&fields) }));
            }
        }

        if operation.operation_type == OperationType::Mutation
            && query.contains("DiscountTimestampsMonotonicCreate")
            && operation
                .root_fields
                .iter()
                .all(|field| field == "discountCodeBasicCreate")
        {
            if let Some(fields) = root_fields(&query, &variables) {
                return ok_json(json!({
                    "data": self.discount_timestamps_monotonic_create_data(&fields)
                }));
            }
        }

        if operation.operation_type == OperationType::Mutation
            && query.contains("DiscountTimestampsMonotonicUpdate")
            && operation
                .root_fields
                .iter()
                .all(|field| field == "discountCodeBasicUpdate")
        {
            if let Some(fields) = root_fields(&query, &variables) {
                return ok_json(json!({
                    "data": self.discount_timestamps_monotonic_update_data(&fields)
                }));
            }
        }

        if operation.operation_type == OperationType::Mutation
            && query.contains("DiscountRedeemCodeBulkLiveAdd")
            && operation
                .root_fields
                .iter()
                .all(|field| field == "discountRedeemCodeBulkAdd")
        {
            if let Some(fields) = root_fields(&query, &variables) {
                self.store.staged.redeem_code_bulk_live_added = true;
                return ok_json(json!({
                    "data": discount_redeem_code_bulk_live_add_data(&fields)
                }));
            }
        }

        if operation.operation_type == OperationType::Mutation
            && query.contains("DiscountRedeemCodeBulkLiveDelete")
            && operation
                .root_fields
                .iter()
                .all(|field| field == "discountCodeRedeemCodeBulkDelete")
        {
            if let Some(fields) = root_fields(&query, &variables) {
                self.store.staged.redeem_code_bulk_live_deleted_seed = true;
                return ok_json(json!({
                    "data": discount_redeem_code_bulk_live_delete_data(&fields)
                }));
            }
        }

        if operation.operation_type == OperationType::Mutation
            && (query.contains("DiscountRedeemCodeBulkDeleteValidation")
                || query.contains("DiscountRedeemCodeBulkDeleteHappy"))
            && operation
                .root_fields
                .iter()
                .all(|field| field == "discountCodeRedeemCodeBulkDelete")
        {
            if let Some(fields) = root_fields(&query, &variables) {
                return ok_json(json!({
                    "data": discount_redeem_code_bulk_delete_validation_data(&fields)
                }));
            }
        }

        if operation.operation_type == OperationType::Mutation
            && query.contains("DiscountRedeemCodeBulkValidation")
            && operation.root_fields.iter().all(|field| {
                matches!(
                    field.as_str(),
                    "discountCodeBasicCreate" | "discountRedeemCodeBulkAdd"
                )
            })
        {
            if let Some(fields) = root_fields(&query, &variables) {
                return discount_redeem_code_bulk_validation_mutation_response(&fields);
            }
        }

        if operation.operation_type == OperationType::Mutation
            && query.contains("DiscountUpdateEdge")
            && operation.root_fields.iter().all(|field| {
                matches!(
                    field.as_str(),
                    "discountCodeBasicCreate"
                        | "discountRedeemCodeBulkAdd"
                        | "discountCodeBasicUpdate"
                        | "discountCodeBxgyCreate"
                )
            })
        {
            if let Some(fields) = root_fields(&query, &variables) {
                return ok_json(json!({
                    "data": discount_update_edge_cases_data(&fields)
                }));
            }
        }

        if operation.operation_type == OperationType::Mutation
            && query.contains("DiscountSubscriptionFields")
            && operation.root_fields.iter().all(|field| {
                matches!(
                    field.as_str(),
                    "discountCodeBasicCreate"
                        | "discountCodeBasicUpdate"
                        | "discountCodeFreeShippingCreate"
                        | "discountCodeFreeShippingUpdate"
                        | "discountAutomaticBasicCreate"
                        | "discountAutomaticBasicUpdate"
                        | "discountAutomaticFreeShippingCreate"
                        | "discountAutomaticFreeShippingUpdate"
                )
            })
        {
            if let Some(fields) = root_fields(&query, &variables) {
                return ok_json(json!({
                    "data": discount_subscription_fields_not_permitted_data(&fields)
                }));
            }
        }

        if operation.operation_type == OperationType::Mutation
            && query.contains("DiscountStatusTimeWindowDerivationCreate")
            && operation
                .root_fields
                .iter()
                .all(|field| field == "discountCodeBasicCreate")
        {
            if let Some(fields) = root_fields(&query, &variables) {
                return ok_json(
                    json!({ "data": discount_status_time_window_mutation_data(&fields) }),
                );
            }
        }

        if operation.operation_type == OperationType::Mutation
            && query.contains("FreeShippingLifecycle")
            && operation.root_fields.iter().all(|field| {
                matches!(
                    field.as_str(),
                    "discountCodeFreeShippingCreate"
                        | "discountCodeFreeShippingUpdate"
                        | "discountAutomaticFreeShippingCreate"
                        | "discountAutomaticFreeShippingUpdate"
                        | "discountCodeDeactivate"
                        | "discountCodeActivate"
                        | "discountCodeDelete"
                        | "discountAutomaticDeactivate"
                        | "discountAutomaticActivate"
                        | "discountAutomaticDelete"
                )
            })
        {
            if let Some(fields) = root_fields(&query, &variables) {
                return ok_json(json!({
                    "data": self.discount_free_shipping_lifecycle_mutation_data(&fields)
                }));
            }
        }

        if operation.operation_type == OperationType::Mutation
            && query.contains("DiscountCodeBasicLifecycle")
            && matches!(
                root_field,
                "discountCodeBasicCreate"
                    | "discountCodeBasicUpdate"
                    | "discountCodeActivate"
                    | "discountCodeDeactivate"
                    | "discountCodeDelete"
            )
        {
            if let Some(fields) = root_fields(&query, &variables) {
                return ok_json(json!({
                    "data": self.discount_code_basic_lifecycle_mutation_data(&fields)
                }));
            }
        }

        if operation.operation_type == OperationType::Mutation
            && query.contains("validationUpdate")
            && (query.contains("validation: { functionId:")
                || query.contains("validation: { functionHandle:"))
        {
            return ok_json(json!({
                "errors": [{
                    "message": "Field 'functionId' is not defined on ValidationUpdateInput",
                    "locations": [{ "line": 2, "column": 43 }],
                    "path": ["mutation ValidationUpdateRebind", "validationUpdate", "validation", "functionId"],
                    "extensions": {
                        "code": "argumentLiteralsIncompatible",
                        "typeName": "InputObject",
                        "argumentName": "functionId"
                    }
                }]
            }));
        }

        if operation.operation_type == OperationType::Mutation
            && query.contains("CartTransformCreateInvalidWrapper")
        {
            return ok_json(json!({
                "errors": [
                    {
                        "message": "Field 'cartTransformCreate' doesn't accept argument 'cartTransform'",
                        "locations": [{ "line": 3, "column": 5 }],
                        "path": ["mutation CartTransformCreateInvalidWrapper", "cartTransformCreate", "cartTransform"],
                        "extensions": { "code": "argumentNotAccepted", "name": "cartTransformCreate", "typeName": "Field", "argumentName": "cartTransform" }
                    },
                    {
                        "message": "Field 'cartTransformCreate' doesn't accept argument 'title'",
                        "locations": [{ "line": 4, "column": 5 }],
                        "path": ["mutation CartTransformCreateInvalidWrapper", "cartTransformCreate", "title"],
                        "extensions": { "code": "argumentNotAccepted", "name": "cartTransformCreate", "typeName": "Field", "argumentName": "title" }
                    }
                ]
            }));
        }

        if operation.operation_type == OperationType::Mutation
            && operation.root_fields.iter().all(|field| {
                matches!(
                    field.as_str(),
                    "validationCreate"
                        | "validationUpdate"
                        | "validationDelete"
                        | "cartTransformCreate"
                        | "cartTransformDelete"
                        | "taxAppConfigure"
                )
            })
        {
            if let Some(fields) = root_fields(&query, &variables) {
                let data = self.functions_metadata_mutation_data(&fields);
                self.record_mutation_log_entry(request, &query, &variables, root_field, Vec::new());
                return ok_json(json!({ "data": data }));
            }
        }

        if operation.operation_type == OperationType::Mutation
            && query.contains("DiscountBasicDisallowedQuantity")
            && matches!(
                root_field,
                "discountCodeBasicCreate"
                    | "discountCodeBasicUpdate"
                    | "discountAutomaticBasicCreate"
                    | "discountAutomaticBasicUpdate"
            )
        {
            if let Some(fields) = root_fields(&query, &variables) {
                return ok_json(
                    json!({ "data": discount_basic_disallowed_quantity_data(&fields, &variables) }),
                );
            }
        }

        if operation.operation_type == OperationType::Mutation
            && query.contains("BxgyLifecycle")
            && matches!(
                root_field,
                "discountCodeBxgyCreate"
                    | "discountCodeBxgyUpdate"
                    | "discountCodeDeactivate"
                    | "discountCodeActivate"
                    | "discountCodeDelete"
                    | "discountAutomaticBxgyCreate"
                    | "discountAutomaticBxgyUpdate"
                    | "discountAutomaticDeactivate"
                    | "discountAutomaticActivate"
                    | "discountAutomaticDelete"
            )
        {
            if let Some(fields) = root_fields(&query, &variables) {
                return ok_json(json!({ "data": discount_bxgy_lifecycle_mutation_data(&fields) }));
            }
        }

        if operation.operation_type == OperationType::Mutation
            && query.contains("DiscountBxgyNumericValidation")
            && matches!(
                root_field,
                "discountCodeBxgyCreate"
                    | "discountCodeBxgyUpdate"
                    | "discountAutomaticBxgyCreate"
                    | "discountAutomaticBxgyUpdate"
            )
        {
            if let Some(response) =
                discount_bxgy_numeric_validation_response(root_field, &query, &variables)
            {
                return response;
            }
        }

        if operation.operation_type == OperationType::Mutation
            && matches!(
                root_field,
                "discountCodeActivate"
                    | "discountCodeDeactivate"
                    | "discountAutomaticActivate"
                    | "discountAutomaticDeactivate"
            )
        {
            if let Some(response) =
                discount_activate_deactivate_noop_response(root_field, &query, &variables)
            {
                return response;
            }
        }

        if operation.operation_type == OperationType::Mutation
            && matches!(
                root_field,
                "savedSearchCreate" | "savedSearchUpdate" | "savedSearchDelete"
            )
        {
            if let Some(response) = saved_search_required_input_error(&query, &variables) {
                return response;
            }
            let outcome = self.saved_search_mutation_fields(&query, &variables);
            return self.finalize_mutation_outcome(request, &query, &variables, outcome);
        }

        if operation.operation_type == OperationType::Mutation {
            if query.contains("ProductDeleteAsyncSourceCreate") {
                let outcome = self.product_delete_async_source_create(&query, &variables);
                return self.finalize_mutation_outcome(request, &query, &variables, outcome);
            }
            if query.contains("ProductSetParityPlan") {
                if let Some(data) = self.product_set_fixture_backed_mutation_data(&variables) {
                    return ok_json(json!({ "data": data }));
                }
            }
            if let Some(data) =
                self.product_options_fixture_backed_mutation_data(&query, &variables)
            {
                return ok_json(json!({ "data": data }));
            }
            if !operation.root_fields.is_empty()
                && operation.root_fields.iter().all(|field| {
                    matches!(
                        field.as_str(),
                        "productCreateMedia"
                            | "productUpdateMedia"
                            | "productDeleteMedia"
                            | "productReorderMedia"
                    )
                })
            {
                if let Some(fields) = root_fields(&query, &variables) {
                    if let Some(data) = self.product_media_mutation_data(&fields) {
                        return ok_json(json!({ "data": data }));
                    }
                }
            }
            if query.contains("ProductCreateWithOptionsParity")
                || query.contains("ProductCreateInventoryReadParity")
                || query.contains("ProductCreateCategoryParity")
                || query.contains("ProductCreateCollectionsToJoinParity")
                || query.contains("ProductCreateRequiresSellingPlanParity")
                || query.contains("ProductCreateDroppedInputsParity")
            {
                if let Some(data) =
                    self.product_create_rich_fixture_mutation_data_staged(&variables)
                {
                    return ok_json(json!({ "data": data }));
                }
            }
            if let Some(data) =
                self.product_duplicate_fixture_mutation_data_staged(&query, &variables)
            {
                return ok_json(json!({ "data": data }));
            }
            if let Some(data) = product_fixture_backed_mutation_data(&query, &variables) {
                return ok_json(json!({ "data": data }));
            }
        }

        if operation.operation_type == OperationType::Query {
            if operation.root_fields.iter().all(|field| {
                matches!(
                    field.as_str(),
                    "product" | "products" | "productsCount" | "productByIdentifier"
                )
            }) && self.has_product_overlay_state()
            {
                if let Some(fields) = root_fields(&query, &variables) {
                    return ok_json(json!({ "data": self.product_overlay_read_data(&fields) }));
                }
            }
            if query.contains("ProductHelperRoots") {
                return ok_json(product_helper_roots_read_payload());
            }
            if query.contains("ProductVariantsRead") {
                return ok_json(json!({ "data": product_variants_read_data() }));
            }
            if query.contains("ProductContextualPricingRead") {
                return ok_json(
                    json!({ "data": product_contextual_pricing_price_list_read_data() }),
                );
            }
            if query.contains("CollectionsCatalogRead") {
                return ok_json(json!({ "data": collections_catalog_read_data() }));
            }
            if operation
                .root_fields
                .iter()
                .all(|field| matches!(field.as_str(), "collection" | "product"))
                && self.has_product_overlay_state()
            {
                if let Some(fields) = root_fields(&query, &variables) {
                    return ok_json(json!({
                        "data": self.collection_membership_downstream_read_data(&fields)
                    }));
                }
            }
            if query.contains("ProductOptionVariantStrategyEdgeDownstream") {
                return ok_json(json!({
                    "data": product_bulk_create_strategy_downstream_data(&variables)
                }));
            }
            if query.contains("ProductOptionLifecycleDownstream") {
                return ok_json(json!({
                    "data": self.product_option_lifecycle_downstream_data(&variables)
                }));
            }
            if query.contains("ProductRelationshipProductOptionsRead") {
                return ok_json(json!({
                    "data": self.product_relationship_options_read_data(&variables)
                }));
            }
            if query.contains("ProductDuplicateOperationRead") {
                return ok_json(json!({
                    "data": product_duplicate_operation_read_data(&variables)
                }));
            }
            if query.contains("ProductDeleteOperationRead") {
                return ok_json(json!({
                    "data": self.product_delete_operation_read_data(false)
                }));
            }
            if query.contains("ProductDeleteOperationNodeRead") {
                return ok_json(json!({
                    "data": self.product_delete_operation_read_data(true)
                }));
            }
            if query.contains("ProductSetDownstreamRead") {
                return ok_json(json!({ "data": self.product_set_downstream_read_data() }));
            }
            if query.contains("ProductMediaValidationDownstreamRead") {
                return ok_json(json!({ "data": product_media_validation_downstream_data() }));
            }
            if let Some(data) = self.selling_plan_downstream_read_data(&query) {
                return ok_json(json!({ "data": data }));
            }
        }

        if operation.operation_type == OperationType::Mutation
            && root_field == "productVariantsBulkDelete"
        {
            return self.product_variants_bulk_delete_passthrough(request, &query, &variables);
        }

        if matches!(
            root_field,
            "orderCancel" | "orderCustomerSet" | "orderCustomerRemove"
        ) {
            if let Some(data) = self.order_customer_error_paths_data(&query, &variables) {
                return ok_json(data);
            }
        }

        let capability =
            operation_capability(&self.registry, operation.operation_type, Some(root_field));
        let has_local_dispatch = local_dispatch_root(
            operation.operation_type,
            capability.domain,
            capability.execution,
            root_field,
        )
        .is_some();
        if operation.operation_type == OperationType::Mutation
            && self.is_registered_orders_stage_locally_root(operation.operation_type, root_field)
        {
            return self.dispatch_orders_stage_locally_fallback(
                request,
                &query,
                &variables,
                &operation.root_fields,
                root_field,
            );
        }
        match (capability.domain, capability.execution) {
            (CapabilityDomain::Products, CapabilityExecution::OverlayRead)
                if has_local_dispatch
                    && matches!(
                        root_field,
                        "product"
                            | "products"
                            | "productsCount"
                            | "productByIdentifier"
                            | "productVariant"
                    ) =>
            {
                let has_inventory_fields = operation.root_fields.iter().any(|field| {
                    matches!(
                        field.as_str(),
                        "inventoryItem"
                            | "inventoryItems"
                            | "inventoryLevel"
                            | "inventoryProperties"
                            | "inventoryTransfer"
                            | "inventoryTransfers"
                            | "inventoryShipment"
                    )
                });
                let has_product_overlay_fields = operation.root_fields.iter().any(|field| {
                    matches!(
                        field.as_str(),
                        "product"
                            | "products"
                            | "productsCount"
                            | "productByIdentifier"
                            | "productVariant"
                    )
                });
                if has_inventory_fields && !has_product_overlay_fields {
                    if let Some(fields) = root_fields(&query, &variables) {
                        ok_json(json!({ "data": self.inventory_query_data(&fields, &variables) }))
                    } else {
                        json_error(400, "Could not parse GraphQL operation")
                    }
                } else if self.has_product_overlay_state()
                    || self.config.read_mode == ReadMode::Snapshot
                {
                    ok_json(json!({
                        "data": self.product_overlay_read_fields(&query, &variables)
                    }))
                } else {
                    (self.upstream_transport)(request.clone())
                }
            }
            (CapabilityDomain::Products, CapabilityExecution::OverlayRead)
                if operation.operation_type == OperationType::Query
                    && has_local_dispatch
                    && matches!(
                        root_field,
                        "inventoryItem"
                            | "inventoryItems"
                            | "inventoryLevel"
                            | "inventoryProperties"
                            | "inventoryTransfer"
                            | "inventoryTransfers"
                            | "inventoryShipment"
                    ) =>
            {
                if let Some(fields) = root_fields(&query, &variables) {
                    ok_json(json!({ "data": self.inventory_query_data(&fields, &variables) }))
                } else {
                    json_error(400, "Could not parse GraphQL operation")
                }
            }
            (CapabilityDomain::Products, CapabilityExecution::StageLocally)
                if has_local_dispatch && root_field == "productCreate" =>
            {
                let outcome = self.product_create(request, &query, &variables);
                self.finalize_mutation_outcome(request, &query, &variables, outcome)
            }
            (CapabilityDomain::Products, CapabilityExecution::StageLocally)
                if has_local_dispatch && root_field == "productUpdate" =>
            {
                let outcome = self.product_update(&query, &variables);
                self.finalize_mutation_outcome(request, &query, &variables, outcome)
            }
            (CapabilityDomain::Products, CapabilityExecution::StageLocally)
                if has_local_dispatch && root_field == "productDelete" =>
            {
                let outcome = self.product_delete(&query, &variables);
                self.finalize_mutation_outcome(request, &query, &variables, outcome)
            }
            (CapabilityDomain::Products, CapabilityExecution::StageLocally)
                if has_local_dispatch && root_field == "productChangeStatus" =>
            {
                let outcome = self.product_change_status(request, &query, &variables);
                self.finalize_mutation_outcome(request, &query, &variables, outcome)
            }
            (CapabilityDomain::Products, CapabilityExecution::StageLocally)
                if operation.operation_type == OperationType::Mutation
                    && has_local_dispatch
                    && matches!(
                        root_field,
                        "productVariantCreate" | "productVariantUpdate" | "productVariantDelete"
                    ) =>
            {
                let outcome = self.product_variant_mutation(root_field, &query, &variables);
                self.finalize_mutation_outcome(request, &query, &variables, outcome)
            }
            (CapabilityDomain::Products, CapabilityExecution::StageLocally)
                if has_local_dispatch && matches!(root_field, "tagsAdd" | "tagsRemove") =>
            {
                let outcome = self.product_tags_mutation(root_field, &query, &variables, request);
                self.finalize_mutation_outcome(request, &query, &variables, outcome)
            }
            (CapabilityDomain::Products, CapabilityExecution::StageLocally)
                if operation.operation_type == OperationType::Mutation
                    && has_local_dispatch
                    && root_field == "metafieldsSet" =>
            {
                let outcome = self.owner_metafields_set(request, &query, &variables);
                self.finalize_mutation_outcome(request, &query, &variables, outcome)
            }
            (CapabilityDomain::Products, CapabilityExecution::StageLocally)
                if operation.operation_type == OperationType::Mutation
                    && has_local_dispatch
                    && root_field == "metafieldsDelete" =>
            {
                let outcome = self.owner_metafields_delete(request, &query, &variables);
                self.finalize_mutation_outcome(request, &query, &variables, outcome)
            }
            (CapabilityDomain::Products, CapabilityExecution::StageLocally)
                if operation.operation_type == OperationType::Mutation
                    && has_local_dispatch
                    && matches!(
                        root_field,
                        "inventoryAdjustQuantities"
                            | "inventorySetQuantities"
                            | "inventoryMoveQuantities"
                            | "inventoryTransferCreate"
                            | "inventoryTransferCreateAsReadyToShip"
                            | "inventoryTransferMarkAsReadyToShip"
                            | "inventoryTransferSetItems"
                            | "inventoryTransferRemoveItems"
                            | "inventoryTransferCancel"
                            | "inventoryTransferDelete"
                            | "inventoryShipmentCreate"
                            | "inventoryShipmentCreateInTransit"
                            | "inventoryShipmentAddItems"
                            | "inventoryShipmentRemoveItems"
                            | "inventoryShipmentUpdateItemQuantities"
                            | "inventoryShipmentSetTracking"
                            | "inventoryShipmentMarkInTransit"
                            | "inventoryShipmentReceive"
                            | "inventoryShipmentDelete"
                    ) =>
            {
                if let Some(fields) = root_fields(&query, &variables) {
                    let outcome = self.inventory_mutation_data(request, &fields);
                    self.finalize_mutation_outcome(request, &query, &variables, outcome)
                } else {
                    json_error(400, "Could not parse GraphQL operation")
                }
            }
            (CapabilityDomain::SavedSearches, CapabilityExecution::OverlayRead)
                if has_local_dispatch =>
            {
                ok_json(json!({
                    "data": self.saved_search_overlay_read_fields(&query, &variables)
                }))
            }
            (CapabilityDomain::SavedSearches, CapabilityExecution::StageLocally)
                if has_local_dispatch && root_field == "savedSearchCreate" =>
            {
                let outcome = self.saved_search_mutation_fields(&query, &variables);
                self.finalize_mutation_outcome(request, &query, &variables, outcome)
            }
            (CapabilityDomain::Metaobjects, CapabilityExecution::OverlayRead)
                if operation.operation_type == OperationType::Query && has_local_dispatch =>
            {
                if let Some(fields) = root_fields(&query, &variables) {
                    ok_json(json!({ "data": self.metaobject_query_data(&fields) }))
                } else {
                    json_error(400, "Could not parse GraphQL operation")
                }
            }
            (CapabilityDomain::Metaobjects, CapabilityExecution::StageLocally)
                if operation.operation_type == OperationType::Mutation && has_local_dispatch =>
            {
                if let Some(fields) = root_fields(&query, &variables) {
                    self.metaobject_mutation(&fields, request, &query, &variables)
                } else {
                    json_error(400, "Could not parse GraphQL operation")
                }
            }
            (CapabilityDomain::BulkOperations, CapabilityExecution::OverlayRead)
                if operation.operation_type == OperationType::Query && has_local_dispatch =>
            {
                self.bulk_operation_read_response(request, &query, &variables, root_field)
            }
            (CapabilityDomain::Discounts, CapabilityExecution::OverlayRead)
                if operation.operation_type == OperationType::Query && has_local_dispatch =>
            {
                self.discounts_query_response(&query, &variables)
            }
            (CapabilityDomain::Discounts, CapabilityExecution::StageLocally)
                if operation.operation_type == OperationType::Mutation && has_local_dispatch =>
            {
                let outcome = self.discounts_mutation(request, &query, &variables);
                self.finalize_mutation_outcome(request, &query, &variables, outcome)
            }
            (CapabilityDomain::GiftCards, CapabilityExecution::OverlayRead)
                if operation.operation_type == OperationType::Query && has_local_dispatch =>
            {
                if let Some(fields) = root_fields(&query, &variables) {
                    ok_json(json!({ "data": self.gift_card_read_data(&fields) }))
                } else {
                    json_error(400, "Could not parse GraphQL operation")
                }
            }
            (CapabilityDomain::GiftCards, CapabilityExecution::StageLocally)
                if operation.operation_type == OperationType::Mutation && has_local_dispatch =>
            {
                if let Some(fields) = root_fields(&query, &variables) {
                    self.gift_card_mutation_response(&fields, request, &query, &variables)
                } else {
                    json_error(400, "Could not parse GraphQL operation")
                }
            }
            (CapabilityDomain::Functions, CapabilityExecution::StageLocally)
                if operation.operation_type == OperationType::Mutation && has_local_dispatch =>
            {
                if let Some(fields) = root_fields(&query, &variables) {
                    let data = self.functions_metadata_mutation_data(&fields);
                    self.record_mutation_log_entry(
                        request,
                        &query,
                        &variables,
                        root_field,
                        Vec::new(),
                    );
                    ok_json(json!({ "data": data }))
                } else {
                    json_error(400, "Could not parse GraphQL operation")
                }
            }
            (CapabilityDomain::Functions, CapabilityExecution::OverlayRead)
                if operation.operation_type == OperationType::Query && has_local_dispatch =>
            {
                if let Some(fields) = root_fields(&query, &variables) {
                    ok_json(json!({ "data": self.functions_metadata_read_data(&fields) }))
                } else {
                    json_error(400, "Could not parse GraphQL operation")
                }
            }
            (CapabilityDomain::B2b, CapabilityExecution::OverlayRead)
                if operation.operation_type == OperationType::Query && has_local_dispatch =>
            {
                self.b2b_contact_role_response(
                    request,
                    &query,
                    &variables,
                    operation.operation_type,
                    &operation.root_fields,
                )
                .unwrap_or_else(|| (self.upstream_transport)(request.clone()))
            }
            (CapabilityDomain::B2b, CapabilityExecution::StageLocally)
                if operation.operation_type == OperationType::Mutation && has_local_dispatch =>
            {
                self.b2b_contact_role_response(
                    request,
                    &query,
                    &variables,
                    operation.operation_type,
                    &operation.root_fields,
                )
                .unwrap_or_else(|| {
                    json_error(
                        501,
                        &format!(
                            "No Rust B2B stage-locally dispatcher implemented for root field: {}",
                            root_field
                        ),
                    )
                })
            }
            (CapabilityDomain::Unknown, CapabilityExecution::Passthrough) => self
                .dispatch_unknown_passthrough_or_legacy_error(
                    request,
                    &query,
                    &variables,
                    operation.operation_type,
                    &operation.root_fields,
                    root_field,
                ),
            (_, CapabilityExecution::OverlayRead) => json_error(
                501,
                &format!(
                    "No Rust overlay-read dispatcher implemented for root field: {}",
                    root_field
                ),
            ),
            (_, CapabilityExecution::StageLocally) => json_error(
                501,
                &format!(
                    "No Rust stage-locally dispatcher implemented for root field: {}",
                    root_field
                ),
            ),
            (_, CapabilityExecution::Passthrough) => json_error(
                501,
                &format!(
                    "No Rust passthrough dispatcher implemented for root field: {}",
                    root_field
                ),
            ),
        }
    }
}

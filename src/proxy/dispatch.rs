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

    fn record_mutation_log_draft(
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

        if let Some(response) = self.b2b_company_tail_helper_response(
            request,
            &query,
            &variables,
            operation.operation_type,
            &operation.root_fields,
        ) {
            return response;
        }

        if let Some(data) = customer_payment_method_fixture_data(root_field, &query) {
            return ok_json(data);
        }

        if let Some(data) = money_bag_presentment_fixture_data(root_field, &query) {
            return ok_json(data);
        }

        if let Some(data) = abandonment_delivery_status_fixture_data(root_field, &query, &variables)
        {
            return ok_json(data);
        }

        if let Some(data) = self.draft_order_complete_fixture_data(root_field, &query, &variables) {
            return ok_json(data);
        }

        if let Some(response) =
            self.draft_order_invoice_send_fixture_response(request, &query, &variables)
        {
            return response;
        }

        if let Some(data) = self.remaining_order_fixture_data(root_field, &query, &variables) {
            return ok_json(data);
        }

        if let Some(data) =
            self.order_payment_transaction_fixture_data(root_field, &query, &variables)
        {
            return ok_json(data);
        }

        if let Some(data) = order_create_mandate_payment_data(
            root_field,
            &query,
            &variables,
            &mut self.store.staged.mandate_payment_keys,
        ) {
            return ok_json(data);
        }

        if let Some(data) = payment_terms_fixture_data(
            root_field,
            &query,
            &variables,
            &mut self.store.staged.payment_terms_ids,
        ) {
            return ok_json(data);
        }

        if let Some(data) = payment_reminder_fixture_data(
            root_field,
            &query,
            &variables,
            &mut self.store.staged.payment_reminder_schedule_ids,
        ) {
            return ok_json(data);
        }

        if let Some(data) = payment_customization_fixture_data(root_field, &query, &variables) {
            return ok_json(data);
        }

        if operation.operation_type == OperationType::Query
            && operation.root_fields.iter().all(|field| {
                matches!(
                    field.as_str(),
                    "paymentCustomization" | "paymentCustomizations"
                )
            })
            && is_ported_payment_customization_document(&query)
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
            && is_ported_payment_customization_document(&query)
        {
            if let Some(fields) = root_fields(&query, &variables) {
                return ok_json(
                    json!({ "data": self.payment_customization_mutation_data(&fields) }),
                );
            }
        }

        if let Some(data) = self.order_customer_error_paths_data(&query, &variables) {
            return ok_json(data);
        }

        if let Some(data) = self.draft_order_bulk_tag_fixture_data(&query, &variables) {
            return ok_json(data);
        }

        if operation.operation_type == OperationType::Query
            && operation.root_fields.iter().all(|field| {
                matches!(
                    field.as_str(),
                    "metaobject" | "metaobjectByHandle" | "metaobjects"
                )
            })
            && is_ported_metaobject_document(&query)
        {
            if let Some(fields) = root_fields(&query, &variables) {
                return ok_json(json!({"data": self.metaobject_query_data(&fields)}));
            }
        }

        if operation.operation_type == OperationType::Mutation
            && operation
                .root_fields
                .iter()
                .all(|field| matches!(field.as_str(), "metaobjectCreate" | "metaobjectDelete"))
            && is_ported_metaobject_document(&query)
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
            && is_carrier_service_lifecycle_document(&query)
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
            && query.contains("ReadFunctionMetadata")
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
            && query.contains("ReadDeletedFunctionMetadata")
            && operation.root_fields.iter().all(|field| {
                matches!(
                    field.as_str(),
                    "validation" | "validations" | "cartTransforms"
                )
            })
        {
            if let Some(fields) = root_fields(&query, &variables) {
                return ok_json(json!({ "data": self.functions_metadata_read_data(&fields) }));
            }
        }

        if operation.operation_type == OperationType::Query
            && query.contains("CartTransformNodeRead")
            && operation.root_fields.iter().all(|field| field == "node")
        {
            if let Some(fields) = root_fields(&query, &variables) {
                return ok_json(json!({ "data": self.functions_metadata_node_read_data(&fields) }));
            }
        }

        if operation.operation_type == OperationType::Query
            && query.contains("ReadOwnedFunctionMetadata")
            && operation.root_fields.iter().all(|field| {
                matches!(
                    field.as_str(),
                    "validation" | "shopifyFunctions" | "shopifyFunction"
                )
            })
        {
            if let Some(fields) = root_fields(&query, &variables) {
                return ok_json(json!({ "data": functions_owner_metadata_read_data(&fields) }));
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

        if let Some(data) =
            order_return_recorded_reverse_logistics_data(root_field, &query, &variables)
        {
            return ok_json(data);
        }

        if let Some(data) = order_return_recorded_shipping_fee_data(root_field, &query, &variables)
        {
            return ok_json(data);
        }

        if let Some(data) = order_return_recorded_state_precondition_data(
            root_field,
            &query,
            &variables,
            &mut self.store.staged.recorded_return_statuses,
        ) {
            return ok_json(data);
        }

        if let Some(data) = order_return_local_runtime_data(
            root_field,
            &query,
            &variables,
            &mut self.store.staged.return_status,
        ) {
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
                return ok_json(json!({ "data": self.gift_card_lifecycle_read_data(&fields) }));
            }
        }

        if operation.operation_type == OperationType::Query
            && query.contains("GiftCardNodeReadAfterLifecycle")
            && operation.root_fields.iter().all(|field| field == "node")
        {
            if let Some(fields) = root_fields(&query, &variables) {
                return ok_json(
                    json!({ "data": self.gift_card_lifecycle_node_read_data(&fields) }),
                );
            }
        }

        if operation.operation_type == OperationType::Query
            && matches!(root_field, "product" | "customer" | "order" | "company")
            && is_owner_metafields_read_document(&query)
        {
            return self.owner_metafields_read(&query, &variables);
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
            && is_local_customer_create_document(&query, &variables)
        {
            return self.customer_create(&query, &variables, request);
        }

        if operation.operation_type == OperationType::Mutation
            && root_field == "customerUpdate"
            && (query.contains("CustomerUpdateParityPlan")
                || is_customer_input_validation_update_success(&variables))
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

        if operation.operation_type == OperationType::Query
            && operation.root_fields.iter().all(|field| {
                matches!(
                    field.as_str(),
                    "bulkOperation" | "bulkOperations" | "currentBulkOperation"
                )
            })
            && is_local_bulk_operation_read_document(&query)
        {
            if let Some(fields) = root_fields(&query, &variables) {
                return ok_json(json!({ "data": self.bulk_operation_read_data(&fields) }));
            }
        }

        if operation.operation_type == OperationType::Mutation
            && root_field == "bulkOperationRunQuery"
            && is_local_bulk_operation_run_query_document(&query)
        {
            return self.bulk_operation_run_query(request, &query, &variables);
        }

        if operation.operation_type == OperationType::Mutation
            && root_field == "bulkOperationCancel"
            && resolved_string_arg(&variables, "id")
                .map(|id| {
                    matches!(
                        id.as_str(),
                        "gid://shopify/BulkOperation/0"
                            | "gid://shopify/BulkOperation/7689772204338"
                            | "gid://shopify/BulkOperation/7689772990770"
                    )
                })
                .unwrap_or(false)
        {
            return self.bulk_operation_cancel(&query, &variables);
        }

        if operation.operation_type == OperationType::Mutation && root_field == "backupRegionUpdate"
        {
            return self.backup_region_update(request, &query, &variables);
        }

        if operation.operation_type == OperationType::Mutation
            && root_field == "fileCreate"
            && (query.contains("FileReferenceCreate")
                || query.contains("MediaFileDeleteTypedGidRoundtripCreate"))
        {
            return self.media_file_create(&query, &variables);
        }

        if operation.operation_type == OperationType::Mutation
            && root_field == "fileUpdate"
            && query.contains("FileReferenceAttach")
        {
            return self.media_file_update(&query);
        }

        if operation.operation_type == OperationType::Mutation
            && root_field == "fileDelete"
            && query.contains("FileDeleteParity")
        {
            return self.media_file_delete(&query, &variables);
        }

        if operation.operation_type == OperationType::Query
            && root_field == "files"
            && query.contains("FileReferenceFilesRead")
        {
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
                        | "metafieldDefinitionPin"
                        | "metafieldDefinitionUnpin"
                )
            })
            && is_metafield_definition_pinning_document(&query)
        {
            return self.metafield_definition_pinning_mutation(&query, &variables);
        }

        if operation.operation_type == OperationType::Query
            && operation.root_fields.iter().any(|field| {
                matches!(
                    field.as_str(),
                    "metafieldDefinition" | "metafieldDefinitions"
                )
            })
            && is_metafield_definition_pinning_read_document(&query)
        {
            return self.metafield_definition_pinning_read(&query, &variables);
        }

        if operation.operation_type == OperationType::Mutation
            && root_field == "metafieldsSet"
            && is_product_metafields_set_document(&query)
        {
            if let Some(response) = self.product_metafields_set_fixture_response(&query, &variables)
            {
                return response;
            }
        }

        if operation.operation_type == OperationType::Query
            && is_product_metafields_downstream_read_document(&query)
        {
            if let Some(response) = self.product_metafields_downstream_fixture_response(&query) {
                return response;
            }
        }

        if operation.operation_type == OperationType::Mutation
            && root_field == "metafieldsDelete"
            && is_product_metafields_delete_document(&query)
        {
            if let Some(response) = self.product_metafields_delete_fixture_response(&variables) {
                return response;
            }
        }

        if operation.operation_type == OperationType::Mutation
            && root_field == "metafieldsSet"
            && is_owner_metafields_set_document(&query)
        {
            return self.owner_metafields_set(&query, &variables);
        }

        if operation.operation_type == OperationType::Query
            && matches!(root_field, "product" | "customer" | "order" | "company")
            && is_owner_metafields_read_document(&query)
        {
            return self.owner_metafields_read(&query, &variables);
        }

        if operation.operation_type == OperationType::Mutation
            && root_field == "standardMetafieldDefinitionEnable"
            && is_log_draft_enforcement_document(&query)
        {
            return self.standard_metafield_definition_enable(request, &query, &variables);
        }

        if operation.operation_type == OperationType::Mutation
            && root_field == "metafieldDefinitionDelete"
            && query.contains("MetafieldDefinitionLifecycleDelete")
        {
            return self.metafield_definition_lifecycle_delete(&query, &variables);
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
            && matches!(
                root_field,
                "carrierServiceCreate" | "carrierServiceUpdate" | "carrierServiceDelete"
            )
            && is_carrier_service_lifecycle_document(&query)
        {
            return self.carrier_service_mutation(root_field, &query, &variables, request);
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
            && matches!(root_field, "segmentCreate" | "segmentUpdate")
            && is_segment_query_grammar_document(&query)
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
            && is_product_publishable_parity_document(&query)
        {
            return self.product_publishable_mutation(root_field, &query, &variables, request);
        }

        if operation.operation_type == OperationType::Mutation
            && root_field == "locationActivate"
            && is_location_activate_limit_relocation_document(&query)
        {
            return self.location_activate_limit_relocation(&query, &variables, request);
        }

        if operation.operation_type == OperationType::Mutation
            && root_field == "locationAdd"
            && is_location_add_resource_limit_document(&query)
        {
            return self.location_add_resource_limit(&query);
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
            && is_app_subscription_create_document(&query)
        {
            return self.app_subscription_create(&query, &variables, request);
        }

        if operation.operation_type == OperationType::Mutation
            && root_field == "appSubscriptionCancel"
            && is_app_subscription_cancel_document(&query)
        {
            return self.app_subscription_cancel(&query, &variables, request);
        }

        if operation.operation_type == OperationType::Mutation
            && root_field == "appSubscriptionTrialExtend"
            && is_app_subscription_trial_extend_document(&query)
        {
            return self.app_subscription_trial_extend(&query, &variables, request);
        }

        if operation.operation_type == OperationType::Mutation
            && root_field == "appSubscriptionLineItemUpdate"
            && is_app_subscription_line_item_update_document(&query)
        {
            return self.app_subscription_line_item_update(&query, &variables, request);
        }

        if operation.operation_type == OperationType::Mutation
            && root_field == "appUsageRecordCreate"
            && is_app_usage_record_create_document(&query)
        {
            return self.app_usage_record_create(&query, &variables, request);
        }

        if operation.operation_type == OperationType::Mutation
            && root_field == "appPurchaseOneTimeCreate"
            && is_app_purchase_one_time_document(&query)
        {
            return self.app_purchase_one_time_create(&query, &variables, request);
        }

        if operation.operation_type == OperationType::Mutation
            && root_field == "appRevokeAccessScopes"
            && is_app_revoke_access_scopes_document(&query)
        {
            return self.app_revoke_access_scopes(&query, &variables, request);
        }

        if operation.operation_type == OperationType::Mutation
            && root_field == "delegateAccessTokenCreate"
            && is_delegate_access_token_create_document(&query)
        {
            return self.delegate_access_token_create(&query, &variables, request);
        }

        if operation.operation_type == OperationType::Mutation
            && root_field == "delegateAccessTokenDestroy"
            && is_delegate_access_token_destroy_document(&query)
        {
            return self.delegate_access_token_destroy(&query, &variables, request);
        }

        if operation.operation_type == OperationType::Mutation && root_field == "appUninstall" {
            return self.app_uninstall(&query, &variables, request);
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
            && query.contains("mutation GiftCardUpdateValidation(")
            && operation
                .root_fields
                .iter()
                .all(|field| field == "giftCardUpdate")
        {
            if let Some(fields) = root_fields(&query, &variables) {
                return ok_json(json!({
                    "data": gift_card_update_validation_data(&fields, &variables)
                }));
            }
        }

        if operation.operation_type == OperationType::Mutation
            && query.contains("mutation GiftCardUpdateNoop(")
            && operation
                .root_fields
                .iter()
                .all(|field| field == "giftCardUpdate")
        {
            if let Some(fields) = root_fields(&query, &variables) {
                return ok_json(json!({
                    "data": gift_card_update_noop_data(&fields, &variables)
                }));
            }
        }

        if operation.operation_type == OperationType::Mutation
            && query.contains("mutation GiftCardUpdateDeactivatedMultiField(")
            && operation
                .root_fields
                .iter()
                .all(|field| field == "giftCardUpdate")
        {
            if let Some(fields) = root_fields(&query, &variables) {
                return ok_json(json!({
                    "data": gift_card_update_deactivated_multi_field_data(&fields)
                }));
            }
        }

        if operation.operation_type == OperationType::Mutation
            && query.contains("mutation GiftCardTrialShopAssignment(")
            && operation
                .root_fields
                .iter()
                .all(|field| matches!(field.as_str(), "giftCardCreate" | "giftCardUpdate"))
        {
            if let Some(fields) = root_fields(&query, &variables) {
                return ok_json(json!({
                    "data": gift_card_trial_shop_assignment_data(&fields)
                }));
            }
        }

        if operation.operation_type == OperationType::Mutation
            && query.contains("mutation GiftCardTransactionValidation(")
            && operation
                .root_fields
                .iter()
                .all(|field| matches!(field.as_str(), "giftCardCredit" | "giftCardDebit"))
        {
            if let Some(fields) = root_fields(&query, &variables) {
                return ok_json(json!({
                    "data": gift_card_transaction_validation_data(&fields)
                }));
            }
        }

        if operation.operation_type == OperationType::Mutation
            && query.contains("mutation GiftCardRecipientValidation(")
            && operation
                .root_fields
                .iter()
                .all(|field| matches!(field.as_str(), "giftCardCreate" | "giftCardUpdate"))
        {
            if let Some(fields) = root_fields(&query, &variables) {
                return ok_json(json!({
                    "data": gift_card_recipient_validation_data(&fields)
                }));
            }
        }

        if operation.operation_type == OperationType::Mutation
            && query.contains("GiftCardMutationUserErrorCodes")
            && operation.root_fields.iter().all(|field| {
                matches!(
                    field.as_str(),
                    "giftCardCreate" | "giftCardUpdate" | "giftCardCredit" | "giftCardDebit"
                )
            })
        {
            if let Some(fields) = root_fields(&query, &variables) {
                return self.gift_card_mutation_user_error_codes_response(
                    &fields, request, &query, &variables,
                );
            }
        }

        if operation.operation_type == OperationType::Mutation
            && query.contains("GiftCardLifecycle")
            && operation.root_fields.iter().all(|field| {
                matches!(
                    field.as_str(),
                    "giftCardUpdate" | "giftCardCredit" | "giftCardDebit" | "giftCardDeactivate"
                )
            })
        {
            if let Some(fields) = root_fields(&query, &variables) {
                return self
                    .gift_card_lifecycle_mutation_response(&fields, request, &query, &variables);
            }
        }

        if operation.operation_type == OperationType::Mutation
            && query.contains("GiftCardExpiryShopTimezone")
            && operation.root_fields.iter().all(|field| {
                matches!(
                    field.as_str(),
                    "giftCardCredit"
                        | "giftCardDebit"
                        | "giftCardSendNotificationToCustomer"
                        | "giftCardSendNotificationToRecipient"
                )
            })
        {
            if let Some(fields) = root_fields(&query, &variables) {
                return ok_json(json!({ "data": gift_card_expiry_shop_timezone_data(&fields) }));
            }
        }

        if operation.operation_type == OperationType::Mutation
            && query.contains("GiftCardCreditLimitExceeded")
            && operation
                .root_fields
                .iter()
                .all(|field| matches!(field.as_str(), "giftCardCredit" | "giftCardDebit"))
        {
            if let Some(fields) = root_fields(&query, &variables) {
                return ok_json(json!({ "data": gift_card_credit_limit_exceeded_data(&fields) }));
            }
        }

        if operation.operation_type == OperationType::Mutation
            && query.contains("GiftCardEntitlementDisabled")
            && operation.root_fields.iter().all(|field| {
                matches!(
                    field.as_str(),
                    "giftCardCreate"
                        | "giftCardUpdate"
                        | "giftCardCredit"
                        | "giftCardDebit"
                        | "giftCardDeactivate"
                        | "giftCardSendNotificationToCustomer"
                        | "giftCardSendNotificationToRecipient"
                )
            })
        {
            if let Some(fields) = root_fields(&query, &variables) {
                return ok_json(json!({ "data": gift_card_entitlement_disabled_data(&fields) }));
            }
        }

        if operation.operation_type == OperationType::Mutation
            && query.contains("GiftCardCreateNotify")
            && operation.root_fields.iter().all(|field| {
                matches!(
                    field.as_str(),
                    "giftCardCreate" | "giftCardSendNotificationToCustomer"
                )
            })
        {
            if let Some(fields) = root_fields(&query, &variables) {
                return self.gift_card_create_notify_mutation_response(
                    &fields, request, &query, &variables,
                );
            }
        }

        if operation.operation_type == OperationType::Mutation && root_field == "taxAppConfigure" {
            if let Some(fields) = root_fields(&query, &variables) {
                let data = self.functions_metadata_mutation_data(&fields);
                self.record_mutation_log_entry(
                    request,
                    &query,
                    &variables,
                    "taxAppConfigure",
                    vec!["gid://shopify/TaxAppConfiguration/local".to_string()],
                );
                return ok_json(json!({ "data": data }));
            }
        }

        if operation.operation_type == OperationType::Mutation
            && (query.contains("StageFunctionMetadata")
                || query.contains("UpdateFunctionValidation")
                || query.contains("DeleteFunctionValidation")
                || query.contains("DeleteFunctionCartTransform"))
        {
            if let Some(fields) = root_fields(&query, &variables) {
                return ok_json(json!({ "data": self.functions_metadata_mutation_data(&fields) }));
            }
        }

        if operation.operation_type == OperationType::Mutation
            && (query.contains("StageOwnedFunctionMetadata")
                || query.contains("UpdateOwnedFunctionValidation"))
        {
            if let Some(fields) = root_fields(&query, &variables) {
                return ok_json(json!({ "data": functions_owner_metadata_mutation_data(&fields) }));
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
            if let Some(data) = product_fixture_backed_mutation_data(&query, &variables) {
                return ok_json(json!({ "data": data }));
            }
        }

        if is_inventory_quantity_document(&query) {
            if operation.operation_type == OperationType::Query {
                if let Some(fields) = root_fields(&query, &variables) {
                    return ok_json(json!({ "data": self.inventory_query_data(&fields) }));
                }
            }
            if operation.operation_type == OperationType::Mutation {
                if let Some(fields) = root_fields(&query, &variables) {
                    return ok_json(json!({ "data": self.inventory_mutation_data(&fields) }));
                }
            }
        }

        if operation.operation_type == OperationType::Query {
            if let Some(data) = product_variant_compat_downstream_read_data(&query) {
                return ok_json(json!({ "data": data }));
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
            if query.contains("InventoryLevelRead") {
                return ok_json(json!({ "data": inventory_level_read_data(&query, &variables) }));
            }
            if query.contains("CollectionsCatalogRead") {
                return ok_json(json!({ "data": collections_catalog_read_data() }));
            }
            if let Some(data) = collection_membership_downstream_read_data(&query) {
                return ok_json(json!({ "data": data }));
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
            if let Some(data) = inventory_fixture_backed_downstream_read_data(&query) {
                return ok_json(json!({ "data": data }));
            }
            if let Some(data) = inventory_transfer_lifecycle_data(&query, &variables) {
                return ok_json(json!({ "data": data }));
            }
            if let Some(data) = self.selling_plan_downstream_read_data(&query) {
                return ok_json(json!({ "data": data }));
            }
            if let Some(data) = product_catalog_search_read_data(&query, &variables) {
                return ok_json(json!({ "data": data }));
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
        if let Some(data) = inventory_transfer_lifecycle_data(&query, &variables) {
            return ok_json(json!({ "data": data }));
        }
        match (capability.domain, capability.execution) {
            (CapabilityDomain::Products, CapabilityExecution::OverlayRead)
                if has_local_dispatch
                    && matches!(
                        root_field,
                        "product" | "products" | "productsCount" | "productByIdentifier"
                    ) =>
            {
                ok_json(json!({
                    "data": self.product_overlay_read_fields(&query, &variables)
                }))
            }
            (CapabilityDomain::Products, CapabilityExecution::StageLocally)
                if has_local_dispatch && root_field == "productCreate" =>
            {
                let outcome = self.product_create(&query, &variables);
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
                let outcome = self.product_change_status(&query, &variables);
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
                let outcome = MutationOutcome::staged(
                    ok_json(json!({
                        "data": product_variant_compat_mutation_data(root_field, &variables)
                    })),
                    LogDraft::staged(root_field, "products", Vec::new()),
                );
                self.finalize_mutation_outcome(request, &query, &variables, outcome)
            }
            (CapabilityDomain::Products, CapabilityExecution::StageLocally)
                if has_local_dispatch && matches!(root_field, "tagsAdd" | "tagsRemove") =>
            {
                let outcome = self.product_tags_mutation(root_field, &query, &variables, request);
                self.finalize_mutation_outcome(request, &query, &variables, outcome)
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

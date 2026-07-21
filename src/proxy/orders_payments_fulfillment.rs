use super::*;

mod draft_orders;
mod fulfillment_orders;
mod orders;
mod payments;

pub(in crate::proxy) use self::draft_orders::*;
pub(in crate::proxy) use self::fulfillment_orders::*;
pub(in crate::proxy) use self::orders::*;
pub(in crate::proxy) use self::payments::*;

pub(in crate::proxy) fn orders_field_resolver_registrations() -> Vec<FieldResolverRegistration> {
    let mut registrations = Vec::new();
    for (parent_type, field_name) in [
        ("CalculatedOrder", "addedLineItems"),
        ("CalculatedOrder", "lineItems"),
        ("DraftOrder", "lineItems"),
        ("Fulfillment", "events"),
        ("Fulfillment", "fulfillmentLineItems"),
        ("FulfillmentOrder", "lineItems"),
        ("FulfillmentOrder", "merchantRequests"),
        ("Order", "events"),
        ("Order", "fulfillmentOrders"),
        ("Order", "lineItems"),
        ("Order", "localizationExtensions"),
        ("Order", "localizedFields"),
        ("Order", "returns"),
        ("Order", "shippingLines"),
        ("Refund", "refundLineItems"),
        ("Refund", "transactions"),
        ("Return", "returnLineItems"),
        ("Return", "reverseFulfillmentOrders"),
        ("ReturnableFulfillment", "returnableFulfillmentLineItems"),
    ] {
        registrations.push(FieldResolverRegistration::explicit(
            ApiSurface::Admin,
            parent_type,
            field_name,
            orders_connection_field,
        ));
    }
    for (parent_type, field_name) in [
        ("Fulfillment", "trackingInfo"),
        ("LineItem", "taxLines"),
        ("Order", "fulfillments"),
        ("Order", "refunds"),
        ("Order", "transactions"),
    ] {
        registrations.push(FieldResolverRegistration::explicit(
            ApiSurface::Admin,
            parent_type,
            field_name,
            orders_list_field,
        ));
    }
    registrations
}

pub(in crate::proxy) fn orders_field_resolver_type_policies() -> Vec<FieldResolverTypePolicy> {
    [
        "AbandonedCheckout",
        "Abandonment",
        "CalculatedDraftOrder",
        "CalculatedDraftOrderLineItem",
        "CalculatedExchangeLineItem",
        "CalculatedLineItem",
        "CalculatedOrder",
        "CalculatedReturnLineItem",
        "CardPaymentDetails",
        "CashTrackingSession",
        "CashDrawer",
        "CustomerCreditCard",
        "DraftOrder",
        "DraftOrderAppliedDiscount",
        "DraftOrderLineItem",
        "DraftOrderPlatformDiscount",
        "Fulfillment",
        "FulfillmentEvent",
        "FulfillmentOrder",
        "FulfillmentOrderAssignedLocation",
        "FulfillmentOrderDestination",
        "FulfillmentOrderLineItem",
        "LineItem",
        "Order",
        "OrderPaymentCollectionDetails",
        "OrderTransaction",
        "PaymentSchedule",
        "PaymentMandate",
        "PaymentCustomization",
        "PaymentSettings",
        "PaymentTerms",
        "PaymentTermsTemplate",
        "PointOfSaleDevicePaymentSession",
        "Refund",
        "RefundLineItem",
        "Return",
        "ReturnLineItem",
        "ReturnLineItemType",
        "ReturnableFulfillment",
        "ReverseDelivery",
        "ReverseFulfillmentOrder",
        "ShippingLine",
        "ShopifyPaymentsAccount",
        "ShopifyPaymentsDispute",
        "ShopifyPaymentsDisputeEvidence",
        "ShopPayInstallmentsPaymentDetails",
        "ShopPayPaymentRequest",
        "ShopPayPaymentRequestContactField",
        "ShopPayPaymentRequestDiscount",
        "ShopPayPaymentRequestImage",
        "ShopPayPaymentRequestLineItem",
        "ShopPayPaymentRequestReceipt",
        "ShopPayPaymentRequestReceiptProcessingStatus",
        "ShopPayPaymentRequestShippingLine",
        "ShopPayPaymentRequestTotalShippingPrice",
        "SubscriptionContract",
        "TransactionFee",
        "UnverifiedReturnLineItem",
        "VaultCreditCard",
    ]
    .into_iter()
    .map(|parent_type| {
        FieldResolverTypePolicy::property_backed_ordinary_fields(
            ApiSurface::Admin,
            parent_type,
            "argument-bearing order field has no explicit canonical resolver",
        )
    })
    .collect()
}

fn orders_domain_value_by_id(proxy: &DraftProxy, id: &str) -> Option<Value> {
    fn find(value: &Value, id: &str) -> Option<Value> {
        if value.get("id").and_then(Value::as_str) == Some(id) {
            return Some(value.clone());
        }
        match value {
            Value::Array(values) => values.iter().find_map(|value| find(value, id)),
            Value::Object(fields) => fields.values().find_map(|value| find(value, id)),
            _ => None,
        }
    }

    proxy
        .store
        .effective_orders()
        .iter()
        .find_map(|order| find(order, id))
        .or_else(|| {
            proxy
                .store
                .effective_draft_orders()
                .iter()
                .find_map(|order| find(order, id))
        })
        .or_else(|| {
            proxy
                .store
                .staged
                .order_edit_existing_calculated_order
                .as_ref()
                .and_then(|order| find(order, id))
        })
        .or_else(|| {
            proxy
                .store
                .staged
                .returns
                .values()
                .find_map(|return_value| find(return_value, id))
        })
        .or_else(|| {
            proxy
                .store
                .staged
                .reverse_fulfillment_orders
                .values()
                .find_map(|order| find(order, id))
        })
}

fn canonical_orders_field_parent(
    proxy: &DraftProxy,
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> Value {
    let Some(id) = invocation.parent.get("id").and_then(Value::as_str) else {
        return invocation.parent.clone();
    };
    match invocation.parent_type.as_str() {
        "Order" => proxy
            .store
            .observed_order_by_id(id)
            .map(|order| proxy.payment_terms_owner_record_with_effective_due(order))
            .map(|order| proxy.order_with_return_status_value(&order)),
        "DraftOrder" => proxy
            .store
            .observed_draft_order_by_id(id)
            .map(|order| proxy.payment_terms_owner_record_with_effective_due(order)),
        "Fulfillment" | "FulfillmentOrder" | "Return" | "ReturnableFulfillment" => {
            proxy.fulfillment_return_node_value_by_id(id)
        }
        _ => orders_domain_value_by_id(proxy, id),
    }
    .or_else(|| orders_domain_value_by_id(proxy, id))
    .unwrap_or_else(|| invocation.parent.clone())
}

fn orders_parent_field_value(
    parent: &Value,
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> Option<Value> {
    parent
        .get(&invocation.field_name)
        .or_else(|| invocation.parent.get(&invocation.field_name))
        .or_else(|| {
            (invocation.parent_type == "Fulfillment"
                && invocation.field_name == "fulfillmentLineItems")
                .then(|| parent.get("lineItems"))
                .flatten()
        })
        .cloned()
}

fn orders_field_nodes(
    parent: &Value,
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> Vec<Value> {
    let Some(value) = orders_parent_field_value(parent, invocation) else {
        return Vec::new();
    };
    orders_nodes_from_value(&value)
}

fn orders_nodes_from_value(value: &Value) -> Vec<Value> {
    if let Some(values) = value.as_array() {
        return values.clone();
    }
    if let Some(edges) = value.get("edges").and_then(Value::as_array) {
        let nodes = edges
            .iter()
            .filter_map(|edge| {
                let mut node = edge.get("node")?.clone();
                if let (Some(object), Some(cursor)) = (
                    node.as_object_mut(),
                    edge.get("cursor").and_then(Value::as_str),
                ) {
                    object.insert("__draftProxyCursor".to_string(), json!(cursor));
                }
                Some(node)
            })
            .collect::<Vec<_>>();
        if !nodes.is_empty() {
            return nodes;
        }
    }
    connection_nodes(value)
}

fn preserve_source_connection_boundary_cursors(
    connection: &mut Value,
    source: &Value,
    source_nodes: &[Value],
) {
    let result_nodes = connection_nodes(connection);
    let result_first_id = result_nodes
        .first()
        .and_then(|node| node.get("id"))
        .and_then(Value::as_str);
    let result_last_id = result_nodes
        .last()
        .and_then(|node| node.get("id"))
        .and_then(Value::as_str);
    let source_first_id = source_nodes
        .first()
        .and_then(|node| node.get("id"))
        .and_then(Value::as_str);
    let source_last_id = source_nodes
        .last()
        .and_then(|node| node.get("id"))
        .and_then(Value::as_str);
    if result_first_id.is_some() && result_first_id == source_first_id {
        if let Some(cursor) = source.pointer("/pageInfo/startCursor").cloned() {
            connection["pageInfo"]["startCursor"] = cursor;
        }
    }
    if result_last_id.is_some() && result_last_id == source_last_id {
        if let Some(cursor) = source.pointer("/pageInfo/endCursor").cloned() {
            connection["pageInfo"]["endCursor"] = cursor;
        }
    }
}

fn orders_field_cursor(value: &Value) -> String {
    value
        .get("__draftProxyCursor")
        .and_then(Value::as_str)
        .or_else(|| value.get("cursor").and_then(Value::as_str))
        .or_else(|| value.get("id").and_then(Value::as_str))
        .or_else(|| {
            value
                .pointer("/fulfillmentLineItem/id")
                .and_then(Value::as_str)
        })
        .or_else(|| value.get("kind").and_then(Value::as_str))
        .or_else(|| value.get("message").and_then(Value::as_str))
        .unwrap_or_default()
        .to_string()
}

fn orders_connection_field(
    proxy: &mut DraftProxy,
    _request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> Result<Value, String> {
    let parent = canonical_orders_field_parent(proxy, invocation);
    let arguments = resolved_arguments_from_json(&invocation.arguments);
    if invocation.parent_type == "Order" && invocation.field_name == "returns" {
        return Ok(proxy.order_returns_connection_value(&parent, &arguments));
    }
    let source = orders_parent_field_value(&parent, invocation);
    let mut nodes = source
        .as_ref()
        .map(orders_nodes_from_value)
        .unwrap_or_default();
    let source_nodes = nodes.clone();
    if invocation.parent_type == "FulfillmentOrder" && invocation.field_name == "merchantRequests" {
        if let Some(kind) = invocation.arguments.get("kind").and_then(Value::as_str) {
            nodes.retain(|request| request.get("kind").and_then(Value::as_str) == Some(kind));
        }
    }
    let mut connection = connection_value_with_args(nodes, &arguments, orders_field_cursor);
    if let Some(source) = source.as_ref() {
        preserve_source_connection_boundary_cursors(&mut connection, source, &source_nodes);
    }
    Ok(connection)
}

fn orders_list_field(
    proxy: &mut DraftProxy,
    _request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> Result<Value, String> {
    // A line item's tax lines are already carried by the canonical parent
    // supplied by the GraphQL engine. Do not rediscover that parent by ID:
    // older staged data can contain repeated synthetic LineItem IDs, and an
    // unrelated order must never replace the value currently being resolved.
    let parent = if invocation.parent_type == "LineItem"
        && invocation.field_name == "taxLines"
        && invocation.parent.get("taxLines").is_some()
    {
        invocation.parent.clone()
    } else {
        canonical_orders_field_parent(proxy, invocation)
    };
    let mut values = orders_field_nodes(&parent, invocation);
    if invocation.field_name == "transactions" {
        for argument in ["capturable", "manuallyResolvable"] {
            if let Some(expected) = invocation.arguments.get(argument).and_then(Value::as_bool) {
                values.retain(|transaction| {
                    transaction
                        .get(argument)
                        .and_then(Value::as_bool)
                        .is_none_or(|actual| actual == expected)
                });
            }
        }
    }
    if let Some(first) = invocation.arguments.get("first").and_then(Value::as_i64) {
        values.truncate(first.max(0) as usize);
    }
    Ok(Value::Array(values))
}

pub(in crate::proxy) struct OrderRootContext<'a> {
    pub request: &'a Request,
    pub query: &'a str,
    pub variables: &'a BTreeMap<String, ResolvedValue>,
    pub root_field: &'a str,
    pub response_key: &'a str,
    pub arguments: &'a BTreeMap<String, ResolvedValue>,
    pub raw_arguments: &'a BTreeMap<String, RawArgumentValue>,
    pub root_location: SourceLocation,
    pub requested_field_paths: &'a BTreeSet<Vec<String>>,
}

impl DraftProxy {
    pub(in crate::proxy) fn orders_query_outcome(
        &mut self,
        context: &OrderRootContext<'_>,
    ) -> ResolverOutcome<Value> {
        if context.root_field == "order"
            && self.should_handle_shipping_fulfillment_order_local_order_read(
                context.arguments,
                context.requested_field_paths,
            )
        {
            return self.shipping_fulfillment_order_local_order_outcome(context.arguments);
        }
        if let Some(outcome) = self.order_create_local_outcome(
            context.request,
            context.root_field,
            context.arguments,
            context.query,
            context.variables,
        ) {
            return outcome;
        }
        if let Some(outcome) = self.draft_order_lifecycle_local_outcome(context) {
            return outcome;
        }
        if let Some(outcome) = self.draft_order_complete_local_outcome(
            context.request,
            context.root_field,
            context.arguments,
            context.raw_arguments,
        ) {
            return outcome;
        }
        if let Some(outcome) =
            self.draft_order_bulk_tag_local_outcome(context.root_field, context.arguments)
        {
            return outcome;
        }
        if let Some(outcome) = self.remaining_order_local_outcome(context) {
            return outcome;
        }
        if self.config.read_mode != ReadMode::Snapshot {
            let result = self
                .cached_or_forward_upstream_graphql_result(context.request, context.response_key);
            if self.config.read_mode == ReadMode::LiveHybrid && result.transport_succeeded {
                self.observe_order_read_data(context.request, &result.data);
                self.observe_draft_order_read_data(context.request, &result.data);
            }
            return result.outcome;
        }

        ResolverOutcome::value(match context.root_field {
            "order" | "draftOrder" | "return" | "abandonment" => Value::Null,
            "orders" => connection_json(Vec::new()),
            "ordersCount" => count_object(0),
            _ => Value::Null,
        })
    }

    pub(in crate::proxy) fn orders_stage_locally_unmodeled_shape_outcome(
        &mut self,
        root_field: &str,
    ) -> ResolverOutcome<Value> {
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

        ResolverOutcome::value(payload).with_log_draft(
            LogDraft::failed(
                root_field,
                "orders",
                "Orders mutation root is registered for local staging, but this argument/selection shape is not modeled yet.",
            ),
        )
    }
}

impl DraftProxy {
    pub(crate) fn orders_query_root(
        &mut self,
        invocation: RootInvocation<'_>,
    ) -> ResolverOutcome<Value> {
        let shared_catalog_read = self.config.read_mode == ReadMode::LiveHybrid
            && invocation.operation_root_names.len() > 1
            && invocation.operation_root_names.iter().all(|root| {
                matches!(
                    root.as_str(),
                    "order"
                        | "orders"
                        | "ordersCount"
                        | "draftOrder"
                        | "draftOrders"
                        | "draftOrdersCount"
                )
            });
        if shared_catalog_read {
            let arguments = resolved_arguments_from_json(&invocation.arguments);
            let has_local_overlay = match invocation.root_name {
                "order" => resolved_string_field(&arguments, "id").is_some_and(|id| {
                    self.store.staged.orders.contains_key(&id)
                        || self.store.staged.orders.is_tombstoned(&id)
                }),
                "orders" | "ordersCount" => !self.store.staged.orders.is_empty(),
                "draftOrder" => resolved_string_field(&arguments, "id").is_some_and(|id| {
                    self.store.staged.draft_orders.contains_key(&id)
                        || self.store.staged.draft_orders.is_tombstoned(&id)
                }),
                "draftOrders" | "draftOrdersCount" => !self.store.staged.draft_orders.is_empty(),
                _ => false,
            };
            let result = self.cached_or_forward_upstream_graphql_result(
                invocation.request,
                invocation.response_key,
            );
            if !result.transport_succeeded {
                return result.outcome;
            }
            self.observe_order_read_data(invocation.request, &result.data);
            self.observe_draft_order_read_data(invocation.request, &result.data);
            if !has_local_overlay {
                return result.outcome;
            }
        }
        let RootInvocation {
            response_key,
            request,
            query,
            variables,
            root_name: root_field,
            root_location,
            raw_arguments,
            arguments,
            requested_field_paths,
            ..
        } = invocation;
        let arguments = resolved_arguments_from_json(&arguments);
        let requests_payment_terms = requested_field_paths
            .iter()
            .any(|path| path.iter().any(|field| field == "paymentTerms"));
        if let Some(outcome) = self.payment_terms_local_outcome(
            request,
            root_field,
            &arguments,
            requests_payment_terms,
        ) {
            return outcome;
        }
        if let Some(outcome) = self.order_return_local_runtime_outcome(
            request,
            root_field,
            &arguments,
            &requested_field_paths,
        ) {
            return outcome;
        }
        self.orders_query_outcome(&OrderRootContext {
            request,
            query,
            variables,
            root_field,
            response_key,
            arguments: &arguments,
            raw_arguments: &raw_arguments,
            root_location,
            requested_field_paths: &requested_field_paths,
        })
    }

    pub(crate) fn orders_mutation_root(
        &mut self,
        invocation: RootInvocation<'_>,
    ) -> ResolverOutcome<Value> {
        let RootInvocation {
            response_key,
            request,
            query,
            variables,
            root_name: root_field,
            root_location,
            raw_arguments,
            arguments,
            requested_field_paths,
            ..
        } = invocation;
        let arguments = resolved_arguments_from_json(&arguments);
        let context = OrderRootContext {
            request,
            query,
            variables,
            root_field,
            response_key,
            arguments: &arguments,
            raw_arguments: &raw_arguments,
            root_location,
            requested_field_paths: &requested_field_paths,
        };
        match root_field {
            "orderCancel" => {
                if let Some(outcome) = self.order_customer_error_paths_outcome(
                    request, root_field, &arguments, query, variables,
                ) {
                    outcome
                } else {
                    resolver_http_error_outcome(
                        501,
                        format!("No Rust orders resolver implemented for {root_field}"),
                    )
                }
            }
            "orderDelete" => {
                if let Some(outcome) = self.remaining_order_local_outcome(&context) {
                    outcome
                } else {
                    resolver_http_error_outcome(
                        501,
                        format!("No Rust orders resolver implemented for {root_field}"),
                    )
                }
            }
            "orderMarkAsPaid"
            | "orderCreateManualPayment"
            | "refundCreate"
            | "orderEditBegin"
            | "orderEditCommit" => {
                if let Some(outcome) = self.money_bag_presentment_local_outcome(
                    request,
                    root_field,
                    &arguments,
                    &requested_field_paths,
                ) {
                    outcome
                } else if let Some(outcome) = self
                    .refund_create_local_outcome(request, root_field, &arguments, query, variables)
                {
                    outcome
                } else if let Some(outcome) = self.order_payment_transaction_local_outcome(&context)
                {
                    outcome
                } else if let Some(outcome) = self.remaining_order_local_outcome(&context) {
                    outcome
                } else {
                    self.orders_stage_locally_unmodeled_shape_outcome(root_field)
                }
            }
            "orderCreate" => {
                if let Some(outcome) = self.payment_terms_local_outcome(
                    request,
                    root_field,
                    &arguments,
                    requested_field_paths
                        .iter()
                        .any(|path| path.iter().any(|field| field == "paymentTerms")),
                ) {
                    outcome
                } else if let Some(outcome) = self.money_bag_presentment_local_outcome(
                    request,
                    root_field,
                    &arguments,
                    &requested_field_paths,
                ) {
                    outcome
                } else if let Some(outcome) = self.order_payment_transaction_local_outcome(&context)
                {
                    outcome
                } else if let Some(outcome) = self.draft_order_complete_local_outcome(
                    request,
                    root_field,
                    &arguments,
                    &raw_arguments,
                ) {
                    outcome
                } else if let Some(outcome) = self.remaining_order_local_outcome(&context) {
                    outcome
                } else if let Some(outcome) = self
                    .order_create_local_outcome(request, root_field, &arguments, query, variables)
                {
                    outcome
                } else {
                    self.customer_order_create(&arguments)
                }
            }
            "orderUpdate" => {
                if let Some(outcome) = self
                    .order_create_local_outcome(request, root_field, &arguments, query, variables)
                {
                    outcome
                } else {
                    self.orders_stage_locally_unmodeled_shape_outcome(root_field)
                }
            }
            "orderClose" | "orderOpen" => {
                if let Some(outcome) = self
                    .order_create_local_outcome(request, root_field, &arguments, query, variables)
                {
                    outcome
                } else {
                    resolver_http_error_outcome(
                        501,
                        format!("No Rust orders resolver implemented for {root_field}"),
                    )
                }
            }
            "draftOrderCreate"
            | "draftOrderInvoiceSend"
            | "draftOrderUpdate"
            | "draftOrderCalculate"
            | "draftOrderDuplicate"
            | "draftOrderDelete"
            | "draftOrderBulkDelete"
            | "draftOrderCreateFromOrder"
            | "draftOrderInvoicePreview" => {
                if let Some(outcome) = self.draft_order_invoice_send_local_outcome(
                    request, root_field, &arguments, query, variables,
                ) {
                    outcome
                } else if let Some(outcome) = self.draft_order_complete_local_outcome(
                    request,
                    root_field,
                    &arguments,
                    &raw_arguments,
                ) {
                    outcome
                } else if let Some(outcome) = self.draft_order_lifecycle_local_outcome(&context) {
                    outcome
                } else if let Some(outcome) =
                    self.draft_order_bulk_tag_local_outcome(root_field, &arguments)
                {
                    outcome
                } else {
                    resolver_http_error_outcome(
                        501,
                        format!("No Rust orders resolver implemented for {root_field}"),
                    )
                }
            }
            "draftOrderComplete" => {
                if let Some(outcome) = self.draft_order_complete_local_outcome(
                    request,
                    root_field,
                    &arguments,
                    &raw_arguments,
                ) {
                    outcome
                } else {
                    resolver_http_error_outcome(
                        501,
                        format!("No Rust orders resolver implemented for {root_field}"),
                    )
                }
            }
            "draftOrderBulkAddTags" | "draftOrderBulkRemoveTags" => {
                let before_tags = self.store.staged.draft_order_tags.clone();
                if let Some(mut outcome) =
                    self.draft_order_bulk_tag_local_outcome(root_field, &arguments)
                {
                    let staged_ids = changed_draft_order_tag_ids(
                        &before_tags,
                        &self.store.staged.draft_order_tags,
                    );
                    if !staged_ids.is_empty() {
                        outcome = outcome
                            .with_log_draft(LogDraft::staged(root_field, "orders", staged_ids));
                    }
                    outcome
                } else {
                    resolver_http_error_outcome(
                        501,
                        format!("No Rust orders resolver implemented for {root_field}"),
                    )
                }
            }
            "fulfillmentCreate"
            | "fulfillmentCreateV2"
            | "fulfillmentCancel"
            | "fulfillmentTrackingInfoUpdate"
            | "fulfillmentTrackingInfoUpdateV2"
            | "fulfillmentEventCreate"
            | "orderEditAddVariant"
            | "orderEditSetQuantity"
            | "orderEditAddCustomItem"
            | "orderEditAddLineItemDiscount"
            | "orderEditRemoveDiscount"
            | "orderEditAddShippingLine"
            | "orderEditUpdateShippingLine"
            | "orderEditRemoveShippingLine" => {
                if let Some(outcome) = self.remaining_order_local_outcome(&context) {
                    outcome
                } else {
                    resolver_http_error_outcome(
                        501,
                        format!("No Rust orders resolver implemented for {root_field}"),
                    )
                }
            }
            "returnCreate"
            | "returnRequest"
            | "returnApproveRequest"
            | "returnDeclineRequest"
            | "returnCancel"
            | "returnClose"
            | "returnReopen"
            | "removeFromReturn"
            | "returnProcess" => {
                if let Some(outcome) = self.order_return_local_runtime_outcome(
                    request,
                    root_field,
                    &arguments,
                    &requested_field_paths,
                ) {
                    outcome
                } else {
                    resolver_http_error_outcome(
                        501,
                        format!("No Rust orders resolver implemented for {root_field}"),
                    )
                }
            }
            "orderCustomerSet" | "orderCustomerRemove" => {
                if let Some(outcome) = self.order_customer_error_paths_outcome(
                    request, root_field, &arguments, query, variables,
                ) {
                    outcome
                } else {
                    resolver_http_error_outcome(400, "Could not parse GraphQL operation")
                }
            }
            "orderInvoiceSend" => {
                if let Some(outcome) = self.order_invoice_send_local_outcome(
                    request,
                    &arguments,
                    &requested_field_paths,
                    query,
                    variables,
                ) {
                    outcome
                } else {
                    resolver_http_error_outcome(
                        501,
                        format!("No Rust orders resolver implemented for {root_field}"),
                    )
                }
            }
            _ => resolver_http_error_outcome(
                501,
                format!("No Rust stage-locally resolver implemented for root field: {root_field}"),
            ),
        }
    }
}

fn changed_draft_order_tag_ids(
    before: &BTreeMap<String, Vec<String>>,
    after: &BTreeMap<String, Vec<String>>,
) -> Vec<String> {
    after
        .iter()
        .filter(|(id, tags)| before.get(*id) != Some(*tags))
        .map(|(id, _)| id.clone())
        .collect()
}

struct OrdersLocalLogEntry<'a> {
    request: &'a Request,
    query: &'a str,
    variables: &'a BTreeMap<String, ResolvedValue>,
    root_field: &'a str,
    staged_resource_ids: Vec<String>,
    outcome: OrdersLocalLogOutcome<'a>,
}

const ORDER_LIFECYCLE_HYDRATE_QUERY: &str = "query OrderManagementDownstreamRead($id: ID!) {\n  order(id: $id) {\n    id\n    name\n    closed\n    closedAt\n    cancelledAt\n    cancelReason\n    displayFinancialStatus\n    paymentGatewayNames\n    totalOutstandingSet {\n      shopMoney {\n        amount\n        currencyCode\n      }\n    }\n    currentTotalPriceSet {\n      shopMoney {\n        amount\n        currencyCode\n      }\n    }\n    customer {\n      id\n      email\n      displayName\n    }\n    transactions {\n      kind\n      status\n      gateway\n      amountSet {\n        shopMoney {\n          amount\n          currencyCode\n        }\n      }\n    }\n  }\n}";
const ORDER_INVOICE_SEND_EMAIL_HYDRATE_QUERY: &str = "query OrderInvoiceSendEmailValidationRead($id: ID!) {\n    order(id: $id) {\n      \n  id\n  name\n  email\n  customer {\n    id\n    email\n    displayName\n  }\n\n    }\n  }";

// Canonical customer hydrate issued for order-customer mutations (orderCustomerSet).
// The selection mirrors the order.customer projection these mutations expose, so a
// live backend returns the same shape the proxy then stores and re-projects.
const ORDER_CUSTOMER_SUMMARY_HYDRATE_QUERY: &str =
    "query CustomerHydrate($id: ID!) { customer(id: $id) { id email displayName } }";

const FULFILLMENT_EVENT_CREATED_AT: &str = "2024-01-01T00:00:03.000Z";
const FULFILLMENT_EVENT_STATUS_VALUES: &[&str] = &[
    "LABEL_PURCHASED",
    "LABEL_PRINTED",
    "READY_FOR_PICKUP",
    "CONFIRMED",
    "IN_TRANSIT",
    "OUT_FOR_DELIVERY",
    "ATTEMPTED_DELIVERY",
    "DELAYED",
    "DELIVERED",
    "FAILURE",
    "CARRIER_PICKED_UP",
];

// Draft-order hydration forwarded on a cold miss for draftOrder reads and
// update/delete/duplicate/complete/invoice-send mutations operating on a draft
// not created locally this scenario, then observed into staged state instead of
// a precondition seed. Shares the `.graphql` file with the capture scripts (via
// include_str!) so the recorded cassette byte-matches the proxy's forward under
// the strict cassette matcher. The file preserves the original constant's bytes
// (leading newline + indentation) so previously recorded cassettes still match.
const DRAFT_ORDER_HYDRATE_QUERY: &str =
    include_str!("../../config/parity-requests/orders/draft-order-hydrate.graphql");
// Order hydration for `orderEditBegin` operating on an order that was not
// created locally in this scenario. Forwarded verbatim on a cold miss and
// observed into staged state so the edit session is built from real line items,
// currency, and editability flags instead of a precondition seed. Shares the
// `.graphql` file with the capture scripts (via include_str!) so the recorded
// cassette byte-matches the proxy's forward under the strict cassette matcher.
const ORDER_EDIT_HYDRATE_QUERY: &str =
    include_str!("../../config/parity-requests/orders/order-edit-hydrate.graphql");
// Order hydration for `returnCreate` / `returnRequest` operating on an order that
// was not created locally in this scenario. Forwarded verbatim on a cold miss and
// observed into staged state so the return engine validates requested lines
// against the order's real fulfillment line items and any outstanding returns,
// instead of a precondition seed. Shares the `.graphql` file with the capture
// scripts (via include_str!) so the recorded cassette byte-matches the proxy's
// forward under the strict cassette matcher.
const RETURN_ORDER_HYDRATE_QUERY: &str =
    include_str!("../../config/parity-requests/orders/return-order-hydrate.graphql");
const RETURN_FULFILLMENT_LINE_ITEMS_HYDRATE_QUERY: &str = include_str!(
    "../../config/parity-requests/orders/return-fulfillment-line-items-hydrate.graphql"
);
const ORDER_HYDRATE_QUERY: &str =
    include_str!("../../config/parity-requests/orders/order-hydrate-pageable.graphql");
// These hydrate queries are forwarded verbatim to the backend; their exact text
// must match the recorded `OrdersDraftOrder*Hydrate` cassette calls (compact
// two-space layout, customer carries firstName/lastName) so the strict cassette
// matcher replays the recorded customer/variant responses instead of returning a
// mismatch.
const DRAFT_ORDER_CUSTOMER_HYDRATE_QUERY: &str =
    "query OrdersDraftOrderCustomerHydrate($id: ID!) {\n  customer(id: $id) { id email displayName firstName lastName }\n}\n";
const DRAFT_ORDER_VARIANT_HYDRATE_QUERY: &str =
    "query OrdersDraftOrderVariantHydrate($id: ID!) {\n  productVariant(id: $id) { id title sku taxable price inventoryItem { requiresShipping } product { title } }\n}\n";
const DRAFT_ORDER_VARIANTS_HYDRATE_QUERY: &str =
    "query OrdersDraftOrderVariantsHydrate($ids: [ID!]!) {\n  nodes(ids: $ids) { __typename ... on ProductVariant { id title sku taxable price inventoryItem { requiresShipping } product { title } } }\n}\n";
const ORDERS_FULFILLMENT_ORDER_HYDRATE_QUERY: &str = "query ShippingFulfillmentOrderHydrate($id: ID!) {\n    fulfillmentOrder(id: $id) {\n      id\n      status\n      requestStatus\n      fulfillAt\n      fulfillBy\n      updatedAt\n      supportedActions {\n        action\n      }\n      assignedLocation {\n        name\n        location {\n          id\n          name\n        }\n      }\n      fulfillmentHolds {\n        id\n        handle\n        reason\n        reasonNotes\n        displayReason\n        heldByApp {\n          id\n          title\n        }\n        heldByRequestingApp\n      }\n      merchantRequests(first: 10) {\n        nodes {\n          kind\n          message\n          requestOptions\n        }\n      }\n      lineItems(first: 20) {\n        nodes {\n          id\n          totalQuantity\n          remainingQuantity\n          lineItem {\n            id\n            title\n            quantity\n            fulfillableQuantity\n          }\n        }\n      }\n      order {\n        id\n        name\n        displayFulfillmentStatus\n      }\n    }\n  }";
// Order hydration for `orderMarkAsPaid` operating on an order that was not
// created locally in this scenario. The proxy forwards this exact query (it is
// byte-identical to the `OrdersOrderHydrate` recording so the strict cassette
// matcher accepts it) to fetch the order's money-bag/transaction state from the
// backend, observes it into staged state, then applies the mutation locally.
const ORDER_MARK_AS_PAID_HYDRATE_QUERY: &str =
    "#graphql\n  fragment OrderMarkAsPaidMoneyBagFields on Order {\n    id\n    name\n    createdAt\n    updatedAt\n    closed\n    closedAt\n    cancelledAt\n    cancelReason\n    presentmentCurrencyCode\n    displayFinancialStatus\n    displayFulfillmentStatus\n    paymentGatewayNames\n    totalOutstandingSet {\n      shopMoney { amount currencyCode }\n      presentmentMoney { amount currencyCode }\n    }\n    currentTotalPriceSet {\n      shopMoney { amount currencyCode }\n      presentmentMoney { amount currencyCode }\n    }\n    totalPriceSet {\n      shopMoney { amount currencyCode }\n      presentmentMoney { amount currencyCode }\n    }\n    transactions {\n      id\n      kind\n      status\n      gateway\n      amountSet {\n        shopMoney { amount currencyCode }\n        presentmentMoney { amount currencyCode }\n      }\n    }\n  }\n\n  query OrdersOrderHydrate($id: ID!) {\n    order(id: $id) {\n      ...OrderMarkAsPaidMoneyBagFields\n    }\n  }";
const ORDERS_FULFILLMENT_HYDRATE_QUERY: &str = r#"#graphql
  query ShippingFulfillmentEventCreateFulfillmentHydrate($id: ID!) {
    fulfillment(id: $id) {
      id
      status
      displayStatus
      createdAt
      updatedAt
      deliveredAt
      estimatedDeliveryAt
      inTransitAt
      trackingInfo(first: 1) { number url company }
      events(first: 5) {
        nodes {
          id
          status
          message
          happenedAt
          createdAt
          estimatedDeliveryAt
          city
          province
          country
          zip
          address1
          latitude
          longitude
        }
        pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
      }
      service {
        id
        handle
        serviceName
        trackingSupport
        type
        location { id name }
      }
      location { id name }
      originAddress { address1 address2 city countryCode provinceCode zip }
      fulfillmentLineItems(first: 5) {
        nodes { id quantity lineItem { id title } }
        pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
      }
      order { id name displayFulfillmentStatus }
    }
  }
"#;
// Fulfillment-lifecycle hydration for `fulfillmentCancel` / `fulfillmentTrackingInfoUpdate`
// operating on a fulfillment that was not created locally in this scenario. Byte-identical
// to the recorded `OrdersFulfillmentHydrate` query so the strict cassette matcher accepts
// it; resolves the fulfillment's owning order plus the sibling fulfillment states (status /
// displayStatus / trackingInfo) the proxy needs to evaluate the state-machine preconditions
// (already-cancelled, already-delivered) locally.
const ORDERS_FULFILLMENT_LIFECYCLE_HYDRATE_QUERY: &str = "query OrdersFulfillmentHydrate($id: ID!) { fulfillment(id: $id) { id order { id name email phone createdAt updatedAt closed closedAt cancelledAt cancelReason displayFinancialStatus displayFulfillmentStatus note tags fulfillments { id status displayStatus createdAt updatedAt trackingInfo { number url company } } } } }";
// Best-effort second-stage enrichment for the lifecycle hydrate. Byte-identical to the
// recorded `OrderFulfillmentLifecycleRead` query so the strict cassette matcher accepts it;
// fetches the order's full fulfillment view *including* `fulfillmentLineItems` so a downstream
// order read observes line items the bare `OrdersFulfillmentHydrate` projection omits. When the
// backend has no such recording the cassette miss is non-fatal and the proxy falls back to the
// stage-one order.
const ORDER_FULFILLMENT_LIFECYCLE_READ_QUERY: &str = "query OrderFulfillmentLifecycleRead($id: ID!) {\n  order(id: $id) {\n    id\n    name\n    updatedAt\n    displayFulfillmentStatus\n    fulfillments(first: 5) {\n      id\n      status\n      displayStatus\n      createdAt\n      updatedAt\n      trackingInfo {\n        number\n        url\n        company\n      }\n      fulfillmentLineItems(first: 5) {\n        nodes {\n          id\n          quantity\n          lineItem {\n            id\n            title\n          }\n        }\n      }\n    }\n    fulfillmentOrders(first: 5) {\n      nodes {\n        id\n        status\n        requestStatus\n        lineItems(first: 5) {\n          nodes {\n            id\n            totalQuantity\n            remainingQuantity\n            lineItem {\n              id\n              title\n            }\n          }\n        }\n      }\n    }\n  }\n}";

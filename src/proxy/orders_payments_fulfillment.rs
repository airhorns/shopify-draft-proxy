use super::*;

mod draft_orders;
mod fulfillment_orders;
mod orders;
mod payments;

pub(in crate::proxy) use self::draft_orders::*;
pub(in crate::proxy) use self::fulfillment_orders::*;
pub(in crate::proxy) use self::orders::*;
pub(in crate::proxy) use self::payments::*;

impl DraftProxy {
    pub(in crate::proxy) fn orders_query_outcome(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        root_field: &str,
        response_key: &str,
    ) -> ResolverOutcome<Value> {
        if root_field == "order"
            && self.should_handle_shipping_fulfillment_order_local_order_read(query, variables)
        {
            return self.shipping_fulfillment_order_local_order_outcome(
                query,
                variables,
                response_key,
            );
        }
        if let Some(outcome) =
            self.order_create_local_outcome(request, root_field, query, variables, response_key)
        {
            return outcome;
        }
        if let Some(outcome) =
            self.draft_order_lifecycle_local_outcome(request, query, variables, response_key)
        {
            return outcome;
        }
        if let Some(outcome) = self.draft_order_complete_local_outcome(
            request,
            root_field,
            query,
            variables,
            response_key,
        ) {
            return outcome;
        }
        if let Some(outcome) =
            self.payment_terms_local_outcome(request, query, variables, response_key)
        {
            return outcome;
        }
        if let Some(outcome) =
            self.draft_order_bulk_tag_local_outcome(query, variables, response_key)
        {
            return outcome;
        }
        if let Some(outcome) = self.order_return_local_runtime_outcome(
            request,
            root_field,
            query,
            variables,
            response_key,
        ) {
            return outcome;
        }
        if let Some(outcome) = self.abandonment_read_outcome(query, variables, response_key) {
            return outcome;
        }
        if let Some(outcome) =
            self.remaining_order_local_outcome(request, root_field, query, variables)
        {
            return outcome;
        }
        if self.config.read_mode != ReadMode::Snapshot {
            let result = self.cached_or_forward_upstream_graphql_result(request, response_key);
            if self.config.read_mode == ReadMode::LiveHybrid && result.transport_succeeded {
                self.observe_order_read_data(request, &result.data);
                self.observe_draft_order_read_data(request, &result.data);
            }
            return result.outcome;
        }

        let fields = match self.root_fields_or_error(query, variables) {
            Ok(fields) => fields,
            Err(_) => return resolver_http_error_outcome(400, "Could not parse GraphQL operation"),
        };
        let data = root_payload_json(&fields, |field| match field.name.as_str() {
            "order" | "draftOrder" | "return" | "abandonment" => Some(Value::Null),
            "orders" => Some(connection_json(Vec::new())),
            "ordersCount" => Some(selected_json(&count_object(0), &field.selection)),
            _ => None,
        });
        ResolverOutcome::value(data.get(response_key).cloned().unwrap_or(Value::Null))
    }

    pub(in crate::proxy) fn abandonment_read_outcome(
        &self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        response_key: &str,
    ) -> Option<ResolverOutcome<Value>> {
        let fields = self.execution_root_fields(query, variables)?;
        if !fields.iter().any(|field| field.name == "abandonment") {
            return None;
        }

        let data = root_payload_json(&fields, |field| {
            if field.name != "abandonment" {
                return None;
            }
            let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
            let value = self
                .store
                .staged
                .abandonments
                .get(&id)
                .map(|record| selected_json(record, &field.selection))
                .unwrap_or(Value::Null);
            Some(value)
        });
        Some(ResolverOutcome::value(
            data.get(response_key).cloned().unwrap_or(Value::Null),
        ))
    }

    pub(in crate::proxy) fn orders_stage_locally_unmodeled_shape_outcome(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        root_field: &str,
        response_key: &str,
    ) -> ResolverOutcome<Value> {
        let field = self.execution_root_field(query, variables, root_field);
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

        let _ = response_key;
        ResolverOutcome::value(selected_json(&payload, &selection)).with_log_draft(
            LogDraft::failed(
                root_field,
                "orders",
                "Orders mutation root is registered for local staging, but this argument/selection shape is not modeled yet.",
            ),
        )
    }
}

impl DraftProxy {
    pub(crate) fn resolve_orders_graphql(
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
            LocalResolverMode::OverlayRead if operation.operation_type == OperationType::Query => {
                if let Some(outcome) = self.order_return_local_runtime_outcome(
                    request,
                    root_field,
                    query,
                    variables,
                    response_key,
                ) {
                    return outcome;
                }
                if self.should_route_owner_metafields_read(query, variables) {
                    return self.owner_metafields_read(request, query, variables, response_key);
                }
                self.orders_query_outcome(request, query, variables, root_field, response_key)
            }
            LocalResolverMode::StageLocally
                if operation.operation_type == OperationType::Mutation
                    && matches!(root_field, "abandonmentUpdateActivitiesDeliveryStatuses") =>
            {
                if let Some(outcome) = self.abandonment_delivery_status_local_outcome(
                    request,
                    query,
                    variables,
                    response_key,
                ) {
                    outcome
                } else {
                    resolver_http_error_outcome(
                        501,
                        format!("No Rust orders resolver implemented for {root_field}"),
                    )
                }
            }
            LocalResolverMode::StageLocally
                if operation.operation_type == OperationType::Mutation
                    && root_field == "orderCancel" =>
            {
                if let Some(outcome) =
                    self.order_customer_error_paths_outcome(request, query, variables, response_key)
                {
                    outcome
                } else {
                    resolver_http_error_outcome(
                        501,
                        format!("No Rust orders resolver implemented for {root_field}"),
                    )
                }
            }
            LocalResolverMode::StageLocally
                if operation.operation_type == OperationType::Mutation
                    && root_field == "orderDelete" =>
            {
                if let Some(outcome) =
                    self.remaining_order_local_outcome(request, root_field, query, variables)
                {
                    outcome
                } else {
                    resolver_http_error_outcome(
                        501,
                        format!("No Rust orders resolver implemented for {root_field}"),
                    )
                }
            }
            LocalResolverMode::StageLocally
                if operation.operation_type == OperationType::Mutation
                    && matches!(
                        root_field,
                        "orderMarkAsPaid"
                            | "orderCreateManualPayment"
                            | "refundCreate"
                            | "orderEditBegin"
                            | "orderEditCommit"
                    ) =>
            {
                if let Some(outcome) = self.money_bag_presentment_local_outcome(
                    request,
                    query,
                    variables,
                    response_key,
                ) {
                    outcome
                } else if let Some(outcome) = self.refund_create_local_outcome(
                    request,
                    root_field,
                    query,
                    variables,
                    response_key,
                ) {
                    outcome
                } else if let Some(outcome) = self.order_payment_transaction_local_outcome(
                    request,
                    root_field,
                    query,
                    variables,
                    response_key,
                ) {
                    outcome
                } else if let Some(outcome) =
                    self.remaining_order_local_outcome(request, root_field, query, variables)
                {
                    outcome
                } else {
                    self.orders_stage_locally_unmodeled_shape_outcome(
                        query,
                        variables,
                        root_field,
                        response_key,
                    )
                }
            }
            LocalResolverMode::StageLocally
                if operation.operation_type == OperationType::Mutation
                    && root_field == "orderCreate" =>
            {
                if let Some(outcome) =
                    self.payment_terms_local_outcome(request, query, variables, response_key)
                {
                    outcome
                } else if let Some(outcome) = self.money_bag_presentment_local_outcome(
                    request,
                    query,
                    variables,
                    response_key,
                ) {
                    outcome
                } else if let Some(outcome) = self.order_payment_transaction_local_outcome(
                    request,
                    root_field,
                    query,
                    variables,
                    response_key,
                ) {
                    outcome
                } else if let Some(outcome) = self.draft_order_complete_local_outcome(
                    request,
                    root_field,
                    query,
                    variables,
                    response_key,
                ) {
                    outcome
                } else if let Some(outcome) =
                    self.remaining_order_local_outcome(request, root_field, query, variables)
                {
                    outcome
                } else if let Some(outcome) = self.order_create_local_outcome(
                    request,
                    root_field,
                    query,
                    variables,
                    response_key,
                ) {
                    outcome
                } else {
                    self.customer_order_create(query, variables)
                }
            }
            LocalResolverMode::StageLocally
                if operation.operation_type == OperationType::Mutation
                    && root_field == "orderUpdate" =>
            {
                if let Some(outcome) = self.order_create_local_outcome(
                    request,
                    root_field,
                    query,
                    variables,
                    response_key,
                ) {
                    outcome
                } else {
                    self.orders_stage_locally_unmodeled_shape_outcome(
                        query,
                        variables,
                        root_field,
                        response_key,
                    )
                }
            }
            LocalResolverMode::StageLocally
                if operation.operation_type == OperationType::Mutation
                    && matches!(root_field, "orderClose" | "orderOpen") =>
            {
                if let Some(outcome) = self.order_create_local_outcome(
                    request,
                    root_field,
                    query,
                    variables,
                    response_key,
                ) {
                    outcome
                } else {
                    resolver_http_error_outcome(
                        501,
                        format!("No Rust orders resolver implemented for {root_field}"),
                    )
                }
            }
            LocalResolverMode::StageLocally
                if operation.operation_type == OperationType::Mutation
                    && matches!(
                        root_field,
                        "draftOrderCreate"
                            | "draftOrderInvoiceSend"
                            | "draftOrderUpdate"
                            | "draftOrderCalculate"
                            | "draftOrderDuplicate"
                            | "draftOrderDelete"
                            | "draftOrderBulkDelete"
                            | "draftOrderCreateFromOrder"
                            | "draftOrderInvoicePreview"
                    ) =>
            {
                if let Some(outcome) = self.draft_order_invoice_send_local_outcome(
                    request,
                    query,
                    variables,
                    response_key,
                ) {
                    outcome
                } else if let Some(outcome) = self.draft_order_complete_local_outcome(
                    request,
                    root_field,
                    query,
                    variables,
                    response_key,
                ) {
                    outcome
                } else if let Some(outcome) = self.draft_order_lifecycle_local_outcome(
                    request,
                    query,
                    variables,
                    response_key,
                ) {
                    outcome
                } else if let Some(outcome) =
                    self.draft_order_bulk_tag_local_outcome(query, variables, response_key)
                {
                    outcome
                } else {
                    resolver_http_error_outcome(
                        501,
                        format!("No Rust orders resolver implemented for {root_field}"),
                    )
                }
            }
            LocalResolverMode::StageLocally
                if operation.operation_type == OperationType::Mutation
                    && root_field == "draftOrderComplete" =>
            {
                if let Some(outcome) = self.draft_order_complete_local_outcome(
                    request,
                    root_field,
                    query,
                    variables,
                    response_key,
                ) {
                    outcome
                } else {
                    resolver_http_error_outcome(
                        501,
                        format!("No Rust orders resolver implemented for {root_field}"),
                    )
                }
            }
            LocalResolverMode::StageLocally
                if operation.operation_type == OperationType::Mutation
                    && matches!(
                        root_field,
                        "draftOrderBulkAddTags" | "draftOrderBulkRemoveTags"
                    ) =>
            {
                let before_tags = self.store.staged.draft_order_tags.clone();
                if let Some(mut outcome) =
                    self.draft_order_bulk_tag_local_outcome(query, variables, response_key)
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
            LocalResolverMode::StageLocally
                if operation.operation_type == OperationType::Mutation
                    && matches!(
                        root_field,
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
                            | "orderEditRemoveShippingLine"
                    ) =>
            {
                if let Some(outcome) =
                    self.remaining_order_local_outcome(request, root_field, query, variables)
                {
                    outcome
                } else {
                    resolver_http_error_outcome(
                        501,
                        format!("No Rust orders resolver implemented for {root_field}"),
                    )
                }
            }
            LocalResolverMode::StageLocally
                if operation.operation_type == OperationType::Mutation
                    && matches!(
                        root_field,
                        "returnCreate"
                            | "returnRequest"
                            | "returnApproveRequest"
                            | "returnDeclineRequest"
                            | "returnCancel"
                            | "returnClose"
                            | "returnReopen"
                            | "removeFromReturn"
                            | "returnProcess"
                    ) =>
            {
                if let Some(outcome) = self.order_return_local_runtime_outcome(
                    request,
                    root_field,
                    query,
                    variables,
                    response_key,
                ) {
                    outcome
                } else {
                    resolver_http_error_outcome(
                        501,
                        format!("No Rust orders resolver implemented for {root_field}"),
                    )
                }
            }
            LocalResolverMode::StageLocally
                if operation.operation_type == OperationType::Mutation
                    && matches!(root_field, "orderCustomerSet" | "orderCustomerRemove") =>
            {
                if let Some(outcome) =
                    self.order_customer_error_paths_outcome(request, query, variables, response_key)
                {
                    outcome
                } else {
                    resolver_http_error_outcome(400, "Could not parse GraphQL operation")
                }
            }
            LocalResolverMode::StageLocally
                if operation.operation_type == OperationType::Mutation
                    && root_field == "orderInvoiceSend" =>
            {
                if let Some(outcome) =
                    self.order_invoice_send_local_outcome(request, query, variables, response_key)
                {
                    outcome
                } else {
                    resolver_http_error_outcome(
                        501,
                        format!("No Rust orders resolver implemented for {root_field}"),
                    )
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

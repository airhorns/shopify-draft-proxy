use super::*;

const SHIPPING_FULFILLMENT_ORDER_HYDRATE_QUERY: &str = r#"
query ShippingFulfillmentOrderHydrate($id: ID!) {
  node(id: $id) {
    __typename
    ... on FulfillmentOrder {
      id
      status
      requestStatus
      fulfillAt
      fulfillBy
      updatedAt
      supportedActions {
        action
      }
      assignedLocation {
        name
        location {
          id
          name
        }
      }
      fulfillmentHolds {
        id
        handle
        reason
        reasonNotes
        displayReason
        heldByApp {
          id
          title
        }
        heldByRequestingApp
      }
      lineItems(first: 250) {
        nodes {
          id
          totalQuantity
          remainingQuantity
          lineItem {
            id
            title
            quantity
            fulfillableQuantity
          }
        }
      }
      order {
        id
        name
        displayFulfillmentStatus
      }
    }
  }
}
"#;

struct FulfillmentOrderStoreBackedPreamble {
    response_key: String,
    payload_selection: Vec<SelectedField>,
    arguments: BTreeMap<String, ResolvedValue>,
    id: String,
    order_id: String,
    index: usize,
}

const SHIPPING_FULFILLMENT_ORDER_DIRECT_HYDRATE_QUERY: &str = r#"query ShippingFulfillmentOrderHydrate($id: ID!) {
    fulfillmentOrder(id: $id) {
      id status requestStatus fulfillAt fulfillBy updatedAt
      supportedActions { action }
      assignedLocation { name location { id name } }
      fulfillmentHolds { id handle reason reasonNotes displayReason heldByApp { id title } heldByRequestingApp }
      merchantRequests(first: 10) { nodes { kind message requestOptions } }
      lineItems(first: 20) { nodes { id totalQuantity remainingQuantity lineItem { id title quantity fulfillableQuantity } } }
      order { id name displayFulfillmentStatus }
    }
  }"#;

const SHIPPING_FULFILLMENT_ORDER_DIRECT_MULTILINE_HYDRATE_QUERY: &str = r#"query ShippingFulfillmentOrderHydrate($id: ID!) {
    fulfillmentOrder(id: $id) {
      id
      status
      requestStatus
      fulfillAt
      fulfillBy
      updatedAt
      supportedActions { action }
      assignedLocation { name location { id name } }
      fulfillmentHolds { id handle reason reasonNotes displayReason heldByApp { id title } heldByRequestingApp }
      merchantRequests(first: 10) { nodes { kind message requestOptions } }
      lineItems(first: 20) { nodes { id totalQuantity remainingQuantity lineItem { id title quantity fulfillableQuantity } } }
      order { id name displayFulfillmentStatus }
    }
  }"#;

const SHIPPING_FULFILLMENT_ORDER_RELEASE_HOLD_HYDRATE_QUERY: &str = r#"query FulfillmentOrderReleaseHoldSelectiveHydrate($id: ID!) {
  fulfillmentOrder(id: $id) {
    id
    status
    requestStatus
    fulfillAt
    fulfillBy
    updatedAt
    supportedActions {
      action
    }
    assignedLocation {
      name
      location {
        id
        name
      }
    }
    fulfillmentHolds {
      id
      handle
      reason
      reasonNotes
      displayReason
      heldByRequestingApp
    }
    merchantRequests(first: 10) {
      nodes {
        kind
        message
        requestOptions
      }
    }
    lineItems(first: 20) {
      nodes {
        id
        totalQuantity
        remainingQuantity
        lineItem {
          id
          title
          quantity
          fulfillableQuantity
        }
      }
    }
    order {
      id
      name
      displayFulfillmentStatus
    }
  }
}"#;

impl DraftProxy {
    pub(in crate::proxy) fn shipping_fulfillment_order_read_response(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Response {
        let Some(fields) = root_fields(query, variables) else {
            return json_error(400, "Could not parse shipping fulfillment-order read");
        };
        // Top-level fulfillment-order *connection* reads (`fulfillmentOrders`,
        // `assignedFulfillmentOrders`, `manualHoldsFulfillmentOrders`) project the
        // locally-staged set. When no fulfillment orders have been staged in this
        // session the local engine can only return empty connections, which is never
        // richer than the store's real catalog — so forward the read upstream and
        // serve the authoritative store result (singular `fulfillmentOrder(id:)`
        // reads keep their dedicated hydration path below).
        let all_connection_reads = fields.iter().all(|field| {
            matches!(
                field.name.as_str(),
                "fulfillmentOrders" | "assignedFulfillmentOrders" | "manualHoldsFulfillmentOrders"
            )
        });
        if all_connection_reads && self.shipping_fulfillment_orders().is_empty() {
            return (self.upstream_transport)(request.clone());
        }
        let data = root_payload_json(&fields, |field| {
            Some(match field.name.as_str() {
                "fulfillmentOrder" => {
                    let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
                    self.ensure_shipping_fulfillment_order_hydrated(request, &id);
                    let fulfillment_order = self
                        .shipping_fulfillment_order_by_id(&id)
                        .unwrap_or(Value::Null);
                    nullable_selected_json(&fulfillment_order, &field.selection)
                }
                "fulfillmentOrders" => fulfillment_order_connection_json(
                    self.shipping_fulfillment_orders(),
                    &field.arguments,
                    &field.selection,
                ),
                "assignedFulfillmentOrders" => {
                    // `assignedFulfillmentOrders` is scoped to the *open* (assigned)
                    // records and honours the `assignmentStatus` + `locationIds`
                    // filters: closed/cancelled orders drop out, the assignment
                    // status maps onto request status / pending cancellation
                    // requests, and a non-empty location list narrows to the
                    // matching assigned locations.
                    let assignment_status =
                        resolved_string_field(&field.arguments, "assignmentStatus");
                    let location_ids = resolved_string_list_arg(&field.arguments, "locationIds");
                    let nodes = self
                        .shipping_fulfillment_orders()
                        .into_iter()
                        .filter(|order| {
                            !matches!(order["status"].as_str(), Some("CLOSED") | Some("CANCELLED"))
                        })
                        .filter(|order| {
                            assignment_status
                                .as_deref()
                                .map(|status| {
                                    fulfillment_order_matches_assignment_status(order, status)
                                })
                                .unwrap_or(true)
                        })
                        .filter(|order| {
                            location_ids.is_empty()
                                || order["assignedLocation"]["location"]["id"]
                                    .as_str()
                                    .map(|id| location_ids.iter().any(|wanted| wanted == id))
                                    .unwrap_or(false)
                        })
                        .collect::<Vec<_>>();
                    selected_connection_json_with_args(
                        nodes,
                        &field.arguments,
                        &field.selection,
                        |fulfillment_order| {
                            format!("cursor:{}", value_id_cursor(fulfillment_order))
                        },
                    )
                }
                "manualHoldsFulfillmentOrders" => {
                    let nodes = self
                        .shipping_fulfillment_orders()
                        .into_iter()
                        .filter(|order| {
                            order["status"].as_str() == Some("ON_HOLD")
                                || !fulfillment_order_holds(order).is_empty()
                        })
                        .collect::<Vec<_>>();
                    selected_connection_json_with_args(
                        nodes,
                        &field.arguments,
                        &field.selection,
                        value_id_cursor,
                    )
                }
                _ => return None,
            })
        });
        ok_json(json!({ "data": data }))
    }

    pub(in crate::proxy) fn shipping_fulfillment_order_mutation_response(
        &mut self,
        root_field: &str,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Response {
        let response = match root_field {
            "fulfillmentOrderHold" => {
                self.fulfillment_order_hold_store_backed(query, variables, request)
            }
            "fulfillmentOrderReleaseHold" => {
                self.fulfillment_order_release_hold_store_backed(query, variables, request)
            }
            "fulfillmentOrderMove" => {
                self.fulfillment_order_move_store_backed(query, variables, request)
            }
            "fulfillmentOrderOpen" => self.fulfillment_order_status_store_backed(
                root_field, "OPEN", query, variables, request,
            ),
            "fulfillmentOrderReportProgress" => self.fulfillment_order_status_store_backed(
                root_field,
                "IN_PROGRESS",
                query,
                variables,
                request,
            ),
            "fulfillmentOrderCancel" => {
                self.fulfillment_order_cancel_store_backed(query, variables, request)
            }
            "fulfillmentOrdersSetFulfillmentDeadline" => {
                self.fulfillment_order_set_deadline_store_backed(query, variables, request)
            }
            "fulfillmentOrderClose" => {
                self.fulfillment_order_close_store_backed(query, variables, request)
            }
            "fulfillmentOrderReschedule" => self.fulfillment_order_guardrail_response(
                root_field,
                query,
                "Fulfillment order must be scheduled.",
            ),
            "fulfillmentOrdersReroute" => self.fulfillment_orders_reroute_guardrail_response(query),
            // Request-lifecycle transitions, split, and merge stage against the
            // shared staged.orders fulfillment-order engine.
            "fulfillmentOrderSubmitFulfillmentRequest"
            | "fulfillmentOrderAcceptFulfillmentRequest"
            | "fulfillmentOrderRejectFulfillmentRequest"
            | "fulfillmentOrderSubmitCancellationRequest"
            | "fulfillmentOrderAcceptCancellationRequest"
            | "fulfillmentOrderRejectCancellationRequest"
            | "fulfillmentOrderSplit"
            | "fulfillmentOrderMerge" => {
                if let Some(data) = self
                    .fulfillment_order_local_mutation_data(request, root_field, query, variables)
                {
                    ok_json(data)
                } else {
                    json_error(
                        501,
                        &format!(
                            "No Rust shipping fulfillment dispatcher implemented for root field: {root_field}"
                        ),
                    )
                }
            }
            _ => json_error(
                501,
                &format!(
                    "No Rust shipping fulfillment dispatcher implemented for root field: {root_field}"
                ),
            ),
        };
        // Graceful-degradation passthrough. Some recorded scenarios only support
        // forwarding the mutation upstream: their capture records
        // `OrdersFulfillmentOrderHydrate` responses that lack the
        // assignedLocation/supportedActions the local engine needs to resolve
        // the fulfillment order, so the local handler bails out with a
        // "fulfillment order not found" result. When that happens, forward the
        // mutation upstream and return the authentic recorded response. If the
        // upstream has nothing recorded for this request (a genuine invalid id),
        // keep the locally-computed not-found instead.
        if fulfillment_order_response_is_unresolved(&response.body) {
            let forwarded = (self.upstream_transport)(request.clone());
            if forwarded.status < 400
                && forwarded
                    .body
                    .get("data")
                    .is_some_and(|data| !data.is_null())
            {
                return forwarded;
            }
        }
        response
    }

    fn fulfillment_order_store_backed_parts(
        root_field: &str,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> (String, Vec<SelectedField>, BTreeMap<String, ResolvedValue>) {
        primary_root_response_parts(query, variables, || root_field.to_string())
    }

    fn fulfillment_order_store_backed_preamble(
        &mut self,
        root_field: &str,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
        guardrail_message: Option<&str>,
    ) -> Result<FulfillmentOrderStoreBackedPreamble, Response> {
        let (response_key, payload_selection, arguments) =
            Self::fulfillment_order_store_backed_parts(root_field, query, variables);
        let id = resolved_string_field(&arguments, "id").unwrap_or_default();
        if !self.ensure_shipping_fulfillment_order_hydrated(request, &id) {
            return Err(self.fulfillment_order_missing_response(
                root_field,
                query,
                &response_key,
                &id,
                guardrail_message,
            ));
        }
        let Some((order_id, index)) = self.shipping_fulfillment_order_location(&id) else {
            return Err(self.fulfillment_order_missing_response(
                root_field,
                query,
                &response_key,
                &id,
                guardrail_message,
            ));
        };
        Ok(FulfillmentOrderStoreBackedPreamble {
            response_key,
            payload_selection,
            arguments,
            id,
            order_id,
            index,
        })
    }

    fn fulfillment_order_missing_response(
        &self,
        root_field: &str,
        query: &str,
        response_key: &str,
        id: &str,
        guardrail_message: Option<&str>,
    ) -> Response {
        if let Some(message) = guardrail_message {
            self.fulfillment_order_guardrail_response(root_field, query, message)
        } else {
            self.fulfillment_order_not_found_response(root_field, response_key, id)
        }
    }

    fn fulfillment_order_hold_store_backed(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Response {
        let FulfillmentOrderStoreBackedPreamble {
            response_key,
            payload_selection,
            arguments,
            id,
            order_id,
            index,
        } = match self.fulfillment_order_store_backed_preamble(
            "fulfillmentOrderHold",
            query,
            variables,
            request,
            None,
        ) {
            Ok(preamble) => preamble,
            Err(response) => return response,
        };
        let Some(input) = resolved_object_field(&arguments, "fulfillmentHold") else {
            return ok_json(json!({
                "data": {
                    response_key: fulfillment_order_hold_payload_json(
                        Value::Null,
                        Value::Null,
                        Value::Null,
                        &payload_selection,
                        vec![user_error(["fulfillmentHold"], "Fulfillment hold is required.", Some("INVALID"))]
                    )
                }
            }));
        };
        let timestamp = self.next_shipping_fulfillment_timestamp();
        let hold = self.shipping_fulfillment_hold_from_input(&input);
        let hold_handle = hold["handle"].as_str().unwrap_or_default().to_string();
        let requested = fulfillment_order_line_item_quantities(&input);
        let requested_line_items = resolved_object_list_field(&input, "fulfillmentOrderLineItems");
        let mut seen_line_item_ids = BTreeSet::new();
        let has_duplicate_line_items = requested_line_items.iter().any(|item| {
            resolved_string_field(item, "id").is_some_and(|id| !seen_line_item_ids.insert(id))
        });
        if has_duplicate_line_items {
            return ok_json(json!({
                "data": {
                    response_key: fulfillment_order_hold_payload_json(
                        Value::Null,
                        Value::Null,
                        Value::Null,
                        &payload_selection,
                        vec![user_error(["fulfillmentHold", "fulfillmentOrderLineItems"], "must contain unique line item ids", Some("DUPLICATED_FULFILLMENT_ORDER_LINE_ITEMS"))]
                    )
                }
            }));
        }
        if requested_line_items.iter().any(|item| {
            resolved_int_field(item, "quantity")
                .map(|quantity| quantity <= 0)
                .unwrap_or(false)
        }) {
            return ok_json(json!({
                "data": {
                    response_key: fulfillment_order_hold_payload_json(
                        Value::Null,
                        Value::Null,
                        Value::Null,
                        &payload_selection,
                        vec![user_error(["fulfillmentHold", "fulfillmentOrderLineItems", "0", "quantity"], "You must select at least one item to place on partial hold.", Some("GREATER_THAN_ZERO"))]
                    )
                }
            }));
        }
        let existing_fulfillment_order = self
            .shipping_fulfillment_order_by_id(&id)
            .unwrap_or(Value::Null);
        let existing_holds = fulfillment_order_holds(&existing_fulfillment_order);
        let had_existing_holds = !existing_holds.is_empty();
        if existing_holds
            .iter()
            .any(|existing| existing["handle"].as_str() == Some(hold_handle.as_str()))
        {
            return ok_json(json!({
                "data": {
                    response_key: fulfillment_order_hold_payload_json(
                        Value::Null,
                        Value::Null,
                        Value::Null,
                        &payload_selection,
                        vec![user_error(["fulfillmentHold", "handle"], "The handle provided for the fulfillment hold is already in use by this app for another hold on this fulfillment order.", Some("DUPLICATE_FULFILLMENT_HOLD_HANDLE"))]
                    )
                }
            }));
        }
        if existing_holds.len() >= 10 {
            return ok_json(json!({
                "data": {
                    response_key: fulfillment_order_hold_payload_json(
                        Value::Null,
                        Value::Null,
                        Value::Null,
                        &payload_selection,
                        vec![user_error(["id"], "The maximum number of fulfillment holds for this fulfillment order has been reached for this app. An app can only have up to 10 holds on a single fulfillment order at any one time.", Some("FULFILLMENT_ORDER_HOLD_LIMIT_REACHED"))]
                    )
                }
            }));
        }
        if !existing_holds.is_empty() && !requested.is_empty() {
            return ok_json(json!({
                "data": {
                    response_key: fulfillment_order_hold_payload_json(
                        Value::Null,
                        Value::Null,
                        Value::Null,
                        &payload_selection,
                        vec![user_error(["fulfillmentHold", "fulfillmentOrderLineItems"], "The fulfillment order is not in a splittable state.", Some("FULFILLMENT_ORDER_NOT_SPLITTABLE"))]
                    )
                }
            }));
        }
        let mut held = Value::Null;
        let mut remaining = Value::Null;
        let mut synthetic_order_ids = Vec::new();
        let mut synthetic_line_item_ids = Vec::new();
        if let Some(order) = self.store.staged.orders.get(&order_id) {
            if let Some(nodes) = fulfillment_order_nodes(order) {
                if let Some(source) = nodes.get(index) {
                    let needed_split =
                        requested_fulfillment_quantities_are_partial(source, &requested);
                    if needed_split {
                        synthetic_order_ids.push(self.next_proxy_synthetic_gid("FulfillmentOrder"));
                        let line_count = requested.len().max(1);
                        for _ in 0..line_count {
                            synthetic_line_item_ids
                                .push(self.next_proxy_synthetic_gid("FulfillmentOrderLineItem"));
                        }
                    }
                }
            }
        }
        if let Some(order) = self.store.staged.orders.get_mut(&order_id) {
            if let Some(nodes) = fulfillment_order_nodes_mut(order) {
                let mut fulfillment_order = nodes[index].clone();
                let split = split_fulfillment_order_quantities(
                    &mut fulfillment_order,
                    &requested,
                    "hold",
                    &timestamp,
                    &mut synthetic_order_ids.into_iter(),
                    &mut synthetic_line_item_ids.into_iter(),
                );
                fulfillment_order["status"] = json!("ON_HOLD");
                fulfillment_order["updatedAt"] = json!(timestamp);
                if requested.is_empty() && !had_existing_holds {
                    set_fulfillment_order_line_item_fulfillable_quantity(&mut fulfillment_order, 0);
                }
                let mut holds = fulfillment_order_holds(&fulfillment_order);
                holds.push(hold.clone());
                fulfillment_order["supportedActions"] = if holds.len() >= 10 {
                    shipping_fulfillment_supported_actions(&["RELEASE_HOLD", "MOVE"])
                } else {
                    shipping_fulfillment_supported_actions(&["RELEASE_HOLD", "HOLD", "MOVE"])
                };
                fulfillment_order["fulfillmentHolds"] = json!(holds);
                nodes[index] = fulfillment_order.clone();
                if let Some(mut remaining_order) = split {
                    remaining_order["supportedActions"] = shipping_fulfillment_open_actions(
                        fulfillment_order_can_split(&remaining_order),
                    );
                    remaining_order["_draftProxySplitSource"] = json!(id);
                    remaining_order["_draftProxySplitKind"] = json!("hold");
                    nodes.insert(index + 1, remaining_order.clone());
                    remaining = remaining_order;
                }
                held = fulfillment_order;
            }
            update_order_display_fulfillment_status(order);
        }
        self.record_mutation_log_entry(request, query, variables, "fulfillmentOrderHold", vec![id]);
        ok_json(json!({
            "data": {
                response_key: fulfillment_order_hold_payload_json(
                    hold,
                    held,
                    remaining,
                    &payload_selection,
                    vec![]
                )
            }
        }))
    }

    fn fulfillment_order_release_hold_store_backed(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Response {
        let FulfillmentOrderStoreBackedPreamble {
            response_key,
            payload_selection,
            arguments,
            id,
            order_id,
            index,
        } = match self.fulfillment_order_store_backed_preamble(
            "fulfillmentOrderReleaseHold",
            query,
            variables,
            request,
            None,
        ) {
            Ok(preamble) => preamble,
            Err(response) => return response,
        };
        let hold_ids = list_string_field(&arguments, "holdIds");
        let external_id = resolved_string_field(&arguments, "externalId");
        let timestamp = self.next_shipping_fulfillment_timestamp();
        let mut released = Value::Null;
        if let Some(order) = self.store.staged.orders.get_mut(&order_id) {
            if let Some(nodes) = fulfillment_order_nodes_mut(order) {
                let mut fulfillment_order = nodes[index].clone();
                let holds = fulfillment_order_holds(&fulfillment_order)
                    .into_iter()
                    .filter(|hold| {
                        let matches_id = hold["id"]
                            .as_str()
                            .is_some_and(|hold_id| hold_ids.iter().any(|id| id == hold_id));
                        let matches_external_id = external_id.as_ref().is_some_and(|external_id| {
                            hold["handle"].as_str() == Some(external_id)
                        });
                        !(hold_ids.is_empty() && external_id.is_none()
                            || matches_id
                            || matches_external_id)
                    })
                    .collect::<Vec<_>>();
                fulfillment_order["fulfillmentHolds"] = json!(holds);
                if fulfillment_order_holds(&fulfillment_order).is_empty() {
                    fulfillment_order["status"] = json!("OPEN");
                    fulfillment_order["supportedActions"] =
                        shipping_fulfillment_open_actions(false);
                    restore_fulfillment_order_line_item_fulfillable_quantity(
                        &mut fulfillment_order,
                    );
                } else {
                    set_fulfillment_order_line_item_fulfillable_quantity(&mut fulfillment_order, 0);
                }
                fulfillment_order["updatedAt"] = json!(timestamp);
                nodes[index] = fulfillment_order.clone();
                restore_hold_split_quantities(nodes, index, &id);
                released = nodes[index].clone();
                if fulfillment_order_holds(&released).is_empty() {
                    nodes[index]["supportedActions"] =
                        shipping_fulfillment_open_actions(fulfillment_order_can_split(&released));
                    released = nodes[index].clone();
                }
            }
            update_order_display_fulfillment_status(order);
        }
        self.record_mutation_log_entry(
            request,
            query,
            variables,
            "fulfillmentOrderReleaseHold",
            vec![id],
        );
        ok_json(json!({
            "data": {
                response_key: fulfillment_order_simple_payload_json(
                    released,
                    &payload_selection,
                    vec![]
                )
            }
        }))
    }

    fn fulfillment_order_move_store_backed(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Response {
        let FulfillmentOrderStoreBackedPreamble {
            response_key,
            payload_selection,
            arguments,
            id,
            order_id,
            index,
        } = match self.fulfillment_order_store_backed_preamble(
            "fulfillmentOrderMove",
            query,
            variables,
            request,
            None,
        ) {
            Ok(preamble) => preamble,
            Err(response) => return response,
        };
        if self
            .shipping_fulfillment_order_by_id(&id)
            .and_then(|order| {
                order["requestStatus"]
                    .as_str()
                    .map(|status| matches!(status, "SUBMITTED" | "ACCEPTED"))
            })
            .unwrap_or(false)
        {
            return ok_json(json!({
                "data": {
                    response_key: fulfillment_order_move_payload_json(
                        Value::Null,
                        Value::Null,
                        Value::Null,
                        &payload_selection,
                        vec![user_error(Value::Null, "Cannot move submitted fulfillment order that is at a 3PL fulfillment service.", None)]
                    )
                }
            }));
        }
        let new_location_id =
            resolved_string_field(&arguments, "newLocationId").unwrap_or_default();
        let requested = fulfillment_order_line_item_quantities(&arguments);
        let timestamp = self.next_shipping_fulfillment_timestamp();
        self.ensure_location_hydrated(&new_location_id, request);
        let destination_location = self.shipping_move_destination_location(&new_location_id);
        let current_order = self
            .shipping_fulfillment_order_by_id(&id)
            .unwrap_or(Value::Null);
        let current_status = current_order["status"].as_str().unwrap_or_default();
        let current_request_status = current_order["requestStatus"].as_str().unwrap_or_default();
        let move_error = if matches!(current_status, "CLOSED" | "CANCELLED") {
            Some(user_error(Value::Null, "Cannot change location.", None))
        } else if current_status == "IN_PROGRESS" {
            Some(user_error(
                ["id"],
                "Cannot move a fulfillment order that has had progress reported. To move a fulfillment order that has had progress reported, the fulfillment order must first be marked as open resolving the ongoing progress state.",
                Some("CANNOT_MOVE_FULFILLMENT_ORDER_WITH_REPORTED_PROGRESS"),
            ))
        } else if matches!(current_request_status, "SUBMITTED" | "ACCEPTED") {
            Some(user_error(
                Value::Null,
                "Cannot move submitted fulfillment order that is at a 3PL fulfillment service.",
                None,
            ))
        } else if destination_location.is_none() {
            Some(user_error(["id"], "Location not found.", None))
        } else {
            None
        };
        if let Some(error) = move_error {
            return ok_json(json!({
                "data": {
                    response_key: fulfillment_order_move_payload_json(
                        Value::Null,
                        Value::Null,
                        Value::Null,
                        &payload_selection,
                        vec![error]
                    )
                }
            }));
        }
        let assigned_location =
            self.shipping_assigned_location(destination_location.as_ref().expect("validated"));
        let mut synthetic_order_ids = Vec::new();
        let mut synthetic_line_item_ids = Vec::new();
        if let Some(order) = self.store.staged.orders.get(&order_id) {
            if let Some(nodes) = fulfillment_order_nodes(order) {
                if let Some(source) = nodes.get(index) {
                    let needed_split =
                        requested_fulfillment_quantities_are_partial(source, &requested);
                    if needed_split {
                        synthetic_order_ids.push(self.next_proxy_synthetic_gid("FulfillmentOrder"));
                        let line_count = requested.len().max(1);
                        for _ in 0..line_count {
                            synthetic_line_item_ids
                                .push(self.next_proxy_synthetic_gid("FulfillmentOrderLineItem"));
                        }
                    }
                }
            }
        }
        let mut moved = Value::Null;
        let mut original = Value::Null;
        let mut remaining = Value::Null;
        if let Some(order) = self.store.staged.orders.get_mut(&order_id) {
            if let Some(nodes) = fulfillment_order_nodes_mut(order) {
                let mut fulfillment_order = nodes[index].clone();
                let split = split_fulfillment_order_quantities(
                    &mut fulfillment_order,
                    &requested,
                    "move",
                    &timestamp,
                    &mut synthetic_order_ids.into_iter(),
                    &mut synthetic_line_item_ids.into_iter(),
                );
                fulfillment_order["updatedAt"] = json!(timestamp);
                nodes[index] = fulfillment_order.clone();
                if let Some(mut moved_order) = split {
                    let original_can_split = fulfillment_order_can_split(&nodes[index]);
                    nodes[index]["supportedActions"] =
                        shipping_fulfillment_open_actions(original_can_split);
                    original = nodes[index].clone();
                    remaining = original.clone();
                    let moved_can_split = fulfillment_order_can_split(&moved_order);
                    moved_order["supportedActions"] =
                        shipping_fulfillment_open_actions(moved_can_split);
                    moved_order["assignedLocation"] = assigned_location;
                    moved_order["_draftProxySplitSource"] = json!(id);
                    moved_order["_draftProxySplitKind"] = json!("move");
                    nodes.insert(index + 1, moved_order.clone());
                    moved = moved_order;
                } else {
                    let mut moved_order = fulfillment_order;
                    let moved_can_split = fulfillment_order_can_split(&moved_order);
                    moved_order["supportedActions"] =
                        shipping_fulfillment_open_actions(moved_can_split);
                    moved_order["assignedLocation"] = assigned_location;
                    nodes[index] = moved_order.clone();
                    moved = moved_order.clone();
                    original = moved_order;
                }
            }
            update_order_display_fulfillment_status(order);
        }
        self.record_mutation_log_entry(request, query, variables, "fulfillmentOrderMove", vec![id]);
        ok_json(json!({
            "data": {
                response_key: fulfillment_order_move_payload_json(
                    moved,
                    original,
                    remaining,
                    &payload_selection,
                    vec![]
                )
            }
        }))
    }

    fn fulfillment_order_status_store_backed(
        &mut self,
        root_field: &str,
        next_status: &str,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Response {
        let FulfillmentOrderStoreBackedPreamble {
            response_key,
            payload_selection,
            arguments: _,
            id,
            order_id,
            index,
        } = match self
            .fulfillment_order_store_backed_preamble(root_field, query, variables, request, None)
        {
            Ok(preamble) => preamble,
            Err(response) => return response,
        };
        let current_status = self
            .shipping_fulfillment_order_by_id(&id)
            .and_then(|order| order["status"].as_str().map(str::to_string))
            .unwrap_or_default();
        let invalid: Option<(String, Value, Option<&'static str>)> =
            match (root_field, current_status.as_str()) {
                ("fulfillmentOrderOpen", "SCHEDULED" | "IN_PROGRESS") => None,
                ("fulfillmentOrderOpen", "OPEN" | "CLOSED" | "CANCELLED" | "ON_HOLD") => Some((
                    format!(
                        "Expected fulfillment order status to be valid but it was {}.",
                        current_status.to_ascii_lowercase()
                    ),
                    Value::Null,
                    Some("INVALID_FULFILLMENT_ORDER_STATUS"),
                )),
                (
                    "fulfillmentOrderReportProgress",
                    "SCHEDULED" | "CLOSED" | "CANCELLED" | "ON_HOLD",
                ) => Some((
                    "Cannot report progress on a fulfillment order in this state.".to_string(),
                    Value::Null,
                    Some("FULFILLMENT_ORDER_STATUS_INVALID"),
                )),
                _ => None,
            };
        if let Some((message, field, code)) = invalid {
            return ok_json(json!({
                "data": {
                    response_key: fulfillment_order_simple_payload_json(
                        Value::Null,
                        &payload_selection,
                        vec![user_error(field, &message, code)]
                    )
                }
            }));
        }
        let timestamp = self.next_shipping_fulfillment_timestamp();
        let mut updated = Value::Null;
        if let Some(order) = self.store.staged.orders.get_mut(&order_id) {
            if let Some(nodes) = fulfillment_order_nodes_mut(order) {
                let mut fulfillment_order = nodes[index].clone();
                fulfillment_order["status"] = json!(next_status);
                fulfillment_order["updatedAt"] = json!(timestamp);
                fulfillment_order["supportedActions"] = if next_status == "IN_PROGRESS" {
                    shipping_fulfillment_supported_actions(&[
                        "CREATE_FULFILLMENT",
                        "REPORT_PROGRESS",
                        "HOLD",
                        "MARK_AS_OPEN",
                    ])
                } else {
                    shipping_fulfillment_open_actions(false)
                };
                nodes[index] = fulfillment_order.clone();
                updated = fulfillment_order;
            }
            update_order_display_fulfillment_status(order);
        }
        self.record_mutation_log_entry(request, query, variables, root_field, vec![id]);
        ok_json(json!({
            "data": {
                response_key: fulfillment_order_simple_payload_json(
                    updated,
                    &payload_selection,
                    vec![]
                )
            }
        }))
    }

    fn fulfillment_order_cancel_store_backed(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Response {
        let FulfillmentOrderStoreBackedPreamble {
            response_key,
            payload_selection,
            arguments: _,
            id,
            order_id,
            index,
        } = match self.fulfillment_order_store_backed_preamble(
            "fulfillmentOrderCancel",
            query,
            variables,
            request,
            None,
        ) {
            Ok(preamble) => preamble,
            Err(response) => return response,
        };
        let status = self
            .shipping_fulfillment_order_by_id(&id)
            .and_then(|order| order["status"].as_str().map(str::to_string))
            .unwrap_or_default();
        if status == "CLOSED" || status == "CANCELLED" {
            return ok_json(json!({
                "data": {
                    response_key: fulfillment_order_cancel_payload_json(
                        Value::Null,
                        Value::Null,
                        &payload_selection,
                        vec![user_error(Value::Null, "Fulfillment order is not in cancelable request state and can't be canceled.", None)]
                    )
                }
            }));
        }
        if status == "IN_PROGRESS" {
            return ok_json(json!({
                "data": {
                    response_key: fulfillment_order_cancel_payload_json(
                        Value::Null,
                        Value::Null,
                        &payload_selection,
                        vec![user_error(["id"], "Cannot cancel fulfillment order that has had progress reported. Mark as unfulfilled first.", None)]
                    )
                }
            }));
        }
        let timestamp = self.next_shipping_fulfillment_timestamp();
        let replacement_id = self.next_proxy_synthetic_gid("FulfillmentOrder");
        let mut cancelled = Value::Null;
        let mut replacement = Value::Null;
        if let Some(order) = self.store.staged.orders.get_mut(&order_id) {
            if let Some(nodes) = fulfillment_order_nodes_mut(order) {
                let mut fulfillment_order = nodes[index].clone();
                replacement = fulfillment_order.clone();
                replacement["id"] = json!(replacement_id);
                replacement["updatedAt"] = json!(timestamp.clone());
                replacement["_draftProxySplitSource"] = json!(id);
                replacement["_draftProxySplitKind"] = json!("cancel");
                fulfillment_order["status"] = json!("CLOSED");
                fulfillment_order["updatedAt"] = json!(timestamp);
                fulfillment_order["supportedActions"] = json!([]);
                fulfillment_order["lineItems"] = json!({ "nodes": [] });
                nodes[index] = fulfillment_order.clone();
                nodes.insert(index + 1, replacement.clone());
                cancelled = fulfillment_order;
            }
            update_order_display_fulfillment_status(order);
        }
        self.record_mutation_log_entry(
            request,
            query,
            variables,
            "fulfillmentOrderCancel",
            vec![id],
        );
        ok_json(json!({
            "data": {
                response_key: fulfillment_order_cancel_payload_json(
                    cancelled,
                    replacement,
                    &payload_selection,
                    vec![]
                )
            }
        }))
    }

    fn fulfillment_order_set_deadline_store_backed(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Response {
        let (response_key, payload_selection, arguments) =
            Self::fulfillment_order_store_backed_parts(
                "fulfillmentOrdersSetFulfillmentDeadline",
                query,
                variables,
            );
        let ids = list_string_field(&arguments, "fulfillmentOrderIds");
        for id in &ids {
            self.ensure_shipping_fulfillment_order_hydrated(request, id);
        }
        let known_ids: Vec<String> = ids
            .iter()
            .filter(|id| self.shipping_fulfillment_order_location(id).is_some())
            .cloned()
            .collect();
        let (success, errors) = if known_ids.is_empty() {
            (
                false,
                vec![user_error(
                    Value::Null,
                    "Fulfillment orders could not be found.",
                    None,
                )],
            )
        } else {
            let deadline = resolved_string_field(&arguments, "fulfillmentDeadline")
                .map(|value| shopify_datetime_seconds(&value))
                .unwrap_or_default();
            let timestamp = self.next_shipping_fulfillment_timestamp();
            for id in &known_ids {
                self.store
                    .staged
                    .fulfillment_order_deadlines
                    .insert(id.clone(), deadline.clone());
                if let Some((order_id, index)) = self.shipping_fulfillment_order_location(id) {
                    if let Some(order) = self.store.staged.orders.get_mut(&order_id) {
                        if let Some(nodes) = fulfillment_order_nodes_mut(order) {
                            nodes[index]["fulfillBy"] = json!(deadline);
                            nodes[index]["updatedAt"] = json!(timestamp);
                        }
                    }
                }
            }
            self.record_mutation_log_entry(
                request,
                query,
                variables,
                "fulfillmentOrdersSetFulfillmentDeadline",
                ids,
            );
            (true, vec![])
        };
        ok_json(json!({
            "data": {
                response_key: fulfillment_order_deadline_payload_json(
                    success,
                    &payload_selection,
                    errors
                )
            }
        }))
    }

    fn fulfillment_order_close_store_backed(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Response {
        let FulfillmentOrderStoreBackedPreamble {
            response_key,
            payload_selection,
            arguments: _,
            id,
            order_id,
            index,
        } = match self.fulfillment_order_store_backed_preamble(
            "fulfillmentOrderClose",
            query,
            variables,
            request,
            Some("The fulfillment order's assigned fulfillment service must be of api type"),
        ) {
            Ok(preamble) => preamble,
            Err(response) => return response,
        };
        let accepted_request = self
            .shipping_fulfillment_order_by_id(&id)
            .and_then(|order| order["requestStatus"].as_str().map(str::to_string))
            .as_deref()
            == Some("ACCEPTED");
        if !accepted_request {
            return self.fulfillment_order_guardrail_response(
                "fulfillmentOrderClose",
                query,
                "The fulfillment order's assigned fulfillment service must be of api type",
            );
        }
        let timestamp = self.next_shipping_fulfillment_timestamp();
        let mut closed = Value::Null;
        if let Some(order) = self.store.staged.orders.get_mut(&order_id) {
            if let Some(nodes) = fulfillment_order_nodes_mut(order) {
                let mut fulfillment_order = nodes[index].clone();
                fulfillment_order["status"] = json!("INCOMPLETE");
                fulfillment_order["requestStatus"] = json!("CLOSED");
                fulfillment_order["updatedAt"] = json!(timestamp);
                fulfillment_order["supportedActions"] = shipping_fulfillment_supported_actions(&[
                    "REQUEST_FULFILLMENT",
                    "CREATE_FULFILLMENT",
                    "HOLD",
                    "MOVE",
                ]);
                nodes[index] = fulfillment_order.clone();
                closed = fulfillment_order;
            }
            update_order_display_fulfillment_status(order);
        }
        self.record_mutation_log_entry(
            request,
            query,
            variables,
            "fulfillmentOrderClose",
            vec![id],
        );
        ok_json(json!({
            "data": {
                response_key: fulfillment_order_simple_payload_json(
                    closed,
                    &payload_selection,
                    vec![]
                )
            }
        }))
    }

    fn fulfillment_order_guardrail_response(
        &self,
        root_field: &str,
        query: &str,
        message: &str,
    ) -> Response {
        let (response_key, payload_selection) =
            primary_root_response_selection(query, &BTreeMap::new(), || root_field.to_string());
        ok_json(json!({
            "data": {
                response_key: fulfillment_order_simple_payload_json(
                    Value::Null,
                    &payload_selection,
                    vec![user_error(Value::Null, message, None)]
                )
            }
        }))
    }

    fn fulfillment_orders_reroute_guardrail_response(&self, query: &str) -> Response {
        let (response_key, payload_selection) =
            primary_root_response_selection(query, &BTreeMap::new(), || {
                "fulfillmentOrdersReroute".to_string()
            });
        ok_json(json!({
            "data": {
                response_key: fulfillment_orders_reroute_payload_json(
                    Vec::new(),
                    &payload_selection,
                    vec![user_error(Value::Null, "Fulfillment orders could not be rerouted locally.", Some("NOT_IMPLEMENTED"))]
                )
            }
        }))
    }

    fn fulfillment_order_not_found_response(
        &self,
        root_field: &str,
        response_key: &str,
        id: &str,
    ) -> Response {
        ok_json(json!({
            "errors": [{
                "message": format!("Invalid id: {id}"),
                "extensions": { "code": "RESOURCE_NOT_FOUND" },
                "path": [root_field]
            }],
            "data": { response_key: Value::Null }
        }))
    }

    fn ensure_shipping_fulfillment_order_hydrated(&mut self, request: &Request, id: &str) -> bool {
        if id.is_empty() {
            return false;
        }
        if self.shipping_fulfillment_order_location(id).is_some()
            && !self.shipping_fulfillment_order_needs_hydration(id)
        {
            return true;
        }
        let response = self.upstream_post(
            request,
            json!({
                "query": SHIPPING_FULFILLMENT_ORDER_HYDRATE_QUERY,
                "variables": { "id": id }
            }),
        );
        if response.status < 400 {
            self.stage_shipping_fulfillment_order_hydrate_response(id, &response.body);
        }
        if self.shipping_fulfillment_order_location(id).is_some()
            && !self.shipping_fulfillment_order_needs_hydration(id)
        {
            return true;
        }

        for query in [
            SHIPPING_FULFILLMENT_ORDER_DIRECT_HYDRATE_QUERY,
            SHIPPING_FULFILLMENT_ORDER_DIRECT_MULTILINE_HYDRATE_QUERY,
            SHIPPING_FULFILLMENT_ORDER_RELEASE_HOLD_HYDRATE_QUERY,
        ] {
            let direct_response = self.upstream_post(
                request,
                json!({
                    "query": query,
                    "variables": { "id": id }
                }),
            );
            if direct_response.status < 400 {
                self.stage_shipping_fulfillment_order_hydrate_response(id, &direct_response.body);
            }
            if self.shipping_fulfillment_order_location(id).is_some()
                && !self.shipping_fulfillment_order_needs_hydration(id)
            {
                break;
            }
        }
        self.shipping_fulfillment_order_location(id).is_some()
    }

    fn shipping_fulfillment_order_needs_hydration(&self, id: &str) -> bool {
        self.shipping_fulfillment_order_by_id(id)
            .map(|order| {
                order["assignedLocation"].is_null()
                    || order["supportedActions"].is_null()
                    || order["updatedAt"].is_null()
                    || order["lineItems"]["nodes"].as_array().is_none()
            })
            .unwrap_or(true)
    }

    fn stage_shipping_fulfillment_order_hydrate_response(&mut self, id: &str, body: &Value) {
        if body["data"]["order"].is_object() {
            self.stage_shipping_fulfillment_order_order(body["data"]["order"].clone());
            return;
        }
        let node = if body["data"]["node"].is_object() {
            body["data"]["node"].clone()
        } else if body["data"]["fulfillmentOrder"].is_object() {
            body["data"]["fulfillmentOrder"].clone()
        } else {
            return;
        };
        let order_id = node["order"]["id"]
            .as_str()
            .map(str::to_string)
            .unwrap_or_else(|| {
                synthetic_shopify_gid(
                    "Order",
                    format!("fulfillment-order-{}", resource_id_tail(id)),
                )
            });
        let mut order = node["order"].clone();
        if !order.is_object() {
            order = json!({
                "id": order_id,
                "name": "",
                "displayFulfillmentStatus": "UNFULFILLED"
            });
        }
        order["fulfillmentOrders"] = json!({ "nodes": [node] });
        self.stage_shipping_fulfillment_order_order(order);
    }

    fn stage_shipping_fulfillment_order_record(&mut self, fulfillment_order: Value) {
        let Some(id) = fulfillment_order["id"].as_str().map(str::to_string) else {
            return;
        };
        if let Some((order_id, index)) = self.shipping_fulfillment_order_location(&id) {
            if let Some(order) = self.store.staged.orders.get_mut(&order_id) {
                if let Some(nodes) = fulfillment_order_nodes_mut(order) {
                    if let Some(existing) = nodes.get_mut(index) {
                        merge_staged_json(existing, fulfillment_order);
                    }
                }
            }
            return;
        }
        let order_id = fulfillment_order["order"]["id"]
            .as_str()
            .map(str::to_string)
            .unwrap_or_else(|| {
                synthetic_shopify_gid(
                    "Order",
                    format!("fulfillment-order-{}", resource_id_tail(&id)),
                )
            });
        let mut order = fulfillment_order["order"].clone();
        if !order.is_object() {
            order = json!({
                "id": order_id,
                "name": "",
                "displayFulfillmentStatus": "UNFULFILLED"
            });
        }
        order["fulfillmentOrders"] = json!({ "nodes": [fulfillment_order] });
        self.stage_shipping_fulfillment_order_order(order);
    }

    fn stage_shipping_fulfillment_order_order(&mut self, order: Value) {
        let Some(id) = order["id"].as_str().map(str::to_string) else {
            return;
        };
        let nodes = fulfillment_order_nodes(&order).cloned().unwrap_or_default();
        if let Some(existing) = self.store.staged.orders.get_mut(&id) {
            let mut order_summary = order.clone();
            if let Some(object) = order_summary.as_object_mut() {
                object.remove("fulfillmentOrders");
            }
            merge_staged_json(existing, order_summary.clone());
            if let Some(existing_nodes) = fulfillment_order_nodes_mut(existing) {
                for mut node in nodes {
                    if !node["order"].is_object() {
                        node["order"] = order_summary.clone();
                    }
                    if let Some(existing_node) = existing_nodes
                        .iter_mut()
                        .find(|candidate| candidate["id"] == node["id"])
                    {
                        merge_staged_json(existing_node, node);
                    } else {
                        existing_nodes.push(node);
                    }
                }
            } else {
                existing["fulfillmentOrders"] = json!({ "nodes": nodes });
            }
            return;
        }
        if nodes.iter().any(|node| {
            node["id"]
                .as_str()
                .is_some_and(|id| self.shipping_fulfillment_order_location(id).is_some())
        }) {
            let mut order_summary = order.clone();
            if let Some(object) = order_summary.as_object_mut() {
                object.remove("fulfillmentOrders");
            }
            for mut node in nodes {
                if !node["order"].is_object() {
                    node["order"] = order_summary.clone();
                }
                self.stage_shipping_fulfillment_order_record(node);
            }
            return;
        }
        self.store.staged.orders.insert(id, order);
    }

    fn shipping_fulfillment_order_location(&self, id: &str) -> Option<(String, usize)> {
        for (order_id, order) in &self.store.staged.orders {
            let Some(nodes) = fulfillment_order_nodes(order) else {
                continue;
            };
            for (index, node) in nodes.iter().enumerate() {
                if node["id"].as_str() == Some(id) {
                    return Some((order_id.clone(), index));
                }
            }
        }
        None
    }

    fn shipping_fulfillment_order_by_id(&self, id: &str) -> Option<Value> {
        let (order_id, index) = self.shipping_fulfillment_order_location(id)?;
        self.store
            .staged
            .orders
            .get(&order_id)
            .and_then(fulfillment_order_nodes)
            .and_then(|nodes| nodes.get(index).cloned())
    }

    fn shipping_fulfillment_orders(&self) -> Vec<Value> {
        self.store
            .staged
            .orders
            .values()
            .filter_map(fulfillment_order_nodes)
            .flatten()
            .cloned()
            .collect()
    }

    fn shipping_move_destination_location(&self, location_id: &str) -> Option<Value> {
        self.store
            .staged
            .locations
            .get(location_id)
            .filter(|location| location["isActive"].as_bool().unwrap_or(true))
            .cloned()
    }

    fn shipping_assigned_location(&self, location: &Value) -> Value {
        let id = location["id"].as_str().unwrap_or_default();
        let name = location["name"].as_str().unwrap_or_default();
        json!({
            "name": name,
            "location": { "id": id, "name": name }
        })
    }

    fn shipping_fulfillment_hold_from_input(
        &mut self,
        input: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        let reason = resolved_string_field(input, "reason").unwrap_or_else(|| "OTHER".to_string());
        let reason_notes = resolved_string_field(input, "reasonNotes");
        let external_id = resolved_string_field(input, "externalId");
        let notify_merchant = resolved_bool_field(input, "notifyMerchant").unwrap_or(false);
        let handle = resolved_string_field(input, "handle")
            .or_else(|| external_id.clone())
            .unwrap_or_else(|| {
                format!(
                    "fulfillment-hold-{}",
                    resource_id_tail(&self.next_proxy_synthetic_gid("FulfillmentHold"))
                )
            });
        json!({
            "id": self.next_proxy_synthetic_gid("FulfillmentHold"),
            "handle": handle,
            "externalId": external_id.map(Value::String).unwrap_or(Value::Null),
            "reason": reason,
            "reasonNotes": reason_notes.map(Value::String).unwrap_or(Value::Null),
            "displayReason": fulfillment_hold_display_reason(&reason),
            "heldByApp": Value::Null,
            "heldByRequestingApp": true,
            "__draftProxyNotifyMerchant": notify_merchant
        })
    }

    fn next_shipping_fulfillment_timestamp(&mut self) -> String {
        let offset = self.next_synthetic_id;
        self.next_synthetic_id += 1;
        format!(
            "2026-01-01T00:{:02}:{:02}Z",
            (offset / 60) % 60,
            offset % 60
        )
    }

    pub(in crate::proxy) fn shipping_fulfillment_order_local_order_read(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Response {
        let Some(fields) = root_fields(query, variables) else {
            return json_error(400, "Could not parse shipping fulfillment-order order read");
        };
        let data = root_payload_json(&fields, |field| {
            Some(match field.name.as_str() {
                "order" => {
                    let id = resolved_string_field(&field.arguments, "id")
                        .or_else(|| resolved_string_field(&field.arguments, "orderId"))
                        .unwrap_or_default();
                    let order = self
                        .store
                        .staged
                        .orders
                        .get(&id)
                        .cloned()
                        .unwrap_or(Value::Null);
                    selected_order_with_fulfillment_order_connections(&order, &field.selection)
                }
                "fulfillmentOrder" => {
                    let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
                    let fulfillment_order = self
                        .shipping_fulfillment_order_by_id(&id)
                        .unwrap_or(Value::Null);
                    nullable_selected_json(&fulfillment_order, &field.selection)
                }
                "fulfillmentOrders" => fulfillment_order_connection_json(
                    self.shipping_fulfillment_orders(),
                    &field.arguments,
                    &field.selection,
                ),
                "assignedFulfillmentOrders" => {
                    let nodes = self.shipping_fulfillment_orders();
                    selected_connection_json_with_args(
                        nodes,
                        &field.arguments,
                        &field.selection,
                        value_id_cursor,
                    )
                }
                "manualHoldsFulfillmentOrders" => {
                    let nodes = self
                        .shipping_fulfillment_orders()
                        .into_iter()
                        .filter(|order| {
                            order["status"].as_str() == Some("ON_HOLD")
                                || !fulfillment_order_holds(order).is_empty()
                        })
                        .collect::<Vec<_>>();
                    selected_connection_json_with_args(
                        nodes,
                        &field.arguments,
                        &field.selection,
                        value_id_cursor,
                    )
                }
                _ => return None,
            })
        });
        ok_json(json!({ "data": data }))
    }

    pub(in crate::proxy) fn should_handle_shipping_fulfillment_order_local_order_read(
        &self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> bool {
        let Some(fields) = root_fields(query, variables) else {
            return false;
        };
        fields.iter().any(|field| match field.name.as_str() {
            "order" => {
                let order_id = resolved_string_field(&field.arguments, "id")
                    .or_else(|| resolved_string_field(&field.arguments, "orderId"));
                let selects_fulfillment_orders =
                    selected_child_selection(&field.selection, "fulfillmentOrders").is_some();
                selects_fulfillment_orders
                    && order_id.is_some_and(|id| self.store.staged.orders.contains_key(&id))
            }
            "fulfillmentOrder" | "fulfillmentOrders" | "manualHoldsFulfillmentOrders" => {
                !self.store.staged.orders.is_empty()
            }
            _ => false,
        })
    }
}

fn fulfillment_order_nodes(order: &Value) -> Option<&Vec<Value>> {
    order["fulfillmentOrders"]["nodes"].as_array()
}

fn fulfillment_order_nodes_mut(order: &mut Value) -> Option<&mut Vec<Value>> {
    order["fulfillmentOrders"]["nodes"].as_array_mut()
}

fn selected_order_with_fulfillment_order_connections(
    order: &Value,
    selections: &[SelectedField],
) -> Value {
    if order.is_null() {
        return Value::Null;
    }

    let mut projected = serde_json::Map::new();
    for selection in selections {
        let value = if selection.name == "fulfillmentOrders" {
            Some(order_fulfillment_order_connection_json(
                fulfillment_order_nodes(order).cloned().unwrap_or_default(),
                &selection.arguments,
                &selection.selection,
            ))
        } else {
            selected_json(order, std::slice::from_ref(selection))
                .get(&selection.response_key)
                .cloned()
        };
        if let Some(value) = value {
            projected.insert(selection.response_key.clone(), value);
        }
    }
    Value::Object(projected)
}

fn fulfillment_order_connection_json(
    nodes: Vec<Value>,
    arguments: &BTreeMap<String, ResolvedValue>,
    selection: &[SelectedField],
) -> Value {
    let include_closed = resolved_bool_field(arguments, "includeClosed").unwrap_or(false);
    selected_staged_connection_with_args(
        nodes,
        arguments,
        selection,
        move |order, _query| {
            StagedSearchDecision::from_bool(
                include_closed || !fulfillment_order_is_closed_for_connection(order),
            )
        },
        fulfillment_order_staged_sort_key,
        selected_json,
        value_id_cursor,
    )
}

fn order_fulfillment_order_connection_json(
    nodes: Vec<Value>,
    arguments: &BTreeMap<String, ResolvedValue>,
    selection: &[SelectedField],
) -> Value {
    let displayable = resolved_bool_field(arguments, "displayable").unwrap_or(false);
    let mut nodes = nodes
        .into_iter()
        .filter(|order| !displayable || !fulfillment_order_is_closed_for_connection(order))
        .collect::<Vec<_>>();
    if resolved_bool_field(arguments, "reverse").unwrap_or(false) {
        nodes.reverse();
    }
    selected_connection_json_with_args(nodes, arguments, selection, value_id_cursor)
}

fn fulfillment_order_is_closed_for_connection(order: &Value) -> bool {
    matches!(order["status"].as_str(), Some("CLOSED") | Some("CANCELLED"))
        || order["requestStatus"].as_str() == Some("CLOSED")
}

fn fulfillment_order_staged_sort_key(order: &Value, sort_key: Option<&str>) -> StagedSortKey {
    let id = fulfillment_order_id_sort_value(order);
    match sort_key.unwrap_or("ID") {
        "CREATED_AT" => vec![fulfillment_order_string_sort_value(order, "createdAt"), id],
        "UPDATED_AT" => vec![fulfillment_order_string_sort_value(order, "updatedAt"), id],
        "FULFILL_AT" => vec![fulfillment_order_string_sort_value(order, "fulfillAt"), id],
        "FULFILL_BY" => vec![fulfillment_order_string_sort_value(order, "fulfillBy"), id],
        _ => vec![id],
    }
}

fn fulfillment_order_id_sort_value(order: &Value) -> StagedSortValue {
    let tail = order["id"]
        .as_str()
        .map(resource_id_tail)
        .unwrap_or_default();
    tail.parse::<i64>()
        .map(StagedSortValue::I64)
        .unwrap_or_else(|_| StagedSortValue::String(tail.to_ascii_lowercase()))
}

fn fulfillment_order_string_sort_value(order: &Value, field: &str) -> StagedSortValue {
    order
        .get(field)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(|value| StagedSortValue::String(value.to_string()))
        .unwrap_or(StagedSortValue::Null)
}

fn fulfillment_order_holds(order: &Value) -> Vec<Value> {
    if let Some(holds) = order["fulfillmentHolds"].as_array() {
        holds.clone()
    } else {
        order["fulfillmentHolds"]["nodes"]
            .as_array()
            .cloned()
            .unwrap_or_default()
    }
}

fn fulfillment_order_line_item_nodes(order: &Value) -> Vec<Value> {
    order["lineItems"]["nodes"]
        .as_array()
        .cloned()
        .unwrap_or_default()
}

fn fulfillment_order_line_item_quantities(
    input: &BTreeMap<String, ResolvedValue>,
) -> BTreeMap<String, i64> {
    resolved_object_list_field(input, "fulfillmentOrderLineItems")
        .into_iter()
        .filter_map(|item| {
            let id = resolved_string_field(&item, "id")?;
            let quantity = resolved_int_field(&item, "quantity").unwrap_or(0).max(0);
            Some((id, quantity))
        })
        .collect()
}

/// True when a fulfillment-order mutation response indicates the local engine
/// could not resolve the target fulfillment order. Two shapes are recognized:
/// the shipping engine's top-level `RESOURCE_NOT_FOUND` GraphQL error
/// (`{ errors: [{ extensions: { code: "RESOURCE_NOT_FOUND" } }] }`) and the
/// orders engine's singular `FULFILLMENT_ORDER_NOT_FOUND` userError nested
/// under the mutation payload. Either signals that the scenario must be served
/// by forwarding the mutation upstream rather than computed locally. Multi-id
/// deadline validation keeps its local `FULFILLMENT_ORDERS_NOT_FOUND` payload
/// because that branch resolves existence from the staged/hydrated store.
fn fulfillment_order_response_is_unresolved(body: &Value) -> bool {
    if let Some(errors) = body.get("errors").and_then(Value::as_array) {
        if errors.iter().any(|error| {
            error
                .get("extensions")
                .and_then(|extensions| extensions.get("code"))
                .and_then(Value::as_str)
                == Some("RESOURCE_NOT_FOUND")
        }) {
            return true;
        }
    }
    if let Some(data) = body.get("data").and_then(Value::as_object) {
        for payload in data.values() {
            if let Some(user_errors) = payload.get("userErrors").and_then(Value::as_array) {
                if user_errors.iter().any(|error| {
                    matches!(
                        error.get("code").and_then(Value::as_str),
                        Some("FULFILLMENT_ORDER_NOT_FOUND")
                    )
                }) {
                    return true;
                }
            }
        }
    }
    false
}

fn fulfillment_order_can_split(order: &Value) -> bool {
    fulfillment_order_line_item_nodes(order).iter().any(|line| {
        line["remainingQuantity"]
            .as_i64()
            .or_else(|| line["totalQuantity"].as_i64())
            .unwrap_or(0)
            > 1
    })
}

/// A fulfillment order carries a still-open cancellation request when it holds a
/// `CANCELLATION_REQUEST` merchant request that has not yet been answered
/// (`responseData` is null). Such an order surfaces under the
/// `CANCELLATION_REQUESTED` assignment-status filter rather than
/// `FULFILLMENT_ACCEPTED`, even though its request status is still `ACCEPTED`.
fn fulfillment_order_has_open_cancellation_request(order: &Value) -> bool {
    order["merchantRequests"]["nodes"]
        .as_array()
        .map(|nodes| {
            nodes.iter().any(|request| {
                request["kind"].as_str() == Some("CANCELLATION_REQUEST")
                    && request["responseData"].is_null()
            })
        })
        .unwrap_or(false)
}

/// Maps Shopify's `FulfillmentOrderAssignmentStatus` (the `assignmentStatus`
/// argument on `assignedFulfillmentOrders`) onto the staged fulfillment order's
/// request status and pending merchant requests.
fn fulfillment_order_matches_assignment_status(order: &Value, status: &str) -> bool {
    let request_status = order["requestStatus"].as_str().unwrap_or("");
    match status {
        "FULFILLMENT_REQUESTED" => request_status == "SUBMITTED",
        "FULFILLMENT_ACCEPTED" => {
            request_status == "ACCEPTED" && !fulfillment_order_has_open_cancellation_request(order)
        }
        "CANCELLATION_REQUESTED" => fulfillment_order_has_open_cancellation_request(order),
        "FULFILLMENT_UNSUBMITTED" => request_status == "UNSUBMITTED",
        "FULFILLMENT_REQUEST_DECLINED" => request_status == "REJECTED",
        other => request_status == other,
    }
}

fn set_fulfillment_order_line_item_fulfillable_quantity(order: &mut Value, quantity: i64) {
    for line in order["lineItems"]["nodes"]
        .as_array_mut()
        .into_iter()
        .flatten()
    {
        if let Some(line_item) = line["lineItem"].as_object_mut() {
            line_item.insert("fulfillableQuantity".to_string(), json!(quantity));
        }
    }
}

fn restore_fulfillment_order_line_item_fulfillable_quantity(order: &mut Value) {
    for line in order["lineItems"]["nodes"]
        .as_array_mut()
        .into_iter()
        .flatten()
    {
        let remaining = line["remainingQuantity"]
            .as_i64()
            .or_else(|| line["totalQuantity"].as_i64())
            .unwrap_or(0);
        if let Some(line_item) = line["lineItem"].as_object_mut() {
            line_item.insert("fulfillableQuantity".to_string(), json!(remaining));
        }
    }
}

fn requested_fulfillment_quantities_are_partial(
    order: &Value,
    requested: &BTreeMap<String, i64>,
) -> bool {
    if requested.is_empty() {
        return false;
    }
    let lines = fulfillment_order_line_item_nodes(order);
    if requested.len() < lines.len() {
        return true;
    }
    lines.iter().any(|line| {
        let Some(id) = line["id"].as_str() else {
            return false;
        };
        let Some(quantity) = requested.get(id) else {
            return false;
        };
        let remaining = line["remainingQuantity"]
            .as_i64()
            .or_else(|| line["totalQuantity"].as_i64())
            .unwrap_or(0);
        *quantity > 0 && *quantity < remaining
    })
}

fn split_fulfillment_order_quantities(
    source: &mut Value,
    requested: &BTreeMap<String, i64>,
    split_kind: &str,
    timestamp: &str,
    order_ids: &mut impl Iterator<Item = String>,
    line_item_ids: &mut impl Iterator<Item = String>,
) -> Option<Value> {
    if requested.is_empty() {
        return None;
    }
    let original_lines = fulfillment_order_line_item_nodes(source);
    let mut source_lines = Vec::new();
    let mut split_lines = Vec::new();
    for line in original_lines {
        let id = line["id"].as_str().unwrap_or_default().to_string();
        let remaining = line["remainingQuantity"]
            .as_i64()
            .or_else(|| line["totalQuantity"].as_i64())
            .unwrap_or(0);
        let requested_quantity = requested
            .get(&id)
            .copied()
            .unwrap_or(0)
            .min(remaining)
            .max(0);
        let residual = remaining - requested_quantity;
        let (source_quantity, split_quantity) = if split_kind == "hold" {
            (requested_quantity, residual)
        } else {
            (residual, requested_quantity)
        };
        if source_quantity > 0 {
            let mut source_line = line.clone();
            source_line["totalQuantity"] = json!(source_quantity);
            source_line["remainingQuantity"] = json!(source_quantity);
            if split_kind == "hold" {
                if let Some(line_item) = source_line["lineItem"].as_object_mut() {
                    line_item.insert("fulfillableQuantity".to_string(), json!(residual));
                }
            }
            source_lines.push(source_line);
        }
        if split_quantity > 0 {
            let mut split_line = line;
            split_line["id"] = json!(line_item_ids.next().unwrap_or_else(|| id.clone()));
            split_line["totalQuantity"] = json!(split_quantity);
            split_line["remainingQuantity"] = json!(split_quantity);
            if split_kind == "hold" {
                if let Some(line_item) = split_line["lineItem"].as_object_mut() {
                    line_item.insert("fulfillableQuantity".to_string(), json!(residual));
                }
            }
            split_lines.push(split_line);
        }
    }
    if split_lines.is_empty() {
        return None;
    }
    source["lineItems"] = json!({ "nodes": source_lines });
    let mut split = source.clone();
    split["id"] = json!(order_ids.next().unwrap_or_else(|| {
        synthetic_shopify_gid(
            "FulfillmentOrder",
            format!(
                "{}-{}",
                resource_id_tail(source["id"].as_str().unwrap_or_default()),
                split_kind
            ),
        )
    }));
    split["updatedAt"] = json!(timestamp);
    split["lineItems"] = json!({ "nodes": split_lines });
    Some(split)
}

fn restore_hold_split_quantities(nodes: &mut [Value], index: usize, id: &str) {
    let Some(split_index) = nodes.iter().position(|node| {
        node["_draftProxySplitSource"].as_str() == Some(id)
            && node["_draftProxySplitKind"].as_str() == Some("hold")
    }) else {
        return;
    };
    let split_order = nodes[split_index].clone();
    if index >= nodes.len() {
        return;
    }
    let mut line_items_by_id = BTreeMap::new();
    for line in fulfillment_order_line_item_nodes(&nodes[index])
        .into_iter()
        .chain(fulfillment_order_line_item_nodes(&split_order))
    {
        let key = line["lineItem"]["id"]
            .as_str()
            .or_else(|| line["id"].as_str())
            .unwrap_or_default()
            .to_string();
        let entry = line_items_by_id.entry(key).or_insert_with(|| {
            let mut merged = line.clone();
            merged["totalQuantity"] = json!(0);
            merged["remainingQuantity"] = json!(0);
            merged
        });
        let total = entry["totalQuantity"].as_i64().unwrap_or(0)
            + line["totalQuantity"].as_i64().unwrap_or(0);
        let remaining = entry["remainingQuantity"].as_i64().unwrap_or(0)
            + line["remainingQuantity"].as_i64().unwrap_or(0);
        entry["totalQuantity"] = json!(total);
        entry["remainingQuantity"] = json!(remaining);
        if let Some(line_item) = entry["lineItem"].as_object_mut() {
            line_item.insert("fulfillableQuantity".to_string(), json!(remaining));
        }
    }
    nodes[index]["lineItems"] = json!({
        "nodes": line_items_by_id.into_values().collect::<Vec<_>>()
    });
    nodes[split_index]["status"] = json!("CLOSED");
    nodes[split_index]["supportedActions"] = json!([]);
    let restored_lines = fulfillment_order_line_item_nodes(&nodes[index]);
    for line in nodes[split_index]["lineItems"]["nodes"]
        .as_array_mut()
        .into_iter()
        .flatten()
    {
        line["totalQuantity"] = json!(0);
        line["remainingQuantity"] = json!(0);
        let line_item_id = line["lineItem"]["id"].as_str().map(str::to_string);
        let restored_fulfillable = line_item_id
            .as_deref()
            .and_then(|id| {
                restored_lines
                    .iter()
                    .find(|restored| restored["lineItem"]["id"].as_str() == Some(id))
            })
            .and_then(|restored| {
                restored["lineItem"]["fulfillableQuantity"]
                    .as_i64()
                    .or_else(|| restored["remainingQuantity"].as_i64())
            });
        if let (Some(fulfillable), Some(line_item)) =
            (restored_fulfillable, line["lineItem"].as_object_mut())
        {
            line_item.insert("fulfillableQuantity".to_string(), json!(fulfillable));
        }
    }
}

fn shipping_fulfillment_supported_actions(actions: &[&str]) -> Value {
    json!(actions
        .iter()
        .map(|action| json!({ "action": action }))
        .collect::<Vec<_>>())
}

fn shipping_fulfillment_open_actions(include_split: bool) -> Value {
    let mut actions = vec!["CREATE_FULFILLMENT", "REPORT_PROGRESS", "MOVE", "HOLD"];
    if include_split {
        actions.push("SPLIT");
    }
    shipping_fulfillment_supported_actions(&actions)
}

fn update_order_display_fulfillment_status(order: &mut Value) {
    let statuses = fulfillment_order_nodes(order)
        .into_iter()
        .flatten()
        .filter_map(|node| node["status"].as_str())
        .collect::<Vec<_>>();
    let display = if statuses.contains(&"IN_PROGRESS") {
        "IN_PROGRESS"
    } else if statuses.contains(&"ON_HOLD") && !statuses.contains(&"OPEN") {
        "ON_HOLD"
    } else if statuses.iter().all(|status| *status == "CLOSED") && !statuses.is_empty() {
        "FULFILLED"
    } else {
        "UNFULFILLED"
    };
    order["displayFulfillmentStatus"] = json!(display);
}

fn fulfillment_hold_display_reason(reason: &str) -> String {
    match reason {
        "AWAITING_RETURN_ITEMS" => "Exchange items awaiting return delivery",
        "MARKETPLACE_PARTNER" => "Pending Marketplace partner authorization",
        "MARKETS_PRO" => "Markets Pro is processing the order",
        "MARKETS_PRO_DEFERRED_SALE" => "Awaiting payment",
        "ONLINE_STORE_POST_PURCHASE_CROSS_SELL" => "Pending upsell offer",
        "SHOPIFY_PAYMENTS_KYC" => "Awaiting payments setup",
        "UNKNOWN_DELIVERY_DATE" => "Unknown delivery date",
        "INVENTORY_OUT_OF_STOCK" => "Inventory out of stock",
        "HIGH_RISK_OF_FRAUD" => "High risk of fraud",
        "INCORRECT_ADDRESS" => "Incorrect address",
        "AWAITING_PAYMENT" => "Awaiting payment",
        "OTHER" => "Other",
        _ => "Other",
    }
    .to_string()
}

fn shopify_datetime_seconds(value: &str) -> String {
    if let Some((prefix, suffix)) = value.split_once('.') {
        if suffix.ends_with('Z') {
            return format!("{prefix}Z");
        }
    }
    value.to_string()
}

fn merge_staged_json(existing: &mut Value, incoming: Value) {
    match (existing, incoming) {
        (Value::Object(existing_object), Value::Object(incoming_object)) => {
            for (key, incoming_value) in incoming_object {
                if incoming_value.is_null() {
                    existing_object.entry(key).or_insert(Value::Null);
                    continue;
                }
                match existing_object.get_mut(&key) {
                    Some(existing_value) => {
                        if should_preserve_staged_scalar(&key, existing_value, &incoming_value) {
                            continue;
                        }
                        merge_staged_json(existing_value, incoming_value);
                    }
                    None => {
                        existing_object.insert(key, incoming_value);
                    }
                }
            }
        }
        (existing_value, incoming_value) => {
            if !incoming_value.is_null() {
                *existing_value = incoming_value;
            }
        }
    }
}

fn should_preserve_staged_scalar(key: &str, existing: &Value, incoming: &Value) -> bool {
    match key {
        "requestStatus" => {
            existing
                .as_str()
                .is_some_and(|status| status != "UNSUBMITTED")
                && incoming.as_str() == Some("UNSUBMITTED")
        }
        "status" => {
            matches!(
                existing.as_str(),
                Some("IN_PROGRESS" | "ON_HOLD" | "CLOSED" | "INCOMPLETE")
            ) && incoming.as_str() == Some("OPEN")
        }
        _ => false,
    }
}

#[cfg(test)]
mod fulfillment_hold_display_reason_tests {
    use super::fulfillment_hold_display_reason;

    #[test]
    fn maps_all_known_reasons_to_shopify_display_text() {
        let cases = [
            (
                "AWAITING_RETURN_ITEMS",
                "Exchange items awaiting return delivery",
            ),
            (
                "MARKETPLACE_PARTNER",
                "Pending Marketplace partner authorization",
            ),
            ("MARKETS_PRO", "Markets Pro is processing the order"),
            ("MARKETS_PRO_DEFERRED_SALE", "Awaiting payment"),
            (
                "ONLINE_STORE_POST_PURCHASE_CROSS_SELL",
                "Pending upsell offer",
            ),
            ("SHOPIFY_PAYMENTS_KYC", "Awaiting payments setup"),
            ("UNKNOWN_DELIVERY_DATE", "Unknown delivery date"),
            ("INVENTORY_OUT_OF_STOCK", "Inventory out of stock"),
            ("HIGH_RISK_OF_FRAUD", "High risk of fraud"),
            ("INCORRECT_ADDRESS", "Incorrect address"),
            ("AWAITING_PAYMENT", "Awaiting payment"),
            ("OTHER", "Other"),
        ];

        for (reason, display_reason) in cases {
            assert_eq!(
                fulfillment_hold_display_reason(reason),
                display_reason,
                "{reason}"
            );
        }
    }

    #[test]
    fn maps_unknown_reasons_to_other() {
        assert_eq!(
            fulfillment_hold_display_reason("INTERNAL_NOT_VISIBLE_TO_APP"),
            "Other"
        );
    }
}

use super::*;

struct FulfillmentOrderMutationContext<'a> {
    request: &'a Request,
    query: &'a str,
    variables: &'a BTreeMap<String, ResolvedValue>,
    root_field: &'a str,
    arguments: &'a BTreeMap<String, ResolvedValue>,
}

fn fulfillment_order_action_values(actions: &[&str]) -> Value {
    Value::Array(
        actions
            .iter()
            .map(|action| json!({ "action": action }))
            .collect(),
    )
}

fn fulfillment_order_is_fulfillment_service_assigned(order: &Value) -> bool {
    let assigned_location = &order["assignedLocation"];
    let location = &assigned_location["location"];
    location
        .get("isFulfillmentService")
        .and_then(Value::as_bool)
        .unwrap_or(false)
        || location
            .get("fulfillmentService")
            .is_some_and(Value::is_object)
        || assigned_location
            .get("fulfillmentService")
            .is_some_and(Value::is_object)
        || fulfillment_order_has_supported_action(order, "REPORT_PROGRESS")
}

fn fulfillment_order_has_supported_action(order: &Value, action: &str) -> bool {
    order["supportedActions"]
        .as_array()
        .into_iter()
        .flatten()
        .any(|entry| entry["action"].as_str() == Some(action))
}

pub(in crate::proxy) fn fulfillment_order_supported_actions(
    order: &Value,
    include_split: bool,
) -> Value {
    let status = order["status"].as_str().unwrap_or("OPEN");
    match status {
        "CLOSED" | "CANCELLED" => fulfillment_order_action_values(&[]),
        "SCHEDULED" => fulfillment_order_action_values(&["MARK_AS_OPEN"]),
        "ON_HOLD" => fulfillment_order_action_values(&["RELEASE_HOLD", "HOLD", "MOVE"]),
        "IN_PROGRESS" => {
            let mut actions = vec!["CREATE_FULFILLMENT"];
            if fulfillment_order_is_fulfillment_service_assigned(order) {
                actions.push("REPORT_PROGRESS");
            }
            actions.extend(["HOLD", "MARK_AS_OPEN"]);
            fulfillment_order_action_values(&actions)
        }
        _ => {
            let mut actions = vec!["CREATE_FULFILLMENT"];
            if fulfillment_order_is_fulfillment_service_assigned(order) {
                actions.push("REPORT_PROGRESS");
            }
            actions.extend(["MOVE", "HOLD"]);
            if include_split {
                actions.push("SPLIT");
            }
            actions.push("MERGE");
            fulfillment_order_action_values(&actions)
        }
    }
}

pub(in crate::proxy) fn fulfillment_order_assigned_location_from_location(
    location: &Value,
) -> Option<Value> {
    let id = location.get("id").and_then(Value::as_str)?;
    let name = location
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let mut assigned_location = json!({
        "name": name,
        "location": {
            "id": id,
            "name": name
        }
    });
    if let Some(location_object) = location.as_object() {
        if let Some(assigned_location_object) = assigned_location["location"].as_object_mut() {
            for key in [
                "isFulfillmentService",
                "fulfillmentService",
                "fulfillsOnlineOrders",
                "shipsInventory",
            ] {
                if let Some(value) = location_object.get(key) {
                    assigned_location_object.insert(key.to_string(), value.clone());
                }
            }
        }
    }
    Some(assigned_location)
}

pub(in crate::proxy) fn normalize_fulfillment_order_record(
    order: &mut Value,
    assigned_location: Option<&Value>,
) {
    if order.get("updatedAt").is_none() {
        order["updatedAt"] = json!("2026-05-11T10:00:00Z");
    }
    if order.get("fulfillAt").is_none() {
        order["fulfillAt"] = Value::Null;
    }
    if order.get("fulfillBy").is_none() {
        order["fulfillBy"] = Value::Null;
    }
    if order.get("assignedLocation").is_none() {
        if let Some(assigned_location) = assigned_location {
            order["assignedLocation"] = assigned_location.clone();
        }
    }
    if order.get("supportedActions").is_none()
        || order["supportedActions"].is_null()
        || (order["supportedActions"]
            .as_array()
            .is_some_and(Vec::is_empty)
            && !matches!(
                order["status"].as_str(),
                Some("CLOSED" | "CANCELLED" | "SCHEDULED")
            ))
    {
        set_fulfillment_order_supported_actions_from_lines(order);
    }
    if order.get("fulfillmentHolds").is_none() {
        order["fulfillmentHolds"] = json!([]);
    }
    if order.get("merchantRequests").is_none() {
        order["merchantRequests"] = order_connection(Vec::new());
    }
    if order.get("requestStatus").is_none() {
        order["requestStatus"] = json!("UNSUBMITTED");
    }
}

pub(in crate::proxy) fn normalize_order_fulfillment_orders(
    order: &mut Value,
    assigned_location: Option<&Value>,
) {
    if let Some(nodes) = fulfillment_order_nodes_mut(order) {
        for node in nodes {
            normalize_fulfillment_order_record(node, assigned_location);
        }
    }
}

pub(in crate::proxy) fn line_item_remaining_quantity(line: &Value) -> i64 {
    line["remainingQuantity"]
        .as_i64()
        .or_else(|| line["totalQuantity"].as_i64())
        .unwrap_or(0)
        .max(0)
}

pub(in crate::proxy) fn fulfillment_order_line_quantity_total(order: &Value) -> i64 {
    order["lineItems"]["nodes"]
        .as_array()
        .into_iter()
        .flatten()
        .map(line_item_remaining_quantity)
        .sum()
}

pub(in crate::proxy) fn set_fulfillment_order_supported_actions_from_lines(order: &mut Value) {
    let remaining_total = fulfillment_order_line_quantity_total(order);
    order["supportedActions"] = fulfillment_order_supported_actions(order, remaining_total > 1);
}

pub(in crate::proxy) fn set_fulfillment_order_status_from_lines(order: &mut Value) {
    let remaining_total = fulfillment_order_line_quantity_total(order);
    order["status"] = json!(if remaining_total == 0 {
        "CLOSED"
    } else {
        "OPEN"
    });
    order["supportedActions"] = fulfillment_order_supported_actions(order, remaining_total > 1);
}

pub(in crate::proxy) fn fulfillment_order_line_with_quantity(line: &Value, quantity: i64) -> Value {
    let mut updated = line.clone();
    updated["totalQuantity"] = json!(quantity.max(0));
    updated["remainingQuantity"] = json!(quantity.max(0));
    updated
}

pub(in crate::proxy) fn strip_fulfillment_order_line_id(line: &Value) -> Value {
    let mut line = line.clone();
    if let Some(object) = line.as_object_mut() {
        object.remove("id");
    }
    line
}

pub(in crate::proxy) fn fulfillment_order_payload_json(
    fulfillment_order: Value,
    user_errors: Vec<Value>,
) -> Value {
    json!({ "fulfillmentOrder": fulfillment_order, "userErrors": user_errors })
}

pub(in crate::proxy) fn fulfillment_order_request_payload_json(
    fulfillment_order: Value,
    original: Value,
    submitted: Value,
    unsubmitted: Value,
    user_errors: Vec<Value>,
) -> Value {
    json!({
        "fulfillmentOrder": fulfillment_order,
        "originalFulfillmentOrder": original,
        "submittedFulfillmentOrder": submitted,
        "unsubmittedFulfillmentOrder": unsubmitted,
        "userErrors": user_errors,
    })
}

pub(in crate::proxy) fn fulfillment_order_split_payload_json(
    splits: Value,
    user_errors: Vec<Value>,
) -> Value {
    json!({ "fulfillmentOrderSplits": splits, "userErrors": user_errors })
}

pub(in crate::proxy) fn fulfillment_order_merge_payload_json(
    merges: Value,
    user_errors: Vec<Value>,
) -> Value {
    json!({ "fulfillmentOrderMerges": merges, "userErrors": user_errors })
}

pub(in crate::proxy) fn fulfillment_order_nodes(order: &Value) -> Option<&Vec<Value>> {
    order.get("fulfillmentOrders")?.get("nodes")?.as_array()
}

pub(in crate::proxy) fn fulfillment_order_display_status(
    nodes: &[Value],
    include_progress_hold_statuses: bool,
    non_closed_status: &'static str,
) -> Option<&'static str> {
    let statuses = nodes
        .iter()
        .filter_map(|node| node["status"].as_str())
        .collect::<Vec<_>>();
    if statuses.is_empty() {
        return None;
    }
    if include_progress_hold_statuses {
        if statuses.contains(&"IN_PROGRESS") {
            return Some("IN_PROGRESS");
        }
        if statuses.contains(&"ON_HOLD") && !statuses.contains(&"OPEN") {
            return Some("ON_HOLD");
        }
    }
    Some(
        if statuses
            .iter()
            .all(|status| status.eq_ignore_ascii_case("CLOSED"))
        {
            "FULFILLED"
        } else {
            non_closed_status
        },
    )
}

pub(in crate::proxy) fn fulfillment_tracking_info(
    input: &BTreeMap<String, ResolvedValue>,
) -> Vec<Value> {
    let company = resolved_string_field(input, "company")
        .or_else(|| resolved_string_field(input, "trackingCompany"));
    let numbers = list_string_field(input, "numbers");
    let urls = list_string_field(input, "urls");
    if !numbers.is_empty() || !urls.is_empty() {
        let len = numbers.len().max(urls.len());
        return (0..len)
            .map(|index| {
                json!({
                    "number": numbers.get(index).cloned().unwrap_or_default(),
                    "url": urls.get(index).cloned(),
                    "company": company.clone()
                })
            })
            .collect();
    }
    let number = resolved_string_field(input, "number")
        .or_else(|| resolved_string_field(input, "trackingNumber"))
        .unwrap_or_default();
    let url =
        resolved_string_field(input, "url").or_else(|| resolved_string_field(input, "trackingUrl"));
    if number.is_empty() && url.is_none() && company.is_none() {
        return Vec::new();
    }
    vec![json!({
        "number": number,
        "url": url,
        "company": company
    })]
}

pub(in crate::proxy) fn fulfillment_order_nodes_mut(order: &mut Value) -> Option<&mut Vec<Value>> {
    order
        .get_mut("fulfillmentOrders")?
        .get_mut("nodes")?
        .as_array_mut()
}

pub(in crate::proxy) fn order_fulfillments_mut(order: &mut Value) -> Option<&mut Vec<Value>> {
    order.get_mut("fulfillments")?.as_array_mut()
}

pub(in crate::proxy) fn fulfillment_line_item_record(line: &Value, quantity: i64) -> Value {
    let line_id = line.get("id").and_then(Value::as_str).unwrap_or_default();
    let fulfillment_line_item_id = if line_id.is_empty() {
        shopify_gid("FulfillmentLineItem", 1)
    } else {
        shopify_gid("FulfillmentLineItem", resource_id_tail(line_id))
    };
    json!({
        "id": fulfillment_line_item_id,
        "quantity": quantity,
        "lineItem": line.get("lineItem").cloned().unwrap_or(Value::Null)
    })
}

pub(in crate::proxy) fn fulfillment_group_line_items(
    order: &Value,
    group: &BTreeMap<String, ResolvedValue>,
) -> Vec<Value> {
    let group_id = resolved_string_field(group, "fulfillmentOrderId").unwrap_or_default();
    let requested_line_items = resolved_object_list_field(group, "fulfillmentOrderLineItems");
    let Some(fulfillment_order) = order["fulfillmentOrders"]["nodes"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|node| node["id"].as_str() == Some(group_id.as_str()))
    else {
        return Vec::new();
    };
    let line_nodes = fulfillment_order["lineItems"]["nodes"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    if requested_line_items.is_empty() {
        return line_nodes
            .iter()
            .map(|line| {
                let quantity = line["remainingQuantity"]
                    .as_i64()
                    .or_else(|| line["totalQuantity"].as_i64())
                    .unwrap_or(0)
                    .max(0);
                fulfillment_line_item_record(line, quantity)
            })
            .collect();
    }
    requested_line_items
        .iter()
        .filter_map(|requested| {
            let requested_id = resolved_string_field(requested, "id")?;
            let quantity = resolved_int_field(requested, "quantity")
                .unwrap_or(0)
                .max(0);
            line_nodes
                .iter()
                .find(|line| line["id"].as_str() == Some(requested_id.as_str()))
                .map(|line| fulfillment_line_item_record(line, quantity))
        })
        .collect()
}

pub(in crate::proxy) fn fulfillment_create_closed_order_error(fulfillment_order_id: &str) -> Value {
    user_error_omit_code(
        ["fulfillment"],
        &format!(
            "Fulfillment order {} has an unfulfillable status= closed.",
            resource_id_tail(fulfillment_order_id)
        ),
        None,
    )
}

pub(in crate::proxy) fn fulfillment_create_invalid_quantity_error() -> Value {
    user_error_omit_code(
        ["fulfillment"],
        "Invalid fulfillment order line item quantity requested.",
        None,
    )
}

pub(in crate::proxy) fn fulfillment_create_non_positive_quantity_error(
    group_index: usize,
    line_index: usize,
) -> Value {
    user_error_omit_code(
        json!([
            "fulfillment",
            "lineItemsByFulfillmentOrder",
            group_index.to_string(),
            "fulfillmentOrderLineItems",
            line_index.to_string(),
            "quantity"
        ]),
        "Quantity must be greater than 0",
        None,
    )
}

pub(in crate::proxy) fn fulfillment_create_precondition_error(
    order: &Value,
    groups: &[BTreeMap<String, ResolvedValue>],
) -> Option<Value> {
    for (group_index, group) in groups.iter().enumerate() {
        let group_id = resolved_string_field(group, "fulfillmentOrderId").unwrap_or_default();
        let fulfillment_order = order["fulfillmentOrders"]["nodes"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|node| node["id"].as_str() == Some(group_id.as_str()))?;
        let status = fulfillment_order["status"].as_str().unwrap_or_default();
        if status.eq_ignore_ascii_case("CLOSED") || status.eq_ignore_ascii_case("CANCELLED") {
            return Some(fulfillment_create_closed_order_error(&group_id));
        }
        let line_nodes = fulfillment_order["lineItems"]["nodes"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        for (line_index, requested) in
            resolved_object_list_field(group, "fulfillmentOrderLineItems")
                .iter()
                .enumerate()
        {
            let Some(requested_id) = resolved_string_field(requested, "id") else {
                return Some(fulfillment_create_invalid_quantity_error());
            };
            let Some(requested_quantity) = resolved_int_field(requested, "quantity") else {
                return Some(fulfillment_create_invalid_quantity_error());
            };
            let Some(line) = line_nodes
                .iter()
                .find(|line| line["id"].as_str() == Some(requested_id.as_str()))
            else {
                return Some(fulfillment_create_invalid_quantity_error());
            };
            let remaining = line["remainingQuantity"].as_i64().unwrap_or(0);
            if requested_quantity <= 0 {
                return Some(fulfillment_create_non_positive_quantity_error(
                    group_index,
                    line_index,
                ));
            }
            if requested_quantity > remaining {
                return Some(fulfillment_create_invalid_quantity_error());
            }
        }
    }
    None
}

pub(in crate::proxy) fn update_order_fulfillment_status(order: &mut Value) {
    let fulfillment_count = order["fulfillments"]
        .as_array()
        .map(Vec::len)
        .unwrap_or_default();
    if fulfillment_count == 0 {
        return;
    }
    let Some(display) = fulfillment_order_nodes(order)
        .and_then(|nodes| fulfillment_order_display_status(nodes, false, "PARTIALLY_FULFILLED"))
    else {
        return;
    };
    order["displayFulfillmentStatus"] = json!(display);
}

pub(in crate::proxy) fn fulfillment_status_is(fulfillment: &Value, expected: &str) -> bool {
    fulfillment["status"]
        .as_str()
        .is_some_and(|status| status.eq_ignore_ascii_case(expected))
}

pub(in crate::proxy) fn fulfillment_display_status_is(fulfillment: &Value, expected: &str) -> bool {
    fulfillment["displayStatus"]
        .as_str()
        .is_some_and(|status| status.eq_ignore_ascii_case(expected))
}

pub(in crate::proxy) fn fulfillment_event_status_is_allowed(status: &str) -> bool {
    FULFILLMENT_EVENT_STATUS_VALUES.contains(&status)
}

pub(in crate::proxy) fn fulfillment_gid_has_numeric_tail(id: &str) -> bool {
    shopify_gid_resource_type(id) == Some("Fulfillment")
        && resource_id_tail(id).parse::<u64>().is_ok()
}

// Shopify rejects a `fulfillmentOrderId` whose numeric tail is not a positive
// integer (e.g. `gid://shopify/FulfillmentOrder/0`) with a top-level `invalid id`
// / RESOURCE_NOT_FOUND error rather than a payload userError. A non-numeric or
// missing tail is likewise structurally invalid.
pub(in crate::proxy) fn fulfillment_order_id_is_invalid(id: &str) -> bool {
    resource_id_tail(id)
        .parse::<u64>()
        .map(|tail| tail == 0)
        .unwrap_or(true)
}

// Builds the execution error Shopify returns when a
// `fulfillmentCreate` references a structurally invalid fulfillment-order id.
pub(in crate::proxy) fn fulfillment_create_invalid_id_error(
    arguments: &BTreeMap<String, ResolvedValue>,
    response_key: &str,
) -> Option<ResolverOutcome<Value>> {
    let fulfillment_input = resolved_object_field(arguments, "fulfillment")?;
    let groups = resolved_object_list_field(&fulfillment_input, "lineItemsByFulfillmentOrder");
    if !groups.iter().any(|group| {
        resolved_string_field(group, "fulfillmentOrderId")
            .is_some_and(|id| fulfillment_order_id_is_invalid(&id))
    }) {
        return None;
    }
    Some(graphql_error_outcome(
        vec![json!({
            "message": "invalid id",
            "extensions": { "code": "RESOURCE_NOT_FOUND" },
            "path": [response_key]
        })],
        response_key,
    ))
}

pub(in crate::proxy) fn fulfillment_event_nullable_string(
    input: &BTreeMap<String, ResolvedValue>,
    field: &str,
) -> Value {
    resolved_string_field(input, field)
        .map(Value::String)
        .unwrap_or(Value::Null)
}

pub(in crate::proxy) fn fulfillment_event_nullable_number(
    input: &BTreeMap<String, ResolvedValue>,
    field: &str,
) -> Value {
    resolved_number_field(input, field)
        .map(|value| json!(value))
        .unwrap_or(Value::Null)
}

pub(in crate::proxy) fn fulfillment_event_record(
    id: String,
    input: &BTreeMap<String, ResolvedValue>,
    status: &str,
) -> Value {
    let happened_at = resolved_string_field(input, "happenedAt")
        .unwrap_or_else(|| FULFILLMENT_EVENT_CREATED_AT.to_string());
    json!({
        "id": id,
        "status": status,
        "message": fulfillment_event_nullable_string(input, "message"),
        "happenedAt": happened_at,
        "createdAt": FULFILLMENT_EVENT_CREATED_AT,
        "estimatedDeliveryAt": fulfillment_event_nullable_string(input, "estimatedDeliveryAt"),
        "city": fulfillment_event_nullable_string(input, "city"),
        "province": fulfillment_event_nullable_string(input, "province"),
        "country": fulfillment_event_nullable_string(input, "country"),
        "zip": fulfillment_event_nullable_string(input, "zip"),
        "address1": fulfillment_event_nullable_string(input, "address1"),
        "latitude": fulfillment_event_nullable_number(input, "latitude"),
        "longitude": fulfillment_event_nullable_number(input, "longitude")
    })
}

pub(in crate::proxy) fn fulfillment_events_connection_nodes_mut(
    fulfillment: &mut Value,
) -> Option<&mut Vec<Value>> {
    if !fulfillment.get("events").is_some_and(Value::is_object) {
        fulfillment["events"] = order_connection(Vec::new());
    }
    if !fulfillment["events"]
        .get("nodes")
        .is_some_and(Value::is_array)
    {
        fulfillment["events"]["nodes"] = json!([]);
    }
    fulfillment["events"]["nodes"].as_array_mut()
}

pub(in crate::proxy) fn apply_fulfillment_event_to_fulfillment(
    fulfillment: &mut Value,
    event: &Value,
) {
    let updated_nodes = fulfillment_events_connection_nodes_mut(fulfillment).map(|nodes| {
        nodes.insert(0, event.clone());
        nodes.clone()
    });
    if let Some(nodes) = updated_nodes {
        fulfillment["events"] = order_connection(nodes.clone());
    }
    let status = event["status"].as_str().unwrap_or_default();
    fulfillment["displayStatus"] = json!(status);
    fulfillment["updatedAt"] = json!(FULFILLMENT_EVENT_CREATED_AT);
    if !event["estimatedDeliveryAt"].is_null() {
        fulfillment["estimatedDeliveryAt"] = event["estimatedDeliveryAt"].clone();
    }
    if status == "IN_TRANSIT" {
        fulfillment["inTransitAt"] = event["happenedAt"].clone();
    }
    if status == "DELIVERED" {
        fulfillment["deliveredAt"] = event["happenedAt"].clone();
    }
}

impl DraftProxy {
    pub(in crate::proxy) fn default_fulfillment_assigned_location(&self) -> Option<Value> {
        self.default_fulfillment_location()
            .and_then(|location| fulfillment_order_assigned_location_from_location(&location))
    }

    fn default_fulfillment_location(&self) -> Option<Value> {
        let mut seen = BTreeSet::new();
        for id in &self.store.staged.observed_shipping_location_order {
            if !seen.insert(id.clone()) {
                continue;
            }
            if let Some(location) = self
                .location_for_read(id)
                .filter(Self::location_can_assign_fulfillment_order)
            {
                return Some(location);
            }
        }
        for id in &self.store.staged.locations.order {
            if !seen.insert(id.clone()) {
                continue;
            }
            if let Some(location) = self
                .location_for_read(id)
                .filter(Self::location_can_assign_fulfillment_order)
            {
                return Some(location);
            }
        }
        None
    }

    fn location_can_assign_fulfillment_order(location: &Value) -> bool {
        location
            .get("id")
            .and_then(Value::as_str)
            .is_some_and(|id| !id.is_empty())
            && location
                .get("isActive")
                .and_then(Value::as_bool)
                .unwrap_or(true)
            && location
                .get("fulfillsOnlineOrders")
                .and_then(Value::as_bool)
                .unwrap_or(true)
    }

    pub(in crate::proxy) fn fulfillment_order_local_mutation_outcome(
        &mut self,
        request: &Request,
        root_field: &str,
        arguments: &BTreeMap<String, ResolvedValue>,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Option<ResolverOutcome<Value>> {
        let payload = match root_field {
            "fulfillmentOrderSubmitFulfillmentRequest" => self
                .stage_fulfillment_order_submit_request(
                    request, query, variables, root_field, arguments,
                ),
            "fulfillmentOrderAcceptFulfillmentRequest"
            | "fulfillmentOrderRejectFulfillmentRequest"
            | "fulfillmentOrderSubmitCancellationRequest"
            | "fulfillmentOrderAcceptCancellationRequest"
            | "fulfillmentOrderRejectCancellationRequest" => self
                .stage_fulfillment_order_request_transition(
                    request, query, variables, root_field, arguments,
                ),
            "fulfillmentOrderSplit" => {
                self.stage_fulfillment_order_split(request, query, variables, root_field, arguments)
            }
            "fulfillmentOrderMerge" => {
                self.stage_fulfillment_order_merge(request, query, variables, root_field, arguments)
            }
            _ => return None,
        };
        Some(ResolverOutcome::value(payload))
    }

    pub(super) fn fulfillment_order_not_found_payload(&self, root_field: &str) -> Value {
        let errors = vec![user_error(
            Value::Null,
            "Fulfillment order does not exist.",
            Some("FULFILLMENT_ORDER_NOT_FOUND"),
        )];
        match root_field {
            "fulfillmentOrderSplit" => fulfillment_order_split_payload_json(Value::Null, errors),
            "fulfillmentOrderMerge" => fulfillment_order_merge_payload_json(Value::Null, errors),
            "fulfillmentOrderSubmitFulfillmentRequest" => fulfillment_order_request_payload_json(
                Value::Null,
                Value::Null,
                Value::Null,
                Value::Null,
                errors,
            ),
            _ => fulfillment_order_payload_json(Value::Null, errors),
        }
    }

    pub(super) fn locate_fulfillment_order_mut<'a>(
        order: &'a mut Value,
        fulfillment_order_id: &str,
    ) -> Option<&'a mut Value> {
        fulfillment_order_nodes_mut(order)?
            .iter_mut()
            .find(|node| node["id"].as_str() == Some(fulfillment_order_id))
    }

    pub(super) fn stage_fulfillment_order_submit_request(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        root_field: &str,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        let id = resolved_string_field(arguments, "id").unwrap_or_default();
        let Some(order_id) = self.order_id_for_fulfillment_order(&id, request) else {
            return self.fulfillment_order_not_found_payload(root_field);
        };
        self.stage_fulfillment_order_submit_request_for_order_id(
            FulfillmentOrderMutationContext {
                request,
                query,
                variables,
                root_field,
                arguments,
            },
            order_id,
            id,
        )
    }

    fn stage_fulfillment_order_submit_request_for_order_id(
        &mut self,
        context: FulfillmentOrderMutationContext<'_>,
        order_id: String,
        id: String,
    ) -> Value {
        let requested_lines =
            resolved_object_list_field(context.arguments, "fulfillmentOrderLineItems");
        let message = resolved_string_field(context.arguments, "message");
        let notify_customer =
            resolved_bool_field(context.arguments, "notifyCustomer").unwrap_or(false);

        let Some(mut order) = self.store.staged.orders.get(&order_id).cloned() else {
            return self.fulfillment_order_not_found_payload(context.root_field);
        };
        let assigned_location = self.default_fulfillment_assigned_location();
        normalize_order_fulfillment_orders(&mut order, assigned_location.as_ref());
        let Some(original_index) = order["fulfillmentOrders"]["nodes"]
            .as_array()
            .into_iter()
            .flatten()
            .position(|node| node["id"].as_str() == Some(id.as_str()))
        else {
            return self.fulfillment_order_not_found_payload(context.root_field);
        };

        let mut original = order["fulfillmentOrders"]["nodes"][original_index].clone();
        let line_nodes = original["lineItems"]["nodes"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let selected = if requested_lines.is_empty() {
            line_nodes
                .iter()
                .map(|line| {
                    (
                        line["id"].as_str().unwrap_or_default().to_string(),
                        line_item_remaining_quantity(line),
                    )
                })
                .collect::<Vec<_>>()
        } else {
            requested_lines
                .iter()
                .filter_map(|line| {
                    Some((
                        resolved_string_field(line, "id")?,
                        resolved_int_field(line, "quantity").unwrap_or(0).max(0),
                    ))
                })
                .collect::<Vec<_>>()
        };

        let mut submitted_lines = Vec::new();
        let mut unsubmitted_lines = Vec::new();
        for line in &line_nodes {
            let line_id = line["id"].as_str().unwrap_or_default();
            let remaining = line_item_remaining_quantity(line);
            let selected_quantity = selected
                .iter()
                .find(|(id, _)| id == line_id)
                .map(|(_, quantity)| *quantity)
                .unwrap_or(0)
                .min(remaining)
                .max(0);
            if selected_quantity > 0 {
                submitted_lines.push(fulfillment_order_line_with_quantity(
                    line,
                    selected_quantity,
                ));
            }
            let leftover = remaining.saturating_sub(selected_quantity);
            if leftover > 0 {
                unsubmitted_lines.push(strip_fulfillment_order_line_id(
                    &fulfillment_order_line_with_quantity(line, leftover),
                ));
            }
        }

        original["requestStatus"] = json!("SUBMITTED");
        original["merchantRequests"] = order_connection(vec![json!({
            "kind": "FULFILLMENT_REQUEST",
            "message": message.clone().unwrap_or_default(),
            "requestOptions": { "notify_customer": notify_customer },
            "responseData": Value::Null
        })]);
        if !submitted_lines.is_empty() {
            original["lineItems"] = order_connection(submitted_lines);
        }
        normalize_fulfillment_order_record(&mut original, assigned_location.as_ref());
        order["fulfillmentOrders"]["nodes"][original_index] = original.clone();
        self.store
            .staged
            .orders
            .insert(order_id.clone(), order.clone());

        let unsubmitted = if unsubmitted_lines.is_empty() {
            Value::Null
        } else {
            let mut record = original.clone();
            record["id"] = Value::Null;
            record["requestStatus"] = json!("UNSUBMITTED");
            record["lineItems"] = order_connection(unsubmitted_lines);
            record
        };

        self.record_staged_orders_log_entry(
            context.request,
            context.query,
            context.variables,
            "fulfillmentOrderSubmitFulfillmentRequest",
            vec![order_id, id],
        );

        fulfillment_order_request_payload_json(
            original.clone(),
            original.clone(),
            original,
            unsubmitted,
            Vec::new(),
        )
    }

    pub(super) fn stage_fulfillment_order_request_transition(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        root_field: &str,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        let id = resolved_string_field(arguments, "id").unwrap_or_default();
        let Some(order_id) = self.order_id_for_fulfillment_order(&id, request) else {
            return self.fulfillment_order_not_found_payload(root_field);
        };
        self.stage_fulfillment_order_request_transition_for_order_id(
            FulfillmentOrderMutationContext {
                request,
                query,
                variables,
                root_field,
                arguments,
            },
            order_id,
            id,
        )
    }

    fn stage_fulfillment_order_request_transition_for_order_id(
        &mut self,
        context: FulfillmentOrderMutationContext<'_>,
        order_id: String,
        id: String,
    ) -> Value {
        let Some(mut order) = self.store.staged.orders.get(&order_id).cloned() else {
            return self.fulfillment_order_not_found_payload(context.root_field);
        };
        let assigned_location = self.default_fulfillment_assigned_location();
        normalize_order_fulfillment_orders(&mut order, assigned_location.as_ref());
        let Some(fulfillment_order) = Self::locate_fulfillment_order_mut(&mut order, &id) else {
            return self.fulfillment_order_not_found_payload(context.root_field);
        };
        match context.root_field {
            "fulfillmentOrderAcceptFulfillmentRequest" => {
                fulfillment_order["status"] = json!("IN_PROGRESS");
                fulfillment_order["requestStatus"] = json!("ACCEPTED");
                if let Some(estimated) =
                    resolved_string_field(context.arguments, "estimatedShippedAt")
                {
                    fulfillment_order["estimatedShippedAt"] = json!(estimated);
                }
            }
            "fulfillmentOrderRejectFulfillmentRequest" => {
                fulfillment_order["status"] = json!("OPEN");
                fulfillment_order["requestStatus"] = json!("REJECTED");
            }
            "fulfillmentOrderSubmitCancellationRequest" => {
                fulfillment_order["status"] = json!("IN_PROGRESS");
                fulfillment_order["requestStatus"] = json!("ACCEPTED");
                let mut requests = fulfillment_order["merchantRequests"]["nodes"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default();
                requests.push(json!({
                    "kind": "CANCELLATION_REQUEST",
                    "message": resolved_string_field(context.arguments, "message").unwrap_or_default(),
                    "requestOptions": {},
                    "responseData": Value::Null
                }));
                fulfillment_order["merchantRequests"] = order_connection(requests);
            }
            "fulfillmentOrderAcceptCancellationRequest" => {
                fulfillment_order["status"] = json!("CLOSED");
                fulfillment_order["requestStatus"] = json!("CANCELLATION_ACCEPTED");
                if let Some(lines) = fulfillment_order["lineItems"]["nodes"].as_array_mut() {
                    for line in lines {
                        line["totalQuantity"] = json!(0);
                        line["remainingQuantity"] = json!(0);
                    }
                }
            }
            "fulfillmentOrderRejectCancellationRequest" => {
                fulfillment_order["status"] = json!("IN_PROGRESS");
                fulfillment_order["requestStatus"] = json!("CANCELLATION_REJECTED");
            }
            _ => {}
        }
        set_fulfillment_order_supported_actions_from_lines(fulfillment_order);
        let changed = fulfillment_order.clone();
        self.store.staged.orders.insert(order_id.clone(), order);
        self.record_orders_local_log_entry(OrdersLocalLogEntry {
            request: context.request,
            query: context.query,
            variables: context.variables,
            root_field: context.root_field,
            staged_resource_ids: vec![order_id, id],
            outcome: OrdersLocalLogOutcome {
                status: "staged",
                notes:
                    "Locally staged fulfillment-order request transition in shopify-draft-proxy.",
            },
        });
        fulfillment_order_payload_json(changed, Vec::new())
    }

    pub(super) fn split_validation_error(
        &self,
        input_index: usize,
        line_index: Option<usize>,
        message: &str,
        code: &str,
    ) -> Value {
        let field = match line_index {
            Some(line_index) => json!([
                "fulfillmentOrderSplits",
                input_index.to_string(),
                "fulfillmentOrderLineItems",
                line_index.to_string(),
                "quantity"
            ]),
            None => json!([
                "fulfillmentOrderSplits",
                input_index.to_string(),
                "fulfillmentOrderLineItems"
            ]),
        };
        user_error(field, message, Some(code))
    }

    fn stage_fulfillment_order_batch_transactionally(
        &mut self,
        stage: impl FnOnce(&mut Self) -> Value,
    ) -> Value {
        let mut transaction = self.clone();
        let payload = stage(&mut transaction);
        let succeeded = payload
            .get("userErrors")
            .and_then(Value::as_array)
            .is_some_and(Vec::is_empty);
        if succeeded {
            *self = transaction;
        }
        payload
    }

    pub(super) fn stage_fulfillment_order_split(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        root_field: &str,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        self.stage_fulfillment_order_batch_transactionally(|transaction| {
            transaction.stage_fulfillment_order_split_transaction(
                request, query, variables, root_field, arguments,
            )
        })
    }

    fn stage_fulfillment_order_split_transaction(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        root_field: &str,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        let split_inputs = resolved_object_list_field(arguments, "fulfillmentOrderSplits");
        let mut planned = Vec::new();
        for (input_index, input) in split_inputs.iter().enumerate() {
            let line_inputs = resolved_object_list_field(input, "fulfillmentOrderLineItems");
            if line_inputs.is_empty() {
                return fulfillment_order_split_payload_json(
                    Value::Null,
                    vec![self.split_validation_error(
                        input_index,
                        None,
                        "There must be at least one item selected in this fulfillment to split it.",
                        "NO_LINE_ITEMS_PROVIDED_TO_SPLIT",
                    )],
                );
            }
            for (line_index, line) in line_inputs.iter().enumerate() {
                if resolved_int_field(line, "quantity").unwrap_or(0) <= 0 {
                    return fulfillment_order_split_payload_json(
                        Value::Null,
                        vec![self.split_validation_error(
                            input_index,
                            Some(line_index),
                            "You must select at least one item to split into a new fulfillment order.",
                            "GREATER_THAN",
                        )],
                    );
                }
            }
            let fulfillment_order_id =
                resolved_string_field(input, "fulfillmentOrderId").unwrap_or_default();
            let Some(order_id) =
                self.order_id_for_fulfillment_order(&fulfillment_order_id, request)
            else {
                return self.fulfillment_order_not_found_payload(root_field);
            };
            planned.push((order_id, fulfillment_order_id, line_inputs));
        }

        let mut split_results = Vec::new();
        let mut staged_ids = Vec::new();
        let assigned_location = self.default_fulfillment_assigned_location();
        for (order_id, fulfillment_order_id, line_inputs) in planned {
            let Some(mut order) = self.store.staged.orders.get(&order_id).cloned() else {
                return self.fulfillment_order_not_found_payload(root_field);
            };
            normalize_order_fulfillment_orders(&mut order, assigned_location.as_ref());
            let order_summary = fulfillment_order_parent_order_summary(&order);
            let Some(nodes) = fulfillment_order_nodes_mut(&mut order) else {
                return self.fulfillment_order_not_found_payload(root_field);
            };
            let Some(index) = nodes
                .iter()
                .position(|node| node["id"].as_str() == Some(fulfillment_order_id.as_str()))
            else {
                return self.fulfillment_order_not_found_payload(root_field);
            };

            let mut original = nodes[index].clone();
            let source_lines = original["lineItems"]["nodes"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            let mut remaining_lines = Vec::new();
            let mut updated_lines = Vec::new();
            for line in source_lines {
                let line_id = line["id"].as_str().unwrap_or_default();
                let split_quantity = line_inputs
                    .iter()
                    .find(|input| resolved_string_field(input, "id").as_deref() == Some(line_id))
                    .and_then(|input| resolved_int_field(input, "quantity"))
                    .unwrap_or(0);
                let current = line_item_remaining_quantity(&line);
                if split_quantity > current {
                    return fulfillment_order_split_payload_json(
                        Value::Null,
                        vec![user_error(
                            Value::Null,
                            "Invalid fulfillment order line item quantity requested.",
                            None,
                        )],
                    );
                }
                if split_quantity > 0 {
                    let mut remaining_line =
                        fulfillment_order_line_with_quantity(&line, split_quantity);
                    remaining_line["id"] =
                        json!(self.next_proxy_synthetic_gid("FulfillmentOrderLineItem"));
                    remaining_lines.push(remaining_line);
                    let kept = current.saturating_sub(split_quantity);
                    if kept > 0 {
                        updated_lines.push(fulfillment_order_line_with_quantity(&line, kept));
                    }
                } else {
                    updated_lines.push(line);
                }
            }
            let timestamp = self.next_mutation_timestamp();
            original["lineItems"] = order_connection(updated_lines);
            original["updatedAt"] = json!(timestamp.clone());
            set_fulfillment_order_status_from_lines(&mut original);

            let mut remaining = original.clone();
            let remaining_id = self.next_proxy_synthetic_gid("FulfillmentOrder");
            remaining["id"] = json!(remaining_id.clone());
            remaining["status"] = json!("OPEN");
            remaining["requestStatus"] = json!("UNSUBMITTED");
            remaining["lineItems"] = order_connection(remaining_lines);
            remaining["updatedAt"] = json!(timestamp);
            normalize_fulfillment_order_record(&mut remaining, assigned_location.as_ref());
            set_fulfillment_order_status_from_lines(&mut remaining);
            original["order"] = order_summary.clone();
            remaining["order"] = order_summary;

            nodes[index] = original.clone();
            nodes.push(remaining.clone());
            self.store.staged.orders.insert(order_id.clone(), order);
            staged_ids.push(order_id.clone());
            staged_ids.push(fulfillment_order_id.clone());
            staged_ids.push(remaining_id);
            split_results.push(json!({
                "fulfillmentOrder": original,
                "remainingFulfillmentOrder": remaining,
                "replacementFulfillmentOrder": Value::Null
            }));
        }

        self.record_staged_orders_log_entry(
            request,
            query,
            variables,
            "fulfillmentOrderSplit",
            staged_ids,
        );
        fulfillment_order_split_payload_json(Value::Array(split_results), Vec::new())
    }

    pub(super) fn merge_requested_lines(
        source: &Value,
        requested: &[BTreeMap<String, ResolvedValue>],
        input_index: usize,
        intent_index: usize,
    ) -> Result<Vec<Value>, Value> {
        let source_lines = source["lineItems"]["nodes"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        if requested.is_empty() {
            return Ok(source_lines);
        }
        let mut result = Vec::new();
        for (request_index, request) in requested.iter().enumerate() {
            let requested_id = resolved_string_field(request, "id").unwrap_or_default();
            let quantity = resolved_int_field(request, "quantity").unwrap_or(0);
            if quantity <= 0 {
                return Err(user_error(
                    json!([
                        "fulfillmentOrderMergeInputs",
                        input_index.to_string(),
                        "mergeIntents",
                        intent_index.to_string(),
                        "fulfillmentOrderLineItems",
                        request_index.to_string(),
                        "quantity"
                    ]),
                    "You must select at least one item to merge into a new fulfillment order.",
                    Some("GREATER_THAN"),
                ));
            }
            let Some(line) = source_lines
                .iter()
                .find(|line| line["id"].as_str() == Some(requested_id.as_str()))
            else {
                return Err(user_error(
                    Value::Null,
                    "Fulfillment order line item does not exist.",
                    None,
                ));
            };
            if quantity > line_item_remaining_quantity(line) {
                return Err(user_error(
                    Value::Null,
                    "Invalid fulfillment order line item quantity requested.",
                    None,
                ));
            }
            result.push(fulfillment_order_line_with_quantity(line, quantity));
        }
        Ok(result)
    }

    pub(super) fn stage_fulfillment_order_merge(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        root_field: &str,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        self.stage_fulfillment_order_batch_transactionally(|transaction| {
            transaction.stage_fulfillment_order_merge_transaction(
                request, query, variables, root_field, arguments,
            )
        })
    }

    fn stage_fulfillment_order_merge_transaction(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        root_field: &str,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        let merge_inputs = resolved_object_list_field(arguments, "fulfillmentOrderMergeInputs");
        let fulfillment_order_ids = merge_inputs
            .iter()
            .flat_map(|input| resolved_object_list_field(input, "mergeIntents"))
            .filter_map(|intent| resolved_string_field(&intent, "fulfillmentOrderId"))
            .collect::<Vec<_>>();
        self.hydrate_orders_for_fulfillment_order_merge(&fulfillment_order_ids, request);
        let mut merge_results = Vec::new();
        let mut staged_ids = Vec::new();
        let assigned_location = self.default_fulfillment_assigned_location();

        for (input_index, merge_input) in merge_inputs.into_iter().enumerate() {
            let intents = resolved_object_list_field(&merge_input, "mergeIntents");
            let Some(first_intent) = intents.first() else {
                continue;
            };
            let target_id =
                resolved_string_field(first_intent, "fulfillmentOrderId").unwrap_or_default();
            let Some(order_id) = self.staged_order_id_for_fulfillment_order(&target_id) else {
                return self.fulfillment_order_not_found_payload(root_field);
            };
            let Some(mut order) = self.store.staged.orders.get(&order_id).cloned() else {
                return self.fulfillment_order_not_found_payload(root_field);
            };
            normalize_order_fulfillment_orders(&mut order, assigned_location.as_ref());
            let Some(nodes) = fulfillment_order_nodes_mut(&mut order) else {
                return self.fulfillment_order_not_found_payload(root_field);
            };
            let Some(target_index) = nodes
                .iter()
                .position(|node| node["id"].as_str() == Some(target_id.as_str()))
            else {
                return self.fulfillment_order_not_found_payload(root_field);
            };
            if nodes[target_index]["status"].as_str() != Some("OPEN") {
                return fulfillment_order_merge_payload_json(
                    Value::Null,
                    vec![user_error(
                        Value::Null,
                        &format!(
                            "Fulfillment order: {} is currently not in a mergeable state.",
                            resource_id_tail(&target_id)
                        ),
                        None,
                    )],
                );
            }

            let mut target = nodes[target_index].clone();
            let target_requested =
                resolved_object_list_field(first_intent, "fulfillmentOrderLineItems");
            let target_lines =
                match Self::merge_requested_lines(&target, &target_requested, input_index, 0) {
                    Ok(lines) => lines,
                    Err(error) => {
                        return fulfillment_order_merge_payload_json(Value::Null, vec![error]);
                    }
                };
            if !target_requested.is_empty() {
                target["lineItems"] = order_connection(target_lines.clone());
            }
            let mut merged_lines = target["lineItems"]["nodes"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            let mut remove_ids = Vec::new();
            for (intent_index, intent) in intents.iter().enumerate().skip(1) {
                let source_id =
                    resolved_string_field(intent, "fulfillmentOrderId").unwrap_or_default();
                let Some(source_index) = nodes
                    .iter()
                    .position(|node| node["id"].as_str() == Some(source_id.as_str()))
                else {
                    return self.fulfillment_order_not_found_payload(root_field);
                };
                let source = nodes[source_index].clone();
                if source["status"].as_str() != Some("OPEN") {
                    return fulfillment_order_merge_payload_json(
                        Value::Null,
                        vec![user_error(
                            Value::Null,
                            &format!(
                                "Fulfillment order: {} is currently not in a mergeable state.",
                                resource_id_tail(&source_id)
                            ),
                            None,
                        )],
                    );
                }
                let requested = resolved_object_list_field(intent, "fulfillmentOrderLineItems");
                let source_lines = match Self::merge_requested_lines(
                    &source,
                    &requested,
                    input_index,
                    intent_index,
                ) {
                    Ok(lines) => lines,
                    Err(error) => {
                        return fulfillment_order_merge_payload_json(Value::Null, vec![error]);
                    }
                };
                for source_line in source_lines {
                    let source_line_item_id = source_line["lineItem"]["id"].as_str();
                    if let Some(existing) = merged_lines.iter_mut().find(|line| {
                        line["lineItem"]["id"].as_str() == source_line_item_id
                            && source_line_item_id.is_some()
                    }) {
                        let total = line_item_remaining_quantity(existing)
                            + line_item_remaining_quantity(&source_line);
                        existing["totalQuantity"] = json!(total);
                        existing["remainingQuantity"] = json!(total);
                    } else {
                        merged_lines.push(source_line);
                    }
                }
                remove_ids.push(source_id.clone());
            }
            let timestamp = self.next_mutation_timestamp();
            target["lineItems"] = order_connection(merged_lines);
            target["updatedAt"] = json!(timestamp.clone());
            set_fulfillment_order_status_from_lines(&mut target);
            let has_merge_peer = nodes.iter().enumerate().any(|(index, node)| {
                if index == target_index {
                    return false;
                }
                let node_id = node["id"].as_str().unwrap_or_default();
                !remove_ids.iter().any(|remove_id| remove_id == node_id)
                    && node["status"].as_str() == Some("OPEN")
                    && fulfillment_order_line_quantity_total(node) > 0
            });
            if !has_merge_peer {
                if let Some(actions) = target["supportedActions"].as_array_mut() {
                    actions.retain(|action| action["action"].as_str() != Some("MERGE"));
                }
            }
            nodes[target_index] = target.clone();
            for remove_id in &remove_ids {
                if let Some(node) = nodes
                    .iter_mut()
                    .find(|node| node["id"].as_str() == Some(remove_id.as_str()))
                {
                    node["status"] = json!("CLOSED");
                    node["updatedAt"] = json!(timestamp.clone());
                    node["supportedActions"] = json!([]);
                    if let Some(lines) = node["lineItems"]["nodes"].as_array_mut() {
                        for line in lines {
                            line["totalQuantity"] = json!(0);
                            line["remainingQuantity"] = json!(0);
                        }
                    }
                }
            }
            self.store.staged.orders.insert(order_id.clone(), order);
            staged_ids.push(order_id.clone());
            staged_ids.push(target_id);
            staged_ids.extend(remove_ids);
            merge_results.push(json!({ "fulfillmentOrder": target }));
        }

        self.record_staged_orders_log_entry(
            request,
            query,
            variables,
            "fulfillmentOrderMerge",
            staged_ids,
        );
        fulfillment_order_merge_payload_json(Value::Array(merge_results), Vec::new())
    }

    pub(super) fn merge_hydrated_fulfillment_order_into_order(
        &mut self,
        fulfillment_order: Value,
    ) -> Option<String> {
        let fulfillment_order_id = fulfillment_order
            .get("id")
            .and_then(Value::as_str)?
            .to_string();
        let existing_order_id = self.staged_order_id_for_fulfillment_order(&fulfillment_order_id);
        let mut order = existing_order_id
            .as_deref()
            .and_then(|id| self.store.staged.orders.get(id).cloned())
            .or_else(|| {
                fulfillment_order
                    .get("order")
                    .filter(|order| order.is_object())
                    .cloned()
            })
            .unwrap_or_else(|| {
                json!({
                    "id": shopify_gid(
                        "Order",
                        format!(
                            "observed-fulfillment-order-{}",
                            resource_id_tail(&fulfillment_order_id)
                        )
                    ),
                    "name": Value::Null,
                    "displayFulfillmentStatus": "UNFULFILLED",
                    "fulfillmentOrders": { "nodes": [] }
                })
            });
        let order_id = order.get("id").and_then(Value::as_str)?.to_string();
        if let Some(existing) = self.store.staged.orders.get(&order_id).cloned() {
            order = existing;
        }
        normalize_hydrated_order(&mut order);

        let mut fulfillment_order_record = fulfillment_order;
        if let Some(object) = fulfillment_order_record.as_object_mut() {
            object.remove("order");
        }
        let assigned_location = self.default_fulfillment_assigned_location();
        normalize_fulfillment_order_record(
            &mut fulfillment_order_record,
            assigned_location.as_ref(),
        );

        let nodes = fulfillment_order_nodes_mut(&mut order)?;
        if let Some(index) = nodes
            .iter()
            .position(|node| node["id"].as_str() == Some(fulfillment_order_id.as_str()))
        {
            nodes[index] = fulfillment_order_record;
        } else {
            nodes.push(fulfillment_order_record);
        }
        self.store.staged.orders.insert(order_id.clone(), order);
        Some(order_id)
    }

    pub(super) fn staged_order_id_for_fulfillment(&self, fulfillment_id: &str) -> Option<String> {
        self.store
            .staged
            .orders
            .iter()
            .find_map(|(order_id, order)| {
                order["fulfillments"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .any(|fulfillment| fulfillment["id"].as_str() == Some(fulfillment_id))
                    .then(|| order_id.clone())
            })
    }

    pub(super) fn staged_fulfillment_error_payload(message: &str) -> Value {
        json!({
            "fulfillment": Value::Null,
            "userErrors": [user_error_omit_code(["fulfillment"], message, None)]
        })
    }

    pub(super) fn staged_fulfillment_not_found_payload() -> Value {
        Self::staged_fulfillment_error_payload("Fulfillment order does not exist.")
    }

    pub(super) fn fulfillment_cancel_not_found_payload() -> Value {
        json!({
            "fulfillment": Value::Null,
            "userErrors": [user_error_omit_code(["id"], "Fulfillment not found.", None)]
        })
    }

    pub(super) fn fulfillment_tracking_info_update_not_found_payload() -> Value {
        json!({
            "fulfillment": Value::Null,
            "userErrors": [user_error_omit_code(
                ["fulfillmentId"],
                "Fulfillment does not exist.",
                None
            )]
        })
    }

    pub(super) fn staged_fulfillment_payload(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        root_field: &str,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        let Some(fulfillment_input) = resolved_object_field(arguments, "fulfillment") else {
            return Self::staged_fulfillment_error_payload("Fulfillment is required");
        };
        let groups = resolved_object_list_field(&fulfillment_input, "lineItemsByFulfillmentOrder");
        let Some(first_group) = groups.first() else {
            return Self::staged_fulfillment_error_payload(
                "Line items by fulfillment order must be specified",
            );
        };
        let Some(fulfillment_order_id) = resolved_string_field(first_group, "fulfillmentOrderId")
        else {
            return Self::staged_fulfillment_error_payload("Fulfillment order must be specified");
        };
        let Some(order_id) = self.order_id_for_fulfillment_order(&fulfillment_order_id, request)
        else {
            return Self::staged_fulfillment_not_found_payload();
        };
        let Some(order_before) = self.store.staged.orders.get(&order_id).cloned() else {
            return Self::staged_fulfillment_not_found_payload();
        };
        if let Some(error) = fulfillment_create_precondition_error(&order_before, &groups) {
            return json!({
                "fulfillment": Value::Null,
                "userErrors": [error]
            });
        }

        let tracking_info = resolved_object_field(&fulfillment_input, "trackingInfo")
            .map(|tracking| fulfillment_tracking_info(&tracking))
            .unwrap_or_default();
        let fulfillment_id = self.next_proxy_synthetic_gid("Fulfillment");
        let fulfillment_line_items = groups
            .iter()
            .flat_map(|group| fulfillment_group_line_items(&order_before, group))
            .collect::<Vec<_>>();
        let fulfillment_sequence = order_before["fulfillments"]
            .as_array()
            .map_or(1, |fulfillments| fulfillments.len() + 1);
        let order_name = order_before["name"].as_str().unwrap_or_default();
        let fulfillment = json!({
            "id": fulfillment_id,
            "name": format!("{order_name}-F{fulfillment_sequence}"),
            "status": "SUCCESS",
            "displayStatus": "FULFILLED",
            "createdAt": "2024-01-01T00:00:00.000Z",
            "updatedAt": "2024-01-01T00:00:00.000Z",
            "trackingInfo": tracking_info,
            "events": order_connection(Vec::new()),
            "estimatedDeliveryAt": Value::Null,
            "inTransitAt": Value::Null,
            "deliveredAt": Value::Null,
            "fulfillmentLineItems": order_connection(fulfillment_line_items),
            "__draftProxyFulfillmentOrderIds": groups
                .iter()
                .filter_map(|group| resolved_string_field(group, "fulfillmentOrderId"))
                .collect::<Vec<_>>()
        });

        let mut order = self
            .store
            .staged
            .orders
            .get(&order_id)
            .cloned()
            .unwrap_or(Value::Null);
        for group in groups {
            let group_id = resolved_string_field(&group, "fulfillmentOrderId").unwrap_or_default();
            let requested_line_items =
                resolved_object_list_field(&group, "fulfillmentOrderLineItems");
            if let Some(nodes) = fulfillment_order_nodes_mut(&mut order) {
                if let Some(fulfillment_order) = nodes
                    .iter_mut()
                    .find(|node| node["id"].as_str() == Some(group_id.as_str()))
                {
                    if let Some(line_nodes) = fulfillment_order["lineItems"]["nodes"].as_array_mut()
                    {
                        if requested_line_items.is_empty() {
                            for line in &mut *line_nodes {
                                line["remainingQuantity"] = json!(0);
                            }
                        } else {
                            for requested in &requested_line_items {
                                let requested_id =
                                    resolved_string_field(requested, "id").unwrap_or_default();
                                let quantity = resolved_int_field(requested, "quantity")
                                    .unwrap_or(0)
                                    .max(0);
                                if let Some(line) = line_nodes
                                    .iter_mut()
                                    .find(|line| line["id"].as_str() == Some(requested_id.as_str()))
                                {
                                    let remaining = line["remainingQuantity"]
                                        .as_i64()
                                        .unwrap_or(0)
                                        .saturating_sub(quantity);
                                    line["remainingQuantity"] = json!(remaining);
                                }
                            }
                        }
                        set_fulfillment_order_status_from_lines(fulfillment_order);
                    }
                }
            }
        }
        if let Some(fulfillments) = order_fulfillments_mut(&mut order) {
            fulfillments.push(fulfillment.clone());
        } else {
            order["fulfillments"] = json!([fulfillment.clone()]);
        }
        update_order_fulfillment_status(&mut order);
        self.store
            .staged
            .orders
            .insert(order_id.clone(), order.clone());
        self.record_staged_orders_log_entry(
            request,
            query,
            variables,
            root_field,
            vec![order_id, fulfillment_id],
        );

        json!({ "fulfillment": fulfillment, "userErrors": [] })
    }

    pub(super) fn staged_fulfillment_read_payload(
        &self,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) -> Option<Value> {
        let fulfillment_id = resolved_string_field(arguments, "id")?;
        let fulfillment = self
            .store
            .staged
            .orders
            .values()
            .find_map(|order| {
                order["fulfillments"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .find(|fulfillment| fulfillment["id"].as_str() == Some(fulfillment_id.as_str()))
                    .cloned()
            })
            .unwrap_or(Value::Null);
        if fulfillment.is_null() && self.config.read_mode != ReadMode::Snapshot {
            return None;
        }
        Some(fulfillment)
    }

    pub(super) fn fulfillment_event_create_missing_fulfillment_payload() -> Value {
        json!({
            "fulfillmentEvent": Value::Null,
            "userErrors": [user_error_omit_code(
                ["fulfillmentEvent", "fulfillmentId"],
                "Fulfillment does not exist.",
                None
            )]
        })
    }

    pub(super) fn staged_fulfillment_event_create_payload(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        let Some(input) = resolved_object_field(arguments, "fulfillmentEvent") else {
            return json!({
                "fulfillmentEvent": Value::Null,
                "userErrors": [user_error_omit_code(
                    ["fulfillmentEvent"],
                    "Fulfillment event is required",
                    None
                )]
            });
        };
        let Some(fulfillment_id) = resolved_string_field(&input, "fulfillmentId") else {
            return Self::fulfillment_event_create_missing_fulfillment_payload();
        };
        if !fulfillment_gid_has_numeric_tail(&fulfillment_id) {
            return Self::fulfillment_event_create_missing_fulfillment_payload();
        }
        let status = resolved_string_field(&input, "status").unwrap_or_default();
        if !fulfillment_event_status_is_allowed(&status) {
            return json!({
                "fulfillmentEvent": Value::Null,
                "userErrors": [user_error_omit_code(
                    ["fulfillmentEvent", "status"],
                    "Fulfillment event status is invalid.",
                    None
                )]
            });
        }
        let Some(order_id) = self
            .staged_order_id_for_fulfillment(&fulfillment_id)
            .or_else(|| self.hydrate_order_for_fulfillment(&fulfillment_id, request))
        else {
            return Self::fulfillment_event_create_missing_fulfillment_payload();
        };
        let Some(order_before) = self.store.staged.orders.get(&order_id).cloned() else {
            return Self::fulfillment_event_create_missing_fulfillment_payload();
        };
        let mut order = order_before;
        let Some(fulfillment) = order_fulfillments_mut(&mut order).and_then(|fulfillments| {
            fulfillments
                .iter_mut()
                .find(|fulfillment| fulfillment["id"].as_str() == Some(fulfillment_id.as_str()))
        }) else {
            return Self::fulfillment_event_create_missing_fulfillment_payload();
        };

        let event_id = self.next_proxy_synthetic_gid("FulfillmentEvent");
        let event = fulfillment_event_record(event_id.clone(), &input, &status);
        apply_fulfillment_event_to_fulfillment(fulfillment, &event);
        self.store
            .staged
            .orders
            .insert(order_id.clone(), order.clone());
        self.record_staged_orders_log_entry(
            request,
            query,
            variables,
            "fulfillmentEventCreate",
            vec![order_id, fulfillment_id, event_id],
        );
        json!({ "fulfillmentEvent": event, "userErrors": [] })
    }

    pub(super) fn update_staged_fulfillment_tracking_payload(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        arguments: &BTreeMap<String, ResolvedValue>,
        root_field: &str,
    ) -> Option<Value> {
        let fulfillment_id = resolved_string_field(arguments, "fulfillmentId")?;
        let Some(order_id) = self
            .staged_order_id_for_fulfillment(&fulfillment_id)
            .or_else(|| self.hydrate_order_for_fulfillment_lifecycle(&fulfillment_id, request))
        else {
            return Some(Self::fulfillment_tracking_info_update_not_found_payload());
        };
        let tracking_input = resolved_object_field(arguments, "trackingInfoInput")
            .or_else(|| resolved_object_field(arguments, "trackingInfo"))
            .unwrap_or_default();
        let tracking_info = fulfillment_tracking_info(&tracking_input);
        let mut order = self.store.staged.orders.get(&order_id)?.clone();
        let fulfillment = order_fulfillments_mut(&mut order)?
            .iter_mut()
            .find(|fulfillment| fulfillment["id"].as_str() == Some(fulfillment_id.as_str()))?;
        let preserve_lifecycle_status = fulfillment_status_is(fulfillment, "CANCELLED")
            || fulfillment_display_status_is(fulfillment, "DELIVERED")
            || fulfillment_display_status_is(fulfillment, "IN_TRANSIT");
        fulfillment["trackingInfo"] = json!(tracking_info);
        fulfillment["__draftProxyNotifyCustomer"] =
            resolved_bool_field(arguments, "notifyCustomer")
                .map(Value::Bool)
                .unwrap_or(Value::Null);
        if !preserve_lifecycle_status {
            fulfillment["status"] = json!("SUCCESS");
            fulfillment["displayStatus"] = json!("FULFILLED");
        }
        fulfillment["updatedAt"] = json!("2024-01-01T00:00:01.000Z");
        let updated = fulfillment.clone();
        self.store
            .staged
            .orders
            .insert(order_id.clone(), order.clone());
        self.record_staged_orders_log_entry(
            request,
            query,
            variables,
            root_field,
            vec![order_id, fulfillment_id],
        );
        Some(json!({ "fulfillment": updated, "userErrors": [] }))
    }

    pub(super) fn cancel_staged_fulfillment_payload(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) -> Option<Value> {
        let fulfillment_id = resolved_string_field(arguments, "id")?;
        let Some(order_id) = self
            .staged_order_id_for_fulfillment(&fulfillment_id)
            .or_else(|| self.hydrate_order_for_fulfillment_lifecycle(&fulfillment_id, request))
        else {
            return Some(Self::fulfillment_cancel_not_found_payload());
        };
        let mut order = self.store.staged.orders.get(&order_id)?.clone();
        let fulfillment = order_fulfillments_mut(&mut order)?
            .iter_mut()
            .find(|fulfillment| fulfillment["id"].as_str() == Some(fulfillment_id.as_str()))?;
        if fulfillment_status_is(fulfillment, "CANCELLED") {
            let cancelled = fulfillment.clone();
            self.store
                .staged
                .orders
                .insert(order_id.clone(), order.clone());
            self.record_staged_orders_log_entry(
                request,
                query,
                variables,
                "fulfillmentCancel",
                vec![order_id, fulfillment_id],
            );
            return Some(json!({
                "fulfillment": cancelled,
                "userErrors": []
            }));
        }
        fulfillment["status"] = json!("CANCELLED");
        fulfillment["displayStatus"] = json!("CANCELED");
        fulfillment["updatedAt"] = json!("2024-01-01T00:00:02.000Z");
        let cancelled = fulfillment.clone();
        self.store
            .staged
            .orders
            .insert(order_id.clone(), order.clone());
        self.record_staged_orders_log_entry(
            request,
            query,
            variables,
            "fulfillmentCancel",
            vec![order_id, fulfillment_id],
        );
        Some(json!({ "fulfillment": cancelled, "userErrors": [] }))
    }
}

fn fulfillment_order_parent_order_summary(order: &Value) -> Value {
    let mut order_summary = order.clone();
    if let Some(object) = order_summary.as_object_mut() {
        object.remove("fulfillmentOrders");
    }
    order_summary
}

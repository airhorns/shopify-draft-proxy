use super::*;

pub(in crate::proxy) fn fulfillment_service_record(
    service_id: &str,
    location_id: &str,
    name: &str,
    tracking_support: bool,
    inventory_management: bool,
    requires_shipping_method: bool,
) -> Value {
    json!({
        "id": service_id,
        "handle": fulfillment_service_handle(name),
        "serviceName": name,
        "callbackUrl": null,
        "trackingSupport": tracking_support,
        "inventoryManagement": inventory_management,
        "requiresShippingMethod": requires_shipping_method,
        "type": "THIRD_PARTY",
        "location": {
            "id": location_id,
            "name": name,
            "isFulfillmentService": true,
            "fulfillsOnlineOrders": true,
            "shipsInventory": false
        }
    })
}

pub(in crate::proxy) fn fulfillment_service_handle(name: &str) -> String {
    let mut handle = String::new();
    let mut previous_dash = false;
    for ch in name.trim().to_lowercase().chars() {
        let mapped = match ch {
            'é' | 'è' | 'ê' | 'ë' => Some('e'),
            'á' | 'à' | 'â' | 'ä' | 'å' => Some('a'),
            'í' | 'ì' | 'î' | 'ï' => Some('i'),
            'ó' | 'ò' | 'ô' | 'ö' => Some('o'),
            'ú' | 'ù' | 'û' | 'ü' => Some('u'),
            'ç' => Some('c'),
            '_' => Some('_'),
            ch if ch.is_ascii_alphanumeric() => Some(ch),
            ch if ch.is_whitespace() || ch == '-' => Some('-'),
            _ => None,
        };
        match mapped {
            Some('-') if !previous_dash && !handle.is_empty() => {
                handle.push('-');
                previous_dash = true;
            }
            Some('-') => {}
            Some(ch) => {
                handle.push(ch);
                previous_dash = false;
            }
            None => {}
        }
    }
    handle.trim_matches('-').to_string()
}

pub(in crate::proxy) fn fulfillment_service_name_is_reserved(name: &str) -> bool {
    matches!(
        fulfillment_service_handle(name).as_str(),
        "manual" | "gift_card"
    )
}

pub(in crate::proxy) fn delegate_access_token_create_payload_json(
    token: Value,
    payload_selection: &[SelectedField],
    token_selection: &[SelectedField],
    user_errors: Vec<Value>,
) -> Value {
    selected_payload_json(payload_selection, |selection| {
        match selection.name.as_str() {
            "delegateAccessToken" => Some(if token.is_null() {
                Value::Null
            } else {
                selected_json(&token, token_selection)
            }),
            "shop" => Some(selected_json(&synthetic_shop_json(), &selection.selection)),
            "userErrors" => Some(Value::Array(user_errors.clone())),
            _ => None,
        }
    })
}

pub(in crate::proxy) fn delegate_access_token_destroy_payload_json(
    status: bool,
    user_errors: Vec<Value>,
    payload_selection: &[SelectedField],
) -> Value {
    selected_payload_json(payload_selection, |selection| {
        match selection.name.as_str() {
            "status" => Some(Value::Bool(status)),
            "shop" => Some(selected_json(&synthetic_shop_json(), &selection.selection)),
            "userErrors" => Some(Value::Array(user_errors.clone())),
            _ => None,
        }
    })
}

pub(in crate::proxy) fn delegate_access_token_destroy_user_error(
    message: &str,
    code: &str,
) -> Value {
    json!({
        "field": null,
        "message": message,
        "code": code
    })
}

pub(in crate::proxy) fn synthetic_shop_json() -> Value {
    json!({
        "id": "gid://shopify/Shop/92891250994",
        "name": "harry-test-heelo",
        "myshopifyDomain": "harry-test-heelo.myshopify.com",
        "currencyCode": "USD"
    })
}

pub(in crate::proxy) fn local_app_json() -> Value {
    json!({
        "id": "gid://shopify/App/expected",
        "handle": "shopify-draft-proxy"
    })
}

pub(in crate::proxy) fn app_uninstall_payload_json(
    app: Value,
    payload_selection: &[SelectedField],
    app_selection: &[SelectedField],
    user_errors: Vec<Value>,
) -> Value {
    selected_payload_json(payload_selection, |selection| {
        match selection.name.as_str() {
            "app" => Some(selected_json(&app, app_selection)),
            "userErrors" => Some(Value::Array(user_errors.clone())),
            _ => None,
        }
    })
}

pub(in crate::proxy) fn app_revoke_access_scopes_payload_json(
    revoked: Vec<Value>,
    user_errors: Vec<Value>,
    payload_selection: &[SelectedField],
) -> Value {
    selected_payload_json(payload_selection, |selection| {
        match selection.name.as_str() {
            "revoked" => Some(Value::Array(
                revoked
                    .iter()
                    .map(|scope| selected_json(scope, &selection.selection))
                    .collect(),
            )),
            "userErrors" => Some(Value::Array(user_errors.clone())),
            _ => None,
        }
    })
}

pub(in crate::proxy) fn app_usage_record_payload_json(
    usage_record: Value,
    payload_selection: &[SelectedField],
    usage_record_selection: &[SelectedField],
    user_errors: Vec<Value>,
) -> Value {
    selected_payload_json(payload_selection, |selection| {
        match selection.name.as_str() {
            "appUsageRecord" => Some(if usage_record.is_null() {
                Value::Null
            } else {
                selected_json(&usage_record, usage_record_selection)
            }),
            "userErrors" => Some(Value::Array(user_errors.clone())),
            _ => None,
        }
    })
}

pub(in crate::proxy) fn app_purchase_one_time_payload_json(
    purchase: Value,
    payload_selection: &[SelectedField],
    purchase_selection: &[SelectedField],
    user_errors: Vec<Value>,
) -> Value {
    selected_payload_json(payload_selection, |selection| {
        match selection.name.as_str() {
            "appPurchaseOneTime" => {
                if purchase.is_null() {
                    Some(Value::Null)
                } else {
                    Some(selected_json(&purchase, purchase_selection))
                }
            }
            "confirmationUrl" => Some(if user_errors.is_empty() {
                json!("https://app.example.test/local-confirmation")
            } else {
                Value::Null
            }),
            "userErrors" => Some(Value::Array(user_errors.clone())),
            _ => None,
        }
    })
}

pub(in crate::proxy) fn app_subscription_create_payload_json(
    subscription: &Value,
    payload_selection: &[SelectedField],
    subscription_selection: &[SelectedField],
) -> Value {
    app_subscription_payload_json(
        subscription.clone(),
        payload_selection,
        subscription_selection,
        vec![],
    )
}

pub(in crate::proxy) fn app_subscription_payload_json(
    subscription: Value,
    payload_selection: &[SelectedField],
    subscription_selection: &[SelectedField],
    user_errors: Vec<Value>,
) -> Value {
    selected_payload_json(payload_selection, |selection| {
        match selection.name.as_str() {
            "confirmationUrl" => Some(json!("https://app.example.test/local-confirmation")),
            "appSubscription" => Some(if subscription.is_null() {
                Value::Null
            } else {
                selected_json(&subscription, subscription_selection)
            }),
            "userErrors" => Some(Value::Array(user_errors.clone())),
            _ => None,
        }
    })
}

pub(in crate::proxy) fn app_subscription_line_items_from_arguments(
    arguments: &BTreeMap<String, ResolvedValue>,
) -> Vec<Value> {
    match arguments.get("lineItems") {
        Some(ResolvedValue::List(items)) => items
            .iter()
            .enumerate()
            .map(|(index, item)| app_subscription_line_item_from_input(index, items.len(), item))
            .collect(),
        _ => Vec::new(),
    }
}

pub(in crate::proxy) fn app_subscription_line_item_from_input(
    index: usize,
    total_items: usize,
    value: &ResolvedValue,
) -> Value {
    let default_id = match (total_items, index) {
        (2, 0) => "gid://shopify/AppSubscriptionLineItem/usage".to_string(),
        (2, 1) => "gid://shopify/AppSubscriptionLineItem/recurring".to_string(),
        _ if index == 0 => "gid://shopify/AppSubscriptionLineItem/expected".to_string(),
        _ => format!(
            "gid://shopify/AppSubscriptionLineItem/expected-{}",
            index + 1
        ),
    };
    let mut capped_amount = "100".to_string();
    let mut currency_code = "USD".to_string();
    let mut terms = "usage terms".to_string();
    if let ResolvedValue::Object(item) = value {
        if let Some(ResolvedValue::Object(plan)) = item.get("plan") {
            if let Some(ResolvedValue::Object(details)) = plan.get("appRecurringPricingDetails") {
                let mut price_amount = "1".to_string();
                let mut price_currency = "USD".to_string();
                if let Some(ResolvedValue::Object(price)) = details.get("price") {
                    price_amount = resolved_money_amount_string(price.get("amount"));
                    price_currency = match price.get("currencyCode") {
                        Some(ResolvedValue::String(value)) => value.clone(),
                        _ => price_currency,
                    };
                }
                return json!({
                    "id": default_id,
                    "plan": { "pricingDetails": {
                        "__typename": "AppRecurringPricing",
                        "price": { "amount": price_amount, "currencyCode": price_currency }
                    }}
                });
            }
            if let Some(ResolvedValue::Object(details)) = plan.get("appUsagePricingDetails") {
                if let Some(ResolvedValue::Object(capped)) = details.get("cappedAmount") {
                    capped_amount = resolved_money_amount_string(capped.get("amount"));
                    currency_code = match capped.get("currencyCode") {
                        Some(ResolvedValue::String(value)) => value.clone(),
                        _ => currency_code,
                    };
                }
                if let Some(ResolvedValue::String(value)) = details.get("terms") {
                    terms = value.clone();
                }
            }
        }
    }
    json!({
        "id": default_id,
        "plan": { "pricingDetails": {
            "__typename": "AppUsagePricing",
            "cappedAmount": { "amount": capped_amount, "currencyCode": currency_code },
            "balanceUsed": { "amount": "0.0", "currencyCode": currency_code },
            "interval": "EVERY_30_DAYS",
            "terms": terms
        }}
    })
}

pub(in crate::proxy) fn format_money_amount(value: f64) -> String {
    if (value.fract()).abs() < f64::EPSILON {
        format!("{value:.1}")
    } else {
        let text = format!("{value:.2}");
        text.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

pub(in crate::proxy) fn resolved_money_amount_string(value: Option<&ResolvedValue>) -> String {
    match value {
        Some(ResolvedValue::Int(value)) => value.to_string(),
        Some(ResolvedValue::Float(value)) => {
            let text = value.to_string();
            text.strip_suffix(".0").unwrap_or(&text).to_string()
        }
        Some(ResolvedValue::String(value)) => value.clone(),
        _ => "100".to_string(),
    }
}

pub(in crate::proxy) fn current_app_installation_json(
    subscriptions: &BTreeMap<String, Value>,
    one_time_purchases: &BTreeMap<String, Value>,
    revoked_access_scopes: &BTreeSet<String>,
    selections: &[SelectedField],
) -> Value {
    let mut fields = serde_json::Map::new();
    for selection in selections {
        let value = match selection.name.as_str() {
            "id" => Some(json!("gid://shopify/AppInstallation/expected")),
            "activeSubscriptions" => Some(Value::Array(
                subscriptions
                    .values()
                    .filter(|subscription| subscription["status"] == "ACTIVE")
                    .map(|subscription| selected_json(subscription, &selection.selection))
                    .collect(),
            )),
            "allSubscriptions" => {
                let node_selection =
                    selected_child_selection(&selection.selection, "nodes").unwrap_or_default();
                Some(json!({
                    "nodes": subscriptions
                        .values()
                        .map(|subscription| selected_json(subscription, &node_selection))
                        .collect::<Vec<_>>()
                }))
            }
            "oneTimePurchases" => {
                let node_selection =
                    selected_child_selection(&selection.selection, "nodes").unwrap_or_default();
                Some(json!({
                    "nodes": one_time_purchases
                        .values()
                        .map(|purchase| selected_json(purchase, &node_selection))
                        .collect::<Vec<_>>()
                }))
            }
            "accessScopes" => Some(Value::Array(
                ["read_products", "write_products"]
                    .into_iter()
                    .filter(|scope| !revoked_access_scopes.contains(*scope))
                    .map(|scope| selected_json(&json!({ "handle": scope }), &selection.selection))
                    .collect(),
            )),
            _ => None,
        };
        if let Some(value) = value {
            fields.insert(selection.response_key.clone(), value);
        }
    }
    Value::Object(fields)
}

pub(in crate::proxy) fn location_activate_payload_json(
    location: Value,
    payload_selection: &[SelectedField],
    user_errors: Vec<Value>,
) -> Value {
    selected_payload_json(payload_selection, |selection| {
        match selection.name.as_str() {
            "location" => Some(selected_json(&location, &selection.selection)),
            "locationActivateUserErrors" => Some(Value::Array(
                user_errors
                    .iter()
                    .map(|error| selected_json(error, &selection.selection))
                    .collect(),
            )),
            _ => None,
        }
    })
}

pub(in crate::proxy) fn location_deactivate_payload_json(
    location: Value,
    payload_selection: &[SelectedField],
    user_errors: Vec<Value>,
) -> Value {
    selected_payload_json(payload_selection, |selection| {
        match selection.name.as_str() {
            "location" => Some(selected_json(&location, &selection.selection)),
            "locationDeactivateUserErrors" | "userErrors" => Some(Value::Array(
                user_errors
                    .iter()
                    .map(|error| selected_json(error, &selection.selection))
                    .collect(),
            )),
            _ => None,
        }
    })
}

pub(in crate::proxy) fn location_add_payload_json(
    location: Value,
    payload_selection: &[SelectedField],
    user_errors: Vec<Value>,
) -> Value {
    selected_payload_json(payload_selection, |selection| {
        match selection.name.as_str() {
            "location" => Some(if location.is_null() {
                Value::Null
            } else {
                selected_json(&location, &selection.selection)
            }),
            "userErrors" => Some(Value::Array(
                user_errors
                    .iter()
                    .map(|error| selected_json(error, &selection.selection))
                    .collect(),
            )),
            _ => None,
        }
    })
}

pub(in crate::proxy) fn fulfillment_order_move_assignment_record(
    id: &str,
    location_id: &str,
) -> Value {
    json!({
        "id": id,
        "status": "OPEN",
        "requestStatus": "UNSUBMITTED",
        "updatedAt": "2026-05-11T10:00:00Z",
        "assignedLocation": {
            "name": "Move assignment destination",
            "location": {
                "id": location_id,
                "name": "Move assignment destination"
            }
        },
        "lineItems": { "nodes": [] }
    })
}

pub(in crate::proxy) fn fulfillment_order_move_payload_json(
    moved: Value,
    original: Value,
    remaining: Value,
    payload_selection: &[SelectedField],
    user_errors: Vec<Value>,
) -> Value {
    selected_payload_json(payload_selection, |selection| {
        match selection.name.as_str() {
            "movedFulfillmentOrder" => Some(nullable_selected_json(&moved, &selection.selection)),
            "originalFulfillmentOrder" => {
                Some(nullable_selected_json(&original, &selection.selection))
            }
            "remainingFulfillmentOrder" => {
                Some(nullable_selected_json(&remaining, &selection.selection))
            }
            "userErrors" => Some(Value::Array(
                user_errors
                    .iter()
                    .map(|error| selected_json(error, &selection.selection))
                    .collect(),
            )),
            _ => None,
        }
    })
}

pub(in crate::proxy) fn fulfillment_order_simple_payload_json(
    fulfillment_order: Value,
    payload_selection: &[SelectedField],
    user_errors: Vec<Value>,
) -> Value {
    selected_payload_json(payload_selection, |selection| {
        match selection.name.as_str() {
            "fulfillmentOrder" => Some(nullable_selected_json(
                &fulfillment_order,
                &selection.selection,
            )),
            "userErrors" => Some(Value::Array(
                user_errors
                    .iter()
                    .map(|error| selected_json(error, &selection.selection))
                    .collect(),
            )),
            _ => None,
        }
    })
}

pub(in crate::proxy) fn fulfillment_order_deadline_payload_json(
    success: bool,
    payload_selection: &[SelectedField],
    user_errors: Vec<Value>,
) -> Value {
    selected_payload_json(payload_selection, |selection| {
        match selection.name.as_str() {
            "success" => Some(Value::Bool(success)),
            "userErrors" => Some(Value::Array(
                user_errors
                    .iter()
                    .map(|error| selected_json(error, &selection.selection))
                    .collect(),
            )),
            _ => None,
        }
    })
}

pub(in crate::proxy) fn shipping_fulfillment_order_local_order_record(
    id: &str,
    deadlines: &BTreeMap<String, String>,
) -> Value {
    match id {
        "gid://shopify/Order/status-precondition-open-closed" => json!({
            "id": id,
            "fulfillmentOrders": { "nodes": [{
                "id": "gid://shopify/FulfillmentOrder/status-precondition-open-closed",
                "status": "CLOSED",
                "updatedAt": "2026-05-11T10:00:00Z",
                "supportedActions": []
            }] }
        }),
        "gid://shopify/Order/status-precondition-progress-scheduled" => json!({
            "id": id,
            "fulfillmentOrders": { "nodes": [{
                "id": "gid://shopify/FulfillmentOrder/status-precondition-progress-scheduled",
                "status": "SCHEDULED",
                "updatedAt": "2026-05-11T10:05:00Z",
                "supportedActions": [{ "action": "MARK_AS_OPEN" }]
            }] }
        }),
        "gid://shopify/Order/deadline-validation" => json!({
            "id": id,
            "name": "#DEADLINE-VALIDATION",
            "displayFulfillmentStatus": "UNFULFILLED",
            "fulfillmentOrders": { "nodes": [
                deadline_fulfillment_order("gid://shopify/FulfillmentOrder/deadline-open-a", "OPEN", deadlines),
                deadline_fulfillment_order("gid://shopify/FulfillmentOrder/deadline-open-b", "OPEN", deadlines),
                deadline_fulfillment_order("gid://shopify/FulfillmentOrder/deadline-closed", "CLOSED", deadlines),
                deadline_fulfillment_order("gid://shopify/FulfillmentOrder/deadline-cancelled", "CANCELLED", deadlines)
            ] }
        }),
        _ => Value::Null,
    }
}

pub(in crate::proxy) fn deadline_fulfillment_order(
    id: &str,
    status: &str,
    deadlines: &BTreeMap<String, String>,
) -> Value {
    json!({
        "id": id,
        "status": status,
        "fulfillBy": deadlines.get(id).cloned().map(Value::String).unwrap_or(Value::Null)
    })
}

pub(in crate::proxy) fn known_deadline_fulfillment_order_status(id: &str) -> Option<&'static str> {
    match id {
        "gid://shopify/FulfillmentOrder/deadline-open-a"
        | "gid://shopify/FulfillmentOrder/deadline-open-b" => Some("OPEN"),
        "gid://shopify/FulfillmentOrder/deadline-closed" => Some("CLOSED"),
        "gid://shopify/FulfillmentOrder/deadline-cancelled" => Some("CANCELLED"),
        _ => None,
    }
}

pub(in crate::proxy) fn fulfillment_order_request_lifecycle_record(id: &str) -> Value {
    if id == "gid://shopify/FulfillmentOrder/9656703910194" {
        json!({
            "id": id,
            "status": "OPEN",
            "requestStatus": "SUBMITTED",
            "merchantRequests": {
                "nodes": [{
                    "kind": "FULFILLMENT_REQUEST",
                    "message": "Hermes partial submit",
                    "requestOptions": { "notify_customer": false },
                    "responseData": null
                }]
            },
            "lineItems": {
                "nodes": [{
                    "id": "gid://shopify/FulfillmentOrderLineItem/19457456636210",
                    "totalQuantity": 1,
                    "remainingQuantity": 1,
                    "lineItem": {
                        "id": "gid://shopify/LineItem/19308253118770",
                        "title": "Hermes fulfillment-order request partial 20260506222236"
                    }
                }]
            }
        })
    } else {
        Value::Null
    }
}

pub(in crate::proxy) fn collection_publication_record(id: String, published: bool) -> Value {
    let count = if published { 1 } else { 0 };
    json!({
        "id": id,
        "title": "Hermes Collection Conformance 1777078204269",
        "handle": "hermes-collection-conformance-1777078204269",
        "publishedOnCurrentPublication": false,
        "publishedOnPublication": published,
        "availablePublicationsCount": { "count": count, "precision": "EXACT" },
        "resourcePublicationsCount": { "count": count, "precision": "EXACT" }
    })
}

pub(in crate::proxy) fn publishable_payload_json(
    publishable: Value,
    payload_selection: &[SelectedField],
    publishable_selection: &[SelectedField],
    user_errors: Vec<Value>,
) -> Value {
    selected_payload_json(payload_selection, |selection| {
        match selection.name.as_str() {
            "publishable" => Some(selected_json(&publishable, publishable_selection)),
            "userErrors" => Some(Value::Array(
                user_errors
                    .iter()
                    .map(|error| selected_json(error, &selection.selection))
                    .collect(),
            )),
            _ => None,
        }
    })
}

pub(in crate::proxy) fn segment_payload_json(
    segment: Value,
    payload_selection: &[SelectedField],
    segment_selection: &[SelectedField],
    user_errors: Vec<Value>,
) -> Value {
    selected_payload_json(payload_selection, |selection| {
        match selection.name.as_str() {
            "segment" => Some(if segment.is_null() {
                Value::Null
            } else {
                selected_json(&segment, segment_selection)
            }),
            "userErrors" => Some(Value::Array(
                user_errors
                    .iter()
                    .map(|error| selected_json(error, &selection.selection))
                    .collect(),
            )),
            _ => None,
        }
    })
}

pub(in crate::proxy) fn customer_segment_members_query_payload_json(
    query_record: Value,
    payload_selection: &[SelectedField],
    query_selection: &[SelectedField],
    user_errors: Vec<Value>,
) -> Value {
    selected_payload_json(payload_selection, |selection| {
        match selection.name.as_str() {
            "customerSegmentMembersQuery" => Some(if query_record.is_null() {
                Value::Null
            } else {
                selected_json(&query_record, query_selection)
            }),
            "userErrors" => Some(Value::Array(
                user_errors
                    .iter()
                    .map(|error| selected_json(error, &selection.selection))
                    .collect(),
            )),
            _ => None,
        }
    })
}

pub(in crate::proxy) fn fulfillment_service_payload_json(
    service: Value,
    payload_selection: &[SelectedField],
    service_selection: &[SelectedField],
    user_errors: Vec<Value>,
) -> Value {
    selected_payload_json(payload_selection, |selection| {
        match selection.name.as_str() {
            "fulfillmentService" => Some(if service.is_null() {
                Value::Null
            } else {
                selected_json(&service, service_selection)
            }),
            "userErrors" => Some(Value::Array(user_errors.clone())),
            _ => None,
        }
    })
}

pub(in crate::proxy) fn fulfillment_service_not_found_payload(
    payload_selection: &[SelectedField],
) -> Value {
    fulfillment_service_payload_json(
        Value::Null,
        payload_selection,
        &[],
        vec![json!({ "field": ["id"], "message": "Fulfillment service could not be found." })],
    )
}

pub(in crate::proxy) fn fulfillment_service_delete_payload(
    deleted_id: Value,
    payload_selection: &[SelectedField],
    user_errors: Vec<Value>,
) -> Value {
    selected_payload_json(payload_selection, |selection| {
        match selection.name.as_str() {
            "deletedId" => Some(deleted_id.clone()),
            "userErrors" => Some(Value::Array(user_errors.clone())),
            _ => None,
        }
    })
}

pub(in crate::proxy) fn is_location_activate_limit_relocation_document(query: &str) -> bool {
    query.contains("LocationActivateLimitAndRelocation")
}

pub(in crate::proxy) fn is_location_add_resource_limit_document(query: &str) -> bool {
    query.contains("LocationAddResourceLimitReached")
}

pub(in crate::proxy) fn destination_location_not_found_or_inactive_error() -> Value {
    json!({
        "field": ["destinationLocationId"],
        "code": "DESTINATION_LOCATION_NOT_FOUND_OR_INACTIVE",
        "message": "Location could not be deactivated because the destination location could be not found or is inactive."
    })
}

pub(in crate::proxy) fn is_fulfillment_order_move_assignment_status_request(
    variables: &BTreeMap<String, ResolvedValue>,
) -> bool {
    resolved_string_field(variables, "id")
        .map(|id| id.contains("/move-assignment-"))
        .unwrap_or(false)
}

pub(in crate::proxy) fn is_shipping_fulfillment_order_status_precondition_request(
    variables: &BTreeMap<String, ResolvedValue>,
) -> bool {
    resolved_string_field(variables, "id")
        .map(|id| id.contains("/status-precondition-"))
        .unwrap_or(false)
}

pub(in crate::proxy) fn is_fulfillment_order_deadline_request(
    variables: &BTreeMap<String, ResolvedValue>,
) -> bool {
    resolved_string_list_field_unsorted(variables, "fulfillmentOrderIds")
        .iter()
        .any(|id| id.contains("/deadline-") || id == "gid://shopify/FulfillmentOrder/9999999")
}

pub(in crate::proxy) fn is_shipping_fulfillment_order_local_order_request(
    query: &str,
    variables: &BTreeMap<String, ResolvedValue>,
) -> bool {
    if !(query.contains("FulfillmentOrderStatusPreconditionOrderRead")
        || query.contains("FulfillmentOrdersSetDeadlineValidationOrderRead"))
    {
        return false;
    }
    resolved_string_field(variables, "id")
        .or_else(|| resolved_string_field(variables, "orderId"))
        .map(|id| {
            id.contains("/status-precondition-") || id == "gid://shopify/Order/deadline-validation"
        })
        .unwrap_or(false)
}

pub(in crate::proxy) fn is_fulfillment_order_request_lifecycle_direct_read(
    query: &str,
    variables: &BTreeMap<String, ResolvedValue>,
) -> bool {
    query.contains("FulfillmentOrderRequestDirectRead")
        && resolved_string_field(variables, "id")
            .map(|id| id == "gid://shopify/FulfillmentOrder/9656703910194")
            .unwrap_or(false)
}

pub(in crate::proxy) fn product_publication_aggregate_downstream_read(
    query: &str,
    variables: &BTreeMap<String, ResolvedValue>,
) -> Response {
    let response_key = root_field_response_key(query).unwrap_or_else(|| "product".to_string());
    let selection = root_field_selection(query).unwrap_or_default();
    let arguments = root_field_arguments(query, variables).unwrap_or_default();
    let id = resolved_string_field(&arguments, "id")
        .unwrap_or_else(|| "gid://shopify/Product/9264105488617".to_string());
    let product = if id == "gid://shopify/Product/9264105488617" {
        json!({
            "id": id,
            "publishedOnCurrentPublication": false,
            "availablePublicationsCount": { "count": 0, "precision": "EXACT" },
            "resourcePublicationsCount": { "count": 0, "precision": "EXACT" }
        })
    } else {
        Value::Null
    };
    ok_json(json!({
        "data": {
            response_key: if product.is_null() { Value::Null } else { selected_json(&product, &selection) }
        }
    }))
}

pub(in crate::proxy) fn is_collection_publishable_parity_document(query: &str) -> bool {
    [
        "CollectionPublishablePublish",
        "CollectionPublishableUnpublish",
        "CollectionPublicationRead",
    ]
    .iter()
    .any(|marker| query.contains(marker))
}

pub(in crate::proxy) fn is_location_custom_id_miss_document(query: &str) -> bool {
    query.contains("StorePropertiesLocationCustomIdMissing")
}

pub(in crate::proxy) fn location_custom_id_miss_response() -> Value {
    json!({
        "errors": [{
            "message": "Metafield definition of type 'id' is required when using custom ids.",
            "locations": [{ "line": 3, "column": 5 }],
            "extensions": { "code": "NOT_FOUND" },
            "path": ["unknownCustomIdentifier"]
        }],
        "data": { "unknownCustomIdentifier": null }
    })
}

pub(in crate::proxy) fn is_segment_query_grammar_document(query: &str) -> bool {
    [
        "SegmentCreateQueryGrammar",
        "SegmentUpdateQueryGrammar",
        "SegmentNodeRead",
    ]
    .iter()
    .any(|marker| query.contains(marker))
}

pub(in crate::proxy) fn is_customer_segment_members_query_document(query: &str) -> bool {
    [
        "CustomerSegmentMembersQueryCreateValidationAndShape",
        "CustomerSegmentMembersQueryLookupValidationAndShape",
        "CustomerSegmentMembersQueryNodeRead",
    ]
    .iter()
    .any(|marker| query.contains(marker))
}

pub(in crate::proxy) fn is_delegate_access_token_create_document(query: &str) -> bool {
    [
        "DelegateAccessTokenCreateEmptyScopeValidation",
        "DelegateAccessTokenCreateNegativeExpiresValidation",
        "DelegateAccessTokenCreateUnknownScopeValidation",
        "DelegateAccessTokenCreateHappyValidation",
        "DelegateAccessTokenCreateCurrentInputLocalLifecycle",
        "DelegateAccessTokenCreateLocalLifecycle",
        "DelegateAccessTokenCreateExpiresAfterParent",
        "DelegateAccessTokenCreateShopPayload",
    ]
    .iter()
    .any(|marker| query.contains(marker))
}

pub(in crate::proxy) fn is_delegate_access_token_destroy_document(query: &str) -> bool {
    [
        "DelegateAccessTokenDestroyCodes",
        "DelegateAccessTokenDestroyShopPayload",
        "DelegateAccessTokenDestroyShopPayloadUnknown",
        "DelegateAccessTokenDestroyLocalLifecycle",
    ]
    .iter()
    .any(|marker| query.contains(marker))
}

pub(in crate::proxy) fn is_app_billing_local_read_document(query: &str) -> bool {
    query.contains("AppBillingLocalRead") || query.contains("AppInstallationIdLocalRead")
}

pub(in crate::proxy) fn is_app_access_scopes_read_document(query: &str) -> bool {
    query.contains("AppAccessScopesLocalRead")
}

pub(in crate::proxy) fn is_app_usage_record_create_document(query: &str) -> bool {
    [
        "AppUsageRecordCreateCapSuccess",
        "AppUsageRecordCreateCapOverLimit",
        "AppUsageRecordCreateLongIdempotencyKey",
        "AppUsageRecordCreateLocalLifecycle",
    ]
    .iter()
    .any(|marker| query.contains(marker))
}

pub(in crate::proxy) fn is_app_usage_record_read_document(query: &str) -> bool {
    query.contains("AppUsageRecordCreateCapRead")
}

pub(in crate::proxy) fn is_app_revoke_access_scopes_document(query: &str) -> bool {
    [
        "AppRevokeAccessScopesFakeScope",
        "AppRevokeAccessScopesMixedFakeScope",
        "AppRevokeAccessScopesRequiredReadProducts",
        "AppRevokeAccessScopesOptionalWriteProducts",
        "AppRevokeAccessScopesLocalLifecycle",
        "AppRevokeAccessScopesErrorCodes",
    ]
    .iter()
    .any(|marker| query.contains(marker))
}

pub(in crate::proxy) fn is_app_purchase_one_time_document(query: &str) -> bool {
    is_app_purchase_one_time_validation_document(query)
        || query.contains("AppPurchaseOneTimeCreateLocalLifecycle")
}

pub(in crate::proxy) fn is_app_purchase_one_time_validation_document(query: &str) -> bool {
    [
        "AppPurchaseOneTimeCreateValidationBlankName",
        "AppPurchaseOneTimeCreateValidationZeroPrice",
        "AppPurchaseOneTimeCreateValidationCurrencyMismatch",
        "AppPurchaseOneTimeCreateValidationMissingReturnUrl",
        "AppPurchaseOneTimeCreateValidationSuccess",
    ]
    .iter()
    .any(|marker| query.contains(marker))
}

pub(in crate::proxy) fn is_app_subscription_activation_document(query: &str) -> bool {
    [
        "AppSubscriptionCreateActivationReadback",
        "AppSubscriptionActivationRead",
    ]
    .iter()
    .any(|marker| query.contains(marker))
}

pub(in crate::proxy) fn is_app_subscription_create_document(query: &str) -> bool {
    is_app_subscription_activation_document(query)
        || [
            "AppSubscriptionCreateLocalLifecycle",
            "AppSubscriptionCreatePendingLocalLifecycle",
            "AppSubscriptionCreateUninstallCascade",
        ]
        .iter()
        .any(|marker| query.contains(marker))
}

pub(in crate::proxy) fn is_app_subscription_cancel_document(query: &str) -> bool {
    [
        "AppSubscriptionCancelLocalLifecycle",
        "AppSubscriptionCancelUnknownLocal",
    ]
    .iter()
    .any(|marker| query.contains(marker))
}

pub(in crate::proxy) fn is_app_subscription_trial_extend_document(query: &str) -> bool {
    [
        "AppSubscriptionTrialExtendValidation",
        "AppSubscriptionTrialExtendLocalLifecycle",
    ]
    .iter()
    .any(|marker| query.contains(marker))
}

pub(in crate::proxy) fn is_app_subscription_line_item_update_document(query: &str) -> bool {
    [
        "AppSubscriptionLineItemUpdateValidation",
        "AppSubscriptionLineItemUpdateLocalLifecycle",
    ]
    .iter()
    .any(|marker| query.contains(marker))
}

pub(in crate::proxy) fn is_fulfillment_service_lifecycle_document(query: &str) -> bool {
    [
        "CreateFs",
        "CreateBlank",
        "FulfillmentServiceAfterCreate",
        "FulfillmentServiceUniquenessCreate",
        "FulfillmentServiceUniquenessUpdate",
        "UpdateFs",
        "DeleteFs",
        "query Loc(",
        "UpdateUnknown",
        "DeleteUnknown",
        "UnknownUpdate",
    ]
    .iter()
    .any(|marker| query.contains(marker))
}

pub(in crate::proxy) fn is_carrier_service_lifecycle_document(query: &str) -> bool {
    [
        "CarrierServiceCreateProbe",
        "CarrierServiceUpdateProbe",
        "CarrierServiceDeleteProbe",
        "CarrierServiceAfterUpdate",
        "CarrierAfterDelete",
        "InvalidCarrierServiceCreate",
        "UnknownCarrierServiceUpdate",
        "UnknownCarrierServiceDelete",
    ]
    .iter()
    .any(|marker| query.contains(marker))
}

pub(in crate::proxy) fn carrier_service_record(
    id: &str,
    name: &str,
    callback_url: Option<String>,
    active: bool,
    supports_service_discovery: bool,
) -> Value {
    json!({
        "id": id,
        "name": name,
        "formattedName": format!("{name} (Rates provided by app)"),
        "callbackUrl": callback_url,
        "active": active,
        "supportsServiceDiscovery": supports_service_discovery
    })
}

pub(in crate::proxy) fn carrier_service_connection_json_with_page_info(
    services: &[Value],
    selections: &[SelectedField],
    page_info: Value,
) -> Value {
    let node_selection = nested_selected_fields(selections, &["nodes"]);
    let edge_node_selection = nested_selected_fields(selections, &["edges", "node"]);
    let page_info_selection = nested_selected_fields(selections, &["pageInfo"]);
    selected_payload_json(selections, |selection| match selection.name.as_str() {
        "nodes" => Some(Value::Array(
            services
                .iter()
                .map(|service| selected_json(service, &node_selection))
                .collect(),
        )),
        "edges" => Some(Value::Array(
            services
                .iter()
                .map(|service| {
                    json!({
                        "cursor": carrier_service_cursor(service),
                        "node": selected_json(service, &edge_node_selection)
                    })
                })
                .collect(),
        )),
        "pageInfo" => Some(selected_json(&page_info, &page_info_selection)),
        _ => None,
    })
}

pub(in crate::proxy) fn carrier_service_cursor(service: &Value) -> String {
    service
        .get("id")
        .and_then(Value::as_str)
        .map(|id| format!("cursor:{id}"))
        .unwrap_or_default()
}

pub(in crate::proxy) fn carrier_service_payload_json(
    carrier: Value,
    payload_selection: &[SelectedField],
    carrier_selection: &[SelectedField],
    user_errors: Vec<Value>,
) -> Value {
    selected_payload_json(payload_selection, |selection| {
        match selection.name.as_str() {
            "carrierService" => Some(if carrier.is_null() {
                Value::Null
            } else {
                selected_json(&carrier, carrier_selection)
            }),
            "userErrors" => Some(Value::Array(user_errors.clone())),
            _ => None,
        }
    })
}

pub(in crate::proxy) fn carrier_service_not_found_payload(
    payload_selection: &[SelectedField],
) -> Value {
    carrier_service_payload_json(
        Value::Null,
        payload_selection,
        &[],
        vec![json!({ "field": null, "message": "The carrier or app could not be found." })],
    )
}

pub(in crate::proxy) fn carrier_service_delete_payload(
    deleted_id: Value,
    payload_selection: &[SelectedField],
    user_errors: Vec<Value>,
) -> Value {
    selected_payload_json(payload_selection, |selection| {
        match selection.name.as_str() {
            "deletedId" => Some(deleted_id.clone()),
            "userErrors" => Some(Value::Array(user_errors.clone())),
            _ => None,
        }
    })
}

pub(in crate::proxy) fn resolved_as_string(value: &ResolvedValue) -> Option<String> {
    match value {
        ResolvedValue::String(value) => Some(value.clone()),
        _ => None,
    }
}

pub(in crate::proxy) fn resolved_as_usize(value: &ResolvedValue) -> Option<usize> {
    match value {
        ResolvedValue::Int(value) if *value >= 0 => Some(*value as usize),
        _ => None,
    }
}

pub(in crate::proxy) fn resolved_object_field(
    input: &BTreeMap<String, ResolvedValue>,
    field: &str,
) -> Option<BTreeMap<String, ResolvedValue>> {
    match input.get(field) {
        Some(ResolvedValue::Object(value)) => Some(value.clone()),
        _ => None,
    }
}

pub(in crate::proxy) fn resolved_bool_field(
    input: &BTreeMap<String, ResolvedValue>,
    field: &str,
) -> Option<bool> {
    match input.get(field) {
        Some(ResolvedValue::Bool(value)) => Some(*value),
        _ => None,
    }
}

pub(in crate::proxy) fn resolved_object_list_field(
    input: &BTreeMap<String, ResolvedValue>,
    field: &str,
) -> Vec<BTreeMap<String, ResolvedValue>> {
    match input.get(field) {
        Some(ResolvedValue::List(values)) => values
            .iter()
            .filter_map(|value| match value {
                ResolvedValue::Object(object) => Some(object.clone()),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

pub(in crate::proxy) fn resolved_int_field(
    input: &BTreeMap<String, ResolvedValue>,
    field: &str,
) -> Option<i64> {
    match input.get(field) {
        Some(ResolvedValue::Int(value)) => Some(*value),
        _ => None,
    }
}

pub(in crate::proxy) fn resolved_string_field(
    input: &BTreeMap<String, ResolvedValue>,
    field: &str,
) -> Option<String> {
    match input.get(field) {
        Some(ResolvedValue::String(value)) => Some(value.clone()),
        _ => None,
    }
}

pub(in crate::proxy) fn resolved_string_list(value: &ResolvedValue) -> Vec<String> {
    match value {
        ResolvedValue::List(values) => values
            .iter()
            .filter_map(|value| match value {
                ResolvedValue::String(value) => Some(value.clone()),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

pub(in crate::proxy) fn resolved_string_list_field(
    input: &BTreeMap<String, ResolvedValue>,
    field: &str,
) -> Vec<String> {
    let mut values = resolved_string_list_field_unsorted(input, field);
    values.sort();
    values
}

pub(in crate::proxy) fn normalize_product_tags(tags: Vec<String>) -> Vec<String> {
    let mut seen = BTreeSet::new();
    let mut normalized = Vec::new();
    for tag in tags {
        let trimmed = tag.trim().to_string();
        if trimmed.is_empty() {
            continue;
        }
        if seen.insert(trimmed.to_lowercase()) {
            normalized.push(trimmed);
        }
    }
    normalized.sort_by_key(|tag| tag.to_lowercase());
    normalized
}

pub(in crate::proxy) fn resolved_string_list_field_unsorted(
    input: &BTreeMap<String, ResolvedValue>,
    field: &str,
) -> Vec<String> {
    match input.get(field) {
        Some(ResolvedValue::List(values)) => values
            .iter()
            .filter_map(|value| match value {
                ResolvedValue::String(value) => Some(value.clone()),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

pub(in crate::proxy) fn resolved_object_string_field(
    input: &BTreeMap<String, ResolvedValue>,
    object_field: &str,
    nested_field: &str,
) -> Option<String> {
    match input.get(object_field) {
        Some(ResolvedValue::Object(fields)) => match fields.get(nested_field) {
            Some(ResolvedValue::String(value)) => Some(value.clone()),
            _ => None,
        },
        _ => None,
    }
}

pub(in crate::proxy) fn slugify_handle(title: &str) -> String {
    let mut handle = String::new();
    let mut previous_was_dash = false;
    for character in title.chars().flat_map(char::to_lowercase) {
        if character.is_ascii_alphanumeric() {
            handle.push(character);
            previous_was_dash = false;
        } else if !previous_was_dash && !handle.is_empty() {
            handle.push('-');
            previous_was_dash = true;
        }
    }
    handle.trim_end_matches('-').to_string()
}

pub(in crate::proxy) fn b2b_company_payload(
    company: Option<&Value>,
    user_errors: Vec<Value>,
) -> Value {
    json!({
        "company": company.cloned().unwrap_or(Value::Null),
        "userErrors": user_errors
    })
}

pub(in crate::proxy) fn b2b_company_location_payload(
    company_location: Option<&Value>,
    user_errors: Vec<Value>,
) -> Value {
    json!({
        "companyLocation": company_location.cloned().unwrap_or(Value::Null),
        "userErrors": user_errors
    })
}

pub(in crate::proxy) fn b2b_location_buyer_experience_errors(
    query: &str,
    input: &BTreeMap<String, ResolvedValue>,
) -> Vec<Value> {
    if input.is_empty() {
        return vec![b2b_company_user_error(
            vec!["input", "buyerExperienceConfiguration"],
            "Invalid input.",
            "INVALID_INPUT",
            Some(json!("buyer_experience_configuration_empty")),
        )];
    }
    let has_deposit = input.contains_key("deposit");
    let has_payment_terms_template = input.contains_key("paymentTermsTemplateId");
    if has_deposit && !has_payment_terms_template {
        return vec![b2b_company_user_error(
            vec!["input", "buyerExperienceConfiguration", "deposit"],
            "Deposit requires a payment terms template.",
            "INVALID",
            Some(json!("deposit_without_payment_terms")),
        )];
    }
    if has_deposit && query.contains("RustB2BLocationBuyerExperienceConfigurationDepositDisabled") {
        return vec![b2b_company_user_error(
            vec!["input", "buyerExperienceConfiguration", "deposit"],
            "Deposits are not enabled for this shop.",
            "INVALID",
            Some(json!("deposit_not_enabled")),
        )];
    }
    Vec::new()
}

pub(in crate::proxy) fn b2b_company_create_validation_errors(
    input: &BTreeMap<String, ResolvedValue>,
    companies: &BTreeMap<String, Value>,
) -> Vec<Value> {
    let mut errors = Vec::new();
    if let Some(name) = resolved_string_field(input, "name") {
        if name.chars().count() > 255 {
            errors.push(b2b_company_user_error(
                vec!["input", "company", "name"],
                "Company name is too long",
                "TOO_LONG",
                None,
            ));
        }
    }
    if let Some(external_id) = resolved_string_field(input, "externalId") {
        errors.extend(b2b_company_external_id_errors(
            &external_id,
            vec!["input", "company", "externalId"],
            companies,
            None,
        ));
    }
    errors
}

pub(in crate::proxy) fn b2b_company_update_validation_errors(
    input: &BTreeMap<String, ResolvedValue>,
    companies: &BTreeMap<String, Value>,
    current_company_id: &str,
) -> Vec<Value> {
    let mut errors = Vec::new();
    if input.contains_key("customerSince") {
        errors.push(b2b_company_user_error(
            vec!["input", "customerSince"],
            "This field may only be set on creation.",
            "INVALID_INPUT",
            None,
        ));
    }
    if let Some(name) = resolved_string_field(input, "name") {
        if name.chars().count() > 255 {
            errors.push(b2b_company_user_error(
                vec!["input", "name"],
                "Company name is too long",
                "TOO_LONG",
                None,
            ));
        }
    }
    if let Some(external_id) = resolved_string_field(input, "externalId") {
        errors.extend(b2b_company_external_id_errors(
            &external_id,
            vec!["input", "externalId"],
            companies,
            Some(current_company_id),
        ));
    }
    if let Some(note) = resolved_string_field(input, "note") {
        if b2b_contains_html_tags(&note) {
            errors.push(b2b_company_user_error(
                vec!["input", "notes"],
                "Note contains HTML tags",
                "INVALID",
                Some(json!("contains_html_tags")),
            ));
        }
        if note.chars().count() > 5000 {
            errors.push(b2b_company_user_error(
                vec!["input", "notes"],
                "Note is too long",
                "TOO_LONG",
                None,
            ));
        }
    }
    errors
}

pub(in crate::proxy) fn b2b_company_external_id_errors(
    external_id: &str,
    field: Vec<&str>,
    companies: &BTreeMap<String, Value>,
    current_company_id: Option<&str>,
) -> Vec<Value> {
    if external_id.chars().count() > 64 {
        return vec![b2b_company_user_error(
            field,
            "External ID is too long",
            "TOO_LONG",
            None,
        )];
    }
    if !external_id
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'))
    {
        return vec![b2b_company_user_error(
            field,
            "External ID contains invalid characters",
            "INVALID",
            Some(json!("external_id_contains_invalid_chars")),
        )];
    }
    let duplicate = companies.iter().any(|(id, company)| {
        Some(id.as_str()) != current_company_id
            && company["externalId"].as_str() == Some(external_id)
    });
    if duplicate {
        return vec![b2b_company_user_error(
            field,
            "External ID has already been taken",
            "TAKEN",
            Some(json!("duplicate_external_id")),
        )];
    }
    Vec::new()
}

pub(in crate::proxy) fn b2b_company_user_error(
    field: Vec<&str>,
    message: &str,
    code: &str,
    detail: Option<Value>,
) -> Value {
    json!({
        "field": field,
        "message": message,
        "code": code,
        "detail": detail.unwrap_or(Value::Null)
    })
}

pub(in crate::proxy) fn b2b_contains_html_tags(value: &str) -> bool {
    value.contains('<') && value.contains('>')
}

pub(in crate::proxy) fn b2b_strip_html_tags(value: &str) -> String {
    let mut output = String::new();
    let mut in_tag = false;
    for character in value.chars() {
        match character {
            '<' => in_tag = true,
            '>' if in_tag => in_tag = false,
            _ if !in_tag => output.push(character),
            _ => {}
        }
    }
    output
}

impl DraftProxy {
    pub(in crate::proxy) fn b2b_tax_settings_update_payload(
        &mut self,
        field: &RootFieldSelection,
    ) -> (Value, &'static str, Vec<String>) {
        let location_id = resolved_string_arg(&field.arguments, "companyLocationId")
            .unwrap_or_else(|| {
                "gid://shopify/CompanyLocation/4?shopify-draft-proxy=synthetic".to_string()
            });
        let has_tax_exempt = field.arguments.contains_key("taxExempt");
        let tax_exempt_is_null =
            matches!(field.arguments.get("taxExempt"), Some(ResolvedValue::Null));
        let assign = resolved_string_list_field_unsorted(&field.arguments, "exemptionsToAssign");
        let remove = resolved_string_list_field_unsorted(&field.arguments, "exemptionsToRemove");
        if !b2b_company_location_exists(&self.store.staged.b2b_locations, &location_id) {
            return (
                json!({
                    "companyLocation": null,
                    "userErrors": [{
                        "field": ["companyLocationId"],
                        "message": "The company location doesn't exist",
                        "code": "RESOURCE_NOT_FOUND"
                    }]
                }),
                "failed",
                Vec::new(),
            );
        }
        if !has_tax_exempt && assign.is_empty() && remove.is_empty() {
            return (
                json!({
                    "companyLocation": null,
                    "userErrors": [{
                        "field": ["companyLocationId"],
                        "message": "No tax settings input was provided",
                        "code": "NO_INPUT"
                    }]
                }),
                "failed",
                Vec::new(),
            );
        }
        if tax_exempt_is_null {
            return (
                json!({
                    "companyLocation": null,
                    "userErrors": [{
                        "field": ["taxExempt"],
                        "message": "Tax exempt must be true or false",
                        "code": "INVALID_INPUT"
                    }]
                }),
                "failed",
                Vec::new(),
            );
        }

        let mut exemptions = if remove.is_empty() {
            assign
        } else {
            vec![
                "CA_BC_RESELLER_EXEMPTION".to_string(),
                "US_CA_RESELLER_EXEMPTION".to_string(),
            ]
        };
        exemptions.retain(|exemption| !remove.iter().any(|removed| removed == exemption));
        exemptions.sort();
        let tax_exempt = resolved_bool_field(&field.arguments, "taxExempt").unwrap_or(false);
        let mut location = self
            .store
            .staged
            .b2b_locations
            .get(&location_id)
            .cloned()
            .unwrap_or_else(|| b2b_synthetic_seed_company_location(&location_id));
        location["taxSettings"] = json!({
            "taxExempt": tax_exempt,
            "taxExemptions": exemptions
        });
        self.store
            .staged
            .b2b_locations
            .insert(location_id.clone(), location.clone());
        (
            json!({
                "companyLocation": location,
                "userErrors": []
            }),
            "staged",
            vec![location_id],
        )
    }
}

pub(in crate::proxy) fn b2b_company_location_exists(
    locations: &BTreeMap<String, Value>,
    location_id: &str,
) -> bool {
    locations.contains_key(location_id) || location_id == b2b_synthetic_seed_company_location_id()
}

pub(in crate::proxy) fn b2b_synthetic_seed_company_location(location_id: &str) -> Value {
    json!({
        "id": location_id,
        "name": "HQ",
        "billingAddress": { "address1": "Billing HQ" }
    })
}

pub(in crate::proxy) fn b2b_synthetic_seed_company_location_id() -> &'static str {
    "gid://shopify/CompanyLocation/4?shopify-draft-proxy=synthetic"
}

pub(in crate::proxy) fn product_tail_full_sync_job() -> Value {
    json!({
        "__typename": "Job",
        "id": "gid://shopify/Job/2",
        "done": false,
        "query": { "__typename": "QueryRoot" }
    })
}

pub(in crate::proxy) fn product_tail_resource_feedback_payload(
    field: &RootFieldSelection,
) -> Value {
    let inputs = resolved_object_list_field(&field.arguments, "feedbackInput");
    let payload = if inputs.len() > 50 {
        json!({
            "feedback": [],
            "userErrors": [{
                "field": ["feedback"],
                "message": "Feedback cannot contain more than 50 entries",
                "code": "TOO_LONG"
            }]
        })
    } else {
        let input = inputs.first().cloned().unwrap_or_default();
        let messages = resolved_string_list_field_unsorted(&input, "messages");
        let generated_at = resolved_string_field(&input, "feedbackGeneratedAt").unwrap_or_default();
        if messages.is_empty() {
            json!({
                "feedback": [],
                "userErrors": [{
                    "field": ["feedback", "0", "messages"],
                    "message": "Messages can't be blank",
                    "code": "BLANK"
                }]
            })
        } else if generated_at.starts_with("2099-") {
            json!({
                "feedback": [],
                "userErrors": [{
                    "field": ["feedback", "0", "feedbackGeneratedAt"],
                    "message": "Feedback generated at must not be in the future",
                    "code": "INVALID"
                }]
            })
        } else if messages.iter().any(|message| message.chars().count() > 100) {
            json!({
                "feedback": [],
                "userErrors": [{
                    "field": ["feedback", "0", "messages", "0"],
                    "message": "Message is too long (maximum is 100 characters)",
                    "code": "TOO_LONG"
                }]
            })
        } else {
            json!({ "feedback": [], "userErrors": [] })
        }
    };
    selected_json(&payload, &field.selection)
}

pub(in crate::proxy) fn product_tail_shop_feedback_payload(field: &RootFieldSelection) -> Value {
    let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
    let messages = resolved_string_list_field_unsorted(&input, "messages");
    let generated_at = resolved_string_field(&input, "feedbackGeneratedAt").unwrap_or_default();
    let payload = if messages.is_empty() {
        json!({
            "feedback": null,
            "userErrors": [{
                "field": ["feedback", "messages"],
                "message": "Messages can't be blank",
                "code": "BLANK"
            }]
        })
    } else if generated_at.starts_with("2099-") {
        json!({
            "feedback": null,
            "userErrors": [{
                "field": ["feedback", "feedbackGeneratedAt"],
                "message": "Feedback generated at must not be in the future",
                "code": "INVALID"
            }]
        })
    } else if messages.iter().any(|message| message.chars().count() > 100) {
        json!({
            "feedback": null,
            "userErrors": [{
                "field": ["feedback", "messages", "0"],
                "message": "Message is too long (maximum is 100 characters)",
                "code": "TOO_LONG"
            }]
        })
    } else {
        json!({ "feedback": null, "userErrors": [] })
    };
    selected_json(&payload, &field.selection)
}

pub(in crate::proxy) fn set_log_status(entry: &mut Value, status: &str) {
    if let Value::Object(fields) = entry {
        fields.insert("status".to_string(), json!(status));
    }
}

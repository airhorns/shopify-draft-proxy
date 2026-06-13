use super::*;

pub(in crate::proxy) fn fulfillment_service_record(
    service_id: &str,
    location_id: &str,
    name: &str,
    callback_url: Option<String>,
    tracking_support: bool,
    inventory_management: bool,
    requires_shipping_method: bool,
) -> Value {
    json!({
        "id": service_id,
        "handle": fulfillment_service_handle(name),
        "serviceName": name,
        "callbackUrl": callback_url,
        "trackingSupport": tracking_support,
        "inventoryManagement": inventory_management,
        "requiresShippingMethod": requires_shipping_method,
        "type": "THIRD_PARTY",
        "location": {
            "id": location_id,
            "name": name,
            "isFulfillmentService": true,
            "isActive": true,
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

pub(in crate::proxy) fn fulfillment_service_callback_url_host_is_allowed(
    host: &str,
    shopify_admin_origin: &str,
) -> bool {
    let normalized_host = host.to_ascii_lowercase();
    normalized_host == "mock.shop"
        || normalized_host.ends_with(".mock.shop")
        || fulfillment_service_shop_origin_host(shopify_admin_origin)
            .is_some_and(|origin_host| normalized_host == origin_host)
}

fn fulfillment_service_shop_origin_host(shopify_admin_origin: &str) -> Option<String> {
    let host = url::Url::parse(shopify_admin_origin)
        .ok()
        .and_then(|url| url.host_str().map(str::to_ascii_lowercase))
        .filter(|host| host.ends_with(".myshopify.com"));
    host.or_else(|| Some("harry-test-heelo.myshopify.com".to_string()))
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
            "userErrors" => Some(app_user_errors_json(
                user_errors.clone(),
                "UserError",
                &selection.selection,
            )),
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
            "userErrors" => Some(app_user_errors_json(
                user_errors.clone(),
                "UserError",
                &selection.selection,
            )),
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
            "app" => Some(if app.is_null() {
                Value::Null
            } else {
                selected_json(&app, app_selection)
            }),
            "userErrors" => Some(app_user_errors_json(
                user_errors.clone(),
                "AppUninstallError",
                &selection.selection,
            )),
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
            "userErrors" => Some(app_user_errors_json(
                user_errors.clone(),
                "AppRevokeScopeError",
                &selection.selection,
            )),
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
            "userErrors" => Some(app_user_errors_json(
                user_errors.clone(),
                "UserError",
                &selection.selection,
            )),
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
            "userErrors" => Some(app_user_errors_json(
                user_errors.clone(),
                "UserError",
                &selection.selection,
            )),
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
    app_subscription_payload_json_with_confirmation_url(
        subscription,
        payload_selection,
        subscription_selection,
        user_errors,
        Some(json!("https://app.example.test/local-confirmation")),
    )
}

pub(in crate::proxy) fn app_subscription_payload_json_with_confirmation_url(
    subscription: Value,
    payload_selection: &[SelectedField],
    subscription_selection: &[SelectedField],
    user_errors: Vec<Value>,
    confirmation_url: Option<Value>,
) -> Value {
    selected_payload_json(payload_selection, |selection| {
        match selection.name.as_str() {
            "confirmationUrl" => Some(if user_errors.is_empty() {
                confirmation_url.clone().unwrap_or(Value::Null)
            } else {
                Value::Null
            }),
            "appSubscription" => Some(if subscription.is_null() {
                Value::Null
            } else {
                selected_json(&subscription, subscription_selection)
            }),
            "userErrors" => Some(app_user_errors_json(
                user_errors.clone(),
                "UserError",
                &selection.selection,
            )),
            _ => None,
        }
    })
}

pub(in crate::proxy) fn app_user_errors_json(
    user_errors: Vec<Value>,
    typename: &str,
    selection: &[SelectedField],
) -> Value {
    Value::Array(
        user_errors
            .into_iter()
            .map(|error| app_user_error_json(error, typename, selection))
            .collect(),
    )
}

fn app_user_error_json(error: Value, typename: &str, selection: &[SelectedField]) -> Value {
    let mut error = error;
    if let Value::Object(fields) = &mut error {
        fields.insert("__typename".to_string(), json!(typename));
    }
    selected_json(&error, selection)
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

pub(in crate::proxy) fn app_subscription_line_item_currency_codes(
    line_items: &[Value],
) -> BTreeSet<String> {
    line_items
        .iter()
        .filter_map(|line_item| {
            let pricing = &line_item["plan"]["pricingDetails"];
            match pricing["__typename"].as_str() {
                Some("AppUsagePricing") => pricing["cappedAmount"]["currencyCode"].as_str(),
                Some("AppRecurringPricing") => pricing["price"]["currencyCode"].as_str(),
                _ => None,
            }
        })
        .map(str::to_string)
        .collect()
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

pub(in crate::proxy) fn is_app_billing_local_read_document(query: &str) -> bool {
    query.contains("AppBillingLocalRead") || query.contains("AppInstallationIdLocalRead")
}

pub(in crate::proxy) fn is_app_access_scopes_read_document(query: &str) -> bool {
    query.contains("AppAccessScopesLocalRead")
}

pub(in crate::proxy) fn is_app_usage_record_read_document(query: &str) -> bool {
    query.contains("AppUsageRecordCreateCapRead")
}

pub(in crate::proxy) fn is_app_subscription_activation_document(query: &str) -> bool {
    [
        "AppSubscriptionCreateActivationReadback",
        "AppSubscriptionActivationRead",
    ]
    .iter()
    .any(|marker| query.contains(marker))
}

pub(in crate::proxy) fn is_fulfillment_service_lifecycle_document(query: &str) -> bool {
    let Some(operation) = parse_operation(query) else {
        return false;
    };
    match operation.operation_type {
        OperationType::Mutation => operation.root_fields.iter().all(|field| {
            matches!(
                field.as_str(),
                "fulfillmentServiceCreate"
                    | "fulfillmentServiceUpdate"
                    | "fulfillmentServiceDelete"
            )
        }),
        OperationType::Query => operation
            .root_fields
            .iter()
            .all(|field| matches!(field.as_str(), "fulfillmentService" | "location")),
        OperationType::Subscription => false,
    }
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
    let user_error_selection = nested_selected_fields(payload_selection, &["userErrors"]);
    selected_payload_json(payload_selection, |selection| {
        match selection.name.as_str() {
            "carrierService" => Some(if carrier.is_null() {
                Value::Null
            } else {
                selected_json(&carrier, carrier_selection)
            }),
            "userErrors" => Some(Value::Array(
                user_errors
                    .iter()
                    .map(|error| selected_json(error, &user_error_selection))
                    .collect(),
            )),
            _ => None,
        }
    })
}

pub(in crate::proxy) fn carrier_service_not_found_payload(
    payload_selection: &[SelectedField],
    code: &str,
) -> Value {
    carrier_service_payload_json(
        Value::Null,
        payload_selection,
        &[],
        vec![carrier_service_user_error(
            Value::Null,
            "The carrier or app could not be found.",
            code,
        )],
    )
}

pub(in crate::proxy) fn carrier_service_delete_payload(
    deleted_id: Value,
    payload_selection: &[SelectedField],
    user_errors: Vec<Value>,
) -> Value {
    let user_error_selection = nested_selected_fields(payload_selection, &["userErrors"]);
    selected_payload_json(payload_selection, |selection| {
        match selection.name.as_str() {
            "deletedId" => Some(deleted_id.clone()),
            "userErrors" => Some(Value::Array(
                user_errors
                    .iter()
                    .map(|error| selected_json(error, &user_error_selection))
                    .collect(),
            )),
            _ => None,
        }
    })
}

pub(in crate::proxy) fn carrier_service_user_error(
    field: Value,
    message: &str,
    code: &str,
) -> Value {
    json!({
        "field": field,
        "message": message,
        "code": code
    })
}

pub(in crate::proxy) fn carrier_service_callback_url_error(
    callback_url: &str,
    code: &str,
) -> Option<Value> {
    let trimmed = callback_url.trim();
    if trimmed.starts_with("http://") {
        return Some(carrier_service_user_error(
            Value::Null,
            "Shipping rate provider callback url must use HTTPS",
            code,
        ));
    }
    let Some(host) = carrier_service_https_callback_host(trimmed) else {
        return Some(carrier_service_user_error(
            Value::Null,
            "Shipping rate provider callback url invalid host",
            code,
        ));
    };
    if carrier_service_callback_host_is_disallowed(&host) {
        return Some(carrier_service_user_error(
            Value::Null,
            "Shipping rate provider callback url invalid host",
            code,
        ));
    }
    None
}

pub(in crate::proxy) fn carrier_service_create_callback_url_coercion_error(
    query: &str,
    field: &RootFieldSelection,
) -> Option<Value> {
    let RawArgumentValue::Variable {
        name: variable_name,
        value: Some(ResolvedValue::Object(input)),
    } = field.raw_arguments.get("input")?
    else {
        return None;
    };
    let problem = match input.get("callbackUrl") {
        None | Some(ResolvedValue::Null) => CarrierServiceCallbackUrlCoercionProblem::Missing,
        Some(ResolvedValue::String(value)) if value.is_empty() || !value.contains("://") => {
            CarrierServiceCallbackUrlCoercionProblem::MissingScheme(value.clone())
        }
        _ => return None,
    };
    let definition = variable_definition_info(query, variable_name);
    let type_display = definition
        .as_ref()
        .map(|definition| definition.type_display.clone())
        .unwrap_or_else(|| "DeliveryCarrierServiceCreateInput!".to_string());
    let location = definition
        .map(|definition| json!({ "line": definition.location.line, "column": definition.location.column }))
        .unwrap_or_else(|| json!({ "line": 1, "column": 1 }));
    let value = resolved_value_json(&ResolvedValue::Object(input.clone()));
    Some(problem.error(variable_name, &type_display, location, value))
}

enum CarrierServiceCallbackUrlCoercionProblem {
    Missing,
    MissingScheme(String),
}

impl CarrierServiceCallbackUrlCoercionProblem {
    fn error(
        &self,
        variable_name: &str,
        type_display: &str,
        location: Value,
        value: Value,
    ) -> Value {
        match self {
            Self::Missing => json!({
                "message": format!(
                    "Variable ${variable_name} of type {type_display} was provided invalid value for callbackUrl (Expected value to not be null)"
                ),
                "locations": [location],
                "extensions": {
                    "code": "INVALID_VARIABLE",
                    "value": value,
                    "problems": [{
                        "path": ["callbackUrl"],
                        "explanation": "Expected value to not be null"
                    }]
                }
            }),
            Self::MissingScheme(callback_url) => {
                let message = format!("Invalid url '{callback_url}', missing scheme");
                json!({
                    "message": format!(
                        "Variable ${variable_name} of type {type_display} was provided invalid value for callbackUrl ({message})"
                    ),
                    "locations": [location],
                    "extensions": {
                        "code": "INVALID_VARIABLE",
                        "value": value,
                        "problems": [{
                            "path": ["callbackUrl"],
                            "explanation": message,
                            "message": message
                        }]
                    }
                })
            }
        }
    }
}

pub(in crate::proxy) fn carrier_service_https_callback_host(callback_url: &str) -> Option<String> {
    let rest = callback_url.strip_prefix("https://")?;
    let host_with_port = rest.split(['/', '?', '#']).next().unwrap_or_default();
    let host = host_with_port
        .split('@')
        .next_back()
        .unwrap_or_default()
        .split(':')
        .next()
        .unwrap_or_default()
        .trim_matches('.')
        .to_ascii_lowercase();
    if host.is_empty()
        || host
            .chars()
            .any(|ch| ch.is_ascii_whitespace() || ch == '/' || ch == '\\')
    {
        return None;
    }
    Some(host)
}

pub(in crate::proxy) fn carrier_service_callback_host_is_disallowed(host: &str) -> bool {
    if host == "shopify.com"
        || host.ends_with(".shopify.com")
        || host.ends_with(".myshopify.com")
        || host.ends_with(".shopifypreview.com")
        || host.ends_with(".myshopify.dev")
        || host == "localhost"
    {
        return true;
    }
    if let Ok(std::net::IpAddr::V4(address)) = host.parse::<std::net::IpAddr>() {
        let octets = address.octets();
        return octets[0] == 0
            || octets[0] == 10
            || octets[0] == 127
            || (octets[0] == 172 && (16..=31).contains(&octets[1]))
            || (octets[0] == 192 && octets[1] == 168);
    }
    false
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

pub(in crate::proxy) fn normalize_taggable_tags(tags: Vec<String>) -> Vec<String> {
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

pub(in crate::proxy) fn normalize_product_tags(tags: Vec<String>) -> Vec<String> {
    normalize_taggable_tags(tags)
}

pub(in crate::proxy) fn normalized_taggable_tags_argument(
    value: Option<&ResolvedValue>,
) -> Vec<String> {
    let raw_tags = match value {
        Some(ResolvedValue::String(value)) => split_taggable_tag_argument(value),
        Some(ResolvedValue::List(values)) => values
            .iter()
            .flat_map(|value| match value {
                ResolvedValue::String(value) => split_taggable_tag_argument(value),
                _ => Vec::new(),
            })
            .collect(),
        _ => Vec::new(),
    };
    normalize_taggable_tags(raw_tags)
}

pub(in crate::proxy) fn add_taggable_tags(
    existing: Vec<String>,
    incoming: Vec<String>,
) -> Vec<String> {
    normalize_taggable_tags(existing.into_iter().chain(incoming).collect())
}

pub(in crate::proxy) fn remove_taggable_tags(
    existing: Vec<String>,
    removals: Vec<String>,
) -> Vec<String> {
    let remove_handles: BTreeSet<String> = removals.iter().map(|tag| tag.to_lowercase()).collect();
    normalize_taggable_tags(existing)
        .into_iter()
        .filter(|tag| !remove_handles.contains(&tag.to_lowercase()))
        .collect()
}

fn split_taggable_tag_argument(value: &str) -> Vec<String> {
    value.split(',').map(str::to_string).collect()
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
        let mut feedback = Vec::new();
        let mut user_errors = Vec::new();
        for (index, input) in inputs.iter().enumerate() {
            if let Some(error) = resource_feedback_validation_error(input, Some(index)) {
                user_errors.push(error);
            } else {
                feedback.push(product_resource_feedback_json(input));
            }
        }
        json!({ "feedback": feedback, "userErrors": user_errors })
    };
    selected_json(&payload, &field.selection)
}

pub(in crate::proxy) fn product_tail_shop_feedback_payload(field: &RootFieldSelection) -> Value {
    let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
    let payload = if let Some(error) = resource_feedback_validation_error(&input, None) {
        json!({
            "feedback": null,
            "userErrors": [error]
        })
    } else {
        json!({ "feedback": shop_resource_feedback_json(&input), "userErrors": [] })
    };
    selected_json(&payload, &field.selection)
}

fn product_resource_feedback_json(input: &BTreeMap<String, ResolvedValue>) -> Value {
    json!({
        "productId": resolved_string_field(input, "productId").unwrap_or_default(),
        "state": resolved_string_field(input, "state").unwrap_or_default(),
        "messages": resolved_string_list_field_unsorted(input, "messages"),
        "feedbackGeneratedAt": resolved_string_field(input, "feedbackGeneratedAt").unwrap_or_default(),
        "productUpdatedAt": resolved_string_field(input, "productUpdatedAt").unwrap_or_default()
    })
}

fn shop_resource_feedback_json(input: &BTreeMap<String, ResolvedValue>) -> Value {
    let messages = resolved_string_list_field_unsorted(input, "messages")
        .into_iter()
        .map(|message| json!({ "message": message }))
        .collect::<Vec<_>>();
    json!({
        "state": resolved_string_field(input, "state").unwrap_or_default(),
        "messages": messages,
        "feedbackGeneratedAt": resolved_string_field(input, "feedbackGeneratedAt").unwrap_or_default()
    })
}

fn resource_feedback_validation_error(
    input: &BTreeMap<String, ResolvedValue>,
    feedback_index: Option<usize>,
) -> Option<Value> {
    let messages = resolved_string_list_field_unsorted(input, "messages");
    if messages.is_empty() {
        return Some(resource_feedback_user_error(
            feedback_field_path(feedback_index, "messages", None),
            "Messages can't be blank",
            "BLANK",
        ));
    }

    let generated_at = resolved_string_field(input, "feedbackGeneratedAt").unwrap_or_default();
    if feedback_generated_at_is_future(&generated_at) {
        return Some(resource_feedback_user_error(
            feedback_field_path(feedback_index, "feedbackGeneratedAt", None),
            "Feedback generated at must not be in the future",
            "INVALID",
        ));
    }

    messages
        .iter()
        .position(|message| message.chars().count() > 100)
        .map(|message_index| {
            resource_feedback_user_error(
                feedback_field_path(feedback_index, "messages", Some(message_index)),
                "Message is too long (maximum is 100 characters)",
                "TOO_LONG",
            )
        })
}

fn feedback_field_path(
    feedback_index: Option<usize>,
    field: &str,
    nested_index: Option<usize>,
) -> Vec<String> {
    let mut path = vec!["feedback".to_string()];
    if let Some(index) = feedback_index {
        path.push(index.to_string());
    }
    path.push(field.to_string());
    if let Some(index) = nested_index {
        path.push(index.to_string());
    }
    path
}

fn resource_feedback_user_error(field: Vec<String>, message: &str, code: &str) -> Value {
    json!({
        "field": field,
        "message": message,
        "code": code
    })
}

fn feedback_generated_at_is_future(generated_at: &str) -> bool {
    let Some(generated_at) = parse_rfc3339_epoch_seconds(generated_at) else {
        return false;
    };
    let Ok(now) = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) else {
        return false;
    };
    generated_at > now.as_secs() as i64
}

fn parse_rfc3339_epoch_seconds(value: &str) -> Option<i64> {
    let bytes = value.as_bytes();
    if bytes.len() < 20 {
        return None;
    }

    let year = parse_fixed_digits(bytes, 0, 4)?;
    expect_byte(bytes, 4, b'-')?;
    let month = parse_fixed_digits(bytes, 5, 2)? as u32;
    expect_byte(bytes, 7, b'-')?;
    let day = parse_fixed_digits(bytes, 8, 2)? as u32;
    match bytes.get(10) {
        Some(b'T' | b't') => {}
        _ => return None,
    }
    let hour = parse_fixed_digits(bytes, 11, 2)? as u32;
    expect_byte(bytes, 13, b':')?;
    let minute = parse_fixed_digits(bytes, 14, 2)? as u32;
    expect_byte(bytes, 16, b':')?;
    let second = parse_fixed_digits(bytes, 17, 2)? as u32;

    if !valid_utc_date_time(year, month, day, hour, minute, second) {
        return None;
    }

    let mut offset_index = 19;
    if bytes.get(offset_index) == Some(&b'.') {
        offset_index += 1;
        let fraction_start = offset_index;
        while bytes
            .get(offset_index)
            .is_some_and(|byte| byte.is_ascii_digit())
        {
            offset_index += 1;
        }
        if offset_index == fraction_start {
            return None;
        }
    }

    let offset_seconds = match bytes.get(offset_index) {
        Some(b'Z' | b'z') if offset_index + 1 == bytes.len() => 0,
        Some(b'+' | b'-') if offset_index + 6 == bytes.len() => {
            let sign = if bytes[offset_index] == b'+' { 1 } else { -1 };
            let offset_hour = parse_fixed_digits(bytes, offset_index + 1, 2)?;
            expect_byte(bytes, offset_index + 3, b':')?;
            let offset_minute = parse_fixed_digits(bytes, offset_index + 4, 2)?;
            if offset_hour > 23 || offset_minute > 59 {
                return None;
            }
            sign * (offset_hour * 3600 + offset_minute * 60)
        }
        _ => return None,
    };

    let days = days_from_civil(year, month, day);
    Some(days * 86_400 + i64::from(hour * 3600 + minute * 60 + second) - i64::from(offset_seconds))
}

fn parse_fixed_digits(bytes: &[u8], start: usize, len: usize) -> Option<i32> {
    let end = start.checked_add(len)?;
    let digits = bytes.get(start..end)?;
    digits.iter().try_fold(0_i32, |value, byte| {
        if byte.is_ascii_digit() {
            Some(value * 10 + i32::from(byte - b'0'))
        } else {
            None
        }
    })
}

fn expect_byte(bytes: &[u8], index: usize, expected: u8) -> Option<()> {
    (bytes.get(index) == Some(&expected)).then_some(())
}

fn valid_utc_date_time(
    year: i32,
    month: u32,
    day: u32,
    hour: u32,
    minute: u32,
    second: u32,
) -> bool {
    (1..=12).contains(&month)
        && day >= 1
        && day <= days_in_month(year, month)
        && hour <= 23
        && minute <= 59
        && second <= 60
}

fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(year) => 29,
        2 => 28,
        _ => 0,
    }
}

fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

fn days_from_civil(year: i32, month: u32, day: u32) -> i64 {
    let year = year - i32::from(month <= 2);
    let era = (if year >= 0 { year } else { year - 399 }) / 400;
    let year_of_era = year - era * 400;
    let month = month as i32;
    let day_of_year = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day as i32 - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    i64::from(era) * 146_097 + i64::from(day_of_era) - 719_468
}

pub(in crate::proxy) fn request_api_client_id(request: &Request) -> String {
    request_header(request, "x-shopify-draft-proxy-api-client-id")
        .unwrap_or_else(|| "gid://shopify/App/local".to_string())
}

pub(in crate::proxy) fn set_log_status(entry: &mut Value, status: &str) {
    if let Value::Object(fields) = entry {
        fields.insert("status".to_string(), json!(status));
    }
}

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
        "shopify" | "amazon" | "gift_card" | "manual"
    )
}

pub(in crate::proxy) fn fulfillment_service_name_whitespace_errors(name: &str) -> Vec<Value> {
    let mut errors = Vec::new();
    if name.starts_with(char::is_whitespace) {
        errors.push(user_error_omit_code(
            ["name"],
            "Name cannot begin with a whitespace character",
            None,
        ));
    }
    if name.ends_with(char::is_whitespace) {
        errors.push(user_error_omit_code(
            ["name"],
            "Name cannot end with a whitespace character",
            None,
        ));
    }
    errors
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
    url::Url::parse(shopify_admin_origin)
        .ok()
        .and_then(|url| url.host_str().map(str::to_ascii_lowercase))
        .filter(|host| host.ends_with(".myshopify.com"))
}

pub(in crate::proxy) fn delegate_access_token_create_payload_json(
    token: Value,
    shop: &Value,
    payload_selection: &[SelectedField],
    token_selection: &[SelectedField],
    user_errors: Vec<Value>,
) -> Value {
    selected_single_data_field_payload_json(
        "delegateAccessToken",
        token,
        token_selection,
        "UserError",
        payload_selection,
        user_errors,
        |selection| match selection.name.as_str() {
            "shop" => Some(selected_json(shop, &selection.selection)),
            _ => None,
        },
    )
}

pub(in crate::proxy) fn delegate_access_token_destroy_payload_json(
    status: bool,
    shop: &Value,
    user_errors: Vec<Value>,
    payload_selection: &[SelectedField],
) -> Value {
    selected_payload_json(payload_selection, |selection| {
        match selection.name.as_str() {
            "status" => Some(Value::Bool(status)),
            "shop" => Some(selected_json(shop, &selection.selection)),
            "userErrors" => Some(app_user_errors_json(
                user_errors.clone(),
                "UserError",
                &selection.selection,
            )),
            _ => None,
        }
    })
}

pub(in crate::proxy) const DEFAULT_LOCAL_APP_ID: &str = "gid://shopify/App/local";
pub(in crate::proxy) const DEFAULT_LOCAL_APP_INSTALLATION_ID: &str =
    "gid://shopify/AppInstallation/local";
pub(in crate::proxy) const DRAFT_PROXY_REQUEST_APP_ID_FIELD: &str = "__draftProxyRequestAppId";

pub(in crate::proxy) fn normalize_app_gid(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        DEFAULT_LOCAL_APP_ID.to_string()
    } else if is_shopify_gid_of_type(trimmed, "App") {
        trimmed.to_string()
    } else {
        shopify_gid("App", trimmed)
    }
}

pub(in crate::proxy) fn normalize_app_installation_gid(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        DEFAULT_LOCAL_APP_INSTALLATION_ID.to_string()
    } else if is_shopify_gid_of_type(trimmed, "AppInstallation") {
        trimmed.to_string()
    } else {
        shopify_gid("AppInstallation", trimmed)
    }
}

pub(in crate::proxy) fn request_app_gid(request: &Request) -> String {
    normalize_app_gid(&request_api_client_id(request))
}

pub(in crate::proxy) fn app_id_from_installation(installation: &Value) -> Option<String> {
    installation
        .get("app")
        .and_then(|app| app.get("id"))
        .and_then(Value::as_str)
        .map(str::to_string)
}

pub(in crate::proxy) fn app_installation_id(installation: &Value) -> Option<String> {
    installation
        .get("id")
        .and_then(Value::as_str)
        .map(str::to_string)
}

pub(in crate::proxy) fn request_app_id_from_installation(installation: &Value) -> Option<String> {
    installation
        .get(DRAFT_PROXY_REQUEST_APP_ID_FIELD)
        .and_then(Value::as_str)
        .map(str::to_string)
}

pub(in crate::proxy) fn current_app_installation_from_request(request: &Request) -> Value {
    let explicit_app_id = request_header(request, API_CLIENT_ID_HEADER);
    let app_id = normalize_app_gid(explicit_app_id.as_deref().unwrap_or(DEFAULT_LOCAL_APP_ID));
    let installation_id = request_header(request, "x-shopify-draft-proxy-app-installation-id")
        .map(|value| normalize_app_installation_gid(&value))
        .unwrap_or_else(|| {
            if explicit_app_id.is_some() {
                synthetic_shopify_gid("AppInstallation", resource_id_tail(&app_id))
            } else {
                DEFAULT_LOCAL_APP_INSTALLATION_ID.to_string()
            }
        });
    let handle = request_header(request, "x-shopify-draft-proxy-app-handle")
        .unwrap_or_else(|| "shopify-draft-proxy".to_string());
    let title = request_header(request, "x-shopify-draft-proxy-app-title")
        .unwrap_or_else(|| handle.clone());
    let access_scopes = request_access_scope_values(request).unwrap_or_else(|| {
        vec![
            access_scope_json("read_products", None),
            access_scope_json("write_products", None),
        ]
    });
    let requested_access_scopes =
        request_required_access_scope_values(request).unwrap_or_else(|| {
            if explicit_app_id.is_some() || request_header(request, ACCESS_SCOPES_HEADER).is_some()
            {
                Vec::new()
            } else {
                vec![access_scope_json("read_products", None)]
            }
        });
    json!({
        "__typename": "AppInstallation",
        "__draftProxySource": if explicit_app_id.is_some() { "request" } else { "default" },
        "__draftProxyRequestAppId": app_id.clone(),
        "id": installation_id,
        "accessScopes": access_scopes,
        "app": {
            "__typename": "App",
            "id": app_id,
            "handle": handle,
            "title": title,
            "requestedAccessScopes": requested_access_scopes
        }
    })
}

fn request_access_scope_values(request: &Request) -> Option<Vec<Value>> {
    request_header(request, ACCESS_SCOPES_HEADER)
        .map(|header| access_scope_values_from_header(&header))
        .filter(|scopes| !scopes.is_empty())
}

fn request_required_access_scope_values(request: &Request) -> Option<Vec<Value>> {
    request_header(request, "x-shopify-draft-proxy-required-access-scopes")
        .map(|header| access_scope_values_from_header(&header))
}

fn access_scope_handles_from_header(header: &str) -> Vec<String> {
    header
        .split(',')
        .map(str::trim)
        .filter(|scope| !scope.is_empty())
        .map(str::to_string)
        .collect()
}

fn access_scope_values_from_header(header: &str) -> Vec<Value> {
    access_scope_handles_from_header(header)
        .into_iter()
        .map(|scope| access_scope_json(&scope, None))
        .collect()
}

pub(in crate::proxy) fn access_scope_json(handle: &str, description: Option<&str>) -> Value {
    json!({
        "handle": handle,
        "description": description.map(Value::from).unwrap_or(Value::Null)
    })
}

pub(in crate::proxy) fn app_access_scope_handles(installation: &Value) -> BTreeSet<String> {
    installation
        .get("accessScopes")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|scope| scope.get("handle").and_then(Value::as_str))
        .map(str::to_string)
        .collect()
}

pub(in crate::proxy) fn app_required_access_scope_handles(
    installation: &Value,
) -> BTreeSet<String> {
    installation
        .pointer("/app/requestedAccessScopes")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|scope| scope.get("handle").and_then(Value::as_str))
        .map(str::to_string)
        .collect()
}

pub(in crate::proxy) fn app_access_scope_value(installation: &Value, handle: &str) -> Value {
    installation
        .get("accessScopes")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find(|scope| scope.get("handle").and_then(Value::as_str) == Some(handle))
        .cloned()
        .unwrap_or_else(|| access_scope_json(handle, None))
}

pub(in crate::proxy) fn merge_app_installation_json(base: &Value, observed: &Value) -> Value {
    let mut merged = base.clone();
    let Some(observed_object) = observed.as_object() else {
        return merged;
    };
    let Some(merged_object) = merged.as_object_mut() else {
        return observed.clone();
    };
    for (key, value) in observed_object {
        if key == "app" {
            let mut app = merged_object
                .get("app")
                .cloned()
                .unwrap_or_else(|| json!({}));
            if let (Some(app_object), Some(observed_app)) = (app.as_object_mut(), value.as_object())
            {
                for (app_key, app_value) in observed_app {
                    if !app_value.is_null() {
                        app_object.insert(app_key.clone(), app_value.clone());
                    }
                }
                merged_object.insert("app".to_string(), app);
            } else if !value.is_null() {
                merged_object.insert(key.clone(), value.clone());
            }
        } else if !value.is_null() {
            merged_object.insert(key.clone(), value.clone());
        }
    }
    merged
}

pub(in crate::proxy) fn app_uninstall_payload_json(
    app: Value,
    payload_selection: &[SelectedField],
    app_selection: &[SelectedField],
    user_errors: Vec<Value>,
) -> Value {
    selected_single_data_field_payload_json(
        "app",
        app,
        app_selection,
        "AppUninstallError",
        payload_selection,
        user_errors,
        |_| None,
    )
}

pub(in crate::proxy) fn app_revoke_access_scopes_payload_json(
    revoked: Option<Vec<Value>>,
    user_errors: Vec<Value>,
    payload_selection: &[SelectedField],
) -> Value {
    selected_payload_json(payload_selection, |selection| {
        match selection.name.as_str() {
            "revoked" => Some(match &revoked {
                Some(scopes) => Value::Array(
                    scopes
                        .iter()
                        .map(|scope| selected_json(scope, &selection.selection))
                        .collect(),
                ),
                None => Value::Null,
            }),
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
    selected_single_data_field_payload_json(
        "appUsageRecord",
        usage_record,
        usage_record_selection,
        "UserError",
        payload_selection,
        user_errors,
        |_| None,
    )
}

pub(in crate::proxy) fn app_purchase_one_time_payload_json(
    purchase: Value,
    payload_selection: &[SelectedField],
    purchase_selection: &[SelectedField],
    user_errors: Vec<Value>,
    confirmation_url: Option<Value>,
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
                confirmation_url.clone().unwrap_or(Value::Null)
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
    confirmation_url: Value,
) -> Value {
    app_subscription_payload_json_with_confirmation_url(
        subscription.clone(),
        payload_selection,
        subscription_selection,
        vec![],
        Some(confirmation_url),
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
        None,
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

pub(in crate::proxy) fn selected_single_data_field_payload_json(
    field_name: &'static str,
    field_value: Value,
    field_selection: &[SelectedField],
    user_error_typename: &'static str,
    payload_selection: &[SelectedField],
    user_errors: Vec<Value>,
    extra_field: impl Fn(&SelectedField) -> Option<Value>,
) -> Value {
    selected_payload_json(payload_selection, |selection| {
        if selection.name == field_name {
            Some(if field_value.is_null() {
                Value::Null
            } else {
                selected_json(&field_value, field_selection)
            })
        } else if selection.name == "userErrors" {
            Some(app_user_errors_json(
                user_errors.clone(),
                user_error_typename,
                &selection.selection,
            ))
        } else {
            extra_field(selection)
        }
    })
}

pub(in crate::proxy) fn failed_payload_outcome(
    payload: Value,
) -> (Value, &'static str, Vec<String>) {
    (payload, "failed", Vec::new())
}

pub(in crate::proxy) fn response_is_success(response: &Response) -> bool {
    (200..300).contains(&response.status)
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
    line_item_ids: &[String],
) -> Vec<Value> {
    match arguments.get("lineItems") {
        Some(ResolvedValue::List(items)) => items
            .iter()
            .enumerate()
            .map(|(index, item)| {
                app_subscription_line_item_from_input(
                    item,
                    line_item_ids.get(index).cloned().unwrap_or_default(),
                )
            })
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

fn app_subscription_line_item_from_input(value: &ResolvedValue, id: String) -> Value {
    if let ResolvedValue::Object(item) = value {
        if let Some(ResolvedValue::Object(plan)) = item.get("plan") {
            if let Some(ResolvedValue::Object(details)) = plan.get("appRecurringPricingDetails") {
                let price = resolved_object_field(details, "price").unwrap_or_default();
                let price_amount = money_amount_string_from_resolved_or(price.get("amount"), "0.0");
                let price_currency =
                    resolved_string_field(&price, "currencyCode").unwrap_or_default();
                return json!({
                    "id": id,
                    "plan": { "pricingDetails": {
                        "__typename": "AppRecurringPricing",
                        "price": money_value(&price_amount, &price_currency)
                    }}
                });
            }
            if let Some(ResolvedValue::Object(details)) = plan.get("appUsagePricingDetails") {
                let capped = resolved_object_field(details, "cappedAmount").unwrap_or_default();
                let capped_amount =
                    money_amount_string_from_resolved_or(capped.get("amount"), "0.0");
                let currency_code =
                    resolved_string_field(&capped, "currencyCode").unwrap_or_default();
                let terms = resolved_string_field(details, "terms").unwrap_or_default();
                return json!({
                    "id": id,
                    "plan": { "pricingDetails": {
                        "__typename": "AppUsagePricing",
                        "cappedAmount": money_value(&capped_amount, &currency_code),
                        "balanceUsed": money_value("0.0", &currency_code),
                        "interval": "EVERY_30_DAYS",
                        "terms": terms
                    }}
                });
            }
        }
    }
    json!({
        "id": id,
        "plan": { "pricingDetails": {
            "__typename": "AppUsagePricing",
            "cappedAmount": money_value("0.0", ""),
            "balanceUsed": money_value("0.0", ""),
            "interval": "EVERY_30_DAYS",
            "terms": ""
        }}
    })
}

pub(in crate::proxy) fn current_app_installation_json(
    installation: &Value,
    subscriptions: &BTreeMap<String, Value>,
    one_time_purchases: &BTreeMap<String, Value>,
    revoked_access_scopes: &BTreeSet<String>,
    selections: &[SelectedField],
) -> Value {
    let mut fields = serde_json::Map::new();
    for selection in selections {
        let value = match selection.name.as_str() {
            "id" => app_installation_id(installation).map(Value::String),
            "__typename" => Some(json!("AppInstallation")),
            "app" => installation
                .get("app")
                .map(|app| selected_json(app, &selection.selection)),
            "activeSubscriptions" if subscriptions.is_empty() => Some(
                installation
                    .get("activeSubscriptions")
                    .map(|value| selected_json(value, &selection.selection))
                    .unwrap_or_else(|| Value::Array(Vec::new())),
            ),
            "activeSubscriptions" => Some(Value::Array(
                subscriptions
                    .values()
                    .filter(|subscription| subscription["status"] == "ACTIVE")
                    .map(|subscription| selected_json(subscription, &selection.selection))
                    .collect(),
            )),
            "allSubscriptions" => Some(app_installation_connection_field(
                installation,
                "allSubscriptions",
                subscriptions.is_empty(),
                subscriptions.values(),
                selection,
            )),
            "oneTimePurchases" => Some(app_installation_connection_field(
                installation,
                "oneTimePurchases",
                one_time_purchases.is_empty(),
                one_time_purchases.values(),
                selection,
            )),
            "accessScopes" => Some(Value::Array(
                installation
                    .get("accessScopes")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter(|scope| {
                        scope
                            .get("handle")
                            .and_then(Value::as_str)
                            .is_none_or(|handle| !revoked_access_scopes.contains(handle))
                    })
                    .map(|scope| selected_json(scope, &selection.selection))
                    .collect(),
            )),
            _ => installation
                .get(selection.name.as_str())
                .filter(|_| !selection.name.starts_with("__draftProxy"))
                .map(|value| selected_json(value, &selection.selection)),
        };
        if let Some(value) = value {
            fields.insert(selection.response_key.clone(), value);
        }
    }
    Value::Object(fields)
}

fn app_installation_connection_field<'a>(
    installation: &Value,
    field_name: &str,
    records_empty: bool,
    records: impl Iterator<Item = &'a Value>,
    selection: &SelectedField,
) -> Value {
    if records_empty {
        if let Some(value) = installation.get(field_name) {
            return selected_json(value, &selection.selection);
        }
    }
    let node_selection =
        selected_child_selection(&selection.selection, "nodes").unwrap_or_default();
    json!({
        "nodes": records
            .map(|record| selected_json(record, &node_selection))
            .collect::<Vec<_>>()
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
            "locationDeactivateUserErrors" | "userErrors" => {
                selected_user_errors_field(user_errors.as_slice(), selection)
            }
            _ => None,
        }
    })
}

pub(in crate::proxy) fn delivery_profile_payload_json(
    profile: Value,
    payload_selection: &[SelectedField],
    user_errors: Vec<Value>,
) -> Value {
    selected_payload_json(payload_selection, |selection| {
        match selection.name.as_str() {
            "profile" => Some(if profile.is_null() {
                Value::Null
            } else {
                delivery_profile_selected_json(&profile, &selection.selection)
            }),
            "userErrors" => selected_user_errors_field(user_errors.as_slice(), selection),
            _ => None,
        }
    })
}

pub(in crate::proxy) fn delivery_profile_remove_payload_json(
    job: Value,
    payload_selection: &[SelectedField],
    user_errors: Vec<Value>,
) -> Value {
    selected_payload_json(payload_selection, |selection| {
        match selection.name.as_str() {
            "job" => Some(if job.is_null() {
                Value::Null
            } else {
                selected_json(&job, &selection.selection)
            }),
            "userErrors" => selected_user_errors_field(user_errors.as_slice(), selection),
            _ => None,
        }
    })
}

pub(in crate::proxy) fn delivery_profile_create_user_errors(
    profile: &BTreeMap<String, ResolvedValue>,
    location_exists: &mut impl FnMut(&str) -> bool,
) -> Vec<Value> {
    if let Some(error) = delivery_profile_name_user_error(profile) {
        return vec![error];
    }
    if !list_string_field(profile, "variantsToDissociate").is_empty() {
        return vec![user_error_omit_code(
            Value::Null,
            "Cannot disassociate variants when creating a profile.",
            None,
        )];
    }
    for group in resolved_object_list_field(profile, "locationGroupsToCreate") {
        if !resolved_object_list_field(&group, "zonesToUpdate").is_empty() {
            return vec![user_error_omit_code(
                Value::Null,
                "Cannot update zones when creating a profile.",
                None,
            )];
        }
        for zone in resolved_object_list_field(&group, "zonesToCreate") {
            if !resolved_object_list_field(&zone, "methodDefinitionsToUpdate").is_empty() {
                return vec![user_error_omit_code(
                    Value::Null,
                    "Profile is invalid: Input cannot include method_definitions_to_update on create.",
                    None,
                )];
            }
        }
    }
    delivery_profile_common_shape_user_errors(profile, location_exists)
}

pub(in crate::proxy) fn delivery_profile_update_user_errors(
    profile: &BTreeMap<String, ResolvedValue>,
    location_exists: &mut impl FnMut(&str) -> bool,
) -> Vec<Value> {
    if let Some(error) = delivery_profile_name_user_error(profile) {
        return vec![error];
    }
    delivery_profile_common_shape_user_errors(profile, location_exists)
}

const DELIVERY_PROFILE_MAX_NAME_LENGTH: usize = 128;
const DELIVERY_PROFILE_NAME_TOO_LONG_MESSAGE: &str =
    "Profile name must be less than 128 characters long";

fn delivery_profile_name_user_error(profile: &BTreeMap<String, ResolvedValue>) -> Option<Value> {
    let name = resolved_string_field(profile, "name")?;
    if name.is_empty() {
        return Some(user_error_omit_code(
            json!(["profile", "name"]),
            "Add a profile name",
            None,
        ));
    }
    if name.chars().count() > DELIVERY_PROFILE_MAX_NAME_LENGTH {
        return Some(user_error_omit_code(
            json!(["profile", "name"]),
            DELIVERY_PROFILE_NAME_TOO_LONG_MESSAGE,
            None,
        ));
    }
    None
}

fn delivery_profile_common_shape_user_errors(
    profile: &BTreeMap<String, ResolvedValue>,
    location_exists: &mut impl FnMut(&str) -> bool,
) -> Vec<Value> {
    for group in resolved_object_list_field(profile, "locationGroupsToCreate") {
        if delivery_profile_has_unknown_location(
            &list_string_field(&group, "locations"),
            location_exists,
        ) {
            return vec![delivery_profile_unknown_location_user_error()];
        }
        for zone in resolved_object_list_field(&group, "zonesToCreate") {
            if delivery_profile_zone_countries_from_input(&zone).is_empty() {
                return vec![user_error_omit_code(
                    Value::Null,
                    "Profile is invalid: cannot create LocationGroupZone without countries.",
                    None,
                )];
            }
        }
    }
    for group in resolved_object_list_field(profile, "locationGroupsToUpdate") {
        if delivery_profile_has_unknown_location(
            &list_string_field(&group, "locationsToAdd"),
            location_exists,
        ) {
            return vec![delivery_profile_unknown_location_user_error()];
        }
    }
    Vec::new()
}

fn delivery_profile_has_unknown_location(
    location_ids: &[String],
    location_exists: &mut impl FnMut(&str) -> bool,
) -> bool {
    location_ids.iter().any(|id| !location_exists(id))
}

fn delivery_profile_unknown_location_user_error() -> Value {
    user_error_omit_code(
        Value::Null,
        "The Location could not be found for this shop.",
        None,
    )
}

pub(in crate::proxy) fn delivery_profile_selected_json(
    profile: &Value,
    selections: &[SelectedField],
) -> Value {
    selected_payload_json(selections, |selection| match selection.name.as_str() {
        "__typename" => Some(json!("DeliveryProfile")),
        "id"
        | "name"
        | "default"
        | "version"
        | "originLocationCount"
        | "zoneCountryCount"
        | "activeMethodDefinitionsCount"
        | "locationsWithoutRatesCount" => profile
            .get(&selection.name)
            .cloned()
            .map(|value| nullable_selected_json(&value, &selection.selection)),
        "productVariantsCount" => {
            let default_count = count_object(0);
            Some(selected_json(
                profile
                    .get("productVariantsCount")
                    .unwrap_or(&default_count),
                &selection.selection,
            ))
        }
        "profileItems" => Some(delivery_profile_items_connection_json(
            profile
                .get("profileItems")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default(),
            &selection.arguments,
            &selection.selection,
        )),
        "profileLocationGroups" => Some(Value::Array(
            profile
                .get("profileLocationGroups")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .map(|group| {
                    delivery_profile_location_group_selected_json(group, &selection.selection)
                })
                .collect(),
        )),
        "sellingPlanGroups" => Some(selected_empty_connection_json(&selection.selection)),
        "unassignedLocationsPaginated" => {
            Some(selected_empty_connection_json(&selection.selection))
        }
        "unassignedLocations" => Some(Value::Array(Vec::new())),
        _ => profile
            .get(&selection.name)
            .cloned()
            .map(|value| nullable_selected_json(&value, &selection.selection)),
    })
}

fn delivery_profile_location_group_selected_json(
    group: &Value,
    selections: &[SelectedField],
) -> Value {
    selected_payload_json(selections, |selection| match selection.name.as_str() {
        "locationGroup" => Some(delivery_location_group_selected_json(
            &group["locationGroup"],
            &selection.selection,
        )),
        "locationGroupZones" => Some(delivery_location_group_zones_connection_json(
            group
                .get("locationGroupZones")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default(),
            &selection.arguments,
            &selection.selection,
        )),
        "countriesInAnyZone" => {
            let stored = group
                .get("countriesInAnyZone")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            let countries = if stored.is_empty() {
                delivery_profile_countries_in_any_zone(group)
            } else {
                stored
            };
            Some(Value::Array(
                countries
                    .into_iter()
                    .map(|country| selected_json(&country, &selection.selection))
                    .collect(),
            ))
        }
        _ => None,
    })
}

fn delivery_profile_countries_in_any_zone(group: &Value) -> Vec<Value> {
    let mut seen = BTreeSet::new();
    let mut countries = Vec::new();
    for zone in group["locationGroupZones"].as_array().into_iter().flatten() {
        let zone_name = zone["zone"]["name"].as_str().unwrap_or_default();
        for country in zone["zone"]["countries"].as_array().into_iter().flatten() {
            let key = delivery_profile_country_union_key(country);
            if key.is_empty() || !seen.insert(key) {
                continue;
            }
            countries.push(json!({
                "zone": zone_name,
                "country": country
            }));
        }
    }
    countries
}

fn delivery_profile_country_union_key(country: &Value) -> String {
    if country["code"]["restOfWorld"].as_bool() == Some(true) {
        return "REST_OF_WORLD".to_string();
    }
    country["code"]["countryCode"]
        .as_str()
        .or_else(|| country.get("id").and_then(Value::as_str))
        .unwrap_or_default()
        .to_string()
}

fn delivery_location_group_selected_json(group: &Value, selections: &[SelectedField]) -> Value {
    selected_payload_json(selections, |selection| match selection.name.as_str() {
        "id" => group.get("id").cloned(),
        "locations" => Some(delivery_profile_locations_connection_json(
            group
                .get("locations")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default(),
            &selection.arguments,
            &selection.selection,
        )),
        "locationsCount" => {
            let default_count = count_object(0);
            Some(selected_json(
                group.get("locationsCount").unwrap_or(&default_count),
                &selection.selection,
            ))
        }
        _ => group
            .get(&selection.name)
            .cloned()
            .map(|value| nullable_selected_json(&value, &selection.selection)),
    })
}

fn delivery_location_group_zones_connection_json(
    zones: Vec<Value>,
    arguments: &BTreeMap<String, ResolvedValue>,
    selections: &[SelectedField],
) -> Value {
    let nodes = limited_nodes(zones, arguments);
    selected_typed_connection(
        &nodes,
        selections,
        delivery_location_group_zone_selected_json,
        |node| node["zone"]["id"].as_str().unwrap_or_default().to_string(),
        |selections| selected_json(&empty_page_info(), selections),
    )
}

fn delivery_location_group_zone_selected_json(zone: &Value, selections: &[SelectedField]) -> Value {
    selected_payload_json(selections, |selection| match selection.name.as_str() {
        "zone" => Some(delivery_zone_selected_json(
            &zone["zone"],
            &selection.selection,
        )),
        "methodDefinitions" => Some(delivery_method_definitions_connection_json(
            zone.get("methodDefinitions")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default(),
            &selection.arguments,
            &selection.selection,
        )),
        _ => None,
    })
}

fn delivery_zone_selected_json(zone: &Value, selections: &[SelectedField]) -> Value {
    selected_payload_json(selections, |selection| match selection.name.as_str() {
        "id" | "name" => zone.get(&selection.name).cloned(),
        "countries" => Some(Value::Array(
            zone.get("countries")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .map(|country| selected_json(country, &selection.selection))
                .collect(),
        )),
        _ => None,
    })
}

fn delivery_method_definitions_connection_json(
    methods: Vec<Value>,
    arguments: &BTreeMap<String, ResolvedValue>,
    selections: &[SelectedField],
) -> Value {
    let nodes = limited_nodes(methods, arguments);
    selected_typed_connection(
        &nodes,
        selections,
        delivery_method_definition_selected_json,
        value_id_cursor,
        |selections| selected_json(&empty_page_info(), selections),
    )
}

fn delivery_method_definition_selected_json(method: &Value, selections: &[SelectedField]) -> Value {
    selected_payload_json(selections, |selection| match selection.name.as_str() {
        "id" | "name" | "active" | "description" => method.get(&selection.name).cloned(),
        "rateProvider" => Some(delivery_rate_provider_selected_json(
            &method["rateProvider"],
            &selection.selection,
        )),
        "methodConditions" => Some(Value::Array(
            method
                .get("methodConditions")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .map(|condition| selected_json(condition, &selection.selection))
                .collect(),
        )),
        _ => None,
    })
}

fn delivery_rate_provider_selected_json(
    rate_provider: &Value,
    selections: &[SelectedField],
) -> Value {
    selected_payload_json(selections, |selection| match selection.name.as_str() {
        "__typename" | "id" => rate_provider.get(&selection.name).cloned(),
        "price" => Some(selected_json(&rate_provider["price"], &selection.selection)),
        "fixedFee" | "percentageOfRateFee" => rate_provider.get(&selection.name).cloned(),
        _ => None,
    })
}

fn delivery_profile_items_connection_json(
    items: Vec<Value>,
    arguments: &BTreeMap<String, ResolvedValue>,
    selections: &[SelectedField],
) -> Value {
    let nodes = limited_nodes(items, arguments);
    let connection = connection_json_with_cursor(
        nodes,
        |index, node| {
            node["product"]["id"]
                .as_str()
                .map(str::to_string)
                .unwrap_or_else(|| format!("delivery-profile-item-{index}"))
        },
        connection_page_info(false, false, None, None),
    );
    selected_payload_json(selections, |selection| match selection.name.as_str() {
        "nodes" => Some(Value::Array(
            connection["nodes"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|item| delivery_profile_item_selected_json(item, &selection.selection))
                .collect(),
        )),
        "pageInfo" => Some(selected_json(&connection["pageInfo"], &selection.selection)),
        _ => None,
    })
}

fn delivery_profile_item_selected_json(item: &Value, selections: &[SelectedField]) -> Value {
    selected_payload_json(selections, |selection| match selection.name.as_str() {
        "product" => Some(selected_json(&item["product"], &selection.selection)),
        "variants" => Some(delivery_profile_variants_connection_json(
            item.get("variants")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default(),
            &selection.arguments,
            &selection.selection,
        )),
        _ => None,
    })
}

fn delivery_profile_variants_connection_json(
    variants: Vec<Value>,
    arguments: &BTreeMap<String, ResolvedValue>,
    selections: &[SelectedField],
) -> Value {
    let nodes = limited_nodes(variants, arguments);
    selected_json(
        &connection_json_with_cursor(
            nodes,
            |_, node| value_id_cursor(node),
            connection_page_info(false, false, None, None),
        ),
        selections,
    )
}

fn delivery_profile_locations_connection_json(
    locations: Vec<Value>,
    arguments: &BTreeMap<String, ResolvedValue>,
    selections: &[SelectedField],
) -> Value {
    let nodes = limited_nodes(locations, arguments);
    selected_json(
        &connection_json_with_cursor(
            nodes,
            |_, node| value_id_cursor(node),
            connection_page_info(false, false, None, None),
        ),
        selections,
    )
}

fn limited_nodes(mut nodes: Vec<Value>, arguments: &BTreeMap<String, ResolvedValue>) -> Vec<Value> {
    if let Some(limit) = arguments.get("first").and_then(resolved_as_usize) {
        nodes.truncate(limit);
    }
    nodes
}

pub(in crate::proxy) fn refresh_delivery_profile_counts(profile: &mut Value) {
    let groups = profile
        .get("profileLocationGroups")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let origin_count = groups
        .iter()
        .map(|group| {
            group["locationGroup"]["locations"]
                .as_array()
                .map(Vec::len)
                .unwrap_or(0)
        })
        .sum::<usize>();
    let mut country_count = 0usize;
    let mut active_methods = 0usize;
    for group in &groups {
        for zone in group["locationGroupZones"].as_array().into_iter().flatten() {
            country_count += zone["zone"]["countries"]
                .as_array()
                .map(Vec::len)
                .unwrap_or(0);
            active_methods += zone["methodDefinitions"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|method| method["active"].as_bool().unwrap_or(false))
                .count();
        }
    }
    let variant_count = profile
        .get("profileItems")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|item| item["variants"].as_array().map(Vec::len).unwrap_or(0))
        .sum::<usize>();
    profile["originLocationCount"] = json!(origin_count);
    profile["zoneCountryCount"] = json!(country_count);
    profile["activeMethodDefinitionsCount"] = json!(active_methods);
    profile["productVariantsCount"] = count_object(variant_count);
}

pub(in crate::proxy) fn delivery_profile_item_for_variant(
    variant_id: &str,
    observed_variant: Option<&Value>,
) -> Value {
    let product = observed_variant.and_then(|variant| variant.get("product"));
    let product_id = product
        .and_then(|product| product.get("id"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| delivery_profile_fallback_product_id(variant_id));
    let product_title = product
        .and_then(|product| product.get("title"))
        .and_then(Value::as_str)
        .unwrap_or("Delivery profile product");
    let variant_title = observed_variant
        .and_then(|variant| variant.get("title"))
        .and_then(Value::as_str)
        .unwrap_or("Default Title");
    json!({
        "product": {
            "id": product_id,
            "title": product_title
        },
        "variants": [{
            "id": variant_id,
            "title": variant_title
        }]
    })
}

fn delivery_profile_fallback_product_id(variant_id: &str) -> String {
    let tail = Some(resource_id_path_tail(variant_id))
        .filter(|tail| !tail.is_empty())
        .unwrap_or("local");
    shopify_gid("Product", format_args!("delivery-profile-{tail}"))
}

pub(in crate::proxy) fn delivery_profile_countries_from_input(
    zone_input: &BTreeMap<String, ResolvedValue>,
) -> Vec<Value> {
    let mut countries = delivery_profile_zone_countries_from_input(zone_input);
    countries.sort();
    countries.dedup();
    countries
        .into_iter()
        .map(|country| delivery_profile_country_record(&country))
        .collect()
}

fn delivery_profile_zone_countries_from_input(
    zone_input: &BTreeMap<String, ResolvedValue>,
) -> Vec<String> {
    match zone_input.get("countries") {
        Some(ResolvedValue::List(values)) => values
            .iter()
            .filter_map(|value| match value {
                ResolvedValue::Object(country) => resolved_string_field(country, "code")
                    .or_else(|| resolved_string_field(country, "countryCode")),
                _ => None,
            })
            .collect(),
        Some(ResolvedValue::Object(countries)) => {
            let rest_of_world = resolved_bool_field(countries, "restOfWorld").unwrap_or(false);
            let mut codes = list_string_field(countries, "countryCodes");
            if rest_of_world {
                codes.push("REST_OF_WORLD".to_string());
            }
            codes
        }
        _ => Vec::new(),
    }
}

fn delivery_profile_country_record(code: &str) -> Value {
    let rest_of_world = code == "REST_OF_WORLD";
    let country_name = delivery_profile_country_name(code);
    json!({
        "id": shopify_gid("DeliveryCountry", code),
        "name": if rest_of_world { "Rest of World".to_string() } else { country_name.clone() },
        "translatedName": if rest_of_world { "Rest of World".to_string() } else { country_name },
        "code": {
            "countryCode": if rest_of_world { Value::Null } else { json!(code) },
            "restOfWorld": rest_of_world
        },
        "provinces": []
    })
}

fn delivery_profile_country_name(code: &str) -> String {
    country_name_for_code(code).unwrap_or(code).to_string()
}

pub(in crate::proxy) fn delivery_price_from_method_input(
    input: &BTreeMap<String, ResolvedValue>,
) -> Value {
    let rate_definition = resolved_object_field(input, "rateDefinition").unwrap_or_default();
    let price = resolved_object_field(&rate_definition, "price").unwrap_or_default();
    json!({
        "amount": money_amount_string_from_resolved(price.get("amount")),
        "currencyCode": resolved_string_field(&price, "currencyCode").unwrap_or_else(|| "USD".to_string())
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
            "userErrors" => selected_user_errors_field(user_errors.as_slice(), selection),
            _ => None,
        }
    })
}

pub(in crate::proxy) fn fulfillment_order_hold_payload_json(
    fulfillment_hold: Value,
    fulfillment_order: Value,
    remaining: Value,
    payload_selection: &[SelectedField],
    user_errors: Vec<Value>,
) -> Value {
    selected_payload_json(payload_selection, |selection| {
        match selection.name.as_str() {
            "fulfillmentHold" => Some(nullable_selected_json(
                &fulfillment_hold,
                &selection.selection,
            )),
            "fulfillmentOrder" => Some(nullable_selected_json(
                &fulfillment_order,
                &selection.selection,
            )),
            "remainingFulfillmentOrder" => {
                Some(nullable_selected_json(&remaining, &selection.selection))
            }
            "userErrors" => selected_user_errors_field(user_errors.as_slice(), selection),
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
            "userErrors" => selected_user_errors_field(user_errors.as_slice(), selection),
            _ => None,
        }
    })
}

pub(in crate::proxy) fn fulfillment_order_cancel_payload_json(
    fulfillment_order: Value,
    replacement: Value,
    payload_selection: &[SelectedField],
    user_errors: Vec<Value>,
) -> Value {
    selected_payload_json(payload_selection, |selection| {
        match selection.name.as_str() {
            "fulfillmentOrder" => Some(nullable_selected_json(
                &fulfillment_order,
                &selection.selection,
            )),
            "replacementFulfillmentOrder" => {
                Some(nullable_selected_json(&replacement, &selection.selection))
            }
            "userErrors" => selected_user_errors_field(user_errors.as_slice(), selection),
            _ => None,
        }
    })
}

pub(in crate::proxy) fn fulfillment_orders_reroute_payload_json(
    moved_orders: Vec<Value>,
    payload_selection: &[SelectedField],
    user_errors: Vec<Value>,
) -> Value {
    selected_payload_json(payload_selection, |selection| {
        match selection.name.as_str() {
            "movedFulfillmentOrders" => Some(Value::Array(
                moved_orders
                    .iter()
                    .map(|order| selected_json(order, &selection.selection))
                    .collect(),
            )),
            "userErrors" => selected_user_errors_field(user_errors.as_slice(), selection),
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
            "userErrors" => selected_user_errors_field(user_errors.as_slice(), selection),
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
            "userErrors" => selected_user_errors_field(user_errors.as_slice(), selection),
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
        vec![user_error_omit_code(
            ["id"],
            "Fulfillment service could not be found.",
            None,
        )],
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
            "userErrors" => selected_user_errors_field(user_errors.as_slice(), selection),
            _ => None,
        }
    })
}

pub(in crate::proxy) fn destination_location_not_found_or_inactive_error() -> Value {
    user_error(
        ["destinationLocationId"],
        "Location could not be deactivated because the destination location could be not found or is inactive.",
        Some("DESTINATION_LOCATION_NOT_FOUND_OR_INACTIVE"),
    )
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
    selected_payload_json(payload_selection, |selection| {
        match selection.name.as_str() {
            "carrierService" => Some(if carrier.is_null() {
                Value::Null
            } else {
                selected_json(&carrier, carrier_selection)
            }),
            "userErrors" => selected_user_errors_field(user_errors.as_slice(), selection),
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
        vec![user_error(
            Value::Null,
            "The carrier or app could not be found.",
            Some(code),
        )],
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
            "userErrors" => selected_user_errors_field(user_errors.as_slice(), selection),
            _ => None,
        }
    })
}

pub(in crate::proxy) fn carrier_service_callback_url_error(
    callback_url: &str,
    code: &str,
) -> Option<Value> {
    let trimmed = callback_url.trim();
    if trimmed.starts_with("http://") {
        return Some(user_error(
            Value::Null,
            "Shipping rate provider callback url must use HTTPS",
            Some(code),
        ));
    }
    let Some(host) = carrier_service_https_callback_host(trimmed) else {
        return Some(user_error(
            Value::Null,
            "Shipping rate provider callback url invalid host",
            Some(code),
        ));
    };
    if carrier_service_callback_host_is_disallowed(&host) {
        return Some(user_error(
            Value::Null,
            "Shipping rate provider callback url invalid host",
            Some(code),
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

    // Shopify coerces `DeliveryCarrierServiceCreateInput!` as a single variable, so a
    // create that omits more than one required field surfaces one INVALID_VARIABLE error
    // whose message and `problems` list every offending field in input-field order
    // (callbackUrl, supportsServiceDiscovery, active).
    let mut message_parts: Vec<String> = Vec::new();
    let mut problems: Vec<Value> = Vec::new();

    match input.get("callbackUrl") {
        None | Some(ResolvedValue::Null) => {
            let explanation = "Expected value to not be null";
            message_parts.push(format!("callbackUrl ({explanation})"));
            problems.push(json!({ "path": ["callbackUrl"], "explanation": explanation }));
        }
        Some(ResolvedValue::String(value)) if value.is_empty() || !value.contains("://") => {
            let message = format!("Invalid url '{value}', missing scheme");
            message_parts.push(format!("callbackUrl ({message})"));
            problems.push(json!({
                "path": ["callbackUrl"],
                "explanation": message,
                "message": message
            }));
        }
        _ => {}
    }

    for required in ["supportsServiceDiscovery", "active"] {
        if matches!(input.get(required), None | Some(ResolvedValue::Null)) {
            let explanation = "Expected value to not be null";
            message_parts.push(format!("{required} ({explanation})"));
            problems.push(json!({ "path": [required], "explanation": explanation }));
        }
    }

    if problems.is_empty() {
        return None;
    }

    let definition = variable_definition_info(query, variable_name);
    let type_display = definition
        .as_ref()
        .map(|definition| definition.type_display.clone())
        .unwrap_or_else(|| "DeliveryCarrierServiceCreateInput!".to_string());
    let location = definition
        .map(|definition| json!({ "line": definition.location.line, "column": definition.location.column }))
        .unwrap_or_else(|| json!({ "line": 1, "column": 1 }));
    let value = resolved_value_json(&ResolvedValue::Object(input.clone()));
    Some(json!({
        "message": format!(
            "Variable ${variable_name} of type {type_display} was provided invalid value for {}",
            message_parts.join(", ")
        ),
        "locations": [location],
        "extensions": {
            "code": "INVALID_VARIABLE",
            "value": value,
            "problems": problems
        }
    }))
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

impl DraftProxy {
pub(in crate::proxy) fn request_api_client_id(request: &Request) -> String {
    request_header(request, API_CLIENT_ID_HEADER)
        .unwrap_or_else(|| "gid://shopify/App/local".to_string())
}

pub(in crate::proxy) fn set_log_status(entry: &mut Value, status: &str) {
    if let Value::Object(fields) = entry {
        fields.insert("status".to_string(), json!(status));
    }
}

impl DraftProxy {
    pub(in crate::proxy) fn record_failed_mutation(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        root_field: &str,
    ) {
        self.record_mutation_log_with_status(
            request,
            query,
            variables,
            root_field,
            Vec::new(),
            "failed",
        );
    }

    pub(in crate::proxy) fn record_mutation_log_with_status(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        root_field: &str,
        staged_ids: Vec<String>,
        status: &str,
    ) {
        self.record_mutation_log_entry(request, query, variables, root_field, staged_ids);
        if status != "staged" {
            if let Some(entry) = self.log_entries.last_mut() {
                set_log_status(entry, status);
            }
        }
    }
}

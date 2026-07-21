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

pub(in crate::proxy) fn delegate_access_token_destroy_user_error(
    message: &str,
    code: &str,
) -> Value {
    user_error(Value::Null, message, Some(code))
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

pub(in crate::proxy) fn request_app_context_key(request: &Request) -> String {
    request_header(request, API_CLIENT_ID_HEADER)
        .map(|value| normalize_app_gid(&value))
        .unwrap_or_else(|| "__default".to_string())
}

pub(in crate::proxy) fn request_has_explicit_app_context(request: &Request) -> bool {
    [
        API_CLIENT_ID_HEADER,
        "x-shopify-draft-proxy-app-installation-id",
        "x-shopify-draft-proxy-app-handle",
        "x-shopify-draft-proxy-app-title",
        "x-shopify-draft-proxy-app-api-key",
        ACCESS_SCOPES_HEADER,
        "x-shopify-draft-proxy-required-access-scopes",
    ]
    .iter()
    .any(|header| request_header(request, header).is_some())
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
    let api_key = request_header(request, "x-shopify-draft-proxy-app-api-key");
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
    let mut installation = json!({
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
    });
    if let Some(api_key) = api_key {
        installation["app"]["apiKey"] = json!(api_key);
    }
    installation
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
    let captured_description = match handle {
        "read_markets" => Some("Read access for Shopify Markets API"),
        "write_markets" => Some("Write access for Shopify Markets API"),
        "read_orders" => Some("Read orders, transactions, and fulfillments"),
        "read_products" => Some("Read products, variants, and collections"),
        "write_products" => Some("Modify products, variants, and collections"),
        _ => None,
    };
    json!({
        "handle": handle,
        "description": description
            .or(captured_description)
            .map(Value::from)
            .unwrap_or(Value::Null)
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

pub(in crate::proxy) fn failed_payload_outcome(
    payload: Value,
) -> (Value, &'static str, Vec<String>) {
    (payload, "failed", Vec::new())
}

pub(in crate::proxy) fn response_is_success(response: &Response) -> bool {
    (200..300).contains(&response.status)
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

pub(in crate::proxy) fn current_app_installation_value(
    installation: &Value,
    subscriptions: &BTreeMap<String, Value>,
    one_time_purchases: &BTreeMap<String, Value>,
    revoked_access_scopes: &BTreeSet<String>,
) -> Value {
    let mut value = installation.clone();
    let Some(fields) = value.as_object_mut() else {
        return value;
    };
    fields.insert("__typename".to_string(), json!("AppInstallation"));
    if let Some(id) = app_installation_id(installation) {
        fields.insert("id".to_string(), json!(id));
    }
    if subscriptions.is_empty() {
        fields
            .entry("activeSubscriptions".to_string())
            .or_insert_with(|| Value::Array(Vec::new()));
    } else {
        fields.insert(
            "activeSubscriptions".to_string(),
            Value::Array(
                subscriptions
                    .values()
                    .filter(|subscription| subscription["status"] == "ACTIVE")
                    .cloned()
                    .collect(),
            ),
        );
    }
    fields.insert(
        "accessScopes".to_string(),
        Value::Array(
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
                .cloned()
                .collect(),
        ),
    );
    if !one_time_purchases.is_empty() {
        fields.insert(
            "oneTimePurchases".to_string(),
            connection_json(one_time_purchases.values().cloned().collect()),
        );
    }
    value
}

pub(in crate::proxy) fn location_deactivate_payload_json(
    location: Value,
    user_errors: Vec<Value>,
) -> Value {
    json!({
        "location": location,
        "locationDeactivateUserErrors": user_errors,
    })
}

pub(in crate::proxy) fn delivery_profile_payload_json(
    profile: Value,
    user_errors: Vec<Value>,
) -> Value {
    json!({
        "profile": if profile.is_null() {
            Value::Null
        } else {
            canonical_delivery_profile_value(&profile)
        },
        "userErrors": user_errors,
    })
}

pub(in crate::proxy) fn delivery_profile_remove_payload_json(
    job: Value,
    user_errors: Vec<Value>,
) -> Value {
    json!({
        "job": job,
        "userErrors": user_errors,
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

pub(in crate::proxy) fn delivery_profile_field_resolver_registrations(
) -> Vec<FieldResolverRegistration> {
    [
        (
            "DeliveryProfile",
            "profileItems",
            delivery_profile_items_field as crate::resolver_registry::FieldResolverHandler,
        ),
        (
            "DeliveryProfile",
            "profileLocationGroups",
            delivery_profile_location_groups_field,
        ),
        (
            "DeliveryProfile",
            "sellingPlanGroups",
            delivery_profile_selling_plan_groups_field,
        ),
        (
            "DeliveryProfile",
            "unassignedLocationsPaginated",
            delivery_profile_unassigned_locations_field,
        ),
        (
            "DeliveryLocationGroup",
            "locations",
            delivery_location_group_locations_field,
        ),
        (
            "DeliveryProfileItem",
            "variants",
            delivery_profile_item_variants_field,
        ),
        (
            "DeliveryProfileLocationGroup",
            "locationGroupZones",
            delivery_profile_location_group_zones_field,
        ),
        (
            "DeliveryLocationGroupZone",
            "methodDefinitions",
            delivery_location_group_zone_method_definitions_field,
        ),
    ]
    .into_iter()
    .map(|(parent_type, field_name, handler)| {
        FieldResolverRegistration::explicit(ApiSurface::Admin, parent_type, field_name, handler)
    })
    .collect()
}

pub(in crate::proxy) fn canonical_delivery_profile_value(profile: &Value) -> Value {
    if profile.is_null() {
        return Value::Null;
    }
    let mut profile = profile.clone();
    let items = stored_delivery_profile_nodes(&profile["profileItems"])
        .iter()
        .map(canonical_delivery_profile_item_value)
        .collect::<Vec<_>>();
    let groups = profile
        .get("profileLocationGroups")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(canonical_delivery_profile_location_group_value)
        .collect::<Vec<_>>();
    if let Some(fields) = profile.as_object_mut() {
        fields.insert("__typename".to_string(), json!("DeliveryProfile"));
        fields.insert("profileItems".to_string(), Value::Array(items));
        fields.insert("profileLocationGroups".to_string(), Value::Array(groups));
        fields
            .entry("productVariantsCount".to_string())
            .or_insert_with(|| count_object(0));
        fields
            .entry("sellingPlanGroups".to_string())
            .or_insert_with(|| Value::Array(Vec::new()));
        fields
            .entry("unassignedLocations".to_string())
            .or_insert_with(|| Value::Array(Vec::new()));
    }
    profile
}

fn canonical_delivery_profile_item_value(item: &Value) -> Value {
    let mut item = item.clone();
    if let Some(fields) = item.as_object_mut() {
        fields.insert("__typename".to_string(), json!("DeliveryProfileItem"));
        fields
            .entry("variants".to_string())
            .or_insert_with(|| Value::Array(Vec::new()));
    }
    item
}

fn canonical_delivery_profile_location_group_value(group: &Value) -> Value {
    let mut group = group.clone();
    let zones = group
        .get("locationGroupZones")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(canonical_delivery_location_group_zone_value)
        .collect::<Vec<_>>();
    let stored_countries = group
        .get("countriesInAnyZone")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let countries = if stored_countries.is_empty() {
        delivery_profile_countries_in_any_zone(&group)
    } else {
        stored_countries
    };
    let mut location_group = group["locationGroup"].clone();
    if let Some(fields) = location_group.as_object_mut() {
        fields.insert("__typename".to_string(), json!("DeliveryLocationGroup"));
        let location_count = fields
            .get("locations")
            .and_then(Value::as_array)
            .map(Vec::len)
            .unwrap_or_default();
        fields
            .entry("locationsCount".to_string())
            .or_insert_with(|| count_object(location_count));
    }
    if let Some(fields) = group.as_object_mut() {
        fields.insert(
            "__typename".to_string(),
            json!("DeliveryProfileLocationGroup"),
        );
        fields.insert("locationGroup".to_string(), location_group);
        fields.insert("locationGroupZones".to_string(), Value::Array(zones));
        fields.insert("countriesInAnyZone".to_string(), Value::Array(countries));
    }
    group
}

fn canonical_delivery_location_group_zone_value(zone: &Value) -> Value {
    let mut zone = zone.clone();
    if let Some(fields) = zone.as_object_mut() {
        fields.insert("__typename".to_string(), json!("DeliveryLocationGroupZone"));
        fields
            .entry("methodDefinitions".to_string())
            .or_insert_with(|| Value::Array(Vec::new()));
    }
    zone
}

fn stored_delivery_profile_nodes(value: &Value) -> Vec<Value> {
    value
        .as_array()
        .cloned()
        .unwrap_or_else(|| connection_nodes(value))
}

fn delivery_profile_items_field(
    _proxy: &mut DraftProxy,
    _request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> Result<Value, String> {
    let items = stored_delivery_profile_nodes(&invocation.parent["profileItems"])
        .iter()
        .map(canonical_delivery_profile_item_value)
        .collect::<Vec<_>>();
    Ok(connection_value_with_args(
        items,
        &resolved_arguments_from_json(&invocation.arguments),
        delivery_profile_item_cursor,
    ))
}

fn delivery_profile_location_groups_field(
    _proxy: &mut DraftProxy,
    _request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> Result<Value, String> {
    let arguments = resolved_arguments_from_json(&invocation.arguments);
    let location_group_id = resolved_string_field(&arguments, "locationGroupId");
    Ok(Value::Array(
        invocation.parent["profileLocationGroups"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|group| {
                location_group_id
                    .as_deref()
                    .is_none_or(|id| group["locationGroup"]["id"].as_str() == Some(id))
            })
            .map(canonical_delivery_profile_location_group_value)
            .collect(),
    ))
}

fn delivery_profile_selling_plan_groups_field(
    _proxy: &mut DraftProxy,
    _request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> Result<Value, String> {
    Ok(connection_value_with_args(
        stored_delivery_profile_nodes(&invocation.parent["sellingPlanGroups"]),
        &resolved_arguments_from_json(&invocation.arguments),
        value_id_cursor,
    ))
}

fn delivery_profile_unassigned_locations_field(
    _proxy: &mut DraftProxy,
    _request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> Result<Value, String> {
    Ok(connection_value_with_args(
        stored_delivery_profile_nodes(&invocation.parent["unassignedLocations"]),
        &resolved_arguments_from_json(&invocation.arguments),
        value_id_cursor,
    ))
}

fn delivery_location_group_locations_field(
    _proxy: &mut DraftProxy,
    _request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> Result<Value, String> {
    Ok(connection_value_with_args(
        stored_delivery_profile_nodes(&invocation.parent["locations"]),
        &resolved_arguments_from_json(&invocation.arguments),
        value_id_cursor,
    ))
}

fn delivery_profile_item_variants_field(
    _proxy: &mut DraftProxy,
    _request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> Result<Value, String> {
    Ok(connection_value_with_args(
        stored_delivery_profile_nodes(&invocation.parent["variants"]),
        &resolved_arguments_from_json(&invocation.arguments),
        value_id_cursor,
    ))
}

fn delivery_profile_location_group_zones_field(
    _proxy: &mut DraftProxy,
    _request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> Result<Value, String> {
    let zones = stored_delivery_profile_nodes(&invocation.parent["locationGroupZones"])
        .iter()
        .map(canonical_delivery_location_group_zone_value)
        .collect::<Vec<_>>();
    Ok(connection_value_with_args(
        zones,
        &resolved_arguments_from_json(&invocation.arguments),
        delivery_location_group_zone_cursor,
    ))
}

fn delivery_location_group_zone_method_definitions_field(
    _proxy: &mut DraftProxy,
    _request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> Result<Value, String> {
    Ok(connection_value_with_args(
        stored_delivery_profile_nodes(&invocation.parent["methodDefinitions"]),
        &resolved_arguments_from_json(&invocation.arguments),
        value_id_cursor,
    ))
}

fn delivery_profile_item_cursor(item: &Value) -> String {
    item["product"]["id"]
        .as_str()
        .map(str::to_string)
        .unwrap_or_else(|| value_id_cursor(item))
}

fn delivery_location_group_zone_cursor(zone: &Value) -> String {
    zone["zone"]["id"]
        .as_str()
        .map(str::to_string)
        .unwrap_or_else(|| value_id_cursor(zone))
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

pub(in crate::proxy) fn delivery_profile_item_for_observed_variant(
    observed_variant: &Value,
) -> Option<Value> {
    let variant_id = observed_variant.get("id").and_then(Value::as_str)?;
    let variant_title = observed_variant.get("title").and_then(Value::as_str)?;
    let product = observed_variant.get("product")?;
    let product_id = product.get("id").and_then(Value::as_str)?;
    let product_title = product.get("title").and_then(Value::as_str)?;
    Some(delivery_profile_item_for_resolved_variant(
        variant_id,
        variant_title,
        product_id,
        product_title,
    ))
}

pub(in crate::proxy) fn delivery_profile_item_for_resolved_variant(
    variant_id: &str,
    variant_title: &str,
    product_id: &str,
    product_title: &str,
) -> Value {
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
    user_errors: Vec<Value>,
) -> Value {
    json!({
        "movedFulfillmentOrder": moved,
        "originalFulfillmentOrder": original,
        "remainingFulfillmentOrder": remaining,
        "userErrors": user_errors,
    })
}

pub(in crate::proxy) fn fulfillment_order_hold_payload_json(
    fulfillment_hold: Value,
    fulfillment_order: Value,
    remaining: Value,
    user_errors: Vec<Value>,
) -> Value {
    json!({
        "fulfillmentHold": fulfillment_hold,
        "fulfillmentOrder": fulfillment_order,
        "remainingFulfillmentOrder": remaining,
        "userErrors": user_errors,
    })
}

pub(in crate::proxy) fn fulfillment_order_simple_payload_json(
    fulfillment_order: Value,
    user_errors: Vec<Value>,
) -> Value {
    json!({
        "fulfillmentOrder": fulfillment_order,
        "userErrors": user_errors,
    })
}

pub(in crate::proxy) fn fulfillment_order_cancel_payload_json(
    fulfillment_order: Value,
    replacement: Value,
    user_errors: Vec<Value>,
) -> Value {
    json!({
        "fulfillmentOrder": fulfillment_order,
        "replacementFulfillmentOrder": replacement,
        "userErrors": user_errors,
    })
}

pub(in crate::proxy) fn fulfillment_orders_reroute_payload_json(
    moved_orders: Vec<Value>,
    user_errors: Vec<Value>,
) -> Value {
    json!({
        "movedFulfillmentOrders": moved_orders,
        "userErrors": user_errors,
    })
}

pub(in crate::proxy) fn fulfillment_order_deadline_payload_json(
    success: bool,
    user_errors: Vec<Value>,
) -> Value {
    json!({ "success": success, "userErrors": user_errors })
}

pub(in crate::proxy) fn fulfillment_service_payload_json(
    service: Value,
    user_errors: Vec<Value>,
) -> Value {
    json!({
        "fulfillmentService": service,
        "userErrors": user_errors,
    })
}

pub(in crate::proxy) fn fulfillment_service_not_found_payload() -> Value {
    fulfillment_service_payload_json(
        Value::Null,
        vec![user_error_omit_code(
            ["id"],
            "Fulfillment service could not be found.",
            None,
        )],
    )
}

pub(in crate::proxy) fn fulfillment_service_delete_payload(
    deleted_id: Value,
    user_errors: Vec<Value>,
) -> Value {
    json!({
        "deletedId": deleted_id,
        "userErrors": user_errors,
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
    user_errors: Vec<Value>,
) -> Value {
    json!({ "carrierService": carrier, "userErrors": user_errors })
}

pub(in crate::proxy) fn carrier_service_not_found_payload(code: &str) -> Value {
    carrier_service_payload_json(
        Value::Null,
        vec![user_error(
            Value::Null,
            "The carrier or app could not be found.",
            Some(code),
        )],
    )
}

pub(in crate::proxy) fn carrier_service_delete_payload(
    deleted_id: Value,
    user_errors: Vec<Value>,
) -> Value {
    json!({ "deletedId": deleted_id, "userErrors": user_errors })
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

pub(in crate::proxy) fn request_api_client_id(request: &Request) -> String {
    request_header(request, API_CLIENT_ID_HEADER)
        .unwrap_or_else(|| "gid://shopify/App/local".to_string())
}

pub(in crate::proxy) fn set_log_status(entry: &mut Value, status: &str) {
    if let Value::Object(fields) = entry {
        fields.insert("status".to_string(), json!(status));
    }
}

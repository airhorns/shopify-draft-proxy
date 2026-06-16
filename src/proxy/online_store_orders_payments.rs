use super::*;

struct OrdersLocalLogEntry<'a> {
    request: &'a Request,
    query: &'a str,
    variables: &'a BTreeMap<String, ResolvedValue>,
    root_field: &'a str,
    staged_resource_ids: Vec<String>,
    outcome: OrdersLocalLogOutcome<'a>,
}

#[derive(Clone, Copy)]
struct OnlineStoreIntegrationDeleteSpec {
    expected_type: &'static str,
    deleted_id_field: &'static str,
    input_object_field: &'static str,
    user_error_typename: Option<&'static str>,
    resource_label: &'static str,
}

const SCRIPT_TAG_DELETE_SPEC: OnlineStoreIntegrationDeleteSpec = OnlineStoreIntegrationDeleteSpec {
    expected_type: "ScriptTag",
    deleted_id_field: "deletedScriptTagId",
    input_object_field: "scriptTag",
    user_error_typename: Some("ScriptTagUserError"),
    resource_label: "Script tag",
};

const WEB_PIXEL_DELETE_SPEC: OnlineStoreIntegrationDeleteSpec = OnlineStoreIntegrationDeleteSpec {
    expected_type: "WebPixel",
    deleted_id_field: "deletedWebPixelId",
    input_object_field: "webPixel",
    user_error_typename: Some("WebPixelUserError"),
    resource_label: "Pixel",
};

const STOREFRONT_ACCESS_TOKEN_DELETE_SPEC: OnlineStoreIntegrationDeleteSpec =
    OnlineStoreIntegrationDeleteSpec {
        expected_type: "StorefrontAccessToken",
        deleted_id_field: "deletedStorefrontAccessTokenId",
        input_object_field: "storefrontAccessToken",
        user_error_typename: None,
        resource_label: "Storefront access token",
    };

const MOBILE_PLATFORM_APPLICATION_DELETE_SPEC: OnlineStoreIntegrationDeleteSpec =
    OnlineStoreIntegrationDeleteSpec {
        expected_type: "MobilePlatformApplication",
        deleted_id_field: "deletedMobilePlatformApplicationId",
        input_object_field: "mobilePlatformApplication",
        user_error_typename: None,
        resource_label: "Mobile platform application",
    };

const MOBILE_PLATFORM_APPLICATION_ID_MAX_LENGTH: usize = 100;
const MOBILE_PLATFORM_APP_CLIP_APPLICATION_ID_MAX_LENGTH: usize = 255;
const ORDERS_FULFILLMENT_ORDER_HYDRATE_QUERY: &str = "query ShippingFulfillmentOrderHydrate($id: ID!) {\n    fulfillmentOrder(id: $id) {\n      id\n      status\n      requestStatus\n      fulfillAt\n      fulfillBy\n      updatedAt\n      supportedActions {\n        action\n      }\n      assignedLocation {\n        name\n        location {\n          id\n          name\n        }\n      }\n      fulfillmentHolds {\n        id\n        handle\n        reason\n        reasonNotes\n        displayReason\n        heldByApp {\n          id\n          title\n        }\n        heldByRequestingApp\n      }\n      merchantRequests(first: 10) {\n        nodes {\n          kind\n          message\n          requestOptions\n        }\n      }\n      lineItems(first: 20) {\n        nodes {\n          id\n          totalQuantity\n          remainingQuantity\n          lineItem {\n            id\n            title\n            quantity\n            fulfillableQuantity\n          }\n        }\n      }\n      order {\n        id\n        name\n        displayFulfillmentStatus\n      }\n    }\n  }";
const ORDERS_FULFILLMENT_ORDER_COMPACT_HYDRATE_QUERY: &str = "query ShippingFulfillmentOrderHydrate($id: ID!) {\n    fulfillmentOrder(id: $id) {\n      id status requestStatus fulfillAt fulfillBy updatedAt\n      supportedActions { action }\n      assignedLocation { name location { id name } }\n      fulfillmentHolds { id handle reason reasonNotes displayReason heldByApp { id title } heldByRequestingApp }\n      merchantRequests(first: 10) { nodes { kind message requestOptions } }\n      lineItems(first: 20) { nodes { id totalQuantity remainingQuantity lineItem { id title quantity fulfillableQuantity } } }\n      order { id name displayFulfillmentStatus }\n    }\n  }";
const ORDERS_FULFILLMENT_HYDRATE_QUERY: &str = "query OrdersFulfillmentHydrate($id: ID!) { fulfillment(id: $id) { id order { id name email phone createdAt updatedAt closed closedAt cancelledAt cancelReason displayFinancialStatus displayFulfillmentStatus note tags fulfillments { id status displayStatus createdAt updatedAt trackingInfo { number url company } } } } }";

fn mobile_application_id_too_long_error<const N: usize>(field: [&str; N]) -> Value {
    mobile_app_error(
        "TOO_LONG",
        field,
        "Application ID is too long (maximum is 100 characters)",
    )
}

fn validate_mobile_app_clip_application_id(
    apple: &BTreeMap<String, ResolvedValue>,
    update_input: bool,
) -> Option<Value> {
    let app_clips_enabled = resolved_bool_field(apple, "appClipsEnabled").unwrap_or(false);
    let app_clip_application_id = resolved_string_field(apple, "appClipApplicationId");
    if app_clips_enabled
        && app_clip_application_id
            .as_deref()
            .is_none_or(|value| value.trim().is_empty())
    {
        return Some(mobile_app_error(
            "BLANK",
            ["input", "apple", "appClipApplicationId"],
            "App clip application can't be blank",
        ));
    }
    if app_clips_enabled
        && app_clip_application_id
            .as_deref()
            .is_some_and(|value| value.len() > MOBILE_PLATFORM_APP_CLIP_APPLICATION_ID_MAX_LENGTH)
    {
        return Some(mobile_app_error(
            "TOO_LONG",
            ["input", "apple", "appClipApplicationId"],
            "App clip application is too long (maximum is 255 characters)",
        ));
    }
    if update_input
        && app_clip_application_id
            .as_deref()
            .is_some_and(|value| value.trim().is_empty())
    {
        return Some(mobile_app_error(
            "BLANK",
            ["input", "apple", "appClipApplicationId"],
            "App clip application can't be blank",
        ));
    }
    None
}

fn draft_order_create_input_email(field: &RootFieldSelection) -> Option<String> {
    let input = resolved_object_field(&field.arguments, "input")?;
    resolved_string_field(&input, "email")
}

fn draft_order_create_first_line_title(field: &RootFieldSelection) -> Option<String> {
    let input = resolved_object_field(&field.arguments, "input")?;
    let line_items = resolved_object_list_field(&input, "lineItems");
    let first_line = line_items.first()?;
    resolved_string_field(first_line, "title")
}

fn draft_order_create_selects_tags(field: &RootFieldSelection) -> bool {
    resolved_object_field(&field.arguments, "input").is_some_and(|input| input.contains_key("tags"))
        || selected_child_selection(&field.selection, "draftOrder")
            .is_some_and(|selection| selection.iter().any(|field| field.name == "tags"))
}

fn order_create_selects_payment_transaction_fields(field: &RootFieldSelection) -> bool {
    selected_child_selection(&field.selection, "order").is_some_and(|selection| {
        selection.iter().any(|field| {
            matches!(
                field.name.as_str(),
                "capturable"
                    | "totalCapturable"
                    | "totalCapturableSet"
                    | "totalOutstandingSet"
                    | "totalReceivedSet"
                    | "netPaymentSet"
                    | "paymentGatewayNames"
                    | "transactions"
            )
        })
    })
}

fn order_create_inventory_behaviour(field: &RootFieldSelection) -> String {
    resolved_object_field(&field.arguments, "options")
        .and_then(|options| resolved_string_field(&options, "inventoryBehaviour"))
        .unwrap_or_else(|| "DECREMENT_IGNORING_POLICY".to_string())
}

fn order_line_inventory_item_id(line_item: &BTreeMap<String, ResolvedValue>) -> Option<String> {
    resolved_string_field(line_item, "inventoryItemId").or_else(|| {
        resolved_string_field(line_item, "variantId").map(|variant_id| {
            format!(
                "gid://shopify/InventoryItem/{}",
                resource_id_tail(&variant_id)
            )
        })
    })
}

fn order_read_selects_payment_transaction_fields(field: &RootFieldSelection) -> bool {
    field.selection.iter().any(|field| {
        matches!(
            field.name.as_str(),
            "displayFinancialStatus"
                | "totalCapturableSet"
                | "totalOutstandingSet"
                | "totalReceivedSet"
                | "transactions"
        )
    })
}

fn order_read_selects_order_edit_existing_fields(field: RootFieldSelection) -> bool {
    field.selection.iter().any(|field| {
        matches!(
            field.name.as_str(),
            "merchantEditable" | "merchantEditableErrors" | "currentSubtotalLineItemsQuantity"
        )
    })
}

fn orders_empty_count_payload() -> Value {
    json!({
        "data": {
            "ordersCount": {
                "count": 0,
                "precision": "EXACT"
            }
        }
    })
}

fn orders_error(field: &[&str], message: &str, code: &str) -> Value {
    json!({
        "field": field,
        "message": message,
        "code": code
    })
}

fn fulfillment_order_user_error(field: Value, message: &str, code: Option<&str>) -> Value {
    let mut error = serde_json::Map::new();
    error.insert("field".to_string(), field);
    error.insert("message".to_string(), json!(message));
    error.insert(
        "code".to_string(),
        code.map_or(Value::Null, |code| json!(code)),
    );
    Value::Object(error)
}

fn fulfillment_order_supported_actions(include_split: bool) -> Value {
    let mut actions = vec![
        json!({ "action": "CREATE_FULFILLMENT" }),
        json!({ "action": "REPORT_PROGRESS" }),
        json!({ "action": "MOVE" }),
        json!({ "action": "HOLD" }),
    ];
    if include_split {
        actions.push(json!({ "action": "SPLIT" }));
    }
    actions.push(json!({ "action": "MERGE" }));
    Value::Array(actions)
}

fn fulfillment_order_assigned_location() -> Value {
    json!({
        "name": "Shop location",
        "location": {
            "id": "gid://shopify/Location/1?shopify-draft-proxy=synthetic",
            "name": "Shop location"
        }
    })
}

fn normalize_fulfillment_order_record(order: &mut Value) {
    if order.get("updatedAt").is_none() {
        order["updatedAt"] = json!("2026-05-11T10:00:00Z");
    }
    if order.get("fulfillAt").is_none() {
        order["fulfillAt"] = Value::Null;
    }
    if order.get("fulfillBy").is_none() {
        order["fulfillBy"] = Value::Null;
    }
    if order.get("supportedActions").is_none() {
        order["supportedActions"] = fulfillment_order_supported_actions(true);
    }
    if order.get("assignedLocation").is_none() {
        order["assignedLocation"] = fulfillment_order_assigned_location();
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

fn normalize_order_fulfillment_orders(order: &mut Value) {
    if let Some(nodes) = fulfillment_order_nodes_mut(order) {
        for node in nodes {
            normalize_fulfillment_order_record(node);
        }
    }
}

fn line_item_remaining_quantity(line: &Value) -> i64 {
    line["remainingQuantity"]
        .as_i64()
        .or_else(|| line["totalQuantity"].as_i64())
        .unwrap_or(0)
        .max(0)
}

fn fulfillment_order_line_quantity_total(order: &Value) -> i64 {
    order["lineItems"]["nodes"]
        .as_array()
        .into_iter()
        .flatten()
        .map(line_item_remaining_quantity)
        .sum()
}

fn set_fulfillment_order_status_from_lines(order: &mut Value) {
    let remaining_total = fulfillment_order_line_quantity_total(order);
    order["status"] = json!(if remaining_total == 0 {
        "CLOSED"
    } else {
        "OPEN"
    });
    order["supportedActions"] = fulfillment_order_supported_actions(remaining_total > 1);
}

fn fulfillment_order_line_with_quantity(line: &Value, quantity: i64) -> Value {
    let mut updated = line.clone();
    updated["totalQuantity"] = json!(quantity.max(0));
    updated["remainingQuantity"] = json!(quantity.max(0));
    updated
}

fn strip_fulfillment_order_line_id(line: &Value) -> Value {
    let mut line = line.clone();
    if let Some(object) = line.as_object_mut() {
        object.remove("id");
    }
    line
}

fn fulfillment_order_payload_json(
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

fn fulfillment_order_request_payload_json(
    root_field: &str,
    fulfillment_order: Value,
    original: Value,
    submitted: Value,
    unsubmitted: Value,
    payload_selection: &[SelectedField],
    user_errors: Vec<Value>,
) -> Value {
    selected_payload_json(payload_selection, |selection| {
        match selection.name.as_str() {
            "fulfillmentOrder" => Some(nullable_selected_json(
                &fulfillment_order,
                &selection.selection,
            )),
            "originalFulfillmentOrder" => {
                Some(nullable_selected_json(&original, &selection.selection))
            }
            "submittedFulfillmentOrder" => {
                Some(nullable_selected_json(&submitted, &selection.selection))
            }
            "unsubmittedFulfillmentOrder" => {
                Some(nullable_selected_json(&unsubmitted, &selection.selection))
            }
            "userErrors" => Some(Value::Array(
                user_errors
                    .iter()
                    .map(|error| selected_json(error, &selection.selection))
                    .collect(),
            )),
            name if root_field == "fulfillmentOrderSubmitFulfillmentRequest"
                && name == "fulfillmentOrder" =>
            {
                Some(nullable_selected_json(&submitted, &selection.selection))
            }
            _ => None,
        }
    })
}

fn fulfillment_order_split_payload_json(
    splits: Value,
    payload_selection: &[SelectedField],
    user_errors: Vec<Value>,
) -> Value {
    selected_payload_json(payload_selection, |selection| {
        match selection.name.as_str() {
            "fulfillmentOrderSplits" => {
                if splits.is_null() {
                    Some(Value::Null)
                } else {
                    Some(Value::Array(
                        splits
                            .as_array()
                            .into_iter()
                            .flatten()
                            .map(|split| selected_json(split, &selection.selection))
                            .collect(),
                    ))
                }
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

fn fulfillment_order_merge_payload_json(
    merges: Value,
    payload_selection: &[SelectedField],
    user_errors: Vec<Value>,
) -> Value {
    selected_payload_json(payload_selection, |selection| {
        match selection.name.as_str() {
            "fulfillmentOrderMerges" => {
                if merges.is_null() {
                    Some(Value::Null)
                } else {
                    Some(Value::Array(
                        merges
                            .as_array()
                            .into_iter()
                            .flatten()
                            .map(|merge| selected_json(merge, &selection.selection))
                            .collect(),
                    ))
                }
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

fn order_create_error(field: Vec<Value>, message: &str, code: &str) -> Value {
    json!({
        "field": field,
        "message": message,
        "code": code
    })
}

fn order_create_money_set(amount: f64, currency_code: &str) -> Value {
    order_money_set(&format_order_amount(amount), currency_code)
}

fn order_create_money_bag(
    amount: f64,
    currency_code: &str,
    presentment_currency_code: &str,
) -> Value {
    let amount = format_order_amount(amount);
    json!({
        "shopMoney": {
            "amount": amount,
            "currencyCode": currency_code
        },
        "presentmentMoney": {
            "amount": amount,
            "currencyCode": presentment_currency_code
        }
    })
}

fn format_order_amount(amount: f64) -> String {
    let rounded = (amount * 100.0).round() / 100.0;
    let formatted = format!("{rounded:.2}");
    let trimmed = formatted.trim_end_matches('0');
    if trimmed.ends_with('.') {
        format!("{trimmed}0")
    } else {
        trimmed.to_string()
    }
}

fn resolved_money_amount(input: &BTreeMap<String, ResolvedValue>) -> Option<f64> {
    resolved_string_field(input, "amount")
        .and_then(|value| value.parse::<f64>().ok())
        .or_else(|| resolved_number_field(input, "amount"))
}

fn resolved_money_currency(input: &BTreeMap<String, ResolvedValue>) -> Option<String> {
    resolved_string_field(input, "currencyCode")
}

fn money_input(input: &BTreeMap<String, ResolvedValue>) -> Option<BTreeMap<String, ResolvedValue>> {
    resolved_object_field(input, "shopMoney").or_else(|| {
        let amount = resolved_money_amount(input)?;
        let currency = resolved_money_currency(input)?;
        Some(BTreeMap::from([
            (
                "amount".to_string(),
                ResolvedValue::String(format_order_amount(amount)),
            ),
            ("currencyCode".to_string(), ResolvedValue::String(currency)),
        ]))
    })
}

fn input_money_amount(input: &BTreeMap<String, ResolvedValue>) -> Option<f64> {
    money_input(input).and_then(|money| resolved_money_amount(&money))
}

fn input_money_currency(input: &BTreeMap<String, ResolvedValue>) -> Option<String> {
    money_input(input).and_then(|money| resolved_money_currency(&money))
}

fn order_create_address(input: Option<BTreeMap<String, ResolvedValue>>) -> Value {
    let Some(input) = input else {
        return Value::Null;
    };
    json!({
        "firstName": resolved_string_field(&input, "firstName").unwrap_or_default(),
        "lastName": resolved_string_field(&input, "lastName").unwrap_or_default(),
        "address1": resolved_string_field(&input, "address1").unwrap_or_default(),
        "address2": resolved_string_field(&input, "address2"),
        "city": resolved_string_field(&input, "city").unwrap_or_default(),
        "province": resolved_string_field(&input, "province"),
        "provinceCode": resolved_string_field(&input, "provinceCode").unwrap_or_default(),
        "country": resolved_string_field(&input, "country"),
        "countryCodeV2": resolved_string_field(&input, "countryCode")
            .or_else(|| resolved_string_field(&input, "countryCodeV2"))
            .unwrap_or_default(),
        "zip": resolved_string_field(&input, "zip").unwrap_or_default(),
        "phone": resolved_string_field(&input, "phone")
    })
}

fn order_create_custom_attributes(
    input: &BTreeMap<String, ResolvedValue>,
    field: &str,
) -> Vec<Value> {
    resolved_object_list_field(input, field)
        .into_iter()
        .filter_map(|attribute| {
            let key = resolved_string_field(&attribute, "key")
                .or_else(|| resolved_string_field(&attribute, "name"))?;
            let value = resolved_string_field(&attribute, "value").unwrap_or_default();
            Some(json!({ "key": key, "value": value }))
        })
        .collect()
}

fn order_create_tax_lines(
    input: &BTreeMap<String, ResolvedValue>,
    field: &str,
    currency_code: &str,
) -> Vec<Value> {
    resolved_object_list_field(input, field)
        .into_iter()
        .map(|tax_line| {
            let price = resolved_object_field(&tax_line, "priceSet")
                .and_then(|price| input_money_amount(&price))
                .unwrap_or(0.0);
            let price_currency = resolved_object_field(&tax_line, "priceSet")
                .and_then(|price| input_money_currency(&price))
                .unwrap_or_else(|| currency_code.to_string());
            json!({
                "title": resolved_string_field(&tax_line, "title").unwrap_or_default(),
                "rate": resolved_number_field(&tax_line, "rate").unwrap_or(0.0),
                "priceSet": order_create_money_set(price, &price_currency)
            })
        })
        .collect()
}

fn order_create_discount_amount(
    input: &BTreeMap<String, ResolvedValue>,
    currency_code: &str,
) -> (f64, Vec<String>) {
    let Some(discount_code) = resolved_object_field(input, "discountCode") else {
        return (0.0, Vec::new());
    };
    let Some(fixed) = resolved_object_field(&discount_code, "itemFixedDiscountCode")
        .or_else(|| resolved_object_field(&discount_code, "fixedAmountDiscountCode"))
    else {
        return (0.0, Vec::new());
    };
    let code = resolved_string_field(&fixed, "code").unwrap_or_default();
    let amount = resolved_object_field(&fixed, "amountSet")
        .and_then(|amount| input_money_amount(&amount))
        .or_else(|| {
            resolved_object_field(&fixed, "amount").and_then(|amount| input_money_amount(&amount))
        })
        .unwrap_or(0.0);
    let codes = if code.is_empty() {
        Vec::new()
    } else {
        vec![code]
    };
    let _ = currency_code;
    (amount, codes)
}

fn order_create_line_item_discount_allocations(discounts: &[Value]) -> Vec<Value> {
    discounts
        .iter()
        .filter_map(|discount| {
            let value = discount.get("value")?;
            let amount = value
                .get("amount")
                .and_then(Value::as_str)
                .and_then(|amount| amount.parse::<f64>().ok())
                .unwrap_or(0.0);
            let currency = value
                .get("currencyCode")
                .and_then(Value::as_str)
                .unwrap_or("CAD");
            Some(json!({ "allocatedAmountSet": order_create_money_set(amount, currency) }))
        })
        .collect()
}

fn order_create_line_item_record(
    input: &BTreeMap<String, ResolvedValue>,
    index: usize,
    currency_code: &str,
    presentment_currency_code: &str,
) -> (Value, f64, f64) {
    let quantity = resolved_i64_field(input, "quantity").unwrap_or(1).max(0);
    let price_input = resolved_object_field(input, "priceSet")
        .or_else(|| resolved_object_field(input, "originalUnitPriceSet"))
        .unwrap_or_default();
    let unit_amount = input_money_amount(&price_input).unwrap_or(0.0);
    let line_currency =
        input_money_currency(&price_input).unwrap_or_else(|| currency_code.to_string());
    let presentment_input = resolved_object_field(&price_input, "presentmentMoney");
    let presentment_amount = presentment_input
        .as_ref()
        .and_then(resolved_money_amount)
        .unwrap_or(unit_amount);
    let presentment_currency = presentment_input
        .as_ref()
        .and_then(resolved_money_currency)
        .unwrap_or_else(|| presentment_currency_code.to_string());
    let tax_lines = order_create_tax_lines(input, "taxLines", currency_code);
    let tax_total = tax_lines
        .iter()
        .filter_map(|tax_line| tax_line["priceSet"]["shopMoney"]["amount"].as_str())
        .filter_map(|amount| amount.parse::<f64>().ok())
        .sum::<f64>();
    let applied_discounts = resolved_object_list_field(input, "appliedDiscounts")
        .into_iter()
        .map(|discount| {
            let fixed = resolved_object_field(&discount, "value")
                .and_then(|value| resolved_object_field(&value, "fixedAmountValue"))
                .unwrap_or_default();
            let amount = resolved_money_amount(&fixed).unwrap_or(0.0);
            let currency =
                resolved_money_currency(&fixed).unwrap_or_else(|| currency_code.to_string());
            json!({
                "title": resolved_string_field(&discount, "title").unwrap_or_default(),
                "value": {
                    "amount": format_order_amount(amount),
                    "currencyCode": currency
                }
            })
        })
        .collect::<Vec<_>>();
    let custom_attributes = order_create_custom_attributes(input, "properties");
    let product_id = resolved_string_field(input, "productId");
    let variant_id = resolved_string_field(input, "variantId");
    let variant = variant_id
        .as_ref()
        .map(|id| json!({ "id": id }))
        .unwrap_or(Value::Null);
    let product = product_id
        .as_ref()
        .map(|id| json!({ "id": id }))
        .unwrap_or(Value::Null);
    let weight = resolved_object_field(input, "weight")
        .map(|weight| {
            json!({
                "value": resolved_number_field(&weight, "value").unwrap_or(0.0),
                "unit": resolved_string_field(&weight, "unit").unwrap_or_else(|| "KILOGRAMS".to_string())
            })
        })
        .unwrap_or(Value::Null);
    let line = json!({
        "id": format!("gid://shopify/LineItem/{}", index + 1),
        "title": resolved_string_field(input, "title").unwrap_or_else(|| "Custom Item".to_string()),
        "quantity": quantity,
        "currentQuantity": quantity,
        "sku": resolved_string_field(input, "sku").unwrap_or_default(),
        "variantTitle": resolved_string_field(input, "variantTitle"),
        "variantId": variant_id,
        "variant": variant,
        "productId": product_id,
        "product": product,
        "customAttributes": custom_attributes,
        "requiresShipping": resolved_bool_field(input, "requiresShipping").unwrap_or(true),
        "taxable": resolved_bool_field(input, "taxable").unwrap_or(true),
        "giftCard": resolved_bool_field(input, "giftCard").unwrap_or(false),
        "vendor": resolved_string_field(input, "vendor"),
        "fulfillmentService": resolved_string_field(input, "fulfillmentService"),
        "fulfillmentStatus": resolved_string_field(input, "fulfillmentStatus"),
        "weight": weight,
        "appliedDiscounts": applied_discounts.clone(),
        "discountAllocations": order_create_line_item_discount_allocations(&applied_discounts),
        "originalUnitPriceSet": json!({
            "shopMoney": {
                "amount": format_order_amount(unit_amount),
                "currencyCode": line_currency
            },
            "presentmentMoney": {
                "amount": format_order_amount(presentment_amount),
                "currencyCode": presentment_currency
            }
        }),
        "priceSet": json!({
            "shopMoney": {
                "amount": format_order_amount(unit_amount),
                "currencyCode": currency_code
            },
            "presentmentMoney": {
                "amount": format_order_amount(presentment_amount),
                "currencyCode": presentment_currency_code
            }
        }),
        "taxLines": tax_lines
    });
    (line, unit_amount * quantity as f64, tax_total)
}

fn order_fulfillment_order_line_item_record(line_item: &Value, index: usize) -> Value {
    let order_line_item_id = line_item
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let id_tail = if order_line_item_id.is_empty() {
        (index + 1).to_string()
    } else {
        resource_id_tail(order_line_item_id).to_string()
    };
    let quantity = line_item
        .get("quantity")
        .and_then(Value::as_i64)
        .unwrap_or(1)
        .max(0);
    json!({
        "id": format!("gid://shopify/FulfillmentOrderLineItem/{id_tail}"),
        "totalQuantity": quantity,
        "remainingQuantity": quantity,
        "lineItem": line_item
    })
}

fn order_default_fulfillment_order(order_id: &str, line_items: &[Value]) -> Value {
    let tail = resource_id_tail(order_id);
    let fulfillment_order_line_items = line_items
        .iter()
        .enumerate()
        .map(|(index, line_item)| order_fulfillment_order_line_item_record(line_item, index))
        .collect::<Vec<_>>();
    json!({
        "id": format!("gid://shopify/FulfillmentOrder/{tail}"),
        "status": "OPEN",
        "requestStatus": "UNSUBMITTED",
        "supportedActions": [],
        "lineItems": order_connection(fulfillment_order_line_items)
    })
}

fn order_create_transaction_record(
    input: &BTreeMap<String, ResolvedValue>,
    index: usize,
    currency_code: &str,
) -> Value {
    let amount_input = resolved_object_field(input, "amountSet").unwrap_or_default();
    let amount = input_money_amount(&amount_input).unwrap_or(0.0);
    let currency = input_money_currency(&amount_input).unwrap_or_else(|| currency_code.to_string());
    json!({
        "id": format!("gid://shopify/OrderTransaction/{}", index + 3),
        "kind": resolved_string_field(input, "kind").unwrap_or_else(|| "SALE".to_string()),
        "status": resolved_string_field(input, "status").unwrap_or_else(|| "SUCCESS".to_string()),
        "gateway": resolved_string_field(input, "gateway").unwrap_or_else(|| "manual".to_string()),
        "paymentId": Value::Null,
        "paymentReferenceId": Value::Null,
        "parentTransaction": Value::Null,
        "amountSet": order_money_set(&format_order_amount(amount), &currency)
    })
}

fn order_create_financial_status(
    input: &BTreeMap<String, ResolvedValue>,
    transactions: &[Value],
    total: f64,
) -> String {
    if let Some(status) = resolved_string_field(input, "financialStatus") {
        return status;
    }
    if transactions
        .iter()
        .any(|transaction| transaction["kind"] == "AUTHORIZATION")
    {
        return "AUTHORIZED".to_string();
    }
    let received = transactions
        .iter()
        .filter(|transaction| transaction["kind"] == "SALE" || transaction["kind"] == "CAPTURE")
        .filter(|transaction| transaction["status"] == "SUCCESS")
        .filter_map(|transaction| transaction["amountSet"]["shopMoney"]["amount"].as_str())
        .filter_map(|amount| amount.parse::<f64>().ok())
        .sum::<f64>();
    if received <= 0.0 || received + 0.005 >= total {
        "PAID".to_string()
    } else {
        "PARTIALLY_PAID".to_string()
    }
}

fn order_create_payment_fields(
    order: &mut Value,
    transactions: &[Value],
    total: f64,
    currency_code: &str,
) {
    let authorization = transactions
        .iter()
        .find(|transaction| transaction["kind"] == "AUTHORIZATION");
    let received = transactions
        .iter()
        .filter(|transaction| transaction["kind"] == "SALE" || transaction["kind"] == "CAPTURE")
        .filter(|transaction| transaction["status"] == "SUCCESS")
        .filter_map(|transaction| transaction["amountSet"]["shopMoney"]["amount"].as_str())
        .filter_map(|amount| amount.parse::<f64>().ok())
        .sum::<f64>();
    let capturable = authorization
        .and_then(|transaction| transaction["amountSet"]["shopMoney"]["amount"].as_str())
        .and_then(|amount| amount.parse::<f64>().ok())
        .unwrap_or(0.0);
    let outstanding = if authorization.is_some() {
        0.0
    } else {
        (total - received).max(0.0)
    };
    order["capturable"] = json!(capturable > 0.0);
    order["totalCapturable"] = json!(format_order_amount(capturable));
    order["totalCapturableSet"] = order_create_money_set(capturable, currency_code);
    order["totalOutstandingSet"] = order_create_money_set(outstanding, currency_code);
    order["totalReceivedSet"] = order_create_money_set(received, currency_code);
    order["netPaymentSet"] = order_create_money_set(received, currency_code);
    order["paymentGatewayNames"] = Value::Array(
        transactions
            .iter()
            .filter_map(|transaction| transaction["gateway"].as_str())
            .map(|gateway| json!(gateway))
            .collect(),
    );
}

fn order_create_validation_error(order: &BTreeMap<String, ResolvedValue>) -> Option<Value> {
    if resolved_string_field(order, "processedAt")
        .as_deref()
        .is_some_and(|value| value.starts_with("2099-"))
    {
        return Some(order_create_error(
            vec![json!("order"), json!("processedAt")],
            "Processed at is invalid",
            "PROCESSED_AT_INVALID",
        ));
    }
    if order.contains_key("customerId") && order.contains_key("customer") {
        return Some(order_create_error(
            vec![json!("order")],
            "Customer fields are redundant",
            "REDUNDANT_CUSTOMER_FIELDS",
        ));
    }
    let line_items = resolved_object_list_field(order, "lineItems");
    if line_items.is_empty() {
        return Some(order_create_error(
            vec![json!("order"), json!("lineItems")],
            "Line items must have at least one line item",
            "INVALID",
        ));
    }
    for (line_index, line_item) in line_items.iter().enumerate() {
        if let Some(service) = resolved_string_field(line_item, "fulfillmentService") {
            if service != "manual" && service != "gift_card" {
                return Some(order_create_error(
                    vec![
                        json!("order"),
                        json!("lineItems"),
                        json!(line_index),
                        json!("fulfillmentService"),
                    ],
                    "Fulfillment service is invalid",
                    "FULFILLMENT_SERVICE_INVALID",
                ));
            }
        }
        for (tax_index, tax_line) in resolved_object_list_field(line_item, "taxLines")
            .iter()
            .enumerate()
        {
            if !matches!(
                tax_line.get("rate"),
                Some(ResolvedValue::Int(_)) | Some(ResolvedValue::Float(_))
            ) {
                return Some(order_create_error(
                    vec![
                        json!("order"),
                        json!("lineItems"),
                        json!(line_index),
                        json!("taxLines"),
                        json!(tax_index),
                        json!("rate"),
                    ],
                    "Tax line rate is missing",
                    "TAX_LINE_RATE_MISSING",
                ));
            }
        }
    }
    for (shipping_index, shipping_line) in resolved_object_list_field(order, "shippingLines")
        .iter()
        .enumerate()
    {
        for (tax_index, tax_line) in resolved_object_list_field(shipping_line, "taxLines")
            .iter()
            .enumerate()
        {
            if !matches!(
                tax_line.get("rate"),
                Some(ResolvedValue::Int(_)) | Some(ResolvedValue::Float(_))
            ) {
                return Some(order_create_error(
                    vec![
                        json!("order"),
                        json!("shippingLines"),
                        json!(shipping_index),
                        json!("taxLines"),
                        json!(tax_index),
                        json!("rate"),
                    ],
                    "Tax line rate is missing",
                    "TAX_LINE_RATE_MISSING",
                ));
            }
        }
    }
    None
}

fn order_edit_existing_base_order() -> Value {
    json!({
        "id": "gid://shopify/Order/6834565087465",
        "name": "#1331",
        "updatedAt": "2024-01-01T00:00:03.000Z",
        "note": "merchant realistic draft order create parity probe",
        "merchantEditable": true,
        "merchantEditableErrors": [],
        "currentSubtotalLineItemsQuantity": 3,
        "lineItems": {
            "nodes": [
                {
                    "id": "gid://shopify/LineItem/1",
                    "title": "Custom installation service",
                    "quantity": 2,
                    "currentQuantity": 2,
                    "sku": "hermes-custom-service-1777076856718",
                    "variant": Value::Null,
                    "originalUnitPriceSet": order_money_set("10.0", "CAD")
                },
                {
                    "id": "gid://shopify/LineItem/2",
                    "title": "Test Product - 6635",
                    "quantity": 1,
                    "currentQuantity": 1,
                    "sku": Value::Null,
                    "variant": { "id": "gid://shopify/ProductVariant/48540157378793" },
                    "originalUnitPriceSet": order_money_set("0.0", "CAD")
                }
            ]
        }
    })
}

fn order_edit_existing_variant_line(quantity: i64, current_quantity: i64) -> Value {
    json!({
        "id": "gid://shopify/LineItem/3",
        "title": "VANS |AUTHENTIC | LO PRO | BURGANDY/WHITE",
        "quantity": quantity,
        "currentQuantity": current_quantity,
        "sku": "VN-01-burgandy-4",
        "variant": { "id": "gid://shopify/ProductVariant/46789254021353" },
        "originalUnitPriceSet": order_money_set("29.0", "CAD")
    })
}

fn order_edit_existing_calculated_line(quantity: i64, current_quantity: i64) -> Value {
    let mut line = order_edit_existing_variant_line(quantity, current_quantity);
    if let Some(object) = line.as_object_mut() {
        object.remove("id");
    }
    line
}

fn order_money_set(amount: &str, currency_code: &str) -> Value {
    json!({
        "shopMoney": {
            "amount": amount,
            "currencyCode": currency_code
        }
    })
}

fn order_money_set_pair(
    shop_amount: &str,
    shop_currency: &str,
    presentment_amount: &str,
    presentment_currency: &str,
) -> Value {
    json!({
        "shopMoney": {
            "amount": shop_amount,
            "currencyCode": shop_currency
        },
        "presentmentMoney": {
            "amount": presentment_amount,
            "currencyCode": presentment_currency
        }
    })
}

fn payment_money_amount(money_set: &Value, money_key: &str) -> Option<String> {
    money_set
        .get(money_key)
        .and_then(|money| money.get("amount"))
        .and_then(Value::as_str)
        .map(ToString::to_string)
}

fn payment_money_currency(money_set: &Value, money_key: &str) -> Option<String> {
    money_set
        .get(money_key)
        .and_then(|money| money.get("currencyCode"))
        .and_then(Value::as_str)
        .map(ToString::to_string)
}

fn payment_money_set_from_input(input: &BTreeMap<String, ResolvedValue>) -> Option<Value> {
    let amount_set = resolved_object_field(input, "amountSet")?;
    let shop_money = resolved_object_field(&amount_set, "shopMoney")?;
    let shop_amount = resolved_string_field(&shop_money, "amount")
        .map(|amount| normalized_order_payment_amount(Some(amount)))?;
    let shop_currency = resolved_string_field(&shop_money, "currencyCode")?;
    if let Some(presentment_money) = resolved_object_field(&amount_set, "presentmentMoney") {
        let presentment_amount = resolved_string_field(&presentment_money, "amount")
            .map(|amount| normalized_order_payment_amount(Some(amount)))
            .unwrap_or_else(|| shop_amount.clone());
        let presentment_currency = resolved_string_field(&presentment_money, "currencyCode")
            .unwrap_or_else(|| {
                resolved_string_field(input, "currency").unwrap_or_else(|| shop_currency.clone())
            });
        Some(order_money_set_pair(
            &shop_amount,
            &shop_currency,
            &presentment_amount,
            &presentment_currency,
        ))
    } else {
        Some(order_money_set(&shop_amount, &shop_currency))
    }
}

fn payment_money_set_value(amount_set: Value) -> Value {
    let shop_amount =
        payment_money_amount(&amount_set, "shopMoney").unwrap_or_else(|| "0.0".to_string());
    let shop_currency =
        payment_money_currency(&amount_set, "shopMoney").unwrap_or_else(|| "CAD".to_string());
    if amount_set.get("presentmentMoney").is_some() {
        let presentment_amount = payment_money_amount(&amount_set, "presentmentMoney")
            .unwrap_or_else(|| shop_amount.clone());
        let presentment_currency = payment_money_currency(&amount_set, "presentmentMoney")
            .unwrap_or_else(|| shop_currency.clone());
        order_money_set_pair(
            &shop_amount,
            &shop_currency,
            &presentment_amount,
            &presentment_currency,
        )
    } else {
        order_money_set(&shop_amount, &shop_currency)
    }
}

fn payment_money_set_for_capture(
    parent_amount_set: &Value,
    requested_amount: &str,
    requested_currency: &str,
) -> Value {
    let shop_currency = payment_money_currency(parent_amount_set, "shopMoney")
        .unwrap_or_else(|| requested_currency.to_string());
    let parent_shop_amount = payment_money_amount(parent_amount_set, "shopMoney")
        .and_then(|amount| amount.parse::<f64>().ok())
        .unwrap_or(0.0);
    let parent_presentment_amount = payment_money_amount(parent_amount_set, "presentmentMoney")
        .and_then(|amount| amount.parse::<f64>().ok())
        .unwrap_or(parent_shop_amount);
    let requested = requested_amount.parse::<f64>().unwrap_or(0.0);
    let shop_amount = if requested_currency == shop_currency {
        requested
    } else if parent_presentment_amount > 0.0 {
        requested * parent_shop_amount / parent_presentment_amount
    } else {
        requested
    };
    let shop_amount = format_order_amount(shop_amount);
    if parent_amount_set.get("presentmentMoney").is_some() || requested_currency != shop_currency {
        order_money_set_pair(
            &shop_amount,
            &shop_currency,
            &normalized_order_payment_amount(Some(requested_amount.to_string())),
            requested_currency,
        )
    } else {
        order_money_set(
            &normalized_order_payment_amount(Some(requested_amount.to_string())),
            requested_currency,
        )
    }
}

fn payment_money_set_for_order_totals(
    parent_amount_set: &Value,
    remaining_amount: f64,
    received_amount: f64,
) -> (Value, Value, Value) {
    let shop_currency =
        payment_money_currency(parent_amount_set, "shopMoney").unwrap_or_else(|| "CAD".to_string());
    if parent_amount_set.get("presentmentMoney").is_some() {
        let presentment_currency = payment_money_currency(parent_amount_set, "presentmentMoney")
            .unwrap_or_else(|| shop_currency.clone());
        (
            order_money_set_pair(
                &format_order_amount(remaining_amount),
                &shop_currency,
                &format_order_amount(remaining_amount),
                &presentment_currency,
            ),
            order_money_set_pair("0.0", &shop_currency, "0.0", &presentment_currency),
            order_money_set_pair(
                &format_order_amount(received_amount),
                &shop_currency,
                &format_order_amount(received_amount),
                &presentment_currency,
            ),
        )
    } else {
        (
            order_money_set(&format_order_amount(remaining_amount), &shop_currency),
            order_money_set(&format_order_amount(remaining_amount), &shop_currency),
            order_money_set(&format_order_amount(received_amount), &shop_currency),
        )
    }
}

fn payment_transaction_record_from_amount_set(
    id: &str,
    kind: &str,
    status: &str,
    amount_set: Value,
    parent_transaction: Value,
) -> Value {
    let transaction_number = id
        .rsplit('/')
        .next()
        .and_then(|value| value.parse::<u64>().ok());
    let payment_id = match (kind, transaction_number) {
        ("AUTHORIZATION", _) => Value::Null,
        (_, Some(number)) => json!(format!("gid://shopify/Payment/{}", number + 1)),
        _ => Value::Null,
    };
    let payment_reference_id = match (kind, transaction_number) {
        ("CAPTURE", Some(number)) if number > 0 => {
            json!(format!("gid://shopify/PaymentReference/{}", number - 1))
        }
        _ => Value::Null,
    };
    json!({
        "id": id,
        "kind": kind,
        "status": status,
        "gateway": "manual",
        "paymentId": payment_id,
        "paymentReferenceId": payment_reference_id,
        "parentTransaction": parent_transaction,
        "amountSet": payment_money_set_value(amount_set)
    })
}

fn payment_transaction_public_parent(transaction: &Value) -> Value {
    json!({
        "id": transaction.get("id").cloned().unwrap_or(Value::Null),
        "kind": transaction.get("kind").cloned().unwrap_or(Value::Null),
        "status": transaction.get("status").cloned().unwrap_or(Value::Null)
    })
}

fn payment_transaction_matches_parent(transaction: &Value, parent_id: &str) -> bool {
    transaction
        .get("parentTransaction")
        .and_then(|parent| parent.get("id"))
        .and_then(Value::as_str)
        == Some(parent_id)
}

fn payment_user_error(field: Value, message: &str, code: Option<&str>) -> Value {
    let mut error = serde_json::Map::new();
    error.insert("field".to_string(), field);
    error.insert("message".to_string(), json!(message));
    if let Some(code) = code {
        error.insert("code".to_string(), json!(code));
    }
    Value::Object(error)
}

fn order_connection(nodes: Vec<Value>) -> Value {
    let start_cursor = nodes
        .first()
        .and_then(|node| node.get("id"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_default();
    let end_cursor = nodes
        .last()
        .and_then(|node| node.get("id"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_default();
    json!({
        "nodes": nodes,
        "pageInfo": {
            "hasNextPage": false,
            "hasPreviousPage": false,
            "startCursor": if start_cursor.is_empty() { Value::Null } else { json!(start_cursor) },
            "endCursor": if end_cursor.is_empty() { Value::Null } else { json!(end_cursor) }
        }
    })
}

fn b2b_purchasing_entity_record<F>(
    purchasing_entity_input: Option<BTreeMap<String, ResolvedValue>>,
    company_for_id: F,
    locations: &BTreeMap<String, Value>,
) -> Value
where
    F: Fn(&str) -> Option<Value>,
{
    let Some(purchasing_entity_input) = purchasing_entity_input else {
        return Value::Null;
    };
    let Some(purchasing_company) =
        resolved_object_field(&purchasing_entity_input, "purchasingCompany")
    else {
        return Value::Null;
    };
    let company_id = resolved_string_field(&purchasing_company, "companyId").unwrap_or_default();
    if company_id.is_empty() {
        return Value::Null;
    }
    let company = company_for_id(&company_id)
        .map(|company| json!({ "id": company["id"].clone(), "name": company["name"].clone() }))
        .unwrap_or_else(|| json!({ "id": company_id }));
    let contact = resolved_string_field(&purchasing_company, "companyContactId")
        .map(|id| json!({ "id": id }))
        .unwrap_or(Value::Null);
    let location = resolved_string_field(&purchasing_company, "companyLocationId")
        .map(|id| {
            locations
                .get(&id)
                .map(|location| {
                    json!({ "id": id, "name": location["name"].clone(), "company": { "id": company_id } })
                })
                .unwrap_or_else(|| json!({ "id": id, "company": { "id": company_id } }))
        })
        .unwrap_or(Value::Null);
    json!({
        "__typename": "PurchasingCompany",
        "company": company,
        "contact": contact,
        "location": location
    })
}

fn data_response(response_key: &str, value: Value) -> Value {
    let mut data = serde_json::Map::new();
    data.insert(response_key.to_string(), value);
    json!({ "data": Value::Object(data) })
}

fn fulfillment_tracking_info(input: &BTreeMap<String, ResolvedValue>) -> Vec<Value> {
    let company = resolved_string_field(input, "company")
        .or_else(|| resolved_string_field(input, "trackingCompany"));
    let numbers = resolved_string_list_field_unsorted(input, "numbers");
    let urls = resolved_string_list_field_unsorted(input, "urls");
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

fn fulfillment_order_nodes_mut(order: &mut Value) -> Option<&mut Vec<Value>> {
    order
        .get_mut("fulfillmentOrders")?
        .get_mut("nodes")?
        .as_array_mut()
}

fn order_fulfillments_mut(order: &mut Value) -> Option<&mut Vec<Value>> {
    order.get_mut("fulfillments")?.as_array_mut()
}

fn normalize_hydrated_order(order: &mut Value) {
    if order
        .get("fulfillments")
        .is_some_and(|fulfillments| fulfillments.is_null())
    {
        order["fulfillments"] = json!([]);
    }
    if let Some(nodes) = order
        .get("fulfillments")
        .and_then(|fulfillments| fulfillments.get("nodes"))
        .and_then(Value::as_array)
        .cloned()
    {
        order["fulfillments"] = Value::Array(nodes);
    }
    if !order
        .get("fulfillments")
        .is_some_and(|fulfillments| fulfillments.is_array())
    {
        order["fulfillments"] = json!([]);
    }
    if !order
        .get("fulfillmentOrders")
        .and_then(|connection| connection.get("nodes"))
        .is_some_and(|nodes| nodes.is_array())
    {
        order["fulfillmentOrders"] = order_connection(Vec::new());
    }
}

fn fulfillment_line_item_record(line: &Value, quantity: i64) -> Value {
    let line_id = line.get("id").and_then(Value::as_str).unwrap_or_default();
    let fulfillment_line_item_id = if line_id.is_empty() {
        "gid://shopify/FulfillmentLineItem/1".to_string()
    } else {
        format!(
            "gid://shopify/FulfillmentLineItem/{}",
            resource_id_tail(line_id)
        )
    };
    json!({
        "id": fulfillment_line_item_id,
        "quantity": quantity,
        "lineItem": line.get("lineItem").cloned().unwrap_or(Value::Null)
    })
}

fn fulfillment_group_line_items(
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
            let quantity = resolved_i64_field(requested, "quantity")
                .unwrap_or(0)
                .max(0);
            line_nodes
                .iter()
                .find(|line| line["id"].as_str() == Some(requested_id.as_str()))
                .map(|line| fulfillment_line_item_record(line, quantity))
        })
        .collect()
}

fn fulfillment_create_closed_order_error(fulfillment_order_id: &str) -> Value {
    json!({
        "field": ["fulfillment"],
        "message": format!(
            "Fulfillment order {} has an unfulfillable status= closed.",
            resource_id_tail(fulfillment_order_id)
        )
    })
}

fn fulfillment_create_invalid_quantity_error() -> Value {
    json!({
        "field": ["fulfillment"],
        "message": "Invalid fulfillment order line item quantity requested."
    })
}

fn fulfillment_create_precondition_error(
    order: &Value,
    groups: &[BTreeMap<String, ResolvedValue>],
) -> Option<Value> {
    for group in groups {
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
        for requested in resolved_object_list_field(group, "fulfillmentOrderLineItems") {
            let Some(requested_id) = resolved_string_field(&requested, "id") else {
                return Some(fulfillment_create_invalid_quantity_error());
            };
            let requested_quantity = resolved_i64_field(&requested, "quantity").unwrap_or(0);
            let Some(line) = line_nodes
                .iter()
                .find(|line| line["id"].as_str() == Some(requested_id.as_str()))
            else {
                return Some(fulfillment_create_invalid_quantity_error());
            };
            let remaining = line["remainingQuantity"].as_i64().unwrap_or(0);
            if requested_quantity <= 0 || requested_quantity > remaining {
                return Some(fulfillment_create_invalid_quantity_error());
            }
        }
    }
    None
}

fn update_order_fulfillment_status(order: &mut Value) {
    let fulfillment_count = order["fulfillments"]
        .as_array()
        .map(Vec::len)
        .unwrap_or_default();
    if fulfillment_count == 0 {
        return;
    }
    let nodes = order["fulfillmentOrders"]["nodes"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    if nodes.is_empty() {
        return;
    }
    let all_closed = nodes.iter().all(|node| {
        node["status"]
            .as_str()
            .is_some_and(|status| status.eq_ignore_ascii_case("CLOSED"))
    });
    order["displayFulfillmentStatus"] = json!(if all_closed {
        "FULFILLED"
    } else {
        "PARTIALLY_FULFILLED"
    });
}

fn fulfillment_status_is(fulfillment: &Value, expected: &str) -> bool {
    fulfillment["status"]
        .as_str()
        .is_some_and(|status| status.eq_ignore_ascii_case(expected))
}

fn fulfillment_display_status_is(fulfillment: &Value, expected: &str) -> bool {
    fulfillment["displayStatus"]
        .as_str()
        .is_some_and(|status| status.eq_ignore_ascii_case(expected))
}

fn draft_order_total_amount(field: &RootFieldSelection) -> String {
    let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
    let line_items = resolved_object_list_field(&input, "lineItems");
    let first_line = line_items.first().cloned().unwrap_or_default();
    let quantity = resolved_i64_field(&first_line, "quantity").unwrap_or(1);
    let unit = resolved_string_field(&first_line, "originalUnitPrice")
        .and_then(|value| value.parse::<f64>().ok())
        .unwrap_or(10.0);
    format!("{:.1}", unit * quantity as f64)
}

fn draft_order_line_item_record(field: &RootFieldSelection) -> Value {
    let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
    let line_items = resolved_object_list_field(&input, "lineItems");
    let first_line = line_items.first().cloned().unwrap_or_default();
    let title = resolved_string_field(&first_line, "title")
        .unwrap_or_else(|| "Draft order item".to_string());
    let quantity = resolved_i64_field(&first_line, "quantity").unwrap_or(1);
    let sku = resolved_string_field(&first_line, "sku").unwrap_or_default();
    json!({
        "id": "gid://shopify/LineItem/5",
        "title": title,
        "quantity": quantity,
        "sku": sku
    })
}

fn payment_order_record(
    id: &str,
    display_financial_status: &str,
    capturable_amount: &str,
    outstanding_amount: &str,
    received_amount: &str,
    currency_code: &str,
    transactions: Vec<Value>,
) -> Value {
    json!({
        "id": id,
        "displayFinancialStatus": display_financial_status,
        "capturable": capturable_amount != "0.00",
        "totalCapturable": capturable_amount,
        "totalCapturableSet": order_money_set(capturable_amount, currency_code),
        "totalOutstandingSet": order_money_set(outstanding_amount, currency_code),
        "totalReceivedSet": order_money_set(received_amount, currency_code),
        "netPaymentSet": order_money_set(received_amount, currency_code),
        "paymentGatewayNames": ["manual"],
        "transactions": transactions
    })
}

fn normalized_order_payment_amount(value: Option<String>) -> String {
    let value = value.unwrap_or_else(|| "25.00".to_string());
    if let Some(prefix) = value.strip_suffix(".00") {
        format!("{prefix}.0")
    } else {
        value
    }
}

fn mandate_payment_order_record(
    order_id: &str,
    idempotency_key: &str,
    amount: &str,
    currency_code: &str,
    auto_capture: bool,
) -> Value {
    let payment_reference_id = format!("{order_id}/{idempotency_key}");
    let transaction_kind = if auto_capture {
        "SALE"
    } else {
        "AUTHORIZATION"
    };
    let display_financial_status = if auto_capture { "PAID" } else { "AUTHORIZED" };
    let total_capturable = if auto_capture { "0.0" } else { amount };
    let outstanding_amount = if auto_capture { "0.0" } else { amount };
    let received_amount = if auto_capture { amount } else { "0.0" };
    let transaction = json!({
        "id": "gid://shopify/OrderTransaction/4",
        "kind": transaction_kind,
        "status": "SUCCESS",
        "gateway": "mandate",
        "paymentReferenceId": payment_reference_id,
        "amountSet": order_money_set(amount, currency_code)
    });
    json!({
        "id": order_id,
        "displayFinancialStatus": display_financial_status,
        "capturable": !auto_capture,
        "totalCapturable": total_capturable,
        "totalCapturableSet": order_money_set(total_capturable, currency_code),
        "totalOutstandingSet": order_money_set(outstanding_amount, currency_code),
        "totalReceivedSet": order_money_set(received_amount, currency_code),
        "netPaymentSet": order_money_set(received_amount, currency_code),
        "paymentGatewayNames": ["mandate"],
        "transactions": [transaction]
    })
}

impl DraftProxy {
    pub(in crate::proxy) fn online_store_query_data(&self, fields: &[RootFieldSelection]) -> Value {
        let mut data = serde_json::Map::new();
        for field in fields {
            let value = match field.name.as_str() {
                "mobilePlatformApplication"
                | "scriptTag"
                | "webPixel"
                | "serverPixel"
                | "theme" => {
                    let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
                    self.store
                        .staged
                        .online_store_integrations
                        .get(&id)
                        .map(|record| selected_json(record, &field.selection))
                        .unwrap_or(Value::Null)
                }
                "themes" => {
                    let roles = resolved_string_list_arg(&field.arguments, "roles");
                    let mut records: Vec<Value> =
                        self.store
                            .staged
                            .online_store_integrations
                            .values()
                            .filter(|record| is_online_store_theme_record(record))
                            .filter(|record| {
                                roles.is_empty()
                                    || record.get("role").and_then(Value::as_str).is_some_and(
                                        |role| roles.iter().any(|expected| expected == role),
                                    )
                            })
                            .cloned()
                            .collect();
                    records.sort_by_key(value_id_cursor);
                    selected_connection_json_with_args(
                        records,
                        &field.arguments,
                        &field.selection,
                        value_id_cursor,
                    )
                }
                "scriptTags" => {
                    let mut records: Vec<Value> = self
                        .store
                        .staged
                        .online_store_integrations
                        .values()
                        .filter(|record| {
                            record
                                .get("id")
                                .and_then(Value::as_str)
                                .is_some_and(|id| id.starts_with("gid://shopify/ScriptTag/"))
                        })
                        .cloned()
                        .collect();
                    records.sort_by_key(value_id_cursor);
                    selected_connection_json_with_args(
                        records,
                        &field.arguments,
                        &field.selection,
                        value_id_cursor,
                    )
                }
                "mobilePlatformApplications" => {
                    let mut records: Vec<Value> = self
                        .store
                        .staged
                        .online_store_integrations
                        .values()
                        .filter(|record| {
                            matches!(
                                record.get("__typename").and_then(Value::as_str),
                                Some("AppleApplication" | "AndroidApplication")
                            )
                        })
                        .cloned()
                        .collect();
                    records.sort_by_key(value_id_cursor);
                    selected_connection_json_with_args(
                        records,
                        &field.arguments,
                        &field.selection,
                        value_id_cursor,
                    )
                }
                _ => Value::Null,
            };
            data.insert(field.response_key.clone(), value);
        }
        Value::Object(data)
    }

    pub(in crate::proxy) fn online_store_mutation(
        &mut self,
        fields: &[RootFieldSelection],
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Response {
        let mut data = serde_json::Map::new();
        let mut staged_ids = Vec::new();
        for field in fields {
            let value = match field.name.as_str() {
                "mobilePlatformApplicationCreate" => {
                    self.mobile_platform_application_create(field, &mut staged_ids)
                }
                "mobilePlatformApplicationUpdate" => {
                    self.mobile_platform_application_update(field, &mut staged_ids)
                }
                "scriptTagCreate" => self.script_tag_create(field, &mut staged_ids),
                "scriptTagUpdate" => self.script_tag_update(field, &mut staged_ids),
                "scriptTagDelete" => self.delete_online_store_integration(
                    field,
                    SCRIPT_TAG_DELETE_SPEC,
                    &mut staged_ids,
                ),
                "themeCreate" => self.theme_create(field, &mut staged_ids),
                "themePublish" => self.theme_publish(field, &mut staged_ids),
                "themeUpdate" => self.theme_update(field, &mut staged_ids),
                "themeDelete" => self.theme_delete(field, &mut staged_ids),
                "themeFilesUpsert" => self.theme_files_upsert(field),
                "themeFilesCopy" => self.theme_files_copy(field),
                "themeFilesDelete" => self.theme_files_delete(field),
                "webPixelCreate" => self.web_pixel_create(field, &mut staged_ids),
                "webPixelUpdate" => self.web_pixel_update(field, false, &mut staged_ids),
                "webPixelDelete" => self.delete_online_store_integration(
                    field,
                    WEB_PIXEL_DELETE_SPEC,
                    &mut staged_ids,
                ),
                "serverPixelCreate" => self.server_pixel_create(field, &mut staged_ids),
                "serverPixelDelete" => self.server_pixel_delete(field, &mut staged_ids),
                "eventBridgeServerPixelUpdate" => self.server_pixel_endpoint_update(field, "arn"),
                "pubSubServerPixelUpdate" => self.server_pixel_endpoint_update(field, "pubsub"),
                "storefrontAccessTokenCreate" => {
                    self.storefront_access_token_create(field, request, &mut staged_ids)
                }
                "storefrontAccessTokenDelete" => self.delete_online_store_integration(
                    field,
                    STOREFRONT_ACCESS_TOKEN_DELETE_SPEC,
                    &mut staged_ids,
                ),
                "mobilePlatformApplicationDelete" => self.delete_online_store_integration(
                    field,
                    MOBILE_PLATFORM_APPLICATION_DELETE_SPEC,
                    &mut staged_ids,
                ),
                _ => Value::Null,
            };
            data.insert(field.response_key.clone(), value);
        }
        if !staged_ids.is_empty() {
            self.record_mutation_log_entry(
                request,
                query,
                variables,
                fields
                    .first()
                    .map(|f| f.name.as_str())
                    .unwrap_or("onlineStore"),
                staged_ids,
            );
        }
        ok_json(json!({ "data": Value::Object(data) }))
    }

    pub(in crate::proxy) fn next_online_store_id(&mut self, typename: &str) -> String {
        let id = format!(
            "gid://shopify/{}/{}?shopify-draft-proxy=synthetic",
            typename, self.next_synthetic_id
        );
        self.next_synthetic_id += 1;
        id
    }

    fn delete_online_store_integration(
        &mut self,
        field: &RootFieldSelection,
        spec: OnlineStoreIntegrationDeleteSpec,
        staged_ids: &mut Vec<String>,
    ) -> Value {
        let supplied_id =
            online_store_delete_id_arg(field, spec.input_object_field).unwrap_or_default();
        let mut deleted_id = Value::Null;
        let mut user_errors = Vec::new();

        if shopify_gid_resource_type(&supplied_id) != Some(spec.expected_type) {
            user_errors.push(online_store_delete_user_error(
                spec.user_error_typename,
                "INVALID",
                &[online_store_delete_id_field_path(spec.input_object_field)],
                &format!("{} id is invalid", spec.resource_label),
            ));
        } else if self
            .store
            .staged
            .online_store_integrations
            .remove(&supplied_id)
            .is_some()
        {
            deleted_id = json!(supplied_id);
            staged_ids.push(
                deleted_id
                    .as_str()
                    .expect("deleted integration id is a string")
                    .to_string(),
            );
        } else {
            user_errors.push(online_store_delete_user_error(
                spec.user_error_typename,
                "NOT_FOUND",
                &[online_store_delete_id_field_path(spec.input_object_field)],
                &format!("{} not found", spec.resource_label),
            ));
        }

        selected_payload_json(&field.selection, |selection| {
            match selection.name.as_str() {
                name if name == spec.deleted_id_field => Some(deleted_id.clone()),
                "userErrors" => Some(selected_online_store_user_errors(
                    &user_errors,
                    &selection.selection,
                )),
                _ => None,
            }
        })
    }

    pub(in crate::proxy) fn mobile_platform_application_create(
        &mut self,
        field: &RootFieldSelection,
        staged_ids: &mut Vec<String>,
    ) -> Value {
        let input = match field.arguments.get("input") {
            Some(ResolvedValue::Object(input)) => input,
            _ => {
                return mobile_app_payload(
                    &field.selection,
                    None,
                    vec![mobile_app_error(
                        "INVALID",
                        ["mobilePlatformApplication"],
                        "Specify either android or apple, not both.",
                    )],
                )
            }
        };
        let android = match input.get("android") {
            Some(ResolvedValue::Object(v)) => Some(v),
            _ => None,
        };
        let apple = match input.get("apple") {
            Some(ResolvedValue::Object(v)) => Some(v),
            _ => None,
        };
        if android.is_none() == apple.is_none() {
            return mobile_app_payload(
                &field.selection,
                None,
                vec![mobile_app_error(
                    "INVALID",
                    ["mobilePlatformApplication"],
                    "Specify either android or apple, not both.",
                )],
            );
        }
        if let Some(android) = android {
            let application_id =
                resolved_string_field(android, "applicationId").unwrap_or_default();
            if application_id.trim().is_empty() {
                return mobile_app_payload(
                    &field.selection,
                    None,
                    vec![mobile_app_error(
                        "BLANK",
                        ["mobilePlatformApplication", "android", "applicationId"],
                        if application_id.is_empty() {
                            "Application can't be blank"
                        } else {
                            "Application ID can't be blank"
                        },
                    )],
                );
            }
            if application_id.len() > MOBILE_PLATFORM_APPLICATION_ID_MAX_LENGTH {
                return mobile_app_payload(
                    &field.selection,
                    None,
                    vec![mobile_application_id_too_long_error([
                        "input",
                        "android",
                        "applicationId",
                    ])],
                );
            }
            if resolved_string_list_field(android, "sha256CertFingerprints").is_empty() {
                return mobile_app_payload(
                    &field.selection,
                    None,
                    vec![mobile_app_error(
                        "BLANK",
                        ["input", "android", "sha256CertFingerprints"],
                        "Sha256 cert fingerprints can't be blank",
                    )],
                );
            }
            let id = self.next_online_store_id("MobilePlatformApplication");
            let record = json!({
                "__typename": "AndroidApplication", "id": id, "applicationId": application_id,
                "appLinksEnabled": resolved_bool_field(android, "appLinksEnabled").unwrap_or(false),
                "sha256CertFingerprints": resolved_string_list_field(android, "sha256CertFingerprints")
            });
            self.store
                .staged
                .online_store_integrations
                .insert(id.clone(), record.clone());
            staged_ids.push(id);
            return mobile_app_payload(&field.selection, Some(record), Vec::new());
        }
        let apple = apple.unwrap();
        let app_id = resolved_string_field(apple, "appId").unwrap_or_default();
        if app_id.trim().is_empty() {
            return mobile_app_payload(
                &field.selection,
                None,
                vec![mobile_app_error(
                    "BLANK",
                    ["mobilePlatformApplication", "apple", "appId"],
                    if app_id.trim().is_empty() && app_id.len() > 1 {
                        "App can't be blank"
                    } else {
                        "App ID can't be blank"
                    },
                )],
            );
        }
        if app_id.len() > MOBILE_PLATFORM_APPLICATION_ID_MAX_LENGTH {
            return mobile_app_payload(
                &field.selection,
                None,
                vec![mobile_application_id_too_long_error([
                    "input", "apple", "appId",
                ])],
            );
        }
        if let Some(error) = validate_mobile_app_clip_application_id(apple, false) {
            return mobile_app_payload(&field.selection, None, vec![error]);
        }
        let id = self.next_online_store_id("MobilePlatformApplication");
        let record = json!({
            "__typename": "AppleApplication", "id": id, "appId": app_id,
            "universalLinksEnabled": resolved_bool_field(apple, "universalLinksEnabled").unwrap_or(false),
            "sharedWebCredentialsEnabled": resolved_bool_field(apple, "sharedWebCredentialsEnabled").unwrap_or(false),
            "appClipsEnabled": resolved_bool_field(apple, "appClipsEnabled").unwrap_or(false),
            "appClipApplicationId": resolved_string_field(apple, "appClipApplicationId").unwrap_or_default()
        });
        self.store
            .staged
            .online_store_integrations
            .insert(id.clone(), record.clone());
        staged_ids.push(id);
        mobile_app_payload(&field.selection, Some(record), Vec::new())
    }

    pub(in crate::proxy) fn mobile_platform_application_update(
        &mut self,
        field: &RootFieldSelection,
        staged_ids: &mut Vec<String>,
    ) -> Value {
        let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
        let Some(existing) = self
            .store
            .staged
            .online_store_integrations
            .get(&id)
            .cloned()
        else {
            return mobile_app_payload(
                &field.selection,
                None,
                vec![mobile_app_error(
                    "NOT_FOUND",
                    ["id"],
                    "Mobile platform application not found",
                )],
            );
        };
        let input = match field.arguments.get("input") {
            Some(ResolvedValue::Object(input)) => input,
            _ => return mobile_app_payload(&field.selection, None, Vec::new()),
        };
        let android = match input.get("android") {
            Some(ResolvedValue::Object(v)) => Some(v),
            _ => None,
        };
        let apple = match input.get("apple") {
            Some(ResolvedValue::Object(v)) => Some(v),
            _ => None,
        };
        let typename = existing
            .get("__typename")
            .and_then(Value::as_str)
            .unwrap_or("");
        if (typename == "AndroidApplication" && apple.is_some())
            || (typename == "AppleApplication" && android.is_some())
        {
            return mobile_app_payload(
                &field.selection,
                None,
                vec![mobile_app_error(
                    "INVALID",
                    ["mobilePlatformApplication"],
                    "Mobile platform application platform is invalid",
                )],
            );
        }
        let mut record = existing;
        if let Some(android) = android {
            if let Some(application_id) = resolved_string_field(android, "applicationId") {
                if application_id.trim().is_empty() {
                    return mobile_app_payload(
                        &field.selection,
                        None,
                        vec![mobile_app_error(
                            "BLANK",
                            ["mobilePlatformApplication", "android", "applicationId"],
                            "Application ID can't be blank",
                        )],
                    );
                }
                if application_id.len() > MOBILE_PLATFORM_APPLICATION_ID_MAX_LENGTH {
                    return mobile_app_payload(
                        &field.selection,
                        None,
                        vec![mobile_application_id_too_long_error([
                            "input",
                            "android",
                            "applicationId",
                        ])],
                    );
                }
                record["applicationId"] = json!(application_id);
            }
            if let Some(v) = resolved_bool_field(android, "appLinksEnabled") {
                record["appLinksEnabled"] = json!(v);
            }
            let fingerprints = resolved_string_list_field(android, "sha256CertFingerprints");
            if fingerprints.is_empty() {
                return mobile_app_payload(
                    &field.selection,
                    None,
                    vec![mobile_app_error(
                        "BLANK",
                        ["input", "android", "sha256CertFingerprints"],
                        "Sha256 cert fingerprints can't be blank",
                    )],
                );
            }
            record["sha256CertFingerprints"] = json!(fingerprints);
        }
        if let Some(apple) = apple {
            if let Some(app_id) = resolved_string_field(apple, "appId") {
                if app_id.trim().is_empty() {
                    return mobile_app_payload(
                        &field.selection,
                        None,
                        vec![mobile_app_error(
                            "BLANK",
                            ["mobilePlatformApplication", "apple", "appId"],
                            "App ID can't be blank",
                        )],
                    );
                }
                if app_id.len() > MOBILE_PLATFORM_APPLICATION_ID_MAX_LENGTH {
                    return mobile_app_payload(
                        &field.selection,
                        None,
                        vec![mobile_application_id_too_long_error([
                            "input", "apple", "appId",
                        ])],
                    );
                }
                record["appId"] = json!(app_id);
            }
            if let Some(error) = validate_mobile_app_clip_application_id(apple, true) {
                return mobile_app_payload(&field.selection, None, vec![error]);
            }
            if let Some(v) = resolved_bool_field(apple, "universalLinksEnabled") {
                record["universalLinksEnabled"] = json!(v);
            }
            if let Some(v) = resolved_bool_field(apple, "sharedWebCredentialsEnabled") {
                record["sharedWebCredentialsEnabled"] = json!(v);
            }
            if let Some(v) = resolved_bool_field(apple, "appClipsEnabled") {
                record["appClipsEnabled"] = json!(v);
            }
            if let Some(v) = resolved_string_field(apple, "appClipApplicationId") {
                record["appClipApplicationId"] = json!(v);
            }
        }
        self.store
            .staged
            .online_store_integrations
            .insert(id.clone(), record.clone());
        staged_ids.push(id);
        mobile_app_payload(&field.selection, Some(record), Vec::new())
    }

    pub(in crate::proxy) fn script_tag_create(
        &mut self,
        field: &RootFieldSelection,
        staged_ids: &mut Vec<String>,
    ) -> Value {
        let input = match field.arguments.get("input") {
            Some(ResolvedValue::Object(input)) => input,
            _ => return script_tag_payload(&field.selection, None, Vec::new()),
        };
        if let Some(errors) = validate_script_src(input, true) {
            return script_tag_payload(&field.selection, None, vec![errors]);
        }
        let id = self.next_online_store_id("ScriptTag");
        let record = json!({
            "id": id, "src": resolved_string_field(input, "src").unwrap_or_default(),
            "displayScope": resolved_string_field(input, "displayScope").unwrap_or_else(|| "ONLINE_STORE".to_string()),
            "event": "onload", "cache": resolved_bool_field(input, "cache").unwrap_or(false)
        });
        self.store
            .staged
            .online_store_integrations
            .insert(id.clone(), record.clone());
        staged_ids.push(id);
        script_tag_payload(&field.selection, Some(record), Vec::new())
    }

    pub(in crate::proxy) fn script_tag_update(
        &mut self,
        field: &RootFieldSelection,
        staged_ids: &mut Vec<String>,
    ) -> Value {
        let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
        let input = match field.arguments.get("input") {
            Some(ResolvedValue::Object(input)) => input,
            _ => return script_tag_payload(&field.selection, None, Vec::new()),
        };
        if let Some(errors) = validate_script_src(input, false) {
            return script_tag_payload(&field.selection, None, vec![errors]);
        }
        if matches!(input.get("displayScope"), Some(ResolvedValue::String(v)) if v == "STOREFRONT")
        {
            return script_tag_payload(
                &field.selection,
                None,
                vec![
                    json!({"code": "INCLUSION", "field": ["displayScope"], "message": "Display scope is not included in the list"}),
                ],
            );
        }
        let mut record = self.store.staged.online_store_integrations.get(&id).cloned().unwrap_or_else(|| json!({"id": id, "src": "https://cdn.example.test/app.js", "displayScope": "ALL", "event": "onload", "cache": false}));
        if let Some(src) = resolved_string_field(input, "src") {
            record["src"] = json!(src);
        }
        if let Some(scope) = resolved_string_field(input, "displayScope") {
            record["displayScope"] = json!(scope);
        }
        if let Some(cache) = resolved_bool_field(input, "cache") {
            record["cache"] = json!(cache);
        }
        record["event"] = json!("onload");
        self.store
            .staged
            .online_store_integrations
            .insert(id.clone(), record.clone());
        staged_ids.push(id);
        script_tag_payload(&field.selection, Some(record), Vec::new())
    }

    pub(in crate::proxy) fn theme_create(
        &mut self,
        field: &RootFieldSelection,
        staged_ids: &mut Vec<String>,
    ) -> Value {
        let id = self.next_online_store_id("OnlineStoreTheme");
        let record = json!({
            "__typename": "OnlineStoreTheme",
            "id": id,
            "name": resolved_string_arg(&field.arguments, "name").unwrap_or_else(|| "Local preview theme".to_string()),
            "role": resolved_string_arg(&field.arguments, "role").unwrap_or_else(|| "UNPUBLISHED".to_string()),
            "processing": false,
            "processingFailed": false,
            "files": {"nodes": []}
        });
        self.store
            .staged
            .online_store_integrations
            .insert(id.clone(), record.clone());
        staged_ids.push(id);
        selected_json(
            &json!({"theme": record, "userErrors": []}),
            &field.selection,
        )
    }

    pub(in crate::proxy) fn theme_publish(
        &mut self,
        field: &RootFieldSelection,
        staged_ids: &mut Vec<String>,
    ) -> Value {
        let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
        let Some(existing) = self
            .store
            .staged
            .online_store_integrations
            .get(&id)
            .cloned()
        else {
            return selected_json(
                &json!({"theme": null, "userErrors": [theme_user_error(vec!["id"], "Theme not found", Some("NOT_FOUND"))]}),
                &field.selection,
            );
        };
        let role = existing
            .get("role")
            .and_then(Value::as_str)
            .unwrap_or("UNPUBLISHED");
        if role == "DEVELOPMENT" {
            return selected_json(
                &json!({"theme": null, "userErrors": [{"field": ["base"], "message": "You cannot publish a development theme.", "code": null}]}),
                &field.selection,
            );
        }
        if matches!(role, "DEMO" | "LOCKED" | "ARCHIVED") {
            return selected_json(
                &json!({"theme": null, "userErrors": [{"field": ["id"], "message": format!("Theme cannot be published from role {role}")}]}),
                &field.selection,
            );
        }
        for record in self.store.staged.online_store_integrations.values_mut() {
            if is_online_store_theme_record(record)
                && record.get("role").and_then(Value::as_str) == Some("MAIN")
            {
                record["role"] = json!("UNPUBLISHED");
            }
        }
        let mut theme = existing;
        theme["role"] = json!("MAIN");
        self.store
            .staged
            .online_store_integrations
            .insert(id.clone(), theme.clone());
        staged_ids.push(id);
        selected_json(&json!({"theme": theme, "userErrors": []}), &field.selection)
    }

    pub(in crate::proxy) fn theme_update(
        &mut self,
        field: &RootFieldSelection,
        staged_ids: &mut Vec<String>,
    ) -> Value {
        let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
        let Some(mut theme) = self
            .store
            .staged
            .online_store_integrations
            .get(&id)
            .cloned()
        else {
            return selected_json(
                &json!({"theme": null, "userErrors": [theme_user_error(vec!["id"], "Theme not found", Some("NOT_FOUND"))]}),
                &field.selection,
            );
        };
        if theme.get("role").and_then(Value::as_str) == Some("LOCKED") {
            return selected_json(
                &json!({"theme": null, "userErrors": [theme_user_error(vec!["id"], "Locked themes cannot be modified.", Some("CANNOT_UPDATE_LOCKED_THEME"))]}),
                &field.selection,
            );
        }
        let input = match field.arguments.get("input") {
            Some(ResolvedValue::Object(input)) => input,
            _ => {
                return selected_json(&json!({"theme": theme, "userErrors": []}), &field.selection)
            }
        };
        if let Some(name) = resolved_string_field(input, "name") {
            if name.trim().is_empty() {
                return selected_json(
                    &json!({"theme": null, "userErrors": [theme_user_error(vec!["input", "name"], "Name can't be blank", Some("INVALID"))]}),
                    &field.selection,
                );
            }
            theme["name"] = json!(name);
        }
        self.store
            .staged
            .online_store_integrations
            .insert(id.clone(), theme.clone());
        staged_ids.push(id);
        selected_json(&json!({"theme": theme, "userErrors": []}), &field.selection)
    }

    pub(in crate::proxy) fn theme_delete(
        &mut self,
        field: &RootFieldSelection,
        staged_ids: &mut Vec<String>,
    ) -> Value {
        let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
        let Some(theme) = self
            .store
            .staged
            .online_store_integrations
            .get(&id)
            .cloned()
        else {
            return selected_json(
                &json!({"deletedThemeId": null, "userErrors": [theme_user_error(vec!["id"], "Theme not found", Some("NOT_FOUND"))]}),
                &field.selection,
            );
        };
        let main_count = self
            .store
            .staged
            .online_store_integrations
            .values()
            .filter(|record| {
                is_online_store_theme_record(record)
                    && record.get("role").and_then(Value::as_str) == Some("MAIN")
            })
            .count();
        if theme.get("role").and_then(Value::as_str) == Some("MAIN") && main_count <= 1 {
            return selected_json(
                &json!({"deletedThemeId": null, "userErrors": [theme_user_error(vec!["id"], "You can't delete your only published theme.", Some("INVALID"))]}),
                &field.selection,
            );
        }
        self.store.staged.online_store_integrations.remove(&id);
        staged_ids.push(id.clone());
        selected_json(
            &json!({"deletedThemeId": id, "userErrors": []}),
            &field.selection,
        )
    }

    pub(in crate::proxy) fn theme_files_upsert(&mut self, field: &RootFieldSelection) -> Value {
        let theme_id = resolved_string_arg(&field.arguments, "themeId").unwrap_or_default();
        let files = resolved_list_arg(&field.arguments, "files");
        if files.iter().any(|file| {
            theme_file_arg_string(file, "filename").as_deref() == Some("evil/path.liquid")
        }) {
            let payload = json!({"upsertedThemeFiles": [], "userErrors": [{"field": ["files", "0", "filename"], "message": "Filename is invalid", "code": "INVALID"}]});
            return selected_json(&payload, &field.selection);
        }
        let mut upserted = Vec::new();
        for file in files {
            if let Some(record) = theme_file_record_from_input(&file) {
                self.upsert_theme_file(&theme_id, record.clone());
                upserted.push(record);
            }
        }
        selected_json(
            &json!({"upsertedThemeFiles": upserted, "userErrors": []}),
            &field.selection,
        )
    }

    pub(in crate::proxy) fn theme_files_copy(&mut self, field: &RootFieldSelection) -> Value {
        let theme_id = resolved_string_arg(&field.arguments, "themeId").unwrap_or_default();
        let files = resolved_list_arg(&field.arguments, "files");
        let mut copied = Vec::new();
        let mut errors = Vec::new();
        for (index, file) in files.iter().enumerate() {
            let src = theme_file_arg_string(file, "srcFilename").unwrap_or_default();
            let dst = theme_file_arg_string(file, "dstFilename").unwrap_or_default();
            let Some(source_file) = self.find_theme_file(&theme_id, &src) else {
                errors.push(json!({"field": ["files", index.to_string(), "srcFilename"], "message": "File not found", "code": "NOT_FOUND"}));
                continue;
            };
            let content = source_file["body"]["content"].as_str().unwrap_or_default();
            copied.push(theme_file_record(&dst, content));
        }
        for file in copied.iter().cloned() {
            self.upsert_theme_file(&theme_id, file);
        }
        selected_json(
            &json!({"copiedThemeFiles": copied, "userErrors": errors}),
            &field.selection,
        )
    }

    pub(in crate::proxy) fn theme_files_delete(&mut self, field: &RootFieldSelection) -> Value {
        let theme_id = resolved_string_arg(&field.arguments, "themeId").unwrap_or_default();
        let files = resolved_string_list_arg(&field.arguments, "files");
        let required = ["config/settings_data.json", "config/settings_schema.json"];
        let errors = files
            .iter()
            .enumerate()
            .filter(|(_, filename)| required.contains(&filename.as_str()))
            .map(|(index, _)| {
                json!({"field": ["files", index.to_string()], "message": "File is required and can't be deleted", "code": "INVALID"})
            })
            .collect::<Vec<_>>();
        if !errors.is_empty() {
            return selected_json(
                &json!({"deletedThemeFiles": [], "userErrors": errors}),
                &field.selection,
            );
        }
        let mut deleted = Vec::new();
        if let Some(theme) = self
            .store
            .staged
            .online_store_integrations
            .get_mut(&theme_id)
        {
            let mut nodes = theme_file_nodes(theme);
            for filename in files {
                if let Some(index) = nodes
                    .iter()
                    .position(|file| file["filename"].as_str() == Some(filename.as_str()))
                {
                    nodes.remove(index);
                    deleted.push(json!({"filename": filename}));
                }
            }
            set_theme_file_nodes(theme, nodes);
        }
        selected_json(
            &json!({"deletedThemeFiles": deleted, "userErrors": []}),
            &field.selection,
        )
    }

    pub(in crate::proxy) fn upsert_theme_file(&mut self, theme_id: &str, file: Value) {
        let Some(theme) = self
            .store
            .staged
            .online_store_integrations
            .get_mut(theme_id)
        else {
            return;
        };
        let filename = file["filename"].as_str().unwrap_or_default().to_string();
        let mut nodes = theme_file_nodes(theme);
        if let Some(index) = nodes
            .iter()
            .position(|existing| existing["filename"].as_str() == Some(filename.as_str()))
        {
            nodes[index] = file;
        } else {
            nodes.push(file);
        }
        set_theme_file_nodes(theme, nodes);
    }

    pub(in crate::proxy) fn find_theme_file(
        &self,
        theme_id: &str,
        filename: &str,
    ) -> Option<Value> {
        self.store
            .staged
            .online_store_integrations
            .get(theme_id)
            .and_then(|theme| {
                theme_file_nodes(theme)
                    .into_iter()
                    .find(|file| file["filename"].as_str() == Some(filename))
            })
    }

    pub(in crate::proxy) fn web_pixel_create(
        &mut self,
        field: &RootFieldSelection,
        staged_ids: &mut Vec<String>,
    ) -> Value {
        if self
            .store
            .staged
            .online_store_integrations
            .values()
            .any(is_web_pixel_record)
        {
            return selected_json(
                &json!({"webPixel": null, "userErrors": [{"__typename": "WebPixelUserError", "code": "TAKEN", "field": null, "message": "Web pixel is taken."}]}),
                &field.selection,
            );
        }
        let id = self.next_online_store_id("WebPixel");
        let settings = field
            .arguments
            .get("webPixel")
            .and_then(|v| match v {
                ResolvedValue::Object(o) => o.get("settings"),
                _ => None,
            })
            .and_then(web_pixel_settings_from_resolved)
            .unwrap_or_else(|| json!({}));
        let record = json!({
            "__typename": "WebPixel",
            "id": id,
            "settings": settings,
            "status": "CONNECTED",
            "webhookEndpointAddress": null
        });
        self.store
            .staged
            .online_store_integrations
            .insert(id.clone(), record.clone());
        staged_ids.push(id);
        selected_json(
            &json!({"webPixel": record, "userErrors": []}),
            &field.selection,
        )
    }

    pub(in crate::proxy) fn web_pixel_update(
        &mut self,
        field: &RootFieldSelection,
        allow_missing_upsert: bool,
        staged_ids: &mut Vec<String>,
    ) -> Value {
        let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
        if !allow_missing_upsert
            && !self
                .store
                .staged
                .online_store_integrations
                .get(&id)
                .is_some_and(is_web_pixel_record)
        {
            return selected_json(
                &json!({"webPixel": null, "userErrors": [{"__typename": "WebPixelUserError", "code": "NOT_FOUND", "field": ["id"], "message": "Pixel not found"}]}),
                &field.selection,
            );
        }
        let input = match field.arguments.get("webPixel") {
            Some(ResolvedValue::Object(input)) => input,
            _ => {
                return selected_json(
                    &json!({"webPixel": null, "userErrors": []}),
                    &field.selection,
                )
            }
        };
        let settings_raw = resolved_string_field(input, "settings").unwrap_or_default();
        let Ok(settings) = serde_json::from_str::<Value>(&settings_raw) else {
            return selected_json(
                &json!({"webPixel": null, "userErrors": [{"__typename": "WebPixelUserError", "code": "INVALID_CONFIGURATION_JSON", "field": ["settings"], "message": "Settings must be valid JSON"}]}),
                &field.selection,
            );
        };
        let record = json!({
            "__typename": "WebPixel",
            "id": id,
            "settings": settings,
            "status": "CONNECTED",
            "webhookEndpointAddress": null
        });
        self.store
            .staged
            .online_store_integrations
            .insert(id.clone(), record.clone());
        staged_ids.push(id);
        selected_json(
            &json!({"webPixel": record, "userErrors": []}),
            &field.selection,
        )
    }

    pub(in crate::proxy) fn server_pixel_create(
        &mut self,
        field: &RootFieldSelection,
        staged_ids: &mut Vec<String>,
    ) -> Value {
        let id = self.next_online_store_id("ServerPixel");
        let record = json!({"__typename": "ServerPixel", "id": id, "status": "CONNECTED", "webhookEndpointAddress": null});
        self.store
            .staged
            .online_store_integrations
            .insert(id.clone(), record.clone());
        staged_ids.push(id);
        selected_json(
            &json!({"serverPixel": record, "userErrors": []}),
            &field.selection,
        )
    }

    pub(in crate::proxy) fn server_pixel_delete(
        &mut self,
        field: &RootFieldSelection,
        staged_ids: &mut Vec<String>,
    ) -> Value {
        let Some(id) = self
            .store
            .staged
            .online_store_integrations
            .iter()
            .find(|(_, record)| is_server_pixel_record(record))
            .map(|(id, _)| id.clone())
        else {
            return selected_payload_json(&field.selection, |selection| {
                match selection.name.as_str() {
                    "deletedServerPixelId" => Some(Value::Null),
                    "userErrors" => Some(selected_online_store_user_errors(
                        &[online_store_delete_user_error(
                            None,
                            "NOT_FOUND",
                            &[json!(["id"])],
                            "Server pixel not found",
                        )],
                        &selection.selection,
                    )),
                    _ => None,
                }
            });
        };

        self.store.staged.online_store_integrations.remove(&id);
        staged_ids.push(id.clone());
        selected_payload_json(&field.selection, |selection| {
            match selection.name.as_str() {
                "deletedServerPixelId" => Some(json!(id)),
                "userErrors" => Some(selected_online_store_user_errors(&[], &selection.selection)),
                _ => None,
            }
        })
    }

    pub(in crate::proxy) fn server_pixel_endpoint_update(
        &mut self,
        field: &RootFieldSelection,
        kind: &str,
    ) -> Value {
        let Some(id) = self
            .store
            .staged
            .online_store_integrations
            .iter()
            .find(|(_, v)| is_server_pixel_record(v))
            .map(|(id, _)| id.clone())
        else {
            return selected_json(
                &json!({"serverPixel": null, "userErrors": [{"__typename": "ServerPixelUserError", "code": "NOT_FOUND", "field": ["id"], "message": "Server pixel not found"}]}),
                &field.selection,
            );
        };
        let endpoint = if kind == "arn" {
            let arn = resolved_string_arg(&field.arguments, "arn").unwrap_or_default();
            if !arn.starts_with("arn:aws:events:") || arn.trim().is_empty() {
                return selected_json(
                    &json!({"serverPixel": null, "userErrors": [{"__typename": "ServerPixelUserError", "code": "INVALID_FIELD_ARGUMENTS", "field": ["arn"], "message": format!("Invalid ARN '{arn}'")}]}),
                    &field.selection,
                );
            }
            arn
        } else {
            let project =
                resolved_string_arg(&field.arguments, "pubSubProject").unwrap_or_default();
            let topic = resolved_string_arg(&field.arguments, "pubSubTopic").unwrap_or_default();
            let mut errors = Vec::new();
            if project.trim().is_empty() {
                errors.push(json!({"__typename": "ServerPixelUserError", "code": "INVALID_FIELD_ARGUMENTS", "field": ["pubSubProject"], "message": "pubSubProject can't be blank"}));
            }
            if topic.trim().is_empty() {
                errors.push(json!({"__typename": "ServerPixelUserError", "code": "INVALID_FIELD_ARGUMENTS", "field": ["pubSubTopic"], "message": "pubSubTopic can't be blank"}));
            }
            if !errors.is_empty() {
                return selected_json(
                    &json!({"serverPixel": null, "userErrors": errors}),
                    &field.selection,
                );
            }
            format!("{project}/{topic}")
        };
        let record = json!({"__typename": "ServerPixel", "id": id, "status": "CONNECTED", "webhookEndpointAddress": endpoint});
        self.store
            .staged
            .online_store_integrations
            .insert(id, record.clone());
        selected_json(
            &json!({"serverPixel": record, "userErrors": []}),
            &field.selection,
        )
    }

    pub(in crate::proxy) fn storefront_access_token_create(
        &mut self,
        field: &RootFieldSelection,
        request: &Request,
        staged_ids: &mut Vec<String>,
    ) -> Value {
        let title = field
            .arguments
            .get("input")
            .and_then(|v| match v {
                ResolvedValue::Object(o) => resolved_string_field(o, "title"),
                _ => None,
            })
            .unwrap_or_default();
        if title.trim().is_empty() {
            return selected_json(
                &json!({"storefrontAccessToken": null, "shop": {"id": "gid://shopify/Shop/92891250994"}, "userErrors": [{"code": "BLANK", "field": ["input", "title"], "message": "Title can't be blank"}]}),
                &field.selection,
            );
        }
        let token_count = self
            .store
            .staged
            .online_store_integrations
            .values()
            .filter(|record| is_storefront_access_token_record(record))
            .count();
        if token_count >= 100 {
            return selected_json(
                &json!({"storefrontAccessToken": null, "shop": {"id": "gid://shopify/Shop/92891250994"}, "userErrors": [{"code": "REACHED_LIMIT", "field": ["input"], "message": "apps.admin.graph_api_errors.storefront_access_token_create.reached_limit"}]}),
                &field.selection,
            );
        }
        let id = self.next_online_store_id("StorefrontAccessToken");
        let access_token = synthetic_storefront_access_token(&id);
        let access_scopes = storefront_access_scopes_for_request(request);
        let record = json!({
            "__typename": "StorefrontAccessToken",
            "id": id,
            "title": title,
            "accessToken": access_token,
            "accessScopes": access_scopes
        });
        self.store
            .staged
            .online_store_integrations
            .insert(id.clone(), record.clone());
        staged_ids.push(id);
        selected_json(
            &json!({"storefrontAccessToken": record, "shop": {"id": "gid://shopify/Shop/92891250994"}, "userErrors": []}),
            &field.selection,
        )
    }

    pub(in crate::proxy) fn draft_order_complete_local_data(
        &mut self,
        root_field: &str,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Option<Value> {
        let fields = root_fields(query, variables)?;
        let field = fields.iter().find(|field| field.name == root_field);

        match root_field {
            "draftOrderCreate" => {
                let field = field?;
                let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
                let has_b2b_purchasing_entity = resolved_object_field(&input, "purchasingEntity")
                    .and_then(|entity| resolved_object_field(&entity, "purchasingCompany"))
                    .is_some();
                match draft_order_create_input_email(field).as_deref() {
                    _ if has_b2b_purchasing_entity => {
                        Some(self.stage_completable_draft_order(field, "OPEN", "10.0", "CAD"))
                    }
                    Some("complete-readback@example.test") => {
                        Some(self.stage_completable_draft_order(field, "PAID", "25.0", "CAD"))
                    }
                    Some("gateway-complete@example.test") => {
                        self.store.staged.draft_order_complete_gateway_create_count += 1;
                        let status =
                            if self.store.staged.draft_order_complete_gateway_create_count == 1 {
                                "PENDING"
                            } else {
                                "OPEN"
                            };
                        Some(self.stage_completable_draft_order(field, status, "10.0", "CAD"))
                    }
                    _ => None,
                }
            }
            "draftOrderComplete" => {
                let field = field?;
                Some(data_response(
                    &field.response_key,
                    self.complete_staged_draft_order(field),
                ))
            }
            "order" => {
                let field = field?;
                let id = resolved_string_arg(&field.arguments, "id")?;
                let order = self.store.staged.orders.get(&id)?;
                Some(data_response(
                    &field.response_key,
                    selected_json(order, &field.selection),
                ))
            }
            "orders" => {
                let field = field?;
                let query_arg = resolved_string_arg(&field.arguments, "query").unwrap_or_default();
                let nodes = self
                    .store
                    .staged
                    .orders
                    .values()
                    .filter(|order| {
                        query_arg.is_empty()
                            || order["name"]
                                .as_str()
                                .is_some_and(|name| query_arg == format!("name:{name}"))
                    })
                    .map(|order| {
                        selected_json(order, &nested_selected_fields(&field.selection, &["nodes"]))
                    })
                    .collect::<Vec<_>>();
                Some(data_response(&field.response_key, order_connection(nodes)))
            }
            _ => None,
        }
    }

    pub(in crate::proxy) fn order_create_local_data(
        &mut self,
        request: &Request,
        root_field: &str,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Option<Value> {
        let fields = root_fields(query, variables)?;
        if !fields.iter().all(|field| {
            matches!(
                field.name.as_str(),
                "orderCreate" | "order" | "orders" | "ordersCount"
            )
        }) {
            return None;
        }
        let all_reads = fields
            .iter()
            .all(|field| matches!(field.name.as_str(), "order" | "orders" | "ordersCount"));
        if all_reads {
            let staged_order_read = fields.iter().any(|field| match field.name.as_str() {
                "order" => resolved_string_arg(&field.arguments, "id").is_some_and(|id| {
                    self.store.staged.orders.contains_key(&id)
                        || self.store.staged.deleted_order_ids.contains(&id)
                }),
                "orders" | "ordersCount" => {
                    !self.store.staged.orders.is_empty()
                        || !self.store.staged.deleted_order_ids.is_empty()
                }
                _ => false,
            });
            if !staged_order_read {
                return None;
            }
        }
        if !fields.iter().any(|field| field.name == root_field) {
            return None;
        }

        let mut data = serde_json::Map::new();
        for field in fields {
            let value = match field.name.as_str() {
                "orderCreate" => self.stage_order_create(request, query, variables, &field),
                "order" => {
                    let id = resolved_string_arg(&field.arguments, "id")?;
                    let order = self
                        .store
                        .staged
                        .orders
                        .get(&id)
                        .cloned()
                        .unwrap_or(Value::Null);
                    nullable_selected_json(&order, &field.selection)
                }
                "orders" => self.staged_orders_connection(&field),
                "ordersCount" => selected_json(
                    &json!({
                        "count": self.store.staged.orders.len(),
                        "precision": "EXACT"
                    }),
                    &field.selection,
                ),
                _ => return None,
            };
            data.insert(field.response_key, value);
        }
        Some(json!({ "data": Value::Object(data) }))
    }

    fn staged_orders_connection(&self, field: &RootFieldSelection) -> Value {
        let query_arg = resolved_string_arg(&field.arguments, "query").unwrap_or_default();
        let node_selection = nested_selected_fields(&field.selection, &["nodes"]);
        let nodes = self
            .store
            .staged
            .orders
            .values()
            .filter(|order| {
                if query_arg.is_empty() {
                    return true;
                }
                order["name"]
                    .as_str()
                    .is_some_and(|name| query_arg == format!("name:{name}"))
                    || order["email"]
                        .as_str()
                        .is_some_and(|email| query_arg == format!("email:{email}"))
            })
            .map(|order| selected_json(order, &node_selection))
            .collect::<Vec<_>>();
        selected_json(&order_connection(nodes), &field.selection)
    }

    fn stage_order_create(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        field: &RootFieldSelection,
    ) -> Value {
        let order_input = resolved_object_field(&field.arguments, "order").unwrap_or_default();
        if let Some(error) = order_create_validation_error(&order_input) {
            return selected_json(
                &json!({ "order": Value::Null, "userErrors": [error] }),
                &field.selection,
            );
        }
        if order_create_inventory_behaviour(field) != "BYPASS" {
            for line_item in resolved_object_list_field(&order_input, "lineItems") {
                if let Some(inventory_item_id) = order_line_inventory_item_id(&line_item) {
                    let quantity = resolved_i64_field(&line_item, "quantity").unwrap_or(1);
                    self.decrement_inventory_item_available(&inventory_item_id, quantity);
                }
            }
        }

        let order_id = format!("gid://shopify/Order/{}", self.store.staged.next_order_id);
        self.store.staged.next_order_id += 1;
        let order = self.build_order_create_record(&order_id, &order_input);
        self.store
            .staged
            .orders
            .insert(order_id.clone(), order.clone());
        if let Some(customer_id) = resolved_string_field(&order_input, "customerId") {
            self.store
                .staged
                .customer_orders
                .entry(customer_id)
                .or_default()
                .push(order.clone());
        }
        self.record_orders_local_log_entry(OrdersLocalLogEntry {
            request,
            query,
            variables,
            root_field: "orderCreate",
            staged_resource_ids: vec![order_id],
            outcome: OrdersLocalLogOutcome {
                status: "staged",
                notes: "Locally staged orderCreate in shopify-draft-proxy.",
            },
        });
        selected_json(
            &json!({ "order": order, "userErrors": [] }),
            &field.selection,
        )
    }

    fn staged_order_id_for_fulfillment_order(&self, fulfillment_order_id: &str) -> Option<String> {
        self.store
            .staged
            .orders
            .iter()
            .find_map(|(order_id, order)| {
                order["fulfillmentOrders"]["nodes"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .any(|node| node["id"].as_str() == Some(fulfillment_order_id))
                    .then(|| order_id.clone())
            })
    }

    fn staged_fulfillment_order(&self, fulfillment_order_id: &str) -> Option<Value> {
        self.store.staged.orders.values().find_map(|order| {
            order["fulfillmentOrders"]["nodes"]
                .as_array()
                .into_iter()
                .flatten()
                .find(|node| node["id"].as_str() == Some(fulfillment_order_id))
                .cloned()
        })
    }

    fn staged_fulfillment_orders(&self) -> Vec<Value> {
        self.store
            .staged
            .orders
            .values()
            .flat_map(|order| {
                order["fulfillmentOrders"]["nodes"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default()
            })
            .collect()
    }

    pub(in crate::proxy) fn fulfillment_order_local_query_data(
        &mut self,
        _root_field: &str,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Option<Value> {
        if self.store.staged.orders.is_empty() {
            return None;
        }
        let fields = root_fields(query, variables)?;
        if !fields.iter().all(|field| {
            matches!(
                field.name.as_str(),
                "order" | "fulfillmentOrder" | "fulfillmentOrders" | "assignedFulfillmentOrders"
            )
        }) {
            return None;
        }
        if !fields.iter().any(|field| {
            matches!(
                field.name.as_str(),
                "fulfillmentOrder" | "fulfillmentOrders" | "assignedFulfillmentOrders"
            )
        }) {
            return None;
        }

        for order in self.store.staged.orders.values_mut() {
            normalize_order_fulfillment_orders(order);
        }

        let mut data = serde_json::Map::new();
        for field in fields {
            let value = match field.name.as_str() {
                "order" => {
                    let id = resolved_string_arg(&field.arguments, "id")?;
                    let order = self
                        .store
                        .staged
                        .orders
                        .get(&id)
                        .cloned()
                        .unwrap_or(Value::Null);
                    nullable_selected_json(&order, &field.selection)
                }
                "fulfillmentOrder" => {
                    let id = resolved_string_arg(&field.arguments, "id")?;
                    let fulfillment_order =
                        self.staged_fulfillment_order(&id).unwrap_or(Value::Null);
                    nullable_selected_json(&fulfillment_order, &field.selection)
                }
                "fulfillmentOrders" | "assignedFulfillmentOrders" => {
                    let nodes = self
                        .staged_fulfillment_orders()
                        .iter()
                        .map(|order| {
                            selected_json(
                                order,
                                &nested_selected_fields(&field.selection, &["nodes"]),
                            )
                        })
                        .collect::<Vec<_>>();
                    selected_json(&order_connection(nodes), &field.selection)
                }
                _ => return None,
            };
            data.insert(field.response_key, value);
        }
        Some(json!({ "data": Value::Object(data) }))
    }

    pub(in crate::proxy) fn fulfillment_order_empty_read_response(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Response {
        let Some(fields) = root_fields(query, variables) else {
            return json_error(400, "Could not parse GraphQL operation");
        };
        let mut data = serde_json::Map::new();
        for field in fields {
            let value = match field.name.as_str() {
                "fulfillmentOrder" => nullable_selected_json(&Value::Null, &field.selection),
                "fulfillmentOrders" | "assignedFulfillmentOrders" => {
                    selected_json(&order_connection(Vec::new()), &field.selection)
                }
                _ => continue,
            };
            data.insert(field.response_key, value);
        }
        ok_json(json!({ "data": Value::Object(data) }))
    }

    pub(in crate::proxy) fn fulfillment_order_local_mutation_data(
        &mut self,
        request: &Request,
        root_field: &str,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Option<Value> {
        let field = root_fields(query, variables)?
            .into_iter()
            .find(|field| field.name == root_field)?;
        let payload = match root_field {
            "fulfillmentOrderSubmitFulfillmentRequest" => {
                self.stage_fulfillment_order_submit_request(request, query, variables, &field)
            }
            "fulfillmentOrderAcceptFulfillmentRequest"
            | "fulfillmentOrderRejectFulfillmentRequest"
            | "fulfillmentOrderSubmitCancellationRequest"
            | "fulfillmentOrderAcceptCancellationRequest"
            | "fulfillmentOrderRejectCancellationRequest" => {
                self.stage_fulfillment_order_request_transition(request, query, variables, &field)
            }
            "fulfillmentOrderSplit" => {
                self.stage_fulfillment_order_split(request, query, variables, &field)
            }
            "fulfillmentOrderMerge" => {
                self.stage_fulfillment_order_merge(request, query, variables, &field)
            }
            _ => return None,
        };
        Some(data_response(&field.response_key, payload))
    }

    fn fulfillment_order_not_found_payload(
        &self,
        root_field: &str,
        selection: &[SelectedField],
    ) -> Value {
        let errors = vec![fulfillment_order_user_error(
            Value::Null,
            "Fulfillment order does not exist.",
            Some("FULFILLMENT_ORDER_NOT_FOUND"),
        )];
        match root_field {
            "fulfillmentOrderSplit" => {
                fulfillment_order_split_payload_json(Value::Null, selection, errors)
            }
            "fulfillmentOrderMerge" => {
                fulfillment_order_merge_payload_json(Value::Null, selection, errors)
            }
            "fulfillmentOrderSubmitFulfillmentRequest" => fulfillment_order_request_payload_json(
                root_field,
                Value::Null,
                Value::Null,
                Value::Null,
                Value::Null,
                selection,
                errors,
            ),
            _ => fulfillment_order_payload_json(Value::Null, selection, errors),
        }
    }

    fn locate_fulfillment_order_mut<'a>(
        order: &'a mut Value,
        fulfillment_order_id: &str,
    ) -> Option<&'a mut Value> {
        fulfillment_order_nodes_mut(order)?
            .iter_mut()
            .find(|node| node["id"].as_str() == Some(fulfillment_order_id))
    }

    fn stage_fulfillment_order_submit_request(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        field: &RootFieldSelection,
    ) -> Value {
        let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
        let Some(order_id) = self.staged_order_id_for_fulfillment_order(&id) else {
            let Some(order_id) = self.hydrate_order_for_fulfillment_order(&id, request) else {
                return self.fulfillment_order_not_found_payload(&field.name, &field.selection);
            };
            return self.stage_fulfillment_order_submit_request_for_order_id(
                request, query, variables, field, order_id, id,
            );
        };
        self.stage_fulfillment_order_submit_request_for_order_id(
            request, query, variables, field, order_id, id,
        )
    }

    fn stage_fulfillment_order_submit_request_for_order_id(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        field: &RootFieldSelection,
        order_id: String,
        id: String,
    ) -> Value {
        let requested_lines =
            resolved_object_list_field(&field.arguments, "fulfillmentOrderLineItems");
        let message = resolved_string_arg(&field.arguments, "message");
        let notify_customer =
            resolved_bool_field(&field.arguments, "notifyCustomer").unwrap_or(false);

        let Some(mut order) = self.store.staged.orders.get(&order_id).cloned() else {
            return self.fulfillment_order_not_found_payload(&field.name, &field.selection);
        };
        normalize_order_fulfillment_orders(&mut order);
        let Some(original_index) = order["fulfillmentOrders"]["nodes"]
            .as_array()
            .into_iter()
            .flatten()
            .position(|node| node["id"].as_str() == Some(id.as_str()))
        else {
            return self.fulfillment_order_not_found_payload(&field.name, &field.selection);
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
                        resolved_i64_field(line, "quantity").unwrap_or(0).max(0),
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
        normalize_fulfillment_order_record(&mut original);
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

        self.record_orders_local_log_entry(OrdersLocalLogEntry {
            request,
            query,
            variables,
            root_field: "fulfillmentOrderSubmitFulfillmentRequest",
            staged_resource_ids: vec![order_id, id],
            outcome: OrdersLocalLogOutcome {
                status: "staged",
                notes: "Locally staged fulfillmentOrderSubmitFulfillmentRequest in shopify-draft-proxy.",
            },
        });

        fulfillment_order_request_payload_json(
            &field.name,
            original.clone(),
            original.clone(),
            original,
            unsubmitted,
            &field.selection,
            Vec::new(),
        )
    }

    fn stage_fulfillment_order_request_transition(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        field: &RootFieldSelection,
    ) -> Value {
        let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
        let Some(order_id) = self.staged_order_id_for_fulfillment_order(&id) else {
            let Some(order_id) = self.hydrate_order_for_fulfillment_order(&id, request) else {
                return self.fulfillment_order_not_found_payload(&field.name, &field.selection);
            };
            return self.stage_fulfillment_order_request_transition_for_order_id(
                request, query, variables, field, order_id, id,
            );
        };
        self.stage_fulfillment_order_request_transition_for_order_id(
            request, query, variables, field, order_id, id,
        )
    }

    fn stage_fulfillment_order_request_transition_for_order_id(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        field: &RootFieldSelection,
        order_id: String,
        id: String,
    ) -> Value {
        let Some(mut order) = self.store.staged.orders.get(&order_id).cloned() else {
            return self.fulfillment_order_not_found_payload(&field.name, &field.selection);
        };
        normalize_order_fulfillment_orders(&mut order);
        let Some(fulfillment_order) = Self::locate_fulfillment_order_mut(&mut order, &id) else {
            return self.fulfillment_order_not_found_payload(&field.name, &field.selection);
        };
        match field.name.as_str() {
            "fulfillmentOrderAcceptFulfillmentRequest" => {
                fulfillment_order["status"] = json!("IN_PROGRESS");
                fulfillment_order["requestStatus"] = json!("ACCEPTED");
                if let Some(estimated) = resolved_string_arg(&field.arguments, "estimatedShippedAt")
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
                    "message": resolved_string_arg(&field.arguments, "message").unwrap_or_default(),
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
        let changed = fulfillment_order.clone();
        self.store.staged.orders.insert(order_id.clone(), order);
        self.record_orders_local_log_entry(OrdersLocalLogEntry {
            request,
            query,
            variables,
            root_field: &field.name,
            staged_resource_ids: vec![order_id, id],
            outcome: OrdersLocalLogOutcome {
                status: "staged",
                notes:
                    "Locally staged fulfillment-order request transition in shopify-draft-proxy.",
            },
        });
        fulfillment_order_payload_json(changed, &field.selection, Vec::new())
    }

    fn split_validation_error(
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
        fulfillment_order_user_error(field, message, Some(code))
    }

    fn stage_fulfillment_order_split(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        field: &RootFieldSelection,
    ) -> Value {
        let split_inputs = resolved_object_list_field(&field.arguments, "fulfillmentOrderSplits");
        let mut planned = Vec::new();
        for (input_index, input) in split_inputs.iter().enumerate() {
            let line_inputs = resolved_object_list_field(input, "fulfillmentOrderLineItems");
            if line_inputs.is_empty() {
                return fulfillment_order_split_payload_json(
                    Value::Null,
                    &field.selection,
                    vec![self.split_validation_error(
                        input_index,
                        None,
                        "There must be at least one item selected in this fulfillment to split it.",
                        "NO_LINE_ITEMS_PROVIDED_TO_SPLIT",
                    )],
                );
            }
            for (line_index, line) in line_inputs.iter().enumerate() {
                if resolved_i64_field(line, "quantity").unwrap_or(0) <= 0 {
                    return fulfillment_order_split_payload_json(
                        Value::Null,
                        &field.selection,
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
            let Some(order_id) = self
                .staged_order_id_for_fulfillment_order(&fulfillment_order_id)
                .or_else(|| {
                    self.hydrate_order_for_fulfillment_order_with_query(
                        &fulfillment_order_id,
                        request,
                        ORDERS_FULFILLMENT_ORDER_COMPACT_HYDRATE_QUERY,
                    )
                })
            else {
                return self.fulfillment_order_not_found_payload(&field.name, &field.selection);
            };
            planned.push((order_id, fulfillment_order_id, line_inputs));
        }

        let mut split_results = Vec::new();
        let mut staged_ids = Vec::new();
        for (order_id, fulfillment_order_id, line_inputs) in planned {
            let Some(mut order) = self.store.staged.orders.get(&order_id).cloned() else {
                return self.fulfillment_order_not_found_payload(&field.name, &field.selection);
            };
            normalize_order_fulfillment_orders(&mut order);
            let Some(nodes) = fulfillment_order_nodes_mut(&mut order) else {
                return self.fulfillment_order_not_found_payload(&field.name, &field.selection);
            };
            let Some(index) = nodes
                .iter()
                .position(|node| node["id"].as_str() == Some(fulfillment_order_id.as_str()))
            else {
                return self.fulfillment_order_not_found_payload(&field.name, &field.selection);
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
                    .and_then(|input| resolved_i64_field(input, "quantity"))
                    .unwrap_or(0);
                let current = line_item_remaining_quantity(&line);
                if split_quantity > current {
                    return fulfillment_order_split_payload_json(
                        Value::Null,
                        &field.selection,
                        vec![fulfillment_order_user_error(
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
            original["lineItems"] = order_connection(updated_lines);
            original["updatedAt"] = json!("2026-05-11T10:00:00Z");
            set_fulfillment_order_status_from_lines(&mut original);

            let mut remaining = original.clone();
            let remaining_id = self.next_proxy_synthetic_gid("FulfillmentOrder");
            remaining["id"] = json!(remaining_id.clone());
            remaining["status"] = json!("OPEN");
            remaining["requestStatus"] = json!("UNSUBMITTED");
            remaining["lineItems"] = order_connection(remaining_lines);
            remaining["updatedAt"] = json!("2026-05-11T10:00:00Z");
            normalize_fulfillment_order_record(&mut remaining);
            set_fulfillment_order_status_from_lines(&mut remaining);

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

        self.record_orders_local_log_entry(OrdersLocalLogEntry {
            request,
            query,
            variables,
            root_field: "fulfillmentOrderSplit",
            staged_resource_ids: staged_ids,
            outcome: OrdersLocalLogOutcome {
                status: "staged",
                notes: "Locally staged fulfillmentOrderSplit in shopify-draft-proxy.",
            },
        });
        fulfillment_order_split_payload_json(
            Value::Array(split_results),
            &field.selection,
            Vec::new(),
        )
    }

    fn merge_requested_lines(
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
            let quantity = resolved_i64_field(request, "quantity").unwrap_or(0);
            if quantity <= 0 {
                return Err(fulfillment_order_user_error(
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
                return Err(fulfillment_order_user_error(
                    Value::Null,
                    "Fulfillment order line item does not exist.",
                    None,
                ));
            };
            if quantity > line_item_remaining_quantity(line) {
                return Err(fulfillment_order_user_error(
                    Value::Null,
                    "Invalid fulfillment order line item quantity requested.",
                    None,
                ));
            }
            result.push(fulfillment_order_line_with_quantity(line, quantity));
        }
        Ok(result)
    }

    fn stage_fulfillment_order_merge(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        field: &RootFieldSelection,
    ) -> Value {
        let merge_inputs =
            resolved_object_list_field(&field.arguments, "fulfillmentOrderMergeInputs");
        let mut merge_results = Vec::new();
        let mut staged_ids = Vec::new();

        for (input_index, merge_input) in merge_inputs.into_iter().enumerate() {
            let intents = resolved_object_list_field(&merge_input, "mergeIntents");
            let Some(first_intent) = intents.first() else {
                continue;
            };
            let target_id =
                resolved_string_field(first_intent, "fulfillmentOrderId").unwrap_or_default();
            let Some(order_id) = self
                .staged_order_id_for_fulfillment_order(&target_id)
                .or_else(|| self.hydrate_order_for_fulfillment_order(&target_id, request))
            else {
                return self.fulfillment_order_not_found_payload(&field.name, &field.selection);
            };
            let Some(mut order) = self.store.staged.orders.get(&order_id).cloned() else {
                return self.fulfillment_order_not_found_payload(&field.name, &field.selection);
            };
            normalize_order_fulfillment_orders(&mut order);
            let Some(nodes) = fulfillment_order_nodes_mut(&mut order) else {
                return self.fulfillment_order_not_found_payload(&field.name, &field.selection);
            };
            let Some(target_index) = nodes
                .iter()
                .position(|node| node["id"].as_str() == Some(target_id.as_str()))
            else {
                return self.fulfillment_order_not_found_payload(&field.name, &field.selection);
            };
            if nodes[target_index]["status"].as_str() != Some("OPEN") {
                return fulfillment_order_merge_payload_json(
                    Value::Null,
                    &field.selection,
                    vec![fulfillment_order_user_error(
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
                        return fulfillment_order_merge_payload_json(
                            Value::Null,
                            &field.selection,
                            vec![error],
                        );
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
                    return self.fulfillment_order_not_found_payload(&field.name, &field.selection);
                };
                let source = nodes[source_index].clone();
                if source["status"].as_str() != Some("OPEN") {
                    return fulfillment_order_merge_payload_json(
                        Value::Null,
                        &field.selection,
                        vec![fulfillment_order_user_error(
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
                        return fulfillment_order_merge_payload_json(
                            Value::Null,
                            &field.selection,
                            vec![error],
                        );
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
            target["lineItems"] = order_connection(merged_lines);
            target["updatedAt"] = json!("2026-05-11T10:00:00Z");
            set_fulfillment_order_status_from_lines(&mut target);
            nodes[target_index] = target.clone();
            for remove_id in &remove_ids {
                if let Some(node) = nodes
                    .iter_mut()
                    .find(|node| node["id"].as_str() == Some(remove_id.as_str()))
                {
                    node["status"] = json!("CLOSED");
                    node["updatedAt"] = json!("2026-05-11T10:00:00Z");
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

        self.record_orders_local_log_entry(OrdersLocalLogEntry {
            request,
            query,
            variables,
            root_field: "fulfillmentOrderMerge",
            staged_resource_ids: staged_ids,
            outcome: OrdersLocalLogOutcome {
                status: "staged",
                notes: "Locally staged fulfillmentOrderMerge in shopify-draft-proxy.",
            },
        });
        fulfillment_order_merge_payload_json(
            Value::Array(merge_results),
            &field.selection,
            Vec::new(),
        )
    }

    fn staged_order_id_for_fulfillment(&self, fulfillment_id: &str) -> Option<String> {
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

    fn stage_hydrated_order(&mut self, mut order: Value) -> Option<String> {
        normalize_hydrated_order(&mut order);
        let id = order.get("id").and_then(Value::as_str)?.to_string();
        self.store.staged.orders.insert(id.clone(), order);
        Some(id)
    }

    fn merge_hydrated_fulfillment_order_into_order(
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
                    "id": format!(
                        "gid://shopify/Order/observed-fulfillment-order-{}",
                        resource_id_tail(&fulfillment_order_id)
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
        normalize_fulfillment_order_record(&mut fulfillment_order_record);

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

    fn stage_observed_fulfillment_orders_from_value(&mut self, value: &Value) {
        match value {
            Value::Object(object) => {
                if object
                    .get("id")
                    .and_then(Value::as_str)
                    .is_some_and(|id| id.starts_with("gid://shopify/Order/"))
                    && object
                        .get("fulfillmentOrders")
                        .and_then(|connection| connection.get("nodes"))
                        .and_then(Value::as_array)
                        .is_some()
                {
                    let _ = self.stage_hydrated_order(value.clone());
                }
                for child in object.values() {
                    self.stage_observed_fulfillment_orders_from_value(child);
                }
            }
            Value::Array(values) => {
                for child in values {
                    self.stage_observed_fulfillment_orders_from_value(child);
                }
            }
            _ => {}
        }
    }

    pub(in crate::proxy) fn observe_order_fulfillment_passthrough_response(
        &mut self,
        response: &Response,
    ) {
        if response.status >= 400 {
            return;
        }
        if let Some(data) = response.body.get("data") {
            self.stage_observed_fulfillment_orders_from_value(data);
        }
    }

    pub(in crate::proxy) fn order_fulfillment_live_hybrid_read_response(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Option<Response> {
        if self.config.read_mode == ReadMode::Snapshot {
            return None;
        }
        let fields = root_fields(query, variables)?;
        if !fields.iter().all(|field| field.name == "order") {
            return None;
        }
        if !fields.iter().any(|field| {
            nested_selected_fields(&field.selection, &["fulfillmentOrders", "nodes"])
                .iter()
                .any(|selected| selected.name == "id")
        }) {
            return None;
        }

        let response = (self.upstream_transport)(request.clone());
        self.observe_order_fulfillment_passthrough_response(&response);
        Some(response)
    }

    fn hydrate_order_for_fulfillment_order(
        &mut self,
        fulfillment_order_id: &str,
        request: &Request,
    ) -> Option<String> {
        self.hydrate_order_for_fulfillment_order_with_query(
            fulfillment_order_id,
            request,
            ORDERS_FULFILLMENT_ORDER_HYDRATE_QUERY,
        )
    }

    fn hydrate_order_for_fulfillment_order_with_query(
        &mut self,
        fulfillment_order_id: &str,
        request: &Request,
        hydrate_query: &str,
    ) -> Option<String> {
        if self.config.read_mode == ReadMode::Snapshot {
            return None;
        }
        let response = (self.upstream_transport)(Request {
            method: "POST".to_string(),
            path: request.path.clone(),
            headers: request.headers.clone(),
            body: json!({
                "query": hydrate_query,
                "variables": { "id": fulfillment_order_id }
            })
            .to_string(),
        });
        if !(200..300).contains(&response.status) {
            return None;
        }
        let fulfillment_order = response.body["data"]["fulfillmentOrder"].clone();
        if fulfillment_order.is_object() {
            return self.merge_hydrated_fulfillment_order_into_order(fulfillment_order);
        }
        let order = response.body["data"]["fulfillmentOrder"]["order"].clone();
        if !order.is_object() {
            return None;
        }
        self.stage_hydrated_order(order)
    }

    fn hydrate_order_for_fulfillment(
        &mut self,
        fulfillment_id: &str,
        request: &Request,
    ) -> Option<String> {
        if self.config.read_mode == ReadMode::Snapshot {
            return None;
        }
        let response = (self.upstream_transport)(Request {
            method: "POST".to_string(),
            path: request.path.clone(),
            headers: request.headers.clone(),
            body: json!({
                "query": ORDERS_FULFILLMENT_HYDRATE_QUERY,
                "variables": { "id": fulfillment_id }
            })
            .to_string(),
        });
        if !(200..300).contains(&response.status) {
            return None;
        }
        let fulfillment = response.body["data"]["fulfillment"].clone();
        let mut order = fulfillment["order"].clone();
        if !order.is_object() {
            return None;
        }
        if !order["fulfillments"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|record| record["id"].as_str() == Some(fulfillment_id))
            && fulfillment.is_object()
        {
            let mut fulfillment_record = fulfillment.clone();
            if let Some(object) = fulfillment_record.as_object_mut() {
                object.remove("order");
            }
            normalize_hydrated_order(&mut order);
            if let Some(fulfillments) = order_fulfillments_mut(&mut order) {
                fulfillments.push(fulfillment_record);
            }
        }
        self.stage_hydrated_order(order)
    }

    fn staged_fulfillment_payload(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        field: &RootFieldSelection,
    ) -> Value {
        let Some(fulfillment_input) = resolved_object_field(&field.arguments, "fulfillment") else {
            return selected_json(
                &json!({
                    "fulfillment": Value::Null,
                    "userErrors": [orders_error(&["fulfillment"], "Fulfillment is required", "INVALID")]
                }),
                &field.selection,
            );
        };
        let groups = resolved_object_list_field(&fulfillment_input, "lineItemsByFulfillmentOrder");
        let Some(first_group) = groups.first() else {
            return selected_json(
                &json!({
                    "fulfillment": Value::Null,
                    "userErrors": [orders_error(&["fulfillment"], "Line items by fulfillment order must be specified", "INVALID")]
                }),
                &field.selection,
            );
        };
        let Some(fulfillment_order_id) = resolved_string_field(first_group, "fulfillmentOrderId")
        else {
            return selected_json(
                &json!({
                    "fulfillment": Value::Null,
                    "userErrors": [orders_error(&["fulfillment"], "Fulfillment order must be specified", "INVALID")]
                }),
                &field.selection,
            );
        };
        let Some(order_id) = self
            .staged_order_id_for_fulfillment_order(&fulfillment_order_id)
            .or_else(|| self.hydrate_order_for_fulfillment_order(&fulfillment_order_id, request))
        else {
            return selected_json(
                &json!({
                    "fulfillment": Value::Null,
                    "userErrors": [orders_error(&["fulfillment"], "Fulfillment order could not be found.", "NOT_FOUND")]
                }),
                &field.selection,
            );
        };
        let Some(order_before) = self.store.staged.orders.get(&order_id).cloned() else {
            return selected_json(
                &json!({
                    "fulfillment": Value::Null,
                    "userErrors": [orders_error(&["fulfillment"], "Fulfillment order could not be found.", "NOT_FOUND")]
                }),
                &field.selection,
            );
        };
        if let Some(error) = fulfillment_create_precondition_error(&order_before, &groups) {
            return selected_json(
                &json!({
                    "fulfillment": Value::Null,
                    "userErrors": [error]
                }),
                &field.selection,
            );
        }

        let tracking_info = resolved_object_field(&fulfillment_input, "trackingInfo")
            .map(|tracking| fulfillment_tracking_info(&tracking))
            .unwrap_or_default();
        let fulfillment_id = self.next_proxy_synthetic_gid("Fulfillment");
        let fulfillment_line_items = groups
            .iter()
            .flat_map(|group| fulfillment_group_line_items(&order_before, group))
            .collect::<Vec<_>>();
        let fulfillment = json!({
            "id": fulfillment_id,
            "status": "SUCCESS",
            "displayStatus": "FULFILLED",
            "createdAt": "2024-01-01T00:00:00.000Z",
            "updatedAt": "2024-01-01T00:00:00.000Z",
            "trackingInfo": tracking_info,
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
                                let quantity = resolved_i64_field(requested, "quantity")
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
                        let remaining_total = line_nodes
                            .iter()
                            .filter_map(|line| line["remainingQuantity"].as_i64())
                            .sum::<i64>();
                        fulfillment_order["status"] = json!(if remaining_total == 0 {
                            "CLOSED"
                        } else {
                            "OPEN"
                        });
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
        self.record_orders_local_log_entry(OrdersLocalLogEntry {
            request,
            query,
            variables,
            root_field: "fulfillmentCreate",
            staged_resource_ids: vec![order_id, fulfillment_id],
            outcome: OrdersLocalLogOutcome {
                status: "staged",
                notes: "Locally staged fulfillmentCreate in shopify-draft-proxy.",
            },
        });

        selected_json(
            &json!({ "fulfillment": fulfillment, "userErrors": [] }),
            &field.selection,
        )
    }

    fn update_staged_fulfillment_tracking_payload(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        field: &RootFieldSelection,
    ) -> Option<Value> {
        let fulfillment_id = resolved_string_arg(&field.arguments, "fulfillmentId")?;
        let order_id = self
            .staged_order_id_for_fulfillment(&fulfillment_id)
            .or_else(|| self.hydrate_order_for_fulfillment(&fulfillment_id, request))?;
        let tracking_input = resolved_object_field(&field.arguments, "trackingInfoInput")
            .or_else(|| resolved_object_field(&field.arguments, "trackingInfo"))
            .unwrap_or_default();
        let tracking_info = fulfillment_tracking_info(&tracking_input);
        let mut order = self.store.staged.orders.get(&order_id)?.clone();
        let fulfillment = order_fulfillments_mut(&mut order)?
            .iter_mut()
            .find(|fulfillment| fulfillment["id"].as_str() == Some(fulfillment_id.as_str()))?;
        if fulfillment_status_is(fulfillment, "CANCELLED") {
            return Some(selected_json(
                &json!({
                    "fulfillment": Value::Null,
                    "userErrors": [orders_error(&["fulfillmentId"], "fulfillment_is_cancelled", "INVALID")]
                }),
                &field.selection,
            ));
        }
        fulfillment["trackingInfo"] = json!(tracking_info);
        fulfillment["status"] = json!("SUCCESS");
        fulfillment["displayStatus"] = json!("FULFILLED");
        fulfillment["updatedAt"] = json!("2024-01-01T00:00:01.000Z");
        let updated = fulfillment.clone();
        self.store
            .staged
            .orders
            .insert(order_id.clone(), order.clone());
        self.record_orders_local_log_entry(OrdersLocalLogEntry {
            request,
            query,
            variables,
            root_field: "fulfillmentTrackingInfoUpdate",
            staged_resource_ids: vec![order_id, fulfillment_id],
            outcome: OrdersLocalLogOutcome {
                status: "staged",
                notes: "Locally staged fulfillmentTrackingInfoUpdate in shopify-draft-proxy.",
            },
        });
        Some(selected_json(
            &json!({ "fulfillment": updated, "userErrors": [] }),
            &field.selection,
        ))
    }

    fn cancel_staged_fulfillment_payload(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        field: &RootFieldSelection,
    ) -> Option<Value> {
        let fulfillment_id = resolved_string_arg(&field.arguments, "id")?;
        let order_id = self
            .staged_order_id_for_fulfillment(&fulfillment_id)
            .or_else(|| self.hydrate_order_for_fulfillment(&fulfillment_id, request))?;
        let mut order = self.store.staged.orders.get(&order_id)?.clone();
        let fulfillment = order_fulfillments_mut(&mut order)?
            .iter_mut()
            .find(|fulfillment| fulfillment["id"].as_str() == Some(fulfillment_id.as_str()))?;
        if fulfillment_status_is(fulfillment, "CANCELLED") {
            return Some(selected_json(
                &json!({
                    "fulfillment": Value::Null,
                    "userErrors": [orders_error(&["id"], "fulfillment_cannot_be_cancelled", "INVALID")]
                }),
                &field.selection,
            ));
        }
        if fulfillment_display_status_is(fulfillment, "DELIVERED") {
            return Some(selected_json(
                &json!({
                    "fulfillment": Value::Null,
                    "userErrors": [orders_error(&["id"], "fulfillment_already_delivered", "INVALID")]
                }),
                &field.selection,
            ));
        }
        fulfillment["status"] = json!("CANCELLED");
        fulfillment["displayStatus"] = json!("CANCELED");
        fulfillment["updatedAt"] = json!("2024-01-01T00:00:02.000Z");
        let cancelled = fulfillment.clone();
        self.store
            .staged
            .orders
            .insert(order_id.clone(), order.clone());
        self.record_orders_local_log_entry(OrdersLocalLogEntry {
            request,
            query,
            variables,
            root_field: "fulfillmentCancel",
            staged_resource_ids: vec![order_id, fulfillment_id],
            outcome: OrdersLocalLogOutcome {
                status: "staged",
                notes: "Locally staged fulfillmentCancel in shopify-draft-proxy.",
            },
        });
        Some(selected_json(
            &json!({ "fulfillment": cancelled, "userErrors": [] }),
            &field.selection,
        ))
    }

    fn build_order_create_record(
        &self,
        order_id: &str,
        order_input: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        let currency_code = resolved_string_field(order_input, "currency")
            .or_else(|| resolved_string_field(order_input, "currencyCode"))
            .unwrap_or_else(|| "CAD".to_string());
        let presentment_currency_code = resolved_string_field(order_input, "presentmentCurrency")
            .or_else(|| resolved_string_field(order_input, "presentmentCurrencyCode"))
            .unwrap_or_else(|| currency_code.clone());
        let mut subtotal = 0.0;
        let mut tax_total = 0.0;
        let line_items = resolved_object_list_field(order_input, "lineItems")
            .into_iter()
            .enumerate()
            .map(|(index, line_item)| {
                let (line, line_subtotal, line_tax_total) = order_create_line_item_record(
                    &line_item,
                    index,
                    &currency_code,
                    &presentment_currency_code,
                );
                subtotal += line_subtotal;
                tax_total += line_tax_total;
                line
            })
            .collect::<Vec<_>>();
        let fulfillment_orders = if line_items.is_empty() {
            Vec::new()
        } else {
            vec![order_default_fulfillment_order(order_id, &line_items)]
        };
        let shipping_lines = resolved_object_list_field(order_input, "shippingLines")
            .into_iter()
            .map(|shipping_line| {
                let price_input =
                    resolved_object_field(&shipping_line, "priceSet").unwrap_or_default();
                let amount = input_money_amount(&price_input).unwrap_or(0.0);
                let shipping_currency =
                    input_money_currency(&price_input).unwrap_or_else(|| currency_code.clone());
                let tax_lines = order_create_tax_lines(&shipping_line, "taxLines", &currency_code);
                tax_total += tax_lines
                    .iter()
                    .filter_map(|tax_line| tax_line["priceSet"]["shopMoney"]["amount"].as_str())
                    .filter_map(|amount| amount.parse::<f64>().ok())
                    .sum::<f64>();
                json!({
                    "title": resolved_string_field(&shipping_line, "title").unwrap_or_default(),
                    "code": resolved_string_field(&shipping_line, "code").unwrap_or_default(),
                    "source": resolved_string_field(&shipping_line, "source").unwrap_or_default(),
                    "originalPriceSet": order_create_money_set(amount, &shipping_currency),
                    "priceSet": order_create_money_set(amount, &shipping_currency),
                    "taxLines": tax_lines
                })
            })
            .collect::<Vec<_>>();
        let shipping_total = shipping_lines
            .iter()
            .filter_map(|line| line["originalPriceSet"]["shopMoney"]["amount"].as_str())
            .filter_map(|amount| amount.parse::<f64>().ok())
            .sum::<f64>();
        let (discount_total, discount_codes) =
            order_create_discount_amount(order_input, &currency_code);
        let total = (subtotal + shipping_total + tax_total - discount_total).max(0.0);
        let transactions = resolved_object_list_field(order_input, "transactions")
            .into_iter()
            .enumerate()
            .map(|(index, transaction)| {
                order_create_transaction_record(&transaction, index, &currency_code)
            })
            .collect::<Vec<_>>();
        let financial_status = order_create_financial_status(order_input, &transactions, total);
        let mut order = json!({
            "id": order_id,
            "name": format!("#{}", self.store.staged.orders.len() + 1),
            "email": resolved_string_field(order_input, "email"),
            "customer": resolved_string_field(order_input, "customerId")
                .map(|id| json!({
                    "id": id,
                    "email": resolved_string_field(order_input, "email"),
                    "displayName": Value::Null
                }))
                .unwrap_or(Value::Null),
            "purchasingEntity": b2b_purchasing_entity_record(
                resolved_object_field(order_input, "purchasingEntity"),
                |id| self.b2b_company_node_for_id(id),
                &self.store.staged.b2b_locations,
            ),
            "note": resolved_string_field(order_input, "note"),
            "tags": resolved_string_list_field(order_input, "tags"),
            "currencyCode": currency_code,
            "presentmentCurrencyCode": presentment_currency_code,
            "displayFinancialStatus": financial_status,
            "displayFulfillmentStatus": resolved_string_field(order_input, "fulfillmentStatus")
                .unwrap_or_else(|| "UNFULFILLED".to_string()),
            "customAttributes": order_create_custom_attributes(order_input, "customAttributes"),
            "billingAddress": order_create_address(resolved_object_field(order_input, "billingAddress")),
            "shippingAddress": order_create_address(resolved_object_field(order_input, "shippingAddress")),
            "subtotalPriceSet": order_create_money_set(subtotal, &currency_code),
            "currentSubtotalPriceSet": order_create_money_set(subtotal, &currency_code),
            "totalTaxSet": order_create_money_set(tax_total, &currency_code),
            "totalDiscountsSet": order_create_money_set(discount_total, &currency_code),
            "currentTotalPriceSet": order_create_money_set(total, &currency_code),
            "totalPriceSet": order_create_money_set(total, &currency_code),
            "discountCodes": discount_codes,
            "shippingLines": order_connection(shipping_lines),
            "lineItems": order_connection(line_items),
            "fulfillments": [],
            "fulfillmentOrders": order_connection(fulfillment_orders),
            "transactions": transactions
        });
        if let Some(object) = order.as_object_mut() {
            object.insert(
                "currentTotalPriceSet".to_string(),
                order_create_money_bag(total, &currency_code, &presentment_currency_code),
            );
            object.insert(
                "totalPriceSet".to_string(),
                order_create_money_bag(total, &currency_code, &presentment_currency_code),
            );
        }
        order_create_payment_fields(&mut order, &transactions, total, &currency_code);
        order
    }

    fn stage_completable_draft_order(
        &mut self,
        field: &RootFieldSelection,
        financial_status: &str,
        fallback_amount: &str,
        currency_code: &str,
    ) -> Value {
        let id = format!(
            "gid://shopify/DraftOrder/{}",
            self.store.staged.next_draft_order_id
        );
        self.store.staged.next_draft_order_id += 1;
        let amount = if draft_order_create_input_email(field).as_deref()
            == Some("complete-readback@example.test")
        {
            draft_order_total_amount(field)
        } else {
            fallback_amount.to_string()
        };
        let name = format!("#D{}", self.store.staged.draft_orders.len() + 1);
        let line_item = draft_order_line_item_record(field);
        let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
        let purchasing_entity = b2b_purchasing_entity_record(
            resolved_object_field(&input, "purchasingEntity"),
            |id| self.b2b_company_node_for_id(id),
            &self.store.staged.b2b_locations,
        );
        let draft_order = json!({
            "id": id,
            "name": name,
            "status": "OPEN",
            "purchasingEntity": purchasing_entity,
            "__draftProxyFinancialStatus": financial_status,
            "__draftProxyLineItems": [line_item],
            "totalPriceSet": order_money_set(&amount, currency_code)
        });
        self.store
            .staged
            .draft_orders
            .insert(id.clone(), draft_order.clone());
        let payload = selected_json(
            &json!({ "draftOrder": draft_order, "userErrors": [] }),
            &field.selection,
        );
        data_response(&field.response_key, payload)
    }

    fn complete_staged_draft_order(&mut self, field: &RootFieldSelection) -> Value {
        let Some(id) = resolved_string_arg(&field.arguments, "id") else {
            return selected_json(
                &json!({
                    "draftOrder": Value::Null,
                    "userErrors": [{ "field": ["id"], "message": "ID is required" }]
                }),
                &field.selection,
            );
        };
        let Some(mut draft_order) = self.store.staged.draft_orders.get(&id).cloned() else {
            return selected_json(
                &json!({
                    "draftOrder": Value::Null,
                    "userErrors": [{ "field": ["id"], "message": "Draft order does not exist" }]
                }),
                &field.selection,
            );
        };
        let payment_gateway_id = resolved_string_arg(&field.arguments, "paymentGatewayId");
        if payment_gateway_id.is_some() {
            return selected_json(
                &json!({
                    "draftOrder": Value::Null,
                    "userErrors": [{
                        "field": ["paymentGatewayId"],
                        "message": "Payment gateway does not exist"
                    }]
                }),
                &field.selection,
            );
        }
        let order_id = "gid://shopify/Order/4".to_string();
        let amount = draft_order["totalPriceSet"]["shopMoney"]["amount"]
            .as_str()
            .unwrap_or("0.0")
            .to_string();
        let currency_code = draft_order["totalPriceSet"]["shopMoney"]["currencyCode"]
            .as_str()
            .unwrap_or("CAD")
            .to_string();
        let payment_pending = matches!(
            field.arguments.get("paymentPending"),
            Some(ResolvedValue::Bool(true))
        );
        let order = json!({
            "id": order_id,
            "name": format!("#{}", self.store.staged.orders.len() + 1),
            "sourceName": "347082227713",
            "displayFinancialStatus": if payment_pending { "PENDING" } else { "PAID" },
            "displayFulfillmentStatus": "UNFULFILLED",
            "purchasingEntity": draft_order["purchasingEntity"].clone(),
            "currentTotalPriceSet": order_money_set(&amount, &currency_code),
            "lineItems": {
                "nodes": draft_order["__draftProxyLineItems"].as_array().cloned().unwrap_or_default()
            }
        });
        draft_order["status"] = json!("COMPLETED");
        draft_order["completedAt"] = json!("2024-01-01T00:00:02.000Z");
        draft_order["order"] = order.clone();
        self.store
            .staged
            .draft_orders
            .insert(id.clone(), draft_order.clone());
        self.store.staged.orders.insert(order_id, order);
        selected_json(
            &json!({ "draftOrder": draft_order, "userErrors": [] }),
            &field.selection,
        )
    }

    pub(in crate::proxy) fn draft_order_invoice_send_local_response(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Option<Response> {
        let fields = root_fields(query, variables)?;
        if !fields.iter().any(|field| {
            field.name == "draftOrderInvoiceSend"
                || (field.name == "draftOrderCreate"
                    && draft_order_create_first_line_title(field).as_deref()
                        == Some("Invoice error parity item"))
        }) {
            return None;
        }

        for field in &fields {
            if field.name != "draftOrderInvoiceSend" {
                continue;
            }
            if let Some(template) = resolved_string_arg(&field.arguments, "templateName") {
                if !is_valid_draft_order_invoice_template(&template) {
                    return Some(ok_json(json!({
                        "errors": [{
                            "message": format!(
                                "Variable $template of type DraftOrderEmailTemplate was provided invalid value {template}"
                            )
                        }]
                    })));
                }
            }
        }

        let mut data = serde_json::Map::new();
        for field in fields {
            let value = match field.name.as_str() {
                "draftOrderCreate"
                    if draft_order_create_first_line_title(&field).as_deref()
                        == Some("Invoice error parity item") =>
                {
                    Some(self.draft_order_invoice_errors_create(&field, request, query, variables))
                }
                "draftOrderInvoiceSend" => {
                    Some(self.draft_order_invoice_errors_send(&field, request, query, variables))
                }
                _ => return None,
            }?;
            data.insert(field.response_key.clone(), value);
        }
        Some(ok_json(json!({ "data": Value::Object(data) })))
    }

    pub(in crate::proxy) fn draft_order_invoice_errors_create(
        &mut self,
        field: &RootFieldSelection,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
        let id = format!(
            "gid://shopify/DraftOrder/{}",
            self.store.staged.next_draft_order_id
        );
        self.store.staged.next_draft_order_id += 1;
        let email = resolved_string_field(&input, "email")
            .filter(|email| !email.trim().is_empty())
            .map(Value::String)
            .unwrap_or(Value::Null);
        let record = json!({
            "id": id,
            "name": "#D1",
            "status": "OPEN",
            "ready": true,
            "email": email,
            "note": Value::Null,
            "purchasingEntity": Value::Null,
            "customer": Value::Null,
            "taxExempt": false,
            "taxesIncluded": false,
            "reserveInventoryUntil": Value::Null,
            "paymentTerms": Value::Null,
            "tags": [],
            "invoiceUrl": format!("https://shopify-draft-proxy.local/draft_orders/{id}/invoice"),
            "customAttributes": [],
            "appliedDiscount": Value::Null,
            "billingAddress": Value::Null,
            "shippingAddress": Value::Null,
            "shippingLine": Value::Null,
            "createdAt": "2024-01-01T00:00:00.000Z",
            "updatedAt": "2024-01-01T00:00:00.000Z",
            "subtotalPriceSet": draft_order_invoice_money_set("1.0", "CAD"),
            "totalDiscountsSet": draft_order_invoice_money_set("0.0", "CAD"),
            "totalShippingPriceSet": draft_order_invoice_money_set("0.0", "CAD"),
            "totalPriceSet": draft_order_invoice_money_set("1.0", "CAD"),
            "totalQuantityOfLineItems": 1,
            "lineItems": { "nodes": [draft_order_invoice_line_item()] }
        });
        self.store
            .staged
            .draft_orders
            .insert(id.clone(), record.clone());
        self.record_orders_local_log_entry(OrdersLocalLogEntry {
            request,
            query,
            variables,
            root_field: "draftOrderCreate",
            staged_resource_ids: vec![id],
            outcome: OrdersLocalLogOutcome {
                status: "staged",
                notes: "Locally staged draftOrderCreate in shopify-draft-proxy.",
            },
        });
        selected_json(
            &json!({
                "draftOrder": record,
                "userErrors": []
            }),
            &field.selection,
        )
    }

    pub(in crate::proxy) fn draft_order_invoice_errors_send(
        &mut self,
        field: &RootFieldSelection,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
        let Some(draft_order) = self.store.staged.draft_orders.get(&id).cloned() else {
            self.record_orders_local_log_entry(OrdersLocalLogEntry {
                request,
                query,
                variables,
                root_field: "draftOrderInvoiceSend",
                staged_resource_ids: Vec::new(),
                outcome: OrdersLocalLogOutcome {
                    status: "failed",
                    notes: "Locally handled draftOrderInvoiceSend safety validation.",
                },
            });
            return selected_json(
                &json!({
                    "draftOrder": Value::Null,
                    "userErrors": [{ "field": Value::Null, "message": "Draft order not found" }],
                    "invoiceErrors": []
                }),
                &field.selection,
            );
        };

        if draft_order_invoice_recipient(&field.arguments, &draft_order).is_none() {
            self.record_orders_local_log_entry(OrdersLocalLogEntry {
                request,
                query,
                variables,
                root_field: "draftOrderInvoiceSend",
                staged_resource_ids: Vec::new(),
                outcome: OrdersLocalLogOutcome {
                    status: "failed",
                    notes: "Locally handled draftOrderInvoiceSend safety validation.",
                },
            });
            return selected_json(
                &json!({
                    "draftOrder": draft_order,
                    "userErrors": [{ "field": Value::Null, "message": "To can't be blank" }],
                    "invoiceErrors": [{
                        "code": "CUSTOMER_NO_EMAIL",
                        "message": "Customer email can't be blank"
                    }]
                }),
                &field.selection,
            );
        }

        let mut updated = draft_order.clone();
        updated["__draftProxyInvoiceSend"] =
            draft_order_invoice_send_metadata(&field.arguments, &draft_order);
        self.store.staged.draft_orders.insert(id.clone(), updated);
        self.record_orders_local_log_entry(OrdersLocalLogEntry {
            request,
            query,
            variables,
            root_field: "draftOrderInvoiceSend",
            staged_resource_ids: vec![id],
            outcome: OrdersLocalLogOutcome {
                status: "staged",
                notes: "Locally handled draftOrderInvoiceSend safety validation.",
            },
        });
        selected_json(
            &json!({
                "draftOrder": draft_order,
                "userErrors": [],
                "invoiceErrors": []
            }),
            &field.selection,
        )
    }

    fn record_orders_local_log_entry(&mut self, entry: OrdersLocalLogEntry<'_>) {
        let root_fields = parse_operation(entry.query)
            .map(|operation| operation.root_fields)
            .unwrap_or_else(|| vec![entry.root_field.to_string()]);
        self.log_entries.push(json!({
            "id": format!("gid://shopify/MutationLogEntry/{}", self.log_entries.len() + 1),
            "operationName": entry.root_field,
            "path": entry.request.path,
            "query": entry.query,
            "variables": resolved_variables_json(entry.variables),
            "rawBody": entry.request.body,
            "stagedResourceIds": entry.staged_resource_ids,
            "status": entry.outcome.status,
            "interpreted": {
                "operationType": "mutation",
                "operationName": entry.root_field,
                "rootFields": root_fields,
                "primaryRootField": entry.root_field,
                "capability": {
                    "operationName": entry.root_field,
                    "domain": "orders",
                    "execution": "stage-locally"
                }
            },
            "notes": entry.outcome.notes
        }));
    }

    pub(in crate::proxy) fn remaining_order_local_data(
        &mut self,
        request: &Request,
        root_field: &str,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Option<Value> {
        let field = root_fields(query, variables)
            .and_then(|fields| fields.into_iter().find(|field| field.name == root_field));
        if root_field == "fulfillmentCreate" {
            let field = field?;
            return Some(data_response(
                &field.response_key,
                self.staged_fulfillment_payload(request, query, variables, &field),
            ));
        }
        if root_field == "fulfillmentCancel" {
            let field = field?;
            if let Some(payload) =
                self.cancel_staged_fulfillment_payload(request, query, variables, &field)
            {
                return Some(data_response(&field.response_key, payload));
            }
            let payload = match resolved_string_arg(&field.arguments, "id")?.as_str() {
                "gid://shopify/Fulfillment/6189145325801" => json!({
                    "fulfillment": Value::Null,
                    "userErrors": [orders_error(&["id"], "fulfillment_cannot_be_cancelled", "INVALID")]
                }),
                "gid://shopify/Fulfillment/7770000000001" => json!({
                    "fulfillment": Value::Null,
                    "userErrors": [orders_error(&["id"], "fulfillment_already_delivered", "INVALID")]
                }),
                _ => return None,
            };
            return Some(data_response(
                &field.response_key,
                selected_json(&payload, &field.selection),
            ));
        }
        if root_field == "fulfillmentTrackingInfoUpdate" {
            let field = field?;
            if let Some(payload) =
                self.update_staged_fulfillment_tracking_payload(request, query, variables, &field)
            {
                return Some(data_response(&field.response_key, payload));
            }
            let payload = match resolved_string_arg(&field.arguments, "fulfillmentId")?.as_str() {
                "gid://shopify/Fulfillment/6189145325801" => json!({
                    "fulfillment": Value::Null,
                    "userErrors": [orders_error(&["fulfillmentId"], "fulfillment_is_cancelled", "INVALID")]
                }),
                _ => return None,
            };
            return Some(data_response(
                &field.response_key,
                selected_json(&payload, &field.selection),
            ));
        }
        if root_field == "ordersCount" {
            return Some(orders_empty_count_payload());
        }
        if root_field == "orderCreate" {
            let field = field?;
            let order_arg = field.arguments.get("order")?;
            if let ResolvedValue::Object(order_input) = order_arg {
                let email = resolved_string_field(order_input, "email").unwrap_or_default();
                if !email.is_empty() && !email.starts_with("order-customer-") {
                    return None;
                }
            }
            let order = self.order_customer_paths_order_create(&field)?;
            return Some(data_response(&field.response_key, order));
        }
        if root_field == "orderDelete" {
            let field = field?;
            let payload = self.stage_order_delete(request, query, variables, &field)?;
            return Some(data_response(&field.response_key, payload));
        }
        if root_field == "orderUpdate"
            && resolved_object_field(variables, "input")
                .and_then(|input| resolved_string_field(&input, "staffMemberId"))
                .is_some()
        {
            let field = field?;
            return Some(data_response(
                &field.response_key,
                selected_json(
                    &json!({
                        "order": Value::Null,
                        "userErrors": [orders_error(&["input", "staffMemberId"], "Staff member does not exist", "NOT_FOUND")]
                    }),
                    &field.selection,
                ),
            ));
        }
        if root_field == "orderEditAddVariant" {
            let field = field?;
            let variant_id = resolved_string_field(variables, "variantId")?;
            match variant_id.as_str() {
                "gid://shopify/ProductVariant/0" => {
                    return Some(data_response(
                        &field.response_key,
                        selected_json(
                            &json!({
                                "calculatedOrder": Value::Null,
                                "calculatedLineItem": Value::Null,
                                "orderEditSession": Value::Null,
                                "userErrors": [{
                                    "field": ["variantId"],
                                    "message": "can't convert Integer[0] to a positive Integer to use as an untrusted id"
                                }]
                            }),
                            &field.selection,
                        ),
                    ));
                }
                "gid://shopify/ProductVariant/48540157378793" => {
                    self.store.staged.order_edit_existing_mode = Some("duplicate".to_string());
                    return Some(data_response(
                        &field.response_key,
                        selected_json(
                            &json!({
                                "calculatedLineItem": {
                                    "title": "Test Product - 6635",
                                    "quantity": 1,
                                    "currentQuantity": 1,
                                    "sku": Value::Null,
                                    "variant": { "id": "gid://shopify/ProductVariant/48540157378793" },
                                    "originalUnitPriceSet": order_money_set("0.0", "CAD")
                                },
                                "userErrors": []
                            }),
                            &field.selection,
                        ),
                    ));
                }
                _ => {}
            }
            self.store.staged.order_edit_existing_mode = Some("add".to_string());
            let mut order = order_edit_existing_base_order();
            if let Some(nodes) = order["lineItems"]["nodes"].as_array_mut() {
                nodes.push(order_edit_existing_variant_line(1, 1));
            }
            self.store.staged.order_edit_existing_order = Some(order.clone());
            let calculated_line = order_edit_existing_calculated_line(1, 1);
            self.store.staged.order_edit_existing_calculated_order = Some(json!({
                "id": resolved_string_arg(&field.arguments, "id").unwrap_or_else(|| "gid://shopify/CalculatedOrder/221172236521".to_string()),
                "lineItems": { "nodes": [] },
                "addedLineItems": { "nodes": [calculated_line.clone()] }
            }));
            return Some(data_response(
                &field.response_key,
                json!({
                    "calculatedLineItem": selected_json(&calculated_line, &selected_child_selection(&field.selection, "calculatedLineItem").unwrap_or_default()),
                    "orderEditSession": selected_json(&json!({ "id": "gid://shopify/OrderEditSession/221172236521" }), &selected_child_selection(&field.selection, "orderEditSession").unwrap_or_default()),
                    "userErrors": []
                }),
            ));
        }
        if root_field == "orderEditSetQuantity" {
            let field = field?;
            self.store.staged.order_edit_existing_mode = Some("zero".to_string());
            let mut order = order_edit_existing_base_order();
            if let Some(nodes) = order["lineItems"]["nodes"].as_array_mut() {
                nodes.push(order_edit_existing_variant_line(1, 0));
            }
            order["currentSubtotalLineItemsQuantity"] = json!(2);
            self.store.staged.order_edit_existing_order = Some(order);
            let calculated_line = order_edit_existing_calculated_line(0, 0);
            self.store.staged.order_edit_existing_calculated_order = Some(json!({
                "id": resolved_string_arg(&field.arguments, "id").unwrap_or_else(|| "gid://shopify/CalculatedOrder/221172236521".to_string()),
                "lineItems": { "nodes": [calculated_line.clone()] },
                "addedLineItems": { "nodes": [] }
            }));
            return Some(data_response(
                &field.response_key,
                json!({
                    "calculatedLineItem": selected_json(&calculated_line, &selected_child_selection(&field.selection, "calculatedLineItem").unwrap_or_default()),
                    "userErrors": []
                }),
            ));
        }
        if root_field == "orderEditCommit" {
            let field = field?;
            let order = self.store.staged.order_edit_existing_order.clone();
            let payload = if let Some(order) = order {
                json!({
                    "order": order,
                    "successMessages": ["Order updated"],
                    "userErrors": []
                })
            } else {
                json!({
                    "order": Value::Null,
                    "successMessages": ["Order updated"],
                    "userErrors": []
                })
            };
            return Some(data_response(
                &field.response_key,
                selected_json(&payload, &field.selection),
            ));
        }
        if root_field == "order"
            && root_fields(query, variables)
                .and_then(|fields| fields.into_iter().find(|field| field.name == "order"))
                .is_some_and(order_read_selects_order_edit_existing_fields)
        {
            let field = field?;
            let order = self.store.staged.order_edit_existing_order.as_ref()?;
            return Some(data_response(
                &field.response_key,
                selected_json(order, &field.selection),
            ));
        }
        None
    }

    fn stage_order_delete(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        field: &RootFieldSelection,
    ) -> Option<Value> {
        let order_id = resolved_string_arg(&field.arguments, "orderId")?;
        if !self.store.staged.orders.contains_key(&order_id) {
            return Some(selected_json(
                &json!({
                    "deletedId": Value::Null,
                    "userErrors": [orders_error(&["orderId"], "Order does not exist", "NOT_FOUND")]
                }),
                &field.selection,
            ));
        }

        self.delete_staged_order(&order_id);
        self.record_orders_local_log_entry(OrdersLocalLogEntry {
            request,
            query,
            variables,
            root_field: "orderDelete",
            staged_resource_ids: vec![order_id.clone()],
            outcome: OrdersLocalLogOutcome {
                status: "staged",
                notes: "Locally staged orderDelete in shopify-draft-proxy.",
            },
        });
        Some(selected_json(
            &json!({
                "deletedId": order_id,
                "userErrors": []
            }),
            &field.selection,
        ))
    }

    fn delete_staged_order(&mut self, order_id: &str) {
        self.store.staged.orders.remove(order_id);
        self.store
            .staged
            .deleted_order_ids
            .insert(order_id.to_string());

        for orders in self.store.staged.customer_orders.values_mut() {
            orders.retain(|order| order["id"].as_str() != Some(order_id));
        }
        self.store
            .staged
            .customer_orders
            .retain(|_, orders| !orders.is_empty());

        if let Some(terms_id) = self.store.staged.payment_terms_owner_index.remove(order_id) {
            self.store.staged.payment_terms.remove(&terms_id);
        }

        if let Some(return_ids) = self.store.staged.returns_by_order.remove(order_id) {
            for return_id in return_ids {
                if let Some(record) = self.store.staged.returns.remove(&return_id) {
                    if let Some(nodes) = record["reverseFulfillmentOrders"]["nodes"].as_array() {
                        for node in nodes {
                            if let Some(reverse_id) = node["id"].as_str() {
                                self.remove_reverse_fulfillment_order(reverse_id);
                            }
                        }
                    }
                }
            }
        }

        self.store.staged.order_customer_orders.remove(order_id);
        self.store
            .staged
            .order_customer_cancelled_ids
            .remove(order_id);
        self.store
            .staged
            .order_customer_b2b_order_ids
            .remove(order_id);
    }

    fn remove_reverse_fulfillment_order(&mut self, reverse_id: &str) {
        self.store
            .staged
            .reverse_fulfillment_orders
            .remove(reverse_id);
        let delivery_ids = self
            .store
            .staged
            .reverse_deliveries
            .iter()
            .filter(|(_, delivery)| {
                delivery["reverseFulfillmentOrder"]["id"].as_str() == Some(reverse_id)
            })
            .map(|(id, _)| id.clone())
            .collect::<Vec<_>>();
        for delivery_id in delivery_ids {
            self.store.staged.reverse_deliveries.remove(&delivery_id);
        }
    }

    pub(in crate::proxy) fn order_payment_transaction_local_data(
        &mut self,
        request: &Request,
        root_field: &str,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Option<Value> {
        let field = root_fields(query, variables)
            .and_then(|fields| fields.into_iter().find(|field| field.name == root_field));
        match root_field {
            "orderCreate"
                if field
                    .as_ref()
                    .is_some_and(order_create_selects_payment_transaction_fields) =>
            {
                let field = field?;
                let order = self.stage_payment_order(&field);
                let order_id = order["id"].as_str().unwrap_or_default().to_string();
                self.record_mutation_log_entry(
                    request,
                    query,
                    variables,
                    root_field,
                    vec![order_id],
                );
                Some(data_response(
                    &field.response_key,
                    selected_json(
                        &json!({ "order": order, "userErrors": [] }),
                        &field.selection,
                    ),
                ))
            }
            "orderCapture" => {
                let field = field?;
                let input = resolved_object_field(variables, "input")?;
                let order_id = resolved_string_field(&input, "id")?;
                let outcome = self.stage_payment_capture(&order_id, &input);
                let (transaction, order, user_errors, staged_ids) = match outcome {
                    Some(outcome) => outcome,
                    None => {
                        let order = self
                            .store
                            .staged
                            .orders
                            .get(&order_id)
                            .cloned()
                            .unwrap_or(Value::Null);
                        (
                            Value::Null,
                            order,
                            vec![payment_user_error(
                                Value::Null,
                                "Unable to find parent transaction",
                                None,
                            )],
                            Vec::new(),
                        )
                    }
                };
                if !staged_ids.is_empty() {
                    self.record_mutation_log_entry(
                        request, query, variables, root_field, staged_ids,
                    );
                }
                Some(data_response(
                    &field.response_key,
                    selected_json(
                        &json!({ "transaction": transaction, "order": order, "userErrors": user_errors }),
                        &field.selection,
                    ),
                ))
            }
            "transactionVoid" => {
                let field = field?;
                let parent_id = resolved_string_arg(&field.arguments, "parentTransactionId")
                    .or_else(|| resolved_string_field(variables, "id"))?;
                let (transaction, user_errors, staged_ids) = self.stage_payment_void(&parent_id);
                if !staged_ids.is_empty() {
                    self.record_mutation_log_entry(
                        request, query, variables, root_field, staged_ids,
                    );
                }
                Some(data_response(
                    &field.response_key,
                    selected_json(
                        &json!({ "transaction": transaction, "userErrors": user_errors }),
                        &field.selection,
                    ),
                ))
            }
            "order"
                if field
                    .as_ref()
                    .is_some_and(order_read_selects_payment_transaction_fields) =>
            {
                let field = field?;
                let id = resolved_string_arg(&field.arguments, "id")?;
                let order = self.store.staged.orders.get(&id)?;
                Some(data_response(
                    &field.response_key,
                    selected_json(order, &field.selection),
                ))
            }
            "orderCreateMandatePayment" => {
                let field = field?;
                if !field.arguments.contains_key("mandateId") {
                    let operation_path = parsed_document(query, variables)
                        .map(|document| document.operation_path)
                        .unwrap_or_else(|| "mutation".to_string());
                    return Some(json!({
                        "errors": [{
                            "message": "Field 'orderCreateMandatePayment' is missing required arguments: mandateId",
                            "locations": [{
                                "line": field.location.line,
                                "column": field.location.column
                            }],
                            "path": [operation_path, "orderCreateMandatePayment"],
                            "extensions": {
                                "code": "missingRequiredArguments",
                                "className": "Field",
                                "name": "orderCreateMandatePayment",
                                "arguments": "mandateId"
                            }
                        }]
                    }));
                }
                let order = resolved_string_arg(&field.arguments, "id")
                    .or_else(|| resolved_string_field(variables, "id"))
                    .and_then(|id| self.store.staged.orders.get(&id).cloned())
                    .unwrap_or(Value::Null);
                let idempotency_key = resolved_string_arg(&field.arguments, "idempotencyKey")
                    .or_else(|| resolved_string_field(variables, "idempotencyKey"));
                let Some(idempotency_key) = idempotency_key else {
                    return Some(data_response(
                        &field.response_key,
                        selected_json(
                            &json!({
                                "job": Value::Null,
                                "paymentReferenceId": Value::Null,
                                "order": order,
                                "userErrors": [{
                                    "field": ["idempotencyKey"],
                                    "message": "Idempotency key is required"
                                }]
                            }),
                            &field.selection,
                        ),
                    ));
                };
                let order_id = resolved_string_arg(&field.arguments, "id")
                    .or_else(|| resolved_string_field(variables, "id"))
                    .unwrap_or_else(|| "gid://shopify/Order/1".to_string());
                let amount_input = resolved_object_field(&field.arguments, "amount")
                    .or_else(|| resolved_object_field(variables, "amount"))
                    .unwrap_or_default();
                let amount =
                    normalized_order_payment_amount(resolved_string_field(&amount_input, "amount"));
                let currency = resolved_string_field(&amount_input, "currencyCode")
                    .unwrap_or_else(|| "CAD".to_string());
                let auto_capture =
                    resolved_bool_field(&field.arguments, "autoCapture").unwrap_or(true);
                let key = format!("{order_id}:{idempotency_key}");
                if !self.store.staged.mandate_payment_keys.contains(&key)
                    || !self.store.staged.orders.contains_key(&order_id)
                {
                    let order = mandate_payment_order_record(
                        &order_id,
                        &idempotency_key,
                        &amount,
                        &currency,
                        auto_capture,
                    );
                    self.store.staged.orders.insert(order_id.clone(), order);
                    self.store.staged.mandate_payment_keys.insert(key);
                }
                let order = self
                    .store
                    .staged
                    .orders
                    .get(&order_id)
                    .cloned()
                    .unwrap_or(Value::Null);
                let payment_reference_id = format!("{order_id}/{idempotency_key}");
                Some(data_response(
                    &field.response_key,
                    selected_json(
                        &json!({
                            "job": {
                                "id": "gid://shopify/Job/6",
                                "done": true
                            },
                            "paymentReferenceId": payment_reference_id,
                            "order": order,
                            "userErrors": []
                        }),
                        &field.selection,
                    ),
                ))
            }
            _ => None,
        }
    }

    fn stage_payment_order(&mut self, field: &RootFieldSelection) -> Value {
        let id = format!("gid://shopify/Order/{}", self.store.staged.next_order_id);
        self.store.staged.next_order_id += 1;
        let order_input = resolved_object_field(&field.arguments, "order").unwrap_or_default();
        let currency =
            resolved_string_field(&order_input, "currency").unwrap_or_else(|| "CAD".to_string());
        let transaction_inputs = resolved_object_list_field(&order_input, "transactions");
        let first_transaction = transaction_inputs.first().cloned().unwrap_or_default();
        let amount_set = payment_money_set_from_input(&first_transaction)
            .unwrap_or_else(|| order_money_set("25.0", &currency));
        let amount = payment_money_amount(&amount_set, "presentmentMoney")
            .or_else(|| payment_money_amount(&amount_set, "shopMoney"))
            .unwrap_or_else(|| "25.0".to_string());
        let transaction_id = format!(
            "gid://shopify/OrderTransaction/{}",
            self.store.staged.order_payment_next_transaction_id
        );
        self.store.staged.order_payment_next_transaction_id += 1;
        let kind = resolved_string_field(&first_transaction, "kind")
            .unwrap_or_else(|| "AUTHORIZATION".to_string());
        let status = resolved_string_field(&first_transaction, "status")
            .unwrap_or_else(|| "SUCCESS".to_string());
        let transaction = payment_transaction_record_from_amount_set(
            &transaction_id,
            &kind,
            &status,
            amount_set.clone(),
            Value::Null,
        );
        let (display_status, capturable_amount, outstanding_amount, received_amount) =
            if kind == "AUTHORIZATION" && status == "SUCCESS" {
                ("AUTHORIZED", amount.as_str(), "0.0", "0.0")
            } else if matches!(kind.as_str(), "CAPTURE" | "SALE") && status == "SUCCESS" {
                ("PAID", "0.0", "0.0", amount.as_str())
            } else {
                ("PENDING", "0.0", amount.as_str(), "0.0")
            };
        let order = payment_order_record(
            &id,
            display_status,
            capturable_amount,
            outstanding_amount,
            received_amount,
            payment_money_currency(&amount_set, "presentmentMoney")
                .or_else(|| payment_money_currency(&amount_set, "shopMoney"))
                .as_deref()
                .unwrap_or(&currency),
            vec![transaction],
        );
        let mut order = order;
        if amount_set.get("presentmentMoney").is_some() {
            let captured_amount = if capturable_amount == "0.0" {
                amount.as_str()
            } else {
                "0.0"
            };
            let (capturable_set, outstanding_set, received_set) =
                payment_money_set_for_order_totals(
                    &amount_set,
                    capturable_amount.parse::<f64>().unwrap_or(0.0),
                    captured_amount.parse::<f64>().unwrap_or(0.0),
                );
            order["totalCapturableSet"] = capturable_set;
            order["totalOutstandingSet"] = outstanding_set;
            order["totalReceivedSet"] = received_set.clone();
            order["netPaymentSet"] = received_set;
        }
        self.store.staged.orders.insert(id, order.clone());
        order
    }

    fn stage_payment_capture(
        &mut self,
        order_id: &str,
        input: &BTreeMap<String, ResolvedValue>,
    ) -> Option<(Value, Value, Vec<Value>, Vec<String>)> {
        let requested_amount = resolved_string_field(input, "amount")?;
        let requested_amount_normalized =
            normalized_order_payment_amount(Some(requested_amount.clone()));
        let requested_amount_value = requested_amount.parse::<f64>().ok()?;
        let parent_id = resolved_string_field(input, "parentTransactionId");
        let final_capture = matches!(input.get("finalCapture"), Some(ResolvedValue::Bool(true)));
        let order = self.store.staged.orders.get(order_id)?;
        let transactions = order["transactions"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let parent_transaction = parent_id
            .as_deref()
            .and_then(|parent_id| {
                transactions
                    .iter()
                    .find(|transaction| transaction["id"].as_str() == Some(parent_id))
                    .cloned()
            })
            .or_else(|| {
                transactions
                    .iter()
                    .find(|transaction| {
                        transaction["kind"].as_str() == Some("AUTHORIZATION")
                            && transaction["status"].as_str() == Some("SUCCESS")
                    })
                    .cloned()
            });
        let Some(parent_transaction) = parent_transaction else {
            return Some((
                Value::Null,
                order.clone(),
                vec![payment_user_error(
                    Value::Null,
                    "Unable to find parent transaction",
                    None,
                )],
                Vec::new(),
            ));
        };
        let parent_id = parent_transaction["id"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        let parent_amount_set = parent_transaction["amountSet"].clone();
        let expected_currency = payment_money_currency(&parent_amount_set, "presentmentMoney")
            .or_else(|| payment_money_currency(&parent_amount_set, "shopMoney"))
            .unwrap_or_else(|| "CAD".to_string());
        let currency = resolved_string_field(input, "currency");
        if currency.as_deref() != Some(expected_currency.as_str()) {
            return Some((
                Value::Null,
                order.clone(),
                vec![payment_user_error(
                    json!(["currency"]),
                    &format!("Currency Currency must match parent transaction {expected_currency}"),
                    None,
                )],
                Vec::new(),
            ));
        }
        if requested_amount_value <= 0.0 {
            return Some((
                Value::Null,
                order.clone(),
                vec![payment_user_error(
                    Value::Null,
                    "Amount must be greater than zero for capture transactions",
                    Some("INVALID_AMOUNT"),
                )],
                Vec::new(),
            ));
        }
        if parent_transaction["kind"].as_str() != Some("AUTHORIZATION")
            || parent_transaction["status"].as_str() != Some("SUCCESS")
        {
            return Some((
                Value::Null,
                order.clone(),
                vec![payment_user_error(
                    json!(["parent_transaction_id"]),
                    "Parent transaction must be a successful authorization",
                    Some("INVALID_TRANSACTION_STATE"),
                )],
                Vec::new(),
            ));
        }
        let already_captured: f64 = transactions
            .iter()
            .filter(|transaction| {
                transaction["kind"].as_str() == Some("CAPTURE")
                    && transaction["status"].as_str() == Some("SUCCESS")
                    && payment_transaction_matches_parent(transaction, &parent_id)
            })
            .filter_map(|transaction| {
                payment_money_amount(&transaction["amountSet"], "presentmentMoney")
                    .or_else(|| payment_money_amount(&transaction["amountSet"], "shopMoney"))
                    .and_then(|amount| amount.parse::<f64>().ok())
            })
            .sum();
        let parent_amount = payment_money_amount(&parent_amount_set, "presentmentMoney")
            .or_else(|| payment_money_amount(&parent_amount_set, "shopMoney"))
            .and_then(|amount| amount.parse::<f64>().ok())
            .unwrap_or(0.0);
        let capturable_amount = (parent_amount - already_captured).max(0.0);
        if requested_amount_value > capturable_amount + 0.000_001 {
            let message = if parent_amount_set.get("presentmentMoney").is_some() {
                format!(
                    "Cannot capture more than the authorized {} for this payment.",
                    format_order_amount(capturable_amount)
                )
            } else {
                "Amount exceeds capturable amount".to_string()
            };
            return Some((
                Value::Null,
                order.clone(),
                vec![payment_user_error(
                    if parent_amount_set.get("presentmentMoney").is_some() {
                        Value::Null
                    } else {
                        json!(["amount"])
                    },
                    &message,
                    Some("OVER_CAPTURE"),
                )],
                Vec::new(),
            ));
        }
        let remaining_amount = if final_capture {
            0.0
        } else {
            (capturable_amount - requested_amount_value).max(0.0)
        };
        let total_received = already_captured + requested_amount_value;
        let transaction_id = format!(
            "gid://shopify/OrderTransaction/{}",
            self.store.staged.order_payment_next_transaction_id
        );
        self.store.staged.order_payment_next_transaction_id += 1;
        let transaction_amount_set = payment_money_set_for_capture(
            &parent_amount_set,
            &requested_amount_normalized,
            currency.as_deref().unwrap_or(&expected_currency),
        );
        let transaction = payment_transaction_record_from_amount_set(
            &transaction_id,
            "CAPTURE",
            "SUCCESS",
            transaction_amount_set,
            payment_transaction_public_parent(&parent_transaction),
        );
        let order = self.store.staged.orders.get_mut(order_id)?;
        if let Some(transactions) = order["transactions"].as_array_mut() {
            transactions.push(transaction.clone());
        }
        let (capturable_set, outstanding_set, received_set) = payment_money_set_for_order_totals(
            &parent_amount_set,
            remaining_amount,
            total_received,
        );
        order["displayFinancialStatus"] = if remaining_amount <= 0.000_001 {
            json!("PAID")
        } else {
            json!("PARTIALLY_PAID")
        };
        order["capturable"] = json!(remaining_amount > 0.000_001);
        order["totalCapturable"] = json!(format_order_amount(remaining_amount));
        order["totalCapturableSet"] = capturable_set;
        order["totalOutstandingSet"] = outstanding_set;
        order["totalReceivedSet"] = received_set.clone();
        order["netPaymentSet"] = received_set;
        Some((
            transaction.clone(),
            order.clone(),
            Vec::new(),
            vec![order_id.to_string(), transaction_id],
        ))
    }

    fn stage_payment_void(&mut self, parent_id: &str) -> (Value, Vec<Value>, Vec<String>) {
        let located = self
            .store
            .staged
            .orders
            .iter()
            .find_map(|(order_id, order)| {
                order["transactions"]
                    .as_array()
                    .and_then(|transactions| {
                        transactions
                            .iter()
                            .find(|transaction| transaction["id"].as_str() == Some(parent_id))
                            .cloned()
                    })
                    .map(|transaction| (order_id.clone(), order.clone(), transaction))
            });
        let Some((order_id, order, parent_transaction)) = located else {
            return (
                Value::Null,
                vec![payment_user_error(
                    json!(["parentTransactionId"]),
                    "Transaction does not exist",
                    Some("TRANSACTION_NOT_FOUND"),
                )],
                Vec::new(),
            );
        };
        if parent_transaction["kind"].as_str() != Some("AUTHORIZATION")
            || parent_transaction["status"].as_str() != Some("SUCCESS")
        {
            return (
                Value::Null,
                vec![payment_user_error(
                    json!(["parentTransactionId"]),
                    "Parent transaction must be a successful authorization",
                    Some("AUTH_NOT_SUCCESSFUL"),
                )],
                Vec::new(),
            );
        }
        let transactions = order["transactions"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let has_successful_capture = transactions.iter().any(|transaction| {
            transaction["kind"].as_str() == Some("CAPTURE")
                && transaction["status"].as_str() == Some("SUCCESS")
                && payment_transaction_matches_parent(transaction, parent_id)
        });
        let has_successful_void = transactions.iter().any(|transaction| {
            transaction["kind"].as_str() == Some("VOID")
                && transaction["status"].as_str() == Some("SUCCESS")
                && payment_transaction_matches_parent(transaction, parent_id)
        });
        if has_successful_capture || has_successful_void {
            return (
                Value::Null,
                vec![payment_user_error(
                    json!(["parentTransactionId"]),
                    "Parent transaction require a parent_id referring to a voidable transaction",
                    Some("AUTH_NOT_VOIDABLE"),
                )],
                Vec::new(),
            );
        }
        let transaction_id = format!(
            "gid://shopify/OrderTransaction/{}",
            self.store.staged.order_payment_next_transaction_id
        );
        self.store.staged.order_payment_next_transaction_id += 1;
        let amount_set = parent_transaction["amountSet"].clone();
        let transaction = payment_transaction_record_from_amount_set(
            &transaction_id,
            "VOID",
            "SUCCESS",
            amount_set.clone(),
            payment_transaction_public_parent(&parent_transaction),
        );
        if let Some(order) = self.store.staged.orders.get_mut(&order_id) {
            let shop_currency = payment_money_currency(&amount_set, "shopMoney")
                .unwrap_or_else(|| "CAD".to_string());
            order["displayFinancialStatus"] = json!("VOIDED");
            order["capturable"] = json!(false);
            order["totalCapturable"] = json!("0.0");
            if amount_set.get("presentmentMoney").is_some() {
                let presentment_currency = payment_money_currency(&amount_set, "presentmentMoney")
                    .unwrap_or_else(|| shop_currency.clone());
                order["totalCapturableSet"] =
                    order_money_set_pair("0.0", &shop_currency, "0.0", &presentment_currency);
                order["totalOutstandingSet"] = amount_set.clone();
                order["totalReceivedSet"] =
                    order_money_set_pair("0.0", &shop_currency, "0.0", &presentment_currency);
                order["netPaymentSet"] =
                    order_money_set_pair("0.0", &shop_currency, "0.0", &presentment_currency);
            } else {
                order["totalCapturableSet"] = order_money_set("0.0", &shop_currency);
                order["totalOutstandingSet"] = amount_set;
                order["totalReceivedSet"] = order_money_set("0.0", &shop_currency);
                order["netPaymentSet"] = order_money_set("0.0", &shop_currency);
            }
            if let Some(transactions) = order["transactions"].as_array_mut() {
                transactions.push(transaction.clone());
            }
        }
        (transaction, Vec::new(), vec![order_id, transaction_id])
    }

    pub(in crate::proxy) fn order_customer_error_paths_data(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Option<Value> {
        let fields = root_fields(query, variables)?;
        let mut data = serde_json::Map::new();
        for field in fields {
            let value = match field.name.as_str() {
                "customerCreate" => self.order_customer_paths_customer_create(&field),
                "companyCreate" => self.order_customer_paths_company_create(&field),
                "companyAssignCustomerAsContact" => {
                    self.order_customer_paths_assign_customer(&field)
                }
                "orderCreate" => self.order_customer_paths_order_create(&field),
                "orderCancel" => {
                    self.order_customer_paths_cancel_order(request, query, variables, &field)
                }
                "orderCustomerSet" => Some(self.order_customer_set_error_paths(&field)),
                "orderCustomerRemove" => Some(self.order_customer_remove_error_paths(&field)),
                _ => None,
            }?;
            data.insert(field.response_key.clone(), value);
        }
        Some(json!({ "data": Value::Object(data) }))
    }

    pub(in crate::proxy) fn order_customer_paths_customer_create(
        &mut self,
        field: &RootFieldSelection,
    ) -> Option<Value> {
        let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
        let email = resolved_string_field(&input, "email").unwrap_or_default();
        if !email.starts_with("order-customer-") {
            return None;
        }
        let customer = json!({
            "id": "gid://shopify/Customer/1?shopify-draft-proxy=synthetic",
            "email": email,
            "displayName": "Order Customer Error Paths"
        });
        self.store.staged.customers.insert(
            customer["id"].as_str().unwrap_or_default().to_string(),
            customer.clone(),
        );
        Some(selected_json(
            &json!({ "customer": customer, "userErrors": [] }),
            &field.selection,
        ))
    }

    pub(in crate::proxy) fn order_customer_paths_company_create(
        &self,
        field: &RootFieldSelection,
    ) -> Option<Value> {
        let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
        let company_input = resolved_object_field(&input, "company").unwrap_or_default();
        let name = resolved_string_field(&company_input, "name")
            .or_else(|| resolved_string_field(&input, "name"))
            .unwrap_or_default();
        if !name.contains("Order Customer Error Paths") {
            return None;
        }
        Some(selected_json(
            &json!({
                "company": {
                    "id": "gid://shopify/Company/1?shopify-draft-proxy=synthetic",
                    "name": name
                },
                "userErrors": []
            }),
            &field.selection,
        ))
    }

    pub(in crate::proxy) fn order_customer_paths_assign_customer(
        &mut self,
        field: &RootFieldSelection,
    ) -> Option<Value> {
        let company_id = resolved_string_arg(&field.arguments, "companyId")?;
        if company_id != "gid://shopify/Company/1?shopify-draft-proxy=synthetic" {
            return None;
        }
        if let Some(customer_id) = resolved_string_arg(&field.arguments, "customerId") {
            self.store
                .staged
                .order_customer_contact_customer_ids
                .insert(customer_id.clone());
        }
        let customer_id =
            resolved_string_arg(&field.arguments, "customerId").unwrap_or_else(|| {
                "gid://shopify/Customer/1?shopify-draft-proxy=synthetic".to_string()
            });
        Some(selected_json(
            &json!({
                "companyContact": {
                    "id": "gid://shopify/CompanyContact/1?shopify-draft-proxy=synthetic",
                    "isMainContact": false,
                    "customer": { "id": customer_id },
                    "company": { "id": company_id, "name": "Order Customer Error Paths Company" }
                },
                "userErrors": []
            }),
            &field.selection,
        ))
    }

    pub(in crate::proxy) fn order_customer_paths_order_create(
        &mut self,
        field: &RootFieldSelection,
    ) -> Option<Value> {
        let order_arg = field.arguments.get("order")?;
        let email = resolved_object_string(order_arg, "email").unwrap_or_default();
        if !email.is_empty() && !email.starts_with("order-customer-") {
            return None;
        }
        let id = format!(
            "gid://shopify/Order/{}?shopify-draft-proxy=synthetic",
            self.store.staged.next_order_customer_order_id
        );
        self.store.staged.next_order_customer_order_id += 1;
        if email == "order-customer-b2b@example.com" {
            self.store
                .staged
                .order_customer_b2b_order_ids
                .insert(id.clone());
        }
        let customer_id = match order_arg {
            ResolvedValue::Object(fields) => resolved_string_arg(fields, "customerId"),
            _ => None,
        };
        let purchasing_entity = match order_arg {
            ResolvedValue::Object(fields) => b2b_purchasing_entity_record(
                resolved_object_field(fields, "purchasingEntity"),
                |id| self.b2b_company_node_for_id(id),
                &self.store.staged.b2b_locations,
            ),
            _ => Value::Null,
        };
        let order = json!({
            "id": id,
            "customer": customer_id.map(|id| json!({ "id": id })).unwrap_or(Value::Null),
            "purchasingEntity": purchasing_entity
        });
        self.store.staged.order_customer_orders.insert(
            order["id"].as_str().unwrap_or_default().to_string(),
            order.clone(),
        );
        Some(selected_json(
            &json!({ "order": order, "userErrors": [] }),
            &field.selection,
        ))
    }

    pub(in crate::proxy) fn order_customer_paths_cancel_order(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        field: &RootFieldSelection,
    ) -> Option<Value> {
        let order_id = resolved_string_arg(&field.arguments, "orderId")?;
        let error_payload = |field_name: &str, message: &str, code: &str| {
            json!({
                "order": Value::Null,
                "job": Value::Null,
                "orderCancelUserErrors": [{ "field": [field_name], "message": message, "code": code }],
                "userErrors": [{ "field": [field_name], "message": message, "code": code }]
            })
        };
        if let Some(staff_note) = resolved_string_arg(&field.arguments, "staffNote") {
            if staff_note.chars().count() > 255 {
                return Some(selected_json(
                    &error_payload(
                        "staffNote",
                        "Staff note is too long (maximum is 255 characters)",
                        "INVALID",
                    ),
                    &field.selection,
                ));
            }
        }
        if matches!(
            field.arguments.get("refund"),
            Some(ResolvedValue::Bool(true))
        ) && field.arguments.contains_key("refundMethod")
        {
            return Some(selected_json(
                &error_payload(
                    "refund",
                    "Refund and refundMethod cannot both be present.",
                    "INVALID",
                ),
                &field.selection,
            ));
        }

        if self.store.staged.orders.contains_key(&order_id) {
            let already_cancelled = self
                .store
                .staged
                .orders
                .get(&order_id)
                .and_then(|order| order.get("cancelledAt"))
                .is_some_and(|cancelled_at| !cancelled_at.is_null());
            if already_cancelled {
                return Some(selected_json(
                    &error_payload("orderId", "Order has already been cancelled", "INVALID"),
                    &field.selection,
                ));
            }

            let reason =
                resolved_string_arg(&field.arguments, "reason").unwrap_or_else(|| "OTHER".into());
            let timestamp = self.order_cancel_timestamp();
            let job_id = format!(
                "gid://shopify/Job/{}?shopify-draft-proxy=synthetic",
                self.log_entries.len() + 1
            );
            let order = self
                .store
                .staged
                .orders
                .get_mut(&order_id)
                .expect("staged order existence was checked before mutation");
            order["closed"] = json!(true);
            order["closedAt"] = json!(timestamp.clone());
            order["cancelledAt"] = json!(timestamp);
            order["cancelReason"] = json!(reason);
            order["updatedAt"] = order["cancelledAt"].clone();
            let order = order.clone();
            if let Some(customer_id) = order["customer"]["id"].as_str() {
                if let Some(customer_orders) =
                    self.store.staged.customer_orders.get_mut(customer_id)
                {
                    for customer_order in customer_orders {
                        if customer_order["id"].as_str() == Some(order_id.as_str()) {
                            *customer_order = order.clone();
                        }
                    }
                }
            }
            self.record_orders_local_log_entry(OrdersLocalLogEntry {
                request,
                query,
                variables,
                root_field: "orderCancel",
                staged_resource_ids: vec![order_id],
                outcome: OrdersLocalLogOutcome {
                    status: "staged",
                    notes: "Locally staged orderCancel in shopify-draft-proxy.",
                },
            });
            return Some(selected_json(
                &json!({
                    "order": order,
                    "job": { "id": job_id, "done": false },
                    "orderCancelUserErrors": [],
                    "userErrors": []
                }),
                &field.selection,
            ));
        }

        let Some(mut order) = self
            .store
            .staged
            .order_customer_orders
            .get(&order_id)
            .cloned()
        else {
            return Some(selected_json(
                &error_payload("orderId", "Order does not exist", "NOT_FOUND"),
                &field.selection,
            ));
        };
        if self
            .store
            .staged
            .order_customer_cancelled_ids
            .contains(&order_id)
        {
            return Some(selected_json(
                &error_payload("orderId", "Order has already been cancelled", "INVALID"),
                &field.selection,
            ));
        }
        self.store
            .staged
            .order_customer_cancelled_ids
            .insert(order_id.clone());
        let reason =
            resolved_string_arg(&field.arguments, "reason").unwrap_or_else(|| "OTHER".into());
        let timestamp = self.order_cancel_timestamp();
        order["closed"] = json!(true);
        order["closedAt"] = json!(timestamp.clone());
        order["cancelledAt"] = json!(timestamp);
        order["cancelReason"] = json!(reason);
        self.store
            .staged
            .order_customer_orders
            .insert(order_id.clone(), order.clone());
        self.record_orders_local_log_entry(OrdersLocalLogEntry {
            request,
            query,
            variables,
            root_field: "orderCancel",
            staged_resource_ids: vec![order_id.clone()],
            outcome: OrdersLocalLogOutcome {
                status: "staged",
                notes: "Locally staged orderCancel in shopify-draft-proxy.",
            },
        });
        Some(selected_json(
            &json!({
                "order": order,
                "job": { "id": "gid://shopify/Job/order-customer-cancel", "done": false },
                "orderCancelUserErrors": [],
                "userErrors": []
            }),
            &field.selection,
        ))
    }

    fn order_cancel_timestamp(&self) -> String {
        format!(
            "2024-01-01T00:00:{:02}.000Z",
            (self.log_entries.len() + 1) % 60
        )
    }

    pub(in crate::proxy) fn order_customer_set_error_paths(
        &mut self,
        field: &RootFieldSelection,
    ) -> Value {
        let order_id = resolved_string_arg(&field.arguments, "orderId").unwrap_or_default();
        let customer_id = resolved_string_arg(&field.arguments, "customerId").unwrap_or_default();
        let customer = self.store.staged.customers.get(&customer_id).cloned();
        let Some(mut order) = self
            .store
            .staged
            .order_customer_orders
            .get(&order_id)
            .cloned()
        else {
            return selected_json(
                &json!({
                    "order": Value::Null,
                    "userErrors": [{ "field": ["orderId"], "message": "Order does not exist", "code": "NOT_FOUND" }]
                }),
                &field.selection,
            );
        };
        let Some(customer) = customer else {
            return selected_json(
                &json!({
                    "order": Value::Null,
                    "userErrors": [{ "field": ["customerId"], "message": "Customer does not exist", "code": "NOT_FOUND" }]
                }),
                &field.selection,
            );
        };
        if self
            .store
            .staged
            .order_customer_b2b_order_ids
            .contains(&order_id)
            && self
                .store
                .staged
                .order_customer_contact_customer_ids
                .contains(&customer_id)
        {
            return selected_json(
                &json!({
                    "order": Value::Null,
                    "userErrors": [{ "field": ["customerId"], "message": "no_customer_role_error", "code": "NOT_PERMITTED" }]
                }),
                &field.selection,
            );
        }
        order["customer"] = customer;
        self.store
            .staged
            .order_customer_orders
            .insert(order_id.clone(), order.clone());
        selected_json(
            &json!({ "order": order, "userErrors": [] }),
            &field.selection,
        )
    }

    pub(in crate::proxy) fn order_customer_remove_error_paths(
        &mut self,
        field: &RootFieldSelection,
    ) -> Value {
        let order_id = resolved_string_arg(&field.arguments, "orderId").unwrap_or_default();
        let Some(mut order) = self
            .store
            .staged
            .order_customer_orders
            .get(&order_id)
            .cloned()
        else {
            return selected_json(
                &json!({
                    "order": Value::Null,
                    "userErrors": [{ "field": ["orderId"], "message": "Order does not exist", "code": "NOT_FOUND" }]
                }),
                &field.selection,
            );
        };
        if self
            .store
            .staged
            .order_customer_cancelled_ids
            .contains(&order_id)
        {
            return selected_json(
                &json!({
                    "order": Value::Null,
                    "userErrors": [{ "field": ["orderId"], "message": "customer_cannot_be_removed", "code": "INVALID" }]
                }),
                &field.selection,
            );
        }
        order["customer"] = Value::Null;
        self.store
            .staged
            .order_customer_orders
            .insert(order_id.clone(), order.clone());
        selected_json(
            &json!({ "order": order, "userErrors": [] }),
            &field.selection,
        )
    }

    pub(in crate::proxy) fn draft_order_bulk_tag_local_data(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Option<Value> {
        let fields = root_fields(query, variables)?;
        if !fields.iter().any(|field| {
            matches!(
                field.name.as_str(),
                "draftOrder" | "draftOrderBulkAddTags" | "draftOrderBulkRemoveTags"
            ) || (field.name == "draftOrderCreate" && draft_order_create_selects_tags(field))
        }) {
            return None;
        }
        let mut data = serde_json::Map::new();
        for field in fields {
            let value = match field.name.as_str() {
                "draftOrderCreate" => Some(self.draft_order_bulk_tag_create(&field)),
                "draftOrder" => Some(self.draft_order_bulk_tag_read(&field)),
                "draftOrderBulkAddTags" => Some(self.draft_order_bulk_add_tags(&field)),
                "draftOrderBulkRemoveTags" => Some(self.draft_order_bulk_remove_tags(&field)),
                _ => None,
            }?;
            data.insert(field.response_key.clone(), value);
        }
        Some(json!({ "data": Value::Object(data) }))
    }

    pub(in crate::proxy) fn draft_order_bulk_tag_create(
        &mut self,
        field: &RootFieldSelection,
    ) -> Value {
        let id = "gid://shopify/DraftOrder/1?shopify-draft-proxy=synthetic".to_string();
        let tags = field
            .arguments
            .get("input")
            .and_then(|input| match input {
                ResolvedValue::Object(fields) => Some(resolved_string_list_arg(fields, "tags")),
                _ => None,
            })
            .unwrap_or_default();
        self.store
            .staged
            .draft_order_tags
            .insert(id.clone(), tags.clone());
        selected_json(
            &json!({
                "draftOrder": { "id": id, "tags": tags },
                "userErrors": []
            }),
            &field.selection,
        )
    }

    pub(in crate::proxy) fn draft_order_bulk_tag_read(&self, field: &RootFieldSelection) -> Value {
        let Some(id) = resolved_string_arg(&field.arguments, "id")
            .or_else(|| resolved_string_arg(&field.arguments, "draftOrderId"))
        else {
            return Value::Null;
        };
        if let Some(record) = self.store.staged.taggable_resources.get(&id) {
            return selected_json(record, &field.selection);
        }
        let value = self
            .store
            .staged
            .draft_order_tags
            .get(&id)
            .map(|tags| json!({ "id": id, "tags": tags }))
            .unwrap_or(Value::Null);
        selected_json(&value, &field.selection)
    }

    pub(in crate::proxy) fn next_draft_order_bulk_tag_job(&mut self) -> Value {
        let id = self.store.staged.next_draft_order_bulk_tag_job_id;
        self.store.staged.next_draft_order_bulk_tag_job_id += 1;
        json!({ "id": format!("gid://shopify/Job/{id}"), "done": false })
    }

    pub(in crate::proxy) fn draft_order_bulk_add_tags(
        &mut self,
        field: &RootFieldSelection,
    ) -> Value {
        let ids = resolved_string_list_arg(&field.arguments, "ids");
        let tags = resolved_string_list_arg(&field.arguments, "tags");
        let normalized_tags: Vec<String> = tags
            .iter()
            .map(|tag| normalize_draft_order_tag(tag))
            .collect();

        let mut user_errors = Vec::new();
        for (index, tag) in normalized_tags.iter().enumerate() {
            if tag.chars().count() >= 256 {
                user_errors.push(json!({
                    "field": ["input", "tags", index.to_string()],
                    "message": "tag_too_long",
                    "code": "INVALID"
                }));
            }
        }

        let mut valid_ids = Vec::new();
        for (index, id) in ids.iter().enumerate() {
            if self.store.staged.draft_order_tags.contains_key(id) {
                valid_ids.push(id.clone());
            } else {
                user_errors.push(json!({
                    "field": ["input", "ids", index.to_string()],
                    "message": "Draft order does not exist",
                    "code": "NOT_FOUND"
                }));
            }
        }

        let too_many = valid_ids.iter().any(|id| {
            let current = self
                .store
                .staged
                .draft_order_tags
                .get(id)
                .cloned()
                .unwrap_or_default();
            let mut identities: BTreeSet<String> = current
                .iter()
                .map(|tag| normalize_draft_order_tag(tag))
                .collect();
            for tag in &normalized_tags {
                identities.insert(tag.clone());
            }
            identities.len() > 250
        });
        if too_many {
            user_errors.clear();
            user_errors.push(json!({
                "field": ["input", "tags"],
                "message": "too_many_tags",
                "code": "INVALID"
            }));
            return selected_json(
                &json!({ "job": Value::Null, "userErrors": user_errors }),
                &field.selection,
            );
        }

        if !normalized_tags.iter().any(|tag| tag.chars().count() >= 256) {
            for id in valid_ids {
                if let Some(current) = self.store.staged.draft_order_tags.get_mut(&id) {
                    let mut existing: BTreeSet<String> = current
                        .iter()
                        .map(|tag| normalize_draft_order_tag(tag))
                        .collect();
                    for tag in &normalized_tags {
                        if existing.insert(tag.clone()) {
                            current.push(tag.clone());
                        }
                    }
                    current.sort_by_key(|tag| normalize_draft_order_tag(tag));
                }
            }
        }

        let job = self.next_draft_order_bulk_tag_job();
        selected_json(
            &json!({ "job": job, "userErrors": user_errors }),
            &field.selection,
        )
    }

    pub(in crate::proxy) fn draft_order_bulk_remove_tags(
        &mut self,
        field: &RootFieldSelection,
    ) -> Value {
        let ids = resolved_string_list_arg(&field.arguments, "ids");
        let tags: BTreeSet<String> = resolved_string_list_arg(&field.arguments, "tags")
            .iter()
            .map(|tag| normalize_draft_order_tag(tag))
            .collect();
        let mut user_errors = Vec::new();
        for (index, id) in ids.iter().enumerate() {
            if let Some(current) = self.store.staged.draft_order_tags.get_mut(id) {
                current.retain(|tag| !tags.contains(&normalize_draft_order_tag(tag)));
            } else {
                user_errors.push(json!({
                    "field": ["input", "ids", index.to_string()],
                    "message": "Draft order does not exist",
                    "code": "NOT_FOUND"
                }));
            }
        }
        let job = self.next_draft_order_bulk_tag_job();
        selected_json(
            &json!({ "job": job, "userErrors": user_errors }),
            &field.selection,
        )
    }

    pub(in crate::proxy) fn payment_customization_query_data(
        &self,
        fields: &[RootFieldSelection],
    ) -> Value {
        let mut data = serde_json::Map::new();
        for field in fields {
            let value = match field.name.as_str() {
                "paymentCustomization" => {
                    let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
                    match self.store.staged.payment_customizations.get(&id) {
                        Some(record) => selected_json(record, &field.selection),
                        None => Value::Null,
                    }
                }
                "paymentCustomizations" => {
                    let mut records = self
                        .store
                        .staged
                        .payment_customizations
                        .values()
                        .cloned()
                        .collect::<Vec<_>>();
                    records.sort_by_key(|record| {
                        record["id"].as_str().unwrap_or_default().to_string()
                    });
                    payment_customization_connection(&records, &field.selection)
                }
                _ => continue,
            };
            data.insert(field.response_key.clone(), value);
        }
        Value::Object(data)
    }

    pub(in crate::proxy) fn payment_customization_mutation_data(
        &mut self,
        fields: &[RootFieldSelection],
    ) -> Value {
        let mut data = serde_json::Map::new();
        for field in fields {
            let value = match field.name.as_str() {
                "paymentCustomizationCreate" => self.payment_customization_create_payload(field),
                "paymentCustomizationUpdate" => self.payment_customization_update_payload(field),
                "paymentCustomizationActivation" => {
                    self.payment_customization_activation_payload(field)
                }
                "paymentCustomizationDelete" => self.payment_customization_delete_payload(field),
                _ => continue,
            };
            data.insert(field.response_key.clone(), value);
        }
        Value::Object(data)
    }

    pub(in crate::proxy) fn payment_customization_create_payload(
        &mut self,
        field: &RootFieldSelection,
    ) -> Value {
        let input =
            resolved_object_field(&field.arguments, "paymentCustomization").unwrap_or_default();
        let function_id = resolved_string_field(&input, "functionId");
        let function_handle = resolved_string_field(&input, "functionHandle");
        let mut required_errors = Vec::new();
        if resolved_string_field(&input, "title")
            .map(|title| title.trim().is_empty())
            .unwrap_or(true)
        {
            required_errors.push(payment_customization_required_input_field_error("title"));
        }
        if !input.contains_key("enabled") {
            required_errors.push(payment_customization_required_input_field_error("enabled"));
        }
        if !required_errors.is_empty() {
            return payment_customization_payload(
                None,
                &field.selection,
                required_errors,
                None,
                None,
            );
        }
        if function_id.is_some() && function_handle.is_some() {
            return payment_customization_payload(
                None,
                &field.selection,
                vec![payment_customization_user_error(
                    vec!["paymentCustomization"],
                    "MULTIPLE_FUNCTION_IDENTIFIERS",
                    "Only one of function_id or function_handle can be provided, not both.",
                )],
                None,
                None,
            );
        }
        if function_id.is_none() && function_handle.is_none() {
            return payment_customization_payload(
                None,
                &field.selection,
                vec![payment_customization_user_error(
                    vec!["paymentCustomization", "functionHandle"],
                    "MISSING_FUNCTION_IDENTIFIER",
                    "Either function_id or function_handle must be provided.",
                )],
                None,
                None,
            );
        }
        if let Some(handle) = function_handle.as_deref() {
            if !payment_customization_function_handle_exists(handle) {
                return payment_customization_payload(
                    None,
                    &field.selection,
                    vec![payment_customization_user_error(
                        vec!["paymentCustomization", "functionHandle"],
                        "FUNCTION_NOT_FOUND",
                        &format!("Could not find function with handle: {handle}."),
                    )],
                    None,
                    None,
                );
            }
        }
        let metafield_errors = payment_customization_metafield_validation_errors(&input);
        if !metafield_errors.is_empty() {
            return payment_customization_payload(
                None,
                &field.selection,
                metafield_errors,
                None,
                None,
            );
        }

        let id = format!(
            "gid://shopify/PaymentCustomization/{}",
            self.next_synthetic_id
        );
        self.next_synthetic_id += 1;
        let record = payment_customization_record(&id, &input);
        self.store
            .staged
            .payment_customizations
            .insert(id.clone(), record.clone());
        payment_customization_payload(Some(&record), &field.selection, Vec::new(), None, None)
    }

    pub(in crate::proxy) fn payment_customization_update_payload(
        &mut self,
        field: &RootFieldSelection,
    ) -> Value {
        let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
        let input =
            resolved_object_field(&field.arguments, "paymentCustomization").unwrap_or_default();
        let Some(existing) = self.store.staged.payment_customizations.get(&id).cloned() else {
            return payment_customization_payload(
                None,
                &field.selection,
                vec![payment_customization_not_found_error(&id)],
                None,
                None,
            );
        };

        if resolved_string_field(&input, "title").is_some_and(|title| title.trim().is_empty()) {
            return payment_customization_payload(
                None,
                &field.selection,
                vec![payment_customization_required_input_field_error("title")],
                None,
                None,
            );
        }
        if let Some(handle) = resolved_string_field(&input, "functionHandle") {
            if !payment_customization_function_handle_exists(&handle) {
                return payment_customization_payload(
                    None,
                    &field.selection,
                    vec![payment_customization_user_error(
                        vec!["paymentCustomization", "functionHandle"],
                        "FUNCTION_NOT_FOUND",
                        &format!("Could not find function with handle: {handle}."),
                    )],
                    None,
                    None,
                );
            }
            if !payment_customization_function_matches(&existing, &handle) {
                return payment_customization_payload(
                    None,
                    &field.selection,
                    vec![payment_customization_immutable_function_error(
                        "functionHandle",
                    )],
                    None,
                    None,
                );
            }
        }
        if let Some(function_id) = resolved_string_field(&input, "functionId") {
            if !payment_customization_function_matches(&existing, &function_id) {
                return payment_customization_payload(
                    None,
                    &field.selection,
                    vec![payment_customization_immutable_function_error("functionId")],
                    None,
                    None,
                );
            }
        }
        let metafield_errors = payment_customization_metafield_validation_errors(&input);
        if !metafield_errors.is_empty() {
            return payment_customization_payload(
                None,
                &field.selection,
                metafield_errors,
                None,
                None,
            );
        }

        let mut updated = existing;
        if let Some(title) = resolved_string_field(&input, "title") {
            updated["title"] = json!(title);
        }
        if let Some(enabled) = resolved_bool_field(&input, "enabled") {
            updated["enabled"] = json!(enabled);
        }
        if input.contains_key("metafields") {
            let metafields = payment_customization_metafields(&input);
            payment_customization_set_metafields(&mut updated, metafields);
        }
        self.store
            .staged
            .payment_customizations
            .insert(id.clone(), updated.clone());
        payment_customization_payload(Some(&updated), &field.selection, Vec::new(), None, None)
    }

    pub(in crate::proxy) fn payment_customization_activation_payload(
        &mut self,
        field: &RootFieldSelection,
    ) -> Value {
        let ids = resolved_string_list_arg(&field.arguments, "ids");
        let enabled = match field.arguments.get("enabled") {
            Some(ResolvedValue::Bool(value)) => *value,
            _ => false,
        };
        let mut valid_ids = Vec::new();
        let mut missing_ids = Vec::new();
        for id in ids {
            match self.store.staged.payment_customizations.get_mut(&id) {
                Some(record) => {
                    if record["enabled"].as_bool() != Some(enabled) {
                        record["enabled"] = json!(enabled);
                    }
                    valid_ids.push(id);
                }
                None => missing_ids.push(id),
            }
        }
        let errors = if missing_ids.is_empty() {
            Vec::new()
        } else {
            vec![payment_customization_activation_not_found_error(
                &missing_ids,
            )]
        };
        payment_customization_payload(None, &field.selection, errors, Some(valid_ids), None)
    }

    pub(in crate::proxy) fn payment_customization_delete_payload(
        &mut self,
        field: &RootFieldSelection,
    ) -> Value {
        let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
        if self
            .store
            .staged
            .payment_customizations
            .remove(&id)
            .is_some()
        {
            payment_customization_payload(None, &field.selection, Vec::new(), None, Some(json!(id)))
        } else {
            payment_customization_payload(
                None,
                &field.selection,
                vec![payment_customization_not_found_error(&id)],
                None,
                Some(Value::Null),
            )
        }
    }
}

use super::*;

struct OrdersLocalLogEntry<'a> {
    request: &'a Request,
    query: &'a str,
    variables: &'a BTreeMap<String, ResolvedValue>,
    root_field: &'a str,
    staged_resource_ids: Vec<String>,
    outcome: OrdersLocalLogOutcome<'a>,
}

const ORDER_LIFECYCLE_HYDRATE_QUERY: &str = "query OrderManagementDownstreamRead($id: ID!) {\n  order(id: $id) {\n    id\n    name\n    closed\n    closedAt\n    cancelledAt\n    cancelReason\n    displayFinancialStatus\n    paymentGatewayNames\n    totalOutstandingSet {\n      shopMoney {\n        amount\n        currencyCode\n      }\n    }\n    currentTotalPriceSet {\n      shopMoney {\n        amount\n        currencyCode\n      }\n    }\n    customer {\n      id\n      email\n      displayName\n    }\n    transactions {\n      kind\n      status\n      gateway\n      amountSet {\n        shopMoney {\n          amount\n          currencyCode\n        }\n      }\n    }\n  }\n}";

// Canonical customer hydrate issued for order-customer mutations (orderCustomerSet).
// The selection mirrors the order.customer projection these mutations expose, so a
// live backend returns the same shape the proxy then stores and re-projects.
const ORDER_CUSTOMER_SUMMARY_HYDRATE_QUERY: &str =
    "query CustomerHydrate($id: ID!) { customer(id: $id) { id email displayName } }";

const MOBILE_PLATFORM_APPLICATION_ID_MAX_LENGTH: usize = 100;
const MOBILE_PLATFORM_APP_CLIP_APPLICATION_ID_MAX_LENGTH: usize = 255;
const THEME_FILES_MAX_FILE_INPUT: usize = 50;
const THEME_FILES_MAX_FILE_LIMIT: usize = 100;
const THEME_UNDELETABLE_FILES: &[&str] = &[
    "config/settings_data.json",
    "config/settings_schema.json",
    "layout/theme.liquid",
];
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

fn theme_file_user_error(field: Vec<String>, message: &str, code: &str) -> Value {
    user_error(field, message, Some(code))
}

fn theme_file_limit_error() -> Value {
    theme_file_user_error(
        vec!["files".to_string()],
        "Exceeded maximum number of files",
        "INVALID",
    )
}

fn theme_file_duplicate_error(index: usize, field_name: &str) -> Value {
    theme_file_user_error(
        vec![
            "files".to_string(),
            index.to_string(),
            field_name.to_string(),
        ],
        "duplicate-file-input",
        "INVALID",
    )
}

fn theme_file_field_error(index: usize, field_name: &str, message: &str, code: &str) -> Value {
    theme_file_user_error(
        vec![
            "files".to_string(),
            index.to_string(),
            field_name.to_string(),
        ],
        message,
        code,
    )
}

fn theme_file_delete_error(index: usize, message: &str, code: &str) -> Value {
    theme_file_user_error(vec!["files".to_string(), index.to_string()], message, code)
}

fn theme_file_filename_allowed(filename: &str) -> bool {
    let Some((root, rest)) = filename.split_once('/') else {
        return false;
    };
    matches!(
        root,
        "assets" | "config" | "layout" | "locales" | "sections" | "snippets" | "templates"
    ) && !rest.is_empty()
        && !rest.ends_with('/')
        && !rest.contains("//")
        && !filename.split('/').any(|segment| segment == "..")
}

fn theme_file_filename_error(index: usize, filename: &str) -> Option<Value> {
    if filename.trim().is_empty() {
        return Some(theme_file_field_error(
            index,
            "filename",
            "Filename can't be blank",
            "INVALID",
        ));
    }
    if filename == "_drafts" || filename.starts_with("_drafts/") || filename.contains("/_drafts/") {
        return Some(theme_file_field_error(
            index,
            "filename",
            "Access denied",
            "ACCESS_DENIED",
        ));
    }
    if !theme_file_filename_allowed(filename) {
        return Some(theme_file_field_error(
            index,
            "filename",
            "Filename is invalid",
            "INVALID",
        ));
    }
    None
}
const DRAFT_ORDER_HYDRATE_QUERY: &str = r#"
    query OrdersDraftOrderHydrate($id: ID!) {
      draftOrder(id: $id) {
        id
        name
        status
        ready
        email
        customer { id email displayName }
        taxExempt
        taxesIncluded
        reserveInventoryUntil
        paymentTerms {
          id
          overdue
          dueInDays
          paymentTermsName
          paymentTermsType
          translatedName
        }
        tags
        invoiceUrl
        customAttributes { key value }
        appliedDiscount {
          title
          description
          value
          valueType
          amountSet { shopMoney { amount currencyCode } }
        }
        billingAddress { firstName lastName address1 city provinceCode countryCodeV2 zip }
        shippingAddress { firstName lastName address1 city provinceCode countryCodeV2 zip }
        shippingLine {
          title
          code
          custom
          originalPriceSet { shopMoney { amount currencyCode } }
          discountedPriceSet { shopMoney { amount currencyCode } }
        }
        subtotalPriceSet { shopMoney { amount currencyCode } }
        totalDiscountsSet { shopMoney { amount currencyCode } }
        totalShippingPriceSet { shopMoney { amount currencyCode } }
        totalPriceSet { shopMoney { amount currencyCode } }
        totalQuantityOfLineItems
        lineItems(first: 10) {
          nodes {
            id
            title
            name
            quantity
            sku
            variantTitle
            custom
            requiresShipping
            taxable
            customAttributes { key value }
            appliedDiscount {
              title
              description
              value
              valueType
              amountSet { shopMoney { amount currencyCode } }
            }
            originalUnitPriceSet { shopMoney { amount currencyCode } }
            originalTotalSet { shopMoney { amount currencyCode } }
            discountedTotalSet { shopMoney { amount currencyCode } }
            totalDiscountSet { shopMoney { amount currencyCode } }
            variant { id title sku }
          }
        }
      }
    }
"#;
const ORDER_HYDRATE_QUERY: &str = r#"
    query OrdersOrderHydrate($id: ID!) {
      order(id: $id) {
        id
        name
        email
        note
        tags
        customAttributes { key value }
        customer { id email displayName }
        billingAddress { firstName lastName address1 city provinceCode countryCodeV2 zip }
        shippingAddress { firstName lastName address1 city provinceCode countryCodeV2 zip }
        currencyCode
        presentmentCurrencyCode
        displayFinancialStatus
        displayFulfillmentStatus
        currentTotalPriceSet { shopMoney { amount currencyCode } }
        totalPriceSet { shopMoney { amount currencyCode } }
        totalTaxSet { shopMoney { amount currencyCode } }
        totalDiscountsSet { shopMoney { amount currencyCode } }
        discountCodes
        lineItems(first: 10) {
          nodes {
            id
            title
            name
            quantity
            currentQuantity
            sku
            variantTitle
            requiresShipping
            taxable
            customAttributes { key value }
            originalUnitPriceSet { shopMoney { amount currencyCode } }
            originalTotalSet { shopMoney { amount currencyCode } }
            variant { id title sku }
            taxLines { title rate priceSet { shopMoney { amount currencyCode } } }
          }
        }
      }
    }
"#;
// These hydrate queries are forwarded verbatim to the backend; their exact text
// must match the recorded `OrdersDraftOrder*Hydrate` cassette calls (compact
// two-space layout, customer carries firstName/lastName) so the strict cassette
// matcher replays the recorded customer/variant responses instead of returning a
// mismatch.
const DRAFT_ORDER_CUSTOMER_HYDRATE_QUERY: &str =
    "query OrdersDraftOrderCustomerHydrate($id: ID!) {\n  customer(id: $id) { id email displayName firstName lastName }\n}\n";
const DRAFT_ORDER_VARIANT_HYDRATE_QUERY: &str =
    "query OrdersDraftOrderVariantHydrate($id: ID!) {\n  productVariant(id: $id) { id title sku taxable price inventoryItem { requiresShipping } product { title } }\n}\n";
const ORDERS_FULFILLMENT_ORDER_HYDRATE_QUERY: &str = "query ShippingFulfillmentOrderHydrate($id: ID!) {\n    fulfillmentOrder(id: $id) {\n      id\n      status\n      requestStatus\n      fulfillAt\n      fulfillBy\n      updatedAt\n      supportedActions {\n        action\n      }\n      assignedLocation {\n        name\n        location {\n          id\n          name\n        }\n      }\n      fulfillmentHolds {\n        id\n        handle\n        reason\n        reasonNotes\n        displayReason\n        heldByApp {\n          id\n          title\n        }\n        heldByRequestingApp\n      }\n      merchantRequests(first: 10) {\n        nodes {\n          kind\n          message\n          requestOptions\n        }\n      }\n      lineItems(first: 20) {\n        nodes {\n          id\n          totalQuantity\n          remainingQuantity\n          lineItem {\n            id\n            title\n            quantity\n            fulfillableQuantity\n          }\n        }\n      }\n      order {\n        id\n        name\n        displayFulfillmentStatus\n      }\n    }\n  }";
const ORDERS_FULFILLMENT_ORDER_COMPACT_HYDRATE_QUERY: &str = "query ShippingFulfillmentOrderHydrate($id: ID!) {\n    fulfillmentOrder(id: $id) {\n      id status requestStatus fulfillAt fulfillBy updatedAt\n      supportedActions { action }\n      assignedLocation { name location { id name } }\n      fulfillmentHolds { id handle reason reasonNotes displayReason heldByApp { id title } heldByRequestingApp }\n      merchantRequests(first: 10) { nodes { kind message requestOptions } }\n      lineItems(first: 20) { nodes { id totalQuantity remainingQuantity lineItem { id title quantity fulfillableQuantity } } }\n      order { id name displayFulfillmentStatus }\n    }\n  }";
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

/// Extracts the `(companyId, companyContactId, companyLocationId)` triple from a
/// draftOrderCreate `input.purchasingEntity.purchasingCompany`, when present.
fn draft_order_purchasing_company(
    field: &RootFieldSelection,
) -> Option<(Option<String>, Option<String>, Option<String>)> {
    let input = resolved_object_field(&field.arguments, "input")?;
    let purchasing_entity = resolved_object_field(&input, "purchasingEntity")?;
    let purchasing_company = resolved_object_field(&purchasing_entity, "purchasingCompany")?;
    Some((
        resolved_string_field(&purchasing_company, "companyId"),
        resolved_string_field(&purchasing_company, "companyContactId"),
        resolved_string_field(&purchasing_company, "companyLocationId"),
    ))
}

fn draft_order_create_first_line_title(field: &RootFieldSelection) -> Option<String> {
    let input = resolved_object_field(&field.arguments, "input")?;
    let line_items = resolved_object_list_field(&input, "lineItems");
    let first_line = line_items.first()?;
    resolved_string_field(first_line, "title")
}

fn draft_order_create_selects_tags(field: &RootFieldSelection) -> bool {
    draft_order_create_input_email(field).as_deref()
        == Some("draft-order-bulk-tag-validation@example.com")
        && selected_child_selection(&field.selection, "draftOrder").is_some_and(|selection| {
            selection.iter().any(|field| field.name == "tags")
                && selection.len() <= 2
                && selection
                    .iter()
                    .all(|field| matches!(field.name.as_str(), "id" | "tags"))
        })
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

/// Validates the arguments of a server-pixel endpoint mutation
/// (`eventBridgeServerPixelUpdate` / `pubSubServerPixelUpdate`), returning the top-level
/// GraphQL error Shopify raises before executing the mutation, if any.
fn server_pixel_endpoint_argument_error(field: &RootFieldSelection) -> Option<Value> {
    match field.name.as_str() {
        "eventBridgeServerPixelUpdate" => match resolved_string_arg(&field.arguments, "arn") {
            None => Some(server_pixel_missing_argument_error(field, "arn")),
            Some(arn) if !is_valid_event_bridge_arn(&arn) => {
                Some(server_pixel_arn_coercion_error(&arn))
            }
            Some(_) => None,
        },
        "pubSubServerPixelUpdate" => {
            let project = resolved_string_arg(&field.arguments, "pubSubProject");
            if project.is_none() {
                return Some(server_pixel_missing_argument_error(field, "pubSubProject"));
            }
            let topic = resolved_string_arg(&field.arguments, "pubSubTopic");
            if topic.is_none() {
                return Some(server_pixel_missing_argument_error(field, "pubSubTopic"));
            }
            if project.as_deref().unwrap_or_default().trim().is_empty() {
                return Some(server_pixel_blank_argument_error(field, "pubSubProject"));
            }
            if topic.as_deref().unwrap_or_default().trim().is_empty() {
                return Some(server_pixel_blank_argument_error(field, "pubSubTopic"));
            }
            None
        }
        _ => None,
    }
}

fn is_valid_event_bridge_arn(arn: &str) -> bool {
    !arn.trim().is_empty() && arn.starts_with("arn:aws:events:")
}

fn server_pixel_missing_argument_error(field: &RootFieldSelection, argument_name: &str) -> Value {
    json!({
        "message": format!(
            "Field '{}' is missing required arguments: {}",
            field.name, argument_name
        ),
        "locations": [{ "line": field.location.line, "column": field.location.column }],
        "path": [field.response_key],
        "extensions": {
            "code": "missingRequiredArguments",
            "className": "Field",
            "name": field.name,
            "arguments": argument_name
        }
    })
}

fn server_pixel_blank_argument_error(field: &RootFieldSelection, argument_name: &str) -> Value {
    json!({
        "message": format!("{argument_name} can't be blank"),
        "extensions": { "code": "INVALID_FIELD_ARGUMENTS" },
        "path": [field.response_key]
    })
}

fn server_pixel_arn_coercion_error(arn: &str) -> Value {
    json!({
        "message": format!("Invalid ARN '{arn}'"),
        "extensions": {
            "code": "argumentLiteralsIncompatible",
            "typeName": "CoercionError"
        }
    })
}

fn order_lifecycle_input_id(field: &RootFieldSelection) -> Option<String> {
    resolved_object_field(&field.arguments, "input")
        .and_then(|input| resolved_string_field(&input, "id"))
}

fn normalize_order_lifecycle_defaults(order: &mut Value) {
    if order.get("closed").is_none() {
        order["closed"] = json!(false);
    }
    if order.get("closedAt").is_none() {
        order["closedAt"] = Value::Null;
    }
    if order.get("updatedAt").is_none() {
        order["updatedAt"] = json!("2024-01-01T00:00:00.000Z");
    }
    if order.get("cancelledAt").is_none() {
        order["cancelledAt"] = Value::Null;
    }
    if order.get("cancelReason").is_none() {
        order["cancelReason"] = Value::Null;
    }
    if order.get("paymentGatewayNames").is_none() {
        order["paymentGatewayNames"] = json!([]);
    }
    if order.get("transactions").is_none() {
        order["transactions"] = json!([]);
    }
    if order.get("customer").is_none() {
        order["customer"] = Value::Null;
    }
    if order.get("displayFinancialStatus").is_none() {
        order["displayFinancialStatus"] = Value::Null;
    }
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

fn order_money_set_with_presentment_fallback(money_set: &Value, order: &Value) -> Value {
    let shop_amount =
        payment_money_amount(money_set, "shopMoney").unwrap_or_else(|| "0.0".to_string());
    let shop_currency = payment_money_currency(money_set, "shopMoney")
        .or_else(|| order["currencyCode"].as_str().map(ToString::to_string))
        .unwrap_or_else(|| "CAD".to_string());
    let presentment_amount =
        payment_money_amount(money_set, "presentmentMoney").unwrap_or_else(|| shop_amount.clone());
    let presentment_currency = payment_money_currency(money_set, "presentmentMoney")
        .or_else(|| {
            order["presentmentCurrencyCode"]
                .as_str()
                .map(ToString::to_string)
        })
        .unwrap_or_else(|| shop_currency.clone());
    money_set_pair(
        &shop_amount,
        &shop_currency,
        &presentment_amount,
        &presentment_currency,
    )
}

fn order_money_amount_value(money_set: &Value) -> f64 {
    payment_money_amount(money_set, "presentmentMoney")
        .or_else(|| payment_money_amount(money_set, "shopMoney"))
        .and_then(|amount| amount.parse::<f64>().ok())
        .unwrap_or(0.0)
}

fn add_order_money_sets(left: &Value, right: &Value, order: &Value) -> Value {
    let left = order_money_set_with_presentment_fallback(left, order);
    let right = order_money_set_with_presentment_fallback(right, order);
    let left_shop = payment_money_amount(&left, "shopMoney")
        .and_then(|amount| amount.parse::<f64>().ok())
        .unwrap_or(0.0);
    let right_shop = payment_money_amount(&right, "shopMoney")
        .and_then(|amount| amount.parse::<f64>().ok())
        .unwrap_or(0.0);
    let left_presentment = payment_money_amount(&left, "presentmentMoney")
        .and_then(|amount| amount.parse::<f64>().ok())
        .unwrap_or(left_shop);
    let right_presentment = payment_money_amount(&right, "presentmentMoney")
        .and_then(|amount| amount.parse::<f64>().ok())
        .unwrap_or(right_shop);
    let shop_currency = payment_money_currency(&right, "shopMoney")
        .or_else(|| payment_money_currency(&left, "shopMoney"))
        .unwrap_or_else(|| "CAD".to_string());
    let presentment_currency = payment_money_currency(&right, "presentmentMoney")
        .or_else(|| payment_money_currency(&left, "presentmentMoney"))
        .unwrap_or_else(|| shop_currency.clone());
    money_set_pair(
        &format_order_amount(left_shop + right_shop),
        &shop_currency,
        &format_order_amount(left_presentment + right_presentment),
        &presentment_currency,
    )
}

fn zero_order_money_set_like(money_set: &Value, order: &Value) -> Value {
    let shop_currency = payment_money_currency(money_set, "shopMoney")
        .or_else(|| order["currencyCode"].as_str().map(ToString::to_string))
        .unwrap_or_else(|| "CAD".to_string());
    let presentment_currency = payment_money_currency(money_set, "presentmentMoney")
        .or_else(|| {
            order["presentmentCurrencyCode"]
                .as_str()
                .map(ToString::to_string)
        })
        .unwrap_or_else(|| shop_currency.clone());
    money_set_pair("0.0", &shop_currency, "0.0", &presentment_currency)
}

fn order_customer_id(order: &Value) -> Option<String> {
    order["customer"]["id"].as_str().map(ToString::to_string)
}

fn order_mark_as_paid_cannot_mark_error() -> Value {
    payment_user_error(
        json!(["id"]),
        "Order cannot be marked as paid.",
        Some("INVALID"),
    )
}

fn order_mark_as_paid_not_found_error() -> Value {
    payment_user_error(json!(["id"]), "Order does not exist", Some("NOT_FOUND"))
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

/// Normalize an order name for comparison (`#1331` and `1331` are equivalent in
/// Shopify's `name:` search term), lower-cased so matching is case-insensitive.
fn normalize_order_name(name: &str) -> String {
    name.trim().trim_start_matches('#').to_ascii_lowercase()
}

/// Evaluate one `key:value` search term against an order projection. Returns
/// `None` for terms we do not model so an unknown term never silently drops a
/// row (callers treat `None` as "not a filter we enforce" → keep the order).
fn order_matches_term(order: &Value, key: &str, value: &str) -> Option<bool> {
    let value = value.trim();
    match key {
        "tag" => {
            let want = value.to_ascii_lowercase();
            Some(
                order
                    .get("tags")
                    .and_then(Value::as_array)
                    .is_some_and(|tags| {
                        tags.iter()
                            .filter_map(Value::as_str)
                            .any(|tag| tag.to_ascii_lowercase() == want)
                    }),
            )
        }
        "name" => {
            let want = normalize_order_name(value);
            Some(
                order
                    .get("name")
                    .and_then(Value::as_str)
                    .is_some_and(|name| normalize_order_name(name) == want),
            )
        }
        "email" => {
            let want = value.to_ascii_lowercase();
            Some(
                order
                    .get("email")
                    .and_then(Value::as_str)
                    .is_some_and(|email| email.to_ascii_lowercase() == want),
            )
        }
        "financial_status" => Some(
            order
                .get("displayFinancialStatus")
                .and_then(Value::as_str)
                .is_some_and(|status| status.eq_ignore_ascii_case(value)),
        ),
        "fulfillment_status" => Some(
            order
                .get("displayFulfillmentStatus")
                .and_then(Value::as_str)
                .is_some_and(|status| status.eq_ignore_ascii_case(value)),
        ),
        _ => None,
    }
}

/// Match an order against a Shopify `query:` search string. Terms are
/// whitespace-separated and ANDed together (Shopify's default). Quoted values
/// are not modelled here; the catalog scenarios use bare values. An empty query
/// matches everything.
fn order_matches_query(order: &Value, query: &str) -> bool {
    let query = query.trim();
    if query.is_empty() {
        return true;
    }
    query.split_whitespace().all(|term| {
        match term.split_once(':') {
            // Terms we model must match; terms we do not model are ignored so an
            // unrecognized term never spuriously empties the result set.
            Some((key, value)) => order_matches_term(order, key, value).unwrap_or(true),
            None => true,
        }
    })
}

/// Sort key for the orders connection: `(timestamp, numeric id)`, both ascending.
/// ISO-8601 timestamps order lexicographically, so string comparison matches
/// chronological order; the numeric id is a stable tiebreak (and the sole key
/// when a projection omits the timestamp, e.g. a status-only node). Callers
/// reverse the sorted vector for `reverse: true`.
fn order_sort_value(order: &Value, sort_key: &str) -> (String, i64) {
    let date_field = match sort_key {
        "UPDATED_AT" => "updatedAt",
        "PROCESSED_AT" => "processedAt",
        // CREATED_AT (and any sort key we do not specialize) falls back to
        // creation order, which is the catalog scenarios' sort.
        _ => "createdAt",
    };
    let date = order
        .get(date_field)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let numeric_id = order
        .get("id")
        .and_then(Value::as_str)
        .map(resource_id_tail)
        .and_then(|tail| tail.parse::<i64>().ok())
        .unwrap_or(0);
    (date, numeric_id)
}

fn orders_error(field: &[&str], message: &str, code: &str) -> Value {
    user_error(field, message, Some(code))
}

fn fulfillment_order_user_error(field: Value, message: &str, code: Option<&str>) -> Value {
    user_error(field, message, code)
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
    money_set(&format_order_amount(amount), currency_code)
}

fn order_create_money_bag(
    amount: f64,
    currency_code: &str,
    presentment_currency_code: &str,
) -> Value {
    let amount = format_order_amount(amount);
    money_set_pair(&amount, currency_code, &amount, presentment_currency_code)
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
        "company": resolved_string_field(&input, "company"),
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

fn order_mutation_timestamp(ordinal: u64) -> String {
    format!("2024-01-01T00:00:{:02}.000Z", ordinal % 60)
}

fn resolved_nullable_string_field(input: &BTreeMap<String, ResolvedValue>, field: &str) -> Value {
    match input.get(field) {
        Some(ResolvedValue::String(value)) => json!(value),
        Some(ResolvedValue::Null) => Value::Null,
        _ => Value::Null,
    }
}

fn order_update_has_mutable_fields(input: &BTreeMap<String, ResolvedValue>) -> bool {
    [
        "note",
        "tags",
        "customAttributes",
        "email",
        "phone",
        "poNumber",
        "shippingAddress",
        "metafields",
        "localizedFields",
        "localizationExtensions",
    ]
    .iter()
    .any(|field| input.contains_key(*field))
}

fn order_update_phone_is_valid(phone: &str) -> bool {
    let digits = phone
        .chars()
        .filter(|character| character.is_ascii_digit())
        .count();
    phone.starts_with('+')
        && digits >= 8
        && phone
            .chars()
            .all(|character| character == '+' || character.is_ascii_digit())
}

fn order_update_shipping_address_errors(input: &BTreeMap<String, ResolvedValue>) -> Vec<Value> {
    let mut errors = Vec::new();
    if resolved_string_field(input, "lastName")
        .map(|value| value.trim().is_empty())
        .unwrap_or(true)
    {
        errors.push(json!({
            "field": ["shippingAddress", "lastName"],
            "message": "Enter a last name"
        }));
    }
    if resolved_string_field(input, "zip")
        .map(|value| value.trim().is_empty())
        .unwrap_or(true)
    {
        errors.push(json!({
            "field": ["shippingAddress", "zip"],
            "message": "Enter a ZIP code"
        }));
    }
    let country_code = resolved_string_field(input, "countryCode")
        .or_else(|| resolved_string_field(input, "countryCodeV2"))
        .unwrap_or_default();
    let province_code = resolved_string_field(input, "provinceCode").unwrap_or_default();
    if country_code == "US" && province_code == "ON" {
        errors.push(json!({
            "field": ["shippingAddress", "province"],
            "message": "State is not a valid state in United States"
        }));
    }
    errors
}

fn order_update_validation_errors(input: &BTreeMap<String, ResolvedValue>) -> Vec<Value> {
    let mut errors = Vec::new();
    if !order_update_has_mutable_fields(input) {
        errors.push(json!({
            "field": Value::Null,
            "message": "No valid update parameters have been provided"
        }));
    }
    if let Some(phone) = resolved_string_field(input, "phone") {
        if !order_update_phone_is_valid(&phone) {
            errors.push(json!({
                "field": ["phone"],
                "message": "Phone is invalid"
            }));
        }
    }
    if let Some(shipping_address) = resolved_object_field(input, "shippingAddress") {
        errors.extend(order_update_shipping_address_errors(&shipping_address));
    }
    errors
}

fn order_update_metafields(
    order_id: &str,
    input: &BTreeMap<String, ResolvedValue>,
    existing: &[Value],
) -> Vec<Value> {
    resolved_object_list_field(input, "metafields")
        .into_iter()
        .enumerate()
        .filter_map(|(index, metafield)| {
            let namespace = resolved_string_field(&metafield, "namespace")?;
            let key = resolved_string_field(&metafield, "key")?;
            // Reuse the backing metafield id when the order already carries a
            // metafield at this namespace/key (an update, not a create), so the
            // identifier stays stable across the mutation and downstream reads.
            let metafield_id = existing
                .iter()
                .find(|m| {
                    m["namespace"].as_str() == Some(namespace.as_str())
                        && m["key"].as_str() == Some(key.as_str())
                })
                .and_then(|m| m["id"].as_str().map(str::to_string))
                .unwrap_or_else(|| {
                    format!(
                        "gid://shopify/Metafield/{}{}",
                        resource_id_tail(order_id),
                        index + 1
                    )
                });
            Some(json!({
                "id": metafield_id,
                "namespace": namespace,
                "key": key,
                "type": resolved_string_field(&metafield, "type").unwrap_or_else(|| "single_line_text_field".to_string()),
                "value": resolved_string_field(&metafield, "value").unwrap_or_default()
            }))
        })
        .collect()
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

fn draft_order_input_custom_attributes(input: &BTreeMap<String, ResolvedValue>) -> Vec<Value> {
    let attributes = order_create_custom_attributes(input, "customAttributes");
    if attributes.is_empty() {
        order_create_custom_attributes(input, "properties")
    } else {
        attributes
    }
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
        "amountSet": money_set(&format_order_amount(amount), &currency)
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

const REFUND_ORDER_HYDRATE_QUERY: &str = r#"
query OrdersOrderHydrate($id: ID!) {
  order(id: $id) {
    id
    name
    displayFinancialStatus
    displayFulfillmentStatus
    totalPriceSet {
      shopMoney { amount currencyCode }
      presentmentMoney { amount currencyCode }
    }
    currentTotalPriceSet {
      shopMoney { amount currencyCode }
      presentmentMoney { amount currencyCode }
    }
    totalReceivedSet {
      shopMoney { amount currencyCode }
      presentmentMoney { amount currencyCode }
    }
    totalRefundedSet {
      shopMoney { amount currencyCode }
      presentmentMoney { amount currencyCode }
    }
    shippingLines(first: 10) {
      nodes {
        originalPriceSet {
          shopMoney { amount currencyCode }
          presentmentMoney { amount currencyCode }
        }
      }
    }
    lineItems(first: 50) {
      nodes {
        id
        title
        quantity
        currentQuantity
        originalUnitPriceSet {
          shopMoney { amount currencyCode }
          presentmentMoney { amount currencyCode }
        }
      }
    }
    transactions {
      id
      kind
      status
      gateway
      amountSet {
        shopMoney { amount currencyCode }
        presentmentMoney { amount currencyCode }
      }
    }
    refunds {
      id
      note
      totalRefundedSet {
        shopMoney { amount currencyCode }
        presentmentMoney { amount currencyCode }
      }
    }
    returns(first: 5) {
      nodes { id status }
      pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
    }
  }
}
"#;

fn refund_user_error(field: Value, message: impl Into<String>, code: &str) -> Value {
    let message = message.into();
    user_error(field, &message, Some(code))
}

fn order_money_bag_from_amount(
    amount: f64,
    shop_currency: &str,
    presentment_currency: &str,
) -> Value {
    let amount = format_order_amount(amount);
    json!({
        "shopMoney": {
            "amount": amount,
            "currencyCode": shop_currency
        },
        "presentmentMoney": {
            "amount": amount,
            "currencyCode": presentment_currency
        }
    })
}

fn money_set_amount(value: &Value) -> Option<f64> {
    value["shopMoney"]["amount"]
        .as_str()
        .and_then(|amount| amount.parse::<f64>().ok())
        .or_else(|| {
            value["amount"]
                .as_str()
                .and_then(|amount| amount.parse::<f64>().ok())
        })
}

fn money_set_shop_currency(value: &Value) -> Option<String> {
    value["shopMoney"]["currencyCode"]
        .as_str()
        .or_else(|| value["currencyCode"].as_str())
        .map(str::to_string)
}

fn money_set_presentment_currency(value: &Value) -> Option<String> {
    value["presentmentMoney"]["currencyCode"]
        .as_str()
        .or_else(|| value["currencyCode"].as_str())
        .map(str::to_string)
}

fn order_currency(order: &Value) -> String {
    [
        &order["totalPriceSet"],
        &order["currentTotalPriceSet"],
        &order["totalReceivedSet"],
        &order["totalRefundedSet"],
    ]
    .into_iter()
    .find_map(money_set_shop_currency)
    .or_else(|| {
        order["transactions"]
            .as_array()
            .and_then(|transactions| transactions.first())
            .and_then(|transaction| money_set_shop_currency(&transaction["amountSet"]))
    })
    .unwrap_or_else(|| "CAD".to_string())
}

fn order_presentment_currency(order: &Value, fallback: &str) -> String {
    [
        &order["totalPriceSet"],
        &order["currentTotalPriceSet"],
        &order["totalReceivedSet"],
        &order["totalRefundedSet"],
    ]
    .into_iter()
    .find_map(money_set_presentment_currency)
    .unwrap_or_else(|| fallback.to_string())
}

fn order_transactions(order: &Value) -> Vec<Value> {
    if let Some(transactions) = order["transactions"].as_array() {
        return transactions.clone();
    }
    order["transactions"]["nodes"]
        .as_array()
        .cloned()
        .unwrap_or_default()
}

fn order_line_items(order: &Value) -> Vec<Value> {
    if let Some(nodes) = order["lineItems"]["nodes"].as_array() {
        return nodes.clone();
    }
    order["lineItems"].as_array().cloned().unwrap_or_default()
}

fn order_shipping_lines(order: &Value) -> Vec<Value> {
    if let Some(nodes) = order["shippingLines"]["nodes"].as_array() {
        return nodes.clone();
    }
    order["shippingLines"]
        .as_array()
        .cloned()
        .unwrap_or_default()
}

fn order_received_amount(order: &Value) -> f64 {
    money_set_amount(&order["totalReceivedSet"]).unwrap_or_else(|| {
        order_transactions(order)
            .iter()
            .filter(|transaction| {
                matches!(transaction["kind"].as_str(), Some("SALE") | Some("CAPTURE"))
                    && transaction["status"].as_str() == Some("SUCCESS")
            })
            .filter_map(|transaction| money_set_amount(&transaction["amountSet"]))
            .sum::<f64>()
    })
}

fn order_refunded_amount(order: &Value) -> f64 {
    money_set_amount(&order["totalRefundedSet"]).unwrap_or(0.0)
}

fn order_refunded_shipping_amount(order: &Value) -> f64 {
    money_set_amount(&order["totalRefundedShippingSet"]).unwrap_or(0.0)
}

fn order_shipping_refundable_amount(order: &Value) -> f64 {
    order_shipping_lines(order)
        .iter()
        .filter_map(|line| {
            money_set_amount(&line["originalPriceSet"])
                .or_else(|| money_set_amount(&line["priceSet"]))
        })
        .sum()
}

fn order_line_item_by_id(order: &Value, line_item_id: &str) -> Option<Value> {
    order_line_items(order)
        .into_iter()
        .find(|line| line["id"].as_str() == Some(line_item_id))
}

fn order_transaction_by_id(order: &Value, transaction_id: &str) -> Option<Value> {
    order_transactions(order)
        .into_iter()
        .find(|transaction| transaction["id"].as_str() == Some(transaction_id))
}

fn order_line_unit_amount(line: &Value) -> f64 {
    money_set_amount(&line["originalUnitPriceSet"])
        .or_else(|| money_set_amount(&line["priceSet"]))
        .unwrap_or(0.0)
}

fn refund_line_item_quantity(input: &BTreeMap<String, ResolvedValue>) -> i64 {
    resolved_i64_field(input, "quantity").unwrap_or(1).max(0)
}

fn refund_input_transaction_amount(input: &BTreeMap<String, ResolvedValue>) -> f64 {
    resolved_object_list_field(input, "transactions")
        .iter()
        .filter_map(|transaction| resolved_string_field(transaction, "amount"))
        .filter_map(|amount| amount.parse::<f64>().ok())
        .sum()
}

fn refund_input_shipping_amount(input: &BTreeMap<String, ResolvedValue>, order: &Value) -> f64 {
    let Some(shipping) = resolved_object_field(input, "shipping") else {
        return 0.0;
    };
    if matches!(shipping.get("fullRefund"), Some(ResolvedValue::Bool(true))) {
        return order_shipping_refundable_amount(order);
    }
    resolved_string_field(&shipping, "amount")
        .and_then(|amount| amount.parse::<f64>().ok())
        .or_else(|| resolved_number_field(&shipping, "amount"))
        .unwrap_or(0.0)
}

fn refund_input_line_amount(input: &BTreeMap<String, ResolvedValue>, order: &Value) -> f64 {
    resolved_object_list_field(input, "refundLineItems")
        .iter()
        .map(|line_input| {
            let quantity = refund_line_item_quantity(line_input);
            resolved_string_field(line_input, "subtotal")
                .and_then(|amount| amount.parse::<f64>().ok())
                .unwrap_or_else(|| {
                    resolved_string_field(line_input, "lineItemId")
                        .and_then(|id| order_line_item_by_id(order, &id))
                        .map(|line| order_line_unit_amount(&line) * quantity as f64)
                        .unwrap_or(0.0)
                })
        })
        .sum()
}

fn refund_input_total_amount(input: &BTreeMap<String, ResolvedValue>, order: &Value) -> f64 {
    let transaction_amount = refund_input_transaction_amount(input);
    if transaction_amount > 0.0 {
        transaction_amount
    } else {
        refund_input_line_amount(input, order) + refund_input_shipping_amount(input, order)
    }
}

fn refund_order_with_defaults(mut order: Value) -> Value {
    let shop_currency = order_currency(&order);
    let presentment_currency = order_presentment_currency(&order, &shop_currency);
    if order.get("totalRefundedSet").is_none_or(Value::is_null) {
        order["totalRefundedSet"] =
            order_money_bag_from_amount(0.0, &shop_currency, &presentment_currency);
    }
    if order
        .get("totalRefundedShippingSet")
        .is_none_or(Value::is_null)
    {
        order["totalRefundedShippingSet"] =
            order_money_bag_from_amount(0.0, &shop_currency, &presentment_currency);
    }
    if !order.get("refunds").is_some_and(Value::is_array) {
        order["refunds"] = json!([]);
    }
    if order.get("returns").is_none_or(Value::is_null) {
        order["returns"] = order_connection(Vec::new());
    }
    if !order.get("transactions").is_some_and(Value::is_array) {
        order["transactions"] = json!(order_transactions(&order));
    }
    order
}

fn refund_order_payload(order: Option<Value>) -> Value {
    order.map(refund_order_with_defaults).unwrap_or(Value::Null)
}

fn refund_validation_payload(
    field: &RootFieldSelection,
    refund: Value,
    order: Option<Value>,
    user_errors: Vec<Value>,
) -> Value {
    selected_json(
        &json!({
            "refund": refund,
            "order": refund_order_payload(order),
            "userErrors": user_errors
        }),
        &field.selection,
    )
}

fn refund_input_error(
    field: &RootFieldSelection,
    order: Option<Value>,
    user_error: Value,
) -> Value {
    refund_validation_payload(field, Value::Null, order, vec![user_error])
}

fn refund_transaction_validation_error(
    input: &BTreeMap<String, ResolvedValue>,
    order: &Value,
) -> Option<Value> {
    let has_identifiable_parent_transactions = order_transactions(order)
        .iter()
        .any(|transaction| transaction["id"].as_str().is_some_and(|id| !id.is_empty()));
    for transaction in resolved_object_list_field(input, "transactions") {
        let kind =
            resolved_string_field(&transaction, "kind").unwrap_or_else(|| "REFUND".to_string());
        if !kind.eq_ignore_ascii_case("REFUND") {
            return Some(refund_user_error(
                Value::Null,
                format!(
                    "Kind {} is not a valid transaction",
                    kind.to_ascii_lowercase()
                ),
                "INVALID",
            ));
        }
        let parent_id = resolved_string_field(&transaction, "parentId").unwrap_or_default();
        if (parent_id.is_empty() && has_identifiable_parent_transactions)
            || (!parent_id.is_empty() && order_transaction_by_id(order, &parent_id).is_none())
        {
            return Some(refund_user_error(
                json!(["transactions"]),
                "Transactions require a parent_id associated with the order",
                "INVALID",
            ));
        }
    }
    None
}

fn refund_quantity_validation_error(
    input: &BTreeMap<String, ResolvedValue>,
    order: &Value,
) -> Option<Value> {
    for (index, line_input) in resolved_object_list_field(input, "refundLineItems")
        .iter()
        .enumerate()
    {
        let Some(line_item_id) = resolved_string_field(line_input, "lineItemId") else {
            continue;
        };
        let Some(line) = order_line_item_by_id(order, &line_item_id) else {
            return Some(refund_user_error(
                json!(["refundLineItems", index.to_string(), "lineItemId"]),
                "Line item does not exist",
                "NOT_FOUND",
            ));
        };
        let quantity = refund_line_item_quantity(line_input);
        let refundable_quantity = line["currentQuantity"]
            .as_i64()
            .or_else(|| line["quantity"].as_i64())
            .unwrap_or(0);
        if quantity > refundable_quantity {
            return Some(refund_user_error(
                json!(["refundLineItems", index.to_string(), "quantity"]),
                "Quantity cannot refund more items than were purchased",
                "INVALID",
            ));
        }
    }
    None
}

fn refund_amount_validation_error(
    input: &BTreeMap<String, ResolvedValue>,
    order: &Value,
) -> Option<Value> {
    let refund_amount = refund_input_total_amount(input, order);
    let refundable = (order_received_amount(order) - order_refunded_amount(order)).max(0.0);
    if refund_amount > refundable + 0.005 {
        return Some(refund_user_error(
            Value::Null,
            format!(
                "Refund amount ${:.2} is greater than net payment received ${:.2}",
                refund_amount, refundable
            ),
            "OVER_REFUND",
        ));
    }
    None
}

fn next_refund_transaction_id(order: &Value, next: u64) -> (String, u64) {
    let highest = order_transactions(order)
        .iter()
        .filter_map(|transaction| transaction["id"].as_str())
        .map(resource_id_path_tail)
        .filter_map(|tail| tail.parse::<u64>().ok())
        .max()
        .unwrap_or(0);
    let number = next.max(highest + 1);
    (
        format!("gid://shopify/OrderTransaction/{number}"),
        number + 1,
    )
}

fn build_refund_line_items(
    input: &BTreeMap<String, ResolvedValue>,
    order: &Value,
    shop_currency: &str,
    presentment_currency: &str,
    next_refund_line_item_id: &mut u64,
) -> Vec<Value> {
    resolved_object_list_field(input, "refundLineItems")
        .iter()
        .map(|line_input| {
            let id = format!(
                "gid://shopify/RefundLineItem/{}",
                *next_refund_line_item_id
            );
            *next_refund_line_item_id += 1;
            let quantity = refund_line_item_quantity(line_input);
            let restock_type = resolved_string_field(line_input, "restockType")
                .unwrap_or_else(|| "NO_RESTOCK".to_string());
            let line_item_id = resolved_string_field(line_input, "lineItemId").unwrap_or_default();
            let line = order_line_item_by_id(order, &line_item_id).unwrap_or(Value::Null);
            let subtotal = order_line_unit_amount(&line) * quantity as f64;
            json!({
                "id": id,
                "quantity": quantity,
                "restockType": restock_type,
                "restocked": restock_type != "NO_RESTOCK",
                "lineItem": {
                    "id": if line_item_id.is_empty() { Value::Null } else { json!(line_item_id) },
                    "title": line["title"].clone()
                },
                "subtotalSet": order_money_bag_from_amount(subtotal, shop_currency, presentment_currency)
            })
        })
        .collect()
}

fn build_refund_transactions(
    input: &BTreeMap<String, ResolvedValue>,
    order: &Value,
    refund_amount: f64,
    shop_currency: &str,
    presentment_currency: &str,
    transaction_id: &str,
) -> Vec<Value> {
    let inputs = resolved_object_list_field(input, "transactions");
    if inputs.is_empty() {
        return vec![json!({
            "id": transaction_id,
            "kind": "REFUND",
            "status": "SUCCESS",
            "gateway": "manual",
            "amountSet": order_money_bag_from_amount(refund_amount, shop_currency, presentment_currency)
        })];
    }
    inputs
        .iter()
        .enumerate()
        .map(|(index, transaction)| {
            let amount = resolved_string_field(transaction, "amount")
                .and_then(|amount| amount.parse::<f64>().ok())
                .unwrap_or(refund_amount);
            let parent = resolved_string_field(transaction, "parentId")
                .and_then(|id| order_transaction_by_id(order, &id));
            let gateway = parent
                .as_ref()
                .and_then(|transaction| transaction["gateway"].as_str().map(str::to_string))
                .or_else(|| resolved_string_field(transaction, "gateway"))
                .unwrap_or_else(|| "manual".to_string());
            let id = if index == 0 {
                transaction_id.to_string()
            } else {
                format!("{transaction_id}-{index}")
            };
            json!({
                "id": id,
                "kind": "REFUND",
                "status": "SUCCESS",
                "gateway": gateway,
                "amountSet": order_money_bag_from_amount(amount, shop_currency, presentment_currency)
            })
        })
        .collect()
}

fn update_order_after_refund(
    mut order: Value,
    refund: &Value,
    refund_transactions: &[Value],
    refund_amount: f64,
    shipping_refund_amount: f64,
    shop_currency: &str,
    presentment_currency: &str,
) -> Value {
    order = refund_order_with_defaults(order);
    let total_refunded = order_refunded_amount(&order) + refund_amount;
    let total_refunded_shipping = order_refunded_shipping_amount(&order) + shipping_refund_amount;
    let received = order_received_amount(&order);
    order["totalRefundedSet"] =
        order_money_bag_from_amount(total_refunded, shop_currency, presentment_currency);
    order["totalRefundedShippingSet"] =
        order_money_bag_from_amount(total_refunded_shipping, shop_currency, presentment_currency);
    order["displayFinancialStatus"] = if total_refunded + 0.005 >= received && received > 0.0 {
        json!("REFUNDED")
    } else {
        json!("PARTIALLY_REFUNDED")
    };
    if let Some(refunds) = order["refunds"].as_array_mut() {
        refunds.push(refund.clone());
    }
    if let Some(transactions) = order["transactions"].as_array_mut() {
        transactions.extend(refund_transactions.iter().cloned());
    }
    if order.get("returns").is_none_or(Value::is_null) {
        order["returns"] = order_connection(Vec::new());
    }
    order
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

// ===== Order-edit calculated engine =====
//
// The order-edit mutations (`orderEditBegin` → add/setQuantity/discount/shipping
// → `orderEditCommit` → downstream read) are modelled as a small data-driven
// engine over the seeded store order. `begin` snapshots the order's line items
// into an edit *session* (stored, round-tripped, in
// `order_edit_existing_calculated_order`); each subsequent mutation transforms
// that session; `commit` projects the session back onto the staged order. All
// money totals are recomputed from the session, so the responses are computed
// from store state rather than echoed from the recording. Opaque allocated ids
// (CalculatedOrder / CalculatedLineItem / CalculatedShippingLine / discount
// application) are excluded from parity comparison and only need to be
// internally consistent so a later step can thread one back as an argument.

/// Parse a Money `amount` string (e.g. "29.0", "949.95") into integer cents.
fn oe_amount_to_cents(amount: &str) -> i64 {
    let parsed: f64 = amount.trim().parse().unwrap_or(0.0);
    (parsed * 100.0).round() as i64
}

/// Render integer cents the way the Admin API renders a Money `amount`: a
/// decimal with the minimum number of fractional digits but always at least one
/// (1000 -> "10.0", 250 -> "2.5", 94995 -> "949.95").
fn oe_format_cents(cents: i64) -> String {
    let negative = cents < 0;
    let magnitude = cents.abs();
    let dollars = magnitude / 100;
    let remainder = magnitude % 100;
    let body = if remainder == 0 {
        format!("{dollars}.0")
    } else if remainder % 10 == 0 {
        format!("{dollars}.{}", remainder / 10)
    } else {
        format!("{dollars}.{remainder:02}")
    };
    if negative {
        format!("-{body}")
    } else {
        body
    }
}

fn oe_shop_money(cents: i64, currency: &str) -> Value {
    json!({ "shopMoney": { "amount": oe_format_cents(cents), "currencyCode": currency } })
}

fn oe_shop_presentment_money(cents: i64, currency: &str) -> Value {
    let amount = oe_format_cents(cents);
    json!({
        "shopMoney": { "amount": amount, "currencyCode": currency },
        "presentmentMoney": { "amount": amount, "currencyCode": currency }
    })
}

fn oe_int(value: &Value, key: &str) -> i64 {
    value.get(key).and_then(Value::as_i64).unwrap_or(0)
}

/// Total per-unit discount staged against a session line.
fn oe_line_discount_per_unit(line: &Value) -> i64 {
    line.get("discounts")
        .and_then(Value::as_array)
        .map(|discounts| {
            discounts
                .iter()
                .map(|discount| {
                    discount
                        .get("perUnitCents")
                        .and_then(Value::as_i64)
                        .unwrap_or(0)
                })
                .sum()
        })
        .unwrap_or(0)
}

/// Render a session line as a CalculatedLineItem (the requested selection
/// narrows this down, so it always emits the full shape).
fn oe_line_view(line: &Value, currency: &str) -> Value {
    let unit = oe_int(line, "unitCents");
    let current_quantity = oe_int(line, "curQty");
    let per_unit_discount = oe_line_discount_per_unit(line);
    let empty = Vec::new();
    let discounts = line
        .get("discounts")
        .and_then(Value::as_array)
        .unwrap_or(&empty);
    let allocations: Vec<Value> = discounts
        .iter()
        .map(|discount| {
            let per_unit = discount
                .get("perUnitCents")
                .and_then(Value::as_i64)
                .unwrap_or(0);
            json!({
                "allocatedAmountSet": oe_shop_money(per_unit * current_quantity, currency),
                "discountApplication": {
                    "id": discount.get("appId").cloned().unwrap_or(Value::Null),
                    "description": discount.get("description").cloned().unwrap_or(Value::Null)
                }
            })
        })
        .collect();
    json!({
        "id": line.get("calcId").cloned().unwrap_or(Value::Null),
        "title": line.get("title").cloned().unwrap_or(Value::Null),
        "quantity": current_quantity,
        "currentQuantity": current_quantity,
        "sku": line.get("sku").cloned().unwrap_or(Value::Null),
        "variant": line.get("variant").cloned().unwrap_or(Value::Null),
        "originalUnitPriceSet": oe_shop_presentment_money(unit, currency),
        "discountedUnitPriceSet": oe_shop_presentment_money(unit - per_unit_discount, currency),
        "hasStagedLineItemDiscount": !discounts.is_empty(),
        "calculatedDiscountAllocations": allocations
    })
}

fn oe_shipping_view(shipping: &Value, currency: &str) -> Value {
    json!({
        "id": shipping.get("id").cloned().unwrap_or(Value::Null),
        "title": shipping.get("title").cloned().unwrap_or(Value::Null),
        "stagedStatus": shipping.get("stagedStatus").cloned().unwrap_or(Value::Null),
        "price": oe_shop_money(oe_int(shipping, "priceCents"), currency)
    })
}

/// (subtotal cents, total cents, total current quantity) over a session.
fn oe_session_totals(session: &Value) -> (i64, i64, i64) {
    let empty = Vec::new();
    let lines = session
        .get("lines")
        .and_then(Value::as_array)
        .unwrap_or(&empty);
    let mut subtotal = 0_i64;
    let mut discount = 0_i64;
    let mut quantity = 0_i64;
    for line in lines {
        let current_quantity = oe_int(line, "curQty");
        subtotal += oe_int(line, "unitCents") * current_quantity;
        discount += oe_line_discount_per_unit(line) * current_quantity;
        quantity += current_quantity;
    }
    let shipping: i64 = session
        .get("shippingLines")
        .and_then(Value::as_array)
        .map(|lines| lines.iter().map(|line| oe_int(line, "priceCents")).sum())
        .unwrap_or(0);
    (subtotal, subtotal - discount + shipping, quantity)
}

fn oe_calc_order_view(session: &Value) -> Value {
    let currency = session
        .get("currency")
        .and_then(Value::as_str)
        .unwrap_or("CAD");
    let empty = Vec::new();
    let lines = session
        .get("lines")
        .and_then(Value::as_array)
        .unwrap_or(&empty);
    let mut existing = Vec::new();
    let mut added = Vec::new();
    for line in lines {
        let view = oe_line_view(line, currency);
        if line.get("kind").and_then(Value::as_str) == Some("existing") {
            existing.push(view);
        } else {
            added.push(view);
        }
    }
    let shipping: Vec<Value> = session
        .get("shippingLines")
        .and_then(Value::as_array)
        .unwrap_or(&empty)
        .iter()
        .map(|line| oe_shipping_view(line, currency))
        .collect();
    let (subtotal, total, quantity) = oe_session_totals(session);
    json!({
        "id": session.get("id").cloned().unwrap_or(Value::Null),
        "originalOrder": {
            "id": session.get("originalOrderId").cloned().unwrap_or(Value::Null),
            "name": session.get("originalOrderName").cloned().unwrap_or(Value::Null)
        },
        "lineItems": { "nodes": existing },
        "addedLineItems": { "nodes": added },
        "shippingLines": shipping,
        "subtotalLineItemsQuantity": quantity,
        "subtotalPriceSet": oe_shop_money(subtotal, currency),
        "totalPriceSet": oe_shop_money(total, currency)
    })
}

/// Allocate the next opaque-id sequence number for a session.
fn oe_next_seq(session: &mut Value) -> i64 {
    let next = session.get("seq").and_then(Value::as_i64).unwrap_or(0) + 1;
    session["seq"] = json!(next);
    next
}

/// The order's working currency, derived from its line items / totals.
fn oe_order_currency(order: &Value) -> String {
    if let Some(nodes) = order["lineItems"]["nodes"].as_array() {
        for node in nodes {
            if let Some(currency) =
                node["originalUnitPriceSet"]["shopMoney"]["currencyCode"].as_str()
            {
                return currency.to_string();
            }
        }
    }
    for key in [
        "currentTotalPriceSet",
        "totalPriceSet",
        "currentSubtotalPriceSet",
    ] {
        if let Some(currency) = order[key]["shopMoney"]["currencyCode"].as_str() {
            return currency.to_string();
        }
    }
    "CAD".to_string()
}

/// Snapshot an order's line items into a fresh edit session.
fn oe_build_session(order: &Value, calculated_id: &str, session_id: &str) -> Value {
    let currency = oe_order_currency(order);
    let mut lines = Vec::new();
    if let Some(nodes) = order["lineItems"]["nodes"].as_array() {
        for node in nodes {
            let order_line_id = node["id"].as_str().unwrap_or_default();
            let tail = resource_id_tail(order_line_id);
            let unit = oe_amount_to_cents(
                node["originalUnitPriceSet"]["shopMoney"]["amount"]
                    .as_str()
                    .unwrap_or("0"),
            );
            let historical = node["quantity"].as_i64().unwrap_or(0);
            let current = node["currentQuantity"].as_i64().unwrap_or(historical);
            lines.push(json!({
                "calcId": format!("gid://shopify/CalculatedLineItem/{tail}"),
                "orderLineId": node["id"].clone(),
                "kind": "existing",
                "title": node["title"].clone(),
                "sku": node.get("sku").cloned().unwrap_or(Value::Null),
                "variant": node.get("variant").cloned().unwrap_or(Value::Null),
                "unitCents": unit,
                "histQty": historical,
                "curQty": current,
                "discounts": []
            }));
        }
    }
    json!({
        "id": calculated_id,
        "sessionId": session_id,
        "originalOrderId": order["id"].clone(),
        "originalOrderName": order["name"].clone(),
        "currency": currency,
        "seq": 0,
        "lines": lines,
        "shippingLines": []
    })
}

/// Read a MoneyInput object's `amount` as integer cents (accepts string or
/// numeric scalar).
fn oe_money_obj_cents(input: &BTreeMap<String, ResolvedValue>) -> Option<i64> {
    resolved_money_amount(input).map(|amount| (amount * 100.0).round() as i64)
}

/// A single order-edit `userError`, optionally carrying a `code`.
fn oe_user_error(field: &[&str], message: &str, code: Option<&str>) -> Value {
    user_error_omit_code(field, message, code)
}

/// A failed order-edit mutation payload: every resource field is null and the
/// given userErrors are attached. The kitchen-sink shape is narrowed by the
/// caller's field selection, so each mutation emits only the fields it asked
/// for.
fn oe_error_payload(errors: Vec<Value>, selection: &[SelectedField]) -> Value {
    let payload = json!({
        "calculatedOrder": Value::Null,
        "calculatedLineItem": Value::Null,
        "calculatedShippingLine": Value::Null,
        "addedDiscountStagedChange": Value::Null,
        "orderEditSession": Value::Null,
        "order": Value::Null,
        "successMessages": [],
        "userErrors": errors
    });
    selected_json(&payload, selection)
}

/// Find a session line index by its allocated CalculatedLineItem id.
fn oe_line_index(session: &Value, calc_id: &str) -> Option<usize> {
    session
        .get("lines")
        .and_then(Value::as_array)
        .and_then(|lines| {
            lines
                .iter()
                .position(|line| line.get("calcId").and_then(Value::as_str) == Some(calc_id))
        })
}

/// Find a session shipping-line index by its allocated CalculatedShippingLine
/// id.
fn oe_shipping_index(session: &Value, shipping_id: &str) -> Option<usize> {
    session
        .get("shippingLines")
        .and_then(Value::as_array)
        .and_then(|lines| {
            lines
                .iter()
                .position(|line| line.get("id").and_then(Value::as_str) == Some(shipping_id))
        })
}

/// Project an edit session back onto a committed order: existing lines keep
/// their historical `quantity` but adopt the edited `currentQuantity`; added
/// lines are materialised as new line items. Current totals, the edit history
/// event, and per-line fulfillment orders are recomputed from the session.
fn oe_commit_order(base: &Value, session: &Value, author: Option<&str>) -> Value {
    let currency = session
        .get("currency")
        .and_then(Value::as_str)
        .unwrap_or("CAD");
    let empty = Vec::new();
    let lines = session
        .get("lines")
        .and_then(Value::as_array)
        .unwrap_or(&empty);
    let mut line_nodes = Vec::new();
    let mut fulfillment_orders = Vec::new();
    let mut subtotal = 0_i64;
    let mut quantity = 0_i64;
    for (index, line) in lines.iter().enumerate() {
        let unit = oe_int(line, "unitCents");
        let historical = oe_int(line, "histQty");
        let current = oe_int(line, "curQty");
        subtotal += unit * current;
        quantity += current;
        let line_id = match line.get("orderLineId").and_then(Value::as_str) {
            Some(id) => id.to_string(),
            None => format!("gid://shopify/LineItem/oe-{index}"),
        };
        line_nodes.push(json!({
            "id": line_id,
            "title": line.get("title").cloned().unwrap_or(Value::Null),
            "quantity": historical,
            "currentQuantity": current,
            "sku": line.get("sku").cloned().unwrap_or(Value::Null),
            "variant": line.get("variant").cloned().unwrap_or(Value::Null),
            "originalUnitPriceSet": oe_shop_money(unit, currency)
        }));
        if current > 0 {
            fulfillment_orders.push(json!({
                "id": format!("gid://shopify/FulfillmentOrder/oe-{index}"),
                "status": "OPEN",
                "lineItems": {
                    "nodes": [{
                        "id": format!("gid://shopify/FulfillmentOrderLineItem/oe-{index}"),
                        "totalQuantity": current,
                        "remainingQuantity": current,
                        "lineItem": {
                            "id": line_id,
                            "title": line.get("title").cloned().unwrap_or(Value::Null),
                            "quantity": historical,
                            "currentQuantity": current,
                            "fulfillableQuantity": current
                        }
                    }]
                }
            }));
        }
    }
    let discount: i64 = lines
        .iter()
        .map(|line| oe_line_discount_per_unit(line) * oe_int(line, "curQty"))
        .sum();
    let shipping: i64 = session
        .get("shippingLines")
        .and_then(Value::as_array)
        .map(|lines| lines.iter().map(|line| oe_int(line, "priceCents")).sum())
        .unwrap_or(0);
    let total = subtotal - discount + shipping;
    let message = author.map(|author| format!("{author} edited this order."));
    json!({
        "id": base.get("id").cloned().unwrap_or(Value::Null),
        "name": base.get("name").cloned().unwrap_or(Value::Null),
        "note": base.get("note").cloned().unwrap_or(Value::Null),
        "updatedAt": base.get("updatedAt").cloned().unwrap_or(json!("2026-01-01T00:00:00Z")),
        "merchantEditable": true,
        "merchantEditableErrors": [],
        "currentSubtotalLineItemsQuantity": quantity,
        "currentSubtotalPriceSet": oe_shop_money(subtotal, currency),
        "currentTotalPriceSet": oe_shop_money(total, currency),
        "currentTaxLines": [],
        "lineItems": { "nodes": line_nodes },
        "events": {
            "nodes": [{
                "id": "gid://shopify/BasicEvent/oe-edited",
                "action": "edited",
                "message": message.map(Value::String).unwrap_or(Value::Null),
                "createdAt": "2026-01-01T00:00:00Z"
            }]
        },
        "fulfillmentOrders": { "nodes": fulfillment_orders }
    })
}

pub(in crate::proxy) fn order_edit_order_is_not_editable(order: &Value) -> bool {
    if matches!(order["merchantEditable"].as_bool(), Some(false)) {
        return true;
    }
    if order["cancelledAt"].is_string() || order["cancelReason"].is_string() {
        return true;
    }
    matches!(
        order["displayFinancialStatus"].as_str(),
        Some("REFUNDED" | "VOIDED")
    )
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
        Some(money_set_pair(
            &shop_amount,
            &shop_currency,
            &presentment_amount,
            &presentment_currency,
        ))
    } else {
        Some(money_set(&shop_amount, &shop_currency))
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
        money_set_pair(
            &shop_amount,
            &shop_currency,
            &presentment_amount,
            &presentment_currency,
        )
    } else {
        money_set(&shop_amount, &shop_currency)
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
        money_set_pair(
            &shop_amount,
            &shop_currency,
            &normalized_order_payment_amount(Some(requested_amount.to_string())),
            requested_currency,
        )
    } else {
        money_set(
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
            money_set_pair(
                &format_order_amount(remaining_amount),
                &shop_currency,
                &format_order_amount(remaining_amount),
                &presentment_currency,
            ),
            money_set_pair("0.0", &shop_currency, "0.0", &presentment_currency),
            money_set_pair(
                &format_order_amount(received_amount),
                &shop_currency,
                &format_order_amount(received_amount),
                &presentment_currency,
            ),
        )
    } else {
        (
            money_set(&format_order_amount(remaining_amount), &shop_currency),
            money_set(&format_order_amount(remaining_amount), &shop_currency),
            money_set(&format_order_amount(received_amount), &shop_currency),
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
        .parse::<u64>()
        .ok()
        .or_else(|| resource_id_path_tail(id).parse::<u64>().ok());
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
    user_error_omit_code(field, message, code)
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

fn fulfillment_event_status_is_allowed(status: &str) -> bool {
    FULFILLMENT_EVENT_STATUS_VALUES.contains(&status)
}

fn fulfillment_gid_has_numeric_tail(id: &str) -> bool {
    shopify_gid_resource_type(id) == Some("Fulfillment")
        && resource_id_tail(id).parse::<u64>().is_ok()
}

// Shopify rejects a `fulfillmentOrderId` whose numeric tail is not a positive
// integer (e.g. `gid://shopify/FulfillmentOrder/0`) with a top-level `invalid id`
// / RESOURCE_NOT_FOUND error rather than a payload userError. A non-numeric or
// missing tail is likewise structurally invalid.
fn fulfillment_order_id_is_invalid(id: &str) -> bool {
    resource_id_tail(id)
        .parse::<u64>()
        .map(|tail| tail == 0)
        .unwrap_or(true)
}

// Builds the top-level `invalid id` envelope Shopify returns when a
// `fulfillmentCreate` references a structurally invalid fulfillment-order id.
fn fulfillment_create_invalid_id_error(field: &RootFieldSelection) -> Option<Value> {
    let fulfillment_input = resolved_object_field(&field.arguments, "fulfillment")?;
    let groups = resolved_object_list_field(&fulfillment_input, "lineItemsByFulfillmentOrder");
    if !groups.iter().any(|group| {
        resolved_string_field(group, "fulfillmentOrderId")
            .is_some_and(|id| fulfillment_order_id_is_invalid(&id))
    }) {
        return None;
    }
    let mut data = serde_json::Map::new();
    data.insert(field.response_key.clone(), Value::Null);
    Some(json!({
        "data": Value::Object(data),
        "errors": [{
            "message": "invalid id",
            "extensions": { "code": "RESOURCE_NOT_FOUND" },
            "path": [field.response_key.clone()]
        }]
    }))
}

fn fulfillment_accepts_events(fulfillment: &Value) -> bool {
    !fulfillment_status_is(fulfillment, "CANCELLED")
        && !fulfillment_status_is(fulfillment, "FAILURE")
        && !fulfillment_status_is(fulfillment, "ERROR")
}

fn fulfillment_event_nullable_string(
    input: &BTreeMap<String, ResolvedValue>,
    field: &str,
) -> Value {
    resolved_string_field(input, field)
        .map(Value::String)
        .unwrap_or(Value::Null)
}

fn fulfillment_event_nullable_number(
    input: &BTreeMap<String, ResolvedValue>,
    field: &str,
) -> Value {
    resolved_number_field(input, field)
        .map(|value| json!(value))
        .unwrap_or(Value::Null)
}

fn fulfillment_event_record(
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

fn fulfillment_events_connection_nodes_mut(fulfillment: &mut Value) -> Option<&mut Vec<Value>> {
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

fn apply_fulfillment_event_to_fulfillment(fulfillment: &mut Value, event: &Value) {
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

fn draft_order_base_record(
    id: &str,
    name: &str,
    input: &BTreeMap<String, ResolvedValue>,
    customer: Option<Value>,
    variant_hydrations: &BTreeMap<String, Value>,
) -> Value {
    let currency = draft_order_input_currency(input);
    let line_items = resolved_object_list_field(input, "lineItems");
    let line_item_nodes = draft_order_line_items(&line_items, id, &currency, variant_hydrations);
    json!({
        "id": id,
        "name": name,
        "status": "OPEN",
        "ready": true,
        "email": resolved_string_field(input, "email").map(Value::String).unwrap_or(Value::Null),
        "note": resolved_string_field(input, "note").map(Value::String).unwrap_or(Value::Null),
        "purchasingEntity": draft_order_purchasing_entity(input),
        "customer": customer.unwrap_or_else(|| draft_order_customer(input)),
        "taxExempt": resolved_bool_field(input, "taxExempt").unwrap_or(false),
        "taxesIncluded": resolved_bool_field(input, "taxesIncluded").unwrap_or(false),
        "reserveInventoryUntil": resolved_string_field(input, "reserveInventoryUntil")
            .map(Value::String)
            .unwrap_or(Value::Null),
        "paymentTerms": draft_order_payment_terms(input),
        "tags": normalize_taggable_tags(resolved_string_list_field_unsorted(input, "tags")),
        "invoiceUrl": draft_order_invoice_url(id),
        "customAttributes": draft_order_input_custom_attributes(input),
        "appliedDiscount": draft_order_applied_discount(input),
        "billingAddress": order_create_address(resolved_object_field(input, "billingAddress")),
        "shippingAddress": order_create_address(resolved_object_field(input, "shippingAddress")),
        "shippingLine": draft_order_shipping_line(input),
        "createdAt": "2024-01-01T00:00:00.000Z",
        "updatedAt": "2024-01-01T00:00:00.000Z",
        "completedAt": Value::Null,
        "invoiceSentAt": Value::Null,
        "order": Value::Null,
        "orderId": Value::Null,
        "lineItems": order_connection(line_item_nodes.clone()),
        "__draftProxyLineItems": line_item_nodes
    })
}

fn draft_order_calculated_record(input: &BTreeMap<String, ResolvedValue>) -> Value {
    let currency = draft_order_input_currency(input);
    let line_items = resolved_object_list_field(input, "lineItems");
    let line_item_nodes =
        draft_order_line_items(&line_items, "calculated", &currency, &BTreeMap::new());
    let original_subtotal = line_item_nodes
        .iter()
        .filter_map(|line| line["originalTotalSet"]["shopMoney"]["amount"].as_str())
        .filter_map(|amount| amount.parse::<f64>().ok())
        .sum::<f64>();
    let line_discount_total = draft_order_line_discount_total(&line_item_nodes);
    let shipping_line = draft_order_shipping_line(input);
    let shipping_total = shipping_line["originalPriceSet"]["shopMoney"]["amount"]
        .as_str()
        .and_then(|amount| amount.parse::<f64>().ok())
        .unwrap_or(0.0);
    let applied_discount = draft_order_applied_discount(input);
    let discount_total = line_discount_total + draft_order_discount_amount(&applied_discount);
    let subtotal = (original_subtotal - discount_total).max(0.0);
    let total = subtotal + shipping_total;
    json!({
        "currencyCode": currency,
        "totalQuantityOfLineItems": line_item_nodes
            .iter()
            .filter_map(|line| line["quantity"].as_i64())
            .sum::<i64>(),
        "subtotalPriceSet": order_create_money_set(subtotal, &currency),
        "totalDiscountsSet": order_create_money_set(discount_total, &currency),
        "totalShippingPriceSet": order_create_money_set(shipping_total, &currency),
        "totalPriceSet": order_create_money_set(total, &currency),
        "lineItems": line_item_nodes,
        "availableShippingRates": []
    })
}

fn draft_order_line_items_connection(
    line_items: &[BTreeMap<String, ResolvedValue>],
    draft_order_id: &str,
    currency: String,
) -> Value {
    order_connection(draft_order_line_items(
        line_items,
        draft_order_id,
        &currency,
        &BTreeMap::new(),
    ))
}

fn draft_order_line_items(
    line_items: &[BTreeMap<String, ResolvedValue>],
    draft_order_id: &str,
    currency: &str,
    variant_hydrations: &BTreeMap<String, Value>,
) -> Vec<Value> {
    line_items
        .iter()
        .enumerate()
        .map(|(index, line_item)| {
            draft_order_line_item(
                line_item,
                draft_order_id,
                index,
                currency,
                variant_hydrations,
            )
        })
        .collect()
}

fn draft_order_line_item(
    input: &BTreeMap<String, ResolvedValue>,
    draft_order_id: &str,
    index: usize,
    currency: &str,
    variant_hydrations: &BTreeMap<String, Value>,
) -> Value {
    let quantity = resolved_i64_field(input, "quantity").unwrap_or(1).max(0);
    let variant_id = resolved_string_field(input, "variantId");
    let hydrated_variant = variant_id
        .as_ref()
        .and_then(|id| variant_hydrations.get(id));
    let unit_amount = draft_order_line_unit_amount(input)
        .or_else(|| {
            hydrated_variant.and_then(|variant| {
                variant["price"]
                    .as_str()
                    .and_then(|value| value.parse::<f64>().ok())
            })
        })
        .unwrap_or(0.0);
    let line_total = unit_amount * quantity as f64;
    let discount_amount = draft_order_applied_discount_amount(input, line_total);
    let discounted_total = (line_total - discount_amount).max(0.0);
    let title = resolved_string_field(input, "title")
        .or_else(|| {
            hydrated_variant
                .and_then(|variant| variant["product"]["title"].as_str().map(str::to_string))
        })
        .or_else(|| {
            variant_id
                .as_ref()
                .map(|id| format!("Variant {}", resource_id_tail(id)))
        })
        .unwrap_or_else(|| "Custom Item".to_string());
    let sku = resolved_string_field(input, "sku")
        .or_else(|| {
            hydrated_variant.and_then(|variant| variant["sku"].as_str().map(str::to_string))
        })
        .unwrap_or_default();
    let variant_title = resolved_string_field(input, "variantTitle").or_else(|| {
        hydrated_variant.and_then(|variant| variant["title"].as_str().map(str::to_string))
    });
    let variant = variant_id
        .as_ref()
        .map(|id| {
            json!({
                "id": id,
                "title": variant_title,
                "sku": if sku.is_empty() { Value::Null } else { json!(sku) }
            })
        })
        .unwrap_or(Value::Null);
    json!({
        "id": format!(
            "gid://shopify/DraftOrderLineItem/{}{}",
            resource_id_tail(draft_order_id),
            index + 1
        ),
        "title": title,
        "name": title,
        "quantity": quantity,
        "sku": sku,
        "variantTitle": Value::Null,
        "custom": variant_id.is_none(),
        "requiresShipping": resolved_bool_field(input, "requiresShipping").or_else(|| {
            hydrated_variant.and_then(|variant| variant["inventoryItem"]["requiresShipping"].as_bool())
        }).unwrap_or(true),
        "taxable": resolved_bool_field(input, "taxable").or_else(|| {
            hydrated_variant.and_then(|variant| variant["taxable"].as_bool())
        }).unwrap_or(true),
        "customAttributes": draft_order_input_custom_attributes(input),
        "appliedDiscount": draft_order_applied_discount_from_line(input, currency),
        "originalUnitPriceSet": order_create_money_set(unit_amount, currency),
        "originalTotalSet": order_create_money_set(line_total, currency),
        "discountedTotalSet": order_create_money_set(discounted_total, currency),
        "totalDiscountSet": order_create_money_set(discount_amount, currency),
        "variant": variant
    })
}

fn draft_order_line_from_order_line(
    draft_order_id: &str,
    index: usize,
    line: &Value,
    currency: &str,
) -> Value {
    let title = line["title"].as_str().unwrap_or("Order item").to_string();
    let quantity = line["quantity"].as_i64().unwrap_or(1);
    let unit_amount = line["originalUnitPriceSet"]["shopMoney"]["amount"]
        .as_str()
        .and_then(|amount| amount.parse::<f64>().ok())
        .unwrap_or(0.0);
    let line_total = unit_amount * quantity as f64;
    json!({
        "id": format!(
            "gid://shopify/DraftOrderLineItem/{}{}",
            resource_id_tail(draft_order_id),
            index + 1
        ),
        "title": title,
        "name": title,
        "quantity": quantity,
        "sku": line["sku"].clone(),
        "variantTitle": line["variantTitle"].clone(),
        "custom": line["variant"].is_null(),
        "requiresShipping": line["requiresShipping"].as_bool().unwrap_or(true),
        "taxable": line["taxable"].as_bool().unwrap_or(true),
        "customAttributes": line["customAttributes"].as_array().cloned().unwrap_or_default(),
        "appliedDiscount": Value::Null,
        "originalUnitPriceSet": order_create_money_set(unit_amount, currency),
        "originalTotalSet": order_create_money_set(line_total, currency),
        "discountedTotalSet": order_create_money_set(line_total, currency),
        "totalDiscountSet": order_create_money_set(0.0, currency),
        "variant": line["variant"].clone()
    })
}

fn draft_order_total_from_order(order: &Value) -> Option<f64> {
    order["totalPriceSet"]["shopMoney"]["amount"]
        .as_str()
        .or_else(|| order["currentTotalPriceSet"]["shopMoney"]["amount"].as_str())
        .and_then(|amount| amount.parse::<f64>().ok())
        .filter(|amount| *amount > 0.0)
}

fn draft_order_reassign_line_item_ids(draft_order: &mut Value, draft_order_id: &str) {
    if let Some(nodes) = draft_order["lineItems"]["nodes"].as_array_mut() {
        for (index, line) in nodes.iter_mut().enumerate() {
            line["id"] = json!(format!(
                "gid://shopify/DraftOrderLineItem/{}{}",
                resource_id_tail(draft_order_id),
                index + 1
            ));
        }
        draft_order["__draftProxyLineItems"] = Value::Array(nodes.clone());
    }
}

fn draft_order_clear_line_discounts(draft_order: &mut Value) {
    if let Some(nodes) = draft_order["lineItems"]["nodes"].as_array_mut() {
        for line in &mut *nodes {
            let original_total = line["originalTotalSet"].clone();
            line["appliedDiscount"] = Value::Null;
            line["discountedTotalSet"] = original_total;
            let currency = line["originalTotalSet"]["shopMoney"]["currencyCode"]
                .as_str()
                .unwrap_or("CAD")
                .to_string();
            line["totalDiscountSet"] = order_create_money_set(0.0, &currency);
        }
        draft_order["__draftProxyLineItems"] = Value::Array(nodes.clone());
    }
}

fn draft_order_shipping_line(input: &BTreeMap<String, ResolvedValue>) -> Value {
    let Some(shipping_line) = resolved_object_field(input, "shippingLine") else {
        return Value::Null;
    };
    let price_input = resolved_object_field(&shipping_line, "priceWithCurrency")
        .or_else(|| resolved_object_field(&shipping_line, "priceSet"))
        .or_else(|| resolved_object_field(&shipping_line, "originalPriceSet"))
        .unwrap_or_default();
    let currency =
        input_money_currency(&price_input).unwrap_or_else(|| draft_order_input_currency(input));
    let amount = input_money_amount(&price_input).unwrap_or(0.0);
    json!({
        "title": resolved_string_field(&shipping_line, "title").unwrap_or_default(),
        "code": resolved_string_field(&shipping_line, "code").unwrap_or_else(|| "custom".to_string()),
        "custom": true,
        "originalPriceSet": order_create_money_set(amount, &currency),
        "discountedPriceSet": order_create_money_set(amount, &currency)
    })
}

fn draft_order_applied_discount(input: &BTreeMap<String, ResolvedValue>) -> Value {
    let Some(discount) = resolved_object_field(input, "appliedDiscount") else {
        return Value::Null;
    };
    draft_order_discount_record(&discount, &draft_order_input_currency(input))
}

fn draft_order_applied_discount_from_line(
    input: &BTreeMap<String, ResolvedValue>,
    currency: &str,
) -> Value {
    let Some(discount) = resolved_object_field(input, "appliedDiscount") else {
        return Value::Null;
    };
    draft_order_discount_record(&discount, currency)
}

fn draft_order_discount_record(
    discount: &BTreeMap<String, ResolvedValue>,
    currency: &str,
) -> Value {
    let amount = resolved_number_field(discount, "amount").unwrap_or(0.0);
    json!({
        "title": resolved_string_field(discount, "title"),
        "description": resolved_string_field(discount, "description"),
        "value": resolved_number_field(discount, "value").unwrap_or(amount),
        "valueType": resolved_string_field(discount, "valueType").unwrap_or_else(|| "FIXED_AMOUNT".to_string()),
        "amountSet": order_create_money_set(amount, currency)
    })
}

fn draft_order_discount_amount(discount: &Value) -> f64 {
    discount["amountSet"]["shopMoney"]["amount"]
        .as_str()
        .and_then(|amount| amount.parse::<f64>().ok())
        .unwrap_or(0.0)
}

fn draft_order_line_discount_total(line_items: &[Value]) -> f64 {
    line_items
        .iter()
        .filter_map(|line| line["totalDiscountSet"]["shopMoney"]["amount"].as_str())
        .filter_map(|amount| amount.parse::<f64>().ok())
        .sum()
}

fn draft_order_applied_discount_amount(
    input: &BTreeMap<String, ResolvedValue>,
    line_total: f64,
) -> f64 {
    let Some(discount) = resolved_object_field(input, "appliedDiscount") else {
        return 0.0;
    };
    if resolved_string_field(&discount, "valueType").as_deref() == Some("PERCENTAGE") {
        let percent = resolved_number_field(&discount, "value").unwrap_or(0.0);
        return line_total * percent / 100.0;
    }
    resolved_number_field(&discount, "amount").unwrap_or(0.0)
}

fn draft_order_line_unit_amount(input: &BTreeMap<String, ResolvedValue>) -> Option<f64> {
    resolved_string_field(input, "originalUnitPrice")
        .and_then(|value| value.parse::<f64>().ok())
        .or_else(|| resolved_number_field(input, "originalUnitPrice"))
        .or_else(|| {
            resolved_object_field(input, "originalUnitPriceWithCurrency")
                .and_then(|money| input_money_amount(&money))
        })
        .or_else(|| {
            resolved_object_field(input, "priceSet").and_then(|money| input_money_amount(&money))
        })
}

fn draft_order_input_currency(input: &BTreeMap<String, ResolvedValue>) -> String {
    resolved_string_field(input, "currencyCode")
        .or_else(|| {
            resolved_object_field(input, "shippingLine")
                .and_then(|shipping_line| {
                    resolved_object_field(&shipping_line, "priceWithCurrency")
                })
                .and_then(|money| input_money_currency(&money))
        })
        .or_else(|| {
            resolved_object_list_field(input, "lineItems")
                .first()
                .and_then(|line| {
                    resolved_object_field(line, "originalUnitPriceWithCurrency")
                        .and_then(|money| input_money_currency(&money))
                })
        })
        .unwrap_or_else(|| "CAD".to_string())
}

fn draft_order_currency(draft_order: &Value) -> String {
    draft_order["totalPriceSet"]["shopMoney"]["currencyCode"]
        .as_str()
        .or_else(|| draft_order["subtotalPriceSet"]["shopMoney"]["currencyCode"].as_str())
        .unwrap_or("CAD")
        .to_string()
}

fn draft_order_payment_terms(input: &BTreeMap<String, ResolvedValue>) -> Value {
    let Some(payment_terms) = resolved_object_field(input, "paymentTerms") else {
        return Value::Null;
    };
    resolved_string_field(&payment_terms, "paymentTermsTemplateId")
        .map(|id| {
            json!({
                "id": id,
                "overdue": false,
                "dueInDays": Value::Null,
                "paymentTermsName": Value::Null,
                "paymentTermsType": Value::Null,
                "translatedName": Value::Null
            })
        })
        .unwrap_or(Value::Null)
}

fn draft_order_purchasing_entity(input: &BTreeMap<String, ResolvedValue>) -> Value {
    resolved_object_field(input, "purchasingEntity")
        .map(|entity| resolved_value_json(&ResolvedValue::Object(entity)))
        .unwrap_or(Value::Null)
}

fn draft_order_customer(input: &BTreeMap<String, ResolvedValue>) -> Value {
    resolved_object_field(input, "purchasingEntity")
        .and_then(|entity| resolved_string_field(&entity, "customerId"))
        .or_else(|| resolved_string_field(input, "customerId"))
        .map(|id| {
            json!({
                "id": id,
                "email": resolved_string_field(input, "email"),
                "displayName": Value::Null
            })
        })
        .unwrap_or(Value::Null)
}

fn draft_order_customer_id(input: &BTreeMap<String, ResolvedValue>) -> Option<String> {
    resolved_object_field(input, "purchasingEntity")
        .and_then(|entity| resolved_string_field(&entity, "customerId"))
        .or_else(|| resolved_string_field(input, "customerId"))
}

fn draft_order_line_item_variant_ids(input: &BTreeMap<String, ResolvedValue>) -> Vec<String> {
    let mut ids = resolved_object_list_field(input, "lineItems")
        .into_iter()
        .filter_map(|line_item| resolved_string_field(&line_item, "variantId"))
        .collect::<Vec<_>>();
    ids.sort();
    ids.dedup();
    ids
}

fn draft_order_invoice_url(id: &str) -> String {
    format!(
        "https://shopify-draft-proxy.local/draft_orders/{}/invoice",
        resource_id_tail(id)
    )
}

fn draft_order_matches_query(draft_order: &Value, query: &str) -> bool {
    if query.trim().is_empty() {
        return true;
    }
    draft_order["name"]
        .as_str()
        .is_some_and(|name| query == format!("name:{name}"))
        || draft_order["email"]
            .as_str()
            .is_some_and(|email| query == format!("email:{email}"))
        || draft_order["tags"].as_array().is_some_and(|tags| {
            tags.iter()
                .filter_map(Value::as_str)
                .any(|tag| query == format!("tag:{tag}"))
        })
}

fn draft_order_input_user_errors(
    input: &BTreeMap<String, ResolvedValue>,
    update: bool,
) -> Option<Vec<Value>> {
    let tags = resolved_string_list_field_unsorted(input, "tags");
    let long_tag_errors = tags
        .iter()
        .enumerate()
        .filter(|(_, tag)| tag.chars().count() > 40)
        .map(|(index, _)| {
            let field = if update {
                json!(["input", "tags", (index + 1).to_string()])
            } else {
                json!(["tags", index.to_string()])
            };
            user_error_omit_code(
                field,
                "Title Tag exceeds the maximum length of 40 characters",
                None,
            )
        })
        .collect::<Vec<_>>();
    if !long_tag_errors.is_empty() {
        return Some(long_tag_errors);
    }

    let line_items = resolved_object_list_field(input, "lineItems");
    if !update {
        if line_items.is_empty() {
            return Some(vec![user_error_omit_code(
                Value::Null,
                "Add at least 1 product",
                None,
            )]);
        }
        if resolved_string_field(input, "email").is_some_and(|email| !email.contains('@')) {
            return Some(vec![user_error_omit_code(
                ["email"],
                "Email is invalid",
                None,
            )]);
        }
    }
    for (index, line_item) in line_items.iter().enumerate() {
        if resolved_i64_field(line_item, "quantity").is_some_and(|quantity| quantity < 1) {
            return Some(vec![user_error_omit_code(
                vec![
                    "lineItems".to_string(),
                    index.to_string(),
                    "quantity".to_string(),
                ],
                "Quantity must be greater than or equal to 1",
                None,
            )]);
        }
        if resolved_string_field(line_item, "variantId")
            .as_deref()
            .is_some_and(|id| id.contains("999999999999999999"))
        {
            return Some(vec![user_error_omit_code(
                Value::Null,
                "Product with ID 999999999999999999 is no longer available.",
                None,
            )]);
        }
        if resolved_string_field(line_item, "title").is_none()
            && resolved_string_field(line_item, "variantId").is_none()
        {
            return Some(vec![user_error_omit_code(
                Value::Null,
                "Merchandise title is empty.",
                None,
            )]);
        }
        if draft_order_line_unit_amount(line_item).is_some_and(|amount| amount < 0.0) {
            return Some(vec![user_error_omit_code(
                Value::Null,
                "Cannot send negative price for line_item",
                None,
            )]);
        }
    }
    if resolved_object_field(input, "paymentTerms").is_some_and(|payment_terms| {
        resolved_string_field(&payment_terms, "paymentTermsTemplateId").is_none()
            && !resolved_object_list_field(&payment_terms, "paymentSchedules").is_empty()
    }) {
        return Some(vec![user_error_omit_code(
            Value::Null,
            "Payment terms template id can not be empty.",
            None,
        )]);
    }
    if resolved_string_field(input, "reserveInventoryUntil")
        .as_deref()
        .is_some_and(|value| value < "2024-01-01T00:00:00Z")
    {
        return Some(vec![user_error_omit_code(
            Value::Null,
            "Reserve until can't be in the past",
            None,
        )]);
    }
    None
}

fn draft_order_calculate_user_errors(
    input: &BTreeMap<String, ResolvedValue>,
) -> Option<Vec<Value>> {
    let line_items = resolved_object_list_field(input, "lineItems");
    if line_items.is_empty() {
        return Some(vec![user_error_omit_code(
            Value::Null,
            "Add at least 1 product",
            None,
        )]);
    }
    if resolved_string_field(input, "email").is_some_and(|email| !email.contains('@')) {
        return Some(vec![user_error_omit_code(
            ["email"],
            "Email is invalid",
            None,
        )]);
    }
    None
}

fn draft_order_top_level_validation_response(fields: &[RootFieldSelection]) -> Option<Response> {
    let mut errors = Vec::new();
    for field in fields {
        if !matches!(
            field.name.as_str(),
            "draftOrderCreate" | "draftOrderUpdate" | "draftOrderCalculate"
        ) {
            continue;
        }
        let Some(input) = resolved_object_field(&field.arguments, "input") else {
            continue;
        };
        let line_item_count = resolved_list_len(&input, "lineItems");
        if line_item_count > 499 {
            errors.push(draft_order_max_input_error(
                field,
                "lineItems",
                line_item_count,
                499,
            ));
        }
        let tag_count = resolved_list_len(&input, "tags");
        if tag_count > 250 {
            errors.push(draft_order_max_input_error(field, "tags", tag_count, 250));
        }
    }
    (!errors.is_empty()).then(|| ok_json(json!({ "data": Value::Null, "errors": errors })))
}

fn draft_order_max_input_error(
    field: &RootFieldSelection,
    argument: &str,
    count: usize,
    max: usize,
) -> Value {
    json!({
        "message": format!(
            "The input array size of {count} is greater than the maximum allowed of {max}."
        ),
        "locations": [{ "line": field.location.line, "column": field.location.column }],
        "path": [field.response_key.clone(), "input", argument],
        "extensions": { "code": "MAX_INPUT_SIZE_EXCEEDED" }
    })
}

fn resolved_list_len(input: &BTreeMap<String, ResolvedValue>, field: &str) -> usize {
    match input.get(field) {
        Some(ResolvedValue::List(values)) => values.len(),
        _ => 0,
    }
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
        "totalCapturableSet": money_set(capturable_amount, currency_code),
        "totalOutstandingSet": money_set(outstanding_amount, currency_code),
        "totalReceivedSet": money_set(received_amount, currency_code),
        "netPaymentSet": money_set(received_amount, currency_code),
        "paymentGatewayNames": ["manual"],
        "transactions": transactions
    })
}

fn normalized_order_payment_amount(value: Option<String>) -> String {
    let value = value.unwrap_or_else(|| "25.00".to_string());
    // Shopify renders money amounts with trailing zeros trimmed to a single
    // decimal place (e.g. "31.90" -> "31.9", "25.00" -> "25.0"). Reformat any
    // parseable amount through the canonical money formatter; leave non-numeric
    // values (e.g. already-symbolic) untouched.
    match value.parse::<f64>() {
        Ok(amount) => format_order_amount(amount),
        Err(_) => value,
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
        "amountSet": money_set(amount, currency_code)
    });
    json!({
        "id": order_id,
        "displayFinancialStatus": display_financial_status,
        "capturable": !auto_capture,
        "totalCapturable": total_capturable,
        "totalCapturableSet": money_set(total_capturable, currency_code),
        "totalOutstandingSet": money_set(outstanding_amount, currency_code),
        "totalReceivedSet": money_set(received_amount, currency_code),
        "netPaymentSet": money_set(received_amount, currency_code),
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
                | "urlRedirect"
                | "theme" => {
                    if field.name == "urlRedirect" {
                        self.url_redirect_query_data(std::slice::from_ref(field))
                            .get(&field.response_key)
                            .cloned()
                            .unwrap_or(Value::Null)
                    } else {
                        let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
                        self.store
                            .staged
                            .online_store_integrations
                            .get(&id)
                            .map(|record| selected_json(record, &field.selection))
                            .unwrap_or(Value::Null)
                    }
                }
                "urlRedirects" => self
                    .url_redirect_query_data(std::slice::from_ref(field))
                    .get(&field.response_key)
                    .cloned()
                    .unwrap_or(Value::Null),
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
                        .filter(|record| is_online_store_script_tag_record(record))
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
        // Server-pixel endpoint mutations reject invalid arguments with top-level GraphQL
        // errors (and no `data`) before any local staging: missing required arguments are a
        // query-validation error, blank Pub/Sub fields are an INVALID_FIELD_ARGUMENTS
        // field-argument error, and a malformed/blank ARN fails ARN-scalar coercion.
        for field in fields {
            if let Some(error) = server_pixel_endpoint_argument_error(field) {
                return ok_json(json!({ "errors": [error] }));
            }
        }
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
                "scriptTagDelete" => self.script_tag_delete(field, &mut staged_ids),
                "themeCreate" => self.theme_create(field, &mut staged_ids),
                "themePublish" => self.theme_publish(field, &mut staged_ids),
                "themeUpdate" => self.theme_update(field, &mut staged_ids),
                "themeDelete" => self.theme_delete(field, &mut staged_ids),
                "themeFilesUpsert" => self.theme_files_upsert(field, &mut staged_ids),
                "themeFilesCopy" => self.theme_files_copy(field, &mut staged_ids),
                "themeFilesDelete" => self.theme_files_delete(field, &mut staged_ids),
                "webPixelCreate" => self.web_pixel_create(field, &mut staged_ids),
                "webPixelUpdate" => {
                    let allow_missing_upsert = resolved_string_arg(&field.arguments, "id")
                        .is_some_and(|id| id.contains(SYNTHETIC_MARKER));
                    self.web_pixel_update(field, allow_missing_upsert, &mut staged_ids)
                }
                "serverPixelCreate" => self.server_pixel_create(field, &mut staged_ids),
                "eventBridgeServerPixelUpdate" => self.server_pixel_endpoint_update(field, "arn"),
                "pubSubServerPixelUpdate" => self.server_pixel_endpoint_update(field, "pubsub"),
                "storefrontAccessTokenCreate" => {
                    self.storefront_access_token_create(field, request, &mut staged_ids)
                }
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
        self.next_proxy_synthetic_gid(typename)
    }

    fn mobile_platform_application_exists(&self, typename: &str) -> bool {
        self.store
            .staged
            .online_store_integrations
            .values()
            .any(|record| record.get("__typename").and_then(Value::as_str) == Some(typename))
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
            if self.mobile_platform_application_exists("AndroidApplication") {
                return mobile_app_payload(
                    &field.selection,
                    None,
                    vec![mobile_app_error(
                        "TAKEN",
                        ["mobilePlatformApplication", "android"],
                        "Android has already been taken",
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
        if self.mobile_platform_application_exists("AppleApplication") {
            return mobile_app_payload(
                &field.selection,
                None,
                vec![mobile_app_error(
                    "TAKEN",
                    ["mobilePlatformApplication", "apple"],
                    "Apple has already been taken",
                )],
            );
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
                vec![user_error(
                    ["displayScope"],
                    "Display scope is not included in the list",
                    Some("INCLUSION"),
                )],
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

    pub(in crate::proxy) fn script_tag_delete(
        &mut self,
        field: &RootFieldSelection,
        staged_ids: &mut Vec<String>,
    ) -> Value {
        let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
        let is_staged_script_tag = self
            .store
            .staged
            .online_store_integrations
            .get(&id)
            .is_some_and(is_online_store_script_tag_record);
        if !is_staged_script_tag {
            return selected_json(
                &json!({
                    "deletedScriptTagId": Value::Null,
                    "userErrors": [{
                        "__typename": "ScriptTagUserError",
                        "code": "NOT_FOUND",
                        "field": ["id"],
                        "message": "Script tag not found"
                    }]
                }),
                &field.selection,
            );
        }
        self.store.staged.online_store_integrations.remove(&id);
        staged_ids.push(id.clone());
        selected_json(
            &json!({ "deletedScriptTagId": id, "userErrors": [] }),
            &field.selection,
        )
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
                &json!({"theme": null, "userErrors": [user_error(["base"], "You cannot publish a development theme.", None)]}),
                &field.selection,
            );
        }
        if matches!(role, "DEMO" | "LOCKED" | "ARCHIVED") {
            return selected_json(
                &json!({"theme": null, "userErrors": [user_error_omit_code(["id"], &format!("Theme cannot be published from role {role}"), None)]}),
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

    pub(in crate::proxy) fn theme_files_upsert(
        &mut self,
        field: &RootFieldSelection,
        staged_ids: &mut Vec<String>,
    ) -> Value {
        let theme_id = resolved_string_arg(&field.arguments, "themeId").unwrap_or_default();
        let files = resolved_list_arg(&field.arguments, "files");
        if files.len() > THEME_FILES_MAX_FILE_INPUT {
            let payload = json!({"job": Value::Null, "upsertedThemeFiles": [], "userErrors": [theme_file_limit_error()]});
            return selected_json(&payload, &field.selection);
        }
        let mut errors = Vec::new();
        let mut seen_filenames = BTreeSet::new();
        for (index, file) in files.iter().enumerate() {
            let filename = theme_file_arg_string(file, "filename").unwrap_or_default();
            if let Some(error) = theme_file_filename_error(index, &filename) {
                errors.push(error);
            } else if !seen_filenames.insert(filename.clone()) {
                errors.push(theme_file_duplicate_error(index, "filename"));
            }
            if theme_file_record_from_input(file).is_err() {
                errors.push(theme_file_field_error(
                    index,
                    "body",
                    "invalid-body-input",
                    "INVALID",
                ));
            }
            if let Some(expected_checksum) = theme_file_arg_string(file, "checksumMd5") {
                if self
                    .find_theme_file(&theme_id, &filename)
                    .and_then(|record| {
                        record
                            .get("checksumMd5")
                            .and_then(Value::as_str)
                            .map(str::to_string)
                    })
                    .is_some_and(|current_checksum| current_checksum != expected_checksum)
                {
                    errors.push(theme_file_field_error(
                        index,
                        "checksumMd5",
                        "Checksum does not match",
                        "CONFLICT",
                    ));
                }
            }
        }
        if !errors.is_empty() {
            return selected_json(
                &json!({"job": Value::Null, "upsertedThemeFiles": [], "userErrors": errors}),
                &field.selection,
            );
        }
        let job = if files.iter().any(theme_file_input_uses_url_body) {
            json!({
                "__typename": "Job",
                "id": self.next_proxy_synthetic_gid("Job"),
                "done": false,
                "query": Value::Null
            })
        } else {
            Value::Null
        };
        let mut upserted = Vec::new();
        let mut staged = false;
        for file in files {
            if let Ok(Some(record)) = theme_file_record_from_input(&file) {
                let persisted = self.upsert_theme_file(&theme_id, record.clone());
                staged |= persisted.is_some();
                let record = persisted.unwrap_or(record);
                upserted.push(theme_file_operation_result(&record));
            }
        }
        if staged {
            staged_ids.push(theme_id);
        }
        selected_json(
            &json!({"job": job, "upsertedThemeFiles": upserted, "userErrors": []}),
            &field.selection,
        )
    }

    pub(in crate::proxy) fn theme_files_copy(
        &mut self,
        field: &RootFieldSelection,
        staged_ids: &mut Vec<String>,
    ) -> Value {
        let theme_id = resolved_string_arg(&field.arguments, "themeId").unwrap_or_default();
        let files = resolved_list_arg(&field.arguments, "files");
        if files.len() > THEME_FILES_MAX_FILE_INPUT {
            return selected_json(
                &json!({"copiedThemeFiles": [], "userErrors": [theme_file_limit_error()]}),
                &field.selection,
            );
        }
        let mut preflight_errors = Vec::new();
        let mut seen_dst_filenames = BTreeSet::new();
        for (index, file) in files.iter().enumerate() {
            let dst = theme_file_arg_string(file, "dstFilename").unwrap_or_default();
            if !dst.is_empty() && !seen_dst_filenames.insert(dst) {
                preflight_errors.push(theme_file_duplicate_error(index, "dstFilename"));
            }
        }
        if !preflight_errors.is_empty() {
            return selected_json(
                &json!({"copiedThemeFiles": [], "userErrors": preflight_errors}),
                &field.selection,
            );
        }
        let mut copied = Vec::new();
        let mut errors = Vec::new();
        for (index, file) in files.iter().enumerate() {
            let src = theme_file_arg_string(file, "srcFilename").unwrap_or_default();
            let dst = theme_file_arg_string(file, "dstFilename").unwrap_or_default();
            let Some(source_file) = self.find_theme_file(&theme_id, &src) else {
                errors.push(user_error(
                    vec![
                        "files".to_string(),
                        index.to_string(),
                        "srcFilename".to_string(),
                    ],
                    "File not found",
                    Some("NOT_FOUND"),
                ));
                continue;
            };
            let content = source_file["body"]["content"].as_str().unwrap_or_default();
            let record = theme_file_record(&dst, content);
            copied.push(record);
        }
        let copied_results = copied
            .iter()
            .filter_map(|file| self.upsert_theme_file(&theme_id, file.clone()))
            .map(|file| theme_file_operation_result(&file))
            .collect::<Vec<_>>();
        if !copied_results.is_empty() {
            staged_ids.push(theme_id);
        }
        selected_json(
            &json!({"copiedThemeFiles": copied_results, "userErrors": errors}),
            &field.selection,
        )
    }

    pub(in crate::proxy) fn theme_files_delete(
        &mut self,
        field: &RootFieldSelection,
        staged_ids: &mut Vec<String>,
    ) -> Value {
        let theme_id = resolved_string_arg(&field.arguments, "themeId").unwrap_or_default();
        let files = resolved_string_list_arg(&field.arguments, "files");
        if files.len() > THEME_FILES_MAX_FILE_LIMIT {
            return selected_json(
                &json!({"deletedThemeFiles": [], "userErrors": [theme_file_limit_error()]}),
                &field.selection,
            );
        }
        let mut errors = Vec::new();
        let mut seen_filenames = BTreeSet::new();
        for (index, filename) in files.iter().enumerate() {
            if !seen_filenames.insert(filename.clone()) {
                errors.push(theme_file_delete_error(
                    index,
                    "duplicate-file-input",
                    "INVALID",
                ));
            }
            if THEME_UNDELETABLE_FILES.contains(&filename.as_str()) {
                errors.push(theme_file_delete_error(
                    index,
                    "File is required and can't be deleted",
                    "INVALID",
                ));
            }
        }
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
                    let removed = nodes.remove(index);
                    deleted.push(theme_file_operation_result(&removed));
                }
            }
            set_theme_file_nodes(theme, nodes);
        }
        if !deleted.is_empty() {
            staged_ids.push(theme_id);
        }
        selected_json(
            &json!({"deletedThemeFiles": deleted, "userErrors": []}),
            &field.selection,
        )
    }

    pub(in crate::proxy) fn upsert_theme_file(
        &mut self,
        theme_id: &str,
        mut file: Value,
    ) -> Option<Value> {
        let theme = self
            .store
            .staged
            .online_store_integrations
            .get_mut(theme_id)?;
        let filename = file["filename"].as_str().unwrap_or_default().to_string();
        let mut nodes = theme_file_nodes(theme);
        let persisted = if let Some(index) = nodes
            .iter()
            .position(|existing| existing["filename"].as_str() == Some(filename.as_str()))
        {
            let created_at = nodes[index]
                .get("createdAt")
                .cloned()
                .unwrap_or_else(|| json!("2024-01-01T00:00:00.000Z"));
            file["createdAt"] = created_at;
            file["updatedAt"] = json!("2024-01-01T00:00:01.000Z");
            nodes[index] = file;
            nodes[index].clone()
        } else {
            file["createdAt"] = json!("2024-01-01T00:00:00.000Z");
            file["updatedAt"] = json!("2024-01-01T00:00:00.000Z");
            nodes.push(file);
            nodes.last().cloned().unwrap_or(Value::Null)
        };
        set_theme_file_nodes(theme, nodes);
        Some(persisted)
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
                &json!({"webPixel": null, "userErrors": [user_error_typed("WebPixelUserError", Value::Null, "Web pixel is taken.", Some("TAKEN"))]}),
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
                &json!({"webPixel": null, "userErrors": [user_error_typed("WebPixelUserError", ["id"], "Pixel not found", Some("NOT_FOUND"))]}),
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
                &json!({"webPixel": null, "userErrors": [user_error_typed("WebPixelUserError", ["settings"], "Settings must be valid JSON", Some("INVALID_CONFIGURATION_JSON"))]}),
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
                &json!({"serverPixel": null, "userErrors": [user_error_typed("ServerPixelUserError", ["id"], "Server pixel not found", Some("NOT_FOUND"))]}),
                &field.selection,
            );
        };
        let endpoint = if kind == "arn" {
            let arn = resolved_string_arg(&field.arguments, "arn").unwrap_or_default();
            if !arn.starts_with("arn:aws:events:") || arn.trim().is_empty() {
                return selected_json(
                    &json!({"serverPixel": null, "userErrors": [user_error_typed("ServerPixelUserError", ["arn"], &format!("Invalid ARN '{arn}'"), Some("INVALID_FIELD_ARGUMENTS"))]}),
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
                errors.push(user_error_typed(
                    "ServerPixelUserError",
                    ["pubSubProject"],
                    "pubSubProject can't be blank",
                    Some("INVALID_FIELD_ARGUMENTS"),
                ));
            }
            if topic.trim().is_empty() {
                errors.push(user_error_typed(
                    "ServerPixelUserError",
                    ["pubSubTopic"],
                    "pubSubTopic can't be blank",
                    Some("INVALID_FIELD_ARGUMENTS"),
                ));
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
                &json!({"storefrontAccessToken": null, "shop": {"id": "gid://shopify/Shop/92891250994"}, "userErrors": [user_error(["input", "title"], "Title can't be blank", Some("BLANK"))]}),
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
                &json!({"storefrontAccessToken": null, "shop": {"id": "gid://shopify/Shop/92891250994"}, "userErrors": [user_error(["input"], "apps.admin.graph_api_errors.storefront_access_token_create.reached_limit", Some("REACHED_LIMIT"))]}),
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
                match draft_order_create_input_email(field).as_deref() {
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
                    _ => draft_order_purchasing_company(field)
                        .map(|purchasing| self.stage_b2b_purchasing_draft_order(field, purchasing)),
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
                "orderCreate"
                    | "orderUpdate"
                    | "orderClose"
                    | "orderOpen"
                    | "order"
                    | "orders"
                    | "ordersCount"
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
                "orderUpdate" => self.stage_order_update(request, query, variables, &field)?,
                "orderClose" | "orderOpen" => {
                    self.stage_order_lifecycle(request, query, variables, &field)
                }
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
                "ordersCount" => self.staged_orders_count(&field),
                _ => return None,
            };
            data.insert(field.response_key, value);
        }
        Some(json!({ "data": Value::Object(data) }))
    }

    /// Full order projections from the seeded catalog that match a connection's
    /// `query:` filter, ordered by `sortKey`/`reverse`. The returned values are
    /// whole orders (not yet selection-projected) so the caller can window them
    /// and then project both `nodes` and `pageInfo` through the field selection.
    fn matching_orders_sorted(&self, arguments: &BTreeMap<String, ResolvedValue>) -> Vec<Value> {
        let query_arg = resolved_string_arg(arguments, "query").unwrap_or_default();
        // Enum arguments resolve to their variant name as a string.
        let sort_key = resolved_string_arg(arguments, "sortKey").unwrap_or_default();
        let reverse = resolved_bool_field(arguments, "reverse").unwrap_or(false);
        let mut matched = self
            .store
            .staged
            .orders
            .values()
            .filter(|order| order_matches_query(order, &query_arg))
            .cloned()
            .collect::<Vec<_>>();
        matched.sort_by_key(|a| order_sort_value(a, &sort_key));
        if reverse {
            matched.reverse();
        }
        matched
    }

    fn staged_orders_connection(&self, field: &RootFieldSelection) -> Value {
        let matched = self.matching_orders_sorted(&field.arguments);
        // Window with the order id as the opaque cursor. The next-page request in
        // the catalog scenario feeds this connection's own `endCursor` back as
        // `after`, so the cursor only needs to round-trip with itself — it is not
        // compared against Shopify's recorded opaque cursors.
        selected_connection_json_with_args(
            matched,
            &field.arguments,
            &field.selection,
            value_id_cursor,
        )
    }

    /// `ordersCount` over the seeded catalog: count matches, then apply Shopify's
    /// `limit` precision semantics — capped at `limit` and reported `AT_LEAST`
    /// when more matches exist than the limit, otherwise the exact total.
    fn staged_orders_count(&self, field: &RootFieldSelection) -> Value {
        let query_arg = resolved_string_arg(&field.arguments, "query").unwrap_or_default();
        let matched = self
            .store
            .staged
            .orders
            .values()
            .filter(|order| order_matches_query(order, &query_arg))
            .count();
        let (count, precision) = match resolved_int_field(&field.arguments, "limit") {
            Some(limit) if limit >= 0 && matched as i64 > limit => (limit as usize, "AT_LEAST"),
            _ => (matched, "EXACT"),
        };
        selected_json(
            &json!({ "count": count, "precision": precision }),
            &field.selection,
        )
    }

    fn stage_order_update(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        field: &RootFieldSelection,
    ) -> Option<Value> {
        let input = resolved_object_field(&field.arguments, "input")?;
        if resolved_string_field(&input, "staffMemberId").is_some() {
            return Some(selected_json(
                &json!({
                    "order": Value::Null,
                    "userErrors": [orders_error(&["input", "staffMemberId"], "Staff member does not exist", "NOT_FOUND")]
                }),
                &field.selection,
            ));
        }

        let order_id = resolved_string_field(&input, "id")?;
        // An update targets an order that already lives in the backend; pull its
        // current state so the merge applies onto real fields (name, customer,
        // line items) rather than a synthetic stub. Only hydrate when the order
        // is not already staged: a record produced by an earlier local mutation
        // (e.g. a prior orderUpdate accumulating localization entries) is more
        // current than the backend snapshot and must not be clobbered. On a
        // cassette miss this is a no-op and we fall through to the
        // "Order does not exist" guard below.
        if !self.store.staged.orders.contains_key(&order_id) {
            self.ensure_order_hydrated(request, &order_id);
        }
        let Some(existing_order) = self.store.staged.orders.get(&order_id).cloned() else {
            if self.config.read_mode != ReadMode::Snapshot
                && self.config.unsupported_mutation_mode
                    == Some(UnsupportedMutationMode::Passthrough)
            {
                return None;
            }
            return Some(selected_json(
                &json!({
                    "order": Value::Null,
                    "userErrors": [orders_error(&["id"], "Order does not exist", "NOT_FOUND")]
                }),
                &field.selection,
            ));
        };

        let validation_errors = order_update_validation_errors(&input);
        if !validation_errors.is_empty() {
            return Some(selected_json(
                &json!({
                    "order": existing_order,
                    "userErrors": validation_errors
                }),
                &field.selection,
            ));
        }

        let mut order = existing_order;
        if input.contains_key("note") {
            order["note"] = resolved_nullable_string_field(&input, "note");
        }
        if input.contains_key("tags") {
            order["tags"] = json!(resolved_string_list_field(&input, "tags"));
        }
        if input.contains_key("customAttributes") {
            order["customAttributes"] =
                json!(order_create_custom_attributes(&input, "customAttributes"));
        }
        if input.contains_key("email") {
            let email = resolved_nullable_string_field(&input, "email");
            order["email"] = email.clone();
        }
        if input.contains_key("phone") {
            order["phone"] = resolved_nullable_string_field(&input, "phone");
        }
        if input.contains_key("poNumber") {
            order["poNumber"] = resolved_nullable_string_field(&input, "poNumber");
        }
        if input.contains_key("shippingAddress") {
            order["shippingAddress"] =
                order_create_address(resolved_object_field(&input, "shippingAddress"));
        }
        if input.contains_key("metafields") {
            let existing_metafields = order["metafields"]["nodes"]
                .as_array()
                .cloned()
                .or_else(|| self.store.staged.owner_metafields.get(&order_id).cloned())
                .unwrap_or_default();
            let metafields = order_update_metafields(&order_id, &input, &existing_metafields);
            self.store
                .staged
                .owner_metafields
                .insert(order_id.clone(), metafields.clone());
            order["metafield"] = metafields.first().cloned().unwrap_or(Value::Null);
            order["metafields"] = order_connection(metafields);
        }
        // Shopify mirrors order localization between `localizedFields` and
        // `localizationExtensions`: a value submitted through either input
        // surfaces under both connections, and successive updates accumulate
        // (deduped by key) rather than replacing the prior set.
        let localization_input: Vec<Value> = resolved_object_list_field(&input, "localizedFields")
            .into_iter()
            .chain(resolved_object_list_field(&input, "localizationExtensions"))
            .filter_map(|entry| {
                let key = resolved_string_field(&entry, "key")?;
                let value = resolved_string_field(&entry, "value")?;
                Some(json!({ "key": key, "value": value }))
            })
            .collect();
        if !localization_input.is_empty() {
            let mut entries = order["localizedFields"]["nodes"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            for entry in localization_input {
                let key = entry["key"].as_str().unwrap_or_default().to_string();
                if let Some(slot) = entries
                    .iter_mut()
                    .find(|existing| existing["key"].as_str() == Some(key.as_str()))
                {
                    *slot = entry;
                } else {
                    entries.push(entry);
                }
            }
            order["localizedFields"] = order_connection(entries.clone());
            order["localizationExtensions"] = order_connection(entries);
        }
        order["updatedAt"] = json!(order_mutation_timestamp(self.log_entries.len() as u64));

        self.store
            .staged
            .orders
            .insert(order_id.clone(), order.clone());
        for orders in self.store.staged.customer_orders.values_mut() {
            for customer_order in orders {
                if customer_order["id"].as_str() == Some(order_id.as_str()) {
                    *customer_order = order.clone();
                }
            }
        }
        self.record_orders_local_log_entry(OrdersLocalLogEntry {
            request,
            query,
            variables,
            root_field: "orderUpdate",
            staged_resource_ids: vec![order_id],
            outcome: OrdersLocalLogOutcome {
                status: "staged",
                notes: "Locally staged orderUpdate in shopify-draft-proxy.",
            },
        });

        Some(selected_json(
            &json!({ "order": order, "userErrors": [] }),
            &field.selection,
        ))
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

    fn stage_order_lifecycle(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        field: &RootFieldSelection,
    ) -> Value {
        let id = order_lifecycle_input_id(field).unwrap_or_default();
        let Some(mut order) = self.order_lifecycle_order(&id, request, field.name.as_str()) else {
            self.record_orders_local_log_entry(OrdersLocalLogEntry {
                request,
                query,
                variables,
                root_field: field.name.as_str(),
                staged_resource_ids: Vec::new(),
                outcome: OrdersLocalLogOutcome {
                    status: "failed",
                    notes: "Locally handled order lifecycle mutation for an unknown order.",
                },
            });
            return selected_json(
                &json!({
                    "order": Value::Null,
                    "userErrors": [{ "field": ["id"], "message": "Order does not exist" }]
                }),
                &field.selection,
            );
        };

        normalize_order_lifecycle_defaults(&mut order);
        let currently_closed = order["closed"].as_bool().unwrap_or(false);
        match field.name.as_str() {
            "orderClose" if !currently_closed => {
                order["closed"] = json!(true);
                order["closedAt"] = json!("2024-01-01T00:00:01.000Z");
                order["updatedAt"] = json!("2024-01-01T00:00:01.000Z");
            }
            "orderOpen" if currently_closed => {
                order["closed"] = json!(false);
                order["closedAt"] = Value::Null;
                order["updatedAt"] = json!("2024-01-01T00:00:02.000Z");
            }
            _ => {}
        }

        self.store.staged.orders.insert(id.clone(), order.clone());
        self.record_orders_local_log_entry(OrdersLocalLogEntry {
            request,
            query,
            variables,
            root_field: field.name.as_str(),
            staged_resource_ids: vec![id],
            outcome: OrdersLocalLogOutcome {
                status: "staged",
                notes: "Locally staged order lifecycle mutation in shopify-draft-proxy.",
            },
        });
        selected_json(
            &json!({ "order": order, "userErrors": [] }),
            &field.selection,
        )
    }

    fn order_lifecycle_order(
        &self,
        id: &str,
        request: &Request,
        root_field: &str,
    ) -> Option<Value> {
        self.store
            .staged
            .orders
            .get(id)
            .cloned()
            .or_else(|| self.hydrate_order_lifecycle_order(id, request, root_field))
    }

    fn hydrate_order_lifecycle_order(
        &self,
        id: &str,
        request: &Request,
        root_field: &str,
    ) -> Option<Value> {
        if self.config.read_mode != ReadMode::LiveHybrid {
            return None;
        }
        let response = self.upstream_post(
            request,
            json!({
                "query": ORDER_LIFECYCLE_HYDRATE_QUERY,
                "variables": { "id": id }
            }),
        );
        if !(200..300).contains(&response.status) {
            return None;
        }
        let mut order = response.body["data"]["order"].clone();
        if order.is_null() {
            order = response.body["data"][root_field]["order"].clone();
        }
        if order.is_null() {
            None
        } else {
            Some(order)
        }
    }

    /// Stage the live lifecycle/summary projection of `id` into `staged.orders`
    /// if it is not already present. Used by order-customer mutations
    /// (orderCancel / orderCustomerSet / orderCustomerRemove) so their happy
    /// path earns the order from the backend rather than 404-ing when no
    /// precondition seed exists.
    fn ensure_order_lifecycle_hydrated(&mut self, request: &Request, id: &str) {
        if id.is_empty() || self.store.staged.orders.contains_key(id) {
            return;
        }
        if let Some(order) = self.hydrate_order_lifecycle_order(id, request, "") {
            self.store.staged.orders.insert(id.to_string(), order);
        }
    }

    /// Confirm an order exists on the backend without staging it. Used by the
    /// refundMethod orderCancel path, which acknowledges the cancel but defers the
    /// authoritative refunded/restocked order projection to the backend by leaving
    /// the order unstaged (the downstream read then forwards upstream).
    fn order_exists_upstream(&self, request: &Request, id: &str) -> bool {
        !id.is_empty()
            && self
                .hydrate_order_lifecycle_order(id, request, "")
                .is_some()
    }

    /// Hydrate the summary customer projection used by orderCustomerSet and
    /// stage it under `staged.customers`. Issues the canonical `CustomerHydrate`
    /// query so a live backend returns the id/email/displayName the mutation
    /// then re-projects.
    fn ensure_order_customer_hydrated(&mut self, request: &Request, id: &str) {
        if id.is_empty() || self.store.staged.customers.contains_key(id) {
            return;
        }
        if self.config.read_mode != ReadMode::LiveHybrid {
            return;
        }
        let response = self.upstream_post(
            request,
            json!({
                "query": ORDER_CUSTOMER_SUMMARY_HYDRATE_QUERY,
                "variables": { "id": id }
            }),
        );
        if !(200..300).contains(&response.status) {
            return;
        }
        let customer = response.body["data"]["customer"].clone();
        if customer.is_object() {
            self.store.staged.customers.insert(id.to_string(), customer);
        }
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
        let response = self.upstream_post(
            request,
            json!({
                "query": hydrate_query,
                "variables": { "id": fulfillment_order_id }
            }),
        );
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

    fn hydrate_order_for_mark_as_paid(
        &mut self,
        order_id: &str,
        request: &Request,
    ) -> Option<String> {
        if self.config.read_mode == ReadMode::Snapshot {
            return None;
        }
        let response = self.upstream_post(
            request,
            json!({
                "query": ORDER_MARK_AS_PAID_HYDRATE_QUERY,
                "variables": { "id": order_id }
            }),
        );
        if !(200..300).contains(&response.status) {
            return None;
        }
        let order = response.body["data"]["order"].clone();
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
        let response = self.upstream_post(
            request,
            json!({
                "query": ORDERS_FULFILLMENT_HYDRATE_QUERY,
                "variables": { "id": fulfillment_id }
            }),
        );
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

    fn hydrate_order_for_fulfillment_lifecycle(
        &mut self,
        fulfillment_id: &str,
        request: &Request,
    ) -> Option<String> {
        if self.config.read_mode == ReadMode::Snapshot {
            return None;
        }
        // Stage one: resolve the fulfillment's owning order and the sibling
        // fulfillment states needed for the cancel/tracking preconditions.
        let response = self.upstream_post(
            request,
            json!({
                "query": ORDERS_FULFILLMENT_LIFECYCLE_HYDRATE_QUERY,
                "variables": { "id": fulfillment_id }
            }),
        );
        if !(200..300).contains(&response.status) {
            return None;
        }
        let fulfillment = response.body["data"]["fulfillment"].clone();
        let mut order = fulfillment["order"].clone();
        if !order.is_object() {
            return None;
        }
        let order_id = order.get("id").and_then(Value::as_str)?.to_string();
        // Stage two (best-effort): enrich with the full fulfillment line-item view so a
        // downstream order read observes line items. A cassette miss here is non-fatal.
        let enriched = self.upstream_post(
            request,
            json!({
                "query": ORDER_FULFILLMENT_LIFECYCLE_READ_QUERY,
                "variables": { "id": order_id }
            }),
        );
        if (200..300).contains(&enriched.status) {
            let enriched_order = enriched.body["data"]["order"].clone();
            if enriched_order.is_object() {
                order = enriched_order;
            }
        }
        // Guarantee the target fulfillment is present in the staged list even when only the
        // stage-one projection was available.
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

    pub(in crate::proxy) fn refund_create_local_data(
        &mut self,
        request: &Request,
        root_field: &str,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Option<Value> {
        if root_field != "refundCreate" {
            return None;
        }
        let fields = root_fields(query, variables)?;
        if !fields.iter().all(|field| field.name == "refundCreate") {
            return None;
        }

        let mut data = serde_json::Map::new();
        for field in fields {
            let (value, staged_ids) = self.stage_refund_create(request, query, variables, &field);
            if !staged_ids.is_empty() {
                self.record_orders_local_log_entry(OrdersLocalLogEntry {
                    request,
                    query,
                    variables,
                    root_field: "refundCreate",
                    staged_resource_ids: staged_ids,
                    outcome: OrdersLocalLogOutcome {
                        status: "staged",
                        notes: "Locally staged refundCreate in shopify-draft-proxy.",
                    },
                });
            }
            data.insert(field.response_key, value);
        }
        Some(json!({ "data": Value::Object(data) }))
    }

    fn stage_refund_create(
        &mut self,
        request: &Request,
        _query: &str,
        _variables: &BTreeMap<String, ResolvedValue>,
        field: &RootFieldSelection,
    ) -> (Value, Vec<String>) {
        let Some(input) = resolved_object_field(&field.arguments, "input") else {
            return (
                refund_input_error(
                    field,
                    None,
                    refund_user_error(json!(["input"]), "Input is required", "INVALID"),
                ),
                Vec::new(),
            );
        };
        let Some(order_id) = resolved_string_field(&input, "orderId") else {
            return (
                refund_input_error(
                    field,
                    None,
                    refund_user_error(json!(["orderId"]), "Order does not exist", "NOT_FOUND"),
                ),
                Vec::new(),
            );
        };

        self.hydrate_order_for_refund(request, &order_id);
        let Some(order) = self.store.staged.orders.get(&order_id).cloned() else {
            return (
                refund_input_error(
                    field,
                    None,
                    refund_user_error(json!(["orderId"]), "Order does not exist", "NOT_FOUND"),
                ),
                Vec::new(),
            );
        };
        let order = refund_order_with_defaults(order);

        if let Some(error) = refund_transaction_validation_error(&input, &order) {
            return (refund_input_error(field, Some(order), error), Vec::new());
        }
        if let Some(error) = refund_quantity_validation_error(&input, &order) {
            return (refund_input_error(field, Some(order), error), Vec::new());
        }
        if let Some(error) = refund_amount_validation_error(&input, &order) {
            return (refund_input_error(field, Some(order), error), Vec::new());
        }

        let shop_currency = order_currency(&order);
        let presentment_currency = order_presentment_currency(&order, &shop_currency);
        let refund_amount = refund_input_total_amount(&input, &order);
        let shipping_refund_amount = refund_input_shipping_amount(&input, &order);
        let refund_id = format!("gid://shopify/Refund/{}", self.store.staged.next_refund_id);
        self.store.staged.next_refund_id += 1;
        let mut next_line_item_id = self.store.staged.next_refund_line_item_id;
        let refund_line_items = build_refund_line_items(
            &input,
            &order,
            &shop_currency,
            &presentment_currency,
            &mut next_line_item_id,
        );
        self.store.staged.next_refund_line_item_id = next_line_item_id;
        let (transaction_id, next_transaction_id) =
            next_refund_transaction_id(&order, self.store.staged.order_payment_next_transaction_id);
        self.store.staged.order_payment_next_transaction_id = next_transaction_id;
        let refund_transactions = build_refund_transactions(
            &input,
            &order,
            refund_amount,
            &shop_currency,
            &presentment_currency,
            &transaction_id,
        );
        let refund = json!({
            "id": refund_id,
            "note": resolved_string_field(&input, "note"),
            "createdAt": "2024-01-01T00:00:00.000Z",
            "updatedAt": "2024-01-01T00:00:00.000Z",
            "totalRefundedSet": order_money_bag_from_amount(refund_amount, &shop_currency, &presentment_currency),
            "refundLineItems": order_connection(refund_line_items),
            "transactions": order_connection(refund_transactions.clone())
        });
        let updated_order = update_order_after_refund(
            order,
            &refund,
            &refund_transactions,
            refund_amount,
            shipping_refund_amount,
            &shop_currency,
            &presentment_currency,
        );
        self.store
            .staged
            .orders
            .insert(order_id.clone(), updated_order.clone());

        (
            selected_json(
                &json!({
                    "refund": refund,
                    "order": updated_order,
                    "userErrors": []
                }),
                &field.selection,
            ),
            vec![refund_id, order_id],
        )
    }

    fn hydrate_order_for_refund(&mut self, request: &Request, order_id: &str) {
        if self.store.staged.orders.contains_key(order_id)
            || self.config.read_mode == ReadMode::Snapshot
        {
            return;
        }
        let response = self.upstream_post(
            request,
            json!({
                "query": REFUND_ORDER_HYDRATE_QUERY,
                "operationName": "OrdersOrderHydrate",
                "variables": { "id": order_id }
            }),
        );
        let order = response.body["data"]["order"].clone();
        if order.is_object() {
            self.store
                .staged
                .orders
                .insert(order_id.to_string(), refund_order_with_defaults(order));
        }
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

    fn staged_fulfillment_read_payload(&self, field: &RootFieldSelection) -> Option<Value> {
        let fulfillment_id = resolved_string_arg(&field.arguments, "id")?;
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
        Some(nullable_selected_json(&fulfillment, &field.selection))
    }

    fn fulfillment_event_create_missing_fulfillment_payload(field: &RootFieldSelection) -> Value {
        selected_json(
            &json!({
                "fulfillmentEvent": Value::Null,
                "userErrors": [orders_error(
                    &["fulfillmentEvent", "fulfillmentId"],
                    "Fulfillment does not exist.",
                    "NOT_FOUND"
                )]
            }),
            &field.selection,
        )
    }

    fn staged_fulfillment_event_create_payload(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        field: &RootFieldSelection,
    ) -> Value {
        let Some(input) = resolved_object_field(&field.arguments, "fulfillmentEvent") else {
            return selected_json(
                &json!({
                    "fulfillmentEvent": Value::Null,
                    "userErrors": [orders_error(&["fulfillmentEvent"], "Fulfillment event is required", "INVALID")]
                }),
                &field.selection,
            );
        };
        let Some(fulfillment_id) = resolved_string_field(&input, "fulfillmentId") else {
            return Self::fulfillment_event_create_missing_fulfillment_payload(field);
        };
        if !fulfillment_gid_has_numeric_tail(&fulfillment_id) {
            return Self::fulfillment_event_create_missing_fulfillment_payload(field);
        }
        let status = resolved_string_field(&input, "status").unwrap_or_default();
        if !fulfillment_event_status_is_allowed(&status) {
            return selected_json(
                &json!({
                    "fulfillmentEvent": Value::Null,
                    "userErrors": [orders_error(
                        &["fulfillmentEvent", "status"],
                        "Fulfillment event status is invalid.",
                        "INVALID"
                    )]
                }),
                &field.selection,
            );
        }
        let Some(order_id) = self
            .staged_order_id_for_fulfillment(&fulfillment_id)
            .or_else(|| self.hydrate_order_for_fulfillment(&fulfillment_id, request))
        else {
            return Self::fulfillment_event_create_missing_fulfillment_payload(field);
        };
        let Some(order_before) = self.store.staged.orders.get(&order_id).cloned() else {
            return Self::fulfillment_event_create_missing_fulfillment_payload(field);
        };
        let mut order = order_before;
        let Some(fulfillment) = order_fulfillments_mut(&mut order).and_then(|fulfillments| {
            fulfillments
                .iter_mut()
                .find(|fulfillment| fulfillment["id"].as_str() == Some(fulfillment_id.as_str()))
        }) else {
            return Self::fulfillment_event_create_missing_fulfillment_payload(field);
        };
        if !fulfillment_accepts_events(fulfillment) {
            return selected_json(
                &json!({
                    "fulfillmentEvent": Value::Null,
                    "userErrors": [orders_error(
                        &["fulfillmentEvent", "fulfillmentId"],
                        "fulfillment_is_cancelled",
                        "INVALID"
                    )]
                }),
                &field.selection,
            );
        }

        let event_id = self.next_proxy_synthetic_gid("FulfillmentEvent");
        let event = fulfillment_event_record(event_id.clone(), &input, &status);
        apply_fulfillment_event_to_fulfillment(fulfillment, &event);
        self.store
            .staged
            .orders
            .insert(order_id.clone(), order.clone());
        self.record_orders_local_log_entry(OrdersLocalLogEntry {
            request,
            query,
            variables,
            root_field: "fulfillmentEventCreate",
            staged_resource_ids: vec![order_id, fulfillment_id, event_id],
            outcome: OrdersLocalLogOutcome {
                status: "staged",
                notes: "Locally staged fulfillmentEventCreate in shopify-draft-proxy.",
            },
        });
        selected_json(
            &json!({ "fulfillmentEvent": event, "userErrors": [] }),
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
            .or_else(|| self.hydrate_order_for_fulfillment_lifecycle(&fulfillment_id, request))?;
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
            .or_else(|| self.hydrate_order_for_fulfillment_lifecycle(&fulfillment_id, request))?;
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
            // Retain the purchasing entity (B2B purchasing company/contact) the
            // order was placed under, the way a real Order exposes it — both so it
            // reads back and so a company delete can detect the order still
            // references it.
            "purchasingEntity": draft_order_purchasing_entity(order_input),
            "closed": false,
            "closedAt": Value::Null,
            "cancelledAt": Value::Null,
            "cancelReason": Value::Null,
            "createdAt": "2024-01-01T00:00:00.000Z",
            "updatedAt": "2024-01-01T00:00:00.000Z",
            "customer": resolved_string_field(order_input, "customerId")
                .map(|id| {
                    // A locally-staged customer carries the authoritative identity
                    // (its own email/displayName, which differ from the order's
                    // contact email). Mirror that record so reads of
                    // order.customer reflect the customer, not the order email.
                    if let Some(customer) = self.store.staged.customers.get(&id) {
                        customer.clone()
                    } else {
                        json!({
                            "id": id,
                            "email": resolved_string_field(order_input, "email"),
                            "displayName": Value::Null
                        })
                    }
                })
                .unwrap_or(Value::Null),
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
            "totalShippingPriceSet": order_create_money_bag(shipping_total, &currency_code, &presentment_currency_code),
            "totalTaxSet": order_create_money_set(tax_total, &currency_code),
            "currentTotalTaxSet": order_create_money_set(tax_total, &currency_code),
            "totalDiscountsSet": order_create_money_set(discount_total, &currency_code),
            "currentTotalDiscountsSet": order_create_money_set(discount_total, &currency_code),
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

    pub(in crate::proxy) fn draft_order_lifecycle_local_response(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Option<Response> {
        let fields = root_fields(query, variables)?;
        if !fields.iter().all(|field| {
            matches!(
                field.name.as_str(),
                "draftOrderCreate"
                    | "draftOrderUpdate"
                    | "draftOrderCalculate"
                    | "draftOrderDuplicate"
                    | "draftOrderDelete"
                    | "draftOrderBulkDelete"
                    | "draftOrderCreateFromOrder"
                    | "draftOrderInvoicePreview"
                    | "draftOrder"
                    | "draftOrders"
                    | "draftOrdersCount"
            )
        }) {
            return None;
        }
        if fields
            .iter()
            .any(|field| field.name == "draftOrderCreate" && draft_order_create_selects_tags(field))
        {
            return None;
        }
        let has_lifecycle_root = fields.iter().any(|field| {
            matches!(
                field.name.as_str(),
                "draftOrderCreate"
                    | "draftOrderUpdate"
                    | "draftOrderCalculate"
                    | "draftOrderDuplicate"
                    | "draftOrderDelete"
                    | "draftOrderBulkDelete"
                    | "draftOrderCreateFromOrder"
                    | "draftOrderInvoicePreview"
            )
        });
        // List/count reads are only served locally once at least one draft order
        // has been staged in this scenario; otherwise they fall through to the
        // upstream passthrough so the recorded live catalog replays verbatim.
        let has_staged_read = fields.iter().any(|field| match field.name.as_str() {
            "draftOrder" => resolved_string_arg(&field.arguments, "id")
                .is_some_and(|id| self.store.staged.draft_orders.contains_key(&id)),
            // List/count reads resolve locally once any draft order has existed this
            // scenario (counter advanced past its base) — a session that created then
            // bulk-deleted every draft must still report `{count: 0}` rather than
            // falling through to the upstream catalog.
            "draftOrders" | "draftOrdersCount" => {
                !self.store.staged.draft_orders.is_empty()
                    || self.store.staged.next_draft_order_id > 1
            }
            _ => false,
        });
        if !has_lifecycle_root && !has_staged_read {
            return None;
        }

        if let Some(response) = draft_order_top_level_validation_response(&fields) {
            return Some(response);
        }

        for field in &fields {
            match field.name.as_str() {
                "draftOrderUpdate" | "draftOrderDuplicate" => {
                    if let Some(id) = resolved_string_arg(&field.arguments, "id") {
                        self.ensure_draft_order_hydrated(request, &id);
                    }
                }
                "draftOrderDelete" => {
                    let input =
                        resolved_object_field(&field.arguments, "input").unwrap_or_default();
                    if let Some(id) = resolved_string_field(&input, "id")
                        .or_else(|| resolved_string_arg(&field.arguments, "id"))
                    {
                        self.ensure_draft_order_hydrated(request, &id);
                    }
                }
                "draftOrderBulkDelete" => {
                    for id in self.draft_order_bulk_target_ids(field) {
                        self.ensure_draft_order_hydrated(request, &id);
                    }
                }
                "draftOrderCreateFromOrder" => {
                    if let Some(order_id) = resolved_string_arg(&field.arguments, "orderId") {
                        self.ensure_order_hydrated(request, &order_id);
                    }
                }
                "draftOrderInvoicePreview" | "draftOrder" => {
                    if let Some(id) = resolved_string_arg(&field.arguments, "id") {
                        self.ensure_draft_order_hydrated(request, &id);
                    }
                }
                _ => {}
            }
        }

        let mut data = serde_json::Map::new();
        for field in fields {
            let value = match field.name.as_str() {
                "draftOrderCreate" => {
                    self.stage_draft_order_create(request, query, variables, &field)
                }
                "draftOrderUpdate" => {
                    self.stage_draft_order_update(request, query, variables, &field)
                }
                "draftOrderCalculate" => self.calculate_draft_order_payload(&field),
                "draftOrderDuplicate" => {
                    self.stage_draft_order_duplicate(request, query, variables, &field)
                }
                "draftOrderDelete" => {
                    self.stage_draft_order_delete(request, query, variables, &field)
                }
                "draftOrderBulkDelete" => {
                    self.stage_draft_order_bulk_delete(request, query, variables, &field)
                }
                "draftOrderCreateFromOrder" => {
                    self.stage_draft_order_create_from_order(request, query, variables, &field)
                }
                "draftOrderInvoicePreview" => {
                    self.draft_order_invoice_preview_payload(request, query, variables, &field)
                }
                "draftOrder" => self.staged_draft_order_read(&field),
                "draftOrders" => self.staged_draft_orders_connection(&field),
                "draftOrdersCount" => selected_json(
                    &json!({
                        "count": self.store.staged.draft_orders.len(),
                        "precision": "EXACT"
                    }),
                    &field.selection,
                ),
                _ => return None,
            };
            data.insert(field.response_key.clone(), value);
        }
        Some(ok_json(json!({ "data": Value::Object(data) })))
    }

    fn stage_draft_order_create(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        field: &RootFieldSelection,
    ) -> Value {
        let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
        if let Some(user_errors) = draft_order_input_user_errors(&input, false) {
            return selected_json(
                &json!({ "draftOrder": Value::Null, "userErrors": user_errors }),
                &field.selection,
            );
        }
        let id = self.next_draft_order_id();
        let name = self.draft_order_name_for_id(&id);
        let customer = draft_order_customer_id(&input)
            .and_then(|customer_id| self.hydrate_draft_order_customer(request, &customer_id));
        let variant_hydrations =
            self.hydrate_draft_order_variants(request, draft_order_line_item_variant_ids(&input));
        let draft_order =
            self.build_draft_order_record(&id, &name, &input, customer, &variant_hydrations);
        self.store
            .staged
            .draft_orders
            .insert(id.clone(), draft_order.clone());
        self.sync_draft_order_tags(&id);
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
            &json!({ "draftOrder": draft_order, "userErrors": [] }),
            &field.selection,
        )
    }

    fn stage_draft_order_update(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        field: &RootFieldSelection,
    ) -> Value {
        let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
        let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
        if let Some(user_errors) = draft_order_input_user_errors(&input, true) {
            return selected_json(
                &json!({ "draftOrder": Value::Null, "userErrors": user_errors }),
                &field.selection,
            );
        }
        let Some(existing) = self.store.staged.draft_orders.get(&id).cloned() else {
            return selected_json(
                &json!({
                    "draftOrder": Value::Null,
                    "userErrors": [user_error(["id"], "Draft order does not exist", Some("NOT_FOUND"))]
                }),
                &field.selection,
            );
        };
        let updated = self.merge_draft_order_input(existing, &input);
        self.store
            .staged
            .draft_orders
            .insert(id.clone(), updated.clone());
        self.sync_draft_order_tags(&id);
        self.record_orders_local_log_entry(OrdersLocalLogEntry {
            request,
            query,
            variables,
            root_field: "draftOrderUpdate",
            staged_resource_ids: vec![id],
            outcome: OrdersLocalLogOutcome {
                status: "staged",
                notes: "Locally staged draftOrderUpdate in shopify-draft-proxy.",
            },
        });
        selected_json(
            &json!({ "draftOrder": updated, "userErrors": [] }),
            &field.selection,
        )
    }

    fn calculate_draft_order_payload(&self, field: &RootFieldSelection) -> Value {
        let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
        if let Some(user_errors) = draft_order_input_user_errors(&input, false) {
            return selected_json(
                &json!({ "calculatedDraftOrder": Value::Null, "userErrors": user_errors }),
                &field.selection,
            );
        }
        if let Some(user_errors) = draft_order_calculate_user_errors(&input) {
            return selected_json(
                &json!({ "calculatedDraftOrder": Value::Null, "userErrors": user_errors }),
                &field.selection,
            );
        }
        let calculated = draft_order_calculated_record(&input);
        selected_json(
            &json!({ "calculatedDraftOrder": calculated, "userErrors": [] }),
            &field.selection,
        )
    }

    fn stage_draft_order_duplicate(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        field: &RootFieldSelection,
    ) -> Value {
        let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
        let Some(source) = self.store.staged.draft_orders.get(&id).cloned() else {
            return selected_json(
                &json!({
                    "draftOrder": Value::Null,
                    "userErrors": [user_error(["id"], "Draft order does not exist", Some("NOT_FOUND"))]
                }),
                &field.selection,
            );
        };
        let new_id = self.next_draft_order_id();
        let new_name = self.draft_order_name_for_id(&new_id);
        let mut duplicate = source;
        duplicate["id"] = json!(new_id.clone());
        duplicate["name"] = json!(new_name);
        duplicate["status"] = json!("OPEN");
        duplicate["ready"] = json!(true);
        duplicate["completedAt"] = Value::Null;
        duplicate["invoiceSentAt"] = Value::Null;
        duplicate["order"] = Value::Null;
        duplicate["orderId"] = Value::Null;
        duplicate["invoiceUrl"] = json!(draft_order_invoice_url(&new_id));
        duplicate["taxExempt"] = json!(false);
        duplicate["reserveInventoryUntil"] = Value::Null;
        duplicate["appliedDiscount"] = Value::Null;
        duplicate["shippingLine"] = Value::Null;
        duplicate["createdAt"] = json!("2024-01-01T00:00:00.000Z");
        duplicate["updatedAt"] = json!("2024-01-01T00:00:00.000Z");
        draft_order_clear_line_discounts(&mut duplicate);
        draft_order_reassign_line_item_ids(&mut duplicate, &new_id);
        self.recalculate_draft_order_totals(&mut duplicate);
        self.store
            .staged
            .draft_orders
            .insert(new_id.clone(), duplicate.clone());
        self.sync_draft_order_tags(&new_id);
        self.record_orders_local_log_entry(OrdersLocalLogEntry {
            request,
            query,
            variables,
            root_field: "draftOrderDuplicate",
            staged_resource_ids: vec![new_id],
            outcome: OrdersLocalLogOutcome {
                status: "staged",
                notes: "Locally staged draftOrderDuplicate in shopify-draft-proxy.",
            },
        });
        selected_json(
            &json!({ "draftOrder": duplicate, "userErrors": [] }),
            &field.selection,
        )
    }

    fn stage_draft_order_delete(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        field: &RootFieldSelection,
    ) -> Value {
        let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
        let id = resolved_string_field(&input, "id")
            .or_else(|| resolved_string_arg(&field.arguments, "id"))
            .unwrap_or_default();
        if self.store.staged.draft_orders.remove(&id).is_none() {
            return selected_json(
                &json!({
                    "deletedId": Value::Null,
                    "userErrors": [user_error(["id"], "Draft order does not exist", Some("NOT_FOUND"))]
                }),
                &field.selection,
            );
        }
        self.store.staged.draft_order_tags.remove(&id);
        self.record_orders_local_log_entry(OrdersLocalLogEntry {
            request,
            query,
            variables,
            root_field: "draftOrderDelete",
            staged_resource_ids: vec![id.clone()],
            outcome: OrdersLocalLogOutcome {
                status: "staged",
                notes: "Locally staged draftOrderDelete in shopify-draft-proxy.",
            },
        });
        selected_json(
            &json!({ "deletedId": id, "userErrors": [] }),
            &field.selection,
        )
    }

    fn stage_draft_order_bulk_delete(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        field: &RootFieldSelection,
    ) -> Value {
        let ids = self.draft_order_bulk_target_ids(field);
        let mut deleted_ids = Vec::new();
        let mut user_errors = Vec::new();
        for (index, id) in ids.iter().enumerate() {
            if self.store.staged.draft_orders.remove(id).is_some() {
                self.store.staged.draft_order_tags.remove(id);
                deleted_ids.push(id.clone());
            } else {
                user_errors.push(user_error(
                    vec!["input".to_string(), "ids".to_string(), index.to_string()],
                    "Draft order does not exist",
                    Some("NOT_FOUND"),
                ));
            }
        }
        if !deleted_ids.is_empty() {
            self.record_orders_local_log_entry(OrdersLocalLogEntry {
                request,
                query,
                variables,
                root_field: "draftOrderBulkDelete",
                staged_resource_ids: deleted_ids,
                outcome: OrdersLocalLogOutcome {
                    status: "staged",
                    notes: "Locally staged draftOrderBulkDelete in shopify-draft-proxy.",
                },
            });
        }
        let job = self.next_draft_order_bulk_tag_job();
        selected_json(
            &json!({ "job": job, "userErrors": user_errors }),
            &field.selection,
        )
    }

    fn stage_draft_order_create_from_order(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        field: &RootFieldSelection,
    ) -> Value {
        let order_id = resolved_string_arg(&field.arguments, "orderId").unwrap_or_default();
        let Some(order) = self.store.staged.orders.get(&order_id).cloned() else {
            return selected_json(
                &json!({
                    "draftOrder": Value::Null,
                    "userErrors": [user_error(["orderId"], "Order does not exist", Some("NOT_FOUND"))]
                }),
                &field.selection,
            );
        };
        let id = self.next_draft_order_id();
        let name = self.draft_order_name_for_id(&id);
        let draft_order = self.build_draft_order_from_order_record(&id, &name, &order);
        self.store
            .staged
            .draft_orders
            .insert(id.clone(), draft_order.clone());
        self.sync_draft_order_tags(&id);
        self.record_orders_local_log_entry(OrdersLocalLogEntry {
            request,
            query,
            variables,
            root_field: "draftOrderCreateFromOrder",
            staged_resource_ids: vec![id],
            outcome: OrdersLocalLogOutcome {
                status: "staged",
                notes: "Locally staged draftOrderCreateFromOrder in shopify-draft-proxy.",
            },
        });
        selected_json(
            &json!({ "draftOrder": draft_order, "userErrors": [] }),
            &field.selection,
        )
    }

    fn draft_order_invoice_preview_payload(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        field: &RootFieldSelection,
    ) -> Value {
        let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
        let Some(draft_order) = self.store.staged.draft_orders.get(&id).cloned() else {
            return selected_json(
                &json!({
                    "previewSubject": Value::Null,
                    "previewHtml": Value::Null,
                    "userErrors": [{ "field": ["id"], "message": "Draft order not found" }]
                }),
                &field.selection,
            );
        };
        let email = resolved_object_field(&field.arguments, "email").unwrap_or_default();
        let subject = resolved_string_field(&email, "subject")
            .unwrap_or_else(|| "Complete your purchase".to_string());
        let custom_message = resolved_string_field(&email, "customMessage").unwrap_or_default();
        let name = draft_order["name"].as_str().unwrap_or("#DRAFT");
        let html = format!(
            "<!DOCTYPE html><html><body><h1>Complete your purchase</h1><p>{custom_message}</p><p>Invoice {name}</p></body></html>"
        );
        self.record_orders_local_log_entry(OrdersLocalLogEntry {
            request,
            query,
            variables,
            root_field: "draftOrderInvoicePreview",
            staged_resource_ids: vec![id],
            outcome: OrdersLocalLogOutcome {
                status: "staged",
                notes: "Locally handled draftOrderInvoicePreview without sending email.",
            },
        });
        selected_json(
            &json!({ "previewSubject": subject, "previewHtml": html, "userErrors": [] }),
            &field.selection,
        )
    }

    fn staged_draft_order_read(&self, field: &RootFieldSelection) -> Value {
        let Some(id) = resolved_string_arg(&field.arguments, "id") else {
            return Value::Null;
        };
        self.store
            .staged
            .draft_orders
            .get(&id)
            .map(|draft_order| selected_json(draft_order, &field.selection))
            .unwrap_or(Value::Null)
    }

    fn staged_draft_orders_connection(&self, field: &RootFieldSelection) -> Value {
        let query_arg = resolved_string_arg(&field.arguments, "query").unwrap_or_default();
        let node_selection = nested_selected_fields(&field.selection, &["nodes"]);
        let edge_node_selection = nested_selected_fields(&field.selection, &["edges", "node"]);
        let mut records = self
            .store
            .staged
            .draft_orders
            .values()
            .filter(|draft_order| draft_order_matches_query(draft_order, &query_arg))
            .cloned()
            .collect::<Vec<_>>();
        records.sort_by(|left, right| {
            right["id"]
                .as_str()
                .unwrap_or_default()
                .cmp(left["id"].as_str().unwrap_or_default())
        });
        let (records, page_info) = connection_window(&records, &field.arguments, value_id_cursor);
        let nodes = records
            .iter()
            .map(|draft_order| selected_json(draft_order, &node_selection))
            .collect::<Vec<_>>();
        let edges = records
            .iter()
            .map(|draft_order| {
                json!({
                    "cursor": value_id_cursor(draft_order),
                    "node": selected_json(draft_order, &edge_node_selection)
                })
            })
            .collect::<Vec<_>>();
        selected_json(
            &json!({ "nodes": nodes, "edges": edges, "pageInfo": page_info }),
            &field.selection,
        )
    }

    fn next_draft_order_id(&mut self) -> String {
        let id = format!(
            "gid://shopify/DraftOrder/{}",
            self.store.staged.next_draft_order_id
        );
        self.store.staged.next_draft_order_id += 1;
        id
    }

    fn draft_order_name_for_id(&self, id: &str) -> String {
        format!("#D{}", resource_id_tail(id))
    }

    fn build_draft_order_record(
        &self,
        id: &str,
        name: &str,
        input: &BTreeMap<String, ResolvedValue>,
        customer: Option<Value>,
        variant_hydrations: &BTreeMap<String, Value>,
    ) -> Value {
        let mut draft_order =
            draft_order_base_record(id, name, input, customer, variant_hydrations);
        self.recalculate_draft_order_totals(&mut draft_order);
        draft_order
    }

    fn ensure_draft_order_hydrated(&mut self, request: &Request, id: &str) {
        if self.config.read_mode == ReadMode::Snapshot
            || id.is_empty()
            || self.store.staged.draft_orders.contains_key(id)
        {
            return;
        }
        let response = self.upstream_post(
            request,
            json!({
                "query": DRAFT_ORDER_HYDRATE_QUERY,
                "operationName": "OrdersDraftOrderHydrate",
                "variables": { "id": id }
            }),
        );
        if !(200..300).contains(&response.status) {
            return;
        }
        let draft_order = response.body["data"]["draftOrder"].clone();
        if !draft_order.is_object() {
            return;
        }
        self.store
            .staged
            .draft_orders
            .insert(id.to_string(), draft_order);
        self.sync_draft_order_tags(id);
    }

    fn ensure_order_hydrated(&mut self, request: &Request, id: &str) {
        if self.config.read_mode == ReadMode::Snapshot || id.is_empty() {
            return;
        }
        // Always attempt a fresh upstream read so the order reflects its live
        // state at the time of this operation. A precondition seed may hold an
        // earlier snapshot of the same order (e.g. the total captured the moment
        // a draft was completed in setup, before the store recalculated
        // tax/shipping), so the recorded hydrate is authoritative when present.
        // On a cassette miss / non-2xx response we keep whatever record is
        // already staged rather than dropping it.
        let response = self.upstream_post(
            request,
            json!({
                "query": ORDER_HYDRATE_QUERY,
                "operationName": "OrdersOrderHydrate",
                "variables": { "id": id }
            }),
        );
        if !(200..300).contains(&response.status) {
            return;
        }
        let order = response.body["data"]["order"].clone();
        if !order.is_object() {
            return;
        }
        self.store.staged.orders.insert(id.to_string(), order);
    }

    fn hydrate_draft_order_customer(&mut self, request: &Request, id: &str) -> Option<Value> {
        if id.is_empty() {
            return None;
        }
        if let Some(customer) = self.store.staged.customers.get(id) {
            return Some(customer.clone());
        }
        if self.config.read_mode == ReadMode::Snapshot {
            return None;
        }
        let response = self.upstream_post(
            request,
            json!({
                "query": DRAFT_ORDER_CUSTOMER_HYDRATE_QUERY,
                "operationName": "OrdersDraftOrderCustomerHydrate",
                "variables": { "id": id }
            }),
        );
        if !(200..300).contains(&response.status) {
            return None;
        }
        let customer = response.body["data"]["customer"].clone();
        if !customer.is_object() {
            return None;
        }
        self.store
            .staged
            .customers
            .insert(id.to_string(), customer.clone());
        Some(customer)
    }

    fn hydrate_draft_order_variants(
        &self,
        request: &Request,
        ids: Vec<String>,
    ) -> BTreeMap<String, Value> {
        if self.config.read_mode == ReadMode::Snapshot {
            return BTreeMap::new();
        }
        ids.into_iter()
            .filter_map(|id| {
                let response = self.upstream_post(
                    request,
                    json!({
                        "query": DRAFT_ORDER_VARIANT_HYDRATE_QUERY,
                        "operationName": "OrdersDraftOrderVariantHydrate",
                        "variables": { "id": id }
                    }),
                );
                if !(200..300).contains(&response.status) {
                    return None;
                }
                let variant = response.body["data"]["productVariant"].clone();
                variant.is_object().then_some((id, variant))
            })
            .collect()
    }

    fn merge_draft_order_input(
        &self,
        mut draft_order: Value,
        input: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        if input.contains_key("email") {
            draft_order["email"] = resolved_string_field(input, "email")
                .map(Value::String)
                .unwrap_or(Value::Null);
        }
        if input.contains_key("note") {
            draft_order["note"] = resolved_string_field(input, "note")
                .map(Value::String)
                .unwrap_or(Value::Null);
        }
        if input.contains_key("tags") {
            draft_order["tags"] = json!(normalize_taggable_tags(
                resolved_string_list_field_unsorted(input, "tags")
            ));
        }
        if input.contains_key("customAttributes") || input.contains_key("properties") {
            draft_order["customAttributes"] = json!(draft_order_input_custom_attributes(input));
        }
        if input.contains_key("shippingLine") {
            draft_order["shippingLine"] = draft_order_shipping_line(input);
        }
        if input.contains_key("billingAddress") {
            draft_order["billingAddress"] =
                order_create_address(resolved_object_field(input, "billingAddress"));
        }
        if input.contains_key("shippingAddress") {
            draft_order["shippingAddress"] =
                order_create_address(resolved_object_field(input, "shippingAddress"));
        }
        if input.contains_key("lineItems") {
            draft_order["lineItems"] = draft_order_line_items_connection(
                &resolved_object_list_field(input, "lineItems"),
                draft_order["id"].as_str().unwrap_or_default(),
                draft_order_currency(&draft_order),
            );
            draft_order["__draftProxyLineItems"] = draft_order["lineItems"]["nodes"].clone();
        }
        if input.contains_key("appliedDiscount") {
            draft_order["appliedDiscount"] = draft_order_applied_discount(input);
        }
        if input.contains_key("taxExempt") {
            draft_order["taxExempt"] =
                json!(resolved_bool_field(input, "taxExempt").unwrap_or(false));
        }
        if input.contains_key("taxesIncluded") {
            draft_order["taxesIncluded"] =
                json!(resolved_bool_field(input, "taxesIncluded").unwrap_or(false));
        }
        if input.contains_key("reserveInventoryUntil") {
            draft_order["reserveInventoryUntil"] =
                resolved_string_field(input, "reserveInventoryUntil")
                    .map(Value::String)
                    .unwrap_or(Value::Null);
        }
        if input.contains_key("paymentTerms") {
            draft_order["paymentTerms"] = draft_order_payment_terms(input);
        }
        draft_order["updatedAt"] = json!("2024-01-01T00:00:00.000Z");
        self.recalculate_draft_order_totals(&mut draft_order);
        draft_order
    }

    fn recalculate_draft_order_totals(&self, draft_order: &mut Value) {
        let currency = draft_order_currency(draft_order);
        let line_items = connection_nodes(&draft_order["lineItems"]);
        let original_subtotal = line_items
            .iter()
            .filter_map(|line| line["originalTotalSet"]["shopMoney"]["amount"].as_str())
            .filter_map(|amount| amount.parse::<f64>().ok())
            .sum::<f64>();
        let line_discount_total = draft_order_line_discount_total(&line_items);
        let shipping_total = draft_order["shippingLine"]["originalPriceSet"]["shopMoney"]["amount"]
            .as_str()
            .and_then(|amount| amount.parse::<f64>().ok())
            .unwrap_or(0.0);
        let discount_total =
            line_discount_total + draft_order_discount_amount(&draft_order["appliedDiscount"]);
        let subtotal = (original_subtotal - discount_total).max(0.0);
        let total = subtotal + shipping_total;
        draft_order["subtotalPriceSet"] = order_create_money_set(subtotal, &currency);
        draft_order["totalDiscountsSet"] = order_create_money_set(discount_total, &currency);
        draft_order["totalShippingPriceSet"] = order_create_money_set(shipping_total, &currency);
        draft_order["totalPriceSet"] = order_create_money_set(total, &currency);
        draft_order["totalQuantityOfLineItems"] = json!(line_items
            .iter()
            .filter_map(|line| line["quantity"].as_i64())
            .sum::<i64>());
    }

    fn build_draft_order_from_order_record(&self, id: &str, name: &str, order: &Value) -> Value {
        let currency = order["currencyCode"]
            .as_str()
            .or_else(|| order["totalPriceSet"]["shopMoney"]["currencyCode"].as_str())
            .unwrap_or("CAD")
            .to_string();
        let line_items = order["lineItems"]["nodes"]
            .as_array()
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .enumerate()
            .map(|(index, line)| draft_order_line_from_order_line(id, index, &line, &currency))
            .collect::<Vec<_>>();
        let mut draft_order = json!({
            "id": id,
            "name": name,
            "status": "OPEN",
            "ready": true,
            "email": order["email"].clone(),
            "note": order["note"].clone(),
            "purchasingEntity": Value::Null,
            "customer": order["customer"].clone(),
            "taxExempt": false,
            "taxesIncluded": false,
            "reserveInventoryUntil": Value::Null,
            "paymentTerms": Value::Null,
            "tags": order["tags"].as_array().cloned().unwrap_or_default(),
            "invoiceUrl": draft_order_invoice_url(id),
            "customAttributes": order["customAttributes"].as_array().cloned().unwrap_or_default(),
            "appliedDiscount": Value::Null,
            "billingAddress": order["billingAddress"].clone(),
            "shippingAddress": order["shippingAddress"].clone(),
            "shippingLine": Value::Null,
            "createdAt": "2024-01-01T00:00:00.000Z",
            "updatedAt": "2024-01-01T00:00:00.000Z",
            "completedAt": Value::Null,
            "invoiceSentAt": Value::Null,
            "order": Value::Null,
            "orderId": Value::Null,
            "lineItems": order_connection(line_items.clone()),
            "__draftProxyLineItems": line_items,
        });
        if draft_order["customer"].is_null() {
            draft_order["customer"] = Value::Null;
        }
        self.recalculate_draft_order_totals(&mut draft_order);
        // A draft created from an order mirrors the source order's monetary
        // totals: Shopify carries the order's grand total onto the new draft
        // rather than recomputing from copied line items (the hydrated order's
        // line items frequently omit per-unit prices, so a recalculation can't
        // reproduce the order's discounts/shipping). Prefer the order total when
        // it's available, falling back to the per-line recalculation otherwise.
        if let Some(order_total) = draft_order_total_from_order(order) {
            draft_order["subtotalPriceSet"] = order_create_money_set(order_total, &currency);
            draft_order["totalPriceSet"] = order_create_money_set(order_total, &currency);
        }
        draft_order
    }

    fn draft_order_bulk_target_ids(&self, field: &RootFieldSelection) -> Vec<String> {
        let mut ids = resolved_string_list_arg(&field.arguments, "ids");
        if ids.is_empty() && resolved_string_arg(&field.arguments, "search").is_some() {
            ids = self.store.staged.draft_orders.keys().cloned().collect();
        }
        ids
    }

    pub(in crate::proxy) fn sync_draft_order_tags(&mut self, id: &str) {
        if let Some(draft_order) = self.store.staged.draft_orders.get(id) {
            let tags = draft_order["tags"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|tag| tag.as_str().map(str::to_string))
                .collect::<Vec<_>>();
            self.store
                .staged
                .draft_order_tags
                .insert(id.to_string(), tags);
        }
    }

    fn sync_draft_order_record_tags(&mut self, id: &str) {
        let Some(tags) = self.store.staged.draft_order_tags.get(id).cloned() else {
            return;
        };
        if let Some(draft_order) = self.store.staged.draft_orders.get_mut(id) {
            draft_order["tags"] = json!(tags);
        }
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
        let draft_order = json!({
            "id": id,
            "name": name,
            "status": "OPEN",
            "__draftProxyFinancialStatus": financial_status,
            "__draftProxyLineItems": [line_item],
            "totalPriceSet": money_set(&amount, currency_code)
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

    /// Stages an OPEN B2B draft order that carries a `purchasingEntity.purchasingCompany`,
    /// retaining the company/contact/location references so a later `draftOrderComplete`
    /// can surface them on the completed order's `purchasingEntity`. This mirrors how live
    /// Shopify links a B2B draft order to the purchasing company, which is the state that
    /// blocks downstream company/location deletion in the deletable-check scenarios.
    fn stage_b2b_purchasing_draft_order(
        &mut self,
        field: &RootFieldSelection,
        purchasing: (Option<String>, Option<String>, Option<String>),
    ) -> Value {
        let id = format!(
            "gid://shopify/DraftOrder/{}",
            self.store.staged.next_draft_order_id
        );
        self.store.staged.next_draft_order_id += 1;
        let name = format!("#D{}", self.store.staged.draft_orders.len() + 1);
        let (company_id, contact_id, location_id) = purchasing;
        let id_ref = |value: &Option<String>| {
            value
                .as_ref()
                .map(|id| json!({ "id": id }))
                .unwrap_or(Value::Null)
        };
        let purchasing_entity = json!({
            "__typename": "PurchasingCompany",
            "company": id_ref(&company_id),
            "contact": id_ref(&contact_id),
            "location": id_ref(&location_id),
        });
        let amount = draft_order_total_amount(field);
        let line_item = draft_order_line_item_record(field);
        let draft_order = json!({
            "id": id,
            "name": name,
            "status": "OPEN",
            "__draftProxyFinancialStatus": "PENDING",
            "__draftProxyLineItems": [line_item],
            "__draftProxyPurchasingEntity": purchasing_entity,
            "totalPriceSet": money_set(&amount, "CAD")
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
        if draft_order.get("__draftProxyLineItems").is_none() {
            draft_order["__draftProxyLineItems"] = draft_order["lineItems"]["nodes"].clone();
        }
        if draft_order["status"].as_str() == Some("COMPLETED") {
            return selected_json(
                &json!({
                    "draftOrder": draft_order,
                    "userErrors": [{
                        "field": Value::Null,
                        "message": "This order has been paid"
                    }]
                }),
                &field.selection,
            );
        }
        let payment_gateway_id = resolved_string_arg(&field.arguments, "paymentGatewayId");
        if payment_gateway_id.is_some() {
            return selected_json(
                &json!({
                    "draftOrder": Value::Null,
                    "userErrors": [{
                        "field": ["paymentGatewayId"],
                        "message": "payment_gateway_not_found",
                        "code": "INVALID"
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
        // Completing a draft materializes order line items: they move into the
        // LineItem id namespace and an absent SKU is reported as null (Shopify
        // surfaces order line items distinctly from their draft counterparts).
        let order_line_items = draft_order["__draftProxyLineItems"]
            .as_array()
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .map(|mut line| {
                if let Some(tail) = line["id"].as_str().map(resource_id_path_tail) {
                    line["id"] = json!(format!("gid://shopify/LineItem/{tail}"));
                }
                if line["sku"].as_str() == Some("") {
                    line["sku"] = Value::Null;
                }
                line
            })
            .collect::<Vec<_>>();
        // The completed order inherits the draft's merchant-facing note and tags,
        // and is settled through the manual payment gateway unless the merchant
        // explicitly marks the payment as pending (in which case no gateway has
        // captured it yet).
        let order_note = draft_order["note"].clone();
        let order_tags = draft_order["tags"].as_array().cloned().unwrap_or_default();
        let payment_gateway_names = if payment_pending {
            Vec::<Value>::new()
        } else {
            vec![json!("manual")]
        };
        // A pending completion has not been captured by any gateway, so it carries
        // no transactions; a settled (non-pending) completion records the manual
        // sale that paid it off.
        let order_transactions = if payment_pending {
            Vec::<Value>::new()
        } else {
            vec![json!({
                "kind": "SALE",
                "status": "SUCCESS",
                "gateway": "manual",
                "amountSet": money_set(&amount, &currency_code)
            })]
        };
        let mut order = json!({
            "id": order_id,
            "name": format!("#{}", self.store.staged.orders.len() + 1),
            "sourceName": "347082227713",
            "note": order_note,
            "tags": order_tags,
            "paymentGatewayNames": payment_gateway_names,
            "transactions": order_transactions,
            "displayFinancialStatus": if payment_pending { "PENDING" } else { "PAID" },
            "displayFulfillmentStatus": "UNFULFILLED",
            "currentTotalPriceSet": money_set(&amount, &currency_code),
            "lineItems": {
                "nodes": order_line_items
            }
        });
        if let Some(purchasing_entity) = draft_order.get("__draftProxyPurchasingEntity") {
            if !purchasing_entity.is_null() {
                order["purchasingEntity"] = purchasing_entity.clone();
            }
        }
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
            "subtotalPriceSet": money_set_pair("1.0", "CAD", "1.0", "CAD"),
            "totalDiscountsSet": money_set_pair("0.0", "CAD", "0.0", "CAD"),
            "totalShippingPriceSet": money_set_pair("0.0", "CAD", "0.0", "CAD"),
            "totalPriceSet": money_set_pair("1.0", "CAD", "1.0", "CAD"),
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

        // Invoice-send validation: a missing recipient yields "To can't be
        // blank", and a draft that has already been completed (paid) can no
        // longer have an invoice sent. Both conditions are checked so a
        // completed draft with no recipient surfaces both userErrors, in the
        // order Shopify reports them (recipient first, then the paid guard).
        let recipient_missing =
            draft_order_invoice_recipient(&field.arguments, &draft_order).is_none();
        let already_paid = draft_order["status"].as_str() == Some("COMPLETED");
        if recipient_missing || already_paid {
            let mut user_errors = Vec::new();
            let mut invoice_errors = Vec::new();
            if recipient_missing {
                user_errors.push(user_error_omit_code(Value::Null, "To can't be blank", None));
                invoice_errors.push(json!({
                    "code": "CUSTOMER_NO_EMAIL",
                    "message": "Customer email can't be blank"
                }));
            }
            if already_paid {
                user_errors.push(json!({
                    "field": Value::Null,
                    "message": "Draft order Invoice can't be sent. This draft order is already paid."
                }));
            }
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
                    "userErrors": user_errors,
                    "invoiceErrors": invoice_errors
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
        if root_field == "fulfillment" {
            let field = field?;
            let payload = self.staged_fulfillment_read_payload(&field)?;
            return Some(data_response(&field.response_key, payload));
        }
        if root_field == "fulfillmentCreate" {
            let field = field?;
            if let Some(error) = fulfillment_create_invalid_id_error(&field) {
                return Some(error);
            }
            return Some(data_response(
                &field.response_key,
                self.staged_fulfillment_payload(request, query, variables, &field),
            ));
        }
        if root_field == "fulfillmentEventCreate" {
            let field = field?;
            return Some(data_response(
                &field.response_key,
                self.staged_fulfillment_event_create_payload(request, query, variables, &field),
            ));
        }
        if root_field == "fulfillmentCancel" {
            let field = field?;
            let payload =
                self.cancel_staged_fulfillment_payload(request, query, variables, &field)?;
            return Some(data_response(&field.response_key, payload));
        }
        if root_field == "fulfillmentTrackingInfoUpdate" {
            let field = field?;
            let payload =
                self.update_staged_fulfillment_tracking_payload(request, query, variables, &field)?;
            return Some(data_response(&field.response_key, payload));
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
        if root_field == "orderEditBegin" {
            let field = field?;
            let order_id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
            let order = match self.store.staged.orders.get(&order_id) {
                Some(order) => order.clone(),
                None => {
                    return Some(data_response(
                        &field.response_key,
                        oe_error_payload(
                            vec![oe_user_error(&["id"], "The order does not exist.", None)],
                            &field.selection,
                        ),
                    ));
                }
            };
            if order_edit_order_is_not_editable(&order) {
                return Some(data_response(
                    &field.response_key,
                    oe_error_payload(
                        vec![oe_user_error(&["base"], "not_editable", None)],
                        &field.selection,
                    ),
                ));
            }
            // Shopify allows only one open order edit per order: beginning a
            // second edit while a prior session is still uncommitted is rejected.
            // The slot is cleared on commit, so post-commit re-edits are allowed.
            if self
                .store
                .staged
                .order_edit_existing_session_order_id
                .as_deref()
                == Some(order_id.as_str())
            {
                return Some(data_response(
                    &field.response_key,
                    oe_error_payload(
                        vec![oe_user_error(
                            &["base"],
                            "This order already has an order edit in progress.",
                            None,
                        )],
                        &field.selection,
                    ),
                ));
            }
            let calculated_id = format!(
                "gid://shopify/CalculatedOrder/{}",
                resource_id_tail(&order_id)
            );
            let session_id = calculated_id.replace("CalculatedOrder", "OrderEditSession");
            let session = oe_build_session(&order, &calculated_id, &session_id);
            let view = oe_calc_order_view(&session);
            self.store.staged.order_edit_existing_order = Some(order);
            self.store.staged.order_edit_existing_calculated_order = Some(session);
            self.store.staged.order_edit_existing_calculated_order_id = Some(calculated_id.clone());
            self.store.staged.order_edit_existing_session_order_id = Some(order_id);
            self.record_mutation_log_entry(
                request,
                query,
                variables,
                "orderEditBegin",
                vec![calculated_id],
            );
            return Some(data_response(
                &field.response_key,
                selected_json(
                    &json!({
                        "calculatedOrder": view,
                        "orderEditSession": { "id": session_id },
                        "userErrors": []
                    }),
                    &field.selection,
                ),
            ));
        }
        if root_field == "orderEditAddVariant" {
            let field = field?;
            let calculated_id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
            if self
                .store
                .staged
                .order_edit_existing_calculated_order_id
                .as_deref()
                != Some(calculated_id.as_str())
            {
                return Some(data_response(
                    &field.response_key,
                    oe_error_payload(
                        vec![oe_user_error(
                            &["id"],
                            "The calculated order does not exist.",
                            None,
                        )],
                        &field.selection,
                    ),
                ));
            }
            let variant_id = resolved_string_arg(&field.arguments, "variantId").unwrap_or_default();
            if resource_id_tail(&variant_id) == "0" {
                return Some(data_response(
                    &field.response_key,
                    oe_error_payload(
                        vec![oe_user_error(
                            &["variantId"],
                            "can't convert Integer[0] to a positive Integer to use as an untrusted id",
                            None,
                        )],
                        &field.selection,
                    ),
                ));
            }
            let quantity = resolved_int_field(&field.arguments, "quantity").unwrap_or(0);
            if quantity == 0 {
                return Some(data_response(
                    &field.response_key,
                    oe_error_payload(
                        vec![oe_user_error(&["quantity"], "must be greater than 0", None)],
                        &field.selection,
                    ),
                ));
            }
            if quantity < 0 {
                return Some(data_response(
                    &field.response_key,
                    oe_error_payload(
                        vec![
                            oe_user_error(&["quantity"], "must be greater than 0", None),
                            oe_user_error(
                                &["quantity"],
                                "must be greater than or equal to 0",
                                None,
                            ),
                        ],
                        &field.selection,
                    ),
                ));
            }
            let allow_duplicates =
                resolved_bool_field(&field.arguments, "allowDuplicates").unwrap_or(false);
            let mut session = self
                .store
                .staged
                .order_edit_existing_calculated_order
                .clone()
                .unwrap_or_else(|| json!({}));
            let currency = session
                .get("currency")
                .and_then(Value::as_str)
                .unwrap_or("CAD")
                .to_string();
            let session_id = calculated_id.replace("CalculatedOrder", "OrderEditSession");
            // When the variant is already on the order and the caller did not opt
            // into duplicates, Shopify returns that line's calculated view
            // unchanged rather than adding a second line.
            if !allow_duplicates {
                let existing = session
                    .get("lines")
                    .and_then(Value::as_array)
                    .and_then(|lines| {
                        lines
                            .iter()
                            .find(|line| {
                                line["variant"]["id"].as_str() == Some(variant_id.as_str())
                            })
                            .cloned()
                    });
                if let Some(line) = existing {
                    let view = oe_line_view(&line, &currency);
                    let order_view = oe_calc_order_view(&session);
                    self.record_mutation_log_entry(
                        request,
                        query,
                        variables,
                        "orderEditAddVariant",
                        vec![calculated_id.clone()],
                    );
                    return Some(data_response(
                        &field.response_key,
                        selected_json(
                            &json!({
                                "calculatedOrder": order_view,
                                "calculatedLineItem": view,
                                "orderEditSession": { "id": session_id },
                                "userErrors": []
                            }),
                            &field.selection,
                        ),
                    ));
                }
            }
            let catalog_entry = self
                .store
                .staged
                .order_edit_variant_catalog
                .get(variant_id.as_str())
                .cloned();
            let catalog_entry = match catalog_entry {
                Some(entry) => entry,
                None => {
                    return Some(data_response(
                        &field.response_key,
                        oe_error_payload(
                            vec![oe_user_error(
                                &["variantId"],
                                "The variant does not exist.",
                                None,
                            )],
                            &field.selection,
                        ),
                    ));
                }
            };
            let seq = oe_next_seq(&mut session);
            let unit = oe_amount_to_cents(
                catalog_entry
                    .get("price")
                    .and_then(Value::as_str)
                    .unwrap_or("0"),
            );
            let line = json!({
                "calcId": format!("gid://shopify/CalculatedLineItem/oe-{seq}"),
                "orderLineId": Value::Null,
                "kind": "added",
                "title": catalog_entry.get("title").cloned().unwrap_or(Value::Null),
                "sku": catalog_entry.get("sku").cloned().unwrap_or(Value::Null),
                "variant": { "id": variant_id },
                "unitCents": unit,
                "histQty": quantity,
                "curQty": quantity,
                "discounts": []
            });
            if let Some(lines) = session.get_mut("lines").and_then(Value::as_array_mut) {
                lines.push(line.clone());
            }
            let view = oe_line_view(&line, &currency);
            let order_view = oe_calc_order_view(&session);
            self.store.staged.order_edit_existing_calculated_order = Some(session);
            self.record_mutation_log_entry(
                request,
                query,
                variables,
                "orderEditAddVariant",
                vec![calculated_id.clone()],
            );
            return Some(data_response(
                &field.response_key,
                selected_json(
                    &json!({
                        "calculatedOrder": order_view,
                        "calculatedLineItem": view,
                        "orderEditSession": { "id": session_id },
                        "userErrors": []
                    }),
                    &field.selection,
                ),
            ));
        }
        if root_field == "orderEditSetQuantity" {
            let field = field?;
            let calculated_id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
            if self
                .store
                .staged
                .order_edit_existing_calculated_order_id
                .as_deref()
                != Some(calculated_id.as_str())
            {
                return Some(data_response(
                    &field.response_key,
                    oe_error_payload(
                        vec![oe_user_error(
                            &["id"],
                            "The calculated order does not exist.",
                            None,
                        )],
                        &field.selection,
                    ),
                ));
            }
            let quantity = resolved_int_field(&field.arguments, "quantity").unwrap_or(0);
            if quantity < 0 {
                return Some(data_response(
                    &field.response_key,
                    oe_error_payload(
                        vec![oe_user_error(
                            &["quantity"],
                            "must be greater than or equal to 0",
                            None,
                        )],
                        &field.selection,
                    ),
                ));
            }
            let line_item_id =
                resolved_string_arg(&field.arguments, "lineItemId").unwrap_or_default();
            let mut session = self
                .store
                .staged
                .order_edit_existing_calculated_order
                .clone()
                .unwrap_or_else(|| json!({}));
            let currency = session
                .get("currency")
                .and_then(Value::as_str)
                .unwrap_or("CAD")
                .to_string();
            let index = match oe_line_index(&session, &line_item_id) {
                Some(index) => index,
                None => {
                    return Some(data_response(
                        &field.response_key,
                        oe_error_payload(
                            vec![oe_user_error(
                                &["lineItemId"],
                                "The line item does not exist.",
                                None,
                            )],
                            &field.selection,
                        ),
                    ));
                }
            };
            session["lines"][index]["curQty"] = json!(quantity);
            let line = session["lines"][index].clone();
            let view = oe_line_view(&line, &currency);
            let order_view = oe_calc_order_view(&session);
            let session_id = calculated_id.replace("CalculatedOrder", "OrderEditSession");
            self.store.staged.order_edit_existing_calculated_order = Some(session);
            self.record_mutation_log_entry(
                request,
                query,
                variables,
                "orderEditSetQuantity",
                vec![calculated_id.clone()],
            );
            return Some(data_response(
                &field.response_key,
                selected_json(
                    &json!({
                        "calculatedOrder": order_view,
                        "calculatedLineItem": view,
                        "orderEditSession": { "id": session_id },
                        "userErrors": []
                    }),
                    &field.selection,
                ),
            ));
        }
        if root_field == "orderEditAddCustomItem" {
            let field = field?;
            let calculated_id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
            if self
                .store
                .staged
                .order_edit_existing_calculated_order_id
                .as_deref()
                != Some(calculated_id.as_str())
            {
                return Some(data_response(
                    &field.response_key,
                    oe_error_payload(
                        vec![oe_user_error(
                            &["id"],
                            "The calculated order does not exist.",
                            None,
                        )],
                        &field.selection,
                    ),
                ));
            }
            let mut session = self
                .store
                .staged
                .order_edit_existing_calculated_order
                .clone()
                .unwrap_or_else(|| json!({}));
            let currency = session
                .get("currency")
                .and_then(Value::as_str)
                .unwrap_or("CAD")
                .to_string();
            let title = resolved_string_arg(&field.arguments, "title").unwrap_or_default();
            if title.trim().is_empty() {
                return Some(data_response(
                    &field.response_key,
                    oe_error_payload(
                        vec![oe_user_error(&["title"], "can't be blank", None)],
                        &field.selection,
                    ),
                ));
            }
            if title.chars().count() > 255 {
                return Some(data_response(
                    &field.response_key,
                    oe_error_payload(
                        vec![oe_user_error(
                            &["title"],
                            "is too long (maximum is 255 characters)",
                            None,
                        )],
                        &field.selection,
                    ),
                ));
            }
            let quantity = resolved_int_field(&field.arguments, "quantity").unwrap_or(0);
            if quantity <= 0 {
                return Some(data_response(
                    &field.response_key,
                    oe_error_payload(
                        vec![oe_user_error(&["quantity"], "must be greater than 0", None)],
                        &field.selection,
                    ),
                ));
            }
            let price = resolved_object_field(&field.arguments, "price").unwrap_or_default();
            if resolved_money_currency(&price).as_deref() != Some(currency.as_str()) {
                return Some(data_response(
                    &field.response_key,
                    oe_error_payload(
                        vec![oe_user_error(
                            &["price", "amount"],
                            &format!("Currency must be {currency}."),
                            None,
                        )],
                        &field.selection,
                    ),
                ));
            }
            let price_cents = oe_money_obj_cents(&price).unwrap_or(0);
            if price_cents < 0 {
                return Some(data_response(
                    &field.response_key,
                    oe_error_payload(
                        vec![oe_user_error(
                            &["price", "amount"],
                            "must be greater than or equal to 0",
                            None,
                        )],
                        &field.selection,
                    ),
                ));
            }
            let seq = oe_next_seq(&mut session);
            let line = json!({
                "calcId": format!("gid://shopify/CalculatedLineItem/oe-{seq}"),
                "orderLineId": Value::Null,
                "kind": "custom",
                "title": title,
                "sku": Value::Null,
                "variant": Value::Null,
                "unitCents": price_cents,
                "histQty": quantity,
                "curQty": quantity,
                "discounts": []
            });
            if let Some(lines) = session.get_mut("lines").and_then(Value::as_array_mut) {
                lines.push(line.clone());
            }
            let view = oe_line_view(&line, &currency);
            let order_view = oe_calc_order_view(&session);
            let session_id = calculated_id.replace("CalculatedOrder", "OrderEditSession");
            self.store.staged.order_edit_existing_calculated_order = Some(session);
            self.record_mutation_log_entry(
                request,
                query,
                variables,
                "orderEditAddCustomItem",
                vec![calculated_id.clone()],
            );
            return Some(data_response(
                &field.response_key,
                selected_json(
                    &json!({
                        "calculatedOrder": order_view,
                        "calculatedLineItem": view,
                        "orderEditSession": { "id": session_id },
                        "userErrors": []
                    }),
                    &field.selection,
                ),
            ));
        }
        if root_field == "orderEditAddLineItemDiscount" {
            let field = field?;
            let calculated_id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
            if self
                .store
                .staged
                .order_edit_existing_calculated_order_id
                .as_deref()
                != Some(calculated_id.as_str())
            {
                return Some(data_response(
                    &field.response_key,
                    oe_error_payload(
                        vec![oe_user_error(
                            &["id"],
                            "The calculated order does not exist.",
                            None,
                        )],
                        &field.selection,
                    ),
                ));
            }
            let mut session = self
                .store
                .staged
                .order_edit_existing_calculated_order
                .clone()
                .unwrap_or_else(|| json!({}));
            let currency = session
                .get("currency")
                .and_then(Value::as_str)
                .unwrap_or("CAD")
                .to_string();
            let line_item_id =
                resolved_string_arg(&field.arguments, "lineItemId").unwrap_or_default();
            let index = match oe_line_index(&session, &line_item_id) {
                Some(index) => index,
                None => {
                    return Some(data_response(
                        &field.response_key,
                        oe_error_payload(
                            vec![oe_user_error(
                                &["lineItemId"],
                                "The line item does not exist.",
                                None,
                            )],
                            &field.selection,
                        ),
                    ));
                }
            };
            let discount = resolved_object_field(&field.arguments, "discount").unwrap_or_default();
            let description = resolved_string_field(&discount, "description");
            let per_unit = resolved_object_field(&discount, "fixedValue")
                .as_ref()
                .and_then(oe_money_obj_cents)
                .unwrap_or(0);
            let seq = oe_next_seq(&mut session);
            let app_id = format!("gid://shopify/CalculatedManualDiscountApplication/oe-disc-{seq}");
            let staged_change_id =
                format!("gid://shopify/OrderStagedChangeAddLineItemDiscount/oe-disc-{seq}");
            let discount_entry = json!({
                "perUnitCents": per_unit,
                "description": description.clone(),
                "appId": app_id
            });
            if let Some(discounts) = session
                .get_mut("lines")
                .and_then(Value::as_array_mut)
                .and_then(|lines| lines.get_mut(index))
                .and_then(|line| line.get_mut("discounts"))
                .and_then(Value::as_array_mut)
            {
                discounts.push(discount_entry);
            }
            let line = session["lines"][index].clone();
            let view = oe_line_view(&line, &currency);
            let order_view = oe_calc_order_view(&session);
            self.store.staged.order_edit_existing_calculated_order = Some(session);
            self.record_mutation_log_entry(
                request,
                query,
                variables,
                "orderEditAddLineItemDiscount",
                vec![calculated_id.clone()],
            );
            return Some(data_response(
                &field.response_key,
                selected_json(
                    &json!({
                        "addedDiscountStagedChange": {
                            "id": staged_change_id,
                            "description": description
                        },
                        "calculatedOrder": order_view,
                        "calculatedLineItem": view,
                        "userErrors": []
                    }),
                    &field.selection,
                ),
            ));
        }
        if root_field == "orderEditRemoveDiscount" {
            let field = field?;
            let calculated_id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
            if self
                .store
                .staged
                .order_edit_existing_calculated_order_id
                .as_deref()
                != Some(calculated_id.as_str())
            {
                return Some(data_response(
                    &field.response_key,
                    oe_error_payload(
                        vec![oe_user_error(
                            &["id"],
                            "The calculated order does not exist.",
                            None,
                        )],
                        &field.selection,
                    ),
                ));
            }
            let mut session = self
                .store
                .staged
                .order_edit_existing_calculated_order
                .clone()
                .unwrap_or_else(|| json!({}));
            let discount_application_id =
                resolved_string_arg(&field.arguments, "discountApplicationId").unwrap_or_default();
            if let Some(lines) = session.get_mut("lines").and_then(Value::as_array_mut) {
                for line in lines.iter_mut() {
                    if let Some(discounts) = line.get_mut("discounts").and_then(Value::as_array_mut)
                    {
                        discounts.retain(|discount| {
                            discount.get("appId").and_then(Value::as_str)
                                != Some(discount_application_id.as_str())
                        });
                    }
                }
            }
            let order_view = oe_calc_order_view(&session);
            self.store.staged.order_edit_existing_calculated_order = Some(session);
            self.record_mutation_log_entry(
                request,
                query,
                variables,
                "orderEditRemoveDiscount",
                vec![calculated_id.clone()],
            );
            return Some(data_response(
                &field.response_key,
                selected_json(
                    &json!({ "calculatedOrder": order_view, "userErrors": [] }),
                    &field.selection,
                ),
            ));
        }
        if root_field == "orderEditAddShippingLine" {
            let field = field?;
            let calculated_id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
            if self
                .store
                .staged
                .order_edit_existing_calculated_order_id
                .as_deref()
                != Some(calculated_id.as_str())
            {
                return Some(data_response(
                    &field.response_key,
                    oe_error_payload(
                        vec![oe_user_error(
                            &["id"],
                            "The calculated order does not exist.",
                            None,
                        )],
                        &field.selection,
                    ),
                ));
            }
            let mut session = self
                .store
                .staged
                .order_edit_existing_calculated_order
                .clone()
                .unwrap_or_else(|| json!({}));
            let currency = session
                .get("currency")
                .and_then(Value::as_str)
                .unwrap_or("CAD")
                .to_string();
            let shipping_line =
                resolved_object_field(&field.arguments, "shippingLine").unwrap_or_default();
            let title = resolved_string_field(&shipping_line, "title");
            let price = resolved_object_field(&shipping_line, "price").unwrap_or_default();
            if resolved_money_currency(&price).as_deref() != Some(currency.as_str()) {
                return Some(data_response(
                    &field.response_key,
                    oe_error_payload(
                        vec![oe_user_error(
                            &["shippingLine", "price"],
                            &format!("The price must be in {currency}."),
                            Some("INVALID"),
                        )],
                        &field.selection,
                    ),
                ));
            }
            let price_cents = oe_money_obj_cents(&price).unwrap_or(0);
            let seq = oe_next_seq(&mut session);
            let shipping = json!({
                "id": format!("gid://shopify/CalculatedShippingLine/oe-ship-{seq}"),
                "title": title,
                "stagedStatus": "ADDED",
                "priceCents": price_cents
            });
            if let Some(lines) = session
                .get_mut("shippingLines")
                .and_then(Value::as_array_mut)
            {
                lines.push(shipping.clone());
            }
            let view = oe_shipping_view(&shipping, &currency);
            let order_view = oe_calc_order_view(&session);
            self.store.staged.order_edit_existing_calculated_order = Some(session);
            self.record_mutation_log_entry(
                request,
                query,
                variables,
                "orderEditAddShippingLine",
                vec![calculated_id.clone()],
            );
            return Some(data_response(
                &field.response_key,
                selected_json(
                    &json!({
                        "calculatedOrder": order_view,
                        "calculatedShippingLine": view,
                        "userErrors": []
                    }),
                    &field.selection,
                ),
            ));
        }
        if root_field == "orderEditUpdateShippingLine" {
            let field = field?;
            let calculated_id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
            if self
                .store
                .staged
                .order_edit_existing_calculated_order_id
                .as_deref()
                != Some(calculated_id.as_str())
            {
                return Some(data_response(
                    &field.response_key,
                    oe_error_payload(
                        vec![oe_user_error(
                            &["id"],
                            "The calculated order does not exist.",
                            None,
                        )],
                        &field.selection,
                    ),
                ));
            }
            let mut session = self
                .store
                .staged
                .order_edit_existing_calculated_order
                .clone()
                .unwrap_or_else(|| json!({}));
            let currency = session
                .get("currency")
                .and_then(Value::as_str)
                .unwrap_or("CAD")
                .to_string();
            let shipping_line_id =
                resolved_string_arg(&field.arguments, "shippingLineId").unwrap_or_default();
            let index = match oe_shipping_index(&session, &shipping_line_id) {
                Some(index) => index,
                None => {
                    return Some(data_response(
                        &field.response_key,
                        oe_error_payload(
                            vec![oe_user_error(
                                &["shippingLineId"],
                                "The shipping line can't be updated because it doesn't exist or wasn't added during this edit.",
                                Some("INVALID"),
                            )],
                            &field.selection,
                        ),
                    ));
                }
            };
            let shipping_line =
                resolved_object_field(&field.arguments, "shippingLine").unwrap_or_default();
            let price = resolved_object_field(&shipping_line, "price");
            if let Some(price) = price.as_ref() {
                if resolved_money_currency(price).as_deref() != Some(currency.as_str()) {
                    return Some(data_response(
                        &field.response_key,
                        oe_error_payload(
                            vec![oe_user_error(
                                &["shippingLine", "price"],
                                &format!("The price must be in {currency}."),
                                Some("INVALID"),
                            )],
                            &field.selection,
                        ),
                    ));
                }
            }
            let new_title = resolved_string_field(&shipping_line, "title");
            let new_price = price.as_ref().and_then(oe_money_obj_cents);
            if let Some(node) = session
                .get_mut("shippingLines")
                .and_then(Value::as_array_mut)
                .and_then(|lines| lines.get_mut(index))
            {
                if let Some(title) = new_title {
                    node["title"] = json!(title);
                }
                if let Some(cents) = new_price {
                    node["priceCents"] = json!(cents);
                }
            }
            let order_view = oe_calc_order_view(&session);
            self.store.staged.order_edit_existing_calculated_order = Some(session);
            self.record_mutation_log_entry(
                request,
                query,
                variables,
                "orderEditUpdateShippingLine",
                vec![calculated_id.clone()],
            );
            return Some(data_response(
                &field.response_key,
                selected_json(
                    &json!({ "calculatedOrder": order_view, "userErrors": [] }),
                    &field.selection,
                ),
            ));
        }
        if root_field == "orderEditRemoveShippingLine" {
            let field = field?;
            let calculated_id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
            if self
                .store
                .staged
                .order_edit_existing_calculated_order_id
                .as_deref()
                != Some(calculated_id.as_str())
            {
                return Some(data_response(
                    &field.response_key,
                    oe_error_payload(
                        vec![oe_user_error(
                            &["id"],
                            "The calculated order does not exist.",
                            None,
                        )],
                        &field.selection,
                    ),
                ));
            }
            let mut session = self
                .store
                .staged
                .order_edit_existing_calculated_order
                .clone()
                .unwrap_or_else(|| json!({}));
            let shipping_line_id =
                resolved_string_arg(&field.arguments, "shippingLineId").unwrap_or_default();
            let index = match oe_shipping_index(&session, &shipping_line_id) {
                Some(index) => index,
                None => {
                    return Some(data_response(
                        &field.response_key,
                        oe_error_payload(
                            vec![oe_user_error(
                                &["shippingLineId"],
                                "The shipping line can't be removed because it doesn't exist or has already been removed.",
                                Some("INVALID"),
                            )],
                            &field.selection,
                        ),
                    ));
                }
            };
            if let Some(lines) = session
                .get_mut("shippingLines")
                .and_then(Value::as_array_mut)
            {
                lines.remove(index);
            }
            let order_view = oe_calc_order_view(&session);
            self.store.staged.order_edit_existing_calculated_order = Some(session);
            self.record_mutation_log_entry(
                request,
                query,
                variables,
                "orderEditRemoveShippingLine",
                vec![calculated_id.clone()],
            );
            return Some(data_response(
                &field.response_key,
                selected_json(
                    &json!({ "calculatedOrder": order_view, "userErrors": [] }),
                    &field.selection,
                ),
            ));
        }
        if root_field == "orderEditCommit" {
            let field = field?;
            let calculated_id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
            if self
                .store
                .staged
                .order_edit_existing_calculated_order_id
                .as_deref()
                != Some(calculated_id.as_str())
            {
                return Some(data_response(
                    &field.response_key,
                    oe_error_payload(
                        vec![oe_user_error(
                            &["id"],
                            "The calculated order does not exist.",
                            None,
                        )],
                        &field.selection,
                    ),
                ));
            }
            let session = self
                .store
                .staged
                .order_edit_existing_calculated_order
                .clone()
                .unwrap_or_else(|| json!({}));
            let base = self
                .store
                .staged
                .order_edit_existing_order
                .clone()
                .unwrap_or_else(|| json!({}));
            let author = self.store.staged.order_edit_author.clone();
            let committed = oe_commit_order(&base, &session, author.as_deref());
            if let Some(order_id) = committed["id"].as_str() {
                self.store
                    .staged
                    .orders
                    .insert(order_id.to_string(), committed.clone());
            }
            let staged_ids = committed["id"]
                .as_str()
                .map(str::to_string)
                .into_iter()
                .collect();
            self.record_mutation_log_entry(
                request,
                query,
                variables,
                "orderEditCommit",
                staged_ids,
            );
            self.store.staged.order_edit_existing_order = Some(committed.clone());
            self.store.staged.order_edit_existing_calculated_order = None;
            self.store.staged.order_edit_existing_calculated_order_id = None;
            self.store.staged.order_edit_existing_session_order_id = None;
            return Some(data_response(
                &field.response_key,
                selected_json(
                    &json!({
                        "order": committed,
                        "successMessages": ["Order updated"],
                        "userErrors": []
                    }),
                    &field.selection,
                ),
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
            "orderMarkAsPaid" => {
                let field = field?;
                let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
                let order_id = resolved_string_field(&input, "id").unwrap_or_default();
                // Orders not created locally in this scenario are hydrated from the
                // backend so the mutation operates on real money-bag state.
                if !order_id.is_empty() && !self.store.staged.orders.contains_key(&order_id) {
                    self.hydrate_order_for_mark_as_paid(&order_id, request);
                }
                let (order, user_errors, staged_ids) = self.stage_order_mark_as_paid(&order_id);
                if !staged_ids.is_empty() {
                    self.record_mutation_log_entry(
                        request, query, variables, root_field, staged_ids,
                    );
                }
                Some(data_response(
                    &field.response_key,
                    selected_json(
                        &json!({ "order": order, "userErrors": user_errors }),
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
        // Base projection: full order math (line items + taxLines, shipping lines +
        // totalShippingPriceSet, subtotals, taxes, discounts). The payment view is
        // layered on top so a payment-field selection still receives the complete
        // order shape rather than the totals-only subset.
        let mut order = self.build_order_create_record(&id, &order_input);
        let transaction_inputs = resolved_object_list_field(&order_input, "transactions");
        let first_transaction = transaction_inputs.first().cloned().unwrap_or_default();
        let amount_set = payment_money_set_from_input(&first_transaction)
            .unwrap_or_else(|| money_set("25.0", &currency));
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
        let payment_view = payment_order_record(
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
        // Override the payment-derived projection onto the full order base.
        for key in [
            "displayFinancialStatus",
            "capturable",
            "totalCapturable",
            "totalCapturableSet",
            "totalOutstandingSet",
            "totalReceivedSet",
            "netPaymentSet",
            "paymentGatewayNames",
            "transactions",
        ] {
            if let Some(value) = payment_view.get(key) {
                order[key] = value.clone();
            }
        }
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

    fn stage_order_mark_as_paid(&mut self, order_id: &str) -> (Value, Vec<Value>, Vec<String>) {
        let Some(order_before) = self.store.staged.orders.get(order_id).cloned() else {
            return (
                Value::Null,
                vec![order_mark_as_paid_not_found_error()],
                Vec::new(),
            );
        };
        let outstanding_set = order_money_set_with_presentment_fallback(
            &order_before["totalOutstandingSet"],
            &order_before,
        );
        if order_before["cancelledAt"].is_string()
            || matches!(
                order_before["displayFinancialStatus"].as_str(),
                Some("PAID" | "REFUNDED" | "PARTIALLY_REFUNDED" | "VOIDED")
            )
            || order_money_amount_value(&outstanding_set) <= 0.000_001
        {
            return (
                order_before,
                vec![order_mark_as_paid_cannot_mark_error()],
                Vec::new(),
            );
        }

        let transaction_id = format!(
            "gid://shopify/OrderTransaction/{}",
            self.store.staged.order_payment_next_transaction_id
        );
        self.store.staged.order_payment_next_transaction_id += 1;
        let transaction = payment_transaction_record_from_amount_set(
            &transaction_id,
            "SALE",
            "SUCCESS",
            outstanding_set.clone(),
            Value::Null,
        );

        let mut order = order_before;
        if let Some(transactions) = order["transactions"].as_array_mut() {
            transactions.push(transaction.clone());
        } else {
            order["transactions"] = json!([transaction.clone()]);
        }
        order["displayFinancialStatus"] = json!("PAID");
        order["capturable"] = json!(false);
        order["totalCapturable"] = json!("0.0");
        order["totalCapturableSet"] = zero_order_money_set_like(&outstanding_set, &order);
        order["totalOutstandingSet"] = zero_order_money_set_like(&outstanding_set, &order);
        let received_set =
            add_order_money_sets(&order["totalReceivedSet"], &outstanding_set, &order);
        order["totalReceivedSet"] = received_set.clone();
        order["netPaymentSet"] = received_set;
        order["paymentGatewayNames"] = json!(["manual"]);

        self.store
            .staged
            .orders
            .insert(order_id.to_string(), order.clone());
        if let Some(customer_id) = order_customer_id(&order) {
            if let Some(customer_orders) = self.store.staged.customer_orders.get_mut(&customer_id) {
                for customer_order in customer_orders {
                    if customer_order["id"].as_str() == Some(order_id) {
                        *customer_order = order.clone();
                    }
                }
            }
        }
        (
            order,
            Vec::new(),
            vec![order_id.to_string(), transaction_id],
        )
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
                    money_set_pair("0.0", &shop_currency, "0.0", &presentment_currency);
                order["totalOutstandingSet"] = amount_set.clone();
                order["totalReceivedSet"] =
                    money_set_pair("0.0", &shop_currency, "0.0", &presentment_currency);
                order["netPaymentSet"] =
                    money_set_pair("0.0", &shop_currency, "0.0", &presentment_currency);
            } else {
                order["totalCapturableSet"] = money_set("0.0", &shop_currency);
                order["totalOutstandingSet"] = amount_set;
                order["totalReceivedSet"] = money_set("0.0", &shop_currency);
                order["netPaymentSet"] = money_set("0.0", &shop_currency);
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
                "orderCustomerSet" => Some(self.order_customer_set_error_paths(request, &field)),
                "orderCustomerRemove" => {
                    Some(self.order_customer_remove_error_paths(request, &field))
                }
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
        // Only the orderCustomerSet/Remove error-path flow's sentinel customer
        // (email "order-customer-...") is owned here; all other company-contact
        // assignments belong to the general b2b handler.
        let is_order_customer_flow = resolved_string_arg(&field.arguments, "customerId")
            .and_then(|customer_id| self.store.staged.customers.get(&customer_id).cloned())
            .and_then(|customer| customer["email"].as_str().map(str::to_string))
            .is_some_and(|email| email.starts_with("order-customer-"));
        if !is_order_customer_flow {
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
        let id = synthetic_shopify_gid("Order", self.store.staged.next_order_customer_order_id);
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
        // Retain the purchasing entity so a later company delete can detect that an
        // order still references the company (mirrors a real B2B Order).
        let purchasing_entity = match order_arg {
            ResolvedValue::Object(fields) => draft_order_purchasing_entity(fields),
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
        let refund_method_cancel = field.arguments.contains_key("refundMethod");
        let order_locally_known = self.store.staged.orders.contains_key(&order_id)
            || self
                .store
                .staged
                .order_customer_orders
                .contains_key(&order_id);
        // Earn the order from the backend when no precondition seed staged it.
        // Synthetic order-customer ids (seeded by orderCreate error-paths) live
        // in `order_customer_orders` and must not trigger an upstream read.
        //
        // A `refundMethod` (full original-payment-method refund) cancel is the one
        // case we deliberately do NOT stage: that mutation's authoritative
        // downstream order projection (the refund ledger and the restocked
        // fulfillment orders) is computed by the backend, not modelled in the
        // local overlay. We confirm the order exists upstream below, acknowledge
        // the cancel, and leave it unstaged so the downstream `order` read forwards
        // to the backend for the real refunded/restocked state instead of serving
        // a stale locally-projected copy.
        if !order_id.contains(SYNTHETIC_MARKER) && !order_locally_known && !refund_method_cancel {
            self.ensure_order_lifecycle_hydrated(request, &order_id);
        }
        let error_payload = |field_name: &str, message: &str, code: &str| {
            let error = user_error([field_name], message, Some(code));
            json!({
                "order": Value::Null,
                "job": Value::Null,
                "orderCancelUserErrors": [error.clone()],
                "userErrors": [error]
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

        // refundMethod cancel of an order not held in local overlay state: confirm
        // it exists upstream, acknowledge the cancel, and leave it unstaged so the
        // downstream order read forwards to the backend for the authoritative
        // refunded/restocked projection (see the staging note above).
        if refund_method_cancel && !order_locally_known {
            if !self.order_exists_upstream(request, &order_id) {
                return Some(selected_json(
                    &error_payload("orderId", "Order does not exist", "NOT_FOUND"),
                    &field.selection,
                ));
            }
            let job_id = synthetic_shopify_gid("Job", self.log_entries.len() + 1);
            self.record_orders_local_log_entry(OrdersLocalLogEntry {
                request,
                query,
                variables,
                root_field: "orderCancel",
                staged_resource_ids: Vec::new(),
                outcome: OrdersLocalLogOutcome {
                    status: "forwarded",
                    notes: "Acknowledged refundMethod orderCancel; downstream order read forwards upstream for the refunded/restocked projection.",
                },
            });
            return Some(selected_json(
                &json!({
                    "order": Value::Null,
                    "job": { "id": job_id, "done": false },
                    "orderCancelUserErrors": [],
                    "userErrors": []
                }),
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
            let job_id = synthetic_shopify_gid("Job", self.log_entries.len() + 1);
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
        request: &Request,
        field: &RootFieldSelection,
    ) -> Value {
        let order_id = resolved_string_arg(&field.arguments, "orderId").unwrap_or_default();
        let customer_id = resolved_string_arg(&field.arguments, "customerId").unwrap_or_default();
        // Earn order + customer from the backend on the happy path (no seed).
        // Synthetic error-path ids stay local-only.
        if !order_id.is_empty()
            && !order_id.contains(SYNTHETIC_MARKER)
            && !self
                .store
                .staged
                .order_customer_orders
                .contains_key(&order_id)
            && !self.store.staged.orders.contains_key(&order_id)
        {
            self.ensure_order_lifecycle_hydrated(request, &order_id);
        }
        if !customer_id.is_empty() && !customer_id.contains(SYNTHETIC_MARKER) {
            self.ensure_order_customer_hydrated(request, &customer_id);
        }
        let customer = self.store.staged.customers.get(&customer_id).cloned();
        let from_customer_map = self
            .store
            .staged
            .order_customer_orders
            .contains_key(&order_id);
        let Some(mut order) = self
            .store
            .staged
            .order_customer_orders
            .get(&order_id)
            .cloned()
            .or_else(|| self.store.staged.orders.get(&order_id).cloned())
        else {
            return selected_json(
                &json!({
                    "order": Value::Null,
                    "userErrors": [user_error(["orderId"], "Order does not exist", Some("NOT_FOUND"))]
                }),
                &field.selection,
            );
        };
        let Some(customer) = customer else {
            return selected_json(
                &json!({
                    "order": Value::Null,
                    "userErrors": [user_error(["customerId"], "Customer does not exist", Some("NOT_FOUND"))]
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
                    "userErrors": [user_error(["customerId"], "no_customer_role_error", Some("NOT_PERMITTED"))]
                }),
                &field.selection,
            );
        }
        order["customer"] = customer;
        if from_customer_map {
            self.store
                .staged
                .order_customer_orders
                .insert(order_id.clone(), order.clone());
        } else {
            self.store
                .staged
                .orders
                .insert(order_id.clone(), order.clone());
        }
        // Maintain the per-customer order index so the b2b `customer.orders`
        // connection reflects the association immediately (read-after-write):
        // detach the order from any prior owner, then attach the full (now
        // customer-bearing) order node to the new customer.
        self.detach_order_from_customer_orders(&order_id);
        self.store
            .staged
            .customer_orders
            .entry(customer_id.clone())
            .or_default()
            .push(order.clone());
        selected_json(
            &json!({ "order": order, "userErrors": [] }),
            &field.selection,
        )
    }

    /// Remove an order from every per-customer order index entry. Used when an
    /// order's customer association changes (set to a new owner / removed) so a
    /// later `customer.orders` read does not surface a stale link.
    fn detach_order_from_customer_orders(&mut self, order_id: &str) {
        for orders in self.store.staged.customer_orders.values_mut() {
            orders.retain(|order| order.get("id").and_then(Value::as_str) != Some(order_id));
        }
    }

    pub(in crate::proxy) fn order_customer_remove_error_paths(
        &mut self,
        request: &Request,
        field: &RootFieldSelection,
    ) -> Value {
        let order_id = resolved_string_arg(&field.arguments, "orderId").unwrap_or_default();
        if !order_id.is_empty()
            && !order_id.contains(SYNTHETIC_MARKER)
            && !self
                .store
                .staged
                .order_customer_orders
                .contains_key(&order_id)
            && !self.store.staged.orders.contains_key(&order_id)
        {
            self.ensure_order_lifecycle_hydrated(request, &order_id);
        }
        let from_customer_map = self
            .store
            .staged
            .order_customer_orders
            .contains_key(&order_id);
        let Some(mut order) = self
            .store
            .staged
            .order_customer_orders
            .get(&order_id)
            .cloned()
            .or_else(|| self.store.staged.orders.get(&order_id).cloned())
        else {
            return selected_json(
                &json!({
                    "order": Value::Null,
                    "userErrors": [user_error(["orderId"], "Order does not exist", Some("NOT_FOUND"))]
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
                    "userErrors": [user_error(["orderId"], "customer_cannot_be_removed", Some("INVALID"))]
                }),
                &field.selection,
            );
        }
        order["customer"] = Value::Null;
        if from_customer_map {
            self.store
                .staged
                .order_customer_orders
                .insert(order_id.clone(), order.clone());
        } else {
            self.store
                .staged
                .orders
                .insert(order_id.clone(), order.clone());
        }
        // The order is no longer attached to any customer: drop it from every
        // per-customer order index entry so `customer.orders` reads reflect the
        // removal.
        self.detach_order_from_customer_orders(&order_id);
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
        // Only claim bulk-tag mutations, tag-selecting creates, or a `draftOrder`
        // read whose id is actually tracked in this handler's tag state. A bare
        // `draftOrder` detail read of an untracked id must fall through to the
        // lifecycle handler / upstream passthrough rather than being shadowed
        // with a tags-only (or null) projection.
        let has_bulk_tag_root = fields.iter().any(|field| {
            matches!(
                field.name.as_str(),
                "draftOrderBulkAddTags" | "draftOrderBulkRemoveTags"
            ) || (field.name == "draftOrderCreate" && draft_order_create_selects_tags(field))
        });
        let has_managed_read = fields.iter().any(|field| {
            field.name == "draftOrder"
                && resolved_string_arg(&field.arguments, "id")
                    .or_else(|| resolved_string_arg(&field.arguments, "draftOrderId"))
                    .is_some_and(|id| {
                        self.store.staged.taggable_resources.contains_key(&id)
                            || self.store.staged.draft_order_tags.contains_key(&id)
                    })
        });
        if !has_bulk_tag_root && !has_managed_read {
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
        if !self.store.staged.draft_orders.contains_key(&id) {
            let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
            let mut record = draft_order_base_record(&id, "#D1", &input, None, &BTreeMap::new());
            self.recalculate_draft_order_totals(&mut record);
            self.store.staged.draft_orders.insert(id.clone(), record);
        }
        self.sync_draft_order_record_tags(&id);
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
            let mut updated_ids = Vec::new();
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
                    updated_ids.push(id);
                }
            }
            for id in updated_ids {
                self.sync_draft_order_record_tags(&id);
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
        let mut updated_ids = Vec::new();
        for (index, id) in ids.iter().enumerate() {
            if let Some(current) = self.store.staged.draft_order_tags.get_mut(id) {
                current.retain(|tag| !tags.contains(&normalize_draft_order_tag(tag)));
                updated_ids.push(id.clone());
            } else {
                user_errors.push(json!({
                    "field": ["input", "ids", index.to_string()],
                    "message": "Draft order does not exist",
                    "code": "NOT_FOUND"
                }));
            }
        }
        for id in updated_ids {
            self.sync_draft_order_record_tags(&id);
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

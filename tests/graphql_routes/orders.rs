use super::common::*;
use pretty_assertions::assert_eq;
use std::sync::atomic::{AtomicUsize, Ordering};

fn snapshot_proxy() -> DraftProxy {
    let mut proxy = super::common::snapshot_proxy();
    restore_shop_currency(&mut proxy, "USD");
    proxy
}

fn snapshot_proxy_with_clock(clock: Arc<Mutex<time::OffsetDateTime>>) -> DraftProxy {
    let mut proxy = super::common::snapshot_proxy_with_clock(clock);
    restore_shop_currency(&mut proxy, "USD");
    proxy
}

fn omit_unavailable_customer_card_digits(query: &str) -> String {
    query
        .lines()
        .filter(|line| !matches!(line.trim(), "lastDigits" | "maskedNumber"))
        .collect::<Vec<_>>()
        .join("\n")
        .replace("        ... on CustomerCreditCard {\n        }\n", "")
        .replace("          ... on CustomerCreditCard {\n          }\n", "")
}

fn omit_graphql_payload_object(query: &str, field: &str) -> String {
    let marker = format!("    {field} {{");
    let mut output = String::new();
    let mut skipped_depth = 0isize;
    for line in query.lines() {
        if skipped_depth == 0 && line == marker {
            skipped_depth = line.matches('{').count() as isize - line.matches('}').count() as isize;
            continue;
        }
        if skipped_depth > 0 {
            skipped_depth +=
                line.matches('{').count() as isize - line.matches('}').count() as isize;
            continue;
        }
        output.push_str(line);
        output.push('\n');
    }
    output
}

fn current_order_payment_document(query: &str) -> String {
    query
        .lines()
        .filter(|line| line.trim() != "paymentReferenceId")
        .collect::<Vec<_>>()
        .join("\n")
}

fn current_order_capture_document(query: &str) -> String {
    omit_graphql_payload_object(&current_order_payment_document(query), "order")
}

fn current_order_mandate_document(query: &str) -> String {
    omit_graphql_payload_object(query, "order")
}

fn read_order_payment_projection(proxy: &mut DraftProxy, id: Value) -> Value {
    proxy
        .process_request(json_graphql_request(
            r#"
            query ReadCurrentOrderPaymentProjection($id: ID!) {
              order(id: $id) {
                id
                displayFinancialStatus
                capturable
                totalCapturable
                totalCapturableSet { shopMoney { amount currencyCode } }
                totalOutstandingSet { shopMoney { amount currencyCode } }
                totalReceivedSet { shopMoney { amount currencyCode } }
                netPaymentSet { shopMoney { amount currencyCode } }
                paymentGatewayNames
                transactions {
                  id
                  kind
                  status
                  gateway
                  amountSet { shopMoney { amount currencyCode } }
                }
              }
            }
            "#,
            json!({ "id": id }),
        ))
        .body["data"]["order"]
        .clone()
}

fn read_preserved_mandate_order(proxy: &mut DraftProxy, id: Value) -> Value {
    proxy
        .process_request(json_graphql_request(
            r#"
            query ReadPreservedMandateOrder($id: ID!) {
              order(id: $id) {
                id
                name
                displayFinancialStatus
                customer { id }
                billingAddress { address1 city countryCodeV2 }
                shippingAddress { address1 city countryCodeV2 }
                lineItems(first: 10) { nodes { id title quantity } }
                paymentGatewayNames
                totalOutstandingSet { shopMoney { amount currencyCode } }
                totalReceivedSet { shopMoney { amount currencyCode } }
                transactions {
                  id
                  kind
                  status
                  gateway
                  amountSet { shopMoney { amount currencyCode } }
                }
              }
            }
            "#,
            json!({ "id": id }),
        ))
        .body["data"]["order"]
        .clone()
}

fn without_extensions(value: &Value) -> Value {
    let mut value = value.clone();
    if let Some(object) = value.as_object_mut() {
        object.remove("extensions");
    }
    value
}

fn assert_draft_order_variant_catalog_line(line: &Value, quantity: i64, currency_code: &str) {
    assert_eq!(line["title"], json!("Catalog product title"));
    assert_eq!(line["name"], json!("Catalog product title"));
    assert_eq!(line["sku"], json!("CATALOG-SKU"));
    assert_eq!(line["quantity"], json!(quantity));
    assert_eq!(line["custom"], json!(false));
    assert_eq!(line["requiresShipping"], json!(true));
    assert_eq!(line["taxable"], json!(true));
    assert_eq!(
        line["originalUnitPriceSet"]["shopMoney"],
        json!({ "amount": "19.95", "currencyCode": currency_code })
    );
    assert_eq!(
        line["variant"],
        json!({
            "id": "gid://shopify/ProductVariant/424242",
            "title": "Catalog option title",
            "sku": "CATALOG-SKU"
        })
    );
}

fn assert_draft_order_custom_line(line: &Value, currency_code: &str) {
    assert_eq!(line["title"], json!("Custom-only item"));
    assert_eq!(line["name"], json!("Custom-only item"));
    assert_eq!(line["sku"], json!("CUSTOM-SKU"));
    assert_eq!(line["quantity"], json!(1));
    assert_eq!(line["custom"], json!(true));
    assert_eq!(line["requiresShipping"], json!(false));
    assert_eq!(line["taxable"], json!(false));
    assert_eq!(
        line["originalUnitPriceSet"]["shopMoney"],
        json!({ "amount": "7.5", "currencyCode": currency_code })
    );
    assert_eq!(line["variant"], Value::Null);
}

fn draft_order_test_variant_node(id: &str) -> Value {
    let tail = id.rsplit('/').next().unwrap_or("unknown");
    json!({
        "__typename": "ProductVariant",
        "id": id,
        "title": format!("Catalog option {tail}"),
        "sku": format!("SKU-{tail}"),
        "taxable": true,
        "price": format!("{tail}.00"),
        "inventoryItem": { "requiresShipping": true },
        "product": { "title": format!("Catalog product {tail}") }
    })
}

fn draft_order_test_variant_response(id: &str) -> Value {
    let mut variant = draft_order_test_variant_node(id);
    variant
        .as_object_mut()
        .expect("variant node should be an object")
        .remove("__typename");
    json!({ "data": { "productVariant": variant } })
}

#[test]
fn order_create_uses_shop_currency_but_preserves_presentment_currency() {
    let mut proxy = snapshot_proxy();
    restore_shop_currency(&mut proxy, "EUR");

    let response = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateOrderWithPresentmentCurrency($order: OrderCreateOrderInput!) {
          orderCreate(order: $order) {
            order {
              currencyCode
              presentmentCurrencyCode
              totalPriceSet { shopMoney { amount currencyCode } }
              lineItems(first: 1) {
                nodes {
                  originalUnitPriceSet {
                    shopMoney { amount currencyCode }
                    presentmentMoney { amount currencyCode }
                  }
                }
              }
            }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "order": {
                "email": "presentment-currency@example.test",
                "presentmentCurrency": "USD",
                "lineItems": [{
                    "title": "Presentment line",
                    "quantity": 1,
                    "priceSet": {
                        "shopMoney": { "amount": "10.00", "currencyCode": "EUR" },
                        "presentmentMoney": { "amount": "12.00", "currencyCode": "USD" }
                    }
                }]
            }
        }),
    ));

    assert_eq!(response.status, 200);
    assert_eq!(
        response.body["data"]["orderCreate"]["userErrors"],
        json!([])
    );
    let order = &response.body["data"]["orderCreate"]["order"];
    assert_eq!(order["currencyCode"], json!("EUR"));
    assert_eq!(order["presentmentCurrencyCode"], json!("USD"));
    assert_eq!(
        order["totalPriceSet"]["shopMoney"],
        json!({ "amount": "10.0", "currencyCode": "EUR" })
    );
    assert_eq!(
        order["lineItems"]["nodes"][0]["originalUnitPriceSet"]["shopMoney"],
        json!({ "amount": "10.0", "currencyCode": "EUR" })
    );
    assert_eq!(
        order["lineItems"]["nodes"][0]["originalUnitPriceSet"]["presentmentMoney"],
        json!({ "amount": "12.0", "currencyCode": "USD" })
    );
}

fn stage_fulfillment_for_event(proxy: &mut DraftProxy) -> (Value, Value) {
    let create_order = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateFulfillmentEventOrder($order: OrderCreateOrderInput!) {
          orderCreate(order: $order) {
            order {
              id
              fulfillmentOrders(first: 5) {
                nodes {
                  id
                  lineItems(first: 5) {
                    nodes { id totalQuantity remainingQuantity }
                  }
                }
              }
            }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "order": {
                "email": "fulfillment-event@example.test",
                "lineItems": [{
                    "title": "Fulfillment event line",
                    "quantity": 1,
                    "priceSet": { "shopMoney": { "amount": "12.00", "currencyCode": "USD" } }
                }]
            }
        }),
    ));
    assert_eq!(create_order.status, 200);
    assert_eq!(
        create_order.body["data"]["orderCreate"]["userErrors"],
        json!([])
    );
    let order = &create_order.body["data"]["orderCreate"]["order"];
    let order_id = order["id"].clone();
    let fulfillment_order_id = order["fulfillmentOrders"]["nodes"][0]["id"].clone();

    let create_fulfillment = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateFulfillmentForEvent($fulfillment: FulfillmentInput!) {
          fulfillmentCreate(fulfillment: $fulfillment) {
            fulfillment {
              id
              status
              displayStatus
              events(first: 5) { nodes { id status } }
            }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "fulfillment": {
                "lineItemsByFulfillmentOrder": [{
                    "fulfillmentOrderId": fulfillment_order_id
                }]
            }
        }),
    ));
    assert_eq!(create_fulfillment.status, 200);
    assert_eq!(
        create_fulfillment.body["data"]["fulfillmentCreate"]["userErrors"],
        json!([])
    );
    (
        order_id,
        create_fulfillment.body["data"]["fulfillmentCreate"]["fulfillment"]["id"].clone(),
    )
}

fn create_fulfillment_validation_order(proxy: &mut DraftProxy) -> (Value, Vec<Value>) {
    let create_order = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateFulfillmentValidationOrder($order: OrderCreateOrderInput!) {
          orderCreate(order: $order) {
            order {
              id
              fulfillmentOrders(first: 5) {
                nodes {
                  id
                  lineItems(first: 5) {
                    nodes { id totalQuantity remainingQuantity }
                  }
                }
              }
            }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "order": {
                "email": "fulfillment-validation@example.test",
                "lineItems": [
                    {
                        "title": "Fulfillment validation first line",
                        "quantity": 2,
                        "priceSet": { "shopMoney": { "amount": "12.00", "currencyCode": "USD" } }
                    },
                    {
                        "title": "Fulfillment validation second line",
                        "quantity": 2,
                        "priceSet": { "shopMoney": { "amount": "18.00", "currencyCode": "USD" } }
                    }
                ]
            }
        }),
    ));
    assert_eq!(create_order.status, 200);
    assert_eq!(
        create_order.body["data"]["orderCreate"]["userErrors"],
        json!([])
    );
    let fulfillment_order =
        &create_order.body["data"]["orderCreate"]["order"]["fulfillmentOrders"]["nodes"][0];
    let line_ids = fulfillment_order["lineItems"]["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|line| line["id"].clone())
        .collect::<Vec<_>>();
    (fulfillment_order["id"].clone(), line_ids)
}

#[test]
fn fulfillment_order_supported_actions_follow_assignment_and_status() {
    let mut proxy = snapshot_proxy();
    let create_order = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateSupportedActionsOrder($order: OrderCreateOrderInput!) {
          orderCreate(order: $order) {
            order {
              id
              fulfillmentOrders(first: 5) {
                nodes {
                  id
                  lineItems(first: 5) { nodes { id totalQuantity remainingQuantity } }
                }
              }
            }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "order": {
                "email": "fulfillment-supported-actions@example.test",
                "lineItems": [{
                    "title": "Supported actions line",
                    "quantity": 2,
                    "priceSet": { "shopMoney": { "amount": "12.00", "currencyCode": "USD" } }
                }]
            }
        }),
    ));
    assert_eq!(
        create_order.body["data"]["orderCreate"]["userErrors"],
        json!([])
    );
    let order = &create_order.body["data"]["orderCreate"]["order"];
    let order_id = order["id"].clone();
    let fulfillment_order = &order["fulfillmentOrders"]["nodes"][0];
    let fulfillment_order_id = fulfillment_order["id"].clone();
    let fulfillment_order_line_item_id = fulfillment_order["lineItems"]["nodes"][0]["id"].clone();

    let split = proxy.process_request(json_graphql_request(
        r#"
        mutation SplitMerchantManagedFulfillmentOrder($splits: [FulfillmentOrderSplitInput!]!) {
          fulfillmentOrderSplit(fulfillmentOrderSplits: $splits) {
            fulfillmentOrderSplits {
              fulfillmentOrder {
                id
                status
                supportedActions { action }
                lineItems(first: 5) { nodes { id remainingQuantity } }
              }
            }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "splits": [{
                "fulfillmentOrderId": fulfillment_order_id,
                "fulfillmentOrderLineItems": [{
                    "id": fulfillment_order_line_item_id,
                    "quantity": 1
                }]
            }]
        }),
    ));
    assert_eq!(
        split.body["data"]["fulfillmentOrderSplit"]["userErrors"],
        json!([])
    );
    let split_order = &split.body["data"]["fulfillmentOrderSplit"]["fulfillmentOrderSplits"][0]
        ["fulfillmentOrder"];
    assert_eq!(split_order["status"], json!("OPEN"));
    let split_actions = split_order["supportedActions"].as_array().unwrap();
    assert!(
        split_actions
            .iter()
            .all(|action| action["action"] != json!("REPORT_PROGRESS")),
        "merchant-managed fulfillment order advertised REPORT_PROGRESS: {split_actions:?}"
    );

    let create_fulfillment = proxy.process_request(json_graphql_request(
        r#"
        mutation FullyFulfillMerchantManagedOrder($fulfillment: FulfillmentInput!) {
          fulfillmentCreate(fulfillment: $fulfillment) {
            fulfillment { id }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "fulfillment": {
                "lineItemsByFulfillmentOrder": [{
                    "fulfillmentOrderId": fulfillment_order_id
                }]
            }
        }),
    ));
    assert_eq!(
        create_fulfillment.body["data"]["fulfillmentCreate"]["userErrors"],
        json!([])
    );

    let closed_read = proxy.process_request(json_graphql_request(
        r#"
        query ReadClosedMerchantManagedFulfillmentOrder($orderId: ID!, $fulfillmentOrderId: ID!) {
          order(id: $orderId) {
            fulfillmentOrders(first: 5) {
              nodes { id status supportedActions { action } }
            }
          }
          fulfillmentOrder(id: $fulfillmentOrderId) {
            id
            status
            supportedActions { action }
          }
        }
        "#,
        json!({
            "orderId": order_id.clone(),
            "fulfillmentOrderId": fulfillment_order_id
        }),
    ));
    assert_eq!(
        closed_read.body["data"]["fulfillmentOrder"]["status"],
        json!("CLOSED")
    );
    assert_eq!(
        closed_read.body["data"]["fulfillmentOrder"]["supportedActions"],
        json!([])
    );
    assert_eq!(
        closed_read.body["data"]["order"]["fulfillmentOrders"]["nodes"][0]["supportedActions"],
        json!([])
    );
}

#[test]
fn fulfillment_create_rejects_non_positive_line_item_quantity_with_indexed_path() {
    let mut proxy = snapshot_proxy();
    let (fulfillment_order_id, line_ids) = create_fulfillment_validation_order(&mut proxy);

    for root in ["fulfillmentCreate", "fulfillmentCreateV2"] {
        for quantity in [0, -1] {
            let response = proxy.process_request(json_graphql_request(
                &format!(
                    r#"
                    mutation FulfillmentCreateNonPositiveQuantity($fulfillment: FulfillmentInput!) {{
                      {root}(fulfillment: $fulfillment) {{
                        fulfillment {{ id }}
                        userErrors {{ field message }}
                      }}
                    }}
                    "#
                ),
                json!({
                    "fulfillment": {
                        "lineItemsByFulfillmentOrder": [{
                            "fulfillmentOrderId": fulfillment_order_id,
                            "fulfillmentOrderLineItems": [
                                {
                                    "id": line_ids[0],
                                    "quantity": 1
                                },
                                {
                                    "id": line_ids[1],
                                    "quantity": quantity
                                }
                            ]
                        }]
                    }
                }),
            ));

            assert_eq!(response.status, 200, "{root} quantity {quantity}");
            assert_eq!(
                response.body["data"][root]["userErrors"],
                json!([{
                    "field": [
                        "fulfillment",
                        "lineItemsByFulfillmentOrder",
                        "0",
                        "fulfillmentOrderLineItems",
                        "1",
                        "quantity"
                    ],
                    "message": "Quantity must be greater than 0"
                }]),
                "{root} quantity {quantity}"
            );
            assert_eq!(
                response.body["data"][root]["fulfillment"],
                Value::Null,
                "{root} quantity {quantity}"
            );
        }
    }
}

#[test]
fn fulfillment_create_rejects_missing_fulfillment_order_with_shopify_message() {
    let mut proxy = snapshot_proxy();

    for root in ["fulfillmentCreate", "fulfillmentCreateV2"] {
        let response = proxy.process_request(json_graphql_request(
            &format!(
                r#"
                mutation FulfillmentCreateMissingOrder($fulfillment: FulfillmentInput!) {{
                  {root}(fulfillment: $fulfillment) {{
                    fulfillment {{ id }}
                    userErrors {{ field message }}
                  }}
                }}
                "#
            ),
            json!({
                "fulfillment": {
                    "lineItemsByFulfillmentOrder": [{
                        "fulfillmentOrderId": "gid://shopify/FulfillmentOrder/999999999",
                        "fulfillmentOrderLineItems": [{
                            "id": "gid://shopify/FulfillmentOrderLineItem/999999998",
                            "quantity": 1
                        }]
                    }]
                }
            }),
        ));

        assert_eq!(response.status, 200, "{root}");
        assert_eq!(
            response.body["data"][root],
            json!({
                "fulfillment": null,
                "userErrors": [{
                    "field": ["fulfillment"],
                    "message": "Fulfillment order does not exist."
                }]
            }),
            "{root}"
        );
    }
    assert_eq!(log_snapshot(&proxy)["entries"], json!([]));
}

#[test]
fn fulfillment_create_preserves_over_remaining_quantity_error() {
    let mut proxy = snapshot_proxy();
    let (fulfillment_order_id, line_ids) = create_fulfillment_validation_order(&mut proxy);

    for root in ["fulfillmentCreate", "fulfillmentCreateV2"] {
        let response = proxy.process_request(json_graphql_request(
            &format!(
                r#"
                mutation FulfillmentCreateOverQuantity($fulfillment: FulfillmentInput!) {{
                  {root}(fulfillment: $fulfillment) {{
                    fulfillment {{ id }}
                    userErrors {{ field message }}
                  }}
                }}
                "#
            ),
            json!({
                "fulfillment": {
                    "lineItemsByFulfillmentOrder": [{
                        "fulfillmentOrderId": fulfillment_order_id,
                        "fulfillmentOrderLineItems": [{
                            "id": line_ids[0],
                            "quantity": 3
                        }]
                    }]
                }
            }),
        ));

        assert_eq!(response.status, 200, "{root}");
        assert_eq!(
            response.body["data"][root],
            json!({
                "fulfillment": null,
                "userErrors": [{
                    "field": ["fulfillment"],
                    "message": "Invalid fulfillment order line item quantity requested."
                }]
            }),
            "{root}"
        );
    }
}

struct ReturnRemovalSetup {
    order_id: Value,
    return_id: Value,
    return_line_item_id: Value,
}

fn stage_fulfilled_order_for_return(proxy: &mut DraftProxy) -> (Value, Value) {
    let create_order = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateReturnRemovalOrder($order: OrderCreateOrderInput!) {
          orderCreate(order: $order) {
            order {
              id
              fulfillmentOrders(first: 5) {
                nodes {
                  id
                  lineItems(first: 5) {
                    nodes { id totalQuantity remainingQuantity }
                  }
                }
              }
            }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "order": {
                "email": "return-removal-status@example.test",
                "lineItems": [{
                    "title": "Return removal status line",
                    "quantity": 2,
                    "priceSet": { "shopMoney": { "amount": "10.00", "currencyCode": "USD" } },
                    "taxLines": [{
                        "title": "State tax",
                        "rate": 0.1,
                        "priceSet": { "shopMoney": { "amount": "2.00", "currencyCode": "USD" } }
                    }]
                }]
            }
        }),
    ));
    assert_eq!(create_order.status, 200);
    assert_eq!(
        create_order.body["data"]["orderCreate"]["userErrors"],
        json!([])
    );
    let order = &create_order.body["data"]["orderCreate"]["order"];
    let order_id = order["id"].clone();
    let fulfillment_order_id = order["fulfillmentOrders"]["nodes"][0]["id"].clone();
    let fulfillment_order_line_item_id =
        order["fulfillmentOrders"]["nodes"][0]["lineItems"]["nodes"][0]["id"].clone();

    let create_fulfillment = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateReturnRemovalFulfillment($fulfillment: FulfillmentInput!) {
          fulfillmentCreate(fulfillment: $fulfillment) {
            fulfillment {
              id
              status
              fulfillmentLineItems(first: 5) {
                nodes { id quantity lineItem { id title } }
              }
            }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "fulfillment": {
                "lineItemsByFulfillmentOrder": [{
                    "fulfillmentOrderId": fulfillment_order_id,
                    "fulfillmentOrderLineItems": [{
                        "id": fulfillment_order_line_item_id,
                        "quantity": 2
                    }]
                }]
            }
        }),
    ));
    assert_eq!(create_fulfillment.status, 200);
    assert_eq!(
        create_fulfillment.body["data"]["fulfillmentCreate"]["userErrors"],
        json!([])
    );
    let fulfillment_line_item_id = create_fulfillment.body["data"]["fulfillmentCreate"]
        ["fulfillment"]["fulfillmentLineItems"]["nodes"][0]["id"]
        .clone();
    (order_id, fulfillment_line_item_id)
}

fn return_removal_setup_from_payload(order_id: Value, payload: &Value) -> ReturnRemovalSetup {
    assert_eq!(payload["userErrors"], json!([]));
    ReturnRemovalSetup {
        order_id,
        return_id: payload["return"]["id"].clone(),
        return_line_item_id: payload["return"]["returnLineItems"]["nodes"][0]["id"].clone(),
    }
}

#[test]
fn fulfillment_plain_user_errors_reject_code_selection() {
    let mut proxy = snapshot_proxy();

    let response = proxy.process_request(json_graphql_request(
        r#"
        mutation FulfillmentPlainUserErrorCodeSelectionRejected {
          create: fulfillmentCreate(fulfillment: {
            lineItemsByFulfillmentOrder: [{ fulfillmentOrderId: "gid://shopify/FulfillmentOrder/1" }]
          }) {
            userErrors { code }
          }
          createV2: fulfillmentCreateV2(fulfillment: {
            lineItemsByFulfillmentOrder: [{ fulfillmentOrderId: "gid://shopify/FulfillmentOrder/1" }]
          }) {
            userErrors { code }
          }
          cancel: fulfillmentCancel(id: "gid://shopify/Fulfillment/1") {
            userErrors { code }
          }
          tracking: fulfillmentTrackingInfoUpdate(
            fulfillmentId: "gid://shopify/Fulfillment/1"
            trackingInfoInput: { number: "TRACK-1" }
          ) {
            userErrors { code }
          }
          trackingV2: fulfillmentTrackingInfoUpdateV2(
            fulfillmentId: "gid://shopify/Fulfillment/1"
            trackingInfoInput: { number: "TRACK-1" }
          ) {
            userErrors { code }
          }
          event: fulfillmentEventCreate(fulfillmentEvent: {
            fulfillmentId: "gid://shopify/Fulfillment/1"
            status: IN_TRANSIT
          }) {
            userErrors { code }
          }
        }
        "#,
        json!({}),
    ));

    assert_eq!(response.status, 200);
    assert!(response.body.get("data").is_none());
    let errors = response.body["errors"].as_array().unwrap();
    assert_eq!(errors.len(), 6);
    for (error, response_key) in errors.iter().zip([
        "create",
        "createV2",
        "cancel",
        "tracking",
        "trackingV2",
        "event",
    ]) {
        assert_eq!(
            error["message"],
            json!("Field 'code' doesn't exist on type 'UserError'")
        );
        assert_eq!(
            error["path"],
            json!([
                "mutation FulfillmentPlainUserErrorCodeSelectionRejected",
                response_key,
                "userErrors",
                "code"
            ])
        );
        assert_eq!(
            error["extensions"],
            json!({
                "code": "undefinedField",
                "typeName": "UserError",
                "fieldName": "code"
            })
        );
    }
}

fn stage_open_return_for_removal(proxy: &mut DraftProxy) -> ReturnRemovalSetup {
    let (order_id, fulfillment_line_item_id) = stage_fulfilled_order_for_return(proxy);
    let response = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateOpenReturnForRemoval($returnInput: ReturnInput!) {
          returnCreate(returnInput: $returnInput) {
            return {
              id
              status
              totalQuantity
              returnLineItems(first: 5) {
                nodes { id quantity processedQuantity unprocessedQuantity }
              }
              reverseFulfillmentOrders(first: 5) {
                nodes {
                  id
                  lineItems(first: 5) {
                    nodes { id totalQuantity fulfillmentLineItem { id } }
                  }
                }
              }
            }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "returnInput": {
                "orderId": order_id,
                "returnLineItems": [{
                    "fulfillmentLineItemId": fulfillment_line_item_id,
                    "quantity": 2,
                    "returnReason": "UNWANTED"
                }]
            }
        }),
    ));
    assert_eq!(response.status, 200);
    return_removal_setup_from_payload(order_id, &response.body["data"]["returnCreate"])
}

#[test]
fn returnable_fulfillments_and_return_calculate_derive_from_staged_fulfillments() {
    let upstream_calls = Arc::new(AtomicUsize::new(0));
    let mut proxy = configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport({
        let upstream_calls = Arc::clone(&upstream_calls);
        move |request| {
            upstream_calls.fetch_add(1, Ordering::SeqCst);
            panic!(
                "staged return query roots should not call upstream: {}",
                request.body
            )
        }
    });
    restore_shop_currency(&mut proxy, "USD");
    let (order_id, fulfillment_line_item_id) = stage_fulfilled_order_for_return(&mut proxy);

    let returnables = proxy.process_request(json_graphql_request(
        r#"
        query ReturnableFulfillmentsForStagedOrder($orderId: ID!) {
          returnableFulfillments(orderId: $orderId, first: 5) {
            nodes {
              fulfillment { id }
              returnableFulfillmentLineItems(first: 5) {
                nodes {
                  fulfillmentLineItem { id }
                  quantity
                }
              }
            }
          }
        }
        "#,
        json!({ "orderId": order_id.clone() }),
    ));
    assert_eq!(returnables.status, 200);
    let returnable_nodes = returnables.body["data"]["returnableFulfillments"]["nodes"]
        .as_array()
        .unwrap();
    assert_eq!(returnable_nodes.len(), 1);
    assert_eq!(
        returnable_nodes[0]["returnableFulfillmentLineItems"]["nodes"],
        json!([{
            "fulfillmentLineItem": { "id": fulfillment_line_item_id.clone() },
            "quantity": 2
        }])
    );

    let calculated = proxy.process_request(json_graphql_request(
        r#"
        query CalculateReturnForStagedOrder($orderId: ID!, $fulfillmentLineItemId: ID!) {
          returnCalculate(input: {
            orderId: $orderId
            returnLineItems: [{
              fulfillmentLineItemId: $fulfillmentLineItemId
              quantity: 1
              restockingFee: { percentage: 10.0 }
            }]
          }) {
            returnLineItems {
              fulfillmentLineItem { id }
              quantity
              restockingFee {
                id
                percentage
                amountSet { shopMoney { amount currencyCode } }
              }
              subtotalBeforeOrderDiscountsSet { shopMoney { amount currencyCode } }
              subtotalSet { shopMoney { amount currencyCode } }
              totalTaxSet { shopMoney { amount currencyCode } }
            }
            returnShippingFee { id }
          }
        }
        "#,
        json!({
            "orderId": order_id.clone(),
            "fulfillmentLineItemId": fulfillment_line_item_id.clone()
        }),
    ));
    assert_eq!(calculated.status, 200);
    assert_eq!(
        calculated.body["data"]["returnCalculate"]["returnLineItems"],
        json!([{
            "fulfillmentLineItem": { "id": fulfillment_line_item_id.clone() },
            "quantity": 1,
            "restockingFee": {
                "id": "gid://shopify/CalculatedRestockingFee/1",
                "percentage": 10.0,
                "amountSet": {
                    "shopMoney": { "amount": "1.0", "currencyCode": "USD" }
                }
            },
            "subtotalBeforeOrderDiscountsSet": {
                "shopMoney": { "amount": "-10.0", "currencyCode": "USD" }
            },
            "subtotalSet": {
                "shopMoney": { "amount": "-10.0", "currencyCode": "USD" }
            },
            "totalTaxSet": {
                "shopMoney": { "amount": "-1.0", "currencyCode": "USD" }
            }
        }])
    );
    assert_eq!(
        calculated.body["data"]["returnCalculate"]["returnShippingFee"],
        Value::Null
    );

    let create_return = proxy.process_request(json_graphql_request(
        r#"
        mutation ConsumeOneReturnableLine($returnInput: ReturnInput!) {
          returnCreate(returnInput: $returnInput) {
            return { id }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "returnInput": {
                "orderId": order_id.clone(),
                "returnLineItems": [{
                    "fulfillmentLineItemId": fulfillment_line_item_id.clone(),
                    "quantity": 1,
                    "returnReason": "UNWANTED"
                }]
            }
        }),
    ));
    assert_eq!(create_return.status, 200);
    assert_eq!(
        create_return.body["data"]["returnCreate"]["userErrors"],
        json!([])
    );

    let after_return = proxy.process_request(json_graphql_request(
        r#"
        query ReturnableFulfillmentsAfterPartialReturn($orderId: ID!) {
          returnableFulfillments(orderId: $orderId, first: 5) {
            nodes {
              returnableFulfillmentLineItems(first: 5) {
                nodes { fulfillmentLineItem { id } quantity }
              }
            }
          }
        }
        "#,
        json!({ "orderId": order_id }),
    ));
    assert_eq!(
        after_return.body["data"]["returnableFulfillments"]["nodes"][0]
            ["returnableFulfillmentLineItems"]["nodes"],
        json!([{
            "fulfillmentLineItem": { "id": fulfillment_line_item_id },
            "quantity": 1
        }])
    );
    assert_eq!(upstream_calls.load(Ordering::SeqCst), 0);
}

fn run_generic_node_resolves_fulfillment_resources_from_staged_order_graph() {
    let mut proxy = snapshot_proxy();
    let create_order = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateFulfillmentNodeOrder($order: OrderCreateOrderInput!) {
          orderCreate(order: $order) {
            order {
              id
              name
              fulfillmentOrders(first: 5) {
                nodes {
                  id
                  status
                  lineItems(first: 5) {
                    nodes { id totalQuantity remainingQuantity lineItem { id title } }
                  }
                }
              }
            }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "order": {
                "email": "fulfillment-node@example.test",
                "lineItems": [{
                    "title": "Fulfillment node line",
                    "quantity": 2,
                    "priceSet": { "shopMoney": { "amount": "12.00", "currencyCode": "USD" } }
                }]
            }
        }),
    ));
    assert_eq!(create_order.status, 200);
    assert_eq!(
        create_order.body["data"]["orderCreate"]["userErrors"],
        json!([])
    );
    let order = &create_order.body["data"]["orderCreate"]["order"];
    let order_id = order["id"].clone();
    let fulfillment_order_id = order["fulfillmentOrders"]["nodes"][0]["id"].clone();
    let fulfillment_order_line_item_id =
        order["fulfillmentOrders"]["nodes"][0]["lineItems"]["nodes"][0]["id"].clone();

    let hold = proxy.process_request(json_graphql_request(
        r#"
        mutation HoldFulfillmentNodeOrder($id: ID!, $fulfillmentHold: FulfillmentOrderHoldInput!) {
          fulfillmentOrderHold(id: $id, fulfillmentHold: $fulfillmentHold) {
            fulfillmentHold { id handle reason displayReason }
            fulfillmentOrder { id status fulfillmentHolds { id handle reason displayReason } }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "id": fulfillment_order_id,
            "fulfillmentHold": {
                "reason": "AWAITING_RETURN_ITEMS",
                "reasonNotes": "node coverage",
                "handle": "node-hold"
            }
        }),
    ));
    assert_eq!(hold.status, 200);
    assert_eq!(
        hold.body["data"]["fulfillmentOrderHold"]["userErrors"],
        json!([])
    );
    let fulfillment_hold_id =
        hold.body["data"]["fulfillmentOrderHold"]["fulfillmentHold"]["id"].clone();

    let held_node = proxy.process_request(json_graphql_request(
        r#"
        query HeldFulfillmentNode($holdId: ID!, $orderId: ID!, $lineId: ID!) {
          held: node(id: $holdId) {
            __typename
            ... on FulfillmentHold { id handle reason displayReason }
          }
          batch: nodes(ids: [$orderId, $lineId, $orderId]) {
            __typename
            ... on FulfillmentOrder {
              id
              status
              order { id }
              fulfillmentHolds { id handle reason displayReason }
            }
            ... on FulfillmentOrderLineItem {
              id
              totalQuantity
              remainingQuantity
              lineItem { id title }
            }
          }
        }
        "#,
        json!({
            "holdId": fulfillment_hold_id,
            "orderId": fulfillment_order_id,
            "lineId": fulfillment_order_line_item_id
        }),
    ));
    assert_eq!(held_node.status, 200);
    assert_eq!(
        held_node.body["data"]["held"],
        json!({
            "__typename": "FulfillmentHold",
            "id": fulfillment_hold_id,
            "handle": "node-hold",
            "reason": "AWAITING_RETURN_ITEMS",
            "displayReason": "Exchange items awaiting return delivery"
        })
    );
    assert_eq!(held_node.body["data"]["batch"][0]["order"]["id"], order_id);
    assert_eq!(
        held_node.body["data"]["batch"][0]["status"],
        json!("ON_HOLD")
    );
    assert_eq!(
        held_node.body["data"]["batch"][1]["lineItem"]["title"],
        json!("Fulfillment node line")
    );
    assert_eq!(
        held_node.body["data"]["batch"][2]["id"],
        fulfillment_order_id
    );

    let release = proxy.process_request(json_graphql_request(
        r#"
        mutation ReleaseFulfillmentNodeOrder($id: ID!, $holdIds: [ID!]) {
          fulfillmentOrderReleaseHold(id: $id, holdIds: $holdIds) {
            fulfillmentOrder { id status fulfillmentHolds { id } }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "id": fulfillment_order_id, "holdIds": [fulfillment_hold_id.clone()] }),
    ));
    assert_eq!(
        release.body["data"]["fulfillmentOrderReleaseHold"]["fulfillmentOrder"]["status"],
        json!("OPEN")
    );

    let released_hold = proxy.process_request(json_graphql_request(
        r#"query ReleasedFulfillmentHoldNode($id: ID!) { node(id: $id) { id } }"#,
        json!({ "id": fulfillment_hold_id }),
    ));
    assert_eq!(released_hold.body["data"]["node"], Value::Null);

    let fulfillment = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateFulfillmentNode($fulfillment: FulfillmentInput!) {
          fulfillmentCreate(fulfillment: $fulfillment) {
            fulfillment {
              id
              status
              displayStatus
              fulfillmentLineItems(first: 5) {
                nodes { id quantity lineItem { id title } }
              }
            }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "fulfillment": {
                "lineItemsByFulfillmentOrder": [{
                    "fulfillmentOrderId": fulfillment_order_id
                }]
            }
        }),
    ));
    assert_eq!(
        fulfillment.body["data"]["fulfillmentCreate"]["userErrors"],
        json!([])
    );
    let fulfillment_id = fulfillment.body["data"]["fulfillmentCreate"]["fulfillment"]["id"].clone();
    let fulfillment_line_item_id = fulfillment.body["data"]["fulfillmentCreate"]["fulfillment"]
        ["fulfillmentLineItems"]["nodes"][0]["id"]
        .clone();

    let event = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateFulfillmentNodeEvent($fulfillmentEvent: FulfillmentEventInput!) {
          fulfillmentEventCreate(fulfillmentEvent: $fulfillmentEvent) {
            fulfillmentEvent { id status message }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "fulfillmentEvent": {
                "fulfillmentId": fulfillment_id,
                "status": "IN_TRANSIT",
                "message": "Carrier pickup"
            }
        }),
    ));
    assert_eq!(
        event.body["data"]["fulfillmentEventCreate"]["userErrors"],
        json!([])
    );
    let event_id = event.body["data"]["fulfillmentEventCreate"]["fulfillmentEvent"]["id"].clone();

    let read = proxy.process_request(json_graphql_request(
        r#"
        query FulfillmentGraphNodeRead($ids: [ID!]!, $fulfillmentId: ID!) {
          fulfillmentNode: node(id: $fulfillmentId) {
            __typename
            ... on Fulfillment {
              id
              status
              displayStatus
              order { id name }
              events(first: 5) { nodes { id status message } }
              fulfillmentLineItems(first: 5) {
                nodes { id quantity lineItem { id title } }
              }
            }
          }
          batch: nodes(ids: $ids) {
            __typename
            ... on Fulfillment { id status displayStatus }
            ... on FulfillmentLineItem { id quantity lineItem { id title } }
            ... on FulfillmentEvent { id status message }
            ... on FulfillmentOrder { id status }
          }
        }
        "#,
        json!({
            "fulfillmentId": fulfillment_id,
            "ids": [
                fulfillment_id,
                fulfillment_line_item_id,
                fulfillment_id,
                event_id,
                fulfillment_order_id,
                "gid://shopify/Fulfillment/999999999"
            ]
        }),
    ));
    assert_eq!(
        read.body["data"]["fulfillmentNode"]["__typename"],
        json!("Fulfillment")
    );
    assert_eq!(
        read.body["data"]["fulfillmentNode"]["order"]["id"],
        order_id
    );
    assert_eq!(
        read.body["data"]["fulfillmentNode"]["displayStatus"],
        json!("IN_TRANSIT")
    );
    assert_eq!(read.body["data"]["batch"][0], read.body["data"]["batch"][2]);
    assert_eq!(
        read.body["data"]["batch"][1]["lineItem"]["title"],
        json!("Fulfillment node line")
    );
    assert_eq!(
        read.body["data"]["batch"][3]["message"],
        json!("Carrier pickup")
    );
    assert_eq!(read.body["data"]["batch"][4]["status"], json!("CLOSED"));
    assert_eq!(read.body["data"]["batch"][5], Value::Null);

    let tracking = proxy.process_request(json_graphql_request(
        r#"
        mutation UpdateFulfillmentNodeTracking($fulfillmentId: ID!) {
          fulfillmentTrackingInfoUpdate(
            fulfillmentId: $fulfillmentId
            trackingInfoInput: { number: "TRACK-NODE", url: "https://tracking.example/TRACK-NODE", company: "Node Carrier" }
          ) {
            fulfillment { id trackingInfo { number url company } }
            userErrors { field message }
          }
        }
        "#,
        json!({ "fulfillmentId": fulfillment_id }),
    ));
    assert_eq!(
        tracking.body["data"]["fulfillmentTrackingInfoUpdate"]["userErrors"],
        json!([])
    );
    let tracked_node = proxy.process_request(json_graphql_request(
        r#"
        query TrackedFulfillmentNode($id: ID!) {
          node(id: $id) {
            ... on Fulfillment {
              displayStatus
              trackingInfo { number url company }
            }
          }
        }
        "#,
        json!({ "id": fulfillment_id }),
    ));
    assert_eq!(
        tracked_node.body["data"]["node"]["trackingInfo"],
        json!([{
            "number": "TRACK-NODE",
            "url": "https://tracking.example/TRACK-NODE",
            "company": "Node Carrier"
        }])
    );
    assert_eq!(
        tracked_node.body["data"]["node"]["displayStatus"],
        json!("IN_TRANSIT")
    );

    let dump = proxy.process_request(request_with_body("POST", "/__meta/dump", ""));
    assert_eq!(dump.status, 200);
    let restore = proxy.process_request(request_with_body(
        "POST",
        "/__meta/restore",
        &dump.body.to_string(),
    ));
    assert_eq!(restore.status, 200);
    let restored = proxy.process_request(json_graphql_request(
        r#"
        query RestoredFulfillmentNode($id: ID!) {
          node(id: $id) {
            ... on Fulfillment { id displayStatus events(first: 5) { nodes { id status } } }
          }
        }
        "#,
        json!({ "id": fulfillment_id }),
    ));
    assert_eq!(
        restored.body["data"]["node"]["events"]["nodes"][0]["id"],
        event_id
    );

    let cancel = proxy.process_request(json_graphql_request(
        r#"
        mutation CancelFulfillmentNode($id: ID!) {
          fulfillmentCancel(id: $id) {
            fulfillment { id status displayStatus }
            userErrors { field message }
          }
        }
        "#,
        json!({ "id": fulfillment_id }),
    ));
    assert_eq!(
        cancel.body["data"]["fulfillmentCancel"]["userErrors"],
        json!([])
    );
    let canceled_node = proxy.process_request(json_graphql_request(
        r#"
        query CanceledFulfillmentNode($id: ID!) {
          node(id: $id) { ... on Fulfillment { id status displayStatus } }
        }
        "#,
        json!({ "id": fulfillment_id }),
    ));
    assert_eq!(
        canceled_node.body["data"]["node"]["status"],
        json!("CANCELLED")
    );
    assert_eq!(
        canceled_node.body["data"]["node"]["displayStatus"],
        json!("CANCELED")
    );

    let delete = proxy.process_request(json_graphql_request(
        r#"
        mutation DeleteFulfillmentNodeOrder($orderId: ID!) {
          orderDelete(orderId: $orderId) { deletedId userErrors { field message code } }
        }
        "#,
        json!({ "orderId": order_id }),
    ));
    assert_eq!(delete.body["data"]["orderDelete"]["userErrors"], json!([]));
    let deleted = proxy.process_request(json_graphql_request(
        r#"
        query DeletedFulfillmentNodes($ids: [ID!]!) {
          nodes(ids: $ids) {
            __typename
            ... on Fulfillment { id }
            ... on FulfillmentLineItem { id }
            ... on FulfillmentEvent { id }
            ... on FulfillmentOrder { id }
            ... on FulfillmentOrderLineItem { id }
          }
        }
        "#,
        json!({
            "ids": [
                fulfillment_id,
                fulfillment_line_item_id,
                event_id,
                fulfillment_order_id,
                fulfillment_order_line_item_id
            ]
        }),
    ));
    assert_eq!(
        deleted.body["data"]["nodes"],
        json!([
            Value::Null,
            Value::Null,
            Value::Null,
            Value::Null,
            Value::Null
        ])
    );
}

fn run_generic_node_reflects_fulfillment_order_move_split_and_merge() {
    let mut proxy = snapshot_proxy();
    let (fulfillment_order_id, line_ids) = create_fulfillment_validation_order(&mut proxy);
    let line_id = line_ids[0].clone();

    let split = proxy.process_request(json_graphql_request(
        r#"
        mutation SplitFulfillmentOrderNodeReadback($splits: [FulfillmentOrderSplitInput!]!) {
          fulfillmentOrderSplit(fulfillmentOrderSplits: $splits) {
            fulfillmentOrderSplits {
              fulfillmentOrder {
                id
                status
                lineItems(first: 5) { nodes { id totalQuantity remainingQuantity } }
              }
              remainingFulfillmentOrder {
                id
                status
                lineItems(first: 5) { nodes { id totalQuantity remainingQuantity } }
              }
            }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "splits": [{
                "fulfillmentOrderId": fulfillment_order_id,
                "fulfillmentOrderLineItems": [{ "id": line_id, "quantity": 1 }]
            }]
        }),
    ));
    let split_payload = &split.body["data"]["fulfillmentOrderSplit"];
    assert_eq!(split_payload["userErrors"], json!([]));
    let split_original = &split_payload["fulfillmentOrderSplits"][0]["fulfillmentOrder"];
    let split_remaining = &split_payload["fulfillmentOrderSplits"][0]["remainingFulfillmentOrder"];
    let split_original_id = split_original["id"].clone();
    let remaining_id = split_remaining["id"].clone();

    let split_nodes = proxy.process_request(json_graphql_request(
        r#"
        query SplitFulfillmentOrderNodes($ids: [ID!]!) {
          nodes(ids: $ids) {
            __typename
            ... on FulfillmentOrder {
              id
              status
              lineItems(first: 5) { nodes { id totalQuantity remainingQuantity } }
            }
          }
        }
        "#,
        json!({ "ids": [split_original_id.clone(), remaining_id.clone()] }),
    ));
    assert_eq!(
        split_nodes.body.get("errors"),
        None,
        "{:?}",
        split_nodes.body
    );
    assert_eq!(
        split_nodes.body["data"]["nodes"][0]["lineItems"], split_original["lineItems"],
        "{:?}",
        split_nodes.body
    );
    assert_eq!(
        split_nodes.body["data"]["nodes"][1]["lineItems"],
        split_remaining["lineItems"]
    );

    let merge = proxy.process_request(json_graphql_request(
        r#"
        mutation MergeFulfillmentOrderNodeReadback($inputs: [FulfillmentOrderMergeInput!]!) {
          fulfillmentOrderMerge(fulfillmentOrderMergeInputs: $inputs) {
            fulfillmentOrderMerges {
              fulfillmentOrder {
                id
                status
                lineItems(first: 5) { nodes { id totalQuantity remainingQuantity } }
              }
            }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "inputs": [{
                "mergeIntents": [
                    { "fulfillmentOrderId": split_original_id.clone() },
                    { "fulfillmentOrderId": remaining_id.clone() }
                ]
            }]
        }),
    ));
    let merge_payload = &merge.body["data"]["fulfillmentOrderMerge"];
    assert_eq!(merge_payload["userErrors"], json!([]));
    let merged_fulfillment_order = &merge_payload["fulfillmentOrderMerges"][0]["fulfillmentOrder"];

    let merge_nodes = proxy.process_request(json_graphql_request(
        r#"
        query MergeFulfillmentOrderNodes($ids: [ID!]!) {
          nodes(ids: $ids) {
            __typename
            ... on FulfillmentOrder {
              id
              status
              lineItems(first: 5) { nodes { id totalQuantity remainingQuantity } }
            }
          }
        }
        "#,
        json!({ "ids": [split_original_id, remaining_id] }),
    ));
    assert_eq!(
        merge_nodes.body["data"]["nodes"][0]["lineItems"],
        merged_fulfillment_order["lineItems"]
    );
    assert_eq!(
        merge_nodes.body["data"]["nodes"][1]["status"],
        json!("CLOSED")
    );

    let (move_fulfillment_order_id, move_line_ids) =
        create_fulfillment_validation_order(&mut proxy);
    let move_line_id = move_line_ids[0].clone();
    let destination = proxy.process_request(json_graphql_request(
        r#"
        mutation SeedFulfillmentOrderNodeMoveDestination($input: LocationAddInput!) {
          locationAdd(input: $input) {
            location { id name }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "input": {
                "name": "Node Move Destination",
                "address": { "countryCode": "CA" }
            }
        }),
    ));
    assert_eq!(
        destination.body["data"]["locationAdd"]["userErrors"],
        json!([])
    );
    let destination_location = destination.body["data"]["locationAdd"]["location"].clone();
    let destination_location_id = destination_location["id"].clone();

    let move_response = proxy.process_request(json_graphql_request(
        r#"
        mutation MoveFulfillmentOrderNodeReadback(
          $id: ID!
          $newLocationId: ID!
          $fulfillmentOrderLineItems: [FulfillmentOrderLineItemInput!]
        ) {
          fulfillmentOrderMove(
            id: $id
            newLocationId: $newLocationId
            fulfillmentOrderLineItems: $fulfillmentOrderLineItems
          ) {
            movedFulfillmentOrder {
              id
              status
              assignedLocation { name location { id name } }
              lineItems(first: 5) { nodes { remainingQuantity } }
            }
            originalFulfillmentOrder {
              id
              lineItems(first: 5) { nodes { remainingQuantity } }
            }
            remainingFulfillmentOrder {
              id
              lineItems(first: 5) { nodes { remainingQuantity } }
            }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "id": move_fulfillment_order_id,
            "newLocationId": destination_location_id,
            "fulfillmentOrderLineItems": [{ "id": move_line_id, "quantity": 1 }]
        }),
    ));
    let move_payload = &move_response.body["data"]["fulfillmentOrderMove"];
    assert_eq!(move_payload["userErrors"], json!([]));
    let moved_id = move_payload["movedFulfillmentOrder"]["id"].clone();

    let moved_nodes = proxy.process_request(json_graphql_request(
        r#"
        query MovedFulfillmentOrderNodes($movedId: ID!, $originalId: ID!) {
          moved: node(id: $movedId) {
            __typename
            ... on FulfillmentOrder {
              id
              assignedLocation { name location { id name } }
              lineItems(first: 5) { nodes { remainingQuantity } }
            }
          }
          original: node(id: $originalId) {
            ... on FulfillmentOrder {
              id
              lineItems(first: 5) { nodes { remainingQuantity } }
            }
          }
        }
        "#,
        json!({ "movedId": moved_id, "originalId": move_fulfillment_order_id }),
    ));
    assert_eq!(
        moved_nodes.body["data"]["moved"]["assignedLocation"]["location"]["id"],
        destination_location["id"]
    );
    assert_eq!(
        moved_nodes.body["data"]["moved"]["lineItems"],
        move_payload["movedFulfillmentOrder"]["lineItems"]
    );
    assert_eq!(
        moved_nodes.body["data"]["original"]["lineItems"],
        move_payload["originalFulfillmentOrder"]["lineItems"]
    );
}

fn run_generic_node_resolves_return_and_reverse_logistics_resources_from_staged_graph() {
    let mut proxy = snapshot_proxy();
    let (order_id, fulfillment_line_item_id) = stage_fulfilled_order_for_return(&mut proxy);

    let returnables = proxy.process_request(json_graphql_request(
        r#"
        query ReturnableFulfillmentNodeSeed($orderId: ID!) {
          returnableFulfillments(orderId: $orderId, first: 5) {
            nodes {
              id
              fulfillment { id }
              returnableFulfillmentLineItems(first: 5) {
                nodes { fulfillmentLineItem { id } quantity }
              }
            }
          }
        }
        "#,
        json!({ "orderId": order_id.clone() }),
    ));
    assert_eq!(returnables.status, 200);
    let returnable_id =
        returnables.body["data"]["returnableFulfillments"]["nodes"][0]["id"].clone();
    let returnable_node = proxy.process_request(json_graphql_request(
        r#"
        query ReturnableFulfillmentNode($id: ID!) {
          node(id: $id) {
            __typename
            ... on ReturnableFulfillment {
              id
              fulfillment { id }
              returnableFulfillmentLineItems(first: 5) {
                nodes { fulfillmentLineItem { id } quantity }
              }
            }
          }
        }
        "#,
        json!({ "id": returnable_id }),
    ));
    assert_eq!(
        returnable_node.body["data"]["node"]["__typename"],
        json!("ReturnableFulfillment")
    );
    assert_eq!(
        returnable_node.body["data"]["node"]["returnableFulfillmentLineItems"]["nodes"][0]
            ["quantity"],
        json!(2)
    );

    let create_return = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateReturnNodeGraph($returnInput: ReturnInput!) {
          returnCreate(returnInput: $returnInput) {
            return {
              id
              status
              totalQuantity
              returnLineItems(first: 5) {
                nodes {
                  id
                  quantity
                  processedQuantity
                  unprocessedQuantity
                  returnReason
                  ... on ReturnLineItem { fulfillmentLineItem { id lineItem { id title } } }
                }
              }
              reverseFulfillmentOrders(first: 5) {
                nodes {
                  id
                  status
                  lineItems(first: 5) {
                    nodes {
                      id
                      totalQuantity
                      fulfillmentLineItem { id lineItem { id title } }
                    }
                  }
                }
              }
            }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "returnInput": {
                "orderId": order_id.clone(),
                "returnLineItems": [{
                    "fulfillmentLineItemId": fulfillment_line_item_id,
                    "quantity": 2,
                    "returnReason": "UNWANTED"
                }]
            }
        }),
    ));
    assert_eq!(create_return.status, 200);
    assert_eq!(
        create_return.body["data"]["returnCreate"]["userErrors"],
        json!([])
    );
    let return_record = &create_return.body["data"]["returnCreate"]["return"];
    let return_id = return_record["id"].clone();
    let return_line_item_id = return_record["returnLineItems"]["nodes"][0]["id"].clone();
    let reverse_fulfillment_order_id =
        return_record["reverseFulfillmentOrders"]["nodes"][0]["id"].clone();
    let reverse_fulfillment_order_line_item_id = return_record["reverseFulfillmentOrders"]["nodes"]
        [0]["lineItems"]["nodes"][0]["id"]
        .clone();

    let consumed_returnable = proxy.process_request(json_graphql_request(
        r#"query ConsumedReturnableFulfillmentNode($id: ID!) { node(id: $id) { id } }"#,
        json!({ "id": returnable_id }),
    ));
    assert_eq!(consumed_returnable.body["data"]["node"], Value::Null);

    let return_nodes = proxy.process_request(json_graphql_request(
        r#"
        query ReturnReverseNodeRead($ids: [ID!]!) {
          nodes(ids: $ids) {
            __typename
            ... on Return {
              id
              status
              totalQuantity
              order { id }
              returnLineItems(first: 5) { nodes { id quantity processableQuantity returnReason } }
              reverseFulfillmentOrders(first: 5) { nodes { id status } }
            }
            ... on ReturnLineItem {
              id
              quantity
              processableQuantity
              returnReason
              fulfillmentLineItem { id quantity lineItem { id title } }
            }
            ... on ReverseFulfillmentOrder {
              id
              status
              order { id }
              lineItems(first: 5) {
                nodes { id totalQuantity fulfillmentLineItem { id } }
              }
            }
            ... on ReverseFulfillmentOrderLineItem {
              id
              totalQuantity
              fulfillmentLineItem { id }
            }
            ... on UnverifiedReturnLineItem { id }
          }
        }
        "#,
        json!({
            "ids": [
                return_id,
                return_line_item_id,
                reverse_fulfillment_order_id,
                reverse_fulfillment_order_line_item_id,
                return_id,
                "gid://shopify/UnverifiedReturnLineItem/999999999"
            ]
        }),
    ));
    assert_eq!(
        return_nodes.body["data"]["nodes"][0]["order"]["id"],
        order_id
    );
    assert_eq!(
        return_nodes.body["data"]["nodes"][1]["fulfillmentLineItem"]["lineItem"]["title"],
        json!("Return removal status line")
    );
    assert_eq!(
        return_nodes.body["data"]["nodes"][1]["processableQuantity"],
        json!(2)
    );
    assert_eq!(
        return_nodes.body["data"]["nodes"][1]["fulfillmentLineItem"]["quantity"],
        json!(2)
    );
    assert_eq!(
        return_nodes.body["data"]["nodes"][0]["returnLineItems"]["nodes"][0]["processableQuantity"],
        json!(2)
    );
    assert_eq!(
        return_nodes.body["data"]["nodes"][2]["order"]["id"],
        order_id
    );
    assert_eq!(
        return_nodes.body["data"]["nodes"][2]["lineItems"]["nodes"][0]["id"],
        reverse_fulfillment_order_line_item_id
    );
    assert_eq!(
        return_nodes.body["data"]["nodes"][0],
        return_nodes.body["data"]["nodes"][4]
    );
    assert_eq!(return_nodes.body["data"]["nodes"][5], Value::Null);

    let delivery = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateReverseDeliveryNode(
          $reverseFulfillmentOrderId: ID!
          $reverseFulfillmentOrderLineItemId: ID!
        ) {
          reverseDeliveryCreateWithShipping(
            reverseFulfillmentOrderId: $reverseFulfillmentOrderId
            reverseDeliveryLineItems: [{
              reverseFulfillmentOrderLineItemId: $reverseFulfillmentOrderLineItemId
              quantity: 2
            }]
            trackingInput: { number: "RD-1", url: "https://example.test/rd-1" }
            labelInput: { fileUrl: "https://example.test/label.pdf" }
          ) {
            reverseDelivery {
              id
              reverseFulfillmentOrder { id status }
              reverseDeliveryLineItems(first: 5) {
                nodes { id quantity reverseFulfillmentOrderLineItem { id } }
              }
            }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "reverseFulfillmentOrderId": reverse_fulfillment_order_id,
            "reverseFulfillmentOrderLineItemId": reverse_fulfillment_order_line_item_id
        }),
    ));
    assert_eq!(
        delivery.body["data"]["reverseDeliveryCreateWithShipping"]["userErrors"],
        json!([])
    );
    assert_eq!(
        delivery.body["data"]["reverseDeliveryCreateWithShipping"]["reverseDelivery"]
            ["reverseFulfillmentOrder"]["status"],
        json!("OPEN")
    );
    let reverse_delivery_id =
        delivery.body["data"]["reverseDeliveryCreateWithShipping"]["reverseDelivery"]["id"].clone();
    let reverse_delivery_line_item_id = delivery.body["data"]["reverseDeliveryCreateWithShipping"]
        ["reverseDelivery"]["reverseDeliveryLineItems"]["nodes"][0]["id"]
        .clone();

    let delivery_nodes = proxy.process_request(json_graphql_request(
        r#"
        query ReverseDeliveryNodeRead($ids: [ID!]!) {
          nodes(ids: $ids) {
            __typename
            ... on ReverseDelivery {
              id
              reverseFulfillmentOrder { id status }
              deliverable {
                __typename
                ... on ReverseDeliveryShippingDeliverable {
                  tracking { number url carrierName }
                  label { publicFileUrl }
                }
              }
              reverseDeliveryLineItems(first: 5) {
                nodes { id quantity reverseFulfillmentOrderLineItem { id } }
              }
            }
            ... on ReverseDeliveryLineItem {
              id
              quantity
              reverseFulfillmentOrderLineItem { id totalQuantity }
            }
          }
        }
        "#,
        json!({ "ids": [reverse_delivery_id, reverse_delivery_line_item_id, reverse_delivery_id] }),
    ));
    assert_eq!(
        delivery_nodes.body["data"]["nodes"][0]["deliverable"]["tracking"]["number"],
        json!("RD-1")
    );
    assert_eq!(
        delivery_nodes.body["data"]["nodes"][1]["reverseFulfillmentOrderLineItem"]["id"],
        reverse_fulfillment_order_line_item_id
    );
    assert_eq!(
        delivery_nodes.body["data"]["nodes"][0],
        delivery_nodes.body["data"]["nodes"][2]
    );

    let update_delivery = proxy.process_request(json_graphql_request(
        r#"
        mutation UpdateReverseDeliveryNode($id: ID!) {
          reverseDeliveryShippingUpdate(
            reverseDeliveryId: $id
            trackingInput: { number: "RD-2", url: "https://example.test/rd-2" }
          ) {
            reverseDelivery { id deliverable { ... on ReverseDeliveryShippingDeliverable { tracking { number carrierName } } } }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "id": reverse_delivery_id }),
    ));
    assert_eq!(
        update_delivery.body["data"]["reverseDeliveryShippingUpdate"]["userErrors"],
        json!([])
    );
    let updated_delivery_node = proxy.process_request(json_graphql_request(
        r#"
        query UpdatedReverseDeliveryNode($id: ID!) {
          node(id: $id) {
            ... on ReverseDelivery {
              deliverable { ... on ReverseDeliveryShippingDeliverable { tracking { number carrierName } } }
            }
          }
        }
        "#,
        json!({ "id": reverse_delivery_id }),
    ));
    assert_eq!(
        updated_delivery_node.body["data"]["node"]["deliverable"]["tracking"],
        json!({ "number": "RD-2", "carrierName": Value::Null })
    );

    let dispose = proxy.process_request(json_graphql_request(
        r#"
        mutation DisposeReverseFulfillmentNodeLine($dispositionInputs: [ReverseFulfillmentOrderDisposeInput!]!) {
          reverseFulfillmentOrderDispose(dispositionInputs: $dispositionInputs) {
            reverseFulfillmentOrderLineItems {
              id
              dispositions { type quantity location { id } }
            }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "dispositionInputs": [{
                "reverseFulfillmentOrderLineItemId": reverse_fulfillment_order_line_item_id,
                "quantity": 2,
                "dispositionType": "NOT_RESTOCKED",
                "locationId": "gid://shopify/Location/123"
            }]
        }),
    ));
    assert_eq!(
        dispose.body["data"]["reverseFulfillmentOrderDispose"]["userErrors"],
        json!([])
    );
    let disposed_line_node = proxy.process_request(json_graphql_request(
        r#"
        query DisposedReverseFulfillmentNodeLine($id: ID!) {
          node(id: $id) {
            ... on ReverseFulfillmentOrderLineItem {
              id
              dispositions { type quantity location { id } }
            }
          }
        }
        "#,
        json!({ "id": reverse_fulfillment_order_line_item_id }),
    ));
    assert_eq!(
        disposed_line_node.body["data"]["node"]["dispositions"][0]["location"]["id"],
        json!("gid://shopify/Location/123")
    );

    let close = return_lifecycle_transition_for_test(&mut proxy, "returnClose", return_id.clone());
    assert_eq!(close["userErrors"], json!([]));
    let closed_return_node = proxy.process_request(json_graphql_request(
        r#"query ClosedReturnNode($id: ID!) { node(id: $id) { ... on Return { id status closedAt } } }"#,
        json!({ "id": return_id }),
    ));
    assert_eq!(
        closed_return_node.body["data"]["node"]["status"],
        json!("CLOSED")
    );
    assert_ne!(
        closed_return_node.body["data"]["node"]["closedAt"],
        Value::Null
    );

    let reopen =
        return_lifecycle_transition_for_test(&mut proxy, "returnReopen", return_id.clone());
    assert_eq!(reopen["userErrors"], json!([]));

    let dump = proxy.process_request(request_with_body("POST", "/__meta/dump", ""));
    assert_eq!(dump.status, 200);
    let restore = proxy.process_request(request_with_body(
        "POST",
        "/__meta/restore",
        &dump.body.to_string(),
    ));
    assert_eq!(restore.status, 200);
    let restored = proxy.process_request(json_graphql_request(
        r#"
        query RestoredReturnReverseNodes($ids: [ID!]!) {
          nodes(ids: $ids) {
            ... on Return { id status }
            ... on ReverseDelivery { id deliverable { ... on ReverseDeliveryShippingDeliverable { tracking { number } } } }
            ... on ReverseFulfillmentOrderLineItem { id dispositions { quantity } }
          }
        }
        "#,
        json!({ "ids": [return_id, reverse_delivery_id, reverse_fulfillment_order_line_item_id] }),
    ));
    assert_eq!(restored.body["data"]["nodes"][0]["status"], json!("OPEN"));
    assert_eq!(
        restored.body["data"]["nodes"][1]["deliverable"]["tracking"]["number"],
        json!("RD-2")
    );
    assert_eq!(
        restored.body["data"]["nodes"][2]["dispositions"][0]["quantity"],
        json!(2)
    );

    let delete = proxy.process_request(json_graphql_request(
        r#"
        mutation DeleteReturnNodeOrder($orderId: ID!) {
          orderDelete(orderId: $orderId) { deletedId userErrors { field message code } }
        }
        "#,
        json!({ "orderId": order_id }),
    ));
    assert_eq!(delete.body["data"]["orderDelete"]["userErrors"], json!([]));
    let deleted = proxy.process_request(json_graphql_request(
        r#"
        query DeletedReturnReverseNodes($ids: [ID!]!) {
          nodes(ids: $ids) {
            __typename
            ... on Return { id }
            ... on ReturnLineItem { id }
            ... on ReverseFulfillmentOrder { id }
            ... on ReverseFulfillmentOrderLineItem { id }
            ... on ReverseDelivery { id }
            ... on ReverseDeliveryLineItem { id }
          }
        }
        "#,
        json!({
            "ids": [
                return_id,
                return_line_item_id,
                reverse_fulfillment_order_id,
                reverse_fulfillment_order_line_item_id,
                reverse_delivery_id,
                reverse_delivery_line_item_id
            ]
        }),
    ));
    assert_eq!(
        deleted.body["data"]["nodes"],
        json!([
            Value::Null,
            Value::Null,
            Value::Null,
            Value::Null,
            Value::Null,
            Value::Null
        ])
    );
}

#[test]
fn generic_node_resolves_fulfillment_resources_from_staged_order_graph() {
    run_generic_node_resolves_fulfillment_resources_from_staged_order_graph();
}

#[test]
fn generic_node_reflects_fulfillment_order_move_split_and_merge() {
    run_generic_node_reflects_fulfillment_order_move_split_and_merge();
}

#[test]
fn generic_node_resolves_return_and_reverse_logistics_resources_from_staged_graph() {
    run_generic_node_resolves_return_and_reverse_logistics_resources_from_staged_graph();
}

#[test]
fn return_process_payload_and_reads_keep_processed_return_open() {
    let mut proxy = snapshot_proxy();
    let setup = stage_open_return_for_removal(&mut proxy);

    let processed = return_process_for_test(
        &mut proxy,
        setup.return_id.clone(),
        setup.return_line_item_id,
    );

    assert_eq!(processed["userErrors"], json!([]));
    assert_eq!(processed["return"]["status"], json!("OPEN"));

    let read_after = read_return_removal_state(&mut proxy, setup.return_id, setup.order_id);
    assert_eq!(read_after["return"]["status"], json!("OPEN"));
    assert_eq!(
        read_after["order"]["returns"]["nodes"][0]["status"],
        json!("OPEN")
    );
}

#[test]
fn return_close_closed_at_uses_request_clock_and_process_keeps_closed_at_null() {
    let clock = Arc::new(Mutex::new(utc_time(1_782_993_600)));
    let mut proxy = snapshot_proxy_with_clock(Arc::clone(&clock));
    let close_setup = stage_open_return_for_removal(&mut proxy);

    set_clock(&clock, 1_783_080_000);
    let close = proxy.process_request(json_graphql_request(
        r#"
        mutation ReturnCloseClockedTimestamp($id: ID!) {
          returnClose(id: $id) {
            return { id status closedAt order { id updatedAt } }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "id": close_setup.return_id.clone() }),
    ));
    assert_eq!(close.status, 200);
    let close_payload = &close.body["data"]["returnClose"];
    assert_eq!(close_payload["userErrors"], json!([]));
    assert_eq!(close_payload["return"]["status"], json!("CLOSED"));
    assert_eq!(
        close_payload["return"]["closedAt"],
        json!("2026-07-03T12:00:00Z")
    );
    assert_ne!(
        close_payload["return"]["order"]["updatedAt"],
        json!("2024-01-01T00:00:03.000Z")
    );

    let close_read = read_return_timestamp_state(
        &mut proxy,
        close_setup.return_id.clone(),
        close_setup.order_id.clone(),
    );
    assert_eq!(
        close_read["return"]["closedAt"],
        json!("2026-07-03T12:00:00Z")
    );
    assert_eq!(
        close_read["order"]["returns"]["nodes"][0]["closedAt"],
        json!("2026-07-03T12:00:00Z")
    );
    assert_ne!(
        close_read["order"]["updatedAt"],
        json!("2024-01-01T00:00:03.000Z")
    );

    let process_setup = stage_open_return_for_removal(&mut proxy);
    set_clock(&clock, 1_783_166_400);
    let processed = return_process_for_test(
        &mut proxy,
        process_setup.return_id.clone(),
        process_setup.return_line_item_id,
    );
    assert_eq!(processed["userErrors"], json!([]));

    let process_read =
        read_return_timestamp_state(&mut proxy, process_setup.return_id, process_setup.order_id);
    assert_eq!(process_read["return"]["status"], json!("OPEN"));
    assert_eq!(process_read["return"]["closedAt"], Value::Null);
    assert_eq!(
        process_read["order"]["returns"]["nodes"][0]["status"],
        json!("OPEN")
    );
    assert_eq!(
        process_read["order"]["returns"]["nodes"][0]["closedAt"],
        Value::Null
    );
}

fn stage_requested_return_for_removal(proxy: &mut DraftProxy) -> ReturnRemovalSetup {
    let (order_id, fulfillment_line_item_id) = stage_fulfilled_order_for_return(proxy);
    let response = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateRequestedReturnForRemoval($input: ReturnRequestInput!) {
          returnRequest(input: $input) {
            return {
              id
              status
              totalQuantity
              returnLineItems(first: 5) {
                nodes { id quantity processedQuantity unprocessedQuantity }
              }
              reverseFulfillmentOrders(first: 5) {
                nodes {
                  id
                  lineItems(first: 5) {
                    nodes { id totalQuantity fulfillmentLineItem { id } }
                  }
                }
              }
            }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "input": {
                "orderId": order_id,
                "returnLineItems": [{
                    "fulfillmentLineItemId": fulfillment_line_item_id,
                    "quantity": 2,
                    "returnReason": "OTHER"
                }]
            }
        }),
    ));
    assert_eq!(response.status, 200);
    return_removal_setup_from_payload(order_id, &response.body["data"]["returnRequest"])
}

#[test]
fn order_returns_window_and_query_from_staged_returns() {
    let mut proxy = snapshot_proxy();
    let (order_id, fulfillment_line_item_id) = stage_fulfilled_order_for_return(&mut proxy);

    let open = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateOpenReturnForConnection($returnInput: ReturnInput!) {
          returnCreate(returnInput: $returnInput) {
            return { id name status totalQuantity }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "returnInput": {
                "orderId": order_id,
                "returnLineItems": [{
                    "fulfillmentLineItemId": fulfillment_line_item_id,
                    "quantity": 1,
                    "returnReason": "UNWANTED"
                }]
            }
        }),
    ));
    assert_eq!(open.status, 200);
    assert_eq!(open.body["data"]["returnCreate"]["userErrors"], json!([]));
    let open_return_id = open.body["data"]["returnCreate"]["return"]["id"].clone();

    let requested = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateRequestedReturnForConnection($input: ReturnRequestInput!) {
          returnRequest(input: $input) {
            return { id name status totalQuantity }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "input": {
                "orderId": order_id,
                "returnLineItems": [{
                    "fulfillmentLineItemId": fulfillment_line_item_id,
                    "quantity": 1,
                    "returnReason": "OTHER"
                }]
            }
        }),
    ));
    assert_eq!(requested.status, 200);
    assert_eq!(
        requested.body["data"]["returnRequest"]["userErrors"],
        json!([])
    );
    let requested_return_id = requested.body["data"]["returnRequest"]["return"]["id"].clone();

    let first_page = proxy.process_request(json_graphql_request(
        r#"
        query ReturnConnectionFirstPage($orderId: ID!) {
          order(id: $orderId) {
            id
            returns(first: 1) {
              nodes { id status }
              pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
            }
          }
        }
        "#,
        json!({ "orderId": order_id }),
    ));
    assert_eq!(first_page.status, 200);
    assert_eq!(
        first_page.body["data"]["order"]["returns"]["nodes"],
        json!([{ "id": open_return_id, "status": "OPEN" }])
    );
    assert_eq!(
        first_page.body["data"]["order"]["returns"]["pageInfo"],
        json!({
            "hasNextPage": true,
            "hasPreviousPage": false,
            "startCursor": open_return_id,
            "endCursor": open_return_id
        })
    );

    let second_page = proxy.process_request(json_graphql_request(
        r#"
        query ReturnConnectionSecondPage($orderId: ID!, $after: String!) {
          order(id: $orderId) {
            returns(first: 1, after: $after) {
              nodes { id status }
              pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
            }
          }
        }
        "#,
        json!({
            "orderId": first_page.body["data"]["order"]["id"].clone(),
            "after": first_page.body["data"]["order"]["returns"]["pageInfo"]["endCursor"].clone()
        }),
    ));
    assert_eq!(second_page.status, 200);
    assert_eq!(
        second_page.body["data"]["order"]["returns"]["nodes"],
        json!([{ "id": requested_return_id, "status": "REQUESTED" }])
    );
    assert_eq!(
        second_page.body["data"]["order"]["returns"]["pageInfo"],
        json!({
            "hasNextPage": false,
            "hasPreviousPage": true,
            "startCursor": requested_return_id,
            "endCursor": requested_return_id
        })
    );

    let filtered = proxy.process_request(json_graphql_request(
        r#"
        query ReturnConnectionFiltered($orderId: ID!) {
          order(id: $orderId) {
            returns(first: 5, query: "status:REQUESTED") {
              nodes { id status }
              pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
            }
          }
        }
        "#,
        json!({ "orderId": order_id }),
    ));
    assert_eq!(filtered.status, 200);
    assert_eq!(
        filtered.body["data"]["order"]["returns"]["nodes"],
        json!([{ "id": requested_return_id, "status": "REQUESTED" }])
    );
    assert_eq!(
        filtered.body["data"]["order"]["returns"]["pageInfo"],
        json!({
            "hasNextPage": false,
            "hasPreviousPage": false,
            "startCursor": requested_return_id,
            "endCursor": requested_return_id
        })
    );
}

fn return_statuses(connection: &Value) -> Vec<String> {
    connection["nodes"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|node| node["status"].as_str().map(str::to_string))
        .collect()
}

fn read_order_return_status_views(proxy: &mut DraftProxy, order_id: Value) -> Value {
    let detail = proxy.process_request(json_graphql_request(
        r#"
        query ReadOrderReturnStatusDetail($id: ID!) {
          order(id: $id) {
            id
            returnStatus
            returns(first: 5) { nodes { id status totalQuantity } }
          }
        }
        "#,
        json!({ "id": order_id.clone() }),
    ));
    assert_eq!(detail.status, 200);

    let list = proxy.process_request(json_graphql_request(
        r#"
        query ReadOrderReturnStatusList {
          orders(first: 5) {
            nodes {
              id
              returnStatus
              returns(first: 5) { nodes { id status totalQuantity } }
            }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(list.status, 200);

    let node = proxy.process_request(json_graphql_request(
        r#"
        query ReadOrderReturnStatusNode($id: ID!) {
          node(id: $id) {
            __typename
            ... on Order {
              id
              returnStatus
              returns(first: 5) { nodes { id status totalQuantity } }
            }
          }
        }
        "#,
        json!({ "id": order_id }),
    ));
    assert_eq!(node.status, 200);

    json!({
        "detail": detail.body["data"]["order"].clone(),
        "list": list.body["data"]["orders"]["nodes"][0].clone(),
        "node": node.body["data"]["node"].clone()
    })
}

fn assert_order_return_status_views(
    proxy: &mut DraftProxy,
    order_id: Value,
    expected_status: &str,
    expected_return_statuses: &[&str],
) {
    let views = read_order_return_status_views(proxy, order_id);
    let expected_return_statuses = expected_return_statuses
        .iter()
        .map(|status| status.to_string())
        .collect::<Vec<_>>();
    for key in ["detail", "list", "node"] {
        assert_eq!(views[key]["returnStatus"], json!(expected_status), "{key}");
        assert_eq!(
            return_statuses(&views[key]["returns"]),
            expected_return_statuses,
            "{key}"
        );
    }
    assert_eq!(views["node"]["__typename"], json!("Order"));
}

#[test]
fn order_return_status_tracks_staged_return_lifecycle_across_order_projections() {
    let mut proxy = snapshot_proxy();
    let (order_id, fulfillment_line_item_id) = stage_fulfilled_order_for_return(&mut proxy);

    let request = proxy.process_request(json_graphql_request(
        r#"
        mutation RequestReturnForOrderStatus($input: ReturnRequestInput!) {
          returnRequest(input: $input) {
            return {
              id
              status
              order {
                id
                returnStatus
                returns(first: 5) { nodes { id status } }
              }
              returnLineItems(first: 5) {
                nodes { id quantity processedQuantity unprocessedQuantity }
              }
            }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "input": {
                "orderId": order_id,
                "returnLineItems": [{
                    "fulfillmentLineItemId": fulfillment_line_item_id,
                    "quantity": 1,
                    "returnReason": "OTHER"
                }]
            }
        }),
    ));
    assert_eq!(request.status, 200);
    assert_eq!(
        request.body["data"]["returnRequest"]["userErrors"],
        json!([])
    );
    let requested_return = &request.body["data"]["returnRequest"]["return"];
    let return_id = requested_return["id"].clone();
    assert_eq!(
        requested_return["order"]["returnStatus"],
        json!("RETURN_REQUESTED")
    );
    assert_order_return_status_views(
        &mut proxy,
        requested_return["order"]["id"].clone(),
        "RETURN_REQUESTED",
        &["REQUESTED"],
    );

    let approve = proxy.process_request(json_graphql_request(
        r#"
        mutation ApproveReturnForOrderStatus($input: ReturnApproveRequestInput!) {
          returnApproveRequest(input: $input) {
            return {
              id
              status
              order {
                id
                returnStatus
                returns(first: 5) { nodes { id status } }
              }
              returnLineItems(first: 5) {
                nodes { id quantity processedQuantity unprocessedQuantity }
              }
            }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "input": { "id": return_id.clone() } }),
    ));
    assert_eq!(approve.status, 200);
    assert_eq!(
        approve.body["data"]["returnApproveRequest"]["userErrors"],
        json!([])
    );
    assert_eq!(
        approve.body["data"]["returnApproveRequest"]["return"]["order"]["returnStatus"],
        json!("IN_PROGRESS")
    );
    assert_order_return_status_views(&mut proxy, order_id.clone(), "IN_PROGRESS", &["OPEN"]);

    let close = return_lifecycle_transition_for_test(&mut proxy, "returnClose", return_id.clone());
    assert_eq!(close["userErrors"], json!([]));
    assert_order_return_status_views(&mut proxy, order_id.clone(), "RETURNED", &["CLOSED"]);

    let reopen =
        return_lifecycle_transition_for_test(&mut proxy, "returnReopen", return_id.clone());
    assert_eq!(reopen["userErrors"], json!([]));
    assert_order_return_status_views(&mut proxy, order_id.clone(), "IN_PROGRESS", &["OPEN"]);

    let return_line_item_id = request.body["data"]["returnRequest"]["return"]["returnLineItems"]
        ["nodes"][0]["id"]
        .clone();
    let processed = return_process_for_test(&mut proxy, return_id, return_line_item_id);
    assert_eq!(processed["userErrors"], json!([]));
    assert_order_return_status_views(&mut proxy, order_id, "IN_PROGRESS", &["OPEN"]);
}

#[test]
fn order_return_status_handles_declined_canceled_and_removed_only_returns() {
    let mut declined_proxy = snapshot_proxy();
    let declined = stage_requested_return_for_removal(&mut declined_proxy);
    let declined_payload =
        decline_return_request_for_test(&mut declined_proxy, declined.return_id.clone());
    assert_eq!(declined_payload["userErrors"], json!([]));
    assert_order_return_status_views(
        &mut declined_proxy,
        declined.order_id,
        "NO_RETURN",
        &["DECLINED"],
    );

    let mut canceled_proxy = snapshot_proxy();
    let canceled = stage_requested_return_for_removal(&mut canceled_proxy);
    let approved = approve_return_request_for_test(&mut canceled_proxy, canceled.return_id.clone());
    assert_eq!(approved["userErrors"], json!([]));
    let canceled_payload = return_lifecycle_transition_for_test(
        &mut canceled_proxy,
        "returnCancel",
        canceled.return_id,
    );
    assert_eq!(canceled_payload["userErrors"], json!([]));
    assert_order_return_status_views(
        &mut canceled_proxy,
        canceled.order_id,
        "NO_RETURN",
        &["CANCELED"],
    );

    let mut removed_proxy = snapshot_proxy();
    let (order_id, fulfillment_line_item_id) = stage_fulfilled_order_for_return(&mut removed_proxy);
    let create = removed_proxy.process_request(json_graphql_request(
        r#"
        mutation CreateOpenReturnForRemovedStatus($returnInput: ReturnInput!) {
          returnCreate(returnInput: $returnInput) {
            return {
              id
              status
              totalQuantity
              returnLineItems(first: 5) {
                nodes { id quantity processedQuantity unprocessedQuantity }
              }
            }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "returnInput": {
                "orderId": order_id,
                "returnLineItems": [{
                    "fulfillmentLineItemId": fulfillment_line_item_id,
                    "quantity": 1,
                    "returnReason": "OTHER",
                    "returnReasonNote": "removed"
                }]
            }
        }),
    ));
    assert_eq!(create.status, 200);
    assert_eq!(create.body["data"]["returnCreate"]["userErrors"], json!([]));
    let open_return = &create.body["data"]["returnCreate"]["return"];
    let removed = remove_from_return_for_test(
        &mut removed_proxy,
        open_return["id"].clone(),
        open_return["returnLineItems"]["nodes"][0]["id"].clone(),
    );
    assert_eq!(removed["userErrors"], json!([]));
    assert_eq!(removed["return"]["status"], json!("CLOSED"));
    assert_eq!(removed["return"]["totalQuantity"], json!(0));
    assert_eq!(removed["return"]["returnLineItems"]["nodes"], json!([]));
    assert_order_return_status_views(&mut removed_proxy, order_id, "RETURNED", &["CLOSED"]);
}

fn remove_from_return_for_test(
    proxy: &mut DraftProxy,
    return_id: Value,
    return_line_item_id: Value,
) -> Value {
    proxy
        .process_request(json_graphql_request(
            r#"
            mutation RemoveFromReturnForStatus($returnId: ID!, $returnLineItems: [ReturnLineItemRemoveFromReturnInput!]) {
              removeFromReturn(returnId: $returnId, returnLineItems: $returnLineItems) {
                return {
                  id
                  status
                  totalQuantity
                  returnLineItems(first: 5) {
                    nodes { id quantity processedQuantity unprocessedQuantity }
                  }
                  reverseFulfillmentOrders(first: 5) {
                    nodes {
                      id
                      lineItems(first: 5) {
                        nodes { id totalQuantity fulfillmentLineItem { id } }
                      }
                    }
                  }
                }
                userErrors { field message code }
              }
            }
            "#,
            json!({
                "returnId": return_id,
                "returnLineItems": [{ "returnLineItemId": return_line_item_id, "quantity": 1 }]
            }),
        ))
        .body["data"]["removeFromReturn"]
        .clone()
}

fn return_lifecycle_transition_for_test(
    proxy: &mut DraftProxy,
    root: &str,
    return_id: Value,
) -> Value {
    let (query, response_key) = match root {
        "returnCancel" => (
            r#"
            mutation ReturnCancelMissingReturnForErrorShape($id: ID!) {
              returnCancel(id: $id) {
                return { id status }
                userErrors { field message code }
              }
            }
            "#,
            "returnCancel",
        ),
        "returnClose" => (
            r#"
            mutation ReturnCloseMissingReturnForErrorShape($id: ID!) {
              returnClose(id: $id) {
                return { id status }
                userErrors { field message code }
              }
            }
            "#,
            "returnClose",
        ),
        "returnReopen" => (
            r#"
            mutation ReturnReopenMissingReturnForErrorShape($id: ID!) {
              returnReopen(id: $id) {
                return { id status }
                userErrors { field message code }
              }
            }
            "#,
            "returnReopen",
        ),
        _ => panic!("unsupported return lifecycle root {root}"),
    };
    proxy
        .process_request(json_graphql_request(query, json!({ "id": return_id })))
        .body["data"][response_key]
        .clone()
}

fn return_process_for_test(
    proxy: &mut DraftProxy,
    return_id: Value,
    return_line_item_id: Value,
) -> Value {
    proxy
        .process_request(json_graphql_request(
            r#"
            mutation ReturnProcessMissingReturnForErrorShape($input: ReturnProcessInput!) {
              returnProcess(input: $input) {
                return { id status }
                userErrors { field message code }
              }
            }
            "#,
            json!({
                "input": {
                    "returnId": return_id,
                    "returnLineItems": [{ "id": return_line_item_id, "quantity": 1 }],
                    "notifyCustomer": false
                }
            }),
        ))
        .body["data"]["returnProcess"]
        .clone()
}

fn approve_return_request_for_test(proxy: &mut DraftProxy, return_id: Value) -> Value {
    proxy
        .process_request(json_graphql_request(
            r#"
            mutation ApproveReturnRequestForErrorShape($input: ReturnApproveRequestInput!) {
              returnApproveRequest(input: $input) {
                return { id status }
                userErrors { field message }
              }
            }
            "#,
            json!({ "input": { "id": return_id } }),
        ))
        .body["data"]["returnApproveRequest"]
        .clone()
}

fn decline_return_request_for_test(proxy: &mut DraftProxy, return_id: Value) -> Value {
    proxy
        .process_request(json_graphql_request(
            r#"
            mutation DeclineReturnRequestForErrorShape($input: ReturnDeclineRequestInput!) {
              returnDeclineRequest(input: $input) {
                return { id status decline { reason note } }
                userErrors { field message }
              }
            }
            "#,
            json!({ "input": { "id": return_id, "declineReason": "OTHER" } }),
        ))
        .body["data"]["returnDeclineRequest"]
        .clone()
}

fn read_return_removal_state(proxy: &mut DraftProxy, return_id: Value, order_id: Value) -> Value {
    proxy
        .process_request(json_graphql_request(
            r#"
            query ReadReturnRemovalState($returnId: ID!, $orderId: ID!) {
              return(id: $returnId) {
                id
                status
                totalQuantity
                returnLineItems(first: 5) {
                  nodes { id quantity processedQuantity unprocessedQuantity }
                }
                reverseFulfillmentOrders(first: 5) {
                  nodes {
                    id
                    lineItems(first: 5) {
                      nodes { id totalQuantity fulfillmentLineItem { id } }
                    }
                  }
                }
              }
              order(id: $orderId) {
                id
                returns(first: 5) {
                  nodes {
                    id
                    status
                    totalQuantity
                    returnLineItems(first: 5) {
                      nodes { id quantity processedQuantity unprocessedQuantity }
                    }
                    reverseFulfillmentOrders(first: 5) {
                      nodes {
                        id
                        lineItems(first: 5) {
                          nodes { id totalQuantity fulfillmentLineItem { id } }
                        }
                      }
                    }
                  }
                }
              }
            }
            "#,
            json!({ "returnId": return_id, "orderId": order_id }),
        ))
        .body["data"]
        .clone()
}

fn read_return_timestamp_state(proxy: &mut DraftProxy, return_id: Value, order_id: Value) -> Value {
    proxy
        .process_request(json_graphql_request(
            r#"
            query ReadReturnTimestampState($returnId: ID!, $orderId: ID!) {
              return(id: $returnId) {
                id
                status
                closedAt
              }
              order(id: $orderId) {
                id
                updatedAt
                returns(first: 5) {
                  nodes { id status closedAt order { id updatedAt } }
                }
              }
            }
            "#,
            json!({ "returnId": return_id, "orderId": order_id }),
        ))
        .body["data"]
        .clone()
}

fn return_reason_validation_fixture() -> Value {
    serde_json::from_str(include_str!(
        "../../fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/orders/return-reason-validation.json"
    ))
    .unwrap()
}

fn return_reason_hydrated_proxy(fixture: &Value) -> DraftProxy {
    let hydrate_body = fixture["upstreamCalls"][0]["response"]["body"].clone();
    configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport(move |_request| Response {
        status: 200,
        headers: Default::default(),
        body: hydrate_body.clone(),
    })
}

fn assert_no_return_validation_side_effects(proxy: &DraftProxy) {
    let state = state_snapshot(proxy);
    assert_eq!(state["stagedState"]["orders"], json!({}));
    assert_eq!(state["stagedState"]["returns"], json!({}));
    assert_eq!(state["stagedState"]["returnsByOrder"], json!({}));
    assert_eq!(log_snapshot(proxy)["entries"], json!([]));
}

fn live_return_reason_validation_proxy(upstream_calls: Arc<AtomicUsize>) -> DraftProxy {
    configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport(move |_request| {
        upstream_calls.fetch_add(1, Ordering::SeqCst);
        Response {
            status: 200,
            headers: Default::default(),
            body: json!({ "data": { "order": Value::Null } }),
        }
    })
}

fn stage_two_line_reverse_fulfillment_order(proxy: &mut DraftProxy) -> (Value, Vec<Value>) {
    let create_order = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateReturnableOrder($order: OrderCreateOrderInput!) {
          orderCreate(order: $order) {
            order {
              id
              fulfillmentOrders(first: 5) {
                nodes {
                  id
                  lineItems(first: 5) {
                    nodes {
                      id
                      totalQuantity
                    }
                  }
                }
              }
            }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "order": {
                "email": "reverse-delivery-lines@example.test",
                "lineItems": [
                    {
                        "title": "First returnable line",
                        "quantity": 2,
                        "priceSet": { "shopMoney": { "amount": "12.00", "currencyCode": "USD" } }
                    },
                    {
                        "title": "Second returnable line",
                        "quantity": 3,
                        "priceSet": { "shopMoney": { "amount": "18.00", "currencyCode": "USD" } }
                    }
                ]
            }
        }),
    ));
    assert_eq!(create_order.status, 200);
    assert_eq!(
        create_order.body["data"]["orderCreate"]["userErrors"],
        json!([])
    );
    let order = &create_order.body["data"]["orderCreate"]["order"];
    let order_id = order["id"].clone();
    let fulfillment_order = &order["fulfillmentOrders"]["nodes"][0];
    let fulfillment_order_id = fulfillment_order["id"].clone();
    let fulfillment_order_lines = fulfillment_order["lineItems"]["nodes"]
        .as_array()
        .unwrap()
        .clone();

    let fulfill = proxy.process_request(json_graphql_request(
        r#"
        mutation FulfillReturnableOrder($fulfillment: FulfillmentInput!) {
          fulfillmentCreate(fulfillment: $fulfillment) {
            fulfillment {
              id
              fulfillmentLineItems(first: 5) {
                nodes {
                  id
                  quantity
                  lineItem {
                    id
                    title
                  }
                }
              }
            }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "fulfillment": {
                "lineItemsByFulfillmentOrder": [{
                    "fulfillmentOrderId": fulfillment_order_id,
                    "fulfillmentOrderLineItems": [
                        {
                            "id": fulfillment_order_lines[0]["id"],
                            "quantity": 2
                        },
                        {
                            "id": fulfillment_order_lines[1]["id"],
                            "quantity": 3
                        }
                    ]
                }]
            }
        }),
    ));
    assert_eq!(fulfill.status, 200);
    assert_eq!(
        fulfill.body["data"]["fulfillmentCreate"]["userErrors"],
        json!([])
    );
    let fulfillment_lines = fulfill.body["data"]["fulfillmentCreate"]["fulfillment"]
        ["fulfillmentLineItems"]["nodes"]
        .as_array()
        .unwrap()
        .clone();

    let create_return = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateOpenReturn($returnInput: ReturnInput!) {
          returnCreate(returnInput: $returnInput) {
            return {
              id
              status
              reverseFulfillmentOrders(first: 5) {
                nodes {
                  id
                  lineItems(first: 5) {
                    nodes {
                      id
                      totalQuantity
                    }
                  }
                }
              }
            }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "returnInput": {
                "orderId": order_id,
                "returnLineItems": [
                    {
                        "fulfillmentLineItemId": fulfillment_lines[0]["id"],
                        "quantity": 2,
                        "returnReason": "OTHER",
                        "returnReasonNote": "First line return"
                    },
                    {
                        "fulfillmentLineItemId": fulfillment_lines[1]["id"],
                        "quantity": 3,
                        "returnReason": "OTHER",
                        "returnReasonNote": "Second line return"
                    }
                ]
            }
        }),
    ));
    assert_eq!(create_return.status, 200);
    assert_eq!(
        create_return.body["data"]["returnCreate"]["userErrors"],
        json!([])
    );
    let rfo = &create_return.body["data"]["returnCreate"]["return"]["reverseFulfillmentOrders"]
        ["nodes"][0];
    (
        rfo["id"].clone(),
        rfo["lineItems"]["nodes"].as_array().unwrap().clone(),
    )
}

#[test]
fn reverse_delivery_create_uses_explicit_line_items_from_input() {
    let mut proxy = snapshot_proxy();
    let (reverse_fulfillment_order_id, rfo_lines) =
        stage_two_line_reverse_fulfillment_order(&mut proxy);

    let response = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateExplicitReverseDelivery(
          $reverseFulfillmentOrderId: ID!
          $reverseDeliveryLineItems: [ReverseDeliveryLineItemInput!]!
        ) {
          reverseDeliveryCreateWithShipping(
            reverseFulfillmentOrderId: $reverseFulfillmentOrderId
            reverseDeliveryLineItems: $reverseDeliveryLineItems
          ) {
            reverseDelivery {
              id
              reverseDeliveryLineItems(first: 5) {
                nodes {
                  id
                  quantity
                  reverseFulfillmentOrderLineItem {
                    id
                    totalQuantity
                  }
                }
              }
            }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "reverseFulfillmentOrderId": reverse_fulfillment_order_id,
            "reverseDeliveryLineItems": [
                {
                    "reverseFulfillmentOrderLineItemId": rfo_lines[1]["id"],
                    "quantity": 3
                },
                {
                    "reverseFulfillmentOrderLineItemId": rfo_lines[0]["id"],
                    "quantity": 2
                }
            ]
        }),
    ));

    assert_eq!(response.status, 200);
    assert_eq!(
        response.body["data"]["reverseDeliveryCreateWithShipping"]["userErrors"],
        json!([])
    );
    let delivery = &response.body["data"]["reverseDeliveryCreateWithShipping"]["reverseDelivery"];
    let delivery_id = delivery["id"].clone();
    let nodes = delivery["reverseDeliveryLineItems"]["nodes"]
        .as_array()
        .unwrap();
    assert_eq!(nodes.len(), 2);
    assert_eq!(nodes[0]["quantity"], json!(3));
    assert_eq!(
        nodes[0]["reverseFulfillmentOrderLineItem"]["id"],
        rfo_lines[1]["id"]
    );
    assert_eq!(
        nodes[0]["reverseFulfillmentOrderLineItem"]["totalQuantity"],
        rfo_lines[1]["totalQuantity"]
    );
    assert_eq!(nodes[1]["quantity"], json!(2));
    assert_eq!(
        nodes[1]["reverseFulfillmentOrderLineItem"]["id"],
        rfo_lines[0]["id"]
    );
    assert_eq!(
        nodes[1]["reverseFulfillmentOrderLineItem"]["totalQuantity"],
        rfo_lines[0]["totalQuantity"]
    );

    let downstream = proxy.process_request(json_graphql_request(
        r#"
        query ReadReverseDelivery($reverseDeliveryId: ID!, $reverseFulfillmentOrderId: ID!) {
          reverseDelivery(id: $reverseDeliveryId) {
            id
            reverseDeliveryLineItems(first: 5) {
              nodes {
                quantity
                reverseFulfillmentOrderLineItem { id }
              }
            }
          }
          reverseFulfillmentOrder(id: $reverseFulfillmentOrderId) {
            id
            reverseDeliveries(first: 5) { nodes { id } }
          }
        }
        "#,
        json!({
            "reverseDeliveryId": delivery_id,
            "reverseFulfillmentOrderId": reverse_fulfillment_order_id
        }),
    ));
    assert_eq!(
        downstream.body["data"]["reverseDelivery"]["reverseDeliveryLineItems"]["nodes"],
        json!([
            {
                "quantity": 3,
                "reverseFulfillmentOrderLineItem": { "id": rfo_lines[1]["id"] }
            },
            {
                "quantity": 2,
                "reverseFulfillmentOrderLineItem": { "id": rfo_lines[0]["id"] }
            }
        ])
    );
    assert_eq!(
        downstream.body["data"]["reverseFulfillmentOrder"]["reverseDeliveries"]["nodes"][0]["id"],
        delivery_id
    );
}

#[test]
fn reverse_delivery_create_empty_line_items_expand_to_all_rfo_lines() {
    let mut proxy = snapshot_proxy();
    let (reverse_fulfillment_order_id, rfo_lines) =
        stage_two_line_reverse_fulfillment_order(&mut proxy);

    let response = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateExpandedReverseDelivery(
          $reverseFulfillmentOrderId: ID!
          $reverseDeliveryLineItems: [ReverseDeliveryLineItemInput!]!
        ) {
          reverseDeliveryCreateWithShipping(
            reverseFulfillmentOrderId: $reverseFulfillmentOrderId
            reverseDeliveryLineItems: $reverseDeliveryLineItems
          ) {
            reverseDelivery {
              id
              reverseDeliveryLineItems(first: 5) {
                nodes {
                  quantity
                  reverseFulfillmentOrderLineItem {
                    id
                    totalQuantity
                  }
                }
              }
            }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "reverseFulfillmentOrderId": reverse_fulfillment_order_id,
            "reverseDeliveryLineItems": []
        }),
    ));

    assert_eq!(response.status, 200);
    assert_eq!(
        response.body["data"]["reverseDeliveryCreateWithShipping"]["userErrors"],
        json!([])
    );
    let nodes = response.body["data"]["reverseDeliveryCreateWithShipping"]["reverseDelivery"]
        ["reverseDeliveryLineItems"]["nodes"]
        .as_array()
        .unwrap();
    assert_eq!(nodes.len(), rfo_lines.len());
    for (node, rfo_line) in nodes.iter().zip(rfo_lines.iter()) {
        assert_eq!(node["quantity"], rfo_line["totalQuantity"]);
        assert_eq!(
            node["reverseFulfillmentOrderLineItem"]["id"],
            rfo_line["id"]
        );
        assert_eq!(
            node["reverseFulfillmentOrderLineItem"]["totalQuantity"],
            rfo_line["totalQuantity"]
        );
    }
}

#[test]
fn return_create_and_request_reject_missing_reason_without_staging() {
    let fixture = return_reason_validation_fixture();

    let mut create_proxy = snapshot_proxy();
    let create = create_proxy.process_request(json_graphql_request(
        include_str!("../../config/parity-requests/orders/return-create-reason-validation.graphql"),
        fixture["missingReasonCreate"]["variables"].clone(),
    ));
    assert_eq!(create.status, 200);
    assert_eq!(
        create.body["data"]["returnCreate"],
        fixture["missingReasonCreate"]["response"]["payload"]["data"]["returnCreate"]
    );
    assert_no_return_validation_side_effects(&create_proxy);

    let mut request_proxy = snapshot_proxy();
    let request = request_proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/orders/return-request-reason-validation.graphql"
        ),
        fixture["missingReasonRequest"]["variables"].clone(),
    ));
    assert_eq!(request.status, 200);
    assert_eq!(
        request.body["data"]["returnRequest"],
        fixture["missingReasonRequest"]["response"]["payload"]["data"]["returnRequest"]
    );
    assert_no_return_validation_side_effects(&request_proxy);
}

#[test]
fn return_reason_validation_failures_do_not_hydrate_live_orders() {
    let fixture = return_reason_validation_fixture();
    let upstream_calls = Arc::new(AtomicUsize::new(0));
    let mut proxy = live_return_reason_validation_proxy(Arc::clone(&upstream_calls));

    let create_missing = proxy.process_request(json_graphql_request(
        include_str!("../../config/parity-requests/orders/return-create-reason-validation.graphql"),
        fixture["missingReasonCreate"]["variables"].clone(),
    ));
    assert_eq!(
        create_missing.body["data"]["returnCreate"],
        fixture["missingReasonCreate"]["response"]["payload"]["data"]["returnCreate"]
    );

    let request_missing = proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/orders/return-request-reason-validation.graphql"
        ),
        fixture["missingReasonRequest"]["variables"].clone(),
    ));
    assert_eq!(
        request_missing.body["data"]["returnRequest"],
        fixture["missingReasonRequest"]["response"]["payload"]["data"]["returnRequest"]
    );

    let create_other_without_note = proxy.process_request(json_graphql_request(
        include_str!("../../config/parity-requests/orders/return-create-reason-validation.graphql"),
        fixture["otherBlankNoteCreate"]["variables"].clone(),
    ));
    assert_eq!(
        create_other_without_note.body["data"]["returnCreate"],
        fixture["otherBlankNoteCreate"]["response"]["payload"]["data"]["returnCreate"]
    );

    assert_eq!(upstream_calls.load(Ordering::SeqCst), 0);
    assert_no_return_validation_side_effects(&proxy);
}

#[test]
fn return_create_rejects_other_without_note_before_staging() {
    let fixture = return_reason_validation_fixture();
    let mut proxy = snapshot_proxy();

    let response = proxy.process_request(json_graphql_request(
        include_str!("../../config/parity-requests/orders/return-create-reason-validation.graphql"),
        fixture["otherBlankNoteCreate"]["variables"].clone(),
    ));

    assert_eq!(response.status, 200);
    assert_eq!(
        response.body["data"]["returnCreate"],
        fixture["otherBlankNoteCreate"]["response"]["payload"]["data"]["returnCreate"]
    );
    assert_no_return_validation_side_effects(&proxy);
}

#[test]
fn return_roots_reject_invalid_reason_enum_variables_before_staging() {
    let fixture = return_reason_validation_fixture();

    let mut create_proxy = snapshot_proxy();
    let create = create_proxy.process_request(json_graphql_request(
        include_str!("../../config/parity-requests/orders/return-create-reason-validation.graphql"),
        fixture["invalidReasonCreate"]["variables"].clone(),
    ));
    assert_eq!(create.status, 200);
    assert_eq!(
        create.body,
        fixture["invalidReasonCreate"]["response"]["payload"]
    );
    assert_no_return_validation_side_effects(&create_proxy);

    let mut request_proxy = snapshot_proxy();
    let request = request_proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/orders/return-request-reason-validation.graphql"
        ),
        fixture["invalidReasonRequest"]["variables"].clone(),
    ));
    assert_eq!(request.status, 200);
    assert_eq!(
        request.body,
        fixture["invalidReasonRequest"]["response"]["payload"]
    );
    assert_no_return_validation_side_effects(&request_proxy);
}

#[test]
fn return_request_accepts_public_other_reason_inputs_without_note() {
    let fixture = return_reason_validation_fixture();

    let mut explicit_other_proxy = return_reason_hydrated_proxy(&fixture);
    let explicit_other = explicit_other_proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/orders/return-request-reason-validation.graphql"
        ),
        fixture["otherBlankNoteRequest"]["variables"].clone(),
    ));
    assert_eq!(explicit_other.status, 200);
    assert_eq!(
        explicit_other.body["data"]["returnRequest"]["userErrors"],
        json!([])
    );
    assert!(explicit_other.body["data"]["returnRequest"]["return"]["id"]
        .as_str()
        .is_some_and(|id| id.starts_with("gid://shopify/Return/")));
    assert_eq!(
        state_snapshot(&explicit_other_proxy)["stagedState"]["returns"]
            .as_object()
            .unwrap()
            .len(),
        1
    );

    let mut definition_proxy = return_reason_hydrated_proxy(&fixture);
    let definition = definition_proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/orders/return-request-reason-validation.graphql"
        ),
        fixture["otherDefinitionNoNoteRequest"]["variables"].clone(),
    ));
    assert_eq!(definition.status, 200);
    assert_eq!(
        definition.body["data"]["returnRequest"]["userErrors"],
        json!([])
    );
    assert!(definition.body["data"]["returnRequest"]["return"]["id"]
        .as_str()
        .is_some_and(|id| id.starts_with("gid://shopify/Return/")));
    assert_eq!(
        state_snapshot(&definition_proxy)["stagedState"]["returns"]
            .as_object()
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn remove_from_return_rejects_closed_return_without_state_changes() {
    let mut proxy = snapshot_proxy();
    let setup = stage_open_return_for_removal(&mut proxy);

    let close = proxy.process_request(json_graphql_request(
        r#"
        mutation CloseBeforeRemoval($id: ID!) {
          returnClose(id: $id) {
            return {
              id
              status
              totalQuantity
              returnLineItems(first: 5) {
                nodes { id quantity processedQuantity unprocessedQuantity }
              }
              reverseFulfillmentOrders(first: 5) {
                nodes {
                  id
                  lineItems(first: 5) {
                    nodes { id totalQuantity fulfillmentLineItem { id } }
                  }
                }
              }
            }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "id": setup.return_id.clone() }),
    ));
    assert_eq!(close.status, 200);
    assert_eq!(close.body["data"]["returnClose"]["userErrors"], json!([]));
    let closed_return = close.body["data"]["returnClose"]["return"].clone();
    assert_eq!(closed_return["status"], json!("CLOSED"));
    assert_eq!(closed_return["totalQuantity"], json!(2));
    let log_before = log_snapshot(&proxy);

    let rejected = remove_from_return_for_test(
        &mut proxy,
        setup.return_id.clone(),
        setup.return_line_item_id,
    );

    assert_eq!(rejected["return"], Value::Null);
    assert_eq!(
        rejected["userErrors"],
        json!([{
            "field": ["returnId"],
            "message": "Return status is invalid.",
            "code": "INVALID_STATE"
        }])
    );
    assert_eq!(log_snapshot(&proxy), log_before);

    let read_after = read_return_removal_state(&mut proxy, setup.return_id, setup.order_id);
    assert_eq!(read_after["return"], closed_return);
    assert_eq!(read_after["order"]["returns"]["nodes"][0], closed_return);
}

#[test]
fn remove_from_return_allows_requested_returns() {
    let mut proxy = snapshot_proxy();
    let setup = stage_requested_return_for_removal(&mut proxy);

    let removed = remove_from_return_for_test(
        &mut proxy,
        setup.return_id.clone(),
        setup.return_line_item_id,
    );

    assert_eq!(removed["userErrors"], json!([]));
    assert_eq!(removed["return"]["status"], json!("REQUESTED"));
    assert_eq!(removed["return"]["totalQuantity"], json!(1));
    assert_eq!(
        removed["return"]["returnLineItems"]["nodes"][0]["quantity"],
        json!(1)
    );
    assert_eq!(
        removed["return"]["returnLineItems"]["nodes"][0]["unprocessedQuantity"],
        json!(1)
    );
    assert_eq!(
        removed["return"]["reverseFulfillmentOrders"],
        json!({ "nodes": [] })
    );

    let read_after = read_return_removal_state(&mut proxy, setup.return_id, setup.order_id);
    assert_eq!(read_after["return"], removed["return"]);
    assert_eq!(
        read_after["order"]["returns"]["nodes"][0],
        removed["return"]
    );
}

#[test]
fn return_create_and_request_reject_quantities_beyond_remaining_fulfillment() {
    let mut proxy = snapshot_proxy();
    let (order_id, fulfillment_line_item_id) = stage_fulfilled_order_for_return(&mut proxy);

    let initial = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateInitialQuantityCapReturn($returnInput: ReturnInput!) {
          returnCreate(returnInput: $returnInput) {
            return {
              id
              status
              totalQuantity
              returnLineItems(first: 5) {
                nodes { id quantity processedQuantity unprocessedQuantity }
              }
              reverseFulfillmentOrders(first: 5) {
                nodes {
                  id
                  lineItems(first: 5) {
                    nodes { id totalQuantity fulfillmentLineItem { id } }
                  }
                }
              }
            }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "returnInput": {
                "orderId": order_id.clone(),
                "returnLineItems": [{
                    "fulfillmentLineItemId": fulfillment_line_item_id.clone(),
                    "quantity": 2,
                    "returnReason": "UNWANTED"
                }]
            }
        }),
    ));
    assert_eq!(initial.status, 200);
    assert_eq!(
        initial.body["data"]["returnCreate"]["userErrors"],
        json!([])
    );
    let staged_return = initial.body["data"]["returnCreate"]["return"].clone();
    let log_before_rejections = log_snapshot(&proxy);

    let over_request = proxy.process_request(json_graphql_request(
        r#"
        mutation RequestBeyondRemainingQuantity($input: ReturnRequestInput!) {
          returnRequest(input: $input) {
            return { id status }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "input": {
                "orderId": order_id.clone(),
                "returnLineItems": [{
                    "fulfillmentLineItemId": fulfillment_line_item_id.clone(),
                    "quantity": 1,
                    "returnReason": "UNWANTED"
                }]
            }
        }),
    ));
    assert_eq!(over_request.status, 200);
    assert_eq!(
        over_request.body["data"]["returnRequest"]["return"],
        Value::Null
    );
    assert_eq!(
        over_request.body["data"]["returnRequest"]["userErrors"],
        json!([{
            "field": ["input", "returnLineItems", "0", "quantity"],
            "message": "Return line item has an invalid quantity.",
            "code": "INVALID"
        }])
    );

    let over_create = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateBeyondRemainingQuantity($returnInput: ReturnInput!) {
          returnCreate(returnInput: $returnInput) {
            return { id status }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "returnInput": {
                "orderId": order_id.clone(),
                "returnLineItems": [{
                    "fulfillmentLineItemId": fulfillment_line_item_id,
                    "quantity": 1,
                    "returnReason": "UNWANTED"
                }]
            }
        }),
    ));
    assert_eq!(over_create.status, 200);
    assert_eq!(
        over_create.body["data"]["returnCreate"]["return"],
        Value::Null
    );
    assert_eq!(
        over_create.body["data"]["returnCreate"]["userErrors"],
        json!([{
            "field": ["returnInput", "returnLineItems", "0", "quantity"],
            "message": "Return line item has an invalid quantity.",
            "code": "INVALID"
        }])
    );

    assert_eq!(log_snapshot(&proxy), log_before_rejections);
    let read_after = read_return_removal_state(&mut proxy, staged_return["id"].clone(), order_id);
    assert_eq!(read_after["return"], staged_return);
    assert_eq!(
        read_after["order"]["returns"]["nodes"],
        json!([staged_return])
    );
}

#[test]
fn remove_from_return_rejects_zero_and_over_quantity_without_state_changes() {
    let mut proxy = snapshot_proxy();
    let setup = stage_open_return_for_removal(&mut proxy);
    let before =
        read_return_removal_state(&mut proxy, setup.return_id.clone(), setup.order_id.clone());
    let log_before_rejections = log_snapshot(&proxy);

    for quantity in [3, 0] {
        let response = proxy.process_request(json_graphql_request(
            r#"
            mutation RemoveInvalidQuantity($returnId: ID!, $returnLineItems: [ReturnLineItemRemoveFromReturnInput!]) {
              removeFromReturn(returnId: $returnId, returnLineItems: $returnLineItems) {
                return { id totalQuantity }
                userErrors { field message }
              }
            }
            "#,
            json!({
                "returnId": setup.return_id.clone(),
                "returnLineItems": [{
                    "returnLineItemId": setup.return_line_item_id.clone(),
                    "quantity": quantity
                }]
            }),
        ));

        assert_eq!(response.status, 200);
        assert_eq!(
            response.body["data"]["removeFromReturn"]["return"],
            Value::Null
        );
        assert_eq!(
            response.body["data"]["removeFromReturn"]["userErrors"],
            if quantity == 0 {
                json!([{
                    "field": null,
                    "message": "Quantity must be greater than 0"
                }])
            } else {
                json!([{
                    "field": ["returnLineItems", "0", "quantity"],
                    "message": "Return line item has an invalid quantity."
                }])
            },
            "quantity {quantity} should be rejected without staging a removal"
        );
        assert_eq!(log_snapshot(&proxy), log_before_rejections);
        assert_eq!(
            read_return_removal_state(&mut proxy, setup.return_id.clone(), setup.order_id.clone()),
            before
        );
    }
}

#[test]
fn return_request_approval_and_decline_invalid_states_use_shopify_error_shapes() {
    let mut proxy = snapshot_proxy();
    let open_return = stage_open_return_for_removal(&mut proxy);

    let rejected_approval =
        approve_return_request_for_test(&mut proxy, open_return.return_id.clone());
    assert_eq!(rejected_approval["return"], Value::Null);
    assert_eq!(
        rejected_approval["userErrors"],
        json!([{
            "field": ["input", "id"],
            "message": "Return is not approvable. Only returns with status REQUESTED can be approved."
        }])
    );

    let rejected_decline = decline_return_request_for_test(&mut proxy, open_return.return_id);
    assert_eq!(rejected_decline["return"], Value::Null);
    assert_eq!(
        rejected_decline["userErrors"],
        json!([{
            "field": ["input", "id"],
            "message": "Return is not declinable. Only non-refunded returns with status REQUESTED can be declined."
        }])
    );

    let requested_return = stage_requested_return_for_removal(&mut proxy);
    let first_decline =
        decline_return_request_for_test(&mut proxy, requested_return.return_id.clone());
    assert_eq!(first_decline["userErrors"], json!([]));
    assert_eq!(first_decline["return"]["status"], json!("DECLINED"));

    let rejected_second_decline =
        decline_return_request_for_test(&mut proxy, requested_return.return_id);
    assert_eq!(rejected_second_decline["return"], Value::Null);
    assert_eq!(
        rejected_second_decline["userErrors"],
        json!([{
            "field": ["input", "id"],
            "message": "The return is already declined."
        }])
    );
}

#[test]
fn return_request_approval_and_decline_unknown_ids_use_not_found_shape() {
    let mut proxy = snapshot_proxy();

    let rejected_approval =
        approve_return_request_for_test(&mut proxy, json!("gid://shopify/Return/999999999991"));
    assert_eq!(rejected_approval["return"], Value::Null);
    assert_eq!(
        rejected_approval["userErrors"],
        json!([{
            "field": ["input", "id"],
            "message": "Return not found."
        }])
    );

    let rejected_decline =
        decline_return_request_for_test(&mut proxy, json!("gid://shopify/Return/999999999992"));
    assert_eq!(rejected_decline["return"], Value::Null);
    assert_eq!(
        rejected_decline["userErrors"],
        json!([{
            "field": ["input", "id"],
            "message": "Return not found."
        }])
    );
}

#[test]
fn return_lifecycle_and_process_unknown_ids_use_not_found_shape() {
    let mut proxy = snapshot_proxy();
    let missing_return_id = json!("gid://shopify/Return/999999999999999");
    let missing_return_line_item_id = json!("gid://shopify/ReturnLineItem/999999999999999");

    for (payload, field) in [
        (
            return_lifecycle_transition_for_test(
                &mut proxy,
                "returnCancel",
                missing_return_id.clone(),
            ),
            json!(["id"]),
        ),
        (
            return_lifecycle_transition_for_test(
                &mut proxy,
                "returnClose",
                missing_return_id.clone(),
            ),
            json!(["id"]),
        ),
        (
            return_lifecycle_transition_for_test(
                &mut proxy,
                "returnReopen",
                missing_return_id.clone(),
            ),
            json!(["id"]),
        ),
        (
            remove_from_return_for_test(
                &mut proxy,
                missing_return_id.clone(),
                missing_return_line_item_id.clone(),
            ),
            json!(["returnId"]),
        ),
        (
            return_process_for_test(&mut proxy, missing_return_id, missing_return_line_item_id),
            json!(["input", "returnId"]),
        ),
    ] {
        assert_eq!(payload["return"], Value::Null);
        assert_eq!(
            payload["userErrors"],
            json!([{
                "field": field,
                "message": "Return not found.",
                "code": "NOT_FOUND"
            }])
        );
    }
}

#[test]
fn return_decline_request_invalid_decline_reason_variable_fails_schema_validation() {
    let mut proxy = snapshot_proxy();

    let response = proxy.process_request(json_graphql_request(
        r#"
        mutation ReturnDeclineRequestInvalidReason($input: ReturnDeclineRequestInput!) {
          returnDeclineRequest(input: $input) {
            return { id status decline { reason note } }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "input": {
                "id": "gid://shopify/Return/999999999",
                "declineReason": "BANANAS",
                "notifyCustomer": false
            }
        }),
    ));

    assert_eq!(response.status, 200);
    assert_eq!(response.body.get("data"), None);
    assert_eq!(
        response.body["errors"][0]["message"],
        json!(
            "Variable $input of type ReturnDeclineRequestInput! was provided invalid value for declineReason (Expected \"BANANAS\" to be one of: RETURN_PERIOD_ENDED, FINAL_SALE, OTHER)"
        )
    );
    assert_eq!(
        response.body["errors"][0]["extensions"]["code"],
        json!("INVALID_VARIABLE")
    );
    assert_eq!(
        response.body["errors"][0]["extensions"]["problems"][0],
        json!({
            "path": ["declineReason"],
            "explanation": "Expected \"BANANAS\" to be one of: RETURN_PERIOD_ENDED, FINAL_SALE, OTHER"
        })
    );
    assert_eq!(log_snapshot(&proxy)["entries"], json!([]));
}

#[test]
fn return_decline_request_hidden_notify_payload_variable_fails_schema_validation() {
    let mut proxy = snapshot_proxy();

    let response = proxy.process_request(json_graphql_request(
        r#"
        mutation ReturnDeclineRequestUnknownNotifyPayload($input: ReturnDeclineRequestInput!) {
          returnDeclineRequest(input: $input) {
            return { id }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "input": {
                "id": "gid://shopify/Return/999999999",
                "declineReason": "OTHER",
                "notifyCustomer": true,
                "tmp_notify_customer": {
                    "email_address": "not-an-email"
                }
            }
        }),
    ));

    assert_eq!(response.status, 200);
    assert_eq!(response.body.get("data"), None);
    assert_eq!(
        response.body["errors"][0]["message"],
        json!(
            "Variable $input of type ReturnDeclineRequestInput! was provided invalid value for tmp_notify_customer (Field is not defined on ReturnDeclineRequestInput)"
        )
    );
    assert_eq!(
        response.body["errors"][0]["extensions"]["code"],
        json!("INVALID_VARIABLE")
    );
    assert_eq!(
        response.body["errors"][0]["extensions"]["problems"][0],
        json!({
            "path": ["tmp_notify_customer"],
            "explanation": "Field is not defined on ReturnDeclineRequestInput"
        })
    );
    assert_eq!(log_snapshot(&proxy)["entries"], json!([]));
}

#[test]
fn order_create_stages_rich_order_and_downstream_reads() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../fixtures/conformance/very-big-test-store.myshopify.com/2025-01/orders/order-create-parity.json"
    ))
    .unwrap();
    let mut proxy = snapshot_proxy();

    let create = proxy.process_request(json_graphql_request(
        include_str!("../../config/parity-requests/orders/orderCreate-parity-plan.graphql"),
        fixture["variables"].clone(),
    ));

    assert_eq!(create.status, 200);
    assert_eq!(create.body["data"]["orderCreate"]["userErrors"], json!([]));
    let order = &create.body["data"]["orderCreate"]["order"];
    assert_eq!(
        order["email"],
        fixture["mutation"]["response"]["data"]["orderCreate"]["order"]["email"]
    );
    assert_eq!(order["displayFinancialStatus"], json!("PAID"));
    assert_eq!(order["displayFulfillmentStatus"], json!("FULFILLED"));
    assert_eq!(
        order["currentTotalPriceSet"]["shopMoney"]["amount"],
        json!("42.5")
    );
    assert_eq!(order["totalTaxSet"]["shopMoney"]["amount"], json!("2.5"));
    assert_eq!(
        order["totalDiscountsSet"]["shopMoney"]["amount"],
        json!("5.0")
    );
    assert_eq!(order["discountCodes"], json!(["SAVE5"]));
    assert_eq!(
        order["lineItems"]["nodes"][0]["originalUnitPriceSet"],
        fixture["mutation"]["response"]["data"]["orderCreate"]["order"]["lineItems"]["nodes"][0]
            ["originalUnitPriceSet"]
    );

    let order_id = order["id"].clone();
    let downstream = proxy.process_request(json_graphql_request(
        include_str!("../../config/parity-requests/orders/orderCreate-downstream-read.graphql"),
        json!({ "id": order_id.clone() }),
    ));
    assert_eq!(
        downstream.body["data"]["order"]["email"],
        fixture["downstreamRead"]["response"]["data"]["order"]["email"]
    );
    assert_eq!(
        downstream.body["data"]["order"]["lineItems"]["nodes"][0]["taxLines"],
        fixture["downstreamRead"]["response"]["data"]["order"]["lineItems"]["nodes"][0]["taxLines"]
    );

    let catalog = proxy.process_request(json_graphql_request(
        r#"
        query OrderCreateCatalog($id: ID!) {
          byId: order(id: $id) { id email }
          orders(first: 5) { nodes { id email } }
          ordersCount { count precision }
        }
        "#,
        json!({ "id": order_id }),
    ));
    assert_eq!(catalog.body["data"]["byId"]["email"], order["email"]);
    assert_eq!(
        catalog.body["data"]["orders"]["nodes"][0]["email"],
        order["email"]
    );
    assert_eq!(
        catalog.body["data"]["ordersCount"],
        json!({ "count": 1, "precision": "EXACT" })
    );
}

#[test]
fn order_create_names_do_not_reuse_numbers_after_delete() {
    let mut proxy = snapshot_proxy();
    let create_query = r#"
        mutation CreateOrderForNumbering($order: OrderCreateOrderInput!) {
          orderCreate(order: $order) {
            order { id name email }
            userErrors { field message code }
          }
        }
    "#;
    let delete_query = r#"
        mutation DeleteOrderForNumbering($orderId: ID!) {
          orderDelete(orderId: $orderId) {
            deletedId
            userErrors { field message code }
          }
        }
    "#;

    let first = proxy.process_request(json_graphql_request(
        create_query,
        json!({
            "order": {
                "email": "numbering-first@example.test",
                "currency": "USD",
                "lineItems": [{
                    "title": "Numbering first",
                    "quantity": 1,
                    "priceSet": { "shopMoney": { "amount": "10.00", "currencyCode": "USD" } }
                }]
            }
        }),
    ));
    assert_eq!(first.body["data"]["orderCreate"]["userErrors"], json!([]));
    assert_eq!(
        first.body["data"]["orderCreate"]["order"]["name"],
        json!("#1")
    );
    let first_id = first.body["data"]["orderCreate"]["order"]["id"].clone();

    let second = proxy.process_request(json_graphql_request(
        create_query,
        json!({
            "order": {
                "email": "numbering-second@example.test",
                "currency": "USD",
                "lineItems": [{
                    "title": "Numbering second",
                    "quantity": 1,
                    "priceSet": { "shopMoney": { "amount": "11.00", "currencyCode": "USD" } }
                }]
            }
        }),
    ));
    assert_eq!(
        second.body["data"]["orderCreate"]["order"]["name"],
        json!("#2")
    );

    let delete = proxy.process_request(json_graphql_request(
        delete_query,
        json!({ "orderId": first_id }),
    ));
    assert_eq!(delete.body["data"]["orderDelete"]["userErrors"], json!([]));

    let third = proxy.process_request(json_graphql_request(
        create_query,
        json!({
            "order": {
                "email": "numbering-third@example.test",
                "currency": "USD",
                "lineItems": [{
                    "title": "Numbering third",
                    "quantity": 1,
                    "priceSet": { "shopMoney": { "amount": "12.00", "currencyCode": "USD" } }
                }]
            }
        }),
    ));
    assert_eq!(third.body["data"]["orderCreate"]["userErrors"], json!([]));
    assert_eq!(
        third.body["data"]["orderCreate"]["order"]["name"],
        json!("#3")
    );
}

#[test]
fn orders_search_unsupported_predicates_do_not_match_everything() {
    let mut proxy = snapshot_proxy();

    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateOrderForUnsupportedSearch($order: OrderCreateOrderInput!) {
          orderCreate(order: $order) {
            order { id email }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "order": {
                "email": "unsupported-order-search@example.test",
                "currency": "USD",
                "lineItems": [{
                    "title": "Unsupported search term",
                    "quantity": 1,
                    "priceSet": { "shopMoney": { "amount": "10.00", "currencyCode": "USD" } }
                }]
            }
        }),
    ));
    assert_eq!(create.status, 200);
    assert_eq!(create.body["data"]["orderCreate"]["userErrors"], json!([]));

    let read = proxy.process_request(json_graphql_request(
        r#"
        query UnsupportedOrderSearchPredicate {
          orders(first: 5, query: "warehouse:nowhere") { nodes { id email } }
          ordersCount(query: "warehouse:nowhere") { count precision }
        }
        "#,
        json!({}),
    ));

    assert_eq!(read.status, 200);
    assert_eq!(read.body["data"]["orders"]["nodes"], json!([]));
    assert_eq!(
        read.body["data"]["ordersCount"],
        json!({ "count": 0, "precision": "EXACT" })
    );
}

#[test]
fn orders_search_common_predicates_share_connection_and_count_semantics() {
    fn create_order(
        proxy: &mut DraftProxy,
        email: &str,
        tag: &str,
        title: &str,
        processed_at: &str,
    ) -> String {
        let create = proxy.process_request(json_graphql_request(
            r#"
            mutation CreateOrderForSearchPredicates($order: OrderCreateOrderInput!) {
              orderCreate(order: $order) {
                order {
                  id
                  email
                  tags
                  createdAt
                  updatedAt
                  processedAt
                  displayFinancialStatus
                  displayFulfillmentStatus
                }
                userErrors { field message }
              }
            }
            "#,
            json!({
                "order": {
                    "email": email,
                    "currency": "USD",
                    "financialStatus": "PENDING",
                    "processedAt": processed_at,
                    "tags": [tag],
                    "lineItems": [{
                        "title": title,
                        "quantity": 1,
                        "priceSet": { "shopMoney": { "amount": "10.00", "currencyCode": "USD" } }
                    }]
                }
            }),
        ));
        assert_eq!(create.status, 200);
        assert_eq!(create.body["data"]["orderCreate"]["userErrors"], json!([]));
        create.body["data"]["orderCreate"]["order"]["id"]
            .as_str()
            .unwrap()
            .to_string()
    }

    let mut proxy = snapshot_proxy().with_clock(|| utc_time(1_704_067_200));
    let alpha_id = create_order(
        &mut proxy,
        "alpha-order-search@example.test",
        "vip",
        "Alpha search predicates",
        "2024-02-02T03:04:05Z",
    );
    let zulu_id = create_order(
        &mut proxy,
        "zulu-order-search@example.test",
        "standard",
        "Zulu search predicates",
        "2024-03-03T03:04:05Z",
    );

    let update = proxy.process_request(json_graphql_request(
        r#"
        mutation UpdateOrderForSearchPredicates($input: OrderInput!) {
          orderUpdate(input: $input) {
            order { id note updatedAt }
            userErrors { field message }
          }
        }
        "#,
        json!({ "input": { "id": alpha_id.clone(), "note": "updated for date filter" } }),
    ));
    assert_eq!(update.status, 200);
    assert_eq!(update.body["data"]["orderUpdate"]["userErrors"], json!([]));

    let combined_query = "id:1 tag:vip email:alpha-order-search@example.test financial_status:pending fulfillment_status:unfulfilled created_at:>=2024-01-01 created_at:<2024-01-02 processed_at:>=2024-02-02 processed_at:<2024-02-03 updated_at:>=2024-01-01T00:00:02.000Z";
    let id_miss_query = format!("id:{alpha_id}");
    let read = proxy.process_request(json_graphql_request(
        r#"
        query OrdersSearchCommonPredicates(
          $combinedQuery: String!
          $idMissQuery: String!
          $tailQuery: String!
          $freeTextQuery: String!
          $freeTextMissQuery: String!
          $processedAtQuery: String!
        ) {
          combined: orders(first: 5, query: $combinedQuery) {
            nodes { id email tags processedAt updatedAt }
          }
          combinedCount: ordersCount(query: $combinedQuery) { count precision }
          idMiss: orders(first: 5, query: $idMissQuery) { nodes { id email } }
          idMissCount: ordersCount(query: $idMissQuery) { count precision }
          tail: orders(first: 5, query: $tailQuery) { nodes { id email } }
          freeText: orders(first: 5, query: $freeTextQuery) { nodes { id email } }
          freeTextMiss: orders(first: 5, query: $freeTextMissQuery) { nodes { id email } }
          processedRange: orders(first: 5, query: $processedAtQuery, sortKey: PROCESSED_AT, reverse: true) {
            nodes { id email processedAt }
          }
        }
        "#,
        json!({
            "combinedQuery": combined_query,
            "idMissQuery": id_miss_query,
            "tailQuery": "id:1",
            "freeTextQuery": "alpha-order-search",
            "freeTextMissQuery": "not-a-staged-order",
            "processedAtQuery": "processed_at:>=2024-03-01",
        }),
    ));
    assert_eq!(read.status, 200);
    assert_eq!(
        read.body["data"]["combined"]["nodes"],
        json!([{
            "id": alpha_id,
            "email": "alpha-order-search@example.test",
            "tags": ["vip"],
            "processedAt": "2024-02-02T03:04:05Z",
            "updatedAt": "2024-01-01T00:00:02.000Z"
        }])
    );
    assert_eq!(
        read.body["data"]["combinedCount"],
        json!({ "count": 1, "precision": "EXACT" })
    );
    assert_eq!(read.body["data"]["idMiss"]["nodes"], json!([]));
    assert_eq!(
        read.body["data"]["idMissCount"],
        json!({ "count": 0, "precision": "EXACT" })
    );
    assert_eq!(
        read.body["data"]["tail"]["nodes"],
        json!([{ "id": read.body["data"]["combined"]["nodes"][0]["id"].clone(), "email": "alpha-order-search@example.test" }])
    );
    assert_eq!(
        read.body["data"]["freeText"]["nodes"],
        json!([{ "id": read.body["data"]["combined"]["nodes"][0]["id"].clone(), "email": "alpha-order-search@example.test" }])
    );
    assert_eq!(read.body["data"]["freeTextMiss"]["nodes"], json!([]));
    assert_eq!(
        read.body["data"]["processedRange"]["nodes"],
        json!([{ "id": zulu_id, "email": "zulu-order-search@example.test", "processedAt": "2024-03-03T03:04:05Z" }])
    );
}

#[test]
fn live_hybrid_orders_merge_upstream_catalog_with_staged_create_update_delete() {
    let upstream_order_id = "gid://shopify/Order/9001";
    let upstream_order = json!({
        "id": upstream_order_id,
        "name": "#9001",
        "email": "existing-live-order@example.test",
        "note": "upstream baseline",
        "tags": ["hybrid"],
        "createdAt": "2024-01-02T00:00:00.000Z",
        "updatedAt": "2024-01-02T00:00:00.000Z",
        "processedAt": "2024-01-02T00:00:00.000Z",
        "closed": false,
        "closedAt": Value::Null,
        "cancelledAt": Value::Null,
        "cancelReason": Value::Null,
        "displayFinancialStatus": "PAID",
        "displayFulfillmentStatus": "UNFULFILLED",
        "totalPriceSet": { "shopMoney": { "amount": "25.0", "currencyCode": "USD" } },
        "currentTotalPriceSet": { "shopMoney": { "amount": "25.0", "currencyCode": "USD" } },
        "lineItems": { "nodes": [] }
    });
    let upstream_calls = Arc::new(AtomicUsize::new(0));
    let clock = Arc::new(Mutex::new(utc_time(1_704_240_000)));
    let mut proxy = configured_proxy(ReadMode::LiveHybrid, None)
        .with_clock({
            let clock = Arc::clone(&clock);
            move || *clock.lock().unwrap()
        })
        .with_upstream_transport({
            let upstream_order = upstream_order.clone();
            let upstream_calls = Arc::clone(&upstream_calls);
            move |_request| {
                upstream_calls.fetch_add(1, Ordering::SeqCst);
                Response {
                    status: 200,
                    headers: Default::default(),
                    body: json!({
                        "data": {
                            "order": upstream_order,
                            "existing": upstream_order,
                            "orders": {
                                "nodes": [upstream_order],
                                "edges": [{ "cursor": upstream_order_id, "node": upstream_order }],
                                "pageInfo": {
                                    "hasNextPage": false,
                                    "hasPreviousPage": false,
                                    "startCursor": upstream_order_id,
                                    "endCursor": upstream_order_id
                                }
                            },
                            "visible": {
                                "nodes": [upstream_order],
                                "edges": [{ "cursor": upstream_order_id, "node": upstream_order }],
                                "pageInfo": {
                                    "hasNextPage": false,
                                    "hasPreviousPage": false,
                                    "startCursor": upstream_order_id,
                                    "endCursor": upstream_order_id
                                }
                            },
                            "ordersCount": { "count": 1, "precision": "EXACT" },
                            "total": { "count": 1, "precision": "EXACT" }
                        }
                    }),
                }
            }
        });

    let catalog_query = r#"
        query LiveHybridMixedOrders($existingId: ID!, $query: String!, $first: Int!) {
          existing: order(id: $existingId) {
            id
            email
            note
            tags
          }
          visible: orders(first: $first, query: $query, sortKey: CREATED_AT, reverse: true) {
            nodes {
              id
              email
              note
              tags
              createdAt
            }
            edges {
              cursor
              node { id email }
            }
            pageInfo {
              hasNextPage
              hasPreviousPage
              startCursor
              endCursor
            }
          }
          total: ordersCount(query: $query, limit: null) {
            count
            precision
          }
        }
    "#;
    let catalog_variables =
        json!({ "existingId": upstream_order_id, "query": "tag:hybrid", "first": 5 });

    let cold = proxy.process_request(json_graphql_request(
        catalog_query,
        catalog_variables.clone(),
    ));
    assert_eq!(cold.status, 200);
    assert_eq!(
        cold.body["data"]["visible"]["nodes"][0]["id"],
        json!(upstream_order_id)
    );
    assert_eq!(upstream_calls.load(Ordering::SeqCst), 1);

    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateLocalOrderForMixedCatalog($order: OrderCreateOrderInput!) {
          orderCreate(order: $order) {
            order { id email tags createdAt }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "order": {
                "email": "local-live-order@example.test",
                "currency": "USD",
                "tags": ["hybrid"],
                "lineItems": [{
                    "title": "Local mixed catalog",
                    "quantity": 1,
                    "priceSet": { "shopMoney": { "amount": "10.00", "currencyCode": "USD" } }
                }]
            }
        }),
    ));
    assert_eq!(create.status, 200);
    assert_eq!(create.body["data"]["orderCreate"]["userErrors"], json!([]));
    let local_order_id = create.body["data"]["orderCreate"]["order"]["id"].clone();
    assert_eq!(
        create.body["data"]["orderCreate"]["order"]["createdAt"],
        json!("2024-01-03T00:00:00Z")
    );

    let mixed = proxy.process_request(json_graphql_request(
        catalog_query,
        catalog_variables.clone(),
    ));
    assert_eq!(mixed.status, 200);
    assert_eq!(
        mixed.body["data"]["visible"]["nodes"],
        json!([
            {
                "id": local_order_id,
                "email": "local-live-order@example.test",
                "note": Value::Null,
                "tags": ["hybrid"],
                "createdAt": "2024-01-03T00:00:00Z"
            },
            {
                "id": upstream_order_id,
                "email": "existing-live-order@example.test",
                "note": "upstream baseline",
                "tags": ["hybrid"],
                "createdAt": "2024-01-02T00:00:00.000Z"
            }
        ])
    );
    assert_eq!(
        mixed.body["data"]["total"],
        json!({ "count": 2, "precision": "EXACT" })
    );
    assert_eq!(
        mixed.body["data"]["existing"]["email"],
        json!("existing-live-order@example.test")
    );

    let update = proxy.process_request(json_graphql_request(
        r#"
        mutation UpdateExistingLiveOrder($input: OrderInput!) {
          orderUpdate(input: $input) {
            order { id note tags }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "input": {
                "id": upstream_order_id,
                "note": "locally edited upstream order",
                "tags": ["hybrid", "edited"]
            }
        }),
    ));
    assert_eq!(update.status, 200);
    assert_eq!(update.body["data"]["orderUpdate"]["userErrors"], json!([]));
    assert_eq!(
        update.body["data"]["orderUpdate"]["order"]["note"],
        json!("locally edited upstream order")
    );

    let after_update = proxy.process_request(json_graphql_request(
        catalog_query,
        catalog_variables.clone(),
    ));
    assert_eq!(
        after_update.body["data"]["visible"]["nodes"][1]["note"],
        json!("locally edited upstream order")
    );
    assert_eq!(
        after_update.body["data"]["total"],
        json!({ "count": 2, "precision": "EXACT" })
    );

    let delete = proxy.process_request(json_graphql_request(
        r#"
        mutation DeleteExistingLiveOrder($orderId: ID!) {
          orderDelete(orderId: $orderId) {
            deletedId
            userErrors { field message code }
          }
        }
        "#,
        json!({ "orderId": upstream_order_id }),
    ));
    assert_eq!(
        delete.body["data"]["orderDelete"],
        json!({ "deletedId": upstream_order_id, "userErrors": [] })
    );

    let after_delete =
        proxy.process_request(json_graphql_request(catalog_query, catalog_variables));
    assert_eq!(after_delete.body["data"]["existing"], Value::Null);
    assert_eq!(
        after_delete.body["data"]["visible"]["nodes"],
        json!([{
            "id": local_order_id,
            "email": "local-live-order@example.test",
            "note": Value::Null,
            "tags": ["hybrid"],
            "createdAt": "2024-01-03T00:00:00Z"
        }])
    );
    assert_eq!(
        after_delete.body["data"]["total"],
        json!({ "count": 1, "precision": "EXACT" })
    );
}

#[test]
fn live_hybrid_orders_observe_singular_order_id_when_selection_omits_id() {
    let upstream_order_id = "gid://shopify/Order/9101";
    let upstream_calls = Arc::new(AtomicUsize::new(0));
    let mut proxy = configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport({
        let upstream_calls = Arc::clone(&upstream_calls);
        move |_request| {
            upstream_calls.fetch_add(1, Ordering::SeqCst);
            Response {
                status: 200,
                headers: Default::default(),
                body: json!({
                    "data": {
                        "existing": {
                            "email": "idless-upstream-order@example.test",
                            "note": "upstream id supplied by root argument",
                            "tags": ["hybrid-idless"],
                            "processedAt": "2024-01-01T00:00:00Z"
                        },
                        "total": { "count": 1, "precision": "EXACT" }
                    }
                }),
            }
        }
    });

    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateLocalOrderForIdlessMixedCatalog($order: OrderCreateOrderInput!) {
          orderCreate(order: $order) {
            order { id email tags processedAt }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "order": {
                "email": "idless-local-order@example.test",
                "note": "local order",
                "currency": "USD",
                "processedAt": "2024-01-02T00:00:00Z",
                "tags": ["hybrid-idless"],
                "lineItems": [{
                    "title": "Local idless mixed catalog",
                    "quantity": 1,
                    "priceSet": { "shopMoney": { "amount": "10.00", "currencyCode": "USD" } }
                }]
            }
        }),
    ));
    assert_eq!(create.status, 200);
    assert_eq!(create.body["data"]["orderCreate"]["userErrors"], json!([]));

    let read = proxy.process_request(json_graphql_request(
        r#"
        query LiveHybridIdlessMixedOrders($existingId: ID!, $query: String!, $first: Int!) {
          existing: order(id: $existingId) {
            email
            note
            tags
            processedAt
          }
          visible: orders(first: $first, query: $query, sortKey: PROCESSED_AT, reverse: false) {
            nodes {
              email
              note
              tags
              processedAt
            }
            pageInfo {
              hasNextPage
              hasPreviousPage
            }
          }
          total: ordersCount(query: $query, limit: null) {
            count
            precision
          }
        }
        "#,
        json!({ "existingId": upstream_order_id, "query": "tag:hybrid-idless", "first": 5 }),
    ));

    assert_eq!(read.status, 200);
    assert_eq!(upstream_calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        read.body["data"]["existing"],
        json!({
            "email": "idless-upstream-order@example.test",
            "note": "upstream id supplied by root argument",
            "tags": ["hybrid-idless"],
            "processedAt": "2024-01-01T00:00:00Z"
        })
    );
    assert_eq!(
        read.body["data"]["visible"]["nodes"],
        json!([
            {
                "email": "idless-upstream-order@example.test",
                "note": "upstream id supplied by root argument",
                "tags": ["hybrid-idless"],
                "processedAt": "2024-01-01T00:00:00Z"
            },
            {
                "email": "idless-local-order@example.test",
                "note": "local order",
                "tags": ["hybrid-idless"],
                "processedAt": "2024-01-02T00:00:00Z"
            }
        ])
    );
    assert_eq!(
        read.body["data"]["total"],
        json!({ "count": 2, "precision": "EXACT" })
    );
}

#[test]
fn live_hybrid_orders_count_adds_staged_delta_to_upstream_count_without_nodes() {
    let mut proxy =
        configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport(|_request| Response {
            status: 200,
            headers: Default::default(),
            body: json!({
                "data": {
                    "total": { "count": 7, "precision": "EXACT" }
                }
            }),
        });

    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateLocalOrderForCountOnly($order: OrderCreateOrderInput!) {
          orderCreate(order: $order) {
            order { id tags }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "order": {
                "email": "count-only-local-order@example.test",
                "currency": "USD",
                "tags": ["count-only"],
                "lineItems": [{
                    "title": "Count only local",
                    "quantity": 1,
                    "priceSet": { "shopMoney": { "amount": "10.00", "currencyCode": "USD" } }
                }]
            }
        }),
    ));
    assert_eq!(create.status, 200);
    assert_eq!(create.body["data"]["orderCreate"]["userErrors"], json!([]));

    let count = proxy.process_request(json_graphql_request(
        r#"
        query CountOnlyMixedOrders($query: String!, $limit: Int) {
          total: ordersCount(query: $query, limit: $limit) {
            count
            precision
          }
        }
        "#,
        json!({ "query": "tag:count-only", "limit": 7 }),
    ));

    assert_eq!(count.status, 200);
    assert_eq!(
        count.body["data"]["total"],
        json!({ "count": 7, "precision": "AT_LEAST" })
    );
}

#[test]
fn orders_sorted_connection_handles_interleaved_create_and_update_windows() {
    fn create_order(proxy: &mut DraftProxy, email: &str, title: &str) -> String {
        let create = proxy.process_request(json_graphql_request(
            r#"
            mutation CreateOrderForConnectionWindow($order: OrderCreateOrderInput!) {
              orderCreate(order: $order) {
                order { id email createdAt updatedAt }
                userErrors { field message code }
              }
            }
            "#,
            json!({
                "order": {
                    "email": email,
                    "currency": "USD",
                    "lineItems": [{
                        "title": title,
                        "quantity": 1,
                        "priceSet": { "shopMoney": { "amount": "10.00", "currencyCode": "USD" } }
                    }]
                }
            }),
        ));
        assert_eq!(create.status, 200);
        assert_eq!(create.body["data"]["orderCreate"]["userErrors"], json!([]));
        create.body["data"]["orderCreate"]["order"]["id"]
            .as_str()
            .unwrap()
            .to_string()
    }

    let mut proxy = snapshot_proxy();
    let alpha_id = create_order(&mut proxy, "alpha-window@example.test", "Alpha window");
    let beta_id = create_order(&mut proxy, "beta-window@example.test", "Beta window");

    let first_page = proxy.process_request(json_graphql_request(
        r#"
        query OrdersCreatedFirstPage {
          orders(first: 1, sortKey: CREATED_AT, reverse: true) {
            edges { cursor node { id email } }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        first_page.body["data"]["orders"]["edges"],
        json!([{ "cursor": beta_id.clone(), "node": { "id": beta_id.clone(), "email": "beta-window@example.test" } }])
    );

    let gamma_id = create_order(&mut proxy, "gamma-window@example.test", "Gamma window");
    let next_page = proxy.process_request(json_graphql_request(
        r#"
        query OrdersCreatedNextPage($after: String!) {
          orders(first: 1, after: $after, sortKey: CREATED_AT, reverse: true) {
            nodes { id email }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
          ordersCount { count precision }
        }
        "#,
        json!({"after": first_page.body["data"]["orders"]["pageInfo"]["endCursor"]}),
    ));
    assert_eq!(
        next_page.body["data"]["orders"]["nodes"],
        json!([{ "id": alpha_id.clone(), "email": "alpha-window@example.test" }])
    );
    assert_eq!(
        next_page.body["data"]["orders"]["pageInfo"]["hasPreviousPage"],
        json!(true)
    );
    assert_eq!(
        next_page.body["data"]["ordersCount"],
        json!({ "count": 3, "precision": "EXACT" })
    );

    let update = proxy.process_request(json_graphql_request(
        r#"
        mutation MoveOrderAcrossUpdatedAtCursor($input: OrderInput!) {
          orderUpdate(input: $input) {
            order { id note updatedAt }
            userErrors { field message }
          }
        }
        "#,
        json!({"input": {"id": alpha_id.clone(), "note": "moves ahead of the prior cursor"}}),
    ));
    assert_eq!(update.status, 200);
    assert_eq!(update.body["data"]["orderUpdate"]["userErrors"], json!([]));

    let fresh_updated_page = proxy.process_request(json_graphql_request(
        r#"
        query OrdersUpdatedFreshPage {
          orders(first: 3, sortKey: UPDATED_AT, reverse: true) {
            nodes { id email }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        fresh_updated_page.body["data"]["orders"]["nodes"],
        json!([
            { "id": alpha_id.clone(), "email": "alpha-window@example.test" },
            { "id": gamma_id, "email": "gamma-window@example.test" },
            { "id": beta_id.clone(), "email": "beta-window@example.test" }
        ])
    );

    let after_stale_cursor = proxy.process_request(json_graphql_request(
        r#"
        query OrdersUpdatedAfterPriorCursor($after: String!) {
          orders(first: 1, after: $after, sortKey: UPDATED_AT, reverse: true) {
            nodes { id email }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
        }
        "#,
        json!({"after": first_page.body["data"]["orders"]["pageInfo"]["endCursor"]}),
    ));
    assert_eq!(
        after_stale_cursor.body["data"]["orders"]["nodes"],
        json!([])
    );
    assert_eq!(
        after_stale_cursor.body["data"]["orders"]["pageInfo"],
        json!({
            "hasNextPage": false,
            "hasPreviousPage": true,
            "startCursor": null,
            "endCursor": null
        })
    );
}

#[test]
fn order_total_price_sort_key_orders_by_amount() {
    fn create_priced_order(proxy: &mut DraftProxy, email: &str, amount: &str) -> String {
        let create = proxy.process_request(json_graphql_request(
            r#"
            mutation CreateOrderForScalarSortKeys($order: OrderCreateOrderInput!) {
              orderCreate(order: $order) {
                order { id email totalPriceSet { shopMoney { amount } } }
                userErrors { field message code }
              }
            }
            "#,
            json!({
                "order": {
                    "email": email,
                    "currency": "USD",
                    "lineItems": [{
                        "title": email,
                        "quantity": 1,
                        "priceSet": { "shopMoney": { "amount": amount, "currencyCode": "USD" } }
                    }]
                }
            }),
        ));
        assert_eq!(create.status, 200);
        assert_eq!(create.body["data"]["orderCreate"]["userErrors"], json!([]));
        create.body["data"]["orderCreate"]["order"]["id"]
            .as_str()
            .unwrap()
            .to_string()
    }

    let mut proxy = snapshot_proxy();
    let expensive_id = create_priced_order(&mut proxy, "expensive-sort@example.test", "30.00");
    let cheap_id = create_priced_order(&mut proxy, "cheap-sort@example.test", "10.00");
    let middle_id = create_priced_order(&mut proxy, "middle-sort@example.test", "20.00");
    assert_eq!(
        state_snapshot(&proxy)["stagedState"]["orders"][&expensive_id]["totalPriceSet"]
            ["shopMoney"]["amount"],
        json!("30.0")
    );

    let read = proxy.process_request(json_graphql_request(
        r#"
        query OrdersScalarSortKeys {
          byTotalPrice: orders(first: 10, sortKey: TOTAL_PRICE) {
            nodes { id email totalPriceSet { shopMoney { amount } } }
          }
          reverseWindow: orders(first: 1, sortKey: TOTAL_PRICE, reverse: true) {
            edges { cursor node { id email } }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
        }
        "#,
        json!({}),
    ));

    assert_eq!(read.status, 200);
    assert_eq!(
        read.body["data"]["byTotalPrice"]["nodes"],
        json!([
            { "id": cheap_id, "email": "cheap-sort@example.test", "totalPriceSet": { "shopMoney": { "amount": "10.0" } } },
            { "id": middle_id, "email": "middle-sort@example.test", "totalPriceSet": { "shopMoney": { "amount": "20.0" } } },
            { "id": expensive_id.clone(), "email": "expensive-sort@example.test", "totalPriceSet": { "shopMoney": { "amount": "30.0" } } }
        ])
    );
    assert_eq!(
        read.body["data"]["reverseWindow"],
        json!({
            "edges": [{
                "cursor": expensive_id.clone(),
                "node": {
                    "id": expensive_id.clone(),
                    "email": "expensive-sort@example.test"
                }
            }],
            "pageInfo": {
                "hasNextPage": true,
                "hasPreviousPage": false,
                "startCursor": expensive_id.clone(),
                "endCursor": expensive_id
            }
        })
    );
}

#[test]
fn orders_count_fallback_preserves_alias_and_selection() {
    let mut proxy = snapshot_proxy();

    let response = proxy.process_request(graphql_request(
        "POST",
        r#"{"query":"query { emptyCount: ordersCount { amount: count } }"}"#,
    ));

    assert_eq!(response.status, 200);
    assert_eq!(
        response.body,
        json!({
            "data": {
                "emptyCount": {
                    "amount": 0
                }
            }
        })
    );
}

#[test]
fn fulfillment_lifecycle_stages_against_created_order_fulfillment_order() {
    let mut proxy = snapshot_proxy();

    let create_order = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateFulfillableOrder($order: OrderCreateOrderInput!) {
          orderCreate(order: $order) {
            order {
              id
              fulfillments { id status trackingInfo { number url company } }
              fulfillmentOrders(first: 5) {
                nodes {
                  id
                  status
                  lineItems(first: 5) {
                    nodes { id totalQuantity remainingQuantity }
                  }
                }
              }
            }
            userErrors { field code message }
          }
        }
        "#,
        json!({
            "order": {
                "email": "fulfillment-lifecycle@example.test",
                "lineItems": [{
                    "title": "Fulfillable line",
                    "quantity": 2,
                    "priceSet": { "shopMoney": { "amount": "12.00", "currencyCode": "USD" } }
                }]
            }
        }),
    ));
    assert_eq!(create_order.status, 200);
    assert_eq!(
        create_order.body["data"]["orderCreate"]["userErrors"],
        json!([])
    );
    let order = &create_order.body["data"]["orderCreate"]["order"];
    let order_id = order["id"].clone();
    let fulfillment_order_id = order["fulfillmentOrders"]["nodes"][0]["id"].clone();
    let fulfillment_order_line_item_id =
        order["fulfillmentOrders"]["nodes"][0]["lineItems"]["nodes"][0]["id"].clone();

    let create_fulfillment = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateFulfillment($fulfillment: FulfillmentInput!) {
          fulfillmentCreate(fulfillment: $fulfillment) {
            fulfillment {
              id
              status
              trackingInfo { number url company }
            }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "fulfillment": {
                "lineItemsByFulfillmentOrder": [{
                    "fulfillmentOrderId": fulfillment_order_id,
                    "fulfillmentOrderLineItems": [{
                        "id": fulfillment_order_line_item_id,
                        "quantity": 1
                    }]
                }],
                "trackingInfo": {
                    "company": "Hermes",
                    "number": "TRACK-1",
                    "url": "https://tracking.example/TRACK-1"
                },
                "notifyCustomer": false
            }
        }),
    ));
    assert_eq!(create_fulfillment.status, 200);
    assert_eq!(
        create_fulfillment.body["data"]["fulfillmentCreate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        create_fulfillment.body["data"]["fulfillmentCreate"]["fulfillment"]["status"],
        json!("SUCCESS")
    );
    assert_eq!(
        create_fulfillment.body["data"]["fulfillmentCreate"]["fulfillment"]["trackingInfo"],
        json!([{
            "number": "TRACK-1",
            "url": "https://tracking.example/TRACK-1",
            "company": "Hermes"
        }])
    );
    let fulfillment_id =
        create_fulfillment.body["data"]["fulfillmentCreate"]["fulfillment"]["id"].clone();

    let update_tracking = proxy.process_request(json_graphql_request(
        r#"
        mutation UpdateFulfillmentTracking($fulfillmentId: ID!, $trackingInfoInput: FulfillmentTrackingInput!) {
          fulfillmentTrackingInfoUpdate(
            fulfillmentId: $fulfillmentId
            trackingInfoInput: $trackingInfoInput
          ) {
            fulfillment {
              id
              status
              trackingInfo { number url company }
            }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "fulfillmentId": fulfillment_id,
            "trackingInfoInput": {
                "company": "UPS",
                "numbers": ["TRACK-2", "TRACK-3"],
                "urls": [
                    "https://tracking.example/TRACK-2",
                    "https://tracking.example/TRACK-3"
                ]
            }
        }),
    ));
    assert_eq!(
        update_tracking.body["data"]["fulfillmentTrackingInfoUpdate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        update_tracking.body["data"]["fulfillmentTrackingInfoUpdate"]["fulfillment"]
            ["trackingInfo"],
        json!([
            {
                "number": "TRACK-2",
                "url": "https://tracking.example/TRACK-2",
                "company": "UPS"
            },
            {
                "number": "TRACK-3",
                "url": "https://tracking.example/TRACK-3",
                "company": "UPS"
            }
        ])
    );

    let cancel = proxy.process_request(json_graphql_request(
        r#"
        mutation CancelFulfillment($id: ID!) {
          fulfillmentCancel(id: $id) {
            fulfillment { id status trackingInfo { number url company } }
            userErrors { field message }
          }
        }
        "#,
        json!({ "id": fulfillment_id }),
    ));
    assert_eq!(
        cancel.body["data"]["fulfillmentCancel"]["userErrors"],
        json!([])
    );
    assert_eq!(
        cancel.body["data"]["fulfillmentCancel"]["fulfillment"]["status"],
        json!("CANCELLED")
    );

    let read_after = proxy.process_request(json_graphql_request(
        r#"
        query ReadOrderFulfillmentLifecycle($id: ID!) {
          order(id: $id) {
            id
            fulfillments {
              id
              status
              trackingInfo { number url company }
            }
            fulfillmentOrders(first: 5) {
              nodes {
                id
                status
                lineItems(first: 5) {
                  nodes { id totalQuantity remainingQuantity }
                }
              }
            }
          }
        }
        "#,
        json!({ "id": order_id }),
    ));
    assert_eq!(
        read_after.body["data"]["order"]["fulfillments"][0]["id"],
        cancel.body["data"]["fulfillmentCancel"]["fulfillment"]["id"]
    );
    assert_eq!(
        read_after.body["data"]["order"]["fulfillments"][0]["status"],
        json!("CANCELLED")
    );
    assert_eq!(
        read_after.body["data"]["order"]["fulfillments"][0]["trackingInfo"],
        update_tracking.body["data"]["fulfillmentTrackingInfoUpdate"]["fulfillment"]
            ["trackingInfo"]
    );
    assert_eq!(
        read_after.body["data"]["order"]["fulfillmentOrders"]["nodes"][0]["lineItems"]["nodes"][0]
            ["remainingQuantity"],
        json!(1)
    );

    let log = log_snapshot(&proxy);
    let entries = log["entries"].as_array().unwrap();
    assert_eq!(entries.len(), 4);
    assert_eq!(
        entries
            .iter()
            .map(|entry| entry["operationName"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec![
            "orderCreate",
            "fulfillmentCreate",
            "fulfillmentTrackingInfoUpdate",
            "fulfillmentCancel"
        ]
    );
    assert!(entries[1]["rawBody"]
        .as_str()
        .unwrap()
        .contains("CreateFulfillment"));
    assert!(entries[2]["rawBody"]
        .as_str()
        .unwrap()
        .contains("UpdateFulfillmentTracking"));
    assert!(entries[3]["rawBody"]
        .as_str()
        .unwrap()
        .contains("CancelFulfillment"));
    assert_eq!(entries[1]["status"], json!("staged"));
    assert_eq!(entries[2]["status"], json!("staged"));
    assert_eq!(entries[3]["status"], json!("staged"));
    assert!(entries[1]["stagedResourceIds"]
        .as_array()
        .unwrap()
        .contains(&cancel.body["data"]["fulfillmentCancel"]["fulfillment"]["id"]));
}

#[test]
fn fulfillment_create_names_are_order_scoped_sequence_numbers() {
    let mut proxy = snapshot_proxy();

    let create_order = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateFulfillmentNameOrder($order: OrderCreateOrderInput!) {
          orderCreate(order: $order) {
            order {
              id
              name
              fulfillmentOrders(first: 5) {
                nodes {
                  id
                  lineItems(first: 5) {
                    nodes { id totalQuantity remainingQuantity }
                  }
                }
              }
            }
            userErrors { field code message }
          }
        }
        "#,
        json!({
            "order": {
                "email": "fulfillment-name@example.test",
                "lineItems": [{
                    "title": "Fulfillment name sequence line",
                    "quantity": 2,
                    "priceSet": { "shopMoney": { "amount": "12.00", "currencyCode": "USD" } }
                }]
            }
        }),
    ));
    assert_eq!(create_order.status, 200);
    assert_eq!(
        create_order.body["data"]["orderCreate"]["userErrors"],
        json!([])
    );
    let order = &create_order.body["data"]["orderCreate"]["order"];
    let order_name = order["name"].as_str().unwrap();
    let fulfillment_order_id = order["fulfillmentOrders"]["nodes"][0]["id"].clone();
    let fulfillment_order_line_item_id =
        order["fulfillmentOrders"]["nodes"][0]["lineItems"]["nodes"][0]["id"].clone();

    let create_fulfillment_query = r#"
        mutation CreateNamedFulfillment($fulfillment: FulfillmentInput!) {
          fulfillmentCreate(fulfillment: $fulfillment) {
            fulfillment {
              id
              name
              status
            }
            userErrors { field message }
          }
        }
        "#;

    let first_fulfillment = proxy.process_request(json_graphql_request(
        create_fulfillment_query,
        json!({
            "fulfillment": {
                "lineItemsByFulfillmentOrder": [{
                    "fulfillmentOrderId": fulfillment_order_id,
                    "fulfillmentOrderLineItems": [{
                        "id": fulfillment_order_line_item_id,
                        "quantity": 1
                    }]
                }]
            }
        }),
    ));
    assert_eq!(first_fulfillment.status, 200);
    assert_eq!(
        first_fulfillment.body["data"]["fulfillmentCreate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        first_fulfillment.body["data"]["fulfillmentCreate"]["fulfillment"]["name"],
        json!(format!("{order_name}-F1"))
    );

    let second_fulfillment = proxy.process_request(json_graphql_request(
        create_fulfillment_query,
        json!({
            "fulfillment": {
                "lineItemsByFulfillmentOrder": [{
                    "fulfillmentOrderId": fulfillment_order_id,
                    "fulfillmentOrderLineItems": [{
                        "id": fulfillment_order_line_item_id,
                        "quantity": 1
                    }]
                }]
            }
        }),
    ));
    assert_eq!(second_fulfillment.status, 200);
    assert_eq!(
        second_fulfillment.body["data"]["fulfillmentCreate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        second_fulfillment.body["data"]["fulfillmentCreate"]["fulfillment"]["name"],
        json!(format!("{order_name}-F2"))
    );
}

#[test]
fn fulfillment_event_create_stages_event_and_top_level_read_after_write() {
    let mut proxy = snapshot_proxy();
    let (order_id, fulfillment_id) = stage_fulfillment_for_event(&mut proxy);

    let event_create = proxy.process_request(json_graphql_request(
        r#"
        mutation FulfillmentEventCreateRuntime($fulfillmentEvent: FulfillmentEventInput!) {
          fulfillmentEventCreate(fulfillmentEvent: $fulfillmentEvent) {
            fulfillmentEvent {
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
            userErrors { field message }
          }
        }
        "#,
        json!({
            "fulfillmentEvent": {
                "fulfillmentId": fulfillment_id,
                "status": "IN_TRANSIT",
                "message": "Package scanned in transit",
                "happenedAt": "2026-05-05T20:21:01Z",
                "estimatedDeliveryAt": "2026-05-07T20:20:01Z",
                "city": "Toronto",
                "province": "Ontario",
                "country": "Canada",
                "zip": "M5H 2M9",
                "address1": "123 Queen St W",
                "latitude": 43.6532,
                "longitude": -79.3832
            }
        }),
    ));
    assert_eq!(event_create.status, 200);
    assert_eq!(
        event_create.body["data"]["fulfillmentEventCreate"]["userErrors"],
        json!([])
    );
    let event = &event_create.body["data"]["fulfillmentEventCreate"]["fulfillmentEvent"];
    assert_eq!(event["status"], json!("IN_TRANSIT"));
    assert_eq!(event["message"], json!("Package scanned in transit"));
    assert_eq!(event["createdAt"], json!("2024-01-01T00:00:03.000Z"));
    let event_id = event["id"].clone();

    let top_level_read = proxy.process_request(json_graphql_request(
        r#"
        query FulfillmentEventTopLevelRead($id: ID!) {
          fulfillment(id: $id) {
            id
            status
            displayStatus
            estimatedDeliveryAt
            inTransitAt
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
          }
        }
        "#,
        json!({ "id": fulfillment_id }),
    ));
    assert_eq!(top_level_read.status, 200);
    let fulfillment = &top_level_read.body["data"]["fulfillment"];
    assert_eq!(fulfillment["displayStatus"], json!("IN_TRANSIT"));
    assert_eq!(fulfillment["inTransitAt"], json!("2026-05-05T20:21:01Z"));
    assert_eq!(
        fulfillment["estimatedDeliveryAt"],
        json!("2026-05-07T20:20:01Z")
    );
    assert_eq!(fulfillment["events"]["nodes"][0]["id"], event_id);
    assert_eq!(
        fulfillment["events"]["nodes"][0]["message"],
        json!("Package scanned in transit")
    );

    let nested_order_read = proxy.process_request(json_graphql_request(
        r#"
        query FulfillmentEventNestedOrderRead($id: ID!) {
          order(id: $id) {
            fulfillments {
              id
              displayStatus
              events(first: 5) { nodes { id status message } }
            }
          }
        }
        "#,
        json!({ "id": order_id }),
    ));
    assert_eq!(
        nested_order_read.body["data"]["order"]["fulfillments"][0]["events"]["nodes"][0]["id"],
        event_id
    );

    let log = log_snapshot(&proxy);
    let entries = log["entries"].as_array().unwrap();
    assert_eq!(
        entries
            .iter()
            .map(|entry| entry["operationName"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec!["orderCreate", "fulfillmentCreate", "fulfillmentEventCreate"]
    );
    assert!(entries[2]["rawBody"]
        .as_str()
        .unwrap()
        .contains("FulfillmentEventCreateRuntime"));
    assert!(entries[2]["stagedResourceIds"]
        .as_array()
        .unwrap()
        .contains(&event_id));
}

#[test]
fn fulfillment_event_create_rejects_unknown_real_fulfillment_gid() {
    let mut proxy = snapshot_proxy();

    let response = proxy.process_request(json_graphql_request(
        r#"
        mutation FulfillmentEventCreateUnknown($fulfillmentEvent: FulfillmentEventInput!) {
          fulfillmentEventCreate(fulfillmentEvent: $fulfillmentEvent) {
            fulfillmentEvent { id status }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "fulfillmentEvent": {
                "fulfillmentId": "gid://shopify/Fulfillment/1234567890",
                "status": "IN_TRANSIT"
            }
        }),
    ));

    assert_eq!(response.status, 200);
    assert_eq!(
        response.body["data"]["fulfillmentEventCreate"],
        json!({
            "fulfillmentEvent": null,
            "userErrors": [{
                "field": ["fulfillmentEvent", "fulfillmentId"],
                "message": "Fulfillment does not exist."
            }]
        })
    );
    assert_eq!(log_snapshot(&proxy)["entries"], json!([]));
}

#[test]
fn fulfillment_cancel_returns_not_found_for_unknown_fulfillment_gid() {
    let mut proxy = snapshot_proxy();

    let response = proxy.process_request(json_graphql_request(
        r#"
        mutation FulfillmentCancelUnknown($id: ID!) {
          cancelAlias: fulfillmentCancel(id: $id) {
            fulfillment { id }
            userErrors { field message }
          }
        }
        "#,
        json!({ "id": "gid://shopify/Fulfillment/999999999" }),
    ));

    assert_eq!(response.status, 200);
    assert_eq!(
        response.body["data"]["cancelAlias"],
        json!({
            "fulfillment": null,
            "userErrors": [{
                "field": ["id"],
                "message": "Fulfillment not found."
            }]
        })
    );
    assert_eq!(log_snapshot(&proxy)["entries"], json!([]));
}

#[test]
fn fulfillment_tracking_update_returns_not_found_for_unknown_fulfillment_gid() {
    let mut proxy = snapshot_proxy();

    let response = proxy.process_request(json_graphql_request(
        r#"
        mutation FulfillmentTrackingInfoUpdateUnknown($fulfillmentId: ID!, $trackingInfoInput: FulfillmentTrackingInput!) {
          trackingAlias: fulfillmentTrackingInfoUpdate(
            fulfillmentId: $fulfillmentId
            trackingInfoInput: $trackingInfoInput
          ) {
            fulfillment { id }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "fulfillmentId": "gid://shopify/Fulfillment/999999999",
            "trackingInfoInput": {
                "company": "UPS",
                "number": "UNKNOWN-TRACK",
                "url": "https://tracking.example/UNKNOWN-TRACK"
            }
        }),
    ));

    assert_eq!(response.status, 200);
    assert_eq!(
        response.body["data"]["trackingAlias"],
        json!({
            "fulfillment": null,
            "userErrors": [{
                "field": ["fulfillmentId"],
                "message": "Fulfillment does not exist."
            }]
        })
    );
    assert_eq!(log_snapshot(&proxy)["entries"], json!([]));
}

#[test]
fn fulfillment_tracking_update_v2_stages_tracking_and_notify_intent() {
    let mut proxy = snapshot_proxy();
    let (order_id, fulfillment_id) = stage_fulfillment_for_event(&mut proxy);

    let response = proxy.process_request(json_graphql_request(
        r#"
        mutation FulfillmentTrackingInfoUpdateV2Runtime($fulfillmentId: ID!, $trackingInfoInput: FulfillmentTrackingInput!, $notifyCustomer: Boolean) {
          trackingV2: fulfillmentTrackingInfoUpdateV2(
            fulfillmentId: $fulfillmentId
            trackingInfoInput: $trackingInfoInput
            notifyCustomer: $notifyCustomer
          ) {
            fulfillment {
              id
              status
              displayStatus
              trackingInfo { number url company }
            }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "fulfillmentId": fulfillment_id,
            "trackingInfoInput": {
                "company": "UPS",
                "numbers": ["V2-TRACK-1", "V2-TRACK-2"],
                "urls": [
                    "https://tracking.example/V2-TRACK-1",
                    "https://tracking.example/V2-TRACK-2"
                ]
            },
            "notifyCustomer": true
        }),
    ));

    assert_eq!(response.status, 200);
    assert_eq!(response.body["data"]["trackingV2"]["userErrors"], json!([]));
    assert_eq!(
        response.body["data"]["trackingV2"]["fulfillment"]["trackingInfo"],
        json!([
            {
                "number": "V2-TRACK-1",
                "url": "https://tracking.example/V2-TRACK-1",
                "company": "UPS"
            },
            {
                "number": "V2-TRACK-2",
                "url": "https://tracking.example/V2-TRACK-2",
                "company": "UPS"
            }
        ])
    );

    let fulfillment_read = proxy.process_request(json_graphql_request(
        r#"
        query FulfillmentTrackingInfoUpdateV2TopLevelRead($fulfillmentId: ID!) {
          fulfillment(id: $fulfillmentId) {
            id
            trackingInfo { number url company }
          }
        }
        "#,
        json!({ "fulfillmentId": fulfillment_id }),
    ));
    assert_eq!(
        fulfillment_read.body["data"]["fulfillment"]["trackingInfo"],
        response.body["data"]["trackingV2"]["fulfillment"]["trackingInfo"]
    );

    let order_read = proxy.process_request(json_graphql_request(
        r#"
        query FulfillmentTrackingInfoUpdateV2OrderRead($orderId: ID!) {
          order(id: $orderId) {
            fulfillments {
              id
              trackingInfo { number url company }
            }
          }
        }
        "#,
        json!({ "orderId": order_id }),
    ));
    assert_eq!(
        order_read.body["data"]["order"]["fulfillments"][0]["trackingInfo"],
        response.body["data"]["trackingV2"]["fulfillment"]["trackingInfo"]
    );

    let state = state_snapshot(&proxy);
    let order_id_str = order_id.as_str().unwrap();
    assert_eq!(
        state["stagedState"]["orders"][order_id_str]["fulfillments"][0]
            ["__draftProxyNotifyCustomer"],
        json!(true)
    );

    let log = log_snapshot(&proxy);
    let entries = log["entries"].as_array().unwrap();
    assert_eq!(
        entries
            .iter()
            .map(|entry| entry["operationName"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec![
            "orderCreate",
            "fulfillmentCreate",
            "fulfillmentTrackingInfoUpdateV2"
        ]
    );
    assert!(entries[2]["rawBody"]
        .as_str()
        .unwrap()
        .contains("FulfillmentTrackingInfoUpdateV2Runtime"));
}

#[test]
fn fulfillment_tracking_update_v2_returns_not_found_for_unknown_fulfillment_gid() {
    let mut proxy = snapshot_proxy();

    let response = proxy.process_request(json_graphql_request(
        r#"
        mutation FulfillmentTrackingInfoUpdateV2Unknown($fulfillmentId: ID!, $trackingInfoInput: FulfillmentTrackingInput!) {
          trackingV2: fulfillmentTrackingInfoUpdateV2(
            fulfillmentId: $fulfillmentId
            trackingInfoInput: $trackingInfoInput
          ) {
            fulfillment { id }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "fulfillmentId": "gid://shopify/Fulfillment/999999999",
            "trackingInfoInput": {
                "company": "UPS",
                "number": "UNKNOWN-V2",
                "url": "https://tracking.example/UNKNOWN-V2"
            }
        }),
    ));

    assert_eq!(response.status, 200);
    assert_eq!(
        response.body["data"]["trackingV2"],
        json!({
            "fulfillment": null,
            "userErrors": [{
                "field": ["fulfillmentId"],
                "message": "Fulfillment does not exist."
            }]
        })
    );
    assert_eq!(log_snapshot(&proxy)["entries"], json!([]));
}

#[test]
fn fulfillment_cancel_and_tracking_accept_cancelled_or_delivered_fulfillments() {
    let mut proxy = snapshot_proxy();
    let (_order_id, fulfillment_id) = stage_fulfillment_for_event(&mut proxy);

    let cancel = proxy.process_request(json_graphql_request(
        r#"
        mutation CancelBeforeRetry($id: ID!) {
          fulfillmentCancel(id: $id) {
            fulfillment { id status displayStatus }
            userErrors { field message }
          }
        }
        "#,
        json!({ "id": fulfillment_id.clone() }),
    ));
    assert_eq!(
        cancel.body["data"]["fulfillmentCancel"]["userErrors"],
        json!([])
    );

    let cancel_again = proxy.process_request(json_graphql_request(
        r#"
        mutation CancelAlreadyCancelled($id: ID!) {
          fulfillmentCancel(id: $id) {
            fulfillment { id status displayStatus }
            userErrors { field message }
          }
        }
        "#,
        json!({ "id": fulfillment_id.clone() }),
    ));
    assert_eq!(
        cancel_again.body["data"]["fulfillmentCancel"],
        json!({
            "fulfillment": {
                "id": fulfillment_id,
                "status": "CANCELLED",
                "displayStatus": "CANCELED"
            },
            "userErrors": []
        })
    );

    let tracking = proxy.process_request(json_graphql_request(
        r#"
        mutation UpdateCancelledTracking($fulfillmentId: ID!, $trackingInfoInput: FulfillmentTrackingInput!) {
          fulfillmentTrackingInfoUpdate(
            fulfillmentId: $fulfillmentId
            trackingInfoInput: $trackingInfoInput
          ) {
            fulfillment {
              id
              status
              displayStatus
              trackingInfo { number url company }
            }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "fulfillmentId": fulfillment_id,
            "trackingInfoInput": {
                "company": "UPS",
                "number": "CANCELLED-TRACK",
                "url": "https://tracking.example/CANCELLED-TRACK"
            }
        }),
    ));
    assert_eq!(
        tracking.body["data"]["fulfillmentTrackingInfoUpdate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        tracking.body["data"]["fulfillmentTrackingInfoUpdate"]["fulfillment"]["trackingInfo"],
        json!([{
            "number": "CANCELLED-TRACK",
            "url": "https://tracking.example/CANCELLED-TRACK",
            "company": "UPS"
        }])
    );

    let (_delivered_order_id, delivered_fulfillment_id) = stage_fulfillment_for_event(&mut proxy);
    let delivered_event = proxy.process_request(json_graphql_request(
        r#"
        mutation MarkFulfillmentDelivered($fulfillmentEvent: FulfillmentEventInput!) {
          fulfillmentEventCreate(fulfillmentEvent: $fulfillmentEvent) {
            fulfillmentEvent { id status }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "fulfillmentEvent": {
                "fulfillmentId": delivered_fulfillment_id,
                "status": "DELIVERED",
                "message": "Delivered before cancel"
            }
        }),
    ));
    assert_eq!(
        delivered_event.body["data"]["fulfillmentEventCreate"]["userErrors"],
        json!([])
    );

    let cancel_delivered = proxy.process_request(json_graphql_request(
        r#"
        mutation CancelDeliveredFulfillment($id: ID!) {
          fulfillmentCancel(id: $id) {
            fulfillment { id status displayStatus }
            userErrors { field message }
          }
        }
        "#,
        json!({ "id": delivered_fulfillment_id }),
    ));
    assert_eq!(
        cancel_delivered.body["data"]["fulfillmentCancel"]["userErrors"],
        json!([])
    );
    assert_eq!(
        cancel_delivered.body["data"]["fulfillmentCancel"]["fulfillment"]["status"],
        json!("CANCELLED")
    );

    let operation_names = log_snapshot(&proxy)["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["operationName"].as_str().unwrap().to_string())
        .collect::<Vec<_>>();
    assert_eq!(
        operation_names,
        vec![
            "orderCreate",
            "fulfillmentCreate",
            "fulfillmentCancel",
            "fulfillmentCancel",
            "fulfillmentTrackingInfoUpdate",
            "orderCreate",
            "fulfillmentCreate",
            "fulfillmentEventCreate",
            "fulfillmentCancel"
        ]
    );
}

#[test]
fn fulfillment_event_create_accepts_cancelled_parent_and_logs() {
    let mut proxy = snapshot_proxy();
    let (_order_id, fulfillment_id) = stage_fulfillment_for_event(&mut proxy);
    let cancel = proxy.process_request(json_graphql_request(
        r#"
        mutation CancelBeforeEvent($id: ID!) {
          fulfillmentCancel(id: $id) {
            fulfillment { id status displayStatus }
            userErrors { field message }
          }
        }
        "#,
        json!({ "id": fulfillment_id }),
    ));
    assert_eq!(
        cancel.body["data"]["fulfillmentCancel"]["userErrors"],
        json!([])
    );

    let rejected = proxy.process_request(json_graphql_request(
        r#"
        mutation FulfillmentEventCreateCancelled($fulfillmentEvent: FulfillmentEventInput!) {
          fulfillmentEventCreate(fulfillmentEvent: $fulfillmentEvent) {
            fulfillmentEvent { id status }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "fulfillmentEvent": {
                "fulfillmentId": fulfillment_id,
                "status": "DELIVERED",
                "message": "Should not stage"
            }
        }),
    ));

    assert_eq!(
        rejected.body["data"]["fulfillmentEventCreate"]["userErrors"],
        json!([])
    );
    let event = &rejected.body["data"]["fulfillmentEventCreate"]["fulfillmentEvent"];
    assert_eq!(event["status"], json!("DELIVERED"));
    assert!(event["id"]
        .as_str()
        .unwrap()
        .starts_with("gid://shopify/FulfillmentEvent/"));
    let log = log_snapshot(&proxy);
    assert_eq!(log["entries"].as_array().unwrap().len(), 4);
    assert_eq!(
        log["entries"][3]["operationName"],
        json!("fulfillmentEventCreate")
    );
}

#[test]
fn fulfillment_event_create_hydrates_real_fulfillment_without_passthrough_mutation() {
    let fulfillment_id = "gid://shopify/Fulfillment/7878997410098";
    let order_id = "gid://shopify/Order/6100000000000";
    let hydrate_body = json!({
        "data": {
            "fulfillment": {
                "id": fulfillment_id,
                "order": {
                    "id": order_id,
                    "name": "#6100",
                    "email": "hydrated-fulfillment@example.test",
                    "phone": null,
                    "createdAt": "2026-05-05T20:20:00Z",
                    "updatedAt": "2026-05-05T20:20:00Z",
                    "closed": false,
                    "closedAt": null,
                    "cancelledAt": null,
                    "cancelReason": null,
                    "displayFinancialStatus": "PAID",
                    "displayFulfillmentStatus": "FULFILLED",
                    "note": null,
                    "tags": [],
                    "fulfillments": [{
                        "id": fulfillment_id,
                        "status": "SUCCESS",
                        "displayStatus": "FULFILLED",
                        "createdAt": "2026-05-05T20:20:00Z",
                        "updatedAt": "2026-05-05T20:20:00Z",
                        "trackingInfo": []
                    }],
                    "fulfillmentOrders": { "nodes": [] }
                }
            }
        }
    });
    let forwarded = Arc::new(Mutex::new(Vec::<Request>::new()));
    let captured = Arc::clone(&forwarded);
    let mut proxy = configured_proxy(
        ReadMode::LiveHybrid,
        Some(shopify_draft_proxy::proxy::UnsupportedMutationMode::Passthrough),
    )
    .with_upstream_transport(move |request| {
        captured.lock().unwrap().push(request);
        Response {
            status: 200,
            headers: Default::default(),
            body: hydrate_body.clone(),
        }
    });

    let response = proxy.process_request(json_graphql_request(
        r#"
        mutation FulfillmentEventCreateHydrated($fulfillmentEvent: FulfillmentEventInput!) {
          fulfillmentEventCreate(fulfillmentEvent: $fulfillmentEvent) {
            fulfillmentEvent { id status message }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "fulfillmentEvent": {
                "fulfillmentId": fulfillment_id,
                "status": "IN_TRANSIT",
                "message": "Hydrated fulfillment event"
            }
        }),
    ));

    assert_eq!(response.status, 200);
    assert_eq!(
        response.body["data"]["fulfillmentEventCreate"]["userErrors"],
        json!([])
    );
    let forwarded = forwarded.lock().unwrap();
    assert_eq!(forwarded.len(), 1);
    assert!(forwarded[0]
        .body
        .contains("ShippingFulfillmentEventCreateFulfillmentHydrate"));
    assert!(!forwarded[0].body.contains("FulfillmentEventCreateHydrated"));
    let log = log_snapshot(&proxy);
    assert_eq!(
        log["entries"][0]["operationName"],
        json!("fulfillmentEventCreate")
    );
    assert!(log["entries"][0]["rawBody"]
        .as_str()
        .unwrap()
        .contains("FulfillmentEventCreateHydrated"));
}

#[test]
fn fulfillment_event_create_invalid_status_variable_fails_schema_validation() {
    let mut proxy = snapshot_proxy();
    let response = proxy.process_request(json_graphql_request(
        r#"
        mutation FulfillmentEventCreateInvalidStatus($fulfillmentEvent: FulfillmentEventInput!) {
          fulfillmentEventCreate(fulfillmentEvent: $fulfillmentEvent) {
            fulfillmentEvent { id status }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "fulfillmentEvent": {
                "fulfillmentId": "gid://shopify/Fulfillment/1234567890",
                "status": "NOT_A_FULFILLMENT_EVENT_STATUS"
            }
        }),
    ));

    assert_eq!(response.status, 200);
    assert_eq!(
        response.body["errors"][0]["extensions"]["code"],
        json!("INVALID_VARIABLE")
    );
    assert!(response.body["errors"][0]["message"]
        .as_str()
        .unwrap()
        .contains("Expected \"NOT_A_FULFILLMENT_EVENT_STATUS\" to be one of"));
    assert_eq!(log_snapshot(&proxy)["entries"], json!([]));
}

#[test]
fn fulfillment_tracking_update_hydrates_existing_fulfillment() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../fixtures/conformance/very-big-test-store.myshopify.com/2025-01/orders/fulfillment-tracking-info-update-parity.json"
    ))
    .unwrap();
    let hydrate_body = fixture["upstreamCalls"][0]["response"]["body"].clone();
    let forwarded = Arc::new(Mutex::new(Vec::<Request>::new()));
    let captured = Arc::clone(&forwarded);
    let mut proxy = configured_proxy(
        ReadMode::LiveHybrid,
        Some(shopify_draft_proxy::proxy::UnsupportedMutationMode::Passthrough),
    )
    .with_upstream_transport(move |request| {
        captured.lock().unwrap().push(request);
        shopify_draft_proxy::proxy::Response {
            status: 200,
            headers: Default::default(),
            body: hydrate_body.clone(),
        }
    });

    let response = proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/orders/fulfillmentTrackingInfoUpdate-parity-plan.graphql"
        ),
        fixture["variables"].clone(),
    ));

    assert_eq!(response.status, 200);
    assert_eq!(
        response.body["data"]["fulfillmentTrackingInfoUpdate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        response.body["data"]["fulfillmentTrackingInfoUpdate"]["fulfillment"]["trackingInfo"],
        fixture["mutation"]["response"]["data"]["fulfillmentTrackingInfoUpdate"]["fulfillment"]
            ["trackingInfo"]
    );
    // `hydrate_order_for_fulfillment_lifecycle` issues a two-stage upstream hydrate
    // (stage-one fulfillment lookup, then stage-two order read) before staging the
    // tracking update, so the LiveHybrid transport sees two forwarded reads.
    assert_eq!(forwarded.lock().unwrap().len(), 2);
    let log = log_snapshot(&proxy);
    assert_eq!(
        log["entries"][0]["operationName"],
        "fulfillmentTrackingInfoUpdate"
    );
}

#[test]
fn order_create_line_item_fields_and_currency_defaults_are_staged() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/orders/order-create-line-item-fields.json"
    ))
    .unwrap();
    let mut proxy = snapshot_proxy();
    restore_shop_currency(&mut proxy, "USD");

    let create = proxy.process_request(json_graphql_request(
        include_str!("../../config/parity-requests/orders/orderCreate-line-item-fields.graphql"),
        fixture["variables"].clone(),
    ));
    assert_eq!(create.body["data"]["orderCreate"]["userErrors"], json!([]));
    let line = &create.body["data"]["orderCreate"]["order"]["lineItems"]["nodes"][0];
    assert_eq!(
        line["customAttributes"],
        json!([
            { "key": "engraving", "value": "Ada" },
            { "key": "fulfillment_note", "value": "Pack flat" }
        ])
    );
    assert_eq!(line["requiresShipping"], json!(false));
    assert_eq!(line["taxable"], json!(false));
    assert_eq!(line["vendor"], json!("Hermes Vendor"));
    assert_eq!(
        line["product"]["id"],
        fixture["downstreamRead"]["response"]["data"]["order"]["lineItems"]["nodes"][0]["product"]
            ["id"]
    );
    assert_eq!(
        create.body["data"]["orderCreate"]["order"]["currentTotalPriceSet"]["shopMoney"],
        json!({ "amount": "24.0", "currencyCode": "USD" })
    );

    let custom = proxy.process_request(json_graphql_request(
        r#"
        mutation OrderCreateInternalLineFields($order: OrderCreateOrderInput!) {
          orderCreate(order: $order) {
            order {
              currencyCode
              presentmentCurrencyCode
              lineItems(first: 5) {
                nodes {
                  isGiftCard
                  fulfillmentService { handle }
                  fulfillmentStatus
                }
              }
            }
            userErrors { field code message }
          }
        }
        "#,
        json!({
            "order": {
                "email": "line-internal-fields@example.com",
                "lineItems": [{
                    "title": "Internal line fields",
                    "quantity": 1,
                    "priceSet": { "shopMoney": { "amount": "9.00", "currencyCode": "CAD" } },
                    "giftCard": true,
                    "fulfillmentService": "manual",
                    "weight": { "value": 2.5, "unit": "KILOGRAMS" }
                }]
            }
        }),
    ));
    assert_eq!(custom.body["data"]["orderCreate"]["userErrors"], json!([]));
    assert_eq!(
        custom.body["data"]["orderCreate"]["order"]["currencyCode"],
        json!("USD")
    );
    assert_eq!(
        custom.body["data"]["orderCreate"]["order"]["presentmentCurrencyCode"],
        json!("USD")
    );
    let custom_line = &custom.body["data"]["orderCreate"]["order"]["lineItems"]["nodes"][0];
    assert_eq!(custom_line["isGiftCard"], json!(true));
    assert_eq!(
        custom_line["fulfillmentService"],
        json!({ "handle": "manual" })
    );
    assert_eq!(custom_line["fulfillmentStatus"], json!("unfulfilled"));
}

#[test]
fn order_close_and_open_stage_lifecycle_state_and_reads() {
    let mut proxy = snapshot_proxy();

    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation SeedLifecycleOrder($order: OrderCreateOrderInput!) {
          orderCreate(order: $order) {
            order { id closed closedAt updatedAt }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "order": {
                "email": "order-lifecycle@example.com",
                "lineItems": [{
                    "title": "Lifecycle item",
                    "quantity": 1,
                    "priceSet": {
                        "shopMoney": { "amount": "10.00", "currencyCode": "USD" }
                    }
                }]
            }
        }),
    ));
    assert_eq!(create.body["data"]["orderCreate"]["userErrors"], json!([]));
    let order_id = create.body["data"]["orderCreate"]["order"]["id"].clone();
    assert_eq!(
        create.body["data"]["orderCreate"]["order"]["closed"],
        json!(false)
    );
    assert_eq!(
        create.body["data"]["orderCreate"]["order"]["closedAt"],
        Value::Null
    );
    assert_eq!(
        create.body["data"]["orderCreate"]["order"]["updatedAt"],
        json!("2024-01-01T00:00:00.000Z")
    );

    let close = proxy.process_request(json_graphql_request(
        r#"
        mutation ClientNamedClose($input: OrderCloseInput!) {
          closeAlias: orderClose(input: $input) {
            order { id closed closedAt updatedAt }
            userErrors { field message }
          }
        }
        "#,
        json!({ "input": { "id": order_id.clone() } }),
    ));
    assert_eq!(close.status, 200);
    assert_eq!(close.body["data"]["closeAlias"]["userErrors"], json!([]));
    assert_eq!(close.body["data"]["closeAlias"]["order"]["id"], order_id);
    assert_eq!(
        close.body["data"]["closeAlias"]["order"]["closed"],
        json!(true)
    );
    assert_eq!(
        close.body["data"]["closeAlias"]["order"]["closedAt"],
        json!("2024-01-01T00:00:01.000Z")
    );
    assert_eq!(
        close.body["data"]["closeAlias"]["order"]["updatedAt"],
        json!("2024-01-01T00:00:01.000Z")
    );

    let read_closed = proxy.process_request(json_graphql_request(
        r#"
        query ReadLifecycleOrder($id: ID!) {
          order(id: $id) { id closed closedAt updatedAt }
        }
        "#,
        json!({ "id": order_id.clone() }),
    ));
    assert_eq!(
        read_closed.body["data"]["order"],
        close.body["data"]["closeAlias"]["order"]
    );

    let redundant_close = proxy.process_request(json_graphql_request(
        r#"
        mutation RedundantClose($input: OrderCloseInput!) {
          orderClose(input: $input) {
            order { id closed closedAt updatedAt }
            userErrors { field message }
          }
        }
        "#,
        json!({ "input": { "id": order_id.clone() } }),
    ));
    assert_eq!(
        redundant_close.body["data"]["orderClose"]["userErrors"],
        json!([])
    );
    assert_eq!(
        redundant_close.body["data"]["orderClose"]["order"]["closedAt"],
        close.body["data"]["closeAlias"]["order"]["closedAt"]
    );
    assert_eq!(
        redundant_close.body["data"]["orderClose"]["order"]["updatedAt"],
        close.body["data"]["closeAlias"]["order"]["updatedAt"]
    );

    let open = proxy.process_request(json_graphql_request(
        r#"
        mutation ClientNamedOpen($input: OrderOpenInput!) {
          openAlias: orderOpen(input: $input) {
            order { id closed closedAt updatedAt }
            userErrors { field message }
          }
        }
        "#,
        json!({ "input": { "id": order_id.clone() } }),
    ));
    assert_eq!(open.body["data"]["openAlias"]["userErrors"], json!([]));
    assert_eq!(
        open.body["data"]["openAlias"]["order"]["closed"],
        json!(false)
    );
    assert_eq!(
        open.body["data"]["openAlias"]["order"]["closedAt"],
        Value::Null
    );
    assert_eq!(
        open.body["data"]["openAlias"]["order"]["updatedAt"],
        json!("2024-01-01T00:00:02.000Z")
    );

    let redundant_open = proxy.process_request(json_graphql_request(
        r#"
        mutation RedundantOpen($input: OrderOpenInput!) {
          orderOpen(input: $input) {
            order { id closed closedAt updatedAt }
            userErrors { field message }
          }
        }
        "#,
        json!({ "input": { "id": order_id.clone() } }),
    ));
    assert_eq!(
        redundant_open.body["data"]["orderOpen"]["userErrors"],
        json!([])
    );
    assert_eq!(
        redundant_open.body["data"]["orderOpen"]["order"],
        open.body["data"]["openAlias"]["order"]
    );

    let read_open = proxy.process_request(json_graphql_request(
        r#"
        query ReadOpenLifecycleOrder($id: ID!) {
          order(id: $id) { id closed closedAt updatedAt }
        }
        "#,
        json!({ "id": order_id }),
    ));
    assert_eq!(
        read_open.body["data"]["order"],
        open.body["data"]["openAlias"]["order"]
    );

    let log = log_snapshot(&proxy);
    assert_eq!(log["entries"][1]["operationName"], json!("orderClose"));
    assert_eq!(log["entries"][1]["status"], json!("staged"));
    assert!(log["entries"][1]["rawBody"]
        .as_str()
        .is_some_and(|raw| raw.contains("ClientNamedClose")));
    assert_eq!(log["entries"][3]["operationName"], json!("orderOpen"));
    assert!(log["entries"][3]["rawBody"]
        .as_str()
        .is_some_and(|raw| raw.contains("ClientNamedOpen")));
}

#[test]
fn order_close_and_open_unknown_ids_return_shopify_user_errors() {
    let mut proxy = snapshot_proxy();

    let close = proxy.process_request(json_graphql_request(
        r#"
        mutation CloseMissingOrder($input: OrderCloseInput!) {
          orderClose(input: $input) {
            order { id closed closedAt }
            userErrors { field message }
          }
        }
        "#,
        json!({ "input": { "id": "gid://shopify/Order/404" } }),
    ));
    assert_eq!(
        close.body["data"]["orderClose"],
        json!({
            "order": Value::Null,
            "userErrors": [{ "field": ["id"], "message": "Order does not exist" }]
        })
    );

    let open = proxy.process_request(json_graphql_request(
        r#"
        mutation OpenMissingOrder($input: OrderOpenInput!) {
          orderOpen(input: $input) {
            order { id closed closedAt }
            userErrors { field message }
          }
        }
        "#,
        json!({ "input": { "id": "gid://shopify/Order/404" } }),
    ));
    assert_eq!(
        open.body["data"]["orderOpen"],
        json!({
            "order": Value::Null,
            "userErrors": [{ "field": ["id"], "message": "Order does not exist" }]
        })
    );

    let log = log_snapshot(&proxy);
    assert_eq!(log["entries"][0]["status"], json!("failed"));
    assert_eq!(log["entries"][0]["operationName"], json!("orderClose"));
    assert_eq!(log["entries"][1]["status"], json!("failed"));
    assert_eq!(log["entries"][1]["operationName"], json!("orderOpen"));
}

#[test]
fn order_lifecycle_plain_user_errors_reject_code_selection() {
    let mut proxy = snapshot_proxy();

    let response = proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/orders/orderLifecycle-plain-usererror-no-code.graphql"
        ),
        serde_json::from_str(include_str!(
            "../../config/parity-requests/orders/orderLifecycle-plain-usererror-no-code.variables.json"
        ))
        .unwrap(),
    ));

    assert_eq!(response.status, 200);
    assert!(response.body.get("data").is_none());
    let errors = response.body["errors"].as_array().unwrap();
    assert_eq!(errors.len(), 4);
    for (error, response_key) in errors.iter().zip(["update", "close", "open", "markAsPaid"]) {
        assert_eq!(
            error["message"],
            json!("Field 'code' doesn't exist on type 'UserError'")
        );
        assert_eq!(
            error["path"],
            json!([
                "mutation OrderLifecyclePlainUserErrorNoCode",
                response_key,
                "userErrors",
                "code"
            ])
        );
        assert_eq!(
            error["extensions"],
            json!({
                "code": "undefinedField",
                "typeName": "UserError",
                "fieldName": "code"
            })
        );
        assert!(error["locations"][0]["line"].as_u64().is_some());
        assert!(error["locations"][0]["column"].as_u64().is_some());
    }
}

#[test]
fn order_update_and_mark_as_paid_plain_user_errors_omit_codes() {
    let mut proxy = snapshot_proxy();

    let update_unknown = proxy.process_request(json_graphql_request(
        r#"
        mutation UpdateMissingOrder($input: OrderInput!) {
          orderUpdate(input: $input) {
            order { id }
            userErrors { field message }
          }
        }
        "#,
        json!({ "input": { "id": "gid://shopify/Order/999999" } }),
    ));
    assert_eq!(
        update_unknown.body["data"]["orderUpdate"],
        json!({
            "order": Value::Null,
            "userErrors": [{ "field": ["id"], "message": "Order does not exist" }]
        })
    );
    assert!(update_unknown.body["data"]["orderUpdate"]["userErrors"][0]
        .get("code")
        .is_none());

    let unknown_mark = proxy.process_request(json_graphql_request(
        r#"
        mutation UnknownMarkAsPaid($input: OrderMarkAsPaidInput!) {
          orderMarkAsPaid(input: $input) {
            order { id }
            userErrors { field message }
          }
        }
        "#,
        json!({ "input": { "id": "gid://shopify/Order/999999" } }),
    ));
    assert_eq!(
        unknown_mark.body["data"]["orderMarkAsPaid"],
        json!({
            "order": Value::Null,
            "userErrors": [{ "field": ["id"], "message": "Order does not exist" }]
        })
    );
    assert!(
        unknown_mark.body["data"]["orderMarkAsPaid"]["userErrors"][0]
            .get("code")
            .is_none()
    );
}

#[test]
fn order_update_live_hybrid_hydration_miss_does_not_forward_mutation() {
    let upstream_calls = Arc::new(Mutex::new(Vec::new()));
    let mut proxy = configured_proxy(
        ReadMode::LiveHybrid,
        Some(shopify_draft_proxy::proxy::UnsupportedMutationMode::Passthrough),
    )
    .with_upstream_transport({
        let upstream_calls = Arc::clone(&upstream_calls);
        move |request| {
            let body: Value = serde_json::from_str(&request.body).expect("upstream GraphQL body");
            let query = body["query"].as_str().unwrap_or_default().to_string();
            assert!(
                query.trim_start().starts_with("query"),
                "orderUpdate must hydrate by query only, got upstream body: {}",
                request.body
            );
            upstream_calls.lock().expect("upstream calls").push(query);
            Response {
                status: 200,
                headers: Default::default(),
                body: json!({ "data": { "order": Value::Null } }),
            }
        }
    });

    let update = proxy.process_request(json_graphql_request(
        r#"
        mutation LiveHybridOrderUpdateMiss($input: OrderInput!) {
          orderUpdate(input: $input) {
            order { id }
            userErrors { field message }
          }
        }
        "#,
        json!({ "input": { "id": "gid://shopify/Order/404404404" } }),
    ));

    assert_eq!(update.status, 200);
    assert_eq!(
        update.body["data"]["orderUpdate"],
        json!({
            "order": Value::Null,
            "userErrors": [{ "field": ["id"], "message": "Order does not exist" }]
        })
    );
    let calls = upstream_calls.lock().expect("upstream calls");
    assert_eq!(calls.len(), 1);
    assert!(calls[0].contains("query OrdersOrderHydrate"));
}

#[test]
fn order_update_live_hybrid_hydrates_all_order_line_item_pages() {
    let line_items = (1..=12)
        .map(|index| {
            json!({
                "id": format!("gid://shopify/LineItem/{index}"),
                "title": format!("Hydrated line {index}"),
                "name": format!("Hydrated line {index}"),
                "quantity": 1,
                "currentQuantity": 1,
                "sku": format!("HYD-{index:02}"),
                "variantTitle": Value::Null,
                "requiresShipping": true,
                "taxable": true,
                "customAttributes": [],
                "originalUnitPriceSet": { "shopMoney": { "amount": "1.00", "currencyCode": "USD" } },
                "originalTotalSet": { "shopMoney": { "amount": "1.00", "currencyCode": "USD" } },
                "variant": Value::Null,
                "taxLines": []
            })
        })
        .collect::<Vec<_>>();
    let first_page = line_items[..10].to_vec();
    let second_page = line_items[10..].to_vec();
    let upstream_calls = Arc::new(Mutex::new(Vec::new()));
    let mut proxy = configured_proxy(
        ReadMode::LiveHybrid,
        Some(shopify_draft_proxy::proxy::UnsupportedMutationMode::Passthrough),
    )
    .with_upstream_transport({
        let upstream_calls = Arc::clone(&upstream_calls);
        move |request| {
            let body: Value = serde_json::from_str(&request.body).expect("upstream GraphQL body");
            let query = body["query"].as_str().unwrap_or_default();
            assert!(
                query.trim_start().starts_with("query"),
                "orderUpdate must hydrate by query only, got upstream body: {}",
                request.body
            );
            let after = body["variables"]["lineItemsAfter"].as_str();
            let (nodes, has_next_page, end_cursor) = if after == Some("cursor-10") {
                (second_page.clone(), false, "cursor-12")
            } else {
                (first_page.clone(), true, "cursor-10")
            };
            upstream_calls.lock().expect("upstream calls").push(body);
            Response {
                status: 200,
                headers: Default::default(),
                body: json!({
                    "data": {
                        "order": {
                            "id": "gid://shopify/Order/1212121212",
                            "name": "#1212",
                            "email": "many-lines@example.test",
                            "note": "before update",
                            "tags": [],
                            "customAttributes": [],
                            "customer": Value::Null,
                            "billingAddress": Value::Null,
                            "shippingAddress": Value::Null,
                            "currencyCode": "USD",
                            "presentmentCurrencyCode": "USD",
                            "displayFinancialStatus": "PAID",
                            "displayFulfillmentStatus": "UNFULFILLED",
                            "currentTotalPriceSet": { "shopMoney": { "amount": "12.00", "currencyCode": "USD" } },
                            "totalPriceSet": { "shopMoney": { "amount": "12.00", "currencyCode": "USD" } },
                            "totalTaxSet": { "shopMoney": { "amount": "0.00", "currencyCode": "USD" } },
                            "totalDiscountsSet": { "shopMoney": { "amount": "0.00", "currencyCode": "USD" } },
                            "discountCodes": [],
                            "lineItems": {
                                "nodes": nodes,
                                "pageInfo": {
                                    "hasNextPage": has_next_page,
                                    "endCursor": end_cursor
                                }
                            }
                        }
                    }
                }),
            }
        }
    });

    let update = proxy.process_request(json_graphql_request(
        r#"
        mutation UpdateManyLineOrder($input: OrderInput!) {
          orderUpdate(input: $input) {
            order {
              id
              note
              lineItems(first: 20) {
                nodes { id title quantity }
              }
            }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "input": {
                "id": "gid://shopify/Order/1212121212",
                "note": "after unrelated update"
            }
        }),
    ));

    assert_eq!(update.status, 200);
    assert_eq!(update.body["data"]["orderUpdate"]["userErrors"], json!([]));
    assert_eq!(
        update.body["data"]["orderUpdate"]["order"]["note"],
        json!("after unrelated update")
    );
    let hydrated_lines = update.body["data"]["orderUpdate"]["order"]["lineItems"]["nodes"]
        .as_array()
        .expect("hydrated line item nodes");
    assert_eq!(hydrated_lines.len(), 12);
    assert_eq!(hydrated_lines[11]["title"], json!("Hydrated line 12"));

    let calls = upstream_calls.lock().expect("upstream calls");
    assert_eq!(calls.len(), 2);
    assert!(calls.iter().all(|body| body["query"]
        .as_str()
        .unwrap_or_default()
        .trim_start()
        .starts_with("query")));
    assert!(calls[0]["query"]
        .as_str()
        .unwrap_or_default()
        .contains("pageInfo"));
    assert_eq!(calls[1]["variables"]["lineItemsAfter"], json!("cursor-10"));
}

#[test]
fn order_create_validation_matrix_returns_typed_user_errors() {
    let mut proxy = snapshot_proxy();
    let variables: Value = serde_json::from_str(include_str!(
        "../../config/parity-requests/orders/orderCreate-validation-matrix-extended.variables.json"
    ))
    .unwrap();

    let invalid_decimal_literals = proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/orders/orderCreate-validation-matrix-extended.graphql"
        ),
        variables.clone(),
    ));
    assert_eq!(invalid_decimal_literals.body.get("errors"), None);
    assert_eq!(
        invalid_decimal_literals.body["data"]["lineItemTaxLineMissingRate"]["userErrors"],
        json!([{
            "field": ["order", "lineItems", 0, "taxLines", 0, "rate"],
            "code": "TAX_LINE_RATE_MISSING"
        }])
    );
    assert_eq!(
        invalid_decimal_literals.body["data"]["shippingLineTaxLineMissingRate"]["userErrors"],
        json!([{
            "field": ["order", "shippingLines", 0, "taxLines", 0, "rate"],
            "code": "TAX_LINE_RATE_MISSING"
        }])
    );

    let response = proxy.process_request(json_graphql_request(
        r#"
        mutation CurrentOrderCreateValidationMatrix(
          $futureProcessedAt: OrderCreateOrderInput!
          $redundantCustomer: OrderCreateOrderInput!
        ) {
          futureProcessedAt: orderCreate(order: $futureProcessedAt) {
            order { id }
            userErrors { field code }
          }
          redundantCustomer: orderCreate(order: $redundantCustomer) {
            order { id }
            userErrors { field code }
          }
        }
        "#,
        variables,
    ));

    assert_eq!(
        response.body["data"]["futureProcessedAt"]["userErrors"],
        json!([{ "field": ["order", "processedAt"], "code": "PROCESSED_AT_INVALID" }])
    );
    assert_eq!(
        response.body["data"]["redundantCustomer"]["userErrors"],
        json!([{ "field": ["order"], "code": "REDUNDANT_CUSTOMER_FIELDS" }])
    );
    let fulfillment = proxy.process_request(json_graphql_request(
        r#"
        mutation OrderCreateFulfillmentServiceValidation($order: OrderCreateOrderInput!) {
          orderCreate(order: $order) {
            order { id }
            userErrors { field code message }
          }
        }
        "#,
        json!({
            "order": {
                "email": "bad-fulfillment-service@example.com",
                "lineItems": [{
                    "title": "Bad fulfillment service",
                    "quantity": 1,
                    "fulfillmentService": "missing-service"
                }]
            }
        }),
    ));
    assert_eq!(
        fulfillment.body["data"]["orderCreate"]["userErrors"][0]["field"],
        json!(["order", "lineItems", 0, "fulfillmentService"])
    );
    assert_eq!(
        fulfillment.body["data"]["orderCreate"]["userErrors"][0]["code"],
        json!("FULFILLMENT_SERVICE_INVALID")
    );
    assert_eq!(
        fulfillment.body["data"]["orderCreate"]["order"],
        Value::Null
    );
}

#[test]
fn order_cancel_state_transitions_replay_validation_guards() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../fixtures/conformance/local-runtime/2026-04/orders/orderCancel-state-transitions.json"
    ))
    .unwrap();
    let mut proxy = snapshot_proxy();
    let cancel_query = omit_user_error_code_selection(include_str!(
        "../../config/parity-requests/orders/orderCancel-state-transitions.graphql"
    ))
    .replace("    order {\n      id\n    }\n", "");
    let setup_cancel_query = omit_user_error_code_selection(include_str!(
        "../../config/parity-requests/orders/orderCancel-state-transitions-setup-cancel.graphql"
    ));
    let expected_cancel = |key: &str| {
        let mut expected = strip_user_error_codes(&fixture["expected"][key]);
        if let Some(payload) = expected
            .pointer_mut("/data/orderCancel")
            .and_then(Value::as_object_mut)
        {
            payload.remove("order");
        }
        expected
    };

    let fresh = proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/orders/orderCancel-state-transitions-order-create.graphql"
        ),
        fixture["setup"]["freshOrderCreate"]["variables"].clone(),
    ));
    assert_eq!(fresh.body["data"]["orderCreate"]["userErrors"], json!([]));
    let fresh_order_id = fresh.body["data"]["orderCreate"]["order"]["id"].clone();

    let to_cancel = proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/orders/orderCancel-state-transitions-order-create.graphql"
        ),
        fixture["setup"]["cancelledOrderCreate"]["variables"].clone(),
    ));
    assert_eq!(
        to_cancel.body["data"]["orderCreate"]["userErrors"],
        json!([])
    );
    let cancelled_id = to_cancel.body["data"]["orderCreate"]["order"]["id"].clone();

    let setup_cancel = proxy.process_request(json_graphql_request(
        &setup_cancel_query,
        json!({ "orderId": cancelled_id.clone(), "restock": false, "reason": "OTHER" }),
    ));
    assert_eq!(setup_cancel.body, expected_cancel("cancelOrderSuccess"));

    let already_cancelled = proxy.process_request(json_graphql_request(
        &cancel_query,
        json!({ "orderId": cancelled_id, "restock": false, "reason": "OTHER" }),
    ));
    assert_eq!(already_cancelled.body, expected_cancel("alreadyCancelled"));

    let staff_note_too_long = proxy.process_request(json_graphql_request(
        &cancel_query,
        json!({
            "orderId": fresh_order_id.clone(),
            "restock": false,
            "reason": "OTHER",
            "staffNote": "x".repeat(300)
        }),
    ));
    assert_eq!(
        staff_note_too_long.body,
        expected_cancel("staffNoteTooLong")
    );

    let refund_conflict = proxy.process_request(json_graphql_request(
        &cancel_query,
        json!({
            "orderId": fresh_order_id.clone(),
            "restock": false,
            "reason": "OTHER",
            "refund": true,
            "refundMethod": { "originalPaymentMethodsRefund": true }
        }),
    ));
    assert_eq!(
        refund_conflict.body,
        expected_cancel("refundAndRefundMethodConflict")
    );

    let refund_false_conflict = proxy.process_request(json_graphql_request(
        &cancel_query,
        json!({
            "orderId": fresh_order_id,
            "restock": false,
            "reason": "OTHER",
            "refund": false,
            "refundMethod": { "originalPaymentMethodsRefund": true }
        }),
    ));
    assert_eq!(
        refund_false_conflict.body,
        expected_cancel("refundFalseAndRefundMethodConflict")
    );
    assert_eq!(log_snapshot(&proxy)["entries"].as_array().unwrap().len(), 3);

    let unknown_order = proxy.process_request(json_graphql_request(
        &cancel_query,
        json!({ "orderId": "gid://shopify/Order/404", "restock": false, "reason": "OTHER" }),
    ));
    assert_eq!(unknown_order.body, expected_cancel("unknownOrder"));
}

#[test]
fn order_cancel_staged_order_create_chain_updates_downstream_state() {
    let upstream_calls = Arc::new(Mutex::new(0usize));
    let upstream_calls_for_transport = Arc::clone(&upstream_calls);
    let mut proxy = configured_proxy(
        ReadMode::LiveHybrid,
        Some(shopify_draft_proxy::proxy::UnsupportedMutationMode::Passthrough),
    )
    .with_upstream_transport(move |_request| {
        *upstream_calls_for_transport.lock().unwrap() += 1;
        Response {
            status: 599,
            headers: Default::default(),
            body: json!({ "errors": [{ "message": "unexpected upstream call" }] }),
        }
    });

    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateOrderForCancel($order: OrderCreateOrderInput!) {
          orderCreate(order: $order) {
            order { id email }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "order": {
                "email": "normal-cancel@example.com",
                "currency": "USD",
                "lineItems": [{
                    "title": "Normal cancel item",
                    "quantity": 1,
                    "priceSet": { "shopMoney": { "amount": "5.00", "currencyCode": "USD" } }
                }]
            }
        }),
    ));
    assert_eq!(create.status, 200);
    assert_eq!(create.body["data"]["orderCreate"]["userErrors"], json!([]));
    let order_id = create.body["data"]["orderCreate"]["order"]["id"].clone();
    assert_eq!(order_id, json!("gid://shopify/Order/1"));

    let cancel_query = r#"
        mutation CancelStagedOrder(
          $orderId: ID!
          $reason: OrderCancelReason!
          $refund: Boolean!
          $restock: Boolean!
          $notifyCustomer: Boolean!
        ) {
          orderCancel(
            orderId: $orderId
            reason: $reason
            refund: $refund
            restock: $restock
            notifyCustomer: $notifyCustomer
          ) {
            job { id done }
            userErrors { field message  }
            orderCancelUserErrors { field message code }
          }
        }
        "#;
    let cancel = proxy.process_request(json_graphql_request(
        cancel_query,
        json!({
            "orderId": order_id.clone(),
            "reason": "CUSTOMER",
            "refund": false,
            "restock": false,
            "notifyCustomer": false
        }),
    ));
    assert_eq!(cancel.status, 200);
    let cancel_payload = &cancel.body["data"]["orderCancel"];
    assert_eq!(cancel_payload["userErrors"], json!([]));
    assert_eq!(cancel_payload["orderCancelUserErrors"], json!([]));
    assert_eq!(cancel_payload["job"]["done"], json!(false));

    let read = proxy.process_request(json_graphql_request(
        r#"
        query ReadCancelledOrder($id: ID!) {
          order(id: $id) {
            id
            email
            closed
            closedAt
            cancelledAt
            cancelReason
          }
        }
        "#,
        json!({ "id": order_id.clone() }),
    ));
    let cancelled_at = read.body["data"]["order"]["cancelledAt"]
        .as_str()
        .expect("cancelledAt should be selected");
    let closed_at = read.body["data"]["order"]["closedAt"]
        .as_str()
        .expect("closedAt should be selected");
    assert!(cancelled_at.starts_with("2024-01-01T00:00:"));
    assert_eq!(closed_at, cancelled_at);
    assert_eq!(
        read.body["data"]["order"],
        json!({
            "id": order_id,
            "email": "normal-cancel@example.com",
            "closed": true,
            "closedAt": closed_at,
            "cancelledAt": cancelled_at,
            "cancelReason": "CUSTOMER"
        })
    );

    let already_cancelled = proxy.process_request(json_graphql_request(
        cancel_query,
        json!({
            "orderId": read.body["data"]["order"]["id"].clone(),
            "reason": "CUSTOMER",
            "refund": false,
            "restock": false,
            "notifyCustomer": false
        }),
    ));
    assert_eq!(
        already_cancelled.body["data"]["orderCancel"]["userErrors"],
        json!([{ "field": ["orderId"], "message": "Cannot cancel an order that has already been canceled" }])
    );

    let log = log_snapshot(&proxy);
    assert_eq!(log["entries"][0]["operationName"], json!("orderCreate"));
    assert_eq!(log["entries"][1]["operationName"], json!("orderCancel"));
    assert_eq!(
        log["entries"][1]["stagedResourceIds"],
        json!(["gid://shopify/Order/1"])
    );
    assert!(log["entries"][1]["rawBody"]
        .as_str()
        .unwrap_or_default()
        .contains("CancelStagedOrder"));
    assert_eq!(*upstream_calls.lock().unwrap(), 0);
}

#[test]
fn order_customer_set_and_remove_error_paths_use_staged_records() {
    let mut proxy = snapshot_proxy();
    let cancel_query = omit_user_error_code_selection(include_str!(
        "../../config/parity-requests/orders/orderCancel-state-transitions.graphql"
    ))
    .replace("    order {\n      id\n    }\n", "");

    let customer = proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/orders/orderCustomer-error-paths-customer-create.graphql"
        ),
        json!({ "input": {
            "email": "customer-set-varied@example.test",
            "firstName": "Customer",
            "lastName": "Varied"
        }}),
    ));
    assert_eq!(
        customer.body["data"]["customerCreate"]["userErrors"],
        json!([])
    );
    let customer_id = customer.body["data"]["customerCreate"]["customer"]["id"].clone();

    let order = proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/orders/orderCancel-state-transitions-order-create.graphql"
        ),
        json!({ "order": {
            "currency": "USD",
            "financialStatus": "PENDING",
            "email": "customer-set-order@example.test",
            "lineItems": [{
                "title": "Order customer item",
                "quantity": 1,
                "priceSet": { "shopMoney": { "amount": "10.00", "currencyCode": "USD" } }
            }]
        }}),
    ));
    assert_eq!(order.body["data"]["orderCreate"]["userErrors"], json!([]));
    let order_id = order.body["data"]["orderCreate"]["order"]["id"].clone();

    let happy_set = proxy.process_request(json_graphql_request(
        include_str!("../../config/parity-requests/orders/orderCustomerSet-error-paths.graphql"),
        json!({ "orderId": order_id.clone(), "customerId": customer_id.clone() }),
    ));
    assert_eq!(
        happy_set.body,
        json!({
            "data": {
                "orderCustomerSet": {
                    "order": {
                        "id": order_id,
                        "customer": {
                            "id": customer_id,
                            "email": "customer-set-varied@example.test",
                            "displayName": "Customer Varied"
                        }
                    },
                    "userErrors": []
                }
            }
        })
    );

    let happy_remove = proxy.process_request(json_graphql_request(
        include_str!("../../config/parity-requests/orders/orderCustomerRemove-error-paths.graphql"),
        json!({ "orderId": order_id.clone() }),
    ));
    assert_eq!(
        happy_remove.body,
        json!({
            "data": {
                "orderCustomerRemove": {
                    "order": {
                        "id": order_id,
                        "customer": Value::Null
                    },
                    "userErrors": []
                }
            }
        })
    );

    let unknown_order = proxy.process_request(json_graphql_request(
        include_str!("../../config/parity-requests/orders/orderCustomerSet-error-paths.graphql"),
        json!({ "orderId": "gid://shopify/Order/order-customer-missing", "customerId": customer_id.clone() }),
    ));
    assert_eq!(
        unknown_order.body,
        json!({
            "data": {
                "orderCustomerSet": {
                    "order": Value::Null,
                    "userErrors": [{
                        "field": ["orderId"],
                        "message": "Order does not exist",
                        "code": "NOT_FOUND"
                    }]
                }
            }
        })
    );

    let unknown_customer = proxy.process_request(json_graphql_request(
        include_str!("../../config/parity-requests/orders/orderCustomerSet-error-paths.graphql"),
        json!({ "orderId": order_id, "customerId": "gid://shopify/Customer/order-customer-missing" }),
    ));
    assert_eq!(
        unknown_customer.body,
        json!({
            "data": {
                "orderCustomerSet": {
                    "order": Value::Null,
                    "userErrors": [{
                        "field": ["customerId"],
                        "message": "Customer does not exist",
                        "code": "NOT_FOUND"
                    }]
                }
            }
        })
    );

    let company = proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/orders/orderCustomer-error-paths-company-create.graphql"
        ),
        json!({ "input": { "company": { "name": "Customer Paths Company" } } }),
    ));
    assert_eq!(
        company.body["data"]["companyCreate"]["userErrors"],
        json!([])
    );
    let company_id = company.body["data"]["companyCreate"]["company"]["id"].clone();
    let company_read = proxy.process_request(json_graphql_request(
        r#"
        query ReadOrderCustomerCompanyLocation($id: ID!) {
          company(id: $id) {
            locations(first: 1) { nodes { id } }
          }
        }
        "#,
        json!({ "id": company_id.clone() }),
    ));
    let company_location_id =
        company_read.body["data"]["company"]["locations"]["nodes"][0]["id"].clone();

    let assign = proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/b2b/b2b-company-contact-main-delete-assign-customer.graphql"
        ),
        json!({ "companyId": company_id, "customerId": customer_id.clone() }),
    ));
    assert_eq!(
        assign.body["data"]["companyAssignCustomerAsContact"]["userErrors"],
        json!([])
    );

    let b2b_order = proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/orders/orderCancel-state-transitions-order-create.graphql"
        ),
        json!({ "order": {
            "currency": "USD",
            "financialStatus": "PENDING",
            "email": "customer-paths-b2b@example.test",
            "companyLocationId": company_location_id,
            "lineItems": [{
                "title": "B2B order customer item",
                "quantity": 1,
                "priceSet": { "shopMoney": { "amount": "10.00", "currencyCode": "USD" } }
            }]
        }}),
    ));
    let b2b_order_id = b2b_order.body["data"]["orderCreate"]["order"]["id"].clone();
    let b2b_not_permitted = proxy.process_request(json_graphql_request(
        include_str!("../../config/parity-requests/orders/orderCustomerSet-error-paths.graphql"),
        json!({ "orderId": b2b_order_id.clone(), "customerId": customer_id.clone() }),
    ));
    assert_eq!(
        b2b_not_permitted.body,
        json!({
            "data": {
                "orderCustomerSet": {
                    "order": Value::Null,
                    "userErrors": [{
                        "field": ["customerId"],
                        "message": "Customer does not have the permissions to place this order",
                        "code": "NOT_PERMITTED"
                    }]
                }
            }
        })
    );

    let b2b_remove = proxy.process_request(json_graphql_request(
        include_str!("../../config/parity-requests/orders/orderCustomerRemove-error-paths.graphql"),
        json!({ "orderId": b2b_order_id }),
    ));
    assert_eq!(
        b2b_remove.body,
        json!({
            "data": {
                "orderCustomerRemove": {
                    "order": Value::Null,
                    "userErrors": [{
                        "field": ["orderId"],
                        "message": "Action not permitted on B2B Orders",
                        "code": "INVALID"
                    }]
                }
            }
        })
    );

    let cancelled_order = proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/orders/orderCancel-state-transitions-order-create.graphql"
        ),
        json!({
            "order": {
                "currency": "USD",
                "financialStatus": "PENDING",
                "email": "customer-paths-cancelled@example.test",
                "customerId": customer_id,
                "lineItems": [{
                    "title": "Cancelled order customer item",
                    "quantity": 1,
                    "priceSet": { "shopMoney": { "amount": "10.00", "currencyCode": "USD" } }
                }]
            }
        }),
    ));
    let cancelled_order_id = cancelled_order.body["data"]["orderCreate"]["order"]["id"].clone();
    let cancel = proxy.process_request(json_graphql_request(
        &cancel_query,
        json!({ "orderId": cancelled_order_id.clone(), "restock": false, "reason": "OTHER" }),
    ));
    assert_eq!(cancel.body["data"]["orderCancel"]["userErrors"], json!([]));

    let cancelled_remove = proxy.process_request(json_graphql_request(
        include_str!("../../config/parity-requests/orders/orderCustomerRemove-error-paths.graphql"),
        json!({ "orderId": cancelled_order_id.clone() }),
    ));
    assert_eq!(
        cancelled_remove.body,
        json!({
            "data": {
                "orderCustomerRemove": {
                    "order": {
                        "id": cancelled_order_id,
                        "customer": Value::Null
                    },
                    "userErrors": []
                }
            }
        })
    );
}

#[test]
fn order_customer_b2b_paths_use_input_state_not_fixture_strings() {
    let mut proxy = snapshot_proxy();

    let customer_query = r#"
        mutation CreateVariedCustomer($input: CustomerInput!) {
          customerCreate(input: $input) {
            customer { id email displayName }
            userErrors { field message  }
          }
        }
    "#;
    let first_customer = proxy.process_request(json_graphql_request(
        customer_query,
        json!({ "input": {
            "email": "buyer.alpha@example.test",
            "firstName": "Avery",
            "lastName": "Atlas"
        }}),
    ));
    let second_customer = proxy.process_request(json_graphql_request(
        customer_query,
        json!({ "input": {
            "email": "buyer.beta@example.test",
            "firstName": "Blair",
            "lastName": "Benton"
        }}),
    ));
    assert_eq!(
        first_customer.body["data"]["customerCreate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        second_customer.body["data"]["customerCreate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        first_customer.body["data"]["customerCreate"]["customer"]["displayName"],
        json!("Avery Atlas")
    );
    let first_customer_id = first_customer.body["data"]["customerCreate"]["customer"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let second_customer_id = second_customer.body["data"]["customerCreate"]["customer"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_ne!(first_customer_id, second_customer_id);

    let company_query = r#"
        mutation CreateVariedCompany($input: CompanyCreateInput!) {
          companyCreate(input: $input) {
            company { id name locations(first: 1) { nodes { id } } }
            userErrors { field message code }
          }
        }
    "#;
    let first_company = proxy.process_request(json_graphql_request(
        company_query,
        json!({ "input": { "company": { "name": "Atlas Procurement LLC" } } }),
    ));
    let second_company = proxy.process_request(json_graphql_request(
        company_query,
        json!({ "input": { "company": { "name": "Blue River Wholesale" } } }),
    ));
    assert_eq!(
        first_company.body["data"]["companyCreate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        first_company.body["data"]["companyCreate"]["company"]["name"],
        json!("Atlas Procurement LLC")
    );
    let first_company_id = first_company.body["data"]["companyCreate"]["company"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let second_company_id = second_company.body["data"]["companyCreate"]["company"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let first_company_location_id = first_company.body["data"]["companyCreate"]["company"]
        ["locations"]["nodes"][0]["id"]
        .clone();
    let second_company_location_id = second_company.body["data"]["companyCreate"]["company"]
        ["locations"]["nodes"][0]["id"]
        .clone();
    assert_ne!(first_company_id, second_company_id);

    let assign_query = r#"
        mutation AssignVariedCustomer($companyId: ID!, $customerId: ID!) {
          companyAssignCustomerAsContact(companyId: $companyId, customerId: $customerId) {
            companyContact {
              id
              isMainContact
              customer { id }
              company { id name }
            }
            userErrors { field message code }
          }
        }
    "#;
    let first_contact = proxy.process_request(json_graphql_request(
        assign_query,
        json!({ "companyId": first_company_id, "customerId": first_customer_id }),
    ));
    let second_contact = proxy.process_request(json_graphql_request(
        assign_query,
        json!({ "companyId": second_company_id, "customerId": second_customer_id }),
    ));
    assert_eq!(
        first_contact.body["data"]["companyAssignCustomerAsContact"]["userErrors"],
        json!([])
    );
    assert_eq!(
        first_contact.body["data"]["companyAssignCustomerAsContact"]["companyContact"]["company"]
            ["name"],
        json!("Atlas Procurement LLC")
    );
    assert_ne!(
        first_contact.body["data"]["companyAssignCustomerAsContact"]["companyContact"]["id"],
        second_contact.body["data"]["companyAssignCustomerAsContact"]["companyContact"]["id"]
    );

    let order_query = r#"
        mutation CreateVariedB2bOrder($order: OrderCreateOrderInput!) {
          orderCreate(order: $order) {
            order { id }
            userErrors { field message code }
          }
        }
    "#;
    let first_order = proxy.process_request(json_graphql_request(
        order_query,
        json!({ "order": {
            "email": "purchase.alpha@example.test",
            "currency": "USD",
            "financialStatus": "PENDING",
            "companyLocationId": first_company_location_id,
            "lineItems": [{
                "title": "B2B varied item",
                "quantity": 1,
                "priceSet": { "shopMoney": { "amount": "10.00", "currencyCode": "USD" } }
            }]
        }}),
    ));
    let second_order = proxy.process_request(json_graphql_request(
        order_query,
        json!({ "order": {
            "email": "purchase.beta@example.test",
            "currency": "USD",
            "financialStatus": "PENDING",
            "companyLocationId": second_company_location_id,
            "lineItems": [{
                "title": "B2B varied item two",
                "quantity": 1,
                "priceSet": { "shopMoney": { "amount": "11.00", "currencyCode": "USD" } }
            }]
        }}),
    ));
    assert_eq!(
        first_order.body["data"]["orderCreate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        second_order.body["data"]["orderCreate"]["userErrors"],
        json!([])
    );
    let first_order_id = first_order.body["data"]["orderCreate"]["order"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let second_order_id = second_order.body["data"]["orderCreate"]["order"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_ne!(first_order_id, second_order_id);

    let set = proxy.process_request(json_graphql_request(
        include_str!("../../config/parity-requests/orders/orderCustomerSet-error-paths.graphql"),
        json!({ "orderId": first_order_id, "customerId": first_customer_id }),
    ));
    assert_eq!(
        set.body["data"]["orderCustomerSet"]["userErrors"],
        json!([{
            "field": ["customerId"],
            "message": "Customer does not have the permissions to place this order",
            "code": "NOT_PERMITTED"
        }])
    );

    let cancel_query = r#"
        mutation CancelVariedOrder($orderId: ID!) {
          orderCancel(orderId: $orderId, restock: false, reason: OTHER) {
            job { id done }
            orderCancelUserErrors { field message code }
            userErrors { field message  }
          }
        }
    "#;
    let first_cancel = proxy.process_request(json_graphql_request(
        cancel_query,
        json!({ "orderId": first_order_id }),
    ));
    let second_cancel = proxy.process_request(json_graphql_request(
        cancel_query,
        json!({ "orderId": second_order_id }),
    ));
    assert_eq!(
        first_cancel.body["data"]["orderCancel"]["userErrors"],
        json!([])
    );
    assert_eq!(
        second_cancel.body["data"]["orderCancel"]["userErrors"],
        json!([])
    );
    assert_ne!(
        first_cancel.body["data"]["orderCancel"]["job"]["id"],
        second_cancel.body["data"]["orderCancel"]["job"]["id"]
    );
}

#[test]
fn draft_order_bulk_tags_validation_replays_captured_stateful_shapes() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../fixtures/conformance/local-runtime/2026-04/orders/draft-order-bulk-tag-validation.json"
    ))
    .unwrap();
    let mut proxy = snapshot_proxy();
    let add_query = omit_user_error_code_selection(include_str!(
        "../../config/parity-requests/orders/draftOrderBulkTag-validation-add.graphql"
    ));
    let remove_query = omit_user_error_code_selection(include_str!(
        "../../config/parity-requests/orders/draftOrderBulkTag-validation-remove.graphql"
    ));

    let create = proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/orders/draftOrderBulkTag-validation-create.graphql"
        ),
        fixture["setup"]["simpleDraftOrderCreate"]["variables"].clone(),
    ));
    assert_eq!(
        create.body["data"]["draftOrderCreate"]["userErrors"],
        json!([])
    );
    let draft_order_id = create.body["data"]["draftOrderCreate"]["draftOrder"]["id"].clone();
    let expected_read = |key: &str, id: &Value| {
        let mut expected = fixture["expected"][key].clone();
        expected["data"]["draftOrder"]["id"] = id.clone();
        expected
    };

    let partial_add = proxy.process_request(json_graphql_request(
        &add_query,
        json!({
            "ids": [draft_order_id.clone(), "gid://shopify/DraftOrder/draft-order-bulk-tag-missing"],
            "tags": [" added ", "ADDED"]
        }),
    ));
    assert_eq!(
        partial_add.body,
        strip_user_error_codes(&fixture["expected"]["partialSuccessWithUnknownId"])
    );

    let read_after_partial = proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/orders/draftOrderBulkTag-validation-read.graphql"
        ),
        json!({ "id": draft_order_id.clone() }),
    ));
    assert_eq!(
        read_after_partial.body,
        expected_read("readAfterPartialSuccess", &draft_order_id)
    );

    let long_tag = proxy.process_request(json_graphql_request(
        &add_query,
        json!({ "ids": [draft_order_id.clone()], "tags": [fixture["inputs"]["longTag"].clone()] }),
    ));
    assert_eq!(
        long_tag.body,
        strip_user_error_codes(&fixture["expected"]["longTagRejected"])
    );

    let remove = proxy.process_request(json_graphql_request(
        &remove_query,
        json!({ "ids": [draft_order_id.clone()], "tags": [" INITIAL "] }),
    ));
    assert_eq!(
        remove.body,
        strip_user_error_codes(&fixture["expected"]["removeNormalizesTagIdentity"])
    );

    let read_after_remove = proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/orders/draftOrderBulkTag-validation-read.graphql"
        ),
        json!({ "id": draft_order_id.clone() }),
    ));
    assert_eq!(
        read_after_remove.body,
        expected_read("readAfterNormalizedRemove", &draft_order_id)
    );

    let too_many = proxy.process_request(json_graphql_request(
        &add_query,
        json!({ "ids": [draft_order_id], "tags": fixture["inputs"]["tooManyTags"].clone() }),
    ));
    assert_eq!(
        too_many.body,
        strip_user_error_codes(&fixture["expected"]["tooManyInputTags"])
    );
}

#[test]
fn draft_order_bulk_add_tags_preserves_display_case_and_dedupes_by_identity() {
    let mut proxy = snapshot_proxy();

    let create = proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/orders/draftOrderBulkTag-validation-create.graphql"
        ),
        json!({
            "input": {
                "email": "draft-order-bulk-tag-case-preservation@example.com",
                "tags": ["VIP"],
                "lineItems": [{
                    "title": "Case preserving bulk tag item",
                    "quantity": 1,
                    "originalUnitPrice": "2.00"
                }]
            }
        }),
    ));
    assert_eq!(
        create.body["data"]["draftOrderCreate"]["userErrors"],
        json!([])
    );
    let draft_order_id = create.body["data"]["draftOrderCreate"]["draftOrder"]["id"].clone();

    let add_variables = json!({
        "ids": [draft_order_id.clone()],
        "tags": [" vip ", " Wholesale ", "wholesale"]
    });
    let add = proxy.process_request(json_graphql_request(
        &omit_user_error_code_selection(include_str!(
            "../../config/parity-requests/orders/draftOrderBulkTag-validation-add.graphql"
        )),
        add_variables.clone(),
    ));
    assert_eq!(
        add.body["data"]["draftOrderBulkAddTags"]["userErrors"],
        json!([])
    );
    let log = log_snapshot(&proxy);
    let add_entry = log["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["interpreted"]["primaryRootField"] == json!("draftOrderBulkAddTags"))
        .unwrap_or_else(|| {
            panic!("draftOrderBulkAddTags should stage locally in the mutation log: {log:?}")
        });
    assert_eq!(add_entry["status"], json!("staged"));
    assert_eq!(add_entry["variables"], add_variables);

    let read_after_add = proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/orders/draftOrderBulkTag-validation-read.graphql"
        ),
        json!({ "id": draft_order_id.clone() }),
    ));
    assert_eq!(
        read_after_add.body["data"]["draftOrder"]["tags"],
        json!(["VIP", "Wholesale"])
    );

    let remove = proxy.process_request(json_graphql_request(
        &omit_user_error_code_selection(include_str!(
            "../../config/parity-requests/orders/draftOrderBulkTag-validation-remove.graphql"
        )),
        json!({ "ids": [draft_order_id.clone()], "tags": ["vip"] }),
    ));
    assert_eq!(
        remove.body["data"]["draftOrderBulkRemoveTags"]["userErrors"],
        json!([])
    );

    let read_after_remove = proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/orders/draftOrderBulkTag-validation-read.graphql"
        ),
        json!({ "id": draft_order_id }),
    ));
    assert_eq!(
        read_after_remove.body["data"]["draftOrder"]["tags"],
        json!(["Wholesale"])
    );
}

#[test]
fn draft_order_lifecycle_family_stages_and_reads_from_store() {
    let mut proxy = snapshot_proxy();
    restore_shop_currency(&mut proxy, "CAD");

    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateDraft($input: DraftOrderInput!) {
          draftOrderCreate(input: $input) {
            draftOrder {
              id
              name
              status
              ready
              email
              tags
              invoiceUrl
              subtotalPriceSet { shopMoney { amount currencyCode } }
              totalShippingPriceSet { shopMoney { amount currencyCode } }
              totalPriceSet { shopMoney { amount currencyCode } }
              totalQuantityOfLineItems
              lineItems(first: 5) {
                nodes {
                  id
                  title
                  name
                  quantity
                  sku
                  custom
                  requiresShipping
                  taxable
                  originalUnitPriceSet { shopMoney { amount currencyCode } }
                  originalTotalSet { shopMoney { amount currencyCode } }
                  discountedTotalSet { shopMoney { amount currencyCode } }
                  totalDiscountSet { shopMoney { amount currencyCode } }
                }
              }
            }
            userErrors { field message  }
          }
        }
        "#,
        json!({
            "input": {
                "email": "ordinary-draft@example.test",
                "tags": ["initial", "draft"],
                "shippingLine": {
                    "title": "Courier",
                    "priceWithCurrency": { "amount": "4.25", "currencyCode": "CAD" }
                },
                "lineItems": [{
                    "title": "Ordinary staged item",
                    "quantity": 2,
                    "originalUnitPrice": "12.50",
                    "sku": "ORD-STAGED",
                    "requiresShipping": false,
                    "taxable": false
                }]
            }
        }),
    ));

    assert_eq!(create.status, 200);
    assert_eq!(
        create.body["data"]["draftOrderCreate"]["userErrors"],
        json!([])
    );
    let draft = &create.body["data"]["draftOrderCreate"]["draftOrder"];
    let draft_id = draft["id"].clone();
    assert_ne!(
        draft["id"],
        json!("gid://shopify/DraftOrder/1?shopify-draft-proxy=synthetic")
    );
    assert_eq!(draft["email"], json!("ordinary-draft@example.test"));
    assert_eq!(draft["tags"], json!(["draft", "initial"]));
    assert_eq!(draft["status"], json!("OPEN"));
    assert_eq!(draft["ready"], json!(true));
    assert_eq!(draft["totalQuantityOfLineItems"], json!(2));
    assert_eq!(
        draft["lineItems"]["nodes"][0]["title"],
        json!("Ordinary staged item")
    );
    assert_eq!(
        draft["subtotalPriceSet"]["shopMoney"],
        json!({ "amount": "25.0", "currencyCode": "CAD" })
    );
    assert_eq!(
        draft["totalPriceSet"]["shopMoney"],
        json!({ "amount": "29.25", "currencyCode": "CAD" })
    );

    let read = proxy.process_request(json_graphql_request(
        r#"
        query ReadDraft($id: ID!) {
          byId: draftOrder(id: $id) { id email tags totalPriceSet { shopMoney { amount currencyCode } } }
          draftOrders(first: 5) { nodes { id email tags } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } }
          draftOrdersCount { count precision }
        }
        "#,
        json!({ "id": draft_id.clone() }),
    ));
    assert_eq!(read.body["data"]["byId"]["id"], draft_id);
    assert_eq!(
        read.body["data"]["draftOrders"]["nodes"][0]["email"],
        json!("ordinary-draft@example.test")
    );
    assert_eq!(
        read.body["data"]["draftOrdersCount"],
        json!({ "count": 1, "precision": "EXACT" })
    );

    let update = proxy.process_request(json_graphql_request(
        r#"
        mutation UpdateDraft($id: ID!, $input: DraftOrderInput!) {
          draftOrderUpdate(id: $id, input: $input) {
            draftOrder {
              id
              email
              tags
              shippingLine { title originalPriceSet { shopMoney { amount currencyCode } } }
              totalPriceSet { shopMoney { amount currencyCode } }
            }
            userErrors { field message  }
          }
        }
        "#,
        json!({
            "id": draft_id.clone(),
            "input": {
                "email": "updated-draft@example.test",
                "tags": ["updated"],
                "shippingLine": {
                    "title": "Express",
                    "priceWithCurrency": { "amount": "6.00", "currencyCode": "CAD" }
                }
            }
        }),
    ));
    assert_eq!(
        update.body["data"]["draftOrderUpdate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        update.body["data"]["draftOrderUpdate"]["draftOrder"]["email"],
        json!("updated-draft@example.test")
    );
    assert_eq!(
        update.body["data"]["draftOrderUpdate"]["draftOrder"]["tags"],
        json!(["updated"])
    );
    assert_eq!(
        update.body["data"]["draftOrderUpdate"]["draftOrder"]["shippingLine"]["title"],
        json!("Express")
    );
    assert_eq!(
        update.body["data"]["draftOrderUpdate"]["draftOrder"]["totalPriceSet"]["shopMoney"]
            ["amount"],
        json!("31.0")
    );

    let calculate = proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/orders/draft-order-residual-helper-calculate.graphql"
        ),
        json!({
            "input": {
                "lineItems": [{
                    "title": "Calculated item",
                    "quantity": 3,
                    "originalUnitPrice": "2.50",
                    "requiresShipping": false
                }]
            }
        }),
    ));
    assert_eq!(
        calculate.body["data"]["draftOrderCalculate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        calculate.body["data"]["draftOrderCalculate"]["calculatedDraftOrder"]["totalPriceSet"]
            ["shopMoney"],
        json!({ "amount": "7.5", "currencyCode": "CAD" })
    );

    let duplicate = proxy.process_request(json_graphql_request(
        r#"
        mutation DuplicateDraft($id: ID!) {
          draftOrderDuplicate(id: $id) {
            draftOrder { id name status ready email tags totalPriceSet { shopMoney { amount currencyCode } } }
            userErrors { field message  }
          }
        }
        "#,
        json!({ "id": draft_id.clone() }),
    ));
    assert_eq!(
        duplicate.body["data"]["draftOrderDuplicate"]["userErrors"],
        json!([])
    );
    let duplicated_id = duplicate.body["data"]["draftOrderDuplicate"]["draftOrder"]["id"].clone();
    assert_ne!(duplicated_id, draft_id);
    assert_eq!(
        duplicate.body["data"]["draftOrderDuplicate"]["draftOrder"]["status"],
        json!("OPEN")
    );

    let delete = proxy.process_request(json_graphql_request(
        include_str!("../../config/parity-requests/orders/draftOrderDelete-parity-plan.graphql"),
        json!({ "input": { "id": draft_id.clone() } }),
    ));
    assert_eq!(
        delete.body["data"]["draftOrderDelete"],
        json!({ "deletedId": draft_id, "userErrors": [] })
    );

    let after_delete = proxy.process_request(json_graphql_request(
        "query ReadDeletedDraft($id: ID!) { draftOrder(id: $id) { id } draftOrdersCount { count precision } }",
        json!({ "id": delete.body["data"]["draftOrderDelete"]["deletedId"].clone() }),
    ));
    assert_eq!(after_delete.body["data"]["draftOrder"], Value::Null);
    assert_eq!(
        after_delete.body["data"]["draftOrdersCount"],
        json!({ "count": 1, "precision": "EXACT" })
    );

    let bulk_delete = proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/orders/draft-order-residual-helper-bulk-delete.graphql"
        ),
        json!({ "ids": [duplicated_id.clone()] }),
    ));
    assert_eq!(
        bulk_delete.body["data"]["draftOrderBulkDelete"]["userErrors"],
        json!([])
    );
    assert_eq!(
        bulk_delete.body["data"]["draftOrderBulkDelete"]["job"]["done"],
        json!(false)
    );
    let after_bulk_delete = proxy.process_request(json_graphql_request(
        "query ReadBulkDeletedDraft($id: ID!) { draftOrder(id: $id) { id } draftOrdersCount { count precision } }",
        json!({ "id": duplicated_id }),
    ));
    assert_eq!(after_bulk_delete.body["data"]["draftOrder"], Value::Null);
    assert_eq!(
        after_bulk_delete.body["data"]["draftOrdersCount"],
        json!({ "count": 0, "precision": "EXACT" })
    );
}

#[test]
fn draft_order_create_reserve_inventory_until_uses_proxy_clock_boundary() {
    let clock = Arc::new(Mutex::new(utc_time(1_783_382_400)));
    let mut proxy = snapshot_proxy_with_clock(Arc::clone(&clock));

    let past = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateDraftWithPastReserveUntil($input: DraftOrderInput!) {
          draftOrderCreate(input: $input) {
            draftOrder { id }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "input": {
                "email": "past-reserve@example.test",
                "reserveInventoryUntil": "2025-06-01T00:00:00Z",
                "lineItems": [{
                    "title": "Past reserve line",
                    "quantity": 1,
                    "originalUnitPrice": "5.00"
                }]
            }
        }),
    ));

    assert_eq!(past.status, 200);
    assert_eq!(
        past.body["data"]["draftOrderCreate"],
        json!({
            "draftOrder": Value::Null,
            "userErrors": [{
                "field": Value::Null,
                "message": "Reserve until can't be in the past"
            }]
        })
    );
    assert_eq!(log_snapshot(&proxy)["entries"], json!([]));

    let future = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateDraftWithFutureReserveUntil($input: DraftOrderInput!) {
          draftOrderCreate(input: $input) {
            draftOrder { id reserveInventoryUntil }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "input": {
                "email": "future-reserve@example.test",
                "reserveInventoryUntil": "2026-07-08T00:00:00Z",
                "lineItems": [{
                    "title": "Future reserve line",
                    "quantity": 1,
                    "originalUnitPrice": "5.00"
                }]
            }
        }),
    ));

    assert_eq!(future.status, 200);
    assert_eq!(
        future.body["data"]["draftOrderCreate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        future.body["data"]["draftOrderCreate"]["draftOrder"]["reserveInventoryUntil"],
        json!("2026-07-08T00:00:00Z")
    );
    let log = log_snapshot(&proxy);
    assert_eq!(log["entries"].as_array().unwrap().len(), 1);
    assert_eq!(
        log["entries"][0]["operationName"],
        json!("draftOrderCreate")
    );
    assert!(log["entries"][0]["rawBody"]
        .as_str()
        .unwrap()
        .contains("CreateDraftWithFutureReserveUntil"));
}

#[test]
fn draft_orders_count_applies_query_filter_like_connection() {
    let mut proxy = snapshot_proxy();

    for (email, tags) in [
        ("count-match@example.test", json!(["match-tag"])),
        ("count-miss@example.test", json!(["other-tag"])),
    ] {
        let create = proxy.process_request(json_graphql_request(
            r#"
            mutation CreateCountedDraft($input: DraftOrderInput!) {
              draftOrderCreate(input: $input) {
                draftOrder { id email tags }
                userErrors { field message  }
              }
            }
            "#,
            json!({
                "input": {
                    "email": email,
                    "tags": tags,
                    "lineItems": [{
                        "title": "Count query item",
                        "quantity": 1,
                        "originalUnitPrice": "5.00"
                    }]
                }
            }),
        ));
        assert_eq!(create.status, 200);
        assert_eq!(
            create.body["data"]["draftOrderCreate"]["userErrors"],
            json!([])
        );
    }

    let read = proxy.process_request(json_graphql_request(
        r#"
        query FilterDraftOrders($emailQuery: String!, $tagQuery: String!, $statusQuery: String!) {
          byEmail: draftOrders(first: 10, query: $emailQuery) {
            nodes { id email }
          }
          byEmailCount: draftOrdersCount(query: $emailQuery) {
            count
            precision
          }
          byTag: draftOrders(first: 10, query: $tagQuery) {
            nodes { id tags }
          }
          byTagCount: draftOrdersCount(query: $tagQuery) {
            count
            precision
          }
          byStatus: draftOrders(first: 10, query: $statusQuery) {
            nodes { id status }
          }
          byStatusCount: draftOrdersCount(query: $statusQuery) {
            count
            precision
          }
        }
        "#,
        json!({
            "emailQuery": "email:count-match@example.test",
            "tagQuery": "tag:match-tag",
            "statusQuery": "status:open"
        }),
    ));
    assert_eq!(read.status, 200);
    assert_eq!(
        read.body["data"]["byEmail"]["nodes"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        read.body["data"]["byEmailCount"],
        json!({ "count": 1, "precision": "EXACT" })
    );
    assert_eq!(
        read.body["data"]["byTag"]["nodes"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        read.body["data"]["byTagCount"],
        json!({ "count": 1, "precision": "EXACT" })
    );
    assert_eq!(
        read.body["data"]["byStatus"]["nodes"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        read.body["data"]["byStatusCount"],
        json!({ "count": 2, "precision": "EXACT" })
    );
}

fn stage_searchable_draft_order(proxy: &mut DraftProxy, input: Value) -> Value {
    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateSearchableDraft($input: DraftOrderInput!) {
          draftOrderCreate(input: $input) {
            draftOrder {
              id
              name
              email
              status
              tags
              createdAt
              updatedAt
              customer { id email displayName }
              totalPriceSet { shopMoney { amount currencyCode } }
              lineItems(first: 5) { nodes { title sku } }
            }
            userErrors { field message }
          }
        }
        "#,
        json!({ "input": input }),
    ));
    assert_eq!(create.status, 200);
    assert_eq!(
        create.body["data"]["draftOrderCreate"]["userErrors"],
        json!([])
    );
    create.body["data"]["draftOrderCreate"]["draftOrder"].clone()
}

fn draft_order_node_emails(value: &Value, field: &str) -> Vec<String> {
    value["data"][field]["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|node| node["email"].as_str().map(str::to_string))
        .collect()
}

#[test]
fn draft_orders_connection_honors_sort_key_reverse_and_pagination() {
    let mut proxy = snapshot_proxy();
    let alpha = stage_searchable_draft_order(
        &mut proxy,
        json!({
            "email": "alpha-sort@example.test",
            "tags": ["sort-alpha"],
            "lineItems": [{
                "title": "Alpha sort line",
                "quantity": 1,
                "originalUnitPrice": "12.50"
            }]
        }),
    );
    let beta = stage_searchable_draft_order(
        &mut proxy,
        json!({
            "email": "beta-sort@example.test",
            "tags": ["sort-beta"],
            "lineItems": [{
                "title": "Beta sort line",
                "quantity": 1,
                "originalUnitPrice": "5.00"
            }]
        }),
    );

    let update_alpha = proxy.process_request(json_graphql_request(
        r#"
        mutation TouchAlphaDraft($id: ID!, $input: DraftOrderInput!) {
          draftOrderUpdate(id: $id, input: $input) {
            draftOrder { id updatedAt }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "id": alpha["id"],
            "input": { "note": "touch alpha for updated sort" }
        }),
    ));
    assert_eq!(
        update_alpha.body["data"]["draftOrderUpdate"]["userErrors"],
        json!([])
    );

    let read = proxy.process_request(json_graphql_request(
        r#"
        query DraftOrderSorts($after: String!) {
          defaultOrder: draftOrders(first: 10) {
            nodes { id email updatedAt totalPriceSet { shopMoney { amount } } }
          }
          idOrder: draftOrders(first: 10, sortKey: ID) {
            nodes { id email }
          }
          idReverse: draftOrders(first: 10, sortKey: ID, reverse: true) {
            nodes { id email }
          }
          updatedAsc: draftOrders(first: 10, sortKey: UPDATED_AT) {
            nodes { id email updatedAt }
          }
          totalAsc: draftOrders(first: 10, sortKey: TOTAL_PRICE) {
            nodes { id email totalPriceSet { shopMoney { amount } } }
          }
          firstPage: draftOrders(first: 1, sortKey: ID) {
            nodes { id email }
            edges { cursor node { id } }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
          secondPage: draftOrders(first: 1, sortKey: ID, after: $after) {
            nodes { id email }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
          countAll: draftOrdersCount { count precision }
        }
        "#,
        json!({ "after": alpha["id"] }),
    ));
    assert_eq!(read.status, 200);
    assert_eq!(
        draft_order_node_emails(&read.body, "defaultOrder"),
        vec!["alpha-sort@example.test", "beta-sort@example.test"]
    );
    assert_eq!(
        draft_order_node_emails(&read.body, "idOrder"),
        vec!["alpha-sort@example.test", "beta-sort@example.test"]
    );
    assert_eq!(
        draft_order_node_emails(&read.body, "idReverse"),
        vec!["beta-sort@example.test", "alpha-sort@example.test"]
    );
    assert_eq!(
        draft_order_node_emails(&read.body, "updatedAsc"),
        vec!["beta-sort@example.test", "alpha-sort@example.test"]
    );
    assert_eq!(
        draft_order_node_emails(&read.body, "totalAsc"),
        vec!["beta-sort@example.test", "alpha-sort@example.test"]
    );
    assert_eq!(
        read.body["data"]["firstPage"]["nodes"][0]["id"],
        alpha["id"]
    );
    assert_eq!(
        read.body["data"]["firstPage"]["edges"][0]["cursor"],
        alpha["id"]
    );
    assert_eq!(
        read.body["data"]["firstPage"]["pageInfo"],
        json!({
            "hasNextPage": true,
            "hasPreviousPage": false,
            "startCursor": alpha["id"],
            "endCursor": alpha["id"]
        })
    );
    assert_eq!(
        read.body["data"]["secondPage"]["nodes"][0]["id"],
        beta["id"]
    );
    assert_eq!(
        read.body["data"]["secondPage"]["pageInfo"],
        json!({
            "hasNextPage": false,
            "hasPreviousPage": true,
            "startCursor": beta["id"],
            "endCursor": beta["id"]
        })
    );
    assert_eq!(
        read.body["data"]["countAll"],
        json!({ "count": 2, "precision": "EXACT" })
    );
}

#[test]
fn draft_orders_query_supports_search_fields_and_ignores_unknown_fields() {
    let mut proxy = snapshot_proxy();
    stage_searchable_draft_order(
        &mut proxy,
        json!({
            "email": "alpha-query@example.test",
            "customerId": "gid://shopify/Customer/111",
            "tags": ["alpha-token", "shared-token"],
            "lineItems": [{
                "title": "Needle Alpha line",
                "quantity": 1,
                "originalUnitPrice": "12.50"
            }]
        }),
    );
    stage_searchable_draft_order(
        &mut proxy,
        json!({
            "email": "beta-query@example.test",
            "customerId": "gid://shopify/Customer/222",
            "tags": ["beta-token"],
            "lineItems": [{
                "title": "Beta line",
                "quantity": 1,
                "originalUnitPrice": "5.00"
            }]
        }),
    );

    let read = proxy.process_request(json_graphql_request(
        r#"
        query DraftOrderSearches(
          $statusTag: String!
          $customer: String!
          $tag: String!
          $created: String!
          $updated: String!
          $total: String!
          $freeText: String!
          $unknown: String!
          $unknownAndKnown: String!
        ) {
          statusTag: draftOrders(first: 10, query: $statusTag) {
            nodes { email status tags }
          }
          customer: draftOrders(first: 10, query: $customer) {
            nodes { email customer { id } }
          }
          tag: draftOrders(first: 10, query: $tag) {
            nodes { email tags }
          }
          created: draftOrders(first: 10, query: $created) {
            nodes { email createdAt }
          }
          updated: draftOrders(first: 10, query: $updated) {
            nodes { email updatedAt }
          }
          total: draftOrders(first: 10, query: $total) {
            nodes { email totalPriceSet { shopMoney { amount } } }
          }
          freeText: draftOrders(first: 10, query: $freeText) {
            nodes { email lineItems(first: 5) { nodes { title } } }
          }
          unknown: draftOrders(first: 10, query: $unknown) {
            nodes { email }
          }
          unknownCount: draftOrdersCount(query: $unknown) {
            count
            precision
          }
          unknownAndKnown: draftOrders(first: 10, query: $unknownAndKnown) {
            nodes { email tags }
          }
          totalCount: draftOrdersCount(query: $total) {
            count
            precision
          }
        }
        "#,
        json!({
            "statusTag": "status:open tag:alpha-token",
            "customer": "customer_id:111",
            "tag": "tag:alpha-token",
            "created": "created_at:>=2024-01-01",
            "updated": "updated_at:>2024-01-01T00:00:01.000Z",
            "total": "total_price:>10",
            "freeText": "Needle",
            "unknown": "notadraftfield:ignored",
            "unknownAndKnown": "notadraftfield:ignored tag:alpha-token"
        }),
    ));
    assert_eq!(read.status, 200);
    assert_eq!(
        draft_order_node_emails(&read.body, "statusTag"),
        vec!["alpha-query@example.test"]
    );
    assert_eq!(
        draft_order_node_emails(&read.body, "customer"),
        vec!["alpha-query@example.test"]
    );
    assert_eq!(
        draft_order_node_emails(&read.body, "tag"),
        vec!["alpha-query@example.test"]
    );
    assert_eq!(
        draft_order_node_emails(&read.body, "created"),
        vec!["alpha-query@example.test", "beta-query@example.test"]
    );
    assert_eq!(
        draft_order_node_emails(&read.body, "updated"),
        vec!["beta-query@example.test"]
    );
    assert_eq!(
        draft_order_node_emails(&read.body, "total"),
        vec!["alpha-query@example.test"]
    );
    assert_eq!(
        draft_order_node_emails(&read.body, "freeText"),
        vec!["alpha-query@example.test"]
    );
    assert_eq!(
        draft_order_node_emails(&read.body, "unknown"),
        vec!["alpha-query@example.test", "beta-query@example.test"]
    );
    assert_eq!(
        read.body["data"]["unknownCount"],
        json!({ "count": 2, "precision": "EXACT" })
    );
    assert_eq!(
        draft_order_node_emails(&read.body, "unknownAndKnown"),
        vec!["alpha-query@example.test"]
    );
    assert_eq!(
        read.body["data"]["totalCount"],
        json!({ "count": 1, "precision": "EXACT" })
    );
}

#[test]
fn draft_order_variant_line_items_use_catalog_values_over_custom_only_input() {
    let upstream_calls = Arc::new(Mutex::new(Vec::<Value>::new()));
    let captured_calls = Arc::clone(&upstream_calls);
    let variant_response = json!({
        "data": {
            "productVariant": {
                "id": "gid://shopify/ProductVariant/424242",
                "title": "Catalog option title",
                "sku": "CATALOG-SKU",
                "taxable": true,
                "price": "19.95",
                "inventoryItem": { "requiresShipping": true },
                "product": { "title": "Catalog product title" }
            }
        }
    });
    let mut proxy = configured_proxy(
        ReadMode::LiveHybrid,
        Some(shopify_draft_proxy::proxy::UnsupportedMutationMode::Passthrough),
    )
    .with_upstream_transport(move |request| {
        let body: Value =
            serde_json::from_str(&request.body).expect("variant hydrate request body parses");
        if body["query"]
            .as_str()
            .is_some_and(|query| query.contains("DraftProxyShopPricingHydrate"))
        {
            captured_calls.lock().unwrap().push(body);
            return Response {
                status: 200,
                headers: Default::default(),
                body: json!({
                    "data": {
                        "shop": {
                            "currencyCode": "CAD",
                            "taxesIncluded": false,
                            "taxShipping": false
                        }
                    }
                }),
            };
        }
        assert_eq!(
            body["operationName"],
            json!("OrdersDraftOrderVariantHydrate")
        );
        assert_eq!(
            body["variables"]["id"],
            json!("gid://shopify/ProductVariant/424242")
        );
        captured_calls.lock().unwrap().push(body);
        Response {
            status: 200,
            headers: Default::default(),
            body: variant_response.clone(),
        }
    });
    let variant_line = json!({
        "variantId": "gid://shopify/ProductVariant/424242",
        "title": "FAKE TITLE",
        "sku": "FAKE-SKU",
        "quantity": 2,
        "originalUnitPrice": "0.01",
        "taxable": false,
        "requiresShipping": false
    });
    let custom_line = json!({
        "title": "Custom-only item",
        "sku": "CUSTOM-SKU",
        "quantity": 1,
        "originalUnitPrice": "7.50",
        "taxable": false,
        "requiresShipping": false
    });

    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation DraftOrderCreateVariantCustomOnlyFields($input: DraftOrderInput!) {
          draftOrderCreate(input: $input) {
            draftOrder {
              id
              lineItems(first: 5) {
                nodes {
                  title
                  name
                  sku
                  quantity
                  custom
                  requiresShipping
                  taxable
                  variant { id title sku }
                  originalUnitPriceSet { shopMoney { amount currencyCode } }
                }
              }
            }
            userErrors { field message  }
          }
        }
        "#,
        json!({
            "input": {
                "email": "variant-backed-draft@example.test",
                "lineItems": [variant_line.clone(), custom_line.clone()]
            }
        }),
    ));
    assert_eq!(
        create.body["data"]["draftOrderCreate"]["userErrors"],
        json!([])
    );
    let created_draft = &create.body["data"]["draftOrderCreate"]["draftOrder"];
    assert_draft_order_variant_catalog_line(&created_draft["lineItems"]["nodes"][0], 2, "CAD");
    assert_draft_order_custom_line(&created_draft["lineItems"]["nodes"][1], "CAD");

    let update = proxy.process_request(json_graphql_request(
        r#"
        mutation DraftOrderUpdateVariantCustomOnlyFields($id: ID!, $input: DraftOrderInput!) {
          draftOrderUpdate(id: $id, input: $input) {
            draftOrder {
              lineItems(first: 5) {
                nodes {
                  title
                  name
                  sku
                  quantity
                  custom
                  requiresShipping
                  taxable
                  variant { id title sku }
                  originalUnitPriceSet { shopMoney { amount currencyCode } }
                }
              }
            }
            userErrors { field message  }
          }
        }
        "#,
        json!({
            "id": created_draft["id"].clone(),
            "input": {
                "lineItems": [variant_line.clone()]
            }
        }),
    ));
    assert_eq!(
        update.body["data"]["draftOrderUpdate"]["userErrors"],
        json!([])
    );
    assert_draft_order_variant_catalog_line(
        &update.body["data"]["draftOrderUpdate"]["draftOrder"]["lineItems"]["nodes"][0],
        2,
        "CAD",
    );

    let calculate = proxy.process_request(json_graphql_request(
        r#"
        mutation DraftOrderCalculateVariantCustomOnlyFields($input: DraftOrderInput!) {
          draftOrderCalculate(input: $input) {
            calculatedDraftOrder {
              lineItems {
                title
                name
                sku
                quantity
                custom
                requiresShipping
                taxable
                variant { id title sku }
                originalUnitPriceSet { shopMoney { amount currencyCode } }
              }
            }
            userErrors { field message  }
          }
        }
        "#,
        json!({ "input": { "lineItems": [variant_line] } }),
    ));
    assert_eq!(
        calculate.body["data"]["draftOrderCalculate"]["userErrors"],
        json!([])
    );
    assert_draft_order_variant_catalog_line(
        &calculate.body["data"]["draftOrderCalculate"]["calculatedDraftOrder"]["lineItems"][0],
        2,
        "CAD",
    );

    let calls = upstream_calls.lock().unwrap();
    assert_eq!(calls.len(), 4);
    assert_eq!(
        calls
            .iter()
            .filter(|body| {
                body["query"]
                    .as_str()
                    .is_some_and(|query| query.contains("DraftProxyShopPricingHydrate"))
            })
            .count(),
        1
    );
    assert_eq!(
        calls
            .iter()
            .filter(|body| body["operationName"] == json!("OrdersDraftOrderVariantHydrate"))
            .count(),
        3
    );
}

#[test]
fn draft_order_variant_hydration_batches_unique_missing_variants_per_operation() {
    let variant_a = "gid://shopify/ProductVariant/100001";
    let variant_b = "gid://shopify/ProductVariant/100002";
    let variant_c = "gid://shopify/ProductVariant/100003";
    let upstream_calls = Arc::new(Mutex::new(Vec::<Value>::new()));
    let captured_calls = Arc::clone(&upstream_calls);
    let mut proxy =
        configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport(move |request| {
            let body: Value =
                serde_json::from_str(&request.body).expect("variant hydrate request parses");
            captured_calls.lock().unwrap().push(body.clone());
            if body["query"]
                .as_str()
                .is_some_and(|query| query.contains("DraftProxyShopPricingHydrate"))
            {
                return Response {
                    status: 200,
                    headers: Default::default(),
                    body: json!({
                        "data": {
                            "shop": {
                                "currencyCode": "USD",
                                "taxesIncluded": false,
                                "taxShipping": false
                            }
                        }
                    }),
                };
            }
            match body["operationName"].as_str() {
                Some("OrdersDraftOrderVariantHydrate") => {
                    let id = body["variables"]["id"]
                        .as_str()
                        .expect("single variant hydrate includes id");
                    Response {
                        status: 200,
                        headers: Default::default(),
                        body: draft_order_test_variant_response(id),
                    }
                }
                Some("OrdersDraftOrderVariantsHydrate") => {
                    let nodes = body["variables"]["ids"]
                        .as_array()
                        .expect("batched variant hydrate includes ids")
                        .iter()
                        .map(|id| {
                            draft_order_test_variant_node(
                                id.as_str().expect("variant id should be a string"),
                            )
                        })
                        .collect::<Vec<_>>();
                    Response {
                        status: 200,
                        headers: Default::default(),
                        body: json!({ "data": { "nodes": nodes } }),
                    }
                }
                other => panic!("unexpected upstream hydrate operation: {other:?}"),
            }
        });

    let input = json!({
        "shippingLine": {
            "title": "No-op shipping",
            "priceWithCurrency": { "amount": "0.00", "currencyCode": "USD" }
        },
        "lineItems": [
            { "variantId": variant_a, "quantity": 1 },
            { "variantId": variant_b, "quantity": 2 },
            { "variantId": variant_a, "quantity": 3 },
            { "variantId": variant_c, "quantity": 4 }
        ]
    });

    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateDraftWithManyVariants($input: DraftOrderInput!) {
          draftOrderCreate(input: $input) {
            draftOrder {
              id
              lineItems(first: 5) {
                nodes { sku variant { id sku } originalUnitPriceSet { shopMoney { amount currencyCode } } }
              }
            }
            userErrors { field message }
          }
        }
        "#,
        json!({ "input": input.clone() }),
    ));
    assert_eq!(create.status, 200);
    assert_eq!(
        create.body["data"]["draftOrderCreate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        create.body["data"]["draftOrderCreate"]["draftOrder"]["lineItems"]["nodes"][2]["sku"],
        json!("SKU-100001")
    );
    {
        let calls = upstream_calls.lock().unwrap();
        assert_eq!(calls.len(), 2);
        assert_eq!(
            calls[0]["operationName"],
            json!("OrdersDraftOrderVariantsHydrate")
        );
        assert_eq!(
            calls[0]["variables"]["ids"],
            json!([variant_a, variant_b, variant_c])
        );
        assert!(calls[1]["query"]
            .as_str()
            .is_some_and(|query| query.contains("DraftProxyShopPricingHydrate")));
    }

    let draft_order_id = create.body["data"]["draftOrderCreate"]["draftOrder"]["id"].clone();
    let update = proxy.process_request(json_graphql_request(
        r#"
        mutation UpdateDraftWithManyVariants($id: ID!, $input: DraftOrderInput!) {
          draftOrderUpdate(id: $id, input: $input) {
            draftOrder { lineItems(first: 5) { nodes { sku variant { id sku } } } }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "id": draft_order_id,
            "input": {
                "shippingLine": {
                    "title": "No-op shipping",
                    "priceWithCurrency": { "amount": "0.00", "currencyCode": "USD" }
                },
                "lineItems": [
                    { "variantId": variant_b, "quantity": 1 },
                    { "variantId": variant_c, "quantity": 1 },
                    { "variantId": variant_b, "quantity": 1 }
                ]
            }
        }),
    ));
    assert_eq!(update.status, 200);
    assert_eq!(
        update.body["data"]["draftOrderUpdate"]["userErrors"],
        json!([])
    );
    {
        let calls = upstream_calls.lock().unwrap();
        assert_eq!(calls.len(), 3);
        assert_eq!(
            calls[2]["operationName"],
            json!("OrdersDraftOrderVariantsHydrate")
        );
        assert_eq!(calls[2]["variables"]["ids"], json!([variant_b, variant_c]));
    }

    let calculate = proxy.process_request(json_graphql_request(
        r#"
        mutation CalculateDraftWithManyVariants($input: DraftOrderInput!) {
          draftOrderCalculate(input: $input) {
            calculatedDraftOrder { lineItems { sku variant { id sku } } }
            userErrors { field message }
          }
        }
        "#,
        json!({ "input": input }),
    ));
    assert_eq!(calculate.status, 200);
    assert_eq!(
        calculate.body["data"]["draftOrderCalculate"]["userErrors"],
        json!([])
    );
    let calls = upstream_calls.lock().unwrap();
    assert_eq!(calls.len(), 4);
    assert_eq!(
        calls[3]["operationName"],
        json!("OrdersDraftOrderVariantsHydrate")
    );
    assert_eq!(
        calls[3]["variables"]["ids"],
        json!([variant_a, variant_b, variant_c])
    );
}

#[test]
fn draft_order_custom_only_line_items_do_not_hydrate_variants() {
    let upstream_calls = Arc::new(AtomicUsize::new(0));
    let captured_calls = Arc::clone(&upstream_calls);
    let mut proxy =
        configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport(move |request| {
            captured_calls.fetch_add(1, Ordering::SeqCst);
            let body: Value =
                serde_json::from_str(&request.body).expect("shop pricing hydrate parses");
            assert!(body["query"]
                .as_str()
                .is_some_and(|query| query.contains("DraftProxyShopPricingHydrate")));
            Response {
                status: 200,
                headers: Default::default(),
                body: json!({
                    "data": {
                        "shop": {
                            "currencyCode": "USD",
                            "taxesIncluded": false,
                            "taxShipping": false
                        }
                    }
                }),
            }
        });
    let input = json!({
        "lineItems": [{
            "title": "Custom batch guard",
            "quantity": 2,
            "originalUnitPriceWithCurrency": { "amount": "8.50", "currencyCode": "USD" },
            "sku": "CUSTOM-BATCH",
            "requiresShipping": false,
            "taxable": false
        }]
    });

    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateCustomOnlyDraft($input: DraftOrderInput!) {
          draftOrderCreate(input: $input) {
            draftOrder { id lineItems(first: 5) { nodes { title sku custom } } }
            userErrors { field message }
          }
        }
        "#,
        json!({ "input": input.clone() }),
    ));
    assert_eq!(create.status, 200);
    assert_eq!(
        create.body["data"]["draftOrderCreate"]["userErrors"],
        json!([])
    );
    let draft_order_id = create.body["data"]["draftOrderCreate"]["draftOrder"]["id"].clone();

    let update = proxy.process_request(json_graphql_request(
        r#"
        mutation UpdateCustomOnlyDraft($id: ID!, $input: DraftOrderInput!) {
          draftOrderUpdate(id: $id, input: $input) {
            draftOrder { lineItems(first: 5) { nodes { title sku custom } } }
            userErrors { field message }
          }
        }
        "#,
        json!({ "id": draft_order_id, "input": input.clone() }),
    ));
    assert_eq!(update.status, 200);
    assert_eq!(
        update.body["data"]["draftOrderUpdate"]["userErrors"],
        json!([])
    );

    let calculate = proxy.process_request(json_graphql_request(
        r#"
        mutation CalculateCustomOnlyDraft($input: DraftOrderInput!) {
          draftOrderCalculate(input: $input) {
            calculatedDraftOrder { lineItems { title sku custom } }
            userErrors { field message }
          }
        }
        "#,
        json!({ "input": input }),
    ));
    assert_eq!(calculate.status, 200);
    assert_eq!(
        calculate.body["data"]["draftOrderCalculate"]["userErrors"],
        json!([])
    );
    assert_eq!(upstream_calls.load(Ordering::SeqCst), 1);
}

#[test]
fn draft_order_scalar_custom_line_prices_use_hydrated_shop_currency() {
    let upstream_calls = Arc::new(Mutex::new(Vec::<Value>::new()));
    let captured_calls = Arc::clone(&upstream_calls);
    let mut proxy = configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport(
        move |request| {
            let body: Value =
                serde_json::from_str(&request.body).expect("shop pricing hydrate parses");
            assert_eq!(
                body["query"],
                json!("query DraftProxyShopPricingHydrate { shop { currencyCode taxesIncluded taxShipping } }")
            );
            captured_calls.lock().unwrap().push(body);
            Response {
                status: 200,
                headers: Default::default(),
                body: json!({
                    "data": {
                        "shop": {
                            "currencyCode": "CAD",
                            "taxesIncluded": false,
                            "taxShipping": false
                        }
                    }
                }),
            }
        },
    );

    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation DraftOrderCreateScalarShopCurrency($input: DraftOrderInput!) {
          draftOrderCreate(input: $input) {
            draftOrder {
              currencyCode
              subtotalPriceSet { shopMoney { amount currencyCode } }
              totalPriceSet { shopMoney { amount currencyCode } }
              lineItems(first: 1) {
                nodes {
                  originalUnitPriceSet { shopMoney { amount currencyCode } }
                  originalTotalSet { shopMoney { amount currencyCode } }
                }
              }
            }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "input": {
                "lineItems": [{
                    "title": "Scalar CAD default",
                    "quantity": 2,
                    "originalUnitPrice": "12.50"
                }]
            }
        }),
    ));
    assert_eq!(
        create.body["data"]["draftOrderCreate"]["userErrors"],
        json!([])
    );
    let draft = &create.body["data"]["draftOrderCreate"]["draftOrder"];
    assert_eq!(draft["currencyCode"], json!("CAD"));
    assert_eq!(
        draft["lineItems"]["nodes"][0]["originalUnitPriceSet"]["shopMoney"],
        json!({ "amount": "12.5", "currencyCode": "CAD" })
    );
    assert_eq!(
        draft["lineItems"]["nodes"][0]["originalTotalSet"]["shopMoney"],
        json!({ "amount": "25.0", "currencyCode": "CAD" })
    );
    assert_eq!(
        draft["subtotalPriceSet"]["shopMoney"],
        json!({ "amount": "25.0", "currencyCode": "CAD" })
    );
    assert_eq!(
        draft["totalPriceSet"]["shopMoney"],
        json!({ "amount": "25.0", "currencyCode": "CAD" })
    );

    let calculate = proxy.process_request(json_graphql_request(
        r#"
        mutation DraftOrderCalculateScalarShopCurrency($input: DraftOrderInput!) {
          draftOrderCalculate(input: $input) {
            calculatedDraftOrder {
              currencyCode
              subtotalPriceSet { shopMoney { amount currencyCode } }
              totalPriceSet { shopMoney { amount currencyCode } }
              lineItems {
                originalUnitPriceSet { shopMoney { amount currencyCode } }
                originalTotalSet { shopMoney { amount currencyCode } }
              }
            }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "input": {
                "lineItems": [{
                    "title": "Calculated scalar CAD default",
                    "quantity": 3,
                    "originalUnitPrice": "2.50"
                }]
            }
        }),
    ));
    assert_eq!(
        calculate.body["data"]["draftOrderCalculate"]["userErrors"],
        json!([])
    );
    let calculated = &calculate.body["data"]["draftOrderCalculate"]["calculatedDraftOrder"];
    assert_eq!(calculated["currencyCode"], json!("CAD"));
    assert_eq!(
        calculated["lineItems"][0]["originalUnitPriceSet"]["shopMoney"],
        json!({ "amount": "2.5", "currencyCode": "CAD" })
    );
    assert_eq!(
        calculated["lineItems"][0]["originalTotalSet"]["shopMoney"],
        json!({ "amount": "7.5", "currencyCode": "CAD" })
    );
    assert_eq!(
        calculated["subtotalPriceSet"]["shopMoney"],
        json!({ "amount": "7.5", "currencyCode": "CAD" })
    );
    assert_eq!(
        calculated["totalPriceSet"]["shopMoney"],
        json!({ "amount": "7.5", "currencyCode": "CAD" })
    );
    assert_eq!(upstream_calls.lock().unwrap().len(), 1);
}

#[test]
fn draft_order_explicit_presentment_currency_still_hydrates_shop_money_currency() {
    let upstream_calls = Arc::new(Mutex::new(Vec::<Value>::new()));
    let captured_calls = Arc::clone(&upstream_calls);
    let mut proxy =
        configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport(move |request| {
            let body: Value =
                serde_json::from_str(&request.body).expect("shop pricing hydrate parses");
            captured_calls.lock().unwrap().push(body.clone());
            assert!(body["query"]
                .as_str()
                .is_some_and(|query| query.contains("DraftProxyShopPricingHydrate")));
            Response {
                status: 200,
                headers: Default::default(),
                body: json!({
                    "data": {
                        "shop": {
                            "currencyCode": "CAD",
                            "taxesIncluded": false,
                            "taxShipping": false
                        }
                    }
                }),
            }
        });

    let response = proxy.process_request(json_graphql_request(
        r#"
        mutation DraftOrderExplicitPresentmentCurrency($input: DraftOrderInput!) {
          draftOrderCreate(input: $input) {
            draftOrder {
              presentmentCurrencyCode
              totalPriceSet {
                shopMoney { amount currencyCode }
                presentmentMoney { amount currencyCode }
              }
            }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "input": {
                "presentmentCurrencyCode": "USD",
                "lineItems": [{
                    "title": "Explicit presentment currency",
                    "quantity": 1,
                    "originalUnitPriceWithCurrency": {
                        "amount": "10.00",
                        "currencyCode": "USD"
                    }
                }]
            }
        }),
    ));

    assert_eq!(response.status, 200);
    assert_eq!(response.body.get("errors"), None);
    let draft = &response.body["data"]["draftOrderCreate"]["draftOrder"];
    assert_eq!(draft["presentmentCurrencyCode"], json!("USD"));
    assert_eq!(
        draft["totalPriceSet"]["shopMoney"]["currencyCode"],
        json!("CAD")
    );
    assert_eq!(
        draft["totalPriceSet"]["presentmentMoney"]["currencyCode"],
        json!("USD")
    );
    assert_eq!(upstream_calls.lock().unwrap().len(), 1);
}

#[test]
fn draft_order_variant_unavailable_uses_hydrated_store_state() {
    let upstream_calls = Arc::new(Mutex::new(Vec::<Value>::new()));
    let captured_calls = Arc::clone(&upstream_calls);
    let mut proxy = configured_proxy(
        ReadMode::LiveHybrid,
        Some(shopify_draft_proxy::proxy::UnsupportedMutationMode::Passthrough),
    )
    .with_upstream_transport(move |request| {
        let body: Value =
            serde_json::from_str(&request.body).expect("variant hydrate request body parses");
        assert_eq!(
            body["operationName"],
            json!("OrdersDraftOrderVariantHydrate")
        );
        assert_eq!(
            body["variables"]["id"],
            json!("gid://shopify/ProductVariant/424243")
        );
        captured_calls.lock().unwrap().push(body);
        Response {
            status: 200,
            headers: Default::default(),
            body: json!({ "data": { "productVariant": Value::Null } }),
        }
    });

    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation DraftOrderCreateUnavailableVariant($input: DraftOrderInput!) {
          draftOrderCreate(input: $input) {
            draftOrder { id }
            userErrors { field message  }
          }
        }
        "#,
        json!({
            "input": {
                "email": "unavailable-variant@example.com",
                "lineItems": [{
                    "variantId": "gid://shopify/ProductVariant/424243",
                    "quantity": 1
                }]
            }
        }),
    ));
    assert_eq!(create.status, 200);
    assert_eq!(
        create.body["data"]["draftOrderCreate"],
        json!({
            "draftOrder": Value::Null,
            "userErrors": [{
                "field": Value::Null,
                "message": "Product with ID 424243 is no longer available."
            }]
        })
    );
    assert_eq!(upstream_calls.lock().unwrap().len(), 1);
}

#[test]
fn draft_order_create_from_order_and_invoice_preview_stage_locally() {
    let mut proxy = snapshot_proxy();
    let order_create = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateOrderForDraft($order: OrderCreateOrderInput!) {
          orderCreate(order: $order) {
            order { id email tags lineItems(first: 5) { nodes { title quantity sku originalUnitPriceSet { shopMoney { amount currencyCode } } } } }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "order": {
                "currency": "CAD",
                "email": "order-backed-draft@example.test",
                "tags": ["source-order"],
                "lineItems": [{
                    "title": "Order backed item",
                    "quantity": 1,
                    "sku": "ORDER-BACKED",
                    "priceSet": { "shopMoney": { "amount": "9.00", "currencyCode": "CAD" } }
                }]
            }
        }),
    ));
    let order_id = order_create.body["data"]["orderCreate"]["order"]["id"].clone();

    let create_from_order = proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/orders/draftOrderCreateFromOrder-parity-plan.graphql"
        ),
        json!({ "orderId": order_id }),
    ));
    assert_eq!(
        create_from_order.body["data"]["draftOrderCreateFromOrder"]["userErrors"],
        json!([])
    );
    let draft_id =
        create_from_order.body["data"]["draftOrderCreateFromOrder"]["draftOrder"]["id"].clone();
    assert_eq!(
        create_from_order.body["data"]["draftOrderCreateFromOrder"]["draftOrder"]["email"],
        json!("order-backed-draft@example.test")
    );

    let preview = proxy.process_request(json_graphql_request(
        include_str!("../../config/parity-requests/orders/draft-order-residual-helper-invoice-preview.graphql"),
        json!({
            "id": draft_id,
            "email": {
                "subject": "Custom draft subject",
                "customMessage": "Custom invoice note"
            }
        }),
    ));
    assert_eq!(
        preview.body["data"]["draftOrderInvoicePreview"]["previewSubject"],
        json!("Custom draft subject")
    );
    assert!(
        preview.body["data"]["draftOrderInvoicePreview"]["previewHtml"]
            .as_str()
            .is_some_and(|html| html.contains("Custom invoice note"))
    );
    assert_eq!(
        preview.body["data"]["draftOrderInvoicePreview"]["userErrors"],
        json!([])
    );
}

#[test]
fn draft_order_validations_return_captured_error_shapes_without_staging() {
    let mut proxy = snapshot_proxy();

    let line_items: Vec<Value> = (0..500)
        .map(|index| {
            json!({
                "title": format!("Bulk item {index}"),
                "quantity": 1,
                "originalUnitPrice": "1.00"
            })
        })
        .collect();
    let line_items_max = proxy.process_request(json_graphql_request(
        include_str!("../../config/parity-requests/orders/draftOrder-line-items-max.graphql"),
        json!({
            "createInput": { "lineItems": line_items.clone() },
            "updateInput": { "lineItems": line_items.clone() },
            "calculateInput": { "lineItems": line_items }
        }),
    ));
    assert_eq!(line_items_max.status, 200);
    assert_eq!(
        line_items_max.body["data"],
        json!({
            "draftOrderCreate": Value::Null,
            "draftOrderUpdate": Value::Null,
            "draftOrderCalculate": Value::Null
        })
    );
    assert_eq!(line_items_max.body["errors"].as_array().unwrap().len(), 3);
    assert!(line_items_max.body["errors"]
        .as_array()
        .unwrap()
        .iter()
        .all(|error| {
            error["message"]
                == json!("The input array size of 500 is greater than the maximum allowed of 499.")
                && error["extensions"]["code"] == json!("MAX_INPUT_SIZE_EXCEEDED")
        }));
    assert_eq!(
        line_items_max.body["errors"]
            .as_array()
            .unwrap()
            .iter()
            .map(|error| error["locations"][0].clone())
            .collect::<Vec<_>>(),
        vec![
            json!({ "line": 6, "column": 3 }),
            json!({ "line": 16, "column": 3 }),
            json!({ "line": 26, "column": 3 })
        ]
    );

    let too_many_tags: Vec<Value> = (0..251)
        .map(|index| json!(format!("tag-{index}")))
        .collect();
    let too_many_tags_response = proxy.process_request(json_graphql_request(
        include_str!("../../config/parity-requests/orders/draftOrder-tag-validation-create.graphql"),
        json!({
            "input": {
                "lineItems": [{ "title": "Tagged item", "quantity": 1, "originalUnitPrice": "1.00" }],
                "tags": too_many_tags
            }
        }),
    ));
    assert_eq!(
        too_many_tags_response.body["data"],
        json!({ "draftOrderCreate": Value::Null })
    );
    assert_eq!(
        too_many_tags_response.body["errors"][0]["path"],
        json!(["draftOrderCreate", "input", "tags"])
    );
    assert_eq!(
        too_many_tags_response.body["errors"][0]["extensions"]["code"],
        json!("MAX_INPUT_SIZE_EXCEEDED")
    );

    let long_tag = "x".repeat(41);
    let long_create = proxy.process_request(json_graphql_request(
        include_str!("../../config/parity-requests/orders/draftOrder-tag-validation-create.graphql"),
        json!({
            "input": {
                "lineItems": [{ "title": "Tagged item", "quantity": 1, "originalUnitPrice": "1.00" }],
                "tags": [long_tag.clone()]
            }
        }),
    ));
    assert_eq!(
        long_create.body["data"]["draftOrderCreate"],
        json!({
            "draftOrder": Value::Null,
            "userErrors": [{
                "field": ["tags", "0"],
                "message": "Title Tag exceeds the maximum length of 40 characters"
            }]
        })
    );

    let setup = proxy.process_request(json_graphql_request(
        include_str!("../../config/parity-requests/orders/draftOrder-tag-validation-create.graphql"),
        json!({
            "input": {
                "lineItems": [{ "title": "Tagged item", "quantity": 1, "originalUnitPrice": "1.00" }],
                "tags": ["initial"]
            }
        }),
    ));
    let draft_id = setup.body["data"]["draftOrderCreate"]["draftOrder"]["id"].clone();
    let long_update = proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/orders/draftOrder-tag-validation-update.graphql"
        ),
        json!({ "id": draft_id.clone(), "input": { "tags": [long_tag.clone()] } }),
    ));
    assert_eq!(
        long_update.body["data"]["draftOrderUpdate"],
        json!({
            "draftOrder": Value::Null,
            "userErrors": [{
                "field": ["input", "tags", "1"],
                "message": "Title Tag exceeds the maximum length of 40 characters"
            }]
        })
    );
    let read_after_reject = proxy.process_request(json_graphql_request(
        include_str!("../../config/parity-requests/orders/draftOrder-tag-validation-read.graphql"),
        json!({ "id": draft_id }),
    ));
    assert_eq!(
        read_after_reject.body["data"]["draftOrder"]["tags"],
        json!(["initial"])
    );

    let long_calculate = proxy.process_request(json_graphql_request(
        include_str!("../../config/parity-requests/orders/draftOrder-tag-validation-calculate.graphql"),
        json!({
            "input": {
                "lineItems": [{ "title": "Tagged item", "quantity": 1, "originalUnitPrice": "1.00" }],
                "tags": [long_tag]
            }
        }),
    ));
    assert_eq!(
        long_calculate.body["data"]["draftOrderCalculate"],
        json!({
            "calculatedDraftOrder": Value::Null,
            "userErrors": [{
                "field": ["tags", "0"],
                "message": "Title Tag exceeds the maximum length of 40 characters"
            }]
        })
    );
}

fn without_id_fields(value: &Value) -> Value {
    match value {
        Value::Array(values) => Value::Array(values.iter().map(without_id_fields).collect()),
        Value::Object(object) => Value::Object(
            object
                .iter()
                .filter(|(key, _)| key.as_str() != "id")
                .map(|(key, value)| (key.clone(), without_id_fields(value)))
                .collect(),
        ),
        _ => value.clone(),
    }
}

fn assert_discount_validation_rejected_aliases(actual: &Value, expected: &Value) {
    for alias in [
        "orderPercentageAboveMax",
        "orderValueTooPrecise",
        "linePercentageAboveMax",
        "lineValueTooPrecise",
    ] {
        assert_eq!(actual[alias], expected[alias], "{alias}");
    }
}

fn assert_discount_validation_accepted_aliases(
    actual: &Value,
    expected: &Value,
    resource_name: &str,
) {
    for alias in [
        "orderPercentageNegative",
        "linePercentageNegative",
        "validOrderPercentage",
        "validLinePercentage",
    ] {
        assert_eq!(actual[alias]["userErrors"], json!([]), "{alias}");
        assert_eq!(expected[alias]["userErrors"], json!([]), "{alias}");
        assert!(
            actual[alias][resource_name].is_object(),
            "{alias} should return {resource_name}"
        );
    }
}

#[test]
fn draft_order_applied_discount_validation_replays_captured_shapes() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/orders/draftOrder-applied-discount-validation.json"
    ))
    .unwrap();
    let mut proxy = snapshot_proxy();
    restore_shop_currency(&mut proxy, "CAD");

    let create = proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/orders/draftOrder-applied-discount-validation-create.graphql"
        ),
        fixture["createValidation"]["variables"].clone(),
    ));
    assert_eq!(create.status, 200);
    let create_data = &create.body["data"];
    let expected_create = &fixture["createValidation"]["response"]["data"];
    assert_discount_validation_rejected_aliases(create_data, expected_create);
    assert_discount_validation_accepted_aliases(create_data, expected_create, "draftOrder");
    assert_eq!(
        without_id_fields(&create_data["validOrderPercentage"]),
        without_id_fields(&expected_create["validOrderPercentage"])
    );
    assert_eq!(
        without_id_fields(&create_data["validLinePercentage"]),
        without_id_fields(&expected_create["validLinePercentage"])
    );

    let setup = proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/orders/draftOrder-applied-discount-validation-setup.graphql"
        ),
        fixture["setupCreate"]["variables"].clone(),
    ));
    assert_eq!(setup.status, 200);
    assert_eq!(
        setup.body["data"]["draftOrderCreate"]["userErrors"],
        json!([])
    );
    let draft_id = setup.body["data"]["draftOrderCreate"]["draftOrder"]["id"].clone();
    let mut update_variables = fixture["updateValidation"]["variables"].clone();
    update_variables["id"] = draft_id;
    let update = proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/orders/draftOrder-applied-discount-validation-update.graphql"
        ),
        update_variables,
    ));
    assert_eq!(update.status, 200);
    let update_data = &update.body["data"];
    let expected_update = &fixture["updateValidation"]["response"]["data"];
    assert_discount_validation_rejected_aliases(update_data, expected_update);
    assert_discount_validation_accepted_aliases(update_data, expected_update, "draftOrder");
    assert_eq!(
        without_id_fields(&update_data["validOrderPercentage"]),
        without_id_fields(&expected_update["validOrderPercentage"])
    );
    assert_eq!(
        update_data["validLinePercentage"]["draftOrder"]["lineItems"],
        expected_update["validLinePercentage"]["draftOrder"]["lineItems"]
    );

    let calculate = proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/orders/draftOrder-applied-discount-validation-calculate.graphql"
        ),
        fixture["calculateValidation"]["variables"].clone(),
    ));
    assert_eq!(calculate.status, 200);
    let calculate_data = &calculate.body["data"];
    let expected_calculate = &fixture["calculateValidation"]["response"]["data"];
    assert_discount_validation_rejected_aliases(calculate_data, expected_calculate);
    assert_discount_validation_accepted_aliases(
        calculate_data,
        expected_calculate,
        "calculatedDraftOrder",
    );
    assert_eq!(
        calculate_data["validOrderPercentage"],
        expected_calculate["validOrderPercentage"]
    );
    assert_eq!(
        calculate_data["validLinePercentage"],
        expected_calculate["validLinePercentage"]
    );
}

#[test]
fn draft_order_applied_discount_value_type_coercion_matches_capture() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/orders/draftOrder-applied-discount-validation.json"
    ))
    .unwrap();
    let mut proxy = snapshot_proxy();

    let response = proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/orders/draftOrder-applied-discount-value-type-required.graphql"
        ),
        fixture["missingValueTypeValidation"]["variables"].clone(),
    ));
    assert_eq!(response.status, 200);
    assert_eq!(
        response.body["errors"],
        fixture["missingValueTypeValidation"]["response"]["errors"]
    );
    assert!(response.body.get("data").is_none());
}

#[test]
fn payment_reminder_send_malformed_gid_and_invalid_selection_covers_current_guardrails() {
    let malformed_fixture: Value = serde_json::from_str(include_str!(
        "../../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/payments/payment-reminder-send-malformed-gid.json"
    ))
    .unwrap();
    let mut proxy = snapshot_proxy();
    let malformed_query = include_str!(
        "../../config/parity-requests/payments/payment-reminder-send-malformed-gid.graphql"
    );

    for index in 0..3 {
        let response = proxy.process_request(json_graphql_request(
            malformed_query,
            malformed_fixture["cases"][index]["request"]["variables"].clone(),
        ));
        assert_eq!(response.status, 200);
        assert_eq!(
            without_extensions(&response.body),
            without_extensions(&malformed_fixture["cases"][index]["response"]["payload"])
        );
    }

    let sibling_abort_query = r#"
      mutation PaymentReminderSendMalformedGid($paymentScheduleId: ID!) {
        paymentReminderSend(paymentScheduleId: $paymentScheduleId) {
          success
          userErrors { field code message }
        }
        paymentCustomizationCreate(paymentCustomization: { title: "Should not stage", enabled: true, functionId: "gid://shopify/ShopifyFunction/payment-a", metafields: [] }) {
          paymentCustomization { id }
          userErrors { field code message }
        }
      }
    "#;
    let sibling_abort = proxy.process_request(json_graphql_request(
        sibling_abort_query,
        json!({ "paymentScheduleId": "not-a-gid" }),
    ));
    assert_eq!(sibling_abort.status, 200);
    assert!(sibling_abort.body.get("data").is_none());
    assert!(sibling_abort.body.to_string().contains("INVALID_VARIABLE"));
    assert!(!sibling_abort
        .body
        .to_string()
        .contains("paymentCustomizationCreate"));

    let invalid_selection = proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/payments/payment-reminder-send-invalid-field.graphql"
        ),
        json!({ "paymentScheduleId": "gid://shopify/PaymentSchedule/shape" }),
    ));
    assert_eq!(invalid_selection.status, 200);
    assert_eq!(
        invalid_selection.body,
        json!({
            "errors": [{
                "message": "Field 'customerPaymentMethod' doesn't exist on type 'PaymentReminderSendPayload'",
                "locations": [{ "line": 3, "column": 5 }],
                "path": [
                    "mutation PaymentReminderSendInvalidField",
                    "paymentReminderSend",
                    "customerPaymentMethod"
                ],
                "extensions": {
                    "code": "undefinedField",
                    "typeName": "PaymentReminderSendPayload",
                    "fieldName": "customerPaymentMethod"
                }
            }]
        })
    );
}

#[test]
fn payment_reminder_send_eligibility_and_rate_limit_covers_current_guardrails() {
    let eligibility_fixture: Value = serde_json::from_str(include_str!(
        "../../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/payments/payment-reminder-send-eligibility.json"
    ))
    .unwrap();
    let additional_fixture: Value = serde_json::from_str(include_str!(
        "../../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/payments/payment-reminder-send-additional-guards.json"
    ))
    .unwrap();
    let (mut proxy, remaining_upstream_calls) =
        payment_reminder_hydrated_proxy(&[&eligibility_fixture, &additional_fixture]);
    let reminder_query =
        include_str!("../../config/parity-requests/payments/payment-reminder-send.graphql");

    for case_name in ["success", "unknown", "paid"] {
        let response = proxy.process_request(json_graphql_request(
            reminder_query,
            eligibility_fixture["cases"][case_name]["request"]["variables"].clone(),
        ));
        assert_eq!(response.status, 200);
        assert_eq!(
            without_extensions(&response.body),
            without_extensions(&eligibility_fixture["cases"][case_name]["response"])
        );
    }

    let missing_email = proxy.process_request(json_graphql_request(
        reminder_query,
        additional_fixture["cases"]["missingEmail"]["request"]["variables"].clone(),
    ));
    assert_eq!(missing_email.status, 200);
    assert_eq!(
        without_extensions(&missing_email.body),
        without_extensions(&additional_fixture["cases"]["missingEmail"]["response"])
    );

    let rate_first = proxy.process_request(json_graphql_request(
        reminder_query,
        additional_fixture["cases"]["rateFirst"]["request"]["variables"].clone(),
    ));
    assert_eq!(rate_first.status, 200);
    assert_eq!(
        without_extensions(&rate_first.body),
        without_extensions(&additional_fixture["cases"]["rateFirst"]["response"])
    );

    let rate_second = proxy.process_request(json_graphql_request(
        reminder_query,
        additional_fixture["cases"]["rateSecond"]["request"]["variables"].clone(),
    ));
    assert_eq!(rate_second.status, 200);
    assert_eq!(
        without_extensions(&rate_second.body),
        without_extensions(&additional_fixture["cases"]["rateSecond"]["response"])
    );

    assert_eq!(remaining_upstream_calls.lock().unwrap().len(), 0);
}

#[test]
fn payment_reminder_send_resolves_staged_payment_terms_schedule_guardrails() {
    let mut proxy = snapshot_proxy();
    let query = include_str!("../../config/parity-requests/payments/payment-reminder-send.graphql");

    // Exercise generated schedule IDs earned through public GraphQL mutations;
    // the removed literal-GID table could not handle this path.
    let (_success_order, success_schedule) =
        stage_reminder_order_payment_schedule(&mut proxy, Some("reminder-success@example.test"));
    let first_send = proxy.process_request(json_graphql_request(
        query,
        json!({ "paymentScheduleId": success_schedule.clone() }),
    ));
    assert_eq!(first_send.status, 200);
    assert_eq!(
        first_send.body,
        json!({ "data": { "paymentReminderSend": { "success": true, "userErrors": [] } } })
    );

    let second_send = proxy.process_request(json_graphql_request(
        query,
        json!({ "paymentScheduleId": success_schedule }),
    ));
    assert_eq!(second_send.status, 200);
    assert_eq!(
        second_send.body,
        json!({
            "data": {
                "paymentReminderSend": payment_reminder_error("You cannot send more than 1 payment reminders for the same order in a 24hour period")
            }
        })
    );

    let unknown_schedule = proxy.process_request(json_graphql_request(
        query,
        json!({ "paymentScheduleId": "gid://shopify/PaymentSchedule/999999" }),
    ));
    assert_eq!(
        unknown_schedule.body,
        json!({ "data": { "paymentReminderSend": payment_reminder_error("Payment schedule does not exist") } })
    );

    let (_missing_email_order, missing_email_schedule) =
        stage_reminder_order_payment_schedule(&mut proxy, Some("   "));
    let missing_email = proxy.process_request(json_graphql_request(
        query,
        json!({ "paymentScheduleId": missing_email_schedule }),
    ));
    assert_eq!(
        missing_email.body,
        json!({ "data": { "paymentReminderSend": payment_reminder_error("Order does not have a contact email") } })
    );

    let (paid_order, paid_schedule) =
        stage_reminder_order_payment_schedule(&mut proxy, Some("reminder-paid@example.test"));
    mark_reminder_order_paid(&mut proxy, paid_order);
    let paid = proxy.process_request(json_graphql_request(
        query,
        json!({ "paymentScheduleId": paid_schedule }),
    ));
    assert_eq!(
        paid.body,
        json!({ "data": { "paymentReminderSend": payment_reminder_error("Payment schedule is already completed") } })
    );

    let (closed_order, closed_schedule) =
        stage_reminder_order_payment_schedule(&mut proxy, Some("reminder-closed@example.test"));
    close_reminder_order(&mut proxy, closed_order);
    let closed = proxy.process_request(json_graphql_request(
        query,
        json!({ "paymentScheduleId": closed_schedule }),
    ));
    assert_eq!(
        closed.body,
        json!({ "data": { "paymentReminderSend": payment_reminder_error("Payment reminder could not be sent") } })
    );

    let draft_schedule = stage_reminder_draft_payment_schedule(&mut proxy);
    let draft = proxy.process_request(json_graphql_request(
        query,
        json!({ "paymentScheduleId": draft_schedule }),
    ));
    assert_eq!(
        draft.body,
        json!({ "data": { "paymentReminderSend": payment_reminder_error("Payment schedule is not for an Order") } })
    );
}

fn stage_reminder_order_payment_schedule(
    proxy: &mut DraftProxy,
    email: Option<&str>,
) -> (Value, Value) {
    let line_item = json!({
        "title": "Reminder order item",
        "quantity": 1,
        "priceSet": {
            "shopMoney": { "amount": "10.00", "currencyCode": "CAD" },
            "presentmentMoney": { "amount": "10.00", "currencyCode": "CAD" }
        },
        "taxable": false
    });
    let mut order = json!({
        "currency": "CAD",
        "presentmentCurrency": "CAD",
        "financialStatus": "PENDING",
        "lineItems": [line_item]
    });
    if let Some(email) = email {
        order["email"] = json!(email);
    }
    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation CreatePaymentReminderOrder($order: OrderCreateOrderInput!) {
          orderCreate(order: $order) {
            order { id displayFinancialStatus email }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "order": order }),
    ));
    assert_eq!(create.body["data"]["orderCreate"]["userErrors"], json!([]));
    let order_id = create.body["data"]["orderCreate"]["order"]["id"].clone();
    let schedule_id = stage_reminder_payment_terms(proxy, order_id.clone());
    (order_id, schedule_id)
}

fn stage_reminder_draft_payment_schedule(proxy: &mut DraftProxy) -> Value {
    let mut draft = Value::Null;
    for index in 0..6 {
        draft = create_payment_terms_test_draft(
            proxy,
            &format!("payment-reminder-draft-{index}@example.test"),
        );
    }
    stage_reminder_payment_terms(proxy, draft["id"].clone())
}

fn stage_reminder_payment_terms(proxy: &mut DraftProxy, owner_id: Value) -> Value {
    let create_terms = proxy.process_request(json_graphql_request(
        r#"
        mutation CreatePaymentReminderTerms($referenceId: ID!, $attrs: PaymentTermsCreateInput!) {
          paymentTermsCreate(referenceId: $referenceId, paymentTermsAttributes: $attrs) {
            paymentTerms {
              id
              overdue
              paymentSchedules(first: 1) {
                nodes { id due dueAt completedAt }
              }
            }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "referenceId": owner_id,
            "attrs": {
                "paymentTermsTemplateId": "gid://shopify/PaymentTermsTemplate/4",
                "paymentSchedules": [{ "issuedAt": "2026-05-05T00:00:00Z" }]
            }
        }),
    ));
    assert_eq!(
        create_terms.body["data"]["paymentTermsCreate"]["userErrors"],
        json!([])
    );
    create_terms.body["data"]["paymentTermsCreate"]["paymentTerms"]["paymentSchedules"]["nodes"][0]
        ["id"]
        .clone()
}

fn mark_reminder_order_paid(proxy: &mut DraftProxy, order_id: Value) {
    let mark = proxy.process_request(json_graphql_request(
        r#"
        mutation MarkPaymentReminderOrderPaid($input: OrderMarkAsPaidInput!) {
          orderMarkAsPaid(input: $input) {
            order { id displayFinancialStatus }
            userErrors { field message }
          }
        }
        "#,
        json!({ "input": { "id": order_id } }),
    ));
    assert_eq!(
        mark.body["data"]["orderMarkAsPaid"]["userErrors"],
        json!([])
    );
    assert_eq!(
        mark.body["data"]["orderMarkAsPaid"]["order"]["displayFinancialStatus"],
        json!("PAID")
    );
}

fn close_reminder_order(proxy: &mut DraftProxy, order_id: Value) {
    let close = proxy.process_request(json_graphql_request(
        r#"
        mutation ClosePaymentReminderOrder($input: OrderCloseInput!) {
          orderClose(input: $input) {
            order { id closed closedAt }
            userErrors { field message }
          }
        }
        "#,
        json!({ "input": { "id": order_id } }),
    ));
    assert_eq!(close.body["data"]["orderClose"]["userErrors"], json!([]));
    assert_eq!(
        close.body["data"]["orderClose"]["order"]["closed"],
        json!(true)
    );
}

fn payment_reminder_error(message: &str) -> Value {
    json!({
        "success": null,
        "userErrors": [{
            "field": null,
            "message": message,
            "code": "PAYMENT_REMINDER_SEND_UNSUCCESSFUL"
        }]
    })
}

fn payment_reminder_hydrated_proxy(fixtures: &[&Value]) -> (DraftProxy, Arc<Mutex<Vec<Value>>>) {
    let upstream_calls = Arc::new(Mutex::new(
        fixtures
            .iter()
            .flat_map(|fixture| {
                fixture["upstreamCalls"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default()
            })
            .collect::<Vec<_>>(),
    ));
    let transport_calls = Arc::clone(&upstream_calls);
    let proxy =
        configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport(move |request| {
            let request_body: Value =
                serde_json::from_str(&request.body).expect("payment reminder hydrate body parses");
            let mut calls = transport_calls.lock().unwrap();
            let index = calls
                .iter()
                .position(|call| {
                    call["query"] == request_body["query"]
                        && call["variables"] == request_body["variables"]
                })
                .unwrap_or_else(|| {
                    panic!(
                        "missing payment reminder hydrate cassette for request: {}",
                        request_body
                    )
                });
            let call = calls.remove(index);
            Response {
                status: call["response"]["status"].as_u64().unwrap_or(200) as u16,
                headers: Default::default(),
                body: call["response"]["body"].clone(),
            }
        });
    (proxy, upstream_calls)
}

fn payment_customization_function_metadata(id: &str, handle: &str) -> Value {
    json!({
        "id": id,
        "title": handle,
        "handle": handle,
        "apiType": "payment_customization",
        "description": format!("{handle} fixture function"),
        "appKey": "347082227713",
        "app": {
            "__typename": "App",
            "id": "gid://shopify/App/347082227713",
            "title": "Payment customization fixture app",
            "handle": "payment-customization-fixture-app",
            "apiKey": "347082227713"
        }
    })
}

fn payment_customization_function_proxy(
    functions: Vec<Value>,
    hits: Arc<Mutex<Vec<Value>>>,
) -> DraftProxy {
    configured_proxy(
        ReadMode::LiveHybrid,
        Some(shopify_draft_proxy::proxy::UnsupportedMutationMode::Passthrough),
    )
    .with_upstream_transport(move |request| {
        let body: Value = serde_json::from_str(&request.body)
            .expect("payment customization function hydrate body should parse");
        hits.lock().unwrap().push(body.clone());
        let response_body = match body["operationName"].as_str().unwrap_or_default() {
            "FunctionHydrateByHandle" => {
                let handle = body["variables"]["handle"].as_str().unwrap_or_default();
                let nodes = functions
                    .iter()
                    .filter(|function| {
                        function["handle"].as_str() == Some(handle)
                            || function["title"].as_str() == Some(handle)
                            || function["description"].as_str() == Some(handle)
                    })
                    .cloned()
                    .collect::<Vec<_>>();
                json!({ "data": { "shopifyFunctions": { "nodes": nodes } } })
            }
            "FunctionHydrateById" => {
                let id = body["variables"]["id"].as_str().unwrap_or_default();
                let function = functions
                    .iter()
                    .find(|function| function["id"].as_str() == Some(id))
                    .cloned()
                    .unwrap_or(Value::Null);
                json!({ "data": { "shopifyFunction": function } })
            }
            "PaymentCustomizationHydrateById" => {
                json!({ "data": { "paymentCustomization": Value::Null } })
            }
            "PaymentCustomizationHydrateCatalog" => {
                json!({ "data": { "paymentCustomizations": { "nodes": [] } } })
            }
            _ => json!({
                "errors": [{
                    "message": format!("unexpected payment customization upstream request: {body}")
                }]
            }),
        };
        Response {
            status: 200,
            headers: Default::default(),
            body: response_body,
        }
    })
}

fn base_payment_customization_record(id: &str, title: &str, enabled: bool) -> Value {
    json!({
        "__typename": "PaymentCustomization",
        "id": id,
        "legacyResourceId": id.rsplit('/').next().unwrap_or_default(),
        "title": title,
        "enabled": enabled,
        "functionId": "gid://shopify/ShopifyFunction/payment-a",
        "functionHandle": Value::Null,
        "shopifyFunction": Value::Null,
        "errorHistory": { "nodes": [] },
        "metafields": {
            "edges": [{
                "node": {
                    "id": "gid://shopify/Metafield/payment-customization-base",
                    "namespace": "app--347082227713--foo",
                    "key": "bar",
                    "type": "single_line_text_field",
                    "value": "base",
                    "createdAt": "2026-07-01T00:00:00Z",
                    "updatedAt": "2026-07-01T00:00:00Z"
                }
            }]
        }
    })
}

fn payment_customization_base_hydration_proxy(
    base_records: Vec<Value>,
    hits: Arc<Mutex<Vec<Value>>>,
) -> DraftProxy {
    let base_records = Arc::new(base_records);
    configured_proxy(
        ReadMode::LiveHybrid,
        Some(shopify_draft_proxy::proxy::UnsupportedMutationMode::Passthrough),
    )
    .with_upstream_transport(move |request| {
        let body: Value = serde_json::from_str(&request.body)
            .expect("payment customization hydrate body should parse");
        hits.lock().unwrap().push(body.clone());
        let response_body = match body["operationName"].as_str().unwrap_or_default() {
            "PaymentCustomizationHydrateById" => {
                let id = body["variables"]["id"].as_str().unwrap_or_default();
                let record = base_records
                    .iter()
                    .find(|record| record["id"].as_str() == Some(id))
                    .cloned()
                    .unwrap_or(Value::Null);
                json!({ "data": { "paymentCustomization": record } })
            }
            "PaymentCustomizationHydrateCatalog" => json!({
                "data": {
                    "paymentCustomizations": {
                        "nodes": base_records.as_ref().clone()
                    }
                }
            }),
            _ => json!({
                "errors": [{
                    "message": format!("unexpected payment customization upstream request: {body}")
                }]
            }),
        };
        Response {
            status: 200,
            headers: Default::default(),
            body: response_body,
        }
    })
}

#[test]
fn payment_customization_mutation_first_hydrates_base_state() {
    let target_id = "gid://shopify/PaymentCustomization/4242";
    let other_id = "gid://shopify/PaymentCustomization/4243";
    let upstream_hits = Arc::new(Mutex::new(Vec::<Value>::new()));
    let mut proxy = payment_customization_base_hydration_proxy(
        vec![
            base_payment_customization_record(target_id, "Hydrated before update", true),
            base_payment_customization_record(other_id, "Hydrated catalog sibling", true),
        ],
        Arc::clone(&upstream_hits),
    );

    let update = proxy.process_request(json_graphql_request(
        r#"
        mutation PaymentCustomizationMutationFirstUpdate($id: ID!, $input: PaymentCustomizationInput!) {
          paymentCustomizationUpdate(id: $id, paymentCustomization: $input) {
            paymentCustomization {
              id
              title
              enabled
              functionId
              metafield(namespace: "$app:foo", key: "bar") { namespace key type value updatedAt }
            }
            userErrors { field code message }
          }
        }
        "#,
        json!({
            "id": target_id,
            "input": {
                "title": "Updated without pre-read",
                "enabled": false,
                "functionId": "gid://shopify/ShopifyFunction/payment-a",
                "metafields": [{
                    "namespace": "$app:foo",
                    "key": "bar",
                    "type": "single_line_text_field",
                    "value": "updated"
                }]
            }
        }),
    ));
    assert_eq!(update.status, 200);
    assert_eq!(
        update.body["data"]["paymentCustomizationUpdate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        update.body["data"]["paymentCustomizationUpdate"]["paymentCustomization"]["title"],
        json!("Updated without pre-read")
    );
    assert_eq!(
        update.body["data"]["paymentCustomizationUpdate"]["paymentCustomization"]["metafield"]
            ["value"],
        json!("updated")
    );

    let activation = proxy.process_request(json_graphql_request(
        r#"
        mutation PaymentCustomizationMutationFirstActivation($ids: [ID!]!, $enabled: Boolean!) {
          paymentCustomizationActivation(ids: $ids, enabled: $enabled) {
            ids
            userErrors { field code message }
          }
        }
        "#,
        json!({ "ids": [target_id, other_id], "enabled": true }),
    ));
    assert_eq!(activation.status, 200);
    assert_eq!(
        activation.body["data"]["paymentCustomizationActivation"],
        json!({ "ids": [target_id, other_id], "userErrors": [] })
    );

    let catalog = proxy.process_request(json_graphql_request(
        r#"
        query PaymentCustomizationMutationFirstCatalog {
          paymentCustomizations(first: 10) {
            nodes { id title enabled }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(catalog.status, 200);
    assert_eq!(
        catalog.body["data"]["paymentCustomizations"]["nodes"],
        json!([
            { "id": target_id, "title": "Updated without pre-read", "enabled": true },
            { "id": other_id, "title": "Hydrated catalog sibling", "enabled": true }
        ])
    );

    let delete = proxy.process_request(json_graphql_request(
        r#"
        mutation PaymentCustomizationMutationFirstDelete($id: ID!) {
          paymentCustomizationDelete(id: $id) {
            deletedId
            userErrors { field code message }
          }
        }
        "#,
        json!({ "id": other_id }),
    ));
    assert_eq!(delete.status, 200);
    assert_eq!(
        delete.body["data"]["paymentCustomizationDelete"],
        json!({ "deletedId": other_id, "userErrors": [] })
    );

    let read_deleted = proxy.process_request(json_graphql_request(
        r#"
        query PaymentCustomizationMutationFirstReadDeleted($id: ID!) {
          paymentCustomization(id: $id) { id title enabled }
        }
        "#,
        json!({ "id": other_id }),
    ));
    assert_eq!(read_deleted.status, 200);
    assert_eq!(
        read_deleted.body["data"]["paymentCustomization"],
        Value::Null
    );

    let log = log_snapshot(&proxy);
    assert_eq!(log["entries"].as_array().unwrap().len(), 3);
    let upstream_operations = upstream_hits
        .lock()
        .unwrap()
        .iter()
        .map(|body| {
            body["operationName"]
                .as_str()
                .unwrap_or_default()
                .to_string()
        })
        .collect::<Vec<_>>();
    assert_eq!(
        upstream_operations,
        vec![
            "PaymentCustomizationHydrateById",
            "PaymentCustomizationHydrateCatalog"
        ]
    );
}

#[test]
fn payment_customization_local_runtime_covers_create_activation_update_readback_helpers() {
    let upstream_hits = Arc::new(Mutex::new(Vec::<Value>::new()));
    let mut proxy = payment_customization_function_proxy(
        vec![payment_customization_function_metadata(
            "gid://shopify/ShopifyFunction/payment-a",
            "payment-a",
        )],
        Arc::clone(&upstream_hits),
    );
    let create_query = r#"
      mutation RustPaymentCustomizationLocalRuntime($input: PaymentCustomizationInput!) {
        paymentCustomizationCreate(paymentCustomization: $input) {
          paymentCustomization {
            id
            title
            enabled
            functionId
            metafields(first: 5) { edges { node { namespace key type value } } }
          }
          userErrors { field code message }
        }
      }
    "#;
    let app_request = |query: &str, variables: serde_json::Value| {
        let mut request = json_graphql_request(query, variables);
        request.headers.insert(
            "x-shopify-draft-proxy-api-client-id".to_string(),
            "347082227713".to_string(),
        );
        request
    };

    let missing_title = proxy.process_request(json_graphql_request(
        create_query,
        json!({ "input": { "enabled": true, "functionId": "gid://shopify/ShopifyFunction/payment-a" } }),
    ));
    assert_eq!(missing_title.status, 200);
    assert_eq!(
        missing_title.body["data"]["paymentCustomizationCreate"]["paymentCustomization"],
        Value::Null
    );
    assert_eq!(
        missing_title.body["data"]["paymentCustomizationCreate"]["userErrors"],
        json!([{
            "field": ["paymentCustomization", "title"],
            "code": "REQUIRED_INPUT_FIELD",
            "message": "Required input field must be present."
        }])
    );

    let blank_title = proxy.process_request(json_graphql_request(
        create_query,
        json!({ "input": { "title": " ", "enabled": true, "functionId": "gid://shopify/ShopifyFunction/payment-a" } }),
    ));
    assert_eq!(blank_title.status, 200);
    assert_eq!(
        blank_title.body["data"]["paymentCustomizationCreate"]["paymentCustomization"],
        Value::Null
    );
    assert_eq!(
        blank_title.body["data"]["paymentCustomizationCreate"]["userErrors"],
        json!([{
            "field": ["paymentCustomization", "title"],
            "code": "REQUIRED_INPUT_FIELD",
            "message": "Required input field must be present."
        }])
    );

    let missing_title_and_enabled = proxy.process_request(json_graphql_request(
        create_query,
        json!({ "input": { "functionId": "gid://shopify/ShopifyFunction/payment-a" } }),
    ));
    assert_eq!(missing_title_and_enabled.status, 200);
    assert_eq!(
        missing_title_and_enabled.body["data"]["paymentCustomizationCreate"]
            ["paymentCustomization"],
        Value::Null
    );
    assert_eq!(
        missing_title_and_enabled.body["data"]["paymentCustomizationCreate"]["userErrors"],
        json!([
            {
                "field": ["paymentCustomization", "title"],
                "code": "REQUIRED_INPUT_FIELD",
                "message": "Required input field must be present."
            },
            {
                "field": ["paymentCustomization", "enabled"],
                "code": "REQUIRED_INPUT_FIELD",
                "message": "Required input field must be present."
            }
        ])
    );

    let missing_metafields = proxy.process_request(json_graphql_request(
        create_query,
        json!({ "input": { "title": "Missing metafields", "enabled": true, "functionId": "gid://shopify/ShopifyFunction/payment-a" } }),
    ));
    assert_eq!(missing_metafields.status, 200);
    let missing_metafields_payload = &missing_metafields.body["data"]["paymentCustomizationCreate"];
    assert_eq!(missing_metafields_payload["userErrors"], json!([]));
    assert_eq!(
        missing_metafields_payload["paymentCustomization"]["id"],
        json!("gid://shopify/PaymentCustomization/1")
    );
    assert_eq!(
        missing_metafields_payload["paymentCustomization"]["metafields"]["edges"],
        json!([])
    );

    let both_identifiers = proxy.process_request(json_graphql_request(
        create_query,
        json!({ "input": { "title": "Both identifiers", "enabled": true, "functionId": "gid://shopify/ShopifyFunction/payment-a", "functionHandle": "payment-a", "metafields": [] } }),
    ));
    assert_eq!(both_identifiers.status, 200);
    assert_eq!(
        both_identifiers.body["data"]["paymentCustomizationCreate"]["paymentCustomization"],
        Value::Null
    );
    assert_eq!(
        both_identifiers.body["data"]["paymentCustomizationCreate"]["userErrors"][0]["code"],
        json!("MULTIPLE_FUNCTION_IDENTIFIERS")
    );

    let missing_identifier = proxy.process_request(json_graphql_request(
        create_query,
        json!({ "input": { "title": "Missing identifier", "enabled": true, "metafields": [] } }),
    ));
    assert_eq!(missing_identifier.status, 200);
    assert_eq!(
        missing_identifier.body["data"]["paymentCustomizationCreate"]["userErrors"][0]["code"],
        json!("MISSING_FUNCTION_IDENTIFIER")
    );

    let mut legacy_missing_identifier_request = json_graphql_request(
        create_query,
        json!({ "input": { "title": "Missing identifier", "enabled": true } }),
    );
    legacy_missing_identifier_request.path = "/admin/api/2025-01/graphql.json".to_string();
    let legacy_missing_identifier = proxy.process_request(legacy_missing_identifier_request);
    assert_eq!(
        legacy_missing_identifier.body["data"]["paymentCustomizationCreate"]["userErrors"],
        json!([{
            "field": ["paymentCustomization", "functionId"],
            "code": "REQUIRED_INPUT_FIELD",
            "message": "Required input field must be present."
        }])
    );

    let invalid_metafield = proxy.process_request(app_request(
        create_query,
        json!({ "input": { "title": "Invalid metafield", "enabled": true, "functionId": "gid://shopify/ShopifyFunction/payment-a", "metafields": [{ "namespace": "$app:foo", "key": "bar", "value": "baz" }] } }),
    ));
    assert_eq!(invalid_metafield.status, 200);
    assert_eq!(
        invalid_metafield.body["data"]["paymentCustomizationCreate"]["paymentCustomization"],
        Value::Null
    );
    assert_eq!(
        invalid_metafield.body["data"]["paymentCustomizationCreate"]["userErrors"][0],
        json!({
            "field": ["paymentCustomization", "metafields", "0", "type"],
            "code": "INVALID_METAFIELDS",
            "message": "can't be blank"
        })
    );

    let invalid_metafields = proxy.process_request(json_graphql_request(
        create_query,
        json!({
            "input": {
                "title": "Invalid metafields",
                "enabled": true,
                "functionId": "gid://shopify/ShopifyFunction/payment-a",
                "metafields": [
                    { "namespace": "every other field missing" },
                    { "key": "every other field missing" },
                    { "namespace": "ab", "key": "present", "type": "", "value": "present" }
                ]
            }
        }),
    ));
    assert_eq!(invalid_metafields.status, 200);
    assert_eq!(
        invalid_metafields.body["data"]["paymentCustomizationCreate"]["paymentCustomization"],
        Value::Null
    );
    assert_eq!(
        invalid_metafields.body["data"]["paymentCustomizationCreate"]["userErrors"],
        json!([
            {
                "field": ["paymentCustomization", "metafields", "0", "key"],
                "code": "INVALID_METAFIELDS",
                "message": "may not be empty"
            },
            {
                "field": ["paymentCustomization", "metafields", "0", "value"],
                "code": "INVALID_METAFIELDS",
                "message": "may not be empty"
            },
            {
                "field": ["paymentCustomization", "metafields", "1", "value"],
                "code": "INVALID_METAFIELDS",
                "message": "may not be empty"
            },
            {
                "field": ["paymentCustomization", "metafields", "2", "type"],
                "code": "INVALID_METAFIELDS",
                "message": "can't be blank"
            },
            {
                "field": ["paymentCustomization", "metafields", "2", "namespace"],
                "code": "INVALID_METAFIELDS",
                "message": "is too short (minimum is 3 characters)"
            }
        ])
    );

    let before = proxy.process_request(app_request(
        create_query,
        json!({
            "input": {
                "title": "Before",
                "enabled": true,
                "functionId": "gid://shopify/ShopifyFunction/payment-a",
                "metafields": [{ "namespace": "$app:foo", "key": "bar", "type": "single_line_text_field", "value": "baz" }]
            }
        }),
    ));
    assert_eq!(before.status, 200);
    let customization_id = before.body["data"]["paymentCustomizationCreate"]
        ["paymentCustomization"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(customization_id, "gid://shopify/PaymentCustomization/2");
    assert_eq!(
        before.body["data"]["paymentCustomizationCreate"]["paymentCustomization"]["metafields"]
            ["edges"][0]["node"],
        json!({
            "namespace": "app--347082227713--foo",
            "key": "bar",
            "type": "single_line_text_field",
            "value": "baz"
        })
    );

    let update_query = r#"
      mutation RustPaymentCustomizationLocalRuntime($id: ID!, $input: PaymentCustomizationInput!) {
        paymentCustomizationUpdate(id: $id, paymentCustomization: $input) {
          paymentCustomization {
            id
            title
            enabled
            functionId
            metafield(namespace: "$app:foo", key: "bar") { namespace key type value }
            metafields(first: 5) { edges { node { namespace key type value } } }
          }
          userErrors { field code message }
        }
      }
    "#;
    let rejected_function_change = proxy.process_request(json_graphql_request(
        update_query,
        json!({ "id": customization_id, "input": { "functionId": "gid://shopify/ShopifyFunction/payment-b" } }),
    ));
    assert_eq!(rejected_function_change.status, 200);
    assert_eq!(
        rejected_function_change.body["data"]["paymentCustomizationUpdate"]["paymentCustomization"],
        Value::Null
    );
    assert_eq!(
        rejected_function_change.body["data"]["paymentCustomizationUpdate"]["userErrors"][0]
            ["code"],
        json!("FUNCTION_ID_CANNOT_BE_CHANGED")
    );

    let read_query = r#"
      query RustPaymentCustomizationLocalRuntime($id: ID!) {
        paymentCustomization(id: $id) {
          id
          title
          enabled
          functionId
          metafield(namespace: "$app:foo", key: "bar") { namespace key type value }
          metafields(first: 5) { edges { node { namespace key type value } } }
        }
      }
    "#;
    let read_after_rejected_update =
        proxy.process_request(app_request(read_query, json!({ "id": customization_id })));
    assert_eq!(read_after_rejected_update.status, 200);
    assert_eq!(
        read_after_rejected_update.body["data"]["paymentCustomization"]["title"],
        json!("Before")
    );
    assert_eq!(
        read_after_rejected_update.body["data"]["paymentCustomization"]["functionId"],
        json!("gid://shopify/ShopifyFunction/payment-a")
    );

    let rejected_metafield_update = proxy.process_request(json_graphql_request(
        update_query,
        json!({ "id": customization_id, "input": { "metafields": [{ "namespace": "ab", "key": "bar", "type": "single_line_text_field", "value": "qux" }] } }),
    ));
    assert_eq!(rejected_metafield_update.status, 200);
    assert_eq!(
        rejected_metafield_update.body["data"]["paymentCustomizationUpdate"]
            ["paymentCustomization"],
        Value::Null
    );
    assert_eq!(
        rejected_metafield_update.body["data"]["paymentCustomizationUpdate"]["userErrors"][0],
        json!({
            "field": ["paymentCustomization", "metafields", "0", "namespace"],
            "code": "INVALID_METAFIELDS",
            "message": "is too short (minimum is 3 characters)"
        })
    );
    let read_after_rejected_metafield_update =
        proxy.process_request(app_request(read_query, json!({ "id": customization_id })));
    assert_eq!(read_after_rejected_metafield_update.status, 200);
    assert_eq!(
        read_after_rejected_metafield_update.body["data"]["paymentCustomization"]["metafield"]
            ["value"],
        json!("baz")
    );

    let accepted_equivalent_handle = proxy.process_request(app_request(
        update_query,
        json!({
            "id": customization_id,
            "input": {
                "title": "After",
                "functionHandle": "payment-a",
                "metafields": [{ "namespace": "$app:foo", "key": "bar", "type": "single_line_text_field", "value": "qux" }]
            }
        }),
    ));
    assert_eq!(accepted_equivalent_handle.status, 200);
    assert_eq!(
        accepted_equivalent_handle.body["data"]["paymentCustomizationUpdate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        accepted_equivalent_handle.body["data"]["paymentCustomizationUpdate"]
            ["paymentCustomization"]["title"],
        json!("After")
    );
    assert_eq!(
        accepted_equivalent_handle.body["data"]["paymentCustomizationUpdate"]
            ["paymentCustomization"]["functionId"],
        json!("gid://shopify/ShopifyFunction/payment-a")
    );
    assert_eq!(
        accepted_equivalent_handle.body["data"]["paymentCustomizationUpdate"]
            ["paymentCustomization"]["metafield"]["value"],
        json!("qux")
    );

    let rejected_blank_title_update = proxy.process_request(json_graphql_request(
        update_query,
        json!({
            "id": customization_id,
            "input": {
                "title": " ",
                "functionId": "gid://shopify/ShopifyFunction/payment-a"
            }
        }),
    ));
    assert_eq!(rejected_blank_title_update.status, 200);
    assert_eq!(
        rejected_blank_title_update.body["data"]["paymentCustomizationUpdate"]
            ["paymentCustomization"],
        Value::Null
    );
    assert_eq!(
        rejected_blank_title_update.body["data"]["paymentCustomizationUpdate"]["userErrors"],
        json!([{
            "field": ["paymentCustomization", "title"],
            "code": "REQUIRED_INPUT_FIELD",
            "message": "Required input field must be present."
        }])
    );
    let read_after_rejected_blank_title =
        proxy.process_request(app_request(read_query, json!({ "id": customization_id })));
    assert_eq!(read_after_rejected_blank_title.status, 200);
    assert_eq!(
        read_after_rejected_blank_title.body["data"]["paymentCustomization"]["title"],
        json!("After")
    );

    let second = proxy.process_request(json_graphql_request(
        create_query,
        json!({ "input": { "title": "Payment customization 3", "enabled": true, "functionId": "gid://shopify/ShopifyFunction/payment-c", "metafields": [] } }),
    ));
    let second_id = second.body["data"]["paymentCustomizationCreate"]["paymentCustomization"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let activation_query = r#"
      mutation RustPaymentCustomizationLocalRuntime($ids: [ID!]!, $enabled: Boolean!) {
        paymentCustomizationActivation(ids: $ids, enabled: $enabled) {
          ids
          userErrors { field code message }
        }
      }
    "#;
    let activation = proxy.process_request(json_graphql_request(
        activation_query,
        json!({ "ids": [customization_id, second_id, "gid://shopify/PaymentCustomization/999"], "enabled": false }),
    ));
    assert_eq!(activation.status, 200);
    assert_eq!(
        activation.body["data"]["paymentCustomizationActivation"]["ids"],
        json!([
            "gid://shopify/PaymentCustomization/2",
            "gid://shopify/PaymentCustomization/3"
        ])
    );
    assert_eq!(
        activation.body["data"]["paymentCustomizationActivation"]["userErrors"][0]["code"],
        json!("PAYMENT_CUSTOMIZATION_NOT_FOUND")
    );
    assert_eq!(
        activation.body["data"]["paymentCustomizationActivation"]["userErrors"][0]["field"],
        json!(["ids"])
    );
    assert_eq!(
        activation.body["data"]["paymentCustomizationActivation"]["userErrors"][0]["message"],
        json!("Could not find payment customizations with IDs: gid://shopify/PaymentCustomization/999")
    );

    let repeated_activation = proxy.process_request(json_graphql_request(
        activation_query,
        json!({ "ids": ["gid://shopify/PaymentCustomization/2"], "enabled": false }),
    ));
    assert_eq!(repeated_activation.status, 200);
    assert_eq!(
        repeated_activation.body["data"]["paymentCustomizationActivation"],
        json!({ "ids": ["gid://shopify/PaymentCustomization/2"], "userErrors": [] })
    );

    let all_invalid_activation = proxy.process_request(json_graphql_request(
        activation_query,
        json!({ "ids": ["gid://shopify/PaymentCustomization/999"], "enabled": true }),
    ));
    assert_eq!(all_invalid_activation.status, 200);
    assert_eq!(
        all_invalid_activation.body["data"]["paymentCustomizationActivation"],
        json!({
            "ids": [],
            "userErrors": [{
                "field": ["ids"],
                "code": "PAYMENT_CUSTOMIZATION_NOT_FOUND",
                "message": "Could not find payment customizations with IDs: gid://shopify/PaymentCustomization/999"
            }]
        })
    );
    let upstream_operations = upstream_hits
        .lock()
        .unwrap()
        .iter()
        .map(|body| {
            body["operationName"]
                .as_str()
                .unwrap_or_default()
                .to_string()
        })
        .collect::<Vec<_>>();
    assert_eq!(
        upstream_operations,
        vec![
            "FunctionHydrateByHandle",
            "PaymentCustomizationHydrateById",
            "PaymentCustomizationHydrateCatalog",
            "PaymentCustomizationHydrateById"
        ]
    );
}

#[test]
fn payment_customization_metafield_timestamps_use_request_clock_on_create_update_and_readback() {
    let upstream_hits = Arc::new(Mutex::new(Vec::<Value>::new()));
    let clock = Arc::new(Mutex::new(utc_time(1_783_080_000)));
    let mut proxy = payment_customization_function_proxy(
        vec![payment_customization_function_metadata(
            "gid://shopify/ShopifyFunction/payment-a",
            "payment-a",
        )],
        Arc::clone(&upstream_hits),
    )
    .with_clock({
        let clock = Arc::clone(&clock);
        move || *clock.lock().unwrap()
    });
    let app_request = |query: &str, variables: serde_json::Value| {
        let mut request = json_graphql_request(query, variables);
        request.headers.insert(
            "x-shopify-draft-proxy-api-client-id".to_string(),
            "347082227713".to_string(),
        );
        request
    };

    let create = proxy.process_request(app_request(
        r#"
        mutation PaymentCustomizationMetafieldClockedCreate($input: PaymentCustomizationInput!) {
          paymentCustomizationCreate(paymentCustomization: $input) {
            paymentCustomization {
              id
              metafields(first: 5) {
                edges { node { namespace key type value createdAt updatedAt } }
              }
            }
            userErrors { field code message }
          }
        }
        "#,
        json!({
            "input": {
                "title": "Clocked payment customization",
                "enabled": true,
                "functionId": "gid://shopify/ShopifyFunction/payment-a",
                "metafields": [{
                    "namespace": "$app:foo",
                    "key": "bar",
                    "type": "single_line_text_field",
                    "value": "baz"
                }]
            }
        }),
    ));
    assert_eq!(create.status, 200);
    let create_payload = &create.body["data"]["paymentCustomizationCreate"];
    assert_eq!(create_payload["userErrors"], json!([]));
    let customization_id = create_payload["paymentCustomization"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let created_metafield =
        &create_payload["paymentCustomization"]["metafields"]["edges"][0]["node"];
    assert_eq!(
        created_metafield["createdAt"],
        json!("2026-07-03T12:00:00Z")
    );
    assert_eq!(
        created_metafield["updatedAt"],
        json!("2026-07-03T12:00:00Z")
    );

    set_clock(&clock, 1_783_166_400);
    let update = proxy.process_request(app_request(
        r#"
        mutation PaymentCustomizationMetafieldClockedUpdate($id: ID!, $input: PaymentCustomizationInput!) {
          paymentCustomizationUpdate(id: $id, paymentCustomization: $input) {
            paymentCustomization {
              id
              metafields(first: 5) {
                edges { node { namespace key type value createdAt updatedAt } }
              }
            }
            userErrors { field code message }
          }
        }
        "#,
        json!({
            "id": customization_id,
            "input": {
                "metafields": [{
                    "namespace": "$app:foo",
                    "key": "bar",
                    "type": "single_line_text_field",
                    "value": "qux"
                }]
            }
        }),
    ));
    assert_eq!(update.status, 200);
    let update_payload = &update.body["data"]["paymentCustomizationUpdate"];
    assert_eq!(update_payload["userErrors"], json!([]));
    let updated_metafield =
        &update_payload["paymentCustomization"]["metafields"]["edges"][0]["node"];
    assert_eq!(updated_metafield["value"], json!("qux"));
    assert_eq!(
        updated_metafield["createdAt"],
        json!("2026-07-03T12:00:00Z")
    );
    assert_eq!(
        updated_metafield["updatedAt"],
        json!("2026-07-04T12:00:00Z")
    );

    let read = proxy.process_request(app_request(
        r#"
        query PaymentCustomizationMetafieldClockedRead($id: ID!) {
          paymentCustomization(id: $id) {
            metafields(first: 5) {
              edges { node { namespace key type value createdAt updatedAt } }
            }
          }
        }
        "#,
        json!({ "id": update_payload["paymentCustomization"]["id"].clone() }),
    ));
    assert_eq!(read.status, 200);
    let read_metafield =
        &read.body["data"]["paymentCustomization"]["metafields"]["edges"][0]["node"];
    assert_eq!(read_metafield["value"], json!("qux"));
    assert_eq!(read_metafield["createdAt"], json!("2026-07-03T12:00:00Z"));
    assert_eq!(read_metafield["updatedAt"], json!("2026-07-04T12:00:00Z"));
}

#[test]
fn payment_customization_create_rejects_unknown_non_sentinel_function_handle() {
    let upstream_hits = Arc::new(Mutex::new(Vec::<Value>::new()));
    let mut proxy = payment_customization_function_proxy(
        vec![payment_customization_function_metadata(
            "gid://shopify/ShopifyFunction/catalog-payment-function",
            "catalog-payment-function",
        )],
        Arc::clone(&upstream_hits),
    );
    let create_query = r#"
      mutation PaymentCustomizationUnknownHandle($input: PaymentCustomizationInput!) {
        paymentCustomizationCreate(paymentCustomization: $input) {
          paymentCustomization { id title functionId }
          userErrors { field code message }
        }
      }
    "#;

    let response = proxy.process_request(json_graphql_request(
        create_query,
        json!({
            "input": {
                "title": "Unknown function",
                "enabled": true,
                "functionHandle": "definitely-absent-non-sentinel-payment-function",
                "metafields": []
            }
        }),
    ));

    assert_eq!(response.status, 200);
    assert_eq!(
        response.body["data"]["paymentCustomizationCreate"]["paymentCustomization"],
        Value::Null
    );
    assert_eq!(
        response.body["data"]["paymentCustomizationCreate"]["userErrors"],
        json!([{
            "field": ["paymentCustomization", "functionHandle"],
            "code": "FUNCTION_NOT_FOUND",
            "message": "Function definitely-absent-non-sentinel-payment-function not found. Ensure that it is released in the current app (gid://shopify/App/local), and that the app is installed."
        }])
    );
    let hits = upstream_hits.lock().unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(
        hits[0]["variables"]["handle"],
        json!("definitely-absent-non-sentinel-payment-function")
    );
}

#[test]
fn payment_customization_function_handle_id_equivalence_uses_function_catalog() {
    let upstream_hits = Arc::new(Mutex::new(Vec::<Value>::new()));
    let function = payment_customization_function_metadata(
        "gid://shopify/ShopifyFunction/non-conformance-payment-function-id",
        "non-conformance-payment-function",
    );
    let mut proxy =
        payment_customization_function_proxy(vec![function], Arc::clone(&upstream_hits));
    let create_query = r#"
      mutation PaymentCustomizationCreateByHandle($input: PaymentCustomizationInput!) {
        paymentCustomizationCreate(paymentCustomization: $input) {
          paymentCustomization { id title functionId }
          userErrors { field code message }
        }
      }
    "#;
    let create = proxy.process_request(json_graphql_request(
        create_query,
        json!({
            "input": {
                "title": "Known non-conformance function",
                "enabled": true,
                "functionHandle": "non-conformance-payment-function",
                "metafields": []
            }
        }),
    ));
    assert_eq!(create.status, 200);
    assert_eq!(
        create.body["data"]["paymentCustomizationCreate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        create.body["data"]["paymentCustomizationCreate"]["paymentCustomization"]["functionId"],
        json!("gid://shopify/ShopifyFunction/non-conformance-payment-function-id")
    );
    let customization_id = create.body["data"]["paymentCustomizationCreate"]
        ["paymentCustomization"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let update_query = r#"
      mutation PaymentCustomizationFunctionUpdate($id: ID!, $input: PaymentCustomizationInput!) {
        paymentCustomizationUpdate(id: $id, paymentCustomization: $input) {
          paymentCustomization { id title functionId }
          userErrors { field code message }
        }
      }
    "#;
    let equivalent_update = proxy.process_request(json_graphql_request(
        update_query,
        json!({
            "id": customization_id,
            "input": {
                "title": "Equivalent id update",
                "functionId": "gid://shopify/ShopifyFunction/non-conformance-payment-function-id"
            }
        }),
    ));
    assert_eq!(equivalent_update.status, 200);
    assert_eq!(
        equivalent_update.body["data"]["paymentCustomizationUpdate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        equivalent_update.body["data"]["paymentCustomizationUpdate"]["paymentCustomization"]
            ["title"],
        json!("Equivalent id update")
    );

    let rejected_update = proxy.process_request(json_graphql_request(
        update_query,
        json!({
            "id": customization_id,
            "input": {
                "functionId": "gid://shopify/ShopifyFunction/different-payment-function-id"
            }
        }),
    ));
    assert_eq!(rejected_update.status, 200);
    assert_eq!(
        rejected_update.body["data"]["paymentCustomizationUpdate"]["paymentCustomization"],
        Value::Null
    );
    assert_eq!(
        rejected_update.body["data"]["paymentCustomizationUpdate"]["userErrors"],
        json!([{
            "field": ["paymentCustomization", "functionId"],
            "code": "FUNCTION_ID_CANNOT_BE_CHANGED",
            "message": "Function ID cannot be changed."
        }])
    );

    let hits = upstream_hits.lock().unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(
        hits[0]["variables"]["handle"],
        json!("non-conformance-payment-function")
    );
}

#[test]
fn payment_customization_parity_fixtures_replay_validation_metafields_activation_and_immutable_paths(
) {
    let validation_fixture: Value = serde_json::from_str(include_str!(
        "../../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/payments/payment-customization-validation.json"
    ))
    .unwrap();
    let create_validation_fixture: Value = serde_json::from_str(include_str!(
        "../../fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/payments/payment-customization-create-validation-gaps.json"
    ))
    .unwrap();
    let empty_read_fixture: Value = serde_json::from_str(include_str!(
        "../../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/payments/payment-customization-empty-read.json"
    ))
    .unwrap();
    let metafields_fixture: Value = serde_json::from_str(include_str!(
        "../../fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/payments/payment-customization-metafields-and-handle-update.json"
    ))
    .unwrap();
    let activation_fixture: Value = serde_json::from_str(include_str!(
        "../../fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/payments/payment-customization-activation-mixed.json"
    ))
    .unwrap();
    let immutable_fixture: Value = serde_json::from_str(include_str!(
        "../../fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/payments/payment-customization-update-immutable-function.json"
    ))
    .unwrap();
    let mut validation_proxy = snapshot_proxy();

    let validation = validation_proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/payments/payment-customization-validation.graphql"
        ),
        validation_fixture["variables"].clone(),
    ));
    assert_eq!(validation.status, 200);
    assert_eq!(
        validation.body["data"]["missingTitle"]["userErrors"][0]["code"],
        json!("REQUIRED_INPUT_FIELD")
    );
    assert_eq!(
        validation.body["data"]["badCreate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        validation.body["data"]["unknownActivation"]["userErrors"][0]["code"],
        json!("PAYMENT_CUSTOMIZATION_NOT_FOUND")
    );

    let mut create_validation_proxy = snapshot_proxy();
    let mut create_validation_request = json_graphql_request(
        include_str!(
            "../../config/parity-requests/payments/payment-customization-create-validation-gaps.graphql"
        ),
        create_validation_fixture["variables"].clone(),
    );
    create_validation_request.headers.insert(
        "x-shopify-draft-proxy-api-client-id".to_string(),
        "347082227713".to_string(),
    );
    let create_validation = create_validation_proxy.process_request(create_validation_request);
    assert_eq!(create_validation.status, 200);
    assert_eq!(
        create_validation.body["data"]["missingTitle"],
        create_validation_fixture["response"]["payload"]["data"]["missingTitle"]
    );
    assert_eq!(
        create_validation.body["data"]["blankTitle"],
        create_validation_fixture["response"]["payload"]["data"]["blankTitle"]
    );
    assert_eq!(
        create_validation.body["data"]["missingEnabled"],
        create_validation_fixture["response"]["payload"]["data"]["missingEnabled"]
    );
    assert_eq!(
        create_validation.body["data"]["missingMetafields"]["userErrors"],
        json!([])
    );
    assert!(
        create_validation.body["data"]["missingMetafields"]["paymentCustomization"]["id"]
            .as_str()
            .is_some_and(|id| id.starts_with("gid://shopify/PaymentCustomization/"))
    );
    assert_eq!(
        create_validation.body["data"]["overflow"]["userErrors"],
        json!([])
    );
    assert!(
        create_validation.body["data"]["overflow"]["paymentCustomization"]["id"]
            .as_str()
            .is_some_and(|id| id.starts_with("gid://shopify/PaymentCustomization/"))
    );
    assert_eq!(
        create_validation.body["data"]["bothIdentifiers"],
        create_validation_fixture["response"]["payload"]["data"]["bothIdentifiers"]
    );
    assert_eq!(
        create_validation.body["data"]["missingIdentifier"],
        create_validation_fixture["response"]["payload"]["data"]["missingIdentifier"]
    );
    assert_eq!(
        create_validation.body["data"]["unknownHandle"],
        create_validation_fixture["response"]["payload"]["data"]["unknownHandle"]
    );

    let mut empty_read_proxy = snapshot_proxy();
    let empty_read = empty_read_proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/payments/payment-customization-empty-read.graphql"
        ),
        empty_read_fixture["variables"].clone(),
    ));
    assert_eq!(empty_read.status, 200);
    assert_eq!(
        empty_read.body["data"]["paymentCustomization"],
        empty_read_fixture["response"]["data"]["paymentCustomization"]
    );
    assert_eq!(
        empty_read.body["data"]["paymentCustomizations"]["nodes"],
        json!([])
    );

    let mut metafields_proxy = payment_customization_function_proxy(
        vec![metafields_fixture["selectedFunction"].clone()],
        Arc::new(Mutex::new(Vec::<Value>::new())),
    );
    let metafields_create = metafields_proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/payments/payment-customization-metafields-create.graphql"
        ),
        metafields_fixture["operations"]["paymentCustomizationCreate"]["variables"].clone(),
    ));
    assert_eq!(metafields_create.status, 200);
    assert_eq!(
        metafields_create.body["data"]["paymentCustomizationCreate"]["userErrors"],
        json!([])
    );
    let metafields_id = metafields_create.body["data"]["paymentCustomizationCreate"]
        ["paymentCustomization"]["id"]
        .clone();
    assert_eq!(
        metafields_create.body["data"]["paymentCustomizationCreate"]["paymentCustomization"]
            ["metafields"]["edges"][0]["node"]["value"],
        json!("baz")
    );

    let metafields_update = metafields_proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/payments/payment-customization-metafields-update.graphql"
        ),
        json!({
            "id": metafields_id.clone(),
            "input": metafields_fixture["operations"]["paymentCustomizationUpdateMetafields"]["variables"]["input"].clone()
        }),
    ));
    assert_eq!(metafields_update.status, 200);
    assert_eq!(
        metafields_update.body["data"]["paymentCustomizationUpdate"]["paymentCustomization"]
            ["metafields"]["edges"][0]["node"]["value"],
        json!("qux")
    );

    let handle_update = metafields_proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/payments/payment-customization-metafields-update.graphql"
        ),
        json!({
            "id": metafields_id.clone(),
            "input": metafields_fixture["operations"]["paymentCustomizationUpdateHandle"]["variables"]["input"].clone()
        }),
    ));
    assert_eq!(handle_update.status, 200);
    assert_eq!(
        handle_update.body["data"]["paymentCustomizationUpdate"]["userErrors"],
        json!([])
    );

    let metafields_read = metafields_proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/payments/payment-customization-metafields-read.graphql"
        ),
        json!({ "id": metafields_id }),
    ));
    assert_eq!(metafields_read.status, 200);
    assert_eq!(
        metafields_read.body["data"]["paymentCustomization"]["metafields"]["edges"][0]["node"]
            ["value"],
        json!("qux")
    );

    let mut activation_proxy = snapshot_proxy();
    let activation_create = activation_proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/payments/payment-customization-immutable-create.graphql"
        ),
        activation_fixture["operations"]["paymentCustomizationCreate"]["variables"].clone(),
    ));
    assert_eq!(activation_create.status, 200);
    let activation_id = activation_create.body["data"]["paymentCustomizationCreate"]
        ["paymentCustomization"]["id"]
        .clone();

    let activation = activation_proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/payments/payment-customization-activation-mixed.graphql"
        ),
        json!({ "ids": [activation_id, "gid://shopify/PaymentCustomization/0"], "enabled": false }),
    ));
    assert_eq!(activation.status, 200);
    assert_eq!(
        activation.body["data"]["paymentCustomizationActivation"]["userErrors"][0]["code"],
        json!("PAYMENT_CUSTOMIZATION_NOT_FOUND")
    );

    let mut immutable_proxy = snapshot_proxy();
    let immutable_create = immutable_proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/payments/payment-customization-immutable-create.graphql"
        ),
        immutable_fixture["operations"]["paymentCustomizationCreate"]["variables"].clone(),
    ));
    assert_eq!(immutable_create.status, 200);
    let immutable_id = immutable_create.body["data"]["paymentCustomizationCreate"]
        ["paymentCustomization"]["id"]
        .clone();

    let immutable_update = immutable_proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/payments/payment-customization-immutable-update.graphql"
        ),
        json!({
            "id": immutable_id.clone(),
            "input": immutable_fixture["operations"]["paymentCustomizationUpdateImmutable"]["variables"]["input"].clone()
        }),
    ));
    assert_eq!(immutable_update.status, 200);
    assert_eq!(
        immutable_update.body["data"]["paymentCustomizationUpdate"]["userErrors"][0]["code"],
        json!("FUNCTION_ID_CANNOT_BE_CHANGED")
    );

    let immutable_read = immutable_proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/payments/payment-customization-immutable-read.graphql"
        ),
        json!({ "id": immutable_id }),
    ));
    assert_eq!(immutable_read.status, 200);
    assert_eq!(
        immutable_read.body["data"]["paymentCustomization"]["functionId"],
        immutable_fixture["operations"]["paymentCustomizationCreate"]["variables"]["input"]
            ["functionId"]
    );
}

#[test]
fn payment_terms_omitted_template_id_create_coerces_update_defaults() {
    let create_query = r#"
        mutation MissingPaymentTermsTemplateCreate($referenceId: ID!, $attrs: PaymentTermsCreateInput!) {
          paymentTermsCreate(referenceId: $referenceId, paymentTermsAttributes: $attrs) {
            paymentTerms { id }
            userErrors { field message code }
          }
        }
    "#;
    let update_query = r#"
        mutation MissingPaymentTermsTemplateUpdate($input: PaymentTermsUpdateInput!) {
          paymentTermsUpdate(input: $input) {
            paymentTerms {
              id
              dueInDays
              paymentTermsName
              paymentTermsType
              paymentSchedules(first: 1) {
                nodes { issuedAt dueAt }
              }
            }
            userErrors { field message code }
          }
        }
    "#;
    let mut proxy = snapshot_proxy();

    let create = proxy.process_request(json_graphql_request(
        create_query,
        json!({
            "referenceId": "gid://shopify/Order/637",
            "attrs": { "paymentSchedules": [{ "issuedAt": "2026-01-01T00:00:00Z" }] }
        }),
    ));
    assert_eq!(create.status, 200);
    assert!(
        create.body.get("data").is_none(),
        "schema coercion should not execute paymentTermsCreate: {:?}",
        create.body
    );
    assert_eq!(
        create.body["errors"][0]["message"],
        json!("Variable $attrs of type PaymentTermsCreateInput! was provided invalid value for paymentTermsTemplateId (Expected value to not be null)")
    );
    assert_eq!(
        create.body["errors"][0]["extensions"]["code"],
        json!("INVALID_VARIABLE")
    );
    assert_eq!(
        create.body["errors"][0]["extensions"]["problems"][0]["path"],
        json!(["paymentTermsTemplateId"])
    );
    assert_eq!(log_snapshot(&proxy), json!({ "entries": [] }));

    let setup_owner_id = create_payment_terms_test_draft(
        &mut proxy,
        "payment-terms-missing-template-setup@example.test",
    )["id"]
        .clone();
    let setup = proxy.process_request(json_graphql_request(
        create_query,
        json!({
            "referenceId": setup_owner_id,
            "attrs": {
                "paymentTermsTemplateId": "gid://shopify/PaymentTermsTemplate/4",
                "paymentSchedules": [{ "issuedAt": "2026-01-01T00:00:00Z" }]
            }
        }),
    ));
    assert_eq!(setup.status, 200);
    assert_eq!(
        setup.body["data"]["paymentTermsCreate"]["userErrors"],
        json!([])
    );
    let payment_terms_id = setup.body["data"]["paymentTermsCreate"]["paymentTerms"]["id"]
        .as_str()
        .expect("setup payment terms id")
        .to_string();

    let update = proxy.process_request(json_graphql_request(
        update_query,
        json!({
            "input": {
                "paymentTermsId": payment_terms_id,
                "paymentTermsAttributes": {
                    "paymentSchedules": [{ "issuedAt": "2026-01-02T00:00:00Z" }]
                }
            }
        }),
    ));
    assert_eq!(update.status, 200);
    assert_eq!(
        update.body["data"]["paymentTermsUpdate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        update.body["data"]["paymentTermsUpdate"]["paymentTerms"]["paymentTermsName"],
        json!("Net 30")
    );
    assert_eq!(
        update.body["data"]["paymentTermsUpdate"]["paymentTerms"]["paymentTermsType"],
        json!("NET")
    );
    assert_eq!(
        update.body["data"]["paymentTermsUpdate"]["paymentTerms"]["paymentSchedules"]["nodes"][0],
        json!({
            "issuedAt": "2026-01-02T00:00:00Z",
            "dueAt": "2026-02-01T00:00:00Z"
        })
    );
}

#[test]
fn payment_terms_create_update_due_state_tracks_schedule_due_at() {
    let create_query = r#"
        mutation PaymentTermsDueStateCreate($referenceId: ID!, $attrs: PaymentTermsCreateInput!) {
          paymentTermsCreate(referenceId: $referenceId, paymentTermsAttributes: $attrs) {
            paymentTerms {
              id
              due
              overdue
              paymentSchedules(first: 1) {
                nodes {
                  dueAt
                  completedAt
                  due
                }
              }
            }
            userErrors { field message code }
          }
        }
    "#;
    let update_query = r#"
        mutation PaymentTermsDueStateUpdate($input: PaymentTermsUpdateInput!) {
          paymentTermsUpdate(input: $input) {
            paymentTerms {
              id
              due
              overdue
              paymentSchedules(first: 1) {
                nodes {
                  dueAt
                  completedAt
                  due
                }
              }
            }
            userErrors { field message code }
          }
        }
    "#;
    let read_query = r#"
        query PaymentTermsDueStateDraftRead($id: ID!) {
          draftOrder(id: $id) {
            paymentTerms {
              id
              due
              overdue
              paymentSchedules(first: 1) {
                nodes {
                  dueAt
                  completedAt
                  due
                }
              }
            }
          }
        }
    "#;
    let mut proxy = snapshot_proxy();

    let past_attrs = json!({
        "paymentTermsTemplateId": "gid://shopify/PaymentTermsTemplate/7",
        "paymentSchedules": [{ "dueAt": "2020-01-01T00:00:00Z" }]
    });
    let future_attrs = json!({
        "paymentTermsTemplateId": "gid://shopify/PaymentTermsTemplate/7",
        "paymentSchedules": [{ "dueAt": "2099-01-01T00:00:00Z" }]
    });
    let past_draft_id =
        create_payment_terms_test_draft(&mut proxy, "payment-terms-past-due@example.test")["id"]
            .clone();
    let future_draft_id =
        create_payment_terms_test_draft(&mut proxy, "payment-terms-future-due@example.test")["id"]
            .clone();

    let past_create = proxy.process_request(json_graphql_request(
        create_query,
        json!({
            "referenceId": past_draft_id.clone(),
            "attrs": past_attrs.clone()
        }),
    ));
    assert_eq!(past_create.status, 200);
    assert_eq!(
        past_create.body["data"]["paymentTermsCreate"]["userErrors"],
        json!([])
    );
    assert_payment_terms_due_state(
        &past_create.body["data"]["paymentTermsCreate"]["paymentTerms"],
        true,
        "2020-01-01T00:00:00Z",
    );

    let past_read = proxy.process_request(json_graphql_request(
        read_query,
        json!({ "id": past_draft_id }),
    ));
    assert_eq!(past_read.status, 200);
    assert_payment_terms_due_state(
        &past_read.body["data"]["draftOrder"]["paymentTerms"],
        true,
        "2020-01-01T00:00:00Z",
    );

    let future_create = proxy.process_request(json_graphql_request(
        create_query,
        json!({
            "referenceId": future_draft_id.clone(),
            "attrs": future_attrs.clone()
        }),
    ));
    assert_eq!(future_create.status, 200);
    assert_eq!(
        future_create.body["data"]["paymentTermsCreate"]["userErrors"],
        json!([])
    );
    assert_payment_terms_due_state(
        &future_create.body["data"]["paymentTermsCreate"]["paymentTerms"],
        false,
        "2099-01-01T00:00:00Z",
    );
    let payment_terms_id = future_create.body["data"]["paymentTermsCreate"]["paymentTerms"]["id"]
        .as_str()
        .expect("payment terms id")
        .to_string();

    let past_update = proxy.process_request(json_graphql_request(
        update_query,
        json!({
            "input": {
                "paymentTermsId": payment_terms_id,
                "paymentTermsAttributes": past_attrs
            }
        }),
    ));
    assert_eq!(past_update.status, 200);
    assert_eq!(
        past_update.body["data"]["paymentTermsUpdate"]["userErrors"],
        json!([])
    );
    assert_payment_terms_due_state(
        &past_update.body["data"]["paymentTermsUpdate"]["paymentTerms"],
        true,
        "2020-01-01T00:00:00Z",
    );

    let past_update_read = proxy.process_request(json_graphql_request(
        read_query,
        json!({ "id": future_draft_id }),
    ));
    assert_eq!(past_update_read.status, 200);
    assert_payment_terms_due_state(
        &past_update_read.body["data"]["draftOrder"]["paymentTerms"],
        true,
        "2020-01-01T00:00:00Z",
    );

    let terms_id_after_past_update = past_update.body["data"]["paymentTermsUpdate"]["paymentTerms"]
        ["id"]
        .as_str()
        .expect("payment terms id after past update")
        .to_string();
    let future_update = proxy.process_request(json_graphql_request(
        update_query,
        json!({
            "input": {
                "paymentTermsId": terms_id_after_past_update,
                "paymentTermsAttributes": future_attrs
            }
        }),
    ));
    assert_eq!(future_update.status, 200);
    assert_eq!(
        future_update.body["data"]["paymentTermsUpdate"]["userErrors"],
        json!([])
    );
    assert_payment_terms_due_state(
        &future_update.body["data"]["paymentTermsUpdate"]["paymentTerms"],
        false,
        "2099-01-01T00:00:00Z",
    );
}

fn assert_payment_terms_due_state(terms: &Value, expected_due: bool, expected_due_at: &str) {
    assert_eq!(terms["due"], json!(expected_due));
    assert_eq!(terms["overdue"], json!(expected_due));
    let schedule = &terms["paymentSchedules"]["nodes"][0];
    assert_eq!(schedule["dueAt"], json!(expected_due_at));
    assert_eq!(schedule["completedAt"], Value::Null);
    assert_eq!(schedule["due"], json!(expected_due));
}

fn create_payment_terms_test_order(
    proxy: &mut DraftProxy,
    email: &str,
    financial_status: &str,
    line_items: Value,
    payment_terms_allowed: Option<bool>,
) -> Value {
    let mut order = json!({
        "email": email,
        "currency": "USD",
        "presentmentCurrency": "CAD",
        "financialStatus": financial_status,
        "lineItems": line_items
    });
    if let Some(allowed) = payment_terms_allowed {
        order["customAttributes"] = json!([{
            "key": "__draftProxyPaymentTermsAllowed",
            "value": allowed.to_string()
        }]);
    }
    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation CreatePaymentTermsGuardOrder($order: OrderCreateOrderInput!) {
          orderCreate(order: $order) {
            order {
              id
              displayFinancialStatus
              currentTotalPriceSet {
                shopMoney { amount currencyCode }
                presentmentMoney { amount currencyCode }
              }
              lineItems(first: 2) {
                nodes {
                  sellingPlan { name }
                }
              }
            }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "order": order }),
    ));
    assert_eq!(create.status, 200);
    assert_eq!(create.body["data"]["orderCreate"]["userErrors"], json!([]));
    create.body["data"]["orderCreate"]["order"].clone()
}

fn create_payment_terms_test_draft(proxy: &mut DraftProxy, email: &str) -> Value {
    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation CreatePaymentTermsGuardDraft($input: DraftOrderInput!) {
          draftOrderCreate(input: $input) {
            draftOrder {
              id
              status
              totalPriceSet {
                shopMoney { amount currencyCode }
                presentmentMoney { amount currencyCode }
              }
            }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "input": {
                "email": email,
                "lineItems": [{
                    "title": "Payment terms guard draft",
                    "quantity": 1,
                    "originalUnitPrice": "18.50"
                }]
            }
        }),
    ));
    assert_eq!(create.status, 200);
    assert_eq!(
        create.body["data"]["draftOrderCreate"]["userErrors"],
        json!([])
    );
    create.body["data"]["draftOrderCreate"]["draftOrder"].clone()
}

#[test]
fn payment_terms_order_create_computes_totals_from_line_prices() {
    let mut proxy = snapshot_proxy();
    let create_query = r#"
        mutation CreatePaymentTermsOrderTotal($order: OrderCreateOrderInput!) {
          orderCreate(order: $order) {
            order {
              id
              currentTotalPriceSet {
                shopMoney { amount currencyCode }
                presentmentMoney { amount currencyCode }
              }
              paymentTerms { id }
            }
            userErrors { field message code }
          }
        }
    "#;

    let priced = proxy.process_request(json_graphql_request(
        create_query,
        json!({
            "order": {
                "email": "payment-terms-total-priced@example.test",
                "currency": "USD",
                "presentmentCurrency": "CAD",
                "lineItems": [
                    {
                        "title": "Two units",
                        "quantity": 2,
                        "priceSet": {
                            "shopMoney": { "amount": "3.25", "currencyCode": "USD" },
                            "presentmentMoney": { "amount": "4.50", "currencyCode": "CAD" }
                        }
                    },
                    {
                        "title": "Three units",
                        "quantity": 3,
                        "priceSet": {
                            "shopMoney": { "amount": "4.50", "currencyCode": "USD" },
                            "presentmentMoney": { "amount": "6.00", "currencyCode": "CAD" }
                        }
                    }
                ]
            }
        }),
    ));
    assert_eq!(priced.status, 200);
    assert_eq!(priced.body["data"]["orderCreate"]["userErrors"], json!([]));
    assert_eq!(
        priced.body["data"]["orderCreate"]["order"]["currentTotalPriceSet"],
        json!({
            "shopMoney": { "amount": "20.0", "currencyCode": "USD" },
            "presentmentMoney": { "amount": "27.0", "currencyCode": "CAD" }
        })
    );

    let missing_prices = proxy.process_request(json_graphql_request(
        create_query,
        json!({
            "order": {
                "email": "payment-terms-total-missing-prices@example.test",
                "currency": "USD",
                "presentmentCurrency": "CAD",
                "lineItems": [{
                    "title": "No price",
                    "quantity": 3
                }]
            }
        }),
    ));
    assert_eq!(missing_prices.status, 200);
    assert_eq!(
        missing_prices.body["data"]["orderCreate"]["order"]["currentTotalPriceSet"],
        json!({
            "shopMoney": { "amount": "0.0", "currencyCode": "USD" },
            "presentmentMoney": { "amount": "0.0", "currencyCode": "CAD" }
        })
    );
}

#[test]
fn payment_terms_due_state_recomputes_from_the_proxy_clock() {
    let clock = Arc::new(Mutex::new(utc_time(1_783_080_000)));
    let mut proxy = snapshot_proxy_with_clock(Arc::clone(&clock));
    let create_query = r#"
        mutation PaymentTermsClockedCreate($referenceId: ID!, $attrs: PaymentTermsCreateInput!) {
          paymentTermsCreate(referenceId: $referenceId, paymentTermsAttributes: $attrs) {
            paymentTerms {
              id
              due
              overdue
              paymentSchedules(first: 1) {
                nodes { dueAt completedAt due }
              }
            }
            userErrors { field message code }
          }
        }
    "#;
    let read_query = r#"
        query PaymentTermsClockedDraftRead($id: ID!) {
          draftOrder(id: $id) {
            paymentTerms {
              due
              overdue
              paymentSchedules(first: 1) {
                nodes { dueAt completedAt due }
              }
            }
          }
        }
    "#;
    let due_today_draft_id =
        create_payment_terms_test_draft(&mut proxy, "payment-terms-clock-due@example.test")["id"]
            .clone();
    let future_draft_id =
        create_payment_terms_test_draft(&mut proxy, "payment-terms-clock-future@example.test")
            ["id"]
            .clone();

    let due_today = proxy.process_request(json_graphql_request(
        create_query,
        json!({
            "referenceId": due_today_draft_id,
            "attrs": {
                "paymentTermsTemplateId": "gid://shopify/PaymentTermsTemplate/7",
                "paymentSchedules": [{ "dueAt": "2026-07-03T00:00:00Z" }]
            }
        }),
    ));
    assert_eq!(due_today.status, 200);
    assert_eq!(
        due_today.body["data"]["paymentTermsCreate"]["userErrors"],
        json!([])
    );
    assert_payment_terms_due_state(
        &due_today.body["data"]["paymentTermsCreate"]["paymentTerms"],
        true,
        "2026-07-03T00:00:00Z",
    );

    let future = proxy.process_request(json_graphql_request(
        create_query,
        json!({
            "referenceId": future_draft_id.clone(),
            "attrs": {
                "paymentTermsTemplateId": "gid://shopify/PaymentTermsTemplate/7",
                "paymentSchedules": [{ "dueAt": "2026-07-04T12:00:00Z" }]
            }
        }),
    ));
    assert_eq!(future.status, 200);
    assert_payment_terms_due_state(
        &future.body["data"]["paymentTermsCreate"]["paymentTerms"],
        false,
        "2026-07-04T12:00:00Z",
    );

    set_clock(&clock, 1_783_252_800);
    let future_read = proxy.process_request(json_graphql_request(
        read_query,
        json!({ "id": future_draft_id }),
    ));
    assert_eq!(future_read.status, 200);
    assert_payment_terms_due_state(
        &future_read.body["data"]["draftOrder"]["paymentTerms"],
        true,
        "2026-07-04T12:00:00Z",
    );
}

#[test]
fn payment_terms_create_update_guardrails_cover_current_helper_edges() {
    let create_query = r#"
        mutation RustPaymentTermsLocalRuntimeCreate($referenceId: ID!, $attrs: PaymentTermsCreateInput!) {
          paymentTermsCreate(referenceId: $referenceId, paymentTermsAttributes: $attrs) {
            paymentTerms {
              id
              paymentTermsName
              paymentTermsType
              paymentSchedules(first: 1) {
                nodes {
                  amount { amount currencyCode }
                  balanceDue { amount currencyCode }
                  totalBalance { amount currencyCode }
                  issuedAt
                  dueAt
                }
              }
            }
            userErrors { field message code }
          }
        }
    "#;
    let update_query = r#"
        mutation RustPaymentTermsLocalRuntimeUpdate($input: PaymentTermsUpdateInput!) {
          paymentTermsUpdate(input: $input) {
            paymentTerms { id paymentTermsName paymentTermsType }
            userErrors { field message code }
          }
        }
    "#;
    let mut proxy = snapshot_proxy();
    let net_attrs = json!({
        "paymentTermsTemplateId": "gid://shopify/PaymentTermsTemplate/4",
        "paymentSchedules": [{ "issuedAt": "2026-05-05T00:00:00Z" }]
    });

    let paid_order = create_payment_terms_test_order(
        &mut proxy,
        "payment-terms-paid-owner@example.test",
        "PAID",
        json!([{
            "title": "Paid payment terms owner",
            "quantity": 1,
            "priceSet": {
                "shopMoney": { "amount": "12.00", "currencyCode": "USD" },
                "presentmentMoney": { "amount": "16.00", "currencyCode": "CAD" }
            }
        }]),
        None,
    );
    let paid_create = proxy.process_request(json_graphql_request(
        create_query,
        json!({ "referenceId": paid_order["id"].clone(), "attrs": net_attrs.clone() }),
    ));
    assert_eq!(paid_create.status, 200);
    assert_eq!(
        paid_create.body["data"]["paymentTermsCreate"]["paymentTerms"],
        Value::Null
    );
    assert_eq!(
        paid_create.body["data"]["paymentTermsCreate"]["userErrors"][0],
        json!({
            "field": Value::Null,
            "message": "Cannot create payment terms on an Order that has already been paid in full.",
            "code": "PAYMENT_TERMS_CREATION_UNSUCCESSFUL"
        })
    );

    let channel_policy_order = create_payment_terms_test_order(
        &mut proxy,
        "payment-terms-channel-policy@example.test",
        "PENDING",
        json!([{
            "title": "Channel policy payment terms owner",
            "quantity": 1,
            "priceSet": {
                "shopMoney": { "amount": "9.00", "currencyCode": "USD" },
                "presentmentMoney": { "amount": "12.00", "currencyCode": "CAD" }
            }
        }]),
        Some(false),
    );
    let channel_policy_create = proxy.process_request(json_graphql_request(
        create_query,
        json!({ "referenceId": channel_policy_order["id"].clone(), "attrs": net_attrs.clone() }),
    ));
    assert_eq!(
        channel_policy_create.body["data"]["paymentTermsCreate"]["userErrors"][0]["message"],
        json!("Cannot create payment terms on an Order where the sales channel does not allow payment terms.")
    );
    assert_eq!(
        channel_policy_create.body["data"]["paymentTermsCreate"]["paymentTerms"],
        Value::Null
    );

    for reference_id in [
        create_payment_terms_test_order(
            &mut proxy,
            "payment-terms-closed-owner@example.test",
            "PENDING",
            json!([{
                "title": "Closed payment terms owner",
                "quantity": 1,
                "priceSet": { "shopMoney": { "amount": "10.00", "currencyCode": "USD" } }
            }]),
            None,
        )["id"]
            .as_str()
            .unwrap()
            .to_string(),
        create_payment_terms_test_draft(&mut proxy, "payment-terms-draft-owner@example.test")["id"]
            .as_str()
            .unwrap()
            .to_string(),
    ] {
        let accepted = proxy.process_request(json_graphql_request(
            create_query,
            json!({ "referenceId": reference_id, "attrs": net_attrs.clone() }),
        ));
        assert_eq!(accepted.status, 200);
        assert_eq!(
            accepted.body["data"]["paymentTermsCreate"]["userErrors"],
            json!([])
        );
        assert!(
            accepted.body["data"]["paymentTermsCreate"]["paymentTerms"]["id"]
                .as_str()
                .unwrap_or_default()
                .starts_with("gid://shopify/PaymentTerms/")
        );
    }

    let multiple_schedules_owner_id = create_payment_terms_test_draft(
        &mut proxy,
        "payment-terms-multiple-schedules@example.test",
    )["id"]
        .clone();
    let multiple_schedules = proxy.process_request(json_graphql_request(
        create_query,
        json!({
            "referenceId": multiple_schedules_owner_id,
            "attrs": {
                "paymentTermsTemplateId": "gid://shopify/PaymentTermsTemplate/4",
                "paymentSchedules": [
                    { "issuedAt": "2026-05-05T00:00:00Z" },
                    { "issuedAt": "2026-05-06T00:00:00Z" }
                ]
            }
        }),
    ));
    assert_eq!(
        multiple_schedules.body["data"]["paymentTermsCreate"],
        json!({
            "paymentTerms": Value::Null,
            "userErrors": [{
                // Matches the conformance capture (payment-terms-multiple-schedules.json):
                // null field, "multiple payment schedules." message.
                "field": Value::Null,
                "message": "Cannot create payment terms with multiple payment schedules.",
                "code": "PAYMENT_TERMS_CREATION_UNSUCCESSFUL"
            }]
        })
    );

    let unknown_order = proxy.process_request(json_graphql_request(
        create_query,
        json!({ "referenceId": "gid://shopify/Order/987654321", "attrs": net_attrs.clone() }),
    ));
    assert_eq!(
        unknown_order.body["data"]["paymentTermsCreate"]["userErrors"][0],
        json!({
            "field": Value::Null,
            "message": "Cannot find the specific Order with id 987654321.",
            "code": "PAYMENT_TERMS_CREATION_UNSUCCESSFUL"
        })
    );

    let unknown_draft = proxy.process_request(json_graphql_request(
        create_query,
        json!({ "referenceId": "gid://shopify/DraftOrder/987654322", "attrs": net_attrs.clone() }),
    ));
    assert_eq!(
        unknown_draft.body["data"]["paymentTermsCreate"]["userErrors"][0],
        json!({
            "field": Value::Null,
            "message": "Cannot find the specific Draft order with id 987654322.",
            "code": "PAYMENT_TERMS_CREATION_UNSUCCESSFUL"
        })
    );

    let unknown_template_owner_id =
        create_payment_terms_test_draft(&mut proxy, "payment-terms-unknown-template@example.test")
            ["id"]
            .clone();
    let unknown_template = proxy.process_request(json_graphql_request(
        create_query,
        json!({
            "referenceId": unknown_template_owner_id,
            "attrs": {
                "paymentTermsTemplateId": "gid://shopify/PaymentTermsTemplate/9999",
                "paymentSchedules": [{ "issuedAt": "2026-01-01T00:00:00Z" }]
            }
        }),
    ));
    assert_eq!(
        unknown_template.body["data"]["paymentTermsCreate"]["userErrors"][0],
        json!({
            "field": Value::Null,
            "message": "Could not find payment terms template.",
            "code": "PAYMENT_TERMS_CREATION_UNSUCCESSFUL"
        })
    );
    assert_eq!(
        unknown_template.body["data"]["paymentTermsCreate"]["paymentTerms"],
        Value::Null
    );

    let fixed_without_due_owner_id =
        create_payment_terms_test_draft(&mut proxy, "payment-terms-fixed-missing-due@example.test")
            ["id"]
            .clone();
    let fixed_without_due = proxy.process_request(json_graphql_request(
        create_query,
        json!({
            "referenceId": fixed_without_due_owner_id,
            "attrs": {
                "paymentTermsTemplateId": "gid://shopify/PaymentTermsTemplate/7",
                "paymentSchedules": [{}]
            }
        }),
    ));
    assert_eq!(
        fixed_without_due.body["data"]["paymentTermsCreate"]["userErrors"][0]["message"],
        json!("A due date is required with fixed or net payment terms.")
    );
    assert_eq!(
        fixed_without_due.body["data"]["paymentTermsCreate"]["userErrors"][0]["code"],
        json!("PAYMENT_TERMS_CREATION_UNSUCCESSFUL")
    );

    let net_without_schedule_date_owner_id =
        create_payment_terms_test_draft(&mut proxy, "payment-terms-net-missing-date@example.test")
            ["id"]
            .clone();
    let net_without_schedule_date = proxy.process_request(json_graphql_request(
        create_query,
        json!({
            "referenceId": net_without_schedule_date_owner_id,
            "attrs": {
                "paymentTermsTemplateId": "gid://shopify/PaymentTermsTemplate/4",
                "paymentSchedules": [{}]
            }
        }),
    ));
    assert_eq!(
        net_without_schedule_date.body["data"]["paymentTermsCreate"]["userErrors"][0]["message"],
        json!("A due date is required with fixed or net payment terms.")
    );

    let receipt_with_due_owner_id =
        create_payment_terms_test_draft(&mut proxy, "payment-terms-receipt-due@example.test")["id"]
            .clone();
    let receipt_with_due = proxy.process_request(json_graphql_request(
        create_query,
        json!({
            "referenceId": receipt_with_due_owner_id,
            "attrs": {
                "paymentTermsTemplateId": "gid://shopify/PaymentTermsTemplate/1",
                "paymentSchedules": [{ "dueAt": "2026-01-01T00:00:00Z" }]
            }
        }),
    ));
    assert_eq!(
        receipt_with_due.body["data"]["paymentTermsCreate"]["userErrors"][0]["message"],
        json!("A due date cannot be set with event payment terms.")
    );

    let fulfillment_with_due_owner_id =
        create_payment_terms_test_draft(&mut proxy, "payment-terms-fulfillment-due@example.test")
            ["id"]
            .clone();
    let fulfillment_with_due = proxy.process_request(json_graphql_request(
        create_query,
        json!({
            "referenceId": fulfillment_with_due_owner_id,
            "attrs": {
                "paymentTermsTemplateId": "gid://shopify/PaymentTermsTemplate/9",
                "paymentSchedules": [{ "dueAt": "2026-01-01T00:00:00Z" }]
            }
        }),
    ));
    assert_eq!(
        fulfillment_with_due.body["data"]["paymentTermsCreate"]["userErrors"][0]["message"],
        json!("A due date cannot be set with event payment terms.")
    );

    let receipt_issued_at_owner_id =
        create_payment_terms_test_draft(&mut proxy, "payment-terms-receipt-issued@example.test")
            ["id"]
            .clone();
    let receipt_issued_at = proxy.process_request(json_graphql_request(
        create_query,
        json!({
            "referenceId": receipt_issued_at_owner_id,
            "attrs": {
                "paymentTermsTemplateId": "gid://shopify/PaymentTermsTemplate/1",
                "paymentSchedules": [{ "issuedAt": "2026-01-01T00:00:00Z" }]
            }
        }),
    ));
    assert_eq!(
        receipt_issued_at.body["data"]["paymentTermsCreate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        receipt_issued_at.body["data"]["paymentTermsCreate"]["paymentTerms"]["paymentTermsName"],
        json!("Due on receipt")
    );
    assert_eq!(
        receipt_issued_at.body["data"]["paymentTermsCreate"]["paymentTerms"]["paymentSchedules"]
            ["nodes"],
        json!([])
    );

    let paid_update_seed_order = create_payment_terms_test_order(
        &mut proxy,
        "payment-terms-paid-update@example.test",
        "PENDING",
        json!([{
            "title": "Payment terms paid update owner",
            "quantity": 1,
            "priceSet": { "shopMoney": { "amount": "10.00", "currencyCode": "USD" } }
        }]),
        None,
    );
    let paid_update_seed = proxy.process_request(json_graphql_request(
        create_query,
        json!({ "referenceId": paid_update_seed_order["id"].clone(), "attrs": net_attrs.clone() }),
    ));
    assert_eq!(
        paid_update_seed.body["data"]["paymentTermsCreate"]["userErrors"],
        json!([])
    );
    let paid_update_id =
        paid_update_seed.body["data"]["paymentTermsCreate"]["paymentTerms"]["id"].clone();
    mark_reminder_order_paid(&mut proxy, paid_update_seed_order["id"].clone());
    let paid_update = proxy.process_request(json_graphql_request(
        update_query,
        json!({ "input": { "paymentTermsId": paid_update_id, "paymentTermsAttributes": net_attrs.clone() } }),
    ));
    assert_eq!(
        paid_update.body["data"]["paymentTermsUpdate"]["userErrors"][0],
        json!({
            "field": Value::Null,
            "message": "Cannot create payment terms on an Order that has already been paid in full.",
            "code": "PAYMENT_TERMS_UPDATE_UNSUCCESSFUL"
        })
    );

    let draft_update_owner_id =
        create_payment_terms_test_draft(&mut proxy, "payment-terms-draft-update@example.test")
            ["id"]
            .clone();
    let draft_update_seed = proxy.process_request(json_graphql_request(
        create_query,
        json!({
            "referenceId": draft_update_owner_id,
            "attrs": {
                "paymentTermsTemplateId": "gid://shopify/PaymentTermsTemplate/7",
                "paymentSchedules": [{ "dueAt": "2026-01-01T00:00:00Z" }]
            }
        }),
    ));
    assert_eq!(
        draft_update_seed.body["data"]["paymentTermsCreate"]["userErrors"],
        json!([])
    );
    let draft_update_id =
        draft_update_seed.body["data"]["paymentTermsCreate"]["paymentTerms"]["id"].clone();

    let draft_update = proxy.process_request(json_graphql_request(
        update_query,
        json!({ "input": { "paymentTermsId": draft_update_id.clone(), "paymentTermsAttributes": net_attrs.clone() } }),
    ));
    assert_eq!(
        draft_update.body["data"]["paymentTermsUpdate"]["paymentTerms"]["id"],
        draft_update_id
    );
    assert_eq!(
        draft_update.body["data"]["paymentTermsUpdate"]["userErrors"],
        json!([])
    );

    let unknown_template_update = proxy.process_request(json_graphql_request(
        update_query,
        json!({
            "input": {
                "paymentTermsId": "gid://shopify/PaymentTerms/123",
                "paymentTermsAttributes": {
                    "paymentTermsTemplateId": "gid://shopify/PaymentTermsTemplate/9999",
                    "paymentSchedules": [{ "issuedAt": "2026-01-01T00:00:00Z" }]
                }
            }
        }),
    ));
    assert_eq!(
        unknown_template_update.body["data"]["paymentTermsUpdate"]["userErrors"][0],
        json!({
            "field": Value::Null,
            "message": "Could not find payment terms template.",
            "code": "PAYMENT_TERMS_UPDATE_UNSUCCESSFUL"
        })
    );
    assert_eq!(
        unknown_template_update.body["data"]["paymentTermsUpdate"]["paymentTerms"],
        Value::Null
    );

    let invalid_update_attrs = proxy.process_request(json_graphql_request(
        update_query,
        json!({
            "input": {
                "paymentTermsId": "gid://shopify/PaymentTerms/123",
                "paymentTermsAttributes": {
                    "paymentTermsTemplateId": "gid://shopify/PaymentTermsTemplate/7",
                    "paymentSchedules": [{}]
                }
            }
        }),
    ));
    assert_eq!(
        invalid_update_attrs.body["data"]["paymentTermsUpdate"]["userErrors"][0],
        json!({
            "field": Value::Null,
            "message": "A due date is required with fixed or net payment terms.",
            "code": "PAYMENT_TERMS_UPDATE_UNSUCCESSFUL"
        })
    );
}

#[test]
fn payment_terms_create_update_reprojects_from_template_catalog() {
    let create_query = r#"
        mutation PaymentTermsTemplateProjectionCreate($referenceId: ID!, $attrs: PaymentTermsCreateInput!) {
          paymentTermsCreate(referenceId: $referenceId, paymentTermsAttributes: $attrs) {
            paymentTerms {
              id
              dueInDays
              paymentTermsName
              paymentTermsType
              translatedName
              paymentSchedules(first: 2) {
                nodes {
                  id
                  issuedAt
                  dueAt
                }
              }
            }
            userErrors { field message code }
          }
        }
    "#;
    let update_query = r#"
        mutation PaymentTermsTemplateProjectionUpdate($input: PaymentTermsUpdateInput!) {
          paymentTermsUpdate(input: $input) {
            paymentTerms {
              id
              dueInDays
              paymentTermsName
              paymentTermsType
              translatedName
              paymentSchedules(first: 2) {
                nodes {
                  id
                  issuedAt
                  dueAt
                }
              }
            }
            userErrors { field message code }
          }
        }
    "#;
    let mut proxy = snapshot_proxy();

    let templates = proxy.process_request(json_graphql_request(
        include_str!("../../config/parity-requests/payments/payment-terms-templates-read.graphql"),
        json!({ "type": "NET" }),
    ));
    assert_eq!(templates.status, 200);
    assert_eq!(
        templates.body["data"]["all"]
            .as_array()
            .and_then(|nodes| nodes
                .iter()
                .find(|node| node["id"] == json!("gid://shopify/PaymentTermsTemplate/2")))
            .map(|node| node["name"].clone()),
        Some(json!("Net 7"))
    );
    assert!(templates.body["data"]["filtered"]
        .as_array()
        .is_some_and(|nodes| nodes
            .iter()
            .all(|node| node["paymentTermsType"] == json!("NET"))));

    let mut create_attrs_for_log = Vec::new();
    let mut created_terms_ids = Vec::new();

    for (owner_email, attrs, expected_name, expected_type, expected_due_days, schedule_count) in [
        (
            "payment-terms-fixed-template@example.test",
            json!({
                "paymentTermsTemplateId": "gid://shopify/PaymentTermsTemplate/7",
                "paymentSchedules": [{ "dueAt": "2026-07-01T00:00:00Z" }]
            }),
            "Fixed",
            "FIXED",
            Value::Null,
            1_usize,
        ),
        (
            "payment-terms-net-7-template@example.test",
            json!({
                "paymentTermsTemplateId": "gid://shopify/PaymentTermsTemplate/2",
                "paymentSchedules": [{ "issuedAt": "2026-07-01T00:00:00Z" }]
            }),
            "Net 7",
            "NET",
            json!(7),
            1_usize,
        ),
        (
            "payment-terms-fulfillment-template@example.test",
            json!({
                "paymentTermsTemplateId": "gid://shopify/PaymentTermsTemplate/9"
            }),
            "Due on fulfillment",
            "FULFILLMENT",
            Value::Null,
            0_usize,
        ),
    ] {
        let owner_id = create_payment_terms_test_draft(&mut proxy, owner_email)["id"].clone();
        let create = proxy.process_request(json_graphql_request(
            create_query,
            json!({ "referenceId": owner_id, "attrs": attrs.clone() }),
        ));
        assert_eq!(create.status, 200);
        assert_eq!(
            create.body["data"]["paymentTermsCreate"]["userErrors"],
            json!([])
        );
        let terms = &create.body["data"]["paymentTermsCreate"]["paymentTerms"];
        assert_eq!(terms["paymentTermsName"], json!(expected_name));
        assert_eq!(terms["paymentTermsType"], json!(expected_type));
        assert_eq!(terms["translatedName"], json!(expected_name));
        assert_eq!(terms["dueInDays"], expected_due_days);
        assert_eq!(
            terms["paymentSchedules"]["nodes"].as_array().map(Vec::len),
            Some(schedule_count)
        );
        create_attrs_for_log.push(attrs);
        created_terms_ids.push(
            terms["id"]
                .as_str()
                .expect("created payment terms id")
                .to_string(),
        );
    }

    let log = log_snapshot(&proxy);
    assert_eq!(
        log["entries"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|entry| entry["interpreted"]["rootFields"] == json!(["paymentTermsCreate"]))
            .count(),
        create_attrs_for_log.len()
    );
    for attrs in create_attrs_for_log {
        let entry = log["entries"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|entry| entry["variables"]["attrs"] == attrs)
            .expect("paymentTermsCreate log should preserve original variables");
        assert!(entry["rawBody"]
            .as_str()
            .is_some_and(|raw| raw.contains("paymentTermsCreate")));
        assert_eq!(entry["status"], json!("staged"));
    }

    for (
        payment_terms_id,
        (attrs, expected_name, expected_type, expected_due_days, schedule_count),
    ) in created_terms_ids.into_iter().zip([
        (
            json!({
                "paymentTermsTemplateId": "gid://shopify/PaymentTermsTemplate/7",
                "paymentSchedules": [{ "dueAt": "2026-08-01T00:00:00Z" }]
            }),
            "Fixed",
            "FIXED",
            Value::Null,
            1_usize,
        ),
        (
            json!({
                "paymentTermsTemplateId": "gid://shopify/PaymentTermsTemplate/2",
                "paymentSchedules": [{ "issuedAt": "2026-08-01T00:00:00Z" }]
            }),
            "Net 7",
            "NET",
            json!(7),
            1_usize,
        ),
        (
            json!({
                "paymentTermsTemplateId": "gid://shopify/PaymentTermsTemplate/9"
            }),
            "Due on fulfillment",
            "FULFILLMENT",
            Value::Null,
            0_usize,
        ),
    ]) {
        let update = proxy.process_request(json_graphql_request(
            update_query,
            json!({
                "input": {
                    "paymentTermsId": payment_terms_id,
                    "paymentTermsAttributes": attrs
                }
            }),
        ));
        assert_eq!(update.status, 200);
        assert_eq!(
            update.body["data"]["paymentTermsUpdate"]["userErrors"],
            json!([])
        );
        let terms = &update.body["data"]["paymentTermsUpdate"]["paymentTerms"];
        assert_eq!(terms["paymentTermsName"], json!(expected_name));
        assert_eq!(terms["paymentTermsType"], json!(expected_type));
        assert_eq!(terms["translatedName"], json!(expected_name));
        assert_eq!(terms["dueInDays"], expected_due_days);
        assert_eq!(
            terms["paymentSchedules"]["nodes"].as_array().map(Vec::len),
            Some(schedule_count)
        );
    }
}

#[test]
fn payment_terms_create_delete_and_owner_cascade_replay_captured_shapes() {
    let mut proxy = snapshot_proxy();
    let order_create_variables = json!({
        "order": {
            "email": "payment-terms-order-runtime@example.com",
            "currency": "USD",
            "presentmentCurrency": "CAD",
            "lineItems": [{
                "title": "Payment terms order runtime",
                "quantity": 1,
                "priceSet": {
                    "shopMoney": { "amount": "42.50", "currencyCode": "USD" },
                    "presentmentMoney": { "amount": "57.00", "currencyCode": "CAD" }
                }
            }]
        }
    });
    let net_30_attrs = json!({
        "paymentTermsTemplateId": "gid://shopify/PaymentTermsTemplate/4",
        "paymentSchedules": [{ "issuedAt": "2026-05-05T00:00:00Z" }]
    });

    let order_create = proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/payments/payment-terms-create-on-order-create.graphql"
        ),
        order_create_variables,
    ));
    assert_eq!(
        order_create.body["data"]["orderCreate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        order_create.body["data"]["orderCreate"]["order"]["currentTotalPriceSet"],
        json!({
            "shopMoney": { "amount": "42.5", "currencyCode": "USD" },
            "presentmentMoney": { "amount": "57.0", "currencyCode": "CAD" }
        })
    );

    let create_terms = proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/payments/payment-terms-lifecycle-create.graphql"
        ),
        json!({
            "referenceId": order_create.body["data"]["orderCreate"]["order"]["id"].clone(),
            "attrs": net_30_attrs.clone()
        }),
    ));
    assert_eq!(
        create_terms.body["data"]["paymentTermsCreate"]["userErrors"],
        json!([])
    );
    let created_terms_id =
        create_terms.body["data"]["paymentTermsCreate"]["paymentTerms"]["id"].clone();
    assert!(created_terms_id
        .as_str()
        .is_some_and(|id| id.starts_with("gid://shopify/PaymentTerms/")));
    assert_eq!(
        create_terms.body["data"]["paymentTermsCreate"]["paymentTerms"]["paymentSchedules"]
            ["nodes"][0]["amount"],
        // `normalize_money_amount` canonicalizes the input "57.00" to "57.0".
        json!({ "amount": "57.0", "currencyCode": "CAD" })
    );
    assert_payment_terms_due_state(
        &create_terms.body["data"]["paymentTermsCreate"]["paymentTerms"],
        true,
        "2026-06-04T00:00:00Z",
    );

    let multiple = proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/payments/payment-terms-create-on-order-multiple.graphql"
        ),
        json!({
            "referenceId": order_create.body["data"]["orderCreate"]["order"]["id"].clone(),
            "attrs": {
                "paymentTermsTemplateId": "gid://shopify/PaymentTermsTemplate/4",
                "paymentSchedules": [
                    { "issuedAt": "2026-05-05T00:00:00Z" },
                    { "issuedAt": "2026-05-06T00:00:00Z" }
                ]
            }
        }),
    ));
    assert_eq!(
        multiple.body,
        json!({
            "data": {
                "paymentTermsCreate": {
                    "paymentTerms": Value::Null,
                    "userErrors": [{
                        "field": Value::Null,
                        "message": "Cannot create payment terms with multiple payment schedules.",
                        "code": "PAYMENT_TERMS_CREATION_UNSUCCESSFUL"
                    }]
                }
            }
        })
    );

    let missing_update = proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/payments/payment-terms-lifecycle-update.graphql"
        ),
        json!({
            "input": {
                "paymentTermsId": "gid://shopify/PaymentTerms/999999",
                "paymentTermsAttributes": net_30_attrs.clone()
            }
        }),
    ));
    assert_eq!(
        missing_update.body["data"]["paymentTermsUpdate"]["userErrors"][0]["code"],
        json!("PAYMENT_TERMS_UPDATE_UNSUCCESSFUL")
    );
    assert_eq!(
        missing_update.body["data"]["paymentTermsUpdate"]["userErrors"][0]["message"],
        json!("Could not find payment terms.")
    );

    let cascade_draft_id =
        create_payment_terms_test_draft(&mut proxy, "payment-terms-delete-cascade@example.test")
            ["id"]
            .clone();
    let draft_terms = proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/payments/payment-terms-lifecycle-create.graphql"
        ),
        json!({
            "referenceId": cascade_draft_id.clone(),
            "attrs": net_30_attrs.clone()
        }),
    ));
    assert_eq!(
        draft_terms.body["data"]["paymentTermsCreate"]["userErrors"],
        json!([])
    );
    let draft_terms_id =
        draft_terms.body["data"]["paymentTermsCreate"]["paymentTerms"]["id"].clone();
    assert_payment_terms_due_state(
        &draft_terms.body["data"]["paymentTermsCreate"]["paymentTerms"],
        true,
        "2026-06-04T00:00:00Z",
    );

    let draft_delete = proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/payments/payment-terms-lifecycle-delete.graphql"
        ),
        json!({
            "input": { "paymentTermsId": draft_terms_id.clone() }
        }),
    ));
    assert_eq!(
        draft_delete.body["data"]["paymentTermsDelete"],
        json!({ "deletedId": draft_terms_id, "userErrors": [] })
    );

    let draft_read = proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/payments/payment-terms-owner-cascade-draft-read.graphql"
        ),
        json!({ "id": cascade_draft_id }),
    ));
    assert_eq!(
        draft_read.body["data"]["draftOrder"]["paymentTerms"],
        Value::Null
    );

    let cascade_order_create = proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/payments/payment-terms-create-on-order-create.graphql"
        ),
        json!({
            "order": {
                "email": "payment-terms-delete-cascade-order@example.com",
                "currency": "USD",
                "presentmentCurrency": "CAD",
                "lineItems": [{
                    "title": "Payment terms delete cascade order",
                    "quantity": 1,
                    "priceSet": {
                        "shopMoney": { "amount": "42.50", "currencyCode": "USD" },
                        "presentmentMoney": { "amount": "57.00", "currencyCode": "CAD" }
                    }
                }]
            }
        }),
    ));
    assert_eq!(
        cascade_order_create.body["data"]["orderCreate"]["userErrors"],
        json!([])
    );
    let cascade_order_id = cascade_order_create.body["data"]["orderCreate"]["order"]["id"].clone();

    let cascade_order_terms = proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/payments/payment-terms-lifecycle-create.graphql"
        ),
        json!({
            "referenceId": cascade_order_id.clone(),
            "attrs": net_30_attrs.clone()
        }),
    ));
    assert_eq!(
        cascade_order_terms.body["data"]["paymentTermsCreate"]["userErrors"],
        json!([])
    );
    let cascade_order_terms_id =
        cascade_order_terms.body["data"]["paymentTermsCreate"]["paymentTerms"]["id"].clone();
    assert_payment_terms_due_state(
        &cascade_order_terms.body["data"]["paymentTermsCreate"]["paymentTerms"],
        true,
        "2026-06-04T00:00:00Z",
    );

    let cascade_order_delete = proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/payments/payment-terms-lifecycle-delete.graphql"
        ),
        json!({
            "input": { "paymentTermsId": cascade_order_terms_id.clone() }
        }),
    ));
    assert_eq!(
        cascade_order_delete.body["data"]["paymentTermsDelete"],
        json!({ "deletedId": cascade_order_terms_id, "userErrors": [] })
    );

    let cascade_order_read = proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/payments/payment-terms-owner-cascade-order-read.graphql"
        ),
        json!({ "id": cascade_order_id }),
    ));
    assert_eq!(
        cascade_order_read.body["data"]["order"]["paymentTerms"],
        Value::Null
    );

    let missing_delete = proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/payments/payment-terms-lifecycle-delete.graphql"
        ),
        json!({ "input": { "paymentTermsId": "gid://shopify/PaymentTerms/999999" } }),
    ));
    assert_eq!(
        missing_delete.body["data"]["paymentTermsDelete"]["userErrors"][0]["field"],
        Value::Null
    );
    assert_eq!(
        missing_delete.body["data"]["paymentTermsDelete"]["userErrors"][0]["message"],
        json!("Could not find payment terms.")
    );
    assert_eq!(
        missing_delete.body["data"]["paymentTermsDelete"]["userErrors"][0]["code"],
        json!("PAYMENT_TERMS_DELETE_UNSUCCESSFUL")
    );
    assert_eq!(
        missing_delete.body["data"]["paymentTermsDelete"]["deletedId"],
        Value::Null
    );
}

#[test]
fn order_create_mandate_payment_replays_idempotent_and_validation_shapes() {
    let mut proxy = snapshot_proxy();

    let missing_mandate = proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/payments/order_create_mandate_payment_missing_mandate.graphql"
        ),
        json!({
            "id": "gid://shopify/Order/1",
            "idempotencyKey": "missing-mandate"
        }),
    ));
    assert!(missing_mandate.body.get("data").is_none());
    assert!(missing_mandate.body["errors"][0]["message"]
        .as_str()
        .is_some_and(|message| message.contains("mandateId") && message.contains("required")));

    let first_mandate = proxy.process_request(json_graphql_request(
        &current_order_mandate_document(include_str!(
            "../../config/parity-requests/payments/order_create_mandate_payment.graphql"
        )),
        json!({
            "id": "gid://shopify/Order/1",
            "mandateId": "gid://shopify/PaymentMandate/har-397",
            "idempotencyKey": "har-353-idempotent-payment",
            "amount": { "amount": "25.00", "currencyCode": "CAD" }
        }),
    ));
    let first_payload = &first_mandate.body["data"]["orderCreateMandatePayment"];
    assert_eq!(
        first_payload["paymentReferenceId"],
        json!("gid://shopify/Order/1/har-353-idempotent-payment")
    );
    assert_eq!(first_payload["userErrors"], json!([]));
    assert_ne!(first_payload["job"]["id"], json!("gid://shopify/Job/6"));
    let first_order = read_order_payment_projection(&mut proxy, json!("gid://shopify/Order/1"));
    let first_transaction_id = first_order["transactions"][0]["id"].clone();
    assert_ne!(
        first_transaction_id,
        json!("gid://shopify/OrderTransaction/4")
    );
    assert_eq!(
        first_order["transactions"][0]["amountSet"]["shopMoney"],
        json!({ "amount": "25.0", "currencyCode": "CAD" })
    );

    let repeat = proxy.process_request(json_graphql_request(
        &current_order_mandate_document(include_str!(
            "../../config/parity-requests/payments/order_create_mandate_payment.graphql"
        )),
        json!({
            "id": "gid://shopify/Order/1",
            "mandateId": "gid://shopify/PaymentMandate/har-397",
            "idempotencyKey": "har-353-idempotent-payment",
            "amount": { "amount": "25.00", "currencyCode": "CAD" }
        }),
    ));
    let repeat_payload = &repeat.body["data"]["orderCreateMandatePayment"];
    assert_eq!(
        repeat_payload["paymentReferenceId"],
        first_payload["paymentReferenceId"]
    );
    assert_eq!(repeat_payload["userErrors"], json!([]));
    assert_ne!(repeat_payload["job"]["id"], json!("gid://shopify/Job/6"));
    assert_eq!(repeat_payload["job"]["id"], first_payload["job"]["id"]);
    assert_eq!(
        read_order_payment_projection(&mut proxy, json!("gid://shopify/Order/1")),
        first_order
    );

    let auth_only = proxy.process_request(json_graphql_request(
        &current_order_mandate_document(include_str!(
            "../../config/parity-requests/payments/order_create_mandate_payment.graphql"
        )),
        json!({
            "id": "gid://shopify/Order/1",
            "mandateId": "gid://shopify/PaymentMandate/har-397",
            "idempotencyKey": "har-848-auth-only",
            "autoCapture": false,
            "amount": { "amount": "25.00", "currencyCode": "CAD" }
        }),
    ));
    let auth_only_payload = &auth_only.body["data"]["orderCreateMandatePayment"];
    assert_eq!(auth_only_payload["userErrors"], json!([]));
    let auth_only_order = read_order_payment_projection(&mut proxy, json!("gid://shopify/Order/1"));
    assert_eq!(
        auth_only_order["displayFinancialStatus"],
        json!("AUTHORIZED")
    );
    let auth_only_transactions = auth_only_order["transactions"].as_array().unwrap();
    assert_eq!(auth_only_transactions.len(), 2);
    assert_eq!(auth_only_transactions[0]["id"], first_transaction_id);
    assert_eq!(auth_only_transactions[1]["kind"], json!("AUTHORIZATION"));
    assert_ne!(auth_only_transactions[1]["id"], first_transaction_id);
}

#[test]
fn order_create_mandate_payment_preserves_existing_staged_order() {
    let mut proxy = snapshot_proxy();

    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateMandatePaymentOrder($order: OrderCreateOrderInput!) {
          orderCreate(order: $order) {
            order {
              id
              name
              customer { id }
              billingAddress { address1 city countryCodeV2 }
              shippingAddress { address1 city countryCodeV2 }
              lineItems(first: 10) { nodes { id title quantity } }
              paymentGatewayNames
              totalOutstandingSet { shopMoney { amount currencyCode } }
              totalReceivedSet { shopMoney { amount currencyCode } }
              transactions {
                id
                kind
                status
                gateway
                amountSet { shopMoney { amount currencyCode } }
              }
            }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "order": {
                "email": "mandate-preserve@example.test",
                "customerId": "gid://shopify/Customer/424242",
                "billingAddress": {
                    "address1": "1 Billing Street",
                    "city": "Toronto",
                    "countryCode": "CA"
                },
                "shippingAddress": {
                    "address1": "2 Shipping Street",
                    "city": "Montreal",
                    "countryCode": "CA"
                },
                "lineItems": [{
                    "title": "Preserved mandate line",
                    "quantity": 2,
                    "priceSet": {
                        "shopMoney": { "amount": "12.50", "currencyCode": "CAD" }
                    }
                }],
                "transactions": [{
                    "kind": "AUTHORIZATION",
                    "status": "SUCCESS",
                    "gateway": "shopify_payments",
                    "amountSet": {
                        "shopMoney": { "amount": "25.00", "currencyCode": "CAD" }
                    }
                }]
            }
        }),
    ));
    assert_eq!(create.status, 200);
    assert_eq!(create.body["data"]["orderCreate"]["userErrors"], json!([]));
    let created_order = &create.body["data"]["orderCreate"]["order"];
    let order_id = created_order["id"].clone();
    let authorization_transaction = created_order["transactions"][0].clone();
    assert_eq!(created_order["name"], json!("#1"));
    assert_eq!(
        created_order["paymentGatewayNames"],
        json!(["shopify_payments"])
    );

    let mandate_query = r#"
        mutation ChargeExistingMandateOrder(
          $id: ID!
          $mandateId: ID!
          $idempotencyKey: String
          $amount: MoneyInput
        ) {
          orderCreateMandatePayment(
            id: $id
            mandateId: $mandateId
            idempotencyKey: $idempotencyKey
            amount: $amount
          ) {
            job { id done }
            paymentReferenceId
            userErrors { field message }
          }
        }
    "#;
    let mandate_variables = json!({
        "id": order_id,
        "mandateId": "gid://shopify/PaymentMandate/preserve-existing-order",
        "idempotencyKey": "preserve-existing-order-key",
        "amount": { "amount": "25.00", "currencyCode": "CAD" }
    });
    let mandate = proxy.process_request(json_graphql_request(
        &current_order_mandate_document(mandate_query),
        mandate_variables.clone(),
    ));
    assert_eq!(mandate.status, 200);
    let mandate_payload = &mandate.body["data"]["orderCreateMandatePayment"];
    assert_eq!(mandate_payload["userErrors"], json!([]));
    assert_eq!(
        mandate_payload["paymentReferenceId"],
        json!("gid://shopify/Order/1/preserve-existing-order-key")
    );
    assert_eq!(mandate_payload["job"]["done"], json!(true));
    let paid_order = read_preserved_mandate_order(&mut proxy, order_id.clone());

    assert_eq!(paid_order["name"], created_order["name"]);
    assert_eq!(paid_order["customer"], created_order["customer"]);
    assert_eq!(
        paid_order["billingAddress"],
        created_order["billingAddress"]
    );
    assert_eq!(
        paid_order["shippingAddress"],
        created_order["shippingAddress"]
    );
    assert_eq!(paid_order["lineItems"], created_order["lineItems"]);
    assert_eq!(
        paid_order["paymentGatewayNames"],
        json!(["shopify_payments"])
    );
    assert_eq!(paid_order["displayFinancialStatus"], json!("PAID"));
    assert_eq!(
        paid_order["totalOutstandingSet"]["shopMoney"],
        json!({ "amount": "0.0", "currencyCode": "CAD" })
    );
    assert_eq!(
        paid_order["totalReceivedSet"]["shopMoney"],
        json!({ "amount": "25.0", "currencyCode": "CAD" })
    );
    assert_eq!(paid_order["transactions"].as_array().unwrap().len(), 2);
    assert_eq!(paid_order["transactions"][0], authorization_transaction);
    assert_ne!(
        paid_order["transactions"][1]["id"],
        authorization_transaction["id"]
    );
    assert_eq!(paid_order["transactions"][1]["kind"], json!("SALE"));
    assert_eq!(
        paid_order["transactions"][1]["gateway"],
        json!("shopify_payments")
    );
    assert_eq!(
        mandate_payload["paymentReferenceId"],
        json!("gid://shopify/Order/1/preserve-existing-order-key")
    );

    let repeat = proxy.process_request(json_graphql_request(
        &current_order_mandate_document(mandate_query),
        mandate_variables,
    ));
    let repeat_payload = &repeat.body["data"]["orderCreateMandatePayment"];
    assert_eq!(repeat.status, 200);
    assert_eq!(repeat_payload["userErrors"], json!([]));
    assert_eq!(repeat_payload["job"]["id"], mandate_payload["job"]["id"]);
    assert_eq!(
        read_preserved_mandate_order(&mut proxy, order_id),
        paid_order
    );
}

#[test]
fn order_payment_transactions_stage_capture_void_and_downstream_reads() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../fixtures/conformance/local-runtime/2026-04/orders/order-payment-transaction-local-staging.json"
    ))
    .unwrap();

    let mut capture_proxy = snapshot_proxy();
    let create = capture_proxy.process_request(json_graphql_request(
        &current_order_payment_document(include_str!(
            "../../config/parity-requests/orders/order-payment-create-local-staging.graphql"
        )),
        fixture["paymentCaptureFlow"]["create"]["variables"].clone(),
    ));
    assert_eq!(create.status, 200);
    assert_eq!(
        create.body["data"]["orderCreate"]["order"]["displayFinancialStatus"],
        json!("AUTHORIZED")
    );
    let order_id = create.body["data"]["orderCreate"]["order"]["id"].clone();
    let parent_transaction_id =
        create.body["data"]["orderCreate"]["order"]["transactions"][0]["id"].clone();

    let over_capture = capture_proxy.process_request(json_graphql_request(
        &current_order_capture_document(include_str!("../../config/parity-requests/orders/order-payment-capture-local-staging.graphql")),
        json!({"input": {"id": order_id, "parentTransactionId": parent_transaction_id, "amount": "30.00", "currency": "CAD"}}),
    ));
    assert_eq!(
        over_capture.body["data"]["orderCapture"]["transaction"],
        Value::Null
    );
    assert_eq!(
        over_capture.body["data"]["orderCapture"]["userErrors"],
        json!([{
            "field": null,
            "message": "Cannot capture more than the authorized 25.00 for this payment."
        }])
    );

    let first_capture = capture_proxy.process_request(json_graphql_request(
        &current_order_capture_document(include_str!("../../config/parity-requests/orders/order-payment-capture-local-staging.graphql")),
        json!({"input": {"id": create.body["data"]["orderCreate"]["order"]["id"].clone(), "parentTransactionId": create.body["data"]["orderCreate"]["order"]["transactions"][0]["id"].clone(), "amount": "10.00", "currency": "CAD"}}),
    ));
    assert_eq!(first_capture.status, 200);
    assert_eq!(
        first_capture.body["data"]["orderCapture"]["userErrors"],
        json!([])
    );
    let first_captured_order = read_order_payment_projection(
        &mut capture_proxy,
        create.body["data"]["orderCreate"]["order"]["id"].clone(),
    );
    assert_eq!(
        first_captured_order["displayFinancialStatus"],
        json!("PARTIALLY_PAID")
    );
    assert_eq!(first_captured_order["totalCapturable"], json!("15.0"));

    let final_capture = capture_proxy.process_request(json_graphql_request(
        &current_order_capture_document(include_str!("../../config/parity-requests/orders/order-payment-capture-local-staging.graphql")),
        json!({"input": {"id": create.body["data"]["orderCreate"]["order"]["id"].clone(), "parentTransactionId": create.body["data"]["orderCreate"]["order"]["transactions"][0]["id"].clone(), "amount": "15.00", "currency": "CAD", "finalCapture": null}}),
    ));
    assert_eq!(final_capture.status, 200);
    assert_eq!(
        final_capture.body["data"]["orderCapture"]["userErrors"],
        json!([])
    );
    let final_order = read_order_payment_projection(
        &mut capture_proxy,
        create.body["data"]["orderCreate"]["order"]["id"].clone(),
    );
    assert_eq!(final_order["displayFinancialStatus"], json!("PAID"));
    assert_eq!(final_order["capturable"], json!(false));
    assert_eq!(
        final_order["transactions"]
            .as_array()
            .expect("transactions")
            .len(),
        3
    );

    let read_after_final = capture_proxy.process_request(json_graphql_request(
        &current_order_payment_document(include_str!(
            "../../config/parity-requests/orders/order-payment-read-local-staging.graphql"
        )),
        json!({"id": create.body["data"]["orderCreate"]["order"]["id"].clone()}),
    ));
    assert_eq!(
        read_after_final.body["data"]["order"]["displayFinancialStatus"],
        final_order["displayFinancialStatus"]
    );

    let void_after_capture = capture_proxy.process_request(json_graphql_request(
        &current_order_payment_document(include_str!(
            "../../config/parity-requests/orders/order-payment-void-local-staging.graphql"
        )),
        json!({"id": create.body["data"]["orderCreate"]["order"]["transactions"][0]["id"].clone()}),
    ));
    assert_eq!(
        void_after_capture.body["data"]["transactionVoid"]["transaction"],
        Value::Null
    );

    let missing_mandate_idempotency = capture_proxy.process_request(json_graphql_request(
        &current_order_mandate_document(include_str!("../../config/parity-requests/orders/order-payment-mandate-local-staging.graphql")),
        json!({"id": create.body["data"]["orderCreate"]["order"]["id"].clone(), "mandateId": "gid://shopify/PaymentMandate/har-397"}),
    ));
    assert_eq!(
        missing_mandate_idempotency.body,
        json!({
            "data": {
                "orderCreateMandatePayment": {
                    "job": Value::Null,
                    "paymentReferenceId": Value::Null,
                    "userErrors": [{
                        "field": ["idempotencyKey"],
                        "message": "Idempotency key is required"
                    }]
                }
            }
        })
    );

    let mut void_proxy = snapshot_proxy();
    let void_create = void_proxy.process_request(json_graphql_request(
        &current_order_payment_document(include_str!(
            "../../config/parity-requests/orders/order-payment-create-local-staging.graphql"
        )),
        fixture["voidFlow"]["create"]["variables"].clone(),
    ));
    assert_eq!(
        void_create.body["data"]["orderCreate"]["order"]["displayFinancialStatus"],
        json!("AUTHORIZED")
    );

    let void_response = void_proxy.process_request(json_graphql_request(
        &current_order_payment_document(include_str!("../../config/parity-requests/orders/order-payment-void-local-staging.graphql")),
        json!({"id": void_create.body["data"]["orderCreate"]["order"]["transactions"][0]["id"].clone()}),
    ));
    assert_eq!(
        void_response.body["data"]["transactionVoid"]["transaction"]["kind"],
        json!("VOID")
    );

    let read_after_void = void_proxy.process_request(json_graphql_request(
        &current_order_payment_document(include_str!(
            "../../config/parity-requests/orders/order-payment-read-local-staging.graphql"
        )),
        json!({"id": void_create.body["data"]["orderCreate"]["order"]["id"].clone()}),
    ));
    assert_eq!(
        read_after_void.body["data"]["order"]["displayFinancialStatus"],
        json!("VOIDED")
    );
}

#[test]
fn order_create_manual_payment_stages_sale_and_round_trips() {
    let mut proxy = snapshot_proxy();
    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateManualPaymentOrder($order: OrderCreateOrderInput!) {
          orderCreate(order: $order) {
            order {
              id
              displayFinancialStatus
              totalOutstandingSet { shopMoney { amount currencyCode } }
              totalReceivedSet { shopMoney { amount currencyCode } }
              transactions { id }
            }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "order": {
                "currency": "USD",
                "email": "manual-payment@example.test",
                "lineItems": [{
                    "title": "Manual payment item",
                    "quantity": 1,
                    "priceSet": { "shopMoney": { "amount": "30.00", "currencyCode": "USD" } }
                }]
            }
        }),
    ));
    assert_eq!(create.status, 200);
    assert_eq!(create.body["data"]["orderCreate"]["userErrors"], json!([]));
    let order_id = create.body["data"]["orderCreate"]["order"]["id"].clone();

    let processed_at = "2026-07-11T03:45:00Z";
    let manual_payment = proxy.process_request(json_graphql_request(
        r#"
        mutation ManualPayment(
          $id: ID!
          $amount: MoneyInput
          $processedAt: DateTime
        ) {
          orderCreateManualPayment(
            id: $id
            amount: $amount
            paymentMethodName: "Cash on delivery"
            processedAt: $processedAt
          ) {
            order {
              id
              displayFinancialStatus
              capturable
              totalCapturable
              paymentGatewayNames
              totalOutstandingSet { shopMoney { amount currencyCode } }
              totalReceivedSet { shopMoney { amount currencyCode } }
              netPaymentSet { shopMoney { amount currencyCode } }
              transactions {
                id
                kind
                status
                gateway
                processedAt
                amountSet { shopMoney { amount currencyCode } }
              }
            }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "id": order_id,
            "amount": { "amount": "12.50", "currencyCode": "USD" },
            "processedAt": processed_at
        }),
    ));
    assert_eq!(manual_payment.status, 200);
    let paid_order = manual_payment.body["data"]["orderCreateManualPayment"]["order"].clone();
    assert_eq!(
        manual_payment.body["data"]["orderCreateManualPayment"]["userErrors"],
        json!([])
    );
    assert_eq!(
        paid_order["displayFinancialStatus"],
        json!("PARTIALLY_PAID")
    );
    assert_eq!(paid_order["capturable"], json!(false));
    assert_eq!(paid_order["totalCapturable"], json!("0.0"));
    assert_eq!(
        paid_order["paymentGatewayNames"],
        json!(["Cash on delivery"])
    );
    assert_eq!(
        paid_order["totalOutstandingSet"]["shopMoney"],
        json!({ "amount": "17.5", "currencyCode": "USD" })
    );
    assert_eq!(
        paid_order["totalReceivedSet"]["shopMoney"],
        json!({ "amount": "12.5", "currencyCode": "USD" })
    );
    assert_eq!(paid_order["netPaymentSet"], paid_order["totalReceivedSet"]);
    assert_eq!(paid_order["transactions"][0]["kind"], json!("SALE"));
    assert_eq!(paid_order["transactions"][0]["status"], json!("SUCCESS"));
    assert_eq!(
        paid_order["transactions"][0]["gateway"],
        json!("Cash on delivery")
    );
    assert_eq!(
        paid_order["transactions"][0]["processedAt"],
        json!(processed_at)
    );
    assert_eq!(
        paid_order["transactions"][0]["amountSet"]["shopMoney"],
        json!({ "amount": "12.5", "currencyCode": "USD" })
    );

    let log = log_snapshot(&proxy);
    let entries = log["entries"].as_array().expect("log entries");
    assert_eq!(entries.len(), 2);
    assert_eq!(
        entries[1]["interpreted"]["primaryRootField"],
        json!("orderCreateManualPayment")
    );
    assert_eq!(entries[1]["status"], json!("staged"));
    assert!(entries[1]["rawBody"]
        .as_str()
        .expect("manual payment raw body")
        .contains("orderCreateManualPayment"));

    let dump = proxy.process_request(request_with_body("POST", "/__meta/dump", ""));
    assert_eq!(dump.status, 200);
    let mut restored = snapshot_proxy();
    let restore = restored.process_request(request_with_body(
        "POST",
        "/__meta/restore",
        &dump.body.to_string(),
    ));
    assert_eq!(restore.status, 200);
    let read_back = restored.process_request(json_graphql_request(
        r#"
        query ReadManualPaymentOrder($id: ID!) {
          order(id: $id) {
            id
            displayFinancialStatus
            capturable
            totalCapturable
            paymentGatewayNames
            totalOutstandingSet { shopMoney { amount currencyCode } }
            totalReceivedSet { shopMoney { amount currencyCode } }
            netPaymentSet { shopMoney { amount currencyCode } }
            transactions {
              id
              kind
              status
              gateway
              processedAt
              amountSet { shopMoney { amount currencyCode } }
            }
          }
        }
        "#,
        json!({ "id": paid_order["id"] }),
    ));
    assert_eq!(read_back.status, 200);
    assert_eq!(read_back.body["data"]["order"], paid_order);

    let reset = restored.process_request(request_with_body("POST", "/__meta/reset", ""));
    assert_eq!(reset.status, 200);
    let after_reset = restored.process_request(json_graphql_request(
        r#"
        query ReadResetManualPaymentOrder($id: ID!) {
          order(id: $id) { id }
        }
        "#,
        json!({ "id": paid_order["id"] }),
    ));
    assert_eq!(after_reset.status, 200);
    assert_eq!(after_reset.body["data"]["order"], Value::Null);
}

#[test]
fn order_create_manual_payment_validation_and_access_denied_do_not_mutate_or_passthrough() {
    let upstream_calls = Arc::new(Mutex::new(Vec::<Value>::new()));
    let captured_calls = Arc::clone(&upstream_calls);
    let mut live_proxy =
        configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport(move |request| {
            captured_calls
                .lock()
                .unwrap()
                .push(serde_json::from_str(&request.body).expect("upstream request body"));
            Response {
                status: 200,
                headers: Default::default(),
                body: json!({ "errors": [{ "message": "unexpected upstream call" }] }),
            }
        });
    let unknown = live_proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/orders/orderCreateManualPayment-access-denied-parity.graphql"
        ),
        json!({
            "id": "gid://shopify/Order/0",
            "amount": { "amount": "16.00", "currencyCode": "USD" },
            "paymentMethodName": "Shopify draft proxy manual payment",
            "processedAt": "2026-05-06T22:09:03.472Z"
        }),
    ));
    assert_eq!(unknown.status, 200);
    assert_eq!(
        unknown.body["data"]["orderCreateManualPayment"],
        Value::Null
    );
    assert_eq!(
        unknown.body["errors"][0]["extensions"]["code"],
        json!("ACCESS_DENIED")
    );
    assert_eq!(
        unknown.body["errors"][0]["path"],
        json!(["orderCreateManualPayment"])
    );
    assert!(upstream_calls.lock().unwrap().is_empty());
    assert_eq!(
        log_snapshot(&live_proxy)["entries"]
            .as_array()
            .expect("log entries")
            .len(),
        0
    );

    let mut proxy = snapshot_proxy();
    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateManualPaymentValidationOrder($order: OrderCreateOrderInput!) {
          orderCreate(order: $order) {
            order {
              id
              displayFinancialStatus
              totalOutstandingSet { shopMoney { amount currencyCode } }
              totalReceivedSet { shopMoney { amount currencyCode } }
              transactions { id }
            }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "order": {
                "currency": "USD",
                "lineItems": [{
                    "title": "Manual payment validation item",
                    "quantity": 1,
                    "priceSet": { "shopMoney": { "amount": "10.00", "currencyCode": "USD" } }
                }]
            }
        }),
    ));
    let order_id = create.body["data"]["orderCreate"]["order"]["id"].clone();
    let state_before = state_snapshot(&proxy);
    let log_before = log_snapshot(&proxy);
    let overpay = proxy.process_request(json_graphql_request(
        r#"
        mutation OverpayManualPayment($id: ID!, $amount: MoneyInput) {
          orderCreateManualPayment(id: $id, amount: $amount) {
            order { id displayFinancialStatus transactions { id } }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "id": order_id,
            "amount": { "amount": "11.00", "currencyCode": "USD" }
        }),
    ));
    assert_eq!(overpay.status, 200);
    assert_eq!(
        overpay.body["data"]["orderCreateManualPayment"]["userErrors"],
        json!([{ "field": ["amount"], "message": "Amount exceeds outstanding balance" }])
    );
    assert_eq!(state_snapshot(&proxy), state_before);
    assert_eq!(log_snapshot(&proxy), log_before);
}

#[test]
fn order_invoice_send_stages_no_delivery_intent_and_round_trips_state() {
    let mut proxy = snapshot_proxy();
    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateOrderForInvoiceSend($order: OrderCreateOrderInput!) {
          orderCreate(order: $order) {
            order { id name email updatedAt }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "order": {
                "currency": "USD",
                "email": "invoice-local@example.test",
                "lineItems": [{
                    "title": "Invoice send item",
                    "quantity": 1,
                    "priceSet": { "shopMoney": { "amount": "16.00", "currencyCode": "USD" } }
                }]
            }
        }),
    ));
    assert_eq!(create.status, 200);
    let order_id = create.body["data"]["orderCreate"]["order"]["id"].clone();

    let send = proxy.process_request(json_graphql_request(
        r#"
        mutation SendOrderInvoice($id: ID!, $email: EmailInput) {
          orderInvoiceSend(id: $id, email: $email) {
            order { id name email updatedAt }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "id": order_id,
            "email": {
                "to": "invoice-recipient@example.test",
                "subject": "Local invoice",
                "customMessage": "No delivery should occur"
            }
        }),
    ));
    assert_eq!(send.status, 200);
    let sent_order = send.body["data"]["orderInvoiceSend"]["order"].clone();
    assert_eq!(
        send.body["data"]["orderInvoiceSend"]["userErrors"],
        json!([])
    );
    assert_eq!(sent_order["email"], json!("invoice-local@example.test"));
    assert_ne!(
        sent_order["updatedAt"],
        create.body["data"]["orderCreate"]["order"]["updatedAt"]
    );

    let state = state_snapshot(&proxy);
    let metadata = &state["stagedState"]["orders"][sent_order["id"].as_str().unwrap()]
        ["__draftProxyInvoiceSend"];
    assert_eq!(metadata["deliveryStatus"], json!("STAGED_NO_DELIVERY"));
    assert_eq!(metadata["delivered"], json!(false));
    assert_eq!(
        metadata["email"]["to"],
        json!("invoice-recipient@example.test")
    );
    assert_eq!(metadata["email"]["subject"], json!("Local invoice"));
    assert_eq!(
        metadata["email"]["customMessage"],
        json!("No delivery should occur")
    );

    let log = log_snapshot(&proxy);
    let entries = log["entries"].as_array().expect("log entries");
    assert_eq!(entries.len(), 2);
    assert_eq!(
        entries[1]["interpreted"]["primaryRootField"],
        json!("orderInvoiceSend")
    );
    assert_eq!(entries[1]["status"], json!("staged"));
    assert!(entries[1]["rawBody"]
        .as_str()
        .expect("invoice raw body")
        .contains("orderInvoiceSend"));

    let dump = proxy.process_request(request_with_body("POST", "/__meta/dump", ""));
    let mut restored = snapshot_proxy();
    let restore = restored.process_request(request_with_body(
        "POST",
        "/__meta/restore",
        &dump.body.to_string(),
    ));
    assert_eq!(restore.status, 200);
    let read_back = restored.process_request(json_graphql_request(
        r#"
        query ReadInvoiceSentOrder($id: ID!) {
          order(id: $id) { id name email updatedAt }
        }
        "#,
        json!({ "id": sent_order["id"] }),
    ));
    assert_eq!(read_back.body["data"]["order"], sent_order);
    assert_eq!(
        state_snapshot(&restored)["stagedState"]["orders"][sent_order["id"].as_str().unwrap()]
            ["__draftProxyInvoiceSend"],
        metadata.clone()
    );
}

#[test]
fn order_invoice_send_live_hybrid_hydrates_by_query_and_validation_does_not_mutate() {
    let upstream_calls = Arc::new(Mutex::new(Vec::<Value>::new()));
    let captured_calls = Arc::clone(&upstream_calls);
    let order_id = "gid://shopify/Order/live-invoice";
    let mut proxy =
        configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport(move |request| {
            let body: Value = serde_json::from_str(&request.body).expect("upstream request body");
            captured_calls.lock().unwrap().push(body.clone());
            assert!(
                body["query"]
                    .as_str()
                    .unwrap_or_default()
                    .starts_with("query "),
                "invoice-send must hydrate with a read query, got {body}"
            );
            assert!(
                !body["query"]
                    .as_str()
                    .unwrap_or_default()
                    .contains("orderInvoiceSend"),
                "invoice-send must not forward the mutation upstream"
            );
            Response {
                status: 200,
                headers: Default::default(),
                body: json!({
                    "data": {
                        "order": {
                            "id": order_id,
                            "name": "#live-invoice",
                            "closed": false,
                            "closedAt": Value::Null,
                            "cancelledAt": Value::Null,
                            "cancelReason": Value::Null,
                            "displayFinancialStatus": Value::Null,
                            "paymentGatewayNames": [],
                            "totalOutstandingSet": {
                                "shopMoney": { "amount": "16.0", "currencyCode": "USD" }
                            },
                            "currentTotalPriceSet": {
                                "shopMoney": { "amount": "16.0", "currencyCode": "USD" }
                            },
                            "customer": {
                                "id": "gid://shopify/Customer/live-invoice",
                                "email": "live-invoice@example.test",
                                "displayName": "live-invoice@example.test"
                            },
                            "transactions": []
                        }
                    }
                }),
            }
        });

    let send = proxy.process_request(json_graphql_request(
        include_str!("../../config/parity-requests/orders/orderInvoiceSend-parity.graphql"),
        json!({
            "id": order_id,
            "email": {
                "to": "live-invoice@example.test",
                "subject": "Live invoice",
                "customMessage": "Hydrate only"
            }
        }),
    ));
    assert_eq!(send.status, 200);
    assert_eq!(
        send.body["data"]["orderInvoiceSend"]["userErrors"],
        json!([])
    );
    assert_eq!(
        send.body["data"]["orderInvoiceSend"]["order"]["id"],
        json!(order_id)
    );
    let calls = upstream_calls.lock().unwrap();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0]["operationName"], json!("OrdersOrderHydrate"));
    assert_eq!(calls[0]["variables"], json!({ "id": order_id }));
    assert!(calls[0]["query"]
        .as_str()
        .unwrap_or_default()
        .starts_with("query OrderManagementDownstreamRead"));
    drop(calls);

    let state_before_invalid = state_snapshot(&proxy);
    let log_before_invalid = log_snapshot(&proxy);
    let invalid = proxy.process_request(json_graphql_request(
        r#"
        mutation InvalidInvoiceRecipient($id: ID!, $email: EmailInput) {
          orderInvoiceSend(id: $id, email: $email) {
            order { id }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "id": order_id,
            "email": { "to": "not an email" }
        }),
    ));
    assert_eq!(invalid.status, 200);
    assert_eq!(
        invalid.body["data"]["orderInvoiceSend"]["order"],
        Value::Null
    );
    assert_eq!(
        invalid.body["data"]["orderInvoiceSend"]["userErrors"],
        json!([{
            "field": null,
            "message": "To is invalid",
            "code": "ORDER_INVOICE_SEND_UNSUCCESSFUL"
        }])
    );
    assert_eq!(upstream_calls.lock().unwrap().len(), 1);
    assert_eq!(state_snapshot(&proxy), state_before_invalid);
    assert_eq!(log_snapshot(&proxy), log_before_invalid);
}

#[test]
fn order_capture_rejects_boolean_final_capture_for_manual_gateway_without_side_effects() {
    let mut proxy = snapshot_proxy();

    let create = proxy.process_request(json_graphql_request(
        &current_order_payment_document(include_str!(
            "../../config/parity-requests/orders/order-payment-create-local-staging.graphql"
        )),
        json!({
            "order": {
                "currency": "CAD",
                "transactions": [{
                    "kind": "AUTHORIZATION",
                    "status": "SUCCESS",
                    "gateway": "manual",
                    "amountSet": { "shopMoney": { "amount": "20.00", "currencyCode": "CAD" } }
                }],
                "lineItems": [{
                    "title": "manual final capture unsupported",
                    "quantity": 1,
                    "priceSet": { "shopMoney": { "amount": "20.00", "currencyCode": "CAD" } }
                }]
            }
        }),
    ));
    assert_eq!(create.status, 200);
    assert_eq!(create.body["data"]["orderCreate"]["userErrors"], json!([]));
    let order_id = create.body["data"]["orderCreate"]["order"]["id"]
        .as_str()
        .expect("order id")
        .to_string();
    let parent_transaction_id = create.body["data"]["orderCreate"]["order"]["transactions"][0]
        ["id"]
        .as_str()
        .expect("parent transaction id")
        .to_string();
    let expected_error = json!([{
        "field": null,
        "message": "Setting final capture is not supported for this transaction's payment gateway. Please remove the parameter or set it to null, then try again."
    }]);

    for final_capture in [true, false] {
        let log_before = log_snapshot(&proxy);
        let state_before = state_snapshot(&proxy);
        let order_before = state_before["stagedState"]["orders"][&order_id].clone();

        let capture = proxy.process_request(json_graphql_request(
            &current_order_capture_document(include_str!(
                "../../config/parity-requests/orders/order-payment-capture-local-staging.graphql"
            )),
            json!({
                "input": {
                    "id": order_id,
                    "parentTransactionId": parent_transaction_id,
                    "amount": "5.00",
                    "currency": "CAD",
                    "finalCapture": final_capture
                }
            }),
        ));

        assert_eq!(capture.status, 200);
        assert_eq!(
            capture.body["data"]["orderCapture"]["transaction"],
            Value::Null
        );
        assert_eq!(
            capture.body["data"]["orderCapture"]["userErrors"],
            expected_error
        );
        assert_eq!(log_snapshot(&proxy), log_before);
        assert_eq!(
            state_snapshot(&proxy)["stagedState"]["orders"][&order_id],
            order_before
        );
    }
}

#[test]
fn order_payment_transactions_dispatch_by_root_for_ordinary_operation_names() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../fixtures/conformance/local-runtime/2026-04/orders/order-payment-transaction-local-staging.json"
    ))
    .unwrap();

    let mut proxy = snapshot_proxy();
    let create = proxy.process_request(json_graphql_request(
        &current_order_payment_document(include_str!(
            "../../config/parity-requests/orders/order-payment-non-recording-create.graphql"
        )),
        fixture["paymentCaptureFlow"]["create"]["variables"].clone(),
    ));
    assert_eq!(create.status, 200);
    assert_eq!(
        create.body["data"]["orderCreate"]["order"]["displayFinancialStatus"],
        json!("AUTHORIZED")
    );
    let order_id = create.body["data"]["orderCreate"]["order"]["id"].clone();
    let authorization_id =
        create.body["data"]["orderCreate"]["order"]["transactions"][0]["id"].clone();

    let capture = proxy.process_request(json_graphql_request(
        &current_order_capture_document(include_str!(
            "../../config/parity-requests/orders/order-payment-non-recording-capture.graphql"
        )),
        json!({
            "input": {
                "id": order_id.clone(),
                "parentTransactionId": authorization_id.clone(),
                "amount": "10.00",
                "currency": "CAD"
            }
        }),
    ));
    assert_eq!(capture.status, 200);
    let captured_order = read_order_payment_projection(&mut proxy, order_id.clone());
    assert_eq!(
        captured_order["displayFinancialStatus"],
        json!("PARTIALLY_PAID")
    );

    let read_after_capture = proxy.process_request(json_graphql_request(
        &current_order_payment_document(include_str!(
            "../../config/parity-requests/orders/order-payment-non-recording-read.graphql"
        )),
        json!({ "id": order_id.clone() }),
    ));
    assert_eq!(
        read_after_capture.body["data"]["order"]["displayFinancialStatus"],
        json!("PARTIALLY_PAID")
    );

    let mandate = proxy.process_request(json_graphql_request(
        &current_order_mandate_document(include_str!(
            "../../config/parity-requests/orders/order-payment-non-recording-mandate.graphql"
        )),
        json!({
            "id": order_id.clone(),
            "mandateId": "gid://shopify/PaymentMandate/non-recording-payment",
            "idempotencyKey": "ordinary-operation-name-payment",
            "amount": { "amount": "15.00", "currencyCode": "CAD" }
        }),
    ));
    assert_eq!(mandate.status, 200);
    assert_eq!(
        mandate.body["data"]["orderCreateMandatePayment"]["userErrors"],
        json!([])
    );
    assert_eq!(
        read_order_payment_projection(&mut proxy, order_id)["displayFinancialStatus"],
        json!("PAID")
    );

    let mut void_proxy = snapshot_proxy();
    let void_create = void_proxy.process_request(json_graphql_request(
        &current_order_payment_document(include_str!(
            "../../config/parity-requests/orders/order-payment-non-recording-create.graphql"
        )),
        fixture["voidFlow"]["create"]["variables"].clone(),
    ));
    let void_response = void_proxy.process_request(json_graphql_request(
        &current_order_payment_document(include_str!(
            "../../config/parity-requests/orders/order-payment-non-recording-void.graphql"
        )),
        json!({
            "id": void_create.body["data"]["orderCreate"]["order"]["transactions"][0]["id"].clone()
        }),
    ));
    assert_eq!(void_response.status, 200);
    assert_eq!(
        void_response.body["data"]["transactionVoid"]["transaction"]["kind"],
        json!("VOID")
    );
}

#[test]
fn order_capture_accepts_omitted_currency_for_single_currency_order() {
    let mut proxy = snapshot_proxy();

    let create = proxy.process_request(json_graphql_request(
        &current_order_payment_document(include_str!(
            "../../config/parity-requests/orders/order-payment-create-local-staging.graphql"
        )),
        json!({
            "order": {
                "currency": "CAD",
                "transactions": [{
                    "kind": "AUTHORIZATION",
                    "status": "SUCCESS",
                    "gateway": "manual",
                    "amountSet": { "shopMoney": { "amount": "20.00", "currencyCode": "CAD" } }
                }],
                "lineItems": [{
                    "title": "single currency omitted capture",
                    "quantity": 1,
                    "priceSet": { "shopMoney": { "amount": "20.00", "currencyCode": "CAD" } }
                }]
            }
        }),
    ));
    assert_eq!(create.status, 200);
    assert_eq!(create.body["data"]["orderCreate"]["userErrors"], json!([]));
    let order_id = create.body["data"]["orderCreate"]["order"]["id"].clone();
    let parent_transaction_id =
        create.body["data"]["orderCreate"]["order"]["transactions"][0]["id"].clone();

    let capture = proxy.process_request(json_graphql_request(
        &current_order_capture_document(include_str!(
            "../../config/parity-requests/orders/order-payment-capture-local-staging.graphql"
        )),
        json!({
            "input": {
                "id": order_id.clone(),
                "parentTransactionId": parent_transaction_id,
                "amount": "10.00"
            }
        }),
    ));

    assert_eq!(capture.status, 200);
    assert_eq!(
        capture.body["data"]["orderCapture"]["userErrors"],
        json!([])
    );
    assert_eq!(
        capture.body["data"]["orderCapture"]["transaction"]["kind"],
        json!("CAPTURE")
    );
    assert_eq!(
        capture.body["data"]["orderCapture"]["transaction"]["amountSet"]["shopMoney"],
        json!({ "amount": "10.0", "currencyCode": "CAD" })
    );
    assert_eq!(
        read_order_payment_projection(&mut proxy, order_id)["displayFinancialStatus"],
        json!("PARTIALLY_PAID")
    );
}

#[test]
fn order_capture_zero_amount_uses_captured_public_error_without_code() {
    let mut proxy = snapshot_proxy();

    let create = proxy.process_request(json_graphql_request(
        &current_order_payment_document(include_str!(
            "../../config/parity-requests/orders/order-payment-create-local-staging.graphql"
        )),
        json!({
            "order": {
                "currency": "CAD",
                "transactions": [{
                    "kind": "AUTHORIZATION",
                    "status": "SUCCESS",
                    "gateway": "manual",
                    "amountSet": { "shopMoney": { "amount": "12.50", "currencyCode": "CAD" } }
                }],
                "lineItems": [{
                    "title": "single currency zero amount capture",
                    "quantity": 1,
                    "priceSet": { "shopMoney": { "amount": "12.50", "currencyCode": "CAD" } }
                }]
            }
        }),
    ));
    assert_eq!(create.status, 200);
    assert_eq!(create.body["data"]["orderCreate"]["userErrors"], json!([]));

    let capture = proxy.process_request(json_graphql_request(
        r#"
        mutation ZeroAmountCapture($input: OrderCaptureInput!) {
          orderCapture(input: $input) {
            transaction { id }
            userErrors { field message  }
          }
        }
        "#,
        json!({
            "input": {
                "id": create.body["data"]["orderCreate"]["order"]["id"].clone(),
                "parentTransactionId": create.body["data"]["orderCreate"]["order"]["transactions"][0]["id"].clone(),
                "amount": "0.00"
            }
        }),
    ));

    assert_eq!(capture.status, 200);
    assert_eq!(
        capture.body["data"]["orderCapture"]["transaction"],
        Value::Null
    );
    assert_eq!(
        capture.body["data"]["orderCapture"]["userErrors"],
        json!([{
            "field": null,
            "message": "Amount must be greater than zero for capture transactions"
        }])
    );
}

#[test]
fn order_payment_create_preserves_empty_payment_view_without_transactions() {
    let mut proxy = snapshot_proxy();
    let plain_status = proxy.process_request(json_graphql_request(
        r#"
        mutation CreatePlainUnpaidOrder($order: OrderCreateOrderInput!) {
          orderCreate(order: $order) {
            order {
              id
              displayFinancialStatus
            }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "order": {
                "currency": "USD",
                "lineItems": [{
                    "title": "Plain unpaid order",
                    "quantity": 1,
                    "priceSet": { "shopMoney": { "amount": "20.00", "currencyCode": "USD" } }
                }]
            }
        }),
    ));
    assert_eq!(
        plain_status.body["data"]["orderCreate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        plain_status.body["data"]["orderCreate"]["order"]["displayFinancialStatus"],
        Value::Null
    );

    let payment_projection = proxy.process_request(json_graphql_request(
        r#"
        mutation CreatePaymentProjectionOrder($order: OrderCreateOrderInput!) {
          orderCreate(order: $order) {
            order {
              id
              displayFinancialStatus
              capturable
              currentTotalPriceSet { shopMoney { amount currencyCode } }
              totalCapturable
              totalCapturableSet { shopMoney { amount currencyCode } }
              totalOutstandingSet { shopMoney { amount currencyCode } }
              totalReceivedSet { shopMoney { amount currencyCode } }
              netPaymentSet { shopMoney { amount currencyCode } }
              paymentGatewayNames
              transactions {
                id
                kind
                status
                gateway
                amountSet { shopMoney { amount currencyCode } }
              }
            }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "order": {
                "currency": "USD",
                "lineItems": [{
                    "title": "No transaction payment projection",
                    "quantity": 2,
                    "priceSet": { "shopMoney": { "amount": "13.25", "currencyCode": "USD" } }
                }]
            }
        }),
    ));
    assert_eq!(
        payment_projection.body["data"]["orderCreate"]["userErrors"],
        json!([])
    );
    let projected_order = &payment_projection.body["data"]["orderCreate"]["order"];
    assert_eq!(projected_order["displayFinancialStatus"], Value::Null);
    assert_eq!(projected_order["capturable"], json!(false));
    assert_eq!(
        projected_order["currentTotalPriceSet"]["shopMoney"],
        json!({ "amount": "26.5", "currencyCode": "USD" })
    );
    assert_eq!(projected_order["totalCapturable"], json!("0.0"));
    assert_eq!(
        projected_order["totalCapturableSet"]["shopMoney"],
        json!({ "amount": "0.0", "currencyCode": "USD" })
    );
    assert_eq!(
        projected_order["totalOutstandingSet"]["shopMoney"],
        json!({ "amount": "26.5", "currencyCode": "USD" })
    );
    assert_eq!(
        projected_order["totalReceivedSet"]["shopMoney"],
        json!({ "amount": "0.0", "currencyCode": "USD" })
    );
    assert_eq!(
        projected_order["netPaymentSet"]["shopMoney"],
        json!({ "amount": "0.0", "currencyCode": "USD" })
    );
    assert_eq!(projected_order["paymentGatewayNames"], json!([]));
    assert_eq!(projected_order["transactions"], json!([]));
}

#[test]
fn order_payment_create_preserves_transaction_backed_payment_view() {
    let mut proxy = snapshot_proxy();
    let create_query = r#"
        mutation CreatePaymentProjectionOrder($order: OrderCreateOrderInput!) {
          orderCreate(order: $order) {
            order {
              id
              displayFinancialStatus
              capturable
              totalCapturableSet { shopMoney { amount currencyCode } }
              paymentGatewayNames
              transactions {
                id
                kind
                status
                gateway
                amountSet { shopMoney { amount currencyCode } }
              }
            }
            userErrors { field message code }
          }
        }
    "#;

    let gateway_create = proxy.process_request(json_graphql_request(
        create_query,
        json!({
            "order": {
                "currency": "CAD",
                "transactions": [{
                    "kind": "AUTHORIZATION",
                    "status": "SUCCESS",
                    "gateway": "shopify_payments",
                    "amountSet": { "shopMoney": { "amount": "31.90", "currencyCode": "CAD" } }
                }],
                "lineItems": [{
                    "title": "Gateway propagation",
                    "quantity": 1,
                    "priceSet": { "shopMoney": { "amount": "31.90", "currencyCode": "CAD" } }
                }]
            }
        }),
    ));
    assert_eq!(
        gateway_create.body["data"]["orderCreate"]["userErrors"],
        json!([])
    );
    let gateway_order = &gateway_create.body["data"]["orderCreate"]["order"];
    assert_eq!(
        gateway_order["paymentGatewayNames"],
        json!(["shopify_payments"])
    );
    assert_eq!(gateway_order["displayFinancialStatus"], json!("AUTHORIZED"));
    assert_eq!(gateway_order["capturable"], json!(true));
    assert_eq!(
        gateway_order["totalCapturableSet"]["shopMoney"],
        json!({ "amount": "31.9", "currencyCode": "CAD" })
    );
    assert_eq!(
        gateway_order["transactions"][0]["gateway"],
        json!("shopify_payments")
    );

    for (kind, expected_status) in [("SALE", "PAID"), ("CAPTURE", "PAID")] {
        let create = proxy.process_request(json_graphql_request(
            create_query,
            json!({
                "order": {
                    "currency": "CAD",
                    "transactions": [{
                        "kind": kind,
                        "status": "SUCCESS",
                        "gateway": "manual",
                        "amountSet": { "shopMoney": { "amount": "12.00", "currencyCode": "CAD" } }
                    }],
                    "lineItems": [{
                        "title": format!("{kind} transaction propagation"),
                        "quantity": 1,
                        "priceSet": { "shopMoney": { "amount": "12.00", "currencyCode": "CAD" } }
                    }]
                }
            }),
        ));
        assert_eq!(create.body["data"]["orderCreate"]["userErrors"], json!([]));
        let order = &create.body["data"]["orderCreate"]["order"];
        assert_eq!(order["displayFinancialStatus"], json!(expected_status));
        assert_eq!(order["capturable"], json!(false));
        assert_eq!(order["paymentGatewayNames"], json!(["manual"]));
        assert_eq!(order["transactions"][0]["kind"], json!(kind));
        assert_eq!(
            order["transactions"][0]["amountSet"]["shopMoney"],
            json!({ "amount": "12.0", "currencyCode": "CAD" })
        );
    }
}

#[test]
fn order_payment_transactions_use_order_transaction_state_not_magic_values() {
    let mut proxy = snapshot_proxy();

    let create_a = proxy.process_request(json_graphql_request(
        &current_order_payment_document(include_str!(
            "../../config/parity-requests/orders/order-payment-create-local-staging.graphql"
        )),
        json!({
            "order": {
                "currency": "CAD",
                "transactions": [{
                    "kind": "AUTHORIZATION",
                    "status": "SUCCESS",
                    "gateway": "manual",
                    "amountSet": { "shopMoney": { "amount": "42.00", "currencyCode": "CAD" } }
                }],
                "lineItems": [{
                    "title": "capture arbitrary amount",
                    "quantity": 1,
                    "priceSet": { "shopMoney": { "amount": "42.00", "currencyCode": "CAD" } }
                }]
            }
        }),
    ));
    let order_a_id = create_a.body["data"]["orderCreate"]["order"]["id"].clone();
    let parent_a_id =
        create_a.body["data"]["orderCreate"]["order"]["transactions"][0]["id"].clone();

    let capture_a = proxy.process_request(json_graphql_request(
        &current_order_capture_document(include_str!(
            "../../config/parity-requests/orders/order-payment-capture-local-staging.graphql"
        )),
        json!({
            "input": {
                "id": order_a_id.clone(),
                "parentTransactionId": parent_a_id.clone(),
                "amount": "42.00",
                "currency": "CAD"
            }
        }),
    ));
    assert_eq!(
        capture_a.body["data"]["orderCapture"]["userErrors"],
        json!([])
    );
    assert_eq!(
        capture_a.body["data"]["orderCapture"]["transaction"]["amountSet"]["shopMoney"]["amount"],
        json!("42.0")
    );
    assert_ne!(
        capture_a.body["data"]["orderCapture"]["transaction"]["id"],
        json!("gid://shopify/OrderTransaction/7")
    );
    assert_eq!(
        read_order_payment_projection(&mut proxy, order_a_id.clone())["displayFinancialStatus"],
        json!("PAID")
    );
    let non_authorization_parent = proxy.process_request(json_graphql_request(
        &current_order_capture_document(include_str!(
            "../../config/parity-requests/orders/order-payment-capture-local-staging.graphql"
        )),
        json!({
            "input": {
                "id": order_a_id,
                "parentTransactionId": capture_a.body["data"]["orderCapture"]["transaction"]["id"].clone(),
                "amount": "5.00",
                "currency": "CAD"
            }
        }),
    ));
    assert_eq!(
        non_authorization_parent.body["data"]["orderCapture"]["transaction"],
        Value::Null
    );
    assert_eq!(
        non_authorization_parent.body["data"]["orderCapture"]["userErrors"],
        json!([{
            "field": ["parentTransactionId"],
            "message": "Parent transaction must be a successful authorization"
        }])
    );

    let create_b = proxy.process_request(json_graphql_request(
        &current_order_payment_document(include_str!(
            "../../config/parity-requests/orders/order-payment-create-local-staging.graphql"
        )),
        json!({
            "order": {
                "currency": "CAD",
                "transactions": [{
                    "kind": "AUTHORIZATION",
                    "status": "SUCCESS",
                    "gateway": "manual",
                    "amountSet": { "shopMoney": { "amount": "20.00", "currencyCode": "CAD" } }
                }],
                "lineItems": [{
                    "title": "over capture computed",
                    "quantity": 1,
                    "priceSet": { "shopMoney": { "amount": "20.00", "currencyCode": "CAD" } }
                }]
            }
        }),
    ));
    let order_b_id = create_b.body["data"]["orderCreate"]["order"]["id"].clone();
    let parent_b_id =
        create_b.body["data"]["orderCreate"]["order"]["transactions"][0]["id"].clone();

    let over_capture_b = proxy.process_request(json_graphql_request(
        &current_order_capture_document(include_str!(
            "../../config/parity-requests/orders/order-payment-capture-local-staging.graphql"
        )),
        json!({
            "input": {
                "id": order_b_id,
                "parentTransactionId": parent_b_id.clone(),
                "amount": "30.00",
                "currency": "CAD"
            }
        }),
    ));
    assert_eq!(
        over_capture_b.body["data"]["orderCapture"]["transaction"],
        Value::Null
    );
    assert_eq!(
        over_capture_b.body["data"]["orderCapture"]["userErrors"],
        json!([{
            "field": null,
            "message": "Cannot capture more than the authorized 20.00 for this payment."
        }])
    );

    let void_b = proxy.process_request(json_graphql_request(
        &current_order_payment_document(include_str!(
            "../../config/parity-requests/orders/order-payment-void-local-staging.graphql"
        )),
        json!({ "id": parent_b_id }),
    ));
    assert_eq!(
        void_b.body["data"]["transactionVoid"]["userErrors"],
        json!([])
    );
    assert_eq!(
        void_b.body["data"]["transactionVoid"]["transaction"]["kind"],
        json!("VOID")
    );
    assert_eq!(
        void_b.body["data"]["transactionVoid"]["transaction"]["amountSet"]["shopMoney"]["amount"],
        json!("20.0")
    );

    let missing_void = proxy.process_request(json_graphql_request(
        &current_order_payment_document(include_str!(
            "../../config/parity-requests/orders/order-payment-void-local-staging.graphql"
        )),
        json!({ "id": "gid://shopify/OrderTransaction/does-not-exist" }),
    ));
    assert_eq!(
        missing_void.body["data"]["transactionVoid"]["userErrors"][0]["field"],
        json!(["parentTransactionId"])
    );
    assert_eq!(
        missing_void.body["data"]["transactionVoid"]["userErrors"][0]["message"],
        json!("Transaction does not exist")
    );

    let log = log_snapshot(&proxy);
    let entries = log["entries"].as_array().expect("log entries");
    assert_eq!(entries.len(), 4);
    assert!(entries[0]["rawBody"]
        .as_str()
        .expect("raw body")
        .contains("OrderPaymentCreate"));
    assert!(entries[1]["rawBody"]
        .as_str()
        .expect("raw body")
        .contains("OrderPaymentCapture"));
    assert!(entries[3]["rawBody"]
        .as_str()
        .expect("raw body")
        .contains("OrderPaymentVoid"));
}

#[test]
fn transaction_void_code_flow_preserves_payment_currency_without_order_currency() {
    let mut proxy = snapshot_proxy();

    let create = proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/payments/transaction-void-codes-order-create.graphql"
        ),
        json!({
            "order": {
                "email": "transaction-void-codes@example.com",
                "test": true,
                "lineItems": [{
                    "title": "transaction void code parity",
                    "quantity": 1,
                    "priceSet": {
                        "shopMoney": {
                            "amount": "25.00",
                            "currencyCode": "CAD"
                        }
                    },
                    "requiresShipping": false,
                    "taxable": false
                }],
                "transactions": [{
                    "kind": "AUTHORIZATION",
                    "status": "SUCCESS",
                    "gateway": "manual",
                    "test": true,
                    "amountSet": {
                        "shopMoney": {
                            "amount": "25.00",
                            "currencyCode": "CAD"
                        }
                    }
                }]
            },
            "options": null
        }),
    ));
    assert_eq!(create.body["data"]["orderCreate"]["userErrors"], json!([]));
    assert_eq!(
        create.body["data"]["orderCreate"]["order"]["transactions"][0]["amountSet"]["shopMoney"]
            ["currencyCode"],
        json!("CAD")
    );
    let order_id = create.body["data"]["orderCreate"]["order"]["id"].clone();
    let parent_transaction_id =
        create.body["data"]["orderCreate"]["order"]["transactions"][0]["id"].clone();

    let capture = proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/payments/transaction-void-codes-order-capture.graphql"
        ),
        json!({
            "input": {
                "id": order_id,
                "parentTransactionId": parent_transaction_id.clone(),
                "amount": "25.00",
                "currency": "CAD"
            }
        }),
    ));
    assert_eq!(
        capture.body["data"]["orderCapture"]["userErrors"],
        json!([])
    );
    assert_eq!(
        capture.body["data"]["orderCapture"]["transaction"]["kind"],
        json!("CAPTURE")
    );

    let void = proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/payments/transaction-void-codes-transaction-void.graphql"
        ),
        json!({ "id": parent_transaction_id }),
    ));
    assert_eq!(
        void.body["data"]["transactionVoid"]["transaction"],
        Value::Null
    );
    assert_eq!(
        void.body["data"]["transactionVoid"]["userErrors"][0],
        json!({
            "field": ["parentTransactionId"],
            "message": "Parent transaction require a parent_id referring to a voidable transaction",
            "code": "AUTH_NOT_VOIDABLE"
        })
    );
}

#[test]
fn order_mark_as_paid_stages_from_stored_order_without_money_selection() {
    let mut proxy = snapshot_proxy();

    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateMarkableOrder($order: OrderCreateOrderInput!) {
          orderCreate(order: $order) {
            order { id displayFinancialStatus }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "order": {
                "email": "markable-order@example.test",
                "currency": "CAD",
                "presentmentCurrency": "USD",
                "financialStatus": "PENDING",
                "lineItems": [{
                    "title": "markable",
                    "quantity": 1,
                    "priceSet": {
                        "shopMoney": { "amount": "17.01", "currencyCode": "CAD" },
                        "presentmentMoney": { "amount": "12.50", "currencyCode": "USD" }
                    },
                    "taxable": false
                }]
            }
        }),
    ));
    assert_eq!(create.body["data"]["orderCreate"]["userErrors"], json!([]));
    let order_id = create.body["data"]["orderCreate"]["order"]["id"].clone();

    let mark = proxy.process_request(json_graphql_request(
        r#"
        mutation MarkAsPaidWithoutMoneySelection($input: OrderMarkAsPaidInput!) {
          orderMarkAsPaid(input: $input) {
            order { id displayFinancialStatus }
            userErrors { field message }
          }
        }
        "#,
        json!({ "input": { "id": order_id.clone() } }),
    ));
    assert_eq!(mark.status, 200);
    assert_eq!(
        mark.body["data"]["orderMarkAsPaid"]["order"],
        json!({
            "id": order_id,
            "displayFinancialStatus": "PAID"
        })
    );
    assert_eq!(
        mark.body["data"]["orderMarkAsPaid"]["userErrors"],
        json!([])
    );

    let read_after = proxy.process_request(json_graphql_request(
        r#"
        query ReadPaidOrder($id: ID!) {
          order(id: $id) {
            id
            displayFinancialStatus
            totalOutstandingSet {
              shopMoney { amount currencyCode }
              presentmentMoney { amount currencyCode }
            }
            totalReceivedSet {
              shopMoney { amount currencyCode }
              presentmentMoney { amount currencyCode }
            }
            netPaymentSet {
              shopMoney { amount currencyCode }
              presentmentMoney { amount currencyCode }
            }
            paymentGatewayNames
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
          }
        }
        "#,
        json!({ "id": mark.body["data"]["orderMarkAsPaid"]["order"]["id"].clone() }),
    ));
    let paid_order = &read_after.body["data"]["order"];
    assert_eq!(paid_order["displayFinancialStatus"], json!("PAID"));
    assert_eq!(
        paid_order["totalOutstandingSet"],
        json!({
            "shopMoney": { "amount": "0.0", "currencyCode": "CAD" },
            "presentmentMoney": { "amount": "0.0", "currencyCode": "USD" }
        })
    );
    assert_eq!(
        paid_order["totalReceivedSet"],
        json!({
            "shopMoney": { "amount": "17.01", "currencyCode": "CAD" },
            "presentmentMoney": { "amount": "17.01", "currencyCode": "USD" }
        })
    );
    assert_eq!(paid_order["netPaymentSet"], paid_order["totalReceivedSet"]);
    assert_eq!(paid_order["paymentGatewayNames"], json!(["manual"]));
    assert_eq!(paid_order["transactions"].as_array().unwrap().len(), 1);
    assert_eq!(paid_order["transactions"][0]["kind"], json!("SALE"));
    assert_eq!(
        paid_order["transactions"][0]["amountSet"],
        paid_order["totalReceivedSet"]
    );

    let log = log_snapshot(&proxy);
    let entries = log["entries"].as_array().expect("log entries");
    assert_eq!(entries.len(), 2);
    assert!(entries[1]["rawBody"]
        .as_str()
        .expect("raw body")
        .contains("MarkAsPaidWithoutMoneySelection"));
}

#[test]
fn order_mark_as_paid_rejects_unknown_and_non_markable_orders_without_staging() {
    let mut proxy = snapshot_proxy();

    let unknown = proxy.process_request(json_graphql_request(
        r#"
        mutation UnknownMarkAsPaid($input: OrderMarkAsPaidInput!) {
          orderMarkAsPaid(input: $input) {
            order { id totalOutstandingSet { presentmentMoney { amount currencyCode } } }
            userErrors { field message }
          }
        }
        "#,
        json!({ "input": { "id": "gid://shopify/Order/999999" } }),
    ));
    assert_eq!(unknown.status, 200);
    assert_eq!(
        unknown.body["data"]["orderMarkAsPaid"]["order"],
        Value::Null
    );
    assert_eq!(
        unknown.body["data"]["orderMarkAsPaid"]["userErrors"][0]["field"],
        json!(["id"])
    );
    assert_eq!(
        unknown.body["data"]["orderMarkAsPaid"]["userErrors"][0]["message"],
        json!("Order does not exist")
    );
    assert_eq!(log_snapshot(&proxy)["entries"], json!([]));
    assert_eq!(state_snapshot(&proxy)["stagedState"]["orders"], json!({}));

    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateRepeatMarkAsPaidOrder($order: OrderCreateOrderInput!) {
          orderCreate(order: $order) {
            order { id displayFinancialStatus }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "order": {
                "email": "repeat-mark-paid@example.test",
                "currency": "USD",
                "financialStatus": "PENDING",
                "lineItems": [{
                    "title": "repeat mark paid",
                    "quantity": 1,
                    "priceSet": { "shopMoney": { "amount": "12.00", "currencyCode": "USD" } },
                    "taxLines": [{
                        "title": "Tax",
                        "rate": 0.125,
                        "priceSet": { "shopMoney": { "amount": "1.50", "currencyCode": "USD" } }
                    }]
                }]
            }
        }),
    ));
    let order_id = create.body["data"]["orderCreate"]["order"]["id"].clone();
    let mark_query = r#"
        mutation MarkAsPaidMoneySelection($input: OrderMarkAsPaidInput!) {
          orderMarkAsPaid(input: $input) {
            order {
              id
              displayFinancialStatus
              totalOutstandingSet {
                shopMoney { amount currencyCode }
                presentmentMoney { amount currencyCode }
              }
              transactions {
                kind
                status
                gateway
                amountSet {
                  shopMoney { amount currencyCode }
                  presentmentMoney { amount currencyCode }
                }
              }
            }
            userErrors { field message }
          }
        }
        "#;
    let first = proxy.process_request(json_graphql_request(
        mark_query,
        json!({ "input": { "id": order_id.clone() } }),
    ));
    assert_eq!(
        first.body["data"]["orderMarkAsPaid"]["userErrors"],
        json!([])
    );
    assert_eq!(
        first.body["data"]["orderMarkAsPaid"]["order"]["transactions"][0]["amountSet"]["shopMoney"]
            ["amount"],
        json!("13.5")
    );

    let repeat = proxy.process_request(json_graphql_request(
        mark_query,
        json!({ "input": { "id": order_id.clone() } }),
    ));
    assert_eq!(
        repeat.body["data"]["orderMarkAsPaid"]["userErrors"][0]["field"],
        json!(["id"])
    );
    assert_eq!(
        repeat.body["data"]["orderMarkAsPaid"]["userErrors"][0]["message"],
        json!("Order cannot be marked as paid.")
    );
    assert!(repeat.body["data"]["orderMarkAsPaid"]["userErrors"][0]
        .get("code")
        .is_none());
    assert_eq!(
        repeat.body["data"]["orderMarkAsPaid"]["order"]["transactions"]
            .as_array()
            .expect("transactions")
            .len(),
        1
    );

    let cancelled_create = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateCancelledMarkAsPaidOrder($order: OrderCreateOrderInput!) {
          orderCreate(order: $order) {
            order { id displayFinancialStatus }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "order": {
                "email": "cancelled-mark-paid@example.test",
                "currency": "USD",
                "financialStatus": "PENDING",
                "lineItems": [{
                    "title": "cancel then mark paid",
                    "quantity": 1,
                    "priceSet": { "shopMoney": { "amount": "10.00", "currencyCode": "USD" } }
                }]
            }
        }),
    ));
    let cancelled_id = cancelled_create.body["data"]["orderCreate"]["order"]["id"].clone();
    let cancel = proxy.process_request(json_graphql_request(
        r#"
        mutation CancelBeforeMarkAsPaid($orderId: ID!, $reason: OrderCancelReason!, $restock: Boolean!) {
          orderCancel(orderId: $orderId, reason: $reason, restock: $restock) {
            userErrors { field message  }
          }
        }
        "#,
        json!({ "orderId": cancelled_id.clone(), "reason": "OTHER", "restock": false }),
    ));
    assert_eq!(cancel.body["data"]["orderCancel"]["userErrors"], json!([]));
    let cancelled_mark = proxy.process_request(json_graphql_request(
        mark_query,
        json!({ "input": { "id": cancelled_id } }),
    ));
    assert_eq!(
        cancelled_mark.body["data"]["orderMarkAsPaid"]["userErrors"][0]["message"],
        json!("Order cannot be marked as paid.")
    );
    assert!(
        cancelled_mark.body["data"]["orderMarkAsPaid"]["userErrors"][0]
            .get("code")
            .is_none()
    );

    let log = log_snapshot(&proxy);
    let entries = log["entries"].as_array().expect("log entries");
    let staged_mark_as_paid_entries = entries
        .iter()
        .filter(|entry| entry["interpreted"]["primaryRootField"] == "orderMarkAsPaid")
        .count();
    assert_eq!(staged_mark_as_paid_entries, 1);
}

#[test]
fn money_bag_presentment_replays_order_payment_refund_and_edit_shapes() {
    let mut proxy = snapshot_proxy();

    let single_create = proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/orders/money-bag-presentment-single-create.graphql"
        ),
        json!({
            "order": {
                "currency": "CAD",
                "presentmentCurrency": "USD",
                "lineItems": [{
                    "title": "MoneyBag line",
                    "quantity": 1,
                    "priceSet": {
                        "shopMoney": { "amount": "12.00", "currencyCode": "CAD" },
                        "presentmentMoney": { "amount": "8.00", "currencyCode": "USD" }
                    },
                    "taxLines": [{
                        "title": "Line tax",
                        "rate": 0.125,
                        "priceSet": {
                            "shopMoney": { "amount": "1.50", "currencyCode": "CAD" },
                            "presentmentMoney": { "amount": "1.00", "currencyCode": "USD" }
                        }
                    }]
                }]
            }
        }),
    ));
    assert_eq!(
        single_create.body["data"]["orderCreate"]["userErrors"],
        json!([])
    );
    let order_id = single_create.body["data"]["orderCreate"]["order"]["id"].clone();
    assert_eq!(
        single_create.body["data"]["orderCreate"]["order"]["totalPriceSet"],
        json!({
            "shopMoney": { "amount": "13.5", "currencyCode": "CAD" },
            "presentmentMoney": { "amount": "9.0", "currencyCode": "USD" }
        })
    );

    let edit_begin = proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/orders/money-bag-presentment-order-edit-begin.graphql"
        ),
        json!({"id": order_id.clone()}),
    ));
    assert_eq!(
        edit_begin.body["data"]["orderEditBegin"]["userErrors"],
        json!([])
    );
    assert_ne!(
        edit_begin.body["data"]["orderEditBegin"]["calculatedOrder"]["id"],
        json!("gid://shopify/CalculatedOrder/7")
    );
    assert_eq!(
        edit_begin.body["data"]["orderEditBegin"]["calculatedOrder"]["originalOrder"]["id"],
        order_id
    );
    assert_eq!(
        edit_begin.body["data"]["orderEditBegin"]["calculatedOrder"]["totalPriceSet"],
        single_create.body["data"]["orderCreate"]["order"]["totalPriceSet"]
    );
    let calculated_order_id =
        edit_begin.body["data"]["orderEditBegin"]["calculatedOrder"]["id"].clone();

    let edit_commit = proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/orders/money-bag-presentment-order-edit-commit.graphql"
        ),
        json!({"id": calculated_order_id}),
    ));
    assert_eq!(
        edit_commit.body["data"]["orderEditCommit"],
        json!({
            "order": Value::Null,
            "userErrors": [{
                "field": ["id"],
                "message": "There must be at least one change to be made."
            }]
        })
    );

    let mark_as_paid = proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/orders/money-bag-presentment-mark-as-paid.graphql"
        ),
        json!({"input": {"id": order_id.clone()}}),
    ));
    assert_eq!(
        mark_as_paid.body["data"]["orderMarkAsPaid"]["order"]["transactions"][0]["amountSet"],
        single_create.body["data"]["orderCreate"]["order"]["totalPriceSet"]
    );

    let refund = proxy.process_request(json_graphql_request(
        include_str!("../../config/parity-requests/orders/money-bag-presentment-refund.graphql"),
        json!({"input": {"orderId": order_id.clone(), "currency": "USD", "allowOverRefunding": true, "transactions": [{"amount": "5.00", "gateway": "manual", "kind": "REFUND", "orderId": order_id.clone(), "parentId": mark_as_paid.body["data"]["orderMarkAsPaid"]["order"]["transactions"][0]["id"].clone()}]}}),
    ));
    assert_eq!(
        refund.body["data"]["refundCreate"]["refund"]["totalRefundedSet"],
        json!({
            "shopMoney": { "amount": "7.5", "currencyCode": "CAD" },
            "presentmentMoney": { "amount": "5.0", "currencyCode": "USD" }
        })
    );
}

#[test]
fn money_bag_order_create_uses_hydrated_shop_currency_without_input_currency() {
    let upstream_calls = Arc::new(Mutex::new(Vec::<Value>::new()));
    let captured_calls = Arc::clone(&upstream_calls);
    let mut proxy = configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport(
        move |request| {
            let body: Value =
                serde_json::from_str(&request.body).expect("shop pricing hydrate parses");
            assert_eq!(
                body["query"],
                json!("query DraftProxyShopPricingHydrate { shop { currencyCode taxesIncluded taxShipping } }")
            );
            captured_calls.lock().unwrap().push(body);
            Response {
                status: 200,
                headers: Default::default(),
                body: json!({
                    "data": {
                        "shop": {
                            "currencyCode": "CAD",
                            "taxesIncluded": false,
                            "taxShipping": false
                        }
                    }
                }),
            }
        },
    );

    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation MoneyBagCreateHydratesShopCurrency($order: OrderCreateOrderInput!) {
          orderCreate(order: $order) {
            order {
              id
              totalPriceSet { shopMoney { amount currencyCode } }
              lineItems(first: 1) {
                nodes {
                  originalUnitPriceSet { shopMoney { amount currencyCode } }
                }
              }
            }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "order": {
                "lineItems": [{
                    "variantId": "gid://shopify/ProductVariant/424242",
                    "quantity": 1
                }]
            }
        }),
    ));

    assert_eq!(create.status, 200);
    assert_eq!(create.body["data"]["orderCreate"]["userErrors"], json!([]));
    assert_eq!(
        create.body["data"]["orderCreate"]["order"]["totalPriceSet"]["shopMoney"],
        json!({ "amount": "0.0", "currencyCode": "CAD" })
    );
    assert_eq!(
        create.body["data"]["orderCreate"]["order"]["lineItems"]["nodes"][0]
            ["originalUnitPriceSet"]["shopMoney"],
        json!({ "amount": "0.0", "currencyCode": "CAD" })
    );
    assert_eq!(upstream_calls.lock().unwrap().len(), 1);
}

#[test]
fn payment_terms_order_create_uses_hydrated_shop_currency_without_input_currency() {
    let upstream_calls = Arc::new(Mutex::new(Vec::<Value>::new()));
    let captured_calls = Arc::clone(&upstream_calls);
    let mut proxy = configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport(
        move |request| {
            let body: Value =
                serde_json::from_str(&request.body).expect("shop pricing hydrate parses");
            assert_eq!(
                body["query"],
                json!("query DraftProxyShopPricingHydrate { shop { currencyCode taxesIncluded taxShipping } }")
            );
            captured_calls.lock().unwrap().push(body);
            Response {
                status: 200,
                headers: Default::default(),
                body: json!({
                    "data": {
                        "shop": {
                            "currencyCode": "CAD",
                            "taxesIncluded": false,
                            "taxShipping": false
                        }
                    }
                }),
            }
        },
    );

    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation PaymentTermsCreateHydratesShopCurrency($order: OrderCreateOrderInput!) {
          orderCreate(order: $order) {
            order {
              id
              currentTotalPriceSet { shopMoney { amount currencyCode } }
              paymentTerms { id }
            }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "order": {
                "lineItems": [{
                    "variantId": "gid://shopify/ProductVariant/424242",
                    "quantity": 1
                }]
            }
        }),
    ));

    assert_eq!(create.status, 200);
    assert_eq!(create.body["data"]["orderCreate"]["userErrors"], json!([]));
    assert_eq!(
        create.body["data"]["orderCreate"]["order"]["currentTotalPriceSet"]["shopMoney"],
        json!({ "amount": "0.0", "currencyCode": "CAD" })
    );
    assert_eq!(upstream_calls.lock().unwrap().len(), 1);
}

#[test]
fn money_bag_order_edit_sessions_use_target_order_and_outstanding_defaults() {
    let mut proxy = snapshot_proxy();
    let create_document = r#"
        mutation StageMoneyBagOrderForEdit($order: OrderCreateOrderInput!) {
          orderCreate(order: $order) {
            order {
              id
              currentTotalPriceSet { shopMoney { amount currencyCode } presentmentMoney { amount currencyCode } }
              totalPriceSet { shopMoney { amount currencyCode } presentmentMoney { amount currencyCode } }
              totalOutstandingSet { shopMoney { amount currencyCode } presentmentMoney { amount currencyCode } }
              lineItems(first: 1) {
                nodes {
                  originalUnitPriceSet {
                    shopMoney { amount currencyCode }
                    presentmentMoney { amount currencyCode }
                  }
                }
              }
            }
            userErrors { field message code }
          }
        }
    "#;
    let first_create = proxy.process_request(json_graphql_request(
        create_document,
        json!({
            "order": {
                "currency": "USD",
                "lineItems": [{
                    "title": "First money bag edit line",
                    "quantity": 1,
                    "priceSet": { "shopMoney": { "amount": "10.00", "currencyCode": "USD" } }
                }]
            }
        }),
    ));
    let second_create = proxy.process_request(json_graphql_request(
        create_document,
        json!({
            "order": {
                "currency": "CAD",
                "lineItems": [{
                    "title": "Second money bag edit line",
                    "quantity": 1,
                    "priceSet": { "shopMoney": { "amount": "22.00", "currencyCode": "CAD" } }
                }]
            }
        }),
    ));
    assert_eq!(
        first_create.body["data"]["orderCreate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        second_create.body["data"]["orderCreate"]["userErrors"],
        json!([])
    );
    let first_order_id = first_create.body["data"]["orderCreate"]["order"]["id"].clone();
    let second_order_id = second_create.body["data"]["orderCreate"]["order"]["id"].clone();

    let begin_document = r#"
        mutation BeginMoneyBagOrderEdit($id: ID!) {
          orderEditBegin(id: $id) {
            calculatedOrder {
              id
              originalOrder { id }
              totalPriceSet {
                shopMoney { amount currencyCode }
                presentmentMoney { amount currencyCode }
              }
            }
            userErrors { field message }
          }
        }
    "#;
    let first_begin = proxy.process_request(json_graphql_request(
        begin_document,
        json!({ "id": first_order_id }),
    ));
    let second_begin = proxy.process_request(json_graphql_request(
        begin_document,
        json!({ "id": second_order_id.clone() }),
    ));
    assert_eq!(
        first_begin.body["data"]["orderEditBegin"]["userErrors"],
        json!([])
    );
    assert_eq!(
        second_begin.body["data"]["orderEditBegin"]["userErrors"],
        json!([])
    );
    let first_calculated_id =
        first_begin.body["data"]["orderEditBegin"]["calculatedOrder"]["id"].clone();
    let second_calculated_id =
        second_begin.body["data"]["orderEditBegin"]["calculatedOrder"]["id"].clone();

    let refund = proxy.process_request(json_graphql_request(
        r#"
        mutation RefundMoneyBagOrderWithoutTransactions($input: RefundInput!) {
          refundCreate(input: $input) {
            refund { totalRefundedSet { shopMoney { amount currencyCode } presentmentMoney { amount currencyCode } } }
            order { totalRefundedSet { shopMoney { amount currencyCode } presentmentMoney { amount currencyCode } } }
            userErrors { field message }
          }
        }
        "#,
        json!({ "input": { "orderId": second_order_id, "allowOverRefunding": true } }),
    ));
    assert_eq!(refund.body["data"]["refundCreate"]["userErrors"], json!([]));

    let commit = proxy.process_request(json_graphql_request(
        r#"
        mutation CommitMoneyBagOrderEdit($id: ID!) {
          orderEditCommit(id: $id, notifyCustomer: false) {
            order {
              id
              currentTotalPriceSet {
                shopMoney { amount currencyCode }
                presentmentMoney { amount currencyCode }
              }
            }
            successMessages
            userErrors { field message }
          }
        }
        "#,
        json!({ "id": second_calculated_id }),
    ));
    assert_eq!(
        commit.body["data"]["orderEditCommit"]["userErrors"],
        json!([{
            "field": ["id"],
            "message": "There must be at least one change to be made."
        }])
    );

    assert_eq!(
        json!({
            "calculatedIdsAreDistinct": first_calculated_id != second_calculated_id,
            "firstCalculatedIdIsNotLegacyFixed": first_calculated_id != "gid://shopify/CalculatedOrder/7",
            "secondCalculatedIdIsNotLegacyFixed": second_calculated_id != "gid://shopify/CalculatedOrder/7",
            "secondBeginOriginalOrderId": second_begin.body["data"]["orderEditBegin"]["calculatedOrder"]["originalOrder"]["id"].clone(),
            "secondBeginTotalPriceSet": second_begin.body["data"]["orderEditBegin"]["calculatedOrder"]["totalPriceSet"].clone(),
            "secondRefundTotalRefundedSet": refund.body["data"]["refundCreate"]["refund"]["totalRefundedSet"].clone(),
            "secondCommitOrder": commit.body["data"]["orderEditCommit"]["order"].clone()
        }),
        json!({
            "calculatedIdsAreDistinct": true,
            "firstCalculatedIdIsNotLegacyFixed": true,
            "secondCalculatedIdIsNotLegacyFixed": true,
            "secondBeginOriginalOrderId": "gid://shopify/Order/2",
            "secondBeginTotalPriceSet": {
                "shopMoney": { "amount": "22.0", "currencyCode": "CAD" },
                "presentmentMoney": { "amount": "22.0", "currencyCode": "CAD" }
            },
            "secondRefundTotalRefundedSet": {
                "shopMoney": { "amount": "22.0", "currencyCode": "CAD" },
                "presentmentMoney": { "amount": "22.0", "currencyCode": "CAD" }
            },
            "secondCommitOrder": Value::Null
        })
    );
}

#[test]
fn order_edit_add_line_item_discount_applies_percent_value_and_keeps_fixed_per_unit() {
    let mut proxy = snapshot_proxy();
    let create_document = r#"
        mutation CreateOrderEditDiscountOrder($order: OrderCreateOrderInput!) {
          orderCreate(order: $order) {
            order { id }
            userErrors { field message code }
          }
        }
    "#;
    let begin_document = r#"
        mutation BeginOrderEditForLineDiscount($id: ID!) {
          orderEditBegin(id: $id) {
            calculatedOrder {
              id
              lineItems(first: 1) {
                nodes { id }
              }
            }
            userErrors { field message }
          }
        }
    "#;
    let discount_document = r#"
        mutation AddOrderEditLineDiscount($id: ID!, $lineItemId: ID!, $discount: OrderEditAppliedDiscountInput!) {
          orderEditAddLineItemDiscount(id: $id, lineItemId: $lineItemId, discount: $discount) {
            addedDiscountStagedChange { id description }
            calculatedLineItem {
              id
              hasStagedLineItemDiscount
              originalUnitPriceSet { shopMoney { amount currencyCode } }
              discountedUnitPriceSet { shopMoney { amount currencyCode } }
              calculatedDiscountAllocations {
                allocatedAmountSet { shopMoney { amount currencyCode } }
                discountApplication { id description }
              }
            }
            calculatedOrder {
              totalPriceSet { shopMoney { amount currencyCode } }
            }
            userErrors { field message }
          }
        }
    "#;

    let create_order = |proxy: &mut DraftProxy, email: &str| {
        let create = proxy.process_request(json_graphql_request(
            create_document,
            json!({
                "order": {
                    "email": email,
                    "currency": "USD",
                    "lineItems": [{
                        "title": "Discountable order edit line",
                        "quantity": 2,
                        "priceSet": { "shopMoney": { "amount": "100.00", "currencyCode": "USD" } }
                    }]
                }
            }),
        ));
        assert_eq!(create.body["data"]["orderCreate"]["userErrors"], json!([]));
        create.body["data"]["orderCreate"]["order"]["id"].clone()
    };
    let begin_edit = |proxy: &mut DraftProxy, order_id: Value| {
        let begin = proxy.process_request(json_graphql_request(
            begin_document,
            json!({ "id": order_id }),
        ));
        assert_eq!(
            begin.body["data"]["orderEditBegin"]["userErrors"],
            json!([])
        );
        (
            begin.body["data"]["orderEditBegin"]["calculatedOrder"]["id"].clone(),
            begin.body["data"]["orderEditBegin"]["calculatedOrder"]["lineItems"]["nodes"][0]["id"]
                .clone(),
        )
    };

    let percent_order_id = create_order(&mut proxy, "order-edit-percent-discount@example.test");
    let (percent_calculated_id, percent_line_id) = begin_edit(&mut proxy, percent_order_id);
    let percent = proxy.process_request(json_graphql_request(
        discount_document,
        json!({
            "id": percent_calculated_id,
            "lineItemId": percent_line_id,
            "discount": {
                "description": "Ten percent off",
                "percentValue": 10.0
            }
        }),
    ));
    assert_eq!(percent.status, 200, "body: {}", percent.body);
    assert!(
        percent.body.get("errors").is_none(),
        "body: {}",
        percent.body
    );
    let percent_payload = &percent.body["data"]["orderEditAddLineItemDiscount"];
    assert_eq!(percent_payload["userErrors"], json!([]));
    assert_eq!(
        percent_payload["calculatedLineItem"]["hasStagedLineItemDiscount"],
        json!(true)
    );
    assert_eq!(
        percent_payload["calculatedLineItem"]["originalUnitPriceSet"]["shopMoney"],
        json!({ "amount": "100.0", "currencyCode": "USD" })
    );
    assert_eq!(
        percent_payload["calculatedLineItem"]["discountedUnitPriceSet"]["shopMoney"],
        json!({ "amount": "90.0", "currencyCode": "USD" })
    );
    assert_eq!(
        percent_payload["calculatedLineItem"]["calculatedDiscountAllocations"][0]
            ["allocatedAmountSet"]["shopMoney"],
        json!({ "amount": "20.0", "currencyCode": "USD" })
    );
    assert_eq!(
        percent_payload["calculatedOrder"]["totalPriceSet"]["shopMoney"],
        json!({ "amount": "180.0", "currencyCode": "USD" })
    );

    let fixed_order_id = create_order(&mut proxy, "order-edit-fixed-discount@example.test");
    let (fixed_calculated_id, fixed_line_id) = begin_edit(&mut proxy, fixed_order_id);
    let fixed = proxy.process_request(json_graphql_request(
        discount_document,
        json!({
            "id": fixed_calculated_id,
            "lineItemId": fixed_line_id,
            "discount": {
                "description": "Fifteen off each unit",
                "fixedValue": { "amount": "15.00", "currencyCode": "USD" }
            }
        }),
    ));
    let fixed_payload = &fixed.body["data"]["orderEditAddLineItemDiscount"];
    assert_eq!(fixed_payload["userErrors"], json!([]));
    assert_eq!(
        fixed_payload["calculatedLineItem"]["discountedUnitPriceSet"]["shopMoney"],
        json!({ "amount": "85.0", "currencyCode": "USD" })
    );
    assert_eq!(
        fixed_payload["calculatedLineItem"]["calculatedDiscountAllocations"][0]
            ["allocatedAmountSet"]["shopMoney"],
        json!({ "amount": "30.0", "currencyCode": "USD" })
    );
    assert_eq!(
        fixed_payload["calculatedOrder"]["totalPriceSet"]["shopMoney"],
        json!({ "amount": "170.0", "currencyCode": "USD" })
    );
}

#[test]
fn order_edit_commit_recomputes_derived_statuses_and_totals_after_added_item() {
    let mut proxy = snapshot_proxy();
    let create_document = r#"
        mutation CreatePaidFulfilledOrderForEdit($order: OrderCreateOrderInput!) {
          orderCreate(order: $order) {
            order {
              id
              displayFinancialStatus
              displayFulfillmentStatus
              totalOutstandingSet { shopMoney { amount currencyCode } }
              totalPriceSet { shopMoney { amount currencyCode } }
            }
            userErrors { field message code }
          }
        }
    "#;
    let paid_fulfilled_order = |title: &str, email: &str| {
        json!({
            "email": email,
            "currency": "USD",
            "fulfillmentStatus": "FULFILLED",
            "lineItems": [{
                "title": title,
                "quantity": 1,
                "priceSet": { "shopMoney": { "amount": "10.00", "currencyCode": "USD" } }
            }],
            "transactions": [{
                "kind": "SALE",
                "status": "SUCCESS",
                "gateway": "manual",
                "amountSet": { "shopMoney": { "amount": "10.00", "currencyCode": "USD" } }
            }]
        })
    };
    let edited_create = proxy.process_request(json_graphql_request(
        create_document,
        json!({ "order": paid_fulfilled_order("Original edited line", "edited-order@example.test") }),
    ));
    let unrelated_create = proxy.process_request(json_graphql_request(
        create_document,
        json!({ "order": paid_fulfilled_order("Unrelated line", "unrelated-order@example.test") }),
    ));
    assert_eq!(
        edited_create.body["data"]["orderCreate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        unrelated_create.body["data"]["orderCreate"]["userErrors"],
        json!([])
    );
    let edited_order_id = edited_create.body["data"]["orderCreate"]["order"]["id"].clone();
    let unrelated_order_id = unrelated_create.body["data"]["orderCreate"]["order"]["id"].clone();

    let begin = proxy.process_request(json_graphql_request(
        r#"
        mutation BeginOrderEditForDerivedStatus($id: ID!) {
          orderEditBegin(id: $id) {
            calculatedOrder { id }
            userErrors { field message }
          }
        }
        "#,
        json!({ "id": edited_order_id.clone() }),
    ));
    assert_eq!(
        begin.body["data"]["orderEditBegin"]["userErrors"],
        json!([])
    );
    let calculated_order_id = begin.body["data"]["orderEditBegin"]["calculatedOrder"]["id"].clone();

    let add = proxy.process_request(json_graphql_request(
        r#"
        mutation AddCustomItemForDerivedStatus($id: ID!, $price: MoneyInput!) {
          orderEditAddCustomItem(
            id: $id
            title: "Added unpaid item"
            quantity: 1
            price: $price
          ) {
            calculatedLineItem { id }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "id": calculated_order_id.clone(),
            "price": { "amount": "5.00", "currencyCode": "USD" }
        }),
    ));
    assert_eq!(
        add.body["data"]["orderEditAddCustomItem"]["userErrors"],
        json!([])
    );

    let commit = proxy.process_request(json_graphql_request(
        r#"
        mutation CommitOrderEditForDerivedStatus($id: ID!) {
          orderEditCommit(id: $id, notifyCustomer: false) {
            order {
              id
              displayFinancialStatus
              displayFulfillmentStatus
              totalOutstandingSet { shopMoney { amount currencyCode } }
              totalPriceSet { shopMoney { amount currencyCode } }
              currentTotalPriceSet { shopMoney { amount currencyCode } }
            }
            userErrors { field message }
          }
        }
        "#,
        json!({ "id": calculated_order_id }),
    ));
    assert_eq!(
        commit.body["data"]["orderEditCommit"]["userErrors"],
        json!([])
    );
    let committed = &commit.body["data"]["orderEditCommit"]["order"];
    assert_eq!(committed["displayFinancialStatus"], json!("PARTIALLY_PAID"));
    assert_eq!(
        committed["displayFulfillmentStatus"],
        json!("PARTIALLY_FULFILLED")
    );
    assert_eq!(
        committed["totalOutstandingSet"]["shopMoney"],
        json!({ "amount": "5.0", "currencyCode": "USD" })
    );
    assert_eq!(
        committed["totalPriceSet"]["shopMoney"],
        json!({ "amount": "15.0", "currencyCode": "USD" })
    );
    assert_eq!(
        committed["currentTotalPriceSet"]["shopMoney"],
        json!({ "amount": "15.0", "currencyCode": "USD" })
    );
    let downstream = proxy.process_request(json_graphql_request(
        r#"
        query ReadEditedAndUnrelatedOrders($editedId: ID!, $unrelatedId: ID!) {
          edited: order(id: $editedId) {
            id
            displayFinancialStatus
            displayFulfillmentStatus
            totalOutstandingSet { shopMoney { amount currencyCode } }
            totalPriceSet { shopMoney { amount currencyCode } }
            events(first: 1) { nodes { id action message } }
          }
          unrelated: order(id: $unrelatedId) {
            id
            displayFinancialStatus
            displayFulfillmentStatus
            totalOutstandingSet { shopMoney { amount currencyCode } }
            totalPriceSet { shopMoney { amount currencyCode } }
          }
        }
        "#,
        json!({ "editedId": edited_order_id, "unrelatedId": unrelated_order_id }),
    ));
    let edited = &downstream.body["data"]["edited"];
    let unrelated = &downstream.body["data"]["unrelated"];
    assert_eq!(edited["displayFinancialStatus"], json!("PARTIALLY_PAID"));
    assert_eq!(
        edited["displayFulfillmentStatus"],
        json!("PARTIALLY_FULFILLED")
    );
    assert_eq!(
        edited["totalOutstandingSet"]["shopMoney"],
        json!({ "amount": "5.0", "currencyCode": "USD" })
    );
    assert_eq!(
        edited["totalPriceSet"]["shopMoney"],
        json!({ "amount": "15.0", "currencyCode": "USD" })
    );
    assert_eq!(
        edited["events"]["nodes"],
        json!([{
            "id": "gid://shopify/BasicEvent/oe-edited",
            "action": "edited",
            "message": ""
        }])
    );
    assert_eq!(unrelated["displayFinancialStatus"], json!("PAID"));
    assert_eq!(unrelated["displayFulfillmentStatus"], json!("FULFILLED"));
    assert_eq!(
        unrelated["totalOutstandingSet"]["shopMoney"],
        json!({ "amount": "0.0", "currencyCode": "USD" })
    );
    assert_eq!(
        unrelated["totalPriceSet"]["shopMoney"],
        json!({ "amount": "10.0", "currencyCode": "USD" })
    );
}

#[test]
fn order_edit_keeps_tax_in_calculated_current_and_historical_money() {
    let mut proxy = snapshot_proxy();

    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateTaxedOrderForEdit($order: OrderCreateOrderInput!) {
          orderCreate(order: $order) {
            order {
              id
              subtotalPriceSet { shopMoney { amount currencyCode } }
              totalTaxSet { shopMoney { amount currencyCode } }
              totalPriceSet { shopMoney { amount currencyCode } }
              lineItems(first: 1) {
                nodes {
                  taxLines {
                    title
                    rate
                    priceSet { shopMoney { amount currencyCode } }
                  }
                }
              }
            }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "order": {
                "email": "order-edit-taxed@example.test",
                "currency": "USD",
                "lineItems": [{
                    "title": "Taxed edit line",
                    "quantity": 2,
                    "priceSet": { "shopMoney": { "amount": "100.00", "currencyCode": "USD" } },
                    "taxLines": [{
                        "title": "State tax",
                        "rate": 0.13,
                        "priceSet": { "shopMoney": { "amount": "26.00", "currencyCode": "USD" } }
                    }]
                }]
            }
        }),
    ));
    assert_eq!(create.status, 200);
    assert_eq!(create.body["data"]["orderCreate"]["userErrors"], json!([]));
    let created_order = &create.body["data"]["orderCreate"]["order"];
    assert_eq!(
        created_order["subtotalPriceSet"]["shopMoney"],
        json!({ "amount": "200.0", "currencyCode": "USD" })
    );
    assert_eq!(
        created_order["totalTaxSet"]["shopMoney"],
        json!({ "amount": "26.0", "currencyCode": "USD" })
    );
    assert_eq!(
        created_order["totalPriceSet"]["shopMoney"],
        json!({ "amount": "226.0", "currencyCode": "USD" })
    );
    let order_id = created_order["id"].clone();

    let begin = proxy.process_request(json_graphql_request(
        r#"
        mutation BeginTaxedOrderEdit($id: ID!) {
          orderEditBegin(id: $id) {
            calculatedOrder {
              id
              subtotalPriceSet { shopMoney { amount currencyCode } }
              totalPriceSet { shopMoney { amount currencyCode } }
              lineItems(first: 1) { nodes { id } }
            }
            userErrors { field message }
          }
        }
        "#,
        json!({ "id": order_id.clone() }),
    ));
    assert_eq!(
        begin.body["data"]["orderEditBegin"]["userErrors"],
        json!([])
    );
    let calculated_order = &begin.body["data"]["orderEditBegin"]["calculatedOrder"];
    assert_eq!(
        calculated_order["subtotalPriceSet"]["shopMoney"],
        json!({ "amount": "200.0", "currencyCode": "USD" })
    );
    assert_eq!(
        calculated_order["totalPriceSet"]["shopMoney"],
        json!({ "amount": "226.0", "currencyCode": "USD" })
    );
    let calculated_order_id = calculated_order["id"].clone();
    let calculated_line_id = calculated_order["lineItems"]["nodes"][0]["id"].clone();

    let set_quantity = proxy.process_request(json_graphql_request(
        r#"
        mutation SetTaxedOrderEditQuantity($id: ID!, $lineItemId: ID!) {
          orderEditSetQuantity(id: $id, lineItemId: $lineItemId, quantity: 1) {
            calculatedOrder {
              subtotalLineItemsQuantity
              subtotalPriceSet { shopMoney { amount currencyCode } }
              totalPriceSet { shopMoney { amount currencyCode } }
            }
            calculatedLineItem {
              quantity
              discountedUnitPriceSet { shopMoney { amount currencyCode } }
            }
            userErrors { field message }
          }
        }
        "#,
        json!({ "id": calculated_order_id.clone(), "lineItemId": calculated_line_id }),
    ));
    assert_eq!(
        set_quantity.body["data"]["orderEditSetQuantity"]["userErrors"],
        json!([])
    );
    let set_quantity_payload = &set_quantity.body["data"]["orderEditSetQuantity"];
    assert_eq!(
        set_quantity_payload["calculatedOrder"]["subtotalLineItemsQuantity"],
        json!(1)
    );
    assert_eq!(
        set_quantity_payload["calculatedOrder"]["subtotalPriceSet"]["shopMoney"],
        json!({ "amount": "100.0", "currencyCode": "USD" })
    );
    assert_eq!(
        set_quantity_payload["calculatedOrder"]["totalPriceSet"]["shopMoney"],
        json!({ "amount": "113.0", "currencyCode": "USD" })
    );
    assert_eq!(
        set_quantity_payload["calculatedLineItem"]["discountedUnitPriceSet"]["shopMoney"],
        json!({ "amount": "100.0", "currencyCode": "USD" })
    );

    let commit = proxy.process_request(json_graphql_request(
        r#"
        mutation CommitTaxedOrderEdit($id: ID!) {
          orderEditCommit(id: $id, notifyCustomer: false) {
            order {
              id
              currentSubtotalPriceSet { shopMoney { amount currencyCode } }
              currentTotalPriceSet { shopMoney { amount currencyCode } }
              currentTotalTaxSet { shopMoney { amount currencyCode } }
              subtotalPriceSet { shopMoney { amount currencyCode } }
              totalTaxSet { shopMoney { amount currencyCode } }
              totalPriceSet { shopMoney { amount currencyCode } }
              currentTaxLines {
                title
                rate
                priceSet { shopMoney { amount currencyCode } }
              }
              lineItems(first: 1) {
                nodes {
                  quantity
                  currentQuantity
                  taxLines {
                    title
                    rate
                    priceSet { shopMoney { amount currencyCode } }
                  }
                }
              }
            }
            userErrors { field message }
          }
        }
        "#,
        json!({ "id": calculated_order_id }),
    ));
    assert_eq!(
        commit.body["data"]["orderEditCommit"]["userErrors"],
        json!([])
    );
    let committed = &commit.body["data"]["orderEditCommit"]["order"];
    assert_eq!(
        committed["currentSubtotalPriceSet"]["shopMoney"],
        json!({ "amount": "100.0", "currencyCode": "USD" })
    );
    assert_eq!(
        committed["totalTaxSet"]["shopMoney"],
        json!({ "amount": "26.0", "currencyCode": "USD" })
    );
    assert_eq!(
        committed["currentTotalTaxSet"]["shopMoney"],
        json!({ "amount": "13.0", "currencyCode": "USD" })
    );
    assert_eq!(
        committed["subtotalPriceSet"]["shopMoney"],
        json!({ "amount": "200.0", "currencyCode": "USD" })
    );
    assert_eq!(
        committed["totalPriceSet"]["shopMoney"],
        json!({ "amount": "226.0", "currencyCode": "USD" })
    );
    assert_eq!(
        committed["currentTotalPriceSet"]["shopMoney"],
        json!({ "amount": "113.0", "currencyCode": "USD" })
    );
    assert_eq!(
        committed["currentTaxLines"],
        json!([{
            "title": "State tax",
            "rate": 0.13,
            "priceSet": { "shopMoney": { "amount": "13.0", "currencyCode": "USD" } }
        }])
    );
    assert_eq!(
        committed["lineItems"]["nodes"][0],
        json!({
            "quantity": 2,
            "currentQuantity": 1,
            "taxLines": [{
                "title": "State tax",
                "rate": 0.13,
                "priceSet": { "shopMoney": { "amount": "26.0", "currencyCode": "USD" } }
            }]
        })
    );

    let downstream = proxy.process_request(json_graphql_request(
        r#"
        query ReadTaxedEditedOrder($id: ID!) {
          order(id: $id) {
            currentSubtotalPriceSet { shopMoney { amount currencyCode } }
            currentTotalPriceSet { shopMoney { amount currencyCode } }
            currentTotalTaxSet { shopMoney { amount currencyCode } }
            subtotalPriceSet { shopMoney { amount currencyCode } }
            totalTaxSet { shopMoney { amount currencyCode } }
            totalPriceSet { shopMoney { amount currencyCode } }
            currentTaxLines {
              title
              rate
              priceSet { shopMoney { amount currencyCode } }
            }
            lineItems(first: 1) {
              nodes {
                quantity
                currentQuantity
                taxLines {
                  title
                  rate
                  priceSet { shopMoney { amount currencyCode } }
                }
              }
            }
          }
        }
        "#,
        json!({ "id": order_id }),
    ));
    assert_eq!(
        downstream.body["data"]["order"],
        json!({
            "currentSubtotalPriceSet": { "shopMoney": { "amount": "100.0", "currencyCode": "USD" } },
            "currentTotalPriceSet": { "shopMoney": { "amount": "113.0", "currencyCode": "USD" } },
            "currentTotalTaxSet": { "shopMoney": { "amount": "13.0", "currencyCode": "USD" } },
            "subtotalPriceSet": { "shopMoney": { "amount": "200.0", "currencyCode": "USD" } },
            "totalTaxSet": { "shopMoney": { "amount": "26.0", "currencyCode": "USD" } },
            "totalPriceSet": { "shopMoney": { "amount": "226.0", "currencyCode": "USD" } },
            "currentTaxLines": [{
                "title": "State tax",
                "rate": 0.13,
                "priceSet": { "shopMoney": { "amount": "13.0", "currencyCode": "USD" } }
            }],
            "lineItems": { "nodes": [{
                "quantity": 2,
                "currentQuantity": 1,
                "taxLines": [{
                    "title": "State tax",
                    "rate": 0.13,
                    "priceSet": { "shopMoney": { "amount": "26.0", "currencyCode": "USD" } }
                }]
            }] }
        })
    );
}

#[test]
fn order_edit_line_item_discount_subtotal_is_net_of_discount() {
    let mut proxy = snapshot_proxy();

    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateDiscountableOrderForEdit($order: OrderCreateOrderInput!) {
          orderCreate(order: $order) {
            order { id }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "order": {
                "email": "order-edit-discount@example.test",
                "currency": "USD",
                "lineItems": [{
                    "title": "Discountable edit line",
                    "quantity": 1,
                    "priceSet": { "shopMoney": { "amount": "100.00", "currencyCode": "USD" } }
                }]
            }
        }),
    ));
    assert_eq!(create.status, 200);
    assert_eq!(create.body["data"]["orderCreate"]["userErrors"], json!([]));
    let order_id = create.body["data"]["orderCreate"]["order"]["id"].clone();

    let begin = proxy.process_request(json_graphql_request(
        r#"
        mutation BeginDiscountableOrderEdit($id: ID!) {
          orderEditBegin(id: $id) {
            calculatedOrder {
              id
              lineItems(first: 1) { nodes { id } }
            }
            userErrors { field message }
          }
        }
        "#,
        json!({ "id": order_id }),
    ));
    assert_eq!(
        begin.body["data"]["orderEditBegin"]["userErrors"],
        json!([])
    );
    let calculated_order = &begin.body["data"]["orderEditBegin"]["calculatedOrder"];
    let calculated_order_id = calculated_order["id"].clone();
    let calculated_line_id = calculated_order["lineItems"]["nodes"][0]["id"].clone();

    let discount = proxy.process_request(json_graphql_request(
        r#"
        mutation DiscountOrderEditLine(
          $id: ID!
          $lineItemId: ID!
          $discount: OrderEditAppliedDiscountInput!
        ) {
          orderEditAddLineItemDiscount(id: $id, lineItemId: $lineItemId, discount: $discount) {
            calculatedOrder {
              subtotalPriceSet { shopMoney { amount currencyCode } }
              totalPriceSet { shopMoney { amount currencyCode } }
            }
            calculatedLineItem {
              discountedUnitPriceSet { shopMoney { amount currencyCode } }
            }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "id": calculated_order_id,
            "lineItemId": calculated_line_id,
            "discount": {
                "description": "Line item discount",
                "fixedValue": { "amount": "20.00", "currencyCode": "USD" }
            }
        }),
    ));
    assert_eq!(
        discount.body["data"]["orderEditAddLineItemDiscount"]["userErrors"],
        json!([])
    );
    let payload = &discount.body["data"]["orderEditAddLineItemDiscount"];
    assert_eq!(
        payload["calculatedOrder"]["subtotalPriceSet"]["shopMoney"],
        json!({ "amount": "80.0", "currencyCode": "USD" })
    );
    assert_eq!(
        payload["calculatedOrder"]["totalPriceSet"]["shopMoney"],
        json!({ "amount": "80.0", "currencyCode": "USD" })
    );
    assert_eq!(
        payload["calculatedLineItem"]["discountedUnitPriceSet"]["shopMoney"],
        json!({ "amount": "80.0", "currencyCode": "USD" })
    );
}

#[test]
fn money_bag_refund_missing_order_returns_user_error_without_canned_money() {
    let mut proxy = snapshot_proxy();
    let response = proxy.process_request(json_graphql_request(
        r#"
        mutation RefundMissingMoneyBagOrder($input: RefundInput!) {
          refundCreate(input: $input) {
            refund { totalRefundedSet { shopMoney { amount currencyCode } presentmentMoney { amount currencyCode } } }
            order { totalRefundedSet { shopMoney { amount currencyCode } presentmentMoney { amount currencyCode } } }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "input": {
                "orderId": "gid://shopify/Order/404",
                "allowOverRefunding": true
            }
        }),
    ));

    assert_eq!(response.status, 200);
    assert_eq!(
        response.body["data"]["refundCreate"],
        json!({
            "refund": Value::Null,
            "order": Value::Null,
            "userErrors": [{
                "field": ["orderId"],
                "message": "Order does not exist"
            }]
        })
    );
}

fn seed_abandonment_delivery_status(
    proxy: &mut DraftProxy,
    abandonment_id: &str,
    marketing_activity_id: &str,
    email_state: &str,
    email_sent_at: Value,
) {
    let dump = proxy.process_request(request_with_body("POST", "/__meta/dump", ""));
    assert_eq!(dump.status, 200);
    let mut restored = dump.body;
    let mut delivery_statuses = serde_json::Map::new();
    delivery_statuses.insert(
        marketing_activity_id.to_string(),
        json!({
            "deliveryStatus": email_state,
            "deliveredAt": email_sent_at.clone()
        }),
    );
    let mut record = json!({
        "id": abandonment_id,
        "emailState": email_state,
        "emailSentAt": email_sent_at
    });
    record["__draftProxyDeliveryStatuses"] = Value::Object(delivery_statuses);
    let staged_state = restored["state"]["stagedState"]
        .as_object_mut()
        .expect("state dump should include stagedState object");
    let abandonments = staged_state
        .entry("abandonments".to_string())
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .expect("staged abandonment state should be an object");
    abandonments.insert(abandonment_id.to_string(), record);
    let restore = proxy.process_request(request_with_body(
        "POST",
        "/__meta/restore",
        &restored.to_string(),
    ));
    assert_eq!(restore.status, 200);
}

#[test]
fn abandonment_delivery_status_requires_staged_abandonment_for_ordinary_ids() {
    let mut proxy = snapshot_proxy();
    let known_abandonment_id = "gid://shopify/Abandonment/3101";
    let known_activity_id = "gid://shopify/MarketingActivity/4101";
    let delivered_at = "2026-06-20T10:00:00Z";
    seed_abandonment_delivery_status(
        &mut proxy,
        known_abandonment_id,
        known_activity_id,
        "NOT_SENT",
        Value::Null,
    );

    let found = proxy.process_request(json_graphql_request(
        r#"
        mutation UpdateKnownAbandonment($abandonmentId: ID!, $marketingActivityId: ID!, $deliveredAt: DateTime) {
          abandonmentUpdateActivitiesDeliveryStatuses(
            abandonmentId: $abandonmentId
            marketingActivityId: $marketingActivityId
            deliveryStatus: SENT
            deliveredAt: $deliveredAt
          ) {
            abandonment { id emailState emailSentAt }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "abandonmentId": known_abandonment_id,
            "marketingActivityId": known_activity_id,
            "deliveredAt": delivered_at
        }),
    ));
    assert_eq!(found.status, 200);
    assert_eq!(
        found.body["data"]["abandonmentUpdateActivitiesDeliveryStatuses"],
        json!({
            "abandonment": {
                "id": known_abandonment_id,
                "emailState": "SENT",
                "emailSentAt": delivered_at
            },
            "userErrors": []
        })
    );

    let missing = proxy.process_request(json_graphql_request(
        r#"
        mutation UpdateMissingAbandonment($abandonmentId: ID!, $marketingActivityId: ID!, $deliveredAt: DateTime) {
          abandonmentUpdateActivitiesDeliveryStatuses(
            abandonmentId: $abandonmentId
            marketingActivityId: $marketingActivityId
            deliveryStatus: SENT
            deliveredAt: $deliveredAt
          ) {
            abandonment { id emailState emailSentAt }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "abandonmentId": "gid://shopify/Abandonment/3102",
            "marketingActivityId": "gid://shopify/MarketingActivity/4102",
            "deliveredAt": delivered_at
        }),
    ));
    assert_eq!(missing.status, 200);
    assert_eq!(
        missing.body["data"]["abandonmentUpdateActivitiesDeliveryStatuses"],
        json!({
            "abandonment": Value::Null,
            "userErrors": [{
                "field": ["abandonmentId"],
                "message": "abandonment_not_found"
            }]
        })
    );

    let log = log_snapshot(&proxy);
    assert_eq!(log["entries"].as_array().unwrap().len(), 1);
}

#[test]
fn abandonment_delivery_status_omitted_delivered_at_uses_runtime_timestamp() {
    let mut proxy = snapshot_proxy();
    let abandonment_id = "gid://shopify/Abandonment/3103";
    let activity_id = "gid://shopify/MarketingActivity/4103";
    seed_abandonment_delivery_status(
        &mut proxy,
        abandonment_id,
        activity_id,
        "NOT_SENT",
        Value::Null,
    );

    let response = proxy.process_request(json_graphql_request(
        r#"
        mutation UpdateKnownAbandonmentWithoutDeliveredAt($abandonmentId: ID!, $marketingActivityId: ID!) {
          abandonmentUpdateActivitiesDeliveryStatuses(
            abandonmentId: $abandonmentId
            marketingActivityId: $marketingActivityId
            deliveryStatus: SENT
          ) {
            abandonment { id emailState emailSentAt }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "abandonmentId": abandonment_id,
            "marketingActivityId": activity_id
        }),
    ));
    assert_eq!(response.status, 200);
    let payload = &response.body["data"]["abandonmentUpdateActivitiesDeliveryStatuses"];
    assert_eq!(payload["userErrors"], json!([]));
    assert_eq!(payload["abandonment"]["id"], json!(abandonment_id));
    assert_eq!(payload["abandonment"]["emailState"], json!("SENT"));
    let email_sent_at = payload["abandonment"]["emailSentAt"]
        .as_str()
        .expect("omitted deliveredAt should synthesize a runtime timestamp");
    assert_ne!(email_sent_at, "2026-04-27T00:00:00Z");
}

#[test]
fn draft_order_complete_replays_resulting_order_and_gateway_paths() {
    let mut staged_proxy = snapshot_proxy();
    let create = staged_proxy.process_request(json_graphql_request(
        include_str!("../../config/parity-requests/orders/draftOrderComplete-stages-resulting-order-create.graphql"),
        json!({}),
    ));
    assert_eq!(create.status, 200);
    assert_eq!(
        create.body["data"]["draftOrderCreate"]["draftOrder"]["status"],
        json!("OPEN")
    );
    let draft_id = create.body["data"]["draftOrderCreate"]["draftOrder"]["id"].clone();

    let complete = staged_proxy.process_request(json_graphql_request(
        include_str!("../../config/parity-requests/orders/draftOrderComplete-stages-resulting-order-complete.graphql"),
        json!({"id": draft_id.clone(), "paymentPending": false}),
    ));
    assert_eq!(complete.status, 200);
    assert_eq!(
        complete.body["data"]["draftOrderComplete"]["draftOrder"]["id"],
        draft_id
    );
    assert_eq!(
        complete.body["data"]["draftOrderComplete"]["draftOrder"]["status"],
        json!("COMPLETED")
    );
    let order = &complete.body["data"]["draftOrderComplete"]["draftOrder"]["order"];
    let order_id = order["id"].clone();
    assert_eq!(order["displayFinancialStatus"], json!("PAID"));
    assert_eq!(
        order["lineItems"]["nodes"][0]["title"],
        json!("Completion service")
    );

    let read_by_id = staged_proxy.process_request(json_graphql_request(
        include_str!("../../config/parity-requests/orders/draftOrderComplete-stages-resulting-order-read-by-id.graphql"),
        json!({"id": order_id.clone()}),
    ));
    assert_eq!(read_by_id.body["data"]["order"], *order);

    let read_by_name = staged_proxy.process_request(json_graphql_request(
        include_str!("../../config/parity-requests/orders/draftOrderComplete-stages-resulting-order-read-by-name.graphql"),
        json!({}),
    ));
    assert_eq!(
        read_by_name.body["data"]["orders"]["nodes"][0]["id"],
        order_id
    );

    let mut gateway_proxy = snapshot_proxy();
    let no_gateway_create = gateway_proxy.process_request(json_graphql_request(
        include_str!("../../config/parity-requests/orders/draftOrderComplete-paymentGateway-paths-create.graphql"),
        json!({}),
    ));
    assert_eq!(no_gateway_create.status, 200);
    let no_gateway_id =
        no_gateway_create.body["data"]["draftOrderCreate"]["draftOrder"]["id"].clone();

    let no_gateway_complete = gateway_proxy.process_request(json_graphql_request(
        include_str!("../../config/parity-requests/orders/draftOrderComplete-paymentGateway-paths-complete.graphql"),
        json!({"id": no_gateway_id, "paymentGatewayId": null, "paymentPending": true}),
    ));
    assert_eq!(
        no_gateway_complete.body["data"]["draftOrderComplete"]["draftOrder"]["order"]
            ["displayFinancialStatus"],
        json!("PENDING")
    );

    let unknown_create = gateway_proxy.process_request(json_graphql_request(
        include_str!("../../config/parity-requests/orders/draftOrderComplete-paymentGateway-paths-create.graphql"),
        json!({}),
    ));
    let unknown_id = unknown_create.body["data"]["draftOrderCreate"]["draftOrder"]["id"].clone();

    let unknown_complete = gateway_proxy.process_request(json_graphql_request(
        include_str!("../../config/parity-requests/orders/draftOrderComplete-paymentGateway-paths-complete.graphql"),
        json!({"id": unknown_id, "paymentGatewayId": "gid://shopify/PaymentGateway/not-installed", "paymentPending": false}),
    ));
    assert_eq!(
        unknown_complete.body["data"]["draftOrderComplete"]["draftOrder"]["id"],
        unknown_id
    );
    assert_eq!(
        unknown_complete.body["data"]["draftOrderComplete"]["draftOrder"]["status"],
        json!("OPEN")
    );
    assert_eq!(
        unknown_complete.body["data"]["draftOrderComplete"]["draftOrder"]["order"],
        Value::Null
    );
    assert_eq!(
        unknown_complete.body["data"]["draftOrderComplete"]["userErrors"],
        json!([{ "field": null, "message": "Invalid payment gateway" }])
    );
}

#[test]
fn draft_order_complete_rejects_user_error_code_selection() {
    let mut proxy = snapshot_proxy();

    let response = proxy.process_request(json_graphql_request(
        r#"mutation DraftOrderCompleteUserErrorNoCode($id: ID!) {
          draftOrderComplete(id: $id, paymentGatewayId: "gid://shopify/PaymentGateway/not-installed") {
            userErrors { field message code }
          }
        }"#,
        json!({ "id": "gid://shopify/DraftOrder/1" }),
    ));

    assert_eq!(response.status, 200);
    assert!(response.body.get("data").is_none());
    let errors = response.body["errors"].as_array().unwrap();
    assert_eq!(errors.len(), 1);
    assert_eq!(
        errors[0]["message"],
        json!("Field 'code' doesn't exist on type 'UserError'")
    );
    assert_eq!(
        errors[0]["path"],
        json!([
            "mutation DraftOrderCompleteUserErrorNoCode",
            "draftOrderComplete",
            "userErrors",
            "code"
        ])
    );
    assert_eq!(
        errors[0]["extensions"],
        json!({
            "code": "undefinedField",
            "typeName": "UserError",
            "fieldName": "code"
        })
    );
}

fn create_draft_for_payment_terms_completion_test(
    proxy: &mut DraftProxy,
    email: &str,
    with_payment_terms: bool,
) -> Value {
    let mut input = json!({
        "email": email,
        "lineItems": [{
            "title": "Payment terms completion item",
            "quantity": 1,
            "originalUnitPrice": "12.00",
            "sku": "TERMS-COMPLETE"
        }]
    });
    if with_payment_terms {
        input["paymentTerms"] = json!({
            "paymentTermsTemplateId": "gid://shopify/PaymentTermsTemplate/4"
        });
    }

    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateDraftForPaymentTermsCompletion($input: DraftOrderInput!) {
          draftOrderCreate(input: $input) {
            draftOrder {
              id
              status
              paymentTerms { id }
            }
            userErrors { field message  }
          }
        }
        "#,
        json!({ "input": input }),
    ));
    assert_eq!(create.status, 200);
    assert_eq!(
        create.body["data"]["draftOrderCreate"]["userErrors"],
        json!([])
    );
    create.body["data"]["draftOrderCreate"]["draftOrder"].clone()
}

fn complete_draft_for_payment_terms_completion_test(
    proxy: &mut DraftProxy,
    draft_id: Value,
    payment_pending: Option<bool>,
) -> Value {
    let (query, variables) = match payment_pending {
        Some(payment_pending) => (
            r#"
            mutation CompleteDraftForPaymentTermsCompletion($id: ID!, $paymentPending: Boolean) {
              draftOrderComplete(id: $id, paymentPending: $paymentPending) {
                draftOrder {
                  id
                  status
                  order {
                    id
                    displayFinancialStatus
                    paymentGatewayNames
                    transactions {
                      kind
                      status
                      gateway
                    }
                  }
                }
                userErrors { field message }
              }
            }
            "#,
            json!({ "id": draft_id, "paymentPending": payment_pending }),
        ),
        None => (
            r#"
            mutation CompleteDraftForPaymentTermsCompletion($id: ID!) {
              draftOrderComplete(id: $id) {
                draftOrder {
                  id
                  status
                  order {
                    id
                    displayFinancialStatus
                    paymentGatewayNames
                    transactions {
                      kind
                      status
                      gateway
                    }
                  }
                }
                userErrors { field message }
              }
            }
            "#,
            json!({ "id": draft_id }),
        ),
    };

    let complete = proxy.process_request(json_graphql_request(query, variables));
    assert_eq!(complete.status, 200);
    assert_eq!(
        complete.body["data"]["draftOrderComplete"]["userErrors"],
        json!([])
    );
    complete.body["data"]["draftOrderComplete"]["draftOrder"].clone()
}

fn read_completed_order_for_payment_terms_completion_test(
    proxy: &mut DraftProxy,
    order_id: Value,
) -> Value {
    let read = proxy.process_request(json_graphql_request(
        r#"
        query ReadCompletedDraftOrderPaymentTerms($id: ID!) {
          order(id: $id) {
            id
            displayFinancialStatus
            paymentGatewayNames
            transactions {
              kind
              status
              gateway
            }
          }
        }
        "#,
        json!({ "id": order_id }),
    ));
    assert_eq!(read.status, 200);
    read.body["data"]["order"].clone()
}

#[test]
fn draft_order_complete_uses_payment_terms_as_implicit_pending_unless_overridden() {
    let mut implicit_terms_proxy = snapshot_proxy();
    let terms_draft = create_draft_for_payment_terms_completion_test(
        &mut implicit_terms_proxy,
        "terms-implicit-completion@example.test",
        true,
    );
    assert_eq!(
        terms_draft["paymentTerms"]["id"],
        json!("gid://shopify/PaymentTermsTemplate/4")
    );

    let completed_terms_draft = complete_draft_for_payment_terms_completion_test(
        &mut implicit_terms_proxy,
        terms_draft["id"].clone(),
        None,
    );
    assert_eq!(completed_terms_draft["status"], json!("COMPLETED"));
    let pending_order = &completed_terms_draft["order"];
    assert_eq!(pending_order["displayFinancialStatus"], json!("PENDING"));
    assert_eq!(pending_order["paymentGatewayNames"], json!(["manual"]));
    assert_eq!(
        pending_order["transactions"],
        json!([{ "kind": "SALE", "status": "PENDING", "gateway": "manual" }])
    );

    let readback_order = read_completed_order_for_payment_terms_completion_test(
        &mut implicit_terms_proxy,
        pending_order["id"].clone(),
    );
    assert_eq!(readback_order, *pending_order);

    let mut no_terms_proxy = snapshot_proxy();
    let no_terms_draft = create_draft_for_payment_terms_completion_test(
        &mut no_terms_proxy,
        "no-terms-completion@example.test",
        false,
    );
    assert_eq!(no_terms_draft["paymentTerms"], Value::Null);
    let paid_draft = complete_draft_for_payment_terms_completion_test(
        &mut no_terms_proxy,
        no_terms_draft["id"].clone(),
        None,
    );
    let paid_order = &paid_draft["order"];
    assert_eq!(paid_draft["status"], json!("COMPLETED"));
    assert_eq!(paid_order["displayFinancialStatus"], json!("PAID"));
    assert_eq!(paid_order["paymentGatewayNames"], json!(["manual"]));
    assert_eq!(
        paid_order["transactions"],
        json!([{ "kind": "SALE", "status": "SUCCESS", "gateway": "manual" }])
    );

    let mut explicit_false_proxy = snapshot_proxy();
    let explicit_false_terms_draft = create_draft_for_payment_terms_completion_test(
        &mut explicit_false_proxy,
        "terms-explicit-paid-completion@example.test",
        true,
    );
    let explicit_paid_draft = complete_draft_for_payment_terms_completion_test(
        &mut explicit_false_proxy,
        explicit_false_terms_draft["id"].clone(),
        Some(false),
    );
    let explicit_paid_order = &explicit_paid_draft["order"];
    assert_eq!(explicit_paid_draft["status"], json!("COMPLETED"));
    assert_eq!(explicit_paid_order["displayFinancialStatus"], json!("PAID"));
    assert_eq!(
        explicit_paid_order["paymentGatewayNames"],
        json!(["manual"])
    );
    assert_eq!(
        explicit_paid_order["transactions"],
        json!([{ "kind": "SALE", "status": "SUCCESS", "gateway": "manual" }])
    );

    let mut explicit_true_proxy = snapshot_proxy();
    let explicit_true_draft = create_draft_for_payment_terms_completion_test(
        &mut explicit_true_proxy,
        "no-terms-explicit-pending-completion@example.test",
        false,
    );
    let explicit_pending_draft = complete_draft_for_payment_terms_completion_test(
        &mut explicit_true_proxy,
        explicit_true_draft["id"].clone(),
        Some(true),
    );
    let explicit_pending_order = &explicit_pending_draft["order"];
    assert_eq!(explicit_pending_draft["status"], json!("COMPLETED"));
    assert_eq!(
        explicit_pending_order["displayFinancialStatus"],
        json!("PENDING")
    );
    assert_eq!(
        explicit_pending_order["paymentGatewayNames"],
        json!(["manual"])
    );
    assert_eq!(
        explicit_pending_order["transactions"],
        json!([{ "kind": "SALE", "status": "PENDING", "gateway": "manual" }])
    );
}

#[test]
fn draft_order_complete_uses_staged_totals_and_source_for_any_email() {
    let mut proxy = snapshot_proxy();
    restore_shop_currency(&mut proxy, "CAD");

    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateCompletableDraft($input: DraftOrderInput!) {
          draftOrderCreate(input: $input) {
            draftOrder {
              id
              status
              lineItemsSubtotalPrice { shopMoney { amount currencyCode } presentmentMoney { amount currencyCode } }
              totalTax
              totalTaxSet { shopMoney { amount currencyCode } presentmentMoney { amount currencyCode } }
              subtotalPriceSet { shopMoney { amount currencyCode } presentmentMoney { amount currencyCode } }
              totalPriceSet { shopMoney { amount currencyCode } }
              lineItems(first: 5) {
                nodes {
                  id
                  title
                  quantity
                  sku
                  originalTotalSet { shopMoney { amount currencyCode } }
                }
              }
            }
            userErrors { field message  }
          }
        }
        "#,
        json!({
            "input": {
                "email": "customer-completion-any-email@example.com",
                "taxExempt": true,
                "shippingLine": {
                    "title": "Local courier",
                    "priceWithCurrency": { "amount": "3.25", "currencyCode": "CAD" }
                },
                "lineItems": [
                    {
                        "title": "Completion service",
                        "quantity": 2,
                        "originalUnitPrice": "12.50",
                        "sku": "COMPLETE-A",
                        "taxable": false
                    },
                    {
                        "title": "Completion add-on",
                        "quantity": 1,
                        "originalUnitPrice": "4.00",
                        "sku": "COMPLETE-B",
                        "taxable": false
                    }
                ]
            }
        }),
    ));
    assert_eq!(create.status, 200);
    assert_eq!(
        create.body["data"]["draftOrderCreate"]["userErrors"],
        json!([])
    );
    let draft = &create.body["data"]["draftOrderCreate"]["draftOrder"];
    assert_eq!(
        draft["totalPriceSet"]["shopMoney"],
        json!({ "amount": "32.25", "currencyCode": "CAD" })
    );
    assert_eq!(
        draft["lineItemsSubtotalPrice"],
        json!({
            "shopMoney": { "amount": "29.0", "currencyCode": "CAD" },
            "presentmentMoney": { "amount": "29.0", "currencyCode": "CAD" }
        })
    );
    assert_eq!(draft["totalTax"], json!("0.00"));
    assert_eq!(
        draft["totalTaxSet"],
        json!({
            "shopMoney": { "amount": "0.0", "currencyCode": "CAD" },
            "presentmentMoney": { "amount": "0.0", "currencyCode": "CAD" }
        })
    );
    assert_eq!(
        draft["subtotalPriceSet"]["presentmentMoney"],
        json!({ "amount": "29.0", "currencyCode": "CAD" })
    );
    assert_eq!(draft["lineItems"]["nodes"].as_array().unwrap().len(), 2);
    let draft_id = draft["id"].clone();

    let mut complete_request = json_graphql_request(
        r#"
        mutation CompleteDraft($id: ID!) {
          draftOrderComplete(id: $id, sourceName: "checkout-ui", paymentPending: false) {
            draftOrder {
              id
              status
              order {
                id
                email
                sourceName
                currencyCode
                displayFinancialStatus
                totalPriceSet { shopMoney { amount currencyCode } }
                currentTotalPriceSet { shopMoney { amount currencyCode } }
                totalPriceSet { shopMoney { amount currencyCode } presentmentMoney { amount currencyCode } }
                subtotalPriceSet { shopMoney { amount currencyCode } presentmentMoney { amount currencyCode } }
                totalTaxSet { shopMoney { amount currencyCode } presentmentMoney { amount currencyCode } }
                totalReceivedSet { shopMoney { amount currencyCode } presentmentMoney { amount currencyCode } }
                lineItems {
                  nodes {
                    id
                    title
                    quantity
                    sku
                  }
                }
              }
            }
            userErrors { field message }
          }
        }
        "#,
        json!({ "id": draft_id.clone() }),
    );
    complete_request.headers.insert(
        "x-shopify-draft-proxy-api-client-id".to_string(),
        "123456789012".to_string(),
    );
    let complete = proxy.process_request(complete_request);
    assert_eq!(complete.status, 200);
    assert_eq!(
        complete.body["data"]["draftOrderComplete"]["userErrors"],
        json!([])
    );
    let completed_draft = &complete.body["data"]["draftOrderComplete"]["draftOrder"];
    assert_eq!(completed_draft["id"], draft_id);
    assert_eq!(completed_draft["status"], json!("COMPLETED"));
    let order = &completed_draft["order"];
    assert_eq!(order["id"], json!("gid://shopify/Order/1"));
    assert_eq!(
        order["email"],
        json!("customer-completion-any-email@example.com")
    );
    assert_eq!(order["sourceName"], json!("123456789012"));
    assert_eq!(order["currencyCode"], json!("CAD"));
    assert_eq!(order["displayFinancialStatus"], json!("PAID"));
    assert_eq!(
        order["totalPriceSet"]["shopMoney"],
        json!({ "amount": "32.25", "currencyCode": "CAD" })
    );
    assert_eq!(
        order["currentTotalPriceSet"]["shopMoney"],
        json!({ "amount": "32.25", "currencyCode": "CAD" })
    );
    assert_eq!(
        order["totalPriceSet"],
        json!({
            "shopMoney": { "amount": "32.25", "currencyCode": "CAD" },
            "presentmentMoney": { "amount": "32.25", "currencyCode": "CAD" }
        })
    );
    assert_eq!(
        order["subtotalPriceSet"],
        json!({
            "shopMoney": { "amount": "29.0", "currencyCode": "CAD" },
            "presentmentMoney": { "amount": "29.0", "currencyCode": "CAD" }
        })
    );
    assert_eq!(
        order["totalTaxSet"],
        json!({
            "shopMoney": { "amount": "0.0", "currencyCode": "CAD" },
            "presentmentMoney": { "amount": "0.0", "currencyCode": "CAD" }
        })
    );
    assert_eq!(
        order["totalReceivedSet"],
        json!({
            "shopMoney": { "amount": "32.25", "currencyCode": "CAD" },
            "presentmentMoney": { "amount": "32.25", "currencyCode": "CAD" }
        })
    );
    assert_eq!(order["lineItems"]["nodes"].as_array().unwrap().len(), 2);
    assert_ne!(
        order["lineItems"]["nodes"][0]["id"],
        json!("gid://shopify/LineItem/5")
    );
    assert_eq!(order["lineItems"]["nodes"][1]["sku"], json!("COMPLETE-B"));
}

#[test]
fn draft_order_complete_rejects_already_completed_draft_without_rewriting_order() {
    let mut proxy = snapshot_proxy();
    let create = proxy.process_request(json_graphql_request(
        include_str!("../../config/parity-requests/orders/draftOrderComplete-stages-resulting-order-create.graphql"),
        json!({}),
    ));
    assert_eq!(create.status, 200);
    let draft_id = create.body["data"]["draftOrderCreate"]["draftOrder"]["id"].clone();

    let first_complete = proxy.process_request(json_graphql_request(
        include_str!("../../config/parity-requests/orders/draftOrderComplete-stages-resulting-order-complete.graphql"),
        json!({"id": draft_id.clone(), "paymentPending": false}),
    ));
    assert_eq!(first_complete.status, 200);
    assert_eq!(
        first_complete.body["data"]["draftOrderComplete"]["userErrors"],
        json!([])
    );
    let first_completed_order =
        first_complete.body["data"]["draftOrderComplete"]["draftOrder"]["order"].clone();

    let orders_after_first = proxy.process_request(json_graphql_request(
        r#"
            query CompletedDraftOrderList {
              orders(first: 10) {
                nodes {
                  id
                  name
                  displayFinancialStatus
                }
              }
              ordersCount { count precision }
            }
        "#,
        json!({}),
    ));
    assert_eq!(orders_after_first.status, 200);

    let second_complete = proxy.process_request(json_graphql_request(
        r#"
            mutation CompleteDraftAgain($id: ID!) {
              draftOrderComplete(id: $id, sourceName: "hermes-cron-orders") {
                draftOrder {
                  id
                  status
                  order { id name }
                }
                userErrors {
                  field
                  message
                }
              }
            }
        "#,
        json!({"id": draft_id}),
    ));
    assert_eq!(second_complete.status, 200);
    assert_eq!(
        second_complete.body["data"]["draftOrderComplete"]["draftOrder"]["id"],
        create.body["data"]["draftOrderCreate"]["draftOrder"]["id"]
    );
    assert_eq!(
        second_complete.body["data"]["draftOrderComplete"]["draftOrder"]["status"],
        json!("COMPLETED")
    );
    assert_eq!(
        second_complete.body["data"]["draftOrderComplete"]["draftOrder"]["order"],
        json!({
            "id": first_completed_order["id"].clone(),
            "name": first_completed_order["name"].clone()
        })
    );
    assert_eq!(
        second_complete.body["data"]["draftOrderComplete"]["userErrors"],
        json!([{
            "field": Value::Null,
            "message": "This order has been paid"
        }])
    );

    let orders_after_second = proxy.process_request(json_graphql_request(
        r#"
            query CompletedDraftOrderList {
              orders(first: 10) {
                nodes {
                  id
                  name
                  displayFinancialStatus
                }
              }
              ordersCount { count precision }
            }
        "#,
        json!({}),
    ));
    assert_eq!(orders_after_second.body, orders_after_first.body);
}

#[test]
fn draft_order_complete_dispatches_by_root_for_ordinary_operation_names() {
    let mut proxy = snapshot_proxy();

    let create = proxy.process_request(json_graphql_request(
        r#"
            mutation MakeDraft {
              draftOrderCreate(
                input: {
                  email: "ordinary-completion-root@example.com"
                  lineItems: [{ title: "Completion service", quantity: 2, originalUnitPrice: "12.50", sku: "COMPLETE" }]
                }
              ) {
                draftOrder {
                  id
                  name
                  status
                  totalPriceSet {
                    shopMoney {
                      amount
                      currencyCode
                    }
                  }
                }
                userErrors {
                  field
                  message
                }
              }
            }
        "#,
        json!({}),
    ));
    assert_eq!(create.status, 200);
    assert_eq!(
        create.body["data"]["draftOrderCreate"]["draftOrder"]["status"],
        json!("OPEN")
    );
    let draft_id = create.body["data"]["draftOrderCreate"]["draftOrder"]["id"].clone();

    let complete = proxy.process_request(json_graphql_request(
        r#"
            mutation CompleteDraft($id: ID!, $paymentPending: Boolean) {
              draftOrderComplete(id: $id, sourceName: "hermes-cron-orders", paymentPending: $paymentPending) {
                draftOrder {
                  id
                  status
                  completedAt
                  order {
                    id
                    name
                    sourceName
                    displayFinancialStatus
                    displayFulfillmentStatus
                    currentTotalPriceSet {
                      shopMoney {
                        amount
                        currencyCode
                      }
                    }
                    lineItems {
                      nodes {
                        id
                        title
                        quantity
                        sku
                      }
                    }
                  }
                }
                userErrors {
                  field
                  message
                }
              }
            }
        "#,
        json!({"id": draft_id, "paymentPending": false}),
    ));
    assert_eq!(complete.status, 200);
    assert_eq!(
        complete.body["data"]["draftOrderComplete"]["draftOrder"]["order"]["lineItems"]["nodes"][0]
            ["sku"],
        json!("COMPLETE")
    );
}

#[test]
fn refund_create_stages_refund_and_downstream_order_reads() {
    let mut proxy = snapshot_proxy();
    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateRefundableOrder($order: OrderCreateOrderInput!) {
          orderCreate(order: $order) {
            order {
              id
              displayFinancialStatus
              lineItems(first: 5) {
                nodes {
                  id
                  title
                  quantity
                }
              }
            }
            userErrors {
              field
              message
            }
          }
        }
        "#,
        json!({
            "order": {
                "email": "refund-create@example.test",
                "currency": "CAD",
                "lineItems": [{
                    "title": "Refundable order item",
                    "quantity": 2,
                    "priceSet": { "shopMoney": { "amount": "10.00", "currencyCode": "CAD" } }
                }],
                "transactions": [{
                    "kind": "SALE",
                    "status": "SUCCESS",
                    "gateway": "manual",
                    "amountSet": { "shopMoney": { "amount": "20.00", "currencyCode": "CAD" } }
                }]
            }
        }),
    ));
    assert_eq!(create.status, 200);
    assert_eq!(create.body["data"]["orderCreate"]["userErrors"], json!([]));
    let order_id = create.body["data"]["orderCreate"]["order"]["id"].clone();
    let line_item_id =
        create.body["data"]["orderCreate"]["order"]["lineItems"]["nodes"][0]["id"].clone();
    let payment_read = proxy.process_request(json_graphql_request(
        r#"
        query ReadRefundParentTransaction($id: ID!) {
          order(id: $id) {
            transactions {
              id
              kind
              status
              gateway
            }
          }
        }
        "#,
        json!({ "id": order_id }),
    ));
    let parent_transaction_id = payment_read.body["data"]["order"]["transactions"][0]["id"].clone();

    let refund_query = r#"
        mutation CreateRefund($input: RefundInput!) {
          refundCreate(input: $input) {
            refund {
              id
              note
              totalRefundedSet {
                shopMoney {
                  amount
                  currencyCode
                }
              }
              refundLineItems(first: 5) {
                nodes {
                  id
                  quantity
                  restockType
                  lineItem {
                    id
                    title
                  }
                  subtotalSet {
                    shopMoney {
                      amount
                      currencyCode
                    }
                  }
                }
              }
              transactions(first: 5) {
                nodes {
                  id
                  kind
                  status
                  gateway
                  amountSet {
                    shopMoney {
                      amount
                      currencyCode
                    }
                  }
                }
              }
            }
            order {
              id
              displayFinancialStatus
              totalRefundedSet {
                shopMoney {
                  amount
                  currencyCode
                }
              }
              totalRefundedShippingSet {
                shopMoney {
                  amount
                  currencyCode
                }
              }
            }
            userErrors {
              field
              message
            }
          }
        }
    "#;
    let variables = json!({
        "input": {
            "orderId": order_id,
            "note": "Customer returned one item",
            "refundLineItems": [{
                "lineItemId": line_item_id,
                "quantity": 1,
                "restockType": "RETURN"
            }],
            "shipping": {
                "amount": "5.00"
            },
            "transactions": [{
                "parentId": parent_transaction_id,
                "kind": "REFUND",
                "gateway": "manual",
                "orderId": order_id,
                "amount": "15.00"
            }]
        }
    });

    let response = proxy.process_request(json_graphql_request(refund_query, variables));

    assert_eq!(response.status, 200);
    assert_eq!(response.body["errors"], Value::Null);
    assert_eq!(
        response.body["data"]["refundCreate"]["userErrors"],
        json!([])
    );
    let payload = &response.body["data"]["refundCreate"];
    assert_eq!(payload["refund"]["id"], json!("gid://shopify/Refund/1"));
    assert_eq!(
        payload["refund"]["note"],
        json!("Customer returned one item")
    );
    assert_eq!(
        payload["refund"]["totalRefundedSet"]["shopMoney"],
        json!({ "amount": "15.0", "currencyCode": "CAD" })
    );
    assert_eq!(
        payload["refund"]["refundLineItems"]["nodes"][0]["quantity"],
        json!(1)
    );
    assert_eq!(
        payload["refund"]["refundLineItems"]["nodes"][0]["restockType"],
        json!("RETURN")
    );
    assert_eq!(
        payload["refund"]["refundLineItems"]["nodes"][0]["lineItem"]["title"],
        json!("Refundable order item")
    );
    assert_eq!(
        payload["refund"]["transactions"]["nodes"][0]["kind"],
        json!("REFUND")
    );
    assert_eq!(
        payload["refund"]["transactions"]["nodes"][0]["status"],
        json!("SUCCESS")
    );
    assert_eq!(
        payload["refund"]["transactions"]["nodes"][0]["gateway"],
        json!("manual")
    );
    assert_eq!(
        payload["order"]["displayFinancialStatus"],
        json!("PARTIALLY_REFUNDED")
    );
    assert_eq!(
        payload["order"]["totalRefundedSet"]["shopMoney"],
        json!({ "amount": "15.0", "currencyCode": "CAD" })
    );
    assert_eq!(
        payload["order"]["totalRefundedShippingSet"]["shopMoney"],
        json!({ "amount": "5.0", "currencyCode": "CAD" })
    );

    let downstream = proxy.process_request(json_graphql_request(
        r#"
        query ReadRefundedOrder($id: ID!) {
          order(id: $id) {
            id
            displayFinancialStatus
            refunds {
              id
              note
              totalRefundedSet {
                shopMoney {
                  amount
                  currencyCode
                }
              }
            }
            returns(first: 5) {
              nodes {
                id
                status
              }
              pageInfo {
                hasNextPage
                hasPreviousPage
                startCursor
                endCursor
              }
            }
            transactions {
              id
              kind
              status
              gateway
              amountSet {
                shopMoney {
                  amount
                  currencyCode
                }
              }
            }
            totalRefundedSet {
              shopMoney {
                amount
                currencyCode
              }
            }
            totalRefundedShippingSet {
              shopMoney {
                amount
                currencyCode
              }
            }
          }
        }
        "#,
        json!({ "id": payload["order"]["id"].clone() }),
    ));
    let order = &downstream.body["data"]["order"];
    assert_eq!(order["refunds"][0]["id"], payload["refund"]["id"]);
    assert_eq!(
        order["refunds"][0]["note"],
        json!("Customer returned one item")
    );
    assert_eq!(
        order["returns"],
        json!({
            "nodes": [],
            "pageInfo": {
                "hasNextPage": false,
                "hasPreviousPage": false,
                "startCursor": Value::Null,
                "endCursor": Value::Null
            }
        })
    );
    assert_eq!(order["transactions"][1]["kind"], json!("REFUND"));
    assert_eq!(
        order["totalRefundedSet"]["shopMoney"],
        json!({ "amount": "15.0", "currencyCode": "CAD" })
    );
    assert_eq!(
        order["totalRefundedShippingSet"]["shopMoney"],
        json!({ "amount": "5.0", "currencyCode": "CAD" })
    );

    let log = log_snapshot(&proxy);
    assert_eq!(log["entries"][1]["operationName"], json!("refundCreate"));
    assert_eq!(log["entries"][1]["status"], json!("staged"));
    assert_eq!(
        log["entries"][1]["interpreted"]["capability"],
        json!({
            "operationName": "refundCreate",
            "domain": "orders",
            "execution": "stage-locally"
        })
    );
    assert!(log["entries"][1]["rawBody"]
        .as_str()
        .is_some_and(|body| body.contains("CreateRefund")));
}

#[test]
fn refund_create_recomputes_sequential_refund_order_money_rollups() {
    let mut proxy = snapshot_proxy();
    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateRefundableOrder($order: OrderCreateOrderInput!) {
          orderCreate(order: $order) {
            order {
              id
              lineItems(first: 5) {
                nodes { id }
              }
              transactions {
                id
              }
            }
            userErrors {
              field
              message
            }
          }
        }
        "#,
        json!({
            "order": {
                "email": "sequential-refund-rollups@example.test",
                "currency": "USD",
                "lineItems": [{
                    "title": "Refund rollup item",
                    "quantity": 2,
                    "priceSet": { "shopMoney": { "amount": "45.00", "currencyCode": "USD" } }
                }],
                "shippingLines": [{
                    "title": "Ground",
                    "priceSet": { "shopMoney": { "amount": "10.00", "currencyCode": "USD" } }
                }],
                "transactions": [{
                    "kind": "SALE",
                    "status": "SUCCESS",
                    "gateway": "manual",
                    "amountSet": { "shopMoney": { "amount": "100.00", "currencyCode": "USD" } }
                }]
            }
        }),
    ));
    assert_eq!(create.status, 200);
    assert_eq!(create.body["data"]["orderCreate"]["userErrors"], json!([]));
    let order = &create.body["data"]["orderCreate"]["order"];
    let order_id = order["id"].clone();
    let line_item_id = order["lineItems"]["nodes"][0]["id"].clone();
    let parent_transaction_id = order["transactions"][0]["id"].clone();

    let refund_query = r#"
        mutation Refund($input: RefundInput!) {
          refundCreate(input: $input) {
            order {
              id
              netPaymentSet { shopMoney { amount currencyCode } }
              totalRefundedSet { shopMoney { amount currencyCode } }
              totalRefundedShippingSet { shopMoney { amount currencyCode } }
              displayFinancialStatus
            }
            userErrors { field message }
          }
        }
    "#;
    let refund_variables = |amount: &str| {
        json!({
            "input": {
                "orderId": order_id.clone(),
                "refundLineItems": [{
                    "lineItemId": line_item_id.clone(),
                    "quantity": 1,
                    "restockType": "NO_RESTOCK"
                }],
                "shipping": { "fullRefund": true },
                "transactions": [{
                    "parentId": parent_transaction_id.clone(),
                    "kind": "REFUND",
                    "gateway": "manual",
                    "orderId": order_id.clone(),
                    "amount": amount
                }]
            }
        })
    };

    let first = proxy.process_request(json_graphql_request(
        refund_query,
        refund_variables("55.00"),
    ));
    assert_eq!(first.status, 200);
    assert_eq!(first.body["data"]["refundCreate"]["userErrors"], json!([]));
    assert_eq!(
        first.body["data"]["refundCreate"]["order"]["netPaymentSet"]["shopMoney"],
        json!({ "amount": "45.0", "currencyCode": "USD" })
    );
    assert_eq!(
        first.body["data"]["refundCreate"]["order"]["totalRefundedSet"]["shopMoney"],
        json!({ "amount": "55.0", "currencyCode": "USD" })
    );
    assert_eq!(
        first.body["data"]["refundCreate"]["order"]["totalRefundedShippingSet"]["shopMoney"],
        json!({ "amount": "10.0", "currencyCode": "USD" })
    );
    assert_eq!(
        first.body["data"]["refundCreate"]["order"]["displayFinancialStatus"],
        json!("PARTIALLY_REFUNDED")
    );

    let second = proxy.process_request(json_graphql_request(
        refund_query,
        refund_variables("45.00"),
    ));
    assert_eq!(second.status, 200);
    assert_eq!(second.body["data"]["refundCreate"]["userErrors"], json!([]));
    assert_eq!(
        second.body["data"]["refundCreate"]["order"]["netPaymentSet"]["shopMoney"],
        json!({ "amount": "0.0", "currencyCode": "USD" })
    );
    assert_eq!(
        second.body["data"]["refundCreate"]["order"]["totalRefundedSet"]["shopMoney"],
        json!({ "amount": "100.0", "currencyCode": "USD" })
    );
    assert_eq!(
        second.body["data"]["refundCreate"]["order"]["totalRefundedShippingSet"]["shopMoney"],
        json!({ "amount": "10.0", "currencyCode": "USD" })
    );
    assert_eq!(
        second.body["data"]["refundCreate"]["order"]["displayFinancialStatus"],
        json!("REFUNDED")
    );

    let downstream = proxy.process_request(json_graphql_request(
        r#"
        query ReadSequentialRefundRollups($id: ID!) {
          order(id: $id) {
            netPaymentSet { shopMoney { amount currencyCode } }
            totalRefundedSet { shopMoney { amount currencyCode } }
            totalRefundedShippingSet { shopMoney { amount currencyCode } }
            refunds { id }
          }
        }
        "#,
        json!({ "id": order_id }),
    ));
    let downstream_order = &downstream.body["data"]["order"];
    assert_eq!(
        downstream_order["netPaymentSet"]["shopMoney"],
        json!({ "amount": "0.0", "currencyCode": "USD" })
    );
    assert_eq!(
        downstream_order["totalRefundedSet"]["shopMoney"],
        json!({ "amount": "100.0", "currencyCode": "USD" })
    );
    assert_eq!(
        downstream_order["totalRefundedShippingSet"]["shopMoney"],
        json!({ "amount": "10.0", "currencyCode": "USD" })
    );
    assert_eq!(downstream_order["refunds"].as_array().unwrap().len(), 2);
}

#[test]
fn refund_create_user_errors_do_not_fall_back_to_not_implemented_or_stage_state() {
    let mut proxy = snapshot_proxy();
    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateRefundableOrder($order: OrderCreateOrderInput!) {
          orderCreate(order: $order) {
            order {
              id
              lineItems(first: 5) {
                nodes {
                  id
                }
              }
            }
            userErrors {
              field
              message
            }
          }
        }
        "#,
        json!({
            "order": {
                "email": "refund-guardrail@example.test",
                "currency": "EUR",
                "lineItems": [{
                    "title": "Refund guardrail item",
                    "quantity": 1,
                    "priceSet": { "shopMoney": { "amount": "10.00", "currencyCode": "EUR" } }
                }],
                "transactions": [{
                    "kind": "SALE",
                    "status": "SUCCESS",
                    "gateway": "manual",
                    "amountSet": { "shopMoney": { "amount": "10.00", "currencyCode": "EUR" } }
                }]
            }
        }),
    ));
    let order_id = create.body["data"]["orderCreate"]["order"]["id"].clone();
    let line_item_id =
        create.body["data"]["orderCreate"]["order"]["lineItems"]["nodes"][0]["id"].clone();
    let payment_read = proxy.process_request(json_graphql_request(
        r#"
        query ReadRefundParentTransaction($id: ID!) {
          order(id: $id) {
            transactions {
              id
            }
          }
        }
        "#,
        json!({ "id": order_id }),
    ));
    let parent_transaction_id = payment_read.body["data"]["order"]["transactions"][0]["id"].clone();

    let refund_query = r#"
        mutation CreateRefund($input: RefundInput!) {
          refundCreate(input: $input) {
            refund {
              id
            }
            order {
              id
              displayFinancialStatus
              totalRefundedSet {
                shopMoney {
                  amount
                  currencyCode
                }
              }
              refunds {
                id
              }
            }
            userErrors {
              field
              message
            }
          }
        }
    "#;

    let over_refund = proxy.process_request(json_graphql_request(
        refund_query,
        json!({
            "input": {
                "orderId": order_id.clone(),
                "transactions": [{
                    "parentId": parent_transaction_id.clone(),
                    "kind": "REFUND",
                    "gateway": "manual",
                    "orderId": order_id.clone(),
                    "amount": "15.00"
                }]
            }
        }),
    ));
    assert_eq!(
        over_refund.body["data"]["refundCreate"]["refund"],
        Value::Null
    );
    assert_eq!(
        over_refund.body["data"]["refundCreate"]["userErrors"][0],
        json!({
            "field": ["transactions"],
            "message": "Refund amount 15.00 EUR is greater than net payment received 10.00 EUR"
        })
    );
    assert!(
        !over_refund.body["data"]["refundCreate"]["userErrors"][0]["message"]
            .as_str()
            .expect("over-refund userError message")
            .contains('$')
    );
    assert_ne!(
        over_refund.body["data"]["refundCreate"]["userErrors"][0]["message"],
        json!("Local staging for refundCreate is not implemented for this request shape")
    );
    assert_eq!(
        over_refund.body["data"]["refundCreate"]["order"]["refunds"],
        json!([])
    );

    let over_quantity = proxy.process_request(json_graphql_request(
        refund_query,
        json!({
            "input": {
                "orderId": order_id.clone(),
                "refundLineItems": [{
                    "lineItemId": line_item_id,
                    "quantity": 2,
                    "restockType": "RETURN"
                }],
                "transactions": [{
                    "parentId": parent_transaction_id,
                    "kind": "REFUND",
                    "gateway": "manual",
                    "orderId": order_id,
                    "amount": "10.00"
                }]
            }
        }),
    ));
    assert_eq!(
        over_quantity.body["data"]["refundCreate"]["refund"],
        Value::Null
    );
    assert_eq!(
        over_quantity.body["data"]["refundCreate"]["userErrors"][0],
        json!({
            "field": ["refundLineItems", "0", "quantity"],
            "message": "Quantity cannot refund more items than were purchased"
        })
    );

    let log = log_snapshot(&proxy);
    assert_eq!(log["entries"].as_array().expect("log entries").len(), 1);
    assert_eq!(log["entries"][0]["operationName"], json!("orderCreate"));
}

#[test]
fn refund_create_caps_sequential_refunds_by_remaining_quantity() {
    let mut proxy = snapshot_proxy();
    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation CreatePartiallyRefundableOrder($order: OrderCreateOrderInput!) {
          orderCreate(order: $order) {
            order {
              id
              lineItems(first: 5) {
                nodes {
                  id
                  title
                }
              }
            }
            userErrors {
              field
              message
            }
          }
        }
        "#,
        json!({
            "order": {
                "email": "sequential-refunds@example.test",
                "currency": "USD",
                "lineItems": [
                    {
                        "title": "Refund cap line A",
                        "quantity": 3,
                        "priceSet": { "shopMoney": { "amount": "10.00", "currencyCode": "USD" } }
                    },
                    {
                        "title": "Refund cap line B",
                        "quantity": 1,
                        "priceSet": { "shopMoney": { "amount": "70.00", "currencyCode": "USD" } }
                    }
                ],
                "transactions": [{
                    "kind": "SALE",
                    "status": "SUCCESS",
                    "gateway": "manual",
                    "amountSet": { "shopMoney": { "amount": "100.00", "currencyCode": "USD" } }
                }]
            }
        }),
    ));
    assert_eq!(create.status, 200);
    assert_eq!(create.body["data"]["orderCreate"]["userErrors"], json!([]));
    let order_id = create.body["data"]["orderCreate"]["order"]["id"].clone();
    let line_a_id =
        create.body["data"]["orderCreate"]["order"]["lineItems"]["nodes"][0]["id"].clone();
    let line_b_id =
        create.body["data"]["orderCreate"]["order"]["lineItems"]["nodes"][1]["id"].clone();
    let payment_read = proxy.process_request(json_graphql_request(
        r#"
        query ReadSequentialRefundParentTransaction($id: ID!) {
          order(id: $id) {
            transactions {
              id
            }
          }
        }
        "#,
        json!({ "id": order_id.clone() }),
    ));
    let parent_transaction_id = payment_read.body["data"]["order"]["transactions"][0]["id"].clone();

    let refund_query = r#"
        mutation SequentialRefund($input: RefundInput!) {
          refundCreate(input: $input) {
            refund {
              id
              totalRefundedSet {
                shopMoney {
                  amount
                  currencyCode
                }
              }
            }
            order {
              id
              totalRefundedSet {
                shopMoney {
                  amount
                  currencyCode
                }
              }
            }
            userErrors {
              field
              message
            }
          }
        }
    "#;

    let first_refund = proxy.process_request(json_graphql_request(
        refund_query,
        json!({
            "input": {
                "orderId": order_id.clone(),
                "refundLineItems": [{
                    "lineItemId": line_a_id.clone(),
                    "quantity": 3,
                    "restockType": "RETURN"
                }],
                "transactions": [{
                    "parentId": parent_transaction_id.clone(),
                    "kind": "REFUND",
                    "gateway": "manual",
                    "orderId": order_id.clone(),
                    "amount": "30.00"
                }]
            }
        }),
    ));

    let read_after_first = proxy.process_request(json_graphql_request(
        r#"
        query ReadRefundableQuantitiesAfterFirstRefund($id: ID!) {
          order(id: $id) {
            lineItems(first: 5) {
              nodes {
                id
                refundableQuantity
              }
            }
          }
        }
        "#,
        json!({ "id": order_id.clone() }),
    ));

    let second_refund = proxy.process_request(json_graphql_request(
        refund_query,
        json!({
            "input": {
                "orderId": order_id.clone(),
                "refundLineItems": [{
                    "lineItemId": line_a_id,
                    "quantity": 3,
                    "restockType": "RETURN"
                }],
                "transactions": [{
                    "parentId": parent_transaction_id,
                    "kind": "REFUND",
                    "gateway": "manual",
                    "orderId": order_id.clone(),
                    "amount": "30.00"
                }]
            }
        }),
    ));

    let read_after_second_attempt = proxy.process_request(json_graphql_request(
        r#"
        query ReadRefundableQuantitiesAfterSecondAttempt($id: ID!) {
          order(id: $id) {
            totalRefundedSet {
              shopMoney {
                amount
                currencyCode
              }
            }
            refunds {
              id
            }
            lineItems(first: 5) {
              nodes {
                id
                refundableQuantity
              }
            }
          }
        }
        "#,
        json!({ "id": order_id }),
    ));

    assert_eq!(first_refund.status, 200);
    assert_eq!(read_after_first.status, 200);
    assert_eq!(second_refund.status, 200);
    assert_eq!(read_after_second_attempt.status, 200);
    assert_eq!(
        json!({
            "firstRefundUserErrors": first_refund.body["data"]["refundCreate"]["userErrors"].clone(),
            "firstRefundTotal": first_refund.body["data"]["refundCreate"]["refund"]["totalRefundedSet"]["shopMoney"].clone(),
            "lineAAfterFirst": read_after_first.body["data"]["order"]["lineItems"]["nodes"][0]["refundableQuantity"].clone(),
            "lineBAfterFirst": read_after_first.body["data"]["order"]["lineItems"]["nodes"][1]["refundableQuantity"].clone(),
            "secondRefund": second_refund.body["data"]["refundCreate"]["refund"].clone(),
            "secondRefundUserErrors": second_refund.body["data"]["refundCreate"]["userErrors"].clone(),
            "totalAfterSecondAttempt": read_after_second_attempt.body["data"]["order"]["totalRefundedSet"]["shopMoney"].clone(),
            "refundCountAfterSecondAttempt": read_after_second_attempt.body["data"]["order"]["refunds"].as_array().expect("refunds array").len(),
            "lineAAfterSecondAttempt": read_after_second_attempt.body["data"]["order"]["lineItems"]["nodes"][0]["refundableQuantity"].clone(),
            "lineBAfterSecondAttempt": read_after_second_attempt.body["data"]["order"]["lineItems"]["nodes"][1]["refundableQuantity"].clone(),
            "lineBIdPreserved": read_after_second_attempt.body["data"]["order"]["lineItems"]["nodes"][1]["id"] == line_b_id
        }),
        json!({
            "firstRefundUserErrors": [],
            "firstRefundTotal": { "amount": "30.0", "currencyCode": "USD" },
            "lineAAfterFirst": 0,
            "lineBAfterFirst": 1,
            "secondRefund": Value::Null,
            "secondRefundUserErrors": [{
                "field": ["refundLineItems", "0", "quantity"],
                "message": "Quantity cannot refund more items than were purchased"
            }],
            "totalAfterSecondAttempt": { "amount": "30.0", "currencyCode": "USD" },
            "refundCountAfterSecondAttempt": 1,
            "lineAAfterSecondAttempt": 0,
            "lineBAfterSecondAttempt": 1,
            "lineBIdPreserved": true
        })
    );
}

#[test]
fn draft_order_invoice_send_success_marks_invoice_sent_and_read_back_matches() {
    let mut proxy = snapshot_proxy();

    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateDraftForInvoiceSend {
          draftOrderCreate(input: {
            lineItems: [{ title: "Invoice transition item", quantity: 1, originalUnitPrice: "1.00" }]
          }) {
            draftOrder { id status invoiceSentAt }
            userErrors { field message }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(create.status, 200);
    let draft_order_id = create.body["data"]["draftOrderCreate"]["draftOrder"]["id"].clone();
    assert_eq!(
        create.body["data"]["draftOrderCreate"]["draftOrder"]["status"],
        json!("OPEN")
    );
    assert_eq!(
        create.body["data"]["draftOrderCreate"]["draftOrder"]["invoiceSentAt"],
        Value::Null
    );

    let send = proxy.process_request(json_graphql_request(
        r#"
        mutation SendDraftInvoice($id: ID!, $email: EmailInput) {
          draftOrderInvoiceSend(id: $id, email: $email) {
            draftOrder { id status invoiceSentAt }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "id": draft_order_id,
            "email": {
                "to": "buyer@example.com",
                "subject": "Draft invoice",
                "customMessage": "Thanks for the order"
            }
        }),
    ));
    assert_eq!(send.status, 200);
    let sent_draft = &send.body["data"]["draftOrderInvoiceSend"]["draftOrder"];
    assert_eq!(sent_draft["status"], json!("INVOICE_SENT"));
    let invoice_sent_at = sent_draft["invoiceSentAt"]
        .as_str()
        .expect("invoiceSentAt should be synthesized")
        .to_string();
    assert!(!invoice_sent_at.is_empty());
    assert_eq!(
        send.body["data"]["draftOrderInvoiceSend"]["userErrors"],
        json!([])
    );
    let read_back = proxy.process_request(json_graphql_request(
        r#"
        query ReadSentDraft($id: ID!) {
          draftOrder(id: $id) { id status invoiceSentAt }
        }
        "#,
        json!({ "id": sent_draft["id"] }),
    ));
    assert_eq!(read_back.status, 200);
    let read_draft = &read_back.body["data"]["draftOrder"];
    assert_eq!(read_draft["status"], json!("INVOICE_SENT"));
    assert_eq!(read_draft["invoiceSentAt"], json!(invoice_sent_at));

    let log = log_snapshot(&proxy);
    let entries = log["entries"]
        .as_array()
        .expect("invoice send should write mutation log entries");
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[1]["operationName"], json!("draftOrderInvoiceSend"));
    assert_eq!(entries[1]["status"], json!("staged"));
}

#[test]
fn draft_order_invoice_send_validation_branches_do_not_mark_invoice_sent() {
    let mut missing_recipient_proxy = snapshot_proxy();
    let missing_create = missing_recipient_proxy.process_request(json_graphql_request(
        r#"
        mutation CreateDraftMissingRecipient {
          draftOrderCreate(input: {
            lineItems: [{ title: "Missing recipient item", quantity: 1, originalUnitPrice: "1.00" }]
          }) {
            draftOrder { id status invoiceSentAt }
          }
        }
        "#,
        json!({}),
    ));
    let missing_draft_id =
        missing_create.body["data"]["draftOrderCreate"]["draftOrder"]["id"].clone();
    let missing_send = missing_recipient_proxy.process_request(json_graphql_request(
        r#"
        mutation SendWithoutRecipient($id: ID!) {
          draftOrderInvoiceSend(id: $id) {
            draftOrder { id status invoiceSentAt }
            userErrors { field message }
          }
        }
        "#,
        json!({ "id": missing_draft_id }),
    ));
    let missing_payload = &missing_send.body["data"]["draftOrderInvoiceSend"];
    assert_eq!(missing_payload["draftOrder"]["status"], json!("OPEN"));
    assert_eq!(missing_payload["draftOrder"]["invoiceSentAt"], Value::Null);
    assert_eq!(
        missing_payload["userErrors"][0]["message"],
        json!("To can't be blank")
    );

    let mut completed_proxy = snapshot_proxy();
    let completed_create = completed_proxy.process_request(json_graphql_request(
        r#"
        mutation CreateDraftForCompletedGuard {
          draftOrderCreate(input: {
            email: "buyer@example.com",
            lineItems: [{ title: "Completed guard item", quantity: 1, originalUnitPrice: "1.00" }]
          }) {
            draftOrder { id status invoiceSentAt }
          }
        }
        "#,
        json!({}),
    ));
    let completed_draft_id =
        completed_create.body["data"]["draftOrderCreate"]["draftOrder"]["id"].clone();
    let complete = completed_proxy.process_request(json_graphql_request(
        r#"
        mutation CompleteDraftForInvoiceGuard($id: ID!) {
          draftOrderComplete(id: $id) {
            draftOrder { id status invoiceSentAt }
            userErrors { field message }
          }
        }
        "#,
        json!({ "id": completed_draft_id }),
    ));
    assert_eq!(
        complete.body["data"]["draftOrderComplete"]["draftOrder"]["status"],
        json!("COMPLETED")
    );
    assert_eq!(
        complete.body["data"]["draftOrderComplete"]["draftOrder"]["invoiceSentAt"],
        Value::Null
    );

    let paid_send = completed_proxy.process_request(json_graphql_request(
        r#"
        mutation SendCompletedDraftInvoice($id: ID!, $email: EmailInput) {
          draftOrderInvoiceSend(id: $id, email: $email) {
            draftOrder { id status invoiceSentAt }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "id": completed_draft_id,
            "email": { "to": "buyer@example.com" }
        }),
    ));
    let paid_payload = &paid_send.body["data"]["draftOrderInvoiceSend"];
    assert_eq!(paid_payload["draftOrder"]["status"], json!("COMPLETED"));
    assert_eq!(paid_payload["draftOrder"]["invoiceSentAt"], Value::Null);
    assert_eq!(
        paid_payload["userErrors"][0]["message"],
        json!("Draft order Invoice can't be sent. This draft order is already paid.")
    );
}

#[test]
fn draft_order_invoice_send_validation_projects_created_draft_shape() {
    let mut proxy = snapshot_proxy();
    restore_shop_currency(&mut proxy, "USD");

    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateDraftForInvoiceProjection($input: DraftOrderInput!) {
          draftOrderCreate(input: $input) {
            draftOrder {
              id
              name
              status
              totalQuantityOfLineItems
              totalPriceSet { shopMoney { amount currencyCode } }
              lineItems(first: 5) {
                nodes {
                  title
                  quantity
                  originalUnitPriceSet { shopMoney { amount currencyCode } }
                  originalTotalSet { shopMoney { amount currencyCode } }
                }
              }
            }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "input": {
                "lineItems": [{
                    "title": "Invoice projection item",
                    "quantity": 2,
                    "originalUnitPrice": "3.25"
                }]
            }
        }),
    ));
    assert_eq!(create.status, 200);
    assert_eq!(
        create.body["data"]["draftOrderCreate"]["userErrors"],
        json!([])
    );
    let created_draft = create.body["data"]["draftOrderCreate"]["draftOrder"].clone();
    assert_eq!(
        created_draft["lineItems"]["nodes"][0]["title"],
        json!("Invoice projection item")
    );
    assert_eq!(created_draft["totalQuantityOfLineItems"], json!(2));
    assert_eq!(
        created_draft["totalPriceSet"]["shopMoney"],
        json!({ "amount": "6.5", "currencyCode": "USD" })
    );
    let draft_order_id = created_draft["id"].clone();

    let send = proxy.process_request(json_graphql_request(
        r#"
        mutation SendDraftInvoiceWithoutRecipient($id: ID!) {
          draftOrderInvoiceSend(id: $id) {
            draftOrder {
              id
              name
              status
              totalQuantityOfLineItems
              totalPriceSet { shopMoney { amount currencyCode } }
              lineItems(first: 5) {
                nodes {
                  title
                  quantity
                  originalUnitPriceSet { shopMoney { amount currencyCode } }
                  originalTotalSet { shopMoney { amount currencyCode } }
                }
              }
            }
            userErrors { field message }
          }
        }
        "#,
        json!({ "id": draft_order_id }),
    ));
    assert_eq!(send.status, 200);
    let payload = &send.body["data"]["draftOrderInvoiceSend"];
    assert_eq!(payload["draftOrder"], created_draft);
    assert_eq!(
        payload["userErrors"][0]["message"],
        json!("To can't be blank")
    );
}

#[test]
fn order_update_rejects_invalid_country_province_pairs_generally() {
    let mut proxy = snapshot_proxy();
    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateOrderForProvinceValidation($order: OrderCreateOrderInput!) {
          orderCreate(order: $order) {
            order { id shippingAddress { countryCodeV2 provinceCode } }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "order": {
                "email": "province-validation@example.test",
                "lineItems": [{
                    "title": "Province validation item",
                    "quantity": 1,
                    "priceSet": { "shopMoney": { "amount": "9.00", "currencyCode": "CAD" } }
                }],
                "shippingAddress": {
                    "firstName": "Valid",
                    "lastName": "Province",
                    "address1": "1 Main St",
                    "city": "Toronto",
                    "countryCode": "CA",
                    "provinceCode": "ON",
                    "zip": "M5V 2T6"
                }
            }
        }),
    ));
    assert_eq!(create.status, 200);
    assert_eq!(create.body["data"]["orderCreate"]["userErrors"], json!([]));
    let order_id = create.body["data"]["orderCreate"]["order"]["id"].clone();

    let invalid = proxy.process_request(json_graphql_request(
        r#"
        mutation UpdateOrderInvalidProvince($input: OrderInput!) {
          orderUpdate(input: $input) {
            order { id shippingAddress { countryCodeV2 provinceCode } }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "input": {
                "id": order_id,
                "shippingAddress": {
                    "firstName": "Invalid",
                    "lastName": "Province",
                    "address1": "1 Main St",
                    "city": "Toronto",
                    "countryCode": "CA",
                    "provinceCode": "NY",
                    "zip": "M5V 2T6"
                }
            }
        }),
    ));
    assert_eq!(invalid.status, 200);
    assert_eq!(
        invalid.body["data"]["orderUpdate"],
        json!({
            "order": {
                "id": create.body["data"]["orderCreate"]["order"]["id"].clone(),
                "shippingAddress": {
                    "countryCodeV2": "CA",
                    "provinceCode": "ON"
                }
            },
            "userErrors": [{
                "field": ["shippingAddress", "province"],
                "message": "Province is not a valid province in Canada"
            }]
        })
    );
}

#[test]
fn remaining_order_fixture_backed_edges_replay_without_passthrough_logs() {
    let residual_fixture: Value = serde_json::from_str(include_str!(
        "../../fixtures/conformance/local-runtime/2026-04/orders/order-edit-residual-local-staging.json"
    ))
    .unwrap();
    let delete_fixture: Value = serde_json::from_str(include_str!(
        "../../fixtures/conformance/local-runtime/2026-04/orders/orderDelete-cascade-and-deletability.json"
    ))
    .unwrap();

    let mut proxy = snapshot_proxy();
    let residual_count = proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/orders/order-edit-residual-local-staging-read.graphql"
        ),
        json!({}),
    ));
    assert_eq!(
        residual_count.body["data"]["ordersCount"],
        residual_fixture["expected"]["emptyOrdersCount"]
    );

    let unknown_delete = proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/orders/orderDelete-cascade-and-deletability.graphql"
        ),
        delete_fixture["requests"]["unknownOrderDelete"]["variables"].clone(),
    ));
    assert_eq!(
        unknown_delete.body["data"],
        delete_fixture["expected"]["unknownOrderDelete"]["data"]
    );
}

#[test]
fn order_delete_stages_local_tombstone_cascade_and_not_found_errors() {
    let mut proxy = snapshot_proxy();
    let create_query = r#"
        mutation CreateDeletableOrder($order: OrderCreateOrderInput!) {
          orderCreate(order: $order) {
            order {
              id
              email
              displayFinancialStatus
              displayFulfillmentStatus
            }
            userErrors { field message code }
          }
        }
    "#;
    let delete_query = r#"
        mutation DeleteOrder($orderId: ID!) {
          orderDelete(orderId: $orderId) {
            deletedId
            userErrors { field message code }
          }
        }
    "#;
    let read_query = r#"
        query ReadDeletedOrder($id: ID!) {
          order(id: $id) { id email }
          orders(first: 5) { nodes { id email } }
          ordersCount { count precision }
        }
    "#;

    let create = proxy.process_request(json_graphql_request(
        create_query,
        json!({
            "order": {
                "email": "order-delete-success@example.com",
                "currency": "USD",
                "financialStatus": "PENDING",
                "lineItems": [{
                    "title": "Order delete success",
                    "quantity": 1,
                    "priceSet": { "shopMoney": { "amount": "10.00", "currencyCode": "USD" } }
                }]
            }
        }),
    ));
    assert_eq!(create.body["data"]["orderCreate"]["userErrors"], json!([]));
    let order_id = create.body["data"]["orderCreate"]["order"]["id"].clone();

    let delete = proxy.process_request(json_graphql_request(
        delete_query,
        json!({ "orderId": order_id.clone() }),
    ));
    assert_eq!(
        delete.body["data"]["orderDelete"],
        json!({ "deletedId": order_id, "userErrors": [] })
    );

    let read = proxy.process_request(json_graphql_request(
        read_query,
        json!({ "id": delete.body["data"]["orderDelete"]["deletedId"].clone() }),
    ));
    assert_eq!(read.body["data"]["order"], Value::Null);
    assert_eq!(read.body["data"]["orders"]["nodes"], json!([]));
    assert_eq!(
        read.body["data"]["ordersCount"],
        json!({ "count": 0, "precision": "EXACT" })
    );

    let log = log_snapshot(&proxy);
    let entries = log["entries"].as_array().unwrap();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0]["operationName"], json!("orderCreate"));
    assert_eq!(entries[1]["operationName"], json!("orderDelete"));
    assert_eq!(entries[1]["status"], json!("staged"));
    assert!(entries[1]["rawBody"]
        .as_str()
        .is_some_and(|body| body.contains("DeleteOrder")));

    let repeat = proxy.process_request(json_graphql_request(
        delete_query,
        json!({ "orderId": delete.body["data"]["orderDelete"]["deletedId"].clone() }),
    ));
    assert_eq!(
        repeat.body["data"]["orderDelete"]["userErrors"],
        json!([{ "field": ["orderId"], "message": "Order does not exist", "code": "NOT_FOUND" }])
    );
    assert_eq!(log_snapshot(&proxy)["entries"].as_array().unwrap().len(), 2);

    let paid = proxy.process_request(json_graphql_request(
        create_query,
        json!({
            "order": {
                "email": "order-delete-paid@example.com",
                "currency": "USD",
                "financialStatus": "PAID",
                "lineItems": [{
                    "title": "Order delete paid",
                    "quantity": 1,
                    "priceSet": { "shopMoney": { "amount": "12.00", "currencyCode": "USD" } }
                }]
            }
        }),
    ));
    let paid_id = paid.body["data"]["orderCreate"]["order"]["id"].clone();
    let paid_delete = proxy.process_request(json_graphql_request(
        delete_query,
        json!({ "orderId": paid_id.clone() }),
    ));
    assert_eq!(
        paid_delete.body["data"]["orderDelete"],
        json!({ "deletedId": paid_id, "userErrors": [] })
    );
    let paid_read = proxy.process_request(json_graphql_request(
        read_query,
        json!({ "id": paid_delete.body["data"]["orderDelete"]["deletedId"].clone() }),
    ));
    assert_eq!(paid_read.body["data"]["order"], Value::Null);

    let fulfilled = proxy.process_request(json_graphql_request(
        create_query,
        json!({
            "order": {
                "email": "order-delete-fulfilled@example.com",
                "currency": "USD",
                "financialStatus": "PENDING",
                "fulfillmentStatus": "FULFILLED",
                "lineItems": [{
                    "title": "Order delete fulfilled",
                    "quantity": 1,
                    "priceSet": { "shopMoney": { "amount": "14.00", "currencyCode": "USD" } }
                }]
            }
        }),
    ));
    let fulfilled_id = fulfilled.body["data"]["orderCreate"]["order"]["id"].clone();
    let fulfilled_delete = proxy.process_request(json_graphql_request(
        delete_query,
        json!({ "orderId": fulfilled_id.clone() }),
    ));
    assert_eq!(
        fulfilled_delete.body["data"]["orderDelete"],
        json!({ "deletedId": fulfilled_id, "userErrors": [] })
    );
    let fulfilled_read = proxy.process_request(json_graphql_request(
        read_query,
        json!({ "id": fulfilled_delete.body["data"]["orderDelete"]["deletedId"].clone() }),
    ));
    assert_eq!(fulfilled_read.body["data"]["order"], Value::Null);

    let unknown = proxy.process_request(json_graphql_request(
        delete_query,
        json!({ "orderId": "gid://shopify/Order/order-delete-missing" }),
    ));
    assert_eq!(
        unknown.body["data"]["orderDelete"],
        json!({
            "deletedId": Value::Null,
            "userErrors": [{
                "field": ["orderId"],
                "message": "Order does not exist",
                "code": "NOT_FOUND"
            }]
        })
    );
}

#[test]
fn order_edit_lifecycle_user_errors_match_captured_missing_resource_shapes() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/orders/order-edit-lifecycle-user-errors.json"
    ))
    .unwrap();

    for (index, document) in [
        include_str!("../../config/parity-requests/orders/orderEdit-lifecycle-userErrors-begin.graphql"),
        include_str!("../../config/parity-requests/orders/orderEdit-lifecycle-userErrors-addVariant.graphql"),
        include_str!("../../config/parity-requests/orders/orderEdit-lifecycle-userErrors-setQuantity.graphql"),
        include_str!("../../config/parity-requests/orders/orderEdit-lifecycle-userErrors-commit.graphql"),
    ]
    .iter()
    .enumerate()
    {
        let mut proxy = snapshot_proxy();
        let response = proxy.process_request(json_graphql_request(
            document,
            fixture["cases"][index]["variables"].clone(),
        ));
        assert_eq!(response.status, 200);
        assert_eq!(
            without_extensions(&response.body),
            without_extensions(&fixture["cases"][index]["response"]["payload"])
        );
    }
}

#[test]
fn order_edit_user_error_messages_match_shopify_i18n_strings() {
    let mut proxy = snapshot_proxy();
    let create_document = r#"
        mutation CreateOrderEditMessageOrder($order: OrderCreateOrderInput!) {
          orderCreate(order: $order) {
            order { id lineItems(first: 1) { nodes { id } } }
            userErrors { field message code }
          }
        }
    "#;
    let begin_document = r#"
        mutation BeginOrderEditForMessageCheck($id: ID!) {
          orderEditBegin(id: $id) {
            calculatedOrder { id lineItems(first: 1) { nodes { id } } }
            userErrors { field message }
          }
        }
    "#;

    let editable_create = proxy.process_request(json_graphql_request(
        create_document,
        json!({
            "order": {
                "email": "order-edit-message@example.test",
                "currency": "USD",
                "lineItems": [{
                    "title": "Order edit message line",
                    "quantity": 1,
                    "priceSet": { "shopMoney": { "amount": "10.00", "currencyCode": "USD" } }
                }]
            }
        }),
    ));
    assert_eq!(
        editable_create.body["data"]["orderCreate"]["userErrors"],
        json!([])
    );
    let editable_order_id = editable_create.body["data"]["orderCreate"]["order"]["id"].clone();
    let begin = proxy.process_request(json_graphql_request(
        begin_document,
        json!({ "id": editable_order_id }),
    ));
    assert_eq!(
        begin.body["data"]["orderEditBegin"]["userErrors"],
        json!([])
    );
    let calculated_order_id = begin.body["data"]["orderEditBegin"]["calculatedOrder"]["id"].clone();

    let set_quantity_unknown_line = proxy.process_request(json_graphql_request(
        r#"
        mutation SetQuantityUnknownLineMessage($id: ID!, $lineItemId: ID!, $quantity: Int!) {
          orderEditSetQuantity(id: $id, lineItemId: $lineItemId, quantity: $quantity) {
            calculatedLineItem { id }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "id": calculated_order_id.clone(),
            "lineItemId": "gid://shopify/CalculatedLineItem/999999999999",
            "quantity": 1
        }),
    ));
    assert_eq!(
        set_quantity_unknown_line.body["data"]["orderEditSetQuantity"]["userErrors"],
        json!([{
            "field": ["lineItemId"],
            "message": "The line item does not exist on the order."
        }])
    );

    let add_line_discount_unknown_line = proxy.process_request(json_graphql_request(
        r#"
        mutation AddLineDiscountUnknownLineMessage($id: ID!, $lineItemId: ID!, $discount: OrderEditAppliedDiscountInput!) {
          orderEditAddLineItemDiscount(id: $id, lineItemId: $lineItemId, discount: $discount) {
            addedDiscountStagedChange { id }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "id": calculated_order_id.clone(),
            "lineItemId": "gid://shopify/CalculatedLineItem/999999999999",
            "discount": {
                "description": "Unknown line discount",
                "fixedValue": { "amount": "1.00", "currencyCode": "USD" }
            }
        }),
    ));
    assert_eq!(
        add_line_discount_unknown_line.body["data"]["orderEditAddLineItemDiscount"]["userErrors"],
        json!([{
            "field": ["id"],
            "message": "The line item does not exist on the order."
        }])
    );

    let add_unknown_variant = proxy.process_request(json_graphql_request(
        r#"
        mutation AddUnknownVariantMessage($id: ID!, $variantId: ID!, $quantity: Int!) {
          orderEditAddVariant(id: $id, variantId: $variantId, quantity: $quantity) {
            calculatedLineItem { id }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "id": calculated_order_id,
            "variantId": "gid://shopify/ProductVariant/999999999999",
            "quantity": 1
        }),
    ));
    assert_eq!(
        add_unknown_variant.body["data"]["orderEditAddVariant"]["userErrors"],
        json!([{
            "field": ["variantId"],
            "message": "The variant does not exist in the shop."
        }])
    );

    let cancelled_create = proxy.process_request(json_graphql_request(
        create_document,
        json!({
            "order": {
                "email": "order-edit-cancelled-message@example.test",
                "currency": "USD",
                "lineItems": [{
                    "title": "Cancelled order edit message line",
                    "quantity": 1,
                    "priceSet": { "shopMoney": { "amount": "12.00", "currencyCode": "USD" } }
                }]
            }
        }),
    ));
    let cancelled_order_id = cancelled_create.body["data"]["orderCreate"]["order"]["id"].clone();
    let cancel = proxy.process_request(json_graphql_request(
        r#"
        mutation CancelOrderBeforeEditMessage($orderId: ID!, $reason: OrderCancelReason!, $restock: Boolean!) {
          orderCancel(orderId: $orderId, reason: $reason, restock: $restock) {
            userErrors { field message  }
          }
        }
        "#,
        json!({ "orderId": cancelled_order_id.clone(), "reason": "OTHER", "restock": false }),
    ));
    assert_eq!(cancel.body["data"]["orderCancel"]["userErrors"], json!([]));

    let begin_cancelled = proxy.process_request(json_graphql_request(
        begin_document,
        json!({ "id": cancelled_order_id }),
    ));
    assert_eq!(
        begin_cancelled.body["data"]["orderEditBegin"]["userErrors"],
        json!([{
            "field": Value::Null,
            "message": "The order cannot be edited."
        }])
    );
}

#[test]
fn order_edit_commit_success_messages_reflect_notify_customer_and_balance() {
    fn commit_messages(order_input: Value, notify_customer: Option<bool>) -> Value {
        let mut proxy = snapshot_proxy();
        let create = proxy.process_request(json_graphql_request(
            r#"
            mutation CreateOrderEditSuccessMessageOrder($order: OrderCreateOrderInput!) {
              orderCreate(order: $order) {
                order {
                  id
                  totalOutstandingSet {
                    shopMoney { amount currencyCode }
                  }
                }
                userErrors { field message code }
              }
            }
            "#,
            json!({ "order": order_input }),
        ));
        assert_eq!(create.status, 200);
        assert_eq!(create.body["data"]["orderCreate"]["userErrors"], json!([]));
        let order_id = create.body["data"]["orderCreate"]["order"]["id"].clone();

        let begin = proxy.process_request(json_graphql_request(
            r#"
            mutation BeginOrderEditForSuccessMessages($id: ID!) {
              orderEditBegin(id: $id) {
                calculatedOrder { id }
                userErrors { field message  }
              }
            }
            "#,
            json!({ "id": order_id }),
        ));
        assert_eq!(
            begin.body["data"]["orderEditBegin"]["userErrors"],
            json!([])
        );
        let calculated_order_id =
            begin.body["data"]["orderEditBegin"]["calculatedOrder"]["id"].clone();

        let commit = match notify_customer {
            Some(notify_customer) => proxy.process_request(json_graphql_request(
                r#"
                mutation CommitOrderEditForSuccessMessages($id: ID!, $notifyCustomer: Boolean!) {
                  orderEditCommit(id: $id, notifyCustomer: $notifyCustomer) {
                    order {
                      id
                      totalOutstandingSet {
                        shopMoney { amount currencyCode }
                      }
                    }
                    successMessages
                    userErrors { field message }
                  }
                }
                "#,
                json!({ "id": calculated_order_id, "notifyCustomer": notify_customer }),
            )),
            None => proxy.process_request(json_graphql_request(
                r#"
                mutation CommitOrderEditForSuccessMessages($id: ID!) {
                  orderEditCommit(id: $id) {
                    order {
                      id
                      totalOutstandingSet {
                        shopMoney { amount currencyCode }
                      }
                    }
                    successMessages
                    userErrors { field message }
                  }
                }
                "#,
                json!({ "id": calculated_order_id }),
            )),
        };
        assert_eq!(
            commit.body["data"]["orderEditCommit"]["userErrors"],
            json!([])
        );
        commit.body["data"]["orderEditCommit"]["successMessages"].clone()
    }

    let paid_order = json!({
        "email": "order-edit-paid-message@example.test",
        "currency": "USD",
        "lineItems": [{
            "title": "Paid edit message line",
            "quantity": 1,
            "priceSet": { "shopMoney": { "amount": "10.00", "currencyCode": "USD" } }
        }],
        "transactions": [{
            "kind": "SALE",
            "status": "SUCCESS",
            "gateway": "manual",
            "amountSet": { "shopMoney": { "amount": "10.00", "currencyCode": "USD" } }
        }]
    });
    assert_eq!(
        commit_messages(paid_order.clone(), None),
        json!(["Order updated"])
    );
    assert_eq!(
        commit_messages(paid_order, Some(false)),
        json!(["Order updated"])
    );

    let fully_paid_notify_order = json!({
        "email": "order-edit-notification-message@example.test",
        "currency": "USD",
        "lineItems": [{
            "title": "Notification edit message line",
            "quantity": 1,
            "priceSet": { "shopMoney": { "amount": "12.00", "currencyCode": "USD" } }
        }],
        "transactions": [{
            "kind": "SALE",
            "status": "SUCCESS",
            "gateway": "manual",
            "amountSet": { "shopMoney": { "amount": "12.00", "currencyCode": "USD" } }
        }]
    });
    assert_eq!(
        commit_messages(fully_paid_notify_order, Some(true)),
        json!(["Order updated", "Notification sent"])
    );

    let balance_due_notify_order = json!({
        "email": "order-edit-invoice-message@example.test",
        "currency": "USD",
        "financialStatus": "PENDING",
        "lineItems": [{
            "title": "Invoice edit message line",
            "quantity": 1,
            "priceSet": { "shopMoney": { "amount": "14.00", "currencyCode": "USD" } }
        }]
    });
    assert_eq!(
        commit_messages(balance_due_notify_order, Some(true)),
        json!(["Order updated", "Invoice sent"])
    );
}

#[test]
fn order_edit_commit_success_messages_include_unarchived_before_notify_message() {
    let upstream_calls = Arc::new(AtomicUsize::new(0));
    let upstream_calls_for_transport = Arc::clone(&upstream_calls);
    let mut proxy = configured_proxy(
        ReadMode::LiveHybrid,
        Some(shopify_draft_proxy::proxy::UnsupportedMutationMode::Passthrough),
    )
    .with_upstream_transport(move |_request| {
        upstream_calls_for_transport.fetch_add(1, Ordering::SeqCst);
        Response {
            status: 599,
            headers: Default::default(),
            body: json!({ "errors": [{ "message": "unexpected upstream call" }] }),
        }
    });
    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateClosedOrderEditSuccessMessageOrder($order: OrderCreateOrderInput!) {
          orderCreate(order: $order) {
            order { id closed closedAt }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "order": {
                "email": "order-edit-unarchive-message@example.test",
                "currency": "USD",
                "lineItems": [{
                    "title": "Unarchive edit message line",
                    "quantity": 1,
                    "priceSet": { "shopMoney": { "amount": "16.00", "currencyCode": "USD" } }
                }],
                "transactions": [{
                    "kind": "SALE",
                    "status": "SUCCESS",
                    "gateway": "manual",
                    "amountSet": { "shopMoney": { "amount": "16.00", "currencyCode": "USD" } }
                }]
            }
        }),
    ));
    assert_eq!(create.body["data"]["orderCreate"]["userErrors"], json!([]));
    let order_id = create.body["data"]["orderCreate"]["order"]["id"].clone();

    let close = proxy.process_request(json_graphql_request(
        r#"
        mutation CloseOrderBeforeEditCommit($id: ID!) {
          orderClose(input: { id: $id }) {
            order { id closed closedAt }
            userErrors { field message }
          }
        }
        "#,
        json!({ "id": order_id.clone() }),
    ));
    assert_eq!(close.body["data"]["orderClose"]["userErrors"], json!([]));
    assert_eq!(
        close.body["data"]["orderClose"]["order"]["closed"],
        json!(true)
    );

    let begin = proxy.process_request(json_graphql_request(
        r#"
        mutation BeginClosedOrderEditForSuccessMessages($id: ID!) {
          orderEditBegin(id: $id) {
            calculatedOrder { id }
            userErrors { field message  }
          }
        }
        "#,
        json!({ "id": order_id }),
    ));
    assert_eq!(
        begin.body["data"]["orderEditBegin"]["userErrors"],
        json!([])
    );
    let calculated_order_id = begin.body["data"]["orderEditBegin"]["calculatedOrder"]["id"].clone();

    let commit = proxy.process_request(json_graphql_request(
        r#"
        mutation CommitClosedOrderEditForSuccessMessages($id: ID!) {
          orderEditCommit(id: $id, notifyCustomer: true) {
            order { id closed closedAt }
            successMessages
            userErrors { field message }
          }
        }
        "#,
        json!({ "id": calculated_order_id }),
    ));
    assert_eq!(
        commit.body["data"]["orderEditCommit"]["userErrors"],
        json!([])
    );
    assert_eq!(
        commit.body["data"]["orderEditCommit"]["order"]["closed"],
        json!(false)
    );
    assert_eq!(
        commit.body["data"]["orderEditCommit"]["order"]["closedAt"],
        Value::Null
    );
    assert_eq!(
        commit.body["data"]["orderEditCommit"]["successMessages"],
        json!(["Order updated", "Order unarchived", "Notification sent"])
    );
    assert_eq!(upstream_calls.load(Ordering::SeqCst), 0);

    let log = log_snapshot(&proxy);
    let entries = log["entries"].as_array().expect("log entries");
    let commit_entry = entries
        .iter()
        .find(|entry| entry["interpreted"]["primaryRootField"] == "orderEditCommit")
        .expect("commit log entry");
    assert_eq!(commit_entry["status"], json!("staged"));
    assert_eq!(
        commit_entry["stagedResourceIds"],
        json!([commit.body["data"]["orderEditCommit"]["order"]["id"].clone()])
    );
    assert!(commit_entry["rawBody"]
        .as_str()
        .unwrap_or_default()
        .contains("CommitClosedOrderEditForSuccessMessages"));
}

#[test]
fn order_edit_existing_fixed_id_unstaged_calculated_order_returns_user_error() {
    let mut proxy = snapshot_proxy();
    // `orderEditAddVariant`/`orderEditSetQuantity` are gated local-support roots;
    // with no `orderEditBegin` session staged they resolve to a 200 response whose
    // payload carries the "calculated order does not exist" user error (the same
    // shape `order_edit_lifecycle_user_errors_match_captured_missing_resource_shapes`
    // asserts), not a 400 unimplemented-dispatcher error.
    let add = proxy.process_request(json_graphql_request(
        include_str!("../../config/parity-requests/orders/orderEditExistingWorkflow-addVariant.graphql"),
        json!({"id": "gid://shopify/CalculatedOrder/1", "variantId": "gid://shopify/ProductVariant/46789254021353", "quantity": 1, "locationId": "gid://shopify/Location/68509171945", "allowDuplicates": false}),
    ));
    assert_eq!(add.status, 200);
    assert_eq!(
        add.body["data"]["orderEditAddVariant"]["userErrors"][0]["message"],
        json!("The calculated order does not exist.")
    );

    let set_zero = proxy.process_request(json_graphql_request(
        include_str!("../../config/parity-requests/orders/orderEditExistingWorkflow-setQuantity.graphql"),
        json!({"id": "gid://shopify/CalculatedOrder/1", "lineItemId": "gid://shopify/LineItem/1", "quantity": 0, "restock": true}),
    ));
    assert_eq!(set_zero.status, 200);
    assert_eq!(
        set_zero.body["data"]["orderEditSetQuantity"]["userErrors"][0]["message"],
        json!("The calculated order does not exist.")
    );
}

#[test]
fn order_edit_existing_validation_unstaged_calculated_order_returns_user_error() {
    let mut proxy = snapshot_proxy();
    // Both the invalid-variant and duplicate-variant fixtures target a calculated
    // order id that was never staged via `orderEditBegin`, so the gated handler
    // short-circuits with the "calculated order does not exist" user error (200)
    // before any variant validation runs.
    let invalid_variant = proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/orders/orderEditExistingWorkflow-addVariant.graphql"
        ),
        json!({
            "id": "gid://shopify/CalculatedOrder/221172138217",
            "variantId": "gid://shopify/ProductVariant/0",
            "quantity": 1,
            "locationId": "gid://shopify/Location/68509171945",
            "allowDuplicates": false
        }),
    ));
    assert_eq!(invalid_variant.status, 200);
    assert_eq!(
        invalid_variant.body["data"]["orderEditAddVariant"]["userErrors"][0]["message"],
        json!("The calculated order does not exist.")
    );

    let duplicate_variant = proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/orders/orderEditExistingWorkflow-addVariant.graphql"
        ),
        json!({
            "id": "gid://shopify/CalculatedOrder/221172138217",
            "variantId": "gid://shopify/ProductVariant/48540157378793",
            "quantity": 1,
            "locationId": "gid://shopify/Location/68509171945",
            "allowDuplicates": false
        }),
    ));
    assert_eq!(duplicate_variant.status, 200);
    assert_eq!(
        duplicate_variant.body["data"]["orderEditAddVariant"]["userErrors"][0]["message"],
        json!("The calculated order does not exist.")
    );
}

#[test]
fn order_edit_shipping_line_and_remove_discount_unstaged_calculated_order_returns_invalid_code() {
    let cases = [
        (
            "orderEditAddShippingLine",
            r#"
            mutation UnknownOrderEditAddShippingLine($id: ID!, $shippingLine: OrderEditAddShippingLineInput!) {
              orderEditAddShippingLine(id: $id, shippingLine: $shippingLine) {
                calculatedOrder { id }
                userErrors { field message code }
              }
            }
            "#,
            json!({
                "id": "gid://shopify/CalculatedOrder/999999",
                "shippingLine": {
                    "title": "Unknown calculated order shipping",
                    "price": { "amount": "9.99", "currencyCode": "CAD" }
                }
            }),
        ),
        (
            "orderEditUpdateShippingLine",
            r#"
            mutation UnknownOrderEditUpdateShippingLine(
              $id: ID!
              $shippingLineId: ID!
              $shippingLine: OrderEditUpdateShippingLineInput!
            ) {
              orderEditUpdateShippingLine(id: $id, shippingLineId: $shippingLineId, shippingLine: $shippingLine) {
                calculatedOrder { id }
                userErrors { field message code }
              }
            }
            "#,
            json!({
                "id": "gid://shopify/CalculatedOrder/999999",
                "shippingLineId": "gid://shopify/CalculatedShippingLine/1",
                "shippingLine": {
                    "title": "Updated unknown calculated order shipping",
                    "price": { "amount": "19.99", "currencyCode": "CAD" }
                }
            }),
        ),
        (
            "orderEditRemoveShippingLine",
            r#"
            mutation UnknownOrderEditRemoveShippingLine($id: ID!, $shippingLineId: ID!) {
              orderEditRemoveShippingLine(id: $id, shippingLineId: $shippingLineId) {
                calculatedOrder { id }
                userErrors { field message code }
              }
            }
            "#,
            json!({
                "id": "gid://shopify/CalculatedOrder/999999",
                "shippingLineId": "gid://shopify/CalculatedShippingLine/1"
            }),
        ),
        (
            "orderEditRemoveDiscount",
            r#"
            mutation UnknownOrderEditRemoveDiscount($id: ID!, $discountApplicationId: ID!) {
              orderEditRemoveDiscount(id: $id, discountApplicationId: $discountApplicationId) {
                calculatedOrder { id }
                userErrors { field message code }
              }
            }
            "#,
            json!({
                "id": "gid://shopify/CalculatedOrder/999999",
                "discountApplicationId": "gid://shopify/DiscountApplication/1"
            }),
        ),
    ];

    for (root, document, variables) in cases {
        let mut proxy = snapshot_proxy();
        let response = proxy.process_request(json_graphql_request(document, variables));
        assert_eq!(response.status, 200, "{root} should stay locally handled");
        assert_eq!(response.body["data"][root]["calculatedOrder"], Value::Null);
        assert_eq!(
            response.body["data"][root]["userErrors"],
            json!([{
                "field": ["id"],
                "message": "The calculated order does not exist.",
                "code": "INVALID"
            }]),
            "{root} should include the typed user error code"
        );
    }
}

#[test]
fn customer_payment_methods_remote_create_validation_covers_current_guardrails() {
    let mut proxy = snapshot_proxy();

    let seed_query = omit_user_error_code_selection(include_str!(
        "../../config/parity-requests/payments/customer-payment-method-remote-create-validation-seed.graphql"
    ));
    let seed = proxy.process_request(json_graphql_request(&seed_query, json!({})));
    assert_eq!(seed.body["data"]["customerCreate"]["userErrors"], json!([]));
    assert!(seed.body["data"]["customerCreate"]["customer"]["id"]
        .as_str()
        .is_some_and(|id| id.starts_with("gid://shopify/Customer/1")));

    let stripe_blank = proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/payments/customer-payment-method-remote-create-validation-stripe.graphql"
        ),
        json!({}),
    ));
    assert_eq!(stripe_blank.status, 200);
    assert_eq!(
        stripe_blank.body,
        json!({
            "errors": [{
                "message": "Argument 'customerId' on InputObject 'RemoteStripePaymentMethodInput' has an invalid value (null). Expected type 'String!'.",
                "locations": [{ "line": 4, "column": 22 }],
                "path": [
                    "mutation CustomerPaymentMethodRemoteCreateStripeBlank",
                    "customerPaymentMethodRemoteCreate",
                    "remoteReference",
                    "stripePaymentMethod",
                    "customerId"
                ],
                "extensions": {
                    "code": "argumentLiteralsIncompatible",
                    "typeName": "InputObject",
                    "argumentName": "customerId"
                }
            }]
        })
    );

    let stripe_empty = proxy.process_request(json_graphql_request(
        r#"
        mutation CustomerPaymentMethodRemoteCreateStripeEmpty {
          customerPaymentMethodRemoteCreate(
            customerId: "gid://shopify/Customer/1"
            remoteReference: { stripePaymentMethod: { customerId: "", paymentMethodId: "pm_x" } }
          ) {
            customerPaymentMethod { id }
            userErrors { field code message }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(stripe_empty.status, 200);
    assert_eq!(
        stripe_empty.body,
        json!({
            "data": {
                "customerPaymentMethodRemoteCreate": {
                    "customerPaymentMethod": Value::Null,
                    "userErrors": [{
                        "field": ["remote_reference", "stripe_payment_method", "customer_id"],
                        "code": "INVALID",
                        "message": "customer_id can't be blank"
                    }]
                }
            }
        })
    );
}

#[test]
fn customer_payment_methods_remote_create_rejects_blank_gateway_required_fields() {
    let cases = [
        (
            "braintree blank customer_id",
            r#"braintreePaymentMethod: { customerId: "", paymentMethodToken: "tok_x" }"#,
            [
                "remote_reference",
                "braintree_payment_method",
                "customer_id",
            ],
            "INVALID",
            "customer_id can't be blank",
        ),
        (
            "braintree blank payment_method_token",
            r#"braintreePaymentMethod: { customerId: "cus_x", paymentMethodToken: "" }"#,
            [
                "remote_reference",
                "braintree_payment_method",
                "payment_method_token",
            ],
            "INVALID",
            "payment_method_token can't be blank",
        ),
        (
            "authorize.net blank customer_profile_id",
            r#"authorizeNetCustomerPaymentProfile: { customerProfileId: "", customerPaymentProfileId: "pay_x" }"#,
            [
                "remote_reference",
                "authorize_net_customer_payment_profile",
                "customer_profile_id",
            ],
            "INVALID",
            "customer_profile_id can't be blank",
        ),
    ];

    for (name, remote_reference, field_path, code, message) in cases {
        let mut proxy = snapshot_proxy();
        let query = format!(
            r#"
            mutation CustomerPaymentMethodRemoteCreateBlankGatewayField {{
              customerPaymentMethodRemoteCreate(
                customerId: "gid://shopify/Customer/1"
                remoteReference: {{ {remote_reference} }}
              ) {{
                customerPaymentMethod {{ id }}
                userErrors {{ field code message }}
              }}
            }}
        "#
        );

        let response = proxy.process_request(json_graphql_request(&query, json!({})));

        assert_eq!(response.status, 200, "{name}");
        let payload = &response.body["data"]["customerPaymentMethodRemoteCreate"];
        assert_eq!(payload["customerPaymentMethod"], Value::Null, "{name}");
        assert_eq!(
            payload["userErrors"],
            json!([{
                "field": field_path,
                "code": code,
                "message": message
            }]),
            "{name}"
        );
    }
}

#[test]
fn customer_payment_methods_remote_create_counts_all_gateway_objects_for_cardinality() {
    let mut proxy = snapshot_proxy();
    let response = proxy.process_request(json_graphql_request(
        r#"
        mutation CustomerPaymentMethodRemoteCreateTwoNonStripeGatewayObjects {
          customerPaymentMethodRemoteCreate(
            customerId: "gid://shopify/Customer/1"
            remoteReference: {
              braintreePaymentMethod: { customerId: "cus_x", paymentMethodToken: "tok_x" }
              authorizeNetCustomerPaymentProfile: { customerProfileId: "profile_x", customerPaymentProfileId: "pay_x" }
            }
          ) {
            customerPaymentMethod { id }
            userErrors { field code message }
          }
        }
        "#,
        json!({}),
    ));

    assert_eq!(response.status, 200);
    let payload = &response.body["data"]["customerPaymentMethodRemoteCreate"];
    assert_eq!(payload["customerPaymentMethod"], Value::Null);
    assert_eq!(
        payload["userErrors"],
        json!([{
            "field": ["remote_reference"],
            "code": "INVALID",
            "message": "Remote reference must contain exactly one payment method."
        }])
    );
}

#[test]
fn customer_payment_methods_replay_shop_pay_guard_shapes() {
    let mut proxy = snapshot_proxy();

    let response = proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/payments/customer-payment-method-shop-pay-guards.graphql"
        ),
        json!({
            "targetCustomerId": "gid://shopify/Customer/8802",
            "blankBillingAddress": {},
            "encryptedDuplicationData": "shopify-draft-proxy:customer-payment-method-duplication:not-used-before-billing-address-validation"
        }),
    ));

    assert_eq!(response.status, 200);
    assert_eq!(
        response.body,
        json!({
            "data": {
                "creditCardDuplication": {
                    "encryptedDuplicationData": Value::Null,
                    "userErrors": [{
                        "field": ["customerPaymentMethodId"],
                        "message": "Invalid instrument",
                        "code": "INVALID_INSTRUMENT"
                    }]
                },
                "sameShopDuplication": {
                    "encryptedDuplicationData": Value::Null,
                    "userErrors": [{
                        "field": ["targetShopId"],
                        "message": "Target shop is not eligible for payment method duplication",
                        "code": "SAME_SHOP"
                    }]
                },
                "creditCardUpdateUrl": {
                    "updatePaymentMethodUrl": Value::Null,
                    "userErrors": [{
                        "field": ["customerPaymentMethodId"],
                        "message": "Invalid instrument",
                        "code": "INVALID_INSTRUMENT"
                    }]
                },
                "blankBillingAddressCreate": {
                    "customerPaymentMethod": Value::Null,
                    "userErrors": [
                        { "field": ["billing_address", "address1"], "message": "can't be blank", "code": Value::Null },
                        { "field": ["billing_address", "city"], "message": "can't be blank", "code": Value::Null },
                        { "field": ["billing_address", "zip"], "message": "can't be blank", "code": Value::Null },
                        { "field": ["billing_address", "country_code"], "message": "can't be blank", "code": Value::Null },
                        { "field": ["billing_address", "province_code"], "message": "can't be blank", "code": Value::Null }
                    ]
                }
            }
        })
    );
}

#[test]
fn customer_payment_methods_replay_local_staging_and_validation_shapes() {
    let mut proxy = snapshot_proxy();
    let billing_address = json!({
        "firstName": "Sensitive",
        "lastName": "Billing",
        "address1": "1 Secret St",
        "city": "New York",
        "zip": "10001",
        "countryCode": "US",
        "provinceCode": "NY"
    });
    let (_reminder_order_id, payment_schedule_id) = stage_reminder_order_payment_schedule(
        &mut proxy,
        Some("customer-payment-method-reminder@example.test"),
    );

    let primary = proxy.process_request(json_graphql_request(
        &omit_unavailable_customer_card_digits(include_str!(
            "../../config/parity-requests/payments/customer-payment-method-local-staging.graphql"
        )),
        json!({
            "customerId": "gid://shopify/Customer/8801",
            "targetCustomerId": "gid://shopify/Customer/8802",
            "billingAddress": billing_address.clone(),
            "sessionId": "csn_sensitive_session",
            "remoteReference": {
                "stripePaymentMethod": {
                    "customerId": "cus_sensitive",
                    "paymentMethodId": "pm_sensitive"
                }
            },
            "paymentScheduleId": payment_schedule_id
        }),
    ));
    assert_eq!(primary.body["data"]["cardCreate"]["userErrors"], json!([]));
    assert_eq!(
        primary.body["data"]["cardCreate"]["customerPaymentMethod"]["id"],
        json!("gid://shopify/CustomerPaymentMethod/1")
    );
    assert_eq!(
        primary.body["data"]["remoteCreate"]["customerPaymentMethod"]["id"],
        json!("gid://shopify/CustomerPaymentMethod/2")
    );
    assert_eq!(
        primary.body["data"]["paypalCreate"]["customerPaymentMethod"]["id"],
        json!("gid://shopify/CustomerPaymentMethod/3")
    );
    assert_eq!(
        primary.body["data"]["reminder"],
        json!({ "success": true, "userErrors": [] })
    );

    let duplication = proxy.process_request(json_graphql_request(
        &omit_unavailable_customer_card_digits(include_str!("../../config/parity-requests/payments/customer-payment-method-duplication-local-staging.graphql")),
        json!({
            "customerId": "gid://shopify/Customer/8802",
            "billingAddress": billing_address.clone(),
            "encryptedDuplicationData": primary.body["data"]["duplication"]["encryptedDuplicationData"].clone()
        }),
    ));
    assert_eq!(
        duplication.body["data"]["customerPaymentMethodCreateFromDuplicationData"]["userErrors"],
        json!([])
    );
    assert_eq!(
        duplication.body["data"]["customerPaymentMethodCreateFromDuplicationData"]
            ["customerPaymentMethod"]["customer"]["id"],
        json!("gid://shopify/Customer/8802")
    );
    let duplicated_method_id = duplication.body["data"]
        ["customerPaymentMethodCreateFromDuplicationData"]["customerPaymentMethod"]["id"]
        .clone();
    assert!(duplicated_method_id
        .as_str()
        .is_some_and(|id| id.starts_with("gid://shopify/CustomerPaymentMethod/")));

    let lifecycle_read = proxy.process_request(json_graphql_request(
        &omit_unavailable_customer_card_digits(include_str!(
            "../../config/parity-requests/payments/customer-payment-method-local-staging-read.graphql"
        )),
        json!({
            "sourceCustomerId": "gid://shopify/Customer/8801",
            "targetCustomerId": "gid://shopify/Customer/8802"
        }),
    ));
    let source_nodes = lifecycle_read.body["data"]["source"]["paymentMethods"]["nodes"]
        .as_array()
        .expect("source payment methods should be an array");
    assert!(source_nodes
        .iter()
        .any(|node| node["id"] == json!("gid://shopify/CustomerPaymentMethod/1")));
    assert!(source_nodes
        .iter()
        .any(|node| node["id"] == json!("gid://shopify/CustomerPaymentMethod/2")));
    assert!(source_nodes
        .iter()
        .any(|node| node["id"] == json!("gid://shopify/CustomerPaymentMethod/3")));
    assert_eq!(
        lifecycle_read.body["data"]["shownRevoked"]["id"],
        json!("gid://shopify/CustomerPaymentMethod/base-card")
    );
    assert_eq!(
        lifecycle_read.body["data"]["target"]["paymentMethods"]["nodes"][0]["id"],
        duplicated_method_id
    );

    let blank = proxy.process_request(json_graphql_request(
        &omit_user_error_code_selection(include_str!("../../config/parity-requests/payments/customer-payment-method-credit-card-create-validation-blank.graphql")),
        json!({
            "customerId": "gid://shopify/Customer/8801",
            "sessionId": "sess_valid",
            "billingAddress": {
                "address1": Value::Null,
                "city": Value::Null,
                "zip": Value::Null,
                "country": Value::Null,
                "province": Value::Null
            }
        }),
    ));
    assert_eq!(
        blank.body["data"]["customerPaymentMethodCreditCardCreate"],
        json!({
            "customerPaymentMethod": Value::Null,
            "processing": false,
            "userErrors": [
                { "field": ["billing_address", "address1"], "message": "can't be blank" },
                { "field": ["billing_address", "city"], "message": "can't be blank" },
                { "field": ["billing_address", "zip"], "message": "can't be blank" },
                { "field": ["billing_address", "country_code"], "message": "can't be blank" },
                { "field": ["billing_address", "province_code"], "message": "can't be blank" }
            ]
        })
    );

    let missing_session = proxy.process_request(json_graphql_request(
        &omit_user_error_code_selection(include_str!("../../config/parity-requests/payments/customer-payment-method-credit-card-create-validation-missing-session.graphql")),
        json!({
            "customerId": "gid://shopify/Customer/8801",
            "billingAddress": {
                "address1": "1 Main St",
                "city": "New York",
                "zip": "10001",
                "country": "US",
                "province": "NY"
            }
        }),
    ));
    // Omitting the required `sessionId` argument is a schema-validation failure, so
    // Shopify returns a top-level `errors` array (missingRequiredArguments) with no
    // data — not a BLANK userError. Mirror the recorded shape exactly.
    assert_eq!(
        missing_session.body["errors"][0]["extensions"]["code"],
        json!("missingRequiredArguments")
    );
    assert!(missing_session.body.get("data").is_none());

    let processing = proxy.process_request(json_graphql_request(
        &omit_user_error_code_selection(include_str!("../../config/parity-requests/payments/customer-payment-method-credit-card-create-validation-processing.graphql")),
        json!({
            "customerId": "gid://shopify/Customer/8801",
            "sessionId": "shopify-draft-proxy:processing",
            "billingAddress": {
                "address1": "1 Main St",
                "city": "New York",
                "zip": "10001",
                "country": "US",
                "province": "NY"
            }
        }),
    ));
    assert_eq!(
        processing.body["data"]["customerPaymentMethodCreditCardCreate"],
        json!({
            "customerPaymentMethod": Value::Null,
            "processing": true,
            "userErrors": []
        })
    );

    let success = proxy.process_request(json_graphql_request(
        &omit_user_error_code_selection(include_str!("../../config/parity-requests/payments/customer-payment-method-credit-card-create-validation-success.graphql")).replace("countryCodeV2", "countryCode"),
        json!({
            "customerId": "gid://shopify/Customer/8801",
            "sessionId": "sess_valid",
            "billingAddress": {
                "firstName": "Ada",
                "lastName": "Lovelace",
                "address1": "1 Main St",
                "city": "New York",
                "zip": "10001",
                "country": "US",
                "province": "NY"
            }
        }),
    ));
    assert_eq!(
        success.body["data"]["customerPaymentMethodCreditCardCreate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        success.body["data"]["customerPaymentMethodCreditCardCreate"]["customerPaymentMethod"]
            ["instrument"]["billingAddress"],
        json!({
            "firstName": "Ada",
            "lastName": "Lovelace",
            "address1": "1 Main St",
            "city": "New York",
            "zip": "10001",
            "countryCode": "US",
            "provinceCode": "NY"
        })
    );
    let success_id = success.body["data"]["customerPaymentMethodCreditCardCreate"]
        ["customerPaymentMethod"]["id"]
        .clone();

    let read = proxy.process_request(json_graphql_request(
        &include_str!("../../config/parity-requests/payments/customer-payment-method-credit-card-create-validation-read.graphql").replace("countryCodeV2", "countryCode"),
        json!({ "id": success_id }),
    ));
    assert_eq!(
        read.body["data"]["customerPaymentMethod"]["instrument"]["billingAddress"],
        json!({
            "firstName": "Ada",
            "lastName": "Lovelace",
            "address1": "1 Main St",
            "city": "New York",
            "zip": "10001",
            "countryCode": "US",
            "provinceCode": "NY"
        })
    );
}

#[test]
fn customer_payment_methods_window_page_info_and_alias_show_revoked() {
    let mut proxy = snapshot_proxy();

    let first_page = proxy.process_request(json_graphql_request(
        r#"
        query CustomerPaymentMethodsFirstPage {
          customer(id: "gid://shopify/Customer/8801") {
            paymentMethods(first: 2, showRevoked: true) {
              nodes { id }
              pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
            }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(first_page.status, 200);
    let first_page_connection = &first_page.body["data"]["customer"]["paymentMethods"];
    assert_eq!(
        first_page_connection["nodes"],
        json!([
            { "id": "gid://shopify/CustomerPaymentMethod/base-card" },
            { "id": "gid://shopify/CustomerPaymentMethod/base-paypal" }
        ])
    );
    assert_eq!(
        first_page_connection["pageInfo"],
        json!({
            "hasNextPage": true,
            "hasPreviousPage": false,
            "startCursor": "gid://shopify/CustomerPaymentMethod/base-card",
            "endCursor": "gid://shopify/CustomerPaymentMethod/base-paypal"
        })
    );

    let second_page = proxy.process_request(json_graphql_request(
        r#"
        query CustomerPaymentMethodsSecondPage($after: String!) {
          customer(id: "gid://shopify/Customer/8801") {
            paymentMethods(first: 2, after: $after, showRevoked: true) {
              nodes { id }
              pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
            }
          }
        }
        "#,
        json!({ "after": first_page_connection["pageInfo"]["endCursor"].clone() }),
    ));
    assert_eq!(second_page.status, 200);
    assert_eq!(
        second_page.body["data"]["customer"]["paymentMethods"]["nodes"],
        json!([{ "id": "gid://shopify/CustomerPaymentMethod/base-shop-pay" }])
    );
    assert_eq!(
        second_page.body["data"]["customer"]["paymentMethods"]["pageInfo"],
        json!({
            "hasNextPage": false,
            "hasPreviousPage": true,
            "startCursor": "gid://shopify/CustomerPaymentMethod/base-shop-pay",
            "endCursor": "gid://shopify/CustomerPaymentMethod/base-shop-pay"
        })
    );

    let alias_read = proxy.process_request(json_graphql_request(
        r#"
        query CustomerPaymentMethodsAliasShowRevoked {
          customer(id: "gid://shopify/Customer/revoke-sentinel") {
            shown: paymentMethods(first: 2, showRevoked: true) {
              nodes { id revokedAt }
              pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
            }
            hidden: paymentMethods(first: 2) {
              nodes { id revokedAt }
              pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
            }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(alias_read.status, 200);
    assert_eq!(
        alias_read.body["data"]["customer"]["shown"]["nodes"],
        json!([
            { "id": "gid://shopify/CustomerPaymentMethod/active-contract", "revokedAt": Value::Null },
            { "id": "gid://shopify/CustomerPaymentMethod/already-revoked", "revokedAt": "2026-05-01T00:00:00.000Z" }
        ])
    );
    assert_eq!(
        alias_read.body["data"]["customer"]["shown"]["pageInfo"],
        json!({
            "hasNextPage": false,
            "hasPreviousPage": false,
            "startCursor": "gid://shopify/CustomerPaymentMethod/active-contract",
            "endCursor": "gid://shopify/CustomerPaymentMethod/already-revoked"
        })
    );
    assert_eq!(
        alias_read.body["data"]["customer"]["hidden"]["nodes"],
        json!([
            { "id": "gid://shopify/CustomerPaymentMethod/active-contract", "revokedAt": Value::Null }
        ])
    );
    assert_eq!(
        alias_read.body["data"]["customer"]["hidden"]["pageInfo"],
        json!({
            "hasNextPage": false,
            "hasPreviousPage": false,
            "startCursor": "gid://shopify/CustomerPaymentMethod/active-contract",
            "endCursor": "gid://shopify/CustomerPaymentMethod/active-contract"
        })
    );
}

#[test]
fn customer_payment_method_update_and_revoke_tail_helpers_cover_current_behavior() {
    let mut proxy = snapshot_proxy();

    let update = proxy.process_request(json_graphql_request(
        r#"
        mutation RustCustomerPaymentMethodCreditCardUpdateValidation {
          customerPaymentMethodCreditCardUpdate(
            id: "gid://shopify/CustomerPaymentMethod/base-card"
            sessionId: "sess_valid"
            billingAddress: { address1: null, city: null, zip: null, country: null, province: null }
          ) {
            customerPaymentMethod { id }
            processing
            userErrors { field  message }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(update.status, 200);
    assert_eq!(
        update.body["data"]["customerPaymentMethodCreditCardUpdate"],
        json!({
            "customerPaymentMethod": Value::Null,
            "processing": false,
            "userErrors": [
                { "field": ["billing_address", "address1"], "message": "can't be blank" },
                { "field": ["billing_address", "city"], "message": "can't be blank" },
                { "field": ["billing_address", "zip"], "message": "can't be blank" },
                { "field": ["billing_address", "country_code"], "message": "can't be blank" },
                { "field": ["billing_address", "province_code"], "message": "can't be blank" }
            ]
        })
    );

    let active_contract = proxy.process_request(json_graphql_request(
        r#"
        mutation RustCustomerPaymentMethodRevokeLocalRuntimeActive {
          customerPaymentMethodRevoke(customerPaymentMethodId: "gid://shopify/CustomerPaymentMethod/active-contract") {
            revokedCustomerPaymentMethodId
            userErrors { field message  }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        active_contract.body["data"]["customerPaymentMethodRevoke"],
        json!({
            "revokedCustomerPaymentMethodId": Value::Null,
            "userErrors": [{
                "field": ["customerPaymentMethodId"],
                "message": "Cannot revoke a payment method with active subscription contracts."
            }]
        })
    );

    let active_read = proxy.process_request(json_graphql_request(
        r#"
        query RustCustomerPaymentMethodRevokeLocalRuntimeActiveRead {
          customerPaymentMethod(id: "gid://shopify/CustomerPaymentMethod/active-contract", showRevoked: true) {
            id
            revokedAt
            revokedReason
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        active_read.body["data"]["customerPaymentMethod"],
        json!({
            "id": "gid://shopify/CustomerPaymentMethod/active-contract",
            "revokedAt": Value::Null,
            "revokedReason": Value::Null
        })
    );

    let success = proxy.process_request(json_graphql_request(
        r#"
        mutation RustCustomerPaymentMethodRevokeLocalRuntimeSuccess {
          customerPaymentMethodRevoke(customerPaymentMethodId: "gid://shopify/CustomerPaymentMethod/base-card") {
            revokedCustomerPaymentMethodId
            userErrors { field message  }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        success.body["data"]["customerPaymentMethodRevoke"],
        json!({
            "revokedCustomerPaymentMethodId": "gid://shopify/CustomerPaymentMethod/base-card",
            "userErrors": []
        })
    );

    let success_read = proxy.process_request(json_graphql_request(
        r#"
        query RustCustomerPaymentMethodRevokeLocalRuntimeSuccessRead {
          customerPaymentMethod(id: "gid://shopify/CustomerPaymentMethod/base-card", showRevoked: true) {
            id
            revokedAt
            revokedReason
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        success_read.body["data"]["customerPaymentMethod"],
        json!({
            "id": "gid://shopify/CustomerPaymentMethod/base-card",
            "revokedAt": "2024-01-01T00:00:02.000Z",
            "revokedReason": "MANUALLY_REVOKED"
        })
    );

    let already_revoked = proxy.process_request(json_graphql_request(
        r#"
        mutation RustCustomerPaymentMethodRevokeLocalRuntimeAlreadyRevoked {
          customerPaymentMethodRevoke(customerPaymentMethodId: "gid://shopify/CustomerPaymentMethod/already-revoked") {
            revokedCustomerPaymentMethodId
            userErrors { field message  }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        already_revoked.body["data"]["customerPaymentMethodRevoke"],
        json!({
            "revokedCustomerPaymentMethodId": "gid://shopify/CustomerPaymentMethod/already-revoked",
            "userErrors": []
        })
    );

    let already_revoked_read = proxy.process_request(json_graphql_request(
        r#"
        query RustCustomerPaymentMethodRevokeLocalRuntimeAlreadyRevokedRead {
          customerPaymentMethod(id: "gid://shopify/CustomerPaymentMethod/already-revoked", showRevoked: true) {
            id
            revokedAt
            revokedReason
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        already_revoked_read.body["data"]["customerPaymentMethod"],
        json!({
            "id": "gid://shopify/CustomerPaymentMethod/already-revoked",
            "revokedAt": "2026-05-01T00:00:00.000Z",
            "revokedReason": "MANUALLY_REVOKED"
        })
    );
}

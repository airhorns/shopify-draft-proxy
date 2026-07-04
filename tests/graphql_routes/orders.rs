use super::common::*;
use pretty_assertions::assert_eq;
use std::sync::atomic::{AtomicUsize, Ordering};

fn without_extensions(value: &Value) -> Value {
    let mut value = value.clone();
    if let Some(object) = value.as_object_mut() {
        object.remove("extensions");
    }
    value
}

fn assert_draft_order_variant_catalog_line(line: &Value, quantity: i64) {
    assert_eq!(line["title"], json!("Catalog product title"));
    assert_eq!(line["name"], json!("Catalog product title"));
    assert_eq!(line["sku"], json!("CATALOG-SKU"));
    assert_eq!(line["quantity"], json!(quantity));
    assert_eq!(line["custom"], json!(false));
    assert_eq!(line["requiresShipping"], json!(true));
    assert_eq!(line["taxable"], json!(true));
    assert_eq!(
        line["originalUnitPriceSet"]["shopMoney"],
        json!({ "amount": "19.95", "currencyCode": "USD" })
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

fn assert_draft_order_custom_line(line: &Value) {
    assert_eq!(line["title"], json!("Custom-only item"));
    assert_eq!(line["name"], json!("Custom-only item"));
    assert_eq!(line["sku"], json!("CUSTOM-SKU"));
    assert_eq!(line["quantity"], json!(1));
    assert_eq!(line["custom"], json!(true));
    assert_eq!(line["requiresShipping"], json!(false));
    assert_eq!(line["taxable"], json!(false));
    assert_eq!(
        line["originalUnitPriceSet"]["shopMoney"],
        json!({ "amount": "7.5", "currencyCode": "USD" })
    );
    assert_eq!(line["variant"], Value::Null);
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
                    "priceSet": { "shopMoney": { "amount": "10.00", "currencyCode": "USD" } }
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
fn order_refund_and_fulfillment_plain_user_errors_reject_code_selection() {
    let mut proxy = snapshot_proxy();

    let response = proxy.process_request(json_graphql_request(
        r#"
        mutation OrdersPlainUserErrorCodeSelectionRejected {
          refund: refundCreate(input: { orderId: "gid://shopify/Order/999999999" }) {
            userErrors { field message code }
          }
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
    assert_eq!(errors.len(), 7);
    for (error, response_key) in errors.iter().zip([
        "refund",
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
                "mutation OrdersPlainUserErrorCodeSelectionRejected",
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
                    nodes { id totalQuantity remainingQuantity returnLineItem { id } }
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
                    nodes { id totalQuantity remainingQuantity returnLineItem { id } }
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

fn read_return_line_customer_note(proxy: &mut DraftProxy, return_id: Value) -> Value {
    proxy
        .process_request(json_graphql_request(
            r#"
            query ReadReturnLineCustomerNote($id: ID!) {
              return(id: $id) {
                id
                returnLineItems(first: 5) {
                  nodes { id quantity customerNote }
                }
              }
            }
            "#,
            json!({ "id": return_id }),
        ))
        .body["data"]["return"]
        .clone()
}

#[test]
fn return_create_and_request_persist_line_item_customer_note_for_read_after_write() {
    let mut request_proxy = snapshot_proxy();
    let (request_order_id, request_fulfillment_line_item_id) =
        stage_fulfilled_order_for_return(&mut request_proxy);
    let request = request_proxy.process_request(json_graphql_request(
        r#"
        mutation RequestReturnWithCustomerNote($input: ReturnRequestInput!) {
          returnRequest(input: $input) {
            return {
              id
              status
              returnLineItems(first: 5) {
                nodes { id quantity customerNote }
              }
            }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "input": {
                "orderId": request_order_id,
                "returnLineItems": [{
                    "fulfillmentLineItemId": request_fulfillment_line_item_id,
                    "quantity": 1,
                    "returnReason": "DEFECTIVE",
                    "customerNote": "Screen arrived cracked"
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
    assert_eq!(requested_return["status"], json!("REQUESTED"));
    assert_eq!(
        requested_return["returnLineItems"]["nodes"][0]["customerNote"],
        json!("Screen arrived cracked")
    );

    let requested_read =
        read_return_line_customer_note(&mut request_proxy, requested_return["id"].clone());
    assert_eq!(
        requested_read["returnLineItems"]["nodes"][0]["customerNote"],
        json!("Screen arrived cracked")
    );

    let (omitted_order_id, omitted_fulfillment_line_item_id) =
        stage_fulfilled_order_for_return(&mut request_proxy);
    let omitted = request_proxy.process_request(json_graphql_request(
        r#"
        mutation RequestReturnWithoutCustomerNote($input: ReturnRequestInput!) {
          returnRequest(input: $input) {
            return {
              id
              returnLineItems(first: 5) {
                nodes { id quantity customerNote }
              }
            }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "input": {
                "orderId": omitted_order_id,
                "returnLineItems": [{
                    "fulfillmentLineItemId": omitted_fulfillment_line_item_id,
                    "quantity": 1,
                    "returnReason": "DEFECTIVE"
                }]
            }
        }),
    ));
    assert_eq!(omitted.status, 200);
    assert_eq!(
        omitted.body["data"]["returnRequest"]["userErrors"],
        json!([])
    );
    assert_eq!(
        omitted.body["data"]["returnRequest"]["return"]["returnLineItems"]["nodes"][0]
            ["customerNote"],
        Value::Null
    );

    let mut create_proxy = snapshot_proxy();
    let (create_order_id, create_fulfillment_line_item_id) =
        stage_fulfilled_order_for_return(&mut create_proxy);
    let create = create_proxy.process_request(json_graphql_request(
        r#"
        mutation CreateReturnWithCustomerNote($returnInput: ReturnInput!) {
          returnCreate(returnInput: $returnInput) {
            return {
              id
              status
              returnLineItems(first: 5) {
                nodes { id quantity customerNote }
              }
            }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "returnInput": {
                "orderId": create_order_id,
                "returnLineItems": [{
                    "fulfillmentLineItemId": create_fulfillment_line_item_id,
                    "quantity": 1,
                    "returnReason": "DEFECTIVE",
                    "customerNote": "Box was dented"
                }]
            }
        }),
    ));
    assert_eq!(create.status, 200);
    assert_eq!(create.body["data"]["returnCreate"]["userErrors"], json!([]));
    let created_return = &create.body["data"]["returnCreate"]["return"];
    assert_eq!(created_return["status"], json!("OPEN"));
    assert_eq!(
        created_return["returnLineItems"]["nodes"][0]["customerNote"],
        json!("Box was dented")
    );

    let created_read =
        read_return_line_customer_note(&mut create_proxy, created_return["id"].clone());
    assert_eq!(
        created_read["returnLineItems"]["nodes"][0]["customerNote"],
        json!("Box was dented")
    );
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
                        nodes { id totalQuantity remainingQuantity returnLineItem { id } }
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

fn approve_return_request_for_test(proxy: &mut DraftProxy, return_id: Value) -> Value {
    proxy
        .process_request(json_graphql_request(
            r#"
            mutation ApproveReturnRequestForErrorShape($input: ReturnApproveRequestInput!) {
              returnApproveRequest(input: $input) {
                return { id status }
                userErrors { field message code }
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
                userErrors { field message code }
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
                      nodes { id totalQuantity remainingQuantity returnLineItem { id } }
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
                          nodes { id totalQuantity remainingQuantity returnLineItem { id } }
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
                      remainingQuantity
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
                      remainingQuantity
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
                    remainingQuantity
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
                    remainingQuantity
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
            node["reverseFulfillmentOrderLineItem"]["remainingQuantity"],
            rfo_line["remainingQuantity"]
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
                    nodes { id totalQuantity remainingQuantity returnLineItem { id } }
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
                    nodes { id totalQuantity remainingQuantity returnLineItem { id } }
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
                userErrors { field message code }
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
                    "message": "Quantity must be greater than 0",
                    "code": "GREATER_THAN"
                }])
            } else {
                json!([{
                    "field": ["returnLineItems", "0", "quantity"],
                    "message": "Return line item has an invalid quantity.",
                    "code": "INVALID"
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
            "message": "Return is not approvable. Only returns with status REQUESTED can be approved.",
            "code": "INVALID_STATE"
        }])
    );

    let rejected_decline = decline_return_request_for_test(&mut proxy, open_return.return_id);
    assert_eq!(rejected_decline["return"], Value::Null);
    assert_eq!(
        rejected_decline["userErrors"],
        json!([{
            "field": ["input", "id"],
            "message": "Return is not declinable. Only non-refunded returns with status REQUESTED can be declined.",
            "code": "INVALID_STATE"
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
            "message": "The return is already declined.",
            "code": "INVALID_STATE"
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
            "message": "Return not found.",
            "code": "NOT_FOUND"
        }])
    );

    let rejected_decline =
        decline_return_request_for_test(&mut proxy, json!("gid://shopify/Return/999999999992"));
    assert_eq!(rejected_decline["return"], Value::Null);
    assert_eq!(
        rejected_decline["userErrors"],
        json!([{
            "field": ["input", "id"],
            "message": "Return not found.",
            "code": "NOT_FOUND"
        }])
    );
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
                userErrors { field message code }
              }
            }
            "#,
            json!({
                "order": {
                    "email": email,
                    "currency": "USD",
                    "financialStatus": "PENDING",
                    "fulfillmentStatus": "UNFULFILLED",
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

    let mut proxy = snapshot_proxy();
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
                  giftCard
                  fulfillmentService
                  fulfillmentStatus
                  weight { value unit }
                  appliedDiscounts {
                    title
                    value { amount currencyCode }
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
                "email": "line-internal-fields@example.com",
                "lineItems": [{
                    "title": "Internal line fields",
                    "quantity": 1,
                    "priceSet": { "shopMoney": { "amount": "9.00", "currencyCode": "CAD" } },
                    "giftCard": true,
                    "fulfillmentService": "manual",
                    "fulfillmentStatus": "FULFILLED",
                    "weight": { "value": 2.5, "unit": "KILOGRAMS" },
                    "appliedDiscounts": [{
                        "title": "line discount",
                        "value": { "fixedAmountValue": { "amount": "1.00", "currencyCode": "CAD" } }
                    }]
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
    assert_eq!(custom_line["giftCard"], json!(true));
    assert_eq!(custom_line["fulfillmentService"], json!("manual"));
    assert_eq!(custom_line["fulfillmentStatus"], json!("FULFILLED"));
    assert_eq!(
        custom_line["weight"],
        json!({ "value": 2.5, "unit": "KILOGRAMS" })
    );
    assert_eq!(
        custom_line["appliedDiscounts"],
        json!([{ "title": "line discount", "value": { "amount": "1.0", "currencyCode": "CAD" } }])
    );
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
fn order_create_validation_matrix_returns_typed_user_errors() {
    let mut proxy = snapshot_proxy();

    let response = proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/orders/orderCreate-validation-matrix-extended.graphql"
        ),
        serde_json::from_str(include_str!(
            "../../config/parity-requests/orders/orderCreate-validation-matrix-extended.variables.json"
        ))
        .unwrap(),
    ));

    assert_eq!(
        response.body["data"]["futureProcessedAt"]["userErrors"],
        json!([{ "field": ["order", "processedAt"], "code": "PROCESSED_AT_INVALID" }])
    );
    assert_eq!(
        response.body["data"]["redundantCustomer"]["userErrors"],
        json!([{ "field": ["order"], "code": "REDUNDANT_CUSTOMER_FIELDS" }])
    );
    assert_eq!(
        response.body["data"]["lineItemTaxLineMissingRate"]["userErrors"],
        json!([{ "field": ["order", "lineItems", 0, "taxLines", 0, "rate"], "code": "TAX_LINE_RATE_MISSING" }])
    );
    assert_eq!(
        response.body["data"]["shippingLineTaxLineMissingRate"]["userErrors"],
        json!([{ "field": ["order", "shippingLines", 0, "taxLines", 0, "rate"], "code": "TAX_LINE_RATE_MISSING" }])
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
        include_str!(
            "../../config/parity-requests/orders/orderCancel-state-transitions-setup-cancel.graphql"
        ),
        json!({ "orderId": cancelled_id.clone(), "restock": false, "reason": "OTHER" }),
    ));
    assert_eq!(setup_cancel.body, fixture["expected"]["cancelOrderSuccess"]);

    let already_cancelled = proxy.process_request(json_graphql_request(
        include_str!("../../config/parity-requests/orders/orderCancel-state-transitions.graphql"),
        json!({ "orderId": cancelled_id, "restock": false, "reason": "OTHER" }),
    ));
    assert_eq!(
        already_cancelled.body,
        fixture["expected"]["alreadyCancelled"]
    );

    let staff_note_too_long = proxy.process_request(json_graphql_request(
        include_str!("../../config/parity-requests/orders/orderCancel-state-transitions.graphql"),
        json!({
            "orderId": fresh_order_id.clone(),
            "restock": false,
            "reason": "OTHER",
            "staffNote": "x".repeat(300)
        }),
    ));
    assert_eq!(
        staff_note_too_long.body,
        fixture["expected"]["staffNoteTooLong"]
    );

    let refund_conflict = proxy.process_request(json_graphql_request(
        include_str!("../../config/parity-requests/orders/orderCancel-state-transitions.graphql"),
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
        fixture["expected"]["refundAndRefundMethodConflict"]
    );

    let refund_false_conflict = proxy.process_request(json_graphql_request(
        include_str!("../../config/parity-requests/orders/orderCancel-state-transitions.graphql"),
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
        fixture["expected"]["refundFalseAndRefundMethodConflict"]
    );
    assert_eq!(log_snapshot(&proxy)["entries"].as_array().unwrap().len(), 3);

    let unknown_order = proxy.process_request(json_graphql_request(
        include_str!("../../config/parity-requests/orders/orderCancel-state-transitions.graphql"),
        json!({ "orderId": "gid://shopify/Order/404", "restock": false, "reason": "OTHER" }),
    ));
    assert_eq!(unknown_order.body, fixture["expected"]["unknownOrder"]);
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
            order { id closed closedAt cancelledAt cancelReason }
            userErrors { field message code }
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
    assert_eq!(cancel_payload["order"]["id"], order_id);
    assert_eq!(cancel_payload["order"]["closed"], json!(true));
    assert_eq!(cancel_payload["order"]["cancelReason"], json!("CUSTOMER"));
    let cancelled_at = cancel_payload["order"]["cancelledAt"]
        .as_str()
        .expect("cancelledAt should be selected");
    let closed_at = cancel_payload["order"]["closedAt"]
        .as_str()
        .expect("closedAt should be selected");
    assert!(cancelled_at.starts_with("2024-01-01T00:00:"));
    assert_eq!(closed_at, cancelled_at);

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
        json!([{ "field": ["orderId"], "message": "Cannot cancel an order that has already been canceled", "code": "INVALID" }])
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
            "purchasingEntity": {
                "purchasingCompany": { "companyId": company_id }
            },
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
        include_str!("../../config/parity-requests/orders/orderCancel-state-transitions.graphql"),
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
            userErrors { field message code }
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
            company { id name }
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
            "purchasingEntity": {
                "purchasingCompany": { "companyId": first_company_id }
            },
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
            "purchasingEntity": {
                "purchasingCompany": { "companyId": second_company_id }
            },
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
            userErrors { field message code }
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
        include_str!("../../config/parity-requests/orders/draftOrderBulkTag-validation-add.graphql"),
        json!({
            "ids": [draft_order_id.clone(), "gid://shopify/DraftOrder/draft-order-bulk-tag-missing"],
            "tags": [" added ", "ADDED"]
        }),
    ));
    assert_eq!(
        partial_add.body,
        fixture["expected"]["partialSuccessWithUnknownId"]
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
        include_str!(
            "../../config/parity-requests/orders/draftOrderBulkTag-validation-add.graphql"
        ),
        json!({ "ids": [draft_order_id.clone()], "tags": [fixture["inputs"]["longTag"].clone()] }),
    ));
    assert_eq!(long_tag.body, fixture["expected"]["longTagRejected"]);

    let remove = proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/orders/draftOrderBulkTag-validation-remove.graphql"
        ),
        json!({ "ids": [draft_order_id.clone()], "tags": [" INITIAL "] }),
    ));
    assert_eq!(
        remove.body,
        fixture["expected"]["removeNormalizesTagIdentity"]
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
        include_str!(
            "../../config/parity-requests/orders/draftOrderBulkTag-validation-add.graphql"
        ),
        json!({ "ids": [draft_order_id], "tags": fixture["inputs"]["tooManyTags"].clone() }),
    ));
    assert_eq!(too_many.body, fixture["expected"]["tooManyInputTags"]);
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
        include_str!(
            "../../config/parity-requests/orders/draftOrderBulkTag-validation-add.graphql"
        ),
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
        include_str!(
            "../../config/parity-requests/orders/draftOrderBulkTag-validation-remove.graphql"
        ),
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
            userErrors { field message code }
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
            userErrors { field message code }
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
        json!({ "amount": "7.5", "currencyCode": "USD" })
    );

    let duplicate = proxy.process_request(json_graphql_request(
        r#"
        mutation DuplicateDraft($id: ID!) {
          draftOrderDuplicate(id: $id) {
            draftOrder { id name status ready email tags totalPriceSet { shopMoney { amount currencyCode } } }
            userErrors { field message code }
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
                userErrors { field message code }
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
            userErrors { field message code }
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
    assert_draft_order_variant_catalog_line(&created_draft["lineItems"]["nodes"][0], 2);
    assert_draft_order_custom_line(&created_draft["lineItems"]["nodes"][1]);

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
            userErrors { field message code }
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
            userErrors { field message code }
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
    );

    assert_eq!(upstream_calls.lock().unwrap().len(), 3);
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
            userErrors { field message code }
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
    assert_eq!(line_items_max.body["data"], Value::Null);
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
    assert_eq!(too_many_tags_response.body["data"], Value::Null);
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
    let (_success_order, success_schedule) = stage_reminder_order_payment_schedule(
        &mut proxy,
        Some("reminder-success@example.test"),
        None,
    );
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
        stage_reminder_order_payment_schedule(&mut proxy, Some("   "), None);
    let missing_email = proxy.process_request(json_graphql_request(
        query,
        json!({ "paymentScheduleId": missing_email_schedule }),
    ));
    assert_eq!(
        missing_email.body,
        json!({ "data": { "paymentReminderSend": payment_reminder_error("Order does not have a contact email") } })
    );

    let (_selling_plan_order, selling_plan_schedule) = stage_reminder_order_payment_schedule(
        &mut proxy,
        Some("reminder-selling-plan@example.test"),
        Some("Subscribe and save"),
    );
    let selling_plan = proxy.process_request(json_graphql_request(
        query,
        json!({ "paymentScheduleId": selling_plan_schedule }),
    ));
    assert_eq!(
        selling_plan.body,
        json!({ "data": { "paymentReminderSend": payment_reminder_error("Order has a selling plan") } })
    );

    let (paid_order, paid_schedule) =
        stage_reminder_order_payment_schedule(&mut proxy, Some("reminder-paid@example.test"), None);
    mark_reminder_order_paid(&mut proxy, paid_order);
    let paid = proxy.process_request(json_graphql_request(
        query,
        json!({ "paymentScheduleId": paid_schedule }),
    ));
    assert_eq!(
        paid.body,
        json!({ "data": { "paymentReminderSend": payment_reminder_error("Payment schedule is already completed") } })
    );

    let (closed_order, closed_schedule) = stage_reminder_order_payment_schedule(
        &mut proxy,
        Some("reminder-closed@example.test"),
        None,
    );
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
    selling_plan_name: Option<&str>,
) -> (Value, Value) {
    let mut line_item = json!({
        "title": "Reminder order item",
        "quantity": 1,
        "priceSet": {
            "shopMoney": { "amount": "10.00", "currencyCode": "CAD" },
            "presentmentMoney": { "amount": "10.00", "currencyCode": "CAD" }
        },
        "taxable": false
    });
    if let Some(name) = selling_plan_name {
        line_item["sellingPlanName"] = json!(name);
    }
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
    let draft_id = create_payment_terms_test_draft(proxy, "payment-reminder-draft@example.test");
    stage_reminder_payment_terms(proxy, json!(draft_id))
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

#[test]
fn payment_customization_local_runtime_covers_create_activation_update_readback_helpers() {
    let mut proxy = snapshot_proxy();
    let create_query = r#"
      mutation RustPaymentCustomizationLocalRuntime($input: PaymentCustomizationInput!) {
        paymentCustomizationCreate(paymentCustomization: $input) {
          paymentCustomization {
            id
            title
            enabled
            functionId
            functionHandle
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
            functionHandle
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
          functionHandle
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
            ["paymentCustomization"]["functionHandle"],
        Value::Null
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
    let create_validation = create_validation_proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/payments/payment-customization-create-validation-gaps.graphql"
        ),
        create_validation_fixture["variables"].clone(),
    ));
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

    let mut metafields_proxy = snapshot_proxy();
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

    let setup_order_id =
        create_payment_terms_test_order(&mut proxy, "payment-terms-omitted-template@example.test");
    let setup = proxy.process_request(json_graphql_request(
        create_query,
        json!({
            "referenceId": setup_order_id,
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
        create_payment_terms_test_draft(&mut proxy, "payment-terms-past-due@example.test");
    let future_draft_id =
        create_payment_terms_test_draft(&mut proxy, "payment-terms-future-due@example.test");

    let past_create = proxy.process_request(json_graphql_request(
        create_query,
        json!({
            "referenceId": past_draft_id,
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
            "referenceId": future_draft_id,
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

fn create_payment_terms_test_order(proxy: &mut DraftProxy, email: &str) -> String {
    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation CreatePaymentTermsTestOrder($order: OrderCreateOrderInput!) {
          orderCreate(order: $order) {
            order { id }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "order": {
                "email": email,
                "currency": "CAD",
                "presentmentCurrency": "CAD",
                "financialStatus": "PENDING",
                "lineItems": [{
                    "title": "Payment terms test owner",
                    "quantity": 1,
                    "priceSet": {
                        "shopMoney": { "amount": "57.00", "currencyCode": "CAD" },
                        "presentmentMoney": { "amount": "57.00", "currencyCode": "CAD" }
                    }
                }]
            }
        }),
    ));
    assert_eq!(create.status, 200);
    assert_eq!(create.body["data"]["orderCreate"]["userErrors"], json!([]));
    create.body["data"]["orderCreate"]["order"]["id"]
        .as_str()
        .expect("payment terms test order id")
        .to_string()
}

fn create_payment_terms_test_draft(proxy: &mut DraftProxy, email: &str) -> String {
    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation CreatePaymentTermsTestDraft($input: DraftOrderInput!) {
          draftOrderCreate(input: $input) {
            draftOrder { id }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "input": {
                "email": email,
                "lineItems": [{
                    "title": "Payment terms draft owner",
                    "quantity": 1,
                    "originalUnitPrice": "21.00"
                }]
            }
        }),
    ));
    assert_eq!(create.status, 200);
    assert_eq!(
        create.body["data"]["draftOrderCreate"]["userErrors"],
        json!([])
    );
    create.body["data"]["draftOrderCreate"]["draftOrder"]["id"]
        .as_str()
        .expect("payment terms test draft id")
        .to_string()
}

#[test]
fn payment_terms_create_update_guardrails_cover_current_helper_edges() {
    let create_query = r#"
        mutation RustPaymentTermsLocalRuntimeCreate($referenceId: ID!, $attrs: PaymentTermsAttributesInput!) {
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
    let setup_order = proxy.process_request(json_graphql_request(
        r#"
        mutation RustPaymentTermsSetupOrder($order: OrderCreateOrderInput!) {
          orderCreate(order: $order) {
            order { id }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "order": {
                "email": "payment-terms-guardrails@example.test",
                "financialStatus": "PENDING",
                "lineItems": [{
                    "title": "Payment terms owner",
                    "quantity": 1,
                    "priceSet": {
                        "shopMoney": { "amount": "57.00", "currencyCode": "CAD" },
                        "presentmentMoney": { "amount": "42.50", "currencyCode": "USD" }
                    }
                }]
            }
        }),
    ));
    assert_eq!(setup_order.status, 200);
    assert_eq!(
        setup_order.body["data"]["orderCreate"]["userErrors"],
        json!([])
    );
    let staged_order_id = setup_order.body["data"]["orderCreate"]["order"]["id"]
        .as_str()
        .expect("setup order id")
        .to_string();

    let setup_draft = proxy.process_request(json_graphql_request(
        r#"
        mutation RustPaymentTermsSetupDraft($input: DraftOrderInput!) {
          draftOrderCreate(input: $input) {
            draftOrder { id }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "input": {
                "email": "payment-terms-draft@example.test",
                "lineItems": [{
                    "title": "Payment terms draft owner",
                    "quantity": 1,
                    "originalUnitPrice": "21.00"
                }]
            }
        }),
    ));
    assert_eq!(setup_draft.status, 200);
    assert_eq!(
        setup_draft.body["data"]["draftOrderCreate"]["userErrors"],
        json!([])
    );
    let staged_draft_order_id = setup_draft.body["data"]["draftOrderCreate"]["draftOrder"]["id"]
        .as_str()
        .expect("setup draft order id")
        .to_string();

    let paid_create = proxy.process_request(json_graphql_request(
        create_query,
        json!({ "referenceId": "gid://shopify/Order/paid", "attrs": net_attrs.clone() }),
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

    for reference_id in [staged_order_id.as_str(), staged_draft_order_id.as_str()] {
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

    let multiple_schedules = proxy.process_request(json_graphql_request(
        create_query,
        json!({
            "referenceId": staged_order_id,
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
        json!({ "referenceId": "gid://shopify/Order/456", "attrs": net_attrs.clone() }),
    ));
    assert_eq!(
        unknown_order.body["data"]["paymentTermsCreate"]["userErrors"][0],
        json!({
            "field": Value::Null,
            "message": "Cannot find the specific Order with id 456.",
            "code": "PAYMENT_TERMS_CREATION_UNSUCCESSFUL"
        })
    );

    let unknown_draft = proxy.process_request(json_graphql_request(
        create_query,
        json!({ "referenceId": "gid://shopify/DraftOrder/42", "attrs": net_attrs.clone() }),
    ));
    assert_eq!(
        unknown_draft.body["data"]["paymentTermsCreate"]["userErrors"][0],
        json!({
            "field": Value::Null,
            "message": "Cannot find the specific Draft order with id 42.",
            "code": "PAYMENT_TERMS_CREATION_UNSUCCESSFUL"
        })
    );

    let unknown_template = proxy.process_request(json_graphql_request(
        create_query,
        json!({
            "referenceId": staged_order_id,
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

    let fixed_without_due = proxy.process_request(json_graphql_request(
        create_query,
        json!({
            "referenceId": staged_order_id,
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

    let receipt_with_due = proxy.process_request(json_graphql_request(
        create_query,
        json!({
            "referenceId": staged_order_id,
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

    let receipt_issued_at = proxy.process_request(json_graphql_request(
        create_query,
        json!({
            "referenceId": staged_order_id,
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

    let missing_update = proxy.process_request(json_graphql_request(
        update_query,
        json!({ "input": { "paymentTermsId": "gid://shopify/PaymentTerms/500000", "paymentTermsAttributes": net_attrs.clone() } }),
    ));
    assert_eq!(
        missing_update.body["data"]["paymentTermsUpdate"]["userErrors"][0],
        json!({
            "field": null,
            "message": "Could not find payment terms.",
            "code": "PAYMENT_TERMS_UPDATE_UNSUCCESSFUL"
        })
    );

    let missing_update_invalid_attrs = proxy.process_request(json_graphql_request(
        update_query,
        json!({
            "input": {
                "paymentTermsId": "gid://shopify/PaymentTerms/500001",
                "paymentTermsAttributes": {
                    "paymentTermsTemplateId": "gid://shopify/PaymentTermsTemplate/9999",
                    "paymentSchedules": [{ "issuedAt": "2026-01-01T00:00:00Z" }]
                }
            }
        }),
    ));
    assert_eq!(
        missing_update_invalid_attrs.body["data"]["paymentTermsUpdate"]["userErrors"][0],
        json!({
            "field": Value::Null,
            "message": "Could not find payment terms.",
            "code": "PAYMENT_TERMS_UPDATE_UNSUCCESSFUL"
        })
    );

    let paid_update = proxy.process_request(json_graphql_request(
        update_query,
        json!({ "input": { "paymentTermsId": "gid://shopify/PaymentTerms/paid-update", "paymentTermsAttributes": net_attrs.clone() } }),
    ));
    assert_eq!(
        paid_update.body["data"]["paymentTermsUpdate"]["userErrors"][0],
        json!({
            "field": Value::Null,
            "message": "Cannot create payment terms on an Order that has already been paid in full.",
            "code": "PAYMENT_TERMS_UPDATE_UNSUCCESSFUL"
        })
    );

    let channel_policy_update = proxy.process_request(json_graphql_request(
        update_query,
        json!({ "input": { "paymentTermsId": "gid://shopify/PaymentTerms/channel-policy-update", "paymentTermsAttributes": net_attrs.clone() } }),
    ));
    assert_eq!(
        channel_policy_update.body["data"]["paymentTermsUpdate"]["userErrors"][0]["message"],
        json!("Cannot create payment terms on an Order where the sales channel does not allow payment terms.")
    );

    let draft_update_seed = proxy.process_request(json_graphql_request(
        create_query,
        json!({
            "referenceId": staged_draft_order_id,
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
                "paymentTermsId": draft_update_id.clone(),
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
                "paymentTermsId": draft_update_id,
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

    for (label, attrs, expected_name, expected_type, expected_due_days, schedule_count) in [
        (
            "fixed-template",
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
            "net-7-template",
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
            "fulfillment-template",
            json!({
                "paymentTermsTemplateId": "gid://shopify/PaymentTermsTemplate/9"
            }),
            "Due on fulfillment",
            "FULFILLMENT",
            Value::Null,
            0_usize,
        ),
    ] {
        let reference_id = create_payment_terms_test_draft(
            &mut proxy,
            &format!("payment-terms-{label}@example.test"),
        );
        let create = proxy.process_request(json_graphql_request(
            create_query,
            json!({ "referenceId": reference_id, "attrs": attrs.clone() }),
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

    let cascade_draft_id = create_payment_terms_test_draft(
        &mut proxy,
        "payment-terms-delete-cascade-draft@example.test",
    );
    let draft_terms = proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/payments/payment-terms-lifecycle-create.graphql"
        ),
        json!({
            "referenceId": cascade_draft_id,
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
    let fixture: Value = serde_json::from_str(include_str!(
        "../../fixtures/conformance/local-runtime/2026-04/orders/order-payment-transaction-local-staging.json"
    ))
    .unwrap();
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
    assert_eq!(
        missing_mandate.body,
        fixture["mandateFlow"]["expected"]["missingMandate"]
    );

    let first_mandate = proxy.process_request(json_graphql_request(
        include_str!("../../config/parity-requests/payments/order_create_mandate_payment.graphql"),
        json!({
            "id": "gid://shopify/Order/1",
            "mandateId": "gid://shopify/PaymentMandate/har-397",
            "idempotencyKey": "har-353-idempotent-payment",
            "amount": { "amount": "25.00", "currencyCode": "CAD" }
        }),
    ));
    assert_eq!(
        first_mandate.body,
        fixture["mandateFlow"]["expected"]["mandate"]
    );

    let repeat = proxy.process_request(json_graphql_request(
        include_str!("../../config/parity-requests/payments/order_create_mandate_payment.graphql"),
        json!({
            "id": "gid://shopify/Order/1",
            "mandateId": "gid://shopify/PaymentMandate/har-397",
            "idempotencyKey": "har-353-idempotent-payment",
            "amount": { "amount": "25.00", "currencyCode": "CAD" }
        }),
    ));
    assert_eq!(
        repeat.body,
        fixture["mandateFlow"]["expected"]["repeatMandate"]
    );

    let auth_only = proxy.process_request(json_graphql_request(
        include_str!("../../config/parity-requests/payments/order_create_mandate_payment.graphql"),
        json!({
            "id": "gid://shopify/Order/1",
            "mandateId": "gid://shopify/PaymentMandate/har-397",
            "idempotencyKey": "har-848-auth-only",
            "autoCapture": false,
            "amount": { "amount": "25.00", "currencyCode": "CAD" }
        }),
    ));
    assert_eq!(
        auth_only.body,
        fixture["mandateFlow"]["expected"]["autoCaptureFalse"]
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
        include_str!(
            "../../config/parity-requests/orders/order-payment-create-local-staging.graphql"
        ),
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
        include_str!("../../config/parity-requests/orders/order-payment-capture-local-staging.graphql"),
        json!({"input": {"id": order_id, "parentTransactionId": parent_transaction_id, "amount": "30.00", "currency": "CAD"}}),
    ));
    assert_eq!(
        over_capture.body["data"]["orderCapture"]["transaction"],
        Value::Null
    );
    assert_eq!(
        over_capture.body["data"]["orderCapture"]["userErrors"][0]["field"],
        json!(["amount"])
    );

    let first_capture = capture_proxy.process_request(json_graphql_request(
        include_str!("../../config/parity-requests/orders/order-payment-capture-local-staging.graphql"),
        json!({"input": {"id": create.body["data"]["orderCreate"]["order"]["id"].clone(), "parentTransactionId": create.body["data"]["orderCreate"]["order"]["transactions"][0]["id"].clone(), "amount": "10.00", "currency": "CAD"}}),
    ));
    assert_eq!(
        first_capture.body["data"]["orderCapture"]["order"]["displayFinancialStatus"],
        json!("PARTIALLY_PAID")
    );
    assert_eq!(
        first_capture.body["data"]["orderCapture"]["order"]["totalCapturable"],
        json!("15.0")
    );

    let final_capture = capture_proxy.process_request(json_graphql_request(
        include_str!("../../config/parity-requests/orders/order-payment-capture-local-staging.graphql"),
        json!({"input": {"id": create.body["data"]["orderCreate"]["order"]["id"].clone(), "parentTransactionId": create.body["data"]["orderCreate"]["order"]["transactions"][0]["id"].clone(), "amount": "15.00", "currency": "CAD", "finalCapture": null}}),
    ));
    let final_order = final_capture.body["data"]["orderCapture"]["order"].clone();
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
        include_str!(
            "../../config/parity-requests/orders/order-payment-read-local-staging.graphql"
        ),
        json!({"id": create.body["data"]["orderCreate"]["order"]["id"].clone()}),
    ));
    assert_eq!(read_after_final.body["data"]["order"], final_order);

    let void_after_capture = capture_proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/orders/order-payment-void-local-staging.graphql"
        ),
        json!({"id": create.body["data"]["orderCreate"]["order"]["transactions"][0]["id"].clone()}),
    ));
    assert_eq!(
        void_after_capture.body["data"]["transactionVoid"]["transaction"],
        Value::Null
    );

    let missing_mandate_idempotency = capture_proxy.process_request(json_graphql_request(
        include_str!("../../config/parity-requests/orders/order-payment-mandate-local-staging.graphql"),
        json!({"id": create.body["data"]["orderCreate"]["order"]["id"].clone(), "mandateId": "gid://shopify/PaymentMandate/har-397"}),
    ));
    assert_eq!(
        missing_mandate_idempotency.body["data"]["orderCreateMandatePayment"]["userErrors"][0]
            ["field"],
        json!(["idempotencyKey"])
    );

    let mut void_proxy = snapshot_proxy();
    let void_create = void_proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/orders/order-payment-create-local-staging.graphql"
        ),
        fixture["voidFlow"]["create"]["variables"].clone(),
    ));
    assert_eq!(
        void_create.body["data"]["orderCreate"]["order"]["displayFinancialStatus"],
        json!("AUTHORIZED")
    );

    let void_response = void_proxy.process_request(json_graphql_request(
        include_str!("../../config/parity-requests/orders/order-payment-void-local-staging.graphql"),
        json!({"id": void_create.body["data"]["orderCreate"]["order"]["transactions"][0]["id"].clone()}),
    ));
    assert_eq!(
        void_response.body["data"]["transactionVoid"]["transaction"]["kind"],
        json!("VOID")
    );

    let read_after_void = void_proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/orders/order-payment-read-local-staging.graphql"
        ),
        json!({"id": void_create.body["data"]["orderCreate"]["order"]["id"].clone()}),
    ));
    assert_eq!(
        read_after_void.body["data"]["order"]["displayFinancialStatus"],
        json!("VOIDED")
    );
}

#[test]
fn order_capture_rejects_boolean_final_capture_for_manual_gateway_without_side_effects() {
    let mut proxy = snapshot_proxy();

    let create = proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/orders/order-payment-create-local-staging.graphql"
        ),
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
            include_str!(
                "../../config/parity-requests/orders/order-payment-capture-local-staging.graphql"
            ),
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
        include_str!(
            "../../config/parity-requests/orders/order-payment-non-recording-create.graphql"
        ),
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
        include_str!(
            "../../config/parity-requests/orders/order-payment-non-recording-capture.graphql"
        ),
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
    assert_eq!(
        capture.body["data"]["orderCapture"]["order"]["displayFinancialStatus"],
        json!("PARTIALLY_PAID")
    );

    let read_after_capture = proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/orders/order-payment-non-recording-read.graphql"
        ),
        json!({ "id": order_id.clone() }),
    ));
    assert_eq!(
        read_after_capture.body["data"]["order"]["displayFinancialStatus"],
        json!("PARTIALLY_PAID")
    );

    let mandate = proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/orders/order-payment-non-recording-mandate.graphql"
        ),
        json!({
            "id": order_id,
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
        mandate.body["data"]["orderCreateMandatePayment"]["order"]["displayFinancialStatus"],
        json!("PAID")
    );

    let mut void_proxy = snapshot_proxy();
    let void_create = void_proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/orders/order-payment-non-recording-create.graphql"
        ),
        fixture["voidFlow"]["create"]["variables"].clone(),
    ));
    let void_response = void_proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/orders/order-payment-non-recording-void.graphql"
        ),
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
        include_str!(
            "../../config/parity-requests/orders/order-payment-create-local-staging.graphql"
        ),
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
        include_str!(
            "../../config/parity-requests/orders/order-payment-capture-local-staging.graphql"
        ),
        json!({
            "input": {
                "id": order_id,
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
        capture.body["data"]["orderCapture"]["order"]["displayFinancialStatus"],
        json!("PARTIALLY_PAID")
    );
}

#[test]
fn order_capture_zero_amount_uses_captured_public_error_without_code() {
    let mut proxy = snapshot_proxy();

    let create = proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/orders/order-payment-create-local-staging.graphql"
        ),
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
            userErrors { field message code }
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
fn order_payment_transactions_use_order_transaction_state_not_magic_values() {
    let mut proxy = snapshot_proxy();

    let create_a = proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/orders/order-payment-create-local-staging.graphql"
        ),
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
        include_str!(
            "../../config/parity-requests/orders/order-payment-capture-local-staging.graphql"
        ),
        json!({
            "input": {
                "id": order_a_id,
                "parentTransactionId": parent_a_id,
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
        capture_a.body["data"]["orderCapture"]["order"]["displayFinancialStatus"],
        json!("PAID")
    );

    let create_b = proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/orders/order-payment-create-local-staging.graphql"
        ),
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
        include_str!(
            "../../config/parity-requests/orders/order-payment-capture-local-staging.graphql"
        ),
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
        over_capture_b.body["data"]["orderCapture"]["userErrors"][0]["field"],
        json!(["amount"])
    );

    let void_b = proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/orders/order-payment-void-local-staging.graphql"
        ),
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
        include_str!(
            "../../config/parity-requests/orders/order-payment-void-local-staging.graphql"
        ),
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
            order { id cancelledAt }
            userErrors { field message code }
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

#[test]
fn abandonment_delivery_status_edge_cases_replay_mutation_and_reads() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../fixtures/conformance/local-runtime/2026-04/orders/abandonmentUpdateActivitiesDeliveryStatuses-edge-cases.json"
    ))
    .unwrap();
    let mut proxy = snapshot_proxy();

    let forward = proxy.process_request(json_graphql_request(
        include_str!("../../config/parity-requests/orders/abandonmentUpdateActivitiesDeliveryStatuses-edge-cases.graphql"),
        fixture["cases"]["forward"]["variables"].clone(),
    ));
    assert_eq!(forward.body, fixture["cases"]["forward"]["expected"]);

    let abandonment_id = forward.body["data"]["abandonmentUpdateActivitiesDeliveryStatuses"]
        ["abandonment"]["id"]
        .clone();
    let read_after = proxy.process_request(json_graphql_request(
        include_str!("../../config/parity-requests/orders/abandonmentUpdateActivitiesDeliveryStatuses-read.graphql"),
        json!({"id": abandonment_id.clone()}),
    ));
    assert_eq!(read_after.body, fixture["cases"]["forwardRead"]["expected"]);

    let node_read = proxy.process_request(json_graphql_request(
        include_str!("../../config/parity-requests/orders/abandonmentUpdateActivitiesDeliveryStatuses-node-read.graphql"),
        json!({"id": abandonment_id}),
    ));
    assert_eq!(
        node_read.body["data"]["node"],
        fixture["cases"]["forwardRead"]["expected"]["data"]["abandonment"]
    );

    for case_name in [
        "unknownMarketingActivity",
        "backwards",
        "sameStatus",
        "futureDeliveredAt",
    ] {
        let response = proxy.process_request(json_graphql_request(
            include_str!("../../config/parity-requests/orders/abandonmentUpdateActivitiesDeliveryStatuses-edge-cases.graphql"),
            fixture["cases"][case_name]["variables"].clone(),
        ));
        assert_eq!(
            response.body["data"]["abandonmentUpdateActivitiesDeliveryStatuses"],
            fixture["cases"][case_name]["expected"]["data"]
                ["abandonmentUpdateActivitiesDeliveryStatuses"],
            "abandonment delivery-status case {case_name} should match fixture"
        );
    }
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
            userErrors { field message code }
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

    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateCompletableDraft($input: DraftOrderInput!) {
          draftOrderCreate(input: $input) {
            draftOrder {
              id
              status
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
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "input": {
                "email": "customer-completion-any-email@example.com",
                "shippingLine": {
                    "title": "Local courier",
                    "priceWithCurrency": { "amount": "3.25", "currencyCode": "CAD" }
                },
                "lineItems": [
                    {
                        "title": "Completion service",
                        "quantity": 2,
                        "originalUnitPrice": "12.50",
                        "sku": "COMPLETE-A"
                    },
                    {
                        "title": "Completion add-on",
                        "quantity": 1,
                        "originalUnitPrice": "4.00",
                        "sku": "COMPLETE-B"
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
                sourceName
                displayFinancialStatus
                currentTotalPriceSet { shopMoney { amount currencyCode } }
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
    assert_eq!(order["sourceName"], json!("123456789012"));
    assert_eq!(order["displayFinancialStatus"], json!("PAID"));
    assert_eq!(
        order["currentTotalPriceSet"]["shopMoney"],
        json!({ "amount": "32.25", "currencyCode": "CAD" })
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
                "currency": "CAD",
                "lineItems": [{
                    "title": "Refund guardrail item",
                    "quantity": 1,
                    "priceSet": { "shopMoney": { "amount": "10.00", "currencyCode": "CAD" } }
                }],
                "transactions": [{
                    "kind": "SALE",
                    "status": "SUCCESS",
                    "gateway": "manual",
                    "amountSet": { "shopMoney": { "amount": "10.00", "currencyCode": "CAD" } }
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
            "field": null,
            "message": "Refund amount $15.00 is greater than net payment received $10.00"
        })
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
            invoiceErrors { code message }
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
    assert_eq!(
        send.body["data"]["draftOrderInvoiceSend"]["invoiceErrors"],
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
            invoiceErrors { code message }
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
    assert_eq!(
        missing_payload["invoiceErrors"][0]["code"],
        json!("CUSTOMER_NO_EMAIL")
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
            invoiceErrors { code message }
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
    assert_eq!(paid_payload["invoiceErrors"], json!([]));
}

#[test]
fn draft_order_invoice_send_invoice_errors_local_runtime_parity() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../fixtures/conformance/local-runtime/2026-04/orders/draft-order-invoice-send-invoice-errors.json"
    ))
    .unwrap();
    let mut proxy = snapshot_proxy();

    let create = proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/orders/draftOrderInvoiceSend-invoice-errors-create.graphql"
        ),
        json!({}),
    ));
    assert_eq!(create.body, fixture["createOpen"]["response"]);
    let draft_order_id = create.body["data"]["draftOrderCreate"]["draftOrder"]["id"].clone();

    let no_recipient = proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/orders/draftOrderInvoiceSend-invoice-errors-send.graphql"
        ),
        json!({
            "id": draft_order_id.clone(),
            "email": null,
            "currency": null,
            "template": null
        }),
    ));
    assert_eq!(no_recipient.body, fixture["noRecipient"]["response"]);

    let valid_send_variables = fixture["validSend"]["request"]["variables"].clone();
    let valid_send = proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/orders/draftOrderInvoiceSend-invoice-errors-send.graphql"
        ),
        valid_send_variables,
    ));
    assert_eq!(valid_send.body, fixture["validSend"]["response"]);

    let state = state_snapshot(&proxy);
    assert_eq!(
        state["stagedState"]["draftOrders"]["gid://shopify/DraftOrder/1"]["data"]
            ["__draftProxyInvoiceSend"],
        fixture["validSend"]["state"]["stagedState"]["draftOrders"]["gid://shopify/DraftOrder/1"]
            ["data"]["__draftProxyInvoiceSend"]
    );

    let log = log_snapshot(&proxy);
    let entries = log["entries"]
        .as_array()
        .expect("invoice send log entries should be an array");
    let operation_statuses: Vec<Value> = entries
        .iter()
        .map(|entry| json!([entry["operationName"].clone(), entry["status"].clone()]))
        .collect();
    assert_eq!(
        Value::Array(operation_statuses),
        json!([
            ["draftOrderCreate", "staged"],
            ["draftOrderInvoiceSend", "failed"],
            ["draftOrderInvoiceSend", "staged"]
        ])
    );

    let invalid_template = proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/orders/draftOrderInvoiceSend-invoice-errors-send.graphql"
        ),
        json!({
            "id": draft_order_id,
            "email": { "to": "buyer@example.com" },
            "currency": "USD",
            "template": "NOT_A_REAL_TEMPLATE"
        }),
    ));
    assert!(invalid_template.body.get("errors").is_some());
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
                    "countryCodeV2": "CA",
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
    let update_fixture: Value = serde_json::from_str(include_str!(
        "../../fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/orders/orderUpdate-localization-and-staff.json"
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

    let unknown_staff = proxy.process_request(json_graphql_request(
        include_str!("../../config/parity-requests/orders/orderUpdate-localization-and-staff-unknown-staff.graphql"),
        json!({"input": {"id": "gid://shopify/Order/8734696014130", "staffMemberId": "gid://shopify/StaffMember/999999999999"}}),
    ));
    assert_eq!(
        unknown_staff.body["data"]["orderUpdate"]["userErrors"],
        update_fixture["localRuntimeStaffUnknown"]["expected"]["data"]["orderUpdate"]["userErrors"]
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
            order { id cancelledAt }
            userErrors { field message code }
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
                userErrors { field message code }
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
            userErrors { field message code }
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

    let seed = proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/payments/customer-payment-method-remote-create-validation-seed.graphql"
        ),
        json!({}),
    ));
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
            "data": {
                "customerPaymentMethodRemoteCreate": {
                    "customerPaymentMethod": Value::Null,
                    "userErrors": [{
                        "field": ["remote_reference", "stripe_payment_method", "customer_id"],
                        "code": "STRIPE_CUSTOMER_ID_BLANK",
                        "message": "customer_id can't be blank"
                    }]
                }
            }
        })
    );

    let paypal_blank = proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/payments/customer-payment-method-remote-create-validation-paypal.graphql"
        ),
        json!({}),
    ));
    assert_eq!(paypal_blank.status, 200);
    assert_eq!(
        paypal_blank.body,
        json!({
            "data": {
                "customerPaymentMethodRemoteCreate": {
                    "customerPaymentMethod": Value::Null,
                    "userErrors": [{
                        "field": ["remote_reference", "paypal_payment_method", "billing_agreement_id"],
                        "code": "BILLING_AGREEMENT_ID_BLANK",
                        "message": "billing_agreement_id can't be blank"
                    }]
                }
            }
        })
    );

    let two_gateways = proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/payments/customer-payment-method-remote-create-validation-two-gateways.graphql"
        ),
        json!({}),
    ));
    assert_eq!(two_gateways.status, 200);
    assert_eq!(
        two_gateways.body,
        json!({
            "data": {
                "customerPaymentMethodRemoteCreate": {
                    "customerPaymentMethod": Value::Null,
                    "userErrors": [{
                        "field": ["remote_reference"],
                        "code": "INVALID",
                        "message": "Remote reference must contain exactly one payment method."
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
        (
            "adyen blank shopper_reference",
            r#"adyenPaymentMethod: { shopperReference: "", storedPaymentMethodId: "stored_x" }"#,
            [
                "remote_reference",
                "adyen_payment_method",
                "shopper_reference",
            ],
            "INVALID",
            "shopper_reference can't be blank",
        ),
        (
            "adyen blank stored_payment_method_id",
            r#"adyenPaymentMethod: { shopperReference: "shopper_x", storedPaymentMethodId: "" }"#,
            [
                "remote_reference",
                "adyen_payment_method",
                "stored_payment_method_id",
            ],
            "INVALID",
            "stored_payment_method_id can't be blank",
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
                        { "field": ["billing_address", "address1"], "message": "can't be blank", "code": "BLANK" },
                        { "field": ["billing_address", "city"], "message": "can't be blank", "code": "BLANK" },
                        { "field": ["billing_address", "zip"], "message": "can't be blank", "code": "BLANK" },
                        { "field": ["billing_address", "country_code"], "message": "can't be blank", "code": "BLANK" },
                        { "field": ["billing_address", "province_code"], "message": "can't be blank", "code": "BLANK" }
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
        None,
    );

    let primary = proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/payments/customer-payment-method-local-staging.graphql"
        ),
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
        include_str!("../../config/parity-requests/payments/customer-payment-method-duplication-local-staging.graphql"),
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
        include_str!(
            "../../config/parity-requests/payments/customer-payment-method-local-staging-read.graphql"
        ),
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
        include_str!("../../config/parity-requests/payments/customer-payment-method-credit-card-create-validation-blank.graphql"),
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
                { "field": ["billing_address", "address1"], "message": "can't be blank", "code": "BLANK" },
                { "field": ["billing_address", "city"], "message": "can't be blank", "code": "BLANK" },
                { "field": ["billing_address", "zip"], "message": "can't be blank", "code": "BLANK" },
                { "field": ["billing_address", "country_code"], "message": "can't be blank", "code": "BLANK" },
                { "field": ["billing_address", "province_code"], "message": "can't be blank", "code": "BLANK" }
            ]
        })
    );

    let missing_session = proxy.process_request(json_graphql_request(
        include_str!("../../config/parity-requests/payments/customer-payment-method-credit-card-create-validation-missing-session.graphql"),
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
        include_str!("../../config/parity-requests/payments/customer-payment-method-credit-card-create-validation-processing.graphql"),
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
        include_str!("../../config/parity-requests/payments/customer-payment-method-credit-card-create-validation-success.graphql"),
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
            "countryCodeV2": "US",
            "provinceCode": "NY"
        })
    );
    let success_id = success.body["data"]["customerPaymentMethodCreditCardCreate"]
        ["customerPaymentMethod"]["id"]
        .clone();

    let read = proxy.process_request(json_graphql_request(
        include_str!("../../config/parity-requests/payments/customer-payment-method-credit-card-create-validation-read.graphql"),
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
            "countryCodeV2": "US",
            "provinceCode": "NY"
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
            userErrors { field code message }
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
                { "field": ["billing_address", "address1"], "code": "BLANK", "message": "can't be blank" },
                { "field": ["billing_address", "city"], "code": "BLANK", "message": "can't be blank" },
                { "field": ["billing_address", "zip"], "code": "BLANK", "message": "can't be blank" },
                { "field": ["billing_address", "country_code"], "code": "BLANK", "message": "can't be blank" },
                { "field": ["billing_address", "province_code"], "code": "BLANK", "message": "can't be blank" }
            ]
        })
    );

    let active_contract = proxy.process_request(json_graphql_request(
        r#"
        mutation RustCustomerPaymentMethodRevokeLocalRuntimeActive {
          customerPaymentMethodRevoke(customerPaymentMethodId: "gid://shopify/CustomerPaymentMethod/active-contract") {
            revokedCustomerPaymentMethodId
            userErrors { field message code }
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
                "message": "Cannot revoke a payment method with active subscription contracts.",
                "code": "ACTIVE_CONTRACT"
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
            userErrors { field message code }
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
            "revokedReason": "CUSTOMER_REVOKED"
        })
    );

    let already_revoked = proxy.process_request(json_graphql_request(
        r#"
        mutation RustCustomerPaymentMethodRevokeLocalRuntimeAlreadyRevoked {
          customerPaymentMethodRevoke(customerPaymentMethodId: "gid://shopify/CustomerPaymentMethod/already-revoked") {
            revokedCustomerPaymentMethodId
            userErrors { field message code }
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
            "revokedReason": "CUSTOMER_REVOKED"
        })
    );
}

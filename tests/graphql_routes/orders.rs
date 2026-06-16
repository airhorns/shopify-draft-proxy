use super::common::*;
use pretty_assertions::assert_eq;

fn without_extensions(value: &Value) -> Value {
    let mut value = value.clone();
    if let Some(object) = value.as_object_mut() {
        object.remove("extensions");
    }
    value
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
            userErrors { field message code }
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
            userErrors { field message code }
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
            userErrors { field message code }
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
            userErrors { field message code }
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
            userErrors { field message code }
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

    let log = proxy.get_log_snapshot();
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
            userErrors { field message code }
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

    let log = proxy.get_log_snapshot();
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
            userErrors { field message code }
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
                "message": "Fulfillment does not exist.",
                "code": "NOT_FOUND"
            }]
        })
    );
    assert_eq!(proxy.get_log_snapshot()["entries"], json!([]));
}

#[test]
fn fulfillment_event_create_rejects_cancelled_parent_without_logging() {
    let mut proxy = snapshot_proxy();
    let (_order_id, fulfillment_id) = stage_fulfillment_for_event(&mut proxy);
    let cancel = proxy.process_request(json_graphql_request(
        r#"
        mutation CancelBeforeEvent($id: ID!) {
          fulfillmentCancel(id: $id) {
            fulfillment { id status displayStatus }
            userErrors { field message code }
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
            userErrors { field message code }
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
        rejected.body["data"]["fulfillmentEventCreate"],
        json!({
            "fulfillmentEvent": null,
            "userErrors": [{
                "field": ["fulfillmentEvent", "fulfillmentId"],
                "message": "fulfillment_is_cancelled",
                "code": "INVALID"
            }]
        })
    );
    let log = proxy.get_log_snapshot();
    assert_eq!(log["entries"].as_array().unwrap().len(), 3);
    assert_eq!(
        log["entries"][2]["operationName"],
        json!("fulfillmentCancel")
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
            userErrors { field message code }
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
    let log = proxy.get_log_snapshot();
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
    assert_eq!(proxy.get_log_snapshot()["entries"], json!([]));
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
    assert_eq!(forwarded.lock().unwrap().len(), 1);
    let log = proxy.get_log_snapshot();
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
        json!("CAD")
    );
    assert_eq!(
        custom.body["data"]["orderCreate"]["order"]["presentmentCurrencyCode"],
        json!("CAD")
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
            "orderId": fresh_order_id,
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
        json!([{ "field": ["orderId"], "message": "Order has already been cancelled", "code": "INVALID" }])
    );

    let log = proxy.get_log_snapshot();
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
fn order_customer_set_and_remove_error_paths_replay_captured_shapes() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../fixtures/conformance/local-runtime/2026-04/orders/orderCustomerSet-and-Remove-error-paths.json"
    ))
    .unwrap();
    let mut proxy = snapshot_proxy();

    let customer = proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/orders/orderCustomer-error-paths-customer-create.graphql"
        ),
        fixture["setup"]["customerCreate"]["variables"].clone(),
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
        fixture["setup"]["orderCreate"]["variables"].clone(),
    ));
    assert_eq!(order.body["data"]["orderCreate"]["userErrors"], json!([]));
    let order_id = order.body["data"]["orderCreate"]["order"]["id"].clone();

    let happy_set = proxy.process_request(json_graphql_request(
        include_str!("../../config/parity-requests/orders/orderCustomerSet-error-paths.graphql"),
        json!({ "orderId": order_id.clone(), "customerId": customer_id.clone() }),
    ));
    assert_eq!(happy_set.body, fixture["expected"]["happySet"]);

    let happy_remove = proxy.process_request(json_graphql_request(
        include_str!("../../config/parity-requests/orders/orderCustomerRemove-error-paths.graphql"),
        json!({ "orderId": order_id.clone() }),
    ));
    assert_eq!(happy_remove.body, fixture["expected"]["happyRemove"]);

    let unknown_order = proxy.process_request(json_graphql_request(
        include_str!("../../config/parity-requests/orders/orderCustomerSet-error-paths.graphql"),
        json!({ "orderId": "gid://shopify/Order/order-customer-missing", "customerId": customer_id.clone() }),
    ));
    assert_eq!(unknown_order.body, fixture["expected"]["unknownOrder"]);

    let unknown_customer = proxy.process_request(json_graphql_request(
        include_str!("../../config/parity-requests/orders/orderCustomerSet-error-paths.graphql"),
        json!({ "orderId": order_id, "customerId": "gid://shopify/Customer/order-customer-missing" }),
    ));
    assert_eq!(
        unknown_customer.body,
        fixture["expected"]["unknownCustomer"]
    );

    let company = proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/orders/orderCustomer-error-paths-company-create.graphql"
        ),
        fixture["setup"]["companyCreate"]["variables"].clone(),
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
        fixture["setup"]["b2bOrderCreate"]["variables"].clone(),
    ));
    let b2b_order_id = b2b_order.body["data"]["orderCreate"]["order"]["id"].clone();
    let b2b_not_permitted = proxy.process_request(json_graphql_request(
        include_str!("../../config/parity-requests/orders/orderCustomerSet-error-paths.graphql"),
        json!({ "orderId": b2b_order_id, "customerId": customer_id.clone() }),
    ));
    assert_eq!(
        b2b_not_permitted.body,
        fixture["expected"]["b2bNotPermitted"]
    );

    let cancelled_order = proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/orders/orderCancel-state-transitions-order-create.graphql"
        ),
        json!({
            "order": {
                "currency": "USD",
                "financialStatus": "PENDING",
                "email": "order-customer-cancelled@example.com",
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
        json!({ "orderId": cancelled_order_id }),
    ));
    assert_eq!(
        cancelled_remove.body,
        fixture["expected"]["cancelledRemove"]
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
        fixture["expected"]["readAfterPartialSuccess"]
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
        fixture["expected"]["readAfterNormalizedRemove"]
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
fn payment_reminder_send_malformed_gid_and_invalid_selection_ports_old_gleam_guards() {
    let malformed_fixture: Value = serde_json::from_str(include_str!(
        "../../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/payments/payment-reminder-send-malformed-gid.json"
    ))
    .unwrap();
    let shape_fixture: Value = serde_json::from_str(include_str!(
        "../../fixtures/conformance/local-runtime/2026-05/payments/payment-reminder-send-shape.json"
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
        shape_fixture["cases"]["invalidSelection"]["request"]["variables"].clone(),
    ));
    assert_eq!(invalid_selection.status, 200);
    assert_eq!(
        invalid_selection.body,
        shape_fixture["cases"]["invalidSelection"]["response"]
    );
}

#[test]
fn payment_reminder_send_eligibility_and_rate_limit_ports_old_gleam_guards() {
    let eligibility_fixture: Value = serde_json::from_str(include_str!(
        "../../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/payments/payment-reminder-send-eligibility.json"
    ))
    .unwrap();
    let additional_fixture: Value = serde_json::from_str(include_str!(
        "../../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/payments/payment-reminder-send-additional-guards.json"
    ))
    .unwrap();
    let mut proxy = snapshot_proxy();
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
}

#[test]
fn payment_reminder_send_local_only_order_guardrails_ported_from_gleam() {
    let mut proxy = snapshot_proxy();
    let query = include_str!("../../config/parity-requests/payments/payment-reminder-send.graphql");
    let cases = [
        (
            "gid://shopify/PaymentSchedule/123",
            json!({ "success": true, "userErrors": [] }),
        ),
        (
            "gid://shopify/PaymentSchedule/selling-plan",
            payment_reminder_error("Order has a selling plan"),
        ),
        (
            "gid://shopify/PaymentSchedule/capture",
            payment_reminder_error("Order has capture at fulfillment terms"),
        ),
        (
            "gid://shopify/PaymentSchedule/missing-email",
            payment_reminder_error("Order does not have a contact email"),
        ),
        (
            "gid://shopify/PaymentSchedule/collection",
            payment_reminder_error("Payment collection request has not been sent"),
        ),
        (
            "gid://shopify/PaymentSchedule/paid",
            payment_reminder_error("Payment schedule is already completed"),
        ),
        (
            "gid://shopify/PaymentSchedule/current",
            payment_reminder_error("Payment reminder could not be sent"),
        ),
        (
            "gid://shopify/PaymentSchedule/cancelled",
            payment_reminder_error("Payment reminder could not be sent"),
        ),
        (
            "gid://shopify/PaymentSchedule/paid-owner",
            payment_reminder_error("Payment schedule is already completed"),
        ),
        (
            "gid://shopify/PaymentSchedule/completed-draft",
            payment_reminder_error("Payment schedule is not for an Order"),
        ),
    ];

    for (schedule_id, expected_payload) in cases {
        let response = proxy.process_request(json_graphql_request(
            query,
            json!({ "paymentScheduleId": schedule_id }),
        ));
        assert_eq!(response.status, 200, "{schedule_id}");
        assert_eq!(
            response.body,
            json!({ "data": { "paymentReminderSend": expected_payload } }),
            "{schedule_id}"
        );
    }

    let first_rate_limited_schedule = proxy.process_request(json_graphql_request(
        query,
        json!({ "paymentScheduleId": "gid://shopify/PaymentSchedule/rate-limit" }),
    ));
    assert_eq!(first_rate_limited_schedule.status, 200);
    assert_eq!(
        first_rate_limited_schedule.body,
        json!({ "data": { "paymentReminderSend": { "success": true, "userErrors": [] } } })
    );

    let second_send = proxy.process_request(json_graphql_request(
        query,
        json!({ "paymentScheduleId": "gid://shopify/PaymentSchedule/rate-limit" }),
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

#[test]
fn payment_customization_local_runtime_ports_old_gleam_create_activation_update_readback_helpers() {
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

    let invalid_metafield = proxy.process_request(json_graphql_request(
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

    let before = proxy.process_request(json_graphql_request(
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
    let read_after_rejected_update = proxy.process_request(json_graphql_request(
        read_query,
        json!({ "id": customization_id }),
    ));
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
    let read_after_rejected_metafield_update = proxy.process_request(json_graphql_request(
        read_query,
        json!({ "id": customization_id }),
    ));
    assert_eq!(read_after_rejected_metafield_update.status, 200);
    assert_eq!(
        read_after_rejected_metafield_update.body["data"]["paymentCustomization"]["metafield"]
            ["value"],
        json!("baz")
    );

    let accepted_equivalent_handle = proxy.process_request(json_graphql_request(
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
    let read_after_rejected_blank_title = proxy.process_request(json_graphql_request(
        read_query,
        json!({ "id": customization_id }),
    ));
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
fn payment_terms_create_update_guardrails_port_old_gleam_helper_edges() {
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

    for reference_id in [
        "gid://shopify/Order/closed",
        "gid://shopify/Order/cancelled-unpaid",
        "gid://shopify/DraftOrder/paid-status",
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

    let multiple_schedules = proxy.process_request(json_graphql_request(
        create_query,
        json!({
            "referenceId": "gid://shopify/Order/637",
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
                "field": ["base"],
                "message": "Cannot create payment terms with multiple schedules.",
                "code": "PAYMENT_TERMS_CREATION_UNSUCCESSFUL"
            }]
        })
    );

    let unknown_order = proxy.process_request(json_graphql_request(
        create_query,
        json!({ "referenceId": "gid://shopify/Order/123", "attrs": net_attrs.clone() }),
    ));
    assert_eq!(
        unknown_order.body["data"]["paymentTermsCreate"]["userErrors"][0],
        json!({
            "field": Value::Null,
            "message": "Cannot find the specific Order with id 123.",
            "code": "PAYMENT_TERMS_CREATION_UNSUCCESSFUL"
        })
    );

    let unknown_draft = proxy.process_request(json_graphql_request(
        create_query,
        json!({ "referenceId": "gid://shopify/DraftOrder/999999", "attrs": net_attrs.clone() }),
    ));
    assert_eq!(
        unknown_draft.body["data"]["paymentTermsCreate"]["userErrors"][0],
        json!({
            "field": Value::Null,
            "message": "Cannot find the specific Draft order with id 999999.",
            "code": "PAYMENT_TERMS_CREATION_UNSUCCESSFUL"
        })
    );

    let unknown_template = proxy.process_request(json_graphql_request(
        create_query,
        json!({
            "referenceId": "gid://shopify/Order/637",
            "attrs": {
                "paymentTermsTemplateId": "gid://shopify/PaymentTermsTemplate/9999",
                "paymentSchedules": [{ "issuedAt": "2026-01-01T00:00:00Z" }]
            }
        }),
    ));
    assert_eq!(
        unknown_template.body["data"]["paymentTermsCreate"]["userErrors"][0]["message"],
        json!("Could not find payment terms template.")
    );
    assert_eq!(
        unknown_template.body["data"]["paymentTermsCreate"]["paymentTerms"],
        Value::Null
    );

    let missing_template = proxy.process_request(json_graphql_request(
        create_query,
        json!({
            "referenceId": "gid://shopify/Order/637",
            "attrs": { "paymentSchedules": [{ "issuedAt": "2026-01-01T00:00:00Z" }] }
        }),
    ));
    assert_eq!(
        missing_template.body["data"]["paymentTermsCreate"]["userErrors"][0],
        json!({
            "field": ["paymentTermsAttributes", "paymentTermsTemplateId"],
            "message": "Payment terms template is required.",
            "code": "REQUIRED"
        })
    );

    let fixed_without_due = proxy.process_request(json_graphql_request(
        create_query,
        json!({
            "referenceId": "gid://shopify/Order/637",
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
            "referenceId": "gid://shopify/Order/637",
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
            "referenceId": "gid://shopify/Order/637",
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
        json!({ "input": { "paymentTermsId": "gid://shopify/PaymentTerms/999999", "paymentTermsAttributes": net_attrs.clone() } }),
    ));
    assert_eq!(
        missing_update.body["data"]["paymentTermsUpdate"]["userErrors"][0],
        json!({
            "field": Value::Null,
            "message": "Payment terms do not exist",
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

    let draft_update = proxy.process_request(json_graphql_request(
        update_query,
        json!({ "input": { "paymentTermsId": "gid://shopify/PaymentTerms/draft-update", "paymentTermsAttributes": net_attrs.clone() } }),
    ));
    assert_eq!(
        draft_update.body["data"]["paymentTermsUpdate"]["paymentTerms"]["id"],
        json!("gid://shopify/PaymentTerms/draft-update")
    );
    assert_eq!(
        draft_update.body["data"]["paymentTermsUpdate"]["userErrors"],
        json!([])
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

    for (reference_id, attrs, expected_name, expected_type, expected_due_days, schedule_count) in [
        (
            "gid://shopify/DraftOrder/fixed-template",
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
            "gid://shopify/DraftOrder/net-7-template",
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
            "gid://shopify/DraftOrder/fulfillment-template",
            json!({
                "paymentTermsTemplateId": "gid://shopify/PaymentTermsTemplate/9"
            }),
            "Due on fulfillment",
            "FULFILLMENT",
            Value::Null,
            0_usize,
        ),
    ] {
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
    }

    let log = proxy.get_log_snapshot();
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
        attrs,
        expected_name,
        expected_type,
        expected_due_days,
        schedule_count,
    ) in [
        (
            "gid://shopify/PaymentTerms/fixed-update",
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
            "gid://shopify/PaymentTerms/net-7-update",
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
            "gid://shopify/PaymentTerms/fulfillment-update",
            json!({
                "paymentTermsTemplateId": "gid://shopify/PaymentTermsTemplate/9"
            }),
            "Due on fulfillment",
            "FULFILLMENT",
            Value::Null,
            0_usize,
        ),
    ] {
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
    let create_fixture: Value = serde_json::from_str(include_str!(
        "../../fixtures/conformance/local-runtime/2026-04/payments/payment-terms-create-on-order.json"
    ))
    .unwrap();
    let cascade_fixture: Value = serde_json::from_str(include_str!(
        "../../fixtures/conformance/local-runtime/2026-04/payments/payment-terms-delete-owner-cascade.json"
    ))
    .unwrap();
    let mut proxy = snapshot_proxy();

    let order_create = proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/payments/payment-terms-create-on-order-create.graphql"
        ),
        create_fixture["paymentTermsCreateOnOrder"]["orderCreate"]["variables"].clone(),
    ));
    assert_eq!(
        order_create.body,
        create_fixture["paymentTermsCreateOnOrder"]["expected"]["orderCreate"]
    );

    let create_terms = proxy.process_request(json_graphql_request(
        include_str!("../../config/parity-requests/payments/payment-terms-lifecycle-create.graphql"),
        json!({
            "referenceId": order_create.body["data"]["orderCreate"]["order"]["id"].clone(),
            "attrs": create_fixture["paymentTermsCreateOnOrder"]["paymentTermsCreate"]["variables"]["attrs"].clone()
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
        json!({ "amount": "57.00", "currencyCode": "CAD" })
    );

    let multiple = proxy.process_request(json_graphql_request(
        include_str!("../../config/parity-requests/payments/payment-terms-create-on-order-multiple.graphql"),
        json!({
            "referenceId": order_create.body["data"]["orderCreate"]["order"]["id"].clone(),
            "attrs": create_fixture["paymentTermsCreateOnOrder"]["multipleSchedules"]["variables"]["attrs"].clone()
        }),
    ));
    assert_eq!(
        multiple.body,
        create_fixture["paymentTermsCreateOnOrder"]["expected"]["multiple"]
    );

    let missing_update = proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/payments/payment-terms-lifecycle-update.graphql"
        ),
        create_fixture["paymentTermsCreateOnOrder"]["missingUpdate"]["variables"].clone(),
    ));
    assert_eq!(
        missing_update.body["data"]["paymentTermsUpdate"]["userErrors"][0]["code"],
        json!("PAYMENT_TERMS_UPDATE_UNSUCCESSFUL")
    );
    assert_eq!(
        missing_update.body["data"]["paymentTermsUpdate"]["userErrors"][0]["message"],
        json!("Payment terms do not exist")
    );

    let draft_terms = proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/payments/payment-terms-lifecycle-create.graphql"
        ),
        json!({
            "referenceId": cascade_fixture["draft"]["owner"]["id"].clone(),
            "attrs": cascade_fixture["draft"]["paymentTermsCreate"]["variables"]["attrs"].clone()
        }),
    ));
    assert_eq!(
        draft_terms.body["data"]["paymentTermsCreate"]["userErrors"],
        json!([])
    );
    let draft_terms_id =
        draft_terms.body["data"]["paymentTermsCreate"]["paymentTerms"]["id"].clone();

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
        json!({ "id": cascade_fixture["draft"]["owner"]["id"].clone() }),
    ));
    assert_eq!(
        draft_read.body["data"]["draftOrder"]["paymentTerms"],
        Value::Null
    );

    let cascade_order_create = proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/payments/payment-terms-create-on-order-create.graphql"
        ),
        cascade_fixture["order"]["orderCreate"]["variables"].clone(),
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
            "attrs": cascade_fixture["order"]["paymentTermsCreate"]["variables"]["attrs"].clone()
        }),
    ));
    assert_eq!(
        cascade_order_terms.body["data"]["paymentTermsCreate"]["userErrors"],
        json!([])
    );
    let cascade_order_terms_id =
        cascade_order_terms.body["data"]["paymentTermsCreate"]["paymentTerms"]["id"].clone();

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
        cascade_fixture["order"]["missingDelete"]["variables"].clone(),
    ));
    assert_eq!(
        missing_delete.body["data"]["paymentTermsDelete"]["userErrors"][0]["message"],
        json!("Payment terms do not exist")
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
        json!({"input": {"id": create.body["data"]["orderCreate"]["order"]["id"].clone(), "parentTransactionId": create.body["data"]["orderCreate"]["order"]["transactions"][0]["id"].clone(), "amount": "10.00", "currency": "CAD", "finalCapture": false}}),
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
        json!({"input": {"id": create.body["data"]["orderCreate"]["order"]["id"].clone(), "parentTransactionId": create.body["data"]["orderCreate"]["order"]["transactions"][0]["id"].clone(), "amount": "15.00", "currency": "CAD", "finalCapture": true}}),
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

    let log = proxy.get_log_snapshot();
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
fn money_bag_presentment_replays_order_payment_refund_and_edit_shapes() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../fixtures/conformance/local-runtime/2026-05/orders/money-bag-presentment-parity.json"
    ))
    .unwrap();
    let mut proxy = snapshot_proxy();

    let single_create = proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/orders/money-bag-presentment-single-create.graphql"
        ),
        fixture["singleCurrencyCreate"]["variables"].clone(),
    ));
    assert_eq!(
        single_create.body,
        fixture["singleCurrencyCreate"]["expected"]
    );
    let order_id = single_create.body["data"]["orderCreate"]["order"]["id"].clone();

    let multi_create = proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/orders/money-bag-presentment-multi-create.graphql"
        ),
        fixture["multiCurrencyCreate"]["variables"].clone(),
    ));
    assert_eq!(
        multi_create.body,
        fixture["multiCurrencyCreate"]["expected"]
    );

    let mark_as_paid = proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/orders/money-bag-presentment-mark-as-paid.graphql"
        ),
        json!({"input": {"id": order_id.clone()}}),
    ));
    assert_eq!(mark_as_paid.body, fixture["markAsPaid"]["expected"]);

    let refund = proxy.process_request(json_graphql_request(
        include_str!("../../config/parity-requests/orders/money-bag-presentment-refund.graphql"),
        json!({"input": {"orderId": order_id.clone(), "allowOverRefunding": true, "transactions": [{"amount": "5.00", "gateway": "manual", "kind": "REFUND", "orderId": order_id.clone()}]}}),
    ));
    assert_eq!(refund.body, fixture["refund"]["expected"]);

    let edit_begin = proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/orders/money-bag-presentment-order-edit-begin.graphql"
        ),
        json!({"id": order_id}),
    ));
    assert_eq!(edit_begin.body, fixture["orderEditBegin"]["expected"]);
    let calculated_order_id =
        edit_begin.body["data"]["orderEditBegin"]["calculatedOrder"]["id"].clone();

    let edit_commit = proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/orders/money-bag-presentment-order-edit-commit.graphql"
        ),
        json!({"id": calculated_order_id}),
    ));
    assert_eq!(edit_commit.body, fixture["orderEditCommit"]["expected"]);
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
        unknown_complete.body["data"]["draftOrderComplete"]["draftOrder"],
        Value::Null
    );
    assert_eq!(
        unknown_complete.body["data"]["draftOrderComplete"]["userErrors"][0]["field"],
        json!(["paymentGatewayId"])
    );
}

#[test]
fn draft_order_complete_dispatches_by_root_for_ordinary_operation_names() {
    let mut proxy = snapshot_proxy();

    let create = proxy.process_request(json_graphql_request(
        r#"
            mutation MakeDraft {
              draftOrderCreate(
                input: {
                  email: "complete-readback@example.test"
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
fn registered_orders_stage_locally_gap_returns_shopify_shaped_200_and_logs_raw_body() {
    let mut proxy = snapshot_proxy();
    let query = r#"
        mutation CreateRefund($input: RefundInput!) {
          refundCreate(input: $input) {
            refund {
              id
            }
            userErrors {
              field
              message
              code
            }
          }
        }
    "#;
    let variables = json!({
        "input": {
            "orderId": "gid://shopify/Order/not-modeled"
        }
    });

    let response = proxy.process_request(json_graphql_request(query, variables));

    assert_eq!(response.status, 200);
    assert_eq!(response.body["errors"], Value::Null);
    assert_eq!(response.body["data"]["refundCreate"]["refund"], Value::Null);
    assert_eq!(
        response.body["data"]["refundCreate"]["userErrors"][0]["message"],
        json!("Local staging for refundCreate is not implemented for this request shape")
    );

    let log = proxy.get_log_snapshot();
    assert_eq!(log["entries"][0]["operationName"], Value::Null);
    assert_eq!(log["entries"][0]["status"], json!("failed"));
    assert_eq!(
        log["entries"][0]["interpreted"]["capability"],
        json!({
            "operationName": "refundCreate",
            "domain": "orders",
            "execution": "stage-locally"
        })
    );
    assert!(log["entries"][0]["rawBody"]
        .as_str()
        .is_some_and(|body| body.contains("CreateRefund")));
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

    let state = proxy.get_state_snapshot();
    assert_eq!(
        state["stagedState"]["draftOrders"]["gid://shopify/DraftOrder/1"]["data"]
            ["__draftProxyInvoiceSend"],
        fixture["validSend"]["state"]["stagedState"]["draftOrders"]["gid://shopify/DraftOrder/1"]
            ["data"]["__draftProxyInvoiceSend"]
    );

    let log = proxy.get_log_snapshot();
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
fn remaining_order_fixture_backed_edges_replay_without_passthrough_logs() {
    let fulfillment_fixture: Value = serde_json::from_str(include_str!(
        "../../fixtures/conformance/local-runtime/2025-01/orders/fulfillment-state-preconditions.json"
    ))
    .unwrap();
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

    let mut fulfillment_proxy = snapshot_proxy();
    let cancel = fulfillment_proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/orders/fulfillment-state-preconditions-cancel.graphql"
        ),
        fulfillment_fixture["cancelAlreadyCancelled"]["variables"].clone(),
    ));
    assert_eq!(
        cancel.body,
        fulfillment_fixture["cancelAlreadyCancelled"]["response"]
    );
    assert_eq!(fulfillment_proxy.get_log_snapshot()["entries"], json!([]));

    let tracking = fulfillment_proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/orders/fulfillment-state-preconditions-tracking.graphql"
        ),
        fulfillment_fixture["trackingAlreadyCancelled"]["variables"].clone(),
    ));
    assert_eq!(
        tracking.body,
        fulfillment_fixture["trackingAlreadyCancelled"]["response"]
    );

    let delivered = fulfillment_proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/orders/fulfillment-state-preconditions-cancel.graphql"
        ),
        fulfillment_fixture["cancelDelivered"]["variables"].clone(),
    ));
    assert_eq!(
        delivered.body,
        fulfillment_fixture["cancelDelivered"]["response"]
    );

    let happy_tracking = fulfillment_proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/orders/fulfillment-state-preconditions-tracking.graphql"
        ),
        fulfillment_fixture["trackingHappyPath"]["variables"].clone(),
    ));
    assert_eq!(
        happy_tracking.body["data"]["fulfillmentTrackingInfoUpdate"]["fulfillment"],
        Value::Null
    );
    assert_eq!(
        happy_tracking.body["data"]["fulfillmentTrackingInfoUpdate"]["userErrors"][0]["code"],
        "NOT_IMPLEMENTED"
    );

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

    let log = proxy.get_log_snapshot();
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
    assert_eq!(
        proxy.get_log_snapshot()["entries"]
            .as_array()
            .unwrap()
            .len(),
        2
    );

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
fn order_edit_existing_downstream_reads_track_add_and_zero_removal_modes() {
    let happy_fixture: Value = serde_json::from_str(include_str!(
        "../../fixtures/conformance/very-big-test-store.myshopify.com/2026-04/orders/order-edit-existing-order-happy-path.json"
    ))
    .unwrap();
    let zero_fixture: Value = serde_json::from_str(include_str!(
        "../../fixtures/conformance/very-big-test-store.myshopify.com/2026-04/orders/order-edit-existing-order-zero-removal.json"
    ))
    .unwrap();

    let mut add_proxy = snapshot_proxy();
    let add = add_proxy.process_request(json_graphql_request(
        include_str!("../../config/parity-requests/orders/orderEditExistingWorkflow-addVariant.graphql"),
        json!({"id": "gid://shopify/CalculatedOrder/1", "variantId": "gid://shopify/ProductVariant/46789254021353", "quantity": 1, "locationId": "gid://shopify/Location/68509171945", "allowDuplicates": false}),
    ));
    assert_eq!(add.body, happy_fixture["addVariant"]["response"]);
    let add_downstream = add_proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/orders/orderEditExistingWorkflow-downstream-read.graphql"
        ),
        json!({"id": "gid://shopify/Order/6834565087465"}),
    ));
    assert_eq!(
        add_downstream.body["data"]["order"]["lineItems"]["nodes"][2]["currentQuantity"],
        json!(1)
    );

    let mut zero_proxy = snapshot_proxy();
    let set_zero = zero_proxy.process_request(json_graphql_request(
        include_str!("../../config/parity-requests/orders/orderEditExistingWorkflow-setQuantity.graphql"),
        json!({"id": "gid://shopify/CalculatedOrder/1", "lineItemId": "gid://shopify/LineItem/1", "quantity": 0, "restock": true}),
    ));
    assert_eq!(set_zero.body, zero_fixture["setZero"]["response"]);
    let zero_downstream = zero_proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/orders/orderEditExistingWorkflow-downstream-read.graphql"
        ),
        json!({"id": "gid://shopify/Order/6834565087465"}),
    ));
    assert_eq!(
        zero_downstream.body["data"]["order"]["lineItems"]["nodes"][2]["currentQuantity"],
        json!(0)
    );
}

#[test]
fn order_edit_existing_validation_replays_invalid_and_duplicate_variant_shapes() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../fixtures/conformance/very-big-test-store.myshopify.com/2026-04/orders/order-edit-existing-order-validation.json"
    ))
    .unwrap();
    let mut proxy = snapshot_proxy();

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
    assert_eq!(invalid_variant.body, fixture["invalidVariant"]["response"]);

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
    assert_eq!(
        duplicate_variant.body,
        fixture["duplicateVariant"]["response"]
    );
}

#[test]
fn customer_payment_methods_remote_create_validation_ports_old_gleam_guards() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../fixtures/conformance/local-runtime/2026-04/payments/customer-payment-method-remote-create-validation.json"
    ))
    .unwrap();
    let mut proxy = snapshot_proxy();

    let seed = proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/payments/customer-payment-method-remote-create-validation-seed.graphql"
        ),
        json!({}),
    ));
    assert_eq!(seed.body, fixture["operations"]["seedCustomer"]["response"]);

    let stripe_blank = proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/payments/customer-payment-method-remote-create-validation-stripe.graphql"
        ),
        json!({}),
    ));
    assert_eq!(stripe_blank.status, 200);
    assert_eq!(
        stripe_blank.body,
        fixture["operations"]["stripeBlankCustomerId"]["response"]
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
        fixture["operations"]["paypalBlankBillingAgreementId"]["response"]
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
        fixture["operations"]["twoGatewayObjects"]["response"]
    );
}

#[test]
fn customer_payment_methods_replay_shop_pay_guard_shapes() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../fixtures/conformance/local-runtime/2026-04/payments/customer-payment-method-shop-pay-guards.json"
    ))
    .unwrap();
    let mut proxy = snapshot_proxy();

    let response = proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/payments/customer-payment-method-shop-pay-guards.graphql"
        ),
        fixture["variables"].clone(),
    ));

    assert_eq!(response.status, 200);
    assert_eq!(response.body, fixture["expected"]["primary"]);
}

#[test]
fn customer_payment_methods_replay_local_staging_and_validation_shapes() {
    let lifecycle: Value = serde_json::from_str(include_str!(
        "../../fixtures/conformance/local-runtime/2026-04/payments/customer-payment-method-local-staging.json"
    ))
    .unwrap();
    let validation: Value = serde_json::from_str(include_str!(
        "../../fixtures/conformance/local-runtime/2026-04/payments/customer-payment-method-credit-card-create-validation.json"
    ))
    .unwrap();
    let mut proxy = snapshot_proxy();

    let primary = proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/payments/customer-payment-method-local-staging.graphql"
        ),
        lifecycle["variables"].clone(),
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
            "billingAddress": lifecycle["variables"]["billingAddress"].clone(),
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
        validation["variables"]["blankBilling"].clone(),
    ));
    assert_eq!(blank.body, validation["expected"]["blankBilling"]);

    let missing_session = proxy.process_request(json_graphql_request(
        include_str!("../../config/parity-requests/payments/customer-payment-method-credit-card-create-validation-missing-session.graphql"),
        validation["variables"]["missingSession"].clone(),
    ));
    assert_eq!(
        missing_session.body["data"]["customerPaymentMethodCreditCardCreate"]["userErrors"][0]
            ["code"],
        json!("BLANK")
    );

    let processing = proxy.process_request(json_graphql_request(
        include_str!("../../config/parity-requests/payments/customer-payment-method-credit-card-create-validation-processing.graphql"),
        validation["variables"]["processing"].clone(),
    ));
    assert_eq!(processing.body, validation["expected"]["processing"]);

    let success = proxy.process_request(json_graphql_request(
        include_str!("../../config/parity-requests/payments/customer-payment-method-credit-card-create-validation-success.graphql"),
        validation["variables"]["success"].clone(),
    ));
    assert_eq!(
        success.body["data"]["customerPaymentMethodCreditCardCreate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        success.body["data"]["customerPaymentMethodCreditCardCreate"]["customerPaymentMethod"]
            ["instrument"]["billingAddress"],
        validation["expected"]["success"]["data"]["customerPaymentMethodCreditCardCreate"]
            ["customerPaymentMethod"]["instrument"]["billingAddress"]
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
        validation["expected"]["readAfter"]["data"]["customerPaymentMethod"]["instrument"]
            ["billingAddress"]
    );
}

#[test]
fn customer_payment_method_update_and_revoke_tail_helpers_ported_from_gleam() {
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
            "revokedAt": "2024-01-01T00:00:01.000Z",
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

#[test]
fn order_return_lifecycle_and_reverse_logistics_replay_local_runtime_shapes() {
    let mut proxy = snapshot_proxy();

    let create = proxy.process_request(json_graphql_request(
        include_str!("../../config/parity-requests/orders/return-create-local-staging.graphql"),
        json!({
            "returnInput": {
                "orderId": "gid://shopify/Order/return-flow",
                "returnLineItems": [{
                    "fulfillmentLineItemId": "gid://shopify/FulfillmentLineItem/return-flow",
                    "quantity": 1,
                    "returnReason": "UNWANTED",
                    "returnReasonNote": "Changed mind"
                }]
            }
        }),
    ));
    assert_eq!(create.status, 200);
    assert_eq!(
        create.body["data"]["returnCreate"]["return"]["id"],
        json!("gid://shopify/Return/2")
    );
    assert_eq!(
        create.body["data"]["returnCreate"]["return"]["returnLineItems"]["nodes"][0]
            ["fulfillmentLineItem"]["lineItem"]["title"],
        json!("Return flow item")
    );

    let close = proxy.process_request(json_graphql_request(
        include_str!("../../config/parity-requests/orders/return-close-local-staging.graphql"),
        json!({ "id": "gid://shopify/Return/2" }),
    ));
    assert_eq!(
        close.body["data"]["returnClose"]["return"],
        json!({
            "id": "gid://shopify/Return/2",
            "status": "CLOSED",
            "closedAt": "2024-01-01T00:00:03.000Z"
        })
    );

    let cancel_read = proxy.process_request(json_graphql_request(
        include_str!("../../config/parity-requests/orders/return-read-local-staging.graphql"),
        json!({
            "id": "gid://shopify/Return/2",
            "orderId": "gid://shopify/Order/return-flow"
        }),
    ));
    assert_eq!(
        cancel_read.body["data"]["order"]["returns"]["nodes"][0]["id"],
        json!("gid://shopify/Return/2")
    );

    let reverse_request = proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/orders/return-request-reverse-local-staging.graphql"
        ),
        json!({
            "input": {
                "orderId": "gid://shopify/Order/return-flow",
                "returnLineItems": [{
                    "fulfillmentLineItemId": "gid://shopify/FulfillmentLineItem/return-flow",
                    "quantity": 1,
                    "returnReason": "OTHER"
                }]
            }
        }),
    ));
    assert_eq!(
        reverse_request.body["data"]["returnRequest"]["return"]["status"],
        json!("REQUESTED")
    );
    let reverse_return_id = reverse_request.body["data"]["returnRequest"]["return"]["id"].clone();

    let approve = proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/orders/return-approve-request-local-staging.graphql"
        ),
        json!({ "input": { "id": reverse_return_id.clone() } }),
    ));
    assert_eq!(
        approve.body["data"]["returnApproveRequest"]["return"]["reverseFulfillmentOrders"]["nodes"]
            [0]["lineItems"]["nodes"][0]["remainingQuantity"],
        json!(1)
    );
    let reverse_fulfillment_order_id = approve.body["data"]["returnApproveRequest"]["return"]
        ["reverseFulfillmentOrders"]["nodes"][0]["id"]
        .clone();
    let reverse_fulfillment_order_line_item_id = approve.body["data"]["returnApproveRequest"]
        ["return"]["reverseFulfillmentOrders"]["nodes"][0]["lineItems"]["nodes"][0]["id"]
        .clone();

    let reverse_delivery = proxy.process_request(json_graphql_request(
        include_str!("../../config/parity-requests/orders/reverse-delivery-create-with-shipping-local-staging.graphql"),
        json!({
            "reverseFulfillmentOrderId": reverse_fulfillment_order_id.clone(),
            "reverseDeliveryLineItems": [{
                "reverseFulfillmentOrderLineItemId": reverse_fulfillment_order_line_item_id,
                "quantity": 1
            }],
            "trackingInput": {
                "number": "TRACK-1",
                "url": "https://tracking.example/1",
                "company": "Example Carrier"
            },
            "labelInput": { "fileUrl": "https://labels.example/return.pdf" }
        }),
    ));
    assert_eq!(
        reverse_delivery.body["data"]["reverseDeliveryCreateWithShipping"]["reverseDelivery"]
            ["deliverable"]["tracking"]["number"],
        json!("TRACK-1")
    );
    let reverse_delivery_id = reverse_delivery.body["data"]["reverseDeliveryCreateWithShipping"]
        ["reverseDelivery"]["id"]
        .clone();
    let downstream = proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/orders/return-reverse-logistics-read-local-staging.graphql"
        ),
        json!({
            "returnId": reverse_return_id,
            "orderId": "gid://shopify/Order/return-flow",
            "reverseDeliveryId": reverse_delivery_id.clone(),
            "reverseFulfillmentOrderId": reverse_fulfillment_order_id
        }),
    ));
    assert_eq!(
        downstream.body["data"]["reverseFulfillmentOrder"]["reverseDeliveries"]["nodes"][0]["id"],
        reverse_delivery_id
    );
}

#[test]
fn order_return_recorded_reverse_logistics_and_shipping_fee_use_staged_store_reads() {
    let reverse_fixture: Value = serde_json::from_str(include_str!(
        "../../fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/orders/return-reverse-logistics-recorded.json"
    ))
    .unwrap();
    let shipping_fixture: Value = serde_json::from_str(include_str!(
        "../../fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/orders/return-shipping-fee-recorded.json"
    ))
    .unwrap();
    let mut proxy = snapshot_proxy();

    let request = proxy.process_request(json_graphql_request(
        include_str!("../../config/parity-requests/orders/return-request-recorded.graphql"),
        reverse_fixture["returnRequest"]["variables"].clone(),
    ));
    assert_eq!(request.status, 200);
    assert_eq!(
        request.body["data"]["returnRequest"]["return"]["status"],
        json!("REQUESTED")
    );
    let return_id = request.body["data"]["returnRequest"]["return"]["id"].clone();
    let order_id = reverse_fixture["returnRequest"]["variables"]["input"]["orderId"].clone();

    let approve = proxy.process_request(json_graphql_request(
        include_str!("../../config/parity-requests/orders/return-approve-request-recorded.graphql"),
        json!({ "input": { "id": return_id.clone() } }),
    ));
    let reverse_fulfillment_order_id = approve.body["data"]["returnApproveRequest"]["return"]
        ["reverseFulfillmentOrders"]["nodes"][0]["id"]
        .clone();
    assert_eq!(
        approve.body["data"]["returnApproveRequest"]["return"]["status"],
        json!("OPEN")
    );

    let delivery_create = proxy.process_request(json_graphql_request(
        include_str!("../../config/parity-requests/orders/reverse-delivery-create-with-shipping-recorded.graphql"),
        json!({
            "reverseFulfillmentOrderId": reverse_fulfillment_order_id.clone(),
            "reverseDeliveryLineItems": [],
            "trackingInput": reverse_fixture["reverseDeliveryCreate"]["variables"]["trackingInput"].clone(),
            "labelInput": reverse_fixture["reverseDeliveryCreate"]["variables"]["labelInput"].clone()
        }),
    ));
    let reverse_delivery_id = delivery_create.body["data"]["reverseDeliveryCreateWithShipping"]
        ["reverseDelivery"]["id"]
        .clone();
    assert_eq!(
        delivery_create.body["data"]["reverseDeliveryCreateWithShipping"]["reverseDelivery"]
            ["deliverable"]["__typename"],
        json!("ReverseDeliveryShippingDeliverable")
    );

    let delivery_update = proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/orders/reverse-delivery-shipping-update-recorded.graphql"
        ),
        json!({
            "reverseDeliveryId": reverse_delivery_id.clone(),
            "trackingInput": reverse_fixture["reverseDeliveryUpdate"]["variables"]["trackingInput"].clone()
        }),
    ));
    assert_eq!(
        delivery_update.body["data"]["reverseDeliveryShippingUpdate"]["reverseDelivery"]["id"],
        reverse_delivery_id
    );

    let downstream = proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/orders/return-reverse-logistics-read-recorded.graphql"
        ),
        json!({
            "returnId": return_id.clone(),
            "orderId": order_id,
            "reverseDeliveryId": reverse_delivery_id,
            "reverseFulfillmentOrderId": reverse_fulfillment_order_id
        }),
    ));
    assert_eq!(downstream.body["data"]["return"]["id"], return_id);
    assert_eq!(
        downstream.body["data"]["order"]["returns"]["nodes"][0]["id"],
        return_id
    );

    let shipping_create = proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/orders/return-create-shipping-fee-recorded.graphql"
        ),
        shipping_fixture["returnCreate"]["variables"].clone(),
    ));
    let shipping_return_id = shipping_create.body["data"]["returnCreate"]["return"]["id"].clone();
    let shipping_order_id =
        shipping_fixture["returnCreate"]["variables"]["returnInput"]["orderId"].clone();
    assert_eq!(
        shipping_create.body["data"]["returnCreate"]["return"]["returnShippingFees"][0]
            ["amountSet"]["shopMoney"]["amount"],
        json!("7.50")
    );

    let shipping_downstream = proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/orders/return-shipping-fee-read-recorded.graphql"
        ),
        json!({ "returnId": shipping_return_id.clone(), "orderId": shipping_order_id }),
    ));
    assert_eq!(
        shipping_downstream.body["data"]["return"]["id"],
        shipping_return_id
    );
    assert_eq!(
        shipping_downstream.body["data"]["order"]["returns"]["nodes"][0]["returnShippingFees"][0]
            ["amountSet"]["shopMoney"]["amount"],
        json!("7.50")
    );
}

#[test]
fn order_return_state_preconditions_use_staged_statuses() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/orders/returnClose-Reopen-Cancel-state-preconditions.json"
    ))
    .unwrap();
    let request_query =
        include_str!("../../config/parity-requests/orders/return-request-recorded.graphql");
    let approve_query =
        include_str!("../../config/parity-requests/orders/return-approve-request-recorded.graphql");
    let decline_query = include_str!(
        "../../config/parity-requests/orders/return-decline-request-local-staging.graphql"
    );
    let close_query =
        include_str!("../../config/parity-requests/orders/return-close-state-precondition.graphql");
    let reopen_query = include_str!(
        "../../config/parity-requests/orders/return-reopen-state-precondition.graphql"
    );
    let cancel_query = include_str!(
        "../../config/parity-requests/orders/return-cancel-state-precondition.graphql"
    );
    let process_query =
        include_str!("../../config/parity-requests/orders/return-process-recorded.graphql");
    let mut proxy = snapshot_proxy();

    let requested = proxy.process_request(json_graphql_request(
        request_query,
        fixture["requestedCase"]["returnRequest"]["variables"].clone(),
    ));
    let requested_id = requested.body["data"]["returnRequest"]["return"]["id"].clone();
    let requested_close = proxy.process_request(json_graphql_request(
        close_query,
        json!({ "id": requested_id.clone() }),
    ));
    assert_eq!(
        requested_close.body["data"]["returnClose"]["return"],
        Value::Null
    );
    assert_eq!(
        requested_close.body["data"]["returnClose"]["userErrors"][0]["code"],
        json!("INVALID")
    );
    let requested_reopen = proxy.process_request(json_graphql_request(
        reopen_query,
        json!({ "id": requested_id }),
    ));
    assert_eq!(
        requested_reopen.body["data"]["returnReopen"]["userErrors"][0]["code"],
        json!("INVALID")
    );

    let cancelable = proxy.process_request(json_graphql_request(
        request_query,
        fixture["cancelableCase"]["returnRequest"]["variables"].clone(),
    ));
    let cancelable_id = cancelable.body["data"]["returnRequest"]["return"]["id"].clone();
    let cancelable_approve = proxy.process_request(json_graphql_request(
        approve_query,
        json!({ "input": { "id": cancelable_id } }),
    ));
    let cancelable_approved_id =
        cancelable_approve.body["data"]["returnApproveRequest"]["return"]["id"].clone();
    let cancel = proxy.process_request(json_graphql_request(
        cancel_query,
        json!({ "id": cancelable_approved_id }),
    ));
    assert_eq!(
        cancel.body["data"]["returnCancel"]["return"]["status"],
        json!("CANCELED")
    );
    let canceled_id = cancel.body["data"]["returnCancel"]["return"]["id"].clone();
    let cancel_again = proxy.process_request(json_graphql_request(
        cancel_query,
        json!({ "id": canceled_id }),
    ));
    assert_eq!(
        cancel_again.body["data"]["returnCancel"]["return"]["status"],
        json!("CANCELED")
    );

    let open_case = proxy.process_request(json_graphql_request(
        request_query,
        fixture["openCloseReopenCase"]["returnRequest"]["variables"].clone(),
    ));
    let open_id = open_case.body["data"]["returnRequest"]["return"]["id"].clone();
    let open_approve = proxy.process_request(json_graphql_request(
        approve_query,
        json!({ "input": { "id": open_id } }),
    ));
    let open_approved_id =
        open_approve.body["data"]["returnApproveRequest"]["return"]["id"].clone();
    let close = proxy.process_request(json_graphql_request(
        close_query,
        json!({ "id": open_approved_id }),
    ));
    assert_eq!(
        close.body["data"]["returnClose"]["return"]["status"],
        json!("CLOSED")
    );
    let closed_id = close.body["data"]["returnClose"]["return"]["id"].clone();
    let reopen = proxy.process_request(json_graphql_request(
        reopen_query,
        json!({ "id": closed_id }),
    ));
    assert_eq!(
        reopen.body["data"]["returnReopen"]["return"]["status"],
        json!("OPEN")
    );

    let declined = proxy.process_request(json_graphql_request(
        request_query,
        fixture["declinedCase"]["returnRequest"]["variables"].clone(),
    ));
    let declined_id = declined.body["data"]["returnRequest"]["return"]["id"].clone();
    let decline = proxy.process_request(json_graphql_request(
        decline_query,
        json!({ "input": { "id": declined_id, "declineReason": fixture["declineRequest"]["declineInput"]["declineReason"].clone(), "declineNote": fixture["declineRequest"]["declineInput"]["declineNote"].clone(), "notifyCustomer": fixture["declineRequest"]["declineInput"]["notifyCustomer"].clone() } }),
    ));
    assert_eq!(
        decline.body["data"]["returnDeclineRequest"]["return"]["status"],
        json!("DECLINED")
    );
    let declined_return_id = decline.body["data"]["returnDeclineRequest"]["return"]["id"].clone();
    let declined_close = proxy.process_request(json_graphql_request(
        close_query,
        json!({ "id": declined_return_id }),
    ));
    assert_eq!(
        declined_close.body["data"]["returnClose"]["userErrors"][0]["code"],
        json!("INVALID")
    );

    let processed = proxy.process_request(json_graphql_request(
        request_query,
        fixture["processedCase"]["returnRequest"]["variables"].clone(),
    ));
    let processed_id = processed.body["data"]["returnRequest"]["return"]["id"].clone();
    let processed_approve = proxy.process_request(json_graphql_request(
        approve_query,
        json!({ "input": { "id": processed_id } }),
    ));
    let process_return_id =
        processed_approve.body["data"]["returnApproveRequest"]["return"]["id"].clone();
    let process_line_id = processed_approve.body["data"]["returnApproveRequest"]["return"]
        ["returnLineItems"]["nodes"][0]["id"]
        .clone();
    let processed_result = proxy.process_request(json_graphql_request(
        process_query,
        json!({ "input": { "returnId": process_return_id, "returnLineItems": [{ "id": process_line_id, "quantity": 1 }], "notifyCustomer": true } }),
    ));
    assert_eq!(
        processed_result.body["data"]["returnProcess"]["return"]["status"],
        json!("OPEN")
    );
    let processed_return_id =
        processed_result.body["data"]["returnProcess"]["return"]["id"].clone();
    let processed_cancel = proxy.process_request(json_graphql_request(
        cancel_query,
        json!({ "id": processed_return_id }),
    ));
    assert_eq!(
        processed_cancel.body["data"]["returnCancel"]["userErrors"][0]["code"],
        json!("INVALID")
    );
}

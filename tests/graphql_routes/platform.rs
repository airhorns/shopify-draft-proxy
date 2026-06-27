use super::common::*;
use pretty_assertions::assert_eq;
use shopify_draft_proxy::proxy::Response;

fn fulfillment_order_hydrate_transport(
    orders: Vec<Value>,
) -> impl Fn(Request) -> Response + Send + Sync + 'static {
    let orders = Arc::new(Mutex::new(orders));
    move |request| {
        let body: Value = serde_json::from_str(&request.body).unwrap();
        let query = body["query"].as_str().unwrap_or_default();
        let requested_id = body["variables"]["id"].as_str().unwrap_or_default();
        let hydrated = orders.lock().unwrap().iter().find_map(|order| {
            order["fulfillmentOrders"]["nodes"]
                .as_array()
                .into_iter()
                .flatten()
                .find(|node| node["id"].as_str() == Some(requested_id))
                .map(|node| {
                    let mut node = node.clone();
                    node["order"] = json!({
                        "id": order["id"],
                        "name": order["name"],
                        "displayFulfillmentStatus": order["displayFulfillmentStatus"]
                    });
                    (order.clone(), node)
                })
        });
        let body = if query.contains("node(id: $id)") {
            let node = hydrated
                .as_ref()
                .map(|(_, node)| node.clone())
                .unwrap_or(Value::Null);
            json!({ "data": { "node": node } })
        } else {
            let order = hydrated
                .as_ref()
                .map(|(order, _)| order.clone())
                .unwrap_or(Value::Null);
            json!({ "data": { "order": order } })
        };
        Response {
            status: 200,
            headers: Default::default(),
            body,
        }
    }
}

fn fulfillment_order_order_fixture(
    order_id: &str,
    name: &str,
    fulfillment_order_id: &str,
    line_item_id: &str,
    quantity: i64,
    status: &str,
) -> Value {
    let supported_actions = if status == "SCHEDULED" {
        json!([{ "action": "MARK_AS_OPEN" }])
    } else {
        json!([
            { "action": "CREATE_FULFILLMENT" },
            { "action": "REPORT_PROGRESS" },
            { "action": "MOVE" },
            { "action": "HOLD" },
            { "action": "SPLIT" }
        ])
    };
    json!({
        "id": order_id,
        "name": name,
        "displayFulfillmentStatus": "UNFULFILLED",
        "fulfillmentOrders": {
            "nodes": [{
                "id": fulfillment_order_id,
                "status": status,
                "requestStatus": "UNSUBMITTED",
                "fulfillAt": "2026-06-15T11:00:00Z",
                "fulfillBy": null,
                "updatedAt": "2026-06-15T11:00:00Z",
                "supportedActions": supported_actions,
                "assignedLocation": {
                    "name": "Primary location",
                    "location": {
                        "id": "gid://shopify/Location/44",
                        "name": "Primary location"
                    }
                },
                "fulfillmentHolds": [],
                "lineItems": {
                    "nodes": [{
                        "id": line_item_id,
                        "totalQuantity": quantity,
                        "remainingQuantity": quantity,
                        "lineItem": {
                            "id": "gid://shopify/LineItem/998877",
                            "title": "Numeric fulfillment item",
                            "quantity": quantity,
                            "fulfillableQuantity": quantity
                        }
                    }]
                }
            }]
        }
    })
}

fn create_fulfillment_order_test_order(proxy: &mut DraftProxy, quantity: i64) -> (Value, Value) {
    let create_order = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateFulfillmentOrderTestOrder($order: OrderCreateOrderInput!) {
          orderCreate(order: $order) {
            order {
              id
              fulfillmentOrders(first: 5) {
                nodes {
                  id
                  status
                  requestStatus
                  lineItems(first: 5) {
                    nodes {
                      id
                      totalQuantity
                      remainingQuantity
                      lineItem { id title quantity fulfillableQuantity }
                    }
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
                "email": format!("fulfillment-order-{quantity}@example.test"),
                "lineItems": [{
                    "title": "Fulfillment order local staging",
                    "quantity": quantity,
                    "priceSet": { "shopMoney": { "amount": "10.00", "currencyCode": "USD" } }
                }]
            }
        }),
    ));
    assert_eq!(
        create_order.body["data"]["orderCreate"]["userErrors"],
        json!([])
    );
    let order = create_order.body["data"]["orderCreate"]["order"].clone();
    let fulfillment_order = order["fulfillmentOrders"]["nodes"][0].clone();
    (order, fulfillment_order)
}

#[test]
fn admin_platform_job_unknown_job_gid_returns_completed_job_shape() {
    let mut proxy = snapshot_proxy();
    let response = proxy.process_request(json_graphql_request(
        r#"
        query AdminPlatformUnknownJob($id: ID!) {
          job(id: $id) {
            __typename
            id
            done
            query { __typename }
          }
        }
        "#,
        json!({ "id": "gid://shopify/Job/0" }),
    ));

    assert_eq!(response.status, 200);
    assert_eq!(
        response.body,
        json!({
            "data": {
                "job": {
                    "__typename": "Job",
                    "id": "gid://shopify/Job/0",
                    "done": true,
                    "query": { "__typename": "QueryRoot" }
                }
            }
        })
    );
}

#[test]
fn admin_platform_job_non_job_gid_returns_resource_not_found_error() {
    let mut proxy = snapshot_proxy();
    let response = proxy.process_request(json_graphql_request(
        r#"
        query AdminPlatformNonJobGid($id: ID!) {
          poll: job(id: $id) {
            id
            done
            query { __typename }
          }
        }
        "#,
        json!({ "id": "gid://shopify/Product/0" }),
    ));

    assert_eq!(response.status, 200);
    assert_eq!(response.body["data"]["poll"], Value::Null);
    assert_eq!(
        response.body["errors"][0]["message"],
        json!("Invalid id: gid://shopify/Product/0")
    );
    assert_eq!(
        response.body["errors"][0]["extensions"]["code"],
        json!("RESOURCE_NOT_FOUND")
    );
    assert_eq!(response.body["errors"][0]["path"], json!(["poll"]));
}

#[test]
fn domain_id_resolves_from_shop_domains() {
    let mut proxy = snapshot_proxy();
    let dump = proxy.process_request(request_with_body("POST", "/__meta/dump", "{}"));
    let mut restored = dump.body.clone();
    restored["state"]["baseState"]["shop"] = json!({
        "id": "gid://shopify/Shop/restored",
        "name": "Restored shop",
        "myshopifyDomain": "restored-shop.myshopify.com",
        "primaryDomain": {
            "id": "gid://shopify/Domain/987654321",
            "host": "restored-shop.example",
            "url": "https://restored-shop.example",
            "sslEnabled": true
        },
        "currencyCode": "USD"
    });
    let restore = proxy.process_request(request_with_body(
        "POST",
        "/__meta/restore",
        &restored.to_string(),
    ));
    assert_eq!(restore.status, 200);

    let response = proxy.process_request(json_graphql_request(
        r#"
        query DomainFromRestoredShop($id: ID!) {
          domain(id: $id) {
            id
            host
            url
            sslEnabled
          }
        }
        "#,
        json!({ "id": "gid://shopify/Domain/987654321" }),
    ));

    assert_eq!(response.status, 200);
    assert_eq!(
        response.body["data"]["domain"],
        json!({
            "id": "gid://shopify/Domain/987654321",
            "host": "restored-shop.example",
            "url": "https://restored-shop.example",
            "sslEnabled": true
        })
    );

    let unknown = proxy.process_request(json_graphql_request(
        r#"
        query UnknownDomainFromRestoredShop($id: ID!) {
          domain(id: $id) { id host url sslEnabled }
        }
        "#,
        json!({ "id": "gid://shopify/Domain/404404404" }),
    ));
    assert_eq!(unknown.status, 200);
    assert_eq!(unknown.body["data"]["domain"], Value::Null);
}

#[test]
fn domain_id_resolves_after_live_hybrid_shop_hydration() {
    let upstream_calls = Arc::new(Mutex::new(Vec::<String>::new()));
    let captured_calls = Arc::clone(&upstream_calls);
    let mut proxy =
        configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport(move |request| {
            let body: Value =
                serde_json::from_str(&request.body).expect("upstream GraphQL body parses");
            let query = body["query"].as_str().unwrap_or_default().to_string();
            captured_calls.lock().unwrap().push(query);
            Response {
                status: 200,
                headers: Default::default(),
                body: json!({
                    "data": {
                        "shop": {
                            "id": "gid://shopify/Shop/live",
                            "name": "Live hydrated shop",
                            "myshopifyDomain": "live-hydrated.myshopify.com",
                            "primaryDomain": {
                                "id": "gid://shopify/Domain/222333444",
                                "host": "live-hydrated.example",
                                "url": "https://live-hydrated.example",
                                "sslEnabled": true
                            },
                            "currencyCode": "CAD"
                        }
                    }
                }),
            }
        });

    let hydrate = proxy.process_request(json_graphql_request(
        r#"
        query HydrateShopDomain {
          shop {
            id
            name
            myshopifyDomain
            primaryDomain { id host url sslEnabled }
            currencyCode
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(hydrate.status, 200);
    assert_eq!(
        hydrate.body["data"]["shop"]["primaryDomain"]["id"],
        json!("gid://shopify/Domain/222333444")
    );

    let domain = proxy.process_request(json_graphql_request(
        r#"
        query DomainAfterShopHydrate($id: ID!) {
          domain(id: $id) { id host url sslEnabled }
        }
        "#,
        json!({ "id": "gid://shopify/Domain/222333444" }),
    ));

    assert_eq!(domain.status, 200);
    assert_eq!(
        domain.body["data"]["domain"],
        json!({
            "id": "gid://shopify/Domain/222333444",
            "host": "live-hydrated.example",
            "url": "https://live-hydrated.example",
            "sslEnabled": true
        })
    );
    assert_eq!(upstream_calls.lock().unwrap().len(), 1);
}

#[test]
fn domain_id_live_hybrid_forwards_cold_domain_reads() {
    let upstream_calls = Arc::new(Mutex::new(Vec::<Value>::new()));
    let captured_calls = Arc::clone(&upstream_calls);
    let mut proxy =
        configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport(move |request| {
            let body: Value =
                serde_json::from_str(&request.body).expect("upstream GraphQL body parses");
            captured_calls.lock().unwrap().push(body.clone());
            Response {
                status: 200,
                headers: Default::default(),
                body: json!({
                    "data": {
                        "domain": {
                            "id": "gid://shopify/Domain/777888999",
                            "host": "cold-live.example",
                            "url": "https://cold-live.example",
                            "sslEnabled": true
                        }
                    }
                }),
            }
        });

    let response = proxy.process_request(json_graphql_request(
        r#"
        query ColdDomainRead($id: ID!) {
          domain(id: $id) { id host url sslEnabled }
        }
        "#,
        json!({ "id": "gid://shopify/Domain/777888999" }),
    ));

    assert_eq!(response.status, 200);
    assert_eq!(
        response.body["data"]["domain"],
        json!({
            "id": "gid://shopify/Domain/777888999",
            "host": "cold-live.example",
            "url": "https://cold-live.example",
            "sslEnabled": true
        })
    );
    assert_eq!(
        upstream_calls.lock().unwrap()[0]["variables"],
        json!({ "id": "gid://shopify/Domain/777888999" })
    );
}

#[test]
fn cold_snapshot_shop_baseline_uses_neutral_identity() {
    let mut proxy = snapshot_proxy();
    let response = proxy.process_request(json_graphql_request(
        r#"
        query ColdShopIdentity {
          shop {
            id
            name
            myshopifyDomain
            primaryDomain { id host url sslEnabled }
            currencyCode
          }
        }
        "#,
        json!({}),
    ));

    assert_eq!(response.status, 200);
    assert_eq!(
        response.body["data"]["shop"],
        json!({
            "id": "gid://shopify/Shop/0",
            "name": "Shopify Draft Proxy",
            "myshopifyDomain": "shopify-draft-proxy.local",
            "primaryDomain": {
                "id": "gid://shopify/Domain/1000",
                "host": "shopify-draft-proxy.local",
                "url": "https://shopify-draft-proxy.local",
                "sslEnabled": true
            },
            "currencyCode": "USD"
        })
    );
}

#[test]
fn fulfillment_order_request_and_cancellation_transitions_stage_and_read_back() {
    let mut proxy = snapshot_proxy();
    let (order, fulfillment_order) = create_fulfillment_order_test_order(&mut proxy, 2);
    let order_id = order["id"].clone();
    let fulfillment_order_id = fulfillment_order["id"].clone();
    let fulfillment_order_line_item_id = fulfillment_order["lineItems"]["nodes"][0]["id"].clone();

    let submit = proxy.process_request(json_graphql_request(
        r#"
        mutation SubmitFulfillmentOrderRequest(
          $id: ID!
          $lineItems: [FulfillmentOrderLineItemInput!]
        ) {
          fulfillmentOrderSubmitFulfillmentRequest(
            id: $id
            fulfillmentOrderLineItems: $lineItems
            message: "please ship"
            notifyCustomer: false
          ) {
            originalFulfillmentOrder {
              id
              status
              requestStatus
              merchantRequests(first: 10) { nodes { kind message requestOptions responseData } }
              lineItems(first: 5) { nodes { id totalQuantity remainingQuantity } }
            }
            submittedFulfillmentOrder { id status requestStatus }
            unsubmittedFulfillmentOrder {
              status
              requestStatus
              lineItems(first: 5) { nodes { totalQuantity remainingQuantity } }
            }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "id": fulfillment_order_id,
            "lineItems": [{ "id": fulfillment_order_line_item_id, "quantity": 1 }]
        }),
    ));
    assert_eq!(submit.status, 200);
    let submit_payload = &submit.body["data"]["fulfillmentOrderSubmitFulfillmentRequest"];
    assert_eq!(submit_payload["userErrors"], json!([]));
    assert_eq!(
        submit_payload["submittedFulfillmentOrder"]["requestStatus"],
        json!("SUBMITTED")
    );
    assert_eq!(
        submit_payload["unsubmittedFulfillmentOrder"]["requestStatus"],
        json!("UNSUBMITTED")
    );
    assert_eq!(
        submit_payload["originalFulfillmentOrder"]["merchantRequests"]["nodes"][0],
        json!({
            "kind": "FULFILLMENT_REQUEST",
            "message": "please ship",
            "requestOptions": { "notify_customer": false },
            "responseData": null
        })
    );

    let direct_read = proxy.process_request(json_graphql_request(
        r#"
        query ReadFulfillmentOrder($id: ID!) {
          fulfillmentOrder(id: $id) {
            id
            status
            requestStatus
            merchantRequests(first: 10) { nodes { kind message requestOptions responseData } }
          }
        }
        "#,
        json!({ "id": fulfillment_order_id }),
    ));
    assert_eq!(
        direct_read.body["data"]["fulfillmentOrder"]["requestStatus"],
        json!("SUBMITTED")
    );

    let accept = proxy.process_request(json_graphql_request(
        r#"
        mutation AcceptFulfillmentOrderRequest($id: ID!) {
          fulfillmentOrderAcceptFulfillmentRequest(
            id: $id
            message: "accepted"
            estimatedShippedAt: "2026-04-27T00:00:00Z"
          ) {
            fulfillmentOrder { id status requestStatus estimatedShippedAt }
            userErrors { field message }
          }
        }
        "#,
        json!({ "id": fulfillment_order_id }),
    ));
    assert_eq!(
        accept.body["data"]["fulfillmentOrderAcceptFulfillmentRequest"]["fulfillmentOrder"]
            ["requestStatus"],
        json!("ACCEPTED")
    );
    assert_eq!(
        accept.body["data"]["fulfillmentOrderAcceptFulfillmentRequest"]["fulfillmentOrder"]
            ["status"],
        json!("IN_PROGRESS")
    );

    let submit_cancel = proxy.process_request(json_graphql_request(
        r#"
        mutation SubmitFulfillmentOrderCancellationRequest($id: ID!) {
          fulfillmentOrderSubmitCancellationRequest(id: $id, message: "cancel please") {
            fulfillmentOrder {
              id
              status
              requestStatus
              merchantRequests(first: 10) { nodes { kind message requestOptions responseData } }
            }
            userErrors { field message }
          }
        }
        "#,
        json!({ "id": fulfillment_order_id }),
    ));
    let submit_cancel_payload =
        &submit_cancel.body["data"]["fulfillmentOrderSubmitCancellationRequest"];
    assert_eq!(submit_cancel_payload["userErrors"], json!([]));
    assert_eq!(
        submit_cancel_payload["fulfillmentOrder"]["merchantRequests"]["nodes"][1]["kind"],
        json!("CANCELLATION_REQUEST")
    );

    let reject_cancel = proxy.process_request(json_graphql_request(
        r#"
        mutation RejectFulfillmentOrderCancellationRequest($id: ID!) {
          fulfillmentOrderRejectCancellationRequest(id: $id, message: "keep shipping") {
            fulfillmentOrder { id status requestStatus }
            userErrors { field message }
          }
        }
        "#,
        json!({ "id": fulfillment_order_id }),
    ));
    assert_eq!(
        reject_cancel.body["data"]["fulfillmentOrderRejectCancellationRequest"]["fulfillmentOrder"]
            ["requestStatus"],
        json!("CANCELLATION_REJECTED")
    );

    let list_read = proxy.process_request(json_graphql_request(
        r#"
        query ReadNestedFulfillmentOrders($orderId: ID!) {
          order(id: $orderId) {
            id
            fulfillmentOrders(first: 5) { nodes { id status requestStatus } }
          }
        }
        "#,
        json!({ "orderId": order_id }),
    ));
    assert_eq!(
        list_read.body["data"]["order"]["fulfillmentOrders"]["nodes"][0]["requestStatus"],
        json!("CANCELLATION_REJECTED")
    );

    let root_list_read = proxy.process_request(json_graphql_request(
        r#"
        query ReadFulfillmentOrderRootLists {
          fulfillmentOrders(first: 5) { nodes { id status requestStatus } }
          assignedFulfillmentOrders(first: 5) { nodes { id status requestStatus } }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        root_list_read.body["data"]["fulfillmentOrders"]["nodes"][0]["requestStatus"],
        json!("CANCELLATION_REJECTED")
    );
    assert_eq!(
        root_list_read.body["data"]["assignedFulfillmentOrders"]["nodes"][0]["requestStatus"],
        json!("CANCELLATION_REJECTED")
    );

    let log = log_snapshot(&proxy);
    let operation_names = log["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["operationName"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        operation_names,
        vec![
            "orderCreate",
            "fulfillmentOrderSubmitFulfillmentRequest",
            "fulfillmentOrderAcceptFulfillmentRequest",
            "fulfillmentOrderSubmitCancellationRequest",
            "fulfillmentOrderRejectCancellationRequest"
        ]
    );
    assert!(log["entries"][1]["rawBody"]
        .as_str()
        .unwrap()
        .contains("SubmitFulfillmentOrderRequest"));
}

#[test]
fn fulfillment_order_reject_and_accept_cancellation_transitions_stage_locally() {
    let mut proxy = snapshot_proxy();
    let (_, fulfillment_order) = create_fulfillment_order_test_order(&mut proxy, 1);
    let fulfillment_order_id = fulfillment_order["id"].clone();

    let submit = proxy.process_request(json_graphql_request(
        r#"
        mutation SubmitForReject($id: ID!) {
          fulfillmentOrderSubmitFulfillmentRequest(id: $id, message: "submit") {
            originalFulfillmentOrder { id requestStatus }
            userErrors { field message }
          }
        }
        "#,
        json!({ "id": fulfillment_order_id }),
    ));
    assert_eq!(
        submit.body["data"]["fulfillmentOrderSubmitFulfillmentRequest"]["userErrors"],
        json!([])
    );

    let reject = proxy.process_request(json_graphql_request(
        r#"
        mutation RejectFulfillmentOrderRequest($id: ID!) {
          fulfillmentOrderRejectFulfillmentRequest(id: $id, reason: OTHER, message: "no") {
            fulfillmentOrder { id status requestStatus }
            userErrors { field message }
          }
        }
        "#,
        json!({ "id": fulfillment_order_id }),
    ));
    assert_eq!(
        reject.body["data"]["fulfillmentOrderRejectFulfillmentRequest"]["fulfillmentOrder"]
            ["requestStatus"],
        json!("REJECTED")
    );

    let submit_again = proxy.process_request(json_graphql_request(
        r#"
        mutation SubmitAgain($id: ID!) {
          fulfillmentOrderSubmitFulfillmentRequest(id: $id, message: "again") {
            originalFulfillmentOrder { id requestStatus }
            userErrors { field message }
          }
        }
        "#,
        json!({ "id": fulfillment_order_id }),
    ));
    assert_eq!(
        submit_again.body["data"]["fulfillmentOrderSubmitFulfillmentRequest"]
            ["originalFulfillmentOrder"]["requestStatus"],
        json!("SUBMITTED")
    );

    let accept = proxy.process_request(json_graphql_request(
        r#"
        mutation AcceptThenCancel($id: ID!) {
          fulfillmentOrderAcceptFulfillmentRequest(id: $id, message: "accepted") {
            fulfillmentOrder { id status requestStatus }
            userErrors { field message }
          }
        }
        "#,
        json!({ "id": fulfillment_order_id }),
    ));
    assert_eq!(
        accept.body["data"]["fulfillmentOrderAcceptFulfillmentRequest"]["fulfillmentOrder"]
            ["requestStatus"],
        json!("ACCEPTED")
    );

    let submit_cancel = proxy.process_request(json_graphql_request(
        r#"
        mutation SubmitCancel($id: ID!) {
          fulfillmentOrderSubmitCancellationRequest(id: $id, message: "cancel") {
            fulfillmentOrder { id status requestStatus }
            userErrors { field message }
          }
        }
        "#,
        json!({ "id": fulfillment_order_id }),
    ));
    assert_eq!(
        submit_cancel.body["data"]["fulfillmentOrderSubmitCancellationRequest"]["fulfillmentOrder"]
            ["requestStatus"],
        json!("ACCEPTED")
    );

    let accept_cancel = proxy.process_request(json_graphql_request(
        r#"
        mutation AcceptCancel($id: ID!) {
          fulfillmentOrderAcceptCancellationRequest(id: $id, message: "ok") {
            fulfillmentOrder {
              id
              status
              requestStatus
              lineItems(first: 5) { nodes { totalQuantity remainingQuantity } }
            }
            userErrors { field message }
          }
        }
        "#,
        json!({ "id": fulfillment_order_id }),
    ));
    let fulfillment_order = &accept_cancel.body["data"]
        ["fulfillmentOrderAcceptCancellationRequest"]["fulfillmentOrder"];
    assert_eq!(fulfillment_order["status"], json!("CLOSED"));
    assert_eq!(
        fulfillment_order["requestStatus"],
        json!("CANCELLATION_ACCEPTED")
    );
    assert_eq!(
        fulfillment_order["lineItems"]["nodes"][0]["remainingQuantity"],
        json!(0)
    );
}

#[test]
fn fulfillment_order_split_and_merge_stage_remaining_records_and_read_back() {
    let mut proxy = snapshot_proxy();
    let (order, fulfillment_order) = create_fulfillment_order_test_order(&mut proxy, 3);
    let order_id = order["id"].clone();
    let fulfillment_order_id = fulfillment_order["id"].clone();
    let fulfillment_order_line_item_id = fulfillment_order["lineItems"]["nodes"][0]["id"].clone();

    let split = proxy.process_request(json_graphql_request(
        r#"
        mutation SplitFulfillmentOrder($splits: [FulfillmentOrderSplitInput!]!) {
          fulfillmentOrderSplit(fulfillmentOrderSplits: $splits) {
            fulfillmentOrderSplits {
              fulfillmentOrder {
                id
                status
                requestStatus
                updatedAt
                supportedActions { action }
                assignedLocation { name location { id name } }
                lineItems(first: 5) { nodes { id totalQuantity remainingQuantity lineItem { id title quantity fulfillableQuantity } } }
              }
              remainingFulfillmentOrder {
                id
                status
                requestStatus
                updatedAt
                supportedActions { action }
                assignedLocation { name location { id name } }
                lineItems(first: 5) { nodes { id totalQuantity remainingQuantity lineItem { id title quantity fulfillableQuantity } } }
              }
              replacementFulfillmentOrder { id }
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
    let split_payload = &split.body["data"]["fulfillmentOrderSplit"];
    assert_eq!(split_payload["userErrors"], json!([]));
    let original_after_split = &split_payload["fulfillmentOrderSplits"][0]["fulfillmentOrder"];
    let remaining_after_split =
        &split_payload["fulfillmentOrderSplits"][0]["remainingFulfillmentOrder"];
    assert_eq!(
        original_after_split["lineItems"]["nodes"][0]["remainingQuantity"],
        json!(2)
    );
    assert_eq!(
        remaining_after_split["lineItems"]["nodes"][0]["remainingQuantity"],
        json!(1)
    );
    assert_eq!(
        split_payload["fulfillmentOrderSplits"][0]["replacementFulfillmentOrder"],
        json!(null)
    );
    let remaining_id = remaining_after_split["id"].clone();

    let list_after_split = proxy.process_request(json_graphql_request(
        r#"
        query ReadNestedAfterSplit($orderId: ID!) {
          order(id: $orderId) {
            fulfillmentOrders(first: 5) { nodes { id lineItems(first: 5) { nodes { remainingQuantity } } } }
          }
        }
        "#,
        json!({ "orderId": order_id }),
    ));
    assert_eq!(
        list_after_split.body["data"]["order"]["fulfillmentOrders"]["nodes"]
            .as_array()
            .unwrap()
            .len(),
        2
    );

    let remaining_read = proxy.process_request(json_graphql_request(
        r#"
        query ReadRemainingAfterSplit($remainingId: ID!) {
          fulfillmentOrder(id: $remainingId) {
            id
            lineItems(first: 5) { nodes { remainingQuantity } }
          }
        }
        "#,
        json!({ "remainingId": remaining_id }),
    ));
    assert_eq!(
        remaining_read.body["data"]["fulfillmentOrder"]["lineItems"]["nodes"][0]
            ["remainingQuantity"],
        json!(1)
    );

    let merge = proxy.process_request(json_graphql_request(
        r#"
        mutation MergeFulfillmentOrders($inputs: [FulfillmentOrderMergeInput!]!) {
          fulfillmentOrderMerge(fulfillmentOrderMergeInputs: $inputs) {
            fulfillmentOrderMerges {
              fulfillmentOrder {
                id
                status
                requestStatus
                fulfillBy
                lineItems(first: 10) { nodes { id totalQuantity remainingQuantity } }
              }
            }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "inputs": [{
                "mergeIntents": [
                    { "fulfillmentOrderId": fulfillment_order_id },
                    { "fulfillmentOrderId": remaining_id }
                ]
            }]
        }),
    ));
    let merge_payload = &merge.body["data"]["fulfillmentOrderMerge"];
    assert_eq!(merge_payload["userErrors"], json!([]));
    assert_eq!(
        merge_payload["fulfillmentOrderMerges"][0]["fulfillmentOrder"]["lineItems"]["nodes"][0]
            ["remainingQuantity"],
        json!(3)
    );

    let list_after_merge = proxy.process_request(json_graphql_request(
        r#"
        query ReadNestedAfterMerge($orderId: ID!) {
          order(id: $orderId) {
            fulfillmentOrders(first: 5) { nodes { id status lineItems(first: 5) { nodes { remainingQuantity } } } }
          }
        }
        "#,
        json!({ "orderId": order_id }),
    ));
    let merged_nodes = list_after_merge.body["data"]["order"]["fulfillmentOrders"]["nodes"]
        .as_array()
        .unwrap();
    assert_eq!(merged_nodes.len(), 2);
    assert_eq!(merged_nodes[0]["status"], json!("OPEN"));
    assert_eq!(merged_nodes[1]["status"], json!("CLOSED"));
    assert_eq!(
        merged_nodes[1]["lineItems"]["nodes"][0]["remainingQuantity"],
        json!(0)
    );

    let root_list_after_merge = proxy.process_request(json_graphql_request(
        r#"
        query ReadRootAfterMerge {
          fulfillmentOrders(first: 5) { nodes { id } }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        root_list_after_merge.body["data"]["fulfillmentOrders"]["nodes"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
}

#[test]
fn fulfillment_order_split_hydrates_observed_fulfillment_orders_without_order_owner() {
    let calls = Arc::new(Mutex::new(Vec::<Request>::new()));
    let captured = Arc::clone(&calls);
    let mut proxy = configured_proxy(
        ReadMode::LiveHybrid,
        Some(shopify_draft_proxy::proxy::UnsupportedMutationMode::Passthrough),
    )
    .with_upstream_transport(move |request| {
        let body: Value = serde_json::from_str(&request.body).unwrap();
        let id = body["variables"]["id"].as_str().unwrap_or_default();
        captured.lock().unwrap().push(request);
        let fulfillment_order = match id {
            "gid://shopify/FulfillmentOrder/live-a" => json!({
                "id": id,
                "status": "OPEN",
                "requestStatus": "UNSUBMITTED",
                "updatedAt": "2026-05-05T02:10:28Z",
                "supportedActions": [{ "action": "SPLIT" }],
                "assignedLocation": {
                    "name": "Shop location",
                    "location": { "id": "gid://shopify/Location/1", "name": "Shop location" }
                },
                "lineItems": { "nodes": [{
                    "id": "gid://shopify/FulfillmentOrderLineItem/live-a-line",
                    "totalQuantity": 2,
                    "remainingQuantity": 2,
                    "lineItem": {
                        "id": "gid://shopify/LineItem/live-a-line",
                        "title": "Live A",
                        "quantity": 2,
                        "fulfillableQuantity": 2
                    }
                }] }
            }),
            "gid://shopify/FulfillmentOrder/live-b" => json!({
                "id": id,
                "status": "OPEN",
                "requestStatus": "UNSUBMITTED",
                "updatedAt": "2026-05-05T02:10:29Z",
                "supportedActions": [{ "action": "SPLIT" }],
                "assignedLocation": {
                    "name": "Custom location",
                    "location": { "id": "gid://shopify/Location/2", "name": "Custom location" }
                },
                "lineItems": { "nodes": [{
                    "id": "gid://shopify/FulfillmentOrderLineItem/live-b-line",
                    "totalQuantity": 3,
                    "remainingQuantity": 3,
                    "lineItem": {
                        "id": "gid://shopify/LineItem/live-b-line",
                        "title": "Live B",
                        "quantity": 3,
                        "fulfillableQuantity": 3
                    }
                }] }
            }),
            _ => Value::Null,
        };
        Response {
            status: 200,
            headers: Default::default(),
            body: json!({ "data": { "fulfillmentOrder": fulfillment_order } }),
        }
    });

    let split = proxy.process_request(json_graphql_request(
        r#"
        mutation SplitObservedFulfillmentOrders($fulfillmentOrderSplits: [FulfillmentOrderSplitInput!]!) {
          fulfillmentOrderSplit(fulfillmentOrderSplits: $fulfillmentOrderSplits) {
            fulfillmentOrderSplits {
              fulfillmentOrder { id lineItems(first: 5) { nodes { remainingQuantity } } }
              remainingFulfillmentOrder { id lineItems(first: 5) { nodes { remainingQuantity } } }
            }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "fulfillmentOrderSplits": [
                {
                    "fulfillmentOrderId": "gid://shopify/FulfillmentOrder/live-a",
                    "fulfillmentOrderLineItems": [{
                        "id": "gid://shopify/FulfillmentOrderLineItem/live-a-line",
                        "quantity": 1
                    }]
                },
                {
                    "fulfillmentOrderId": "gid://shopify/FulfillmentOrder/live-b",
                    "fulfillmentOrderLineItems": [{
                        "id": "gid://shopify/FulfillmentOrderLineItem/live-b-line",
                        "quantity": 2
                    }]
                }
            ]
        }),
    ));

    assert_eq!(split.status, 200);
    assert_eq!(
        split.body["data"]["fulfillmentOrderSplit"]["userErrors"],
        json!([])
    );
    assert_eq!(
        split.body["data"]["fulfillmentOrderSplit"]["fulfillmentOrderSplits"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(calls.lock().unwrap().len(), 2);
}

#[test]
fn backup_region_update_uses_staged_market_region_and_computed_coercion_locations() {
    let mut proxy = snapshot_proxy();

    let omitted = proxy.process_request(json_graphql_request(
        r#"
        mutation BackupRegionUpdateOmitted {
          backupRegionUpdate {
            backupRegion { __typename id name ... on MarketRegionCountry { code } }
            userErrors { field message code }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        omitted.body["data"]["backupRegionUpdate"],
        json!({
            "backupRegion": null,
            "userErrors": []
        })
    );

    let null_region = proxy.process_request(json_graphql_request(
        r#"
        mutation BackupRegionUpdateNull {
          backupRegionUpdate(region: null) {
            backupRegion { __typename id name ... on MarketRegionCountry { code } }
            userErrors { field message code }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(null_region.body, omitted.body);

    let valid_country_without_market = proxy.process_request(json_graphql_request(
        r#"
        mutation BackupRegionUpdateValidCountryWithoutMarket {
          backupRegionUpdate(region: { countryCode: JP }) {
            backupRegion { id }
            userErrors { field message code }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        valid_country_without_market.body["data"]["backupRegionUpdate"],
        json!({
            "backupRegion": null,
            "userErrors": [{
                "field": ["region"],
                "message": "Region not found.",
                "code": "REGION_NOT_FOUND"
            }]
        })
    );

    let created_market = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateJapanMarket {
          marketCreate(input: { name: "Japan", enabled: true, regions: [{ countryCode: "JP" }] }) {
            market {
              id
              name
              enabled
              status
              type
              conditions { regionsCondition { regions(first: 5) { nodes { code } } } }
            }
            userErrors { field message code }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        created_market.body["data"]["marketCreate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        created_market.body["data"]["marketCreate"]["market"]["conditions"]["regionsCondition"]
            ["regions"]["nodes"],
        json!([{ "code": "JP" }])
    );

    let update = proxy.process_request(json_graphql_request(
        r#"
        mutation BackupRegionUpdateJapan {
          backupRegionUpdate(region: { countryCode: JP }) {
            backupRegion { __typename id name ... on MarketRegionCountry { code } }
            userErrors { field message code }
          }
        }
        "#,
        json!({}),
    ));
    let updated_region = update.body["data"]["backupRegionUpdate"]["backupRegion"].clone();
    assert_eq!(
        update.body["data"]["backupRegionUpdate"]["userErrors"],
        json!([])
    );
    assert_eq!(updated_region["__typename"], json!("MarketRegionCountry"));
    assert_eq!(updated_region["name"], json!("Japan"));
    assert_eq!(updated_region["code"], json!("JP"));
    let region_id = updated_region["id"]
        .as_str()
        .expect("backup region id is selected")
        .to_string();
    assert!(
        region_id.starts_with("gid://shopify/Market/Region/"),
        "locally staged market region ids must come from the modeled market region node, got {region_id}"
    );

    let read = proxy.process_request(json_graphql_request(
        r#"
        query BackupRegionRead {
          backupRegion { __typename id name ... on MarketRegionCountry { code } }
        }
        "#,
        json!({}),
    ));
    assert_eq!(read.body["data"]["backupRegion"], updated_region);

    let node = proxy.process_request(json_graphql_request(
        r#"
        query BackupRegionNode($ids: [ID!]!) {
          nodes(ids: $ids) { __typename ... on MarketRegionCountry { id name code } }
        }
        "#,
        json!({ "ids": [
            "gid://shopify/MarketRegionCountry/4062110482738",
            region_id
        ] }),
    ));
    assert_eq!(node.body["data"]["nodes"][0], json!(null));
    assert_eq!(node.body["data"]["nodes"][1], updated_region);

    let valid_uncovered_country = proxy.process_request(json_graphql_request(
        r#"
        mutation BackupRegionUpdateUncoveredValidCountry {
          backupRegionUpdate(region: { countryCode: ZZ }) {
            backupRegion { id }
            userErrors { field message code }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        valid_uncovered_country.body["data"]["backupRegionUpdate"],
        json!({
            "backupRegion": null,
            "userErrors": [{
                "field": ["region"],
                "message": "Region not found.",
                "code": "REGION_NOT_FOUND"
            }]
        })
    );

    let uncovered_with_typename = proxy.process_request(json_graphql_request(
        r#"
        mutation BackupRegionUpdateValidationTypename {
          backupRegionUpdate(region: { countryCode: ZZ }) {
            backupRegion { id }
            userErrors { __typename field message code }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        uncovered_with_typename.body["data"]["backupRegionUpdate"]["userErrors"][0]["__typename"],
        json!("MarketUserError")
    );

    for (name, query, code) in [
        (
            "missing-country-code",
            r#"
            mutation FooMissingCountryCode {
              backupRegionUpdate(region: {}) { backupRegion { id } userErrors { field code } }
            }
            "#,
            "missingRequiredInputObjectAttribute",
        ),
        (
            "null-country-code",
            r#"
            mutation FooNullCountryCode {
              backupRegionUpdate(region: { countryCode: null }) { backupRegion { id } userErrors { field code } }
            }
            "#,
            "argumentLiteralsIncompatible",
        ),
        (
            "numeric-country-code",
            r#"
            mutation FooNumericCountryCode {
              backupRegionUpdate(region: { countryCode: 42 }) { backupRegion { id } userErrors { field code } }
            }
            "#,
            "argumentLiteralsIncompatible",
        ),
    ] {
        let response = proxy.process_request(json_graphql_request(query, json!({})));
        assert_eq!(
            response.body["errors"][0]["extensions"]["code"],
            json!(code),
            "{name} should fail during GraphQL input coercion"
        );
        assert_eq!(
            response.body["errors"][0]["path"][0],
            json!(query
                .lines()
                .find_map(|line| line.trim().strip_prefix("mutation "))
                .and_then(|line| line.split_whitespace().next())
                .map(|operation| format!("mutation {operation}"))
                .unwrap()),
            "{name} should derive the operation path from the parsed document"
        );
        assert!(
            response.body.get("data").is_none()
                || response.body["data"]["backupRegionUpdate"].is_null(),
            "{name} must not fabricate a successful payload"
        );
    }

    let invalid_country_location_query = r#"
        mutation BackupRegionUpdateInvalidCountryLocation {
          backupRegionUpdate(
            region: {
              countryCode: XX
            }
          ) { backupRegion { id } userErrors { field code } }
        }
        "#;
    let expected_line = invalid_country_location_query
        .lines()
        .position(|line| line.contains("region: {"))
        .map(|index| index + 1)
        .unwrap();
    let expected_column = invalid_country_location_query
        .lines()
        .find(|line| line.contains("region: {"))
        .and_then(|line| line.find('{'))
        .map(|index| index + 1)
        .unwrap();
    let invalid_country_location = proxy.process_request(json_graphql_request(
        invalid_country_location_query,
        json!({}),
    ));
    assert_eq!(
        invalid_country_location.body["errors"][0]["extensions"]["code"],
        json!("argumentLiteralsIncompatible")
    );
    assert_eq!(
        invalid_country_location.body["errors"][0]["locations"][0],
        json!({ "line": expected_line, "column": expected_column })
    );
    assert_ne!(
        invalid_country_location.body["errors"][0]["locations"][0],
        json!({ "line": 2, "column": 30 })
    );
}

#[test]
fn backup_region_update_uses_delegate_token_scopes_for_access_denied() {
    let mut proxy = snapshot_proxy();
    let created_market = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateJapanMarketForScopedBackupRegion {
          marketCreate(input: { name: "Japan", enabled: true, regions: [{ countryCode: "JP" }] }) {
            market { id }
            userErrors { field message code }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        created_market.body["data"]["marketCreate"]["userErrors"],
        json!([])
    );

    let markets_delegate = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateMarketsDelegate {
          delegateAccessTokenCreate(input: { delegateAccessScope: ["read_markets", "write_markets"], expiresIn: 300 }) {
            delegateAccessToken { accessToken accessScopes }
            userErrors { field message code }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        markets_delegate.body["data"]["delegateAccessTokenCreate"]["userErrors"],
        json!([])
    );
    let markets_token = markets_delegate.body["data"]["delegateAccessTokenCreate"]
        ["delegateAccessToken"]["accessToken"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(markets_token, "shpat_delegate_proxy_1");

    let mut allowed_request = json_graphql_request(
        r#"
        mutation BackupRegionUpdateAllowedDelegate {
          backupRegionUpdate(region: { countryCode: JP }) {
            backupRegion { id name ... on MarketRegionCountry { code } }
            userErrors { field message code }
          }
        }
        "#,
        json!({}),
    );
    allowed_request
        .headers
        .insert("X-Shopify-Access-Token".to_string(), markets_token);
    let allowed = proxy.process_request(allowed_request);
    assert_eq!(
        allowed.body["data"]["backupRegionUpdate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        allowed.body["data"]["backupRegionUpdate"]["backupRegion"]["code"],
        json!("JP")
    );

    let product_delegate = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateProductOnlyDelegate {
          delegateAccessTokenCreate(input: { delegateAccessScope: ["read_products"], expiresIn: 300 }) {
            delegateAccessToken { accessToken accessScopes }
            userErrors { field message code }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        product_delegate.body["data"]["delegateAccessTokenCreate"]["userErrors"],
        json!([])
    );
    let product_token = product_delegate.body["data"]["delegateAccessTokenCreate"]
        ["delegateAccessToken"]["accessToken"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(product_token, "shpat_delegate_proxy_2");

    let mut denied_request = json_graphql_request(
        r#"
        mutation BackupRegionUpdateDeniedDelegate {
          backupRegionUpdate(region: { countryCode: JP }) {
            backupRegion { id }
            userErrors { field message code }
          }
        }
        "#,
        json!({}),
    );
    denied_request
        .headers
        .insert("X-Shopify-Access-Token".to_string(), product_token);
    let denied = proxy.process_request(denied_request);
    assert_eq!(denied.body["data"]["backupRegionUpdate"], json!(null));
    assert_eq!(
        denied.body["errors"][0]["extensions"]["code"],
        json!("ACCESS_DENIED")
    );
}

#[test]
fn backup_region_update_hydrates_market_region_from_upstream_in_live_hybrid() {
    let upstream_calls = Arc::new(Mutex::new(Vec::<Value>::new()));
    let captured_calls = Arc::clone(&upstream_calls);
    let mut proxy =
        configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport(move |request| {
            let body =
                serde_json::from_str::<Value>(&request.body).expect("upstream GraphQL body parses");
            captured_calls.lock().unwrap().push(body.clone());
            match body["operationName"].as_str() {
                Some("BackupRegionAccessScopes") => Response {
                    status: 200,
                    headers: Default::default(),
                    body: json!({
                        "data": {
                            "currentAppInstallation": {
                                "accessScopes": [
                                    { "handle": "read_markets" },
                                    { "handle": "write_markets" }
                                ]
                            }
                        }
                    }),
                },
                Some("BackupRegionMarketsHydrate") => Response {
                    status: 200,
                    headers: Default::default(),
                    body: json!({
                        "data": {
                            "markets": {
                                "nodes": [{
                                    "id": "gid://shopify/Market/97997685042",
                                    "name": "Japan",
                                    "handle": "japan",
                                    "status": "ACTIVE",
                                    "enabled": true,
                                    "type": "REGION",
                                    "conditions": {
                                        "regionsCondition": {
                                            "regions": {
                                                "nodes": [{
                                                    "__typename": "MarketRegionCountry",
                                                    "id": "gid://shopify/MarketRegionCountry/shop-jp",
                                                    "name": "Japan",
                                                    "code": "JP"
                                                }]
                                            }
                                        }
                                    }
                                }]
                            }
                        }
                    }),
                },
                other => panic!("unexpected upstream operation: {other:?} body={body}"),
            }
        });

    let mut update_request = json_graphql_request(
        r#"
        mutation BackupRegionUpdateHydratedJapan {
          backupRegionUpdate(region: { countryCode: JP }) {
            backupRegion { __typename id name ... on MarketRegionCountry { code } }
            userErrors { field message code }
          }
        }
        "#,
        json!({}),
    );
    update_request.headers.insert(
        "X-Shopify-Access-Token".to_string(),
        "parent-live-token".to_string(),
    );
    let update = proxy.process_request(update_request);
    assert_eq!(
        update.body["data"]["backupRegionUpdate"],
        json!({
            "backupRegion": {
                "__typename": "MarketRegionCountry",
                "id": "gid://shopify/MarketRegionCountry/shop-jp",
                "name": "Japan",
                "code": "JP"
            },
            "userErrors": []
        })
    );

    let read = proxy.process_request(json_graphql_request(
        r#"
        query BackupRegionReadAfterHydrate {
          backupRegion { __typename id name ... on MarketRegionCountry { code } }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        read.body["data"]["backupRegion"],
        update.body["data"]["backupRegionUpdate"]["backupRegion"]
    );

    let node = proxy.process_request(json_graphql_request(
        r#"
        query BackupRegionNodeAfterHydrate {
          nodes(ids: ["gid://shopify/MarketRegionCountry/shop-jp"]) {
            __typename
            ... on MarketRegionCountry { id name code }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        node.body["data"]["nodes"][0],
        update.body["data"]["backupRegionUpdate"]["backupRegion"]
    );
    assert_eq!(
        upstream_calls
            .lock()
            .unwrap()
            .iter()
            .map(|body| body["operationName"].as_str().unwrap().to_string())
            .collect::<Vec<_>>(),
        vec![
            "BackupRegionAccessScopes".to_string(),
            "BackupRegionMarketsHydrate".to_string()
        ]
    );
}

#[test]
fn finance_and_pos_node_no_data_reads_return_null_nodes_locally() {
    let mut proxy = configured_proxy(ReadMode::LiveHybrid, None);
    let query = r#"
        query AdminPlatformFinanceRiskNodeNoData($ids: [ID!]!) {
          safeNodes: nodes(ids: $ids) {
            __typename
            ... on Node { id }
          }
        }
    "#;

    let response = proxy.process_request(json_graphql_request(
        query,
        json!({
            "ids": [
                "gid://shopify/CashTrackingSession/0",
                "gid://shopify/PointOfSaleDevice/0",
                "gid://shopify/ShopifyPaymentsDispute/0"
            ]
        }),
    ));

    assert_eq!(response.status, 200);
    assert_eq!(
        response.body,
        json!({ "data": { "safeNodes": [null, null, null] } })
    );
}

#[test]
fn generic_node_reads_reject_malformed_global_id_literals() {
    let cases = [
        (
            "query NodeMissingScheme { node(id: \"not-a-gid\") { __typename id } }",
            "not-a-gid",
            json!(["query NodeMissingScheme", "node", "id"]),
        ),
        (
            "query NodeMissingTypeAndId { node(id: \"gid://shopify/\") { __typename id } }",
            "gid://shopify/",
            json!(["query NodeMissingTypeAndId", "node", "id"]),
        ),
        (
            "query NodeEmptyType { node(id: \"gid://shopify//123\") { __typename id } }",
            "gid://shopify//123",
            json!(["query NodeEmptyType", "node", "id"]),
        ),
        (
            "query NodeMissingId { node(id: \"gid://shopify/Product\") { __typename id } }",
            "gid://shopify/Product",
            json!(["query NodeMissingId", "node", "id"]),
        ),
    ];

    for (query, invalid_id, path) in cases {
        let mut proxy = snapshot_proxy();
        let response = proxy.process_request(json_graphql_request(query, json!({})));

        assert_eq!(response.status, 200);
        assert!(response.body.get("data").is_none());
        assert_eq!(
            response.body["errors"][0]["message"],
            json!(format!("Invalid global id '{invalid_id}'"))
        );
        assert_eq!(response.body["errors"][0]["path"], path);
        assert_eq!(
            response.body["errors"][0]["extensions"],
            json!({
                "code": "argumentLiteralsIncompatible",
                "typeName": "CoercionError"
            })
        );
    }

    let mut proxy = snapshot_proxy();
    let mixed_nodes = proxy.process_request(json_graphql_request(
        r#"query NodesMixed { nodes(ids: ["gid://shopify/Product/0", "gid://shopify/Product", "gid://shopify/UnknownType/123"]) { __typename id } }"#,
        json!({}),
    ));
    assert_eq!(mixed_nodes.status, 200);
    assert!(mixed_nodes.body.get("data").is_none());
    assert_eq!(
        mixed_nodes.body["errors"][0]["message"],
        json!("Invalid global id 'gid://shopify/Product'")
    );
    assert_eq!(
        mixed_nodes.body["errors"][0]["path"],
        json!(["query NodesMixed", "nodes", "ids"])
    );
    assert_eq!(
        mixed_nodes.body["errors"][0]["extensions"],
        json!({
            "code": "argumentLiteralsIncompatible",
            "typeName": "CoercionError"
        })
    );
}

#[test]
fn generic_node_reads_reject_malformed_global_id_variables() {
    let mut proxy = snapshot_proxy();
    let node = proxy.process_request(json_graphql_request(
        r#"query VariableNodeMissingId($id: ID!) { node(id: $id) { __typename id } }"#,
        json!({ "id": "gid://shopify/Product" }),
    ));

    assert_eq!(node.status, 200);
    assert!(node.body.get("data").is_none());
    assert_eq!(
        node.body["errors"][0]["message"],
        json!("Variable $id of type ID! was provided invalid value")
    );
    assert_eq!(
        node.body["errors"][0]["extensions"],
        json!({
            "code": "INVALID_VARIABLE",
            "value": "gid://shopify/Product",
            "problems": [{
                "path": [],
                "explanation": "Invalid global id 'gid://shopify/Product'",
                "message": "Invalid global id 'gid://shopify/Product'"
            }]
        })
    );

    let mut proxy = snapshot_proxy();
    let nodes = proxy.process_request(json_graphql_request(
        r#"query VariableNodesMixed($ids: [ID!]!) { nodes(ids: $ids) { __typename id } }"#,
        json!({
            "ids": [
                "gid://shopify/Product/0",
                "gid://shopify/Product",
                "gid://shopify/UnknownType/123"
            ]
        }),
    ));

    assert_eq!(nodes.status, 200);
    assert!(nodes.body.get("data").is_none());
    assert_eq!(
        nodes.body["errors"][0]["message"],
        json!(
            "Variable $ids of type [ID!]! was provided invalid value for 1 (Invalid global id 'gid://shopify/Product')"
        )
    );
    assert_eq!(
        nodes.body["errors"][0]["extensions"],
        json!({
            "code": "INVALID_VARIABLE",
            "value": [
                "gid://shopify/Product/0",
                "gid://shopify/Product",
                "gid://shopify/UnknownType/123"
            ],
            "problems": [{
                "path": [1],
                "explanation": "Invalid global id 'gid://shopify/Product'",
                "message": "Invalid global id 'gid://shopify/Product'"
            }]
        })
    );
}

#[test]
fn generic_node_reads_keep_well_formed_absent_and_unknown_ids_null() {
    let mut proxy = snapshot_proxy();
    let response = proxy.process_request(json_graphql_request(
        r#"
        query WellFormedUnknownNodeReads($ids: [ID!]!) {
          absentProduct: node(id: "gid://shopify/Product/0") { __typename id }
          unknownType: node(id: "gid://shopify/UnknownType/123") { __typename id }
          nodes(ids: $ids) { __typename id }
        }
        "#,
        json!({
            "ids": [
                "gid://shopify/Product/0",
                "gid://shopify/UnknownType/123"
            ]
        }),
    ));

    assert_eq!(response.status, 200);
    assert!(response.body.get("errors").is_none());
    assert_eq!(
        response.body["data"],
        json!({
            "absentProduct": null,
            "unknownType": null,
            "nodes": [null, null]
        })
    );
}

#[test]
fn finance_and_risk_no_data_top_level_reads_return_safe_empty_shapes_locally() {
    let mut proxy = configured_proxy(ReadMode::LiveHybrid, None);
    let response = proxy.process_request(json_graphql_request(
        r#"
        query FinanceRiskNoDataRead(
          $cashId: ID!
          $posId: ID!
          $disputeId: ID!
          $evidenceId: ID!
          $token: String!
          $first: Int!
        ) {
          cashTrackingSession(id: $cashId) { __typename }
          cashTrackingSessions(first: $first) {
            nodes { __typename }
            edges { node { __typename } }
            pageInfo { hasNextPage hasPreviousPage }
          }
          pointOfSaleDevice(id: $posId) { __typename }
          dispute(id: $disputeId) { __typename }
          disputeEvidence(id: $evidenceId) { __typename }
          disputes(first: $first) {
            nodes { __typename }
            edges { node { __typename } }
            pageInfo { hasNextPage hasPreviousPage }
          }
          shopPayPaymentRequestReceipt(token: $token) { __typename }
          shopPayPaymentRequestReceipts(first: $first) {
            nodes { __typename }
            edges { node { __typename } }
            pageInfo { hasNextPage hasPreviousPage }
          }
        }
        "#,
        json!({
            "cashId": "gid://shopify/CashTrackingSession/0",
            "posId": "gid://shopify/PointOfSaleDevice/0",
            "disputeId": "gid://shopify/ShopifyPaymentsDispute/0",
            "evidenceId": "gid://shopify/ShopifyPaymentsDisputeEvidence/0",
            "token": "codex-missing-shop-pay-payment-request-receipt-token",
            "first": 1
        }),
    ));

    assert_eq!(response.status, 200);
    assert_eq!(
        response.body,
        json!({
            "data": {
                "cashTrackingSession": null,
                "cashTrackingSessions": { "nodes": [], "edges": [], "pageInfo": { "hasNextPage": false, "hasPreviousPage": false } },
                "pointOfSaleDevice": null,
                "dispute": null,
                "disputeEvidence": null,
                "disputes": { "nodes": [], "edges": [], "pageInfo": { "hasNextPage": false, "hasPreviousPage": false } },
                "shopPayPaymentRequestReceipt": null,
                "shopPayPaymentRequestReceipts": { "nodes": [], "edges": [], "pageInfo": { "hasNextPage": false, "hasPreviousPage": false } }
            }
        })
    );
}

#[test]
fn shopify_payments_account_access_probe_returns_captured_null_account_data() {
    let mut proxy = configured_proxy(ReadMode::LiveHybrid, None);
    let response = proxy.process_request(json_graphql_request(
        r#"
        query ShopifyPaymentsAccountAccessProbe {
          shopifyPaymentsAccount {
            id
            activated
            country
            defaultCurrency
            onboardable
            payouts(first: 1) { nodes { id } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } }
            disputes(first: 1) { edges { node { id } } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } }
            balanceTransactions(first: 1) { nodes { id } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } }
          }
        }
        "#,
        json!({}),
    ));

    assert_eq!(response.status, 200);
    assert_eq!(
        response.body,
        json!({ "data": { "shopifyPaymentsAccount": null } })
    );
}

#[test]
fn flow_generate_signature_validates_arguments_and_stages_locally() {
    let mut proxy = snapshot_proxy();

    let missing_payload = proxy.process_request(json_graphql_request(
        r#"
        mutation FlowGenerateSignatureMissingPayloadRequiredArg {
          flowGenerateSignature(id: "gid://shopify/FlowActionDefinition/0") {
            signature
            payload
            userErrors { field message }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        missing_payload.body,
        json!({
            "errors": [{
                "message": "Field 'flowGenerateSignature' is missing required arguments: payload",
                "locations": [{ "line": 3, "column": 11 }],
                "path": ["mutation FlowGenerateSignatureMissingPayloadRequiredArg", "flowGenerateSignature"],
                "extensions": {
                    "code": "missingRequiredArguments",
                    "className": "Field",
                    "name": "flowGenerateSignature",
                    "arguments": "payload"
                }
            }]
        })
    );

    let invalid_id = proxy.process_request(json_graphql_request(
        r#"
        mutation FlowGenerateSignatureUnknown {
          flowGenerateSignature(id: "gid://shopify/FlowTrigger/0", payload: "{}") {
            signature
            userErrors { field message }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        invalid_id.body["data"]["flowGenerateSignature"],
        json!(null)
    );
    assert_eq!(
        invalid_id.body["errors"][0]["message"],
        json!("Invalid id: gid://shopify/FlowTrigger/0")
    );
    assert_eq!(
        invalid_id.body["errors"][0]["extensions"]["code"],
        json!("RESOURCE_NOT_FOUND")
    );

    let invalid_payload = proxy.process_request(json_graphql_request(
        r#"
        mutation FlowGenerateSignatureInvalidPayload {
          flowGenerateSignature(id: "gid://shopify/FlowActionDefinition/0", payload: "not json") {
            signature
            payload
            userErrors { field message }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        invalid_payload.body["data"]["flowGenerateSignature"],
        json!({
            "signature": null,
            "payload": null,
            "userErrors": [{
                "field": ["payload"],
                "message": "Payload must be valid JSON"
            }]
        })
    );

    let generated = proxy.process_request(json_graphql_request(
        r#"
        mutation FlowGenerateSignatureValid {
          local: flowGenerateSignature(id: "gid://shopify/FlowActionDefinition/0", payload: "{\"b\":2,\"a\":1}") {
            signature
            payload
            userErrors { field message }
          }
        }
        "#,
        json!({}),
    ));
    let payload = &generated.body["data"]["local"];
    assert_eq!(payload["payload"], json!("{\"a\":1,\"b\":2}"));
    assert!(payload["signature"]
        .as_str()
        .is_some_and(|value| !value.is_empty()));
    assert_eq!(payload["userErrors"], json!([]));

    let log = proxy.process_request(Request {
        method: "GET".to_string(),
        path: "/__meta/log".to_string(),
        headers: Default::default(),
        body: String::new(),
    });
    assert_eq!(log.body["entries"].as_array().unwrap().len(), 1);
    assert_eq!(
        log.body["entries"][0]["interpreted"]["primaryRootField"],
        json!("flowGenerateSignature")
    );
    assert_eq!(log.body["entries"][0]["status"], json!("staged"));
    assert!(log.body["entries"][0]["rawBody"]
        .as_str()
        .is_some_and(|raw| raw.contains("FlowGenerateSignatureValid")));
}

#[test]
fn flow_trigger_receive_validation_branches_match_captures() {
    let mut proxy = snapshot_proxy();

    let body_and_handle = proxy.process_request(json_graphql_request(
        r#"
        mutation FlowTriggerReceiveBodyAndHandleConflict {
          flowTriggerReceive(body: "{\"trigger_id\":\"abc\",\"properties\":{}}", handle: "test") {
            userErrors { field message }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        body_and_handle.body["data"]["flowTriggerReceive"]["userErrors"],
        json!([{
            "field": ["body"],
            "message": "Cannot use `handle` and `payload` arguments with `body` argument"
        }])
    );

    for query in [
        r#"
        mutation FlowTriggerReceiveEmptyHandleEmptyBody {
          flowTriggerReceive {
            userErrors { field message }
          }
        }
        "#,
        r#"
        mutation FlowTriggerReceivePayloadOnlyNoHandle {
          flowTriggerReceive(payload: { test: "value" }) {
            userErrors { field message }
          }
        }
        "#,
        r#"
        mutation FlowTriggerReceiveEmptyHandleString {
          flowTriggerReceive(handle: "") {
            userErrors { field message }
          }
        }
        "#,
    ] {
        let response = proxy.process_request(json_graphql_request(query, json!({})));
        assert_eq!(
            response.body["data"]["flowTriggerReceive"]["userErrors"],
            json!([{
                "field": ["handle"],
                "message": "`handle` and `payload` arguments are required"
            }])
        );
    }

    let unknown_handle = proxy.process_request(json_graphql_request(
        r#"
        mutation FlowTriggerReceiveInvalid {
          flowTriggerReceive(handle: "har-374-missing", payload: { test: "value" }) {
            userErrors { field message }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        unknown_handle.body["data"]["flowTriggerReceive"]["userErrors"],
        json!([{
            "field": ["body"],
            "message": "Errors validating schema:\n  Invalid handle 'har-374-missing'.\n"
        }])
    );

    let body_not_json = proxy.process_request(json_graphql_request(
        r#"
        mutation FlowTriggerReceiveBodyNotJson {
          flowTriggerReceive(body: "not json") {
            userErrors { field message }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        body_not_json.body["data"]["flowTriggerReceive"]["userErrors"],
        json!([{
            "field": ["body"],
            "message": "Errors validating schema:\n  unexpected token 'not' at line 1 column 1\n"
        }])
    );

    let body_schema = proxy.process_request(json_graphql_request(
        r#"
        mutation FlowTriggerReceiveBodySchemaGaps {
          missingTriggerReference: flowTriggerReceive(body: "{\"properties\":{}}") {
            userErrors { field message }
          }
          nonAbsoluteResourceUrl: flowTriggerReceive(body: "{\"trigger_id\":\"abc\",\"properties\":{},\"resources\":[{\"url\":\"not-a-url\",\"name\":\"x\"}]}") {
            userErrors { field message }
          }
          multipleSchemaErrors: flowTriggerReceive(body: "{\"properties\":{},\"resources\":[{\"url\":\"not-a-url\"}],\"unknown_root\":1}") {
            userErrors { field message }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        body_schema.body["data"]["missingTriggerReference"]["userErrors"][0]["message"],
        json!("Errors validating schema:\n  Required field missing: 'trigger_id'.\n")
    );
    assert_eq!(
        body_schema.body["data"]["nonAbsoluteResourceUrl"]["userErrors"][0]["message"],
        json!("Errors validating schema:\n  Type error for field 'url': not-a-url is not an absolute URL.\n")
    );
    assert_eq!(
        body_schema.body["data"]["multipleSchemaErrors"]["userErrors"][0]["message"],
        json!("Errors validating schema:\n  Invalid field: 'unknown_root'.\n  Required field missing: 'name'.\n  Type error for field 'url': not-a-url is not an absolute URL.\n")
    );
}

#[test]
fn flow_trigger_receive_accepts_local_handle_and_preserves_commit_log() {
    let mut proxy = snapshot_proxy();
    let response = proxy.process_request(json_graphql_request(
        r#"
        mutation FlowTriggerReceiveLocalHandle($payload: JSON) {
          flowTriggerReceive(handle: "local-flow-trigger", payload: $payload) {
            userErrors { field message }
          }
        }
        "#,
        json!({ "payload": { "nested": { "value": 1 }, "text": "hello" } }),
    ));

    assert_eq!(
        response.body["data"]["flowTriggerReceive"],
        json!({ "userErrors": [] })
    );

    let log = proxy.process_request(Request {
        method: "GET".to_string(),
        path: "/__meta/log".to_string(),
        headers: Default::default(),
        body: String::new(),
    });
    assert_eq!(log.body["entries"].as_array().unwrap().len(), 1);
    assert_eq!(
        log.body["entries"][0]["interpreted"]["primaryRootField"],
        json!("flowTriggerReceive")
    );
    assert_eq!(
        log.body["entries"][0]["variables"]["payload"],
        json!({ "nested": { "value": 1 }, "text": "hello" })
    );
    assert!(log.body["entries"][0]["rawBody"]
        .as_str()
        .is_some_and(|raw| raw.contains("FlowTriggerReceiveLocalHandle")));
}

#[test]
fn location_activate_limit_relocation_and_control_branches_match_local_runtime() {
    let mut proxy = snapshot_proxy();
    let query = r#"
        mutation LocationActivateLimitAndRelocation($locationId: ID!, $idempotencyKey: String!) {
          locationActivate(locationId: $locationId) @idempotent(key: $idempotencyKey) {
            location { id isActive }
            locationActivateUserErrors { field code message }
          }
        }
    "#;

    let limit = proxy.process_request(json_graphql_request(
        query,
        json!({ "locationId": "gid://shopify/Location/activate-limit", "idempotencyKey": "activate-limit" }),
    ));
    assert_eq!(
        limit.body["data"]["locationActivate"],
        json!({
            "location": { "id": "gid://shopify/Location/activate-limit", "isActive": false },
            "locationActivateUserErrors": [{
                "field": ["locationId"],
                "code": "LOCATION_LIMIT",
                "message": "Your shop has reached its location limit."
            }]
        })
    );

    let relocation = proxy.process_request(json_graphql_request(
        query,
        json!({ "locationId": "gid://shopify/Location/activate-relocation", "idempotencyKey": "activate-relocation" }),
    ));
    assert_eq!(
        relocation.body["data"]["locationActivate"],
        json!({
            "location": { "id": "gid://shopify/Location/activate-relocation", "isActive": false },
            "locationActivateUserErrors": [{
                "field": ["locationId"],
                "code": "HAS_ONGOING_RELOCATION",
                "message": "This location currently cannot be activated as inventory, pending orders or transfers are being relocated from this location. Please try again later."
            }]
        })
    );

    let control = proxy.process_request(json_graphql_request(
        query,
        json!({ "locationId": "gid://shopify/Location/activate-control", "idempotencyKey": "activate-control" }),
    ));
    assert_eq!(
        control.body["data"]["locationActivate"],
        json!({
            "location": { "id": "gid://shopify/Location/activate-control", "isActive": true },
            "locationActivateUserErrors": []
        })
    );
}

#[test]
fn location_add_resource_limit_guard_matches_local_runtime_without_logging_rejections() {
    let mut proxy = snapshot_proxy();
    let seed_query = r#"
        mutation LocationActivateLimitAndRelocation($locationId: ID!, $idempotencyKey: String!) {
          locationActivate(locationId: $locationId) @idempotent(key: $idempotencyKey) {
            location { id isActive }
            locationActivateUserErrors { field code message }
          }
        }
    "#;

    let seed = proxy.process_request(json_graphql_request(
        seed_query,
        json!({
            "locationId": "gid://shopify/Location/location-add-limit-seed",
            "idempotencyKey": "location-add-limit-seed"
        }),
    ));
    assert_eq!(
        seed.body["data"]["locationActivate"],
        json!({
            "location": { "id": "gid://shopify/Location/location-add-limit-seed", "isActive": false },
            "locationActivateUserErrors": [{
                "field": ["locationId"],
                "code": "LOCATION_LIMIT",
                "message": "Your shop has reached its location limit."
            }]
        })
    );

    let add = proxy.process_request(json_graphql_request(
        r#"
        mutation LocationAddResourceLimitReached($input: LocationAddInput!) {
          locationAdd(input: $input) {
            location { id name }
            userErrors { field code message }
          }
        }
        "#,
        json!({
            "input": {
                "name": "Proxy Cap Overflow 20260508142042",
                "address": {
                    "countryCode": "US",
                    "address1": "1 Resource Limit St",
                    "city": "New York",
                    "zip": "10001"
                }
            }
        }),
    ));
    assert_eq!(
        add.body["data"]["locationAdd"],
        json!({
            "location": null,
            "userErrors": [{
                "field": ["input"],
                "code": "INVALID",
                "message": "You have reached the maximum number of locations (200)"
            }]
        })
    );

    let log = proxy.process_request(Request {
        method: "GET".to_string(),
        path: "/__meta/log".to_string(),
        headers: Default::default(),
        body: String::new(),
    });
    assert_eq!(log.body, json!({ "entries": [] }));
}

#[test]
fn generic_location_add_stages_location_and_downstream_reads() {
    let mut proxy = snapshot_proxy();
    let add = proxy.process_request(json_graphql_request(
        r#"
        mutation GenericLocationAdd($input: LocationAddInput!) {
          locationAdd(input: $input) {
            location {
              id
              name
              isActive
              fulfillsOnlineOrders
              address { address1 city countryCode zip }
              metafield(namespace: "custom", key: "generic_add") { namespace key value type }
              metafields(first: 5, namespace: "custom") {
                nodes { namespace key value type }
                pageInfo { hasNextPage hasPreviousPage }
              }
            }
            userErrors { field code message }
          }
        }
        "#,
        json!({
            "input": {
                "name": "Generic Add Location",
                "address": {
                    "address1": "1 Spadina",
                    "city": "Toronto",
                    "countryCode": "CA",
                    "zip": "M5T 2C2"
                },
                "metafields": [{
                    "namespace": "custom",
                    "key": "generic_add",
                    "type": "single_line_text_field",
                    "value": "preserved"
                }]
            }
        }),
    ));

    let location = &add.body["data"]["locationAdd"]["location"];
    let location_id = location["id"].as_str().unwrap();
    assert_eq!(
        add.body["data"]["locationAdd"],
        json!({
            "location": {
                "id": location_id,
                "name": "Generic Add Location",
                "isActive": true,
                "fulfillsOnlineOrders": true,
                "address": {
                    "address1": "1 Spadina",
                    "city": "Toronto",
                    "countryCode": "CA",
                    "zip": "M5T 2C2"
                },
                "metafield": {
                    "namespace": "custom",
                    "key": "generic_add",
                    "value": "preserved",
                    "type": "single_line_text_field"
                },
                "metafields": {
                    "nodes": [{
                        "namespace": "custom",
                        "key": "generic_add",
                        "value": "preserved",
                        "type": "single_line_text_field"
                    }],
                    "pageInfo": { "hasNextPage": false, "hasPreviousPage": false }
                }
            },
            "userErrors": []
        })
    );

    let duplicate = proxy.process_request(json_graphql_request(
        r#"
        mutation GenericLocationAddDuplicate($input: LocationAddInput!) {
          locationAdd(input: $input) {
            location { id }
            userErrors { field code message }
          }
        }
        "#,
        json!({
            "input": {
                "name": "Generic Add Location",
                "address": { "countryCode": "CA" }
            }
        }),
    ));
    assert_eq!(
        duplicate.body["data"]["locationAdd"],
        json!({
            "location": null,
            "userErrors": [{
                "field": ["input", "name"],
                "code": "TAKEN",
                "message": "You already have a location with this name"
            }]
        })
    );

    let read = proxy.process_request(json_graphql_request(
        r#"
        query GenericLocationAddRead($id: ID!) {
          location(id: $id) { id name fulfillsOnlineOrders address { countryCode } }
          byIdentifier: locationByIdentifier(identifier: { id: $id }) { id name }
          locations(first: 5) { nodes { id name } pageInfo { hasNextPage hasPreviousPage } }
        }
        "#,
        json!({ "id": location_id }),
    ));
    assert_eq!(
        read.body["data"],
        json!({
            "location": {
                "id": location_id,
                "name": "Generic Add Location",
                "fulfillsOnlineOrders": true,
                "address": { "countryCode": "CA" }
            },
            "byIdentifier": {
                "id": location_id,
                "name": "Generic Add Location"
            },
            "locations": {
                "nodes": [{ "id": location_id, "name": "Generic Add Location" }],
                "pageInfo": { "hasNextPage": false, "hasPreviousPage": false }
            }
        })
    );

    let omitted_optional_address = proxy.process_request(json_graphql_request(
        r#"
        mutation GenericLocationAddOmittedAddress($input: LocationAddInput!) {
          locationAdd(input: $input) {
            location { address { address1 city countryCode provinceCode zip } }
            userErrors { field code message }
          }
        }
        "#,
        json!({
            "input": {
                "name": "Generic Add Omitted Optional Address",
                "address": { "countryCode": "CA" }
            }
        }),
    ));
    assert_eq!(
        omitted_optional_address.body["data"]["locationAdd"],
        json!({
            "location": {
                "address": {
                    "address1": null,
                    "city": null,
                    "countryCode": "CA",
                    "provinceCode": null,
                    "zip": null
                }
            },
            "userErrors": []
        })
    );

    let too_long_address = proxy.process_request(json_graphql_request(
        r#"
        mutation GenericLocationAddTooLongAddress($input: LocationAddInput!) {
          locationAdd(input: $input) {
            location { id }
            userErrors { field code message }
          }
        }
        "#,
        json!({
            "input": {
                "name": "Generic Add Too Long Address",
                "address": {
                    "address1": "A".repeat(256),
                    "countryCode": "CA"
                }
            }
        }),
    ));
    assert_eq!(
        too_long_address.body["data"]["locationAdd"],
        json!({
            "location": null,
            "userErrors": [{
                "field": ["input", "address", "address1"],
                "code": "TOO_LONG",
                "message": "Use a shorter name for the street (up to 255 characters)"
            }]
        })
    );

    let inventory_item_id = "gid://shopify/InventoryItem/generic-location-add";
    let set_quantities = r#"
        mutation InventoryQuantitySet($input: InventorySetQuantitiesInput!) {
          inventorySetQuantities(input: $input) {
            inventoryAdjustmentGroup { changes { name delta location { id name } } }
            userErrors { field message }
          }
        }
    "#;
    let seed = proxy.process_request(json_graphql_request(
        set_quantities,
        json!({
            "input": {
                "name": "available",
                "reason": "correction",
                "ignoreCompareQuantity": true,
                "quantities": [{
                    "inventoryItemId": inventory_item_id,
                    "locationId": location_id,
                    "quantity": 7
                }]
            }
        }),
    ));
    assert_eq!(
        seed.body["data"]["inventorySetQuantities"]["userErrors"],
        json!([])
    );

    let inventory_read = proxy.process_request(json_graphql_request(
        r#"
        query InventoryQuantityDownstreamRead($itemId: ID!) {
          inventoryItem(id: $itemId) {
            inventoryLevels(first: 5) {
              nodes {
                location { id name isActive fulfillsOnlineOrders }
                quantities(names: ["available"]) { name quantity }
              }
            }
          }
        }
        "#,
        json!({ "itemId": inventory_item_id }),
    ));
    assert_eq!(
        inventory_read.body["data"]["inventoryItem"]["inventoryLevels"]["nodes"],
        json!([{
            "location": {
                "id": location_id,
                "name": "Generic Add Location",
                "isActive": true,
                "fulfillsOnlineOrders": true
            },
            "quantities": [{ "name": "available", "quantity": 7 }]
        }])
    );
}

#[test]
fn generic_location_edit_stages_location_validates_and_downstream_reads() {
    let mut proxy = snapshot_proxy();
    let primary = proxy.process_request(json_graphql_request(
        r#"
        mutation LocationEditSeedPrimary($input: LocationAddInput!) {
          locationAdd(input: $input) { location { id name } userErrors { field code message } }
        }
        "#,
        json!({
            "input": {
                "name": "Edit Primary",
                "address": { "countryCode": "CA" }
            }
        }),
    ));
    assert_eq!(primary.status, 200);
    let primary_id = primary.body["data"]["locationAdd"]["location"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let backup = proxy.process_request(json_graphql_request(
        r#"
        mutation LocationEditSeedBackup($input: LocationAddInput!) {
          locationAdd(input: $input) { location { id name } userErrors { field code message } }
        }
        "#,
        json!({
            "input": {
                "name": "Edit Backup",
                "address": {
                    "address1": "1 Spadina",
                    "city": "Toronto",
                    "countryCode": "CA",
                    "zip": "M5T 2C2"
                }
            }
        }),
    ));
    assert_eq!(backup.status, 200);
    let backup_id = backup.body["data"]["locationAdd"]["location"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let duplicate = proxy.process_request(json_graphql_request(
        r#"
        mutation GenericLocationEditDuplicate($id: ID!, $input: LocationEditInput!) {
          locationEdit(id: $id, input: $input) {
            location { id }
            userErrors { field code message }
          }
        }
        "#,
        json!({ "id": backup_id, "input": { "name": "Edit Primary" } }),
    ));
    assert_eq!(
        duplicate.body["data"]["locationEdit"],
        json!({
            "location": null,
            "userErrors": [{
                "field": ["input", "name"],
                "code": "TAKEN",
                "message": "You already have a location with this name"
            }]
        })
    );

    let unknown = proxy.process_request(json_graphql_request(
        r#"
        mutation GenericLocationEditUnknown($id: ID!, $input: LocationEditInput!) {
          locationEdit(id: $id, input: $input) {
            location { id name }
            userErrors { field message }
          }
        }
        "#,
        json!({ "id": "gid://shopify/Location/999999999999", "input": { "name": "Nope" } }),
    ));
    assert_eq!(
        unknown.body["data"]["locationEdit"],
        json!({
            "location": null,
            "userErrors": [{
                "field": ["id"],
                "message": "Location not found."
            }]
        })
    );

    let invalid_country = proxy.process_request(json_graphql_request(
        r#"
        mutation GenericLocationEditInvalidCountry($id: ID!, $input: LocationEditInput!) {
          locationEdit(id: $id, input: $input) {
            location { id }
            userErrors { field code message }
          }
        }
        "#,
        json!({ "id": primary_id, "input": { "address": { "countryCode": "XX" } } }),
    ));
    assert_eq!(
        invalid_country.body["errors"][0]["extensions"]["code"],
        json!("INVALID_VARIABLE")
    );
    assert_eq!(
        invalid_country.body["errors"][0]["extensions"]["problems"][0]["path"],
        json!(["address", "countryCode"])
    );

    let edit = proxy.process_request(json_graphql_request(
        r#"
        mutation GenericLocationEdit($id: ID!, $input: LocationEditInput!) {
          locationEdit(id: $id, input: $input) {
            location {
              id
              name
              fulfillsOnlineOrders
              address { address1 city countryCode zip }
              metafield(namespace: "custom", key: "edit") { namespace key value type }
              metafields(first: 5, namespace: "custom") {
                nodes { namespace key value type }
                pageInfo { hasNextPage hasPreviousPage }
              }
            }
            userErrors { field code message }
          }
        }
        "#,
        json!({
            "id": primary_id,
            "input": {
                "name": "Edited Primary",
                "fulfillsOnlineOrders": false,
                "address": {
                    "address1": "2 Spadina",
                    "city": "Ottawa",
                    "countryCode": "CA",
                    "zip": "K1A 0B1"
                },
                "metafields": [{
                    "namespace": "custom",
                    "key": "edit",
                    "type": "single_line_text_field",
                    "value": "updated"
                }]
            }
        }),
    ));
    assert_eq!(
        edit.body["data"]["locationEdit"],
        json!({
            "location": {
                "id": primary_id,
                "name": "Edited Primary",
                "fulfillsOnlineOrders": false,
                "address": {
                    "address1": "2 Spadina",
                    "city": "Ottawa",
                    "countryCode": "CA",
                    "zip": "K1A 0B1"
                },
                "metafield": {
                    "namespace": "custom",
                    "key": "edit",
                    "value": "updated",
                    "type": "single_line_text_field"
                },
                "metafields": {
                    "nodes": [{
                        "namespace": "custom",
                        "key": "edit",
                        "value": "updated",
                        "type": "single_line_text_field"
                    }],
                    "pageInfo": { "hasNextPage": false, "hasPreviousPage": false }
                }
            },
            "userErrors": []
        })
    );

    let read = proxy.process_request(json_graphql_request(
        r#"
        query GenericLocationEditRead($id: ID!) {
          location(id: $id) { id name fulfillsOnlineOrders address { city } }
          locationByIdentifier(identifier: { id: $id }) { id name }
          locations(first: 5) { nodes { id name } }
        }
        "#,
        json!({ "id": primary_id }),
    ));
    assert_eq!(
        read.body["data"]["location"],
        json!({
            "id": primary_id,
            "name": "Edited Primary",
            "fulfillsOnlineOrders": false,
            "address": { "city": "Ottawa" }
        })
    );
    assert_eq!(
        read.body["data"]["locationByIdentifier"],
        json!({ "id": primary_id, "name": "Edited Primary" })
    );
    assert_eq!(
        read.body["data"]["locations"]["nodes"][0],
        json!({ "id": primary_id, "name": "Edited Primary" })
    );

    let log = log_snapshot(&proxy);
    let roots: Vec<_> = log["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["interpreted"]["primaryRootField"].as_str().unwrap())
        .collect();
    assert_eq!(roots, vec!["locationAdd", "locationAdd", "locationEdit"]);
    assert!(log["entries"][2]["rawBody"]
        .as_str()
        .unwrap()
        .contains("GenericLocationEdit"));
}

#[test]
fn generic_location_activate_stages_state_and_scope_guards() {
    let mut proxy = snapshot_proxy();
    let location_id = "gid://shopify/Location/activate-control";
    let activate = proxy.process_request(json_graphql_request(
        r#"
        mutation GenericLocationActivate($locationId: ID!) {
          locationActivate(locationId: $locationId) @idempotent(key: "generic-location-activate") {
            location { id isActive }
            locationActivateUserErrors { field code message }
          }
        }
        "#,
        json!({ "locationId": location_id }),
    ));
    assert_eq!(
        activate.body["data"]["locationActivate"],
        json!({
            "location": { "id": location_id, "isActive": true },
            "locationActivateUserErrors": []
        })
    );

    let read = proxy.process_request(json_graphql_request(
        r#"
        query GenericLocationActivateRead($id: ID!) {
          location(id: $id) { id isActive activatable deactivatable }
        }
        "#,
        json!({ "id": location_id }),
    ));
    assert_eq!(
        read.body["data"]["location"],
        json!({
            "id": location_id,
            "isActive": true,
            "activatable": true,
            "deactivatable": true
        })
    );

    let service = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateFs($name: String!) {
          fulfillmentServiceCreate(
            name: $name
            trackingSupport: true
            inventoryManagement: true
            requiresShippingMethod: true
          ) {
            fulfillmentService { location { id isActive isFulfillmentService } }
            userErrors { field message }
          }
        }
        "#,
        json!({ "name": "Generic Activation FS" }),
    ));
    let fs_location_id = service.body["data"]["fulfillmentServiceCreate"]["fulfillmentService"]
        ["location"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let fs_activate = proxy.process_request(json_graphql_request(
        r#"
        mutation GenericLocationActivateFs($locationId: ID!) {
          locationActivate(locationId: $locationId) @idempotent(key: "generic-location-activate-fs") {
            location { id isActive isFulfillmentService }
            locationActivateUserErrors { field code message }
          }
        }
        "#,
        json!({ "locationId": fs_location_id }),
    ));
    assert_eq!(
        fs_activate.body["data"]["locationActivate"],
        json!({
            "location": {
                "id": fs_location_id,
                "isActive": true,
                "isFulfillmentService": true
            },
            "locationActivateUserErrors": [{
                "field": ["locationId"],
                "code": "LOCATION_NOT_FOUND",
                "message": "Location not found."
            }]
        })
    );
}

#[test]
fn generic_location_activate_rejects_non_unique_active_name() {
    let active_duplicate_id = "gid://shopify/Location/active-duplicate-name";
    let target_id = "gid://shopify/Location/inactive-duplicate-name";
    let duplicate_name = "Duplicate Activation Name";
    let upstream_calls = Arc::new(Mutex::new(Vec::<Value>::new()));
    let captured_calls = Arc::clone(&upstream_calls);
    let mut proxy =
        configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport(move |request| {
            let body = serde_json::from_str::<Value>(&request.body).unwrap_or(Value::Null);
            captured_calls.lock().unwrap().push(body.clone());
            if body["query"].as_str().is_some_and(|query| {
                query.contains("locationsAvailableForDeliveryProfilesConnection")
            }) {
                Response {
                    status: 200,
                    headers: Default::default(),
                    body: json!({
                        "data": {
                            "locationsAvailableForDeliveryProfilesConnection": {
                                "nodes": [{
                                    "id": active_duplicate_id,
                                    "name": duplicate_name,
                                    "isActive": true,
                                    "isFulfillmentService": false
                                }],
                                "pageInfo": { "hasNextPage": false, "hasPreviousPage": false }
                            }
                        }
                    }),
                }
            } else if body["variables"]["id"] == target_id {
                Response {
                    status: 200,
                    headers: Default::default(),
                    body: json!({
                        "data": {
                            "location": {
                                "id": target_id,
                                "legacyResourceId": "inactive-duplicate-name",
                                "name": duplicate_name,
                                "activatable": true,
                                "addressVerified": true,
                                "createdAt": "2026-06-24T00:00:00Z",
                                "deactivatable": true,
                                "deactivatedAt": "2026-06-24T00:00:00Z",
                                "deletable": true,
                                "fulfillsOnlineOrders": false,
                                "hasActiveInventory": false,
                                "hasUnfulfilledOrders": false,
                                "isActive": false,
                                "isFulfillmentService": false,
                                "isPrimary": false,
                                "shipsInventory": false,
                                "updatedAt": "2026-06-24T00:00:00Z",
                                "fulfillmentService": null,
                                "address": null,
                                "suggestedAddresses": [],
                                "metafield": null,
                                "metafields": {
                                    "nodes": [],
                                    "pageInfo": {
                                        "hasNextPage": false,
                                        "hasPreviousPage": false,
                                        "startCursor": null,
                                        "endCursor": null
                                    }
                                },
                                "inventoryLevels": {
                                    "nodes": [],
                                    "pageInfo": {
                                        "hasNextPage": false,
                                        "hasPreviousPage": false,
                                        "startCursor": null,
                                        "endCursor": null
                                    }
                                }
                            }
                        }
                    }),
                }
            } else {
                Response {
                    status: 599,
                    headers: Default::default(),
                    body: json!({ "errors": [{ "message": "unexpected upstream call" }] }),
                }
            }
        });

    let observe = proxy.process_request(json_graphql_request(
        r#"
        query ObserveActiveDuplicate($first: Int!) {
          locationsAvailableForDeliveryProfilesConnection(first: $first) {
            nodes { id name isActive isFulfillmentService }
            pageInfo { hasNextPage hasPreviousPage }
          }
        }
        "#,
        json!({ "first": 1 }),
    ));
    assert_eq!(
        observe.body["data"]["locationsAvailableForDeliveryProfilesConnection"]["nodes"][0],
        json!({
            "id": active_duplicate_id,
            "name": duplicate_name,
            "isActive": true,
            "isFulfillmentService": false
        })
    );

    let activate = proxy.process_request(json_graphql_request(
        r#"
        mutation ActivateDuplicateName($locationId: ID!, $idempotencyKey: String!) {
          locationActivate(locationId: $locationId) @idempotent(key: $idempotencyKey) {
            location { id name isActive }
            locationActivateUserErrors { field code message }
          }
        }
        "#,
        json!({
            "locationId": target_id,
            "idempotencyKey": "activate-duplicate-name"
        }),
    ));
    assert_eq!(
        activate.body["data"]["locationActivate"],
        json!({
            "location": {
                "id": target_id,
                "name": duplicate_name,
                "isActive": false
            },
            "locationActivateUserErrors": [{
                "field": ["locationId"],
                "code": "HAS_NON_UNIQUE_NAME",
                "message": "This location currently cannot be activated because there exists an active location with the same name."
            }]
        })
    );

    let target_read = proxy.process_request(json_graphql_request(
        r#"
        query ReadRejectedDuplicateNameActivation($id: ID!) {
          location(id: $id) { id name isActive }
        }
        "#,
        json!({ "id": target_id }),
    ));
    assert_eq!(
        target_read.body["data"]["location"],
        json!({
            "id": target_id,
            "name": duplicate_name,
            "isActive": false
        })
    );
    assert_eq!(
        log_snapshot(&proxy)["entries"],
        json!([]),
        "rejected activation must not append a staged mutation log entry"
    );
    assert_eq!(upstream_calls.lock().unwrap().len(), 2);
}

#[test]
fn generic_location_delete_stages_tombstone_and_cascades_inventory_levels() {
    let mut proxy = snapshot_proxy();
    let target_add = proxy.process_request(json_graphql_request(
        r#"
        mutation LocationDeleteSeed($input: LocationAddInput!) {
          locationAdd(input: $input) {
            location { id name isActive hasActiveInventory }
            userErrors { field code message }
          }
        }
        "#,
        json!({
            "input": {
                "name": "Delete Target",
                "address": { "countryCode": "CA" }
            }
        }),
    ));
    let target_id = target_add.body["data"]["locationAdd"]["location"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let active_delete = proxy.process_request(json_graphql_request(
        r#"
        mutation GenericLocationDeleteActive($locationId: ID!) {
          locationDelete(locationId: $locationId) {
            deletedLocationId
            locationDeleteUserErrors { field code message }
          }
        }
        "#,
        json!({ "locationId": target_id }),
    ));
    assert_eq!(
        active_delete.body["data"]["locationDelete"],
        json!({
            "deletedLocationId": null,
            "locationDeleteUserErrors": [{
                "field": ["locationId"],
                "code": "LOCATION_IS_ACTIVE",
                "message": "The location cannot be deleted while it is active."
            }]
        })
    );

    let inventory_item_id = "gid://shopify/InventoryItem/delete-cascade";
    let seed_inventory = proxy.process_request(json_graphql_request(
        r#"
        mutation InventoryQuantitySet($input: InventorySetQuantitiesInput!) {
          inventorySetQuantities(input: $input) {
            inventoryAdjustmentGroup { changes { name delta location { id name } } }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "input": {
                "name": "available",
                "reason": "correction",
                "ignoreCompareQuantity": true,
                "quantities": [{
                    "inventoryItemId": inventory_item_id,
                    "locationId": target_id,
                    "quantity": 5
                }]
            }
        }),
    ));
    assert_eq!(
        seed_inventory.body["data"]["inventorySetQuantities"]["userErrors"],
        json!([])
    );

    let forced_inactive = proxy.process_request(json_graphql_request(
        r#"
        mutation GenericLocationDeleteForceInactive($id: ID!, $input: LocationEditInput!) {
          locationEdit(id: $id, input: $input) {
            location { id isActive hasActiveInventory }
            userErrors { field code message }
          }
        }
        "#,
        json!({ "id": target_id, "input": { "active": false } }),
    ));
    assert_eq!(
        forced_inactive.body["data"]["locationEdit"],
        json!({
            "location": {
                "id": target_id,
                "isActive": false,
                "hasActiveInventory": true
            },
            "userErrors": []
        })
    );

    let inventory_delete_guard = proxy.process_request(json_graphql_request(
        r#"
        mutation GenericLocationDeleteInventoryGuard($locationId: ID!) {
          locationDelete(locationId: $locationId) {
            deletedLocationId
            locationDeleteUserErrors { field code message }
          }
        }
        "#,
        json!({ "locationId": target_id }),
    ));
    assert_eq!(
        inventory_delete_guard.body["data"]["locationDelete"],
        json!({
            "deletedLocationId": null,
            "locationDeleteUserErrors": [{
                "field": ["locationId"],
                "code": "LOCATION_HAS_INVENTORY",
                "message": "The location cannot be deleted while it has inventory."
            }]
        })
    );

    let cleared = proxy.process_request(json_graphql_request(
        r#"
        mutation InventoryQuantityClear($input: InventorySetQuantitiesInput!) {
          inventorySetQuantities(input: $input) {
            inventoryAdjustmentGroup { changes { name delta location { id } } }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "input": {
                "name": "available",
                "reason": "correction",
                "ignoreCompareQuantity": true,
                "quantities": [{
                    "inventoryItemId": inventory_item_id,
                    "locationId": target_id,
                    "quantity": 0
                }]
            }
        }),
    ));
    assert_eq!(
        cleared.body["data"]["inventorySetQuantities"]["userErrors"],
        json!([])
    );

    let delete = proxy.process_request(json_graphql_request(
        r#"
        mutation GenericLocationDelete($locationId: ID!) {
          locationDelete(locationId: $locationId) {
            deletedLocationId
            locationDeleteUserErrors { field code message }
          }
        }
        "#,
        json!({ "locationId": target_id }),
    ));
    assert_eq!(
        delete.body["data"]["locationDelete"],
        json!({
            "deletedLocationId": target_id,
            "locationDeleteUserErrors": []
        })
    );

    let location_read = proxy.process_request(json_graphql_request(
        r#"
        query GenericLocationDeleteLocationRead($locationId: ID!) {
          location(id: $locationId) { id name }
          locationByIdentifier(identifier: { id: $locationId }) { id name }
          locations(first: 5) { nodes { id name } }
        }
        "#,
        json!({ "locationId": target_id }),
    ));
    assert_eq!(location_read.body["data"]["location"], Value::Null);
    assert_eq!(
        location_read.body["data"]["locationByIdentifier"],
        Value::Null
    );
    assert_eq!(
        location_read.body["data"]["locations"],
        json!({ "nodes": [] })
    );

    let inventory_read = proxy.process_request(json_graphql_request(
        r#"
        query GenericLocationDeleteInventoryRead($locationId: ID!, $itemId: ID!) {
          inventoryItem(id: $itemId) {
            locationsCount { count precision }
            inventoryLevel(locationId: $locationId) { id location { id } }
            inventoryLevels(first: 5) { nodes { location { id } quantities(names: ["available"]) { name quantity } } }
          }
        }
        "#,
        json!({ "locationId": target_id, "itemId": inventory_item_id }),
    ));
    assert_eq!(
        inventory_read.body["data"]["inventoryItem"],
        json!({
            "locationsCount": { "count": 0, "precision": "EXACT" },
            "inventoryLevel": null,
            "inventoryLevels": { "nodes": [] }
        })
    );
    assert_eq!(
        state_snapshot(&proxy)["stagedState"]["deletedLocationIds"],
        json!([target_id])
    );
}

#[test]
fn location_edit_and_delete_are_local_in_live_hybrid_mode() {
    let upstream_calls = Arc::new(Mutex::new(0usize));
    let upstream_requests = Arc::new(Mutex::new(Vec::<Value>::new()));
    let calls = Arc::clone(&upstream_calls);
    let requests = Arc::clone(&upstream_requests);
    let mut proxy =
        configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport(move |request| {
            *calls.lock().unwrap() += 1;
            let body = serde_json::from_str::<Value>(&request.body).unwrap_or(Value::Null);
            requests.lock().unwrap().push(body.clone());
            if body["operationName"] == "StorePropertiesLocationHydrate"
                && body["variables"]["id"] == "gid://shopify/Location/live-base"
            {
                Response {
                    status: 200,
                    headers: Default::default(),
                    body: json!({
                        "data": {
                            "location": {
                                "id": "gid://shopify/Location/live-base",
                                "name": "Live Base",
                                "isActive": true,
                                "deletable": false,
                                "fulfillsOnlineOrders": true,
                                "hasActiveInventory": true,
                                "hasUnfulfilledOrders": false,
                                "isFulfillmentService": false,
                                "shipsInventory": true
                            }
                        }
                    }),
                }
            } else {
                Response {
                    status: 599,
                    headers: Default::default(),
                    body: json!({ "errors": [{ "message": "unexpected upstream call" }] }),
                }
            }
        });

    let add = proxy.process_request(json_graphql_request(
        r#"
        mutation LocationLiveSeed($input: LocationAddInput!) {
          locationAdd(input: $input) { location { id name } userErrors { field message } }
        }
        "#,
        json!({ "input": { "name": "Live Local", "address": { "countryCode": "CA" } } }),
    ));
    let location_id = add.body["data"]["locationAdd"]["location"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let edit = proxy.process_request(json_graphql_request(
        r#"
        mutation LocationLiveEdit($id: ID!, $input: LocationEditInput!) {
          locationEdit(id: $id, input: $input) {
            location { id name }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "id": location_id, "input": { "name": "Live Local Edited" } }),
    ));
    assert_eq!(edit.status, 200);
    assert_eq!(
        edit.body["data"]["locationEdit"],
        json!({
            "location": { "id": location_id, "name": "Live Local Edited" },
            "userErrors": []
        })
    );

    let deactivate = proxy.process_request(json_graphql_request(
        r#"
        mutation LocationLiveForceInactive($id: ID!, $input: LocationEditInput!) {
          locationEdit(id: $id, input: $input) {
            location { id isActive }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "id": location_id, "input": { "active": false } }),
    ));
    assert_eq!(
        deactivate.body["data"]["locationEdit"]["location"]["isActive"],
        json!(false)
    );

    let delete = proxy.process_request(json_graphql_request(
        r#"
        mutation LocationLiveDelete($locationId: ID!) {
          locationDelete(locationId: $locationId) {
            deletedLocationId
            locationDeleteUserErrors { field message code }
          }
        }
        "#,
        json!({ "locationId": location_id }),
    ));
    assert_eq!(
        delete.body["data"]["locationDelete"],
        json!({
            "deletedLocationId": location_id,
            "locationDeleteUserErrors": []
        })
    );

    assert_eq!(*upstream_calls.lock().unwrap(), 0);

    let base_delete = proxy.process_request(json_graphql_request(
        r#"
        mutation LocationLiveBaseDelete($locationId: ID!) {
          locationDelete(locationId: $locationId) {
            deletedLocationId
            locationDeleteUserErrors { field message code }
          }
        }
        "#,
        json!({ "locationId": "gid://shopify/Location/live-base" }),
    ));
    assert_eq!(
        base_delete.body["data"]["locationDelete"],
        json!({
            "deletedLocationId": null,
            "locationDeleteUserErrors": [
                {
                    "field": ["locationId"],
                    "message": "The location cannot be deleted while it is active.",
                    "code": "LOCATION_IS_ACTIVE"
                },
                {
                    "field": ["locationId"],
                    "message": "The location cannot be deleted while it has inventory.",
                    "code": "LOCATION_HAS_INVENTORY"
                }
            ]
        })
    );
    assert_eq!(*upstream_calls.lock().unwrap(), 1);
    let requests = upstream_requests.lock().unwrap();
    assert_eq!(
        requests[0]["operationName"],
        json!("StorePropertiesLocationHydrate")
    );
    assert_eq!(
        requests[0]["variables"],
        json!({ "id": "gid://shopify/Location/live-base" })
    );
}

#[test]
fn location_deactivate_with_destination_relocates_and_merges_inventory_quantities() {
    let mut proxy = snapshot_proxy();
    let source_location_id = "gid://shopify/Location/1";
    let destination_location_id = "gid://shopify/Location/2";
    let inventory_item_id = "gid://shopify/InventoryItem/tracked";
    let set_quantities = r#"
        mutation InventoryQuantitySet($input: InventorySetQuantitiesInput!) {
          inventorySetQuantities(input: $input) {
            inventoryAdjustmentGroup { changes { name delta location { id } } }
            userErrors { field message }
          }
        }
    "#;

    let seed = proxy.process_request(json_graphql_request(
        set_quantities,
        json!({
            "input": {
                "name": "available",
                "reason": "correction",
                "ignoreCompareQuantity": true,
                "quantities": [
                    { "inventoryItemId": inventory_item_id, "locationId": source_location_id, "quantity": 5 },
                    { "inventoryItemId": inventory_item_id, "locationId": destination_location_id, "quantity": 9 }
                ]
            }
        }),
    ));
    assert_eq!(
        seed.body["data"]["inventorySetQuantities"]["userErrors"],
        json!([])
    );

    let deactivate = proxy.process_request(json_graphql_request(
        r#"
        mutation LocationDeactivateRelocation($source: ID!, $destination: ID!) {
          locationDeactivate(locationId: $source, destinationLocationId: $destination) @idempotent(key: "relocate") {
            location { isActive hasActiveInventory deletable }
            locationDeactivateUserErrors { field code message }
          }
        }
        "#,
        json!({ "source": source_location_id, "destination": destination_location_id }),
    ));
    assert_eq!(
        deactivate.body["data"]["locationDeactivate"],
        json!({
            "location": { "isActive": false, "hasActiveInventory": false, "deletable": true },
            "locationDeactivateUserErrors": []
        })
    );

    let read = proxy.process_request(json_graphql_request(
        r#"
        query InventoryQuantityDownstreamRead($itemId: ID!, $source: ID!) {
          inventoryItem(id: $itemId) {
            locationsCount { count precision }
            inventoryLevel(locationId: $source) {
              id
              location { id name }
              quantities(names: ["available", "on_hand"]) { name quantity }
            }
            inventoryLevels(first: 10) {
              nodes {
                id
                location { id name }
                quantities(names: ["available", "on_hand"]) { name quantity }
              }
            }
          }
        }
        "#,
        json!({ "itemId": inventory_item_id, "source": source_location_id }),
    ));

    assert_eq!(
        read.body["data"]["inventoryItem"],
        json!({
            "locationsCount": { "count": 1, "precision": "EXACT" },
            "inventoryLevel": null,
            "inventoryLevels": {
                "nodes": [{
                    "id": "gid://shopify/InventoryLevel/tracked-2?inventory_item_id=gid://shopify/InventoryItem/tracked",
                    "location": { "id": destination_location_id, "name": "Destination location" },
                    "quantities": [
                        { "name": "available", "quantity": 14 },
                        { "name": "on_hand", "quantity": 14 }
                    ]
                }]
            }
        })
    );
}

#[test]
fn location_deactivate_user_error_does_not_relocate_inventory_quantities() {
    let mut proxy = snapshot_proxy();
    let source_location_id = "gid://shopify/Location/1";
    let inactive_destination_location_id = "gid://shopify/Location/inactive";
    let inventory_item_id = "gid://shopify/InventoryItem/tracked";
    let set_quantities = r#"
        mutation InventoryQuantitySet($input: InventorySetQuantitiesInput!) {
          inventorySetQuantities(input: $input) {
            inventoryAdjustmentGroup { changes { name delta location { id } } }
            userErrors { field message }
          }
        }
    "#;

    let seed = proxy.process_request(json_graphql_request(
        set_quantities,
        json!({
            "input": {
                "name": "available",
                "reason": "correction",
                "ignoreCompareQuantity": true,
                "quantities": [
                    { "inventoryItemId": inventory_item_id, "locationId": source_location_id, "quantity": 5 },
                    { "inventoryItemId": inventory_item_id, "locationId": inactive_destination_location_id, "quantity": 9 }
                ]
            }
        }),
    ));
    assert_eq!(
        seed.body["data"]["inventorySetQuantities"]["userErrors"],
        json!([])
    );

    let deactivate = proxy.process_request(json_graphql_request(
        r#"
        mutation LocationDeactivateRelocationGuard($source: ID!, $destination: ID!) {
          locationDeactivate(locationId: $source, destinationLocationId: $destination) @idempotent(key: "no-relocate") {
            location { isActive hasActiveInventory deletable }
            locationDeactivateUserErrors { field code message }
          }
        }
        "#,
        json!({ "source": source_location_id, "destination": inactive_destination_location_id }),
    ));
    assert_eq!(
        deactivate.body["data"]["locationDeactivate"],
        json!({
            "location": { "isActive": true, "hasActiveInventory": true, "deletable": false },
            "locationDeactivateUserErrors": [{
                "field": ["destinationLocationId"],
                "code": "DESTINATION_LOCATION_NOT_FOUND_OR_INACTIVE",
                "message": "Location could not be deactivated because the destination location could be not found or is inactive."
            }]
        })
    );

    let read = proxy.process_request(json_graphql_request(
        r#"
        query InventoryQuantityDownstreamRead($itemId: ID!, $source: ID!) {
          inventoryItem(id: $itemId) {
            locationsCount { count precision }
            inventoryLevel(locationId: $source) {
              id
              location { id name }
              quantities(names: ["available", "on_hand"]) { name quantity }
            }
            inventoryLevels(first: 10) {
              nodes {
                location { id }
                quantities(names: ["available", "on_hand"]) { name quantity }
              }
            }
          }
        }
        "#,
        json!({ "itemId": inventory_item_id, "source": source_location_id }),
    ));

    assert_eq!(
        read.body["data"]["inventoryItem"],
        json!({
            "locationsCount": { "count": 2, "precision": "EXACT" },
            "inventoryLevel": {
                "id": "gid://shopify/InventoryLevel/tracked-1?inventory_item_id=gid://shopify/InventoryItem/tracked",
                "location": { "id": source_location_id, "name": "Source location" },
                "quantities": [
                    { "name": "available", "quantity": 5 },
                    { "name": "on_hand", "quantity": 5 }
                ]
            },
            "inventoryLevels": {
                "nodes": [
                    {
                        "location": { "id": source_location_id },
                        "quantities": [
                            { "name": "available", "quantity": 5 },
                            { "name": "on_hand", "quantity": 5 }
                        ]
                    },
                    {
                        "location": { "id": inactive_destination_location_id },
                        "quantities": [
                            { "name": "available", "quantity": 9 },
                            { "name": "on_hand", "quantity": 9 }
                        ]
                    }
                ]
            }
        })
    );
}

#[test]
fn location_deactivate_state_machine_errors_match_captured_codes_fields_and_location_state() {
    let mut proxy = snapshot_proxy();
    let query = r#"
        fragment LocationDeactivateStateMachineFields on Location {
          id
          name
          isActive
          activatable
          deactivatable
          fulfillsOnlineOrders
          hasActiveInventory
          hasUnfulfilledOrders
          deletable
          shipsInventory
        }

        mutation LocationDeactivateStateMachineWithDestination(
          $locationId: ID!
          $destinationLocationId: ID
          $idempotencyKey: String!
        ) {
          locationDeactivate(locationId: $locationId, destinationLocationId: $destinationLocationId)
            @idempotent(key: $idempotencyKey) {
            location { ...LocationDeactivateStateMachineFields }
            locationDeactivateUserErrors { field message code }
          }
        }
    "#;

    let same_id = proxy.process_request(json_graphql_request(
        query,
        json!({
            "locationId": "gid://shopify/Location/112849125682",
            "destinationLocationId": "gid://shopify/Location/112849125682",
            "idempotencyKey": "same"
        }),
    ));
    assert_eq!(
        same_id.body["data"]["locationDeactivate"],
        json!({
            "location": {
                "id": "gid://shopify/Location/112849125682",
                "name": "location-deactivate-state-machine source 20260506013233",
                "isActive": true,
                "activatable": true,
                "deactivatable": true,
                "fulfillsOnlineOrders": false,
                "hasActiveInventory": false,
                "hasUnfulfilledOrders": false,
                "deletable": false,
                "shipsInventory": false
            },
            "locationDeactivateUserErrors": [{
                "field": ["destinationLocationId"],
                "message": "Location could not be deactivated because the destination location cannot be set to the location to be deactivated.",
                "code": "DESTINATION_LOCATION_IS_THE_SAME_LOCATION"
            }]
        })
    );

    let active_inventory = proxy.process_request(json_graphql_request(
        query,
        json!({
            "locationId": "gid://shopify/Location/112849191218",
            "destinationLocationId": null,
            "idempotencyKey": "inventory"
        }),
    ));
    assert_eq!(
        active_inventory.body["data"]["locationDeactivate"]["locationDeactivateUserErrors"],
        json!([{
            "field": ["locationId"],
            "message": "Location could not be deactivated without specifying where to relocate inventory stocked at the location.",
            "code": "HAS_ACTIVE_INVENTORY_ERROR"
        }])
    );
    assert_eq!(
        active_inventory.body["data"]["locationDeactivate"]["location"]["hasActiveInventory"],
        json!(true)
    );

    let only_online = proxy.process_request(json_graphql_request(
        query,
        json!({
            "locationId": "gid://shopify/Location/112849223986",
            "destinationLocationId": null,
            "idempotencyKey": "online"
        }),
    ));
    assert_eq!(
        only_online.body["data"]["locationDeactivate"]["locationDeactivateUserErrors"],
        json!([{
            "field": ["locationId"],
            "message": "At least one location must fulfill online orders.",
            "code": "CANNOT_DISABLE_ONLINE_ORDER_FULFILLMENT"
        }])
    );
    assert_eq!(
        only_online.body["data"]["locationDeactivate"]["location"]["fulfillsOnlineOrders"],
        json!(true)
    );

    let permanent = proxy.process_request(json_graphql_request(
        query,
        json!({
            "locationId": "gid://shopify/Location/106318430514",
            "destinationLocationId": null,
            "idempotencyKey": "permanent"
        }),
    ));
    assert_eq!(
        permanent.body["data"]["locationDeactivate"]["locationDeactivateUserErrors"],
        json!([{
            "field": ["locationId"],
            "message": "Location could not be deactivated because it either has a fulfillment service or is the only location with a shipping address.",
            "code": "PERMANENTLY_BLOCKED_FROM_DEACTIVATION_ERROR"
        }])
    );
    assert_eq!(
        permanent.body["data"]["locationDeactivate"]["location"]["deactivatable"],
        json!(false)
    );
}

#[test]
fn location_by_identifier_custom_id_miss_returns_null_with_not_found_error() {
    let mut proxy = snapshot_proxy();
    let response = proxy.process_request(json_graphql_request(
        r#"
        query StorePropertiesLocationCustomIdMissing {
          unknownCustomIdentifier: locationByIdentifier(
            identifier: { customId: { namespace: "custom", key: "location_code", value: "missing" } }
          ) { id name }
        }
        "#,
        json!({}),
    ));

    assert_eq!(
        response.body["data"],
        json!({ "unknownCustomIdentifier": null })
    );
    assert_eq!(
        response.body["errors"][0]["message"],
        json!("Metafield definition of type 'id' is required when using custom ids.")
    );
    assert_eq!(
        response.body["errors"][0]["path"],
        json!(["unknownCustomIdentifier"])
    );
    assert_eq!(
        response.body["errors"][0]["extensions"]["code"],
        json!("NOT_FOUND")
    );
}

#[test]
fn fulfillment_order_hold_release_stages_real_numeric_ids_and_downstream_reads() {
    let order_id = "gid://shopify/Order/7001001";
    let fulfillment_order_id = "gid://shopify/FulfillmentOrder/1234567890";
    let line_item_id = "gid://shopify/FulfillmentOrderLineItem/2233445500";
    let mut proxy =
        snapshot_proxy().with_upstream_transport(fulfillment_order_hydrate_transport(vec![
            fulfillment_order_order_fixture(
                order_id,
                "#7001",
                fulfillment_order_id,
                line_item_id,
                2,
                "OPEN",
            ),
        ]));

    let hold = proxy.process_request(json_graphql_request(
        r#"
        mutation HoldNumericFulfillmentOrder($id: ID!, $fulfillmentHold: FulfillmentOrderHoldInput!) {
          fulfillmentOrderHold(id: $id, fulfillmentHold: $fulfillmentHold) {
            fulfillmentHold { id handle reason reasonNotes heldByRequestingApp }
            fulfillmentOrder { id status fulfillmentHolds { id handle } lineItems(first: 5) { nodes { id totalQuantity remainingQuantity lineItem { fulfillableQuantity } } } }
            remainingFulfillmentOrder { id status lineItems(first: 5) { nodes { totalQuantity remainingQuantity } } }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "id": fulfillment_order_id,
            "fulfillmentHold": {
                "reason": "OTHER",
                "reasonNotes": "wait",
                "handle": "numeric-hold",
                "fulfillmentOrderLineItems": [{ "id": line_item_id, "quantity": 1 }]
            }
        }),
    ));
    assert_eq!(hold.status, 200);
    let hold_payload = &hold.body["data"]["fulfillmentOrderHold"];
    assert_eq!(hold_payload["userErrors"], json!([]));
    assert_eq!(hold_payload["fulfillmentOrder"]["status"], json!("ON_HOLD"));
    assert_eq!(
        hold_payload["fulfillmentOrder"]["lineItems"]["nodes"][0]["remainingQuantity"],
        json!(1)
    );
    assert_eq!(
        hold_payload["remainingFulfillmentOrder"]["status"],
        json!("OPEN")
    );

    let after_hold = proxy.process_request(json_graphql_request(
        r#"
        query ReadHeldFulfillmentOrder($orderId: ID!, $fulfillmentOrderId: ID!) {
          order(id: $orderId) { id fulfillmentOrders(first: 10) { nodes { id status fulfillmentHolds { id handle } } } }
          fulfillmentOrder(id: $fulfillmentOrderId) { id status }
          manualHoldsFulfillmentOrders(first: 10) { nodes { id status } }
        }
        "#,
        json!({ "orderId": order_id, "fulfillmentOrderId": fulfillment_order_id }),
    ));
    assert_eq!(
        after_hold.body["data"]["order"]["fulfillmentOrders"]["nodes"][0]["status"],
        json!("ON_HOLD")
    );
    assert_eq!(
        after_hold.body["data"]["manualHoldsFulfillmentOrders"]["nodes"][0]["id"],
        json!(fulfillment_order_id)
    );

    let hold_id = hold_payload["fulfillmentHold"]["id"].as_str().unwrap();
    let release = proxy.process_request(json_graphql_request(
        r#"
        mutation ReleaseNumericFulfillmentOrder($id: ID!, $holdIds: [ID!]) {
          fulfillmentOrderReleaseHold(id: $id, holdIds: $holdIds) {
            fulfillmentOrder { id status fulfillmentHolds { id } lineItems(first: 5) { nodes { totalQuantity remainingQuantity } } }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "id": fulfillment_order_id, "holdIds": [hold_id] }),
    ));
    assert_eq!(release.status, 200);
    assert_eq!(
        release.body["data"]["fulfillmentOrderReleaseHold"]["fulfillmentOrder"]["status"],
        json!("OPEN")
    );
    assert_eq!(
        release.body["data"]["fulfillmentOrderReleaseHold"]["fulfillmentOrder"]["lineItems"]
            ["nodes"][0]["totalQuantity"],
        json!(2)
    );

    let after_release = proxy.process_request(json_graphql_request(
        r#"
        query ReadReleasedFulfillmentOrder($orderId: ID!) {
          order(id: $orderId) { fulfillmentOrders(first: 10) { nodes { id status lineItems(first: 5) { nodes { totalQuantity remainingQuantity } } } } }
          manualHoldsFulfillmentOrders(first: 10) { nodes { id } }
        }
        "#,
        json!({ "orderId": order_id }),
    ));
    assert_eq!(
        after_release.body["data"]["manualHoldsFulfillmentOrders"]["nodes"],
        json!([])
    );
    assert_eq!(
        after_release.body["data"]["order"]["fulfillmentOrders"]["nodes"][1]["status"],
        json!("CLOSED")
    );
    assert!(log_snapshot(&proxy)["entries"][0]["rawBody"]
        .as_str()
        .unwrap()
        .contains("HoldNumericFulfillmentOrder"));
}

#[test]
fn fulfillment_order_status_deadline_move_and_cancel_stage_real_numeric_ids() {
    let order_id = "gid://shopify/Order/7002001";
    let fulfillment_order_id = "gid://shopify/FulfillmentOrder/2234567890";
    let line_item_id = "gid://shopify/FulfillmentOrderLineItem/3233445500";
    let mut proxy =
        snapshot_proxy().with_upstream_transport(fulfillment_order_hydrate_transport(vec![
            fulfillment_order_order_fixture(
                order_id,
                "#7002",
                fulfillment_order_id,
                line_item_id,
                2,
                "SCHEDULED",
            ),
        ]));

    let open = proxy.process_request(json_graphql_request(
        r#"
        mutation OpenNumericFulfillmentOrder($id: ID!) {
          fulfillmentOrderOpen(id: $id) {
            fulfillmentOrder { id status supportedActions { action } }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "id": fulfillment_order_id }),
    ));
    assert_eq!(
        open.body["data"]["fulfillmentOrderOpen"]["fulfillmentOrder"]["status"],
        json!("OPEN")
    );

    let move_response = proxy.process_request(json_graphql_request(
        r#"
        mutation MoveNumericFulfillmentOrder($id: ID!, $newLocationId: ID!, $fulfillmentOrderLineItems: [FulfillmentOrderLineItemInput!]) {
          fulfillmentOrderMove(id: $id, newLocationId: $newLocationId, fulfillmentOrderLineItems: $fulfillmentOrderLineItems) {
            movedFulfillmentOrder { id status assignedLocation { location { id } } lineItems(first: 5) { nodes { remainingQuantity } } }
            originalFulfillmentOrder { id lineItems(first: 5) { nodes { remainingQuantity } } }
            remainingFulfillmentOrder { id lineItems(first: 5) { nodes { remainingQuantity } } }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "id": fulfillment_order_id,
            "newLocationId": "gid://shopify/Location/55",
            "fulfillmentOrderLineItems": [{ "id": line_item_id, "quantity": 1 }]
        }),
    ));
    let moved_id = move_response.body["data"]["fulfillmentOrderMove"]["movedFulfillmentOrder"]
        ["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_ne!(moved_id, fulfillment_order_id);
    assert_eq!(
        move_response.body["data"]["fulfillmentOrderMove"]["movedFulfillmentOrder"]
            ["assignedLocation"]["location"]["id"],
        json!("gid://shopify/Location/55")
    );

    let progress = proxy.process_request(json_graphql_request(
        r#"
        mutation ProgressNumericFulfillmentOrder($id: ID!) {
          fulfillmentOrderReportProgress(id: $id, progressReport: { reasonNotes: "working" }) {
            fulfillmentOrder { id status }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "id": fulfillment_order_id }),
    ));
    assert_eq!(
        progress.body["data"]["fulfillmentOrderReportProgress"]["fulfillmentOrder"]["status"],
        json!("IN_PROGRESS")
    );

    let reopen = proxy.process_request(json_graphql_request(
        r#"
        mutation ReopenNumericFulfillmentOrder($id: ID!) {
          fulfillmentOrderOpen(id: $id) {
            fulfillmentOrder { id status }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "id": fulfillment_order_id }),
    ));
    assert_eq!(
        reopen.body["data"]["fulfillmentOrderOpen"]["fulfillmentOrder"]["status"],
        json!("OPEN")
    );

    let deadline = proxy.process_request(json_graphql_request(
        r#"
        mutation DeadlineNumericFulfillmentOrder($fulfillmentOrderIds: [ID!]!, $fulfillmentDeadline: DateTime!) {
          fulfillmentOrdersSetFulfillmentDeadline(fulfillmentOrderIds: $fulfillmentOrderIds, fulfillmentDeadline: $fulfillmentDeadline) {
            success
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "fulfillmentOrderIds": [fulfillment_order_id, moved_id],
            "fulfillmentDeadline": "2026-12-01T00:00:00.000Z"
        }),
    ));
    assert_eq!(
        deadline.body["data"]["fulfillmentOrdersSetFulfillmentDeadline"]["success"],
        json!(true)
    );

    let cancel = proxy.process_request(json_graphql_request(
        r#"
        mutation CancelNumericFulfillmentOrder($id: ID!) {
          fulfillmentOrderCancel(id: $id) {
            fulfillmentOrder { id status lineItems(first: 5) { nodes { id } } }
            replacementFulfillmentOrder { id status lineItems(first: 5) { nodes { remainingQuantity } } }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "id": fulfillment_order_id }),
    ));
    assert_eq!(
        cancel.body["data"]["fulfillmentOrderCancel"]["fulfillmentOrder"]["status"],
        json!("CLOSED")
    );
    assert_eq!(
        cancel.body["data"]["fulfillmentOrderCancel"]["replacementFulfillmentOrder"]["status"],
        json!("OPEN")
    );

    let read = proxy.process_request(json_graphql_request(
        r#"
        query ReadMovedDeadlineAndCancelledFulfillmentOrders($orderId: ID!) {
          order(id: $orderId) {
            displayFulfillmentStatus
            fulfillmentOrders(first: 10) { nodes { id status fulfillBy } }
          }
          fulfillmentOrders(first: 10, includeClosed: true) { nodes { id status fulfillBy } }
        }
        "#,
        json!({ "orderId": order_id }),
    ));
    let nodes = read.body["data"]["order"]["fulfillmentOrders"]["nodes"]
        .as_array()
        .unwrap();
    assert!(nodes
        .iter()
        .any(|node| node["id"] == json!(moved_id)
            && node["fulfillBy"] == json!("2026-12-01T00:00:00Z")));
    assert!(nodes.iter().any(|node| node["status"] == json!("CLOSED")));
}

#[test]
fn fulfillment_order_close_stages_after_accepted_request_passthrough_observation() {
    let fulfillment_order_id = "gid://shopify/FulfillmentOrder/4234567890";
    let order_id = "gid://shopify/Order/7003001";
    let order = fulfillment_order_order_fixture(
        order_id,
        "#7003",
        fulfillment_order_id,
        "gid://shopify/FulfillmentOrderLineItem/4233445500",
        1,
        "IN_PROGRESS",
    );
    let mut hydrated_fulfillment_order = order["fulfillmentOrders"]["nodes"][0].clone();
    hydrated_fulfillment_order["requestStatus"] = json!("ACCEPTED");
    hydrated_fulfillment_order["supportedActions"] = json!([
        { "action": "REQUEST_FULFILLMENT" },
        { "action": "CREATE_FULFILLMENT" },
        { "action": "HOLD" },
        { "action": "MOVE" }
    ]);
    hydrated_fulfillment_order["order"] = json!({
        "id": order_id,
        "name": "#7003",
        "displayFulfillmentStatus": "IN_PROGRESS"
    });
    let hydrate_record = hydrated_fulfillment_order.clone();
    let mut proxy =
        configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport(move |request| {
            let body: Value = serde_json::from_str(&request.body).unwrap();
            let query = body["query"].as_str().unwrap_or_default();
            let response_body = if query.contains("fulfillmentOrderAcceptFulfillmentRequest") {
                json!({
                    "data": {
                        "fulfillmentOrderAcceptFulfillmentRequest": {
                            "fulfillmentOrder": {
                                "id": fulfillment_order_id,
                                "status": "IN_PROGRESS",
                                "requestStatus": "ACCEPTED"
                            },
                            "userErrors": []
                        }
                    }
                })
            } else {
                json!({ "data": { "node": hydrate_record.clone() } })
            };
            Response {
                status: 200,
                headers: Default::default(),
                body: response_body,
            }
        });

    let accept = proxy.process_request(json_graphql_request(
        r#"
        mutation AcceptRequest($id: ID!) {
          fulfillmentOrderAcceptFulfillmentRequest(id: $id) {
            fulfillmentOrder { id status requestStatus }
            userErrors { field message }
          }
        }
        "#,
        json!({ "id": fulfillment_order_id }),
    ));
    assert_eq!(accept.status, 200);

    let close = proxy.process_request(json_graphql_request(
        r#"
        mutation CloseAcceptedFulfillmentOrder($id: ID!) {
          fulfillmentOrderClose(id: $id, message: "done") {
            fulfillmentOrder {
              id
              status
              requestStatus
              fulfillBy
              assignedLocation { location { id } }
            }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "id": fulfillment_order_id }),
    ));
    assert_eq!(
        close.body["data"]["fulfillmentOrderClose"]["fulfillmentOrder"]["status"],
        json!("INCOMPLETE")
    );
    assert_eq!(
        close.body["data"]["fulfillmentOrderClose"]["fulfillmentOrder"]["requestStatus"],
        json!("CLOSED")
    );
    assert_eq!(
        close.body["data"]["fulfillmentOrderClose"]["userErrors"],
        json!([])
    );

    let read = proxy.process_request(json_graphql_request(
        r#"
        query ReadClosedFulfillmentOrder($id: ID!) {
          fulfillmentOrder(id: $id) { id status requestStatus fulfillBy }
        }
        "#,
        json!({ "id": fulfillment_order_id }),
    ));
    assert_eq!(
        read.body["data"]["fulfillmentOrder"]["requestStatus"],
        json!("CLOSED")
    );
}

#[test]
fn fulfillment_order_close_reschedule_and_reroute_return_guardrail_payloads() {
    let mut proxy = snapshot_proxy();

    let close = proxy.process_request(json_graphql_request(
        r#"
        mutation CloseNumericFulfillmentOrder($id: ID!) {
          fulfillmentOrderClose(id: $id, message: "done") {
            fulfillmentOrder { id status }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "id": "gid://shopify/FulfillmentOrder/3234567890" }),
    ));
    assert_eq!(close.status, 200);
    assert_eq!(
        close.body["data"]["fulfillmentOrderClose"]["userErrors"][0]["message"],
        json!("The fulfillment order's assigned fulfillment service must be of api type")
    );

    let reschedule = proxy.process_request(json_graphql_request(
        r#"
        mutation RescheduleNumericFulfillmentOrder($id: ID!) {
          fulfillmentOrderReschedule(id: $id, fulfillAt: "2026-12-01T00:00:00Z") {
            fulfillmentOrder { id status }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "id": "gid://shopify/FulfillmentOrder/3234567890" }),
    ));
    assert_eq!(
        reschedule.body["data"]["fulfillmentOrderReschedule"]["userErrors"][0]["message"],
        json!("Fulfillment order must be scheduled.")
    );

    let reroute = proxy.process_request(json_graphql_request(
        r#"
        mutation RerouteNumericFulfillmentOrder($fulfillmentOrderIds: [ID!]!) {
          fulfillmentOrdersReroute(fulfillmentOrderIds: $fulfillmentOrderIds, includedLocationIds: ["gid://shopify/Location/55"]) {
            movedFulfillmentOrders { id }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "fulfillmentOrderIds": ["gid://shopify/FulfillmentOrder/3234567890"] }),
    ));
    assert_eq!(
        reroute.body["data"]["fulfillmentOrdersReroute"]["movedFulfillmentOrders"],
        json!([])
    );
    assert_eq!(
        reroute.body["data"]["fulfillmentOrdersReroute"]["userErrors"][0]["code"],
        json!("NOT_IMPLEMENTED")
    );
}

#[test]
fn fulfillment_order_move_assignment_status_allows_cancellation_assignment_states() {
    let mut proxy = snapshot_proxy();
    let query = r#"
        fragment FulfillmentOrderMoveValidationFields on FulfillmentOrder {
          id
          status
          requestStatus
          updatedAt
          assignedLocation { name location { id name } }
          lineItems(first: 5) { nodes { id totalQuantity remainingQuantity lineItem { id title quantity fulfillableQuantity } } }
        }
        mutation FulfillmentOrderMoveValidationMove($id: ID!, $newLocationId: ID!, $fulfillmentOrderLineItems: [FulfillmentOrderLineItemInput!]) {
          fulfillmentOrderMove(id: $id, newLocationId: $newLocationId, fulfillmentOrderLineItems: $fulfillmentOrderLineItems) {
            movedFulfillmentOrder { ...FulfillmentOrderMoveValidationFields }
            originalFulfillmentOrder { ...FulfillmentOrderMoveValidationFields }
            remainingFulfillmentOrder { ...FulfillmentOrderMoveValidationFields }
            userErrors { field message code }
          }
        }
    "#;

    for id in [
        "gid://shopify/FulfillmentOrder/move-assignment-cancellation-requested",
        "gid://shopify/FulfillmentOrder/move-assignment-cancellation-rejected",
    ] {
        let response = proxy.process_request(json_graphql_request(
            query,
            json!({
                "id": id,
                "newLocationId": "gid://shopify/Location/move-assignment-destination",
                "fulfillmentOrderLineItems": null
            }),
        ));
        let payload = &response.body["data"]["fulfillmentOrderMove"];
        assert_eq!(
            payload["movedFulfillmentOrder"]["assignedLocation"]["location"]["id"],
            json!("gid://shopify/Location/move-assignment-destination")
        );
        assert_eq!(
            payload["originalFulfillmentOrder"]["assignedLocation"]["location"]["id"],
            json!("gid://shopify/Location/move-assignment-destination")
        );
        assert_eq!(payload["remainingFulfillmentOrder"], json!(null));
        assert_eq!(payload["userErrors"], json!([]));
    }

    let submitted = proxy.process_request(json_graphql_request(
        query,
        json!({
            "id": "gid://shopify/FulfillmentOrder/move-assignment-submitted",
            "newLocationId": "gid://shopify/Location/move-assignment-destination",
            "fulfillmentOrderLineItems": null
        }),
    ));
    assert_eq!(
        submitted.body["data"]["fulfillmentOrderMove"],
        json!({
            "movedFulfillmentOrder": null,
            "originalFulfillmentOrder": null,
            "remainingFulfillmentOrder": null,
            "userErrors": [{
                "field": null,
                "message": "Cannot move submitted fulfillment order that is at a 3PL fulfillment service.",
                "code": null
            }]
        })
    );
}

#[test]
fn fulfillment_order_open_rejects_already_open_without_mutating_hydrated_order() {
    let order_id = "gid://shopify/Order/7002002";
    let fulfillment_order_id = "gid://shopify/FulfillmentOrder/2234567891";
    let mut proxy =
        snapshot_proxy().with_upstream_transport(fulfillment_order_hydrate_transport(vec![
            fulfillment_order_order_fixture(
                order_id,
                "#7003",
                fulfillment_order_id,
                "gid://shopify/FulfillmentOrderLineItem/3233445501",
                2,
                "OPEN",
            ),
        ]));

    let rejected = proxy.process_request(json_graphql_request(
        r#"
        mutation OpenAlreadyOpenFulfillmentOrder($id: ID!) {
          fulfillmentOrderOpen(id: $id) {
            fulfillmentOrder { id status updatedAt supportedActions { action } }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "id": fulfillment_order_id }),
    ));
    assert_eq!(
        rejected.body["data"]["fulfillmentOrderOpen"],
        json!({
            "fulfillmentOrder": null,
            "userErrors": [{
                "field": null,
                "message": "Expected fulfillment order status to be valid but it was open.",
                "code": "INVALID_FULFILLMENT_ORDER_STATUS"
            }]
        })
    );

    let after_rejection = proxy.process_request(json_graphql_request(
        r#"
        query ReadOpenFulfillmentOrderAfterRejectedOpen($orderId: ID!) {
          order(id: $orderId) {
            id
            fulfillmentOrders(first: 10) {
              nodes { id status updatedAt supportedActions { action } }
            }
          }
        }
        "#,
        json!({ "orderId": order_id }),
    ));
    assert_eq!(
        after_rejection.body["data"]["order"],
        json!({
            "id": order_id,
            "fulfillmentOrders": { "nodes": [{
                "id": fulfillment_order_id,
                "status": "OPEN",
                "updatedAt": "2026-06-15T11:00:00Z",
                "supportedActions": [
                    { "action": "CREATE_FULFILLMENT" },
                    { "action": "REPORT_PROGRESS" },
                    { "action": "MOVE" },
                    { "action": "HOLD" },
                    { "action": "SPLIT" }
                ]
            }] }
        })
    );
    assert_eq!(log_snapshot(&proxy)["entries"], json!([]));
}

#[test]
fn fulfillment_order_status_precondition_rejections_do_not_mutate_order_reads() {
    let mut proxy = snapshot_proxy();
    let open = proxy.process_request(json_graphql_request(
        r#"
        mutation FulfillmentOrderStatusPreconditionOpen($id: ID!) {
          fulfillmentOrderOpen(id: $id) {
            fulfillmentOrder { id status updatedAt supportedActions { action } }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "id": "gid://shopify/FulfillmentOrder/status-precondition-open-closed" }),
    ));
    assert_eq!(
        open.body["data"]["fulfillmentOrderOpen"],
        json!({
            "fulfillmentOrder": null,
            "userErrors": [{
                "field": ["id"],
                "message": "Fulfillment order must be scheduled.",
                "code": "INVALID_FULFILLMENT_ORDER_STATUS"
            }]
        })
    );

    let after_open = proxy.process_request(json_graphql_request(
        r#"
        query FulfillmentOrderStatusPreconditionOrderRead($orderId: ID!) {
          order(id: $orderId) {
            id
            fulfillmentOrders(first: 10, includeClosed: true) {
              nodes { id status updatedAt supportedActions { action } }
            }
          }
        }
        "#,
        json!({ "orderId": "gid://shopify/Order/status-precondition-open-closed" }),
    ));
    assert_eq!(
        after_open.body["data"]["order"],
        json!({
            "id": "gid://shopify/Order/status-precondition-open-closed",
            "fulfillmentOrders": { "nodes": [{
                "id": "gid://shopify/FulfillmentOrder/status-precondition-open-closed",
                "status": "CLOSED",
                "updatedAt": "2026-05-11T10:00:00Z",
                "supportedActions": []
            }] }
        })
    );

    let progress = proxy.process_request(json_graphql_request(
        r#"
        mutation FulfillmentOrderStatusPreconditionReportProgress($id: ID!, $progressReport: FulfillmentOrderReportProgressInput) {
          fulfillmentOrderReportProgress(id: $id, progressReport: $progressReport) {
            fulfillmentOrder { id status updatedAt supportedActions { action } }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "id": "gid://shopify/FulfillmentOrder/status-precondition-progress-scheduled",
            "progressReport": { "reasonNotes": "local-runtime progress precondition" }
        }),
    ));
    assert_eq!(
        progress.body["data"]["fulfillmentOrderReportProgress"],
        json!({
            "fulfillmentOrder": null,
            "userErrors": [{
                "field": ["id"],
                "message": "Fulfillment order must be in progress.",
                "code": "INVALID_FULFILLMENT_ORDER_STATUS"
            }]
        })
    );

    let after_progress = proxy.process_request(json_graphql_request(
        r#"
        query FulfillmentOrderStatusPreconditionOrderRead($orderId: ID!) {
          order(id: $orderId) {
            id
            fulfillmentOrders(first: 10, includeClosed: true) {
              nodes { id status updatedAt supportedActions { action } }
            }
          }
        }
        "#,
        json!({ "orderId": "gid://shopify/Order/status-precondition-progress-scheduled" }),
    ));
    assert_eq!(
        after_progress.body["data"]["order"],
        json!({
            "id": "gid://shopify/Order/status-precondition-progress-scheduled",
            "fulfillmentOrders": { "nodes": [{
                "id": "gid://shopify/FulfillmentOrder/status-precondition-progress-scheduled",
                "status": "SCHEDULED",
                "updatedAt": "2026-05-11T10:05:00Z",
                "supportedActions": [{ "action": "MARK_AS_OPEN" }]
            }] }
        })
    );
}

#[test]
fn fulfillment_order_deadline_validation_is_atomic_and_stages_successful_open_orders() {
    let mut proxy = snapshot_proxy();
    let read_query = r#"
        query FulfillmentOrdersSetDeadlineValidationOrderRead($id: ID!) {
          order(id: $id) {
            id name displayFulfillmentStatus
            fulfillmentOrders(first: 10) { nodes { id status fulfillBy } }
          }
        }
    "#;
    let mutation = r#"
        mutation FulfillmentOrdersSetDeadlineValidation($fulfillmentOrderIds: [ID!]!, $fulfillmentDeadline: DateTime!) {
          fulfillmentOrdersSetFulfillmentDeadline(fulfillmentOrderIds: $fulfillmentOrderIds, fulfillmentDeadline: $fulfillmentDeadline) {
            success
            userErrors { field message code }
          }
        }
    "#;

    let unknown = proxy.process_request(json_graphql_request(
        mutation,
        json!({
            "fulfillmentOrderIds": ["gid://shopify/FulfillmentOrder/9999999"],
            "fulfillmentDeadline": "2026-12-01T00:00:00Z"
        }),
    ));
    assert_eq!(
        unknown.body["data"]["fulfillmentOrdersSetFulfillmentDeadline"],
        json!({
            "success": false,
            "userErrors": [{
                "field": ["base"],
                "message": "The fulfillment orders could not be found.",
                "code": "FULFILLMENT_ORDERS_NOT_FOUND"
            }]
        })
    );

    let mixed = proxy.process_request(json_graphql_request(
        mutation,
        json!({
            "fulfillmentOrderIds": ["gid://shopify/FulfillmentOrder/deadline-open-a", "gid://shopify/FulfillmentOrder/9999999"],
            "fulfillmentDeadline": "2026-12-01T00:00:00Z"
        }),
    ));
    assert_eq!(
        mixed.body["data"]["fulfillmentOrdersSetFulfillmentDeadline"],
        unknown.body["data"]["fulfillmentOrdersSetFulfillmentDeadline"]
    );

    let after_mixed = proxy.process_request(json_graphql_request(
        read_query,
        json!({ "id": "gid://shopify/Order/deadline-validation" }),
    ));
    assert_eq!(
        after_mixed.body["data"]["order"]["fulfillmentOrders"]["nodes"][0]["fulfillBy"],
        json!(null)
    );

    for id in [
        "gid://shopify/FulfillmentOrder/deadline-closed",
        "gid://shopify/FulfillmentOrder/deadline-cancelled",
    ] {
        let rejected = proxy.process_request(json_graphql_request(
            mutation,
            json!({
                "fulfillmentOrderIds": [id],
                "fulfillmentDeadline": "2026-12-01T00:00:00Z"
            }),
        ));
        assert_eq!(
            rejected.body["data"]["fulfillmentOrdersSetFulfillmentDeadline"],
            json!({
                "success": false,
                "userErrors": [{
                    "field": ["base"],
                    "message": "The fulfillment order is closed or cancelled and cannot be assigned a fulfillment deadline.",
                    "code": null
                }]
            })
        );
    }

    let happy = proxy.process_request(json_graphql_request(
        mutation,
        json!({
            "fulfillmentOrderIds": ["gid://shopify/FulfillmentOrder/deadline-open-a", "gid://shopify/FulfillmentOrder/deadline-open-b"],
            "fulfillmentDeadline": "2026-12-01T00:00:00Z"
        }),
    ));
    assert_eq!(
        happy.body["data"]["fulfillmentOrdersSetFulfillmentDeadline"],
        json!({ "success": true, "userErrors": [] })
    );

    let after_happy = proxy.process_request(json_graphql_request(
        read_query,
        json!({ "id": "gid://shopify/Order/deadline-validation" }),
    ));
    assert_eq!(
        after_happy.body["data"]["order"],
        json!({
            "id": "gid://shopify/Order/deadline-validation",
            "name": "#DEADLINE-VALIDATION",
            "displayFulfillmentStatus": "UNFULFILLED",
            "fulfillmentOrders": { "nodes": [
                { "id": "gid://shopify/FulfillmentOrder/deadline-open-a", "status": "OPEN", "fulfillBy": "2026-12-01T00:00:00Z" },
                { "id": "gid://shopify/FulfillmentOrder/deadline-open-b", "status": "OPEN", "fulfillBy": "2026-12-01T00:00:00Z" },
                { "id": "gid://shopify/FulfillmentOrder/deadline-closed", "status": "CLOSED", "fulfillBy": null },
                { "id": "gid://shopify/FulfillmentOrder/deadline-cancelled", "status": "CANCELLED", "fulfillBy": null }
            ] }
        })
    );
}

#[test]
fn fulfillment_order_request_lifecycle_direct_read_preserves_submitted_request_status() {
    let mut proxy = snapshot_proxy();
    let (_, fulfillment_order) = create_fulfillment_order_test_order(&mut proxy, 1);
    let fulfillment_order_id = fulfillment_order["id"].clone();
    let line_item_id = fulfillment_order["lineItems"]["nodes"][0]["id"].clone();

    let submit = proxy.process_request(json_graphql_request(
        r#"
        mutation SubmitFulfillmentOrderForDirectRead(
          $id: ID!
          $lineItems: [FulfillmentOrderLineItemInput!]
        ) {
          fulfillmentOrderSubmitFulfillmentRequest(
            id: $id
            message: "Hermes partial submit"
            notifyCustomer: false
            fulfillmentOrderLineItems: $lineItems
          ) {
            submittedFulfillmentOrder { id requestStatus }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "id": fulfillment_order_id,
            "lineItems": [{
                "id": line_item_id,
                "quantity": 1
            }]
        }),
    ));
    assert_eq!(
        submit.body["data"]["fulfillmentOrderSubmitFulfillmentRequest"]["userErrors"],
        json!([])
    );

    let response = proxy.process_request(json_graphql_request(
        r#"
        query FulfillmentOrderRequestDirectRead($id: ID!) {
          fulfillmentOrder(id: $id) {
            id
            status
            requestStatus
            merchantRequests(first: 10) { nodes { kind message requestOptions responseData } }
            lineItems(first: 5) { nodes { totalQuantity remainingQuantity lineItem { id title } } }
          }
        }
        "#,
        json!({ "id": fulfillment_order_id }),
    ));
    assert_eq!(
        response.body["data"]["fulfillmentOrder"]["requestStatus"],
        json!("SUBMITTED")
    );
    assert_eq!(
        response.body["data"]["fulfillmentOrder"]["merchantRequests"]["nodes"][0]["message"],
        json!("Hermes partial submit")
    );
}

#[test]
fn store_property_node_reads_resolve_known_shop_records_locally() {
    let mut proxy = configured_proxy(ReadMode::LiveHybrid, None);
    let query = r#"
        query AdminPlatformStorePropertyNodeReads {
          shopAddressNode: node(id: "gid://shopify/ShopAddress/63755419881") { ... on ShopAddress { id address1 city country formatted } }
          shopPolicyNode: node(id: "gid://shopify/ShopPolicy/42438689001") { ... on ShopPolicy { id title type translations(locale: "fr") { key locale value } } }
          nodes(ids: ["gid://shopify/ShopAddress/63755419881", "gid://shopify/ShopPolicy/42438689001"]) {
            ... on ShopAddress { id address1 city country formatted }
            ... on ShopPolicy { id title type translations(locale: "fr") { key locale value } }
          }
        }
    "#;

    let response = proxy.process_request(json_graphql_request(query, json!({})));

    assert_eq!(response.status, 200);
    assert_eq!(
        response.body,
        json!({
            "data": {
                "shopAddressNode": {
                    "id": "gid://shopify/ShopAddress/63755419881",
                    "address1": "103 ossington",
                    "city": "Ottawa",
                    "country": "Canada",
                    "formatted": ["103 ossington", "Ottawa ON k1s3b7", "Canada"]
                },
                "shopPolicyNode": {
                    "id": "gid://shopify/ShopPolicy/42438689001",
                    "title": "Contact",
                    "type": "CONTACT_INFORMATION",
                    "translations": []
                },
                "nodes": [
                    {
                        "id": "gid://shopify/ShopAddress/63755419881",
                        "address1": "103 ossington",
                        "city": "Ottawa",
                        "country": "Canada",
                        "formatted": ["103 ossington", "Ottawa ON k1s3b7", "Canada"]
                    },
                    {
                        "id": "gid://shopify/ShopPolicy/42438689001",
                        "title": "Contact",
                        "type": "CONTACT_INFORMATION",
                        "translations": []
                    }
                ]
            }
        })
    );
}

#[test]
fn shop_policy_update_stages_policy_and_downstream_reads_locally() {
    let mut proxy = configured_proxy(
        ReadMode::Snapshot,
        Some(shopify_draft_proxy::proxy::UnsupportedMutationMode::Reject),
    );
    let query = r#"
        mutation AnyOperationName($shopPolicy: ShopPolicyInput!) {
          aliasedUpdate: shopPolicyUpdate(shopPolicy: $shopPolicy) {
            shopPolicy {
              __typename
              id
              type
              title
              body
              url
              createdAt
              updatedAt
              translations(locale: "fr") { key locale value }
            }
            userErrors { field message code }
          }
        }
    "#;

    let response = proxy.process_request(json_graphql_request(
        query,
        json!({ "shopPolicy": { "type": "PRIVACY_POLICY", "body": "<p>Hi</p>" } }),
    ));

    assert_eq!(response.status, 200);
    assert_eq!(
        response.body["data"]["aliasedUpdate"]["userErrors"],
        json!([])
    );
    let policy = &response.body["data"]["aliasedUpdate"]["shopPolicy"];
    let id = policy["id"].as_str().expect("policy id").to_string();
    assert_eq!(policy["__typename"], json!("ShopPolicy"));
    assert_eq!(policy["type"], json!("PRIVACY_POLICY"));
    assert_eq!(policy["title"], json!("Privacy Policy"));
    assert_eq!(policy["body"], json!("<p>Hi</p>"));
    assert_eq!(
        policy["url"],
        json!("https://shopify-draft-proxy.local/policies/1.html?locale=en")
    );
    assert_eq!(policy["translations"], json!([]));

    let read = proxy.process_request(json_graphql_request(
        r#"
        query ShopPolicyDownstreamRead($id: ID!) {
          shop {
            id
            myshopifyDomain
            shopPolicies { __typename id type title body url updatedAt }
          }
          nodePolicy: node(id: $id) { __typename ... on ShopPolicy { id type title body url } }
          nodes(ids: [$id]) { __typename ... on ShopPolicy { id type title body url } }
        }
        "#,
        json!({ "id": id }),
    ));

    assert_eq!(read.status, 200);
    assert_eq!(
        read.body["data"]["shop"]["shopPolicies"][0]["title"],
        json!("Privacy Policy")
    );
    assert_eq!(
        read.body["data"]["shop"]["shopPolicies"][0]["body"],
        json!("<p>Hi</p>")
    );
    assert_eq!(
        read.body["data"]["shop"]["shopPolicies"][0]["__typename"],
        json!("ShopPolicy")
    );
    assert_eq!(
        read.body["data"]["nodePolicy"]["__typename"],
        json!("ShopPolicy")
    );
    assert_eq!(read.body["data"]["nodePolicy"]["id"], policy["id"]);
    assert_eq!(read.body["data"]["nodes"][0]["url"], policy["url"]);
    let log = log_snapshot(&proxy);
    assert_eq!(
        log["entries"][0]["stagedResourceIds"],
        json!([policy["id"]])
    );
    assert_eq!(
        log["entries"][0]["interpreted"]["capability"],
        json!({
            "operationName": "shopPolicyUpdate",
            "domain": "store-properties",
            "execution": "stage-locally"
        })
    );
    assert!(log["entries"][0]["rawBody"]
        .as_str()
        .expect("raw body")
        .contains("shopPolicyUpdate"));
}

#[test]
fn shop_policy_update_overlays_restored_base_shop_policies() {
    let mut proxy = snapshot_proxy();
    let restore = proxy.process_request(Request {
        method: "POST".to_string(),
        path: "/__meta/restore".to_string(),
        headers: Default::default(),
        body: json!({
            "schema": "shopify-draft-proxy-rust-state/v1",
            "createdAt": "2026-06-15T00:00:00.000Z",
            "nextSyntheticId": 9,
            "state": {
                "baseState": {
                    "products": {},
                    "productOrder": [],
                    "savedSearches": {},
                    "savedSearchOrder": [],
                    "shop": {
                        "id": "gid://shopify/Shop/seed",
                        "myshopifyDomain": "seeded-policy-shop.myshopify.com",
                        "primaryDomain": { "host": "policies.example.com" },
                        "shopPolicies": [
                            {
                                "id": "gid://shopify/ShopPolicy/111",
                                "title": "Contact",
                                "body": "<p>Contact</p>",
                                "type": "CONTACT_INFORMATION",
                                "url": "https://checkout.shopify.com/seed/policies/111.html?locale=en",
                                "createdAt": "2026-01-01T00:00:00Z",
                                "updatedAt": "2026-01-01T00:00:00Z"
                            },
                            {
                                "id": "gid://shopify/ShopPolicy/222",
                                "title": "Privacy policy",
                                "body": "<p>Old</p>",
                                "type": "PRIVACY_POLICY",
                                "url": "https://checkout.shopify.com/seed/policies/222.html?locale=en",
                                "createdAt": "2026-01-02T00:00:00Z",
                                "updatedAt": "2026-01-02T00:00:00Z"
                            }
                        ]
                    },
                    "publicationIds": [],
                    "publicationCount": 0
                },
                "stagedState": {
                    "products": {},
                    "productOrder": [],
                    "deletedProductIds": [],
                    "savedSearches": {},
                    "savedSearchOrder": [],
                    "deletedSavedSearchIds": [],
                    "shippingPackages": {},
                    "deletedShippingPackageIds": {},
                    "delegatedAccessTokens": {},
                    "customers": {},
                    "deletedCustomerIds": [],
                    "customerOrders": {}
                }
            },
            "log": { "entries": [] }
        })
        .to_string(),
    });
    assert_eq!(restore.status, 200);

    let update = proxy.process_request(json_graphql_request(
        r#"
        mutation ShopPolicyUpdate($shopPolicy: ShopPolicyInput!) {
          shopPolicyUpdate(shopPolicy: $shopPolicy) {
            shopPolicy { id title body url createdAt }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "shopPolicy": { "type": "PRIVACY_POLICY", "body": "<p>New</p>" } }),
    ));
    assert_eq!(update.status, 200);
    assert_eq!(
        update.body["data"]["shopPolicyUpdate"]["shopPolicy"],
        json!({
            "id": "gid://shopify/ShopPolicy/222",
            "title": "Privacy Policy",
            "body": "<p>New</p>",
            "url": "https://policies.example.com/policies/222.html?locale=en",
            "createdAt": "2026-01-02T00:00:00Z"
        })
    );

    let read = proxy.process_request(json_graphql_request(
        r#"
        query ShopPolicies {
          shop { shopPolicies { id type title body url } }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        read.body["data"]["shop"]["shopPolicies"],
        json!([
            {
                "id": "gid://shopify/ShopPolicy/111",
                "type": "CONTACT_INFORMATION",
                "title": "Contact",
                "body": "<p>Contact</p>",
                "url": "https://checkout.shopify.com/seed/policies/111.html?locale=en"
            },
            {
                "id": "gid://shopify/ShopPolicy/222",
                "type": "PRIVACY_POLICY",
                "title": "Privacy Policy",
                "body": "<p>New</p>",
                "url": "https://policies.example.com/policies/222.html?locale=en"
            }
        ])
    );
}

#[test]
fn shop_policy_update_rejects_only_privacy_liquid_syntax_errors() {
    let mut proxy = snapshot_proxy();
    let update_query = r#"
        mutation ShopPolicyUpdate($shopPolicy: ShopPolicyInput!) {
          shopPolicyUpdate(shopPolicy: $shopPolicy) {
            shopPolicy { id type body }
            userErrors { field message code }
          }
        }
    "#;
    let read_query = r#"
        query ShopPolicyRead {
          shop { shopPolicies { type body } }
        }
    "#;

    let invalid_privacy = proxy.process_request(json_graphql_request(
        update_query,
        json!({ "shopPolicy": { "type": "PRIVACY_POLICY", "body": "{% unknownTag %}" } }),
    ));
    assert_eq!(invalid_privacy.status, 200);
    assert_eq!(
        invalid_privacy.body["data"]["shopPolicyUpdate"],
        json!({
            "shopPolicy": null,
            "userErrors": [{
                "field": ["shopPolicy", "body"],
                "message": "Body Liquid syntax error: Unknown tag 'unknownTag'",
                "code": null
            }]
        })
    );
    assert_eq!(log_snapshot(&proxy)["entries"], json!([]));
    let read_after_invalid = proxy.process_request(json_graphql_request(read_query, json!({})));
    assert_eq!(
        read_after_invalid.body["data"]["shop"]["shopPolicies"],
        json!([])
    );

    let refund = proxy.process_request(json_graphql_request(
        update_query,
        json!({ "shopPolicy": { "type": "REFUND_POLICY", "body": "{% unknownTag %}" } }),
    ));
    assert_eq!(refund.status, 200);
    assert_eq!(
        refund.body["data"]["shopPolicyUpdate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        refund.body["data"]["shopPolicyUpdate"]["shopPolicy"]["body"],
        json!("{% unknownTag %}")
    );

    let valid_privacy_body = "Line one {{ shop.name }}";
    let valid_privacy = proxy.process_request(json_graphql_request(
        update_query,
        json!({ "shopPolicy": { "type": "PRIVACY_POLICY", "body": valid_privacy_body } }),
    ));
    assert_eq!(valid_privacy.status, 200);
    assert_eq!(
        valid_privacy.body["data"]["shopPolicyUpdate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        valid_privacy.body["data"]["shopPolicyUpdate"]["shopPolicy"]["body"],
        json!(valid_privacy_body)
    );

    let read = proxy.process_request(json_graphql_request(read_query, json!({})));
    assert_eq!(
        read.body["data"]["shop"]["shopPolicies"],
        json!([
            {
                "type": "REFUND_POLICY",
                "body": "{% unknownTag %}"
            },
            {
                "type": "PRIVACY_POLICY",
                "body": valid_privacy_body
            }
        ])
    );
}

#[test]
fn shop_policy_update_validation_branches_match_shopify_shapes() {
    let mut proxy = snapshot_proxy();
    let query = r#"
        mutation ShopPolicyUpdate($shopPolicy: ShopPolicyInput!) {
          shopPolicyUpdate(shopPolicy: $shopPolicy) {
            shopPolicy { id type title body }
            userErrors { field message code }
          }
        }
    "#;

    let blank_subscription = proxy.process_request(json_graphql_request(
        query,
        json!({ "shopPolicy": { "type": "SUBSCRIPTION_POLICY", "body": "  \n" } }),
    ));
    assert_eq!(blank_subscription.status, 200);
    assert_eq!(
        blank_subscription.body["data"]["shopPolicyUpdate"]["shopPolicy"],
        json!(null)
    );
    assert_eq!(
        blank_subscription.body["data"]["shopPolicyUpdate"]["userErrors"],
        json!([{
            "field": ["shopPolicy", "body"],
            "message": "Purchase options cancellation policy required",
            "code": null
        }])
    );
    assert_eq!(log_snapshot(&proxy)["entries"], json!([]));

    let max_body = "a".repeat(524_287);
    let max_response = proxy.process_request(json_graphql_request(
        query,
        json!({ "shopPolicy": { "type": "REFUND_POLICY", "body": max_body } }),
    ));
    assert_eq!(
        max_response.body["data"]["shopPolicyUpdate"]["userErrors"],
        json!([])
    );

    let too_big = "b".repeat(524_288);
    let too_big_response = proxy.process_request(json_graphql_request(
        query,
        json!({ "shopPolicy": { "type": "REFUND_POLICY", "body": too_big } }),
    ));
    assert_eq!(
        too_big_response.body["data"]["shopPolicyUpdate"]["shopPolicy"],
        json!(null)
    );
    assert_eq!(
        too_big_response.body["data"]["shopPolicyUpdate"]["userErrors"],
        json!([{
            "field": ["shopPolicy", "body"],
            "message": "Body is too big (maximum is 512 KB)",
            "code": "TOO_BIG"
        }])
    );

    for variables in [
        json!({ "shopPolicy": { "type": "BOGUS_TYPE", "body": "<p>Hi</p>" } }),
        json!({ "shopPolicy": { "type": "REFUND_POLICY" } }),
        json!({ "shopPolicy": { "type": "REFUND_POLICY", "body": null } }),
    ] {
        let invalid = proxy.process_request(json_graphql_request(query, variables));
        assert_eq!(invalid.status, 200);
        assert_eq!(
            invalid.body["errors"][0]["extensions"]["code"],
            json!("INVALID_VARIABLE")
        );
        assert!(invalid.body.get("data").is_none());
    }
}

#[test]
fn shop_policy_update_uses_title_cased_titles_for_every_policy_type() {
    let mut proxy = snapshot_proxy();
    let query = r#"
        mutation ShopPolicyUpdate($shopPolicy: ShopPolicyInput!) {
          shopPolicyUpdate(shopPolicy: $shopPolicy) {
            shopPolicy { type title }
            userErrors { field message code }
          }
        }
    "#;

    for (policy_type, title) in [
        ("PRIVACY_POLICY", "Privacy Policy"),
        ("REFUND_POLICY", "Refund Policy"),
        ("TERMS_OF_SERVICE", "Terms of Service"),
        ("SHIPPING_POLICY", "Shipping Policy"),
        ("SUBSCRIPTION_POLICY", "Subscription Policy"),
        ("CONTACT_INFORMATION", "Contact Information"),
        ("LEGAL_NOTICE", "Legal Notice"),
        ("TERMS_OF_SALE", "Terms of Sale"),
    ] {
        let response = proxy.process_request(json_graphql_request(
            query,
            json!({ "shopPolicy": { "type": policy_type, "body": "<p>Body</p>" } }),
        ));
        assert_eq!(response.status, 200);
        assert_eq!(
            response.body["data"]["shopPolicyUpdate"]["userErrors"],
            json!([])
        );
        assert_eq!(
            response.body["data"]["shopPolicyUpdate"]["shopPolicy"],
            json!({ "type": policy_type, "title": title })
        );
    }
}

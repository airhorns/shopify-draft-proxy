use super::common::*;
use pretty_assertions::assert_eq;
use shopify_draft_proxy::proxy::Response;

#[test]
fn mixed_admin_platform_read_forwards_the_original_document_once() {
    let upstream_bodies = Arc::new(Mutex::new(Vec::<Value>::new()));
    let captured_bodies = Arc::clone(&upstream_bodies);
    let mut proxy =
        configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport(move |request| {
            let body: Value = serde_json::from_str(&request.body).expect("upstream body parses");
            captured_bodies.lock().unwrap().push(body);
            Response {
                status: 200,
                headers: Default::default(),
                body: json!({
                    "data": {
                        "publicApiVersions": [{
                            "handle": "2026-04",
                            "displayName": "2026-04",
                            "supported": true
                        }],
                        "domain": {
                            "id": "gid://shopify/Domain/1",
                            "host": "example.test"
                        }
                    }
                }),
            }
        });

    let response = proxy.process_request(json_graphql_request(
        r#"
        query MixedAdminPlatformRead($domainId: ID!) {
          publicApiVersions { handle displayName supported }
          domain(id: $domainId) { id host }
        }
        "#,
        json!({"domainId": "gid://shopify/Domain/1"}),
    ));

    assert_eq!(
        response.body["data"],
        json!({
            "publicApiVersions": [{
                "handle": "2026-04",
                "displayName": "2026-04",
                "supported": true
            }],
            "domain": {
                "id": "gid://shopify/Domain/1",
                "host": "example.test"
            }
        })
    );
    let bodies = upstream_bodies.lock().unwrap();
    assert_eq!(bodies.len(), 1);
    let query = bodies[0]["query"].as_str().expect("query is preserved");
    assert!(query.contains("publicApiVersions"));
    assert!(query.contains("domain(id: $domainId)"));
}

fn add_platform_location(
    proxy: &mut DraftProxy,
    name: &str,
    fulfills_online_orders: bool,
) -> String {
    let response = proxy.process_request(json_graphql_request(
        r#"
        mutation PlatformLocationSeed($input: LocationAddInput!) {
          locationAdd(input: $input) {
            location { id name }
            userErrors { field code message }
          }
        }
        "#,
        json!({
            "input": {
                "name": name,
                "fulfillsOnlineOrders": fulfills_online_orders,
                "address": { "countryCode": "US" }
            }
        }),
    ));
    assert_eq!(response.status, 200);
    assert_eq!(
        response.body["data"]["locationAdd"]["userErrors"],
        json!([])
    );
    response.body["data"]["locationAdd"]["location"]["id"]
        .as_str()
        .unwrap()
        .to_string()
}

fn platform_inventory_base_product(id: &str, title: &str) -> ProductRecord {
    ProductRecord {
        id: id.to_string(),
        created_at: "2024-01-01T00:00:00.000Z".to_string(),
        updated_at: "2024-01-01T00:00:00.000Z".to_string(),
        title: title.to_string(),
        handle: title.to_ascii_lowercase().replace(' ', "-"),
        status: "ACTIVE".to_string(),
        ..ProductRecord::default()
    }
}

fn fulfillment_order_hydrate_transport(
    orders: Vec<Value>,
) -> impl Fn(Request) -> Response + Send + Sync + 'static {
    let orders = Arc::new(Mutex::new(orders));
    move |request| {
        let body: Value = serde_json::from_str(&request.body).unwrap();
        let query = body["query"].as_str().unwrap_or_default();
        let requested_id = body["variables"]["id"].as_str().unwrap_or_default();
        let orders = orders.lock().unwrap();
        let hydrate = |requested_id: &str| {
            orders.iter().find_map(|order| {
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
            })
        };
        if query.contains("nodes(ids: $ids)") {
            let nodes = body["variables"]["ids"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|id| {
                    id.as_str()
                        .and_then(|id| hydrate(id).map(|(_, node)| node))
                        .unwrap_or(Value::Null)
                })
                .collect::<Vec<_>>();
            return Response {
                status: 200,
                headers: Default::default(),
                body: json!({ "data": { "nodes": nodes } }),
            };
        }
        let hydrated = hydrate(requested_id);
        let body = if query.contains("node(id: $id)") {
            let node = hydrated
                .as_ref()
                .map(|(_, node)| node.clone())
                .unwrap_or(Value::Null);
            json!({ "data": { "node": node } })
        } else if query.contains("fulfillmentOrder(id: $id)") {
            let fulfillment_order = hydrated
                .as_ref()
                .map(|(_, node)| node.clone())
                .unwrap_or(Value::Null);
            json!({ "data": { "fulfillmentOrder": fulfillment_order } })
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

fn fulfillment_order_catalog_transport(
    orders: Vec<Value>,
    observed_queries: Arc<Mutex<Vec<String>>>,
) -> impl Fn(Request) -> Response + Send + Sync + 'static {
    let orders = Arc::new(Mutex::new(orders));
    move |request| {
        let body: Value = serde_json::from_str(&request.body).unwrap();
        let query = body["query"].as_str().unwrap_or_default().to_string();
        observed_queries.lock().unwrap().push(query.clone());
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
        } else if query.contains("fulfillmentOrder(id: $id)") {
            let fulfillment_order = hydrated
                .as_ref()
                .map(|(_, node)| node.clone())
                .unwrap_or(Value::Null);
            json!({ "data": { "fulfillmentOrder": fulfillment_order } })
        } else if let Some(root_key) = if query.contains("manualHoldsFulfillmentOrders(") {
            Some("manualHoldsFulfillmentOrders")
        } else if query.contains("assignedFulfillmentOrders(") {
            Some("assignedFulfillmentOrders")
        } else if query.contains("fulfillmentOrders(") {
            Some("fulfillmentOrders")
        } else {
            None
        } {
            let nodes = orders
                .lock()
                .unwrap()
                .iter()
                .flat_map(|order| {
                    order["fulfillmentOrders"]["nodes"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .map(|node| {
                            let mut node = node.clone();
                            node["order"] = json!({
                                "id": order["id"],
                                "name": order["name"],
                                "displayFulfillmentStatus": order["displayFulfillmentStatus"]
                            });
                            node
                        })
                        .collect::<Vec<_>>()
                })
                .collect::<Vec<_>>();
            let mut data = serde_json::Map::new();
            data.insert(
                root_key.to_string(),
                json!({
                    "nodes": nodes,
                    "edges": [],
                    "pageInfo": {
                        "hasNextPage": false,
                        "hasPreviousPage": false,
                        "startCursor": null,
                        "endCursor": null
                    }
                }),
            );
            let mut body = serde_json::Map::new();
            body.insert("data".to_string(), Value::Object(data));
            Value::Object(body)
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

fn resource_id_tail_for_test(id: &str) -> &str {
    id.rsplit('/')
        .next()
        .unwrap_or(id)
        .split('?')
        .next()
        .unwrap_or_default()
}

fn fulfillment_order_order_fixture(
    order_id: &str,
    name: &str,
    fulfillment_order_id: &str,
    line_item_id: &str,
    quantity: i64,
    status: &str,
) -> Value {
    let supported_actions = match status {
        "SCHEDULED" => json!([{ "action": "MARK_AS_OPEN" }]),
        "CLOSED" | "CANCELLED" => json!([]),
        "ON_HOLD" => json!([
            { "action": "RELEASE_HOLD" },
            { "action": "HOLD" },
            { "action": "MOVE" }
        ]),
        _ => json!([
            { "action": "CREATE_FULFILLMENT" },
            { "action": "REPORT_PROGRESS" },
            { "action": "MOVE" },
            { "action": "HOLD" },
            { "action": "SPLIT" }
        ]),
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

struct FulfillmentOrderCatalogFixture<'a> {
    order_id: &'a str,
    name: &'a str,
    fulfillment_order_id: &'a str,
    line_item_id: &'a str,
    status: &'a str,
    location_id: &'a str,
    location_name: &'a str,
    updated_at: &'a str,
}

fn fulfillment_order_catalog_fixture(input: FulfillmentOrderCatalogFixture<'_>) -> Value {
    let mut order = fulfillment_order_order_fixture(
        input.order_id,
        input.name,
        input.fulfillment_order_id,
        input.line_item_id,
        1,
        input.status,
    );
    let fulfillment_order = &mut order["fulfillmentOrders"]["nodes"][0];
    fulfillment_order["updatedAt"] = json!(input.updated_at);
    fulfillment_order["assignedLocation"] = json!({
        "name": input.location_name,
        "location": {
            "id": input.location_id,
            "name": input.location_name
        }
    });
    order
}

fn fulfillment_order_ids(response: &Response, root_key: &str) -> Vec<String> {
    response.body["data"][root_key]["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|node| node["id"].as_str().unwrap().to_string())
        .collect()
}

fn required_string(value: &Value, context: &str) -> String {
    value
        .as_str()
        .unwrap_or_else(|| panic!("{context} should be a string, got {value:?}"))
        .to_string()
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

fn create_inventory_item_for_location_test(proxy: &mut DraftProxy, title: &str) -> String {
    let response = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateLocationInventoryItem($product: ProductCreateInput!) {
          productCreate(product: $product) {
            product {
              variants(first: 1) {
                nodes { inventoryItem { id } }
              }
            }
            userErrors { field message }
          }
        }
        "#,
        json!({ "product": { "title": title } }),
    ));
    assert_eq!(
        response.body["data"]["productCreate"]["userErrors"],
        json!([])
    );
    response.body["data"]["productCreate"]["product"]["variants"]["nodes"][0]["inventoryItem"]["id"]
        .as_str()
        .unwrap()
        .to_string()
}

fn inventory_level_id_for_platform_test(inventory_item_id: &str, location_id: &str) -> String {
    let item_tail = inventory_item_id
        .rsplit('/')
        .next()
        .unwrap_or(inventory_item_id)
        .split('?')
        .next()
        .unwrap_or(inventory_item_id);
    let location_tail = location_id
        .rsplit('/')
        .next()
        .unwrap_or(location_id)
        .split('?')
        .next()
        .unwrap_or(location_id);
    format!(
        "gid://shopify/InventoryLevel/{item_tail}-{location_tail}?inventory_item_id={inventory_item_id}"
    )
}

fn create_location_for_platform_test(proxy: &mut DraftProxy, name: &str) -> String {
    let response = proxy.process_request(json_graphql_request(
        r#"
        mutation CreatePlatformLocation($input: LocationAddInput!) {
          locationAdd(input: $input) {
            location { id }
            userErrors { field code message }
          }
        }
        "#,
        json!({ "input": { "name": name, "address": { "countryCode": "CA" } } }),
    ));
    assert_eq!(
        response.body["data"]["locationAdd"]["userErrors"],
        json!([])
    );
    response.body["data"]["locationAdd"]["location"]["id"]
        .as_str()
        .unwrap()
        .to_string()
}

fn set_location_active_for_platform_test(proxy: &mut DraftProxy, location_id: &str, active: bool) {
    let (query, response_key, error_key) = if active {
        (
            r#"
            mutation SetPlatformLocationActive($locationId: ID!) {
              locationActivate(locationId: $locationId) @idempotent(key: "platform-location-active") {
                location { id isActive }
                locationActivateUserErrors { field code message }
              }
            }
            "#,
            "locationActivate",
            "locationActivateUserErrors",
        )
    } else {
        (
            r#"
            mutation SetPlatformLocationInactive($locationId: ID!) {
              locationDeactivate(locationId: $locationId) @idempotent(key: "platform-location-inactive") {
                location { id isActive }
                locationDeactivateUserErrors { field code message }
              }
            }
            "#,
            "locationDeactivate",
            "locationDeactivateUserErrors",
        )
    };
    let response = proxy.process_request(json_graphql_request(
        query,
        json!({ "locationId": location_id }),
    ));
    assert_eq!(response.body["data"][response_key][error_key], json!([]));
    assert_eq!(
        response.body["data"][response_key]["location"]["isActive"],
        json!(active)
    );
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
fn mixed_domain_query_merges_aliases_and_field_errors_in_document_order() {
    let mut proxy = snapshot_proxy();
    let response = proxy.process_request(json_graphql_request(
        r#"
        query MixedTemplatesThenInvalidJob {
          templates: paymentTermsTemplates {
            id
            name
          }
          poll: job(id: "gid://shopify/Product/0") {
            id
            done
          }
        }
        "#,
        json!({}),
    ));

    assert_eq!(response.status, 200);
    assert_eq!(
        response.body["data"]["templates"][0],
        json!({
            "id": "gid://shopify/PaymentTermsTemplate/1",
            "name": "Due on receipt"
        })
    );
    assert_eq!(response.body["data"]["poll"], Value::Null);
    assert_eq!(
        response.body["errors"][0]["message"],
        json!("Invalid id: gid://shopify/Product/0")
    );
    assert_eq!(response.body["errors"][0]["path"], json!(["poll"]));
}

#[test]
fn mixed_domain_query_field_order_does_not_change_executed_roots() {
    let mut proxy = snapshot_proxy();
    let job_first = proxy.process_request(json_graphql_request(
        r#"
        query MixedJobThenTemplates($type: PaymentTermsType) {
          job(id: "gid://shopify/Job/1") {
            id
            done
            query { __typename }
          }
          terms: paymentTermsTemplates(paymentTermsType: $type) {
            id
            paymentTermsType
          }
        }
        "#,
        json!({ "type": "NET" }),
    ));
    let templates_first = proxy.process_request(json_graphql_request(
        r#"
        query MixedTemplatesThenJob($type: PaymentTermsType) {
          terms: paymentTermsTemplates(paymentTermsType: $type) {
            id
            paymentTermsType
          }
          job(id: "gid://shopify/Job/1") {
            id
            done
            query { __typename }
          }
        }
        "#,
        json!({ "type": "NET" }),
    ));

    assert_eq!(job_first.status, 200);
    assert_eq!(templates_first.status, 200);
    assert_eq!(
        job_first.body["data"]["job"],
        json!({
            "id": "gid://shopify/Job/1",
            "done": true,
            "query": { "__typename": "QueryRoot" }
        })
    );
    assert_eq!(
        templates_first.body["data"]["job"],
        job_first.body["data"]["job"]
    );
    assert_eq!(job_first.body["data"]["terms"].as_array().unwrap().len(), 6);
    assert_eq!(
        templates_first.body["data"]["terms"],
        job_first.body["data"]["terms"]
    );
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
fn dump_restore_round_trips_hydrated_shop_identity() {
    let mut proxy =
        configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport(|_| Response {
            status: 200,
            headers: Default::default(),
            body: json!({
                "data": {
                    "shop": {
                        "id": "gid://shopify/Shop/live-round-trip",
                        "name": "Restored live shop",
                        "myshopifyDomain": "restored-live.myshopify.com",
                        "primaryDomain": {
                            "id": "gid://shopify/Domain/444555666",
                            "host": "restored-live.example",
                            "url": "https://restored-live.example",
                            "sslEnabled": true
                        },
                        "currencyCode": "EUR"
                    }
                }
            }),
        });

    let hydrate = proxy.process_request(json_graphql_request(
        r#"
        query HydrateShopForDumpRestore {
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
        hydrate.body["data"]["shop"]["id"],
        json!("gid://shopify/Shop/live-round-trip")
    );

    let dump = proxy.process_request(request_with_body("POST", "/__meta/dump", ""));
    assert_eq!(dump.status, 200);
    assert_eq!(
        dump.body["state"]["baseState"]["shop"]["primaryDomain"]["id"],
        json!("gid://shopify/Domain/444555666")
    );

    let mut restored = snapshot_proxy();
    let restore = restored.process_request(request_with_body(
        "POST",
        "/__meta/restore",
        &dump.body.to_string(),
    ));
    assert_eq!(restore.status, 200);

    let restored_shop = restored.process_request(json_graphql_request(
        r#"
        query RestoredHydratedShop {
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
    assert_eq!(restored_shop.status, 200);
    assert_eq!(
        restored_shop.body["data"]["shop"],
        hydrate.body["data"]["shop"]
    );

    let restored_domain = restored.process_request(json_graphql_request(
        r#"
        query RestoredHydratedDomain($id: ID!) {
          domain(id: $id) { id host url sslEnabled }
        }
        "#,
        json!({ "id": "gid://shopify/Domain/444555666" }),
    ));
    assert_eq!(restored_domain.status, 200);
    assert_eq!(
        restored_domain.body["data"]["domain"]["host"],
        json!("restored-live.example")
    );
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
fn cold_snapshot_shop_baseline_leaves_identity_absent() {
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
    assert!(response.body["data"].is_null());
    assert_eq!(
        response.body["errors"][0]["message"],
        json!("Local resolver did not implement `Shop.id`")
    );
    assert_eq!(response.body["errors"][0]["path"], json!(["shop", "id"]));
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
fn assigned_fulfillment_orders_filters_by_assignment_status_and_location_ids() {
    let mut proxy = snapshot_proxy();
    let _default_location_id = add_platform_location(&mut proxy, "Assigned filter warehouse", true);
    let (_order, fulfillment_order) = create_fulfillment_order_test_order(&mut proxy, 2);
    let fulfillment_order_id = fulfillment_order["id"].clone();
    let fulfillment_order_line_item_id = fulfillment_order["lineItems"]["nodes"][0]["id"].clone();
    let query = r#"
        query AssignedFulfillmentOrdersFiltering($locationIds: [ID!]) {
          requested: assignedFulfillmentOrders(
            first: 10
            assignmentStatus: FULFILLMENT_REQUESTED
            locationIds: $locationIds
            sortKey: ID
          ) {
            nodes {
              id
              requestStatus
              assignedLocation { location { id } }
              merchantRequests(first: 10) { nodes { kind responseData } }
            }
          }
          accepted: assignedFulfillmentOrders(
            first: 10
            assignmentStatus: FULFILLMENT_ACCEPTED
            locationIds: $locationIds
            sortKey: ID
          ) {
            nodes { id requestStatus merchantRequests(first: 10) { nodes { kind responseData } } }
          }
          cancellationRequested: assignedFulfillmentOrders(
            first: 10
            assignmentStatus: CANCELLATION_REQUESTED
            locationIds: $locationIds
            sortKey: ID
          ) {
            nodes { id requestStatus merchantRequests(first: 10) { nodes { kind responseData } } }
          }
        }
    "#;

    let submit = proxy.process_request(json_graphql_request(
        r#"
        mutation SubmitFulfillmentOrderRequest($id: ID!, $lineItems: [FulfillmentOrderLineItemInput!]) {
          fulfillmentOrderSubmitFulfillmentRequest(
            id: $id
            fulfillmentOrderLineItems: $lineItems
            message: "please ship"
            notifyCustomer: false
          ) {
            submittedFulfillmentOrder { id requestStatus }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "id": fulfillment_order_id,
            "lineItems": [{ "id": fulfillment_order_line_item_id, "quantity": 1 }]
        }),
    ));
    assert_eq!(
        submit.body["data"]["fulfillmentOrderSubmitFulfillmentRequest"]["userErrors"],
        json!([])
    );

    let requested =
        proxy.process_request(json_graphql_request(query, json!({ "locationIds": null })));
    assert_eq!(
        requested.body["data"]["requested"]["nodes"][0]["id"],
        fulfillment_order_id
    );
    assert_eq!(
        requested.body["data"]["requested"]["nodes"][0]["requestStatus"],
        json!("SUBMITTED")
    );
    assert_eq!(requested.body["data"]["accepted"]["nodes"], json!([]));
    let assigned_location_id = requested.body["data"]["requested"]["nodes"][0]["assignedLocation"]
        ["location"]["id"]
        .clone();

    let matching_location = proxy.process_request(json_graphql_request(
        query,
        json!({ "locationIds": [assigned_location_id] }),
    ));
    assert_eq!(
        matching_location.body["data"]["requested"]["nodes"][0]["id"],
        fulfillment_order_id
    );

    let mismatched_location = proxy.process_request(json_graphql_request(
        query,
        json!({ "locationIds": ["gid://shopify/Location/not-this-order"] }),
    ));
    assert_eq!(
        mismatched_location.body["data"]["requested"]["nodes"],
        json!([])
    );

    let accept = proxy.process_request(json_graphql_request(
        r#"
        mutation AcceptFulfillmentOrderRequest($id: ID!) {
          fulfillmentOrderAcceptFulfillmentRequest(id: $id, message: "accepted") {
            fulfillmentOrder { id requestStatus }
            userErrors { field message }
          }
        }
        "#,
        json!({ "id": fulfillment_order_id }),
    ));
    assert_eq!(
        accept.body["data"]["fulfillmentOrderAcceptFulfillmentRequest"]["userErrors"],
        json!([])
    );
    let accepted =
        proxy.process_request(json_graphql_request(query, json!({ "locationIds": null })));
    assert_eq!(accepted.body["data"]["requested"]["nodes"], json!([]));
    assert_eq!(
        accepted.body["data"]["accepted"]["nodes"][0]["id"],
        fulfillment_order_id
    );

    let submit_cancel = proxy.process_request(json_graphql_request(
        r#"
        mutation SubmitFulfillmentOrderCancellationRequest($id: ID!) {
          fulfillmentOrderSubmitCancellationRequest(id: $id, message: "cancel please") {
            fulfillmentOrder { id requestStatus merchantRequests(first: 10) { nodes { kind responseData } } }
            userErrors { field message }
          }
        }
        "#,
        json!({ "id": fulfillment_order_id }),
    ));
    assert_eq!(
        submit_cancel.body["data"]["fulfillmentOrderSubmitCancellationRequest"]["userErrors"],
        json!([])
    );
    let cancellation_requested =
        proxy.process_request(json_graphql_request(query, json!({ "locationIds": null })));
    assert_eq!(
        cancellation_requested.body["data"]["cancellationRequested"]["nodes"][0]["id"],
        fulfillment_order_id
    );
    assert_eq!(
        cancellation_requested.body["data"]["accepted"]["nodes"],
        json!([])
    );
}

#[test]
fn created_order_fulfillment_order_uses_staged_default_location_assignment() {
    let mut proxy = snapshot_proxy();
    let default_location_id = add_platform_location(&mut proxy, "Warehouse Zero", true);
    let (_order, fulfillment_order) = create_fulfillment_order_test_order(&mut proxy, 2);
    let fulfillment_order_id = fulfillment_order["id"].clone();
    let fulfillment_order_line_item_id = fulfillment_order["lineItems"]["nodes"][0]["id"].clone();

    let submit = proxy.process_request(json_graphql_request(
        r#"
        mutation SubmitFulfillmentOrderRequest($id: ID!, $lineItems: [FulfillmentOrderLineItemInput!]) {
          fulfillmentOrderSubmitFulfillmentRequest(
            id: $id
            fulfillmentOrderLineItems: $lineItems
            message: "please ship"
            notifyCustomer: false
          ) {
            submittedFulfillmentOrder {
              id
              assignedLocation { name location { id name } }
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
    assert_eq!(
        submit.body["data"]["fulfillmentOrderSubmitFulfillmentRequest"]["userErrors"],
        json!([])
    );
    assert_eq!(
        submit.body["data"]["fulfillmentOrderSubmitFulfillmentRequest"]
            ["submittedFulfillmentOrder"]["assignedLocation"]["location"]["id"],
        default_location_id
    );
    assert_eq!(
        submit.body["data"]["fulfillmentOrderSubmitFulfillmentRequest"]
            ["submittedFulfillmentOrder"]["assignedLocation"]["name"],
        json!("Warehouse Zero")
    );

    let read = proxy.process_request(json_graphql_request(
        r#"
        query ReadAssignedFulfillmentOrders {
          assignedFulfillmentOrders(first: 5) {
            nodes {
              id
              assignedLocation { name location { id name } }
            }
          }
        }
        "#,
        json!({}),
    ));
    let assigned_location =
        &read.body["data"]["assignedFulfillmentOrders"]["nodes"][0]["assignedLocation"];
    assert_eq!(assigned_location["location"]["id"], default_location_id);
    assert_eq!(
        assigned_location["location"]["name"],
        json!("Warehouse Zero")
    );
    assert_eq!(assigned_location["name"], json!("Warehouse Zero"));
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
                lineItems(first: 5) { nodes { id totalQuantity remainingQuantity lineItem { id title quantity fulfillableQuantity } } }
              }
              remainingFulfillmentOrder {
                id
                status
                requestStatus
                updatedAt
                supportedActions { action }
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
          fulfillmentOrders(first: 5, includeClosed: true) { nodes { id } }
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
    let dump = proxy.process_request(request_with_body("POST", "/__meta/dump", "{}"));
    let mut restored = dump.body.clone();
    restored["state"]["baseState"]["shop"]["myshopifyDomain"] =
        json!("backup-region-gb.myshopify.com");
    restored["state"]["baseState"]["shop"]["shopAddress"]["countryCodeV2"] = json!("GB");
    let restore = proxy.process_request(request_with_body(
        "POST",
        "/__meta/restore",
        &restored.to_string(),
    ));
    assert_eq!(restore.status, 200);

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

    let current_country = proxy.process_request(json_graphql_request(
        r#"
        mutation BackupRegionUpdateCurrentCountry {
          backupRegionUpdate(region: { countryCode: GB }) {
            backupRegion { __typename id name ... on MarketRegionCountry { code } }
            userErrors { field message code }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        current_country.body["data"]["backupRegionUpdate"],
        json!({
            "backupRegion": {
                "__typename": "MarketRegionCountry",
                "id": "gid://shopify/MarketRegionCountry/local-GB",
                "name": "United Kingdom",
                "code": "GB"
            },
            "userErrors": []
        })
    );
    let current_region = current_country.body["data"]["backupRegionUpdate"]["backupRegion"].clone();

    let current_node = proxy.process_request(json_graphql_request(
        r#"
        query BackupRegionCurrentCountryNode($ids: [ID!]!) {
          nodes(ids: $ids) { __typename ... on MarketRegionCountry { id name code } }
        }
        "#,
        json!({ "ids": [current_region["id"].as_str().unwrap()] }),
    ));
    assert_eq!(current_node.body["data"]["nodes"][0], current_region);

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
              conditions {
                regionsCondition {
                  regions(first: 5) {
                    nodes { ... on MarketRegionCountry { code } }
                  }
                }
              }
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

    let numeric_country_code = proxy.process_request(json_graphql_request(
        r#"
        mutation FooNumericCountryCode {
          backupRegionUpdate(region: { countryCode: 42 }) { backupRegion { id } userErrors { field code } }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        numeric_country_code.body["errors"][0],
        json!({
            "message": "Argument 'countryCode' on InputObject 'BackupRegionUpdateInput' has an invalid value (42). Expected type 'CountryCode!'.",
            "locations": [{ "line": 3, "column": 38 }],
            "path": [
                "mutation FooNumericCountryCode",
                "backupRegionUpdate",
                "region",
                "countryCode"
            ],
            "extensions": {
                "code": "argumentLiteralsIncompatible",
                "typeName": "InputObject",
                "argumentName": "countryCode"
            }
        })
    );
    assert!(numeric_country_code.body.get("data").is_none());

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
fn backup_region_update_does_not_infer_country_from_shop_domain() {
    let mut proxy = snapshot_proxy();
    let dump = proxy.process_request(request_with_body("POST", "/__meta/dump", "{}"));
    let mut restored = dump.body.clone();
    restored["state"]["baseState"]["shop"] = json!({
        "id": "gid://shopify/Shop/1991",
        "name": "Domain-only shop",
        "myshopifyDomain": "harry-test-heelo.myshopify.com"
    });
    let restore = proxy.process_request(request_with_body(
        "POST",
        "/__meta/restore",
        &restored.to_string(),
    ));
    assert_eq!(restore.status, 200);

    let response = proxy.process_request(json_graphql_request(
        r#"
        mutation BackupRegionUpdateDomainOnlyCanada {
          backupRegionUpdate(region: { countryCode: CA }) {
            backupRegion { __typename id name ... on MarketRegionCountry { code } }
            userErrors { field message code }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        response.body["data"]["backupRegionUpdate"],
        json!({
            "backupRegion": null,
            "userErrors": [{
                "field": ["region"],
                "message": "Region not found.",
                "code": "REGION_NOT_FOUND"
            }]
        })
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

    let mut markets_delegate_request = json_graphql_request(
        r#"
        mutation CreateMarketsDelegate {
          delegateAccessTokenCreate(input: { delegateAccessScope: ["read_markets", "write_markets"], expiresIn: 300 }) {
            delegateAccessToken { accessToken accessScopes }
            userErrors { field message code }
          }
        }
        "#,
        json!({}),
    );
    markets_delegate_request.headers.insert(
        "x-shopify-draft-proxy-access-scopes".to_string(),
        "read_products,write_products,read_markets,write_markets".to_string(),
    );
    let markets_delegate = proxy.process_request(markets_delegate_request);
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
                Some("BackupRegionAvailableHydrate") => {
                    assert_eq!(body["variables"], json!({}));
                    let query = body["query"].as_str().unwrap_or_default();
                    assert!(query.contains("availableBackupRegions"));
                    assert!(!query.contains("markets(first:"));
                    assert!(!query.contains("regions(first:"));
                    Response {
                        status: 200,
                        headers: Default::default(),
                        body: json!({
                            "data": {
                                "availableBackupRegions": [{
                                    "__typename": "MarketRegionCountry",
                                    "id": "gid://shopify/MarketRegionCountry/shop-jp",
                                    "name": "Japan",
                                    "code": "JP"
                                }]
                            }
                        }),
                    }
                }
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

    let mut second_update_request = json_graphql_request(
        r#"
        mutation BackupRegionUpdateHydratedJapanAgain {
          backupRegionUpdate(region: { countryCode: JP }) {
            backupRegion { __typename id name ... on MarketRegionCountry { code } }
            userErrors { field message code }
          }
        }
        "#,
        json!({}),
    );
    second_update_request.headers.insert(
        "X-Shopify-Access-Token".to_string(),
        "parent-live-token".to_string(),
    );
    let second_update = proxy.process_request(second_update_request);
    assert_eq!(
        second_update.body["data"]["backupRegionUpdate"],
        update.body["data"]["backupRegionUpdate"]
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
            "BackupRegionAvailableHydrate".to_string()
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
fn live_hybrid_nodes_batch_merges_staged_and_tombstoned_records_over_one_upstream_read() {
    let upstream_requests = Arc::new(Mutex::new(Vec::<Value>::new()));
    let captured_requests = Arc::clone(&upstream_requests);
    let mut proxy =
        configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport(move |request| {
            let body: Value =
                serde_json::from_str(&request.body).expect("node request body parses");
            captured_requests.lock().unwrap().push(body.clone());
            let nodes = body["variables"]["ids"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|id| match id.as_str().unwrap_or_default() {
                    "gid://shopify/Order/upstream" => json!({
                        "__typename": "Order",
                        "id": "gid://shopify/Order/upstream",
                        "name": "#UPSTREAM"
                    }),
                    id => json!({
                        "__typename": "Product",
                        "id": id,
                        "title": "stale upstream product"
                    }),
                })
                .collect::<Vec<_>>();
            Response {
                status: 200,
                headers: Default::default(),
                body: json!({ "data": { "nodes": nodes } }),
            }
        });

    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateProductsForMixedNodeBatch {
          kept: productCreate(product: { title: "Local staged product" }) {
            product { id }
            userErrors { field message }
          }
          deleted: productCreate(product: { title: "Deleted staged product" }) {
            product { id }
            userErrors { field message }
          }
        }
        "#,
        json!({}),
    ));
    let kept_id = required_string(
        &create.body["data"]["kept"]["product"]["id"],
        "kept staged product id",
    );
    let deleted_id = required_string(
        &create.body["data"]["deleted"]["product"]["id"],
        "deleted staged product id",
    );
    let delete = proxy.process_request(json_graphql_request(
        r#"
        mutation DeleteProductForMixedNodeBatch($input: ProductDeleteInput!) {
          productDelete(input: $input) { deletedProductId userErrors { field message } }
        }
        "#,
        json!({ "input": { "id": deleted_id } }),
    ));
    assert_eq!(
        delete.body["data"]["productDelete"]["deletedProductId"],
        json!(deleted_id)
    );

    let node_read = proxy.process_request(json_graphql_request(
        r#"
        query MixedLocalAndColdNodes($ids: [ID!]!) {
          nodes(ids: $ids) {
            __typename
            ... on Product { id title }
            ... on Order { id name }
          }
        }
        "#,
        json!({
            "ids": [kept_id, "gid://shopify/Order/upstream", deleted_id]
        }),
    ));

    assert_eq!(
        node_read.body["data"]["nodes"],
        json!([
            { "__typename": "Product", "id": kept_id, "title": "Local staged product" },
            { "__typename": "Order", "id": "gid://shopify/Order/upstream", "name": "#UPSTREAM" },
            null
        ])
    );
    let upstream_requests = upstream_requests.lock().unwrap();
    assert_eq!(upstream_requests.len(), 1);
    assert!(upstream_requests[0]["query"]
        .as_str()
        .is_some_and(|query| query.contains("query MixedLocalAndColdNodes")));
}

#[test]
fn live_hybrid_multi_root_nodes_hydrate_entities_once_without_overwriting_local_roots() {
    let upstream_requests = Arc::new(Mutex::new(Vec::<Value>::new()));
    let captured_requests = Arc::clone(&upstream_requests);
    let mut proxy =
        configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport(move |request| {
            let body: Value =
                serde_json::from_str(&request.body).expect("multi-root node request body parses");
            captured_requests.lock().unwrap().push(body.clone());
            let local_id = body["variables"]["localId"].as_str().unwrap_or_default();
            Response {
                status: 200,
                headers: Default::default(),
                body: json!({
                    "data": {
                        "local": {
                            "__typename": "Product",
                            "id": local_id,
                            "title": "stale upstream product"
                        },
                        "cold": {
                            "__typename": "Order",
                            "id": "gid://shopify/Order/cold-multi-root",
                            "name": "#COLD"
                        }
                    }
                }),
            }
        });

    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateLocalProductForMultiRootNode {
          productCreate(product: { title: "Canonical local product" }) {
            product { id }
            userErrors { field message }
          }
        }
        "#,
        json!({}),
    ));
    let local_id = required_string(
        &create.body["data"]["productCreate"]["product"]["id"],
        "local product id",
    );

    let response = proxy.process_request(json_graphql_request(
        r#"
        query MultiRootEntityHydration($localId: ID!, $coldId: ID!) {
          local: node(id: $localId) {
            __typename
            ... on Product { id title }
          }
          cold: node(id: $coldId) {
            __typename
            ... on Order { id name }
          }
        }
        "#,
        json!({
            "localId": local_id,
            "coldId": "gid://shopify/Order/cold-multi-root"
        }),
    ));

    assert_eq!(
        response.body["data"],
        json!({
            "local": {
                "__typename": "Product",
                "id": local_id,
                "title": "Canonical local product"
            },
            "cold": {
                "__typename": "Order",
                "id": "gid://shopify/Order/cold-multi-root",
                "name": "#COLD"
            }
        })
    );
    let upstream_requests = upstream_requests.lock().unwrap();
    assert_eq!(upstream_requests.len(), 1);
    assert!(upstream_requests[0]["query"]
        .as_str()
        .is_some_and(|query| query.contains("query MultiRootEntityHydration")));
}

#[test]
fn generic_node_reads_project_customer_payment_store_credit_and_gift_card_records() {
    let mut proxy = snapshot_proxy();

    let customer_setup = proxy.process_request(json_graphql_request(
        r#"
        mutation AdminNodeCustomerSetup($input: CustomerInput!) {
          customerCreate(input: $input) {
            customer {
              id
              email
              displayName
              addressesV2(first: 2) {
                nodes { id address1 city countryCodeV2 }
              }
            }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "input": {
                "email": "admin-node-customer@example.test",
                "firstName": "Node",
                "lastName": "Customer",
                "addresses": [{
                    "address1": "1 Node St",
                    "city": "Ottawa",
                    "countryCode": "CA",
                    "provinceCode": "ON",
                    "zip": "K1A 0B1"
                }]
            }
        }),
    ));
    assert_eq!(customer_setup.status, 200);
    assert_eq!(
        customer_setup.body["data"]["customerCreate"]["userErrors"],
        json!([])
    );
    let customer_id = required_string(
        &customer_setup.body["data"]["customerCreate"]["customer"]["id"],
        "created customer id",
    );
    let address_id = required_string(
        &customer_setup.body["data"]["customerCreate"]["customer"]["addressesV2"]["nodes"][0]["id"],
        "created address id",
    );

    let payment_setup = proxy.process_request(json_graphql_request(
        r#"
        mutation AdminNodeCustomerPaymentMethodSetup($customerId: ID!) {
          customerPaymentMethodCreditCardCreate(
            customerId: $customerId
            sessionId: "sess_node"
            billingAddress: {
              firstName: "Node"
              lastName: "Billing"
              address1: "2 Billing St"
              city: "Toronto"
              zip: "M5V 2T6"
              country: "CA"
              province: "ON"
            }
          ) {
            customerPaymentMethod {
              id
              customer { id }
              instrument {
                __typename
                ... on CustomerCreditCard {
                  billingAddress { address1 city countryCode }
                }
              }
              revokedAt
              revokedReason
            }
            processing
            userErrors { field message }
          }
        }
        "#,
        json!({ "customerId": customer_id }),
    ));
    assert_eq!(payment_setup.status, 200);
    assert_eq!(
        payment_setup.body["data"]["customerPaymentMethodCreditCardCreate"]["userErrors"],
        json!([])
    );
    let payment_method_id = required_string(
        &payment_setup.body["data"]["customerPaymentMethodCreditCardCreate"]
            ["customerPaymentMethod"]["id"],
        "created payment method id",
    );

    let store_credit_setup = proxy.process_request(json_graphql_request(
        r#"
        mutation AdminNodeStoreCreditSetup($customerId: ID!) {
          credit: storeCreditAccountCredit(
            id: $customerId
            creditInput: { creditAmount: { amount: "10.00", currencyCode: USD } }
          ) {
            storeCreditAccountTransaction {
              id
              __typename
              amount { amount currencyCode }
              balanceAfterTransaction { amount currencyCode }
              account { id owner { ... on Customer { id } } balance { amount currencyCode } }
            }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "customerId": customer_id }),
    ));
    assert_eq!(store_credit_setup.status, 200);
    assert_eq!(
        store_credit_setup.body["data"]["credit"]["userErrors"],
        json!([])
    );
    let store_credit_account_id = required_string(
        &store_credit_setup.body["data"]["credit"]["storeCreditAccountTransaction"]["account"]
            ["id"],
        "store credit account id",
    );
    let store_credit_credit_id = required_string(
        &store_credit_setup.body["data"]["credit"]["storeCreditAccountTransaction"]["id"],
        "store credit credit transaction id",
    );

    let store_credit_debit = proxy.process_request(json_graphql_request(
        r#"
        mutation AdminNodeStoreCreditDebitSetup($accountId: ID!) {
          debit: storeCreditAccountDebit(
            id: $accountId
            debitInput: { debitAmount: { amount: "3.00", currencyCode: USD } }
          ) {
            storeCreditAccountTransaction {
              id
              __typename
              amount { amount currencyCode }
              balanceAfterTransaction { amount currencyCode }
              account { id balance { amount currencyCode } }
            }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "accountId": store_credit_account_id }),
    ));
    assert_eq!(store_credit_debit.status, 200);
    assert_eq!(
        store_credit_debit.body["data"]["debit"]["userErrors"],
        json!([])
    );
    let store_credit_debit_id = required_string(
        &store_credit_debit.body["data"]["debit"]["storeCreditAccountTransaction"]["id"],
        "store credit debit transaction id",
    );

    restore_shop_currency(&mut proxy, "CAD");
    let gift_card_setup = proxy.process_request(json_graphql_request(
        r#"
        mutation AdminNodeGiftCardSetup {
          create: giftCardCreate(input: { initialValue: "20.00" }) {
            giftCard { id balance { amount currencyCode } }
            userErrors { field code message }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(gift_card_setup.status, 200);
    assert_eq!(
        gift_card_setup.body["data"]["create"]["userErrors"],
        json!([])
    );
    let gift_card_id = required_string(
        &gift_card_setup.body["data"]["create"]["giftCard"]["id"],
        "gift card id",
    );

    let gift_card_transactions = proxy.process_request(json_graphql_request(
        r#"
        mutation AdminNodeGiftCardTransactions($id: ID!) {
          credit: giftCardCredit(
            id: $id
            creditInput: { creditAmount: { amount: "2.00", currencyCode: CAD }, note: "node credit" }
          ) {
            giftCardCreditTransaction { id note amount { amount currencyCode } giftCard { id balance { amount currencyCode } } }
            userErrors { field code message }
          }
          debit: giftCardDebit(
            id: $id
            debitInput: { debitAmount: { amount: "4.00", currencyCode: CAD }, note: "node debit" }
          ) {
            giftCardDebitTransaction { id note amount { amount currencyCode } giftCard { id balance { amount currencyCode } } }
            userErrors { field code message }
          }
        }
        "#,
        json!({ "id": gift_card_id }),
    ));
    assert_eq!(gift_card_transactions.status, 200);
    assert_eq!(
        gift_card_transactions.body["data"]["credit"]["userErrors"],
        json!([])
    );
    assert_eq!(
        gift_card_transactions.body["data"]["debit"]["userErrors"],
        json!([])
    );
    let gift_card_credit_id = required_string(
        &gift_card_transactions.body["data"]["credit"]["giftCardCreditTransaction"]["id"],
        "gift card credit transaction id",
    );
    let gift_card_debit_id = required_string(
        &gift_card_transactions.body["data"]["debit"]["giftCardDebitTransaction"]["id"],
        "gift card debit transaction id",
    );

    let node_read = proxy.process_request(json_graphql_request(
        r#"
        query AdminNodeReadback(
          $customerId: ID!
          $addressId: ID!
          $paymentMethodId: ID!
          $storeCreditAccountId: ID!
          $storeCreditCreditId: ID!
          $storeCreditDebitId: ID!
          $giftCardCreditId: ID!
          $giftCardDebitId: ID!
          $ids: [ID!]!
          $accountBatchIds: [ID!]!
          $mixedStoreCreditIds: [ID!]!
        ) {
          customerNode: node(id: $customerId) {
            __typename
            ... on Customer {
              id
              email
              displayName
              addressesV2(first: 2) { nodes { id address1 city countryCodeV2 } }
            }
          }
          addressNode: node(id: $addressId) {
            __typename
            ... on MailingAddress { id address1 city countryCodeV2 }
          }
          paymentNode: node(id: $paymentMethodId) {
            __typename
            ... on CustomerPaymentMethod {
              id
              customer { id }
              revokedAt
              revokedReason
              instrument {
                __typename
                ... on CustomerCreditCard {
                  billingAddress { address1 city countryCode }
                }
              }
            }
          }
          storeCreditAccountNode: node(id: $storeCreditAccountId) {
            __typename
            ... on StoreCreditAccount {
              id
              owner { ... on Customer { id } }
              balance { amount currencyCode }
              transactions(first: 5) {
                nodes {
                  __typename
                  balanceAfterTransaction { amount currencyCode }
                  event
                  origin
                  ... on StoreCreditAccountCreditTransaction { id amount { amount currencyCode } balanceAfterTransaction { amount currencyCode } }
                  ... on StoreCreditAccountDebitTransaction { id amount { amount currencyCode } balanceAfterTransaction { amount currencyCode } }
                }
              }
            }
          }
          storeCreditCreditNode: node(id: $storeCreditCreditId) {
            __typename
            ... on StoreCreditAccountCreditTransaction {
              id
              amount { amount currencyCode }
              balanceAfterTransaction { amount currencyCode }
              account { id }
            }
          }
          storeCreditDebitNode: node(id: $storeCreditDebitId) {
            __typename
            ... on StoreCreditAccountDebitTransaction {
              id
              amount { amount currencyCode }
              balanceAfterTransaction { amount currencyCode }
              account { id }
            }
          }
          giftCardCreditNode: node(id: $giftCardCreditId) {
            __typename
            ... on GiftCardCreditTransaction {
              id
              note
              amount { amount currencyCode }
              giftCard { id balance { amount currencyCode } }
            }
          }
          giftCardDebitNode: node(id: $giftCardDebitId) {
            __typename
            ... on GiftCardDebitTransaction {
              id
              note
              amount { amount currencyCode }
              giftCard { id balance { amount currencyCode } }
            }
          }
          ordered: nodes(ids: $ids) {
            __typename
            ... on MailingAddress { id address1 }
            ... on StoreCreditAccountCreditTransaction { id amount { amount currencyCode } }
            ... on GiftCardDebitTransaction { id note }
            ... on Customer { id email }
          }
          accountBatch: nodes(ids: $accountBatchIds) {
            __typename
            ... on StoreCreditAccount {
              id
              transactions(first: 5) {
                nodes {
                  __typename
                  amount { amount currencyCode }
                  balanceAfterTransaction { amount currencyCode }
                  event
                  origin
                  ... on StoreCreditAccountCreditTransaction {
                    id
                    remainingAmount { amount currencyCode }
                  }
                  ... on StoreCreditAccountDebitTransaction {
                    id
                    account { id }
                  }
                }
              }
            }
          }
          mixedStoreCreditBatch: nodes(ids: $mixedStoreCreditIds) {
            __typename
            ... on StoreCreditAccount {
              id
              transactions(first: 5) {
                nodes {
                  __typename
                  amount { amount currencyCode }
                  balanceAfterTransaction { amount currencyCode }
                  event
                  origin
                  ... on StoreCreditAccountCreditTransaction {
                    id
                    remainingAmount { amount currencyCode }
                  }
                  ... on StoreCreditAccountDebitTransaction {
                    id
                    account { id }
                  }
                }
              }
            }
            ... on StoreCreditAccountCreditTransaction {
              id
              amount { amount currencyCode }
              balanceAfterTransaction { amount currencyCode }
              event
              origin
              remainingAmount { amount currencyCode }
              account { id }
            }
            ... on StoreCreditAccountDebitTransaction {
              id
              amount { amount currencyCode }
              balanceAfterTransaction { amount currencyCode }
              event
              origin
              account { id }
            }
            ... on GiftCard {
              id
              transactions(first: 5) {
                nodes {
                  __typename
                  id
                  note
                  amount { amount currencyCode }
                }
              }
            }
          }
        }
        "#,
        json!({
            "customerId": customer_id,
            "addressId": address_id,
            "paymentMethodId": payment_method_id,
            "storeCreditAccountId": store_credit_account_id,
            "storeCreditCreditId": store_credit_credit_id,
            "storeCreditDebitId": store_credit_debit_id,
            "giftCardCreditId": gift_card_credit_id,
            "giftCardDebitId": gift_card_debit_id,
            "ids": [
                address_id,
                "gid://shopify/CustomerPaymentMethod/999999999999",
                store_credit_credit_id,
                gift_card_debit_id,
                customer_id
            ],
            "accountBatchIds": [
                store_credit_account_id,
                "gid://shopify/StoreCreditAccount/999999999999"
            ],
            "mixedStoreCreditIds": [
                store_credit_account_id,
                store_credit_credit_id,
                store_credit_debit_id,
                gift_card_id
            ]
        }),
    ));
    assert_eq!(node_read.status, 200);
    assert_eq!(node_read.body.get("errors"), None);

    assert_eq!(
        node_read.body["data"]["customerNode"],
        json!({
            "__typename": "Customer",
            "id": customer_id,
            "email": "admin-node-customer@example.test",
            "displayName": "Node Customer",
            "addressesV2": {
                "nodes": [{
                    "id": address_id,
                    "address1": "1 Node St",
                    "city": "Ottawa",
                    "countryCodeV2": "CA"
                }]
            }
        })
    );
    assert_eq!(
        node_read.body["data"]["addressNode"],
        json!({
            "__typename": "MailingAddress",
            "id": address_id,
            "address1": "1 Node St",
            "city": "Ottawa",
            "countryCodeV2": "CA"
        })
    );
    assert_eq!(
        node_read.body["data"]["paymentNode"],
        json!({
            "__typename": "CustomerPaymentMethod",
            "id": payment_method_id,
            "customer": { "id": customer_id },
            "revokedAt": Value::Null,
            "revokedReason": Value::Null,
            "instrument": {
                "__typename": "CustomerCreditCard",
                "billingAddress": {
                    "address1": "2 Billing St",
                    "city": "Toronto",
                    "countryCode": "CA"
                }
            }
        })
    );
    assert_eq!(
        node_read.body["data"]["storeCreditAccountNode"],
        json!({
            "__typename": "StoreCreditAccount",
            "id": store_credit_account_id,
            "owner": { "id": customer_id },
            "balance": { "amount": "7.0", "currencyCode": "USD" },
            "transactions": {
                "nodes": [
                    {
                        "__typename": "StoreCreditAccountCreditTransaction",
                        "id": store_credit_credit_id,
                        "amount": { "amount": "10.0", "currencyCode": "USD" },
                        "balanceAfterTransaction": { "amount": "10.0", "currencyCode": "USD" },
                        "event": "ADJUSTMENT",
                        "origin": Value::Null
                    },
                    {
                        "__typename": "StoreCreditAccountDebitTransaction",
                        "id": store_credit_debit_id,
                        "amount": { "amount": "-3.0", "currencyCode": "USD" },
                        "balanceAfterTransaction": { "amount": "7.0", "currencyCode": "USD" },
                        "event": "ADJUSTMENT",
                        "origin": Value::Null
                    }
                ]
            }
        })
    );
    assert_eq!(
        node_read.body["data"]["storeCreditCreditNode"],
        json!({
            "__typename": "StoreCreditAccountCreditTransaction",
            "id": store_credit_credit_id,
            "amount": { "amount": "10.0", "currencyCode": "USD" },
            "balanceAfterTransaction": { "amount": "10.0", "currencyCode": "USD" },
            "account": { "id": store_credit_account_id }
        })
    );
    assert_eq!(
        node_read.body["data"]["storeCreditDebitNode"],
        json!({
            "__typename": "StoreCreditAccountDebitTransaction",
            "id": store_credit_debit_id,
            "amount": { "amount": "-3.0", "currencyCode": "USD" },
            "balanceAfterTransaction": { "amount": "7.0", "currencyCode": "USD" },
            "account": { "id": store_credit_account_id }
        })
    );
    assert_eq!(
        node_read.body["data"]["accountBatch"],
        json!([
            {
                "__typename": "StoreCreditAccount",
                "id": store_credit_account_id,
                "transactions": {
                    "nodes": [
                        {
                            "__typename": "StoreCreditAccountCreditTransaction",
                            "id": store_credit_credit_id,
                            "amount": { "amount": "10.0", "currencyCode": "USD" },
                            "balanceAfterTransaction": { "amount": "10.0", "currencyCode": "USD" },
                            "event": "ADJUSTMENT",
                            "origin": Value::Null,
                            "remainingAmount": { "amount": "7.0", "currencyCode": "USD" }
                        },
                        {
                            "__typename": "StoreCreditAccountDebitTransaction",
                            "id": store_credit_debit_id,
                            "amount": { "amount": "-3.0", "currencyCode": "USD" },
                            "balanceAfterTransaction": { "amount": "7.0", "currencyCode": "USD" },
                            "event": "ADJUSTMENT",
                            "origin": Value::Null,
                            "account": { "id": store_credit_account_id }
                        }
                    ]
                }
            },
            Value::Null
        ])
    );
    assert_eq!(
        node_read.body["data"]["mixedStoreCreditBatch"][0]["transactions"],
        node_read.body["data"]["accountBatch"][0]["transactions"]
    );
    assert_eq!(
        node_read.body["data"]["giftCardCreditNode"],
        json!({
            "__typename": "GiftCardCreditTransaction",
            "id": gift_card_credit_id,
            "note": "node credit",
            "amount": { "amount": "2.0", "currencyCode": "CAD" },
            "giftCard": {
                "id": gift_card_id,
                "balance": { "amount": "22.0", "currencyCode": "CAD" }
            }
        })
    );
    assert_eq!(
        node_read.body["data"]["giftCardDebitNode"],
        json!({
            "__typename": "GiftCardDebitTransaction",
            "id": gift_card_debit_id,
            "note": "node debit",
            "amount": { "amount": "-4.0", "currencyCode": "CAD" },
            "giftCard": {
                "id": gift_card_id,
                "balance": { "amount": "18.0", "currencyCode": "CAD" }
            }
        })
    );
    assert_eq!(
        node_read.body["data"]["ordered"],
        json!([
            {
                "__typename": "MailingAddress",
                "id": address_id,
                "address1": "1 Node St"
            },
            Value::Null,
            {
                "__typename": "StoreCreditAccountCreditTransaction",
                "id": store_credit_credit_id,
                "amount": { "amount": "10.0", "currencyCode": "USD" }
            },
            {
                "__typename": "GiftCardDebitTransaction",
                "id": gift_card_debit_id,
                "note": "node debit"
            },
            {
                "__typename": "Customer",
                "id": customer_id,
                "email": "admin-node-customer@example.test"
            }
        ])
    );

    let dump = proxy.process_request(request_with_body("POST", "/__meta/dump", ""));
    assert_eq!(dump.status, 200);
    let mut restored_proxy = snapshot_proxy();
    let restore = restored_proxy.process_request(request_with_body(
        "POST",
        "/__meta/restore",
        &dump.body.to_string(),
    ));
    assert_eq!(restore.status, 200);

    let restored_read = restored_proxy.process_request(json_graphql_request(
        r#"
        query AdminNodeRestoredRead($ids: [ID!]!) {
          nodes(ids: $ids) {
            __typename
            ... on Customer { id }
            ... on MailingAddress { id }
            ... on CustomerPaymentMethod { id }
            ... on StoreCreditAccount { id }
            ... on StoreCreditAccountCreditTransaction { id }
            ... on GiftCardCreditTransaction { id }
          }
        }
        "#,
        json!({
            "ids": [
                customer_id,
                address_id,
                payment_method_id,
                store_credit_account_id,
                store_credit_credit_id,
                gift_card_credit_id
            ]
        }),
    ));
    assert_eq!(restored_read.status, 200);
    assert_eq!(
        restored_read.body["data"]["nodes"],
        json!([
            { "__typename": "Customer", "id": customer_id },
            { "__typename": "MailingAddress", "id": address_id },
            { "__typename": "CustomerPaymentMethod", "id": payment_method_id },
            { "__typename": "StoreCreditAccount", "id": store_credit_account_id },
            { "__typename": "StoreCreditAccountCreditTransaction", "id": store_credit_credit_id },
            { "__typename": "GiftCardCreditTransaction", "id": gift_card_credit_id }
        ])
    );

    let reset = restored_proxy.process_request(request_with_body("POST", "/__meta/reset", ""));
    assert_eq!(reset.status, 200);
    let reset_read = restored_proxy.process_request(json_graphql_request(
        r#"
        query AdminNodeResetRead($ids: [ID!]!) {
          nodes(ids: $ids) { __typename ... on Node { id } }
        }
        "#,
        json!({
            "ids": [
                customer_id,
                address_id,
                payment_method_id,
                store_credit_account_id,
                store_credit_credit_id,
                gift_card_credit_id
            ]
        }),
    ));
    assert_eq!(reset_read.status, 200);
    assert_eq!(
        reset_read.body["data"]["nodes"],
        json!([null, null, null, null, null, null])
    );

    let revoke = proxy.process_request(json_graphql_request(
        r#"
        mutation AdminNodePaymentMethodRevoke($id: ID!) {
          customerPaymentMethodRevoke(customerPaymentMethodId: $id) {
            revokedCustomerPaymentMethodId
            userErrors { field message }
          }
        }
        "#,
        json!({ "id": payment_method_id }),
    ));
    assert_eq!(revoke.status, 200);
    assert_eq!(
        revoke.body["data"]["customerPaymentMethodRevoke"]["userErrors"],
        json!([])
    );

    let delete = proxy.process_request(json_graphql_request(
        r#"
        mutation AdminNodeCustomerDelete($id: ID!) {
          customerDelete(input: { id: $id }) {
            deletedCustomerId
            userErrors { field message }
          }
        }
        "#,
        json!({ "id": customer_id }),
    ));
    assert_eq!(delete.status, 200);
    assert_eq!(
        delete.body["data"]["customerDelete"]["userErrors"],
        json!([])
    );

    let removed_read = proxy.process_request(json_graphql_request(
        r#"
        query AdminNodeRemovedRead($customerId: ID!, $addressId: ID!, $paymentMethodId: ID!) {
          customerNode: node(id: $customerId) { __typename ... on Customer { id } }
          addressNode: node(id: $addressId) { __typename ... on MailingAddress { id } }
          paymentNode: node(id: $paymentMethodId) { __typename ... on CustomerPaymentMethod { id } }
        }
        "#,
        json!({
            "customerId": customer_id,
            "addressId": address_id,
            "paymentMethodId": payment_method_id
        }),
    ));
    assert_eq!(removed_read.status, 200);
    assert_eq!(
        removed_read.body["data"],
        json!({
            "customerNode": null,
            "addressNode": null,
            "paymentNode": null
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
fn location_activate_limit_and_control_branches_use_store_state() {
    let at_limit = Arc::new(Mutex::new(false));
    let limit_flag = Arc::clone(&at_limit);
    let mut proxy =
        configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport(move |_request| {
            let limit_reached = *limit_flag.lock().unwrap();
            let nodes = if limit_reached {
                json!([{ "id": "gid://shopify/Location/live-limit", "isActive": true, "isFulfillmentService": false }])
            } else {
                json!([])
            };
            Response {
                status: 200,
                headers: Default::default(),
                body: json!({
                    "data": {
                        "shop": { "resourceLimits": { "locationLimit": if limit_reached { 1 } else { 10 } } },
                        "locations": {
                            "nodes": nodes,
                            "pageInfo": { "hasNextPage": false }
                        }
                    }
                }),
            }
        });
    let deactivate_query = r#"
        mutation LocationActivateLimitSetupDeactivate($locationId: ID!, $idempotencyKey: String!) {
          locationDeactivate(locationId: $locationId) @idempotent(key: $idempotencyKey) {
            location { id isActive }
            locationDeactivateUserErrors { field code message }
          }
        }
    "#;
    let activate_query = r#"
        mutation LocationActivateLimitAndRelocation($locationId: ID!, $idempotencyKey: String!) {
          locationActivate(locationId: $locationId) @idempotent(key: $idempotencyKey) {
            location { id isActive }
            locationActivateUserErrors { field code message }
          }
        }
    "#;

    let control_id = add_platform_location(&mut proxy, "Activation control", false);
    let deactivate_control = proxy.process_request(json_graphql_request(
        deactivate_query,
        json!({ "locationId": control_id.clone(), "idempotencyKey": "activate-control-deactivate" }),
    ));
    assert_eq!(
        deactivate_control.body["data"]["locationDeactivate"]["locationDeactivateUserErrors"],
        json!([])
    );

    let control = proxy.process_request(json_graphql_request(
        activate_query,
        json!({ "locationId": control_id.clone(), "idempotencyKey": "activate-control" }),
    ));
    assert_eq!(
        control.body["data"]["locationActivate"],
        json!({
            "location": { "id": control.body["data"]["locationActivate"]["location"]["id"].clone(), "isActive": true },
            "locationActivateUserErrors": []
        })
    );

    let target_id = add_platform_location(&mut proxy, "Activation limit target", false);
    let deactivate_target = proxy.process_request(json_graphql_request(
        deactivate_query,
        json!({ "locationId": target_id.clone(), "idempotencyKey": "activate-limit-deactivate" }),
    ));
    assert_eq!(
        deactivate_target.body["data"]["locationDeactivate"]["locationDeactivateUserErrors"],
        json!([])
    );
    *at_limit.lock().unwrap() = true;

    let limit = proxy.process_request(json_graphql_request(
        activate_query,
        json!({ "locationId": target_id.clone(), "idempotencyKey": "activate-limit" }),
    ));
    assert_eq!(
        limit.body["data"]["locationActivate"],
        json!({
            "location": { "id": limit.body["data"]["locationActivate"]["location"]["id"].clone(), "isActive": false },
            "locationActivateUserErrors": [{
                "field": ["locationId"],
                "code": "LOCATION_LIMIT",
                "message": "Shop has reached its location limit."
            }]
        })
    );
}

#[test]
fn location_activate_ongoing_relocation_branch_uses_hydrated_upstream_state() {
    let target_id = "gid://shopify/Location/hydrated-relocation";
    let mut proxy =
        configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport(move |request| {
            let body: Value = serde_json::from_str(&request.body).unwrap();
            let query = body["query"].as_str().unwrap_or_default();
            let response_body = if query.contains("StorePropertiesLocationHydrate") {
                json!({
                    "data": {
                        "location": {
                            "__typename": "Location",
                            "id": target_id,
                            "legacyResourceId": "hydrated-relocation",
                            "name": "Hydrated relocation",
                            "activatable": true,
                            "addressVerified": true,
                            "createdAt": "2026-06-30T00:00:00Z",
                            "deactivatable": true,
                            "deactivatedAt": "2026-06-30T00:00:00Z",
                            "deletable": true,
                            "fulfillsOnlineOrders": false,
                            "hasActiveInventory": false,
                            "hasUnfulfilledOrders": false,
                            "isActive": false,
                            "isFulfillmentService": false,
                            "isPrimary": false,
                            "shipsInventory": false,
                            "updatedAt": "2026-06-30T00:00:00Z",
                            "hasOngoingRelocation": true,
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
                })
            } else {
                json!({
                    "data": {
                        "shop": { "resourceLimits": { "locationLimit": 200 } },
                        "locations": {
                            "nodes": [{ "id": "gid://shopify/Location/primary", "isActive": true, "isFulfillmentService": false }],
                            "pageInfo": { "hasNextPage": false }
                        }
                    }
                })
            };
            Response {
                status: 200,
                headers: Default::default(),
                body: response_body,
            }
        });

    let relocation = proxy.process_request(json_graphql_request(
        r#"
        mutation HydratedRelocationActivate($locationId: ID!, $idempotencyKey: String!) {
          locationActivate(locationId: $locationId) @idempotent(key: $idempotencyKey) {
            location { id isActive }
            locationActivateUserErrors { field code message }
          }
        }
        "#,
        json!({ "locationId": target_id, "idempotencyKey": "hydrated-relocation" }),
    ));
    assert_eq!(
        relocation.body["data"]["locationActivate"],
        json!({
            "location": { "id": target_id, "isActive": false },
            "locationActivateUserErrors": [{
                "field": ["locationId"],
                "code": "HAS_ONGOING_RELOCATION",
                "message": "This location currently cannot be activated as inventory, pending orders or transfers are being relocated from this location. Please try again later."
            }]
        })
    );
}

#[test]
fn location_add_resource_limit_guard_uses_hydrated_resource_limit_without_logging_rejections() {
    let upstream_requests = Arc::new(Mutex::new(Vec::<Value>::new()));
    let captured_requests = Arc::clone(&upstream_requests);
    let mut proxy =
        configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport(move |request| {
            let body = serde_json::from_str::<Value>(&request.body).unwrap_or(Value::Null);
            captured_requests.lock().unwrap().push(body);
            Response {
                status: 200,
                headers: Default::default(),
                body: json!({
                    "data": {
                        "shop": { "resourceLimits": { "locationLimit": 2 } },
                        "locations": {
                            "nodes": [
                                { "id": "gid://shopify/Location/limit-1", "isActive": true, "isFulfillmentService": false },
                                { "id": "gid://shopify/Location/limit-2", "isActive": true, "isFulfillmentService": false }
                            ],
                            "pageInfo": { "hasNextPage": false }
                        }
                    }
                }),
            }
        });
    let add_query = r#"
        mutation LocationAddResourceLimitReached($input: LocationAddInput!) {
          locationAdd(input: $input) {
            location { id name }
            userErrors { field code message }
          }
        }
    "#;

    let add = proxy.process_request(json_graphql_request(
        add_query,
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
                "message": "You have reached the maximum number of locations (2)"
            }]
        })
    );
    assert_eq!(
        upstream_requests.lock().unwrap()[0]["operationName"],
        json!("StorePropertiesLocationLimitStatus")
    );

    let log = proxy.process_request(Request {
        method: "GET".to_string(),
        path: "/__meta/log".to_string(),
        headers: Default::default(),
        body: String::new(),
    });
    assert_eq!(
        log.body["entries"].as_array().unwrap().len(),
        0,
        "rejected over-cap locationAdd must not append a staged mutation log entry"
    );
}

#[test]
fn location_limit_hydration_falls_back_to_minimal_status_query() {
    let upstream_requests = Arc::new(Mutex::new(Vec::<Value>::new()));
    let captured_requests = Arc::clone(&upstream_requests);
    let mut proxy =
        configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport(move |request| {
            let body = serde_json::from_str::<Value>(&request.body).unwrap_or(Value::Null);
            let query = body["query"].as_str().unwrap_or_default().to_string();
            captured_requests.lock().unwrap().push(body);
            if query.contains("includeLegacy: true") {
                return Response {
                    status: 502,
                    headers: Default::default(),
                    body: json!({ "errors": [{ "message": "catalog unavailable" }] }),
                };
            }
            Response {
                status: 200,
                headers: Default::default(),
                body: json!({
                    "data": {
                        "shop": { "resourceLimits": { "locationLimit": 1 } },
                        "locations": {
                            "nodes": [{
                                "id": "gid://shopify/Location/fallback-limit",
                                "isActive": true,
                                "isFulfillmentService": false
                            }],
                            "pageInfo": { "hasNextPage": false }
                        }
                    }
                }),
            }
        });

    let add = proxy.process_request(json_graphql_request(
        r#"
        mutation LocationFallbackLimit($input: LocationAddInput!) {
          locationAdd(input: $input) {
            location { id }
            userErrors { field code message }
          }
        }
        "#,
        json!({
            "input": {
                "name": "Fallback overflow",
                "address": { "countryCode": "US" }
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
                "message": "You have reached the maximum number of locations (1)"
            }]
        })
    );
    let requests = upstream_requests.lock().unwrap();
    assert_eq!(requests.len(), 2);
    assert!(requests[0]["query"]
        .as_str()
        .is_some_and(|query| query.contains("includeInactive: true, includeLegacy: true")));
    assert!(requests[1]["query"]
        .as_str()
        .is_some_and(|query| query.contains("includeInactive: true) { nodes { id isActive")));
}

#[test]
fn location_overlay_preserves_hydrated_catalog_for_reads_validation_and_limits() {
    let upstream_requests = Arc::new(Mutex::new(Vec::<Value>::new()));
    let captured_requests = Arc::clone(&upstream_requests);
    let mut proxy =
        configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport(move |request| {
            let body = serde_json::from_str::<Value>(&request.body).unwrap_or(Value::Null);
            captured_requests.lock().unwrap().push(body.clone());
            if body["query"]
                .as_str()
                .is_some_and(|query| query.contains("ShippingDeliveryProfileLocationsHydrate"))
            {
                return Response {
                    status: 200,
                    headers: Default::default(),
                    body: json!({
                        "data": {
                            "locationsAvailableForDeliveryProfilesConnection": {
                                "nodes": [
                                    {
                                        "id": "gid://shopify/Location/base-east",
                                        "name": "Baseline East",
                                        "isActive": true,
                                        "isFulfillmentService": false
                                    },
                                    {
                                        "id": "gid://shopify/Location/base-filter-decoy",
                                        "name": "Baseline Fill East 1",
                                        "isActive": true,
                                        "isFulfillmentService": false
                                    }
                                ]
                            }
                        }
                    }),
                };
            }
            Response {
                status: 200,
                headers: Default::default(),
                body: json!({
                    "data": {
                        "shop": { "resourceLimits": { "locationLimit": 4 } },
                        "locations": {
                            "nodes": [
                                {
                                    "__typename": "Location",
                                    "id": "gid://shopify/Location/base-east",
                                    "name": "Baseline East",
                                    "isActive": true,
                                    "isFulfillmentService": false,
                                    "fulfillsOnlineOrders": true,
                                    "hasActiveInventory": false,
                                    "hasUnfulfilledOrders": false,
                                    "deletable": false,
                                    "address": { "countryCode": "US" }
                                },
                                {
                                    "__typename": "Location",
                                    "id": "gid://shopify/Location/base-west",
                                    "name": "Baseline West",
                                    "isActive": true,
                                    "isFulfillmentService": false,
                                    "fulfillsOnlineOrders": true,
                                    "hasActiveInventory": false,
                                    "hasUnfulfilledOrders": false,
                                    "deletable": false,
                                    "address": { "countryCode": "US" }
                                },
                                {
                                    "__typename": "Location",
                                    "id": "gid://shopify/Location/base-filter-decoy",
                                    "name": "Baseline Fill East 1",
                                    "isActive": true,
                                    "isFulfillmentService": false,
                                    "fulfillsOnlineOrders": true,
                                    "hasActiveInventory": false,
                                    "hasUnfulfilledOrders": false,
                                    "deletable": false,
                                    "address": { "countryCode": "US" }
                                }
                            ],
                            "pageInfo": { "hasNextPage": false }
                        }
                    }
                }),
            }
        });

    let duplicate = proxy.process_request(json_graphql_request(
        r#"
        mutation LocationOverlayDuplicate($input: LocationAddInput!) {
          locationAdd(input: $input) {
            location { id }
            userErrors { field code message }
          }
        }
        "#,
        json!({
            "input": {
                "name": "Baseline West",
                "address": { "countryCode": "US" }
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

    let add = proxy.process_request(json_graphql_request(
        r#"
        mutation LocationOverlayAdd($input: LocationAddInput!) {
          locationAdd(input: $input) {
            location { id name }
            userErrors { field code message }
          }
        }
        "#,
        json!({
            "input": {
                "name": "Staged East",
                "address": { "countryCode": "US" }
            }
        }),
    ));
    assert_eq!(add.body["data"]["locationAdd"]["userErrors"], json!([]));
    let staged_id = add.body["data"]["locationAdd"]["location"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let read = proxy.process_request(json_graphql_request(
        r#"
        query LocationOverlayRead($baseId: ID!) {
          base: location(id: $baseId) { id name address { countryCode } }
          locations(first: 5) {
            nodes { id name }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
          locationsCount { count precision }
          filteredCount: locationsCount(query: "name:Baseline") { count precision }
          exactFiltered: locations(
            first: 5
            query: "name:'Baseline East' OR name:'Staged East'"
            sortKey: NAME
          ) {
            nodes { id name }
          }
          availableForDeliveryProfiles: locationsAvailableForDeliveryProfilesConnection(first: 5) {
            nodes { id name }
          }
        }
        "#,
        json!({ "baseId": "gid://shopify/Location/base-west" }),
    ));
    assert_eq!(
        read.body["data"]["base"],
        json!({
            "id": "gid://shopify/Location/base-west",
            "name": "Baseline West",
            "address": { "countryCode": "US" }
        })
    );
    assert_eq!(
        read.body["data"]["locations"]["nodes"],
        json!([
            { "id": "gid://shopify/Location/base-east", "name": "Baseline East" },
            { "id": "gid://shopify/Location/base-filter-decoy", "name": "Baseline Fill East 1" },
            { "id": "gid://shopify/Location/base-west", "name": "Baseline West" },
            { "id": staged_id, "name": "Staged East" }
        ])
    );
    assert_eq!(
        read.body["data"]["locationsCount"],
        json!({ "count": 4, "precision": "EXACT" })
    );
    assert_eq!(
        read.body["data"]["filteredCount"],
        json!({ "count": 3, "precision": "EXACT" })
    );
    assert_eq!(
        read.body["data"]["exactFiltered"]["nodes"],
        json!([
            { "id": "gid://shopify/Location/base-east", "name": "Baseline East" },
            { "id": staged_id, "name": "Staged East" }
        ])
    );
    assert_eq!(
        read.body["data"]["availableForDeliveryProfiles"]["nodes"],
        json!([
            { "id": "gid://shopify/Location/base-east", "name": "Baseline East" },
            { "id": "gid://shopify/Location/base-filter-decoy", "name": "Baseline Fill East 1" }
        ]),
        "a staged add must not invent delivery-profile eligibility for a location absent from the hydrated eligibility catalog"
    );

    let over_limit = proxy.process_request(json_graphql_request(
        r#"
        mutation LocationOverlayOverLimit($input: LocationAddInput!) {
          locationAdd(input: $input) {
            location { id }
            userErrors { field code message }
          }
        }
        "#,
        json!({
            "input": {
                "name": "Staged North",
                "address": { "countryCode": "US" }
            }
        }),
    ));
    assert_eq!(
        over_limit.body["data"]["locationAdd"],
        json!({
            "location": null,
            "userErrors": [{
                "field": ["input"],
                "code": "INVALID",
                "message": "You have reached the maximum number of locations (4)"
            }]
        })
    );

    assert!(
        upstream_requests
            .lock()
            .unwrap()
            .iter()
            .all(|request| request["query"]
                .as_str()
                .is_some_and(|query| query.trim_start().starts_with("query "))),
        "supported location lifecycle traffic may hydrate read-only catalogs but must never forward a mutation before commit"
    );
}

#[test]
fn generic_location_add_stages_location_and_downstream_reads() {
    let product_id = "gid://shopify/Product/9101";
    let mut proxy = snapshot_proxy().with_base_products(vec![platform_inventory_base_product(
        product_id,
        "Generic Location Add Product",
    )]);
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

    let unlisted_country_display = proxy.process_request(json_graphql_request(
        r#"
        mutation GenericLocationAddCountryDisplay($input: LocationAddInput!) {
          locationAdd(input: $input) {
            location { id address { country countryCode province provinceCode } }
            userErrors { field code message }
          }
        }
        "#,
        json!({
            "input": {
                "name": "Generic Add Bangkok Display",
                "address": { "countryCode": "TH" }
            }
        }),
    ));
    assert_eq!(
        unlisted_country_display.body["data"]["locationAdd"]["userErrors"],
        json!([])
    );
    let thailand_id = unlisted_country_display.body["data"]["locationAdd"]["location"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(
        unlisted_country_display.body["data"]["locationAdd"]["location"]["address"],
        json!({
            "country": "Thailand",
            "countryCode": "TH",
            "province": null,
            "provinceCode": null
        })
    );

    let thailand_read = proxy.process_request(json_graphql_request(
        r#"
        query GenericLocationAddCountryDisplayRead($id: ID!) {
          location(id: $id) { address { country countryCode province provinceCode } }
        }
        "#,
        json!({ "id": thailand_id }),
    ));
    assert_eq!(
        thailand_read.body["data"]["location"]["address"],
        json!({
            "country": "Thailand",
            "countryCode": "TH",
            "province": null,
            "provinceCode": null
        })
    );

    let raw_province_display = proxy.process_request(json_graphql_request(
        r#"
        mutation GenericLocationAddRawProvinceDisplay($input: LocationAddInput!) {
          locationAdd(input: $input) {
            location { address { country countryCode province provinceCode } }
            userErrors { field code message }
          }
        }
        "#,
        json!({
            "input": {
                "name": "Generic Add Raw Province Display",
                "address": { "countryCode": "TH", "provinceCode": "10" }
            }
        }),
    ));
    assert_eq!(
        raw_province_display.body["data"]["locationAdd"],
        json!({
            "location": {
                "address": {
                    "country": "Thailand",
                    "countryCode": "TH",
                    "province": "10",
                    "provinceCode": "10"
                }
            },
            "userErrors": []
        })
    );

    let dubai_display = proxy.process_request(json_graphql_request(
        r#"
        mutation GenericLocationAddProvinceDisplay($input: LocationAddInput!) {
          locationAdd(input: $input) {
            location { id address { country countryCode province provinceCode } }
            userErrors { field code message }
          }
        }
        "#,
        json!({
            "input": {
                "name": "Generic Add Dubai Display",
                "address": { "countryCode": "AE", "provinceCode": "DU" }
            }
        }),
    ));
    assert_eq!(
        dubai_display.body["data"]["locationAdd"]["userErrors"],
        json!([])
    );
    let dubai_id = dubai_display.body["data"]["locationAdd"]["location"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(
        dubai_display.body["data"]["locationAdd"]["location"]["address"],
        json!({
            "country": "United Arab Emirates",
            "countryCode": "AE",
            "province": "Dubai",
            "provinceCode": "DU"
        })
    );

    let dubai_read = proxy.process_request(json_graphql_request(
        r#"
        query GenericLocationAddProvinceDisplayRead($id: ID!) {
          location(id: $id) { address { country countryCode province provinceCode } }
        }
        "#,
        json!({ "id": dubai_id }),
    ));
    assert_eq!(
        dubai_read.body["data"]["location"]["address"],
        json!({
            "country": "United Arab Emirates",
            "countryCode": "AE",
            "province": "Dubai",
            "provinceCode": "DU"
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

    let inventory_item_id =
        create_inventory_item_for_location_test(&mut proxy, "Generic location add inventory item");
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
                "quantities": [{
                    "inventoryItemId": inventory_item_id,
                    "locationId": location_id,
                    "quantity": 7,
                    "changeFromQuantity": null
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
fn top_level_locations_connection_filters_sorts_windows_and_counts() {
    let mut proxy = snapshot_proxy();
    let zulu_id = add_platform_location(&mut proxy, "Zulu Connection", false);
    let alpha_id = add_platform_location(&mut proxy, "Alpha Connection", false);
    let beta_id = add_platform_location(&mut proxy, "Beta Connection", false);
    let fulfillment_service = proxy.process_request(json_graphql_request(
        r#"
        mutation SeedLegacyLocation($name: String!) {
          fulfillmentServiceCreate(
            name: $name
            trackingSupport: true
            inventoryManagement: true
            requiresShippingMethod: true
          ) {
            fulfillmentService {
              location { id name isActive isFulfillmentService }
            }
            userErrors { field message }
          }
        }
        "#,
        json!({ "name": "Carrier Connection" }),
    ));
    assert_eq!(
        fulfillment_service.body["data"]["fulfillmentServiceCreate"]["userErrors"],
        json!([])
    );
    let legacy_location_id = fulfillment_service.body["data"]["fulfillmentServiceCreate"]
        ["fulfillmentService"]["location"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let deactivate = proxy.process_request(json_graphql_request(
        r#"
        mutation DeactivateConnectionLocation($locationId: ID!) {
          locationDeactivate(locationId: $locationId) @idempotent(key: "locations-connection-filter") {
            location { id name isActive }
            locationDeactivateUserErrors { field code message }
          }
        }
        "#,
        json!({ "locationId": beta_id }),
    ));
    assert_eq!(
        deactivate.body["data"]["locationDeactivate"]["locationDeactivateUserErrors"],
        json!([])
    );
    assert_eq!(
        deactivate.body["data"]["locationDeactivate"]["location"],
        json!({
            "id": beta_id,
            "name": "Beta Connection",
            "isActive": false
        })
    );

    let read = proxy.process_request(json_graphql_request(
        r#"
        query LocationConnectionRead($alphaCursor: String!, $zuluCursor: String!) {
          defaultLocations: locations(first: 10) {
            nodes { id name isActive isFulfillmentService }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
          includeInactive: locations(first: 10, includeInactive: true) {
            nodes { id name isActive isFulfillmentService }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
          includeLegacy: locations(first: 10, includeLegacy: true) {
            nodes { id name isActive isFulfillmentService }
          }
          queryAlpha: locations(first: 10, query: "name:Alpha") {
            nodes { id name isActive }
          }
          nameFirst: locations(first: 1, sortKey: NAME) {
            nodes { id name }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
          afterAlpha: locations(first: 1, after: $alphaCursor) {
            nodes { id name }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
          beforeZulu: locations(last: 1, before: $zuluCursor) {
            nodes { id name }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
          reversed: locations(first: 10, reverse: true) {
            nodes { id name }
          }
          activeCount: locationsCount { count precision }
          inactiveCount: locationsCount { count precision }
          legacyCount: locationsCount { count precision }
          limitedCount: locationsCount(limit: 3) { count precision }
          queryCount: locationsCount(query: "name:Alpha") { count precision }
        }
        "#,
        json!({
            "alphaCursor": alpha_id,
            "zuluCursor": zulu_id
        }),
    ));
    assert_eq!(read.status, 200);
    assert_eq!(
        read.body["data"]["defaultLocations"],
        json!({
            "nodes": [
                {
                    "id": alpha_id,
                    "name": "Alpha Connection",
                    "isActive": true,
                    "isFulfillmentService": false
                },
                {
                    "id": zulu_id,
                    "name": "Zulu Connection",
                    "isActive": true,
                    "isFulfillmentService": false
                }
            ],
            "pageInfo": {
                "hasNextPage": false,
                "hasPreviousPage": false,
                "startCursor": alpha_id,
                "endCursor": zulu_id
            }
        })
    );
    assert_eq!(
        read.body["data"]["includeInactive"]["nodes"],
        json!([
            {
                "id": alpha_id,
                "name": "Alpha Connection",
                "isActive": true,
                "isFulfillmentService": false
            },
            {
                "id": beta_id,
                "name": "Beta Connection",
                "isActive": false,
                "isFulfillmentService": false
            },
            {
                "id": zulu_id,
                "name": "Zulu Connection",
                "isActive": true,
                "isFulfillmentService": false
            }
        ])
    );
    assert_eq!(
        read.body["data"]["includeLegacy"]["nodes"],
        json!([
            {
                "id": alpha_id,
                "name": "Alpha Connection",
                "isActive": true,
                "isFulfillmentService": false
            },
            {
                "id": legacy_location_id,
                "name": "Carrier Connection",
                "isActive": true,
                "isFulfillmentService": true
            },
            {
                "id": zulu_id,
                "name": "Zulu Connection",
                "isActive": true,
                "isFulfillmentService": false
            }
        ])
    );
    assert_eq!(
        read.body["data"]["queryAlpha"]["nodes"],
        json!([{ "id": alpha_id, "name": "Alpha Connection", "isActive": true }])
    );
    assert_eq!(
        read.body["data"]["nameFirst"],
        json!({
            "nodes": [{ "id": alpha_id, "name": "Alpha Connection" }],
            "pageInfo": {
                "hasNextPage": true,
                "hasPreviousPage": false,
                "startCursor": alpha_id,
                "endCursor": alpha_id
            }
        })
    );
    assert_eq!(
        read.body["data"]["afterAlpha"],
        json!({
            "nodes": [{ "id": zulu_id, "name": "Zulu Connection" }],
            "pageInfo": {
                "hasNextPage": false,
                "hasPreviousPage": true,
                "startCursor": zulu_id,
                "endCursor": zulu_id
            }
        })
    );
    assert_eq!(
        read.body["data"]["beforeZulu"],
        json!({
            "nodes": [{ "id": alpha_id, "name": "Alpha Connection" }],
            "pageInfo": {
                "hasNextPage": true,
                "hasPreviousPage": false,
                "startCursor": alpha_id,
                "endCursor": alpha_id
            }
        })
    );
    assert_eq!(
        read.body["data"]["reversed"]["nodes"],
        json!([
            { "id": zulu_id, "name": "Zulu Connection" },
            { "id": alpha_id, "name": "Alpha Connection" }
        ])
    );
    assert_eq!(
        read.body["data"]["activeCount"],
        json!({ "count": 4, "precision": "EXACT" })
    );
    assert_eq!(
        read.body["data"]["inactiveCount"],
        json!({ "count": 4, "precision": "EXACT" })
    );
    assert_eq!(
        read.body["data"]["legacyCount"],
        json!({ "count": 4, "precision": "EXACT" })
    );
    assert_eq!(
        read.body["data"]["limitedCount"],
        json!({ "count": 3, "precision": "AT_LEAST" })
    );
    assert_eq!(
        read.body["data"]["queryCount"],
        json!({ "count": 1, "precision": "EXACT" })
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

    let display_edit = proxy.process_request(json_graphql_request(
        r#"
        mutation GenericLocationEditDisplayNames($id: ID!, $input: LocationEditInput!) {
          locationEdit(id: $id, input: $input) {
            location { address { country countryCode province provinceCode } }
            userErrors { field code message }
          }
        }
        "#,
        json!({
            "id": primary_id,
            "input": {
                "address": { "countryCode": "AE", "provinceCode": "DU" }
            }
        }),
    ));
    assert_eq!(
        display_edit.body["data"]["locationEdit"],
        json!({
            "location": {
                "address": {
                    "country": "United Arab Emirates",
                    "countryCode": "AE",
                    "province": "Dubai",
                    "provinceCode": "DU"
                }
            },
            "userErrors": []
        })
    );

    let display_read = proxy.process_request(json_graphql_request(
        r#"
        query GenericLocationEditDisplayRead($id: ID!) {
          location(id: $id) { address { country countryCode province provinceCode } }
        }
        "#,
        json!({ "id": primary_id }),
    ));
    assert_eq!(
        display_read.body["data"]["location"]["address"],
        json!({
            "country": "United Arab Emirates",
            "countryCode": "AE",
            "province": "Dubai",
            "provinceCode": "DU"
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
        read.body["data"]["locations"]["nodes"],
        json!([
            { "id": backup_id, "name": "Edit Backup" },
            { "id": primary_id, "name": "Edited Primary" }
        ])
    );

    let log = log_snapshot(&proxy);
    let roots: Vec<_> = log["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["interpreted"]["primaryRootField"].as_str().unwrap())
        .collect();
    assert_eq!(
        roots,
        vec!["locationAdd", "locationAdd", "locationEdit", "locationEdit"]
    );
    assert!(log["entries"][2]["rawBody"]
        .as_str()
        .unwrap()
        .contains("GenericLocationEdit"));
}

#[test]
fn location_edit_preserves_hydrated_address_display_names() {
    let location_id = "gid://shopify/Location/hydrated-address";
    let upstream_requests = Arc::new(Mutex::new(Vec::<Value>::new()));
    let captured_requests = Arc::clone(&upstream_requests);
    let mut proxy =
        configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport(move |request| {
            let body = serde_json::from_str::<Value>(&request.body).unwrap_or(Value::Null);
            captured_requests.lock().unwrap().push(body);
            Response {
                status: 200,
                headers: Default::default(),
                body: json!({
                    "data": {
                        "location": {
                            "id": location_id,
                            "legacyResourceId": "hydrated-address",
                            "name": "Hydrated Address Location",
                            "activatable": true,
                            "addressVerified": true,
                            "createdAt": "2026-07-01T00:00:00Z",
                            "deactivatable": true,
                            "deactivatedAt": null,
                            "deletable": false,
                            "fulfillsOnlineOrders": false,
                            "hasActiveInventory": false,
                            "hasUnfulfilledOrders": false,
                            "isActive": true,
                            "isFulfillmentService": false,
                            "isPrimary": false,
                            "shipsInventory": true,
                            "updatedAt": "2026-07-01T00:00:00Z",
                            "fulfillmentService": null,
                            "address": {
                                "address1": "Old Creek Road",
                                "address2": null,
                                "city": "Dubai",
                                "country": "United Arab Emirates",
                                "countryCode": "AE",
                                "formatted": ["Old Creek Road", "Dubai", "United Arab Emirates"],
                                "latitude": null,
                                "longitude": null,
                                "phone": null,
                                "province": "Dubai",
                                "provinceCode": "DU",
                                "zip": "00000"
                            },
                            "suggestedAddresses": [],
                            "metafield": null,
                            "metafields": { "nodes": [], "pageInfo": { "hasNextPage": false, "hasPreviousPage": false, "startCursor": null, "endCursor": null } },
                            "inventoryLevels": { "nodes": [], "pageInfo": { "hasNextPage": false, "hasPreviousPage": false, "startCursor": null, "endCursor": null } }
                        }
                    }
                }),
            }
        });

    let edit = proxy.process_request(json_graphql_request(
        r#"
        mutation PreserveHydratedAddressNames($id: ID!, $input: LocationEditInput!) {
          locationEdit(id: $id, input: $input) {
            location {
              id
              name
              address { address1 country countryCode province provinceCode }
            }
            userErrors { field code message }
          }
        }
        "#,
        json!({
            "id": location_id,
            "input": {
                "address": { "address1": "New Creek Road" }
            }
        }),
    ));

    assert_eq!(
        edit.body["data"]["locationEdit"],
        json!({
            "location": {
                "id": location_id,
                "name": "Hydrated Address Location",
                "address": {
                    "address1": "New Creek Road",
                    "country": "United Arab Emirates",
                    "countryCode": "AE",
                    "province": "Dubai",
                    "provinceCode": "DU"
                }
            },
            "userErrors": []
        })
    );
    assert_eq!(upstream_requests.lock().unwrap().len(), 1);
}

#[test]
fn generic_location_activate_stages_existing_state_and_rejects_absent_ids() {
    let mut proxy = snapshot_proxy();
    let activate_query = r#"
        mutation GenericLocationActivate($locationId: ID!, $idempotencyKey: String!) {
          locationActivate(locationId: $locationId) @idempotent(key: $idempotencyKey) {
            location { id isActive }
            locationActivateUserErrors { field code message }
          }
        }
    "#;
    let unknown_id = "gid://shopify/Location/999999999999";
    let unknown = proxy.process_request(json_graphql_request(
        activate_query,
        json!({ "locationId": unknown_id, "idempotencyKey": "generic-location-activate-unknown" }),
    ));
    assert_eq!(
        unknown.body["data"]["locationActivate"],
        json!({
            "location": null,
            "locationActivateUserErrors": [{
                "field": ["locationId"],
                "code": "LOCATION_NOT_FOUND",
                "message": "Location not found."
            }]
        })
    );

    let unknown_read = proxy.process_request(json_graphql_request(
        r#"
        query GenericLocationActivateUnknownRead($id: ID!) {
          location(id: $id) { id isActive }
        }
        "#,
        json!({ "id": unknown_id }),
    ));
    assert_eq!(unknown_read.body["data"]["location"], Value::Null);
    assert_eq!(log_snapshot(&proxy)["entries"], json!([]));

    let location_id = add_platform_location(&mut proxy, "Activation control", false);
    set_location_active_for_platform_test(&mut proxy, &location_id, false);

    let activate = proxy.process_request(json_graphql_request(
        activate_query,
        json!({ "locationId": location_id, "idempotencyKey": "generic-location-activate" }),
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

    let deleted_id = add_platform_location(&mut proxy, "Deleted activation target", false);
    set_location_active_for_platform_test(&mut proxy, &deleted_id, false);
    let delete = proxy.process_request(json_graphql_request(
        r#"
        mutation GenericLocationActivateDeleteTarget($locationId: ID!) {
          locationDelete(locationId: $locationId) {
            deletedLocationId
            locationDeleteUserErrors { field code message }
          }
        }
        "#,
        json!({ "locationId": deleted_id }),
    ));
    assert_eq!(
        delete.body["data"]["locationDelete"],
        json!({
            "deletedLocationId": deleted_id,
            "locationDeleteUserErrors": []
        })
    );
    let log_len_before_deleted_activate = log_snapshot(&proxy)["entries"].as_array().unwrap().len();
    let deleted_activate = proxy.process_request(json_graphql_request(
        activate_query,
        json!({ "locationId": deleted_id, "idempotencyKey": "generic-location-activate-deleted" }),
    ));
    assert_eq!(
        deleted_activate.body["data"]["locationActivate"],
        json!({
            "location": null,
            "locationActivateUserErrors": [{
                "field": ["locationId"],
                "code": "LOCATION_NOT_FOUND",
                "message": "Location not found."
            }]
        })
    );
    assert_eq!(
        log_snapshot(&proxy)["entries"].as_array().unwrap().len(),
        log_len_before_deleted_activate
    );
    let deleted_read = proxy.process_request(json_graphql_request(
        r#"
        query GenericLocationActivateDeletedRead($id: ID!) {
          location(id: $id) { id isActive }
        }
        "#,
        json!({ "id": deleted_id }),
    ));
    assert_eq!(deleted_read.body["data"]["location"], Value::Null);
    assert_eq!(
        state_snapshot(&proxy)["stagedState"]["deletedLocationIds"],
        json!([deleted_id])
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
    let calls = upstream_calls.lock().unwrap();
    assert_eq!(calls.len(), 4);
    assert!(calls.iter().all(|request| request["query"]
        .as_str()
        .is_some_and(|query| query.trim_start().starts_with("query "))));
}

#[test]
fn location_deactivate_hydrates_fixture_gid_instead_of_using_fixture_table() {
    let location_id = "gid://shopify/Location/112849125682";
    let upstream_requests = Arc::new(Mutex::new(Vec::<Value>::new()));
    let captured_requests = Arc::clone(&upstream_requests);
    let mut proxy =
        configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport(move |request| {
            let body = serde_json::from_str::<Value>(&request.body).unwrap_or(Value::Null);
            captured_requests.lock().unwrap().push(body);
            Response {
                status: 200,
                headers: Default::default(),
                body: json!({
                    "data": {
                        "location": {
                            "id": location_id,
                            "legacyResourceId": "112849125682",
                            "name": "Hydrated Real Source",
                            "activatable": true,
                            "addressVerified": true,
                            "createdAt": "2026-07-01T00:00:00Z",
                            "deactivatable": true,
                            "deactivatedAt": null,
                            "deletable": false,
                            "fulfillsOnlineOrders": false,
                            "hasActiveInventory": false,
                            "hasUnfulfilledOrders": false,
                            "isActive": true,
                            "isFulfillmentService": false,
                            "isPrimary": false,
                            "shipsInventory": false,
                            "updatedAt": "2026-07-01T00:00:00Z",
                            "fulfillmentService": null,
                            "address": {
                                "address1": "Hydrated Street",
                                "address2": null,
                                "city": "Dubai",
                                "country": "United Arab Emirates",
                                "countryCode": "AE",
                                "formatted": ["Hydrated Street", "Dubai", "United Arab Emirates"],
                                "latitude": null,
                                "longitude": null,
                                "phone": null,
                                "province": "Dubai",
                                "provinceCode": "DU",
                                "zip": "00000"
                            },
                            "suggestedAddresses": [],
                            "metafield": null,
                            "metafields": { "nodes": [], "pageInfo": { "hasNextPage": false, "hasPreviousPage": false, "startCursor": null, "endCursor": null } },
                            "inventoryLevels": { "nodes": [], "pageInfo": { "hasNextPage": false, "hasPreviousPage": false, "startCursor": null, "endCursor": null } }
                        }
                    }
                }),
            }
        });

    let deactivate = proxy.process_request(json_graphql_request(
        r#"
        mutation HydratedFixtureGidDeactivate($locationId: ID!) {
          locationDeactivate(locationId: $locationId) @idempotent(key: "hydrated-fixture-gid") {
            location {
              id
              name
              isActive
              address { country province }
            }
            locationDeactivateUserErrors { field code message }
          }
        }
        "#,
        json!({ "locationId": location_id }),
    ));

    assert_eq!(
        deactivate.body["data"]["locationDeactivate"],
        json!({
            "location": {
                "id": location_id,
                "name": "Hydrated Real Source",
                "isActive": false,
                "address": {
                    "country": "United Arab Emirates",
                    "province": "Dubai"
                }
            },
            "locationDeactivateUserErrors": []
        })
    );
    let requests = upstream_requests.lock().unwrap();
    assert_eq!(
        requests.len(),
        1,
        "deactivate should issue one hydrate read only"
    );
    assert_eq!(requests[0]["variables"], json!({ "id": location_id }));
}

#[test]
fn generic_location_delete_stages_tombstone_and_cascades_inventory_levels() {
    let product_id = "gid://shopify/Product/9102";
    let mut proxy = snapshot_proxy().with_base_products(vec![platform_inventory_base_product(
        product_id,
        "Generic Location Delete Product",
    )]);
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
                "fulfillsOnlineOrders": false,
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

    let inventory_item_id =
        create_inventory_item_for_location_test(&mut proxy, "Delete cascade inventory item");
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
                "quantities": [{
                    "inventoryItemId": inventory_item_id,
                    "locationId": target_id,
                    "quantity": 5,
                    "changeFromQuantity": null
                }]
            }
        }),
    ));
    assert_eq!(
        seed_inventory.body["data"]["inventorySetQuantities"]["userErrors"],
        json!([])
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
                "quantities": [{
                    "inventoryItemId": inventory_item_id,
                    "locationId": target_id,
                    "quantity": 0,
                    "changeFromQuantity": null
                }]
            }
        }),
    ));
    assert_eq!(
        cleared.body["data"]["inventorySetQuantities"]["userErrors"],
        json!([])
    );
    set_location_active_for_platform_test(&mut proxy, &target_id, false);

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
        json!({
            "input": {
                "name": "Live Local",
                "fulfillsOnlineOrders": false,
                "address": { "countryCode": "CA" }
            }
        }),
    ));
    let location_id = add.body["data"]["locationAdd"]["location"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(*upstream_calls.lock().unwrap(), 2);
    assert!(upstream_requests
        .lock()
        .unwrap()
        .iter()
        .all(|request| request["query"]
            .as_str()
            .is_some_and(|query| query.trim_start().starts_with("query "))));
    *upstream_calls.lock().unwrap() = 0;
    upstream_requests.lock().unwrap().clear();

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
        mutation LocationLiveForceInactive($locationId: ID!) {
          locationDeactivate(locationId: $locationId) @idempotent(key: "location-live-force-inactive") {
            location { id isActive }
            locationDeactivateUserErrors { field message code }
          }
        }
        "#,
        json!({ "locationId": location_id }),
    ));
    assert_eq!(
        deactivate.body["data"]["locationDeactivate"]["location"]["isActive"],
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
fn location_deactivate_recomputes_inventory_for_hydrated_base_location() {
    let location_id = "gid://shopify/Location/live-inventory-base";
    let upstream_calls = Arc::new(Mutex::new(Vec::<Value>::new()));
    let captured_calls = Arc::clone(&upstream_calls);
    let mut proxy =
        configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport(move |request| {
            let body = serde_json::from_str::<Value>(&request.body).unwrap_or(Value::Null);
            captured_calls.lock().unwrap().push(body.clone());
            let requested_ids = body["variables"]["ids"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            if requested_ids
                .iter()
                .any(|id| id.as_str() == Some(location_id))
            {
                Response {
                    status: 200,
                    headers: Default::default(),
                    body: json!({
                        "data": {
                            "nodes": [{
                                "__typename": "Location",
                                "id": location_id,
                                "name": "Live Inventory Base",
                                "isActive": true,
                                "deactivatable": true,
                                "deletable": false,
                                "fulfillsOnlineOrders": false,
                                "hasActiveInventory": false,
                                "hasUnfulfilledOrders": false,
                                "isFulfillmentService": false,
                                "shipsInventory": true
                            }]
                        }
                    }),
                }
            } else if body["variables"]["id"] == location_id {
                Response {
                    status: 200,
                    headers: Default::default(),
                    body: json!({
                        "data": {
                            "location": {
                                "id": location_id,
                                "name": "Live Inventory Base",
                                "isActive": true,
                                "deactivatable": true,
                                "deletable": false,
                                "fulfillsOnlineOrders": false,
                                "hasActiveInventory": false,
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
    let inventory_item_id =
        create_inventory_item_for_location_test(&mut proxy, "Hydrated location inventory guard");

    let seed_inventory = proxy.process_request(json_graphql_request(
        r#"
        mutation SeedHydratedLocationInventory($input: InventorySetQuantitiesInput!) {
          inventorySetQuantities(input: $input) {
            inventoryAdjustmentGroup { changes { location { id } } }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "input": {
                "name": "available",
                "reason": "correction",
                "quantities": [{
                    "inventoryItemId": inventory_item_id,
                    "locationId": location_id,
                    "quantity": 7,
                    "changeFromQuantity": null
                }]
            }
        }),
    ));
    assert_eq!(
        seed_inventory.body["data"]["inventorySetQuantities"]["userErrors"],
        json!([])
    );

    let deactivate = proxy.process_request(json_graphql_request(
        r#"
        mutation LocationDeactivateHydratedInventory($locationId: ID!) {
          locationDeactivate(locationId: $locationId) @idempotent(key: "hydrated-inventory") {
            location { id isActive hasActiveInventory }
            locationDeactivateUserErrors { field message code }
          }
        }
        "#,
        json!({ "locationId": location_id }),
    ));
    assert_eq!(
        deactivate.body["data"]["locationDeactivate"],
        json!({
            "location": {
                "id": location_id,
                "isActive": true,
                "hasActiveInventory": true
            },
            "locationDeactivateUserErrors": [{
                "field": ["locationId"],
                "message": "Location could not be deactivated without specifying where to relocate inventory stocked at the location.",
                "code": "HAS_ACTIVE_INVENTORY_ERROR"
            }]
        })
    );

    let calls = upstream_calls.lock().unwrap();
    assert_eq!(calls.len(), 2, "unexpected upstream calls: {calls:#?}");
    assert_eq!(calls[0]["variables"], json!({ "ids": [location_id] }));
    assert!(calls[0]["query"]
        .as_str()
        .unwrap_or_default()
        .contains("InventoryLocationsHydrateNodes"));
    assert_eq!(calls[1]["variables"], json!({ "id": location_id }));
    assert!(calls[1]["query"]
        .as_str()
        .unwrap_or_default()
        .contains("StorePropertiesLocationHydrate"));
}

#[test]
fn location_deactivate_with_destination_relocates_and_merges_inventory_quantities() {
    let mut proxy = snapshot_proxy();
    let source_location_id = create_location_for_platform_test(&mut proxy, "Source location");
    let destination_location_id =
        create_location_for_platform_test(&mut proxy, "Destination location");
    let inventory_item_id =
        create_inventory_item_for_location_test(&mut proxy, "Relocation inventory item");
    let destination_level_id =
        inventory_level_id_for_platform_test(&inventory_item_id, &destination_location_id);
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
                "quantities": [
                    { "inventoryItemId": inventory_item_id, "locationId": source_location_id, "quantity": 5, "changeFromQuantity": null },
                    { "inventoryItemId": inventory_item_id, "locationId": destination_location_id, "quantity": 9, "changeFromQuantity": null }
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

    let inventory_item = &read.body["data"]["inventoryItem"];
    assert_eq!(
        inventory_item["locationsCount"],
        json!({ "count": 1, "precision": "EXACT" })
    );
    assert_eq!(inventory_item["inventoryLevel"], Value::Null);
    let relocated_level = &inventory_item["inventoryLevels"]["nodes"][0];
    assert_eq!(relocated_level["id"], json!(destination_level_id));
    assert_eq!(
        relocated_level["location"],
        json!({ "id": destination_location_id, "name": "Destination location" })
    );
    assert_eq!(
        relocated_level["quantities"],
        json!([
            { "name": "available", "quantity": 14 },
            { "name": "on_hand", "quantity": 14 }
        ])
    );
}

#[test]
fn location_deactivate_user_error_does_not_relocate_inventory_quantities() {
    let mut proxy = snapshot_proxy();
    let source_location_id = create_location_for_platform_test(&mut proxy, "Source location");
    let inactive_destination_location_id =
        create_location_for_platform_test(&mut proxy, "Inactive destination location");
    set_location_active_for_platform_test(&mut proxy, &inactive_destination_location_id, false);
    let inventory_item_id =
        create_inventory_item_for_location_test(&mut proxy, "Relocation guard inventory item");
    let source_level_id =
        inventory_level_id_for_platform_test(&inventory_item_id, &source_location_id);
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
                "quantities": [
                    { "inventoryItemId": inventory_item_id, "locationId": source_location_id, "quantity": 5, "changeFromQuantity": null },
                    { "inventoryItemId": inventory_item_id, "locationId": inactive_destination_location_id, "quantity": 9, "changeFromQuantity": null }
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

    let inventory_item = &read.body["data"]["inventoryItem"];
    assert_eq!(
        inventory_item["locationsCount"],
        json!({ "count": 2, "precision": "EXACT" })
    );
    assert_eq!(
        inventory_item["inventoryLevel"]["location"],
        json!({ "id": source_location_id, "name": "Source location" })
    );
    assert_eq!(
        inventory_item["inventoryLevel"]["id"],
        json!(source_level_id)
    );
    assert_eq!(
        inventory_item["inventoryLevel"]["quantities"],
        json!([
            { "name": "available", "quantity": 5 },
            { "name": "on_hand", "quantity": 5 }
        ])
    );
    assert_eq!(
        inventory_item["inventoryLevels"]["nodes"][0],
        json!({
            "location": { "id": source_location_id },
            "quantities": [
                { "name": "available", "quantity": 5 },
                { "name": "on_hand", "quantity": 5 }
            ]
        })
    );
    assert_eq!(
        inventory_item["inventoryLevels"]["nodes"][1],
        json!({
            "location": { "id": inactive_destination_location_id },
            "quantities": [
                { "name": "available", "quantity": 9 },
                { "name": "on_hand", "quantity": 9 }
            ]
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

    let same_id_location = add_platform_location(&mut proxy, "State machine source", false);
    let same_id = proxy.process_request(json_graphql_request(
        query,
        json!({
            "locationId": same_id_location,
            "destinationLocationId": same_id_location,
            "idempotencyKey": "same"
        }),
    ));
    assert_eq!(
        same_id.body["data"]["locationDeactivate"],
        json!({
            "location": {
                "id": same_id_location,
                "name": "State machine source",
                "isActive": true,
                "activatable": false,
                "deactivatable": true,
                "fulfillsOnlineOrders": false,
                "hasActiveInventory": false,
                "hasUnfulfilledOrders": false,
                "deletable": false,
                "shipsInventory": true
            },
            "locationDeactivateUserErrors": [{
                "field": ["destinationLocationId"],
                "message": "Location could not be deactivated because the destination location cannot be set to the location to be deactivated.",
                "code": "DESTINATION_LOCATION_IS_THE_SAME_LOCATION"
            }]
        })
    );

    let active_inventory_location =
        add_platform_location(&mut proxy, "State machine active inventory", false);
    let active_inventory_item =
        create_inventory_item_for_location_test(&mut proxy, "State machine active inventory item");
    let seed_inventory = proxy.process_request(json_graphql_request(
        r#"
        mutation StateMachineActiveInventory($input: InventorySetQuantitiesInput!) {
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
                "quantities": [{
                    "inventoryItemId": active_inventory_item,
                    "locationId": active_inventory_location,
                    "quantity": 5,
                    "changeFromQuantity": null
                }]
            }
        }),
    ));
    assert_eq!(
        seed_inventory.body["data"]["inventorySetQuantities"]["userErrors"],
        json!([])
    );
    let active_inventory = proxy.process_request(json_graphql_request(
        query,
        json!({
            "locationId": active_inventory_location,
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

    let only_online_location = add_platform_location(&mut proxy, "State machine only online", true);
    let only_online = proxy.process_request(json_graphql_request(
        query,
        json!({
            "locationId": only_online_location,
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

    let permanent_location_id = "gid://shopify/Location/permanent-block";
    let mut proxy =
        configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport(move |_request| {
            Response {
                status: 200,
                headers: Default::default(),
                body: json!({
                    "data": {
                        "location": {
                            "id": permanent_location_id,
                            "legacyResourceId": "permanent-block",
                            "name": "Hydrated permanent block",
                            "activatable": true,
                            "addressVerified": true,
                            "createdAt": "2026-07-01T00:00:00Z",
                            "deactivatable": false,
                            "deactivatedAt": null,
                            "deletable": false,
                            "fulfillsOnlineOrders": true,
                            "hasActiveInventory": true,
                            "hasUnfulfilledOrders": true,
                            "isActive": true,
                            "isFulfillmentService": false,
                            "isPrimary": true,
                            "shipsInventory": true,
                            "updatedAt": "2026-07-01T00:00:00Z",
                            "fulfillmentService": null,
                            "address": null,
                            "suggestedAddresses": [],
                            "metafield": null,
                            "metafields": { "nodes": [], "pageInfo": { "hasNextPage": false, "hasPreviousPage": false, "startCursor": null, "endCursor": null } },
                            "inventoryLevels": { "nodes": [], "pageInfo": { "hasNextPage": false, "hasPreviousPage": false, "startCursor": null, "endCursor": null } }
                        }
                    }
                }),
            }
        });
    let permanent = proxy.process_request(json_graphql_request(
        query,
        json!({
            "locationId": permanent_location_id,
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
fn location_deactivate_rejects_unknown_non_fixture_location_without_staging() {
    let mut proxy = snapshot_proxy();
    let unknown_location_id = "gid://shopify/Location/424242424244";
    let response = proxy.process_request(json_graphql_request(
        r#"
        mutation UnknownLocationDeactivate($locationId: ID!) {
          locationDeactivate(locationId: $locationId) @idempotent(key: "unknown-location") {
            location { id name isActive }
            locationDeactivateUserErrors { field message code }
          }
        }
        "#,
        json!({ "locationId": unknown_location_id }),
    ));

    assert_eq!(
        response.body["data"]["locationDeactivate"],
        json!({
            "location": null,
            "locationDeactivateUserErrors": [{
                "field": ["locationId"],
                "message": "Location not found.",
                "code": "LOCATION_NOT_FOUND"
            }]
        })
    );
    assert_eq!(log_snapshot(&proxy)["entries"], json!([]));

    let read = proxy.process_request(json_graphql_request(
        r#"
        query UnknownLocationRead($locationId: ID!) {
          location(id: $locationId) { id name isActive }
        }
        "#,
        json!({ "locationId": unknown_location_id }),
    ));
    assert_eq!(read.body["data"]["location"], Value::Null);
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
fn fulfillment_order_hydration_miss_does_not_forward_mutation() {
    let upstream_queries = Arc::new(Mutex::new(Vec::new()));
    let mut proxy = configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport({
        let upstream_queries = Arc::clone(&upstream_queries);
        move |request| {
            let body: Value = serde_json::from_str(&request.body).expect("upstream GraphQL body");
            let query = body["query"].as_str().unwrap_or_default().to_string();
            assert!(
                query.trim_start().starts_with("query"),
                "fulfillment order fallback must hydrate by query only, got upstream body: {}",
                request.body
            );
            upstream_queries
                .lock()
                .expect("upstream queries")
                .push(query.clone());
            let body = if query.contains("fulfillmentOrder(id: $id)") {
                json!({ "data": { "fulfillmentOrder": Value::Null } })
            } else {
                json!({ "data": { "node": Value::Null } })
            };
            Response {
                status: 200,
                headers: Default::default(),
                body,
            }
        }
    });

    let response = proxy.process_request(json_graphql_request(
        r#"
        mutation HoldMissingFulfillmentOrder($id: ID!, $fulfillmentHold: FulfillmentOrderHoldInput!) {
          fulfillmentOrderHold(id: $id, fulfillmentHold: $fulfillmentHold) {
            fulfillmentOrder { id }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "id": "gid://shopify/FulfillmentOrder/404404404",
            "fulfillmentHold": {
                "reason": "INVENTORY_OUT_OF_STOCK",
                "reasonNotes": "missing"
            }
        }),
    ));

    assert_eq!(response.status, 200);
    assert_eq!(response.body["data"]["fulfillmentOrderHold"], Value::Null);
    assert_eq!(
        response.body["errors"][0]["extensions"]["code"],
        json!("RESOURCE_NOT_FOUND")
    );
    let queries = upstream_queries.lock().expect("upstream queries");
    assert_eq!(queries.len(), 4);
    assert!(queries
        .iter()
        .all(|query| query.trim_start().starts_with("query")));
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
            fulfillmentHold { id handle reason reasonNotes displayReason heldByRequestingApp }
            fulfillmentOrder { id status fulfillmentHolds { id handle reason displayReason } lineItems(first: 5) { nodes { id totalQuantity remainingQuantity lineItem { fulfillableQuantity } } } }
            remainingFulfillmentOrder { id status lineItems(first: 5) { nodes { totalQuantity remainingQuantity } } }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "id": fulfillment_order_id,
            "fulfillmentHold": {
                "reason": "AWAITING_RETURN_ITEMS",
                "reasonNotes": "wait",
                "handle": "numeric-hold",
                "fulfillmentOrderLineItems": [{ "id": line_item_id, "quantity": 1 }]
            }
        }),
    ));
    assert_eq!(hold.status, 200);
    let hold_payload = &hold.body["data"]["fulfillmentOrderHold"];
    assert_eq!(hold_payload["userErrors"], json!([]));
    assert_eq!(
        hold_payload["fulfillmentHold"]["reason"],
        json!("AWAITING_RETURN_ITEMS")
    );
    assert_eq!(
        hold_payload["fulfillmentHold"]["displayReason"],
        json!("Exchange items awaiting return delivery")
    );
    assert_eq!(hold_payload["fulfillmentOrder"]["status"], json!("ON_HOLD"));
    assert_eq!(
        hold_payload["fulfillmentOrder"]["fulfillmentHolds"][0]["displayReason"],
        json!("Exchange items awaiting return delivery")
    );
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
          order(id: $orderId) { id fulfillmentOrders(first: 10) { nodes { id status fulfillmentHolds { id handle reason displayReason } } } }
          fulfillmentOrder(id: $fulfillmentOrderId) { id status fulfillmentHolds { reason displayReason } }
          manualHoldsFulfillmentOrders(first: 10) { nodes { id status fulfillmentHolds { reason displayReason } } }
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
    assert_eq!(
        after_hold.body["data"]["fulfillmentOrder"]["fulfillmentHolds"][0]["displayReason"],
        json!("Exchange items awaiting return delivery")
    );
    assert_eq!(
        after_hold.body["data"]["manualHoldsFulfillmentOrders"]["nodes"][0]["fulfillmentHolds"][0]
            ["displayReason"],
        json!("Exchange items awaiting return delivery")
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
    assert_eq!(
        log_snapshot(&proxy)["entries"][0]["variables"]["fulfillmentHold"]["reason"],
        json!("AWAITING_RETURN_ITEMS")
    );
}

#[test]
fn fulfillment_order_top_level_catalog_merges_live_siblings_after_local_hold() {
    let held_order_id = "gid://shopify/Order/7001101";
    let held_fulfillment_order_id = "gid://shopify/FulfillmentOrder/70011011";
    let live_order_id = "gid://shopify/Order/7001102";
    let live_fulfillment_order_id = "gid://shopify/FulfillmentOrder/70011021";
    let observed_queries = Arc::new(Mutex::new(Vec::new()));
    let mut proxy = snapshot_proxy().with_upstream_transport(fulfillment_order_catalog_transport(
        vec![
            fulfillment_order_order_fixture(
                held_order_id,
                "#70011",
                held_fulfillment_order_id,
                "gid://shopify/FulfillmentOrderLineItem/70011012",
                1,
                "OPEN",
            ),
            fulfillment_order_order_fixture(
                live_order_id,
                "#70012",
                live_fulfillment_order_id,
                "gid://shopify/FulfillmentOrderLineItem/70011022",
                1,
                "OPEN",
            ),
        ],
        observed_queries.clone(),
    ));

    let hold = proxy.process_request(json_graphql_request(
        r#"
        mutation HoldOneCatalogFulfillmentOrder($id: ID!, $fulfillmentHold: FulfillmentOrderHoldInput!) {
          fulfillmentOrderHold(id: $id, fulfillmentHold: $fulfillmentHold) {
            fulfillmentOrder { id status }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "id": held_fulfillment_order_id,
            "fulfillmentHold": {
                "reason": "INVENTORY_OUT_OF_STOCK",
                "handle": "catalog-hold"
            }
        }),
    ));
    assert_eq!(hold.status, 200);
    assert_eq!(
        hold.body["data"]["fulfillmentOrderHold"]["fulfillmentOrder"]["status"],
        json!("ON_HOLD")
    );

    let catalog = proxy.process_request(json_graphql_request(
        r#"
        query ReadFulfillmentOrderCatalogAfterHold {
          fulfillmentOrders(first: 10, includeClosed: true, sortKey: ID) {
            nodes { id status order { id name } }
          }
        }
        "#,
        json!({}),
    ));
    let nodes = catalog.body["data"]["fulfillmentOrders"]["nodes"]
        .as_array()
        .unwrap();
    let query_snapshot = observed_queries.lock().unwrap().clone();
    assert!(
        nodes.iter().any(
            |node| node["id"].as_str() == Some(held_fulfillment_order_id)
                && node["status"].as_str() == Some("ON_HOLD")
        ),
        "locally held fulfillment order should stay overlaid in the top-level catalog: {nodes:#?}"
    );
    assert!(
        nodes
            .iter()
            .any(|node| node["id"].as_str() == Some(live_fulfillment_order_id)),
        "unrelated upstream fulfillment order disappeared after staging a hold: nodes={nodes:#?}; upstream_queries={query_snapshot:#?}; body={:#?}",
        catalog.body
    );
    let queries = query_snapshot;
    assert!(queries
        .iter()
        .any(|query| query.contains("ShippingFulfillmentOrderHydrate")));
    assert!(queries
        .iter()
        .all(|query| query.trim_start().starts_with("query")));
}

#[test]
fn fulfillment_order_top_level_catalog_applies_filters_sort_and_cursor_windows() {
    let location_a = "gid://shopify/Location/70012044";
    let location_b = "gid://shopify/Location/70012055";
    let open_a = "gid://shopify/FulfillmentOrder/70012011";
    let closed_b = "gid://shopify/FulfillmentOrder/70012021";
    let open_c = "gid://shopify/FulfillmentOrder/70012031";
    let held_d = "gid://shopify/FulfillmentOrder/70012041";
    let observed_queries = Arc::new(Mutex::new(Vec::new()));
    let mut proxy = snapshot_proxy().with_upstream_transport(fulfillment_order_catalog_transport(
        vec![
            fulfillment_order_catalog_fixture(FulfillmentOrderCatalogFixture {
                order_id: "gid://shopify/Order/7001201",
                name: "#7001201",
                fulfillment_order_id: open_a,
                line_item_id: "gid://shopify/FulfillmentOrderLineItem/70012012",
                status: "OPEN",
                location_id: location_a,
                location_name: "North warehouse",
                updated_at: "2026-06-15T11:00:10Z",
            }),
            fulfillment_order_catalog_fixture(FulfillmentOrderCatalogFixture {
                order_id: "gid://shopify/Order/7001202",
                name: "#7001202",
                fulfillment_order_id: closed_b,
                line_item_id: "gid://shopify/FulfillmentOrderLineItem/70012022",
                status: "CLOSED",
                location_id: location_b,
                location_name: "South warehouse",
                updated_at: "2026-06-15T11:00:20Z",
            }),
            fulfillment_order_catalog_fixture(FulfillmentOrderCatalogFixture {
                order_id: "gid://shopify/Order/7001203",
                name: "#7001203",
                fulfillment_order_id: open_c,
                line_item_id: "gid://shopify/FulfillmentOrderLineItem/70012032",
                status: "OPEN",
                location_id: location_b,
                location_name: "South warehouse",
                updated_at: "2026-06-15T11:00:30Z",
            }),
            fulfillment_order_catalog_fixture(FulfillmentOrderCatalogFixture {
                order_id: "gid://shopify/Order/7001204",
                name: "#7001204",
                fulfillment_order_id: held_d,
                line_item_id: "gid://shopify/FulfillmentOrderLineItem/70012042",
                status: "ON_HOLD",
                location_id: location_a,
                location_name: "North warehouse",
                updated_at: "2026-06-15T11:00:40Z",
            }),
        ],
        observed_queries.clone(),
    ));

    let hydrate = proxy.process_request(json_graphql_request(
        r#"
        query HydrateOneFulfillmentOrder($id: ID!) {
          fulfillmentOrder(id: $id) { id status updatedAt }
        }
        "#,
        json!({ "id": open_a }),
    ));
    assert_eq!(
        hydrate.body["data"]["fulfillmentOrder"]["id"],
        json!(open_a)
    );

    let default_open = proxy.process_request(json_graphql_request(
        r#"
        query ReadOpenFulfillmentOrders {
          fulfillmentOrders(first: 10, sortKey: ID) {
            nodes { id status }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
        }
        "#,
        json!({}),
    ));
    let query_snapshot = observed_queries.lock().unwrap().clone();
    assert_eq!(
        fulfillment_order_ids(&default_open, "fulfillmentOrders"),
        vec![open_a, open_c, held_d],
        "default open catalog body={:#?}; upstream_queries={query_snapshot:#?}",
        default_open.body
    );

    let include_closed = proxy.process_request(json_graphql_request(
        r#"
        query ReadAllFulfillmentOrders {
          fulfillmentOrders(first: 10, includeClosed: true, sortKey: ID) {
            nodes { id status }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        fulfillment_order_ids(&include_closed, "fulfillmentOrders"),
        vec![open_a, closed_b, open_c, held_d]
    );

    let status_filtered = proxy.process_request(json_graphql_request(
        r#"
        query ReadStatusFilteredFulfillmentOrders($query: String!) {
          fulfillmentOrders(first: 10, includeClosed: true, query: $query, sortKey: ID) {
            nodes { id status }
          }
        }
        "#,
        json!({ "query": "status:OPEN" }),
    ));
    assert_eq!(
        fulfillment_order_ids(&status_filtered, "fulfillmentOrders"),
        vec![open_a, open_c]
    );

    let location_filtered = proxy.process_request(json_graphql_request(
        r#"
        query ReadLocationFilteredFulfillmentOrders($query: String!) {
          fulfillmentOrders(first: 10, includeClosed: true, query: $query, sortKey: ID) {
            nodes { id assignedLocation { location { id } } }
          }
        }
        "#,
        json!({ "query": "assigned_location_id:70012055" }),
    ));
    assert_eq!(
        fulfillment_order_ids(&location_filtered, "fulfillmentOrders"),
        vec![closed_b, open_c]
    );

    let updated_filtered = proxy.process_request(json_graphql_request(
        r#"
        query ReadUpdatedFilteredFulfillmentOrders($query: String!) {
          fulfillmentOrders(first: 10, includeClosed: true, query: $query, sortKey: UPDATED_AT) {
            nodes { id updatedAt }
          }
        }
        "#,
        json!({ "query": "updated_at:>=2026-06-15T11:00:20Z" }),
    ));
    assert_eq!(
        fulfillment_order_ids(&updated_filtered, "fulfillmentOrders"),
        vec![closed_b, open_c, held_d]
    );

    let reverse_updated = proxy.process_request(json_graphql_request(
        r#"
        query ReadReverseUpdatedFulfillmentOrders {
          fulfillmentOrders(first: 2, includeClosed: true, sortKey: UPDATED_AT, reverse: true) {
            nodes { id updatedAt }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        fulfillment_order_ids(&reverse_updated, "fulfillmentOrders"),
        vec![held_d, open_c]
    );
    assert_eq!(
        reverse_updated.body["data"]["fulfillmentOrders"]["pageInfo"]["hasNextPage"],
        json!(true)
    );

    let first_page = proxy.process_request(json_graphql_request(
        r#"
        query ReadFirstFulfillmentOrderWindow {
          fulfillmentOrders(first: 2, includeClosed: true, sortKey: ID) {
            nodes { id }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        fulfillment_order_ids(&first_page, "fulfillmentOrders"),
        vec![open_a, closed_b]
    );
    let after = first_page.body["data"]["fulfillmentOrders"]["pageInfo"]["endCursor"]
        .as_str()
        .unwrap()
        .to_string();
    let next_page = proxy.process_request(json_graphql_request(
        r#"
        query ReadNextFulfillmentOrderWindow($after: String!) {
          fulfillmentOrders(first: 2, after: $after, includeClosed: true, sortKey: ID) {
            nodes { id }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
        }
        "#,
        json!({ "after": after }),
    ));
    assert_eq!(
        fulfillment_order_ids(&next_page, "fulfillmentOrders"),
        vec![open_c, held_d]
    );
    assert_eq!(
        next_page.body["data"]["fulfillmentOrders"]["pageInfo"]["hasPreviousPage"],
        json!(true)
    );
    let before = next_page.body["data"]["fulfillmentOrders"]["pageInfo"]["startCursor"]
        .as_str()
        .unwrap()
        .to_string();
    let previous_page = proxy.process_request(json_graphql_request(
        r#"
        query ReadPreviousFulfillmentOrderWindow($before: String!) {
          fulfillmentOrders(last: 1, before: $before, includeClosed: true, sortKey: ID) {
            nodes { id }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
        }
        "#,
        json!({ "before": before }),
    ));
    assert_eq!(
        fulfillment_order_ids(&previous_page, "fulfillmentOrders"),
        vec![closed_b]
    );
    assert_eq!(
        previous_page.body["data"]["fulfillmentOrders"]["pageInfo"]["hasNextPage"],
        json!(true)
    );

    let assigned = proxy.process_request(json_graphql_request(
        r#"
        query ReadAssignedFulfillmentOrders($locationIds: [ID!]) {
          assignedFulfillmentOrders(first: 10, locationIds: $locationIds, sortKey: ID) {
            nodes { id status assignedLocation { location { id } } }
          }
        }
        "#,
        json!({ "locationIds": [location_b] }),
    ));
    assert_eq!(
        fulfillment_order_ids(&assigned, "assignedFulfillmentOrders"),
        vec![open_c]
    );

    let manual_holds = proxy.process_request(json_graphql_request(
        r#"
        query ReadManualHoldsFulfillmentOrders {
          manualHoldsFulfillmentOrders(first: 1, reverse: true) {
            nodes { id status }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        fulfillment_order_ids(&manual_holds, "manualHoldsFulfillmentOrders"),
        vec![held_d]
    );
    assert!(observed_queries
        .lock()
        .unwrap()
        .iter()
        .all(|query| query.trim_start().starts_with("query")));
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
            userErrors { field message  }
          }
        }
        "#,
        json!({ "id": fulfillment_order_id }),
    ));
    assert_eq!(
        open.body["data"]["fulfillmentOrderOpen"]["fulfillmentOrder"]["status"],
        json!("OPEN")
    );

    let unknown_destination = proxy.process_request(json_graphql_request(
        r#"
        mutation MoveNumericFulfillmentOrderUnknownLocation($id: ID!) {
          fulfillmentOrderMove(id: $id, newLocationId: "gid://shopify/Location/7002555") {
            movedFulfillmentOrder { id }
            originalFulfillmentOrder { id }
            remainingFulfillmentOrder { id }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "id": fulfillment_order_id }),
    ));
    assert_eq!(
        unknown_destination.body["data"]["fulfillmentOrderMove"],
        json!({
            "movedFulfillmentOrder": null,
            "originalFulfillmentOrder": null,
            "remainingFulfillmentOrder": null,
            "userErrors": [{
                "field": ["id"],
                "message": "Location not found.",
                "code": null
            }]
        })
    );

    let destination = proxy.process_request(json_graphql_request(
        r#"
        mutation SeedFulfillmentOrderMoveDestination($input: LocationAddInput!) {
          locationAdd(input: $input) {
            location { id name }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "input": {
                "name": "Numeric FO Move Destination",
                "address": { "countryCode": "CA" }
            }
        }),
    ));
    assert_eq!(
        destination.body["data"]["locationAdd"]["userErrors"],
        json!([])
    );
    let destination_location = destination.body["data"]["locationAdd"]["location"].clone();
    let destination_location_id = destination_location["id"].as_str().unwrap();

    let move_response = proxy.process_request(json_graphql_request(
        r#"
        mutation MoveNumericFulfillmentOrder($id: ID!, $newLocationId: ID!, $fulfillmentOrderLineItems: [FulfillmentOrderLineItemInput!]) {
          fulfillmentOrderMove(id: $id, newLocationId: $newLocationId, fulfillmentOrderLineItems: $fulfillmentOrderLineItems) {
            movedFulfillmentOrder { id status updatedAt assignedLocation { name location { id name } } lineItems(first: 5) { nodes { remainingQuantity } } }
            originalFulfillmentOrder { id lineItems(first: 5) { nodes { remainingQuantity } } }
            remainingFulfillmentOrder { id lineItems(first: 5) { nodes { remainingQuantity } } }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "id": fulfillment_order_id,
            "newLocationId": destination_location_id,
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
        destination_location["id"]
    );
    assert_eq!(
        move_response.body["data"]["fulfillmentOrderMove"]["movedFulfillmentOrder"]
            ["assignedLocation"]["location"]["name"],
        destination_location["name"]
    );
    assert_eq!(
        move_response.body["data"]["fulfillmentOrderMove"]["movedFulfillmentOrder"]
            ["assignedLocation"]["name"],
        destination_location["name"]
    );
    assert_ne!(
        move_response.body["data"]["fulfillmentOrderMove"]["movedFulfillmentOrder"]["updatedAt"],
        json!("2026-05-11T10:00:00Z")
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
            userErrors { field message  }
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
            userErrors { field message  }
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
            userErrors { field message  }
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
        Value::Null
    );
}

#[test]
fn fulfillment_order_move_uses_staged_destination_location_state() {
    let order_id = "gid://shopify/Order/7004001";
    let fulfillment_order_id = "gid://shopify/FulfillmentOrder/70040011";
    let line_item_id = "gid://shopify/FulfillmentOrderLineItem/70040012";
    let mut proxy =
        snapshot_proxy().with_upstream_transport(fulfillment_order_hydrate_transport(vec![
            fulfillment_order_order_fixture(
                order_id,
                "#7004",
                fulfillment_order_id,
                line_item_id,
                1,
                "OPEN",
            ),
        ]));
    let query = r#"
        fragment FulfillmentOrderMoveLocationFields on FulfillmentOrder {
          id
          status
          requestStatus
          updatedAt
          assignedLocation { name location { id name } }
          lineItems(first: 5) { nodes { id totalQuantity remainingQuantity lineItem { id title quantity fulfillableQuantity } } }
        }
        mutation FulfillmentOrderMoveWithStagedDestination($id: ID!, $newLocationId: ID!, $fulfillmentOrderLineItems: [FulfillmentOrderLineItemInput!]) {
          fulfillmentOrderMove(id: $id, newLocationId: $newLocationId, fulfillmentOrderLineItems: $fulfillmentOrderLineItems) {
            movedFulfillmentOrder { ...FulfillmentOrderMoveLocationFields }
            originalFulfillmentOrder { ...FulfillmentOrderMoveLocationFields }
            remainingFulfillmentOrder { ...FulfillmentOrderMoveLocationFields }
            userErrors { field message code }
          }
        }
    "#;

    let unstaged_destination = proxy.process_request(json_graphql_request(
        query,
        json!({
            "id": fulfillment_order_id,
            "newLocationId": "gid://shopify/Location/70040099",
            "fulfillmentOrderLineItems": null
        }),
    ));
    assert_eq!(
        unstaged_destination.body["data"]["fulfillmentOrderMove"],
        json!({
            "movedFulfillmentOrder": null,
            "originalFulfillmentOrder": null,
            "remainingFulfillmentOrder": null,
            "userErrors": [{
                "field": ["id"],
                "message": "Location not found.",
                "code": null
            }]
        })
    );

    let destination = proxy.process_request(json_graphql_request(
        r#"
        mutation SeedMoveDestination($input: LocationAddInput!) {
          locationAdd(input: $input) {
            location { id name }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "input": {
                "name": "Store-backed FO Destination",
                "address": { "countryCode": "US" }
            }
        }),
    ));
    assert_eq!(
        destination.body["data"]["locationAdd"]["userErrors"],
        json!([])
    );
    let destination_location = destination.body["data"]["locationAdd"]["location"].clone();

    let moved = proxy.process_request(json_graphql_request(
        query,
        json!({
            "id": fulfillment_order_id,
            "newLocationId": destination_location["id"],
            "fulfillmentOrderLineItems": null
        }),
    ));
    let payload = &moved.body["data"]["fulfillmentOrderMove"];
    assert_eq!(
        payload["movedFulfillmentOrder"]["assignedLocation"]["location"]["id"],
        destination_location["id"]
    );
    assert_eq!(
        payload["movedFulfillmentOrder"]["assignedLocation"]["location"]["name"],
        destination_location["name"]
    );
    assert_eq!(
        payload["movedFulfillmentOrder"]["assignedLocation"]["name"],
        destination_location["name"]
    );
    assert_eq!(
        payload["originalFulfillmentOrder"]["assignedLocation"]["location"]["id"],
        destination_location["id"]
    );
    assert_eq!(payload["remainingFulfillmentOrder"], json!(null));
    assert_eq!(payload["userErrors"], json!([]));

    let read = proxy.process_request(json_graphql_request(
        r#"
        query ReadMovedFulfillmentOrderLocation($orderId: ID!) {
          order(id: $orderId) {
            fulfillmentOrders(first: 5) {
              nodes { id assignedLocation { name location { id name } } }
            }
          }
        }
        "#,
        json!({ "orderId": order_id }),
    ));
    assert_eq!(
        read.body["data"]["order"]["fulfillmentOrders"]["nodes"][0]["assignedLocation"]["location"]
            ["id"],
        destination_location["id"]
    );
    assert_eq!(
        read.body["data"]["order"]["fulfillmentOrders"]["nodes"][0]["assignedLocation"]["name"],
        destination_location["name"]
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
            userErrors { field message  }
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
                "message": "Expected fulfillment order status to be valid but it was open."
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
fn fulfillment_order_status_invalid_state_rejections_do_not_mutate_order_reads() {
    for (status, status_message, supported_actions) in [
        ("CLOSED", "closed", json!([])),
        ("CANCELLED", "cancelled", json!([])),
        (
            "ON_HOLD",
            "on_hold",
            json!([
                { "action": "RELEASE_HOLD" },
                { "action": "HOLD" },
                { "action": "MOVE" }
            ]),
        ),
    ] {
        let order_id = format!("gid://shopify/Order/open-invalid-{status_message}");
        let fulfillment_order_id =
            format!("gid://shopify/FulfillmentOrder/open-invalid-{status_message}");
        let mut proxy =
            snapshot_proxy().with_upstream_transport(fulfillment_order_hydrate_transport(vec![
                fulfillment_order_order_fixture(
                    &order_id,
                    "#OPEN-INVALID",
                    &fulfillment_order_id,
                    &format!(
                        "gid://shopify/FulfillmentOrderLineItem/open-invalid-{status_message}"
                    ),
                    1,
                    status,
                ),
            ]));

        let open = proxy.process_request(json_graphql_request(
            r#"
            mutation FulfillmentOrderInvalidStateOpen($id: ID!) {
              fulfillmentOrderOpen(id: $id) {
                fulfillmentOrder { id status updatedAt supportedActions { action } }
                userErrors { field message  }
              }
            }
            "#,
            json!({ "id": fulfillment_order_id }),
        ));
        assert_eq!(
            open.body["data"]["fulfillmentOrderOpen"],
            json!({
                "fulfillmentOrder": null,
                "userErrors": [{
                    "field": null,
                    "message": format!("Expected fulfillment order status to be valid but it was {status_message}.")
                }]
            })
        );

        let after_open = proxy.process_request(json_graphql_request(
            r#"
            query FulfillmentOrderInvalidStateOrderRead($orderId: ID!) {
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
            after_open.body["data"]["order"],
            json!({
                "id": order_id,
                "fulfillmentOrders": { "nodes": [{
                    "id": fulfillment_order_id,
                    "status": status,
                    "updatedAt": "2026-06-15T11:00:00Z",
                    "supportedActions": supported_actions
                }] }
            })
        );
        assert_eq!(log_snapshot(&proxy)["entries"], json!([]));
    }

    let order_id = "gid://shopify/Order/progress-invalid-scheduled";
    let fulfillment_order_id = "gid://shopify/FulfillmentOrder/progress-invalid-scheduled";
    let mut proxy =
        snapshot_proxy().with_upstream_transport(fulfillment_order_hydrate_transport(vec![
            fulfillment_order_order_fixture(
                order_id,
                "#PROGRESS-INVALID",
                fulfillment_order_id,
                "gid://shopify/FulfillmentOrderLineItem/progress-invalid-scheduled",
                1,
                "SCHEDULED",
            ),
        ]));

    let progress = proxy.process_request(json_graphql_request(
        r#"
        mutation FulfillmentOrderInvalidStateReportProgress($id: ID!, $progressReport: FulfillmentOrderReportProgressInput) {
          fulfillmentOrderReportProgress(id: $id, progressReport: $progressReport) {
            fulfillmentOrder { id status updatedAt supportedActions { action } }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "id": fulfillment_order_id,
            "progressReport": { "reasonNotes": "local-runtime progress invalid state" }
        }),
    ));
    assert_eq!(
        progress.body["data"]["fulfillmentOrderReportProgress"],
        json!({
            "fulfillmentOrder": null,
            "userErrors": [{
                "field": null,
                "message": "Cannot report progress on a fulfillment order in this state.",
                "code": "FULFILLMENT_ORDER_STATUS_INVALID"
            }]
        })
    );

    let after_progress = proxy.process_request(json_graphql_request(
        r#"
        query FulfillmentOrderInvalidStateOrderRead($orderId: ID!) {
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
        after_progress.body["data"]["order"],
        json!({
            "id": order_id,
            "fulfillmentOrders": { "nodes": [{
                "id": fulfillment_order_id,
                "status": "SCHEDULED",
                "updatedAt": "2026-06-15T11:00:00Z",
                "supportedActions": [{ "action": "MARK_AS_OPEN" }]
            }] }
        })
    );
    assert_eq!(log_snapshot(&proxy)["entries"], json!([]));
}

#[test]
fn fulfillment_order_deadline_stages_existing_orders_and_reports_all_missing_ids() {
    let order_id = "gid://shopify/Order/7005001";
    let open_a_id = "gid://shopify/FulfillmentOrder/70050011";
    let open_b_id = "gid://shopify/FulfillmentOrder/70050012";
    let closed_id = "gid://shopify/FulfillmentOrder/70050013";
    let cancelled_id = "gid://shopify/FulfillmentOrder/70050014";
    let unknown_id = "gid://shopify/FulfillmentOrder/70059998";
    let mut order = fulfillment_order_order_fixture(
        order_id,
        "#7005",
        open_a_id,
        "gid://shopify/FulfillmentOrderLineItem/70050021",
        1,
        "OPEN",
    );
    for (fulfillment_order_id, line_item_id, status) in [
        (
            open_b_id,
            "gid://shopify/FulfillmentOrderLineItem/70050022",
            "OPEN",
        ),
        (
            closed_id,
            "gid://shopify/FulfillmentOrderLineItem/70050023",
            "CLOSED",
        ),
        (
            cancelled_id,
            "gid://shopify/FulfillmentOrderLineItem/70050024",
            "CANCELLED",
        ),
    ] {
        let sibling = fulfillment_order_order_fixture(
            order_id,
            "#7005",
            fulfillment_order_id,
            line_item_id,
            1,
            status,
        );
        order["fulfillmentOrders"]["nodes"]
            .as_array_mut()
            .unwrap()
            .push(sibling["fulfillmentOrders"]["nodes"][0].clone());
    }
    let mut proxy =
        snapshot_proxy().with_upstream_transport(fulfillment_order_hydrate_transport(vec![order]));
    let read_query = r#"
        query FulfillmentOrdersSetDeadlineAtomicOrderRead($id: ID!) {
          order(id: $id) {
            id name displayFulfillmentStatus
            fulfillmentOrders(first: 10) { nodes { id status fulfillBy } }
          }
        }
    "#;
    let mutation = r#"
        mutation FulfillmentOrdersSetDeadlineAtomic($fulfillmentOrderIds: [ID!]!, $fulfillmentDeadline: DateTime!) {
          fulfillmentOrdersSetFulfillmentDeadline(fulfillmentOrderIds: $fulfillmentOrderIds, fulfillmentDeadline: $fulfillmentDeadline) {
            success
            userErrors { field message code }
          }
        }
    "#;

    let unknown = proxy.process_request(json_graphql_request(
        mutation,
        json!({
            "fulfillmentOrderIds": [unknown_id],
            "fulfillmentDeadline": "2026-12-01T00:00:00Z"
        }),
    ));
    assert_eq!(
        unknown.body["data"]["fulfillmentOrdersSetFulfillmentDeadline"],
        json!({
            "success": false,
            "userErrors": [{
                "field": null,
                "message": "Fulfillment orders could not be found.",
                "code": null
            }]
        })
    );

    let mixed = proxy.process_request(json_graphql_request(
        mutation,
        json!({
            "fulfillmentOrderIds": [open_a_id, unknown_id],
            "fulfillmentDeadline": "2026-12-01T00:00:00Z"
        }),
    ));
    assert_eq!(
        mixed.body["data"]["fulfillmentOrdersSetFulfillmentDeadline"],
        json!({ "success": true, "userErrors": [] })
    );

    let after_mixed =
        proxy.process_request(json_graphql_request(read_query, json!({ "id": order_id })));
    assert_eq!(
        after_mixed.body["data"]["order"]["fulfillmentOrders"]["nodes"][0]["fulfillBy"],
        json!("2026-12-01T00:00:00Z")
    );

    for id in [closed_id, cancelled_id] {
        let set_deadline = proxy.process_request(json_graphql_request(
            mutation,
            json!({
                "fulfillmentOrderIds": [id],
                "fulfillmentDeadline": "2026-12-01T00:00:00Z"
            }),
        ));
        assert_eq!(
            set_deadline.body["data"]["fulfillmentOrdersSetFulfillmentDeadline"],
            json!({ "success": true, "userErrors": [] })
        );
    }

    let happy = proxy.process_request(json_graphql_request(
        mutation,
        json!({
            "fulfillmentOrderIds": [open_a_id, open_b_id],
            "fulfillmentDeadline": "2026-12-01T00:00:00Z"
        }),
    ));
    assert_eq!(
        happy.body["data"]["fulfillmentOrdersSetFulfillmentDeadline"],
        json!({ "success": true, "userErrors": [] })
    );

    let after_happy =
        proxy.process_request(json_graphql_request(read_query, json!({ "id": order_id })));
    assert_eq!(after_happy.body["data"]["order"]["id"], json!(order_id));
    assert_eq!(after_happy.body["data"]["order"]["name"], json!("#7005"));
    let nodes = after_happy.body["data"]["order"]["fulfillmentOrders"]["nodes"]
        .as_array()
        .unwrap();
    assert_eq!(nodes.len(), 4);
    for (id, status, fulfill_by) in [
        (open_a_id, "OPEN", json!("2026-12-01T00:00:00Z")),
        (open_b_id, "OPEN", json!("2026-12-01T00:00:00Z")),
        (closed_id, "CLOSED", json!("2026-12-01T00:00:00Z")),
        (cancelled_id, "CANCELLED", json!("2026-12-01T00:00:00Z")),
    ] {
        let node = nodes
            .iter()
            .find(|node| node["id"].as_str() == Some(id))
            .unwrap();
        assert_eq!(
            node,
            &json!({ "id": id, "status": status, "fulfillBy": fulfill_by })
        );
    }
    assert_eq!(
        after_happy.body["data"]["order"]["displayFulfillmentStatus"],
        json!("UNFULFILLED")
    );
}

#[test]
fn fulfillment_order_prepared_for_pickup_stages_selected_orders_only() {
    let order_id = "gid://shopify/Order/7005201";
    let prepared_id = "gid://shopify/FulfillmentOrder/70052011";
    let untouched_id = "gid://shopify/FulfillmentOrder/70052012";
    let mut order = fulfillment_order_order_fixture(
        order_id,
        "#70052",
        prepared_id,
        "gid://shopify/FulfillmentOrderLineItem/70052021",
        2,
        "OPEN",
    );
    let sibling = fulfillment_order_order_fixture(
        order_id,
        "#70052",
        untouched_id,
        "gid://shopify/FulfillmentOrderLineItem/70052022",
        1,
        "OPEN",
    );
    {
        let nodes = order["fulfillmentOrders"]["nodes"].as_array_mut().unwrap();
        nodes[0]["deliveryMethod"] = json!({ "methodType": "PICK_UP" });
        let mut untouched = sibling["fulfillmentOrders"]["nodes"][0].clone();
        untouched["deliveryMethod"] = json!({ "methodType": "PICK_UP" });
        nodes.push(untouched);
    }
    let mut proxy =
        snapshot_proxy().with_upstream_transport(fulfillment_order_hydrate_transport(vec![order]));

    let prehydrate = proxy.process_request(json_graphql_request(
        r#"
        query HydratePickupFulfillmentOrders($preparedId: ID!, $untouchedId: ID!) {
          prepared: fulfillmentOrder(id: $preparedId) { id status deliveryMethod { methodType } }
          untouched: fulfillmentOrder(id: $untouchedId) { id status deliveryMethod { methodType } }
        }
        "#,
        json!({ "preparedId": prepared_id, "untouchedId": untouched_id }),
    ));
    assert_eq!(
        prehydrate.body["data"]["prepared"]["deliveryMethod"]["methodType"],
        json!("PICK_UP")
    );
    assert_eq!(
        prehydrate.body["data"]["untouched"]["deliveryMethod"]["methodType"],
        json!("PICK_UP")
    );

    let prepared = proxy.process_request(json_graphql_request(
        r#"
        mutation PreparedForPickupRuntime($input: FulfillmentOrderLineItemsPreparedForPickupInput!) {
          fulfillmentOrderLineItemsPreparedForPickup(input: $input) {
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "input": {
                "lineItemsByFulfillmentOrder": [
                    { "fulfillmentOrderId": prepared_id }
                ]
            }
        }),
    ));
    assert_eq!(prepared.status, 200);
    assert_eq!(
        prepared.body["data"]["fulfillmentOrderLineItemsPreparedForPickup"]["userErrors"],
        json!([])
    );

    let read = proxy.process_request(json_graphql_request(
        r#"
        query ReadPreparedForPickupOrder($orderId: ID!) {
          order(id: $orderId) {
            displayFulfillmentStatus
            fulfillmentOrders(first: 10) {
              nodes {
                id
                status
                supportedActions { action }
                lineItems(first: 5) {
                  nodes {
                    id
                    remainingQuantity
                    lineItem { fulfillableQuantity }
                  }
                }
              }
            }
          }
        }
        "#,
        json!({ "orderId": order_id }),
    ));
    let nodes = read.body["data"]["order"]["fulfillmentOrders"]["nodes"]
        .as_array()
        .unwrap();
    let prepared_node = nodes
        .iter()
        .find(|node| node["id"].as_str() == Some(prepared_id))
        .unwrap();
    let untouched_node = nodes
        .iter()
        .find(|node| node["id"].as_str() == Some(untouched_id))
        .unwrap();
    assert_eq!(prepared_node["status"], json!("IN_PROGRESS"));
    assert!(prepared_node["supportedActions"]
        .as_array()
        .unwrap()
        .iter()
        .any(|action| action["action"] == json!("MARK_AS_OPEN")));
    assert_eq!(
        prepared_node["lineItems"]["nodes"][0]["remainingQuantity"],
        json!(2)
    );
    assert_eq!(
        prepared_node["lineItems"]["nodes"][0]["lineItem"]["fulfillableQuantity"],
        json!(0)
    );
    assert_eq!(untouched_node["status"], json!("OPEN"));
    assert_eq!(
        untouched_node["lineItems"]["nodes"][0]["lineItem"]["fulfillableQuantity"],
        json!(1)
    );
    assert_eq!(
        read.body["data"]["order"]["displayFulfillmentStatus"],
        json!("IN_PROGRESS")
    );

    let entries = log_snapshot(&proxy)["entries"].as_array().unwrap().clone();
    assert_eq!(entries.len(), 1);
    assert_eq!(
        entries[0]["interpreted"]["primaryRootField"],
        json!("fulfillmentOrderLineItemsPreparedForPickup")
    );
    assert!(entries[0]["rawBody"]
        .as_str()
        .unwrap()
        .contains("PreparedForPickupRuntime"));
    assert_eq!(entries[0]["stagedResourceIds"], json!([prepared_id]));
}

#[test]
fn fulfillment_order_prepared_for_pickup_invalid_batches_are_atomic() {
    let order_id = "gid://shopify/Order/7005301";
    let valid_id = "gid://shopify/FulfillmentOrder/70053011";
    let unknown_id = "gid://shopify/FulfillmentOrder/70053999";
    let mut order = fulfillment_order_order_fixture(
        order_id,
        "#70053",
        valid_id,
        "gid://shopify/FulfillmentOrderLineItem/70053021",
        1,
        "OPEN",
    );
    order["fulfillmentOrders"]["nodes"][0]["deliveryMethod"] = json!({ "methodType": "PICK_UP" });
    let mut proxy =
        snapshot_proxy().with_upstream_transport(fulfillment_order_hydrate_transport(vec![order]));

    let prehydrate = proxy.process_request(json_graphql_request(
        r#"
        query HydrateValidPickupFulfillmentOrder($id: ID!) {
          fulfillmentOrder(id: $id) { id status deliveryMethod { methodType } }
        }
        "#,
        json!({ "id": valid_id }),
    ));
    assert_eq!(
        prehydrate.body["data"]["fulfillmentOrder"]["deliveryMethod"]["methodType"],
        json!("PICK_UP")
    );

    let mixed = proxy.process_request(json_graphql_request(
        r#"
        mutation PreparedForPickupMixedInvalid($input: FulfillmentOrderLineItemsPreparedForPickupInput!) {
          fulfillmentOrderLineItemsPreparedForPickup(input: $input) {
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "input": {
                "lineItemsByFulfillmentOrder": [
                    { "fulfillmentOrderId": valid_id },
                    { "fulfillmentOrderId": unknown_id }
                ]
            }
        }),
    ));
    assert_eq!(
        mixed.body["data"]["fulfillmentOrderLineItemsPreparedForPickup"]["userErrors"],
        json!([{
            "field": ["input", "lineItemsByFulfillmentOrder", "1", "fulfillmentOrderId"],
            "message": "Invalid fulfillment_order_id provided 70053999",
            "code": "FULFILLMENT_ORDER_INVALID"
        }])
    );
    let after_mixed = proxy.process_request(json_graphql_request(
        r#"
        query ReadValidPickupAfterMixedInvalid($id: ID!) {
          fulfillmentOrder(id: $id) {
            id
            status
            lineItems(first: 5) { nodes { lineItem { fulfillableQuantity } } }
          }
        }
        "#,
        json!({ "id": valid_id }),
    ));
    assert_eq!(
        after_mixed.body["data"]["fulfillmentOrder"]["status"],
        json!("OPEN")
    );
    assert_eq!(
        after_mixed.body["data"]["fulfillmentOrder"]["lineItems"]["nodes"][0]["lineItem"]
            ["fulfillableQuantity"],
        json!(1)
    );
    assert_eq!(log_snapshot(&proxy)["entries"], json!([]));
}

#[test]
fn fulfillment_order_prepared_for_pickup_rejects_wrong_kind_and_ineligible_orders() {
    let mut wrong_kind_proxy = snapshot_proxy();
    let wrong_kind = wrong_kind_proxy.process_request(json_graphql_request(
        r#"
        mutation PreparedForPickupWrongKind($input: FulfillmentOrderLineItemsPreparedForPickupInput!) {
          fulfillmentOrderLineItemsPreparedForPickup(input: $input) {
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "input": {
                "lineItemsByFulfillmentOrder": [
                    { "fulfillmentOrderId": "gid://shopify/Product/70054011" }
                ]
            }
        }),
    ));
    assert_eq!(wrong_kind.body["errors"][0]["message"], json!("invalid id"));
    assert_eq!(
        wrong_kind.body["errors"][0]["extensions"]["code"],
        json!("RESOURCE_NOT_FOUND")
    );
    assert_eq!(
        wrong_kind.body["data"]["fulfillmentOrderLineItemsPreparedForPickup"],
        Value::Null
    );
    assert_eq!(log_snapshot(&wrong_kind_proxy)["entries"], json!([]));

    for (case, status, quantity, method_type) in [
        ("shipping", "OPEN", 1, "SHIPPING"),
        ("closed", "CLOSED", 1, "PICK_UP"),
        ("cancelled", "CANCELLED", 1, "PICK_UP"),
        ("fulfilled", "OPEN", 0, "PICK_UP"),
    ] {
        let order_id = format!("gid://shopify/Order/70054-{case}");
        let fulfillment_order_id = format!("gid://shopify/FulfillmentOrder/70054{}", case.len());
        let line_item_id = format!("gid://shopify/FulfillmentOrderLineItem/70054{}", quantity);
        let mut order = fulfillment_order_order_fixture(
            &order_id,
            "#70054",
            &fulfillment_order_id,
            &line_item_id,
            quantity,
            status,
        );
        order["fulfillmentOrders"]["nodes"][0]["deliveryMethod"] =
            json!({ "methodType": method_type });
        let mut proxy = snapshot_proxy()
            .with_upstream_transport(fulfillment_order_hydrate_transport(vec![order]));

        let response = proxy.process_request(json_graphql_request(
            r#"
            mutation PreparedForPickupIneligible($input: FulfillmentOrderLineItemsPreparedForPickupInput!) {
              fulfillmentOrderLineItemsPreparedForPickup(input: $input) {
                userErrors { field message code }
              }
            }
            "#,
            json!({
                "input": {
                    "lineItemsByFulfillmentOrder": [
                        { "fulfillmentOrderId": fulfillment_order_id }
                    ]
                }
            }),
        ));

        assert_eq!(
            response.body["data"]["fulfillmentOrderLineItemsPreparedForPickup"]["userErrors"],
            json!([{
                "field": ["input", "lineItemsByFulfillmentOrder", "0", "fulfillmentOrderId"],
                "message": format!(
                    "Invalid fulfillment_order_id provided {}",
                    resource_id_tail_for_test(&fulfillment_order_id)
                ),
                "code": "FULFILLMENT_ORDER_INVALID"
            }]),
            "{case}"
        );
        assert_eq!(log_snapshot(&proxy)["entries"], json!([]), "{case}");
    }
}

#[test]
fn fulfillment_order_deadline_batches_cold_hydration_without_line_items() {
    let order_id = "gid://shopify/Order/7005101";
    let open_a_id = "gid://shopify/FulfillmentOrder/70051011";
    let open_b_id = "gid://shopify/FulfillmentOrder/70051012";
    let unknown_id = "gid://shopify/FulfillmentOrder/70051999";
    let mut order = fulfillment_order_order_fixture(
        order_id,
        "#70051",
        open_a_id,
        "gid://shopify/FulfillmentOrderLineItem/70051021",
        1,
        "OPEN",
    );
    let sibling = fulfillment_order_order_fixture(
        order_id,
        "#70051",
        open_b_id,
        "gid://shopify/FulfillmentOrderLineItem/70051022",
        1,
        "OPEN",
    );
    order["fulfillmentOrders"]["nodes"]
        .as_array_mut()
        .unwrap()
        .push(sibling["fulfillmentOrders"]["nodes"][0].clone());

    let upstream_requests = Arc::new(Mutex::new(Vec::<Value>::new()));
    let mut proxy = snapshot_proxy().with_upstream_transport({
        let upstream_requests = Arc::clone(&upstream_requests);
        move |request| {
            let body: Value = serde_json::from_str(&request.body).expect("upstream GraphQL body");
            upstream_requests.lock().unwrap().push(body.clone());
            let query = body["query"].as_str().unwrap_or_default();
            if query.contains("nodes(ids: $ids)") {
                let nodes = body["variables"]["ids"]
                    .as_array()
                    .expect("batch hydrate ids")
                    .iter()
                    .map(|id| {
                        let requested_id = id.as_str().unwrap_or_default();
                        order["fulfillmentOrders"]["nodes"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .find(|node| node["id"].as_str() == Some(requested_id))
                            .map(|node| {
                                let mut node = node.clone();
                                node["__typename"] = json!("FulfillmentOrder");
                                node["order"] = json!({
                                    "id": order["id"],
                                    "name": order["name"],
                                    "displayFulfillmentStatus": order["displayFulfillmentStatus"]
                                });
                                node
                            })
                            .unwrap_or(Value::Null)
                    })
                    .collect::<Vec<_>>();
                return Response {
                    status: 200,
                    headers: Default::default(),
                    body: json!({ "data": { "nodes": nodes } }),
                };
            }

            let requested_id = body["variables"]["id"].as_str().unwrap_or_default();
            let node = order["fulfillmentOrders"]["nodes"]
                .as_array()
                .into_iter()
                .flatten()
                .find(|node| node["id"].as_str() == Some(requested_id))
                .cloned()
                .unwrap_or(Value::Null);
            Response {
                status: 200,
                headers: Default::default(),
                body: if query.contains("node(id: $id)") {
                    json!({ "data": { "node": node } })
                } else {
                    json!({ "data": { "fulfillmentOrder": node } })
                },
            }
        }
    });

    let deadline = proxy.process_request(json_graphql_request(
        r#"
        mutation BatchDeadlineHydration($fulfillmentOrderIds: [ID!]!, $fulfillmentDeadline: DateTime!) {
          fulfillmentOrdersSetFulfillmentDeadline(fulfillmentOrderIds: $fulfillmentOrderIds, fulfillmentDeadline: $fulfillmentDeadline) {
            success
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "fulfillmentOrderIds": [open_a_id, open_b_id, unknown_id, open_a_id, unknown_id],
            "fulfillmentDeadline": "2026-12-01T00:00:00Z"
        }),
    ));

    assert_eq!(
        deadline.body["data"]["fulfillmentOrdersSetFulfillmentDeadline"],
        json!({ "success": true, "userErrors": [] })
    );
    let requests = upstream_requests.lock().unwrap();
    assert_eq!(requests.len(), 1);
    let hydrate_query = requests[0]["query"].as_str().unwrap_or_default();
    assert!(hydrate_query.contains("nodes(ids: $ids)"));
    assert!(!hydrate_query.contains("lineItems("));
    let hydrated_ids = requests[0]["variables"]["ids"].as_array().unwrap();
    assert_eq!(hydrated_ids.len(), 3);
    assert!(hydrated_ids.iter().any(|id| id.as_str() == Some(open_a_id)));
    assert!(hydrated_ids.iter().any(|id| id.as_str() == Some(open_b_id)));
    assert!(hydrated_ids
        .iter()
        .any(|id| id.as_str() == Some(unknown_id)));
}

#[test]
fn fulfillment_order_split_deadline_uses_split_response_ids_locally() {
    let order_id = "gid://shopify/Order/7006001";
    let fulfillment_order_id = "gid://shopify/FulfillmentOrder/70060011";
    let line_item_id = "gid://shopify/FulfillmentOrderLineItem/70060021";
    let mut proxy = configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport(
        fulfillment_order_hydrate_transport(vec![fulfillment_order_order_fixture(
            order_id,
            "#7006",
            fulfillment_order_id,
            line_item_id,
            3,
            "OPEN",
        )]),
    );

    let split = proxy.process_request(json_graphql_request(
        r#"
        mutation FulfillmentOrderSplitThenDeadline($fulfillmentOrderSplits: [FulfillmentOrderSplitInput!]!) {
          fulfillmentOrderSplit(fulfillmentOrderSplits: $fulfillmentOrderSplits) {
            fulfillmentOrderSplits {
              fulfillmentOrder {
                id
                fulfillBy
                supportedActions { action }
                lineItems(first: 5) { nodes { remainingQuantity } }
              }
              remainingFulfillmentOrder {
                id
                fulfillBy
                supportedActions { action }
                lineItems(first: 5) { nodes { remainingQuantity } }
              }
            }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "fulfillmentOrderSplits": [{
                "fulfillmentOrderId": fulfillment_order_id,
                "fulfillmentOrderLineItems": [{ "id": line_item_id, "quantity": 1 }]
            }]
        }),
    ));
    assert_eq!(
        split.body["data"]["fulfillmentOrderSplit"]["userErrors"],
        json!([])
    );
    let original_id = split.body["data"]["fulfillmentOrderSplit"]["fulfillmentOrderSplits"][0]
        ["fulfillmentOrder"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let remaining_id = split.body["data"]["fulfillmentOrderSplit"]["fulfillmentOrderSplits"][0]
        ["remainingFulfillmentOrder"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(
        split.body["data"]["fulfillmentOrderSplit"]["fulfillmentOrderSplits"][0]
            ["fulfillmentOrder"]["supportedActions"],
        json!([
            { "action": "CREATE_FULFILLMENT" },
            { "action": "REPORT_PROGRESS" },
            { "action": "MOVE" },
            { "action": "HOLD" },
            { "action": "SPLIT" },
            { "action": "MERGE" }
        ])
    );
    assert_eq!(
        split.body["data"]["fulfillmentOrderSplit"]["fulfillmentOrderSplits"][0]
            ["remainingFulfillmentOrder"]["supportedActions"],
        json!([
            { "action": "CREATE_FULFILLMENT" },
            { "action": "REPORT_PROGRESS" },
            { "action": "MOVE" },
            { "action": "HOLD" },
            { "action": "MERGE" }
        ])
    );

    let split_dump = proxy.process_request(request_with_body("POST", "/__meta/dump", ""));
    assert_eq!(split_dump.status, 200);
    let restore_after_split = proxy.process_request(request_with_body(
        "POST",
        "/__meta/restore",
        &split_dump.body.to_string(),
    ));
    assert_eq!(restore_after_split.status, 200);

    let after_split_read = proxy.process_request(json_graphql_request(
        r#"
        query ReadSplitFulfillmentOrders($id: ID!) {
          order(id: $id) {
            fulfillmentOrders(first: 10) {
              nodes { id fulfillBy }
            }
          }
        }
        "#,
        json!({ "id": order_id }),
    ));
    assert_eq!(after_split_read.status, 200);

    let after_split_read_dump =
        proxy.process_request(request_with_body("POST", "/__meta/dump", ""));
    assert_eq!(after_split_read_dump.status, 200);
    let restore_after_split_read = proxy.process_request(request_with_body(
        "POST",
        "/__meta/restore",
        &after_split_read_dump.body.to_string(),
    ));
    assert_eq!(restore_after_split_read.status, 200);

    let deadline = proxy.process_request(json_graphql_request(
        r#"
        mutation DeadlineSplitFulfillmentOrders($fulfillmentOrderIds: [ID!]!, $fulfillmentDeadline: DateTime!) {
          fulfillmentOrdersSetFulfillmentDeadline(fulfillmentOrderIds: $fulfillmentOrderIds, fulfillmentDeadline: $fulfillmentDeadline) {
            success
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "fulfillmentOrderIds": [original_id, remaining_id],
            "fulfillmentDeadline": "2026-12-01T00:00:00.000Z"
        }),
    ));
    assert_eq!(
        deadline.body["data"]["fulfillmentOrdersSetFulfillmentDeadline"],
        json!({ "success": true, "userErrors": [] })
    );

    let read = proxy.process_request(json_graphql_request(
        r#"
        query ReadSplitDeadlineFulfillmentOrders($id: ID!) {
          order(id: $id) {
            fulfillmentOrders(first: 10) {
              nodes { id fulfillBy }
            }
          }
        }
        "#,
        json!({ "id": order_id }),
    ));
    let nodes = read.body["data"]["order"]["fulfillmentOrders"]["nodes"]
        .as_array()
        .unwrap();
    assert!(
        nodes
            .iter()
            .filter(|node| node["fulfillBy"] == json!("2026-12-01T00:00:00Z"))
            .count()
            >= 2
    );

    let merge = proxy.process_request(json_graphql_request(
        r#"
        mutation MergeSplitFulfillmentOrders($fulfillmentOrderMergeInputs: [FulfillmentOrderMergeInput!]!) {
          fulfillmentOrderMerge(fulfillmentOrderMergeInputs: $fulfillmentOrderMergeInputs) {
            fulfillmentOrderMerges {
              fulfillmentOrder {
                id
                supportedActions { action }
                lineItems(first: 5) { nodes { remainingQuantity } }
              }
            }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "fulfillmentOrderMergeInputs": [{
                "mergeIntents": [
                    { "fulfillmentOrderId": original_id },
                    { "fulfillmentOrderId": remaining_id }
                ]
            }]
        }),
    ));
    assert_eq!(
        merge.body["data"]["fulfillmentOrderMerge"]["userErrors"],
        json!([])
    );
    assert_eq!(
        merge.body["data"]["fulfillmentOrderMerge"]["fulfillmentOrderMerges"][0]
            ["fulfillmentOrder"]["supportedActions"],
        json!([
            { "action": "CREATE_FULFILLMENT" },
            { "action": "REPORT_PROGRESS" },
            { "action": "MOVE" },
            { "action": "HOLD" },
            { "action": "SPLIT" }
        ])
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
            userErrors { field message  }
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
fn store_property_node_reads_resolve_shop_records_from_store_state() {
    let mut proxy = snapshot_proxy();
    let dump = proxy.process_request(request_with_body("POST", "/__meta/dump", ""));
    assert_eq!(dump.status, 200);
    let mut restored = dump.body;
    restored["state"]["baseState"]["shop"]["myshopifyDomain"] = json!("state-shop.myshopify.com");
    restored["state"]["baseState"]["shop"]["shopAddress"] = json!({
        "id": "gid://shopify/ShopAddress/900001",
        "address1": "55 Store State Ave",
        "address2": null,
        "city": "Hamilton",
        "company": "Stateful Shop",
        "coordinatesValidated": true,
        "country": "Canada",
        "countryCodeV2": "CA",
        "formatted": ["55 Store State Ave", "Hamilton ON L8P1A1", "Canada"],
        "formattedArea": "Hamilton ON, Canada",
        "latitude": 43.2557,
        "longitude": -79.8711,
        "phone": "+1 555 0100",
        "province": "Ontario",
        "provinceCode": "ON",
        "zip": "L8P1A1"
    });
    let restore = proxy.process_request(request_with_body(
        "POST",
        "/__meta/restore",
        &restored.to_string(),
    ));
    assert_eq!(restore.status, 200);

    let update = proxy.process_request(json_graphql_request(
        r#"
        mutation SeedShopPolicyForNodeRead($shopPolicy: ShopPolicyInput!) {
          shopPolicyUpdate(shopPolicy: $shopPolicy) {
            shopPolicy { id type title body url }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "shopPolicy": {
                "type": "CONTACT_INFORMATION",
                "body": "<p>Use the store-state contact channel.</p>"
            }
        }),
    ));
    assert_eq!(update.status, 200);
    assert_eq!(
        update.body["data"]["shopPolicyUpdate"]["userErrors"],
        json!([])
    );
    let policy_id = update.body["data"]["shopPolicyUpdate"]["shopPolicy"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_ne!(policy_id, "gid://shopify/ShopPolicy/42438689001");

    let product = proxy.process_request(json_graphql_request(
        r#"
        mutation SeedProductVariantForMixedNodeRead($product: ProductCreateInput!) {
          productCreate(product: $product) {
            product {
              id
              title
              variants(first: 1) { nodes { id title } }
            }
            userErrors { field message }
          }
        }
        "#,
        json!({ "product": { "title": "Mixed node resolver product" } }),
    ));
    assert_eq!(product.status, 200);
    assert_eq!(
        product.body["data"]["productCreate"]["userErrors"],
        json!([])
    );
    let product_id = product.body["data"]["productCreate"]["product"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let variant_id = product.body["data"]["productCreate"]["product"]["variants"]["nodes"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let query = r#"
        query AdminPlatformStorePropertyNodeReads($policyId: ID!, $variantId: ID!) {
          shop { myshopifyDomain }
          shopAddressNode: node(id: "gid://shopify/ShopAddress/900001") {
            __typename
            ... on ShopAddress { id address1 city country formatted }
          }
          shopPolicyNode: node(id: $policyId) {
            __typename
            ... on ShopPolicy { id title type body url translations(locale: "fr") { key locale value } }
          }
          productVariantNode: node(id: $variantId) {
            __typename
            ... on ProductVariant { id title product { id title } }
          }
          nodes(ids: ["gid://shopify/ShopAddress/900001", $policyId, $variantId]) {
            __typename
            ... on ShopAddress { id address1 city country formatted }
            ... on ShopPolicy { id title type body url translations(locale: "fr") { key locale value } }
            ... on ProductVariant { id title product { id title } }
          }
        }
    "#;

    let response = proxy.process_request(json_graphql_request(
        query,
        json!({ "policyId": policy_id.clone(), "variantId": variant_id.clone() }),
    ));

    assert_eq!(response.status, 200);
    assert_eq!(
        response.body,
        json!({
            "data": {
                "shop": {
                    "myshopifyDomain": "state-shop.myshopify.com"
                },
                "shopAddressNode": {
                    "__typename": "ShopAddress",
                    "id": "gid://shopify/ShopAddress/900001",
                    "address1": "55 Store State Ave",
                    "city": "Hamilton",
                    "country": "Canada",
                    "formatted": ["55 Store State Ave", "Hamilton ON L8P1A1", "Canada"]
                },
                "shopPolicyNode": {
                    "__typename": "ShopPolicy",
                    "id": policy_id,
                    "title": "Contact Information",
                    "type": "CONTACT_INFORMATION",
                    "body": "<p>Use the store-state contact channel.</p>",
                    "url": "https://state-shop.myshopify.com/policies/1.html?locale=en",
                    "translations": []
                },
                "productVariantNode": {
                    "__typename": "ProductVariant",
                    "id": variant_id,
                    "title": "Default Title",
                    "product": {
                        "id": product_id,
                        "title": "Mixed node resolver product"
                    }
                },
                "nodes": [
                    {
                        "__typename": "ShopAddress",
                        "id": "gid://shopify/ShopAddress/900001",
                        "address1": "55 Store State Ave",
                        "city": "Hamilton",
                        "country": "Canada",
                        "formatted": ["55 Store State Ave", "Hamilton ON L8P1A1", "Canada"]
                    },
                    {
                        "__typename": "ShopPolicy",
                        "id": policy_id,
                        "title": "Contact Information",
                        "type": "CONTACT_INFORMATION",
                        "body": "<p>Use the store-state contact channel.</p>",
                        "url": "https://state-shop.myshopify.com/policies/1.html?locale=en",
                        "translations": []
                    },
                    {
                        "__typename": "ProductVariant",
                        "id": variant_id,
                        "title": "Default Title",
                        "product": {
                            "id": product_id,
                            "title": "Mixed node resolver product"
                        }
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
                        "id": "gid://shopify/Shop/92891250994",
                        "myshopifyDomain": "seeded-policy-shop.myshopify.com",
                        "primaryDomain": { "host": "policies.example.com" },
                        "shopPolicies": [
                            {
                                "id": "gid://shopify/ShopPolicy/111",
                                "title": "Contact",
                                "body": "<p>Contact</p>",
                                "type": "CONTACT_INFORMATION",
                                "url": "https://checkout.shopify.com/92891250994/policies/111.html?locale=en",
                                "createdAt": "2026-01-01T00:00:00Z",
                                "updatedAt": "2026-01-01T00:00:00Z"
                            },
                            {
                                "id": "gid://shopify/ShopPolicy/222",
                                "title": "Privacy policy",
                                "body": "<p>Old</p>",
                                "type": "PRIVACY_POLICY",
                                "url": "https://checkout.shopify.com/92891250994/policies/222.html?locale=en",
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
            "url": "https://checkout.shopify.com/92891250994/policies/222.html?locale=en",
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
                "url": "https://checkout.shopify.com/92891250994/policies/111.html?locale=en"
            },
            {
                "id": "gid://shopify/ShopPolicy/222",
                "type": "PRIVACY_POLICY",
                "title": "Privacy Policy",
                "body": "<p>New</p>",
                "url": "https://checkout.shopify.com/92891250994/policies/222.html?locale=en"
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

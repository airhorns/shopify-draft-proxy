use super::common::*;
use pretty_assertions::assert_eq;
use shopify_draft_proxy::proxy::{Response, UnsupportedMutationMode};

fn seeded_product(id: &str, title: &str) -> ProductRecord {
    ProductRecord {
        id: id.to_string(),
        created_at: "2024-01-01T00:00:00.000Z".to_string(),
        updated_at: "2024-01-01T00:00:00.000Z".to_string(),
        title: title.to_string(),
        handle: title.to_ascii_lowercase().replace(' ', "-"),
        status: "ACTIVE".to_string(),
        description_html: String::new(),
        vendor: String::new(),
        product_type: String::new(),
        tags: Vec::new(),
        template_suffix: String::new(),
        seo_title: String::new(),
        seo_description: String::new(),
        ..ProductRecord::default()
    }
}

fn valid_selling_plan_input(name: &str) -> Value {
    json!({
        "name": name,
        "options": [name],
        "category": "SUBSCRIPTION",
        "billingPolicy": { "recurring": { "interval": "MONTH", "intervalCount": 1 } },
        "deliveryPolicy": { "recurring": { "interval": "MONTH", "intervalCount": 1 } }
    })
}

fn valid_selling_plan_group_input(name: &str) -> Value {
    json!({
        "name": name,
        "options": ["Delivery frequency"],
        "sellingPlansToCreate": [valid_selling_plan_input("Monthly")]
    })
}

#[test]
fn shop_currency_drives_discount_and_selling_plan_money_in_one_session() {
    let mut proxy = snapshot_proxy();
    restore_shop_currency(&mut proxy, "EUR");

    let shop = proxy.process_request(json_graphql_request(
        "query ShopCurrencyForMoneyFields { shop { currencyCode } }",
        json!({}),
    ));
    assert_eq!(shop.status, 200);
    assert_eq!(shop.body["data"]["shop"]["currencyCode"], json!("EUR"));

    let discount = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateFixedAmountDiscount($input: DiscountCodeBasicInput!) {
          discountCodeBasicCreate(basicCodeDiscount: $input) {
            codeDiscountNode {
              codeDiscount {
                __typename
                ... on DiscountCodeBasic {
                  customerGets {
                    value {
                      __typename
                      ... on DiscountAmount {
                        amount { amount currencyCode }
                        appliesOnEachItem
                      }
                    }
                  }
                }
              }
            }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "input": {
            "title": "Fixed amount currency",
            "code": "FIXEDCURRENCY",
            "startsAt": "2026-04-25T00:00:00Z",
            "customerGets": {
                "value": { "discountAmount": { "amount": "10.00", "appliesOnEachItem": true } },
                "items": { "all": true }
            }
        }}),
    ));
    assert_eq!(
        discount.body["data"]["discountCodeBasicCreate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        discount.body["data"]["discountCodeBasicCreate"]["codeDiscountNode"]["codeDiscount"]
            ["customerGets"]["value"]["amount"],
        json!({ "amount": "10.0", "currencyCode": "EUR" })
    );

    let selling_plan = create_selling_plan_group_with_summary(
        &mut proxy,
        json!({
            "name": "Fixed shop currency summary",
            "options": ["Delivery frequency"],
            "sellingPlansToCreate": [selling_plan_input_with_policy(
                "Fixed monthly",
                vec!["Fixed monthly"],
                "FIXED_AMOUNT",
                json!({ "fixedValue": "5.0" }),
            )]
        }),
    );
    assert_eq!(
        selling_plan.body["data"]["sellingPlanGroupCreate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        selling_plan.body["data"]["sellingPlanGroupCreate"]["sellingPlanGroup"]["sellingPlans"]
            ["nodes"][0]["pricingPolicies"][0]["adjustmentValue"],
        json!({
            "__typename": "MoneyV2",
            "amount": "5.0",
            "currencyCode": "EUR"
        })
    );
    assert_eq!(
        selling_plan.body["data"]["sellingPlanGroupCreate"]["sellingPlanGroup"]["summary"],
        json!("1 delivery frequency, 5 EUR discount")
    );
}

fn create_selling_plan_group(proxy: &mut DraftProxy, input: Value) -> Response {
    proxy.process_request(json_graphql_request(
        r#"
        mutation CreateSellingPlanGroupForValidation($input: SellingPlanGroupInput!) {
          sellingPlanGroupCreate(input: $input) {
            sellingPlanGroup {
              id
              sellingPlans(first: 5) { nodes { id } }
            }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "input": input }),
    ))
}

fn create_selling_plan_group_with_resources(
    proxy: &mut DraftProxy,
    input: Value,
    resources: Value,
) -> Value {
    let response = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateSellingPlanGroupWithResources(
          $input: SellingPlanGroupInput!
          $resources: SellingPlanGroupResourceInput
        ) {
          sellingPlanGroupCreate(input: $input, resources: $resources) {
            sellingPlanGroup { id name merchantCode }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "input": input, "resources": resources }),
    ));
    assert_eq!(response.status, 200);
    assert_eq!(
        response.body["data"]["sellingPlanGroupCreate"]["userErrors"],
        json!([])
    );
    response.body["data"]["sellingPlanGroupCreate"]["sellingPlanGroup"].clone()
}

fn create_selling_plan_group_with_summary(proxy: &mut DraftProxy, input: Value) -> Response {
    proxy.process_request(json_graphql_request(
        r#"
        mutation CreateSellingPlanGroupForSummary($input: SellingPlanGroupInput!) {
          sellingPlanGroupCreate(input: $input) {
            sellingPlanGroup {
              id
              summary
              sellingPlans(first: 10) {
                nodes {
                  id
                  pricingPolicies {
                    __typename
                    ... on SellingPlanFixedPricingPolicy {
                      adjustmentType
                      adjustmentValue {
                        __typename
                        ... on SellingPlanPricingPolicyPercentageValue { percentage }
                        ... on MoneyV2 { amount currencyCode }
                      }
                    }
                  }
                }
              }
            }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "input": input }),
    ))
}

fn selling_plan_group_nodes(proxy: &mut DraftProxy) -> Value {
    proxy
        .process_request(json_graphql_request(
            r#"
            query SellingPlanGroupNodesAfterValidation {
              sellingPlanGroups(first: 5) { nodes { id } }
            }
            "#,
            json!({}),
        ))
        .body["data"]["sellingPlanGroups"]["nodes"]
        .clone()
}

fn selling_plan_group_plan_nodes(proxy: &mut DraftProxy, group_id: &str) -> Value {
    proxy
        .process_request(json_graphql_request(
            r#"
            query SellingPlanGroupPlanNodesAfterUpdate($id: ID!) {
              sellingPlanGroup(id: $id) {
                sellingPlans(first: 5) { nodes { id } }
              }
            }
            "#,
            json!({ "id": group_id }),
        ))
        .body["data"]["sellingPlanGroup"]["sellingPlans"]["nodes"]
        .clone()
}

fn selling_plan_input_with_policy(
    name: &str,
    options: Vec<&str>,
    adjustment_type: &str,
    adjustment_value: Value,
) -> Value {
    json!({
        "name": name,
        "options": options,
        "category": "SUBSCRIPTION",
        "billingPolicy": { "recurring": { "interval": "MONTH", "intervalCount": 1 } },
        "deliveryPolicy": { "recurring": { "interval": "MONTH", "intervalCount": 1 } },
        "pricingPolicies": [{
            "fixed": {
                "adjustmentType": adjustment_type,
                "adjustmentValue": adjustment_value
            }
        }]
    })
}

fn selling_plan_input_with_pricing_policies(
    name: &str,
    options: Vec<&str>,
    pricing_policies: Vec<Value>,
) -> Value {
    json!({
        "name": name,
        "options": options,
        "category": "SUBSCRIPTION",
        "billingPolicy": { "recurring": { "interval": "MONTH", "intervalCount": 1 } },
        "deliveryPolicy": { "recurring": { "interval": "MONTH", "intervalCount": 1 } },
        "pricingPolicies": pricing_policies
    })
}

fn fixed_percentage_pricing_policy(percentage: f64) -> Value {
    json!({
        "fixed": {
            "adjustmentType": "PERCENTAGE",
            "adjustmentValue": { "percentage": percentage }
        }
    })
}

fn recurring_percentage_pricing_policy(percentage: f64, after_cycle: i64) -> Value {
    json!({
        "recurring": {
            "adjustmentType": "PERCENTAGE",
            "adjustmentValue": { "percentage": percentage },
            "afterCycle": after_cycle
        }
    })
}

fn live_selling_plan_group_node(
    group_id: &str,
    name: &str,
    product_ids: &[&str],
    product_variant_ids: &[&str],
) -> Value {
    json!({
        "__typename": "SellingPlanGroup",
        "id": group_id,
        "appId": "gid://shopify/App/123",
        "name": name,
        "merchantCode": name.to_ascii_lowercase().replace(' ', "-"),
        "description": format!("{name} description"),
        "options": ["Delivery frequency"],
        "position": 1,
        "createdAt": "2024-04-01T00:00:00Z",
        "updatedAt": "2024-04-01T00:00:00Z",
        "products": {
            "nodes": product_ids
                .iter()
                .map(|id| json!({ "__typename": "Product", "id": id, "title": format!("Product {}", id.rsplit('/').next().unwrap_or_default()) }))
                .collect::<Vec<_>>()
        },
        "productVariants": {
            "nodes": product_variant_ids
                .iter()
                .map(|id| json!({
                    "__typename": "ProductVariant",
                    "id": id,
                    "title": format!("Variant {}", id.rsplit('/').next().unwrap_or_default()),
                    "product": { "id": product_ids.first().copied().unwrap_or("gid://shopify/Product/1") }
                }))
                .collect::<Vec<_>>()
        },
        "sellingPlans": {
            "nodes": [{
                "__typename": "SellingPlan",
                "id": format!("{group_id}/SellingPlan/1"),
                "name": "Monthly",
                "description": "Monthly plan",
                "options": ["Monthly"],
                "position": 1,
                "category": "SUBSCRIPTION",
                "createdAt": "2024-04-01T00:00:00Z",
                "billingPolicy": {
                    "__typename": "SellingPlanRecurringBillingPolicy",
                    "interval": "MONTH",
                    "intervalCount": 1,
                    "minCycles": null,
                    "maxCycles": null
                },
                "deliveryPolicy": {
                    "__typename": "SellingPlanRecurringDeliveryPolicy",
                    "interval": "MONTH",
                    "intervalCount": 1,
                    "cutoff": 0,
                    "intent": "FULFILLMENT_BEGIN",
                    "preAnchorBehavior": "ASAP"
                },
                "inventoryPolicy": { "reserve": "ON_FULFILLMENT" },
                "pricingPolicies": []
            }]
        }
    })
}

fn live_product_node(product_id: &str, variant_id: &str, title: &str) -> Value {
    json!({
        "__typename": "Product",
        "id": product_id,
        "title": title,
        "handle": title.to_ascii_lowercase().replace(' ', "-"),
        "status": "ACTIVE",
        "createdAt": "2024-04-01T00:00:00Z",
        "updatedAt": "2024-04-01T00:00:00Z",
        "variants": {
            "nodes": [{
                "__typename": "ProductVariant",
                "id": variant_id,
                "title": "Default Title",
                "sku": "",
                "price": "10.00",
                "compareAtPrice": null,
                "selectedOptions": [{ "name": "Title", "value": "Default Title" }],
                "inventoryItem": { "id": "gid://shopify/InventoryItem/7301" }
            }]
        }
    })
}

fn live_variant_node(variant_id: &str, product_id: &str) -> Value {
    json!({
        "__typename": "ProductVariant",
        "id": variant_id,
        "title": "Default Title",
        "sku": "",
        "price": "10.00",
        "compareAtPrice": null,
        "selectedOptions": [{ "name": "Title", "value": "Default Title" }],
        "inventoryItem": { "id": "gid://shopify/InventoryItem/7301" },
        "product": {
            "id": product_id,
            "title": "Live product",
            "handle": "live-product",
            "status": "ACTIVE",
            "createdAt": "2024-04-01T00:00:00Z",
            "updatedAt": "2024-04-01T00:00:00Z"
        }
    })
}

#[test]
fn selling_plan_group_create_reads_back_mixed_pricing_policies() {
    let upstream_called = Arc::new(Mutex::new(false));
    let upstream_called_for_proxy = Arc::clone(&upstream_called);
    let mut proxy = configured_proxy(
        ReadMode::LiveHybrid,
        Some(UnsupportedMutationMode::Passthrough),
    )
    .with_upstream_transport(move |_request| {
        *upstream_called_for_proxy.lock().unwrap() = true;
        Response {
            status: 200,
            headers: Default::default(),
            body: json!({"data": {"unexpected": true}}),
        }
    });

    let query = r#"
        mutation CreateSellingPlanGroupWithRecurringPolicies($input: SellingPlanGroupInput!) {
          sellingPlanGroupCreate(input: $input) {
            sellingPlanGroup {
              id
              summary
              sellingPlans(first: 5) {
                nodes {
                  name
                  pricingPolicies {
                    __typename
                    ... on SellingPlanFixedPricingPolicy {
                      adjustmentType
                      adjustmentValue {
                        __typename
                        ... on SellingPlanPricingPolicyPercentageValue { percentage }
                      }
                    }
                    ... on SellingPlanRecurringPricingPolicy {
                      afterCycle
                      createdAt
                      adjustmentType
                      adjustmentValue {
                        __typename
                        ... on SellingPlanPricingPolicyPercentageValue { percentage }
                      }
                    }
                  }
                }
              }
            }
            userErrors { field message code }
          }
        }
        "#;
    let variables = json!({
        "input": {
            "name": "Recurring policy group",
            "options": ["Delivery frequency"],
            "sellingPlansToCreate": [
                selling_plan_input_with_pricing_policies(
                    "Fixed and recurring",
                    vec!["Fixed and recurring"],
                    vec![
                        fixed_percentage_pricing_policy(10.0),
                        recurring_percentage_pricing_policy(5.0, 2),
                    ],
                )
            ]
        }
    });
    let raw_body = json!({ "query": query, "variables": variables }).to_string();
    let response = proxy.process_request(graphql_request("POST", &raw_body));

    assert_eq!(response.status, 200);
    let payload = &response.body["data"]["sellingPlanGroupCreate"];
    assert_eq!(payload["userErrors"], json!([]));
    assert_eq!(
        payload["sellingPlanGroup"]["summary"],
        json!("1 delivery frequency, 5-10% discount")
    );
    assert_eq!(
        payload["sellingPlanGroup"]["sellingPlans"]["nodes"][0]["pricingPolicies"],
        json!([
            {
                "__typename": "SellingPlanFixedPricingPolicy",
                "adjustmentType": "PERCENTAGE",
                "adjustmentValue": {
                    "__typename": "SellingPlanPricingPolicyPercentageValue",
                    "percentage": 10.0
                }
            },
            {
                "__typename": "SellingPlanRecurringPricingPolicy",
                "afterCycle": 2,
                "createdAt": "2024-01-01T00:00:01.000Z",
                "adjustmentType": "PERCENTAGE",
                "adjustmentValue": {
                    "__typename": "SellingPlanPricingPolicyPercentageValue",
                    "percentage": 5.0
                }
            }
        ])
    );
    assert_eq!(*upstream_called.lock().unwrap(), false);
    assert_eq!(
        log_snapshot(&proxy)["entries"][0]["rawBody"],
        json!(raw_body)
    );
}

#[test]
fn selling_plan_group_update_reads_back_recurring_pricing_policy() {
    let mut proxy = snapshot_proxy();
    let create = create_selling_plan_group(
        &mut proxy,
        valid_selling_plan_group_input("Recurring update seed"),
    );
    assert_eq!(
        create.body["data"]["sellingPlanGroupCreate"]["userErrors"],
        json!([])
    );
    let group_id = create.body["data"]["sellingPlanGroupCreate"]["sellingPlanGroup"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let plan_id = create.body["data"]["sellingPlanGroupCreate"]["sellingPlanGroup"]["sellingPlans"]
        ["nodes"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let update = proxy.process_request(json_graphql_request(
        r#"
        mutation UpdateSellingPlanRecurringPolicy($id: ID!, $input: SellingPlanGroupInput!) {
          sellingPlanGroupUpdate(id: $id, input: $input) {
            sellingPlanGroup {
              sellingPlans(first: 5) {
                nodes {
                  id
                  pricingPolicies {
                    __typename
                    ... on SellingPlanFixedPricingPolicy {
                      adjustmentType
                      adjustmentValue {
                        __typename
                        ... on SellingPlanPricingPolicyPercentageValue { percentage }
                      }
                    }
                    ... on SellingPlanRecurringPricingPolicy {
                      afterCycle
                      createdAt
                      adjustmentType
                      adjustmentValue {
                        __typename
                        ... on SellingPlanPricingPolicyPercentageValue { percentage }
                      }
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
            "id": group_id,
            "input": {
                "sellingPlansToUpdate": [{
                    "id": plan_id,
                    "pricingPolicies": [
                        fixed_percentage_pricing_policy(10.0),
                        recurring_percentage_pricing_policy(7.5, 3)
                    ]
                }]
            }
        }),
    ));

    assert_eq!(update.status, 200);
    let payload = &update.body["data"]["sellingPlanGroupUpdate"];
    assert_eq!(payload["userErrors"], json!([]));
    assert_eq!(
        payload["sellingPlanGroup"]["sellingPlans"]["nodes"][0]["pricingPolicies"],
        json!([
            {
                "__typename": "SellingPlanFixedPricingPolicy",
                "adjustmentType": "PERCENTAGE",
                "adjustmentValue": {
                    "__typename": "SellingPlanPricingPolicyPercentageValue",
                    "percentage": 10.0
                }
            },
            {
                "__typename": "SellingPlanRecurringPricingPolicy",
                "afterCycle": 3,
                "createdAt": "2024-01-01T00:00:01.000Z",
                "adjustmentType": "PERCENTAGE",
                "adjustmentValue": {
                    "__typename": "SellingPlanPricingPolicyPercentageValue",
                    "percentage": 7.5
                }
            }
        ])
    );
}

#[test]
fn selling_plan_group_create_validates_locally_without_upstream_passthrough() {
    let upstream_called = Arc::new(Mutex::new(false));
    let upstream_called_for_proxy = Arc::clone(&upstream_called);
    let mut proxy = configured_proxy(
        ReadMode::LiveHybrid,
        Some(UnsupportedMutationMode::Passthrough),
    )
    .with_upstream_transport(move |_request| {
        *upstream_called_for_proxy.lock().unwrap() = true;
        Response {
            status: 200,
            headers: Default::default(),
            body: json!({"data": {"unexpected": true}}),
        }
    });

    let raw_body = json!({
        "query": r#"
        mutation ValidateSellingPlanGroupCreate($input: SellingPlanGroupInput!) {
          sellingPlanGroupCreate(input: $input) {
            sellingPlanGroup { id }
            userErrors { field message code }
          }
        }
        "#,
        "variables": {
            "input": {
                "name": "Validation group",
                "options": ["a", "b", "c", "d"]
            }
        }
    })
    .to_string();
    let response = proxy.process_request(graphql_request("POST", &raw_body));

    assert_eq!(response.status, 200);
    assert_eq!(
        response.body,
        json!({
            "data": {
                "sellingPlanGroupCreate": {
                    "sellingPlanGroup": null,
                    "userErrors": [{
                        "field": ["input", "options"],
                        "message": "Too many selling plan group options (maximum 3 options)",
                        "code": "TOO_LONG"
                    }]
                }
            }
        })
    );
    assert_eq!(*upstream_called.lock().unwrap(), false);

    let log = proxy.process_request(Request {
        method: "GET".to_string(),
        path: "/__meta/log".to_string(),
        headers: Default::default(),
        body: String::new(),
    });
    assert_eq!(log.body["entries"][0]["status"], json!("failed"));
    assert_eq!(log.body["entries"][0]["rawBody"], json!(raw_body));
}

#[test]
fn live_hybrid_selling_plan_group_read_hydrates_upstream_groups() {
    let group_id = "gid://shopify/SellingPlanGroup/7101";
    let product_id = "gid://shopify/Product/7201";
    let variant_id = "gid://shopify/ProductVariant/7301";
    let group =
        live_selling_plan_group_node(group_id, "Live subscriptions", &[product_id], &[variant_id]);
    let detail_requests = Arc::new(Mutex::new(Vec::<String>::new()));
    let captured_detail_requests = Arc::clone(&detail_requests);
    let mut detail_proxy = configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport({
        let group = group.clone();
        move |request| {
            captured_detail_requests
                .lock()
                .unwrap()
                .push(request.body.clone());
            Response {
                status: 200,
                headers: Default::default(),
                body: json!({
                    "data": {
                        "nodes": [group],
                        "sellingPlanGroup": group
                    }
                }),
            }
        }
    });

    let read_one = detail_proxy.process_request(json_graphql_request(
        r#"
        query LiveSellingPlanGroup($id: ID!) {
          sellingPlanGroup(id: $id) {
            id
            name
            merchantCode
            productsCount { count precision }
            productVariantsCount { count precision }
          }
        }
        "#,
        json!({ "id": group_id }),
    ));
    assert_eq!(read_one.status, 200);
    assert_eq!(
        read_one.body["data"]["sellingPlanGroup"]["id"],
        json!(group_id)
    );
    assert_eq!(
        read_one.body["data"]["sellingPlanGroup"]["name"],
        json!("Live subscriptions")
    );
    assert_eq!(
        read_one.body["data"]["sellingPlanGroup"]["productsCount"],
        json!({ "count": 1, "precision": "EXACT" })
    );

    let product_only_group_id = "gid://shopify/SellingPlanGroup/7104";
    let mut product_only_group = live_selling_plan_group_node(
        product_only_group_id,
        "Product-only live subscriptions",
        &[product_id],
        &[],
    );
    product_only_group["products"]["nodes"][0]["variants"] = json!({
        "nodes": [{
            "__typename": "ProductVariant",
            "id": variant_id,
            "title": "Default Title",
            "sku": "",
            "price": "10.00",
            "compareAtPrice": null,
            "selectedOptions": [{ "name": "Title", "value": "Default Title" }],
            "inventoryItem": { "id": "gid://shopify/InventoryItem/7301" }
        }]
    });
    let mut product_only_proxy = configured_proxy(ReadMode::LiveHybrid, None)
        .with_upstream_transport({
            let product_only_group = product_only_group.clone();
            move |_| Response {
                status: 200,
                headers: Default::default(),
                body: json!({ "data": { "nodes": [product_only_group] } }),
            }
        });
    let product_only_read = product_only_proxy.process_request(json_graphql_request(
        r#"
        query LiveProductOnlySellingPlanGroup($id: ID!) {
          sellingPlanGroup(id: $id) { id name }
        }
        "#,
        json!({ "id": product_only_group_id }),
    ));
    assert_eq!(product_only_read.status, 200);
    let variant_after_product_observation =
        product_only_proxy.process_request(json_graphql_request(
            r#"
            query VariantAfterProductOnlyGroupObservation($variantId: ID!) {
              productVariant(id: $variantId) {
                id
                sellingPlanGroupsCount { count precision }
                sellingPlanGroups(first: 5) { nodes { id } }
              }
            }
            "#,
            json!({ "variantId": variant_id }),
        ));
    assert_eq!(variant_after_product_observation.status, 200);
    assert_eq!(
        variant_after_product_observation.body["data"]["productVariant"]["id"],
        json!(variant_id)
    );
    assert_eq!(
        variant_after_product_observation.body["data"]["productVariant"]["sellingPlanGroupsCount"],
        json!({ "count": 0, "precision": "EXACT" })
    );
    assert_eq!(
        variant_after_product_observation.body["data"]["productVariant"]["sellingPlanGroups"]
            ["nodes"],
        json!([{ "id": product_only_group_id }])
    );

    let connection_requests = Arc::new(Mutex::new(Vec::<String>::new()));
    let captured_connection_requests = Arc::clone(&connection_requests);
    let mut connection_proxy = configured_proxy(ReadMode::LiveHybrid, None)
        .with_upstream_transport({
            let group = group.clone();
            move |request| {
                captured_connection_requests
                    .lock()
                    .unwrap()
                    .push(request.body.clone());
                let body: Value = serde_json::from_str(&request.body).unwrap();
                if body
                    .get("query")
                    .and_then(Value::as_str)
                    .is_some_and(|query| query.contains("nodes(ids: $ids)"))
                {
                    return Response {
                        status: 200,
                        headers: Default::default(),
                        body: json!({ "data": { "nodes": [group] } }),
                    };
                }
                Response {
                    status: 200,
                    headers: Default::default(),
                    body: json!({
                        "data": {
                            "sellingPlanGroups": {
                                "nodes": [group],
                                "pageInfo": {
                                    "hasNextPage": false,
                                    "hasPreviousPage": false,
                                    "startCursor": "cursor-1",
                                    "endCursor": "cursor-1"
                                }
                            }
                        }
                    }),
                }
            }
        });
    let read_many = connection_proxy.process_request(json_graphql_request(
        r#"
        query LiveSellingPlanGroups {
          sellingPlanGroups(first: 5) {
            nodes { id name merchantCode }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(read_many.status, 200);
    assert_eq!(
        read_many.body["data"]["sellingPlanGroups"]["nodes"],
        json!([{ "id": group_id, "name": "Live subscriptions", "merchantCode": "live-subscriptions" }])
    );

    let merge_requests = Arc::new(Mutex::new(Vec::<String>::new()));
    let captured_merge_requests = Arc::clone(&merge_requests);
    let mut duplicate = live_selling_plan_group_node(
        "gid://shopify/SellingPlanGroup/7102",
        "Remote duplicate",
        &[],
        &[],
    );
    duplicate["merchantCode"] = json!("local-subscriptions");
    let unrelated = live_selling_plan_group_node(
        "gid://shopify/SellingPlanGroup/7103",
        "Unrelated live",
        &[],
        &[],
    );
    let mut merge_proxy = configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport({
        let duplicate = duplicate.clone();
        let unrelated = unrelated.clone();
        move |request| {
            captured_merge_requests
                .lock()
                .unwrap()
                .push(request.body.clone());
            let body: Value = serde_json::from_str(&request.body).unwrap();
            if body
                .get("query")
                .and_then(Value::as_str)
                .is_some_and(|query| query.contains("nodes(ids: $ids)"))
            {
                let ids = body
                    .pointer("/variables/ids")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                let nodes = ids
                    .into_iter()
                    .map(|id| match id.as_str() {
                        Some("gid://shopify/SellingPlanGroup/7102") => duplicate.clone(),
                        Some("gid://shopify/SellingPlanGroup/7103") => unrelated.clone(),
                        _ => Value::Null,
                    })
                    .collect::<Vec<_>>();
                return Response {
                    status: 200,
                    headers: Default::default(),
                    body: json!({ "data": { "nodes": nodes } }),
                };
            }
            Response {
                status: 200,
                headers: Default::default(),
                body: json!({
                    "data": {
                        "sellingPlanGroups": {
                            "nodes": [duplicate, unrelated],
                            "pageInfo": {
                                "hasNextPage": false,
                                "hasPreviousPage": false,
                                "startCursor": "cursor-1",
                                "endCursor": "cursor-2"
                            }
                        }
                    }
                }),
            }
        }
    });
    let mut local_input = valid_selling_plan_group_input("Local subscriptions");
    local_input["merchantCode"] = json!("local-subscriptions");
    let local_create = merge_proxy.process_request(json_graphql_request(
        r#"
        mutation CreateLocalSellingPlanGroup($input: SellingPlanGroupInput!) {
          sellingPlanGroupCreate(input: $input) {
            sellingPlanGroup { id name merchantCode }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "input": local_input }),
    ));
    assert_eq!(local_create.status, 200);
    assert_eq!(
        local_create.body["data"]["sellingPlanGroupCreate"]["userErrors"],
        json!([])
    );
    let merged_read = merge_proxy.process_request(json_graphql_request(
        r#"
        query MergedSellingPlanGroups {
          sellingPlanGroups(first: 5, sortKey: ID) {
            nodes { name merchantCode }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        merged_read.body["data"]["sellingPlanGroups"]["nodes"],
        json!([
            { "name": "Local subscriptions", "merchantCode": "local-subscriptions" },
            { "name": "Unrelated live", "merchantCode": "unrelated-live" }
        ])
    );

    let mut requests = detail_requests.lock().unwrap().clone();
    requests.extend(connection_requests.lock().unwrap().clone());
    requests.extend(merge_requests.lock().unwrap().clone());
    assert!(
        requests.iter().all(|body| !body.contains("mutation")),
        "selling-plan live reads should only issue query requests: {requests:?}"
    );
    assert!(
        detail_requests
            .lock()
            .unwrap()
            .iter()
            .any(|body| body.contains("sellingPlanGroup"))
            && connection_requests
                .lock()
                .unwrap()
                .iter()
                .any(|body| body.contains("sellingPlanGroups")),
        "both cold read forms should hydrate from upstream: {requests:?}"
    );
}

#[test]
fn live_hybrid_existing_selling_plan_mutations_preflight_and_stage_locally() {
    let group_id = "gid://shopify/SellingPlanGroup/7101";
    let product_id = "gid://shopify/Product/7201";
    let variant_id = "gid://shopify/ProductVariant/7301";
    let upstream_requests = Arc::new(Mutex::new(Vec::<String>::new()));
    let captured_requests = Arc::clone(&upstream_requests);
    let mut proxy = configured_proxy(
        ReadMode::LiveHybrid,
        Some(UnsupportedMutationMode::Passthrough),
    )
    .with_upstream_transport({
        let group = live_selling_plan_group_node(group_id, "Live subscriptions", &[], &[]);
        let product = live_product_node(product_id, variant_id, "Live product");
        let variant = live_variant_node(variant_id, product_id);
        move |request| {
            captured_requests.lock().unwrap().push(request.body.clone());
            let body: Value = serde_json::from_str(&request.body).unwrap();
            let ids = body
                .pointer("/variables/ids")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            let nodes = ids
                .into_iter()
                .map(|id| match id.as_str() {
                    Some(id) if id == group_id => group.clone(),
                    Some(id) if id == product_id => product.clone(),
                    Some(id) if id == variant_id => variant.clone(),
                    _ => Value::Null,
                })
                .collect::<Vec<_>>();
            Response {
                status: 200,
                headers: Default::default(),
                body: json!({ "data": { "nodes": nodes } }),
            }
        }
    });

    let update = proxy.process_request(json_graphql_request(
        r#"
        mutation UpdateLiveSellingPlanGroup($id: ID!, $input: SellingPlanGroupInput!) {
          sellingPlanGroupUpdate(id: $id, input: $input) {
            sellingPlanGroup { id name merchantCode }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "id": group_id,
            "input": { "name": "Locally updated subscriptions", "merchantCode": "local-code" }
        }),
    ));
    assert_eq!(update.status, 200);
    assert_eq!(
        update.body["data"]["sellingPlanGroupUpdate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        update.body["data"]["sellingPlanGroupUpdate"]["sellingPlanGroup"],
        json!({ "id": group_id, "name": "Locally updated subscriptions", "merchantCode": "local-code" })
    );

    let join = proxy.process_request(json_graphql_request(
        r#"
        mutation JoinLiveProduct($id: ID!, $sellingPlanGroupIds: [ID!]!) {
          productJoinSellingPlanGroups(id: $id, sellingPlanGroupIds: $sellingPlanGroupIds) {
            product { id sellingPlanGroupsCount { count precision } }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "id": product_id, "sellingPlanGroupIds": [group_id] }),
    ));
    assert_eq!(join.status, 200);
    assert_eq!(
        join.body["data"]["productJoinSellingPlanGroups"]["userErrors"],
        json!([])
    );
    assert_eq!(
        join.body["data"]["productJoinSellingPlanGroups"]["product"]["sellingPlanGroupsCount"],
        json!({ "count": 1, "precision": "EXACT" })
    );

    let leave = proxy.process_request(json_graphql_request(
        r#"
        mutation LeaveLiveProduct($id: ID!, $sellingPlanGroupIds: [ID!]!) {
          productLeaveSellingPlanGroups(id: $id, sellingPlanGroupIds: $sellingPlanGroupIds) {
            product { id sellingPlanGroupsCount { count precision } }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "id": product_id, "sellingPlanGroupIds": [group_id] }),
    ));
    assert_eq!(leave.status, 200);
    assert_eq!(
        leave.body["data"]["productLeaveSellingPlanGroups"]["userErrors"],
        json!([])
    );
    assert_eq!(
        leave.body["data"]["productLeaveSellingPlanGroups"]["product"]["sellingPlanGroupsCount"],
        json!({ "count": 0, "precision": "EXACT" })
    );

    let variant_join = proxy.process_request(json_graphql_request(
        r#"
        mutation JoinLiveVariant($id: ID!, $sellingPlanGroupIds: [ID!]!) {
          productVariantJoinSellingPlanGroups(id: $id, sellingPlanGroupIds: $sellingPlanGroupIds) {
            productVariant { id sellingPlanGroupsCount { count precision } }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "id": variant_id, "sellingPlanGroupIds": [group_id] }),
    ));
    assert_eq!(variant_join.status, 200);
    assert_eq!(
        variant_join.body["data"]["productVariantJoinSellingPlanGroups"]["userErrors"],
        json!([])
    );
    assert_eq!(
        variant_join.body["data"]["productVariantJoinSellingPlanGroups"]["productVariant"]
            ["sellingPlanGroupsCount"],
        json!({ "count": 1, "precision": "EXACT" })
    );

    let after_variant_join = proxy.process_request(json_graphql_request(
        r#"
        query DirectProductMembershipAfterVariantJoin($productId: ID!, $variantId: ID!) {
          product(id: $productId) {
            sellingPlanGroupsCount { count precision }
            sellingPlanGroups(first: 5) { nodes { id } }
          }
          productVariant(id: $variantId) {
            sellingPlanGroupsCount { count precision }
            sellingPlanGroups(first: 5) { nodes { id } }
          }
        }
        "#,
        json!({ "productId": product_id, "variantId": variant_id }),
    ));
    assert_eq!(after_variant_join.status, 200);
    assert_eq!(
        after_variant_join.body["data"]["product"]["sellingPlanGroupsCount"],
        json!({ "count": 0, "precision": "EXACT" })
    );
    assert_eq!(
        after_variant_join.body["data"]["product"]["sellingPlanGroups"]["nodes"],
        json!([{ "id": group_id }])
    );
    assert_eq!(
        after_variant_join.body["data"]["productVariant"]["sellingPlanGroupsCount"],
        json!({ "count": 1, "precision": "EXACT" })
    );
    assert_eq!(
        after_variant_join.body["data"]["productVariant"]["sellingPlanGroups"]["nodes"],
        json!([{ "id": group_id }])
    );

    let variant_leave = proxy.process_request(json_graphql_request(
        r#"
        mutation LeaveLiveVariant($id: ID!, $sellingPlanGroupIds: [ID!]!) {
          productVariantLeaveSellingPlanGroups(id: $id, sellingPlanGroupIds: $sellingPlanGroupIds) {
            productVariant { id sellingPlanGroupsCount { count precision } }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "id": variant_id, "sellingPlanGroupIds": [group_id] }),
    ));
    assert_eq!(variant_leave.status, 200);
    assert_eq!(
        variant_leave.body["data"]["productVariantLeaveSellingPlanGroups"]["userErrors"],
        json!([])
    );
    assert_eq!(
        variant_leave.body["data"]["productVariantLeaveSellingPlanGroups"]["productVariant"]
            ["sellingPlanGroupsCount"],
        json!({ "count": 0, "precision": "EXACT" })
    );

    let delete = proxy.process_request(json_graphql_request(
        r#"
        mutation DeleteLiveSellingPlanGroup($id: ID!) {
          sellingPlanGroupDelete(id: $id) {
            deletedSellingPlanGroupId
            userErrors { field message code }
          }
        }
        "#,
        json!({ "id": group_id }),
    ));
    assert_eq!(delete.status, 200);
    assert_eq!(
        delete.body["data"]["sellingPlanGroupDelete"]["deletedSellingPlanGroupId"],
        json!(group_id)
    );
    assert_eq!(
        delete.body["data"]["sellingPlanGroupDelete"]["userErrors"],
        json!([])
    );

    let requests = upstream_requests.lock().unwrap();
    assert!(
        requests.iter().all(|body| !body.contains("mutation")),
        "existing selling-plan mutations must not forward caller mutation documents: {requests:?}"
    );

    let log = log_snapshot(&proxy);
    assert_eq!(log["entries"][0]["status"], json!("staged"));
    assert_eq!(log["entries"][1]["status"], json!("staged"));
    assert_eq!(log["entries"][2]["status"], json!("staged"));
    assert_eq!(log["entries"][3]["status"], json!("staged"));
    assert_eq!(log["entries"][4]["status"], json!("staged"));
    assert_eq!(log["entries"][5]["status"], json!("staged"));
    assert!(
        log["entries"][0]["rawBody"]
            .as_str()
            .unwrap_or_default()
            .contains("UpdateLiveSellingPlanGroup"),
        "commit replay should retain the caller mutation body"
    );
}

#[test]
fn live_hybrid_missing_selling_plan_targets_return_local_user_errors_after_preflight() {
    let missing_group_id = "gid://shopify/SellingPlanGroup/7999";
    let existing_group_id = "gid://shopify/SellingPlanGroup/7101";
    let missing_product_id = "gid://shopify/Product/7999";
    let upstream_requests = Arc::new(Mutex::new(Vec::<String>::new()));
    let captured_requests = Arc::clone(&upstream_requests);
    let mut proxy = configured_proxy(
        ReadMode::LiveHybrid,
        Some(UnsupportedMutationMode::Passthrough),
    )
    .with_upstream_transport({
        let group = live_selling_plan_group_node(existing_group_id, "Live subscriptions", &[], &[]);
        move |request| {
            captured_requests.lock().unwrap().push(request.body.clone());
            let body: Value = serde_json::from_str(&request.body).unwrap();
            let ids = body
                .pointer("/variables/ids")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            let nodes = ids
                .into_iter()
                .map(|id| match id.as_str() {
                    Some(id) if id == existing_group_id => group.clone(),
                    _ => Value::Null,
                })
                .collect::<Vec<_>>();
            Response {
                status: 200,
                headers: Default::default(),
                body: json!({ "data": { "nodes": nodes } }),
            }
        }
    });

    let missing_group = proxy.process_request(json_graphql_request(
        r#"
        mutation UpdateMissingLiveSellingPlanGroup($id: ID!, $input: SellingPlanGroupInput!) {
          sellingPlanGroupUpdate(id: $id, input: $input) {
            sellingPlanGroup { id }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "id": missing_group_id, "input": { "name": "Missing group" } }),
    ));
    assert_eq!(missing_group.status, 200);
    assert_eq!(
        missing_group.body["data"]["sellingPlanGroupUpdate"],
        json!({
            "sellingPlanGroup": null,
            "userErrors": [{
                "field": ["id"],
                "message": "Selling plan group does not exist.",
                "code": "GROUP_DOES_NOT_EXIST"
            }]
        })
    );

    let missing_product = proxy.process_request(json_graphql_request(
        r#"
        mutation JoinMissingLiveProduct($id: ID!, $sellingPlanGroupIds: [ID!]!) {
          productJoinSellingPlanGroups(id: $id, sellingPlanGroupIds: $sellingPlanGroupIds) {
            product { id }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "id": missing_product_id, "sellingPlanGroupIds": [existing_group_id] }),
    ));
    assert_eq!(missing_product.status, 200);
    assert_eq!(
        missing_product.body["data"]["productJoinSellingPlanGroups"],
        json!({
            "product": null,
            "userErrors": [{
                "field": ["id"],
                "message": "Product does not exist.",
                "code": "NOT_FOUND"
            }]
        })
    );

    let requests = upstream_requests.lock().unwrap();
    assert!(
        requests.iter().all(|body| !body.contains("mutation")),
        "missing-target preflights must stay read-only: {requests:?}"
    );

    let log = log_snapshot(&proxy);
    assert_eq!(log["entries"][0]["status"], json!("failed"));
    assert_eq!(log["entries"][1]["status"], json!("failed"));
}

#[test]
fn selling_plan_group_create_rejects_active_model_validation_without_staging() {
    let mut absent_name_input = valid_selling_plan_group_input("Absent name");
    absent_name_input.as_object_mut().unwrap().remove("name");

    let mut absent_plans_input = valid_selling_plan_group_input("Absent plans");
    absent_plans_input
        .as_object_mut()
        .unwrap()
        .remove("sellingPlansToCreate");

    let too_many_plans = (1..=32)
        .map(|index| valid_selling_plan_input(&format!("Monthly {index}")))
        .collect::<Vec<_>>();

    let cases = vec![
        (
            "blank name",
            valid_selling_plan_group_input("   "),
            json!([{
                "field": ["input", "name"],
                "message": "Name can't be blank",
                "code": "BLANK"
            }]),
        ),
        (
            "absent name",
            absent_name_input,
            json!([{
                "field": ["input", "name"],
                "message": "Name can't be blank",
                "code": "BLANK"
            }]),
        ),
        (
            "zero plans",
            json!({
                "name": "Zero plans",
                "options": ["Delivery frequency"],
                "sellingPlansToCreate": []
            }),
            json!([{
                "field": ["input"],
                "message": "Selling plan groups must have at least 1 selling plan.",
                "code": "SELLING_PLAN_COUNT_LOWER_BOUND"
            }]),
        ),
        (
            "absent plans",
            absent_plans_input,
            json!([{
                "field": ["input"],
                "message": "Selling plan groups must have at least 1 selling plan.",
                "code": "SELLING_PLAN_COUNT_LOWER_BOUND"
            }]),
        ),
        (
            "too many plans",
            json!({
                "name": "Too many plans",
                "options": ["Delivery frequency"],
                "sellingPlansToCreate": too_many_plans
            }),
            json!([{
                "field": ["input"],
                "message": "Selling plan groups can't have more than 31 selling plans.",
                "code": "SELLING_PLAN_COUNT_UPPER_BOUND"
            }]),
        ),
        (
            "missing billing policy",
            json!({
                "name": "Missing billing",
                "options": ["Delivery frequency"],
                "sellingPlansToCreate": [{
                    "name": "Monthly",
                    "options": ["Monthly"],
                    "category": "SUBSCRIPTION",
                    "deliveryPolicy": { "recurring": { "interval": "MONTH", "intervalCount": 1 } }
                }]
            }),
            json!([{
                "field": ["input", "sellingPlansToCreate", "0", "billingPolicy"],
                "message": "Selling plans to create billing policy must be present.",
                "code": "SELLING_PLAN_BILLING_POLICY_MISSING"
            }]),
        ),
        (
            "missing delivery policy",
            json!({
                "name": "Missing delivery",
                "options": ["Delivery frequency"],
                "sellingPlansToCreate": [{
                    "name": "Monthly",
                    "options": ["Monthly"],
                    "category": "SUBSCRIPTION",
                    "billingPolicy": { "recurring": { "interval": "MONTH", "intervalCount": 1 } }
                }]
            }),
            json!([{
                "field": ["input", "sellingPlansToCreate", "0", "deliveryPolicy"],
                "message": "Selling plans to create delivery policy must be present.",
                "code": "SELLING_PLAN_DELIVERY_POLICY_MISSING"
            }]),
        ),
        (
            "missing both policies",
            json!({
                "name": "Missing both",
                "options": ["Delivery frequency"],
                "sellingPlansToCreate": [{
                    "name": "Monthly",
                    "options": ["Monthly"],
                    "category": "SUBSCRIPTION"
                }]
            }),
            json!([
                {
                    "field": ["input", "sellingPlansToCreate", "0", "billingPolicy"],
                    "message": "Selling plans to create billing policy must be present.",
                    "code": "SELLING_PLAN_BILLING_POLICY_MISSING"
                },
                {
                    "field": ["input", "sellingPlansToCreate", "0", "deliveryPolicy"],
                    "message": "Selling plans to create delivery policy must be present.",
                    "code": "SELLING_PLAN_DELIVERY_POLICY_MISSING"
                }
            ]),
        ),
    ];

    for (label, input, expected_user_errors) in cases {
        let mut proxy = snapshot_proxy();
        let response = create_selling_plan_group(&mut proxy, input);
        assert_eq!(response.status, 200, "{label}");
        assert_eq!(
            response.body["data"]["sellingPlanGroupCreate"]["sellingPlanGroup"],
            Value::Null,
            "{label}"
        );
        assert_eq!(
            response.body["data"]["sellingPlanGroupCreate"]["userErrors"], expected_user_errors,
            "{label}"
        );
        assert_eq!(selling_plan_group_nodes(&mut proxy), json!([]), "{label}");

        let log = log_snapshot(&proxy);
        assert_eq!(log["entries"][0]["status"], json!("failed"), "{label}");
        assert_eq!(log["entries"][0]["stagedResourceIds"], json!([]), "{label}");
    }
}

#[test]
fn selling_plan_group_create_rejects_recurring_only_pricing_policies_without_staging() {
    let mut proxy = snapshot_proxy();
    let response = create_selling_plan_group(
        &mut proxy,
        json!({
            "name": "Recurring-only policy group",
            "options": ["Delivery frequency"],
            "sellingPlansToCreate": [
                selling_plan_input_with_pricing_policies(
                    "Recurring-only policy plan",
                    vec!["Monthly"],
                    vec![recurring_percentage_pricing_policy(5.0, 2)],
                )
            ]
        }),
    );

    assert_eq!(response.status, 200);
    assert_eq!(
        response.body["data"]["sellingPlanGroupCreate"],
        json!({
            "sellingPlanGroup": null,
            "userErrors": [{
                "field": ["input", "sellingPlansToCreate", "0", "pricingPolicies"],
                "message": "Selling plans to create pricing policies must contain one fixed pricing policy",
                "code": "SELLING_PLAN_PRICING_POLICIES_MUST_CONTAIN_A_FIXED_PRICING_POLICY"
            }]
        })
    );
    assert_eq!(selling_plan_group_nodes(&mut proxy), json!([]));

    let log = log_snapshot(&proxy);
    assert_eq!(log["entries"][0]["status"], json!("failed"));
    assert_eq!(log["entries"][0]["stagedResourceIds"], json!([]));
}

#[test]
fn selling_plan_group_update_accepts_empty_create_list_without_lower_bound_rejection() {
    let mut proxy = snapshot_proxy();

    let create = create_selling_plan_group(
        &mut proxy,
        valid_selling_plan_group_input("Create only lower-bound seed"),
    );
    assert_eq!(
        create.body["data"]["sellingPlanGroupCreate"]["userErrors"],
        json!([])
    );
    let group_id = create.body["data"]["sellingPlanGroupCreate"]["sellingPlanGroup"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let update = proxy.process_request(json_graphql_request(
        r#"
        mutation EmptyCreateListOnUpdate($id: ID!, $input: SellingPlanGroupInput!) {
          sellingPlanGroupUpdate(id: $id, input: $input) {
            deletedSellingPlanIds
            sellingPlanGroup { id sellingPlans(first: 5) { nodes { id } } }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "id": group_id,
            "input": { "sellingPlansToCreate": [] }
        }),
    ));

    assert_eq!(update.status, 200);
    assert_eq!(
        update.body["data"]["sellingPlanGroupUpdate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        update.body["data"]["sellingPlanGroupUpdate"]["deletedSellingPlanIds"],
        json!([])
    );
    assert_eq!(
        update.body["data"]["sellingPlanGroupUpdate"]["sellingPlanGroup"]["sellingPlans"]["nodes"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn selling_plan_group_update_rejects_recurring_only_pricing_policies_without_staging() {
    let mut proxy = snapshot_proxy();

    let create = create_selling_plan_group(
        &mut proxy,
        valid_selling_plan_group_input("Recurring-only update seed"),
    );
    assert_eq!(
        create.body["data"]["sellingPlanGroupCreate"]["userErrors"],
        json!([])
    );
    let group_id = create.body["data"]["sellingPlanGroupCreate"]["sellingPlanGroup"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let plan_id = create.body["data"]["sellingPlanGroupCreate"]["sellingPlanGroup"]["sellingPlans"]
        ["nodes"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let update = proxy.process_request(json_graphql_request(
        r#"
        mutation RecurringOnlyPricingPolicyUpdate($id: ID!, $input: SellingPlanGroupInput!) {
          sellingPlanGroupUpdate(id: $id, input: $input) {
            deletedSellingPlanIds
            sellingPlanGroup { id }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "id": group_id,
            "input": {
                "sellingPlansToUpdate": [{
                    "id": plan_id,
                    "pricingPolicies": [recurring_percentage_pricing_policy(5.0, 2)]
                }]
            }
        }),
    ));

    assert_eq!(update.status, 200);
    assert_eq!(
        update.body["data"]["sellingPlanGroupUpdate"],
        json!({
            "deletedSellingPlanIds": null,
            "sellingPlanGroup": null,
            "userErrors": [{
                "field": ["input", "sellingPlansToUpdate", "0", "pricingPolicies"],
                "message": "Selling plans to update pricing policies must contain one fixed pricing policy",
                "code": "SELLING_PLAN_PRICING_POLICIES_MUST_CONTAIN_A_FIXED_PRICING_POLICY"
            }]
        })
    );
    assert_eq!(
        selling_plan_group_plan_nodes(&mut proxy, &group_id),
        json!([{ "id": plan_id }])
    );

    let log = log_snapshot(&proxy);
    assert_eq!(log["entries"][1]["status"], json!("failed"));
    assert_eq!(log["entries"][1]["stagedResourceIds"], json!([]));
}

#[test]
fn selling_plan_group_update_rejects_deleting_final_plan_without_replacement() {
    let mut proxy = snapshot_proxy();

    let create = create_selling_plan_group(
        &mut proxy,
        valid_selling_plan_group_input("Delete final plan seed"),
    );
    assert_eq!(
        create.body["data"]["sellingPlanGroupCreate"]["userErrors"],
        json!([])
    );
    let group_id = create.body["data"]["sellingPlanGroupCreate"]["sellingPlanGroup"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let plan_id = create.body["data"]["sellingPlanGroupCreate"]["sellingPlanGroup"]["sellingPlans"]
        ["nodes"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let update = proxy.process_request(json_graphql_request(
        r#"
        mutation DeleteFinalSellingPlan($id: ID!, $input: SellingPlanGroupInput!) {
          sellingPlanGroupUpdate(id: $id, input: $input) {
            deletedSellingPlanIds
            sellingPlanGroup { id sellingPlans(first: 5) { nodes { id } } }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "id": group_id,
            "input": { "sellingPlansToDelete": [plan_id] }
        }),
    ));

    assert_eq!(update.status, 200);
    assert_eq!(
        update.body["data"]["sellingPlanGroupUpdate"]["userErrors"],
        json!([
            {
                "field": ["input", "sellingPlansToDelete"],
                "message": "Selling plans to delete can't result in a selling plan group with no selling plan.",
                "code": "SELLING_PLAN_COUNT_LOWER_BOUND"
            }
        ])
    );
    assert_eq!(
        update.body["data"]["sellingPlanGroupUpdate"]["deletedSellingPlanIds"],
        Value::Null
    );
    assert_eq!(
        update.body["data"]["sellingPlanGroupUpdate"]["sellingPlanGroup"],
        Value::Null
    );
    assert_eq!(
        selling_plan_group_plan_nodes(&mut proxy, &group_id),
        json!([{ "id": plan_id }])
    );

    let log = log_snapshot(&proxy);
    assert_eq!(log["entries"][1]["status"], json!("failed"));
    assert_eq!(log["entries"][1]["stagedResourceIds"], json!([]));
}

#[test]
fn selling_plan_group_update_allows_deleting_some_but_not_all_plans() {
    let mut proxy = snapshot_proxy();

    let create = create_selling_plan_group(
        &mut proxy,
        json!({
            "name": "Partial delete seed",
            "options": ["Delivery frequency"],
            "sellingPlansToCreate": [
                valid_selling_plan_input("Monthly"),
                valid_selling_plan_input("Weekly")
            ]
        }),
    );
    assert_eq!(
        create.body["data"]["sellingPlanGroupCreate"]["userErrors"],
        json!([])
    );
    let group_id = create.body["data"]["sellingPlanGroupCreate"]["sellingPlanGroup"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let delete_id = create.body["data"]["sellingPlanGroupCreate"]["sellingPlanGroup"]
        ["sellingPlans"]["nodes"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let keep_id = create.body["data"]["sellingPlanGroupCreate"]["sellingPlanGroup"]["sellingPlans"]
        ["nodes"][1]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let update = proxy.process_request(json_graphql_request(
        r#"
        mutation DeleteOneSellingPlan($id: ID!, $input: SellingPlanGroupInput!) {
          sellingPlanGroupUpdate(id: $id, input: $input) {
            deletedSellingPlanIds
            sellingPlanGroup { id sellingPlans(first: 5) { nodes { id } } }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "id": group_id,
            "input": { "sellingPlansToDelete": [delete_id] }
        }),
    ));

    assert_eq!(update.status, 200);
    assert_eq!(
        update.body["data"]["sellingPlanGroupUpdate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        update.body["data"]["sellingPlanGroupUpdate"]["deletedSellingPlanIds"],
        json!([delete_id])
    );
    assert_eq!(
        update.body["data"]["sellingPlanGroupUpdate"]["sellingPlanGroup"]["sellingPlans"]["nodes"],
        json!([{ "id": keep_id }])
    );
    assert_eq!(
        selling_plan_group_plan_nodes(&mut proxy, &group_id),
        json!([{ "id": keep_id }])
    );

    let log = log_snapshot(&proxy);
    assert_eq!(log["entries"][1]["status"], json!("staged"));
    assert_eq!(log["entries"][1]["stagedResourceIds"], json!([group_id]));
}

#[test]
fn selling_plan_group_update_allows_deleting_final_plan_with_replacement() {
    let mut proxy = snapshot_proxy();

    let create = create_selling_plan_group(
        &mut proxy,
        valid_selling_plan_group_input("Replacement plan seed"),
    );
    assert_eq!(
        create.body["data"]["sellingPlanGroupCreate"]["userErrors"],
        json!([])
    );
    let group_id = create.body["data"]["sellingPlanGroupCreate"]["sellingPlanGroup"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let delete_id = create.body["data"]["sellingPlanGroupCreate"]["sellingPlanGroup"]
        ["sellingPlans"]["nodes"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let update = proxy.process_request(json_graphql_request(
        r#"
        mutation ReplaceFinalSellingPlan($id: ID!, $input: SellingPlanGroupInput!) {
          sellingPlanGroupUpdate(id: $id, input: $input) {
            deletedSellingPlanIds
            sellingPlanGroup { id sellingPlans(first: 5) { nodes { id name } } }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "id": group_id,
            "input": {
                "sellingPlansToDelete": [delete_id],
                "sellingPlansToCreate": [valid_selling_plan_input("Replacement")]
            }
        }),
    ));

    assert_eq!(update.status, 200);
    assert_eq!(
        update.body["data"]["sellingPlanGroupUpdate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        update.body["data"]["sellingPlanGroupUpdate"]["deletedSellingPlanIds"],
        json!([delete_id])
    );
    let replacement_plans = update.body["data"]["sellingPlanGroupUpdate"]["sellingPlanGroup"]
        ["sellingPlans"]["nodes"]
        .as_array()
        .unwrap();
    assert_eq!(replacement_plans.len(), 1);
    assert_eq!(replacement_plans[0]["name"], json!("Replacement"));
    assert_ne!(replacement_plans[0]["id"], json!(delete_id));

    let log = log_snapshot(&proxy);
    assert_eq!(log["entries"][1]["status"], json!("staged"));
    assert_eq!(log["entries"][1]["stagedResourceIds"], json!([group_id]));
}

#[test]
fn selling_plan_group_recurring_policy_ranges_validate_locally() {
    let mut proxy = snapshot_proxy();

    let create_mutation = r#"
        mutation CreateSellingPlanGroup($input: SellingPlanGroupInput!) {
          sellingPlanGroupCreate(input: $input) {
            sellingPlanGroup {
              id
              sellingPlans(first: 5) { nodes { id } }
            }
            userErrors { field message code }
          }
        }
        "#;
    let invalid_plan = json!({
        "name": "Invalid recurring ranges",
        "options": ["Monthly"],
        "billingPolicy": {
            "recurring": {
                "interval": "MONTH",
                "intervalCount": 1,
                "minCycles": 0,
                "maxCycles": 2_147_483_648_i64
            }
        },
        "deliveryPolicy": {
            "recurring": {
                "interval": "MONTH",
                "intervalCount": 1,
                "cutoff": -1
            }
        }
    });

    let invalid_create = proxy.process_request(json_graphql_request(
        create_mutation,
        json!({
            "input": {
                "name": "Invalid range group",
                "sellingPlansToCreate": [invalid_plan]
            }
        }),
    ));

    assert_eq!(invalid_create.status, 200);
    assert_eq!(
        invalid_create.body["data"]["sellingPlanGroupCreate"],
        json!({
            "sellingPlanGroup": null,
            "userErrors": [
                {
                    "field": ["input", "sellingPlansToCreate", "0", "deliveryPolicy", "recurring", "cutoff"],
                    "message": "Cutoff must be within the range of 0 to 2,147,483,647",
                    "code": "INVALID"
                },
                {
                    "field": ["input", "sellingPlansToCreate", "0", "billingPolicy", "recurring", "minCycles"],
                    "message": "Min cycles must be within the range of 1 to 2,147,483,647",
                    "code": "INVALID"
                },
                {
                    "field": ["input", "sellingPlansToCreate", "0", "billingPolicy", "recurring", "maxCycles"],
                    "message": "Max cycles must be within the range of 1 to 2,147,483,647",
                    "code": "INVALID"
                }
            ]
        })
    );

    let boundary_create = proxy.process_request(json_graphql_request(
        create_mutation,
        json!({
            "input": {
                "name": "Boundary valid group",
                "sellingPlansToCreate": [{
                    "name": "Boundary recurring ranges",
                    "options": ["Monthly"],
                    "billingPolicy": {
                        "recurring": {
                            "interval": "MONTH",
                            "intervalCount": 1,
                            "minCycles": 1,
                            "maxCycles": 2_147_483_647_i64
                        }
                    },
                    "deliveryPolicy": {
                        "recurring": {
                            "interval": "MONTH",
                            "intervalCount": 1,
                            "cutoff": 2_147_483_647_i64
                        }
                    }
                }]
            }
        }),
    ));

    assert_eq!(boundary_create.status, 200);
    assert_eq!(
        boundary_create.body["data"]["sellingPlanGroupCreate"]["userErrors"],
        json!([])
    );
    let group_id = boundary_create.body["data"]["sellingPlanGroupCreate"]["sellingPlanGroup"]["id"]
        .as_str()
        .expect("valid boundary input should stage a group")
        .to_string();

    let read = proxy.process_request(json_graphql_request(
        r#"
        query ReadBoundaryGroup($id: ID!) {
          sellingPlanGroup(id: $id) { id }
        }
        "#,
        json!({ "id": group_id }),
    ));
    assert_eq!(read.body["data"]["sellingPlanGroup"]["id"], json!(group_id));

    let plan_id = boundary_create.body["data"]["sellingPlanGroupCreate"]["sellingPlanGroup"]
        ["sellingPlans"]["nodes"][0]["id"]
        .as_str()
        .expect("valid boundary input should stage a selling plan")
        .to_string();
    let invalid_update = proxy.process_request(json_graphql_request(
        r#"
        mutation UpdateSellingPlanGroup($id: ID!, $input: SellingPlanGroupInput!) {
          sellingPlanGroupUpdate(id: $id, input: $input) {
            sellingPlanGroup { id }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "id": group_id,
            "input": {
                "sellingPlansToUpdate": [{
                    "id": plan_id,
                    "billingPolicy": {
                        "recurring": {
                            "interval": "MONTH",
                            "intervalCount": 1,
                            "minCycles": 0,
                            "maxCycles": 2_147_483_648_i64
                        }
                    },
                    "deliveryPolicy": {
                        "recurring": {
                            "interval": "MONTH",
                            "intervalCount": 1,
                            "cutoff": -1
                        }
                    }
                }]
            }
        }),
    ));

    assert_eq!(invalid_update.status, 200);
    assert_eq!(
        invalid_update.body["data"]["sellingPlanGroupUpdate"],
        json!({
            "sellingPlanGroup": null,
            "userErrors": [
                {
                    "field": ["input", "sellingPlansToUpdate", "0", "deliveryPolicy", "recurring", "cutoff"],
                    "message": "Cutoff must be within the range of 0 to 2,147,483,647",
                    "code": "INVALID"
                },
                {
                    "field": ["input", "sellingPlansToUpdate", "0", "billingPolicy", "recurring", "minCycles"],
                    "message": "Min cycles must be within the range of 1 to 2,147,483,647",
                    "code": "INVALID"
                },
                {
                    "field": ["input", "sellingPlansToUpdate", "0", "billingPolicy", "recurring", "maxCycles"],
                    "message": "Max cycles must be within the range of 1 to 2,147,483,647",
                    "code": "INVALID"
                }
            ]
        })
    );
}

#[test]
fn selling_plan_group_create_stages_and_reads_back_fixed_and_recurring_pricing_policies() {
    let mut proxy = snapshot_proxy();

    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation MixedPricingPolicyCreate($input: SellingPlanGroupInput!) {
          sellingPlanGroupCreate(input: $input) {
            sellingPlanGroup {
              id
              sellingPlans(first: 5) {
                nodes {
                  id
                  pricingPolicies {
                    __typename
                    ... on SellingPlanFixedPricingPolicy {
                      adjustmentType
                      adjustmentValue {
                        __typename
                        ... on SellingPlanPricingPolicyPercentageValue { percentage }
                      }
                    }
                    ... on SellingPlanRecurringPricingPolicy {
                      adjustmentType
                      afterCycle
                      adjustmentValue {
                        __typename
                        ... on SellingPlanPricingPolicyPercentageValue { percentage }
                      }
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
            "input": {
                "name": "Mixed pricing policy group",
                "options": ["Delivery frequency"],
                "sellingPlansToCreate": [
                    selling_plan_input_with_pricing_policies(
                        "Mixed pricing policy plan",
                        vec!["Monthly"],
                        vec![
                            fixed_percentage_pricing_policy(10.0),
                            recurring_percentage_pricing_policy(5.0, 2),
                        ],
                    )
                ]
            }
        }),
    ));

    assert_eq!(create.status, 200);
    assert_eq!(
        create.body["data"]["sellingPlanGroupCreate"]["userErrors"],
        json!([])
    );
    let group = &create.body["data"]["sellingPlanGroupCreate"]["sellingPlanGroup"];
    let group_id = group["id"].as_str().unwrap().to_string();
    assert_eq!(
        group["sellingPlans"]["nodes"][0]["pricingPolicies"],
        json!([
            {
                "__typename": "SellingPlanFixedPricingPolicy",
                "adjustmentType": "PERCENTAGE",
                "adjustmentValue": {
                    "__typename": "SellingPlanPricingPolicyPercentageValue",
                    "percentage": 10.0
                }
            },
            {
                "__typename": "SellingPlanRecurringPricingPolicy",
                "adjustmentType": "PERCENTAGE",
                "afterCycle": 2,
                "adjustmentValue": {
                    "__typename": "SellingPlanPricingPolicyPercentageValue",
                    "percentage": 5.0
                }
            }
        ])
    );

    let read = proxy.process_request(json_graphql_request(
        r#"
        query ReadMixedPricingPolicies($id: ID!) {
          sellingPlanGroup(id: $id) {
            sellingPlans(first: 5) {
              nodes {
                pricingPolicies {
                  __typename
                  ... on SellingPlanFixedPricingPolicy { adjustmentType }
                  ... on SellingPlanRecurringPricingPolicy { adjustmentType afterCycle }
                }
              }
            }
          }
        }
        "#,
        json!({ "id": group_id }),
    ));

    assert_eq!(
        read.body["data"]["sellingPlanGroup"]["sellingPlans"]["nodes"][0]["pricingPolicies"],
        json!([
            {
                "__typename": "SellingPlanFixedPricingPolicy",
                "adjustmentType": "PERCENTAGE"
            },
            {
                "__typename": "SellingPlanRecurringPricingPolicy",
                "adjustmentType": "PERCENTAGE",
                "afterCycle": 2
            }
        ])
    );
}

#[test]
fn selling_plan_group_summary_matches_shopify_count_pluralization_and_discount_ranges() {
    let mut proxy = snapshot_proxy();
    restore_shop_currency(&mut proxy, "USD");

    let single_percentage = create_selling_plan_group_with_summary(
        &mut proxy,
        json!({
            "name": "Single percentage summary",
            "options": ["Delivery frequency"],
            "sellingPlansToCreate": [
                selling_plan_input_with_policy(
                    "Monthly",
                    vec!["Monthly"],
                    "PERCENTAGE",
                    json!({ "percentage": 12.5 }),
                )
            ]
        }),
    );
    assert_eq!(single_percentage.status, 200);
    assert_eq!(
        single_percentage.body["data"]["sellingPlanGroupCreate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        single_percentage.body["data"]["sellingPlanGroupCreate"]["sellingPlanGroup"]["summary"],
        json!("1 delivery frequency, 12.5% discount")
    );

    let percentage_range = create_selling_plan_group_with_summary(
        &mut proxy,
        json!({
            "name": "Percentage range summary",
            "options": ["Delivery frequency", "Billing cadence"],
            "sellingPlansToCreate": [
                selling_plan_input_with_policy(
                    "Monthly",
                    vec!["Monthly", "Monthly billing"],
                    "PERCENTAGE",
                    json!({ "percentage": 10 }),
                ),
                selling_plan_input_with_policy(
                    "Quarterly",
                    vec!["Quarterly", "Quarterly billing"],
                    "PERCENTAGE",
                    json!({ "percentage": 15.5 }),
                ),
                selling_plan_input_with_policy(
                    "Annual",
                    vec!["Annual", "Annual billing"],
                    "PERCENTAGE",
                    json!({ "percentage": 20 }),
                )
            ]
        }),
    );
    assert_eq!(
        percentage_range.body["data"]["sellingPlanGroupCreate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        percentage_range.body["data"]["sellingPlanGroupCreate"]["sellingPlanGroup"]["summary"],
        json!("3 delivery frequencies, 10-20% discount")
    );

    let fixed_range = create_selling_plan_group_with_summary(
        &mut proxy,
        json!({
            "name": "Fixed range summary",
            "options": ["Delivery frequency"],
            "sellingPlansToCreate": [
                selling_plan_input_with_policy(
                    "Fixed low",
                    vec!["Fixed low"],
                    "FIXED_AMOUNT",
                    json!({ "fixedValue": "5.0" }),
                ),
                selling_plan_input_with_policy(
                    "Fixed high",
                    vec!["Fixed high"],
                    "FIXED_AMOUNT",
                    json!({ "fixedValue": "7.5" }),
                )
            ]
        }),
    );
    assert_eq!(
        fixed_range.body["data"]["sellingPlanGroupCreate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        fixed_range.body["data"]["sellingPlanGroupCreate"]["sellingPlanGroup"]["summary"],
        json!("2 delivery frequencies, $5-$8 discount")
    );
    assert_eq!(
        fixed_range.body["data"]["sellingPlanGroupCreate"]["sellingPlanGroup"]["sellingPlans"]
            ["nodes"][0]["pricingPolicies"][0]["adjustmentValue"],
        json!({
            "__typename": "MoneyV2",
            "amount": "5.0",
            "currencyCode": "USD"
        })
    );

    let mixed_range = create_selling_plan_group_with_summary(
        &mut proxy,
        json!({
            "name": "Mixed range summary",
            "options": ["Delivery frequency"],
            "sellingPlansToCreate": [
                selling_plan_input_with_policy(
                    "Percentage low",
                    vec!["Percentage low"],
                    "PERCENTAGE",
                    json!({ "percentage": 10 }),
                ),
                selling_plan_input_with_policy(
                    "Percentage high",
                    vec!["Percentage high"],
                    "PERCENTAGE",
                    json!({ "percentage": 20 }),
                ),
                selling_plan_input_with_policy(
                    "Fixed low",
                    vec!["Fixed low"],
                    "FIXED_AMOUNT",
                    json!({ "fixedValue": "5.0" }),
                ),
                selling_plan_input_with_policy(
                    "Fixed high",
                    vec!["Fixed high"],
                    "FIXED_AMOUNT",
                    json!({ "fixedValue": "7.5" }),
                )
            ]
        }),
    );
    assert_eq!(
        mixed_range.body["data"]["sellingPlanGroupCreate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        mixed_range.body["data"]["sellingPlanGroupCreate"]["sellingPlanGroup"]["summary"],
        json!("4 delivery frequencies, 10-20%·$5-$8 discount")
    );
}

#[test]
fn selling_plan_group_app_id_is_staged_and_read_after_write() {
    let mut proxy = snapshot_proxy();
    let mut create_input = valid_selling_plan_group_input("App ID group");
    create_input["appId"] = json!("app-id-create");

    let create_with_app_id = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateSellingPlanGroup($input: SellingPlanGroupInput!) {
          sellingPlanGroupCreate(input: $input) {
            sellingPlanGroup { id appId name }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "input": create_input }),
    ));

    assert_eq!(create_with_app_id.status, 200);
    assert_eq!(
        create_with_app_id.body["data"]["sellingPlanGroupCreate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        create_with_app_id.body["data"]["sellingPlanGroupCreate"]["sellingPlanGroup"]["appId"],
        json!("app-id-create")
    );
    let group_id = create_with_app_id.body["data"]["sellingPlanGroupCreate"]["sellingPlanGroup"]
        ["id"]
        .as_str()
        .expect("group id should be staged")
        .to_string();

    let read_created = proxy.process_request(json_graphql_request(
        r#"
        query ReadSellingPlanGroup($id: ID!) {
          sellingPlanGroup(id: $id) { id appId name }
        }
        "#,
        json!({ "id": group_id }),
    ));
    assert_eq!(
        read_created.body["data"]["sellingPlanGroup"]["appId"],
        json!("app-id-create")
    );

    let create_without_app_id = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateSellingPlanGroup($input: SellingPlanGroupInput!) {
          sellingPlanGroupCreate(input: $input) {
            sellingPlanGroup { id appId name }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "input": valid_selling_plan_group_input("No app ID group") }),
    ));
    assert_eq!(
        create_without_app_id.body["data"]["sellingPlanGroupCreate"]["sellingPlanGroup"]["appId"],
        Value::Null
    );

    let update_app_id = proxy.process_request(json_graphql_request(
        r#"
        mutation UpdateSellingPlanGroup($id: ID!, $input: SellingPlanGroupInput!) {
          sellingPlanGroupUpdate(id: $id, input: $input) {
            deletedSellingPlanIds
            sellingPlanGroup { id appId name }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "id": group_id, "input": { "appId": "app-id-update" } }),
    ));
    assert_eq!(
        update_app_id.body["data"]["sellingPlanGroupUpdate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        update_app_id.body["data"]["sellingPlanGroupUpdate"]["sellingPlanGroup"]["appId"],
        json!("app-id-update")
    );

    let read_updated = proxy.process_request(json_graphql_request(
        r#"
        query ReadSellingPlanGroup($id: ID!) {
          sellingPlanGroup(id: $id) { id appId name }
        }
        "#,
        json!({ "id": group_id }),
    ));
    assert_eq!(
        read_updated.body["data"]["sellingPlanGroup"]["appId"],
        json!("app-id-update")
    );

    let clear_app_id = proxy.process_request(json_graphql_request(
        r#"
        mutation ClearSellingPlanGroupAppId($id: ID!, $input: SellingPlanGroupInput!) {
          sellingPlanGroupUpdate(id: $id, input: $input) {
            sellingPlanGroup { id appId name }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "id": group_id, "input": { "appId": null } }),
    ));
    assert_eq!(
        clear_app_id.body["data"]["sellingPlanGroupUpdate"]["sellingPlanGroup"]["appId"],
        Value::Null
    );

    let read_cleared = proxy.process_request(json_graphql_request(
        r#"
        query ReadSellingPlanGroup($id: ID!) {
          sellingPlanGroup(id: $id) { id appId name }
        }
        "#,
        json!({ "id": group_id }),
    ));
    assert_eq!(
        read_cleared.body["data"]["sellingPlanGroup"]["appId"],
        Value::Null
    );
}

#[test]
fn selling_plan_group_membership_is_staged_and_visible_to_reads() {
    let product_id = "gid://shopify/Product/1";
    let mut proxy =
        snapshot_proxy().with_base_products(vec![seeded_product(product_id, "Seed product")]);
    restore_shop_currency(&mut proxy, "CAD");
    let variant = create_legacy_variant(&mut proxy, product_id, "DEFAULT", "1.00");
    let variant_id = variant["id"].as_str().unwrap().to_string();

    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateSellingPlanGroup($input: SellingPlanGroupInput!, $resources: SellingPlanGroupResourceInput!, $productId: ID!, $variantId: ID!) {
          sellingPlanGroupCreate(input: $input, resources: $resources) {
            sellingPlanGroup {
              id
              name
              merchantCode
              productsCount { count precision }
              productVariantsCount { count precision }
              appliesToProduct(productId: $productId)
              appliesToProductVariant(productVariantId: $variantId)
              appliesToProductVariants(productId: $productId)
              products(first: 5) { nodes { id title } }
              productVariants(first: 5) { nodes { id title } }
              sellingPlans(first: 5) { nodes { id name } }
            }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "input": {
                "name": "Subscription group",
                "merchantCode": "subscription-group",
                "options": ["Delivery frequency"],
                "sellingPlansToCreate": [{
                    "name": "Monthly delivery",
                    "options": ["Monthly"],
                    "billingPolicy": { "recurring": { "interval": "MONTH", "intervalCount": 1 } },
                    "deliveryPolicy": { "recurring": { "interval": "MONTH", "intervalCount": 1 } },
                    "pricingPolicies": [{
                        "fixed": {
                            "adjustmentType": "PERCENTAGE",
                            "adjustmentValue": { "percentage": 10 }
                        }
                    }]
                }]
            },
            "resources": { "productIds": [product_id] },
            "productId": product_id,
            "variantId": variant_id
        }),
    ));

    assert_eq!(create.status, 200);
    assert_eq!(
        create.body["data"]["sellingPlanGroupCreate"]["userErrors"],
        json!([])
    );
    let group_id = create.body["data"]["sellingPlanGroupCreate"]["sellingPlanGroup"]["id"]
        .as_str()
        .expect("group id should be staged")
        .to_string();
    assert_eq!(
        create.body["data"]["sellingPlanGroupCreate"]["sellingPlanGroup"]["productsCount"],
        json!({"count": 1, "precision": "EXACT"})
    );
    assert_eq!(
        create.body["data"]["sellingPlanGroupCreate"]["sellingPlanGroup"]["productVariantsCount"],
        json!({"count": 0, "precision": "EXACT"})
    );

    let read = proxy.process_request(json_graphql_request(
        r#"
        query ReadSellingPlanMembership($groupId: ID!, $productId: ID!, $variantId: ID!) {
          sellingPlanGroup(id: $groupId) {
            id
            products(first: 5) {
              nodes {
                id
                title
                priceRangeV2 {
                  minVariantPrice { amount currencyCode }
                  maxVariantPrice { amount currencyCode }
                }
              }
            }
            productVariants(first: 5) { nodes { id title } }
          }
          product(id: $productId) {
            id
            requiresSellingPlan
            sellingPlanGroupsCount { count precision }
            sellingPlanGroups(first: 5) { nodes { id name merchantCode } }
          }
          productVariant(id: $variantId) {
            id
            sellingPlanGroupsCount { count precision }
            sellingPlanGroups(first: 5) { nodes { id name merchantCode } }
          }
        }
        "#,
        json!({
            "groupId": group_id,
            "productId": product_id,
            "variantId": variant_id
        }),
    ));

    assert_eq!(read.status, 200);
    assert_eq!(
        read.body["data"]["sellingPlanGroup"]["products"]["nodes"][0]["id"],
        json!(product_id)
    );
    assert_eq!(
        read.body["data"]["sellingPlanGroup"]["products"]["nodes"][0]["title"],
        json!("Seed product")
    );
    assert_eq!(
        read.body["data"]["sellingPlanGroup"]["products"]["nodes"][0]["priceRangeV2"]
            ["minVariantPrice"]["currencyCode"],
        json!("CAD")
    );
    assert_eq!(
        read.body["data"]["sellingPlanGroup"]["products"]["nodes"][0]["priceRangeV2"]
            ["maxVariantPrice"]["currencyCode"],
        json!("CAD")
    );
    assert_eq!(
        read.body["data"]["product"]["sellingPlanGroupsCount"],
        json!({"count": 1, "precision": "EXACT"})
    );
    assert_eq!(
        read.body["data"]["productVariant"]["sellingPlanGroupsCount"],
        json!({"count": 0, "precision": "EXACT"})
    );
    assert_eq!(
        read.body["data"]["productVariant"]["sellingPlanGroups"]["nodes"][0]["id"],
        create.body["data"]["sellingPlanGroupCreate"]["sellingPlanGroup"]["id"]
    );
}

#[test]
fn selling_plan_group_connections_filter_sort_reverse_and_window() {
    let product_id = "gid://shopify/Product/1";
    let mut proxy =
        snapshot_proxy().with_base_products(vec![seeded_product(product_id, "Window product")]);
    let variant = create_legacy_variant(&mut proxy, product_id, "WINDOW", "1.00");
    let variant_id = variant["id"].as_str().unwrap().to_string();

    let alpha = create_selling_plan_group_with_resources(
        &mut proxy,
        valid_selling_plan_group_input("Alpha group"),
        json!({ "productIds": [product_id] }),
    );
    let beta_input = json!({
        "name": "Beta group",
        "options": ["Delivery frequency"],
        "sellingPlansToCreate": [selling_plan_input_with_policy(
            "Monthly",
            vec!["Monthly"],
            "PERCENTAGE",
            json!({ "percentage": 15.0 })
        )]
    });
    let beta = create_selling_plan_group_with_resources(
        &mut proxy,
        beta_input,
        json!({ "productIds": [product_id] }),
    );
    let gamma = create_selling_plan_group_with_resources(
        &mut proxy,
        valid_selling_plan_group_input("Gamma group"),
        json!({ "productIds": [product_id] }),
    );

    let alpha_id = alpha["id"].as_str().unwrap();
    let beta_id = beta["id"].as_str().unwrap();
    let gamma_id = gamma["id"].as_str().unwrap();

    let update_beta = proxy.process_request(json_graphql_request(
        r#"
        mutation UpdateBetaSellingPlanGroup($id: ID!, $input: SellingPlanGroupInput!) {
          sellingPlanGroupUpdate(id: $id, input: $input) {
            sellingPlanGroup { id name }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "id": beta_id,
            "input": { "description": "Beta was updated for UPDATED_AT sort coverage" }
        }),
    ));
    assert_eq!(
        update_beta.body["data"]["sellingPlanGroupUpdate"]["userErrors"],
        json!([])
    );

    let top_level = proxy.process_request(json_graphql_request(
        r#"
        query SellingPlanGroupsTopLevelWindow($after: String!) {
          defaultId: sellingPlanGroups(first: 3) {
            nodes { id name }
          }
          nameReverse: sellingPlanGroups(first: 2, query: "name:group", sortKey: NAME, reverse: true) {
            nodes { id name }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
          updatedReverse: sellingPlanGroups(first: 1, sortKey: UPDATED_AT, reverse: true) {
            nodes { id name }
          }
          afterWindow: sellingPlanGroups(first: 1, after: $after, query: "name:group", sortKey: NAME, reverse: true) {
            nodes { id name }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
          betaOnly: sellingPlanGroups(first: 5, query: "name:Beta", sortKey: ID) {
            nodes { id name }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
          percentageOnly: sellingPlanGroups(first: 5, query: "percentage_off:15", sortKey: ID) {
            nodes { id name }
          }
          monthly: sellingPlanGroups(first: 5, query: "delivery_frequency:MONTH", sortKey: ID) {
            nodes { id }
          }
          subscriptions: sellingPlanGroups(first: 5, query: "category:SUBSCRIPTION", sortKey: ID) {
            nodes { id }
          }
          unknownFilter: sellingPlanGroups(first: 5, query: "unknown_filter:Beta") {
            nodes { id }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
        }
        "#,
        json!({ "after": beta_id }),
    ));

    assert_eq!(top_level.status, 200);
    assert_eq!(
        top_level.body["data"]["defaultId"]["nodes"],
        json!([
            { "id": alpha_id, "name": "Alpha group" },
            { "id": beta_id, "name": "Beta group" },
            { "id": gamma_id, "name": "Gamma group" }
        ])
    );
    assert_eq!(
        top_level.body["data"]["nameReverse"]["nodes"],
        json!([
            { "id": gamma_id, "name": "Gamma group" },
            { "id": beta_id, "name": "Beta group" }
        ])
    );
    assert_eq!(
        top_level.body["data"]["updatedReverse"]["nodes"],
        json!([{ "id": gamma_id, "name": "Gamma group" }])
    );
    assert_eq!(
        top_level.body["data"]["nameReverse"]["pageInfo"],
        json!({
            "hasNextPage": true,
            "hasPreviousPage": false,
            "startCursor": gamma_id,
            "endCursor": beta_id
        })
    );
    assert_eq!(
        top_level.body["data"]["afterWindow"]["nodes"],
        json!([{ "id": alpha_id, "name": "Alpha group" }])
    );
    assert_eq!(
        top_level.body["data"]["afterWindow"]["pageInfo"],
        json!({
            "hasNextPage": false,
            "hasPreviousPage": true,
            "startCursor": alpha_id,
            "endCursor": alpha_id
        })
    );
    assert_eq!(
        top_level.body["data"]["betaOnly"]["nodes"],
        json!([{ "id": beta_id, "name": "Beta group" }])
    );
    assert_eq!(
        top_level.body["data"]["percentageOnly"]["nodes"],
        json!([{ "id": beta_id, "name": "Beta group" }])
    );
    assert_eq!(
        top_level.body["data"]["monthly"]["nodes"],
        json!([{ "id": alpha_id }, { "id": beta_id }, { "id": gamma_id }])
    );
    assert_eq!(
        top_level.body["data"]["subscriptions"]["nodes"],
        json!([{ "id": alpha_id }, { "id": beta_id }, { "id": gamma_id }])
    );
    assert_eq!(top_level.body["data"]["unknownFilter"]["nodes"], json!([]));

    let nested = proxy.process_request(json_graphql_request(
        r#"
        query NestedSellingPlanGroups($productId: ID!, $variantId: ID!, $after: String!) {
          product(id: $productId) {
            sellingPlanGroupsCount { count precision }
            sellingPlanGroups(first: 1, after: $after, reverse: true) {
              nodes { id name }
              pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
            }
          }
          productVariant(id: $variantId) {
            sellingPlanGroupsCount { count precision }
            sellingPlanGroups(first: 2, reverse: true) {
              nodes { id name }
              pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
            }
          }
        }
        "#,
        json!({ "productId": product_id, "variantId": variant_id, "after": beta_id }),
    ));

    assert_eq!(nested.status, 200);
    assert_eq!(
        nested.body["data"]["product"]["sellingPlanGroupsCount"],
        json!({ "count": 3, "precision": "EXACT" })
    );
    assert_eq!(
        nested.body["data"]["product"]["sellingPlanGroups"]["nodes"],
        json!([{ "id": alpha_id, "name": "Alpha group" }])
    );
    assert_eq!(
        nested.body["data"]["product"]["sellingPlanGroups"]["pageInfo"],
        json!({
            "hasNextPage": false,
            "hasPreviousPage": true,
            "startCursor": alpha_id,
            "endCursor": alpha_id
        })
    );
    assert_eq!(
        nested.body["data"]["productVariant"]["sellingPlanGroupsCount"],
        json!({ "count": 0, "precision": "EXACT" })
    );
    assert_eq!(
        nested.body["data"]["productVariant"]["sellingPlanGroups"]["nodes"],
        json!([
            { "id": gamma_id, "name": "Gamma group" },
            { "id": beta_id, "name": "Beta group" }
        ])
    );
}

#[test]
fn product_and_variant_join_leave_validate_membership_inputs() {
    let product_id = "gid://shopify/Product/1";
    let mut proxy =
        snapshot_proxy().with_base_products(vec![seeded_product(product_id, "Join product")]);
    let variant = create_legacy_variant(&mut proxy, product_id, "DEFAULT", "1.00");
    let variant_id = variant["id"].as_str().unwrap().to_string();

    let setup_group = create_selling_plan_group(
        &mut proxy,
        valid_selling_plan_group_input("Join validation group"),
    );
    assert_eq!(
        setup_group.body["data"]["sellingPlanGroupCreate"]["userErrors"],
        json!([])
    );
    let group_id = setup_group.body["data"]["sellingPlanGroupCreate"]["sellingPlanGroup"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let blank = proxy.process_request(json_graphql_request(
        r#"
        mutation BlankProductJoin($id: ID!, $sellingPlanGroupIds: [ID!]!) {
          productJoinSellingPlanGroups(id: $id, sellingPlanGroupIds: $sellingPlanGroupIds) {
            userErrors { field message code }
          }
        }
        "#,
        json!({ "id": product_id, "sellingPlanGroupIds": [] }),
    ));
    assert_eq!(
        blank.body["data"]["productJoinSellingPlanGroups"]["userErrors"],
        json!([{
            "field": ["sellingPlanGroupIds"],
            "message": "Selling plan group IDs can't be blank",
            "code": "BLANK"
        }])
    );

    let duplicate = proxy.process_request(json_graphql_request(
        r#"
        mutation DuplicateVariantJoin($id: ID!, $sellingPlanGroupIds: [ID!]!) {
          productVariantJoinSellingPlanGroups(id: $id, sellingPlanGroupIds: $sellingPlanGroupIds) {
            userErrors { field message code }
          }
        }
        "#,
        json!({ "id": variant_id, "sellingPlanGroupIds": [group_id, group_id] }),
    ));
    assert_eq!(
        duplicate.body["data"]["productVariantJoinSellingPlanGroups"]["userErrors"],
        json!([{
            "field": ["sellingPlanGroupIds"],
            "message": "Selling plan group IDs contains duplicate values.",
            "code": null
        }])
    );

    let leave_non_member = proxy.process_request(json_graphql_request(
        r#"
        mutation ProductLeaveNonMember($id: ID!, $sellingPlanGroupIds: [ID!]!) {
          productLeaveSellingPlanGroups(id: $id, sellingPlanGroupIds: $sellingPlanGroupIds) {
            userErrors { field message code }
          }
        }
        "#,
        json!({ "id": product_id, "sellingPlanGroupIds": [group_id] }),
    ));
    assert_eq!(
        leave_non_member.body["data"]["productLeaveSellingPlanGroups"]["userErrors"],
        json!([{
            "field": ["sellingPlanGroupIds"],
            "message": "Selling plan group is not a member.",
            "code": null
        }])
    );
}

#[test]
fn product_join_selling_plan_groups_accepts_thirty_two_memberships() {
    let product_id = "gid://shopify/Product/1";
    let mut proxy =
        snapshot_proxy().with_base_products(vec![seeded_product(product_id, "Cap product")]);
    let group_ids = (1..=32)
        .map(|index| {
            let create = create_selling_plan_group(
                &mut proxy,
                valid_selling_plan_group_input(&format!("Cap group {index}")),
            );
            create.body["data"]["sellingPlanGroupCreate"]["sellingPlanGroup"]["id"]
                .as_str()
                .unwrap()
                .to_string()
        })
        .collect::<Vec<_>>();

    let join = proxy.process_request(json_graphql_request(
        r#"
        mutation JoinProduct($id: ID!, $sellingPlanGroupIds: [ID!]!) {
          productJoinSellingPlanGroups(id: $id, sellingPlanGroupIds: $sellingPlanGroupIds) {
            product {
              id
              sellingPlanGroupsCount { count precision }
            }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "id": product_id, "sellingPlanGroupIds": group_ids }),
    ));

    assert_eq!(
        join.body["data"]["productJoinSellingPlanGroups"]["userErrors"],
        json!([])
    );
    assert_eq!(
        join.body["data"]["productJoinSellingPlanGroups"]["product"]["sellingPlanGroupsCount"],
        json!({"count": 32, "precision": "EXACT"})
    );
}

#[test]
fn named_downstream_membership_reads_are_store_backed() {
    let product_id = "gid://shopify/Product/1";
    let mut proxy =
        snapshot_proxy().with_base_products(vec![seeded_product(product_id, "Named read product")]);
    let variant = create_legacy_variant(&mut proxy, product_id, "DEFAULT", "1.00");
    let variant_id = variant["id"].as_str().unwrap().to_string();

    let mut group_input = valid_selling_plan_group_input("Named downstream group");
    group_input
        .as_object_mut()
        .unwrap()
        .insert("merchantCode".to_string(), json!("named-downstream-group"));
    let setup_group = create_selling_plan_group(&mut proxy, group_input);
    let group_id = setup_group.body["data"]["sellingPlanGroupCreate"]["sellingPlanGroup"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let variant_join = proxy.process_request(json_graphql_request(
        r#"
        mutation JoinVariant($id: ID!, $sellingPlanGroupIds: [ID!]!) {
          productVariantJoinSellingPlanGroups(id: $id, sellingPlanGroupIds: $sellingPlanGroupIds) {
            userErrors { field message code }
          }
        }
        "#,
        json!({ "id": variant_id, "sellingPlanGroupIds": [group_id] }),
    ));
    assert_eq!(
        variant_join.body["data"]["productVariantJoinSellingPlanGroups"]["userErrors"],
        json!([])
    );

    let downstream = proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/products/product-relationship-selling-plan-membership-read.graphql"
        ),
        json!({ "productId": product_id, "variantId": variant_id }),
    ));

    assert_eq!(downstream.status, 200);
    assert_eq!(downstream.body["data"]["product"]["id"], json!(product_id));
    assert_eq!(
        downstream.body["data"]["product"]["sellingPlanGroups"]["nodes"][0],
        json!({
            "id": group_id,
            "name": "Named downstream group",
            "merchantCode": "named-downstream-group"
        })
    );
    assert_eq!(
        downstream.body["data"]["product"]["sellingPlanGroupsCount"],
        json!({"count": 0, "precision": "EXACT"})
    );
    assert_eq!(
        downstream.body["data"]["productVariant"]["sellingPlanGroupsCount"],
        json!({"count": 1, "precision": "EXACT"})
    );
}

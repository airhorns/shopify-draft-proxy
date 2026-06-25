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
fn selling_plan_group_summary_matches_shopify_count_pluralization_and_discount_ranges() {
    let mut proxy = snapshot_proxy();

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

    let zero_seed = create_selling_plan_group_with_summary(
        &mut proxy,
        json!({
            "name": "Zero summary seed",
            "options": ["Delivery frequency"],
            "sellingPlansToCreate": [
                selling_plan_input_with_policy(
                    "Monthly",
                    vec!["Monthly"],
                    "PERCENTAGE",
                    json!({ "percentage": 10 }),
                )
            ]
        }),
    );
    let group_id = zero_seed.body["data"]["sellingPlanGroupCreate"]["sellingPlanGroup"]["id"]
        .as_str()
        .expect("group id should be staged")
        .to_string();
    let plan_id = zero_seed.body["data"]["sellingPlanGroupCreate"]["sellingPlanGroup"]
        ["sellingPlans"]["nodes"][0]["id"]
        .as_str()
        .expect("plan id should be staged")
        .to_string();

    let zero_plan_update = proxy.process_request(json_graphql_request(
        r#"
        mutation DeleteLastSellingPlanForSummary($id: ID!, $input: SellingPlanGroupInput!) {
          sellingPlanGroupUpdate(id: $id, input: $input) {
            sellingPlanGroup { id summary sellingPlans(first: 5) { nodes { id } } }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "id": group_id,
            "input": { "sellingPlansToDelete": [plan_id] }
        }),
    ));
    assert_eq!(
        zero_plan_update.body["data"]["sellingPlanGroupUpdate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        zero_plan_update.body["data"]["sellingPlanGroupUpdate"]["sellingPlanGroup"]["summary"],
        json!("")
    );
    assert_eq!(
        zero_plan_update.body["data"]["sellingPlanGroupUpdate"]["sellingPlanGroup"]["sellingPlans"]
            ["nodes"],
        json!([])
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
            products(first: 5) { nodes { id title } }
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
        read.body["data"]["sellingPlanGroup"]["products"]["nodes"][0],
        json!({ "id": product_id, "title": "Seed product" })
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
            "code": "DUPLICATE"
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
            "code": "NOT_A_MEMBER"
        }])
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

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

    let setup_group = proxy.process_request(json_graphql_request(
        r#"
        mutation SetupSellingPlanGroup($input: SellingPlanGroupInput!) {
          sellingPlanGroupCreate(input: $input) {
            sellingPlanGroup { id }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "input": { "name": "Join validation group" } }),
    ));
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

    let setup_group = proxy.process_request(json_graphql_request(
        r#"
        mutation SetupSellingPlanGroup($input: SellingPlanGroupInput!) {
          sellingPlanGroupCreate(input: $input) {
            sellingPlanGroup { id }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "input": {
                "name": "Named downstream group",
                "merchantCode": "named-downstream-group"
            }
        }),
    ));
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

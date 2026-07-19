use super::common::*;
use pretty_assertions::assert_eq;

fn json_string(value: &Value, context: &str) -> String {
    value
        .as_str()
        .unwrap_or_else(|| panic!("{context} should be a string, got {value}"))
        .to_string()
}

fn assert_synthetic_gid(id: &str, resource_type: &str) {
    assert!(
        id.starts_with(&format!("gid://shopify/{resource_type}/")),
        "{id} should be a {resource_type} gid"
    );
    assert!(
        id.contains("shopify-draft-proxy=synthetic"),
        "{id} should be synthetic"
    );
}

fn assert_datetime_string(value: &Value, context: &str) {
    let timestamp = value
        .as_str()
        .unwrap_or_else(|| panic!("{context} should be a string, got {value}"));
    assert!(
        timestamp.contains('T') && timestamp.ends_with('Z'),
        "{context} should be an ISO-8601 DateTime-shaped string, got {timestamp}"
    );
}

fn assert_starts_at_required_error(data: &Value, alias: &str, node_field: &str, input_arg: &str) {
    assert_eq!(data[alias][node_field], json!(null));
    assert_eq!(
        data[alias]["userErrors"],
        json!([{
            "field": [input_arg, "startsAt"],
            "message": "Starts at can't be blank",
            "code": "BLANK",
            "extraInfo": null
        }])
    );
}

#[test]
fn discount_stage_locally_roots_dispatch_by_root_field_not_operation_name_or_alias() {
    let hits = Arc::new(Mutex::new(0usize));
    let hit_counter = Arc::clone(&hits);
    let mut proxy = configured_proxy(
        ReadMode::LiveHybrid,
        Some(shopify_draft_proxy::proxy::UnsupportedMutationMode::Passthrough),
    )
    .with_upstream_transport(move |request| {
        // The discount mutation itself must always stage locally and never
        // passthrough. The one upstream request the create path is allowed to
        // make is the duplicate-code uniqueness read (`codeDiscountNodeByCode`),
        // which decides TAKEN from real store state now that the `/__meta/seed`
        // discount index has been removed.
        assert!(
            request.body.contains("codeDiscountNodeByCode") && !request.body.contains("mutation"),
            "only the duplicate-code uniqueness read may be forwarded, got: {}",
            request.body
        );
        *hit_counter.lock().unwrap() += 1;
        shopify_draft_proxy::proxy::Response {
            status: 200,
            headers: Default::default(),
            body: json!({ "data": { "codeDiscountNodeByCode": null } }),
        }
    });

    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateDiscount($input: DiscountCodeBasicInput!) {
          createdDiscount: discountCodeBasicCreate(basicCodeDiscount: $input) {
            codeDiscountNode {
              id
              codeDiscount {
                __typename
                ... on DiscountCodeBasic {
                  title
                  status
                  discountClasses
                  combinesWith { productDiscounts orderDiscounts shippingDiscounts }
                  codes(first: 1) { nodes { code } }
                }
              }
            }
            userErrors { field message code extraInfo }
          }
        }
        "#,
        json!({ "input": {
            "title": "Normal operation discount",
            "code": "NORMAL1404",
            "startsAt": "2026-04-27T19:31:14Z",
            "combinesWith": { "productDiscounts": false, "orderDiscounts": true, "shippingDiscounts": false },
            "context": { "all": "ALL" },
            "customerGets": { "value": { "percentage": 0.1 }, "items": { "all": true } }
        }}),
    ));

    assert_eq!(create.status, 200);
    // Exactly one upstream call: the duplicate-code uniqueness read-through. The
    // create mutation stages locally (asserted in the transport above).
    assert_eq!(*hits.lock().unwrap(), 1);
    let id = create.body["data"]["createdDiscount"]["codeDiscountNode"]["id"]
        .as_str()
        .expect("discount create should return a staged id")
        .to_string();
    assert!(id.contains("shopify-draft-proxy=synthetic"));
    assert_eq!(
        create.body["data"]["createdDiscount"]["codeDiscountNode"]["codeDiscount"]["title"],
        json!("Normal operation discount")
    );
    assert_eq!(
        create.body["data"]["createdDiscount"]["codeDiscountNode"]["codeDiscount"]["codes"]
            ["nodes"][0]["code"],
        json!("NORMAL1404")
    );
    assert_eq!(
        create.body["data"]["createdDiscount"]["userErrors"],
        json!([])
    );

    let read = proxy.process_request(json_graphql_request(
        r#"
        query ReadDiscount($id: ID!, $code: String!) {
          byId: discountNode(id: $id) { id discount { __typename ... on DiscountCodeBasic { title status } } }
          byCode: codeDiscountNodeByCode(code: $code) { id codeDiscount { __typename ... on DiscountCodeBasic { title status } } }
          activeCount: discountNodesCount(query: "status:active") { count precision }
        }
        "#,
        json!({ "id": id, "code": "NORMAL1404" }),
    ));
    assert_eq!(read.status, 200);
    assert_eq!(
        read.body["data"]["byId"]["discount"]["title"],
        json!("Normal operation discount")
    );
    assert_eq!(
        read.body["data"]["byCode"]["id"],
        read.body["data"]["byId"]["id"]
    );
    assert_eq!(
        read.body["data"]["activeCount"],
        json!({ "count": 1, "precision": "EXACT" })
    );
    assert_eq!(
        proxy.get_log_snapshot()["entries"][0]["rawBody"]
            .as_str()
            .unwrap()
            .contains("mutation CreateDiscount"),
        true
    );
}

fn starts_at_required_variables(starts_at: Option<Value>) -> Value {
    let mut variables = json!({
        "basicCode": {
            "title": "StartsAt required code basic",
            "code": "STARTSAT-BASIC",
            "combinesWith": { "productDiscounts": false, "orderDiscounts": true, "shippingDiscounts": false },
            "context": { "all": "ALL" },
            "customerGets": { "value": { "percentage": 0.1 }, "items": { "all": true } }
        },
        "bxgyCode": {
            "title": "StartsAt required code BXGY",
            "code": "STARTSAT-BXGY",
            "combinesWith": { "productDiscounts": true, "orderDiscounts": false, "shippingDiscounts": false },
            "context": { "all": "ALL" },
            "customerBuys": { "value": { "quantity": "1" }, "items": { "products": { "productsToAdd": ["gid://shopify/Product/10180236017970"] } } },
            "customerGets": { "value": { "discountOnQuantity": { "quantity": "1", "effect": { "percentage": 1 } } }, "items": { "products": { "productsToAdd": ["gid://shopify/Product/10180236017970"] } } }
        },
        "freeShippingCode": {
            "title": "StartsAt required code free shipping",
            "code": "STARTSAT-SHIP",
            "combinesWith": { "productDiscounts": false, "orderDiscounts": true, "shippingDiscounts": false },
            "context": { "all": "ALL" },
            "destination": { "all": true }
        },
        "automaticBasic": {
            "title": "StartsAt required automatic basic",
            "combinesWith": { "productDiscounts": false, "orderDiscounts": true, "shippingDiscounts": false },
            "context": { "all": "ALL" },
            "customerGets": { "value": { "percentage": 0.1 }, "items": { "all": true } }
        },
        "automaticBxgy": {
            "title": "StartsAt required automatic BXGY",
            "combinesWith": { "productDiscounts": true, "orderDiscounts": false, "shippingDiscounts": false },
            "context": { "all": "ALL" },
            "customerBuys": { "value": { "quantity": "1" }, "items": { "products": { "productsToAdd": ["gid://shopify/Product/10180236017970"] } } },
            "customerGets": { "value": { "discountOnQuantity": { "quantity": "1", "effect": { "percentage": 1 } } }, "items": { "products": { "productsToAdd": ["gid://shopify/Product/10180236017970"] } } }
        },
        "automaticFreeShipping": {
            "title": "StartsAt required automatic free shipping",
            "combinesWith": { "productDiscounts": false, "orderDiscounts": true, "shippingDiscounts": false },
            "context": { "all": "ALL" },
            "destination": { "all": true }
        }
    });
    if let Some(starts_at) = starts_at {
        for key in [
            "basicCode",
            "bxgyCode",
            "freeShippingCode",
            "automaticBasic",
            "automaticBxgy",
            "automaticFreeShipping",
        ] {
            variables[key]["startsAt"] = starts_at.clone();
        }
    }
    variables
}

#[test]
fn discount_native_create_requires_starts_at_for_all_roots() {
    let mut proxy = snapshot_proxy();
    let query = r#"
        mutation DiscountStartsAtRequiredValidation(
          $basicCode: DiscountCodeBasicInput!
          $bxgyCode: DiscountCodeBxgyInput!
          $freeShippingCode: DiscountCodeFreeShippingInput!
          $automaticBasic: DiscountAutomaticBasicInput!
          $automaticBxgy: DiscountAutomaticBxgyInput!
          $automaticFreeShipping: DiscountAutomaticFreeShippingInput!
        ) {
          basicCode: discountCodeBasicCreate(basicCodeDiscount: $basicCode) { codeDiscountNode { id } userErrors { field message code extraInfo } }
          bxgyCode: discountCodeBxgyCreate(bxgyCodeDiscount: $bxgyCode) { codeDiscountNode { id } userErrors { field message code extraInfo } }
          freeShippingCode: discountCodeFreeShippingCreate(freeShippingCodeDiscount: $freeShippingCode) { codeDiscountNode { id } userErrors { field message code extraInfo } }
          automaticBasic: discountAutomaticBasicCreate(automaticBasicDiscount: $automaticBasic) { automaticDiscountNode { id } userErrors { field message code extraInfo } }
          automaticBxgy: discountAutomaticBxgyCreate(automaticBxgyDiscount: $automaticBxgy) { automaticDiscountNode { id } userErrors { field message code extraInfo } }
          automaticFreeShipping: discountAutomaticFreeShippingCreate(freeShippingAutomaticDiscount: $automaticFreeShipping) { automaticDiscountNode { id } userErrors { field message code extraInfo } }
        }
    "#;

    for variables in [
        starts_at_required_variables(None),
        starts_at_required_variables(Some(Value::Null)),
    ] {
        let response = proxy.process_request(json_graphql_request(query, variables));
        assert_eq!(response.status, 200);
        let data = &response.body["data"];
        assert_starts_at_required_error(data, "basicCode", "codeDiscountNode", "basicCodeDiscount");
        assert_starts_at_required_error(data, "bxgyCode", "codeDiscountNode", "bxgyCodeDiscount");
        assert_starts_at_required_error(
            data,
            "freeShippingCode",
            "codeDiscountNode",
            "freeShippingCodeDiscount",
        );
        assert_starts_at_required_error(
            data,
            "automaticBasic",
            "automaticDiscountNode",
            "automaticBasicDiscount",
        );
        assert_starts_at_required_error(
            data,
            "automaticBxgy",
            "automaticDiscountNode",
            "automaticBxgyDiscount",
        );
        assert_starts_at_required_error(
            data,
            "automaticFreeShipping",
            "automaticDiscountNode",
            "freeShippingAutomaticDiscount",
        );
    }
}

#[test]
fn discount_native_update_preserves_existing_starts_at_when_omitted() {
    let mut proxy = snapshot_proxy();
    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateDiscount($input: DiscountCodeBasicInput!) {
          discountCodeBasicCreate(basicCodeDiscount: $input) {
            codeDiscountNode { id codeDiscount { __typename ... on DiscountCodeBasic { startsAt } } }
            userErrors { field message code extraInfo }
          }
        }
        "#,
        json!({ "input": {
            "title": "Preserve startsAt",
            "code": "PRESERVE-STARTS-AT",
            "startsAt": "2026-04-27T19:31:14Z",
            "context": { "all": "ALL" },
            "customerGets": { "value": { "percentage": 0.1 }, "items": { "all": true } }
        }}),
    ));
    assert_eq!(
        create.body["data"]["discountCodeBasicCreate"]["userErrors"],
        json!([])
    );
    let id = json_string(
        &create.body["data"]["discountCodeBasicCreate"]["codeDiscountNode"]["id"],
        "created code discount id",
    );

    let update = proxy.process_request(json_graphql_request(
        r#"
        mutation UpdateDiscount($id: ID!, $input: DiscountCodeBasicInput!) {
          discountCodeBasicUpdate(id: $id, basicCodeDiscount: $input) {
            codeDiscountNode { id codeDiscount { __typename ... on DiscountCodeBasic { title startsAt } } }
            userErrors { field message code extraInfo }
          }
        }
        "#,
        json!({ "id": id, "input": {
            "title": "Preserved startsAt renamed",
            "code": "PRESERVE-STARTS-AT",
            "context": { "all": "ALL" },
            "customerGets": { "value": { "percentage": 0.2 }, "items": { "all": true } }
        }}),
    ));
    assert_eq!(
        update.body["data"]["discountCodeBasicUpdate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        update.body["data"]["discountCodeBasicUpdate"]["codeDiscountNode"]["codeDiscount"]["title"],
        json!("Preserved startsAt renamed")
    );
    assert_eq!(
        update.body["data"]["discountCodeBasicUpdate"]["codeDiscountNode"]["codeDiscount"]
            ["startsAt"],
        json!("2026-04-27T19:31:14Z")
    );
}

#[test]
fn discount_code_app_title_validation_matches_shopify() {
    let mut proxy =
        configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport(|request| {
            let body = serde_json::from_str::<Value>(&request.body)
                .expect("upstream function lookup request should parse");
            assert!(
                body["query"]
                    .as_str()
                    .is_some_and(|query| query.contains("ShopifyFunctionByHandle")),
                "expected app discount Function lookup, got {body}"
            );
            Response {
                status: 200,
                headers: Default::default(),
                body: json!({
                    "data": {
                        "shopifyFunctions": {
                            "nodes": [{
                                "id": "gid://shopify/ShopifyFunction/discount-function",
                                "title": "Discount Function",
                                "handle": "discount-function",
                                "apiType": "DISCOUNT",
                                "description": "Local discount function",
                                "appKey": "discount-app-key",
                                "app": {
                                    "id": "gid://shopify/App/discount-app",
                                    "title": "Discount App",
                                    "handle": "discount-app",
                                    "apiKey": "discount-app-key"
                                }
                            }]
                        }
                    }
                }),
            }
        });

    let long_title = "x".repeat(256);
    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation CodeAppTitleCreate(
          $blank: DiscountCodeAppInput!
          $omitted: DiscountCodeAppInput!
          $long: DiscountCodeAppInput!
          $automatic: DiscountAutomaticAppInput!
        ) {
          blank: discountCodeAppCreate(codeAppDiscount: $blank) {
            codeAppDiscount { discountId title }
            userErrors { field message code extraInfo }
          }
          omitted: discountCodeAppCreate(codeAppDiscount: $omitted) {
            codeAppDiscount { discountId title }
            userErrors { field message code extraInfo }
          }
          long: discountCodeAppCreate(codeAppDiscount: $long) {
            codeAppDiscount { discountId title }
            userErrors { field message code extraInfo }
          }
          automatic: discountAutomaticAppCreate(automaticAppDiscount: $automatic) {
            automaticAppDiscount { discountId title }
            userErrors { field message code extraInfo }
          }
        }
        "#,
        json!({
            "blank": {
                "title": " ",
                "code": "APP-BLANK-TITLE",
                "startsAt": "2026-05-05T00:00:00Z",
                "functionHandle": "discount-function",
                "discountClasses": ["ORDER"]
            },
            "omitted": {
                "code": "APP-OMITTED-TITLE",
                "startsAt": "2026-05-05T00:00:00Z",
                "functionHandle": "discount-function",
                "discountClasses": ["ORDER"]
            },
            "long": {
                "title": long_title,
                "code": "APP-LONG-TITLE",
                "startsAt": "2026-05-05T00:00:00Z",
                "functionHandle": "discount-function",
                "discountClasses": ["ORDER"]
            },
            "automatic": {
                "title": "Automatic setup",
                "startsAt": "2026-05-05T00:00:00Z",
                "functionHandle": "discount-function",
                "discountClasses": ["ORDER"]
            }
        }),
    ));
    assert_eq!(create.status, 200);
    assert_eq!(
        create.body["data"]["blank"],
        json!({
            "codeAppDiscount": null,
            "userErrors": [{
                "field": ["codeAppDiscount", "title"],
                "message": "can't be blank",
                "code": "INVALID",
                "extraInfo": null
            }]
        })
    );
    assert_eq!(
        create.body["data"]["omitted"],
        json!({
            "codeAppDiscount": null,
            "userErrors": [{
                "field": ["codeAppDiscount", "title"],
                "message": "Required argument not found.",
                "code": "INVALID",
                "extraInfo": null
            }]
        })
    );
    assert_eq!(
        create.body["data"]["long"],
        json!({
            "codeAppDiscount": null,
            "userErrors": [{
                "field": ["codeAppDiscount", "title"],
                "message": "is too long (maximum is 255 characters)",
                "code": "INVALID",
                "extraInfo": null
            }]
        })
    );
    assert_eq!(create.body["data"]["automatic"]["userErrors"], json!([]));

    let setup = proxy.process_request(json_graphql_request(
        r#"
        mutation CodeAppTitleUpdateSetup($input: DiscountCodeAppInput!) {
          discountCodeAppCreate(codeAppDiscount: $input) {
            codeAppDiscount { discountId title }
            userErrors { field message code extraInfo }
          }
        }
        "#,
        json!({
            "input": {
                "title": "Code app setup",
                "code": "APP-TITLE-SETUP",
                "startsAt": "2026-05-05T00:00:00Z",
                "functionHandle": "discount-function",
                "discountClasses": ["ORDER"]
            }
        }),
    ));
    assert_eq!(setup.status, 200);
    assert_eq!(
        setup.body["data"]["discountCodeAppCreate"]["userErrors"],
        json!([])
    );
    let code_id = json_string(
        &setup.body["data"]["discountCodeAppCreate"]["codeAppDiscount"]["discountId"],
        "code app discount id",
    );
    let automatic_id = json_string(
        &create.body["data"]["automatic"]["automaticAppDiscount"]["discountId"],
        "automatic app discount id",
    );

    let update = proxy.process_request(json_graphql_request(
        r#"
        mutation CodeAppTitleUpdate(
          $codeId: ID!
          $automaticId: ID!
          $blank: DiscountCodeAppInput!
          $omitted: DiscountCodeAppInput!
          $long: DiscountCodeAppInput!
          $automaticBlank: DiscountAutomaticAppInput!
        ) {
          blank: discountCodeAppUpdate(id: $codeId, codeAppDiscount: $blank) {
            codeAppDiscount { discountId title }
            userErrors { field message code extraInfo }
          }
          omitted: discountCodeAppUpdate(id: $codeId, codeAppDiscount: $omitted) {
            codeAppDiscount { discountId title }
            userErrors { field message code extraInfo }
          }
          long: discountCodeAppUpdate(id: $codeId, codeAppDiscount: $long) {
            codeAppDiscount { discountId title }
            userErrors { field message code extraInfo }
          }
          automaticBlank: discountAutomaticAppUpdate(id: $automaticId, automaticAppDiscount: $automaticBlank) {
            automaticAppDiscount { discountId title }
            userErrors { field message code extraInfo }
          }
        }
        "#,
        json!({
            "codeId": code_id,
            "automaticId": automatic_id,
            "blank": {
                "title": "",
                "code": "APP-TITLE-UP-BLANK",
                "startsAt": "2026-05-05T00:00:00Z",
                "functionHandle": "discount-function",
                "discountClasses": ["ORDER"]
            },
            "omitted": {
                "code": "APP-TITLE-UP-OMITTED",
                "startsAt": "2026-05-05T00:00:00Z",
                "functionHandle": "discount-function",
                "discountClasses": ["ORDER"]
            },
            "long": {
                "title": "y".repeat(256),
                "code": "APP-TITLE-UP-LONG",
                "startsAt": "2026-05-05T00:00:00Z",
                "functionHandle": "discount-function",
                "discountClasses": ["ORDER"]
            },
            "automaticBlank": {
                "title": "",
                "startsAt": "2026-05-05T00:00:00Z",
                "functionHandle": "discount-function",
                "discountClasses": ["ORDER"]
            }
        }),
    ));
    assert_eq!(update.status, 200);
    assert_eq!(
        update.body["data"]["blank"],
        json!({
            "codeAppDiscount": null,
            "userErrors": [{
                "field": ["codeAppDiscount", "title"],
                "message": "can't be blank",
                "code": "INVALID",
                "extraInfo": null
            }]
        })
    );
    assert_eq!(update.body["data"]["omitted"]["userErrors"], json!([]));
    assert_eq!(
        update.body["data"]["omitted"]["codeAppDiscount"]["title"],
        json!("Code app setup")
    );
    assert_eq!(
        update.body["data"]["long"],
        json!({
            "codeAppDiscount": null,
            "userErrors": [{
                "field": ["codeAppDiscount", "title"],
                "message": "is too long (maximum is 255 characters)",
                "code": "INVALID",
                "extraInfo": null
            }]
        })
    );
    assert_eq!(
        update.body["data"]["automaticBlank"],
        json!({
            "automaticAppDiscount": null,
            "userErrors": [{
                "field": ["automaticAppDiscount", "title"],
                "message": "Title can't be blank.",
                "code": "INVALID",
                "extraInfo": null
            }]
        })
    );
}

#[test]
fn discount_generic_handler_validates_input_and_handles_lifecycle_by_arguments() {
    let mut proxy = snapshot_proxy();

    let invalid = proxy.process_request(json_graphql_request(
        r#"
        mutation AnyName($input: DiscountCodeBasicInput!) {
          discountCodeBasicCreate(basicCodeDiscount: $input) {
            codeDiscountNode { id }
            userErrors { field message code extraInfo }
          }
        }
        "#,
        json!({ "input": {
            "title": " ",
            "startsAt": "2026-04-27T19:31:14Z",
            "context": { "all": "ALL" },
            "customerSelection": { "all": true },
            "minimumRequirement": {
                "quantity": { "greaterThanOrEqualToQuantity": "1" },
                "subtotal": { "greaterThanOrEqualToSubtotal": "1.00" }
            },
            "customerGets": {
                "value": {
                    "percentage": 1.5
                },
                "items": { "all": true }
            }
        }}),
    ));
    assert_eq!(invalid.status, 200);
    assert_eq!(
        invalid.body["data"]["discountCodeBasicCreate"]["codeDiscountNode"],
        json!(null)
    );
    assert!(
        invalid.body["data"]["discountCodeBasicCreate"]["userErrors"]
            .as_array()
            .unwrap()
            .iter()
            .any(|error| error["field"] == json!(["basicCodeDiscount", "code"]))
    );
    assert!(
        invalid.body["data"]["discountCodeBasicCreate"]["userErrors"]
            .as_array()
            .unwrap()
            .iter()
            .any(|error| error["field"] == json!(["basicCodeDiscount", "context"]))
    );

    let created = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateDiscount($input: DiscountCodeBasicInput!) {
          discountCodeBasicCreate(basicCodeDiscount: $input) {
            codeDiscountNode { id codeDiscount { __typename ... on DiscountCodeBasic { title status endsAt } } }
            userErrors { field message code extraInfo }
          }
        }
        "#,
        json!({ "input": {
            "title": "Lifecycle discount",
            "code": "LIFE1404",
            "startsAt": "2026-04-27T19:31:14Z",
            "context": { "all": "ALL" },
            "customerGets": { "value": { "percentage": 0.1 }, "items": { "all": true } }
        }}),
    ));
    let id = created.body["data"]["discountCodeBasicCreate"]["codeDiscountNode"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(
        created.body["data"]["discountCodeBasicCreate"]["codeDiscountNode"]["codeDiscount"]
            ["status"],
        json!("ACTIVE")
    );

    let deactivated = proxy.process_request(json_graphql_request(
        r#"
        mutation Whatever($id: ID!) {
          discountCodeDeactivate(id: $id) {
            codeDiscountNode { id codeDiscount { __typename ... on DiscountCodeBasic { status endsAt } } }
            userErrors { field message code extraInfo }
          }
        }
        "#,
        json!({ "id": id }),
    ));
    assert_eq!(
        deactivated.body["data"]["discountCodeDeactivate"]["codeDiscountNode"]["codeDiscount"]
            ["status"],
        json!("EXPIRED")
    );

    let deleted = proxy.process_request(json_graphql_request(
        r#"
        mutation DeleteIt($id: ID!) {
          discountCodeDelete(id: $id) { deletedCodeDiscountId userErrors { field message code extraInfo } }
        }
        "#,
        json!({ "id": deactivated.body["data"]["discountCodeDeactivate"]["codeDiscountNode"]["id"].clone() }),
    ));
    assert_eq!(
        deleted.body["data"]["discountCodeDelete"]["userErrors"],
        json!([])
    );

    let missing_activate = proxy.process_request(json_graphql_request(
        r#"
        mutation Missing($id: ID!) {
          discountCodeActivate(id: $id) { codeDiscountNode { id } userErrors { field message code extraInfo } }
        }
        "#,
        json!({ "id": "gid://shopify/DiscountCodeNode/not-found" }),
    ));
    assert_eq!(
        missing_activate.body["data"]["discountCodeActivate"]["userErrors"][0]["field"],
        json!(["id"])
    );
}

fn discount_minimum_requirement_conflict_input(code: Option<&str>) -> Value {
    let mut input = json!({
        "title": "Minimum requirement conflict",
        "startsAt": "2026-04-27T19:31:14Z",
        "context": { "all": "ALL" },
        "customerGets": {
            "value": { "percentage": 0.1 },
            "items": { "all": true }
        },
        "minimumRequirement": {
            "quantity": { "greaterThanOrEqualToQuantity": "2" },
            "subtotal": { "greaterThanOrEqualToSubtotal": "10.00" }
        }
    });
    if let Some(code) = code {
        input
            .as_object_mut()
            .unwrap()
            .insert("code".to_string(), json!(code));
    }
    input
}

fn discount_minimum_requirement_conflict_errors(input_arg: &str) -> Value {
    json!([
        {
            "field": [
                input_arg,
                "minimumRequirement",
                "subtotal",
                "greaterThanOrEqualToSubtotal"
            ],
            "message": "Minimum subtotal cannot be defined when minimum quantity is.",
            "code": "CONFLICT",
            "extraInfo": null
        },
        {
            "field": [
                input_arg,
                "minimumRequirement",
                "quantity",
                "greaterThanOrEqualToQuantity"
            ],
            "message": "Minimum quantity cannot be defined when minimum subtotal is.",
            "code": "CONFLICT",
            "extraInfo": null
        }
    ])
}

fn discount_minimum_requirement_bound_error(
    input_arg: &str,
    requirement: &str,
    value_field: &str,
    message: &str,
) -> Value {
    json!([{
        "field": [input_arg, "minimumRequirement", requirement, value_field],
        "message": message,
        "code": "LESS_THAN",
        "extraInfo": null
    }])
}

#[test]
fn discount_minimum_requirement_conflict_errors_use_concrete_paths() {
    let mut proxy = snapshot_proxy();

    let setup = proxy.process_request(json_graphql_request(
        r#"
        mutation MinimumRequirementConflictSetup(
          $codeInput: DiscountCodeBasicInput!
          $automaticInput: DiscountAutomaticBasicInput!
        ) {
          codeSetup: discountCodeBasicCreate(basicCodeDiscount: $codeInput) {
            codeDiscountNode { id }
            userErrors { field message code extraInfo }
          }
          automaticSetup: discountAutomaticBasicCreate(automaticBasicDiscount: $automaticInput) {
            automaticDiscountNode { id }
            userErrors { field message code extraInfo }
          }
        }
        "#,
        json!({
            "codeInput": {
                "title": "Minimum requirement code setup",
                "code": "MINREQSETUP",
                "startsAt": "2026-04-27T19:31:14Z",
                "context": { "all": "ALL" },
                "customerGets": { "value": { "percentage": 0.1 }, "items": { "all": true } }
            },
            "automaticInput": {
                "title": "Minimum requirement automatic setup",
                "startsAt": "2026-04-27T19:31:14Z",
                "customerGets": { "value": { "percentage": 0.1 }, "items": { "all": true } }
            }
        }),
    ));
    assert_eq!(setup.status, 200);
    assert_eq!(setup.body["data"]["codeSetup"]["userErrors"], json!([]));
    assert_eq!(
        setup.body["data"]["automaticSetup"]["userErrors"],
        json!([])
    );
    let code_id = setup.body["data"]["codeSetup"]["codeDiscountNode"]["id"]
        .as_str()
        .unwrap();
    let automatic_id = setup.body["data"]["automaticSetup"]["automaticDiscountNode"]["id"]
        .as_str()
        .unwrap();

    let invalid = proxy.process_request(json_graphql_request(
        r#"
        mutation MinimumRequirementConflicts(
          $codeId: ID!
          $automaticId: ID!
          $codeCreateInput: DiscountCodeBasicInput!
          $codeUpdateInput: DiscountCodeBasicInput!
          $automaticCreateInput: DiscountAutomaticBasicInput!
          $automaticUpdateInput: DiscountAutomaticBasicInput!
        ) {
          codeCreate: discountCodeBasicCreate(basicCodeDiscount: $codeCreateInput) {
            codeDiscountNode { id }
            userErrors { field message code extraInfo }
          }
          codeUpdate: discountCodeBasicUpdate(id: $codeId, basicCodeDiscount: $codeUpdateInput) {
            codeDiscountNode { id }
            userErrors { field message code extraInfo }
          }
          automaticCreate: discountAutomaticBasicCreate(automaticBasicDiscount: $automaticCreateInput) {
            automaticDiscountNode { id }
            userErrors { field message code extraInfo }
          }
          automaticUpdate: discountAutomaticBasicUpdate(id: $automaticId, automaticBasicDiscount: $automaticUpdateInput) {
            automaticDiscountNode { id }
            userErrors { field message code extraInfo }
          }
        }
        "#,
        json!({
            "codeId": code_id,
            "automaticId": automatic_id,
            "codeCreateInput": discount_minimum_requirement_conflict_input(Some("MINREQCREATE")),
            "codeUpdateInput": discount_minimum_requirement_conflict_input(Some("MINREQUPDATE")),
            "automaticCreateInput": discount_minimum_requirement_conflict_input(None),
            "automaticUpdateInput": discount_minimum_requirement_conflict_input(None)
        }),
    ));
    assert_eq!(invalid.status, 200);

    let basic_errors = discount_minimum_requirement_conflict_errors("basicCodeDiscount");
    let automatic_errors = discount_minimum_requirement_conflict_errors("automaticBasicDiscount");
    assert_eq!(
        invalid.body["data"]["codeCreate"]["codeDiscountNode"],
        json!(null)
    );
    assert_eq!(
        invalid.body["data"]["codeCreate"]["userErrors"],
        basic_errors
    );
    assert_eq!(
        invalid.body["data"]["codeUpdate"]["codeDiscountNode"],
        json!(null)
    );
    assert_eq!(
        invalid.body["data"]["codeUpdate"]["userErrors"],
        basic_errors
    );
    assert_eq!(
        invalid.body["data"]["automaticCreate"]["automaticDiscountNode"],
        json!(null)
    );
    assert_eq!(
        invalid.body["data"]["automaticCreate"]["userErrors"],
        automatic_errors
    );
    assert_eq!(
        invalid.body["data"]["automaticUpdate"]["automaticDiscountNode"],
        json!(null)
    );
    assert_eq!(
        invalid.body["data"]["automaticUpdate"]["userErrors"],
        automatic_errors
    );
}

#[test]
fn discount_minimum_requirement_bounds_use_concrete_paths() {
    let mut proxy = snapshot_proxy();

    let response = proxy.process_request(json_graphql_request(
        r#"
        mutation MinimumRequirementBounds(
          $quantityLimit: DiscountCodeBasicInput!
          $subtotalLimit: DiscountCodeBasicInput!
          $automaticQuantityLimit: DiscountAutomaticBasicInput!
        ) {
          quantityLimit: discountCodeBasicCreate(basicCodeDiscount: $quantityLimit) {
            codeDiscountNode { id }
            userErrors { field message code extraInfo }
          }
          subtotalLimit: discountCodeBasicCreate(basicCodeDiscount: $subtotalLimit) {
            codeDiscountNode { id }
            userErrors { field message code extraInfo }
          }
          automaticQuantityLimit: discountAutomaticBasicCreate(automaticBasicDiscount: $automaticQuantityLimit) {
            automaticDiscountNode { id }
            userErrors { field message code extraInfo }
          }
        }
        "#,
        json!({
            "quantityLimit": {
                "title": "Minimum quantity limit",
                "code": "MINREQQTY",
                "startsAt": "2026-04-27T19:31:14Z",
                "context": { "all": "ALL" },
                "customerGets": { "value": { "percentage": 0.1 }, "items": { "all": true } },
                "minimumRequirement": {
                    "quantity": { "greaterThanOrEqualToQuantity": "9999999999" }
                }
            },
            "subtotalLimit": {
                "title": "Minimum subtotal limit",
                "code": "MINREQSUB",
                "startsAt": "2026-04-27T19:31:14Z",
                "context": { "all": "ALL" },
                "customerGets": { "value": { "percentage": 0.1 }, "items": { "all": true } },
                "minimumRequirement": {
                    "subtotal": {
                        "greaterThanOrEqualToSubtotal": "1000000000000000001.00"
                    }
                }
            },
            "automaticQuantityLimit": {
                "title": "Automatic minimum quantity limit",
                "startsAt": "2026-04-27T19:31:14Z",
                "customerGets": { "value": { "percentage": 0.1 }, "items": { "all": true } },
                "minimumRequirement": {
                    "quantity": { "greaterThanOrEqualToQuantity": "9999999999" }
                }
            }
        }),
    ));
    assert_eq!(response.status, 200);
    assert_eq!(
        response.body["data"]["quantityLimit"]["codeDiscountNode"],
        json!(null)
    );
    assert_eq!(
        response.body["data"]["quantityLimit"]["userErrors"],
        discount_minimum_requirement_bound_error(
            "basicCodeDiscount",
            "quantity",
            "greaterThanOrEqualToQuantity",
            "Minimum quantity must be less than 2147483647"
        )
    );
    assert_eq!(
        response.body["data"]["subtotalLimit"]["codeDiscountNode"],
        json!(null)
    );
    assert_eq!(
        response.body["data"]["subtotalLimit"]["userErrors"],
        discount_minimum_requirement_bound_error(
            "basicCodeDiscount",
            "subtotal",
            "greaterThanOrEqualToSubtotal",
            "Minimum subtotal must be less than 1000000000000000000"
        )
    );
    assert_eq!(
        response.body["data"]["automaticQuantityLimit"]["automaticDiscountNode"],
        json!(null)
    );
    assert_eq!(
        response.body["data"]["automaticQuantityLimit"]["userErrors"],
        discount_minimum_requirement_bound_error(
            "automaticBasicDiscount",
            "quantity",
            "greaterThanOrEqualToQuantity",
            "Minimum quantity must be less than 2147483647"
        )
    );
}

#[test]
fn discount_basic_customer_gets_value_bounds_match_captured_shopify_behavior() {
    let mut proxy = snapshot_proxy();

    let create = r#"
        mutation DiscountValueBounds(
          $percentageHigh: DiscountCodeBasicInput!
          $percentageNegative: DiscountCodeBasicInput!
          $percentageZero: DiscountCodeBasicInput!
          $amountNegative: DiscountCodeBasicInput!
          $amountZero: DiscountCodeBasicInput!
          $amountHigh: DiscountCodeBasicInput!
        ) {
          percentageHigh: discountCodeBasicCreate(basicCodeDiscount: $percentageHigh) { codeDiscountNode { id } userErrors { field message code extraInfo } }
          percentageNegative: discountCodeBasicCreate(basicCodeDiscount: $percentageNegative) { codeDiscountNode { id } userErrors { field message code extraInfo } }
          percentageZero: discountCodeBasicCreate(basicCodeDiscount: $percentageZero) { codeDiscountNode { id } userErrors { field message code extraInfo } }
          amountNegative: discountCodeBasicCreate(basicCodeDiscount: $amountNegative) { codeDiscountNode { id } userErrors { field message code extraInfo } }
          amountZero: discountCodeBasicCreate(basicCodeDiscount: $amountZero) { codeDiscountNode { id } userErrors { field message code extraInfo } }
          amountHigh: discountCodeBasicCreate(basicCodeDiscount: $amountHigh) { codeDiscountNode { id } userErrors { field message code extraInfo } }
        }
    "#;
    let base = json!({
        "startsAt": "2026-04-25T00:00:00Z",
        "combinesWith": { "productDiscounts": false, "orderDiscounts": true, "shippingDiscounts": false },
        "context": { "all": "ALL" },
        "customerGets": { "items": { "all": true } }
    });
    let input = |title: &str, code: &str, value: Value| {
        let mut input = base.clone();
        input["title"] = json!(title);
        input["code"] = json!(code);
        input["customerGets"]["value"] = value;
        input
    };

    let response = proxy.process_request(json_graphql_request(
        create,
        json!({
            "percentageHigh": input("Percentage high", "PCTHIGH1440", json!({ "percentage": 1.5 })),
            "percentageNegative": input("Percentage negative", "PCTNEG1440", json!({ "percentage": -0.1 })),
            "percentageZero": input("Percentage zero", "PCTZERO1440", json!({ "percentage": 0 })),
            "amountNegative": input("Amount negative", "AMTNEG1440", json!({ "discountAmount": { "amount": "-5", "appliesOnEachItem": false } })),
            "amountZero": input("Amount zero", "AMTZERO1440", json!({ "discountAmount": { "amount": "0", "appliesOnEachItem": false } })),
            "amountHigh": input("Amount high", "AMTHIGH1440", json!({ "discountAmount": { "amount": "1000000000000000000", "appliesOnEachItem": false } }))
        }),
    ));

    assert_eq!(
        response.body["data"]["percentageHigh"]["userErrors"],
        json!([{
            "field": ["basicCodeDiscount", "customerGets", "value", "percentage"],
            "message": "Value must be between 0.0 and 1.0",
            "code": "VALUE_OUTSIDE_RANGE",
            "extraInfo": null
        }])
    );
    assert_eq!(
        response.body["data"]["percentageNegative"]["userErrors"],
        response.body["data"]["percentageHigh"]["userErrors"]
    );
    assert!(
        response.body["data"]["percentageZero"]["codeDiscountNode"]["id"]
            .as_str()
            .unwrap()
            .contains("shopify-draft-proxy=synthetic")
    );
    assert_eq!(
        response.body["data"]["percentageZero"]["userErrors"],
        json!([])
    );
    assert_eq!(
        response.body["data"]["amountNegative"]["userErrors"],
        json!([{
            "field": ["basicCodeDiscount", "customerGets", "value", "discountAmount", "amount"],
            "message": "Value must be less than or equal to 0",
            "code": "LESS_THAN_OR_EQUAL_TO",
            "extraInfo": null
        }])
    );
    assert!(
        response.body["data"]["amountZero"]["codeDiscountNode"]["id"]
            .as_str()
            .unwrap()
            .contains("shopify-draft-proxy=synthetic")
    );
    assert_eq!(response.body["data"]["amountZero"]["userErrors"], json!([]));
    assert_eq!(
        response.body["data"]["amountHigh"]["userErrors"],
        json!([{
            "field": ["basicCodeDiscount", "customerGets", "value", "discountAmount", "amount"],
            "message": "Value must be greater than -1000000000000000000",
            "code": "LESS_THAN",
            "extraInfo": null
        }])
    );
}

#[test]
fn discount_automatic_basic_customer_gets_value_bounds_match_captured_shopify_behavior() {
    let mut proxy = snapshot_proxy();

    let setup = proxy.process_request(json_graphql_request(
        r#"
        mutation Setup($input: DiscountAutomaticBasicInput!) {
          discountAutomaticBasicCreate(automaticBasicDiscount: $input) {
            automaticDiscountNode { id }
            userErrors { field message code extraInfo }
          }
        }
        "#,
        json!({ "input": {
            "title": "Automatic shared bounds setup",
            "startsAt": "2026-04-25T00:00:00Z",
            "context": { "all": "ALL" },
            "customerGets": { "value": { "percentage": 0.1 }, "items": { "all": true } }
        }}),
    ));
    assert_eq!(
        setup.body["data"]["discountAutomaticBasicCreate"]["userErrors"],
        json!([])
    );
    let id = setup.body["data"]["discountAutomaticBasicCreate"]["automaticDiscountNode"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let base = json!({
        "startsAt": "2026-04-25T00:00:00Z",
        "combinesWith": { "productDiscounts": false, "orderDiscounts": true, "shippingDiscounts": false },
        "context": { "all": "ALL" },
        "customerGets": { "items": { "all": true } }
    });
    let input = |title: &str, value: Value| {
        let mut input = base.clone();
        input["title"] = json!(title);
        input["customerGets"]["value"] = value;
        input
    };

    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation Create(
          $percentageHigh: DiscountAutomaticBasicInput!
          $percentageNegative: DiscountAutomaticBasicInput!
          $percentageZero: DiscountAutomaticBasicInput!
          $amountNegative: DiscountAutomaticBasicInput!
          $amountZero: DiscountAutomaticBasicInput!
        ) {
          percentageHigh: discountAutomaticBasicCreate(automaticBasicDiscount: $percentageHigh) {
            automaticDiscountNode { id }
            userErrors { field message code extraInfo }
          }
          percentageNegative: discountAutomaticBasicCreate(automaticBasicDiscount: $percentageNegative) {
            automaticDiscountNode { id }
            userErrors { field message code extraInfo }
          }
          percentageZero: discountAutomaticBasicCreate(automaticBasicDiscount: $percentageZero) {
            automaticDiscountNode { id }
            userErrors { field message code extraInfo }
          }
          amountNegative: discountAutomaticBasicCreate(automaticBasicDiscount: $amountNegative) {
            automaticDiscountNode { id }
            userErrors { field message code extraInfo }
          }
          amountZero: discountAutomaticBasicCreate(automaticBasicDiscount: $amountZero) {
            automaticDiscountNode { id }
            userErrors { field message code extraInfo }
          }
        }
        "#,
        json!({
            "percentageHigh": input("Automatic bounds create percentage high", json!({ "percentage": 1.5 })),
            "percentageNegative": input("Automatic bounds create percentage negative", json!({ "percentage": -0.1 })),
            "percentageZero": input("Automatic bounds create percentage zero", json!({ "percentage": 0 })),
            "amountNegative": input("Automatic bounds create amount negative", json!({ "discountAmount": { "amount": "-1", "appliesOnEachItem": false } })),
            "amountZero": input("Automatic bounds create amount zero", json!({ "discountAmount": { "amount": "0", "appliesOnEachItem": false } }))
        }),
    ));

    let update = proxy.process_request(json_graphql_request(
        r#"
        mutation Update(
          $id: ID!
          $percentageHigh: DiscountAutomaticBasicInput!
          $percentageNegative: DiscountAutomaticBasicInput!
          $percentageZero: DiscountAutomaticBasicInput!
          $amountNegative: DiscountAutomaticBasicInput!
          $amountZero: DiscountAutomaticBasicInput!
        ) {
          percentageHigh: discountAutomaticBasicUpdate(id: $id, automaticBasicDiscount: $percentageHigh) {
            automaticDiscountNode { id }
            userErrors { field message code extraInfo }
          }
          percentageNegative: discountAutomaticBasicUpdate(id: $id, automaticBasicDiscount: $percentageNegative) {
            automaticDiscountNode { id }
            userErrors { field message code extraInfo }
          }
          percentageZero: discountAutomaticBasicUpdate(id: $id, automaticBasicDiscount: $percentageZero) {
            automaticDiscountNode { id }
            userErrors { field message code extraInfo }
          }
          amountNegative: discountAutomaticBasicUpdate(id: $id, automaticBasicDiscount: $amountNegative) {
            automaticDiscountNode { id }
            userErrors { field message code extraInfo }
          }
          amountZero: discountAutomaticBasicUpdate(id: $id, automaticBasicDiscount: $amountZero) {
            automaticDiscountNode { id }
            userErrors { field message code extraInfo }
          }
        }
        "#,
        json!({
            "id": id,
            "percentageHigh": input("Automatic bounds update percentage high", json!({ "percentage": 1.5 })),
            "percentageNegative": input("Automatic bounds update percentage negative", json!({ "percentage": -0.1 })),
            "percentageZero": input("Automatic bounds update percentage zero", json!({ "percentage": 0 })),
            "amountNegative": input("Automatic bounds update amount negative", json!({ "discountAmount": { "amount": "-1", "appliesOnEachItem": false } })),
            "amountZero": input("Automatic bounds update amount zero", json!({ "discountAmount": { "amount": "0", "appliesOnEachItem": false } }))
        }),
    ));

    let percentage_error = json!([{
        "field": ["automaticBasicDiscount", "customerGets", "value", "percentage"],
        "message": "Value must be between 0.0 and 1.0",
        "code": "VALUE_OUTSIDE_RANGE",
        "extraInfo": null
    }]);
    let amount_error = json!([{
        "field": ["automaticBasicDiscount", "customerGets", "value", "discountAmount", "amount"],
        "message": "Value must be less than 0",
        "code": "GREATER_THAN",
        "extraInfo": null
    }]);

    for root in ["percentageHigh", "percentageNegative", "percentageZero"] {
        assert_eq!(
            create.body["data"][root]["automaticDiscountNode"],
            json!(null)
        );
        assert_eq!(create.body["data"][root]["userErrors"], percentage_error);
        assert_eq!(
            update.body["data"][root]["automaticDiscountNode"],
            json!(null)
        );
        assert_eq!(update.body["data"][root]["userErrors"], percentage_error);
    }

    for root in ["amountNegative", "amountZero"] {
        assert_eq!(
            create.body["data"][root]["automaticDiscountNode"],
            json!(null)
        );
        assert_eq!(create.body["data"][root]["userErrors"], amount_error);
        assert_eq!(
            update.body["data"][root]["automaticDiscountNode"],
            json!(null)
        );
        assert_eq!(update.body["data"][root]["userErrors"], amount_error);
    }
}

#[test]
fn discount_basic_non_numeric_decimal_variable_fails_before_resolver_execution() {
    let mut proxy = snapshot_proxy();

    let response = proxy.process_request(json_graphql_request(
        r#"
        mutation DiscountValueBoundsNonNumeric($input: DiscountCodeBasicInput!) {
          discountCodeBasicCreate(basicCodeDiscount: $input) {
            codeDiscountNode { id }
            userErrors { field message code extraInfo }
          }
        }
        "#,
        json!({ "input": {
            "title": "Value Bounds NonNumeric",
            "code": "VALUEBOUNDSNAN1440",
            "startsAt": "2026-04-25T00:00:00Z",
            "combinesWith": { "productDiscounts": false, "orderDiscounts": true, "shippingDiscounts": false },
            "context": { "all": "ALL" },
            "customerGets": {
                "value": { "discountAmount": { "amount": "abc", "appliesOnEachItem": false } },
                "items": { "all": true }
            }
        }}),
    ));

    assert_eq!(response.body["data"], Value::Null);
    assert_eq!(
        response.body["errors"][0]["message"],
        json!(
            "Variable $input of type DiscountCodeBasicInput! was provided invalid value for customerGets.value.discountAmount.amount (invalid decimal 'abc')"
        )
    );
    assert_eq!(
        response.body["errors"][0]["extensions"]["code"],
        json!("INVALID_VARIABLE")
    );
    assert_eq!(
        response.body["errors"][0]["extensions"]["problems"],
        json!([{
            "path": ["customerGets", "value", "discountAmount", "amount"],
            "explanation": "invalid decimal 'abc'",
            "message": "invalid decimal 'abc'"
        }])
    );
    assert_eq!(proxy.get_log_snapshot()["entries"], json!([]));
}

#[test]
fn discount_lifecycle_unknown_ids_use_type_specific_not_found_messages() {
    let mut proxy = snapshot_proxy();

    let response = proxy.process_request(json_graphql_request(
        r#"
        mutation DiscountLifecycleUnknowns {
          codeActivate: discountCodeActivate(id: "gid://shopify/DiscountCodeNode/0") {
            codeDiscountNode { id }
            userErrors { field message code extraInfo }
          }
          codeDeactivate: discountCodeDeactivate(id: "gid://shopify/DiscountCodeNode/0") {
            codeDiscountNode { id }
            userErrors { field message code extraInfo }
          }
          codeDelete: discountCodeDelete(id: "gid://shopify/DiscountCodeNode/0") {
            deletedCodeDiscountId
            userErrors { field message code extraInfo }
          }
          automaticActivate: discountAutomaticActivate(id: "gid://shopify/DiscountAutomaticNode/0") {
            automaticDiscountNode { id }
            userErrors { field message code extraInfo }
          }
          automaticDeactivate: discountAutomaticDeactivate(id: "gid://shopify/DiscountAutomaticNode/0") {
            automaticDiscountNode { id }
            userErrors { field message code extraInfo }
          }
          automaticDelete: discountAutomaticDelete(id: "gid://shopify/DiscountAutomaticNode/0") {
            deletedAutomaticDiscountId
            userErrors { field message code extraInfo }
          }
        }
        "#,
        json!({}),
    ));

    assert_eq!(response.status, 200);
    assert_eq!(
        response.body["data"]["codeActivate"]["codeDiscountNode"],
        json!(null)
    );
    assert_eq!(
        response.body["data"]["codeDeactivate"]["codeDiscountNode"],
        json!(null)
    );
    assert_eq!(
        response.body["data"]["codeDelete"]["deletedCodeDiscountId"],
        json!(null)
    );
    assert_eq!(
        response.body["data"]["automaticActivate"]["automaticDiscountNode"],
        json!(null)
    );
    assert_eq!(
        response.body["data"]["automaticDeactivate"]["automaticDiscountNode"],
        json!(null)
    );
    assert_eq!(
        response.body["data"]["automaticDelete"]["deletedAutomaticDiscountId"],
        json!(null)
    );

    for response_key in ["codeActivate", "codeDeactivate", "codeDelete"] {
        assert_eq!(
            response.body["data"][response_key]["userErrors"],
            json!([{ "field": ["id"], "message": "Code discount does not exist.", "code": "INVALID", "extraInfo": null }])
        );
    }
    for response_key in [
        "automaticActivate",
        "automaticDeactivate",
        "automaticDelete",
    ] {
        assert_eq!(
            response.body["data"][response_key]["userErrors"],
            json!([{ "field": ["id"], "message": "Automatic discount does not exist.", "code": "INVALID", "extraInfo": null }])
        );
    }
}

#[test]
fn discount_activate_deactivate_noops_preserve_captured_timestamp_shapes() {
    let mut proxy = snapshot_proxy();

    let setup = proxy.process_request(json_graphql_request(
        r#"
        mutation DiscountNoopTransitionSetup {
          codeActive: discountCodeBasicCreate(basicCodeDiscount: { title: "Noop active code", code: "NOOP-ACTIVE-CODE", startsAt: "2026-04-01T00:00:00Z", context: { all: "ALL" }, customerGets: { value: { percentage: 0.1 }, items: { all: true } } }) {
            codeDiscountNode { id codeDiscount { __typename ... on DiscountCodeBasic { startsAt endsAt status updatedAt } } }
            userErrors { field message code extraInfo }
          }
          codeExpired: discountCodeBasicCreate(basicCodeDiscount: { title: "Noop expired code", code: "NOOP-EXPIRED-CODE", startsAt: "2020-01-01T00:00:00Z", endsAt: "2020-01-02T00:00:00Z", context: { all: "ALL" }, customerGets: { value: { percentage: 0.1 }, items: { all: true } } }) {
            codeDiscountNode { id codeDiscount { __typename ... on DiscountCodeBasic { startsAt endsAt status updatedAt } } }
            userErrors { field message code extraInfo }
          }
          automaticActive: discountAutomaticBasicCreate(automaticBasicDiscount: { title: "Noop active automatic", startsAt: "2026-04-01T00:00:00Z" }) {
            automaticDiscountNode { id automaticDiscount { __typename ... on DiscountAutomaticBasic { startsAt endsAt status updatedAt } } }
            userErrors { field message code extraInfo }
          }
          automaticExpired: discountAutomaticBasicCreate(automaticBasicDiscount: { title: "Noop expired automatic", startsAt: "2020-01-01T00:00:00Z", endsAt: "2020-01-02T00:00:00Z" }) {
            automaticDiscountNode { id automaticDiscount { __typename ... on DiscountAutomaticBasic { startsAt endsAt status updatedAt } } }
            userErrors { field message code extraInfo }
          }
        }
        "#,
        json!({}),
    ));
    for response_key in [
        "codeActive",
        "codeExpired",
        "automaticActive",
        "automaticExpired",
    ] {
        assert_eq!(setup.body["data"][response_key]["userErrors"], json!([]));
    }
    let code_active_id = json_string(
        &setup.body["data"]["codeActive"]["codeDiscountNode"]["id"],
        "active code discount id",
    );
    let code_expired_id = json_string(
        &setup.body["data"]["codeExpired"]["codeDiscountNode"]["id"],
        "expired code discount id",
    );
    let automatic_active_id = json_string(
        &setup.body["data"]["automaticActive"]["automaticDiscountNode"]["id"],
        "active automatic discount id",
    );
    let automatic_expired_id = json_string(
        &setup.body["data"]["automaticExpired"]["automaticDiscountNode"]["id"],
        "expired automatic discount id",
    );
    for id in [&code_active_id, &code_expired_id] {
        assert_synthetic_gid(id, "DiscountCodeNode");
    }
    for id in [&automatic_active_id, &automatic_expired_id] {
        assert_synthetic_gid(id, "DiscountAutomaticNode");
    }

    let code_activate = proxy.process_request(json_graphql_request(
        r#"
        mutation DiscountCodeActivateNoopIdempotence($id: ID!) {
          discountCodeActivate(id: $id) {
            codeDiscountNode {
              id
              codeDiscount { __typename ... on DiscountCodeBasic { startsAt endsAt status updatedAt } }
            }
            userErrors { field message code extraInfo }
          }
        }
        "#,
        json!({ "id": code_active_id }),
    ));
    assert_eq!(
        code_activate.body["data"]["discountCodeActivate"]["codeDiscountNode"]["codeDiscount"],
        setup.body["data"]["codeActive"]["codeDiscountNode"]["codeDiscount"]
    );
    assert_eq!(
        code_activate.body["data"]["discountCodeActivate"]["userErrors"],
        json!([])
    );

    let code_deactivate = proxy.process_request(json_graphql_request(
        r#"
        mutation DiscountCodeDeactivateNoopIdempotence($id: ID!) {
          discountCodeDeactivate(id: $id) {
            codeDiscountNode {
              id
              codeDiscount { __typename ... on DiscountCodeBasic { startsAt endsAt status updatedAt } }
            }
            userErrors { field message code extraInfo }
          }
        }
        "#,
        json!({ "id": code_expired_id }),
    ));
    assert_eq!(
        code_deactivate.body["data"]["discountCodeDeactivate"]["codeDiscountNode"]["codeDiscount"],
        setup.body["data"]["codeExpired"]["codeDiscountNode"]["codeDiscount"]
    );
    assert_eq!(
        code_deactivate.body["data"]["discountCodeDeactivate"]["userErrors"],
        json!([])
    );

    let automatic_activate = proxy.process_request(json_graphql_request(
        r#"
        mutation DiscountAutomaticActivateNoopIdempotence($id: ID!) {
          discountAutomaticActivate(id: $id) {
            automaticDiscountNode {
              id
              automaticDiscount { __typename ... on DiscountAutomaticBasic { startsAt endsAt status updatedAt } }
            }
            userErrors { field message code extraInfo }
          }
        }
        "#,
        json!({ "id": automatic_active_id }),
    ));
    assert_eq!(
        automatic_activate.body["data"]["discountAutomaticActivate"]["automaticDiscountNode"]
            ["automaticDiscount"],
        setup.body["data"]["automaticActive"]["automaticDiscountNode"]["automaticDiscount"]
    );
    assert_eq!(
        automatic_activate.body["data"]["discountAutomaticActivate"]["userErrors"],
        json!([])
    );

    let automatic_deactivate = proxy.process_request(json_graphql_request(
        r#"
        mutation DiscountAutomaticDeactivateNoopIdempotence($id: ID!) {
          discountAutomaticDeactivate(id: $id) {
            automaticDiscountNode {
              id
              automaticDiscount { __typename ... on DiscountAutomaticBasic { startsAt endsAt status updatedAt } }
            }
            userErrors { field message code extraInfo }
          }
        }
        "#,
        json!({ "id": automatic_expired_id }),
    ));
    assert_eq!(
        automatic_deactivate.body["data"]["discountAutomaticDeactivate"]["automaticDiscountNode"]
            ["automaticDiscount"],
        setup.body["data"]["automaticExpired"]["automaticDiscountNode"]["automaticDiscount"]
    );
    assert_eq!(
        automatic_deactivate.body["data"]["discountAutomaticDeactivate"]["userErrors"],
        json!([])
    );
}

#[test]
fn discount_automatic_basic_buyer_context_lifecycle_stages_selected_context_reads() {
    let mut proxy = snapshot_proxy();

    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation DiscountAutomaticBasicBuyerContextCreate($input: DiscountAutomaticBasicInput!) {
          discountAutomaticBasicCreate(automaticBasicDiscount: $input) {
            automaticDiscountNode { id automaticDiscount { __typename ... on DiscountAutomaticBasic { title status context { __typename ... on DiscountCustomers { customers { __typename id displayName } } ... on DiscountCustomerSegments { segments { __typename id name } } } } } }
            userErrors { field message code extraInfo }
          }
        }
        "#,
        json!({ "input": { "title": "HAR-390 automatic customer context 1777346878525", "startsAt": "2026-04-25T00:00:00Z", "context": { "customers": { "add": ["gid://shopify/Customer/10548596015410"] } } } }),
    ));
    let discount_id = json_string(
        &create.body["data"]["discountAutomaticBasicCreate"]["automaticDiscountNode"]["id"],
        "automatic discount id",
    );
    assert_synthetic_gid(&discount_id, "DiscountAutomaticNode");
    assert_eq!(
        create.body["data"]["discountAutomaticBasicCreate"]["automaticDiscountNode"]
            ["automaticDiscount"],
        json!({
            "__typename": "DiscountAutomaticBasic",
            "title": "HAR-390 automatic customer context 1777346878525",
            "status": "ACTIVE",
            "context": {
                "__typename": "DiscountCustomers",
                "customers": [{
                    "__typename": "Customer",
                    "id": "gid://shopify/Customer/10548596015410"
                }]
            }
        })
    );

    let update = proxy.process_request(json_graphql_request(
        r#"
        mutation DiscountAutomaticBasicBuyerContextUpdate($id: ID!, $input: DiscountAutomaticBasicInput!) {
          discountAutomaticBasicUpdate(id: $id, automaticBasicDiscount: $input) {
            automaticDiscountNode { id automaticDiscount { __typename ... on DiscountAutomaticBasic { title status context { __typename ... on DiscountCustomerSegments { segments { __typename id name } } } } } }
            userErrors { field message code extraInfo }
          }
        }
        "#,
        json!({ "id": discount_id.clone(), "input": { "title": "HAR-390 automatic segment context 1777346878525", "context": { "customerSegments": { "add": ["gid://shopify/Segment/647746715954"] } } } }),
    ));
    assert_eq!(
        update.body["data"]["discountAutomaticBasicUpdate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        update.body["data"]["discountAutomaticBasicUpdate"]["automaticDiscountNode"]
            ["automaticDiscount"]["context"],
        json!({
            "__typename": "DiscountCustomerSegments",
            "segments": [{
                "__typename": "Segment",
                "id": "gid://shopify/Segment/647746715954"
            }]
        })
    );

    let read = proxy.process_request(json_graphql_request(
        r#"
        query DiscountAutomaticBasicBuyerContextRead($id: ID!) {
          automaticDiscountNode(id: $id) {
            id
            automaticDiscount { __typename ... on DiscountAutomaticBasic { title context { __typename ... on DiscountCustomerSegments { segments { __typename id name } } } } }
          }
        }
        "#,
        json!({ "id": discount_id.clone() }),
    ));
    assert_eq!(
        read.body["data"]["automaticDiscountNode"]["automaticDiscount"],
        json!({
            "__typename": "DiscountAutomaticBasic",
            "title": "HAR-390 automatic segment context 1777346878525",
            "context": {
                "__typename": "DiscountCustomerSegments",
                "segments": [{
                    "__typename": "Segment",
                    "id": "gid://shopify/Segment/647746715954"
                }]
            }
        })
    );

    let delete = proxy.process_request(json_graphql_request(
        r#"
        mutation DiscountAutomaticBasicBuyerContextDelete($id: ID!) {
          discountAutomaticDelete(id: $id) { deletedAutomaticDiscountId userErrors { field message code extraInfo } }
        }
        "#,
        json!({ "id": discount_id.clone() }),
    ));
    assert_eq!(
        delete.body["data"]["discountAutomaticDelete"],
        json!({ "deletedAutomaticDiscountId": discount_id, "userErrors": [] })
    );
}

#[test]
fn discount_automatic_nodes_read_returns_empty_connection_without_staged_discounts() {
    let mut proxy = snapshot_proxy();

    let response = proxy.process_request(json_graphql_request(
        r#"
        query DiscountAutomaticNodesRead($first: Int!, $query: String) {
          automaticDiscountNodes(first: $first, query: $query) {
            nodes {
              id
              automaticDiscount {
                __typename
                ... on DiscountAutomaticBasic { title status summary startsAt endsAt createdAt updatedAt asyncUsageCount discountClasses combinesWith { productDiscounts orderDiscounts shippingDiscounts } }
                ... on DiscountAutomaticBxgy { title status summary startsAt endsAt createdAt updatedAt asyncUsageCount discountClasses combinesWith { productDiscounts orderDiscounts shippingDiscounts } }
              }
            }
            edges { cursor node { id automaticDiscount { __typename ... on DiscountAutomaticBasic { title status } ... on DiscountAutomaticBxgy { title status } } } }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
        }
        "#,
        json!({ "first": 5, "query": null }),
    ));

    assert_eq!(response.status, 200);
    assert_eq!(
        response.body["data"]["automaticDiscountNodes"]["nodes"],
        json!([])
    );
    assert_eq!(
        response.body["data"]["automaticDiscountNodes"]["edges"],
        json!([])
    );
    assert_eq!(
        response.body["data"]["automaticDiscountNodes"]["pageInfo"],
        json!({
            "hasNextPage": false,
            "hasPreviousPage": false,
            "startCursor": null,
            "endCursor": null
        })
    );
}

#[test]
fn functions_metadata_local_staging_updates_deletes_and_reads_validation_cart_and_function_roots() {
    let mut proxy = snapshot_proxy();
    let stage = r#"mutation StageFunctionMetadata($validation: ValidationCreateInput!, $cartFunctionHandle: String!, $cartBlockOnFailure: Boolean!, $ready: Boolean!) { validationCreate(validation: $validation) { validation { id title enable blockOnFailure functionHandle createdAt updatedAt shopifyFunction { id title handle apiType } } userErrors { field message code } } cartTransformCreate(functionHandle: $cartFunctionHandle, blockOnFailure: $cartBlockOnFailure) { cartTransform { id blockOnFailure functionId } userErrors { field message code } } taxAppConfigure(ready: $ready) { taxAppConfiguration { id ready state updatedAt } userErrors { field message code } } }"#;
    let missing_validation_delete = r#"mutation DeleteFunctionValidation($id: ID!) { validationDelete(id: $id) { deletedId userErrors { field message code } } }"#;
    let missing_validation_response = proxy.process_request(json_graphql_request(
        missing_validation_delete,
        json!({ "id": "gid://shopify/Validation/999999999999" }),
    ));
    assert_eq!(
        missing_validation_response.body["data"]["validationDelete"],
        json!({
            "deletedId": null,
            "userErrors": [{ "field": ["id"], "message": "Extension not found.", "code": "NOT_FOUND" }]
        })
    );

    let missing_cart_delete = r#"mutation DeleteFunctionCartTransform($id: ID!) { cartTransformDelete(id: $id) { deletedId userErrors { field message code } } }"#;
    let missing_cart_response = proxy.process_request(json_graphql_request(
        missing_cart_delete,
        json!({ "id": "gid://shopify/CartTransform/999999999999" }),
    ));
    assert_eq!(
        missing_cart_response.body["data"]["cartTransformDelete"],
        json!({
            "deletedId": null,
            "userErrors": [{ "field": ["id"], "message": "Could not find cart transform with id: gid://shopify/CartTransform/999999999999", "code": "NOT_FOUND" }]
        })
    );

    let stage_response = proxy.process_request(json_graphql_request(stage, json!({
        "validation": { "functionHandle": "validation-local", "title": "Local validation", "enable": true, "blockOnFailure": true },
        "cartFunctionHandle": "cart-transform-local",
        "cartBlockOnFailure": true,
        "ready": true
    })));
    let validation_id = stage_response.body["data"]["validationCreate"]["validation"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let cart_transform_id = stage_response.body["data"]["cartTransformCreate"]["cartTransform"]
        ["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(
        stage_response.body["data"]["validationCreate"]["validation"]["shopifyFunction"],
        json!({
            "id": "gid://shopify/ShopifyFunction/validation-local",
            "title": "Validation Local",
            "handle": "validation-local",
            "apiType": "VALIDATION"
        })
    );
    assert_eq!(
        stage_response.body["data"]["cartTransformCreate"]["cartTransform"],
        json!({
            "id": "gid://shopify/CartTransform/3",
            "blockOnFailure": true,
            "functionId": "gid://shopify/ShopifyFunction/cart-transform-local"
        })
    );

    let update = r#"mutation UpdateFunctionValidation($id: ID!, $validation: ValidationUpdateInput!) { validationUpdate(id: $id, validation: $validation) { validation { id title enable blockOnFailure functionHandle updatedAt } userErrors { field message code } } }"#;
    let update_response = proxy.process_request(json_graphql_request(update, json!({
        "id": validation_id,
        "validation": { "title": "Updated validation", "enable": false, "blockOnFailure": false }
    })));
    assert_eq!(
        update_response.body["data"]["validationUpdate"]["validation"],
        json!({
            "id": "gid://shopify/Validation/2",
            "title": "Updated validation",
            "enable": false,
            "blockOnFailure": false,
            "functionHandle": "validation-local",
            "updatedAt": "2024-01-01T00:00:05.000Z"
        })
    );

    let read = r#"query ReadFunctionMetadata($validationId: ID!) { validation(id: $validationId) { id title enable blockOnFailure functionHandle shopifyFunction { id title handle apiType } } validations(first: 5) { nodes { id title enable blockOnFailure } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } cartTransforms(first: 5) { nodes { id blockOnFailure functionId } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } validationFunctions: shopifyFunctions(first: 5, apiType: VALIDATION) { nodes { id title handle apiType } } cartFunctions: shopifyFunctions(first: 5, apiType: CART_TRANSFORM) { nodes { id title handle apiType } } cartFunction: shopifyFunction(id: "gid://shopify/ShopifyFunction/cart-transform-local") { id title handle apiType } }"#;
    let read_response = proxy.process_request(json_graphql_request(
        read,
        json!({ "validationId": validation_id }),
    ));
    assert_eq!(
        read_response.body["data"]["validation"]["title"],
        json!("Updated validation")
    );
    assert_eq!(
        read_response.body["data"]["validations"]["nodes"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        read_response.body["data"]["cartTransforms"]["nodes"][0]["id"],
        json!(cart_transform_id)
    );
    assert_eq!(
        read_response.body["data"]["validationFunctions"]["nodes"][0]["handle"],
        json!("validation-local")
    );
    assert_eq!(
        read_response.body["data"]["cartFunctions"]["nodes"][0]["handle"],
        json!("cart-transform-local")
    );
    assert_eq!(
        read_response.body["data"]["cartFunction"]["apiType"],
        json!("CART_TRANSFORM")
    );

    let node_read = r#"query CartTransformNodeRead($id: ID!) { node(id: $id) { ... on CartTransform { id blockOnFailure functionId } } }"#;
    let node_response = proxy.process_request(json_graphql_request(
        node_read,
        json!({ "id": cart_transform_id }),
    ));
    assert_eq!(
        node_response.body["data"]["node"],
        read_response.body["data"]["cartTransforms"]["nodes"][0]
    );

    let delete_validation = r#"mutation DeleteFunctionValidation($id: ID!) { validationDelete(id: $id) { deletedId userErrors { field message code } } }"#;
    let validation_delete_response = proxy.process_request(json_graphql_request(
        delete_validation,
        json!({ "id": validation_id }),
    ));
    assert_eq!(
        validation_delete_response.body["data"]["validationDelete"],
        json!({ "deletedId": "gid://shopify/Validation/2", "userErrors": [] })
    );

    let delete_cart_transform = r#"mutation DeleteFunctionCartTransform($id: ID!) { cartTransformDelete(id: $id) { deletedId userErrors { field message code } } }"#;
    let cart_delete_response = proxy.process_request(json_graphql_request(
        delete_cart_transform,
        json!({ "id": cart_transform_id }),
    ));
    assert_eq!(
        cart_delete_response.body["data"]["cartTransformDelete"],
        json!({ "deletedId": "gid://shopify/CartTransform/3", "userErrors": [] })
    );

    let empty_read = r#"query ReadDeletedFunctionMetadata($validationId: ID!) { validation(id: $validationId) { id } validations(first: 5) { nodes { id } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } cartTransforms(first: 5) { nodes { id } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } }"#;
    let empty_response = proxy.process_request(json_graphql_request(
        empty_read,
        json!({ "validationId": "gid://shopify/Validation/2" }),
    ));
    assert_eq!(empty_response.body["data"]["validation"], Value::Null);
    assert_eq!(
        empty_response.body["data"]["validations"]["nodes"],
        json!([])
    );
    assert_eq!(
        empty_response.body["data"]["cartTransforms"]["nodes"],
        json!([])
    );
}

#[test]
fn functions_owner_metadata_stages_validation_cart_tax_and_downstream_reads() {
    let mut proxy = snapshot_proxy();

    let stage = proxy.process_request(json_graphql_request(
        r#"
        mutation StageOwnedFunctionMetadata($validation: ValidationCreateInput!, $cartFunctionHandle: String!, $cartBlockOnFailure: Boolean!, $ready: Boolean!) {
          validationCreate(validation: $validation) { validation { id title enable blockOnFailure functionId functionHandle createdAt updatedAt shopifyFunction { id title handle apiType description appKey app { __typename id title handle apiKey } } } userErrors { field message code } }
          cartTransformCreate(functionHandle: $cartFunctionHandle, blockOnFailure: $cartBlockOnFailure) { cartTransform { id blockOnFailure functionId } userErrors { field message code } }
          taxAppConfigure(ready: $ready) { taxAppConfiguration { id ready state updatedAt } userErrors { field message code } }
        }
        "#,
        json!({
            "validation": { "functionId": "gid://shopify/ShopifyFunction/validation-owned", "title": "Owned validation", "enable": true, "blockOnFailure": true },
            "cartFunctionHandle": "cart-owned",
            "cartBlockOnFailure": true,
            "ready": true
        }),
    ));
    assert_eq!(
        stage.body["data"]["validationCreate"]["validation"]["id"],
        json!("gid://shopify/Validation/2")
    );
    assert_eq!(
        stage.body["data"]["validationCreate"]["validation"]["shopifyFunction"]["app"]["apiKey"],
        json!("validation-app-key")
    );
    assert_eq!(
        stage.body["data"]["cartTransformCreate"]["cartTransform"]["functionId"],
        json!("gid://shopify/ShopifyFunction/cart-owned")
    );
    assert_eq!(
        stage.body["data"]["taxAppConfigure"]["taxAppConfiguration"]["state"],
        json!("READY")
    );

    let update = proxy.process_request(json_graphql_request(
        r#"
        mutation UpdateOwnedFunctionValidation($id: ID!, $validation: ValidationUpdateInput!) {
          validationUpdate(id: $id, validation: $validation) { validation { id title enable blockOnFailure functionId functionHandle updatedAt shopifyFunction { id handle appKey app { title apiKey } } } userErrors { field message code } }
        }
        "#,
        json!({ "id": "gid://shopify/Validation/2", "validation": { "title": "Owned validation renamed" } }),
    ));
    assert_eq!(
        update.body["data"]["validationUpdate"]["validation"]["title"],
        json!("Owned validation renamed")
    );
    assert_eq!(
        update.body["data"]["validationUpdate"]["validation"]["enable"],
        json!(false)
    );
    assert_eq!(
        update.body["data"]["validationUpdate"]["validation"]["shopifyFunction"]["app"]["apiKey"],
        json!("validation-app-key")
    );

    let read = proxy.process_request(json_graphql_request(
        r#"
        query ReadOwnedFunctionMetadata($validationId: ID!) {
          validation(id: $validationId) { id title enable blockOnFailure functionId functionHandle shopifyFunction { id title handle apiType description appKey app { __typename id title handle apiKey } } }
          validationFunctions: shopifyFunctions(first: 5, apiType: VALIDATION) { nodes { id title handle apiType appKey app { title apiKey } } }
          cartFunction: shopifyFunction(id: "gid://shopify/ShopifyFunction/cart-owned") { id title handle apiType appKey app { __typename title apiKey } }
        }
        "#,
        json!({ "validationId": "gid://shopify/Validation/2" }),
    ));
    assert_eq!(
        read.body["data"]["validation"]["title"],
        json!("Owned validation renamed")
    );
    assert_eq!(
        read.body["data"]["validationFunctions"]["nodes"][0]["app"]["apiKey"],
        json!("validation-app-key")
    );
    assert_eq!(
        read.body["data"]["cartFunction"]["app"]["apiKey"],
        json!("cart-app-key")
    );
}

#[test]
fn functions_validation_create_errors_return_null_and_do_not_stage_records() {
    let mut proxy = snapshot_proxy();

    let response = proxy.process_request(json_graphql_request(
        r#"
        mutation FunctionsValidationCreateErrorShape(
          $unknownFunctionId: String!
          $cartFunctionId: String!
          $cartFunctionHandle: String!
        ) {
          unknownFunction: validationCreate(validation: { functionId: $unknownFunctionId, title: "Unknown" }) {
            validation { id }
            userErrors { code field message }
          }
          apiMismatch: validationCreate(validation: { functionId: $cartFunctionId, title: "Wrong API" }) {
            validation { id }
            userErrors { code field message }
          }
          missingIdentifier: validationCreate(validation: {}) {
            validation { id }
            userErrors { code field message }
          }
          multipleIdentifiers: validationCreate(validation: { functionId: $cartFunctionId, functionHandle: $cartFunctionHandle }) {
            validation { id }
            userErrors { code field message }
          }
        }
        "#,
        json!({
            "unknownFunctionId": "01900000-0000-7000-8000-000000000000",
            "cartFunctionId": "019dd44b-127f-724b-a49c-70fc98ff4d72",
            "cartFunctionHandle": "conformance-cart-transform"
        }),
    ));

    assert_eq!(
        response.body["data"],
        json!({
            "unknownFunction": {
                "validation": null,
                "userErrors": [{ "code": "NOT_FOUND", "field": ["validation", "functionId"], "message": "Extension not found." }]
            },
            "apiMismatch": {
                "validation": null,
                "userErrors": [{ "code": "FUNCTION_DOES_NOT_IMPLEMENT", "field": ["validation", "functionId"], "message": "Unexpected Function API. The provided function must implement one of the following extension targets: [%{targets}]." }]
            },
            "missingIdentifier": {
                "validation": null,
                "userErrors": [{ "code": "MISSING_FUNCTION_IDENTIFIER", "field": ["validation", "functionHandle"], "message": "Either function_id or function_handle must be provided." }]
            },
            "multipleIdentifiers": {
                "validation": null,
                "userErrors": [{ "code": "MULTIPLE_FUNCTION_IDENTIFIERS", "field": ["validation"], "message": "Only one of function_id or function_handle can be provided, not both." }]
            }
        })
    );

    let read = proxy.process_request(json_graphql_request(
        r#"query FunctionsValidationCreateErrorRead { validations(first: 5) { nodes { id } } }"#,
        json!({}),
    ));
    assert_eq!(read.body["data"]["validations"]["nodes"], json!([]));
}

#[test]
fn functions_validation_max_cap_update_defaults_and_metafield_rejection_preserve_state() {
    let mut proxy = snapshot_proxy();

    let mut stage = String::from("mutation ValidationCapAndDefaultsStage {");
    stage.push_str(
        r#" subject: validationCreate(validation: { functionHandle: "validation-alpha", title: "Subject", enable: false, blockOnFailure: true }) { validation { id enable blockOnFailure title } userErrors { field message code } }"#,
    );
    for index in 1..=25 {
        stage.push_str(&format!(
            r#" active{index}: validationCreate(validation: {{ functionHandle: "validation-alpha", title: "Active {index}", enable: true }}) {{ validation {{ id enable blockOnFailure }} userErrors {{ field message code }} }}"#
        ));
    }
    stage.push_str(
        r#" maxActive: validationCreate(validation: { functionHandle: "validation-alpha", title: "Max", enable: true }) { validation { id } userErrors { field message code } } }"#,
    );

    let stage_response = proxy.process_request(json_graphql_request(&stage, json!({})));
    assert_eq!(
        stage_response.body["data"]["maxActive"],
        json!({
            "validation": null,
            "userErrors": [{ "field": [], "message": "Cannot have more than 25 active validation functions.", "code": "MAX_VALIDATIONS_ACTIVATED" }]
        })
    );
    let subject_id = stage_response.body["data"]["subject"]["validation"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let update_default = proxy.process_request(json_graphql_request(
        r#"mutation ValidationUpdateDefaults($id: ID!) { validationUpdate(id: $id, validation: { title: "Renamed" }) { validation { id title enable enabled blockOnFailure } userErrors { field message code } } }"#,
        json!({ "id": subject_id }),
    ));
    assert_eq!(
        update_default.body["data"]["validationUpdate"]["validation"],
        json!({
            "id": "gid://shopify/Validation/2",
            "title": "Renamed",
            "enable": false,
            "enabled": false,
            "blockOnFailure": false
        })
    );

    let rejected_metafield = proxy.process_request(json_graphql_request(
        r#"mutation ValidationMetafieldsInvalidUpdate($id: ID!) { validationUpdate(id: $id, validation: { metafields: [{ namespace: "custom", type: "single_line_text_field", value: "loose" }] }) { validation { id } userErrors { field message code } } }"#,
        json!({ "id": "gid://shopify/Validation/2" }),
    ));
    assert_eq!(
        rejected_metafield.body["data"]["validationUpdate"],
        json!({
            "validation": null,
            "userErrors": [{ "field": ["validation", "metafields", "0"], "message": "presence", "code": null }]
        })
    );

    let read = proxy.process_request(json_graphql_request(
        r#"query ValidationAfterRejectedMetafield($id: ID!) { validation(id: $id) { title enable blockOnFailure metafields(first: 5) { nodes { namespace key value } } } }"#,
        json!({ "id": "gid://shopify/Validation/2" }),
    ));
    assert_eq!(
        read.body["data"]["validation"],
        json!({
            "title": "Renamed",
            "enable": false,
            "blockOnFailure": false,
            "metafields": { "nodes": [] }
        })
    );
}

#[test]
fn functions_cart_transform_create_validates_identifier_api_conflict_and_metafields() {
    let mut proxy = snapshot_proxy();

    let unknown_id = proxy.process_request(json_graphql_request(
        r#"mutation CartTransformUnknownId { cartTransformCreate(functionId: "00000000-0000-0000-0000-000000000000") { cartTransform { id functionId } userErrors { field message code } } }"#,
        json!({}),
    ));
    assert_eq!(
        unknown_id.body["data"]["cartTransformCreate"],
        json!({
            "cartTransform": null,
            "userErrors": [{ "field": ["functionId"], "message": "Function 00000000-0000-0000-0000-000000000000 not found. Ensure that it is released in the current app (347082227713), and that the app is installed.", "code": "FUNCTION_NOT_FOUND" }]
        })
    );
    let read_after_unknown_id = proxy.process_request(json_graphql_request(
        r#"query CartTransformsAfterUnknownId { cartTransforms(first: 5) { nodes { id functionId } } }"#,
        json!({}),
    ));
    assert_eq!(
        read_after_unknown_id.body["data"]["cartTransforms"],
        json!({ "nodes": [] })
    );

    let unknown_handle = proxy.process_request(json_graphql_request(
        r#"mutation CartTransformUnknownHandle { cartTransformCreate(functionHandle: "missing-cart-transform") { cartTransform { id functionId } userErrors { field message code } } }"#,
        json!({}),
    ));
    assert_eq!(
        unknown_handle.body["data"]["cartTransformCreate"],
        json!({
            "cartTransform": null,
            "userErrors": [{ "field": ["functionHandle"], "message": "Could not find function with handle: missing-cart-transform.", "code": "FUNCTION_NOT_FOUND" }]
        })
    );
    let read_after_unknown_handle = proxy.process_request(json_graphql_request(
        r#"query CartTransformsAfterUnknownHandle { cartTransforms(first: 5) { nodes { id functionId } } }"#,
        json!({}),
    ));
    assert_eq!(
        read_after_unknown_handle.body["data"]["cartTransforms"],
        json!({ "nodes": [] })
    );

    let api_mismatch = proxy.process_request(json_graphql_request(
        r#"mutation CartTransformApiMismatch { cartTransformCreate(functionHandle: "conformance-validation") { cartTransform { id } userErrors { field message code } } }"#,
        json!({}),
    ));
    assert_eq!(
        api_mismatch.body["data"]["cartTransformCreate"],
        json!({
            "cartTransform": null,
            "userErrors": [{ "field": ["functionHandle"], "message": "Unexpected Function API. The provided function must implement one of the following extension targets: [purchase.cart-transform.run, cart.transform.run].", "code": "FUNCTION_DOES_NOT_IMPLEMENT" }]
        })
    );

    let invalid_metafield = proxy.process_request(json_graphql_request(
        r#"mutation CartTransformInvalidMetafield { cartTransformCreate(functionId: "019dd44b-127f-724b-a49c-70fc98ff4d72", metafields: [{ namespace: "bundles", key: "config", type: "json", value: "not-json" }]) { cartTransform { id } userErrors { field message code } } }"#,
        json!({}),
    ));
    assert_eq!(
        invalid_metafield.body["data"]["cartTransformCreate"],
        json!({
            "cartTransform": null,
            "userErrors": [{ "field": ["metafields", "0", "value"], "message": "is invalid JSON: unexpected token 'not-json' at line 1 column 1.", "code": "INVALID_METAFIELDS" }]
        })
    );

    let create = proxy.process_request(json_graphql_request(
        r#"mutation CartTransformCreateSetup { cartTransformCreate(functionId: "019dd44b-127f-724b-a49c-70fc98ff4d72", blockOnFailure: false) { cartTransform { id functionId blockOnFailure } userErrors { field message code } } }"#,
        json!({}),
    ));
    assert_eq!(
        create.body["data"]["cartTransformCreate"]["cartTransform"],
        json!({
            "id": "gid://shopify/CartTransform/3",
            "functionId": "019dd44b-127f-724b-a49c-70fc98ff4d72",
            "blockOnFailure": false
        })
    );

    let conflict = proxy.process_request(json_graphql_request(
        r#"mutation CartTransformCreateConflict { cartTransformCreate(functionId: "019dd44b-127f-724b-a49c-70fc98ff4d72", blockOnFailure: false) { cartTransform { id functionId } userErrors { field message code } } }"#,
        json!({}),
    ));
    assert_eq!(
        conflict.body["data"]["cartTransformCreate"],
        json!({
            "cartTransform": null,
            "userErrors": [{ "field": ["functionId"], "message": "Could not enable cart transform because it is already registered", "code": "FUNCTION_ALREADY_REGISTERED" }]
        })
    );
}

#[test]
fn functions_fulfillment_constraint_rules_stage_locally_and_read_after_write() {
    let upstream_hits = Arc::new(Mutex::new(0usize));
    let hit_counter = Arc::clone(&upstream_hits);
    let mut proxy = configured_proxy(
        ReadMode::LiveHybrid,
        Some(shopify_draft_proxy::proxy::UnsupportedMutationMode::Passthrough),
    )
    .with_upstream_transport(move |_request| {
        *hit_counter.lock().unwrap() += 1;
        Response {
            status: 500,
            headers: Default::default(),
            body: json!({
                "errors": [{ "message": "fulfillment constraint rule should not hit upstream" }]
            }),
        }
    });

    let create_query = r#"
        mutation CreateFulfillmentConstraintRule {
          fulfillmentConstraintRuleCreate(
            functionHandle: "fulfillment-constraint-local"
            deliveryMethodTypes: [SHIPPING, LOCAL]
            metafields: [{ namespace: "custom", key: "config", type: "json", value: "{\"mode\":\"local\"}" }]
          ) {
            fulfillmentConstraintRule {
              id
              deliveryMethodTypes
              function { id handle apiType }
              metafields(first: 5) { nodes { namespace key type value ownerType } }
            }
            userErrors { code field message }
          }
        }
    "#;
    let create = proxy.process_request(json_graphql_request(create_query, json!({})));
    assert_eq!(create.status, 200);
    assert_eq!(*upstream_hits.lock().unwrap(), 0);
    assert_eq!(
        create.body["data"]["fulfillmentConstraintRuleCreate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        create.body["data"]["fulfillmentConstraintRuleCreate"]["fulfillmentConstraintRule"]["id"],
        json!("gid://shopify/FulfillmentConstraintRule/1")
    );
    assert_eq!(
        create.body["data"]["fulfillmentConstraintRuleCreate"]["fulfillmentConstraintRule"]
            ["deliveryMethodTypes"],
        json!(["SHIPPING", "LOCAL"])
    );
    assert_eq!(
        create.body["data"]["fulfillmentConstraintRuleCreate"]["fulfillmentConstraintRule"]
            ["function"],
        json!({
            "id": "gid://shopify/ShopifyFunction/fulfillment-constraint-local",
            "handle": "fulfillment-constraint-local",
            "apiType": "FULFILLMENT_CONSTRAINT_RULE"
        })
    );
    assert_eq!(
        create.body["data"]["fulfillmentConstraintRuleCreate"]["fulfillmentConstraintRule"]
            ["metafields"]["nodes"][0]["ownerType"],
        json!("FULFILLMENTCONSTRAINTRULE")
    );

    let log = proxy.get_log_snapshot();
    assert_eq!(log["entries"].as_array().unwrap().len(), 1);
    assert_eq!(
        log["entries"][0]["interpreted"]["rootFields"],
        json!(["fulfillmentConstraintRuleCreate"])
    );
    assert_eq!(
        log["entries"][0]["rawBody"]
            .as_str()
            .unwrap()
            .contains("CreateFulfillmentConstraintRule"),
        true
    );

    let read = proxy.process_request(json_graphql_request(
        r#"
        query ReadFulfillmentConstraintRules {
          fulfillmentConstraintRules {
            id
            deliveryMethodTypes
            function { handle apiType }
            metafield(namespace: "custom", key: "config") { namespace key value }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        read.body["data"]["fulfillmentConstraintRules"][0],
        json!({
            "id": "gid://shopify/FulfillmentConstraintRule/1",
            "deliveryMethodTypes": ["SHIPPING", "LOCAL"],
            "function": {
                "handle": "fulfillment-constraint-local",
                "apiType": "FULFILLMENT_CONSTRAINT_RULE"
            },
            "metafield": {
                "namespace": "custom",
                "key": "config",
                "value": "{\"mode\":\"local\"}"
            }
        })
    );

    let node_read = proxy.process_request(json_graphql_request(
        r#"
        query ReadFulfillmentConstraintRuleNode($id: ID!) {
          node(id: $id) {
            ... on FulfillmentConstraintRule {
              id
              deliveryMethodTypes
              function { handle }
            }
          }
        }
        "#,
        json!({ "id": "gid://shopify/FulfillmentConstraintRule/1" }),
    ));
    assert_eq!(
        node_read.body["data"]["node"],
        json!({
            "id": "gid://shopify/FulfillmentConstraintRule/1",
            "deliveryMethodTypes": ["SHIPPING", "LOCAL"],
            "function": { "handle": "fulfillment-constraint-local" }
        })
    );

    let update = proxy.process_request(json_graphql_request(
        r#"
        mutation UpdateFulfillmentConstraintRule($id: ID!) {
          fulfillmentConstraintRuleUpdate(id: $id, deliveryMethodTypes: [PICK_UP]) {
            fulfillmentConstraintRule { id deliveryMethodTypes function { handle } }
            userErrors { code field message }
          }
        }
        "#,
        json!({ "id": "gid://shopify/FulfillmentConstraintRule/1" }),
    ));
    assert_eq!(
        update.body["data"]["fulfillmentConstraintRuleUpdate"],
        json!({
            "fulfillmentConstraintRule": {
                "id": "gid://shopify/FulfillmentConstraintRule/1",
                "deliveryMethodTypes": ["PICK_UP"],
                "function": { "handle": "fulfillment-constraint-local" }
            },
            "userErrors": []
        })
    );

    let delete = proxy.process_request(json_graphql_request(
        r#"
        mutation DeleteFulfillmentConstraintRule($id: ID!) {
          fulfillmentConstraintRuleDelete(id: $id) {
            success
            userErrors { code field message }
          }
        }
        "#,
        json!({ "id": "gid://shopify/FulfillmentConstraintRule/1" }),
    ));
    assert_eq!(
        delete.body["data"]["fulfillmentConstraintRuleDelete"],
        json!({ "success": true, "userErrors": [] })
    );

    let empty_read = proxy.process_request(json_graphql_request(
        r#"query ReadDeletedFulfillmentConstraintRules { fulfillmentConstraintRules { id } }"#,
        json!({}),
    ));
    assert_eq!(
        empty_read.body["data"]["fulfillmentConstraintRules"],
        json!([])
    );
    assert_eq!(*upstream_hits.lock().unwrap(), 0);
    assert_eq!(
        proxy.get_log_snapshot()["entries"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
}

#[test]
fn functions_fulfillment_constraint_rules_return_shopify_like_user_errors() {
    let mut proxy = snapshot_proxy();

    let response = proxy.process_request(json_graphql_request(
        r#"
        mutation FulfillmentConstraintRuleUserErrors {
          missing: fulfillmentConstraintRuleCreate(deliveryMethodTypes: [SHIPPING]) {
            fulfillmentConstraintRule { id }
            userErrors { code field message }
          }
          multiple: fulfillmentConstraintRuleCreate(
            functionId: "gid://shopify/ShopifyFunction/fulfillment-constraint-local"
            functionHandle: "fulfillment-constraint-local"
            deliveryMethodTypes: [SHIPPING]
          ) {
            fulfillmentConstraintRule { id }
            userErrors { code field message }
          }
          emptyDelivery: fulfillmentConstraintRuleCreate(
            functionHandle: "fulfillment-constraint-local"
            deliveryMethodTypes: []
          ) {
            fulfillmentConstraintRule { id }
            userErrors { code field message }
          }
          invalidDelivery: fulfillmentConstraintRuleCreate(
            functionHandle: "fulfillment-constraint-local"
            deliveryMethodTypes: [EXPRESS]
          ) {
            fulfillmentConstraintRule { id }
            userErrors { code field message }
          }
          unknownId: fulfillmentConstraintRuleCreate(
            functionId: "gid://shopify/ShopifyFunction/999999999999"
            deliveryMethodTypes: [SHIPPING]
          ) {
            fulfillmentConstraintRule { id }
            userErrors { code field message }
          }
          unknownHandle: fulfillmentConstraintRuleCreate(
            functionHandle: "definitely-missing-fulfillment-constraint"
            deliveryMethodTypes: [SHIPPING]
          ) {
            fulfillmentConstraintRule { id }
            userErrors { code field message }
          }
          wrongApi: fulfillmentConstraintRuleCreate(
            functionHandle: "conformance-validation"
            deliveryMethodTypes: [SHIPPING]
          ) {
            fulfillmentConstraintRule { id }
            userErrors { code field message }
          }
          deleteUnknown: fulfillmentConstraintRuleDelete(
            id: "gid://shopify/FulfillmentConstraintRule/999999999999"
          ) {
            success
            userErrors { code field message }
          }
        }
        "#,
        json!({}),
    ));

    assert_eq!(
        response.body["data"],
        json!({
            "missing": {
                "fulfillmentConstraintRule": null,
                "userErrors": [{
                    "code": "MISSING_FUNCTION_IDENTIFIER",
                    "field": ["functionHandle"],
                    "message": "Either function_id or function_handle must be provided."
                }]
            },
            "multiple": {
                "fulfillmentConstraintRule": null,
                "userErrors": [{
                    "code": "MULTIPLE_FUNCTION_IDENTIFIERS",
                    "field": ["functionHandle"],
                    "message": "Only one of function_id or function_handle can be provided, not both."
                }]
            },
            "emptyDelivery": {
                "fulfillmentConstraintRule": null,
                "userErrors": [{
                    "code": "INPUT_INVALID",
                    "field": ["deliveryMethodTypes"],
                    "message": "Delivery method types cannot be empty."
                }]
            },
            "invalidDelivery": {
                "fulfillmentConstraintRule": null,
                "userErrors": [{
                    "code": "INPUT_INVALID",
                    "field": ["deliveryMethodTypes"],
                    "message": "One or more delivery method types are invalid."
                }]
            },
            "unknownId": {
                "fulfillmentConstraintRule": null,
                "userErrors": [{
                    "code": "FUNCTION_NOT_FOUND",
                    "field": ["functionId"],
                    "message": "Function gid://shopify/ShopifyFunction/999999999999 not found. Ensure that it is released in the current app (347082227713), and that the app is installed."
                }]
            },
            "unknownHandle": {
                "fulfillmentConstraintRule": null,
                "userErrors": [{
                    "code": "FUNCTION_NOT_FOUND",
                    "field": ["functionHandle"],
                    "message": "Function definitely-missing-fulfillment-constraint not found. Ensure that it is released in the current app (347082227713), and that the app is installed."
                }]
            },
            "wrongApi": {
                "fulfillmentConstraintRule": null,
                "userErrors": [{
                    "code": "FUNCTION_DOES_NOT_IMPLEMENT",
                    "field": ["functionHandle"],
                    "message": "Unexpected Function API. The provided function must implement one of the following extension targets: [purchase.fulfillment-constraint-rule.run, cart.fulfillment-constraints.generate.run]."
                }]
            },
            "deleteUnknown": {
                "success": false,
                "userErrors": [{
                    "code": "NOT_FOUND",
                    "field": ["id"],
                    "message": "Could not find FulfillmentConstraintRule with id: gid://shopify/FulfillmentConstraintRule/999999999999"
                }]
            }
        })
    );

    let read = proxy.process_request(json_graphql_request(
        r#"query FulfillmentConstraintRuleErrorsDoNotStage { fulfillmentConstraintRules { id } }"#,
        json!({}),
    ));
    assert_eq!(read.body["data"]["fulfillmentConstraintRules"], json!([]));
}

#[test]
fn functions_fulfillment_constraint_rule_create_enforces_rule_cap() {
    let mut proxy = snapshot_proxy();

    for _ in 0..25 {
        let response = proxy.process_request(json_graphql_request(
            r#"
            mutation StageFulfillmentConstraintRule {
              fulfillmentConstraintRuleCreate(
                functionHandle: "fulfillment-constraint-local"
                deliveryMethodTypes: [SHIPPING]
              ) {
                fulfillmentConstraintRule { id }
                userErrors { code field message }
              }
            }
            "#,
            json!({}),
        ));
        assert_eq!(
            response.body["data"]["fulfillmentConstraintRuleCreate"]["userErrors"],
            json!([])
        );
    }

    let overflow = proxy.process_request(json_graphql_request(
        r#"
        mutation OverflowFulfillmentConstraintRule {
          fulfillmentConstraintRuleCreate(
            functionHandle: "fulfillment-constraint-local"
            deliveryMethodTypes: [SHIPPING]
          ) {
            fulfillmentConstraintRule { id }
            userErrors { code field message }
          }
        }
        "#,
        json!({}),
    ));

    assert_eq!(
        overflow.body["data"]["fulfillmentConstraintRuleCreate"],
        json!({
            "fulfillmentConstraintRule": null,
            "userErrors": [{
                "code": "INPUT_INVALID",
                "field": [],
                "message": "cannot have more than 25 fulfillment constraint rules"
            }]
        })
    );

    let read = proxy.process_request(json_graphql_request(
        r#"query FulfillmentConstraintRuleCapRead { fulfillmentConstraintRules { id } }"#,
        json!({}),
    ));
    assert_eq!(
        read.body["data"]["fulfillmentConstraintRules"]
            .as_array()
            .unwrap()
            .len(),
        25
    );
    assert_eq!(
        proxy.get_state_snapshot()["stagedState"]["functionFulfillmentConstraintRules"]
            .as_object()
            .unwrap()
            .len(),
        25
    );
}

#[test]
fn functions_fulfillment_constraint_rule_update_rejects_unknown_function_identifiers() {
    let mut proxy = snapshot_proxy();

    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation StageFulfillmentConstraintRuleForUpdateErrors {
          fulfillmentConstraintRuleCreate(
            functionHandle: "fulfillment-constraint-local"
            deliveryMethodTypes: [SHIPPING]
          ) {
            fulfillmentConstraintRule { id deliveryMethodTypes function { handle } }
            userErrors { code field message }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        create.body["data"]["fulfillmentConstraintRuleCreate"]["userErrors"],
        json!([])
    );
    let rule_id = create.body["data"]["fulfillmentConstraintRuleCreate"]
        ["fulfillmentConstraintRule"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let update = proxy.process_request(json_graphql_request(
        r#"
        mutation FulfillmentConstraintRuleUpdateUnknownFunction($id: ID!) {
          unknownId: fulfillmentConstraintRuleUpdate(
            id: $id
            functionId: "gid://shopify/ShopifyFunction/999999999999"
            deliveryMethodTypes: [SHIPPING]
          ) {
            fulfillmentConstraintRule { id }
            userErrors { code field message }
          }
          unknownHandle: fulfillmentConstraintRuleUpdate(
            id: $id
            functionHandle: "definitely-missing-fulfillment-constraint"
            deliveryMethodTypes: [SHIPPING]
          ) {
            fulfillmentConstraintRule { id }
            userErrors { code field message }
          }
        }
        "#,
        json!({ "id": rule_id }),
    ));

    assert_eq!(
        update.body["data"],
        json!({
            "unknownId": {
                "fulfillmentConstraintRule": null,
                "userErrors": [{
                    "code": "FUNCTION_NOT_FOUND",
                    "field": ["functionId"],
                    "message": "Function gid://shopify/ShopifyFunction/999999999999 not found. Ensure that it is released in the current app (347082227713), and that the app is installed."
                }]
            },
            "unknownHandle": {
                "fulfillmentConstraintRule": null,
                "userErrors": [{
                    "code": "FUNCTION_NOT_FOUND",
                    "field": ["functionHandle"],
                    "message": "Function definitely-missing-fulfillment-constraint not found. Ensure that it is released in the current app (347082227713), and that the app is installed."
                }]
            }
        })
    );

    let read = proxy.process_request(json_graphql_request(
        r#"query FulfillmentConstraintRuleAfterUnknownFunctionUpdate { fulfillmentConstraintRules { id deliveryMethodTypes function { handle } } }"#,
        json!({}),
    ));
    assert_eq!(
        read.body["data"]["fulfillmentConstraintRules"],
        json!([{
            "id": rule_id,
            "deliveryMethodTypes": ["SHIPPING"],
            "function": { "handle": "fulfillment-constraint-local" }
        }])
    );
}

#[test]
fn localization_locale_and_translation_lifecycle_stages_reads_and_clears_locale_translations() {
    let mut proxy = snapshot_proxy();
    let title_digest = fallback_product_title_digest();

    let initial = proxy.process_request(json_graphql_request(
        r#"query LocalizationLocaleTranslationRead($first: Int!, $resourceType: TranslatableResourceType!, $ids: [ID!]!) {
          availableLocalesExcerpt: availableLocales { isoCode name }
          allShopLocales: shopLocales { locale name primary published }
          publishedShopLocales: shopLocales(published: true) { locale name primary published }
          resources: translatableResources(first: $first, resourceType: $resourceType) { nodes { resourceId translatableContent { key value digest locale type } translations(locale: "fr") { key value locale outdated market { id } } } pageInfo { hasNextPage hasPreviousPage } }
          byIds: translatableResourcesByIds(first: $first, resourceIds: $ids) { nodes { resourceId } edges { cursor node { resourceId } } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } }
          missing: translatableResource(resourceId: "gid://shopify/Product/999999999999999") { resourceId }
        }"#,
        json!({ "first": 3, "resourceType": "PRODUCT", "ids": ["gid://shopify/Product/999999999999999"] }),
    ));
    assert_eq!(
        initial.body["data"]["allShopLocales"][0]["locale"],
        json!("en")
    );
    assert!(initial.body["data"]["availableLocalesExcerpt"]
        .as_array()
        .unwrap()
        .iter()
        .any(|locale| locale["isoCode"] == json!("fr") && locale["name"] == json!("French")));
    assert_eq!(initial.body["data"]["missing"], Value::Null);

    let enable = proxy.process_request(json_graphql_request(
        r#"mutation LocalizationShopLocaleEnable($locale: String!) { shopLocaleEnable(locale: $locale) { shopLocale { locale name primary published } userErrors { field message } } }"#,
        json!({ "locale": "fr" }),
    ));
    assert_eq!(
        enable.body["data"]["shopLocaleEnable"]["shopLocale"]["locale"],
        json!("fr")
    );
    assert_eq!(
        enable.body["data"]["shopLocaleEnable"]["userErrors"],
        json!([])
    );

    let registered = proxy.process_request(json_graphql_request(
        r#"mutation LocalizationTranslationsRegister($resourceId: ID!, $translations: [TranslationInput!]!) { translationsRegister(resourceId: $resourceId, translations: $translations) { translations { key value locale outdated market { id } } userErrors { field message code } } }"#,
        json!({ "resourceId": "gid://shopify/Product/9801098789170", "translations": [{ "locale": "fr", "key": "title", "value": "Titre local", "translatableContentDigest": title_digest }] }),
    ));
    assert_eq!(
        registered.body["data"]["translationsRegister"]["translations"][0]["value"],
        json!("Titre local")
    );

    let downstream = proxy.process_request(json_graphql_request(
        r#"query LocalizationTranslationsRead($resourceId: ID!) { translatableResource(resourceId: $resourceId) { resourceId translations(locale: "fr") { key value locale outdated market { id } } } }"#,
        json!({ "resourceId": "gid://shopify/Product/9801098789170" }),
    ));
    assert_eq!(
        downstream.body["data"]["translatableResource"]["translations"][0]["value"],
        json!("Titre local")
    );

    let disable = proxy.process_request(json_graphql_request(
        r#"mutation LocalizationShopLocaleDisable($locale: String!) { shopLocaleDisable(locale: $locale) { locale userErrors { field message } } }"#,
        json!({ "locale": "fr" }),
    ));
    assert_eq!(
        disable.body["data"]["shopLocaleDisable"],
        json!({ "locale": "fr", "userErrors": [] })
    );

    let after_disable = proxy.process_request(json_graphql_request(
        r#"query LocalizationTranslationsRead($resourceId: ID!) { translatableResource(resourceId: $resourceId) { resourceId translations(locale: "fr") { key value locale outdated market { id } } } }"#,
        json!({ "resourceId": "gid://shopify/Product/9801098789170" }),
    ));
    assert_eq!(
        after_disable.body["data"]["translatableResource"]["translations"],
        json!([])
    );
}

#[test]
fn localization_catalog_reads_are_store_backed_without_ported_document_marker() {
    let mut proxy = snapshot_proxy();

    let baseline = proxy.process_request(json_graphql_request(
        r#"query ArbitraryLocaleCatalogRead {
          locales: availableLocales { isoCode name }
          all: shopLocales { locale name primary published marketWebPresences { id subfolderSuffix } }
          published: shopLocales(published: true) { locale published }
        }"#,
        json!({}),
    ));
    assert_eq!(baseline.status, 200);
    assert_eq!(
        baseline.body["data"]["all"],
        json!([{
            "locale": "en",
            "name": "English",
            "primary": true,
            "published": true,
            "marketWebPresences": [{
                "id": "gid://shopify/MarketWebPresence/62842765618",
                "subfolderSuffix": null
            }]
        }])
    );
    assert!(baseline.body["data"]["locales"]
        .as_array()
        .unwrap()
        .iter()
        .any(|locale| locale["isoCode"] == json!("tr") && locale["name"] == json!("Turkish")));
    assert_eq!(
        baseline.body["data"]["published"],
        json!([{ "locale": "en", "published": true }])
    );

    let lifecycle = proxy.process_request(json_graphql_request(
        r#"mutation LocalizationShopLocaleEnable($known: ID!) {
          enable: shopLocaleEnable(locale: "fr") { shopLocale { locale published } userErrors { field message code } }
          update: shopLocaleUpdate(locale: "fr", shopLocale: { published: true, marketWebPresenceIds: [$known] }) { shopLocale { locale name published marketWebPresences { id __typename defaultLocale { locale } } } userErrors { field message code } }
        }"#,
        json!({ "known": "gid://shopify/MarketWebPresence/known" }),
    ));
    assert_eq!(lifecycle.status, 200);
    assert_eq!(
        lifecycle.body["data"]["update"]["shopLocale"],
        json!({
            "locale": "fr",
            "name": "French",
            "published": true,
            "marketWebPresences": [{
                "id": "gid://shopify/MarketWebPresence/known",
                "__typename": "MarketWebPresence",
                "defaultLocale": { "locale": "en" }
            }]
        })
    );

    let after_update = proxy.process_request(json_graphql_request(
        r#"query AnyNameCanReadStagedLocales {
          all: shopLocales { locale name published marketWebPresences { id __typename defaultLocale { locale } } }
          published: shopLocales(published: true) { locale published }
        }"#,
        json!({}),
    ));
    let all = after_update.body["data"]["all"].as_array().unwrap();
    assert!(all
        .iter()
        .any(|locale| locale["locale"] == json!("fr") && locale["published"] == json!(true)));
    assert!(after_update.body["data"]["published"]
        .as_array()
        .unwrap()
        .iter()
        .any(|locale| locale["locale"] == json!("fr")));

    let disabled = proxy.process_request(json_graphql_request(
        r#"mutation LocalizationShopLocaleDisable($locale: String!) { shopLocaleDisable(locale: $locale) { locale userErrors { field message code } } }"#,
        json!({ "locale": "fr" }),
    ));
    assert_eq!(
        disabled.body["data"]["shopLocaleDisable"],
        json!({ "locale": "fr", "userErrors": [] })
    );

    let after_disable = proxy.process_request(json_graphql_request(
        r#"query NoMarkerShopLocaleAfterDisable { shopLocales { locale published } }"#,
        json!({}),
    ));
    assert_eq!(
        after_disable.body["data"]["shopLocales"],
        json!([{ "locale": "en", "published": true }])
    );
}

#[test]
fn localization_markets_read_returns_empty_connection_without_source_data() {
    let mut proxy = snapshot_proxy();

    let localization_read = proxy.process_request(json_graphql_request(
        r#"query LocalizationMarketsNoData {
          markets(first: 5) {
            nodes { id name handle status type }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
        }"#,
        json!({}),
    ));
    assert_eq!(localization_read.status, 200);
    assert_eq!(
        localization_read.body["data"]["markets"],
        json!({
            "nodes": [],
            "pageInfo": {
                "hasNextPage": false,
                "hasPreviousPage": false,
                "startCursor": null,
                "endCursor": null
            }
        })
    );
    let serialized = serde_json::to_string(&localization_read.body).unwrap();
    assert!(!serialized.contains("gid://shopify/Market/123"));
    assert!(!serialized.contains("gid://shopify/Market/ca"));

    let market_localization_read = proxy.process_request(json_graphql_request(
        r#"query RustMarketLocalizationsLocalRuntimeSourceEmpty {
          markets(first: 5) { nodes { id name handle status type } }
          marketLocalizableResource(resourceId: "gid://shopify/Metafield/localizable") { resourceId }
        }"#,
        json!({}),
    ));
    assert_eq!(market_localization_read.status, 200);
    assert_eq!(
        market_localization_read.body["data"]["markets"]["nodes"],
        json!([])
    );
    let serialized = serde_json::to_string(&market_localization_read.body).unwrap();
    assert!(!serialized.contains("gid://shopify/Market/123"));
    assert!(!serialized.contains("gid://shopify/Market/ca"));
}

#[test]
fn localization_markets_read_hydrates_from_live_source_and_reuses_observed_market() {
    let upstream_requests = Arc::new(Mutex::new(Vec::new()));
    let captured_requests = Arc::clone(&upstream_requests);
    let mut proxy =
        configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport(move |request| {
            let body =
                serde_json::from_str::<Value>(&request.body).expect("upstream GraphQL body parses");
            // A cold LiveHybrid markets read forwards the client's query verbatim
            // upstream and hydrates the staged markets store from the response as a
            // side effect — it does not synthesize a separate hydration operation.
            // This matches the recorded conformance cassettes (e.g. markets-catalog
            // records a verbatim MarketsCatalogRead upstream call; none synthesize a
            // LocalizationMarketsHydrate operation), so the forwarded document is the
            // original markets read rather than a fabricated one.
            assert!(
                body["query"]
                    .as_str()
                    .is_some_and(|query| query.contains("markets(first")),
                "expected the markets read forwarded verbatim upstream, got {body}"
            );
            captured_requests.lock().unwrap().push(request.clone());
            shopify_draft_proxy::proxy::Response {
                status: 200,
                headers: Default::default(),
                body: json!({
                    "data": {
                        "markets": {
                            "nodes": [{
                                "id": "gid://shopify/Market/97997685042",
                                "name": "Source Market",
                                "handle": "source-market",
                                "status": "DRAFT",
                                "type": "NONE"
                            }]
                        }
                    }
                }),
            }
        });

    let mut request = json_graphql_request(
        r#"query LocalizationMarketsFromSource($first: Int!) {
          markets(first: $first) { nodes { id name handle status type } }
        }"#,
        json!({ "first": 1 }),
    );
    request.path = "/admin/api/2026-04/graphql.json".to_string();
    request.headers.insert(
        "x-shopify-access-token".to_string(),
        "source-token".to_string(),
    );
    let hydrated = proxy.process_request(request.clone());

    assert_eq!(hydrated.status, 200);
    assert_eq!(
        hydrated.body["data"]["markets"]["nodes"],
        json!([{
            "id": "gid://shopify/Market/97997685042",
            "name": "Source Market",
            "handle": "source-market",
            "status": "DRAFT",
            "type": "NONE"
        }])
    );
    {
        let requests = upstream_requests.lock().unwrap();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].path, "/admin/api/2026-04/graphql.json");
        assert_eq!(
            requests[0]
                .headers
                .get("x-shopify-access-token")
                .map(String::as_str),
            Some("source-token")
        );
    }

    let cached = proxy.process_request(request);
    assert_eq!(
        cached.body["data"]["markets"]["nodes"][0]["id"],
        json!("gid://shopify/Market/97997685042")
    );
    assert_eq!(upstream_requests.lock().unwrap().len(), 1);
}

#[test]
fn localization_source_read_stages_observed_markets_and_shop_locales_for_translation_replay() {
    let upstream_requests = Arc::new(Mutex::new(Vec::new()));
    let captured_requests = Arc::clone(&upstream_requests);
    let title_digest = fallback_product_title_digest();
    let mut proxy = configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport(
        move |request| {
            let body = serde_json::from_str::<Value>(&request.body)
                .expect("upstream GraphQL body parses");
            // The source read forwards the client's `markets`/`shopLocales` document
            // verbatim once and hydrates both staged stores from the single response;
            // it does not synthesize a separate LocalizationMarketsHydrate operation
            // (no conformance cassette records one), so there is no second fallback
            // call to recover from.
            assert_ne!(body["operationName"], json!("LocalizationMarketsHydrate"));
            captured_requests.lock().unwrap().push(request.clone());
            shopify_draft_proxy::proxy::Response {
                status: 200,
                headers: Default::default(),
                body: json!({
                    "data": {
                        "markets": {
                            "nodes": [{
                                "id": "gid://shopify/Market/97997685042",
                                "name": "Captured Market",
                                "handle": "captured-market",
                                "status": "ACTIVE",
                                "type": "REGION"
                            }]
                        },
                        "allShopLocales": [
                            { "locale": "en", "name": "English", "primary": true, "published": true, "marketWebPresences": [] },
                            { "locale": "es", "name": "Spanish", "primary": false, "published": false, "marketWebPresences": [] }
                        ]
                    }
                }),
            }
        },
    );

    let source_read = proxy.process_request(json_graphql_request(
        r#"query LocalizationTranslationsMarketScopedRead($marketsFirst: Int!) {
          markets(first: $marketsFirst) { nodes { id name handle status type } }
          allShopLocales: shopLocales { locale name primary published marketWebPresences { id } }
        }"#,
        json!({ "marketsFirst": 1 }),
    ));
    assert_eq!(source_read.status, 200);
    assert_eq!(
        source_read.body["data"]["markets"]["nodes"][0]["id"],
        json!("gid://shopify/Market/97997685042")
    );
    // One verbatim upstream forward serves the whole multi-root source read and
    // hydrates both the markets and shop-locale stores for the translation replay
    // below.
    assert_eq!(upstream_requests.lock().unwrap().len(), 1);

    let registered = proxy.process_request(json_graphql_request(
        r#"mutation LocalizationTranslationsRegister($resourceId: ID!, $translations: [TranslationInput!]!) {
          translationsRegister(resourceId: $resourceId, translations: $translations) {
            translations { key value locale market { id } }
            userErrors { field message code }
          }
        }"#,
        json!({
            "resourceId": "gid://shopify/Product/9801098789170",
            "translations": [{
                "locale": "es",
                "key": "title",
                "value": "Titulo de mercado",
                "marketId": "gid://shopify/Market/97997685042",
                "translatableContentDigest": title_digest
            }]
        }),
    ));
    assert_eq!(registered.status, 200);
    assert_eq!(
        registered.body["data"]["translationsRegister"]["translations"],
        json!([{
            "key": "title",
            "value": "Titulo de mercado",
            "locale": "es",
            "market": { "id": "gid://shopify/Market/97997685042" }
        }])
    );
    assert_eq!(
        registered.body["data"]["translationsRegister"]["userErrors"],
        json!([])
    );
}

#[test]
fn localization_markets_read_uses_locally_staged_markets_before_upstream() {
    let upstream_hits = Arc::new(Mutex::new(0usize));
    let captured_hits = Arc::clone(&upstream_hits);
    let mut proxy = configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport(
        move |_request| {
            *captured_hits.lock().unwrap() += 1;
            shopify_draft_proxy::proxy::Response {
                status: 500,
                headers: Default::default(),
                body: json!({ "errors": [{ "message": "staged market read should not hit upstream" }] }),
            }
        },
    );

    let created = proxy.process_request(json_graphql_request(
        r#"mutation RustMarketCreateLocalRuntimeSourceBacked($input: MarketCreateInput!) {
          marketCreate(input: $input) {
            market { id name handle status }
            userErrors { field message code }
          }
        }"#,
        json!({ "input": { "name": "Canada", "regions": [{ "countryCode": "CA" }] } }),
    ));
    assert_eq!(created.status, 200);
    assert_eq!(
        created.body["data"]["marketCreate"]["userErrors"],
        json!([])
    );
    let market = created.body["data"]["marketCreate"]["market"].clone();

    let read = proxy.process_request(json_graphql_request(
        r#"query LocalizationMarketsStagedRead {
          markets(first: 5) { nodes { id name handle status } }
        }"#,
        json!({}),
    ));
    assert_eq!(read.status, 200);
    assert_eq!(read.body["data"]["markets"]["nodes"], json!([market]));
    assert_eq!(*upstream_hits.lock().unwrap(), 0);
}

#[test]
fn localization_translations_register_multi_row_round_trip_and_indexed_errors() {
    let mut proxy = snapshot_proxy();
    let resource_id = "gid://shopify/Product/9801098789170";
    let title_digest = fallback_product_title_digest();
    let meta_title_digest = localization_content_digest("");

    for locale in ["fr", "es"] {
        let enable = proxy.process_request(json_graphql_request(
            r#"mutation LocalizationShopLocaleEnable($locale: String!) {
              shopLocaleEnable(locale: $locale) { userErrors { field message code } }
            }"#,
            json!({ "locale": locale }),
        ));
        assert_eq!(
            enable.body["data"]["shopLocaleEnable"]["userErrors"],
            json!([])
        );
    }

    let registered = proxy.process_request(json_graphql_request(
        r#"mutation LocalizationTranslationsRegister($resourceId: ID!, $translations: [TranslationInput!]!) { translationsRegister(resourceId: $resourceId, translations: $translations) { translations { key value locale outdated updatedAt market { id } } userErrors { field message code } } }"#,
        json!({
            "resourceId": resource_id,
            "translations": [
                { "locale": "fr", "key": "title", "value": "Titre local", "translatableContentDigest": title_digest },
                { "locale": "fr", "key": "body_html", "value": "Description locale", "translatableContentDigest": "digest-body" }
            ]
        }),
    ));
    assert_eq!(
        registered.body["data"]["translationsRegister"]["translations"],
        json!([
            { "key": "title", "value": "Titre local", "locale": "fr", "outdated": false, "updatedAt": registered.body["data"]["translationsRegister"]["translations"][0]["updatedAt"], "market": null },
            { "key": "body_html", "value": "Description locale", "locale": "fr", "outdated": false, "updatedAt": registered.body["data"]["translationsRegister"]["translations"][1]["updatedAt"], "market": null }
        ])
    );
    assert_datetime_string(
        &registered.body["data"]["translationsRegister"]["translations"][0]["updatedAt"],
        "registered title translation updatedAt",
    );
    assert_datetime_string(
        &registered.body["data"]["translationsRegister"]["translations"][1]["updatedAt"],
        "registered body translation updatedAt",
    );
    assert_eq!(
        registered.body["data"]["translationsRegister"]["userErrors"],
        json!([])
    );

    let downstream = proxy.process_request(json_graphql_request(
        r#"query LocalizationTranslationsRead($resourceId: ID!) { translatableResource(resourceId: $resourceId) { resourceId translations(locale: "fr") { key value locale outdated updatedAt market { id } } } }"#,
        json!({ "resourceId": resource_id }),
    ));
    assert_eq!(
        downstream.body["data"]["translatableResource"]["translations"],
        json!([
            { "key": "title", "value": "Titre local", "locale": "fr", "outdated": false, "updatedAt": registered.body["data"]["translationsRegister"]["translations"][0]["updatedAt"], "market": null },
            { "key": "body_html", "value": "Description locale", "locale": "fr", "outdated": false, "updatedAt": registered.body["data"]["translationsRegister"]["translations"][1]["updatedAt"], "market": null }
        ])
    );

    let mixed = proxy.process_request(json_graphql_request(
        r#"mutation LocalizationTranslationsRegister($resourceId: ID!, $translations: [TranslationInput!]!) { translationsRegister(resourceId: $resourceId, translations: $translations) { translations { key value locale outdated updatedAt market { id } } userErrors { field message code } } }"#,
        json!({
            "resourceId": resource_id,
            "translations": [
                { "locale": "fr", "key": "meta_title", "value": "Titre SEO", "translatableContentDigest": meta_title_digest },
                { "locale": "fr", "key": "title", "value": "Invalid digest row", "translatableContentDigest": "invalid-title" },
                { "locale": "es", "key": "title", "value": "Titulo local", "translatableContentDigest": title_digest }
            ]
        }),
    ));
    assert_eq!(
        mixed.body["data"]["translationsRegister"]["translations"],
        json!([
            { "key": "meta_title", "value": "Titre SEO", "locale": "fr", "outdated": false, "updatedAt": mixed.body["data"]["translationsRegister"]["translations"][0]["updatedAt"], "market": null },
            { "key": "title", "value": "Titulo local", "locale": "es", "outdated": false, "updatedAt": mixed.body["data"]["translationsRegister"]["translations"][1]["updatedAt"], "market": null }
        ])
    );
    assert_datetime_string(
        &mixed.body["data"]["translationsRegister"]["translations"][0]["updatedAt"],
        "mixed seo translation updatedAt",
    );
    assert_datetime_string(
        &mixed.body["data"]["translationsRegister"]["translations"][1]["updatedAt"],
        "mixed es title translation updatedAt",
    );
    assert_eq!(
        mixed.body["data"]["translationsRegister"]["userErrors"][0]["field"],
        json!(["translations", "1", "translatableContentDigest"])
    );

    let downstream_after_mixed = proxy.process_request(json_graphql_request(
        r#"query LocalizationTranslationsRead($resourceId: ID!) { translatableResource(resourceId: $resourceId) { resourceId translations(locale: "fr") { key value locale outdated updatedAt market { id } } } }"#,
        json!({ "resourceId": resource_id }),
    ));
    assert_eq!(
        downstream_after_mixed.body["data"]["translatableResource"]["translations"],
        json!([
            { "key": "title", "value": "Titre local", "locale": "fr", "outdated": false, "updatedAt": registered.body["data"]["translationsRegister"]["translations"][0]["updatedAt"], "market": null },
            { "key": "body_html", "value": "Description locale", "locale": "fr", "outdated": false, "updatedAt": registered.body["data"]["translationsRegister"]["translations"][1]["updatedAt"], "market": null },
            { "key": "meta_title", "value": "Titre SEO", "locale": "fr", "outdated": false, "updatedAt": mixed.body["data"]["translationsRegister"]["translations"][0]["updatedAt"], "market": null }
        ])
    );
    let original_title_updated_at =
        registered.body["data"]["translationsRegister"]["translations"][0]["updatedAt"].clone();
    let reregister = proxy.process_request(json_graphql_request(
        r#"mutation LocalizationTranslationsRegister($resourceId: ID!, $translations: [TranslationInput!]!) { translationsRegister(resourceId: $resourceId, translations: $translations) { translations { key value locale outdated updatedAt market { id } } userErrors { field message code } } }"#,
        json!({
            "resourceId": resource_id,
            "translations": [
                { "locale": "fr", "key": "title", "value": "Titre local rafraichi", "translatableContentDigest": title_digest }
            ]
        }),
    ));
    let refreshed_title_updated_at =
        reregister.body["data"]["translationsRegister"]["translations"][0]["updatedAt"].clone();
    assert_datetime_string(
        &refreshed_title_updated_at,
        "reregistered title translation updatedAt",
    );
    assert_ne!(refreshed_title_updated_at, original_title_updated_at);

    let downstream_after_reregister = proxy.process_request(json_graphql_request(
        r#"query LocalizationTranslationsRead($resourceId: ID!) { translatableResource(resourceId: $resourceId) { resourceId translations(locale: "fr") { key value locale outdated updatedAt market { id } } } }"#,
        json!({ "resourceId": resource_id }),
    ));
    assert_eq!(
        downstream_after_reregister.body["data"]["translatableResource"]["translations"],
        json!([
            { "key": "body_html", "value": "Description locale", "locale": "fr", "outdated": false, "updatedAt": registered.body["data"]["translationsRegister"]["translations"][1]["updatedAt"], "market": null },
            { "key": "meta_title", "value": "Titre SEO", "locale": "fr", "outdated": false, "updatedAt": mixed.body["data"]["translationsRegister"]["translations"][0]["updatedAt"], "market": null },
            { "key": "title", "value": "Titre local rafraichi", "locale": "fr", "outdated": false, "updatedAt": refreshed_title_updated_at, "market": null }
        ])
    );
}

#[test]
fn localization_translations_register_rejects_invalid_product_key_without_staging_it() {
    let mut proxy = snapshot_proxy();
    let resource_id = "gid://shopify/Product/9801098789170";
    let title_digest = fallback_product_title_digest();

    let enable = proxy.process_request(json_graphql_request(
        r#"mutation LocalizationShopLocaleEnable($locale: String!) {
          shopLocaleEnable(locale: $locale) { userErrors { field message code } }
        }"#,
        json!({ "locale": "fr" }),
    ));
    assert_eq!(
        enable.body["data"]["shopLocaleEnable"]["userErrors"],
        json!([])
    );

    let registered = proxy.process_request(json_graphql_request(
        r#"mutation LocalizationTranslationsRegister($resourceId: ID!, $translations: [TranslationInput!]!) {
          translationsRegister(resourceId: $resourceId, translations: $translations) {
            translations { key value locale outdated market { id } }
            userErrors { field message code }
          }
        }"#,
        json!({
            "resourceId": resource_id,
            "translations": [
                { "locale": "fr", "key": "title", "value": "Titre valide", "translatableContentDigest": title_digest },
                { "locale": "fr", "key": "incorrect_key", "value": "Valeur invalide", "translatableContentDigest": title_digest }
            ]
        }),
    ));
    assert_eq!(
        registered.body["data"]["translationsRegister"]["translations"],
        json!([
            { "key": "title", "value": "Titre valide", "locale": "fr", "outdated": false, "market": null }
        ])
    );
    assert_eq!(
        registered.body["data"]["translationsRegister"]["userErrors"],
        json!([{
            "field": ["translations", "1", "key"],
            "message": "Key incorrect_key is not a valid translatable field",
            "code": "INVALID_KEY_FOR_MODEL"
        }])
    );

    let downstream = proxy.process_request(json_graphql_request(
        r#"query LocalizationTranslationsRead($resourceId: ID!) {
          translatableResource(resourceId: $resourceId) {
            resourceId
            translations(locale: "fr") { key value locale outdated market { id } }
          }
        }"#,
        json!({ "resourceId": resource_id }),
    ));
    assert_eq!(
        downstream.body["data"]["translatableResource"]["translations"],
        json!([
            { "key": "title", "value": "Titre valide", "locale": "fr", "outdated": false, "market": null }
        ])
    );
}

#[test]
fn localization_translatable_roots_are_store_backed_without_operation_markers() {
    let mut proxy = snapshot_proxy();
    let product_id = "gid://shopify/Product/9801098789170";
    let collection_id = "gid://shopify/Collection/9801098789170";
    let product_title_digest = fallback_product_title_digest();
    let product_type_digest = localization_content_digest("snowboard");
    let collection_title_digest = localization_content_digest("Collection title");

    let enable = proxy.process_request(json_graphql_request(
        r#"mutation EnableLocale($locale: String!) {
          shopLocaleEnable(locale: $locale) { userErrors { field message code } }
        }"#,
        json!({ "locale": "fr" }),
    ));
    assert_eq!(
        enable.body["data"]["shopLocaleEnable"]["userErrors"],
        json!([])
    );

    let product_register = proxy.process_request(json_graphql_request(
        r#"mutation RegisterProductTranslation($resourceId: ID!, $translations: [TranslationInput!]!) {
          translationsRegister(resourceId: $resourceId, translations: $translations) {
            translations { key value locale market { id } }
            userErrors { field message code }
          }
        }"#,
        json!({
            "resourceId": product_id,
            "translations": [
                { "locale": "fr", "key": "title", "value": "Titre produit", "translatableContentDigest": product_title_digest },
                { "locale": "fr", "key": "product_type", "value": "Produit", "translatableContentDigest": product_type_digest, "marketId": "gid://shopify/Market/123" }
            ]
        }),
    ));
    assert_eq!(
        product_register.body["data"]["translationsRegister"]["userErrors"],
        json!([])
    );

    let collection_register = proxy.process_request(json_graphql_request(
        r#"mutation RegisterCollectionTranslation($resourceId: ID!, $translations: [TranslationInput!]!) {
          translationsRegister(resourceId: $resourceId, translations: $translations) {
            translations { key value locale market { id } }
            userErrors { field message code }
          }
        }"#,
        json!({
            "resourceId": collection_id,
            "translations": [
                { "locale": "fr", "key": "title", "value": "Titre collection", "translatableContentDigest": collection_title_digest }
            ]
        }),
    ));
    assert_eq!(
        collection_register.body["data"]["translationsRegister"]["userErrors"],
        json!([])
    );

    let read = proxy.process_request(json_graphql_request(
        r#"query ArbitraryLocalizationRead($productId: ID!, $collectionId: ID!, $ids: [ID!]!) {
          direct: translatableResource(resourceId: $productId) {
            resourceId
            allFr: translations(locale: "fr") { key value locale market { id } }
            marketFr: translations(locale: "fr", marketId: "gid://shopify/Market/123") { key value locale market { id } }
          }
          byType: translatableResources(first: 2, resourceType: PRODUCT) {
            aliasedNodes: nodes { resourceId translations(locale: "fr") { key value } }
            aliasedEdges: edges { aliasedCursor: cursor node { resourceId } }
            aliasedPage: pageInfo { next: hasNextPage previous: hasPreviousPage }
          }
          byIds: translatableResourcesByIds(first: 3, resourceIds: $ids) {
            nodes { resourceId translations(locale: "fr") { key value } }
            edges { cursor node { resourceId } }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
          missing: translatableResource(resourceId: "gid://shopify/Product/999999999999999") { resourceId }
        }"#,
        json!({ "productId": product_id, "collectionId": collection_id, "ids": [collection_id, product_id] }),
    ));

    assert_eq!(read.status, 200);
    assert_eq!(
        read.body["data"]["direct"]["allFr"],
        json!([
            { "key": "title", "value": "Titre produit", "locale": "fr", "market": null },
            { "key": "product_type", "value": "Produit", "locale": "fr", "market": { "id": "gid://shopify/Market/123" } }
        ])
    );
    assert_eq!(
        read.body["data"]["direct"]["marketFr"],
        json!([{ "key": "product_type", "value": "Produit", "locale": "fr", "market": { "id": "gid://shopify/Market/123" } }])
    );
    assert_eq!(
        read.body["data"]["byType"]["aliasedNodes"][0]["translations"],
        json!([
            { "key": "title", "value": "Titre produit" },
            { "key": "product_type", "value": "Produit" }
        ])
    );
    assert_eq!(
        read.body["data"]["byType"]["aliasedEdges"][0]["node"]["resourceId"],
        json!(product_id)
    );
    assert_eq!(
        read.body["data"]["byType"]["aliasedPage"],
        json!({ "next": false, "previous": false })
    );
    assert_eq!(
        read.body["data"]["byIds"]["nodes"][0]["resourceId"],
        json!(collection_id)
    );
    assert_eq!(
        read.body["data"]["byIds"]["nodes"][0]["translations"],
        json!([{ "key": "title", "value": "Titre collection" }])
    );
    assert_eq!(read.body["data"]["missing"], Value::Null);
}

#[test]
fn localization_translations_reject_unknown_supported_product_resource_ids() {
    let mut proxy = snapshot_proxy();
    let unknown_resource_id = "gid://shopify/Product/123";

    let enable = proxy.process_request(json_graphql_request(
        r#"mutation LocalizationShopLocaleEnable($locale: String!) {
          shopLocaleEnable(locale: $locale) { userErrors { field message code } }
        }"#,
        json!({ "locale": "fr" }),
    ));
    assert_eq!(
        enable.body["data"]["shopLocaleEnable"]["userErrors"],
        json!([])
    );

    let register = proxy.process_request(json_graphql_request(
        r#"mutation LocalizationTranslationsRegister($resourceId: ID!, $translations: [TranslationInput!]!) {
          translationsRegister(resourceId: $resourceId, translations: $translations) {
            translations { key value locale }
            userErrors { field message code }
          }
        }"#,
        json!({
            "resourceId": unknown_resource_id,
            "translations": [{
                "locale": "fr",
                "key": "title",
                "value": "Bonjour",
                "translatableContentDigest": "digest"
            }]
        }),
    ));
    assert_eq!(
        register.body["data"]["translationsRegister"],
        json!({
            "translations": null,
            "userErrors": [{
                "field": ["resourceId"],
                "message": format!("Resource {unknown_resource_id} does not exist"),
                "code": "RESOURCE_NOT_FOUND"
            }]
        })
    );

    let downstream = proxy.process_request(json_graphql_request(
        r#"query LocalizationTranslationsRead($resourceId: ID!) {
          translatableResource(resourceId: $resourceId) {
            resourceId
            translations(locale: "fr") { key value locale }
          }
        }"#,
        json!({ "resourceId": unknown_resource_id }),
    ));
    assert_eq!(downstream.body["data"]["translatableResource"], Value::Null);

    let remove = proxy.process_request(json_graphql_request(
        r#"mutation LocalizationTranslationsRemove($resourceId: ID!, $keys: [String!]!, $locales: [String!]!) {
          translationsRemove(resourceId: $resourceId, translationKeys: $keys, locales: $locales) {
            translations { key value locale }
            userErrors { field message code }
          }
        }"#,
        json!({ "resourceId": unknown_resource_id, "keys": ["title"], "locales": ["fr"] }),
    ));
    assert_eq!(
        remove.body["data"]["translationsRemove"],
        json!({
            "translations": null,
            "userErrors": [{
                "field": ["resourceId"],
                "message": format!("Resource {unknown_resource_id} does not exist"),
                "code": "RESOURCE_NOT_FOUND"
            }]
        })
    );
}

#[test]
fn localization_unknown_resource_and_market_scoped_translation_validation_match_shopify_shapes() {
    let mut proxy = snapshot_proxy();
    let title_digest = fallback_product_title_digest();
    let handle_digest = fallback_product_handle_digest();

    let unknown_resource = proxy.process_request(json_graphql_request(
        r#"mutation LocalizationUnknownResourceValidation($resourceId: ID!, $translations: [TranslationInput!]!, $keys: [String!]!, $locales: [String!]!) {
          register: translationsRegister(resourceId: $resourceId, translations: $translations) { translations { key } userErrors { field message code } }
          remove: translationsRemove(resourceId: $resourceId, translationKeys: $keys, locales: $locales) { translations { key } userErrors { field message code } }
        }"#,
        json!({ "resourceId": "gid://shopify/Product/999999999999999", "translations": [{ "locale": "fr", "key": "title", "value": "Missing", "translatableContentDigest": "missing" }], "keys": ["title"], "locales": ["fr"] }),
    ));
    assert_eq!(
        unknown_resource.body["data"]["register"]["translations"],
        Value::Null
    );
    assert_eq!(
        unknown_resource.body["data"]["register"]["userErrors"][0]["code"],
        json!("RESOURCE_NOT_FOUND")
    );
    assert_eq!(
        unknown_resource.body["data"]["remove"]["userErrors"][0]["field"],
        json!(["resourceId"])
    );

    let primary_disable = proxy.process_request(json_graphql_request(
        r#"mutation LocalizationShopLocaleDisable($locale: String!) { shopLocaleDisable(locale: $locale) { locale userErrors { field message } } }"#,
        json!({ "locale": "en" }),
    ));
    assert_eq!(
        primary_disable.body["data"]["shopLocaleDisable"]["locale"],
        Value::Null
    );
    assert_eq!(
        primary_disable.body["data"]["shopLocaleDisable"]["userErrors"][0]["field"],
        json!(["locale"])
    );

    for locale in ["fr", "es"] {
        let enable = proxy.process_request(json_graphql_request(
            r#"mutation LocalizationShopLocaleEnable($locale: String!) {
              shopLocaleEnable(locale: $locale) { userErrors { field message code } }
            }"#,
            json!({ "locale": locale }),
        ));
        assert_eq!(
            enable.body["data"]["shopLocaleEnable"]["userErrors"],
            json!([])
        );
    }

    let blank_translation = proxy.process_request(json_graphql_request(
        r#"mutation LocalizationTranslationsRegister($resourceId: ID!, $translations: [TranslationInput!]!) { translationsRegister(resourceId: $resourceId, translations: $translations) { translations { key value locale } userErrors { field message code } } }"#,
        json!({ "resourceId": "gid://shopify/Product/9801098789170", "translations": [{ "locale": "fr", "key": "title", "value": "", "translatableContentDigest": title_digest }] }),
    ));
    assert_eq!(
        blank_translation.body["data"]["translationsRegister"]["userErrors"][0]["code"],
        json!("FAILS_RESOURCE_VALIDATION")
    );

    let normalized_handle = proxy.process_request(json_graphql_request(
        r#"mutation LocalizationTranslationsRegister($resourceId: ID!, $translations: [TranslationInput!]!) { translationsRegister(resourceId: $resourceId, translations: $translations) { translations { key value locale outdated market { id } } userErrors { field message code } } }"#,
        json!({ "resourceId": "gid://shopify/Product/9801098789170", "translations": [{ "locale": "fr", "key": "handle", "value": "Bad Value With Spaces", "translatableContentDigest": handle_digest }] }),
    ));
    assert_eq!(
        normalized_handle.body["data"]["translationsRegister"]["translations"][0]["value"],
        json!("bad-value-with-spaces")
    );

    let unknown_market = proxy.process_request(json_graphql_request(
        r#"mutation LocalizationTranslationsRegister($resourceId: ID!, $translations: [TranslationInput!]!) { translationsRegister(resourceId: $resourceId, translations: $translations) { translations { key value locale outdated market { id } } userErrors { field message code } } }"#,
        json!({ "resourceId": "gid://shopify/Product/9801098789170", "translations": [{ "locale": "es", "key": "title", "value": "Titulo", "translatableContentDigest": title_digest, "marketId": "gid://shopify/Market/999999" }] }),
    ));
    assert_eq!(
        unknown_market.body["data"]["translationsRegister"]["translations"],
        Value::Null
    );
    assert_eq!(
        unknown_market.body["data"]["translationsRegister"]["userErrors"][0]["code"],
        json!("MARKET_DOES_NOT_EXIST")
    );

    let registered = proxy.process_request(json_graphql_request(
        r#"mutation LocalizationTranslationsRegister($resourceId: ID!, $translations: [TranslationInput!]!) { translationsRegister(resourceId: $resourceId, translations: $translations) { translations { key value locale outdated market { id } } userErrors { field message code } } }"#,
        json!({ "resourceId": "gid://shopify/Product/9801098789170", "translations": [{ "locale": "es", "key": "title", "value": "Titulo", "translatableContentDigest": title_digest, "marketId": "gid://shopify/Market/123" }] }),
    ));
    assert_eq!(
        registered.body["data"]["translationsRegister"]["translations"][0]["market"]["id"],
        json!("gid://shopify/Market/123")
    );

    let removed = proxy.process_request(json_graphql_request(
        r#"mutation LocalizationTranslationsMarketScopedRemove($resourceId: ID!, $keys: [String!]!, $locales: [String!]!, $marketIds: [ID!]!) { translationsRemove(resourceId: $resourceId, translationKeys: $keys, locales: $locales, marketIds: $marketIds) { translations { key value locale outdated market { id } } userErrors { field message code } } }"#,
        json!({ "resourceId": "gid://shopify/Product/9801098789170", "keys": ["title"], "locales": ["es"], "marketIds": ["gid://shopify/Market/123"] }),
    ));
    assert_eq!(
        removed.body["data"]["translationsRemove"]["translations"][0]["market"]["id"],
        json!("gid://shopify/Market/123")
    );
    assert_eq!(
        removed.body["data"]["translationsRemove"]["userErrors"],
        json!([])
    );
}

#[test]
fn localization_translations_register_validation_order_matches_shopify_precedence() {
    let mut locale_proxy = snapshot_proxy();

    let non_enabled_blank = locale_proxy.process_request(json_graphql_request(
        r#"mutation LocalizationTranslationsRegister($resourceId: ID!, $translations: [TranslationInput!]!) {
          translationsRegister(resourceId: $resourceId, translations: $translations) {
            translations { key value locale }
            userErrors { field message code }
          }
        }"#,
        json!({
            "resourceId": "gid://shopify/Product/9801098789170",
            "translations": [{
                "locale": "it",
                "key": "title",
                "value": "",
                "translatableContentDigest": "digest"
            }]
        }),
    ));
    assert_eq!(
        non_enabled_blank.body["data"]["translationsRegister"],
        json!({
            "translations": [],
            "userErrors": [{
                "field": ["translations", "0", "locale"],
                "message": "Locale is not a valid locale for the shop",
                "code": "INVALID_LOCALE_FOR_SHOP"
            }]
        })
    );

    let primary_blank = locale_proxy.process_request(json_graphql_request(
        r#"mutation LocalizationTranslationsRegister($resourceId: ID!, $translations: [TranslationInput!]!) {
          translationsRegister(resourceId: $resourceId, translations: $translations) {
            translations { key value locale }
            userErrors { field message code }
          }
        }"#,
        json!({
            "resourceId": "gid://shopify/Product/9801098789170",
            "translations": [{
                "locale": "en",
                "key": "title",
                "value": "",
                "translatableContentDigest": "digest"
            }]
        }),
    ));
    assert_eq!(
        primary_blank.body["data"]["translationsRegister"],
        json!({
            "translations": [],
            "userErrors": [{
                "field": ["translations", "0", "locale"],
                "message": "Locale cannot be the same as the shop's primary locale",
                "code": "INVALID_LOCALE_FOR_SHOP"
            }]
        })
    );

    let mut market_proxy = snapshot_proxy();
    let enable = market_proxy.process_request(json_graphql_request(
        r#"mutation LocalizationShopLocaleEnable($locale: String!) {
          shopLocaleEnable(locale: $locale) { userErrors { field message code } }
        }"#,
        json!({ "locale": "es" }),
    ));
    assert_eq!(
        enable.body["data"]["shopLocaleEnable"]["userErrors"],
        json!([])
    );
    let unknown_market_blank = market_proxy.process_request(json_graphql_request(
        r#"mutation LocalizationTranslationsRegister($resourceId: ID!, $translations: [TranslationInput!]!) {
          translationsRegister(resourceId: $resourceId, translations: $translations) {
            translations { key value locale market { id } }
            userErrors { field message code }
          }
        }"#,
        json!({
            "resourceId": "gid://shopify/Product/9801098789170",
            "translations": [{
                "locale": "es",
                "key": "title",
                "value": "",
                "translatableContentDigest": "digest",
                "marketId": "gid://shopify/Market/999999"
            }]
        }),
    ));
    assert_eq!(
        unknown_market_blank.body["data"]["translationsRegister"],
        json!({
            "translations": null,
            "userErrors": [{
                "field": ["translations", "0", "marketId"],
                "message": "The market corresponding to the `marketId` argument doesn't exist",
                "code": "MARKET_DOES_NOT_EXIST"
            }]
        })
    );
}

#[test]
fn localization_translations_register_stages_locally_and_keeps_raw_mutation_for_commit() {
    let upstream_hits = Arc::new(Mutex::new(0usize));
    let hit_counter = Arc::clone(&upstream_hits);
    let title_digest = fallback_product_title_digest();
    let mut proxy = configured_proxy(
        ReadMode::LiveHybrid,
        Some(shopify_draft_proxy::proxy::UnsupportedMutationMode::Passthrough),
    )
    .with_upstream_transport(move |_request| {
        *hit_counter.lock().unwrap() += 1;
        shopify_draft_proxy::proxy::Response {
            status: 500,
            headers: Default::default(),
            body: json!({ "errors": [{ "message": "translationsRegister should stay local" }] }),
        }
    });

    let enable = proxy.process_request(json_graphql_request(
        r#"mutation LocalizationShopLocaleEnable($locale: String!) {
          shopLocaleEnable(locale: $locale) { userErrors { field message code } }
        }"#,
        json!({ "locale": "fr" }),
    ));
    assert_eq!(
        enable.body["data"]["shopLocaleEnable"]["userErrors"],
        json!([])
    );

    let register = proxy.process_request(json_graphql_request(
        r#"mutation LocalizationTranslationsRegister($resourceId: ID!, $translations: [TranslationInput!]!) {
          translationsRegister(resourceId: $resourceId, translations: $translations) {
            translations { key value locale }
            userErrors { field message code }
          }
        }"#,
        json!({
            "resourceId": "gid://shopify/Product/9801098789170",
            "translations": [{
                "locale": "fr",
                "key": "title",
                "value": "Titre local",
                "translatableContentDigest": title_digest
            }]
        }),
    ));
    assert_eq!(register.status, 200);
    assert_eq!(*upstream_hits.lock().unwrap(), 0);
    assert_eq!(
        register.body["data"]["translationsRegister"]["translations"],
        json!([{ "key": "title", "value": "Titre local", "locale": "fr" }])
    );
    assert_eq!(
        register.body["data"]["translationsRegister"]["userErrors"],
        json!([])
    );

    let log = proxy.get_log_snapshot();
    assert_eq!(log["entries"].as_array().unwrap().len(), 2);
    assert!(log["entries"][1]["rawBody"]
        .as_str()
        .unwrap()
        .contains("mutation LocalizationTranslationsRegister"));
    assert!(log["entries"][1]["rawBody"]
        .as_str()
        .unwrap()
        .contains("Titre local"));
}

#[test]
fn localization_shop_locale_update_disable_tail_helpers_ported_from_gleam() {
    let mut proxy = snapshot_proxy();
    let known_presence = "gid://shopify/MarketWebPresence/known";
    let unknown_presence = "gid://shopify/MarketWebPresence/9999999999";

    let lifecycle = proxy.process_request(json_graphql_request(
        r#"mutation RustLocalizationShopLocaleTailHelpers($known: ID!, $unknown: ID!) {
          enableFr: shopLocaleEnable(locale: "fr") { shopLocale { locale published } userErrors { field message code } }
          publishFr: shopLocaleUpdate(locale: "fr", shopLocale: { published: true, marketWebPresenceIds: [$known, $unknown] }) { shopLocale { locale name published marketWebPresences { id __typename defaultLocale { locale } } } userErrors { field message code } }
          attachMissing: shopLocaleUpdate(locale: "tr", shopLocale: { marketWebPresenceIds: [$known] }) { shopLocale { locale name published marketWebPresences { id __typename defaultLocale { locale } } } userErrors { field message code } }
          missingWithPresenceUnpublish: shopLocaleUpdate(locale: "zz", shopLocale: { published: false, marketWebPresenceIds: [$known] }) { shopLocale { locale } userErrors { field message code } }
          missingWithPresencePublish: shopLocaleUpdate(locale: "zz", shopLocale: { published: true, marketWebPresenceIds: [$known] }) { shopLocale { locale } userErrors { field message code } }
          missingNoPresence: shopLocaleUpdate(locale: "de", shopLocale: { published: true }) { shopLocale { locale } userErrors { field message code } }
          primaryPublish: shopLocaleUpdate(locale: "en", shopLocale: { published: true }) { shopLocale { locale } userErrors { field message code } }
          primaryUnpublish: shopLocaleUpdate(locale: "en", shopLocale: { published: false }) { shopLocale { locale } userErrors { field message code } }
          disablePrimary: shopLocaleDisable(locale: "en") { locale userErrors { field message code } }
          disableUnknown: shopLocaleDisable(locale: "de") { locale userErrors { field message code } }
        }"#,
        json!({ "known": known_presence, "unknown": unknown_presence }),
    ));
    assert_eq!(lifecycle.status, 200);
    assert_eq!(
        lifecycle.body["data"]["enableFr"],
        json!({ "shopLocale": { "locale": "fr", "published": false }, "userErrors": [] })
    );
    assert_eq!(
        lifecycle.body["data"]["publishFr"],
        json!({
            "shopLocale": {
                "locale": "fr",
                "name": "French",
                "published": true,
                "marketWebPresences": [{
                    "id": known_presence,
                    "__typename": "MarketWebPresence",
                    "defaultLocale": { "locale": "en" }
                }]
            },
            "userErrors": []
        })
    );
    assert_eq!(
        lifecycle.body["data"]["attachMissing"],
        json!({
            "shopLocale": {
                "locale": "tr",
                "name": "Turkish",
                "published": false,
                "marketWebPresences": [{
                    "id": known_presence,
                    "__typename": "MarketWebPresence",
                    "defaultLocale": { "locale": "en" }
                }]
            },
            "userErrors": []
        })
    );
    assert_eq!(
        lifecycle.body["data"]["missingNoPresence"],
        json!({
            "shopLocale": null,
            "userErrors": [{
                "field": ["locale"],
                "message": "The locale doesn't exist.",
                "code": "SHOP_LOCALE_DOES_NOT_EXIST"
            }]
        })
    );
    let missing_locale_error = json!({
        "shopLocale": null,
        "userErrors": [{
            "field": ["locale"],
            "message": "The locale doesn't exist.",
            "code": "SHOP_LOCALE_DOES_NOT_EXIST"
        }]
    });
    assert_eq!(
        lifecycle.body["data"]["missingWithPresenceUnpublish"],
        missing_locale_error
    );
    assert_eq!(
        lifecycle.body["data"]["missingWithPresencePublish"],
        missing_locale_error
    );
    assert_eq!(
        lifecycle.body["data"]["primaryUnpublish"],
        json!({
            "shopLocale": null,
            "userErrors": [{
                "field": ["locale"],
                "message": "The primary locale of your store can't be changed through this endpoint.",
                "code": "CAN_NOT_MUTATE_PRIMARY_LOCALE"
            }]
        })
    );
    assert_eq!(
        lifecycle.body["data"]["primaryPublish"],
        json!({
            "shopLocale": null,
            "userErrors": [{
                "field": ["locale"],
                "message": "The primary locale of your store can't be changed through this endpoint.",
                "code": "CAN_NOT_MUTATE_PRIMARY_LOCALE"
            }]
        })
    );
    assert_eq!(
        lifecycle.body["data"]["disablePrimary"],
        json!({
            "locale": null,
            "userErrors": [{
                "field": ["locale"],
                "message": "The primary locale of your store can't be changed through this endpoint.",
                "code": "CAN_NOT_MUTATE_PRIMARY_LOCALE"
            }]
        })
    );
    assert_eq!(
        lifecycle.body["data"]["disableUnknown"],
        json!({
            "locale": null,
            "userErrors": [{
                "field": ["locale"],
                "message": "The locale doesn't exist.",
                "code": "SHOP_LOCALE_DOES_NOT_EXIST"
            }]
        })
    );

    let read = proxy.process_request(json_graphql_request(
        r#"query RustLocalizationShopLocaleTailHelpersRead {
          allLocales: shopLocales { locale published marketWebPresences { id __typename defaultLocale { locale } } }
          publishedLocales: shopLocales(published: true) { locale published }
        }"#,
        json!({}),
    ));
    let all_locales = read.body["data"]["allLocales"].as_array().unwrap();
    let staged_fr = all_locales
        .iter()
        .find(|locale| locale["locale"] == json!("fr"))
        .unwrap();
    assert_eq!(staged_fr["published"], json!(true));
    assert_eq!(
        staged_fr["marketWebPresences"],
        json!([{ "id": known_presence, "__typename": "MarketWebPresence", "defaultLocale": { "locale": "en" } }])
    );
    assert!(all_locales
        .iter()
        .any(|locale| locale["locale"] == json!("tr")));
    assert!(!all_locales
        .iter()
        .any(|locale| locale["locale"] == json!("zz")));
    assert!(read.body["data"]["publishedLocales"]
        .as_array()
        .unwrap()
        .iter()
        .any(|locale| locale["locale"] == json!("fr")));

    let disabled = proxy.process_request(json_graphql_request(
        r#"mutation RustLocalizationShopLocaleTailHelpersDisable { shopLocaleDisable(locale: "fr") { locale userErrors { field message code } } }"#,
        json!({}),
    ));
    assert_eq!(
        disabled.body["data"]["shopLocaleDisable"],
        json!({ "locale": "fr", "userErrors": [] })
    );

    let after_disable = proxy.process_request(json_graphql_request(
        r#"query RustLocalizationShopLocaleTailHelpersReadAfterDisable { shopLocales { locale published } }"#,
        json!({}),
    ));
    assert!(!after_disable.body["data"]["shopLocales"]
        .as_array()
        .unwrap()
        .iter()
        .any(|locale| locale["locale"] == json!("fr")));
}

#[test]
fn localization_locale_cap_register_guards_and_remove_combinations_match_captured_behavior() {
    let mut proxy = snapshot_proxy();
    let title_digest = fallback_product_title_digest();
    // Stage 20 non-primary locales (the snapshot's primary "en" is excluded from the
    // cap count) so that the 21st enable below trips Shopify's 20-language limit. The
    // `localization-shop-locale-enable-validation` parity scenario proves the cap fires
    // only once 20 alternate locales are already present.
    let locale_codes = [
        "fr", "af", "ak", "sq", "am", "ar", "hy", "as", "az", "bm", "bn", "eu", "be", "bs", "br",
        "bg", "my", "ca", "ckb", "ce",
    ];
    for locale in locale_codes {
        let enable = proxy.process_request(json_graphql_request(
            r#"mutation LocalizationShopLocaleEnable($locale: String!) {
              shopLocaleEnable(locale: $locale) {
                shopLocale { locale }
                userErrors { field message code }
              }
            }"#,
            json!({ "locale": locale }),
        ));
        assert_eq!(
            enable.body["data"]["shopLocaleEnable"]["userErrors"],
            json!([])
        );
    }

    let over_limit = proxy.process_request(json_graphql_request(
        r#"mutation LocalizationShopLocaleEnable($locale: String!) {
          shopLocaleEnable(locale: $locale) {
            shopLocale { locale }
            userErrors { field message code }
          }
        }"#,
        json!({ "locale": "zh-CN" }),
    ));
    assert_eq!(
        over_limit.body["data"]["shopLocaleEnable"],
        json!({
            "shopLocale": null,
            "userErrors": [{
                "field": null,
                "message": "Your store has reached its 20 language limit. To add Chinese (Simplified), delete one of your other languages.",
                "code": "SHOP_LOCALE_LIMIT_REACHED"
            }]
        })
    );

    let mut guard_proxy = snapshot_proxy();
    let non_enabled = guard_proxy.process_request(json_graphql_request(
        r#"mutation LocalizationTranslationsRegister($resourceId: ID!, $translations: [TranslationInput!]!) {
          translationsRegister(resourceId: $resourceId, translations: $translations) {
            translations { key value locale }
            userErrors { field message code }
          }
        }"#,
        json!({
            "resourceId": "gid://shopify/Product/9801098789170",
            "translations": [{
                "locale": "es",
                "key": "title",
                "value": "Titulo local",
                "translatableContentDigest": "digest"
            }]
        }),
    ));
    assert_eq!(
        non_enabled.body["data"]["translationsRegister"],
        json!({
            "translations": [],
            "userErrors": [{
                "field": ["translations", "0", "locale"],
                "message": "Locale is not a valid locale for the shop",
                "code": "INVALID_LOCALE_FOR_SHOP"
            }]
        })
    );

    let primary_locale = guard_proxy.process_request(json_graphql_request(
        r#"mutation LocalizationTranslationsRegister($resourceId: ID!, $translations: [TranslationInput!]!) {
          translationsRegister(resourceId: $resourceId, translations: $translations) {
            translations { key value locale }
            userErrors { field message code }
          }
        }"#,
        json!({
            "resourceId": "gid://shopify/Product/9801098789170",
            "translations": [{
                "locale": "en",
                "key": "title",
                "value": "Primary title",
                "translatableContentDigest": "digest"
            }]
        }),
    ));
    assert_eq!(
        primary_locale.body["data"]["translationsRegister"]["userErrors"][0]["code"],
        json!("INVALID_LOCALE_FOR_SHOP")
    );

    let mut remove_proxy = snapshot_proxy();
    for locale in ["es", "fr"] {
        let enable = remove_proxy.process_request(json_graphql_request(
            r#"mutation LocalizationShopLocaleEnable($locale: String!) {
              shopLocaleEnable(locale: $locale) { userErrors { field message code } }
            }"#,
            json!({ "locale": locale }),
        ));
        assert_eq!(
            enable.body["data"]["shopLocaleEnable"]["userErrors"],
            json!([])
        );
    }
    let register = remove_proxy.process_request(json_graphql_request(
        r#"mutation LocalizationTranslationsRegister($resourceId: ID!, $translations: [TranslationInput!]!) {
          translationsRegister(resourceId: $resourceId, translations: $translations) {
            translations { key value locale market { id } }
            userErrors { field message code }
          }
        }"#,
        json!({
            "resourceId": "gid://shopify/Product/9801098789170",
            "translations": [
                { "locale": "es", "key": "title", "value": "Titulo", "translatableContentDigest": title_digest, "marketId": "gid://shopify/Market/123" },
                { "locale": "es", "key": "body_html", "value": "Cuerpo", "translatableContentDigest": "digest", "marketId": "gid://shopify/Market/123" },
                { "locale": "fr", "key": "title", "value": "Titre", "translatableContentDigest": title_digest }
            ]
        }),
    ));
    assert_eq!(
        register.body["data"]["translationsRegister"]["translations"]
            .as_array()
            .unwrap()
            .len(),
        3
    );

    let remove = remove_proxy.process_request(json_graphql_request(
        r#"mutation LocalizationTranslationsMarketScopedRemove($resourceId: ID!, $keys: [String!]!, $locales: [String!]!, $marketIds: [ID!]!) {
          translationsRemove(resourceId: $resourceId, translationKeys: $keys, locales: $locales, marketIds: $marketIds) {
            translations { key value locale market { id } }
            userErrors { field message code }
          }
        }"#,
        json!({
            "resourceId": "gid://shopify/Product/9801098789170",
            "keys": ["title", "body_html"],
            "locales": ["es", "fr"],
            "marketIds": ["gid://shopify/Market/123"]
        }),
    ));
    let removed = remove.body["data"]["translationsRemove"]["translations"]
        .as_array()
        .unwrap();
    assert_eq!(removed.len(), 2);
    assert!(removed
        .iter()
        .any(|translation| translation["key"] == json!("title")));
    assert!(removed
        .iter()
        .any(|translation| translation["key"] == json!("body_html")));

    let read_after_remove = remove_proxy.process_request(json_graphql_request(
        r#"query LocalizationTranslationsMarketScopedRead($resourceId: ID!) {
          translatableResource(resourceId: $resourceId) {
            translations(locale: "fr") {
              key value locale market { id }
            }
          }
        }"#,
        json!({ "resourceId": "gid://shopify/Product/9801098789170" }),
    ));
    assert_eq!(
        read_after_remove.body["data"]["translatableResource"]["translations"],
        json!([{
            "key": "title",
            "value": "Titre",
            "locale": "fr",
            "market": null
        }])
    );
}

#[test]
fn gift_card_update_validation_rejects_deactivated_empty_missing_and_long_inputs_and_allows_note() {
    let mut proxy = snapshot_proxy();

    let response = proxy.process_request(json_graphql_request(
        r#"mutation GiftCardUpdateValidation($activeId: ID!, $deactivatedId: ID!, $missingCustomerId: ID!, $recipientId: ID!, $tooLongPreferredName: String!, $tooLongMessage: String!, $successNote: String!) {
          deactivatedExpiresOn: giftCardUpdate(id: $deactivatedId, input: { expiresOn: "2099-12-31" }) { giftCard { id enabled expiresOn } userErrors { field code message } }
          emptyInput: giftCardUpdate(id: $activeId, input: {}) { giftCard { id note } userErrors { field code message } }
          missingCustomer: giftCardUpdate(id: $activeId, input: { customerId: $missingCustomerId }) { giftCard { id customer { id } } userErrors { field code message } }
          longRecipientName: giftCardUpdate(id: $activeId, input: { recipientAttributes: { id: $recipientId, preferredName: $tooLongPreferredName } }) { giftCard { id recipientAttributes { preferredName recipient { id } } } userErrors { field code message } }
          longRecipientMessage: giftCardUpdate(id: $activeId, input: { recipientAttributes: { id: $recipientId, message: $tooLongMessage } }) { giftCard { id recipientAttributes { message recipient { id } } } userErrors { field code message } }
          success: giftCardUpdate(id: $activeId, input: { note: $successNote }) { giftCard { id note updatedAt } userErrors { field code message } }
        }"#,
        json!({
            "activeId": "gid://shopify/GiftCard/har694-active",
            "deactivatedId": "gid://shopify/GiftCard/har694-deactivated",
            "missingCustomerId": "gid://shopify/Customer/999999999999",
            "recipientId": "gid://shopify/Customer/10582524297522",
            "tooLongPreferredName": "x".repeat(256),
            "tooLongMessage": "x".repeat(201),
            "successNote": "HAR-694 updated note"
        }),
    ));

    assert_eq!(
        response.body["data"],
        json!({
            "deactivatedExpiresOn": { "giftCard": null, "userErrors": [{ "field": ["input", "expiresOn"], "message": "The gift card is deactivated.", "code": "INVALID" }] },
            "emptyInput": { "giftCard": null, "userErrors": [{ "field": ["input"], "message": "At least one argument is required in the input.", "code": "INVALID" }] },
            "missingCustomer": { "giftCard": null, "userErrors": [{ "field": ["input", "customerId"], "message": "The customer could not be found.", "code": "CUSTOMER_NOT_FOUND" }] },
            "longRecipientName": { "giftCard": null, "userErrors": [{ "field": ["input", "recipientAttributes", "preferredName"], "code": "TOO_LONG", "message": "preferredName is too long (maximum is 255)" }] },
            "longRecipientMessage": { "giftCard": null, "userErrors": [{ "field": ["input", "recipientAttributes", "message"], "code": "TOO_LONG", "message": "message is too long (maximum is 200)" }] },
            "success": { "giftCard": { "id": "gid://shopify/GiftCard/har694-active", "note": "HAR-694 updated note", "updatedAt": "2024-01-01T00:00:00.000Z" }, "userErrors": [] }
        })
    );
}

#[test]
fn gift_card_update_noop_accepts_same_values_and_rejects_empty_input() {
    let mut proxy = snapshot_proxy();

    let response = proxy.process_request(json_graphql_request(
        r#"mutation GiftCardUpdateNoop($id: ID!, $note: String!, $expiresOn: Date!, $templateSuffix: String!) {
          noteNoop: giftCardUpdate(id: $id, input: { note: $note }) { giftCard { id note updatedAt } userErrors { field code message } }
          expiresNoop: giftCardUpdate(id: $id, input: { expiresOn: $expiresOn }) { giftCard { id expiresOn updatedAt } userErrors { field code message } }
          templateNoop: giftCardUpdate(id: $id, input: { templateSuffix: $templateSuffix }) { giftCard { id templateSuffix updatedAt } userErrors { field code message } }
          emptyInput: giftCardUpdate(id: $id, input: {}) { giftCard { id note } userErrors { field code message } }
        }"#,
        json!({
            "id": "gid://shopify/GiftCard/1?shopify-draft-proxy=synthetic",
            "note": "HAR-766 no-op current note",
            "expiresOn": "2030-01-01",
            "templateSuffix": "birthday"
        }),
    ));

    assert_eq!(
        response.body["data"],
        json!({
            "noteNoop": { "giftCard": { "id": "gid://shopify/GiftCard/1?shopify-draft-proxy=synthetic", "note": "HAR-766 no-op current note", "updatedAt": "2024-01-01T00:00:00.000Z" }, "userErrors": [] },
            "expiresNoop": { "giftCard": { "id": "gid://shopify/GiftCard/1?shopify-draft-proxy=synthetic", "expiresOn": "2030-01-01", "updatedAt": "2024-01-01T00:00:00.000Z" }, "userErrors": [] },
            "templateNoop": { "giftCard": { "id": "gid://shopify/GiftCard/1?shopify-draft-proxy=synthetic", "templateSuffix": "birthday", "updatedAt": "2024-01-01T00:00:00.000Z" }, "userErrors": [] },
            "emptyInput": { "giftCard": null, "userErrors": [{ "field": ["input"], "message": "At least one argument is required in the input.", "code": "INVALID" }] }
        })
    );
}

#[test]
fn gift_card_update_deactivated_multi_field_prioritizes_deactivated_errors() {
    let mut proxy = snapshot_proxy();

    let response = proxy.process_request(json_graphql_request(
        r#"mutation GiftCardUpdateDeactivatedMultiField($deactivatedId: ID!, $customerId: ID!, $recipientId: ID!) {
          expiresAndCustomer: giftCardUpdate(id: $deactivatedId, input: { expiresOn: "2099-12-31", customerId: $customerId }) { giftCard { id } userErrors { field code message } }
          customerAndRecipient: giftCardUpdate(id: $deactivatedId, input: { customerId: $customerId, recipientAttributes: { id: $recipientId } }) { giftCard { id } userErrors { field code message } }
          customerRecipientAndExpires: giftCardUpdate(id: $deactivatedId, input: { customerId: $customerId, recipientAttributes: { id: $recipientId }, expiresOn: "2099-12-31" }) { giftCard { id } userErrors { field code message } }
        }"#,
        json!({
            "deactivatedId": "gid://shopify/GiftCard/deactivated",
            "customerId": "gid://shopify/Customer/1",
            "recipientId": "gid://shopify/Customer/1"
        }),
    ));

    assert_eq!(
        response.body["data"],
        json!({
            "expiresAndCustomer": { "giftCard": null, "userErrors": [{ "field": ["input", "expiresOn"], "message": "The gift card is deactivated.", "code": "INVALID" }] },
            "customerAndRecipient": { "giftCard": null, "userErrors": [{ "field": ["input", "customerId"], "message": "The gift card is deactivated.", "code": "INVALID" }] },
            "customerRecipientAndExpires": { "giftCard": null, "userErrors": [{ "field": ["input", "expiresOn"], "message": "The gift card is deactivated.", "code": "INVALID" }] }
        })
    );
}

#[test]
fn gift_card_trial_shop_assignment_rejects_customer_and_recipient_assignment() {
    let mut proxy = snapshot_proxy();

    let response = proxy.process_request(json_graphql_request(
        r#"mutation GiftCardTrialShopAssignment($customerId: ID!, $recipientId: ID!, $updateGiftCardId: ID!) {
          createCustomerAssignment: giftCardCreate(input: { initialValue: "10", customerId: $customerId }) { giftCard { id } giftCardCode userErrors { field code message } }
          createRecipientAssignment: giftCardCreate(input: { initialValue: "10", recipientAttributes: { id: $recipientId } }) { giftCard { id } giftCardCode userErrors { field code message } }
          updateCustomerAssignment: giftCardUpdate(id: $updateGiftCardId, input: { customerId: $customerId }) { giftCard { id } userErrors { field code message } }
          updateRecipientAssignment: giftCardUpdate(id: $updateGiftCardId, input: { recipientAttributes: { id: $recipientId } }) { giftCard { id } userErrors { field code message } }
        }"#,
        json!({
            "customerId": "gid://shopify/Customer/trial-customer",
            "recipientId": "gid://shopify/Customer/trial-recipient",
            "updateGiftCardId": "gid://shopify/GiftCard/trial-update-card"
        }),
    ));

    assert_eq!(
        response.body["data"],
        json!({
            "createCustomerAssignment": { "giftCard": null, "giftCardCode": null, "userErrors": [{ "field": ["input", "customerId"], "code": "INVALID", "message": "A trial shop cannot assign a customer to a gift card." }] },
            "createRecipientAssignment": { "giftCard": null, "giftCardCode": null, "userErrors": [{ "field": ["input", "recipientAttributes"], "code": "INVALID", "message": "A trial shop cannot assign a recipient to a gift card." }] },
            "updateCustomerAssignment": { "giftCard": null, "userErrors": [{ "field": ["input", "customerId"], "code": "INVALID", "message": "A trial shop cannot assign a customer to a gift card." }] },
            "updateRecipientAssignment": { "giftCard": null, "userErrors": [{ "field": ["input", "recipientAttributes"], "code": "INVALID", "message": "A trial shop cannot assign a recipient to a gift card." }] }
        })
    );
}

#[test]
fn gift_card_notification_trial_shop_rejects_customer_and_recipient_notifications() {
    let mut proxy = snapshot_proxy();

    let response = proxy.process_request(json_graphql_request(
        r#"mutation GiftCardNotificationTrialShop($id: ID!) {
          customerNotification: giftCardSendNotificationToCustomer(id: $id) { giftCard { id } userErrors { field code message } }
          recipientNotification: giftCardSendNotificationToRecipient(id: $id) { giftCard { id } userErrors { field code message } }
        }"#,
        json!({ "id": "gid://shopify/GiftCard/trial-update-card" }),
    ));

    let trial_error = json!([{
        "field": ["base"],
        "code": "INVALID",
        "message": "Notifications are not available on trial shops."
    }]);
    assert_eq!(
        response.body["data"],
        json!({
            "customerNotification": { "giftCard": null, "userErrors": trial_error },
            "recipientNotification": { "giftCard": null, "userErrors": trial_error }
        })
    );
    assert_eq!(proxy.get_log_snapshot()["entries"], json!([]));
}

#[test]
fn gift_card_notification_entitlement_wins_before_trial_and_trial_wins_before_card_state() {
    let mut proxy = snapshot_proxy();

    let response = proxy.process_request(json_graphql_request(
        r#"mutation GiftCardNotificationPriority {
          entitlementBeforeTrial: giftCardSendNotificationToCustomer(id: "gid://shopify/GiftCard/disabled-entitlement-trial-notification") { giftCard { id } userErrors { field code message } }
          trialBeforeMissing: giftCardSendNotificationToCustomer(id: "gid://shopify/GiftCard/trial-missing-notification") { giftCard { id } userErrors { field code message } }
          trialBeforeNotifyDisabled: giftCardSendNotificationToCustomer(id: "gid://shopify/GiftCard/trial-notify-disabled-notification") { giftCard { id } userErrors { field code message } }
          trialBeforeExpired: giftCardSendNotificationToCustomer(id: "gid://shopify/GiftCard/trial-expired-notification") { giftCard { id } userErrors { field code message } }
          trialBeforeDeactivated: giftCardSendNotificationToCustomer(id: "gid://shopify/GiftCard/trial-deactivated-notification") { giftCard { id } userErrors { field code message } }
          trialBeforeNoCustomer: giftCardSendNotificationToCustomer(id: "gid://shopify/GiftCard/trial-no-customer-notification") { giftCard { id } userErrors { field code message } }
          trialBeforeNoContact: giftCardSendNotificationToRecipient(id: "gid://shopify/GiftCard/trial-no-contact-notification") { giftCard { id } userErrors { field code message } }
        }"#,
        json!({}),
    ));

    let entitlement_error = json!([{ "field": ["base"], "code": null, "message": "Gift cards are unavailable on your plan." }]);
    let trial_error = json!([{
        "field": ["base"],
        "code": "INVALID",
        "message": "Notifications are not available on trial shops."
    }]);
    assert_eq!(
        response.body["data"],
        json!({
            "entitlementBeforeTrial": { "giftCard": null, "userErrors": entitlement_error },
            "trialBeforeMissing": { "giftCard": null, "userErrors": trial_error },
            "trialBeforeNotifyDisabled": { "giftCard": null, "userErrors": trial_error },
            "trialBeforeExpired": { "giftCard": null, "userErrors": trial_error },
            "trialBeforeDeactivated": { "giftCard": null, "userErrors": trial_error },
            "trialBeforeNoCustomer": { "giftCard": null, "userErrors": trial_error },
            "trialBeforeNoContact": { "giftCard": null, "userErrors": trial_error }
        })
    );
    assert_eq!(proxy.get_log_snapshot()["entries"], json!([]));
}

#[test]
fn gift_card_notification_uses_hydrated_trial_shop_plan() {
    let mut proxy = snapshot_proxy();
    let dump = proxy
        .process_request(request_with_body(
            "POST",
            "/__meta/dump",
            &json!({ "createdAt": "2026-06-16T00:00:00.000Z" }).to_string(),
        ))
        .body;
    let mut restored = dump.clone();
    restored["state"]["baseState"]["shop"]["plan"] = json!({
        "partnerDevelopment": false,
        "publicDisplayName": "Trial",
        "shopifyPlus": false
    });
    let restore = proxy.process_request(request_with_body(
        "POST",
        "/__meta/restore",
        &restored.to_string(),
    ));
    assert_eq!(restore.status, 200);

    let response = proxy.process_request(json_graphql_request(
        r#"mutation GiftCardNotificationTrialPlan($id: ID!) {
          customerNotification: giftCardSendNotificationToCustomer(id: $id) { giftCard { id } userErrors { field code message } }
          recipientNotification: giftCardSendNotificationToRecipient(id: $id) { giftCard { id } userErrors { field code message } }
        }"#,
        json!({ "id": "gid://shopify/GiftCard/654773256498" }),
    ));

    let trial_error = json!([{
        "field": ["base"],
        "code": "INVALID",
        "message": "Notifications are not available on trial shops."
    }]);
    assert_eq!(
        response.body["data"],
        json!({
            "customerNotification": { "giftCard": null, "userErrors": trial_error },
            "recipientNotification": { "giftCard": null, "userErrors": trial_error }
        })
    );
    assert_eq!(proxy.get_log_snapshot()["entries"], json!([]));
}

#[test]
fn gift_card_transaction_validation_rejects_state_currency_dates_and_allows_success_credit() {
    let mut proxy = snapshot_proxy();

    let response = proxy.process_request(json_graphql_request(
        r#"mutation GiftCardTransactionValidation($activeId: ID!, $expiredId: ID!, $deactivatedId: ID!, $validCreditInput: GiftCardCreditInput!, $mismatchCreditInput: GiftCardCreditInput!, $futureCreditInput: GiftCardCreditInput!, $preEpochCreditInput: GiftCardCreditInput!, $validDebitInput: GiftCardDebitInput!, $futureDebitInput: GiftCardDebitInput!, $preEpochDebitInput: GiftCardDebitInput!) {
          expiredCredit: giftCardCredit(id: $expiredId, creditInput: $validCreditInput) { giftCardCreditTransaction { id __typename processedAt amount { amount currencyCode } } userErrors { field code message } }
          expiredDebit: giftCardDebit(id: $expiredId, debitInput: $validDebitInput) { giftCardDebitTransaction { id __typename processedAt amount { amount currencyCode } } userErrors { field code message } }
          deactivatedCredit: giftCardCredit(id: $deactivatedId, creditInput: $validCreditInput) { giftCardCreditTransaction { id __typename processedAt amount { amount currencyCode } } userErrors { field code message } }
          mismatchCredit: giftCardCredit(id: $activeId, creditInput: $mismatchCreditInput) { giftCardCreditTransaction { id __typename processedAt amount { amount currencyCode } } userErrors { field code message } }
          futureCredit: giftCardCredit(id: $activeId, creditInput: $futureCreditInput) { giftCardCreditTransaction { id __typename processedAt amount { amount currencyCode } } userErrors { field code message } }
          preEpochCredit: giftCardCredit(id: $activeId, creditInput: $preEpochCreditInput) { giftCardCreditTransaction { id __typename processedAt amount { amount currencyCode } } userErrors { field code message } }
          futureDebit: giftCardDebit(id: $activeId, debitInput: $futureDebitInput) { giftCardDebitTransaction { id __typename processedAt amount { amount currencyCode } } userErrors { field code message } }
          preEpochDebit: giftCardDebit(id: $activeId, debitInput: $preEpochDebitInput) { giftCardDebitTransaction { id __typename processedAt amount { amount currencyCode } } userErrors { field code message } }
          deactivatedDebit: giftCardDebit(id: $deactivatedId, debitInput: $validDebitInput) { giftCardDebitTransaction { id __typename processedAt amount { amount currencyCode } } userErrors { field code message } }
          successCredit: giftCardCredit(id: $activeId, creditInput: $validCreditInput) { giftCardCreditTransaction { id __typename processedAt amount { amount currencyCode } } userErrors { field code message } }
        }"#,
        json!({
            "activeId": "gid://shopify/GiftCard/654808252722",
            "expiredId": "gid://shopify/GiftCard/654808285490",
            "deactivatedId": "gid://shopify/GiftCard/654808318258",
            "validCreditInput": { "creditAmount": { "amount": "5.00", "currencyCode": "CAD" } },
            "mismatchCreditInput": { "creditAmount": { "amount": "5.00", "currencyCode": "EUR" } },
            "futureCreditInput": { "processedAt": "2030-01-01T00:00:00Z", "creditAmount": { "amount": "5.00", "currencyCode": "CAD" } },
            "preEpochCreditInput": { "processedAt": "1960-01-01T00:00:00Z", "creditAmount": { "amount": "5.00", "currencyCode": "CAD" } },
            "validDebitInput": { "debitAmount": { "amount": "5.00", "currencyCode": "CAD" } },
            "futureDebitInput": { "processedAt": "2030-01-01T00:00:00Z", "debitAmount": { "amount": "5.00", "currencyCode": "CAD" } },
            "preEpochDebitInput": { "processedAt": "1960-01-01T00:00:00Z", "debitAmount": { "amount": "5.00", "currencyCode": "CAD" } }
        }),
    ));

    assert_eq!(
        response.body["data"],
        json!({
            "expiredCredit": { "giftCardCreditTransaction": null, "userErrors": [{ "field": ["id"], "code": "INVALID", "message": "The gift card has expired." }] },
            "expiredDebit": { "giftCardDebitTransaction": null, "userErrors": [{ "field": ["id"], "code": "INVALID", "message": "The gift card has expired." }] },
            "deactivatedCredit": { "giftCardCreditTransaction": null, "userErrors": [{ "field": ["id"], "code": "INVALID", "message": "The gift card is deactivated." }] },
            "mismatchCredit": { "giftCardCreditTransaction": null, "userErrors": [{ "field": ["creditInput", "creditAmount", "currencyCode"], "code": "MISMATCHING_CURRENCY", "message": "The currency provided does not match the currency of the gift card." }] },
            "futureCredit": { "giftCardCreditTransaction": null, "userErrors": [{ "field": ["creditInput", "processedAt"], "code": "INVALID", "message": "The processed date must not be in the future." }] },
            "preEpochCredit": { "giftCardCreditTransaction": null, "userErrors": [{ "field": ["creditInput", "processedAt"], "code": "INVALID", "message": "A valid processed date must be used." }] },
            "futureDebit": { "giftCardDebitTransaction": null, "userErrors": [{ "field": ["debitInput", "processedAt"], "code": "INVALID", "message": "The processed date must not be in the future." }] },
            "preEpochDebit": { "giftCardDebitTransaction": null, "userErrors": [{ "field": ["debitInput", "processedAt"], "code": "INVALID", "message": "A valid processed date must be used." }] },
            "deactivatedDebit": { "giftCardDebitTransaction": null, "userErrors": [{ "field": ["id"], "code": "INVALID", "message": "The gift card is deactivated." }] },
            "successCredit": { "giftCardCreditTransaction": { "id": "gid://shopify/GiftCardCreditTransaction/1", "__typename": "GiftCardCreditTransaction", "processedAt": "2026-04-29T09:31:02Z", "amount": { "amount": "5.0", "currencyCode": "CAD" } }, "userErrors": [] }
        })
    );

    let read = proxy.process_request(json_graphql_request(
        r#"query GiftCardTransactionValidationRead($id: ID!) {
          giftCard(id: $id) {
            balance { amount currencyCode }
            transactions(first: 5) {
              nodes { processedAt amount { amount currencyCode } }
            }
          }
        }"#,
        json!({ "id": "gid://shopify/GiftCard/654808252722" }),
    ));
    assert_eq!(
        read.body["data"]["giftCard"],
        json!({
            "balance": { "amount": "10.0", "currencyCode": "CAD" },
            "transactions": {
                "nodes": [{
                    "processedAt": "2026-04-29T09:31:02Z",
                    "amount": { "amount": "5.0", "currencyCode": "CAD" }
                }]
            }
        })
    );
}

#[test]
fn gift_card_recipient_validation_rejects_length_html_and_send_at_bounds() {
    let mut proxy = snapshot_proxy();

    let response = proxy.process_request(json_graphql_request(
        r#"mutation GiftCardRecipientValidation(
          $activeId: ID!,
          $recipientId: ID!,
          $tooLongPreferredName: String!,
          $tooLongMessage: String!,
          $htmlPreferredName: String!,
          $htmlMessage: String!,
          $futureSendAt: DateTime!,
          $pastSendAt: DateTime!,
          $validSendAt: DateTime!
        ) {
          createLongPreferredName: giftCardCreate(input: { initialValue: "10", recipientAttributes: { id: $recipientId, preferredName: $tooLongPreferredName } }) { giftCard { id recipientAttributes { preferredName } } giftCardCode userErrors { field code message } }
          createLongMessage: giftCardCreate(input: { initialValue: "10", recipientAttributes: { id: $recipientId, message: $tooLongMessage } }) { giftCard { id recipientAttributes { message } } giftCardCode userErrors { field code message } }
          createHtmlPreferredName: giftCardCreate(input: { initialValue: "10", recipientAttributes: { id: $recipientId, preferredName: $htmlPreferredName } }) { giftCard { id recipientAttributes { preferredName } } giftCardCode userErrors { field code message } }
          createHtmlMessage: giftCardCreate(input: { initialValue: "10", recipientAttributes: { id: $recipientId, message: $htmlMessage } }) { giftCard { id recipientAttributes { message } } giftCardCode userErrors { field code message } }
          createFutureSendAt: giftCardCreate(input: { initialValue: "10", recipientAttributes: { id: $recipientId, sendNotificationAt: $futureSendAt } }) { giftCard { id recipientAttributes { sendNotificationAt } } giftCardCode userErrors { field code message } }
          createPastSendAt: giftCardCreate(input: { initialValue: "10", recipientAttributes: { id: $recipientId, sendNotificationAt: $pastSendAt } }) { giftCard { id recipientAttributes { sendNotificationAt } } giftCardCode userErrors { field code message } }
          createValidSendAt: giftCardCreate(input: { initialValue: "10", recipientAttributes: { id: $recipientId, sendNotificationAt: $validSendAt } }) { giftCard { id recipientAttributes { sendNotificationAt } } giftCardCode userErrors { field code message } }
          updateLongPreferredName: giftCardUpdate(id: $activeId, input: { recipientAttributes: { id: $recipientId, preferredName: $tooLongPreferredName } }) { giftCard { id recipientAttributes { preferredName } } userErrors { field code message } }
          updateLongMessage: giftCardUpdate(id: $activeId, input: { recipientAttributes: { id: $recipientId, message: $tooLongMessage } }) { giftCard { id recipientAttributes { message } } userErrors { field code message } }
          updateHtmlPreferredName: giftCardUpdate(id: $activeId, input: { recipientAttributes: { id: $recipientId, preferredName: $htmlPreferredName } }) { giftCard { id recipientAttributes { preferredName } } userErrors { field code message } }
          updateHtmlMessage: giftCardUpdate(id: $activeId, input: { recipientAttributes: { id: $recipientId, message: $htmlMessage } }) { giftCard { id recipientAttributes { message } } userErrors { field code message } }
          updatePastSendAt: giftCardUpdate(id: $activeId, input: { recipientAttributes: { id: $recipientId, sendNotificationAt: $pastSendAt } }) { giftCard { id recipientAttributes { sendNotificationAt } } userErrors { field code message } }
          updateFutureSendAt: giftCardUpdate(id: $activeId, input: { recipientAttributes: { id: $recipientId, sendNotificationAt: $futureSendAt } }) { giftCard { id recipientAttributes { sendNotificationAt } } userErrors { field code message } }
          updateValidSendAt: giftCardUpdate(id: $activeId, input: { recipientAttributes: { id: $recipientId, sendNotificationAt: $validSendAt } }) { giftCard { id recipientAttributes { sendNotificationAt } } userErrors { field code message } }
        }"#,
        json!({
            "activeId": "gid://shopify/GiftCard/1?shopify-draft-proxy=synthetic",
            "recipientId": "gid://shopify/Customer/10587888714034",
            "tooLongPreferredName": "x".repeat(256),
            "tooLongMessage": "x".repeat(201),
            "htmlPreferredName": "<b>Recipient</b>",
            "htmlMessage": "<script>alert(1)</script>",
            "futureSendAt": "2026-10-01T00:00:00Z",
            "pastSendAt": "2026-04-28T09:31:02Z",
            "validSendAt": "2026-07-01T00:00:00Z"
        }),
    ));

    assert_eq!(
        response.body["data"],
        json!({
            "createLongPreferredName": { "giftCard": null, "userErrors": [{ "field": ["input", "recipientAttributes", "preferredName"], "code": "TOO_LONG", "message": "preferredName is too long (maximum is 255)" }], "giftCardCode": null },
            "createLongMessage": { "giftCard": null, "userErrors": [{ "field": ["input", "recipientAttributes", "message"], "code": "TOO_LONG", "message": "message is too long (maximum is 200)" }], "giftCardCode": null },
            "createHtmlPreferredName": { "giftCard": null, "userErrors": [{ "field": ["input", "recipientAttributes", "preferredName"], "code": "INVALID", "message": "Preferred name cannot contain HTML tags" }], "giftCardCode": null },
            "createHtmlMessage": { "giftCard": null, "userErrors": [{ "field": ["input", "recipientAttributes", "message"], "code": "INVALID", "message": "Message cannot contain HTML tags" }], "giftCardCode": null },
            "createFutureSendAt": { "giftCard": null, "userErrors": [{ "field": ["input", "recipientAttributes", "sendNotificationAt"], "code": "INVALID", "message": "Send notification at must be within 90 days from now" }], "giftCardCode": null },
            "createPastSendAt": { "giftCard": null, "userErrors": [{ "field": ["input", "recipientAttributes", "sendNotificationAt"], "code": "INVALID", "message": "Send notification at must be within 90 days from now" }], "giftCardCode": null },
            "createValidSendAt": { "giftCard": { "id": "gid://shopify/GiftCard/1?shopify-draft-proxy=synthetic", "recipientAttributes": { "sendNotificationAt": "2026-07-01T00:00:00Z" } }, "giftCardCode": "giftcard00000001", "userErrors": [] },
            "updateLongPreferredName": { "giftCard": null, "userErrors": [{ "field": ["input", "recipientAttributes", "preferredName"], "code": "TOO_LONG", "message": "preferredName is too long (maximum is 255)" }] },
            "updateLongMessage": { "giftCard": null, "userErrors": [{ "field": ["input", "recipientAttributes", "message"], "code": "TOO_LONG", "message": "message is too long (maximum is 200)" }] },
            "updateHtmlPreferredName": { "giftCard": null, "userErrors": [{ "field": ["input", "recipientAttributes", "preferredName"], "code": "INVALID", "message": "Preferred name cannot contain HTML tags" }] },
            "updateHtmlMessage": { "giftCard": null, "userErrors": [{ "field": ["input", "recipientAttributes", "message"], "code": "INVALID", "message": "Message cannot contain HTML tags" }] },
            "updatePastSendAt": { "giftCard": null, "userErrors": [{ "field": ["input", "recipientAttributes", "sendNotificationAt"], "code": "INVALID", "message": "Send notification at must be within 90 days from now" }] },
            "updateFutureSendAt": { "giftCard": null, "userErrors": [{ "field": ["input", "recipientAttributes", "sendNotificationAt"], "code": "INVALID", "message": "Send notification at must be within 90 days from now" }] },
            "updateValidSendAt": { "giftCard": { "id": "gid://shopify/GiftCard/1?shopify-draft-proxy=synthetic", "recipientAttributes": { "sendNotificationAt": "2026-07-01T00:00:00Z" } }, "userErrors": [] }
        })
    );
}

#[test]
fn gift_card_mutation_user_error_codes_cover_create_update_credit_and_debit_paths() {
    let mut proxy = snapshot_proxy();

    let response = proxy.process_request(json_graphql_request(
        r#"mutation GiftCardMutationUserErrorCodes {
          setupSmallBalance: giftCardCreate(input: { initialValue: "5", code: "har686smallcard" }) { giftCard { id } userErrors { field code message } }
          zeroInitialValue: giftCardCreate(input: { initialValue: "0" }) { giftCard { id } userErrors { field code message } }
          missingUpdate: giftCardUpdate(id: "gid://shopify/GiftCard/9999999", input: { note: "x" }) { giftCard { id } userErrors { field code message } }
          negativeCredit: giftCardCredit(id: "gid://shopify/GiftCard/1?shopify-draft-proxy=synthetic", creditInput: { creditAmount: { amount: "-1", currencyCode: "CAD" } }) { giftCardCreditTransaction { id } userErrors { field code message } }
          insufficientDebit: giftCardDebit(id: "gid://shopify/GiftCard/1?shopify-draft-proxy=synthetic", debitInput: { debitAmount: { amount: "9999", currencyCode: "CAD" } }) { giftCardDebitTransaction { id } userErrors { field code message } }
        }"#,
        json!({}),
    ));

    assert_eq!(
        response.body["data"],
        json!({
            "setupSmallBalance": { "giftCard": { "id": "gid://shopify/GiftCard/1?shopify-draft-proxy=synthetic" }, "userErrors": [] },
            "zeroInitialValue": {
                "giftCard": null,
                "userErrors": [{ "field": ["input", "initialValue"], "code": "GREATER_THAN", "message": "must be greater than 0" }]
            },
            "missingUpdate": {
                "giftCard": null,
                "userErrors": [{ "field": ["id"], "code": "GIFT_CARD_NOT_FOUND", "message": "The gift card could not be found." }]
            },
            "negativeCredit": {
                "giftCardCreditTransaction": null,
                "userErrors": [{ "field": ["creditInput", "creditAmount", "amount"], "code": "NEGATIVE_OR_ZERO_AMOUNT", "message": "A positive amount must be used." }]
            },
            "insufficientDebit": {
                "giftCardDebitTransaction": null,
                "userErrors": [{ "field": ["debitInput", "debitAmount", "amount"], "code": "INSUFFICIENT_FUNDS", "message": "The gift card does not have sufficient funds to satisfy the request." }]
            }
        })
    );
}

#[test]
fn gift_card_create_validation_is_input_driven_under_ordinary_operation_name() {
    let mut proxy = snapshot_proxy();

    let response = proxy.process_request(json_graphql_request(
        r#"mutation IssueGiftCards($validCode: String!, $tooLongCode: String!, $missingCustomerId: ID!) {
          zeroInitialValue: giftCardCreate(input: { initialValue: "0" }) {
            giftCard { id }
            giftCardCode
            userErrors { field code message }
          }
          shortCode: giftCardCreate(input: { initialValue: "10", code: "abc" }) {
            giftCard { id }
            giftCardCode
            userErrors { field code message }
          }
          longCode: giftCardCreate(input: { initialValue: "10", code: $tooLongCode }) {
            giftCard { id }
            giftCardCode
            userErrors { field code message }
          }
          invalidCode: giftCardCreate(input: { initialValue: "10", code: "bad!code" }) {
            giftCard { id }
            giftCardCode
            userErrors { field code message }
          }
          shortCodeMissingCustomer: giftCardCreate(input: { initialValue: "10", code: "abc", customerId: $missingCustomerId }) {
            giftCard { id }
            giftCardCode
            userErrors { field code message }
          }
          missingCustomer: giftCardCreate(input: { initialValue: "10", customerId: $missingCustomerId }) {
            giftCard { id }
            giftCardCode
            userErrors { field code message }
          }
          success: giftCardCreate(input: { initialValue: "10", code: $validCode }) {
            giftCard { id lastCharacters maskedCode initialValue { amount currencyCode } balance { amount currencyCode } }
            giftCardCode
            userErrors { field code message }
          }
          duplicate: giftCardCreate(input: { initialValue: "10", code: $validCode }) {
            giftCard { id }
            giftCardCode
            userErrors { field code message }
          }
          autoGenerated: giftCardCreate(input: { initialValue: "10" }) {
            giftCard { id lastCharacters maskedCode initialValue { amount currencyCode } balance { amount currencyCode } }
            giftCardCode
            userErrors { field code message }
          }
        }"#,
        json!({
            "validCode": "ParityOkMowpZlrz",
            "tooLongCode": "x".repeat(21),
            "missingCustomerId": "gid://shopify/Customer/999999999"
        }),
    ));

    assert_eq!(response.status, 200);
    assert_eq!(
        response.body["data"],
        json!({
            "zeroInitialValue": {
                "giftCard": null,
                "giftCardCode": null,
                "userErrors": [{ "field": ["input", "initialValue"], "code": "GREATER_THAN", "message": "must be greater than 0" }]
            },
            "shortCode": {
                "giftCard": null,
                "giftCardCode": null,
                "userErrors": [{ "field": ["input", "code"], "code": "TOO_SHORT", "message": "Code must be at least 8 characters long" }]
            },
            "longCode": {
                "giftCard": null,
                "giftCardCode": null,
                "userErrors": [{ "field": ["input", "code"], "code": "TOO_LONG", "message": "Code must be at most 20 characters long" }]
            },
            "invalidCode": {
                "giftCard": null,
                "giftCardCode": null,
                "userErrors": [{ "field": ["input", "code"], "code": "INVALID", "message": "Code can only contain letters(a-z) and numbers(0-9)" }]
            },
            "shortCodeMissingCustomer": {
                "giftCard": null,
                "giftCardCode": null,
                "userErrors": [{ "field": ["input", "customerId"], "code": "CUSTOMER_NOT_FOUND", "message": "The customer could not be found." }]
            },
            "missingCustomer": {
                "giftCard": null,
                "giftCardCode": null,
                "userErrors": [{ "field": ["input", "customerId"], "code": "CUSTOMER_NOT_FOUND", "message": "The customer could not be found." }]
            },
            "success": {
                "giftCard": {
                    "id": "gid://shopify/GiftCard/1?shopify-draft-proxy=synthetic",
                    "lastCharacters": "zlrz",
                    "maskedCode": "•••• •••• •••• zlrz",
                    "initialValue": { "amount": "10.0", "currencyCode": "CAD" },
                    "balance": { "amount": "10.0", "currencyCode": "CAD" }
                },
                "giftCardCode": "parityokmowpzlrz",
                "userErrors": []
            },
            "duplicate": {
                "giftCard": null,
                "giftCardCode": null,
                "userErrors": [{ "field": ["input", "code"], "code": null, "message": "Code has already been taken" }]
            },
            "autoGenerated": {
                "giftCard": {
                    "id": "gid://shopify/GiftCard/2?shopify-draft-proxy=synthetic",
                    "lastCharacters": "0002",
                    "maskedCode": "•••• •••• •••• 0002",
                    "initialValue": { "amount": "10.0", "currencyCode": "CAD" },
                    "balance": { "amount": "10.0", "currencyCode": "CAD" }
                },
                "giftCardCode": "giftcard00000002",
                "userErrors": []
            }
        })
    );
}

#[test]
fn gift_card_create_omitted_optional_fields_are_null_and_supplied_values_round_trip() {
    let mut proxy = snapshot_proxy();

    let plain_create = proxy.process_request(json_graphql_request(
        r#"mutation GiftCardCreatePlain {
          plain: giftCardCreate(input: { initialValue: "25" }) {
            giftCard {
              id
              note
              expiresOn
              customer { id }
              templateSuffix
              recipientAttributes {
                message
                preferredName
                sendNotificationAt
                recipient { id }
              }
            }
            giftCardCode
            userErrors { field code message }
          }
        }"#,
        json!({}),
    ));
    assert_eq!(plain_create.status, 200);
    assert_eq!(
        plain_create.body["data"]["plain"],
        json!({
            "giftCard": {
                "id": "gid://shopify/GiftCard/1?shopify-draft-proxy=synthetic",
                "note": null,
                "expiresOn": null,
                "customer": null,
                "templateSuffix": null,
                "recipientAttributes": null
            },
            "giftCardCode": "giftcard00000001",
            "userErrors": []
        })
    );

    let plain_read = proxy.process_request(json_graphql_request(
        r#"query GiftCardCreatePlainRead($id: ID!) {
          giftCard(id: $id) {
            id
            note
            expiresOn
            customer { id }
            templateSuffix
            recipientAttributes {
              message
              preferredName
              sendNotificationAt
              recipient { id }
            }
          }
        }"#,
        json!({ "id": "gid://shopify/GiftCard/1?shopify-draft-proxy=synthetic" }),
    ));
    assert_eq!(plain_read.status, 200);
    assert_eq!(
        plain_read.body["data"]["giftCard"],
        json!({
            "id": "gid://shopify/GiftCard/1?shopify-draft-proxy=synthetic",
            "note": null,
            "expiresOn": null,
            "customer": null,
            "templateSuffix": null,
            "recipientAttributes": null
        })
    );

    let supplied_create = proxy.process_request(json_graphql_request(
        r#"mutation GiftCardCreateSupplied($recipientId: ID!, $sendAt: DateTime!) {
          supplied: giftCardCreate(input: {
            initialValue: "30"
            note: "Requested gift card note"
            expiresOn: "2028-01-31"
            recipientAttributes: {
              id: $recipientId
              preferredName: "Requested Recipient"
              message: "Requested recipient message"
              sendNotificationAt: $sendAt
            }
          }) {
            giftCard {
              id
              note
              expiresOn
              customer { id }
              templateSuffix
              recipientAttributes {
                message
                preferredName
                sendNotificationAt
                recipient { id }
              }
            }
            giftCardCode
            userErrors { field code message }
          }
        }"#,
        json!({
            "recipientId": "gid://shopify/Customer/10587888714034",
            "sendAt": "2026-07-01T00:00:00Z"
        }),
    ));
    assert_eq!(supplied_create.status, 200);
    let supplied_card = json!({
        "id": "gid://shopify/GiftCard/2?shopify-draft-proxy=synthetic",
        "note": "Requested gift card note",
        "expiresOn": "2028-01-31",
        "customer": null,
        "templateSuffix": null,
        "recipientAttributes": {
            "message": "Requested recipient message",
            "preferredName": "Requested Recipient",
            "sendNotificationAt": "2026-07-01T00:00:00Z",
            "recipient": { "id": "gid://shopify/Customer/10587888714034" }
        }
    });
    assert_eq!(
        supplied_create.body["data"]["supplied"],
        json!({
            "giftCard": supplied_card,
            "giftCardCode": "giftcard00000002",
            "userErrors": []
        })
    );

    let supplied_read = proxy.process_request(json_graphql_request(
        r#"query GiftCardCreateSuppliedRead($id: ID!) {
          giftCard(id: $id) {
            id
            note
            expiresOn
            customer { id }
            templateSuffix
            recipientAttributes {
              message
              preferredName
              sendNotificationAt
              recipient { id }
            }
          }
        }"#,
        json!({ "id": "gid://shopify/GiftCard/2?shopify-draft-proxy=synthetic" }),
    ));
    assert_eq!(supplied_read.status, 200);
    assert_eq!(supplied_read.body["data"]["giftCard"], supplied_card);
}

#[test]
fn gift_card_create_released_schema_rejects_missing_initial_value_and_initial_amount() {
    let mut proxy = snapshot_proxy();

    let inline_missing = proxy.process_request(json_graphql_request(
        r#"mutation ReleasedMissingInline {
          missing: giftCardCreate(input: { note: "x" }) {
            giftCard { id }
            userErrors { field code message }
          }
        }"#,
        json!({}),
    ));
    assert_eq!(
        inline_missing.body,
        json!({
            "errors": [{
                "message": "Argument 'initialValue' on InputObject 'GiftCardCreateInput' is required. Expected type Decimal!",
                "locations": [{ "line": 2, "column": 42 }],
                "path": ["mutation ReleasedMissingInline", "missing", "input", "initialValue"],
                "extensions": {
                    "code": "missingRequiredInputObjectAttribute",
                    "argumentName": "initialValue",
                    "argumentType": "Decimal!",
                    "inputObjectType": "GiftCardCreateInput"
                }
            }]
        })
    );

    let variable_missing = proxy.process_request(json_graphql_request(
        r#"mutation ReleasedMissingVariable($input: GiftCardCreateInput!) {
          missing: giftCardCreate(input: $input) {
            giftCard { id }
            userErrors { field code message }
          }
        }"#,
        json!({ "input": { "note": "x" } }),
    ));
    assert_eq!(
        variable_missing.body,
        json!({
            "errors": [{
                "message": "Variable $input of type GiftCardCreateInput! was provided invalid value for initialValue (Expected value to not be null)",
                "locations": [{ "line": 1, "column": 34 }],
                "extensions": {
                    "code": "INVALID_VARIABLE",
                    "value": { "note": "x" },
                    "problems": [{ "path": ["initialValue"], "explanation": "Expected value to not be null" }]
                }
            }]
        })
    );

    let variable_initial_amount = proxy.process_request(json_graphql_request(
        r#"mutation ReleasedInitialAmount($input: GiftCardCreateInput!) {
          money: giftCardCreate(input: $input) {
            giftCard { id }
            userErrors { field code message }
          }
        }"#,
        json!({
            "input": {
                "initialValue": "10",
                "initialAmount": { "amount": "10", "currencyCode": "USD" }
            }
        }),
    ));
    assert_eq!(
        variable_initial_amount.body,
        json!({
            "errors": [{
                "message": "Variable $input of type GiftCardCreateInput! was provided invalid value for initialAmount (Field is not defined on GiftCardCreateInput)",
                "locations": [{ "line": 1, "column": 32 }],
                "extensions": {
                    "code": "INVALID_VARIABLE",
                    "value": {
                        "initialAmount": { "amount": "10", "currencyCode": "USD" },
                        "initialValue": "10"
                    },
                    "problems": [{ "path": ["initialAmount"], "explanation": "Field is not defined on GiftCardCreateInput" }]
                }
            }]
        })
    );

    let variable_multiple_errors = proxy.process_request(json_graphql_request(
        r#"mutation ReleasedMultipleVariableErrors($input: GiftCardCreateInput!) {
          money: giftCardCreate(input: $input) {
            giftCard { id }
            userErrors { field code message }
          }
        }"#,
        json!({
            "input": {
                "note": "x",
                "initialAmount": { "amount": "10", "currencyCode": "USD" }
            }
        }),
    ));
    assert_eq!(
        variable_multiple_errors.body,
        json!({
            "errors": [{
                "message": "Variable $input of type GiftCardCreateInput! was provided invalid value for initialAmount (Field is not defined on GiftCardCreateInput), initialValue (Expected value to not be null)",
                "locations": [{ "line": 1, "column": 41 }],
                "extensions": {
                    "code": "INVALID_VARIABLE",
                    "value": {
                        "initialAmount": { "amount": "10", "currencyCode": "USD" },
                        "note": "x"
                    },
                    "problems": [
                        { "path": ["initialAmount"], "explanation": "Field is not defined on GiftCardCreateInput" },
                        { "path": ["initialValue"], "explanation": "Expected value to not be null" }
                    ]
                }
            }]
        })
    );

    let inline_initial_amount = proxy.process_request(json_graphql_request(
        r#"mutation ReleasedInitialAmountInline {
          money: giftCardCreate(input: { initialValue: "10", initialAmount: { amount: "10", currencyCode: USD } }) {
            giftCard { id }
            userErrors { field code message }
          }
        }"#,
        json!({}),
    ));
    assert_eq!(
        inline_initial_amount.body,
        json!({
            "errors": [{
                "message": "InputObject 'GiftCardCreateInput' doesn't accept argument 'initialAmount'",
                "locations": [{ "line": 2, "column": 62 }],
                "path": ["mutation ReleasedInitialAmountInline", "money", "input", "initialAmount"],
                "extensions": {
                    "code": "argumentNotAccepted",
                    "name": "GiftCardCreateInput",
                    "typeName": "InputObject",
                    "argumentName": "initialAmount"
                }
            }]
        })
    );

    assert_eq!(proxy.get_log_snapshot()["entries"], json!([]));
}

#[test]
fn gift_card_roots_accept_ordinary_operation_names_without_501s() {
    let mut proxy = snapshot_proxy();

    let create = proxy.process_request(json_graphql_request(
        r#"mutation IssueLocalGiftCard {
          issue: giftCardCreate(input: { initialValue: "12.50", notify: false }) {
            giftCard { id balance { amount currencyCode } }
            giftCardCode
            userErrors { field code message }
          }
        }"#,
        json!({}),
    ));
    assert_eq!(create.status, 200);
    assert_eq!(
        create.body["data"]["issue"],
        json!({
            "giftCard": {
                "id": "gid://shopify/GiftCard/1?shopify-draft-proxy=synthetic",
                "balance": { "amount": "12.5", "currencyCode": "CAD" }
            },
            "giftCardCode": "giftcard00000001",
            "userErrors": []
        })
    );

    let read = proxy.process_request(json_graphql_request(
        r#"query ReadLocalGiftCard($id: ID!, $query: String!) {
          card: giftCard(id: $id) { id balance { amount currencyCode } }
          cards: giftCards(first: 5, query: $query, sortKey: ID) { nodes { id balance { amount currencyCode } } }
          count: giftCardsCount(query: $query) { count precision }
          config: giftCardConfiguration { issueLimit { amount currencyCode } }
        }"#,
        json!({
            "id": "gid://shopify/GiftCard/1?shopify-draft-proxy=synthetic",
            "query": "id:1"
        }),
    ));
    assert_eq!(read.status, 200);
    assert_eq!(
        read.body["data"],
        json!({
            "card": {
                "id": "gid://shopify/GiftCard/1?shopify-draft-proxy=synthetic",
                "balance": { "amount": "12.5", "currencyCode": "CAD" }
            },
            "cards": {
                "nodes": [{
                    "id": "gid://shopify/GiftCard/1?shopify-draft-proxy=synthetic",
                    "balance": { "amount": "12.5", "currencyCode": "CAD" }
                }]
            },
            "count": { "count": 1, "precision": "EXACT" },
            "config": { "issueLimit": { "amount": "3000.0", "currencyCode": "CAD" } }
        })
    );

    let validations = proxy.process_request(json_graphql_request(
        r#"mutation ValidateLocalGiftCards {
          emptyUpdate: giftCardUpdate(id: "gid://shopify/GiftCard/1?shopify-draft-proxy=synthetic", input: {}) {
            giftCard { id }
            userErrors { field code message }
          }
          missingUpdate: giftCardUpdate(id: "gid://shopify/GiftCard/999999999", input: { note: "x" }) {
            giftCard { id }
            userErrors { field code message }
          }
          negativeCredit: giftCardCredit(id: "gid://shopify/GiftCard/1?shopify-draft-proxy=synthetic", creditInput: { creditAmount: { amount: "-1", currencyCode: CAD } }) {
            giftCardCreditTransaction { id }
            userErrors { field code message }
          }
          insufficientDebit: giftCardDebit(id: "gid://shopify/GiftCard/1?shopify-draft-proxy=synthetic", debitInput: { debitAmount: { amount: "999", currencyCode: CAD } }) {
            giftCardDebitTransaction { id }
            userErrors { field code message }
          }
          missingDeactivate: giftCardDeactivate(id: "gid://shopify/GiftCard/999999999") {
            giftCard { id }
            userErrors { field code message }
          }
          notifyDisabled: giftCardSendNotificationToCustomer(id: "gid://shopify/GiftCard/1?shopify-draft-proxy=synthetic") {
            giftCard { id }
            userErrors { field code message }
          }
          missingRecipientNotify: giftCardSendNotificationToRecipient(id: "gid://shopify/GiftCard/999999999") {
            giftCard { id }
            userErrors { field code message }
          }
        }"#,
        json!({}),
    ));
    assert_eq!(validations.status, 200);
    assert_eq!(
        validations.body["data"],
        json!({
            "emptyUpdate": {
                "giftCard": null,
                "userErrors": [{ "field": ["input"], "code": "INVALID", "message": "At least one argument is required in the input." }]
            },
            "missingUpdate": {
                "giftCard": null,
                "userErrors": [{ "field": ["id"], "code": "GIFT_CARD_NOT_FOUND", "message": "The gift card could not be found." }]
            },
            "negativeCredit": {
                "giftCardCreditTransaction": null,
                "userErrors": [{ "field": ["creditInput", "creditAmount", "amount"], "code": "NEGATIVE_OR_ZERO_AMOUNT", "message": "A positive amount must be used." }]
            },
            "insufficientDebit": {
                "giftCardDebitTransaction": null,
                "userErrors": [{ "field": ["debitInput", "debitAmount", "amount"], "code": "INSUFFICIENT_FUNDS", "message": "The gift card does not have sufficient funds to satisfy the request." }]
            },
            "missingDeactivate": {
                "giftCard": null,
                "userErrors": [{ "field": ["id"], "code": "GIFT_CARD_NOT_FOUND", "message": "The gift card could not be found." }]
            },
            "notifyDisabled": {
                "giftCard": null,
                "userErrors": [{ "field": ["id"], "code": "INVALID", "message": "Notifications for this gift card are disabled." }]
            },
            "missingRecipientNotify": {
                "giftCard": null,
                "userErrors": [{ "field": ["id"], "code": "GIFT_CARD_NOT_FOUND", "message": "The gift card could not be found." }]
            }
        })
    );

    let transactions = proxy.process_request(json_graphql_request(
        r#"mutation AdjustLocalGiftCard {
          credit: giftCardCredit(id: "gid://shopify/GiftCard/1?shopify-draft-proxy=synthetic", creditInput: { creditAmount: { amount: "2.50", currencyCode: CAD }, note: "manual credit" }) {
            giftCardCreditTransaction { __typename amount { amount currencyCode } giftCard { balance { amount currencyCode } } }
            userErrors { field code message }
          }
          debit: giftCardDebit(id: "gid://shopify/GiftCard/1?shopify-draft-proxy=synthetic", debitInput: { debitAmount: { amount: "3.00", currencyCode: CAD }, note: "manual debit" }) {
            giftCardDebitTransaction { __typename amount { amount currencyCode } giftCard { balance { amount currencyCode } } }
            userErrors { field code message }
          }
          deactivate: giftCardDeactivate(id: "gid://shopify/GiftCard/1?shopify-draft-proxy=synthetic") {
            giftCard { id enabled balance { amount currencyCode } }
            userErrors { field code message }
          }
        }"#,
        json!({}),
    ));
    assert_eq!(transactions.status, 200);
    assert_eq!(
        transactions.body["data"],
        json!({
            "credit": {
                "giftCardCreditTransaction": {
                    "__typename": "GiftCardCreditTransaction",
                    "amount": { "amount": "2.5", "currencyCode": "CAD" },
                    "giftCard": { "balance": { "amount": "15.0", "currencyCode": "CAD" } }
                },
                "userErrors": []
            },
            "debit": {
                "giftCardDebitTransaction": {
                    "__typename": "GiftCardDebitTransaction",
                    "amount": { "amount": "-3.0", "currencyCode": "CAD" },
                    "giftCard": { "balance": { "amount": "12.0", "currencyCode": "CAD" } }
                },
                "userErrors": []
            },
            "deactivate": {
                "giftCard": {
                    "id": "gid://shopify/GiftCard/1?shopify-draft-proxy=synthetic",
                    "enabled": false,
                    "balance": { "amount": "12.0", "currencyCode": "CAD" }
                },
                "userErrors": []
            }
        })
    );
}

#[test]
fn gift_card_credit_debit_preserve_optional_transaction_notes() {
    let mut proxy = snapshot_proxy();

    let create = proxy.process_request(json_graphql_request(
        r#"mutation IssueLocalGiftCard {
          giftCardCreate(input: { initialValue: "20.00", notify: false }) {
            giftCard { id }
            userErrors { field code message }
          }
        }"#,
        json!({}),
    ));
    assert_eq!(create.status, 200);
    let gift_card_id = create.body["data"]["giftCardCreate"]["giftCard"]["id"].clone();

    let transactions = proxy.process_request(json_graphql_request(
        r#"mutation AdjustGiftCardNotes($id: ID!) {
          creditWithoutNote: giftCardCredit(id: $id, creditInput: { creditAmount: { amount: "2.00", currencyCode: CAD } }) {
            giftCardCreditTransaction { note amount { amount currencyCode } }
            userErrors { field code message }
          }
          debitWithoutNote: giftCardDebit(id: $id, debitInput: { debitAmount: { amount: "1.00", currencyCode: CAD } }) {
            giftCardDebitTransaction { note amount { amount currencyCode } }
            userErrors { field code message }
          }
          creditWithNote: giftCardCredit(id: $id, creditInput: { creditAmount: { amount: "3.00", currencyCode: CAD }, note: "manual credit" }) {
            giftCardCreditTransaction { note amount { amount currencyCode } }
            userErrors { field code message }
          }
          debitWithNote: giftCardDebit(id: $id, debitInput: { debitAmount: { amount: "4.00", currencyCode: CAD }, note: "manual debit" }) {
            giftCardDebitTransaction { note amount { amount currencyCode } }
            userErrors { field code message }
          }
        }"#,
        json!({ "id": gift_card_id }),
    ));
    assert_eq!(transactions.status, 200);
    assert_eq!(
        transactions.body["data"],
        json!({
            "creditWithoutNote": {
                "giftCardCreditTransaction": { "note": null, "amount": { "amount": "2.0", "currencyCode": "CAD" } },
                "userErrors": []
            },
            "debitWithoutNote": {
                "giftCardDebitTransaction": { "note": null, "amount": { "amount": "-1.0", "currencyCode": "CAD" } },
                "userErrors": []
            },
            "creditWithNote": {
                "giftCardCreditTransaction": { "note": "manual credit", "amount": { "amount": "3.0", "currencyCode": "CAD" } },
                "userErrors": []
            },
            "debitWithNote": {
                "giftCardDebitTransaction": { "note": "manual debit", "amount": { "amount": "-4.0", "currencyCode": "CAD" } },
                "userErrors": []
            }
        })
    );

    let readback = proxy.process_request(json_graphql_request(
        r#"query GiftCardTransactionNoteReadback($id: ID!) {
          giftCard(id: $id) {
            transactions(first: 5) {
              nodes {
                note
                amount { amount currencyCode }
              }
            }
          }
        }"#,
        json!({ "id": gift_card_id }),
    ));
    assert_eq!(readback.status, 200);
    assert_eq!(
        readback.body["data"]["giftCard"]["transactions"]["nodes"],
        json!([
            { "note": null, "amount": { "amount": "2.0", "currencyCode": "CAD" } },
            { "note": null, "amount": { "amount": "-1.0", "currencyCode": "CAD" } },
            { "note": "manual credit", "amount": { "amount": "3.0", "currencyCode": "CAD" } },
            { "note": "manual debit", "amount": { "amount": "-4.0", "currencyCode": "CAD" } }
        ])
    );
}

#[test]
fn gift_card_lifecycle_stages_update_transactions_deactivate_and_downstream_reads() {
    let mut proxy = snapshot_proxy();

    let empty = proxy.process_request(json_graphql_request(
        r#"query GiftCardReadEvidence($unknownId: ID!, $query: String!) {
          missingGiftCard: giftCard(id: $unknownId) { id }
          filteredEmptyGiftCards: giftCards(first: 2, query: $query, sortKey: ID) {
            nodes { id lastCharacters }
            pageInfo { hasNextPage hasPreviousPage }
          }
          filteredEmptyGiftCardsCount: giftCardsCount(query: $query) { count precision }
          giftCardConfiguration { issueLimit { amount currencyCode } purchaseLimit { amount currencyCode } }
        }"#,
        json!({
            "unknownId": "gid://shopify/GiftCard/999999999999",
            "query": "id:999999999999"
        }),
    ));
    assert_eq!(empty.body["data"]["missingGiftCard"], Value::Null);
    assert_eq!(
        empty.body["data"]["filteredEmptyGiftCards"],
        json!({ "nodes": [], "pageInfo": { "hasNextPage": false, "hasPreviousPage": false } })
    );
    assert_eq!(
        empty.body["data"]["filteredEmptyGiftCardsCount"],
        json!({ "count": 0, "precision": "EXACT" })
    );
    assert_eq!(
        empty.body["data"]["giftCardConfiguration"],
        json!({
            "issueLimit": { "amount": "3000.0", "currencyCode": "CAD" },
            "purchaseLimit": { "amount": "14000.0", "currencyCode": "CAD" }
        })
    );

    let lifecycle = proxy.process_request(json_graphql_request(
        r#"mutation GiftCardLifecycle($id: ID!, $updateInput: GiftCardUpdateInput!, $creditInput: GiftCardCreditInput!, $debitInput: GiftCardDebitInput!) {
          update: giftCardUpdate(id: $id, input: $updateInput) { giftCard { note templateSuffix expiresOn balance { amount currencyCode } } userErrors { field message } }
          credit: giftCardCredit(id: $id, creditInput: $creditInput) { giftCardCreditTransaction { note amount { amount currencyCode } giftCard { balance { amount currencyCode } } } userErrors { field message } }
          debit: giftCardDebit(id: $id, debitInput: $debitInput) { giftCardDebitTransaction { note amount { amount currencyCode } giftCard { balance { amount currencyCode } } } userErrors { field message } }
          deactivate: giftCardDeactivate(id: $id) { giftCard { enabled balance { amount currencyCode } } userErrors { field message } }
        }"#,
        json!({
            "id": "gid://shopify/GiftCard/654773256498",
            "updateInput": { "note": "HAR-310 conformance gift card updated", "templateSuffix": "birthday", "expiresOn": "2028-04-26" },
            "creditInput": { "creditAmount": { "amount": "2.00", "currencyCode": "CAD" }, "note": "HAR-310 credit" },
            "debitInput": { "debitAmount": { "amount": "3.00", "currencyCode": "CAD" }, "note": "HAR-310 debit" }
        }),
    ));
    assert_eq!(
        lifecycle.body["data"],
        json!({
            "update": {
                "giftCard": { "note": "HAR-310 conformance gift card updated", "templateSuffix": "birthday", "expiresOn": "2028-04-26", "balance": { "amount": "5.0", "currencyCode": "CAD" } },
                "userErrors": []
            },
            "credit": {
                "giftCardCreditTransaction": { "note": "HAR-310 credit", "amount": { "amount": "2.0", "currencyCode": "CAD" }, "giftCard": { "balance": { "amount": "7.0", "currencyCode": "CAD" } } },
                "userErrors": []
            },
            "debit": {
                "giftCardDebitTransaction": { "note": "HAR-310 debit", "amount": { "amount": "-3.0", "currencyCode": "CAD" }, "giftCard": { "balance": { "amount": "4.0", "currencyCode": "CAD" } } },
                "userErrors": []
            },
            "deactivate": {
                "giftCard": { "enabled": false, "balance": { "amount": "4.0", "currencyCode": "CAD" } },
                "userErrors": []
            }
        })
    );

    let downstream = proxy.process_request(json_graphql_request(
        r#"query GiftCardReadAfterLifecycle($id: ID!, $query: String!) {
          giftCard(id: $id) { note templateSuffix expiresOn enabled balance { amount currencyCode } transactions(first: 5) { nodes { note amount { amount currencyCode } } pageInfo { hasNextPage hasPreviousPage } } }
          giftCards(first: 2, query: $query, sortKey: ID) { nodes { id lastCharacters enabled } pageInfo { hasNextPage hasPreviousPage } }
          giftCardsCount(query: $query) { count precision }
        }"#,
        json!({
            "id": "gid://shopify/GiftCard/654773256498",
            "query": "id:654773256498"
        }),
    ));
    let expected_card = json!({
        "note": "HAR-310 conformance gift card updated",
        "templateSuffix": "birthday",
        "expiresOn": "2028-04-26",
        "enabled": false,
        "balance": { "amount": "4.0", "currencyCode": "CAD" },
        "transactions": {
            "nodes": [
                { "note": "HAR-310 credit", "amount": { "amount": "2.0", "currencyCode": "CAD" } },
                { "note": "HAR-310 debit", "amount": { "amount": "-3.0", "currencyCode": "CAD" } }
            ],
            "pageInfo": { "hasNextPage": false, "hasPreviousPage": false }
        }
    });
    assert_eq!(downstream.body["data"]["giftCard"], expected_card);
    assert_eq!(
        downstream.body["data"]["giftCards"],
        json!({ "nodes": [{ "id": "gid://shopify/GiftCard/654773256498", "lastCharacters": "2053", "enabled": false }], "pageInfo": { "hasNextPage": false, "hasPreviousPage": false } })
    );
    assert_eq!(
        downstream.body["data"]["giftCardsCount"],
        json!({ "count": 1, "precision": "EXACT" })
    );

    let node = proxy.process_request(json_graphql_request(
        r#"query GiftCardNodeReadAfterLifecycle($id: ID!) {
          node(id: $id) { ... on GiftCard { note templateSuffix expiresOn enabled balance { amount currencyCode } transactions(first: 5) { nodes { note amount { amount currencyCode } } pageInfo { hasNextPage hasPreviousPage } } } }
        }"#,
        json!({ "id": "gid://shopify/GiftCard/654773256498" }),
    ));
    assert_eq!(node.body["data"]["node"], expected_card);
}

#[test]
fn gift_card_expiry_uses_shop_timezone_boundary_before_expired_validation() {
    let mut proxy = snapshot_proxy();

    let dump = proxy.process_request(request_with_body(
        "POST",
        "/__meta/dump",
        r#"{"createdAt":"2026-04-29T09:31:02Z"}"#,
    ));
    let mut restored = dump.body.clone();
    restored["state"]["baseState"]["shop"]["ianaTimezone"] = json!("Pacific/Honolulu");
    restored["state"]["baseState"]["shop"]["timezoneOffsetMinutes"] = json!(-600);
    let restore = proxy.process_request(request_with_body(
        "POST",
        "/__meta/restore",
        &restored.to_string(),
    ));
    assert_eq!(restore.status, 200);

    let setup = proxy.process_request(json_graphql_request(
        r#"mutation GiftCardExpiryShopTimezoneSetup {
          creditCard: giftCardCreate(input: { initialValue: "20", expiresOn: "2026-04-28" }) { giftCard { id } giftCardCode userErrors { field code message } }
          debitCard: giftCardCreate(input: { initialValue: "20", expiresOn: "2026-04-28" }) { giftCard { id } giftCardCode userErrors { field code message } }
          customerNotificationCard: giftCardCreate(input: { initialValue: "20", expiresOn: "2026-04-28", customerId: "gid://shopify/Customer/10552623464754" }) { giftCard { id } giftCardCode userErrors { field code message } }
          recipientNotificationCard: giftCardCreate(input: { initialValue: "20", expiresOn: "2026-04-28", recipientAttributes: { id: "gid://shopify/Customer/timezone-recipient" } }) { giftCard { id } giftCardCode userErrors { field code message } }
        }"#,
        json!({}),
    ));
    assert_eq!(setup.body["data"]["creditCard"]["userErrors"], json!([]));
    assert_eq!(setup.body["data"]["debitCard"]["userErrors"], json!([]));
    assert_eq!(
        setup.body["data"]["customerNotificationCard"]["userErrors"],
        json!([])
    );
    assert_eq!(
        setup.body["data"]["recipientNotificationCard"]["userErrors"],
        json!([])
    );
    let credit_id = json_string(
        &setup.body["data"]["creditCard"]["giftCard"]["id"],
        "credit card id",
    );
    let debit_id = json_string(
        &setup.body["data"]["debitCard"]["giftCard"]["id"],
        "debit card id",
    );
    let customer_notification_id = json_string(
        &setup.body["data"]["customerNotificationCard"]["giftCard"]["id"],
        "customer notification card id",
    );
    let recipient_notification_id = json_string(
        &setup.body["data"]["recipientNotificationCard"]["giftCard"]["id"],
        "recipient notification card id",
    );

    let response = proxy.process_request(json_graphql_request(
        r#"mutation GiftCardExpiryShopTimezone($creditId: ID!, $debitId: ID!, $customerNotificationId: ID!, $recipientNotificationId: ID!, $creditInput: GiftCardCreditInput!, $debitInput: GiftCardDebitInput!) {
          credit: giftCardCredit(id: $creditId, creditInput: $creditInput) { giftCardCreditTransaction { __typename } userErrors { field code message } }
          debit: giftCardDebit(id: $debitId, debitInput: $debitInput) { giftCardDebitTransaction { __typename } userErrors { field code message } }
          customerNotification: giftCardSendNotificationToCustomer(id: $customerNotificationId) { giftCard { id } userErrors { field code message } }
          recipientNotification: giftCardSendNotificationToRecipient(id: $recipientNotificationId) { giftCard { id } userErrors { field code message } }
        }"#,
        json!({
            "creditId": credit_id,
            "debitId": debit_id,
            "customerNotificationId": customer_notification_id,
            "recipientNotificationId": recipient_notification_id,
            "creditInput": { "creditAmount": { "amount": "5.00", "currencyCode": "CAD" } },
            "debitInput": { "debitAmount": { "amount": "2.00", "currencyCode": "CAD" } }
        }),
    ));

    assert_eq!(
        response.body["data"],
        json!({
            "credit": { "giftCardCreditTransaction": { "__typename": "GiftCardCreditTransaction" }, "userErrors": [] },
            "debit": { "giftCardDebitTransaction": { "__typename": "GiftCardDebitTransaction" }, "userErrors": [] },
            "customerNotification": { "giftCard": { "id": customer_notification_id }, "userErrors": [] },
            "recipientNotification": { "giftCard": { "id": recipient_notification_id }, "userErrors": [] }
        })
    );
}

#[test]
fn gift_card_expiry_uses_utc_fallback_when_shop_timezone_is_missing() {
    let mut proxy = snapshot_proxy();

    let setup = proxy.process_request(json_graphql_request(
        r#"mutation GiftCardExpiryUtcFallbackSetup {
          expired: giftCardCreate(input: { initialValue: "10", expiresOn: "2026-04-28" }) { giftCard { id } giftCardCode userErrors { field code message } }
          active: giftCardCreate(input: { initialValue: "10", expiresOn: "2026-04-30" }) { giftCard { id } giftCardCode userErrors { field code message } }
        }"#,
        json!({}),
    ));
    assert_eq!(setup.body["data"]["expired"]["userErrors"], json!([]));
    assert_eq!(setup.body["data"]["active"]["userErrors"], json!([]));
    let expired_id = json_string(
        &setup.body["data"]["expired"]["giftCard"]["id"],
        "expired card id",
    );
    let active_id = json_string(
        &setup.body["data"]["active"]["giftCard"]["id"],
        "active card id",
    );

    let response = proxy.process_request(json_graphql_request(
        r#"mutation GiftCardExpiryUtcFallback($expiredId: ID!, $activeId: ID!) {
          expiredCredit: giftCardCredit(id: $expiredId, creditInput: { creditAmount: { amount: "1.00", currencyCode: CAD } }) {
            giftCardCreditTransaction { id giftCard { balance { amount currencyCode } } }
            userErrors { field code message }
          }
          activeCredit: giftCardCredit(id: $activeId, creditInput: { creditAmount: { amount: "1.00", currencyCode: CAD } }) {
            giftCardCreditTransaction { id giftCard { balance { amount currencyCode } } }
            userErrors { field code message }
          }
        }"#,
        json!({ "expiredId": expired_id, "activeId": active_id }),
    ));

    assert_eq!(
        response.body["data"],
        json!({
            "expiredCredit": { "giftCardCreditTransaction": null, "userErrors": [{ "field": ["id"], "code": "INVALID", "message": "The gift card has expired." }] },
            "activeCredit": { "giftCardCreditTransaction": { "id": "gid://shopify/GiftCardCreditTransaction/3", "giftCard": { "balance": { "amount": "11.0", "currencyCode": "CAD" } } }, "userErrors": [] }
        })
    );

    let read = proxy.process_request(json_graphql_request(
        r#"query GiftCardExpiryUtcFallbackRead($expiredId: ID!, $activeId: ID!) {
          expired: giftCard(id: $expiredId) { balance { amount currencyCode } transactions(first: 5) { nodes { id } } }
          active: giftCard(id: $activeId) { balance { amount currencyCode } transactions(first: 5) { nodes { id } } }
        }"#,
        json!({ "expiredId": expired_id, "activeId": active_id }),
    ));
    assert_eq!(
        read.body["data"],
        json!({
            "expired": { "balance": { "amount": "10.0", "currencyCode": "CAD" }, "transactions": { "nodes": [] } },
            "active": { "balance": { "amount": "11.0", "currencyCode": "CAD" }, "transactions": { "nodes": [{ "id": "gid://shopify/GiftCardCreditTransaction/3" }] } }
        })
    );
}

#[test]
fn gift_card_credit_limit_rejects_credit_but_allows_followup_debit_transaction() {
    let mut proxy = snapshot_proxy();

    let response = proxy.process_request(json_graphql_request(
        r#"mutation GiftCardCreditLimitExceeded($boundaryId: ID!, $creditInput: GiftCardCreditInput!, $debitInput: GiftCardDebitInput!) {
          overLimitCredit: giftCardCredit(id: $boundaryId, creditInput: $creditInput) {
            giftCardCreditTransaction { __typename amount { amount currencyCode } }
            userErrors { field code message }
          }
          debitAfterRejectedCredit: giftCardDebit(id: $boundaryId, debitInput: $debitInput) {
            giftCardDebitTransaction { __typename amount { amount currencyCode } }
            userErrors { field code message }
          }
        }"#,
        json!({
            "boundaryId": "gid://shopify/GiftCard/654867595570",
            "creditInput": { "creditAmount": { "amount": "0.01", "currencyCode": "CAD" } },
            "debitInput": { "debitAmount": { "amount": "0.01", "currencyCode": "CAD" } }
        }),
    ));

    assert_eq!(
        response.body["data"],
        json!({
            "overLimitCredit": {
                "giftCardCreditTransaction": null,
                "userErrors": [{
                    "field": ["creditInput", "creditAmount", "amount"],
                    "code": "GIFT_CARD_LIMIT_EXCEEDED",
                    "message": "The gift card's value exceeds the allowed limits."
                }]
            },
            "debitAfterRejectedCredit": {
                "giftCardDebitTransaction": {
                    "__typename": "GiftCardDebitTransaction",
                    "amount": { "amount": "-0.01", "currencyCode": "CAD" }
                },
                "userErrors": []
            }
        })
    );
}

#[test]
fn gift_card_entitlement_disabled_wins_for_all_supported_mutation_roots() {
    let mut proxy = snapshot_proxy();

    let response = proxy.process_request(json_graphql_request(
        r#"mutation GiftCardEntitlementDisabled {
          createError: giftCardCreate(input: { initialValue: "0", customerId: "gid://shopify/Customer/disabled-entitlement-customer" }) { giftCard { id } giftCardCode userErrors { field code message } }
          updateError: giftCardUpdate(id: "gid://shopify/GiftCard/disabled-entitlement-card", input: { note: "x" }) { giftCard { id } userErrors { field code message } }
          creditError: giftCardCredit(id: "gid://shopify/GiftCard/disabled-entitlement-card", creditInput: { creditAmount: { amount: "-1", currencyCode: CAD } }) { giftCardCreditTransaction { id } userErrors { field code message } }
          debitError: giftCardDebit(id: "gid://shopify/GiftCard/disabled-entitlement-card", debitInput: { debitAmount: { amount: "9999", currencyCode: CAD } }) { giftCardDebitTransaction { id } userErrors { field code message } }
          deactivateError: giftCardDeactivate(id: "gid://shopify/GiftCard/disabled-entitlement-card") { giftCard { id } userErrors { field code message } }
          notificationCustomerError: giftCardSendNotificationToCustomer(id: "gid://shopify/GiftCard/disabled-entitlement-card") { giftCard { id } userErrors { field code message } }
          notificationRecipientError: giftCardSendNotificationToRecipient(id: "gid://shopify/GiftCard/disabled-entitlement-card") { giftCard { id } userErrors { field code message } }
        }"#,
        json!({}),
    ));

    let base_error = json!([{ "field": ["base"], "code": null, "message": "Gift cards are unavailable on your plan." }]);
    assert_eq!(
        response.body["data"],
        json!({
            "createError": { "giftCard": null, "giftCardCode": null, "userErrors": base_error },
            "updateError": { "giftCard": null, "userErrors": base_error },
            "creditError": { "giftCardCreditTransaction": null, "userErrors": base_error },
            "debitError": { "giftCardDebitTransaction": null, "userErrors": base_error },
            "deactivateError": { "giftCard": null, "userErrors": base_error },
            "notificationCustomerError": { "giftCard": null, "userErrors": base_error },
            "notificationRecipientError": { "giftCard": null, "userErrors": base_error }
        })
    );
    assert_eq!(proxy.get_log_snapshot()["entries"], json!([]));
}

#[test]
fn gift_card_create_notify_false_stages_card_and_notification_disabled_error() {
    let mut proxy = snapshot_proxy();

    let response = proxy.process_request(json_graphql_request(
        r#"mutation GiftCardCreateNotify {
          createNotifyFalse: giftCardCreate(input: { initialValue: "10", notify: false }) {
            giftCard { id }
            userErrors { field code message }
          }
          notifyDisabled: giftCardSendNotificationToCustomer(id: "gid://shopify/GiftCard/1?shopify-draft-proxy=synthetic") {
            giftCard { id }
            userErrors { field code message }
          }
        }"#,
        json!({}),
    ));

    assert_eq!(
        response.body["data"],
        json!({
            "createNotifyFalse": {
                "giftCard": { "id": "gid://shopify/GiftCard/1?shopify-draft-proxy=synthetic" },
                "userErrors": []
            },
            "notifyDisabled": {
                "giftCard": null,
                "userErrors": [{
                    "field": ["id"],
                    "code": "INVALID",
                    "message": "Notifications for this gift card are disabled."
                }]
            }
        })
    );

    let log = proxy.get_log_snapshot();
    assert_eq!(log["entries"].as_array().unwrap().len(), 1);
    assert_eq!(
        log["entries"][0]["stagedResourceIds"],
        json!(["gid://shopify/GiftCard/1?shopify-draft-proxy=synthetic"])
    );
}

#[test]
fn discount_timestamps_create_update_and_code_reads_preserve_staged_values() {
    let mut proxy = snapshot_proxy();
    let create = r#"mutation DiscountTimestampsMonotonicCreate($input: DiscountCodeBasicInput!) { discountCodeBasicCreate(basicCodeDiscount: $input) { codeDiscountNode { id codeDiscount { __typename ... on DiscountCodeBasic { title createdAt updatedAt codes(first: 1) { nodes { code } } } } } userErrors { field message code } } }"#;
    let first_create = proxy.process_request(json_graphql_request(
        create,
        json!({ "input": {
            "title": "HAR-603 first 1777990267935",
            "code": "HAR603A1777990267935",
            "startsAt": "2026-05-05T14:10:07.935Z",
            "context": { "all": "ALL" },
            "customerGets": { "value": { "percentage": 0.1 }, "items": { "all": true } }
        }}),
    ));
    let first_id = first_create.body["data"]["discountCodeBasicCreate"]["codeDiscountNode"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let first_created_at = first_create.body["data"]["discountCodeBasicCreate"]["codeDiscountNode"]
        ["codeDiscount"]["createdAt"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(
        first_create.body["data"]["discountCodeBasicCreate"]["codeDiscountNode"]["codeDiscount"]
            ["title"],
        json!("HAR-603 first 1777990267935")
    );
    assert_eq!(
        first_create.body["data"]["discountCodeBasicCreate"]["codeDiscountNode"]["codeDiscount"]
            ["updatedAt"],
        json!(first_created_at)
    );
    assert_eq!(
        first_create.body["data"]["discountCodeBasicCreate"]["codeDiscountNode"]["codeDiscount"]
            ["codes"],
        json!({ "nodes": [{ "code": "HAR603A1777990267935" }] })
    );
    assert_eq!(
        first_create.body["data"]["discountCodeBasicCreate"]["userErrors"],
        json!([])
    );

    let second_create = proxy.process_request(json_graphql_request(
        create,
        json!({ "input": {
            "title": "HAR-603 second 1777990267935",
            "code": "HAR603B1777990267935",
            "startsAt": "2026-05-05T14:10:07.935Z",
            "context": { "all": "ALL" },
            "customerGets": { "value": { "percentage": 0.1 }, "items": { "all": true } }
        }}),
    ));
    let second_id = second_create.body["data"]["discountCodeBasicCreate"]["codeDiscountNode"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let second_created_at = second_create.body["data"]["discountCodeBasicCreate"]
        ["codeDiscountNode"]["codeDiscount"]["createdAt"]
        .as_str()
        .unwrap()
        .to_string();
    assert_ne!(first_id, second_id);
    assert_synthetic_gid(&first_id, "DiscountCodeNode");
    assert_synthetic_gid(&second_id, "DiscountCodeNode");
    assert_eq!(
        second_create.body["data"]["discountCodeBasicCreate"]["codeDiscountNode"]["codeDiscount"]
            ["updatedAt"],
        json!(second_created_at)
    );

    let update = r#"mutation DiscountTimestampsMonotonicUpdate($id: ID!, $input: DiscountCodeBasicInput!) { discountCodeBasicUpdate(id: $id, basicCodeDiscount: $input) { codeDiscountNode { id codeDiscount { __typename ... on DiscountCodeBasic { title createdAt updatedAt codes(first: 1) { nodes { code } } } } } userErrors { field message code } } }"#;
    let update_response = proxy.process_request(json_graphql_request(
        update,
        json!({ "id": first_id, "input": {
            "title": "HAR-603 first updated 1777990267935",
            "code": "HAR603A1777990267935",
            "startsAt": "2026-05-05T14:10:07.935Z",
            "context": { "all": "ALL" },
            "customerGets": { "value": { "percentage": 0.2 }, "items": { "all": true } }
        }}),
    ));
    let updated_at = update_response.body["data"]["discountCodeBasicUpdate"]["codeDiscountNode"]
        ["codeDiscount"]["updatedAt"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(
        update_response.body["data"]["discountCodeBasicUpdate"]["codeDiscountNode"]["id"],
        json!(first_id)
    );
    assert_eq!(
        update_response.body["data"]["discountCodeBasicUpdate"]["codeDiscountNode"]["codeDiscount"]
            ["createdAt"],
        json!(first_created_at)
    );
    assert!(!updated_at.is_empty());
    assert_eq!(
        update_response.body["data"]["discountCodeBasicUpdate"]["codeDiscountNode"]["codeDiscount"]
            ["title"],
        json!("HAR-603 first updated 1777990267935")
    );
    assert_eq!(
        update_response.body["data"]["discountCodeBasicUpdate"]["userErrors"],
        json!([])
    );

    let read = r#"query DiscountTimestampsMonotonicRead($firstId: ID!, $secondId: ID!, $firstCode: String!, $secondCode: String!) { first: codeDiscountNode(id: $firstId) { id codeDiscount { __typename ... on DiscountCodeBasic { title createdAt updatedAt } } } second: codeDiscountNode(id: $secondId) { id codeDiscount { __typename ... on DiscountCodeBasic { title createdAt updatedAt } } } firstByCode: codeDiscountNodeByCode(code: $firstCode) { id codeDiscount { __typename ... on DiscountCodeBasic { title createdAt updatedAt } } } secondByCode: codeDiscountNodeByCode(code: $secondCode) { id codeDiscount { __typename ... on DiscountCodeBasic { title createdAt updatedAt } } } }"#;
    let read_response = proxy.process_request(json_graphql_request(
        read,
        json!({
            "firstId": first_id,
            "secondId": second_id,
            "firstCode": "HAR603A1777990267935",
            "secondCode": "HAR603B1777990267935"
        }),
    ));
    assert_eq!(
        read_response.body["data"]["first"],
        read_response.body["data"]["firstByCode"]
    );
    assert_eq!(
        read_response.body["data"]["second"],
        read_response.body["data"]["secondByCode"]
    );
    assert_eq!(
        read_response.body["data"]["first"]["codeDiscount"]["updatedAt"],
        json!(updated_at)
    );
    assert_eq!(
        read_response.body["data"]["second"]["codeDiscount"]["updatedAt"],
        json!(second_created_at)
    );
}

#[test]
fn discount_redeem_code_bulk_live_add_delete_stages_case_insensitive_code_lookups() {
    let mut proxy = snapshot_proxy();
    let create = r#"mutation SeedRedeemCodeBulk($input: DiscountCodeBasicInput!) { discountCodeBasicCreate(basicCodeDiscount: $input) { codeDiscountNode { id codeDiscount { ... on DiscountCodeBasic { codes { nodes { id code } } } } } userErrors { field message code extraInfo } } }"#;
    let created = proxy.process_request(json_graphql_request(
        create,
        json!({ "input": {
            "title": "Redeem code bulk generic lifecycle",
            "code": "HAR438BASE1777416023154",
            "startsAt": "2026-04-28T22:39:23Z",
            "combinesWith": { "productDiscounts": false, "orderDiscounts": true, "shippingDiscounts": false },
            "context": { "all": "ALL" },
            "customerGets": { "value": { "percentage": 0.1 }, "items": { "all": true } }
        }}),
    ));
    let discount_id = created.body["data"]["discountCodeBasicCreate"]["codeDiscountNode"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let seed_redeem_code_id = created.body["data"]["discountCodeBasicCreate"]["codeDiscountNode"]
        ["codeDiscount"]["codes"]["nodes"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let add = r#"mutation AnyBulkAdd($discountId: ID!, $codes: [DiscountRedeemCodeInput!]!) { discountRedeemCodeBulkAdd(discountId: $discountId, codes: $codes) { bulkCreation { done codesCount importedCount failedCount } userErrors { field message code extraInfo } } }"#;
    let add_response = proxy.process_request(json_graphql_request(
        add,
        json!({
            "discountId": discount_id,
            "codes": [{ "code": "HAR438ADD1777416023154" }, { "code": "HAR438PLUS1777416023154" }]
        }),
    ));
    assert_eq!(
        add_response.body["data"]["discountRedeemCodeBulkAdd"]["bulkCreation"]["codesCount"],
        json!(2)
    );
    assert_eq!(
        add_response.body["data"]["discountRedeemCodeBulkAdd"]["bulkCreation"]["failedCount"],
        json!(0)
    );
    assert_eq!(
        add_response.body["data"]["discountRedeemCodeBulkAdd"]["userErrors"],
        json!([])
    );

    let read = r#"query AnyBulkRead($id: ID!, $exactAddedCode: String!, $lowerAddedCode: String!, $removedCode: String!) { codeDiscountNode(id: $id) { id codeDiscount { ... on DiscountCodeBasic { codesCount { count precision } } } } exactAdded: codeDiscountNodeByCode(code: $exactAddedCode) { id } lowerAdded: codeDiscountNodeByCode(code: $lowerAddedCode) { id } removed: codeDiscountNodeByCode(code: $removedCode) { id } }"#;
    let read_vars = json!({
        "id": discount_id,
        "exactAddedCode": "HAR438ADD1777416023154",
        "lowerAddedCode": "har438add1777416023154",
        "removedCode": "HAR438BASE1777416023154"
    });
    let after_add = proxy.process_request(json_graphql_request(read, read_vars.clone()));
    assert_eq!(
        after_add.body["data"]["codeDiscountNode"]["codeDiscount"]["codesCount"],
        json!({ "count": 3, "precision": "EXACT" })
    );
    assert_eq!(
        after_add.body["data"]["exactAdded"]["id"],
        after_add.body["data"]["codeDiscountNode"]["id"]
    );
    assert_eq!(
        after_add.body["data"]["lowerAdded"]["id"],
        after_add.body["data"]["codeDiscountNode"]["id"]
    );
    assert_eq!(
        after_add.body["data"]["removed"]["id"],
        after_add.body["data"]["codeDiscountNode"]["id"]
    );

    let delete = r#"mutation AnyBulkDelete($discountId: ID!, $ids: [ID!]!) { discountCodeRedeemCodeBulkDelete(discountId: $discountId, ids: $ids) { job { done } userErrors { field message code extraInfo } } }"#;
    let delete_response = proxy.process_request(json_graphql_request(
        delete,
        json!({
            "discountId": read_vars["id"].clone(),
            "ids": [seed_redeem_code_id]
        }),
    ));
    assert_eq!(
        delete_response.body["data"]["discountCodeRedeemCodeBulkDelete"]["job"]["done"],
        json!(true)
    );
    assert_eq!(
        delete_response.body["data"]["discountCodeRedeemCodeBulkDelete"]["userErrors"],
        json!([])
    );

    let after_delete = proxy.process_request(json_graphql_request(read, read_vars));
    assert_eq!(
        after_delete.body["data"]["codeDiscountNode"]["codeDiscount"]["codesCount"],
        json!({ "count": 2, "precision": "EXACT" })
    );
    assert_eq!(
        after_delete.body["data"]["exactAdded"]["id"],
        after_delete.body["data"]["codeDiscountNode"]["id"]
    );
    assert_eq!(
        after_delete.body["data"]["lowerAdded"]["id"],
        after_delete.body["data"]["codeDiscountNode"]["id"]
    );
    assert_eq!(after_delete.body["data"]["removed"], Value::Null);
}

#[test]
fn discount_redeem_code_bulk_delete_validation_matches_selector_errors_and_happy_job() {
    let mut proxy = snapshot_proxy();
    let create = r#"mutation SeedBulkDeleteValidation($input: DiscountCodeBasicInput!) { discountCodeBasicCreate(basicCodeDiscount: $input) { codeDiscountNode { id codeDiscount { ... on DiscountCodeBasic { codes { nodes { id code } } } } } userErrors { field message code extraInfo } } }"#;
    let created = proxy.process_request(json_graphql_request(
        create,
        json!({ "input": {
            "title": "Redeem code bulk delete validation",
            "code": "HAR1442BASE",
            "startsAt": "2026-04-27T19:31:14Z",
            "combinesWith": { "productDiscounts": false, "orderDiscounts": true, "shippingDiscounts": false },
            "context": { "all": "ALL" },
            "customerGets": { "value": { "percentage": 0.1 }, "items": { "all": true } }
        }}),
    ));
    let discount_id = created.body["data"]["discountCodeBasicCreate"]["codeDiscountNode"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let redeem_code_id = created.body["data"]["discountCodeBasicCreate"]["codeDiscountNode"]
        ["codeDiscount"]["codes"]["nodes"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let validation = r#"mutation BulkDelete($discountId: ID!, $unknownDiscountId: ID!, $ids: [ID!], $emptyIds: [ID!], $search: String, $blankSearch: String, $savedSearchId: ID!) { missing: discountCodeRedeemCodeBulkDelete(discountId: $discountId) { job { id done } userErrors { field message code extraInfo } } tooMany: discountCodeRedeemCodeBulkDelete(discountId: $discountId, ids: $ids, search: $search) { job { id done } userErrors { field message code extraInfo } } unknownDiscount: discountCodeRedeemCodeBulkDelete(discountId: $unknownDiscountId, ids: $ids) { job { id done } userErrors { field message code extraInfo } } emptyIds: discountCodeRedeemCodeBulkDelete(discountId: $discountId, ids: $emptyIds) { job { id done } userErrors { field message code extraInfo } } blankSearch: discountCodeRedeemCodeBulkDelete(discountId: $discountId, search: $blankSearch) { job { id done } userErrors { field message code extraInfo } } invalidSavedSearch: discountCodeRedeemCodeBulkDelete(discountId: $discountId, savedSearchId: $savedSearchId) { job { id done } userErrors { field message code extraInfo } } }"#;
    let variables = json!({
        "discountId": discount_id,
        "unknownDiscountId": "gid://shopify/DiscountCodeNode/0",
        "ids": [redeem_code_id],
        "emptyIds": [],
        "search": "code:ANY",
        "blankSearch": "   ",
        "savedSearchId": "gid://shopify/SavedSearch/0"
    });
    let response = proxy.process_request(json_graphql_request(validation, variables.clone()));
    assert_eq!(
        response.body["data"]["missing"],
        json!({ "job": null, "userErrors": [{ "field": null, "message": "Missing expected argument key: 'ids', 'search' or 'saved_search_id'.", "code": "MISSING_ARGUMENT", "extraInfo": null }] })
    );
    assert_eq!(
        response.body["data"]["tooMany"],
        json!({ "job": null, "userErrors": [{ "field": null, "message": "Only one of 'ids', 'search' or 'saved_search_id' is allowed.", "code": "TOO_MANY_ARGUMENTS", "extraInfo": null }] })
    );
    assert_eq!(
        response.body["data"]["unknownDiscount"],
        json!({ "job": null, "userErrors": [{ "field": ["discountId"], "message": "Code discount does not exist.", "code": "INVALID", "extraInfo": null }] })
    );
    assert_eq!(
        response.body["data"]["emptyIds"],
        json!({ "job": null, "userErrors": [{ "field": null, "message": "Something went wrong, please try again.", "code": null, "extraInfo": null }] })
    );
    assert_eq!(
        response.body["data"]["blankSearch"],
        json!({ "job": null, "userErrors": [{ "field": ["search"], "message": "'Search' can't be blank.", "code": "BLANK", "extraInfo": null }] })
    );
    assert_eq!(
        response.body["data"]["invalidSavedSearch"],
        json!({ "job": null, "userErrors": [{ "field": ["savedSearchId"], "message": "Invalid 'saved_search_id'.", "code": "INVALID", "extraInfo": null }] })
    );

    let happy = r#"mutation BulkDeleteHappy($discountId: ID!, $ids: [ID!]!) { happy: discountCodeRedeemCodeBulkDelete(discountId: $discountId, ids: $ids) { job { id done } userErrors { field message code extraInfo } } }"#;
    let happy_response = proxy.process_request(json_graphql_request(
        happy,
        json!({ "discountId": variables["discountId"].clone(), "ids": variables["ids"].clone() }),
    ));
    assert_eq!(
        happy_response.body["data"]["happy"]["job"]["done"],
        json!(true)
    );
    assert!(happy_response.body["data"]["happy"]["job"]["id"]
        .as_str()
        .unwrap()
        .starts_with("gid://shopify/Job/"));
    assert_eq!(
        happy_response.body["data"]["happy"]["userErrors"],
        json!([])
    );
}

#[test]
fn discount_redeem_code_bulk_add_validation_tracks_async_results_and_downstream_reads() {
    let mut proxy = snapshot_proxy();
    let create = r#"mutation DiscountRedeemCodeBulkValidationCreate($input: DiscountCodeBasicInput!) { discountCodeBasicCreate(basicCodeDiscount: $input) { codeDiscountNode { id } userErrors { field message code extraInfo } } }"#;
    let created = proxy.process_request(json_graphql_request(create, json!({ "input": { "title": "HAR-784 redeem code validation 1778166762181", "code": "HAR784BASE1778166762181", "startsAt": "2026-05-07T15:11:42.181Z", "combinesWith": { "productDiscounts": false, "orderDiscounts": true, "shippingDiscounts": false }, "context": { "all": "ALL" }, "customerGets": { "value": { "percentage": 0.1 }, "items": { "all": true } } } })));
    let discount_id = json_string(
        &created.body["data"]["discountCodeBasicCreate"]["codeDiscountNode"]["id"],
        "bulk validation discount id",
    );
    assert_synthetic_gid(&discount_id, "DiscountCodeNode");
    assert_eq!(
        created.body["data"]["discountCodeBasicCreate"]["userErrors"],
        json!([])
    );
    let cross_discount = proxy.process_request(json_graphql_request(create, json!({ "input": { "title": "HAR-784 cross discount validation 1778166762181", "code": "HAR784CROSS1778166762181", "startsAt": "2026-05-07T15:11:42.181Z", "combinesWith": { "productDiscounts": false, "orderDiscounts": true, "shippingDiscounts": false }, "context": { "all": "ALL" }, "customerGets": { "value": { "percentage": 0.1 }, "items": { "all": true } } } })));
    let cross_discount_id = json_string(
        &cross_discount.body["data"]["discountCodeBasicCreate"]["codeDiscountNode"]["id"],
        "cross discount id",
    );
    assert_synthetic_gid(&cross_discount_id, "DiscountCodeNode");
    assert_eq!(
        cross_discount.body["data"]["discountCodeBasicCreate"]["userErrors"],
        json!([])
    );

    let add = r#"mutation DiscountRedeemCodeBulkValidationAdd($discountId: ID!, $codes: [DiscountRedeemCodeInput!]!) { discountRedeemCodeBulkAdd(discountId: $discountId, codes: $codes) { bulkCreation { id done codesCount importedCount failedCount codes(first: 10) { nodes { code errors { field message code extraInfo } discountRedeemCode { id code } } } } userErrors { field message code extraInfo } } }"#;
    let unknown = proxy.process_request(json_graphql_request(
        add,
        json!({ "discountId": "gid://shopify/DiscountCodeNode/0", "codes": [{"code":"ABC"}] }),
    ));
    assert_eq!(
        unknown.body["data"]["discountRedeemCodeBulkAdd"]["bulkCreation"],
        json!(null)
    );
    assert_eq!(
        unknown.body["data"]["discountRedeemCodeBulkAdd"]["userErrors"],
        json!([{ "field": ["discountId"], "message": "Code discount does not exist.", "code": "INVALID", "extraInfo": null }])
    );

    let too_many_codes: Vec<_> = (0..251)
        .map(|i| json!({ "code": format!("HAR784MAX1778166762181-{i}") }))
        .collect();
    let too_many = proxy.process_request(json_graphql_request(
        add,
        json!({ "discountId": discount_id.clone(), "codes": too_many_codes }),
    ));
    assert_eq!(
        too_many.body["errors"][0]["message"],
        json!("The input array size of 251 is greater than the maximum allowed of 250.")
    );
    assert_eq!(
        too_many.body["errors"][0]["path"],
        json!(["discountRedeemCodeBulkAdd", "codes"])
    );
    assert_eq!(
        too_many.body["errors"][0]["extensions"]["code"],
        json!("MAX_INPUT_SIZE_EXCEEDED")
    );

    let empty = proxy.process_request(json_graphql_request(
        add,
        json!({ "discountId": discount_id.clone(), "codes": [] }),
    ));
    assert_eq!(
        empty.body["data"]["discountRedeemCodeBulkAdd"]["userErrors"],
        json!([{ "field": ["codes"], "message": "Codes can't be blank", "code": "BLANK", "extraInfo": null }])
    );

    let invalid_codes = json!([{"code":""},{"code":"HAR784NL1778166762181\nBAD"},{"code":"HAR784CR1778166762181\rBAD"},{"code":"XXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX"},{"code":"HAR784DUP1778166762181"},{"code":"HAR784DUP1778166762181"},{"code":"HAR784OK1778166762181"}]);
    let invalid_add = proxy.process_request(json_graphql_request(
        add,
        json!({ "discountId": discount_id.clone(), "codes": invalid_codes }),
    ));
    let invalid_bulk_id = invalid_add.body["data"]["discountRedeemCodeBulkAdd"]["bulkCreation"]
        ["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(
        invalid_add.body["data"]["discountRedeemCodeBulkAdd"]["bulkCreation"]["done"],
        json!(false)
    );
    assert_eq!(
        invalid_add.body["data"]["discountRedeemCodeBulkAdd"]["bulkCreation"]["codesCount"],
        json!(7)
    );
    assert_eq!(
        invalid_add.body["data"]["discountRedeemCodeBulkAdd"]["bulkCreation"]["importedCount"],
        json!(0)
    );
    assert_eq!(
        invalid_add.body["data"]["discountRedeemCodeBulkAdd"]["bulkCreation"]["failedCount"],
        json!(0)
    );
    assert_eq!(
        invalid_add.body["data"]["discountRedeemCodeBulkAdd"]["bulkCreation"]["codes"]["nodes"][0]
            ["errors"],
        json!([])
    );

    let creation_read = r#"query DiscountRedeemCodeBulkValidationCreationRead($id: ID!) { discountRedeemCodeBulkCreation(id: $id) { done codesCount importedCount failedCount codes(first: 10) { nodes { code errors { field message code extraInfo } discountRedeemCode { code } } } } }"#;
    let invalid_final = proxy.process_request(json_graphql_request(
        creation_read,
        json!({ "id": invalid_bulk_id }),
    ));
    assert_eq!(
        invalid_final.body["data"]["discountRedeemCodeBulkCreation"]["done"],
        json!(true)
    );
    assert_eq!(
        invalid_final.body["data"]["discountRedeemCodeBulkCreation"]["importedCount"],
        json!(2)
    );
    assert_eq!(
        invalid_final.body["data"]["discountRedeemCodeBulkCreation"]["failedCount"],
        json!(5)
    );
    assert_eq!(
        invalid_final.body["data"]["discountRedeemCodeBulkCreation"]["codes"]["nodes"][0]["errors"]
            [0]["message"],
        json!("is too short (minimum is 1 character)")
    );
    assert_eq!(
        invalid_final.body["data"]["discountRedeemCodeBulkCreation"]["codes"]["nodes"][5]["errors"]
            [0]["message"],
        json!("Codes must be unique within BulkDiscountCodeCreation")
    );

    let read = r#"query DiscountRedeemCodeBulkValidationRead($discountId: ID!, $duplicateCode: String!, $validCode: String!) { codeDiscountNode(id: $discountId) { codeDiscount { ... on DiscountCodeBasic { codes(first: 10) { nodes { code } } codesCount { count precision } } } } duplicate: codeDiscountNodeByCode(code: $duplicateCode) { id } valid: codeDiscountNodeByCode(code: $validCode) { id } }"#;
    let read_after_invalid = proxy.process_request(json_graphql_request(read, json!({ "discountId": discount_id.clone(), "duplicateCode": "HAR784DUP1778166762181", "validCode": "HAR784OK1778166762181" })));
    assert_eq!(
        read_after_invalid.body["data"]["codeDiscountNode"]["codeDiscount"]["codesCount"],
        json!({ "count": 3, "precision": "EXACT" })
    );
    assert_eq!(
        read_after_invalid.body["data"]["duplicate"]["id"],
        json!(discount_id)
    );
    assert_eq!(
        read_after_invalid.body["data"]["valid"]["id"],
        json!(discount_id)
    );

    let conflicts = json!([{"code":"HAR784BASE1778166762181"},{"code":"HAR784CROSS1778166762181"},{"code":"HAR784FRESH1778166762181"}]);
    let conflicts_add = proxy.process_request(json_graphql_request(
        add,
        json!({ "discountId": discount_id.clone(), "codes": conflicts }),
    ));
    let conflicts_bulk_id = conflicts_add.body["data"]["discountRedeemCodeBulkAdd"]["bulkCreation"]
        ["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(
        conflicts_add.body["data"]["discountRedeemCodeBulkAdd"]["bulkCreation"]["done"],
        json!(false)
    );
    assert_eq!(
        conflicts_add.body["data"]["discountRedeemCodeBulkAdd"]["bulkCreation"]["codesCount"],
        json!(3)
    );

    let conflicts_final = proxy.process_request(json_graphql_request(
        creation_read,
        json!({ "id": conflicts_bulk_id }),
    ));
    assert_eq!(
        conflicts_final.body["data"]["discountRedeemCodeBulkCreation"]["importedCount"],
        json!(1)
    );
    assert_eq!(
        conflicts_final.body["data"]["discountRedeemCodeBulkCreation"]["failedCount"],
        json!(2)
    );
    assert_eq!(
        conflicts_final.body["data"]["discountRedeemCodeBulkCreation"]["codes"]["nodes"][0]
            ["errors"][0]["message"],
        json!("must be unique. Please try a different code.")
    );
    assert_eq!(
        conflicts_final.body["data"]["discountRedeemCodeBulkCreation"]["codes"]["nodes"][2]
            ["discountRedeemCode"]["code"],
        json!("HAR784FRESH1778166762181")
    );

    let existing_read = r#"query DiscountRedeemCodeBulkValidationExistingRead($discountId: ID!, $sameDiscountCode: String!, $crossDiscountCode: String!, $freshCode: String!) { codeDiscountNode(id: $discountId) { codeDiscount { ... on DiscountCodeBasic { codes(first: 10) { nodes { code } } codesCount { count precision } } } } sameDiscount: codeDiscountNodeByCode(code: $sameDiscountCode) { id } crossDiscount: codeDiscountNodeByCode(code: $crossDiscountCode) { id } fresh: codeDiscountNodeByCode(code: $freshCode) { id } }"#;
    let read_after_conflicts = proxy.process_request(json_graphql_request(existing_read, json!({ "discountId": discount_id.clone(), "sameDiscountCode": "HAR784BASE1778166762181", "crossDiscountCode": "HAR784CROSS1778166762181", "freshCode": "HAR784FRESH1778166762181" })));
    assert_eq!(
        read_after_conflicts.body["data"]["codeDiscountNode"]["codeDiscount"]["codesCount"],
        json!({ "count": 4, "precision": "EXACT" })
    );
    assert_eq!(
        read_after_conflicts.body["data"]["sameDiscount"]["id"],
        json!(discount_id)
    );
    assert_eq!(
        read_after_conflicts.body["data"]["crossDiscount"]["id"],
        json!(cross_discount_id)
    );
    assert_eq!(
        read_after_conflicts.body["data"]["fresh"]["id"],
        json!(discount_id)
    );
}

#[test]
fn discount_update_edge_cases_reject_bulk_code_change_and_coerce_bxgy() {
    let mut proxy = snapshot_proxy();
    let create_basic = r#"mutation DiscountUpdateEdgeBasicCreate($input: DiscountCodeBasicInput!) { discountCodeBasicCreate(basicCodeDiscount: $input) { codeDiscountNode { id } userErrors { field message code extraInfo } } }"#;
    let created = proxy.process_request(json_graphql_request(create_basic, json!({ "input": { "title": "HAR-605 bulk rule 1778002393771", "code": "HAR605BULK1778002393771", "startsAt": "2026-04-25T00:00:00Z", "context": { "all": "ALL" }, "customerGets": { "value": { "percentage": 0.1 }, "items": { "all": true } } } })));
    assert_eq!(
        created.body["data"]["discountCodeBasicCreate"]["userErrors"],
        json!([])
    );
    let bulk_discount_id = json_string(
        &created.body["data"]["discountCodeBasicCreate"]["codeDiscountNode"]["id"],
        "bulk discount id",
    );
    assert_synthetic_gid(&bulk_discount_id, "DiscountCodeNode");

    let bulk_add = r#"mutation DiscountUpdateEdgeBulkAdd($discountId: ID!, $codes: [DiscountRedeemCodeInput!]!) { discountRedeemCodeBulkAdd(discountId: $discountId, codes: $codes) { bulkCreation { codesCount } userErrors { field message code extraInfo } } }"#;
    let bulk_added = proxy.process_request(json_graphql_request(bulk_add, json!({ "discountId": bulk_discount_id.clone(), "codes": [{"code":"HAR605BULK1778002393771_1"},{"code":"HAR605BULK1778002393771_2"},{"code":"HAR605BULK1778002393771_3"},{"code":"HAR605BULK1778002393771_4"},{"code":"HAR605BULK1778002393771_5"}] })));
    assert_eq!(
        bulk_added.body["data"]["discountRedeemCodeBulkAdd"]["bulkCreation"]["codesCount"],
        json!(5)
    );
    assert_eq!(
        bulk_added.body["data"]["discountRedeemCodeBulkAdd"]["userErrors"],
        json!([])
    );

    let basic_update = r#"mutation DiscountUpdateEdgeBasicUpdate($id: ID!, $input: DiscountCodeBasicInput!) { discountCodeBasicUpdate(id: $id, basicCodeDiscount: $input) { codeDiscountNode { id codeDiscount { __typename } } userErrors { field message code extraInfo } } }"#;
    let code_change = proxy.process_request(json_graphql_request(basic_update, json!({ "id": bulk_discount_id.clone(), "input": { "title": "HAR-605 bulk renamed 1778002393771", "code": "HAR605BULKNEW1778002393771", "startsAt": "2026-04-25T00:00:00Z", "context": { "all": "ALL" }, "customerGets": { "value": { "percentage": 0.2 }, "items": { "all": true } } } })));
    assert_eq!(
        code_change.body["data"]["discountCodeBasicUpdate"]["codeDiscountNode"],
        json!(null)
    );
    assert_eq!(
        code_change.body["data"]["discountCodeBasicUpdate"]["userErrors"],
        json!([{ "field": ["id"], "message": "Cannot update the code of a bulk discount.", "code": null, "extraInfo": null }])
    );

    let create_bxgy = r#"mutation DiscountUpdateEdgeBxgyCreate($input: DiscountCodeBxgyInput!) { discountCodeBxgyCreate(bxgyCodeDiscount: $input) { codeDiscountNode { id codeDiscount { __typename } } userErrors { field message code extraInfo } } }"#;
    let bxgy = proxy.process_request(json_graphql_request(create_bxgy, json!({ "input": { "title": "HAR-605 BXGY 1778002393771", "code": "HAR605BXGY1778002393771", "startsAt": "2026-04-25T00:00:00Z", "context": { "all": "ALL" }, "customerBuys": { "value": { "quantity": "1" }, "items": { "products": { "productsToAdd": ["gid://shopify/Product/10177504608562"] } } }, "customerGets": { "value": { "discountOnQuantity": { "quantity": "1", "effect": { "percentage": 0.5 } } }, "items": { "products": { "productsToAdd": ["gid://shopify/Product/10177504641330"] } } } } })));
    assert_eq!(
        bxgy.body["data"]["discountCodeBxgyCreate"]["codeDiscountNode"]["codeDiscount"]
            ["__typename"],
        json!("DiscountCodeBxgy")
    );
    let bxgy_id = json_string(
        &bxgy.body["data"]["discountCodeBxgyCreate"]["codeDiscountNode"]["id"],
        "bxgy discount id",
    );
    assert_synthetic_gid(&bxgy_id, "DiscountCodeNode");

    let bxgy_to_basic = proxy.process_request(json_graphql_request(basic_update, json!({ "id": bxgy_id, "input": { "title": "HAR-605 coerced basic 1778002393771", "code": "HAR605BXGY1778002393771", "startsAt": "2026-04-25T00:00:00Z", "context": { "all": "ALL" }, "customerGets": { "value": { "percentage": 0.25 }, "items": { "all": true } } } })));
    assert_eq!(
        bxgy_to_basic.body["data"]["discountCodeBasicUpdate"]["codeDiscountNode"]["codeDiscount"]
            ["__typename"],
        json!("DiscountCodeBasic")
    );
    assert_eq!(
        bxgy_to_basic.body["data"]["discountCodeBasicUpdate"]["userErrors"],
        json!([])
    );

    let unknown = r#"mutation DiscountUpdateEdgeUnknownUpdate($id: ID!, $input: DiscountCodeBasicInput!) { discountCodeBasicUpdate(id: $id, basicCodeDiscount: $input) { codeDiscountNode { id } userErrors { field message code extraInfo } } }"#;
    let unknown_response = proxy.process_request(json_graphql_request(unknown, json!({ "id": "gid://shopify/DiscountCodeNode/0", "input": { "title": "HAR-605 unknown 1778002393771", "code": "HAR605UNKNOWN1778002393771", "startsAt": "2026-04-25T00:00:00Z", "context": { "all": "ALL" }, "customerGets": { "value": { "percentage": 0.1 }, "items": { "all": true } } } })));
    assert_eq!(
        unknown_response.body["data"]["discountCodeBasicUpdate"]["codeDiscountNode"],
        json!(null)
    );
    assert_eq!(
        unknown_response.body["data"]["discountCodeBasicUpdate"]["userErrors"],
        json!([{ "field": ["id"], "message": "Discount does not exist", "code": null, "extraInfo": null }])
    );
}

#[test]
fn discount_subscription_fields_not_permitted_matches_local_runtime_gating() {
    let mut proxy = snapshot_proxy();
    let primary = r#"
        mutation DiscountSubscriptionFieldsNotPermitted {
          basicSub: discountCodeBasicCreate(basicCodeDiscount: { title: "Sub gated", code: "SUB-GATED", startsAt: "2026-04-25T00:00:00Z", customerGets: { value: { percentage: 0.1 }, items: { all: true }, appliesOnSubscription: true } }) { codeDiscountNode { id } userErrors { field message code extraInfo } }
          basicBlank: discountCodeBasicCreate(basicCodeDiscount: { title: "Sub blank", code: "SUB-BLANK", startsAt: "2026-04-25T00:00:00Z", customerGets: { value: { percentage: 0.1 }, items: { all: true }, appliesOnSubscription: null } }) { codeDiscountNode { id } userErrors { field message code extraInfo } }
          freeShippingSub: discountCodeFreeShippingCreate(freeShippingCodeDiscount: { title: "Free shipping sub gated", code: "SHIP-SUB-GATED", startsAt: "2026-04-25T00:00:00Z", destination: { all: true }, appliesOnSubscription: true }) { codeDiscountNode { id } userErrors { field message code extraInfo } }
          freeShippingRecurring: discountCodeFreeShippingCreate(freeShippingCodeDiscount: { title: "Free shipping recurring gated", code: "SHIP-REC-GATED", startsAt: "2026-04-25T00:00:00Z", destination: { all: true }, recurringCycleLimit: 2 }) { codeDiscountNode { id } userErrors { field message code extraInfo } }
          automaticBasicSub: discountAutomaticBasicCreate(automaticBasicDiscount: { title: "Automatic basic sub gated", startsAt: "2026-04-25T00:00:00Z", customerGets: { value: { percentage: 0.1 }, items: { all: true }, appliesOnSubscription: true } }) { automaticDiscountNode { id } userErrors { field message code extraInfo } }
          automaticBasicRecurring: discountAutomaticBasicCreate(automaticBasicDiscount: { title: "Automatic basic recurring gated", startsAt: "2026-04-25T00:00:00Z", recurringCycleLimit: 2, customerGets: { value: { percentage: 0.1 }, items: { all: true } } }) { automaticDiscountNode { id } userErrors { field message code extraInfo } }
          automaticFreeShippingSkip: discountAutomaticFreeShippingCreate(freeShippingAutomaticDiscount: { title: "Automatic shipping skip", startsAt: "2026-04-25T00:00:00Z", destination: { all: true }, appliesOnSubscription: true, appliesOnOneTimePurchase: false, recurringCycleLimit: 2 }) { automaticDiscountNode { id } userErrors { field message code extraInfo } }
          setupBasic: discountCodeBasicCreate(basicCodeDiscount: { title: "Setup basic", code: "SETUP-BASIC-SUB", startsAt: "2026-04-25T00:00:00Z", customerGets: { value: { percentage: 0.1 }, items: { all: true } } }) { codeDiscountNode { id } userErrors { field message code extraInfo } }
          setupFreeShipping: discountCodeFreeShippingCreate(freeShippingCodeDiscount: { title: "Setup shipping", code: "SETUP-SHIP-SUB", startsAt: "2026-04-25T00:00:00Z", destination: { all: true } }) { codeDiscountNode { id } userErrors { field message code extraInfo } }
          setupAutomaticBasic: discountAutomaticBasicCreate(automaticBasicDiscount: { title: "Setup automatic basic", startsAt: "2026-04-25T00:00:00Z", customerGets: { value: { percentage: 0.1 }, items: { all: true } } }) { automaticDiscountNode { id } userErrors { field message code extraInfo } }
        }
    "#;
    let response = proxy.process_request(json_graphql_request(primary, json!({})));
    assert_eq!(
        response.body["data"]["basicSub"]["codeDiscountNode"],
        json!(null)
    );
    assert_eq!(
        response.body["data"]["basicSub"]["userErrors"][0]["field"],
        json!(["basicCodeDiscount", "customerGets", "appliesOnSubscription"])
    );
    assert_eq!(
        response.body["data"]["freeShippingRecurring"]["userErrors"][0]["message"],
        json!("Recurring cycle limit is not permitted for this shop.")
    );
    let automatic_free_shipping_id = json_string(
        &response.body["data"]["automaticFreeShippingSkip"]["automaticDiscountNode"]["id"],
        "automatic free shipping id",
    );
    let setup_basic_id = json_string(
        &response.body["data"]["setupBasic"]["codeDiscountNode"]["id"],
        "setup basic discount id",
    );
    let setup_free_shipping_id = json_string(
        &response.body["data"]["setupFreeShipping"]["codeDiscountNode"]["id"],
        "setup free shipping id",
    );
    let setup_automatic_basic_id = json_string(
        &response.body["data"]["setupAutomaticBasic"]["automaticDiscountNode"]["id"],
        "setup automatic basic id",
    );
    assert_synthetic_gid(&automatic_free_shipping_id, "DiscountAutomaticNode");
    assert_synthetic_gid(&setup_basic_id, "DiscountCodeNode");
    assert_synthetic_gid(&setup_free_shipping_id, "DiscountCodeNode");
    assert_synthetic_gid(&setup_automatic_basic_id, "DiscountAutomaticNode");

    let basic_update = r#"mutation DiscountSubscriptionFieldsBasicUpdate($id: ID!) { basicUpdate: discountCodeBasicUpdate(id: $id, basicCodeDiscount: { title: "Setup basic", code: "SETUP-BASIC-SUB", startsAt: "2026-04-25T00:00:00Z", customerGets: { value: { percentage: 0.1 }, items: { all: true }, appliesOnSubscription: true } }) { codeDiscountNode { id } userErrors { field message code extraInfo } } }"#;
    let updated = proxy.process_request(json_graphql_request(
        basic_update,
        json!({ "id": setup_basic_id }),
    ));
    assert_eq!(
        updated.body["data"]["basicUpdate"]["codeDiscountNode"],
        json!(null)
    );
    assert_eq!(
        updated.body["data"]["basicUpdate"]["userErrors"][0]["message"],
        json!("Customer gets applies on subscription is not permitted for this shop.")
    );

    let automatic_free_shipping_update = r#"mutation DiscountSubscriptionFieldsAutomaticFreeShippingUpdate($id: ID!) { automaticFreeShippingUpdate: discountAutomaticFreeShippingUpdate(id: $id, freeShippingAutomaticDiscount: { title: "Automatic shipping skip", startsAt: "2026-04-25T00:00:00Z", destination: { all: true }, appliesOnSubscription: true, appliesOnOneTimePurchase: false, recurringCycleLimit: 3 }) { automaticDiscountNode { id } userErrors { field message code extraInfo } } }"#;
    let automatic_free_shipping_updated = proxy.process_request(json_graphql_request(
        automatic_free_shipping_update,
        json!({ "id": automatic_free_shipping_id.clone() }),
    ));
    assert_eq!(
        automatic_free_shipping_updated.body["data"]["automaticFreeShippingUpdate"]
            ["automaticDiscountNode"]["id"],
        json!(automatic_free_shipping_id)
    );
    assert_eq!(
        automatic_free_shipping_updated.body["data"]["automaticFreeShippingUpdate"]["userErrors"],
        json!([])
    );
}

#[test]
fn discount_status_time_window_derives_create_and_read_filters() {
    let mut proxy = snapshot_proxy();
    let create_query = r#"
        mutation DiscountStatusTimeWindowDerivationCreate(
          $scheduled: DiscountCodeBasicInput!
          $expired: DiscountCodeBasicInput!
          $active: DiscountCodeBasicInput!
        ) {
          scheduled: discountCodeBasicCreate(basicCodeDiscount: $scheduled) { codeDiscountNode { id codeDiscount { __typename ... on DiscountCodeBasic { title status startsAt endsAt } } } userErrors { field message code extraInfo } }
          expired: discountCodeBasicCreate(basicCodeDiscount: $expired) { codeDiscountNode { id codeDiscount { __typename ... on DiscountCodeBasic { title status startsAt endsAt } } } userErrors { field message code extraInfo } }
          active: discountCodeBasicCreate(basicCodeDiscount: $active) { codeDiscountNode { id codeDiscount { __typename ... on DiscountCodeBasic { title status startsAt endsAt } } } userErrors { field message code extraInfo } }
        }
    "#;
    let created = proxy.process_request(json_graphql_request(create_query, json!({
        "scheduled": { "title": "HAR-593 scheduled 1777950794226", "code": "HAR593S1777950794226", "startsAt": "2099-01-01T00:00:00Z", "context": { "all": "ALL" }, "customerGets": { "value": { "percentage": 0.1 }, "items": { "all": true } } },
        "expired": { "title": "HAR-593 expired 1777950794226", "code": "HAR593E1777950794226", "startsAt": "2019-01-01T00:00:00Z", "endsAt": "2020-01-01T00:00:00Z", "context": { "all": "ALL" }, "customerGets": { "value": { "percentage": 0.1 }, "items": { "all": true } } },
        "active": { "title": "HAR-593 active 1777950794226", "code": "HAR593A1777950794226", "startsAt": "2020-01-01T00:00:00Z", "endsAt": "2099-01-01T00:00:00Z", "context": { "all": "ALL" }, "customerGets": { "value": { "percentage": 0.1 }, "items": { "all": true } } }
    })));
    assert_eq!(
        created.body["data"]["scheduled"]["codeDiscountNode"]["codeDiscount"]["status"],
        json!("SCHEDULED")
    );
    assert_eq!(
        created.body["data"]["expired"]["codeDiscountNode"]["codeDiscount"]["status"],
        json!("EXPIRED")
    );
    assert_eq!(
        created.body["data"]["active"]["codeDiscountNode"]["codeDiscount"]["status"],
        json!("ACTIVE")
    );
    assert_eq!(created.body["data"]["scheduled"]["userErrors"], json!([]));
    let scheduled_id = json_string(
        &created.body["data"]["scheduled"]["codeDiscountNode"]["id"],
        "scheduled discount id",
    );
    let expired_id = json_string(
        &created.body["data"]["expired"]["codeDiscountNode"]["id"],
        "expired discount id",
    );
    let active_id = json_string(
        &created.body["data"]["active"]["codeDiscountNode"]["id"],
        "active discount id",
    );
    for id in [&scheduled_id, &expired_id, &active_id] {
        assert_synthetic_gid(id, "DiscountCodeNode");
    }

    let read_query = r#"
        query DiscountStatusTimeWindowDerivationRead($scheduledId: ID!, $expiredId: ID!, $activeId: ID!, $scheduledQuery: String!, $expiredQuery: String!) {
          scheduledNode: codeDiscountNode(id: $scheduledId) { codeDiscount { __typename ... on DiscountCodeBasic { title status startsAt endsAt } } }
          expiredNode: codeDiscountNode(id: $expiredId) { codeDiscount { __typename ... on DiscountCodeBasic { title status startsAt endsAt } } }
          activeNode: discountNode(id: $activeId) { discount { __typename ... on DiscountCodeBasic { title status startsAt endsAt } } }
          scheduledDiscountNodes: discountNodes(first: 5, query: $scheduledQuery) { nodes { discount { __typename ... on DiscountCodeBasic { title status } } } }
          expiredDiscountNodesCount: discountNodesCount(query: $expiredQuery) { count precision }
        }
    "#;
    let read = proxy.process_request(json_graphql_request(
        read_query,
        json!({
            "scheduledId": scheduled_id,
            "expiredId": expired_id,
            "activeId": active_id,
            "scheduledQuery": "status:scheduled title:'HAR-593 scheduled 1777950794226'",
            "expiredQuery": "status:expired title:'HAR-593 expired 1777950794226'"
        }),
    ));
    assert_eq!(
        read.body["data"]["scheduledNode"]["codeDiscount"]["status"],
        json!("SCHEDULED")
    );
    assert_eq!(
        read.body["data"]["expiredNode"]["codeDiscount"]["endsAt"],
        json!("2020-01-01T00:00:00Z")
    );
    assert_eq!(
        read.body["data"]["activeNode"]["discount"]["title"],
        json!("HAR-593 active 1777950794226")
    );
    assert_eq!(
        read.body["data"]["scheduledDiscountNodes"]["nodes"],
        json!([{ "discount": { "__typename": "DiscountCodeBasic", "title": "HAR-593 scheduled 1777950794226", "status": "SCHEDULED" } }])
    );
    assert_eq!(
        read.body["data"]["expiredDiscountNodesCount"],
        json!({ "count": 1, "precision": "EXACT" })
    );
}

#[test]
fn discount_free_shipping_lifecycle_stages_code_and_automatic_statuses() {
    let mut proxy = snapshot_proxy();
    let create_query = r#"
        mutation DiscountFreeShippingLifecycleCreate($codeInput: DiscountCodeFreeShippingInput!, $automaticInput: DiscountAutomaticFreeShippingInput!) {
          discountCodeFreeShippingCreate(freeShippingCodeDiscount: $codeInput) { codeDiscountNode { id codeDiscount { __typename ... on DiscountCodeFreeShipping { title asyncUsageCount discountClasses combinesWith { productDiscounts orderDiscounts shippingDiscounts } codes(first: 2) { nodes { code asyncUsageCount } } destinationSelection { __typename ... on DiscountCountryAll { allCountries } ... on DiscountCountries { countries includeRestOfWorld } } maximumShippingPrice { amount currencyCode } appliesOncePerCustomer appliesOnOneTimePurchase appliesOnSubscription recurringCycleLimit usageLimit } } } userErrors { field message code extraInfo } }
          discountAutomaticFreeShippingCreate(freeShippingAutomaticDiscount: $automaticInput) { automaticDiscountNode { id automaticDiscount { __typename ... on DiscountAutomaticFreeShipping { title asyncUsageCount discountClasses combinesWith { productDiscounts orderDiscounts shippingDiscounts } destinationSelection { __typename ... on DiscountCountryAll { allCountries } ... on DiscountCountries { countries includeRestOfWorld } } maximumShippingPrice { amount currencyCode } appliesOnOneTimePurchase appliesOnSubscription recurringCycleLimit } } } userErrors { field message code extraInfo } }
        }
    "#;
    let created = proxy.process_request(json_graphql_request(create_query, json!({
        "codeInput": { "title": "HAR-196 code free shipping 1777150170404", "code": "HAR196FREE1777150170404", "startsAt": "2026-04-25T20:48:30.404Z", "combinesWith": { "productDiscounts": true, "orderDiscounts": false, "shippingDiscounts": false }, "context": { "all": "ALL" }, "minimumRequirement": { "subtotal": { "greaterThanOrEqualToSubtotal": "10.00" } }, "destination": { "all": true }, "maximumShippingPrice": "25.00", "appliesOncePerCustomer": true, "usageLimit": 5 },
        "automaticInput": { "title": "HAR-196 automatic free shipping 1777150170404", "startsAt": "2026-04-25T20:48:30.404Z", "endsAt": null, "combinesWith": { "productDiscounts": false, "orderDiscounts": true, "shippingDiscounts": false }, "context": { "all": "ALL" }, "minimumRequirement": { "subtotal": { "greaterThanOrEqualToSubtotal": "15.00" } }, "destination": { "all": true }, "maximumShippingPrice": "20.00" }
    })));
    assert_eq!(
        created.body["data"]["discountCodeFreeShippingCreate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        created.body["data"]["discountCodeFreeShippingCreate"]["codeDiscountNode"]["codeDiscount"]
            ["codes"]["nodes"][0]["code"],
        json!("HAR196FREE1777150170404")
    );
    assert_eq!(
        created.body["data"]["discountAutomaticFreeShippingCreate"]["automaticDiscountNode"]
            ["automaticDiscount"]["maximumShippingPrice"],
        json!({ "amount": "20.0", "currencyCode": "CAD" })
    );
    let code_id = json_string(
        &created.body["data"]["discountCodeFreeShippingCreate"]["codeDiscountNode"]["id"],
        "free shipping code discount id",
    );
    let automatic_id = json_string(
        &created.body["data"]["discountAutomaticFreeShippingCreate"]["automaticDiscountNode"]["id"],
        "free shipping automatic discount id",
    );
    assert_synthetic_gid(&code_id, "DiscountCodeNode");
    assert_synthetic_gid(&automatic_id, "DiscountAutomaticNode");

    let code_update = r#"mutation DiscountCodeFreeShippingLifecycleUpdate($id: ID!, $input: DiscountCodeFreeShippingInput!) { discountCodeFreeShippingUpdate(id: $id, freeShippingCodeDiscount: $input) { codeDiscountNode { id codeDiscount { __typename ... on DiscountCodeFreeShipping { title destinationSelection { __typename ... on DiscountCountries { countries includeRestOfWorld } } maximumShippingPrice { amount currencyCode } appliesOncePerCustomer appliesOnOneTimePurchase appliesOnSubscription recurringCycleLimit usageLimit } } } userErrors { field message code extraInfo } } }"#;
    let updated = proxy.process_request(json_graphql_request(
        code_update,
        json!({ "id": code_id.clone(), "input": { "title": "HAR-196 code free shipping updated 1777150170404", "code": "HAR196SHIP1777150170404", "startsAt": "2026-04-25T20:48:30.404Z", "combinesWith": { "productDiscounts": true, "orderDiscounts": false, "shippingDiscounts": false }, "context": { "all": "ALL" }, "destination": { "countries": { "add": ["CA", "US"] } }, "maximumShippingPrice": "30.00", "appliesOncePerCustomer": false, "usageLimit": 10 } }),
    ));
    assert_eq!(
        updated.body["data"]["discountCodeFreeShippingUpdate"]["codeDiscountNode"]["codeDiscount"]
            ["destinationSelection"],
        json!({ "__typename": "DiscountCountries", "countries": ["CA", "US"], "includeRestOfWorld": false })
    );
    assert_eq!(
        updated.body["data"]["discountCodeFreeShippingUpdate"]["userErrors"],
        json!([])
    );

    let automatic_update = r#"mutation DiscountAutomaticFreeShippingLifecycleUpdate($id: ID!, $input: DiscountAutomaticFreeShippingInput!) { discountAutomaticFreeShippingUpdate(id: $id, freeShippingAutomaticDiscount: $input) { automaticDiscountNode { id automaticDiscount { __typename ... on DiscountAutomaticFreeShipping { title destinationSelection { __typename ... on DiscountCountries { countries includeRestOfWorld } } maximumShippingPrice { amount currencyCode } appliesOnOneTimePurchase appliesOnSubscription recurringCycleLimit } } } userErrors { field message code extraInfo } } }"#;
    let automatic_updated = proxy.process_request(json_graphql_request(
        automatic_update,
        json!({ "id": automatic_id.clone(), "input": { "title": "HAR-196 automatic free shipping updated 1777150170404", "startsAt": "2026-04-25T20:48:30.404Z", "combinesWith": { "productDiscounts": false, "orderDiscounts": true, "shippingDiscounts": false }, "context": { "all": "ALL" }, "destination": { "countries": { "add": ["US"] } }, "maximumShippingPrice": "18.00" } }),
    ));
    assert_eq!(
        automatic_updated.body["data"]["discountAutomaticFreeShippingUpdate"]
            ["automaticDiscountNode"]["automaticDiscount"]["destinationSelection"],
        json!({ "__typename": "DiscountCountries", "countries": ["US"], "includeRestOfWorld": false })
    );

    let read_query = r#"query DiscountFreeShippingLifecycleRead($codeId: ID!, $automaticId: ID!, $code: String!) { discountNode(id: $codeId) { id discount { __typename ... on DiscountCodeFreeShipping { title status } } } codeDiscountNodeByCode(code: $code) { id } automaticDiscountNode(id: $automaticId) { id automaticDiscount { __typename ... on DiscountAutomaticFreeShipping { title status } } } }"#;
    let read_after_update = proxy.process_request(json_graphql_request(read_query, json!({ "codeId": code_id.clone(), "automaticId": automatic_id.clone(), "code": "HAR196SHIP1777150170404" })));
    assert_eq!(
        read_after_update.body["data"]["discountNode"]["discount"]["title"],
        json!("HAR-196 code free shipping updated 1777150170404")
    );
    assert_eq!(
        read_after_update.body["data"]["automaticDiscountNode"]["automaticDiscount"]["status"],
        json!("ACTIVE")
    );

    let code_deactivate = r#"mutation DiscountFreeShippingLifecycleDeactivate($id: ID!) { discountCodeDeactivate(id: $id) { codeDiscountNode { id codeDiscount { __typename ... on DiscountCodeFreeShipping { title status } } } userErrors { field message code extraInfo } } }"#;
    let code_deactivated = proxy.process_request(json_graphql_request(
        code_deactivate,
        json!({ "id": code_id.clone() }),
    ));
    assert_eq!(
        code_deactivated.body["data"]["discountCodeDeactivate"]["codeDiscountNode"]["codeDiscount"]
            ["status"],
        json!("EXPIRED")
    );

    let automatic_delete = r#"mutation DiscountFreeShippingLifecycleDelete($id: ID!) { discountAutomaticDelete(id: $id) { deletedAutomaticDiscountId userErrors { field message code extraInfo } } }"#;
    let automatic_deleted = proxy.process_request(json_graphql_request(
        automatic_delete,
        json!({ "id": automatic_id.clone() }),
    ));
    assert_eq!(
        automatic_deleted.body["data"]["discountAutomaticDelete"]["userErrors"],
        json!([])
    );

    let code_delete = r#"mutation DiscountFreeShippingLifecycleDelete($id: ID!) { discountCodeDelete(id: $id) { deletedCodeDiscountId userErrors { field message code extraInfo } } }"#;
    let _ = proxy.process_request(json_graphql_request(
        code_delete,
        json!({ "id": code_id.clone() }),
    ));
    let read_after_delete = proxy.process_request(json_graphql_request(read_query, json!({ "codeId": code_id, "automaticId": automatic_id, "code": "HAR196SHIP1777150170404" })));
    assert_eq!(read_after_delete.body["data"]["discountNode"], json!(null));
    assert_eq!(
        read_after_delete.body["data"]["codeDiscountNodeByCode"],
        json!(null)
    );
    assert_eq!(
        read_after_delete.body["data"]["automaticDiscountNode"],
        json!(null)
    );
}

#[test]
fn discount_class_inference_stages_all_discount_classes_and_product_count() {
    let mut proxy = snapshot_proxy();
    let create_query = r#"
        mutation DiscountClassInferenceCreate(
          $basicAll: DiscountCodeBasicInput!
          $basicProduct: DiscountCodeBasicInput!
          $basicCollection: DiscountCodeBasicInput!
          $bxgy: DiscountCodeBxgyInput!
          $freeShipping: DiscountCodeFreeShippingInput!
        ) {
          basicAll: discountCodeBasicCreate(basicCodeDiscount: $basicAll) { codeDiscountNode { codeDiscount { __typename ... on DiscountCodeBasic { title discountClasses } } } userErrors { field message code extraInfo } }
          basicProduct: discountCodeBasicCreate(basicCodeDiscount: $basicProduct) { codeDiscountNode { codeDiscount { __typename ... on DiscountCodeBasic { title discountClasses } } } userErrors { field message code extraInfo } }
          basicCollection: discountCodeBasicCreate(basicCodeDiscount: $basicCollection) { codeDiscountNode { codeDiscount { __typename ... on DiscountCodeBasic { title discountClasses } } } userErrors { field message code extraInfo } }
          bxgy: discountCodeBxgyCreate(bxgyCodeDiscount: $bxgy) { codeDiscountNode { codeDiscount { __typename ... on DiscountCodeBxgy { title discountClasses } } } userErrors { field message code extraInfo } }
          freeShipping: discountCodeFreeShippingCreate(freeShippingCodeDiscount: $freeShipping) { codeDiscountNode { codeDiscount { __typename ... on DiscountCodeFreeShipping { title discountClasses } } } userErrors { field message code extraInfo } }
        }
    "#;
    let created = proxy.process_request(json_graphql_request(
        create_query,
        json!({
            "basicAll": { "title": "HAR597CLASS1777950382203 basic order", "code": "HAR597ORDER1777950382203", "startsAt": "2026-05-05T03:05:22.203Z", "context": { "all": "ALL" }, "customerGets": { "value": { "percentage": 0.1 }, "items": { "all": true } } },
            "basicProduct": { "title": "HAR597CLASS1777950382203 basic product", "code": "HAR597PRODUCT1777950382203", "startsAt": "2026-05-05T03:05:22.203Z", "context": { "all": "ALL" }, "customerGets": { "value": { "percentage": 0.1 }, "items": { "products": { "productsToAdd": ["gid://shopify/Product/10177002799410"] } } } },
            "basicCollection": { "title": "HAR597CLASS1777950382203 basic collection", "code": "HAR597COLL1777950382203", "startsAt": "2026-05-05T03:05:22.203Z", "context": { "all": "ALL" }, "customerGets": { "value": { "percentage": 0.1 }, "items": { "collections": { "add": ["gid://shopify/Collection/512409665842"] } } } },
            "bxgy": { "title": "HAR597CLASS1777950382203 bxgy product", "code": "HAR597BXGY1777950382203", "startsAt": "2026-05-05T03:05:22.203Z", "context": { "all": "ALL" }, "customerBuys": { "value": { "quantity": "1" }, "items": { "products": { "productsToAdd": ["gid://shopify/Product/10177002832178"] } } }, "customerGets": { "value": { "discountOnQuantity": { "quantity": "1", "effect": { "percentage": 0.5 } } }, "items": { "products": { "productsToAdd": ["gid://shopify/Product/10177002799410"] } } } },
            "freeShipping": { "title": "HAR597CLASS1777950382203 free shipping", "code": "HAR597SHIP1777950382203", "startsAt": "2026-05-05T03:05:22.203Z", "context": { "all": "ALL" }, "destination": { "all": true } }
        }),
    ));

    assert_eq!(
        created.body["data"]["basicAll"]["codeDiscountNode"]["codeDiscount"],
        json!({ "__typename": "DiscountCodeBasic", "title": "HAR597CLASS1777950382203 basic order", "discountClasses": ["ORDER"] })
    );
    assert_eq!(
        created.body["data"]["basicProduct"]["codeDiscountNode"]["codeDiscount"]["discountClasses"],
        json!(["PRODUCT"])
    );
    assert_eq!(
        created.body["data"]["basicCollection"]["codeDiscountNode"]["codeDiscount"]
            ["discountClasses"],
        json!(["PRODUCT"])
    );
    assert_eq!(
        created.body["data"]["bxgy"]["codeDiscountNode"]["codeDiscount"],
        json!({ "__typename": "DiscountCodeBxgy", "title": "HAR597CLASS1777950382203 bxgy product", "discountClasses": ["PRODUCT"] })
    );
    assert_eq!(
        created.body["data"]["freeShipping"]["codeDiscountNode"]["codeDiscount"],
        json!({ "__typename": "DiscountCodeFreeShipping", "title": "HAR597CLASS1777950382203 free shipping", "discountClasses": ["SHIPPING"] })
    );
    assert_eq!(
        created.body["data"]["freeShipping"]["userErrors"],
        json!([])
    );

    let read = proxy.process_request(json_graphql_request(
        r#"query DiscountClassInferenceRead($productQuery: String!) { discountNodesCount(query: $productQuery) { count precision } }"#,
        json!({ "productQuery": "discount_class:product HAR597CLASS1777950382203" }),
    ));
    assert_eq!(
        read.body["data"]["discountNodesCount"],
        json!({ "count": 3, "precision": "EXACT" })
    );
}

#[test]
fn discount_code_basic_lifecycle_tracks_status_counts_and_delete_readback() {
    let mut proxy = snapshot_proxy();
    let create_query = r#"
        mutation DiscountCodeBasicLifecycleCreate($input: DiscountCodeBasicInput!) {
          discountCodeBasicCreate(basicCodeDiscount: $input) {
            codeDiscountNode { id codeDiscount { __typename ... on DiscountCodeBasic { title status asyncUsageCount discountClasses combinesWith { productDiscounts orderDiscounts shippingDiscounts } codes(first: 2) { nodes { code asyncUsageCount } } context { __typename ... on DiscountBuyerSelectionAll { all } } } } }
            userErrors { field message code extraInfo }
          }
        }
    "#;
    let create_input = json!({
        "title": "HAR-193 lifecycle 1777318334676",
        "code": "HAR193LIFE1777318334676",
        "startsAt": "2026-04-27T19:31:14.676Z",
        "combinesWith": { "productDiscounts": false, "orderDiscounts": true, "shippingDiscounts": false },
        "context": { "all": "ALL" },
        "minimumRequirement": { "subtotal": { "greaterThanOrEqualToSubtotal": "1.00" } },
        "customerGets": { "value": { "percentage": 0.1 }, "items": { "all": true } }
    });
    let created = proxy.process_request(json_graphql_request(
        create_query,
        json!({ "input": create_input }),
    ));
    assert_eq!(
        created.body["data"]["discountCodeBasicCreate"]["userErrors"],
        json!([])
    );
    let discount_id = json_string(
        &created.body["data"]["discountCodeBasicCreate"]["codeDiscountNode"]["id"],
        "code basic lifecycle discount id",
    );
    assert_synthetic_gid(&discount_id, "DiscountCodeNode");
    assert_eq!(
        created.body["data"]["discountCodeBasicCreate"]["codeDiscountNode"]["codeDiscount"]
            ["codes"]["nodes"][0]["code"],
        json!("HAR193LIFE1777318334676")
    );

    let update_query = r#"
        mutation DiscountCodeBasicLifecycleUpdate($id: ID!, $input: DiscountCodeBasicInput!) {
          discountCodeBasicUpdate(id: $id, basicCodeDiscount: $input) {
            codeDiscountNode { id codeDiscount { __typename ... on DiscountCodeBasic { title status asyncUsageCount discountClasses combinesWith { productDiscounts orderDiscounts shippingDiscounts } codes(first: 2) { nodes { code asyncUsageCount } } customerGets { items { __typename ... on AllDiscountItems { allItems } } } } } }
            userErrors { field message code extraInfo }
          }
        }
    "#;
    let update_input = json!({
        "title": "HAR-193 lifecycle updated 1777318334676",
        "code": "HAR193LIVE1777318334676",
        "startsAt": "2026-04-27T19:31:14.676Z",
        "combinesWith": { "productDiscounts": false, "orderDiscounts": true, "shippingDiscounts": false },
        "context": { "all": "ALL" },
        "minimumRequirement": { "subtotal": { "greaterThanOrEqualToSubtotal": "2.00" } },
        "customerGets": { "value": { "discountAmount": { "amount": "5.00", "appliesOnEachItem": false } }, "items": { "all": true } }
    });
    let updated = proxy.process_request(json_graphql_request(
        update_query,
        json!({ "id": discount_id.clone(), "input": update_input }),
    ));
    assert_eq!(
        updated.body["data"]["discountCodeBasicUpdate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        updated.body["data"]["discountCodeBasicUpdate"]["codeDiscountNode"]["codeDiscount"]
            ["title"],
        json!("HAR-193 lifecycle updated 1777318334676")
    );
    assert_eq!(
        updated.body["data"]["discountCodeBasicUpdate"]["codeDiscountNode"]["codeDiscount"]
            ["codes"]["nodes"][0]["code"],
        json!("HAR193LIVE1777318334676")
    );

    let read_query = r#"
        query DiscountCodeBasicLifecycleRead($id: ID!, $code: String!) {
          discountNode(id: $id) { id discount { __typename ... on DiscountCodeBasic { title status } } }
          codeDiscountNodeByCode(code: $code) { id }
          discountNodes(first: 5, query: "status:active") { nodes { id } }
          discountNodesCount(query: "status:active") { count precision }
        }
    "#;
    let read_active = proxy.process_request(json_graphql_request(
        read_query,
        json!({ "id": discount_id.clone(), "code": "HAR193LIVE1777318334676" }),
    ));
    assert_eq!(
        read_active.body["data"]["discountNode"]["discount"]["status"],
        json!("ACTIVE")
    );
    assert_eq!(
        read_active.body["data"]["discountNodesCount"],
        json!({ "count": 1, "precision": "EXACT" })
    );

    let deactivate_query = r#"
        mutation DiscountCodeBasicLifecycleDeactivate($id: ID!) {
          discountCodeDeactivate(id: $id) { codeDiscountNode { id codeDiscount { __typename ... on DiscountCodeBasic { title status } } } userErrors { field message code extraInfo } }
        }
    "#;
    let deactivated = proxy.process_request(json_graphql_request(
        deactivate_query,
        json!({ "id": discount_id.clone() }),
    ));
    assert_eq!(
        deactivated.body["data"]["discountCodeDeactivate"]["codeDiscountNode"]["codeDiscount"]
            ["status"],
        json!("EXPIRED")
    );
    let read_expired = proxy.process_request(json_graphql_request(
        read_query,
        json!({ "id": discount_id.clone(), "code": "HAR193LIVE1777318334676" }),
    ));
    assert_eq!(
        read_expired.body["data"]["discountNode"]["discount"]["status"],
        json!("EXPIRED")
    );
    assert_eq!(
        read_expired.body["data"]["discountNodes"]["nodes"],
        json!([])
    );
    assert_eq!(
        read_expired.body["data"]["discountNodesCount"],
        json!({ "count": 0, "precision": "EXACT" })
    );

    let activate_query = r#"
        mutation DiscountCodeBasicLifecycleActivate($id: ID!) {
          discountCodeActivate(id: $id) { codeDiscountNode { id codeDiscount { __typename ... on DiscountCodeBasic { title status } } } userErrors { field message code extraInfo } }
        }
    "#;
    let activated = proxy.process_request(json_graphql_request(
        activate_query,
        json!({ "id": discount_id.clone() }),
    ));
    assert_eq!(
        activated.body["data"]["discountCodeActivate"]["codeDiscountNode"]["codeDiscount"]
            ["status"],
        json!("ACTIVE")
    );

    let delete_query = r#"
        mutation DiscountCodeBasicLifecycleDelete($id: ID!) {
          discountCodeDelete(id: $id) { deletedCodeDiscountId userErrors { field message code extraInfo } }
        }
    "#;
    let deleted = proxy.process_request(json_graphql_request(
        delete_query,
        json!({ "id": discount_id.clone() }),
    ));
    assert_eq!(
        deleted.body["data"]["discountCodeDelete"]["userErrors"],
        json!([])
    );
    let read_deleted = proxy.process_request(json_graphql_request(
        read_query,
        json!({ "id": discount_id, "code": "HAR193LIVE1777318334676" }),
    ));
    assert_eq!(read_deleted.body["data"]["discountNode"], json!(null));
    assert_eq!(
        read_deleted.body["data"]["codeDiscountNodeByCode"],
        json!(null)
    );
    assert_eq!(
        read_deleted.body["data"]["discountNodesCount"],
        json!({ "count": 0, "precision": "EXACT" })
    );
}

#[test]
fn discount_code_basic_buyer_context_lifecycle_stages_segment_readback() {
    let mut proxy = snapshot_proxy();

    let create_query = r#"
        mutation DiscountCodeBasicBuyerContextCreate($input: DiscountCodeBasicInput!) {
          discountCodeBasicCreate(basicCodeDiscount: $input) {
            codeDiscountNode {
              id
              codeDiscount {
                __typename
                ... on DiscountCodeBasic {
                  title
                  status
                  codes(first: 1) { nodes { code asyncUsageCount } }
                  context {
                    __typename
                    ... on DiscountCustomers { customers { __typename id displayName } }
                    ... on DiscountCustomerSegments { segments { __typename id name } }
                  }
                }
              }
            }
            userErrors { field message code extraInfo }
          }
        }
    "#;
    let create_input = json!({
        "title": "HAR-390 code customer context 1777346878525",
        "code": "HAR390CTX1777346878525",
        "startsAt": "2023-01-01T00:00:00Z",
        "combinesWith": { "productDiscounts": false, "orderDiscounts": true, "shippingDiscounts": false },
        "context": { "customers": { "add": ["gid://shopify/Customer/10548596015410"] } },
        "customerGets": { "value": { "percentage": 0.1 }, "items": { "all": true } }
    });
    let created = proxy.process_request(json_graphql_request(
        create_query,
        json!({ "input": create_input }),
    ));
    let discount_id = json_string(
        &created.body["data"]["discountCodeBasicCreate"]["codeDiscountNode"]["id"],
        "code buyer context discount id",
    );
    assert_synthetic_gid(&discount_id, "DiscountCodeNode");
    assert_eq!(
        created.body["data"]["discountCodeBasicCreate"]["codeDiscountNode"]["codeDiscount"]
            ["context"],
        json!({
            "__typename": "DiscountCustomers",
            "customers": [{
                "__typename": "Customer",
                "id": "gid://shopify/Customer/10548596015410"
            }]
        })
    );

    let update_query = r#"
        mutation DiscountCodeBasicBuyerContextUpdate($id: ID!, $input: DiscountCodeBasicInput!) {
          discountCodeBasicUpdate(id: $id, basicCodeDiscount: $input) {
            codeDiscountNode { id codeDiscount { __typename ... on DiscountCodeBasic { title status codes(first: 1) { nodes { code asyncUsageCount } } context { __typename ... on DiscountCustomerSegments { segments { __typename id name } } } } } }
            userErrors { field message code extraInfo }
          }
        }
    "#;
    let update_input = json!({
        "title": "HAR-390 code segment context 1777346878525",
        "code": "HAR390SEG1777346878525",
        "startsAt": "2023-01-01T00:00:00Z",
        "combinesWith": { "productDiscounts": false, "orderDiscounts": true, "shippingDiscounts": false },
        "context": { "customerSegments": { "add": ["gid://shopify/Segment/647746715954"] } },
        "customerGets": { "value": { "discountAmount": { "amount": "5.00", "appliesOnEachItem": false } }, "items": { "all": true } }
    });
    let updated = proxy.process_request(json_graphql_request(
        update_query,
        json!({ "id": discount_id.clone(), "input": update_input }),
    ));
    assert_eq!(
        updated.body["data"]["discountCodeBasicUpdate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        updated.body["data"]["discountCodeBasicUpdate"]["codeDiscountNode"]["codeDiscount"]
            ["context"],
        json!({
            "__typename": "DiscountCustomerSegments",
            "segments": [{
                "__typename": "Segment",
                "id": "gid://shopify/Segment/647746715954"
            }]
        })
    );

    let read = proxy.process_request(json_graphql_request(r#"
        query DiscountCodeBasicBuyerContextRead($id: ID!, $code: String!) {
          discountNode(id: $id) { id discount { __typename ... on DiscountCodeBasic { title context { __typename ... on DiscountCustomerSegments { segments { __typename id name } } } } } }
          codeDiscountNodeByCode(code: $code) { codeDiscount { __typename ... on DiscountCodeBasic { title context { __typename ... on DiscountCustomerSegments { segments { __typename id name } } } } } }
        }
    "#, json!({ "id": discount_id.clone(), "code": "HAR390SEG1777346878525" })));
    assert_eq!(
        read.body["data"]["discountNode"]["discount"]["title"],
        json!("HAR-390 code segment context 1777346878525")
    );
    assert_eq!(
        read.body["data"]["codeDiscountNodeByCode"]["codeDiscount"]["context"]["segments"][0]["id"],
        json!("gid://shopify/Segment/647746715954")
    );

    let deleted = proxy.process_request(json_graphql_request(r#"
        mutation DiscountCodeBasicBuyerContextDelete($id: ID!) {
          discountCodeDelete(id: $id) { deletedCodeDiscountId userErrors { field message code extraInfo } }
        }
    "#, json!({ "id": discount_id })));
    assert_eq!(
        deleted.body["data"]["discountCodeDelete"]["userErrors"],
        json!([])
    );
}

#[test]
fn discount_basic_rejects_discount_on_quantity_for_non_bxgy_inputs() {
    let mut proxy = snapshot_proxy();

    let code_setup = proxy.process_request(json_graphql_request(
        r#"
        mutation DiscountBasicDisallowedQuantityCodeCreate($input: DiscountCodeBasicInput!) {
          discountCodeBasicCreate(basicCodeDiscount: $input) { codeDiscountNode { id } userErrors { field message code extraInfo } }
        }
        "#,
        json!({ "input": {
            "title": "Basic disallowed quantity code SETUP 1778038410003",
            "code": "BASICQTYSETUP1778038410003",
            "startsAt": "2026-04-25T00:00:00Z",
            "customerSelection": { "all": true },
            "customerGets": { "value": { "percentage": 0.1 }, "items": { "all": true } }
        }}),
    ));
    let code_discount_id = json_string(
        &code_setup.body["data"]["discountCodeBasicCreate"]["codeDiscountNode"]["id"],
        "basic quantity validation code discount id",
    );
    assert_synthetic_gid(&code_discount_id, "DiscountCodeNode");
    assert_eq!(
        code_setup.body["data"]["discountCodeBasicCreate"]["userErrors"],
        json!([])
    );

    let automatic_setup = proxy.process_request(json_graphql_request(
        r#"
        mutation DiscountBasicDisallowedQuantityAutomaticCreate($input: DiscountAutomaticBasicInput!) {
          discountAutomaticBasicCreate(automaticBasicDiscount: $input) { automaticDiscountNode { id } userErrors { field message code extraInfo } }
        }
        "#,
        json!({ "input": {
            "title": "Basic disallowed quantity automatic SETUP 1778038410003",
            "startsAt": "2026-04-25T00:00:00Z",
            "customerGets": { "value": { "percentage": 0.1 }, "items": { "all": true } }
        }}),
    ));
    let automatic_discount_id = json_string(
        &automatic_setup.body["data"]["discountAutomaticBasicCreate"]["automaticDiscountNode"]
            ["id"],
        "basic quantity validation automatic discount id",
    );
    assert_synthetic_gid(&automatic_discount_id, "DiscountAutomaticNode");
    assert_eq!(
        automatic_setup.body["data"]["discountAutomaticBasicCreate"]["userErrors"],
        json!([])
    );

    let invalid_value = json!({
        "title": "Basic disallowed quantity CREATE 1778038410003",
        "startsAt": "2026-04-25T00:00:00Z",
        "customerGets": {
            "value": { "discountOnQuantity": { "quantity": "2", "effect": { "percentage": 0.5 } } },
            "items": { "all": true }
        }
    });
    let mut invalid_code_value = invalid_value.clone();
    invalid_code_value["code"] = json!("BASICQTYCREATE1778038410003");

    let code_create = proxy.process_request(json_graphql_request(
        r#"
        mutation DiscountBasicDisallowedQuantityCodeCreate($input: DiscountCodeBasicInput!) {
          discountCodeBasicCreate(basicCodeDiscount: $input) { codeDiscountNode { id } userErrors { field message code extraInfo } }
        }
        "#,
        json!({ "input": invalid_code_value }),
    ));
    assert_eq!(
        code_create.body["data"]["discountCodeBasicCreate"]["codeDiscountNode"],
        json!(null)
    );
    assert_eq!(
        code_create.body["data"]["discountCodeBasicCreate"]["userErrors"][0]["field"],
        json!([
            "basicCodeDiscount",
            "customerGets",
            "value",
            "discountOnQuantity"
        ])
    );
    assert_eq!(
        code_create.body["data"]["discountCodeBasicCreate"]["userErrors"][0]["code"],
        json!("INVALID")
    );

    let automatic_update = proxy.process_request(json_graphql_request(
        r#"
        mutation DiscountBasicDisallowedQuantityAutomaticUpdate($id: ID!, $input: DiscountAutomaticBasicInput!) {
          discountAutomaticBasicUpdate(id: $id, automaticBasicDiscount: $input) { automaticDiscountNode { id } userErrors { field message code extraInfo } }
        }
        "#,
        json!({ "id": automatic_discount_id, "input": invalid_value }),
    ));
    assert_eq!(
        automatic_update.body["data"]["discountAutomaticBasicUpdate"]["automaticDiscountNode"],
        json!(null)
    );
    assert_eq!(
        automatic_update.body["data"]["discountAutomaticBasicUpdate"]["userErrors"][0]["field"],
        json!([
            "automaticBasicDiscount",
            "customerGets",
            "value",
            "discountOnQuantity"
        ])
    );
    assert_eq!(
        automatic_update.body["data"]["discountAutomaticBasicUpdate"]["userErrors"][0]["message"],
        json!("discountOnQuantity field is only permitted with bxgy discounts.")
    );
}

#[test]
fn discount_bxgy_numeric_validation_handles_bounds_and_variable_coercion() {
    let mut proxy = snapshot_proxy();

    let code_query = r#"
        mutation DiscountBxgyNumericValidationCodeCreate($input: DiscountCodeBxgyInput!) {
          discountCodeBxgyCreate(bxgyCodeDiscount: $input) { codeDiscountNode { id } userErrors { field message code extraInfo } }
        }
    "#;
    let automatic_query = r#"
        mutation DiscountBxgyNumericValidationAutomaticUpdate($id: ID!, $input: DiscountAutomaticBxgyInput!) {
          discountAutomaticBxgyUpdate(id: $id, automaticBxgyDiscount: $input) { automaticDiscountNode { id } userErrors { field message code extraInfo } }
        }
    "#;
    let automatic_create_query = r#"
        mutation DiscountBxgyNumericValidationAutomaticCreate($input: DiscountAutomaticBxgyInput!) {
          discountAutomaticBxgyCreate(automaticBxgyDiscount: $input) { automaticDiscountNode { id } userErrors { field message code extraInfo } }
        }
    "#;

    let mut base = json!({
        "title": "Conformance BXGY code SETUP 1778195290726",
        "code": "BXGYNSETUP1778195290726",
        "startsAt": "2026-04-25T00:00:00Z",
        "combinesWith": { "productDiscounts": true, "orderDiscounts": false, "shippingDiscounts": false },
        "context": { "all": "ALL" },
        "customerBuys": { "value": { "quantity": "1" }, "items": { "products": { "productsToAdd": ["gid://shopify/Product/10180236017970"] } } },
        "customerGets": { "value": { "discountOnQuantity": { "quantity": "1", "effect": { "percentage": 0.5 } } }, "items": { "products": { "productsToAdd": ["gid://shopify/Product/10180236017970"] } } }
    });

    let setup = proxy.process_request(json_graphql_request(
        code_query,
        json!({ "input": base.clone() }),
    ));
    assert_eq!(
        setup.body["data"]["discountCodeBxgyCreate"]["userErrors"],
        json!([])
    );
    let code_discount_id = json_string(
        &setup.body["data"]["discountCodeBxgyCreate"]["codeDiscountNode"]["id"],
        "bxgy numeric code discount id",
    );
    assert_synthetic_gid(&code_discount_id, "DiscountCodeNode");
    let mut automatic_base = base.clone();
    automatic_base.as_object_mut().unwrap().remove("code");
    let automatic_setup = proxy.process_request(json_graphql_request(
        automatic_create_query,
        json!({ "input": automatic_base.clone() }),
    ));
    let automatic_discount_id = json_string(
        &automatic_setup.body["data"]["discountAutomaticBxgyCreate"]["automaticDiscountNode"]["id"],
        "bxgy numeric automatic discount id",
    );
    assert_synthetic_gid(&automatic_discount_id, "DiscountAutomaticNode");
    assert_eq!(
        automatic_setup.body["data"]["discountAutomaticBxgyCreate"]["userErrors"],
        json!([])
    );

    base["usesPerOrderLimit"] = json!(0);
    let uses_zero = proxy.process_request(json_graphql_request(
        code_query,
        json!({ "input": base.clone() }),
    ));
    assert_eq!(
        uses_zero.body["data"]["discountCodeBxgyCreate"]["codeDiscountNode"],
        json!(null)
    );
    assert_eq!(
        uses_zero.body["data"]["discountCodeBxgyCreate"]["userErrors"][0],
        json!({
            "field": ["bxgyCodeDiscount", "usesPerOrderLimit"],
            "message": "Allocation limit cannot be zero",
            "code": "VALUE_OUTSIDE_RANGE",
            "extraInfo": null
        })
    );

    base["usesPerOrderLimit"] = json!("1.5");
    let uses_float = proxy.process_request(json_graphql_request(
        code_query,
        json!({ "input": base.clone() }),
    ));
    assert_eq!(
        uses_float.body["errors"][0]["extensions"]["code"],
        json!("INVALID_VARIABLE")
    );
    assert_eq!(
        uses_float.body["errors"][0]["extensions"]["problems"][0]["path"],
        json!(["usesPerOrderLimit"])
    );

    base.as_object_mut().unwrap().remove("usesPerOrderLimit");
    base["customerBuys"]["value"]["quantity"] = json!("100000");
    let buy_too_large = proxy.process_request(json_graphql_request(
        code_query,
        json!({ "input": base.clone() }),
    ));
    assert_eq!(
        buy_too_large.body["data"]["discountCodeBxgyCreate"]["userErrors"][0]["message"],
        json!("Prerequisite to entitlement quantity ratio antecedent must be less than 100000")
    );

    base["customerBuys"]["value"]["quantity"] = json!("1");
    automatic_base["customerGets"]["value"]["discountOnQuantity"]["quantity"] = json!("0");
    let get_zero = proxy.process_request(json_graphql_request(
        automatic_query,
        json!({ "id": automatic_discount_id.clone(), "input": automatic_base.clone() }),
    ));
    assert_eq!(
        get_zero.body["data"]["discountAutomaticBxgyUpdate"]["userErrors"][0]["field"],
        json!([
            "automaticBxgyDiscount",
            "customerGets",
            "value",
            "discountOnQuantity",
            "quantity"
        ])
    );

    automatic_base["customerGets"]["value"]["discountOnQuantity"]["quantity"] = json!("2");
    let ratio_ok = proxy.process_request(json_graphql_request(
        automatic_query,
        json!({ "id": automatic_discount_id.clone(), "input": automatic_base }),
    ));
    assert_eq!(
        ratio_ok.body["data"]["discountAutomaticBxgyUpdate"]["automaticDiscountNode"]["id"],
        json!(automatic_discount_id)
    );
    assert_eq!(
        ratio_ok.body["data"]["discountAutomaticBxgyUpdate"]["userErrors"],
        json!([])
    );
}

#[test]
fn discount_bxgy_lifecycle_stages_code_and_automatic_readback() {
    let mut proxy = snapshot_proxy();

    let create_query = r#"
        mutation DiscountBxgyLifecycleCreate($codeInput: DiscountCodeBxgyInput!, $automaticInput: DiscountAutomaticBxgyInput!) {
          discountCodeBxgyCreate(bxgyCodeDiscount: $codeInput) {
            codeDiscountNode {
              id
              codeDiscount {
                __typename
                ... on DiscountCodeBxgy {
                  title status summary usesPerOrderLimit
                  codes(first: 2) { nodes { code asyncUsageCount } }
                  customerBuys { value { __typename ... on DiscountQuantity { quantity } } items { __typename ... on DiscountProducts { products(first: 5) { nodes { id } } productVariants(first: 5) { nodes { id } } } } }
                  customerGets { value { __typename ... on DiscountOnQuantity { quantity { quantity } effect { __typename ... on DiscountPercentage { percentage } } } } items { __typename ... on DiscountCollections { collections(first: 5) { nodes { id } } } } appliesOnOneTimePurchase appliesOnSubscription }
                }
              }
            }
            userErrors { field message code extraInfo }
          }
          discountAutomaticBxgyCreate(automaticBxgyDiscount: $automaticInput) {
            automaticDiscountNode {
              id
              automaticDiscount {
                __typename
                ... on DiscountAutomaticBxgy {
                  title status summary usesPerOrderLimit
                  customerBuys { value { __typename ... on DiscountQuantity { quantity } } items { __typename ... on DiscountCollections { collections(first: 5) { nodes { id } } } } }
                  customerGets { value { __typename ... on DiscountOnQuantity { quantity { quantity } effect { __typename ... on DiscountPercentage { percentage } } } } items { __typename ... on DiscountProducts { products(first: 5) { nodes { id } } productVariants(first: 5) { nodes { id } } } } appliesOnOneTimePurchase appliesOnSubscription }
                }
              }
            }
            userErrors { field message code extraInfo }
          }
        }
    "#;
    let code_input = json!({
        "title": "HAR-195 code BXGY 1777150259502",
        "code": "HAR195BXGY1777150259502",
        "startsAt": "2026-04-25T00:00:00Z",
        "combinesWith": { "productDiscounts": true, "orderDiscounts": false, "shippingDiscounts": false },
        "context": { "all": "ALL" },
        "customerBuys": { "value": { "quantity": "2" }, "items": { "products": { "productsToAdd": ["gid://shopify/Product/10170555597106"], "productVariantsToAdd": ["gid://shopify/ProductVariant/51098643235122"] } } },
        "customerGets": { "value": { "discountOnQuantity": { "quantity": "1", "effect": { "percentage": 1 } } }, "items": { "collections": { "add": ["gid://shopify/Collection/512147128626"] } } },
        "usesPerOrderLimit": 1
    });
    let automatic_input = json!({
        "title": "HAR-195 automatic BXGY 1777150259502",
        "startsAt": "2026-04-25T00:00:00Z",
        "combinesWith": { "productDiscounts": true, "orderDiscounts": false, "shippingDiscounts": false },
        "context": { "all": "ALL" },
        "customerBuys": { "value": { "quantity": "1" }, "items": { "collections": { "add": ["gid://shopify/Collection/512147128626"] } } },
        "customerGets": { "value": { "discountOnQuantity": { "quantity": "1", "effect": { "percentage": 0.5 } } }, "items": { "products": { "productsToAdd": ["gid://shopify/Product/10170555629874"] } } },
        "usesPerOrderLimit": "1"
    });

    let created = proxy.process_request(json_graphql_request(
        create_query,
        json!({ "codeInput": code_input, "automaticInput": automatic_input }),
    ));
    assert_eq!(
        created.body["data"]["discountCodeBxgyCreate"]["userErrors"],
        json!([])
    );
    let code_id = json_string(
        &created.body["data"]["discountCodeBxgyCreate"]["codeDiscountNode"]["id"],
        "bxgy lifecycle code discount id",
    );
    assert_synthetic_gid(&code_id, "DiscountCodeNode");
    assert_eq!(
        created.body["data"]["discountCodeBxgyCreate"]["codeDiscountNode"]["codeDiscount"]
            ["summary"],
        json!("Buy 2 items, get 1 item free")
    );
    assert_eq!(
        created.body["data"]["discountCodeBxgyCreate"]["codeDiscountNode"]["codeDiscount"]
            ["customerBuys"]["items"]["products"]["nodes"][0]["id"],
        json!("gid://shopify/Product/10170555597106")
    );
    let automatic_id = json_string(
        &created.body["data"]["discountAutomaticBxgyCreate"]["automaticDiscountNode"]["id"],
        "bxgy lifecycle automatic discount id",
    );
    assert_synthetic_gid(&automatic_id, "DiscountAutomaticNode");
    assert_eq!(
        created.body["data"]["discountAutomaticBxgyCreate"]["automaticDiscountNode"]
            ["automaticDiscount"]["summary"],
        json!("Buy 1 item, get 1 item at 50% off")
    );
    assert_eq!(
        created.body["data"]["discountAutomaticBxgyCreate"]["automaticDiscountNode"]
            ["automaticDiscount"]["customerGets"]["items"]["products"]["nodes"][0]["id"],
        json!("gid://shopify/Product/10170555629874")
    );

    let code_update_query = r#"
        mutation DiscountCodeBxgyLifecycleUpdate($id: ID!, $input: DiscountCodeBxgyInput!) {
          discountCodeBxgyUpdate(id: $id, bxgyCodeDiscount: $input) { codeDiscountNode { id codeDiscount { __typename ... on DiscountCodeBxgy { title status summary customerGets { value { __typename ... on DiscountOnQuantity { quantity { quantity } effect { __typename ... on DiscountPercentage { percentage } } } } } } } } userErrors { field message code extraInfo } }
        }
    "#;
    let code_update_input = json!({
        "title": "HAR-195 code BXGY updated 1777150259502",
        "code": "HAR195BXGYUP1777150259502",
        "startsAt": "2026-04-25T00:00:00Z",
        "combinesWith": { "productDiscounts": true, "orderDiscounts": false, "shippingDiscounts": false },
        "context": { "all": "ALL" },
        "customerBuys": { "value": { "quantity": "2" }, "items": { "products": { "productsToAdd": ["gid://shopify/Product/10170555597106"], "productVariantsToAdd": ["gid://shopify/ProductVariant/51098643235122"] } } },
        "customerGets": { "value": { "discountOnQuantity": { "quantity": "2", "effect": { "percentage": 0.5 } } }, "items": { "collections": { "add": ["gid://shopify/Collection/512147128626"] } } },
        "usesPerOrderLimit": 1
    });
    let updated_code = proxy.process_request(json_graphql_request(
        code_update_query,
        json!({ "id": code_id.clone(), "input": code_update_input.clone() }),
    ));
    assert_eq!(
        updated_code.body["data"]["discountCodeBxgyUpdate"]["codeDiscountNode"]["codeDiscount"]
            ["title"],
        json!("HAR-195 code BXGY updated 1777150259502")
    );
    assert_eq!(
        updated_code.body["data"]["discountCodeBxgyUpdate"]["codeDiscountNode"]["codeDiscount"]
            ["summary"],
        json!("Buy 2 items, get 2 items at 50% off")
    );

    let status_query = r#"
        mutation DiscountCodeBxgyLifecycleStatus($id: ID!) {
          discountCodeDeactivate(id: $id) { codeDiscountNode { id codeDiscount { __typename ... on DiscountCodeBxgy { status endsAt } } } userErrors { field message code extraInfo } }
        }
    "#;
    let deactivated = proxy.process_request(json_graphql_request(
        status_query,
        json!({ "id": code_id.clone() }),
    ));
    assert_eq!(
        deactivated.body["data"]["discountCodeDeactivate"]["codeDiscountNode"]["codeDiscount"]
            ["status"],
        json!("EXPIRED")
    );

    let read = proxy.process_request(json_graphql_request(r#"
        query DiscountBxgyLifecycleRead($codeId: ID!, $automaticId: ID!, $code: String!) {
          discountNode(id: $codeId) { id discount { __typename ... on DiscountCodeBxgy { title status } } }
          codeDiscountNodeByCode(code: $code) { id }
          automaticDiscountNode(id: $automaticId) { id automaticDiscount { __typename ... on DiscountAutomaticBxgy { title status } } }
        }
    "#, json!({ "codeId": code_id.clone(), "automaticId": automatic_id.clone(), "code": "HAR195BXGYUP1777150259502" })));
    assert_eq!(
        read.body["data"]["discountNode"]["discount"]["title"],
        json!("HAR-195 code BXGY updated 1777150259502")
    );
    assert_eq!(
        read.body["data"]["codeDiscountNodeByCode"]["id"],
        json!(code_id)
    );
    assert_eq!(
        read.body["data"]["automaticDiscountNode"]["automaticDiscount"]["title"],
        json!("HAR-195 automatic BXGY 1777150259502")
    );

    let delete_query = r#"
        mutation DiscountBxgyLifecycleDelete($codeId: ID!, $automaticId: ID!) {
          discountCodeDelete(id: $codeId) { deletedCodeDiscountId userErrors { field message code extraInfo } }
          discountAutomaticDelete(id: $automaticId) { deletedAutomaticDiscountId userErrors { field message code extraInfo } }
        }
    "#;
    let deleted = proxy.process_request(json_graphql_request(
        delete_query,
        json!({ "codeId": code_id, "automaticId": automatic_id }),
    ));
    assert_eq!(
        deleted.body["data"]["discountCodeDelete"]["userErrors"],
        json!([])
    );
    assert_eq!(
        deleted.body["data"]["discountAutomaticDelete"]["userErrors"],
        json!([])
    );
}

fn fallback_product_title_digest() -> String {
    localization_content_digest("The Inventory Not Tracked Snowboard")
}

fn localization_content_digest(value: &str) -> String {
    use sha2::{Digest, Sha256};

    format!("{:x}", Sha256::digest(value.as_bytes()))
}

fn fallback_product_handle_digest() -> String {
    localization_content_digest("the-inventory-not-tracked-snowboard")
}

use super::common::*;
use pretty_assertions::assert_eq;

#[test]
fn discount_activate_deactivate_noops_preserve_captured_timestamp_shapes() {
    let mut proxy = snapshot_proxy();

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
        json!({ "id": "gid://shopify/DiscountCodeNode/1640637301042" }),
    ));
    assert_eq!(
        code_activate.body["data"]["discountCodeActivate"],
        json!({
            "codeDiscountNode": {
                "id": "gid://shopify/DiscountCodeNode/1640637301042",
                "codeDiscount": {
                    "__typename": "DiscountCodeBasic",
                    "startsAt": "2026-05-06T23:06:09Z",
                    "endsAt": null,
                    "status": "ACTIVE",
                    "updatedAt": "2026-05-06T23:08:09Z"
                }
            },
            "userErrors": []
        })
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
        json!({ "id": "gid://shopify/DiscountCodeNode/1640637333810" }),
    ));
    assert_eq!(
        code_deactivate.body["data"]["discountCodeDeactivate"],
        json!({
            "codeDiscountNode": {
                "id": "gid://shopify/DiscountCodeNode/1640637333810",
                "codeDiscount": {
                    "__typename": "DiscountCodeBasic",
                    "startsAt": "2026-05-06T23:06:09Z",
                    "endsAt": "2026-05-06T23:08:10Z",
                    "status": "EXPIRED",
                    "updatedAt": "2026-05-06T23:08:10Z"
                }
            },
            "userErrors": []
        })
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
        json!({ "id": "gid://shopify/DiscountAutomaticNode/1640637366578" }),
    ));
    assert_eq!(
        automatic_activate.body["data"]["discountAutomaticActivate"],
        json!({
            "automaticDiscountNode": {
                "id": "gid://shopify/DiscountAutomaticNode/1640637366578",
                "automaticDiscount": {
                    "__typename": "DiscountAutomaticBasic",
                    "startsAt": "2026-05-06T23:06:09Z",
                    "endsAt": null,
                    "status": "ACTIVE",
                    "updatedAt": "2026-05-06T23:08:09Z"
                }
            },
            "userErrors": []
        })
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
        json!({ "id": "gid://shopify/DiscountAutomaticNode/1640637432114" }),
    ));
    assert_eq!(
        automatic_deactivate.body["data"]["discountAutomaticDeactivate"],
        json!({
            "automaticDiscountNode": {
                "id": "gid://shopify/DiscountAutomaticNode/1640637432114",
                "automaticDiscount": {
                    "__typename": "DiscountAutomaticBasic",
                    "startsAt": "2026-05-06T23:06:09Z",
                    "endsAt": "2026-05-06T23:08:10Z",
                    "status": "EXPIRED",
                    "updatedAt": "2026-05-06T23:08:10Z"
                }
            },
            "userErrors": []
        })
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
        json!({ "input": { "title": "HAR-390 automatic customer context 1777346878525", "context": { "customers": { "add": ["gid://shopify/Customer/10548596015410"] } } } }),
    ));
    let discount_id = "gid://shopify/DiscountAutomaticNode/1638894666034";
    assert_eq!(
        create.body["data"]["discountAutomaticBasicCreate"]["automaticDiscountNode"]["id"],
        json!(discount_id)
    );
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
                    "id": "gid://shopify/Customer/10548596015410",
                    "displayName": "HAR390 Buyer Context"
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
        json!({ "id": discount_id, "input": { "title": "HAR-390 automatic segment context 1777346878525", "context": { "customerSegments": { "add": ["gid://shopify/Segment/647746715954"] } } } }),
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
                "id": "gid://shopify/Segment/647746715954",
                "name": "HAR-390 buyer context 1777346878525"
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
        json!({ "id": discount_id }),
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
                    "id": "gid://shopify/Segment/647746715954",
                    "name": "HAR-390 buyer context 1777346878525"
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
        json!({ "id": discount_id }),
    ));
    assert_eq!(
        delete.body["data"]["discountAutomaticDelete"],
        json!({ "deletedAutomaticDiscountId": discount_id, "userErrors": [] })
    );
}

#[test]
fn discount_automatic_nodes_read_returns_captured_catalog_connection_shape() {
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
        json!([
            {
                "id": "gid://shopify/DiscountAutomaticNode/1547497439538",
                "automaticDiscount": {
                    "__typename": "DiscountAutomaticBxgy",
                    "title": "Buy one, get the second 10 percent off",
                    "status": "EXPIRED",
                    "summary": "Buy 1 item, get 1 item at 10% off",
                    "startsAt": "2025-04-10T00:00:00Z",
                    "endsAt": "2025-04-25T00:00:00Z",
                    "createdAt": "2025-03-26T19:51:38Z",
                    "updatedAt": "2025-03-26T19:51:38Z",
                    "asyncUsageCount": 0,
                    "discountClasses": ["PRODUCT"],
                    "combinesWith": { "productDiscounts": false, "orderDiscounts": false, "shippingDiscounts": false }
                }
            },
            {
                "id": "gid://shopify/DiscountAutomaticNode/1547497472306",
                "automaticDiscount": {
                    "__typename": "DiscountAutomaticBasic",
                    "title": "Buy three, get 30 percent off",
                    "status": "EXPIRED",
                    "summary": "30% off The Complete Snowboard (Ice) • Minimum quantity of 3",
                    "startsAt": "2025-03-26T00:00:00Z",
                    "endsAt": "2025-04-05T00:00:00Z",
                    "createdAt": "2025-03-26T19:51:38Z",
                    "updatedAt": "2025-03-26T19:51:38Z",
                    "asyncUsageCount": 0,
                    "discountClasses": ["PRODUCT"],
                    "combinesWith": { "productDiscounts": true, "orderDiscounts": false, "shippingDiscounts": false }
                }
            }
        ])
    );
    assert_eq!(
        response.body["data"]["automaticDiscountNodes"]["pageInfo"],
        json!({
            "hasNextPage": false,
            "hasPreviousPage": false,
            "startCursor": "eyJsYXN0X2lkIjoxNTQ3NDk3NDM5NTM4LCJsYXN0X3ZhbHVlIjoxNTQ3NDk3NDM5NTM4fQ==",
            "endCursor": "eyJsYXN0X2lkIjoxNTQ3NDk3NDcyMzA2LCJsYXN0X3ZhbHVlIjoxNTQ3NDk3NDcyMzA2fQ=="
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
fn localization_locale_and_translation_lifecycle_stages_reads_and_clears_locale_translations() {
    let mut proxy = snapshot_proxy();

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
        json!({ "resourceId": "gid://shopify/Product/9801098789170", "translations": [{ "locale": "fr", "key": "title", "value": "Titre local", "translatableContentDigest": "digest" }] }),
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
fn localization_unknown_resource_and_market_scoped_translation_validation_match_shopify_shapes() {
    let mut proxy = snapshot_proxy();

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
        json!({ "resourceId": "gid://shopify/Product/9801098789170", "translations": [{ "locale": "fr", "key": "title", "value": "", "translatableContentDigest": "digest" }] }),
    ));
    assert_eq!(
        blank_translation.body["data"]["translationsRegister"]["userErrors"][0]["code"],
        json!("FAILS_RESOURCE_VALIDATION")
    );

    let normalized_handle = proxy.process_request(json_graphql_request(
        r#"mutation LocalizationTranslationsRegister($resourceId: ID!, $translations: [TranslationInput!]!) { translationsRegister(resourceId: $resourceId, translations: $translations) { translations { key value locale outdated market { id } } userErrors { field message code } } }"#,
        json!({ "resourceId": "gid://shopify/Product/9801098789170", "translations": [{ "locale": "fr", "key": "handle", "value": "Bad Value With Spaces", "translatableContentDigest": "digest" }] }),
    ));
    assert_eq!(
        normalized_handle.body["data"]["translationsRegister"]["translations"][0]["value"],
        json!("bad-value-with-spaces")
    );

    let unknown_market = proxy.process_request(json_graphql_request(
        r#"mutation LocalizationTranslationsRegister($resourceId: ID!, $translations: [TranslationInput!]!) { translationsRegister(resourceId: $resourceId, translations: $translations) { translations { key value locale outdated market { id } } userErrors { field message code } } }"#,
        json!({ "resourceId": "gid://shopify/Product/9801098789170", "translations": [{ "locale": "es", "key": "title", "value": "Titulo", "translatableContentDigest": "digest", "marketId": "gid://shopify/Market/999999" }] }),
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
        json!({ "resourceId": "gid://shopify/Product/9801098789170", "translations": [{ "locale": "es", "key": "title", "value": "Titulo", "translatableContentDigest": "digest", "marketId": "gid://shopify/Market/123" }] }),
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
fn localization_shop_locale_update_disable_tail_helpers_ported_from_gleam() {
    let mut proxy = snapshot_proxy();
    let known_presence = "gid://shopify/MarketWebPresence/known";
    let unknown_presence = "gid://shopify/MarketWebPresence/9999999999";

    let lifecycle = proxy.process_request(json_graphql_request(
        r#"mutation RustLocalizationShopLocaleTailHelpers($known: ID!, $unknown: ID!) {
          enableFr: shopLocaleEnable(locale: "fr") { shopLocale { locale published } userErrors { field message code } }
          publishFr: shopLocaleUpdate(locale: "fr", shopLocale: { published: true, marketWebPresenceIds: [$known, $unknown] }) { shopLocale { locale name published marketWebPresences { id __typename defaultLocale { locale } } } userErrors { field message code } }
          attachMissing: shopLocaleUpdate(locale: "tr", shopLocale: { marketWebPresenceIds: [$known] }) { shopLocale { locale name published marketWebPresences { id __typename defaultLocale { locale } } } userErrors { field message code } }
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
    let locale_codes = [
        "fr", "af", "ak", "sq", "am", "ar", "hy", "as", "az", "bm", "bn", "eu", "be", "bs", "br",
        "bg", "my", "ca", "ckb",
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
                "message": "Locale is not enabled for this shop",
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
                { "locale": "es", "key": "title", "value": "Titulo", "translatableContentDigest": "digest", "marketId": "gid://shopify/Market/123" },
                { "locale": "es", "key": "body_html", "value": "Cuerpo", "translatableContentDigest": "digest", "marketId": "gid://shopify/Market/123" },
                { "locale": "fr", "key": "title", "value": "Titre", "translatableContentDigest": "digest" }
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
            "customerId": "gid://shopify/Customer/1",
            "recipientId": "gid://shopify/Customer/2",
            "updateGiftCardId": "gid://shopify/GiftCard/trial-assignment"
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
fn gift_card_transaction_validation_rejects_state_currency_dates_and_allows_success_credit() {
    let mut proxy = snapshot_proxy();

    let response = proxy.process_request(json_graphql_request(
        r#"mutation GiftCardTransactionValidation($activeId: ID!, $expiredId: ID!, $deactivatedId: ID!, $validCreditInput: GiftCardCreditInput!, $mismatchCreditInput: GiftCardCreditInput!, $futureCreditInput: GiftCardCreditInput!, $preEpochCreditInput: GiftCardCreditInput!, $validDebitInput: GiftCardDebitInput!) {
          expiredCredit: giftCardCredit(id: $expiredId, creditInput: $validCreditInput) { giftCardCreditTransaction { id __typename processedAt amount { amount currencyCode } } userErrors { field code message } }
          deactivatedCredit: giftCardCredit(id: $deactivatedId, creditInput: $validCreditInput) { giftCardCreditTransaction { id __typename processedAt amount { amount currencyCode } } userErrors { field code message } }
          mismatchCredit: giftCardCredit(id: $activeId, creditInput: $mismatchCreditInput) { giftCardCreditTransaction { id __typename processedAt amount { amount currencyCode } } userErrors { field code message } }
          futureCredit: giftCardCredit(id: $activeId, creditInput: $futureCreditInput) { giftCardCreditTransaction { id __typename processedAt amount { amount currencyCode } } userErrors { field code message } }
          preEpochCredit: giftCardCredit(id: $activeId, creditInput: $preEpochCreditInput) { giftCardCreditTransaction { id __typename processedAt amount { amount currencyCode } } userErrors { field code message } }
          deactivatedDebit: giftCardDebit(id: $deactivatedId, debitInput: $validDebitInput) { giftCardDebitTransaction { id __typename processedAt amount { amount currencyCode } } userErrors { field code message } }
          successCredit: giftCardCredit(id: $activeId, creditInput: $validCreditInput) { giftCardCreditTransaction { id __typename processedAt amount { amount currencyCode } } userErrors { field code message } }
        }"#,
        json!({
            "activeId": "gid://shopify/GiftCard/654808252722",
            "expiredId": "gid://shopify/GiftCard/654808285490",
            "deactivatedId": "gid://shopify/GiftCard/654808318258",
            "validCreditInput": { "creditAmount": { "amount": "5.00", "currencyCode": "CAD" } },
            "mismatchCreditInput": { "creditAmount": { "amount": "5.00", "currencyCode": "EUR" } },
            "futureCreditInput": { "processedAt": "2099-01-01T00:00:00Z", "creditAmount": { "amount": "5.00", "currencyCode": "CAD" } },
            "preEpochCreditInput": { "processedAt": "1969-12-31T23:59:59Z", "creditAmount": { "amount": "5.00", "currencyCode": "CAD" } },
            "validDebitInput": { "debitAmount": { "amount": "5.00", "currencyCode": "CAD" } }
        }),
    ));

    assert_eq!(
        response.body["data"],
        json!({
            "expiredCredit": { "giftCardCreditTransaction": null, "userErrors": [{ "field": ["id"], "code": "INVALID", "message": "The gift card has expired." }] },
            "deactivatedCredit": { "giftCardCreditTransaction": null, "userErrors": [{ "field": ["id"], "code": "INVALID", "message": "The gift card is deactivated." }] },
            "mismatchCredit": { "giftCardCreditTransaction": null, "userErrors": [{ "field": ["creditInput", "creditAmount", "currencyCode"], "code": "MISMATCHING_CURRENCY", "message": "The currency provided does not match the currency of the gift card." }] },
            "futureCredit": { "giftCardCreditTransaction": null, "userErrors": [{ "field": ["creditInput", "processedAt"], "code": "INVALID", "message": "The processed date must not be in the future." }] },
            "preEpochCredit": { "giftCardCreditTransaction": null, "userErrors": [{ "field": ["creditInput", "processedAt"], "code": "INVALID", "message": "A valid processed date must be used." }] },
            "deactivatedDebit": { "giftCardDebitTransaction": null, "userErrors": [{ "field": ["id"], "code": "INVALID", "message": "The gift card is deactivated." }] },
            "successCredit": { "giftCardCreditTransaction": { "id": "gid://shopify/GiftCardCreditTransaction/246551773490", "__typename": "GiftCardCreditTransaction", "processedAt": "2026-05-05T06:50:35Z", "amount": { "amount": "5.0", "currencyCode": "CAD" } }, "userErrors": [] }
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
          $pastSendAt: DateTime!
        ) {
          createLongPreferredName: giftCardCreate(input: { initialValue: "10", recipientAttributes: { id: $recipientId, preferredName: $tooLongPreferredName } }) { giftCard { id recipientAttributes { preferredName } } giftCardCode userErrors { field code message } }
          createLongMessage: giftCardCreate(input: { initialValue: "10", recipientAttributes: { id: $recipientId, message: $tooLongMessage } }) { giftCard { id recipientAttributes { message } } giftCardCode userErrors { field code message } }
          createHtmlPreferredName: giftCardCreate(input: { initialValue: "10", recipientAttributes: { id: $recipientId, preferredName: $htmlPreferredName } }) { giftCard { id recipientAttributes { preferredName } } giftCardCode userErrors { field code message } }
          createHtmlMessage: giftCardCreate(input: { initialValue: "10", recipientAttributes: { id: $recipientId, message: $htmlMessage } }) { giftCard { id recipientAttributes { message } } giftCardCode userErrors { field code message } }
          createFutureSendAt: giftCardCreate(input: { initialValue: "10", recipientAttributes: { id: $recipientId, sendNotificationAt: $futureSendAt } }) { giftCard { id recipientAttributes { sendNotificationAt } } giftCardCode userErrors { field code message } }
          updateLongPreferredName: giftCardUpdate(id: $activeId, input: { recipientAttributes: { id: $recipientId, preferredName: $tooLongPreferredName } }) { giftCard { id recipientAttributes { preferredName } } userErrors { field code message } }
          updateLongMessage: giftCardUpdate(id: $activeId, input: { recipientAttributes: { id: $recipientId, message: $tooLongMessage } }) { giftCard { id recipientAttributes { message } } userErrors { field code message } }
          updateHtmlPreferredName: giftCardUpdate(id: $activeId, input: { recipientAttributes: { id: $recipientId, preferredName: $htmlPreferredName } }) { giftCard { id recipientAttributes { preferredName } } userErrors { field code message } }
          updateHtmlMessage: giftCardUpdate(id: $activeId, input: { recipientAttributes: { id: $recipientId, message: $htmlMessage } }) { giftCard { id recipientAttributes { message } } userErrors { field code message } }
          updatePastSendAt: giftCardUpdate(id: $activeId, input: { recipientAttributes: { id: $recipientId, sendNotificationAt: $pastSendAt } }) { giftCard { id recipientAttributes { sendNotificationAt } } userErrors { field code message } }
        }"#,
        json!({
            "activeId": "gid://shopify/GiftCard/1?shopify-draft-proxy=synthetic",
            "recipientId": "gid://shopify/Customer/10587888714034",
            "tooLongPreferredName": "x".repeat(256),
            "tooLongMessage": "x".repeat(201),
            "htmlPreferredName": "<b>Recipient</b>",
            "htmlMessage": "<script>alert(1)</script>",
            "futureSendAt": "2099-01-01T00:00:00Z",
            "pastSendAt": "1990-01-01T00:00:00Z"
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
            "updateLongPreferredName": { "giftCard": null, "userErrors": [{ "field": ["input", "recipientAttributes", "preferredName"], "code": "TOO_LONG", "message": "preferredName is too long (maximum is 255)" }] },
            "updateLongMessage": { "giftCard": null, "userErrors": [{ "field": ["input", "recipientAttributes", "message"], "code": "TOO_LONG", "message": "message is too long (maximum is 200)" }] },
            "updateHtmlPreferredName": { "giftCard": null, "userErrors": [{ "field": ["input", "recipientAttributes", "preferredName"], "code": "INVALID", "message": "Preferred name cannot contain HTML tags" }] },
            "updateHtmlMessage": { "giftCard": null, "userErrors": [{ "field": ["input", "recipientAttributes", "message"], "code": "INVALID", "message": "Message cannot contain HTML tags" }] },
            "updatePastSendAt": { "giftCard": null, "userErrors": [{ "field": ["input", "recipientAttributes", "sendNotificationAt"], "code": "INVALID", "message": "Send notification at must be within 90 days from now" }] }
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

    let response = proxy.process_request(json_graphql_request(
        r#"mutation GiftCardExpiryShopTimezone($creditId: ID!, $debitId: ID!, $customerNotificationId: ID!, $recipientNotificationId: ID!, $creditInput: GiftCardCreditInput!, $debitInput: GiftCardDebitInput!) {
          credit: giftCardCredit(id: $creditId, creditInput: $creditInput) { giftCardCreditTransaction { __typename } userErrors { field code message } }
          debit: giftCardDebit(id: $debitId, debitInput: $debitInput) { giftCardDebitTransaction { __typename } userErrors { field code message } }
          customerNotification: giftCardSendNotificationToCustomer(id: $customerNotificationId) { giftCard { id } userErrors { field code message } }
          recipientNotification: giftCardSendNotificationToRecipient(id: $recipientNotificationId) { giftCard { id } userErrors { field code message } }
        }"#,
        json!({
            "creditId": "gid://shopify/GiftCard/timezone-credit",
            "debitId": "gid://shopify/GiftCard/timezone-debit",
            "customerNotificationId": "gid://shopify/GiftCard/timezone-customer-notification",
            "recipientNotificationId": "gid://shopify/GiftCard/timezone-recipient-notification",
            "creditInput": { "creditAmount": { "amount": "5.00", "currencyCode": "CAD" } },
            "debitInput": { "debitAmount": { "amount": "2.00", "currencyCode": "CAD" } }
        }),
    ));

    assert_eq!(
        response.body["data"],
        json!({
            "credit": { "giftCardCreditTransaction": { "__typename": "GiftCardCreditTransaction" }, "userErrors": [] },
            "debit": { "giftCardDebitTransaction": { "__typename": "GiftCardDebitTransaction" }, "userErrors": [] },
            "customerNotification": { "giftCard": { "id": "gid://shopify/GiftCard/timezone-customer-notification" }, "userErrors": [] },
            "recipientNotification": { "giftCard": { "id": "gid://shopify/GiftCard/timezone-recipient-notification" }, "userErrors": [] }
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

    let base_error = json!([{ "field": ["base"], "code": null, "message": "Gift cards are not available on this plan." }]);
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
                    "message": "Gift card notifications are disabled."
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
fn discount_timestamps_monotonic_create_update_and_code_reads_preserve_synthetic_order() {
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
    assert!(first_created_at < second_created_at);
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
    assert!(second_created_at < updated_at);
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
    let add = r#"mutation DiscountRedeemCodeBulkLiveAdd($discountId: ID!, $codes: [DiscountRedeemCodeInput!]!) { discountRedeemCodeBulkAdd(discountId: $discountId, codes: $codes) { bulkCreation { done codesCount importedCount failedCount } userErrors { field message code extraInfo } } }"#;
    let add_response = proxy.process_request(json_graphql_request(
        add,
        json!({
            "discountId": "gid://shopify/DiscountCodeNode/1639018103090",
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

    let read = r#"query DiscountRedeemCodeBulkLiveRead($id: ID!, $exactAddedCode: String!, $lowerAddedCode: String!, $removedCode: String!) { codeDiscountNode(id: $id) { id codeDiscount { ... on DiscountCodeBasic { codesCount { count precision } } } } exactAdded: codeDiscountNodeByCode(code: $exactAddedCode) { id } lowerAdded: codeDiscountNodeByCode(code: $lowerAddedCode) { id } removed: codeDiscountNodeByCode(code: $removedCode) { id } }"#;
    let read_vars = json!({
        "id": "gid://shopify/DiscountCodeNode/1639018103090",
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
        json!("gid://shopify/DiscountCodeNode/1639018103090")
    );
    assert_eq!(
        after_add.body["data"]["lowerAdded"]["id"],
        json!("gid://shopify/DiscountCodeNode/1639018103090")
    );
    assert_eq!(
        after_add.body["data"]["removed"]["id"],
        json!("gid://shopify/DiscountCodeNode/1639018103090")
    );

    let delete = r#"mutation DiscountRedeemCodeBulkLiveDelete($discountId: ID!, $ids: [ID!]!) { discountCodeRedeemCodeBulkDelete(discountId: $discountId, ids: $ids) { job { done } userErrors { field message code extraInfo } } }"#;
    let delete_response = proxy.process_request(json_graphql_request(
        delete,
        json!({
            "discountId": "gid://shopify/DiscountCodeNode/1639018103090",
            "ids": ["gid://shopify/DiscountRedeemCode/21582085751090"]
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
        json!("gid://shopify/DiscountCodeNode/1639018103090")
    );
    assert_eq!(
        after_delete.body["data"]["lowerAdded"]["id"],
        json!("gid://shopify/DiscountCodeNode/1639018103090")
    );
    assert_eq!(after_delete.body["data"]["removed"], Value::Null);
}

#[test]
fn discount_redeem_code_bulk_delete_validation_matches_selector_errors_and_happy_job() {
    let mut proxy = snapshot_proxy();
    let validation = r#"mutation DiscountRedeemCodeBulkDeleteValidation($discountId: ID!, $unknownDiscountId: ID!, $ids: [ID!], $emptyIds: [ID!], $search: String, $blankSearch: String, $savedSearchId: ID!) { missing: discountCodeRedeemCodeBulkDelete(discountId: $discountId) { job { id done } userErrors { field message code extraInfo } } tooMany: discountCodeRedeemCodeBulkDelete(discountId: $discountId, ids: $ids, search: $search) { job { id done } userErrors { field message code extraInfo } } unknownDiscount: discountCodeRedeemCodeBulkDelete(discountId: $unknownDiscountId, ids: $ids) { job { id done } userErrors { field message code extraInfo } } emptyIds: discountCodeRedeemCodeBulkDelete(discountId: $discountId, ids: $emptyIds) { job { id done } userErrors { field message code extraInfo } } blankSearch: discountCodeRedeemCodeBulkDelete(discountId: $discountId, search: $blankSearch) { job { id done } userErrors { field message code extraInfo } } invalidSavedSearch: discountCodeRedeemCodeBulkDelete(discountId: $discountId, savedSearchId: $savedSearchId) { job { id done } userErrors { field message code extraInfo } } }"#;
    let variables = json!({
        "discountId": "gid://shopify/DiscountCodeNode/1640468283698",
        "unknownDiscountId": "gid://shopify/DiscountCodeNode/0",
        "ids": ["gid://shopify/DiscountRedeemCode/21667051995442"],
        "emptyIds": [],
        "search": "code:ANY",
        "blankSearch": "   ",
        "savedSearchId": "gid://shopify/SavedSearch/0"
    });
    let response = proxy.process_request(json_graphql_request(validation, variables));
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

    let happy = r#"mutation DiscountRedeemCodeBulkDeleteHappy($discountId: ID!, $ids: [ID!]!) { happy: discountCodeRedeemCodeBulkDelete(discountId: $discountId, ids: $ids) { job { id done } userErrors { field message code extraInfo } } }"#;
    let happy_response = proxy.process_request(json_graphql_request(
        happy,
        json!({
            "discountId": "gid://shopify/DiscountCodeNode/1640468283698",
            "ids": ["gid://shopify/DiscountRedeemCode/21667051995442"]
        }),
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
    assert_eq!(
        created.body["data"]["discountCodeBasicCreate"]["codeDiscountNode"]["id"],
        json!("gid://shopify/DiscountCodeNode/1640746221874")
    );
    assert_eq!(
        created.body["data"]["discountCodeBasicCreate"]["userErrors"],
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
    let too_many = proxy.process_request(json_graphql_request(add, json!({ "discountId": "gid://shopify/DiscountCodeNode/1640746221874", "codes": too_many_codes })));
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
        json!({ "discountId": "gid://shopify/DiscountCodeNode/1640746221874", "codes": [] }),
    ));
    assert_eq!(
        empty.body["data"]["discountRedeemCodeBulkAdd"]["userErrors"],
        json!([{ "field": ["codes"], "message": "Codes can't be blank", "code": "BLANK", "extraInfo": null }])
    );

    let invalid_codes = json!([{"code":""},{"code":"HAR784NL1778166762181\nBAD"},{"code":"HAR784CR1778166762181\rBAD"},{"code":"XXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX"},{"code":"HAR784DUP1778166762181"},{"code":"HAR784DUP1778166762181"},{"code":"HAR784OK1778166762181"}]);
    let invalid_add = proxy.process_request(json_graphql_request(add, json!({ "discountId": "gid://shopify/DiscountCodeNode/1640746221874", "codes": invalid_codes })));
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
    let read_after_invalid = proxy.process_request(json_graphql_request(read, json!({ "discountId": "gid://shopify/DiscountCodeNode/1640746221874", "duplicateCode": "HAR784DUP1778166762181", "validCode": "HAR784OK1778166762181" })));
    assert_eq!(
        read_after_invalid.body["data"]["codeDiscountNode"]["codeDiscount"]["codesCount"],
        json!({ "count": 3, "precision": "EXACT" })
    );
    assert_eq!(
        read_after_invalid.body["data"]["duplicate"]["id"],
        json!("gid://shopify/DiscountCodeNode/1640746221874")
    );
    assert_eq!(
        read_after_invalid.body["data"]["valid"]["id"],
        json!("gid://shopify/DiscountCodeNode/1640746221874")
    );

    let conflicts = json!([{"code":"HAR784BASE1778166762181"},{"code":"HAR784CROSS1778166762181"},{"code":"HAR784FRESH1778166762181"}]);
    let conflicts_add = proxy.process_request(json_graphql_request(
        add,
        json!({ "discountId": "gid://shopify/DiscountCodeNode/1640746221874", "codes": conflicts }),
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
    let read_after_conflicts = proxy.process_request(json_graphql_request(existing_read, json!({ "discountId": "gid://shopify/DiscountCodeNode/1640746221874", "sameDiscountCode": "HAR784BASE1778166762181", "crossDiscountCode": "HAR784CROSS1778166762181", "freshCode": "HAR784FRESH1778166762181" })));
    assert_eq!(
        read_after_conflicts.body["data"]["codeDiscountNode"]["codeDiscount"]["codesCount"],
        json!({ "count": 4, "precision": "EXACT" })
    );
    assert_eq!(
        read_after_conflicts.body["data"]["sameDiscount"]["id"],
        json!("gid://shopify/DiscountCodeNode/1640746221874")
    );
    assert_eq!(
        read_after_conflicts.body["data"]["crossDiscount"]["id"],
        json!("gid://shopify/DiscountCodeNode/1640746254642")
    );
    assert_eq!(
        read_after_conflicts.body["data"]["fresh"]["id"],
        json!("gid://shopify/DiscountCodeNode/1640746221874")
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
    assert_eq!(
        created.body["data"]["discountCodeBasicCreate"]["codeDiscountNode"]["id"],
        json!("gid://shopify/DiscountCodeNode/1640428962098")
    );

    let bulk_add = r#"mutation DiscountUpdateEdgeBulkAdd($discountId: ID!, $codes: [DiscountRedeemCodeInput!]!) { discountRedeemCodeBulkAdd(discountId: $discountId, codes: $codes) { bulkCreation { codesCount } userErrors { field message code extraInfo } } }"#;
    let bulk_added = proxy.process_request(json_graphql_request(bulk_add, json!({ "discountId": "gid://shopify/DiscountCodeNode/1640428962098", "codes": [{"code":"HAR605BULK1778002393771_1"},{"code":"HAR605BULK1778002393771_2"},{"code":"HAR605BULK1778002393771_3"},{"code":"HAR605BULK1778002393771_4"},{"code":"HAR605BULK1778002393771_5"}] })));
    assert_eq!(
        bulk_added.body["data"]["discountRedeemCodeBulkAdd"]["bulkCreation"]["codesCount"],
        json!(5)
    );
    assert_eq!(
        bulk_added.body["data"]["discountRedeemCodeBulkAdd"]["userErrors"],
        json!([])
    );

    let basic_update = r#"mutation DiscountUpdateEdgeBasicUpdate($id: ID!, $input: DiscountCodeBasicInput!) { discountCodeBasicUpdate(id: $id, basicCodeDiscount: $input) { codeDiscountNode { id codeDiscount { __typename } } userErrors { field message code extraInfo } } }"#;
    let code_change = proxy.process_request(json_graphql_request(basic_update, json!({ "id": "gid://shopify/DiscountCodeNode/1640428962098", "input": { "title": "HAR-605 bulk renamed 1778002393771", "code": "HAR605BULKNEW1778002393771", "startsAt": "2026-04-25T00:00:00Z", "context": { "all": "ALL" }, "customerGets": { "value": { "percentage": 0.2 }, "items": { "all": true } } } })));
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

    let bxgy_to_basic = proxy.process_request(json_graphql_request(basic_update, json!({ "id": "gid://shopify/DiscountCodeNode/1640428994866", "input": { "title": "HAR-605 coerced basic 1778002393771", "code": "HAR605BXGY1778002393771", "startsAt": "2026-04-25T00:00:00Z", "context": { "all": "ALL" }, "customerGets": { "value": { "percentage": 0.25 }, "items": { "all": true } } } })));
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
    assert_eq!(
        response.body["data"]["automaticFreeShippingSkip"]["automaticDiscountNode"]["id"],
        json!("gid://shopify/DiscountAutomaticNode/1?shopify-draft-proxy=synthetic")
    );
    assert_eq!(
        response.body["data"]["setupBasic"]["codeDiscountNode"]["id"],
        json!("gid://shopify/DiscountCodeNode/2?shopify-draft-proxy=synthetic")
    );
    assert_eq!(
        response.body["data"]["setupFreeShipping"]["codeDiscountNode"]["id"],
        json!("gid://shopify/DiscountCodeNode/4?shopify-draft-proxy=synthetic")
    );
    assert_eq!(
        response.body["data"]["setupAutomaticBasic"]["automaticDiscountNode"]["id"],
        json!("gid://shopify/DiscountAutomaticNode/6?shopify-draft-proxy=synthetic")
    );

    let basic_update = r#"mutation DiscountSubscriptionFieldsBasicUpdate($id: ID!) { basicUpdate: discountCodeBasicUpdate(id: $id, basicCodeDiscount: { title: "Setup basic", code: "SETUP-BASIC-SUB", startsAt: "2026-04-25T00:00:00Z", customerGets: { value: { percentage: 0.1 }, items: { all: true }, appliesOnSubscription: true } }) { codeDiscountNode { id } userErrors { field message code extraInfo } } }"#;
    let updated = proxy.process_request(json_graphql_request(
        basic_update,
        json!({ "id": "gid://shopify/DiscountCodeNode/2?shopify-draft-proxy=synthetic" }),
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
        json!({ "id": "gid://shopify/DiscountAutomaticNode/1?shopify-draft-proxy=synthetic" }),
    ));
    assert_eq!(
        automatic_free_shipping_updated.body["data"]["automaticFreeShippingUpdate"]
            ["automaticDiscountNode"]["id"],
        json!("gid://shopify/DiscountAutomaticNode/1?shopify-draft-proxy=synthetic")
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
            "scheduledId": "gid://shopify/DiscountCodeNode/1640295530802",
            "expiredId": "gid://shopify/DiscountCodeNode/1640295563570",
            "activeId": "gid://shopify/DiscountCodeNode/1640295596338",
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
        "codeInput": { "title": "HAR-196 code free shipping 1777150170404", "code": "HAR196FREE1777150170404", "startsAt": "2026-04-25T20:48:30.404Z", "combinesWith": { "productDiscounts": true, "orderDiscounts": false, "shippingDiscounts": false }, "context": { "all": "ALL" }, "minimumRequirement": { "subtotal": { "greaterThanOrEqualToSubtotal": "10.00" } }, "destination": { "all": true }, "maximumShippingPrice": "25.00", "appliesOncePerCustomer": true, "appliesOnOneTimePurchase": true, "appliesOnSubscription": false, "recurringCycleLimit": 1, "usageLimit": 5 },
        "automaticInput": { "title": "HAR-196 automatic free shipping 1777150170404", "startsAt": "2026-04-25T20:48:30.404Z", "endsAt": null, "combinesWith": { "productDiscounts": false, "orderDiscounts": true, "shippingDiscounts": false }, "context": { "all": "ALL" }, "minimumRequirement": { "subtotal": { "greaterThanOrEqualToSubtotal": "15.00" } }, "destination": { "all": true }, "maximumShippingPrice": "20.00", "appliesOnOneTimePurchase": true, "appliesOnSubscription": false, "recurringCycleLimit": 1 }
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

    let code_update = r#"mutation DiscountCodeFreeShippingLifecycleUpdate($id: ID!, $input: DiscountCodeFreeShippingInput!) { discountCodeFreeShippingUpdate(id: $id, freeShippingCodeDiscount: $input) { codeDiscountNode { id codeDiscount { __typename ... on DiscountCodeFreeShipping { title destinationSelection { __typename ... on DiscountCountries { countries includeRestOfWorld } } maximumShippingPrice { amount currencyCode } appliesOncePerCustomer appliesOnOneTimePurchase appliesOnSubscription recurringCycleLimit usageLimit } } } userErrors { field message code extraInfo } } }"#;
    let updated = proxy.process_request(json_graphql_request(
        code_update,
        json!({ "id": "gid://shopify/DiscountCodeNode/1638465372466", "input": {} }),
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
        json!({ "id": "gid://shopify/DiscountAutomaticNode/1638465405234", "input": {} }),
    ));
    assert_eq!(
        automatic_updated.body["data"]["discountAutomaticFreeShippingUpdate"]
            ["automaticDiscountNode"]["automaticDiscount"]["destinationSelection"],
        json!({ "__typename": "DiscountCountries", "countries": ["US"], "includeRestOfWorld": false })
    );

    let read_query = r#"query DiscountFreeShippingLifecycleRead($codeId: ID!, $automaticId: ID!, $code: String!) { discountNode(id: $codeId) { id discount { __typename ... on DiscountCodeFreeShipping { title status } } } codeDiscountNodeByCode(code: $code) { id } automaticDiscountNode(id: $automaticId) { id automaticDiscount { __typename ... on DiscountAutomaticFreeShipping { title status } } } }"#;
    let read_after_update = proxy.process_request(json_graphql_request(read_query, json!({ "codeId": "gid://shopify/DiscountCodeNode/1638465372466", "automaticId": "gid://shopify/DiscountAutomaticNode/1638465405234", "code": "HAR196SHIP1777150170404" })));
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
        json!({ "id": "gid://shopify/DiscountCodeNode/1638465372466" }),
    ));
    assert_eq!(
        code_deactivated.body["data"]["discountCodeDeactivate"]["codeDiscountNode"]["codeDiscount"]
            ["status"],
        json!("EXPIRED")
    );

    let automatic_delete = r#"mutation DiscountFreeShippingLifecycleDelete($id: ID!) { discountAutomaticDelete(id: $id) { deletedAutomaticDiscountId userErrors { field message code extraInfo } } }"#;
    let automatic_deleted = proxy.process_request(json_graphql_request(
        automatic_delete,
        json!({ "id": "gid://shopify/DiscountAutomaticNode/1638465405234" }),
    ));
    assert_eq!(
        automatic_deleted.body["data"]["discountAutomaticDelete"]["userErrors"],
        json!([])
    );

    let code_delete = r#"mutation DiscountFreeShippingLifecycleDelete($id: ID!) { discountCodeDelete(id: $id) { deletedCodeDiscountId userErrors { field message code extraInfo } } }"#;
    let _ = proxy.process_request(json_graphql_request(
        code_delete,
        json!({ "id": "gid://shopify/DiscountCodeNode/1638465372466" }),
    ));
    let read_after_delete = proxy.process_request(json_graphql_request(read_query, json!({ "codeId": "gid://shopify/DiscountCodeNode/1638465372466", "automaticId": "gid://shopify/DiscountAutomaticNode/1638465405234", "code": "HAR196SHIP1777150170404" })));
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
    assert_eq!(
        created.body["data"]["discountCodeBasicCreate"]["codeDiscountNode"]["id"],
        json!("gid://shopify/DiscountCodeNode/1638844039474")
    );
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
        json!({ "id": "gid://shopify/DiscountCodeNode/1638844039474", "input": update_input }),
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
    let read_active = proxy.process_request(json_graphql_request(read_query, json!({ "id": "gid://shopify/DiscountCodeNode/1638844039474", "code": "HAR193LIVE1777318334676" })));
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
        json!({ "id": "gid://shopify/DiscountCodeNode/1638844039474" }),
    ));
    assert_eq!(
        deactivated.body["data"]["discountCodeDeactivate"]["codeDiscountNode"]["codeDiscount"]
            ["status"],
        json!("EXPIRED")
    );
    let read_expired = proxy.process_request(json_graphql_request(read_query, json!({ "id": "gid://shopify/DiscountCodeNode/1638844039474", "code": "HAR193LIVE1777318334676" })));
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
        json!({ "id": "gid://shopify/DiscountCodeNode/1638844039474" }),
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
        json!({ "id": "gid://shopify/DiscountCodeNode/1638844039474" }),
    ));
    assert_eq!(
        deleted.body["data"]["discountCodeDelete"]["userErrors"],
        json!([])
    );
    let read_deleted = proxy.process_request(json_graphql_request(read_query, json!({ "id": "gid://shopify/DiscountCodeNode/1638844039474", "code": "HAR193LIVE1777318334676" })));
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
    assert_eq!(
        created.body["data"]["discountCodeBasicCreate"]["codeDiscountNode"]["id"],
        json!("gid://shopify/DiscountCodeNode/1638894633266")
    );
    assert_eq!(
        created.body["data"]["discountCodeBasicCreate"]["codeDiscountNode"]["codeDiscount"]
            ["context"],
        json!({
            "__typename": "DiscountCustomers",
            "customers": [{
                "__typename": "Customer",
                "id": "gid://shopify/Customer/10548596015410",
                "displayName": "HAR390 Buyer Context"
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
        json!({ "id": "gid://shopify/DiscountCodeNode/1638894633266", "input": update_input }),
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
                "id": "gid://shopify/Segment/647746715954",
                "name": "HAR-390 buyer context 1777346878525"
            }]
        })
    );

    let read = proxy.process_request(json_graphql_request(r#"
        query DiscountCodeBasicBuyerContextRead($id: ID!, $code: String!) {
          discountNode(id: $id) { id discount { __typename ... on DiscountCodeBasic { title context { __typename ... on DiscountCustomerSegments { segments { __typename id name } } } } } }
          codeDiscountNodeByCode(code: $code) { codeDiscount { __typename ... on DiscountCodeBasic { title context { __typename ... on DiscountCustomerSegments { segments { __typename id name } } } } } }
        }
    "#, json!({ "id": "gid://shopify/DiscountCodeNode/1638894633266", "code": "HAR390SEG1777346878525" })));
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
    "#, json!({ "id": "gid://shopify/DiscountCodeNode/1638894633266" })));
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
    assert_eq!(
        code_setup.body["data"]["discountCodeBasicCreate"]["codeDiscountNode"]["id"],
        json!("gid://shopify/DiscountCodeNode/1640501739826")
    );
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
    assert_eq!(
        automatic_setup.body["data"]["discountAutomaticBasicCreate"]["automaticDiscountNode"]["id"],
        json!("gid://shopify/DiscountAutomaticNode/1640501772594")
    );
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

    let code_create = proxy.process_request(json_graphql_request(
        r#"
        mutation DiscountBasicDisallowedQuantityCodeCreate($input: DiscountCodeBasicInput!) {
          discountCodeBasicCreate(basicCodeDiscount: $input) { codeDiscountNode { id } userErrors { field message code extraInfo } }
        }
        "#,
        json!({ "input": invalid_value.clone() }),
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
        json!({ "id": "gid://shopify/DiscountAutomaticNode/1640501772594", "input": invalid_value }),
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
    assert_eq!(
        setup.body["data"]["discountCodeBxgyCreate"]["codeDiscountNode"]["id"],
        json!("gid://shopify/DiscountCodeNode/1640810610994")
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
    base["customerGets"]["value"]["discountOnQuantity"]["quantity"] = json!("0");
    let get_zero = proxy.process_request(json_graphql_request(
        automatic_query,
        json!({ "id": "gid://shopify/DiscountAutomaticNode/1640810643762", "input": base.clone() }),
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

    base["customerGets"]["value"]["discountOnQuantity"]["quantity"] = json!("2");
    let ratio_ok = proxy.process_request(json_graphql_request(
        automatic_query,
        json!({ "id": "gid://shopify/DiscountAutomaticNode/1640810643762", "input": base }),
    ));
    assert_eq!(
        ratio_ok.body["data"]["discountAutomaticBxgyUpdate"]["automaticDiscountNode"]["id"],
        json!("gid://shopify/DiscountAutomaticNode/1640810643762")
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
    assert_eq!(
        created.body["data"]["discountCodeBxgyCreate"]["codeDiscountNode"]["id"],
        json!("gid://shopify/DiscountCodeNode/1638465831218")
    );
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
    assert_eq!(
        created.body["data"]["discountAutomaticBxgyCreate"]["automaticDiscountNode"]["id"],
        json!("gid://shopify/DiscountAutomaticNode/1638465863986")
    );
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
    let updated_code = proxy.process_request(json_graphql_request(code_update_query, json!({ "id": "gid://shopify/DiscountCodeNode/1638465831218", "input": code_update_input.clone() })));
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
        json!({ "id": "gid://shopify/DiscountCodeNode/1638465831218" }),
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
    "#, json!({ "codeId": "gid://shopify/DiscountCodeNode/1638465831218", "automaticId": "gid://shopify/DiscountAutomaticNode/1638465863986", "code": "HAR195BXGYUP1777150259502" })));
    assert_eq!(
        read.body["data"]["discountNode"]["discount"]["title"],
        json!("HAR-195 code BXGY updated 1777150259502")
    );
    assert_eq!(
        read.body["data"]["codeDiscountNodeByCode"]["id"],
        json!("gid://shopify/DiscountCodeNode/1638465831218")
    );
    assert_eq!(
        read.body["data"]["automaticDiscountNode"]["automaticDiscount"]["title"],
        json!("HAR-195 automatic BXGY updated 1777150259502")
    );

    let delete_query = r#"
        mutation DiscountBxgyLifecycleDelete($codeId: ID!, $automaticId: ID!) {
          discountCodeDelete(id: $codeId) { deletedCodeDiscountId userErrors { field message code extraInfo } }
          discountAutomaticDelete(id: $automaticId) { deletedAutomaticDiscountId userErrors { field message code extraInfo } }
        }
    "#;
    let deleted = proxy.process_request(json_graphql_request(delete_query, json!({ "codeId": "gid://shopify/DiscountCodeNode/1638465831218", "automaticId": "gid://shopify/DiscountAutomaticNode/1638465863986" })));
    assert_eq!(
        deleted.body["data"]["discountCodeDelete"]["userErrors"],
        json!([])
    );
    assert_eq!(
        deleted.body["data"]["discountAutomaticDelete"]["userErrors"],
        json!([])
    );
}

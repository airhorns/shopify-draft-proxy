use super::common::*;
use pretty_assertions::assert_eq;

#[test]
fn backup_region_update_handles_omitted_null_known_invalid_and_node_reads_locally() {
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
            "backupRegion": {
                "__typename": "MarketRegionCountry",
                "id": "gid://shopify/MarketRegionCountry/4062110417202",
                "name": "Canada",
                "code": "CA"
            },
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

    let update_ae = proxy.process_request(json_graphql_request(
        r#"
        mutation BackupRegionUpdateAe {
          backupRegionUpdate(region: { countryCode: AE }) {
            backupRegion { __typename id name ... on MarketRegionCountry { code } }
            userErrors { field message code }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        update_ae.body["data"]["backupRegionUpdate"],
        json!({
            "backupRegion": {
                "__typename": "MarketRegionCountry",
                "id": "gid://shopify/MarketRegionCountry/4062110482738",
                "name": "United Arab Emirates",
                "code": "AE"
            },
            "userErrors": []
        })
    );

    let read = proxy.process_request(json_graphql_request(
        r#"
        query BackupRegionRead {
          backupRegion { __typename id name ... on MarketRegionCountry { code } }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        read.body["data"]["backupRegion"],
        json!({
            "__typename": "MarketRegionCountry",
            "id": "gid://shopify/MarketRegionCountry/4062110482738",
            "name": "United Arab Emirates",
            "code": "AE"
        })
    );

    let node = proxy.process_request(json_graphql_request(
        r#"
        query BackupRegionNode($ids: [ID!]!) {
          nodes(ids: $ids) { __typename ... on MarketRegionCountry { id name code } }
        }
        "#,
        json!({ "ids": ["gid://shopify/MarketRegionCountry/4062110482738"] }),
    ));
    assert_eq!(
        node.body["data"]["nodes"][0],
        json!({
            "__typename": "MarketRegionCountry",
            "id": "gid://shopify/MarketRegionCountry/4062110482738",
            "name": "United Arab Emirates",
            "code": "AE"
        })
    );

    let invalid = proxy.process_request(json_graphql_request(
        r#"
        mutation BackupRegionUpdateInvalid {
          backupRegionUpdate(region: { countryCode: ZZ }) {
            backupRegion { id }
            userErrors { field message code }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        invalid.body["data"]["backupRegionUpdate"],
        json!({
            "backupRegion": null,
            "userErrors": [{
                "field": ["region"],
                "message": "Region not found.",
                "code": "REGION_NOT_FOUND"
            }]
        })
    );

    let invalid_with_typename = proxy.process_request(json_graphql_request(
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
        invalid_with_typename.body["data"]["backupRegionUpdate"]["userErrors"][0]["__typename"],
        json!("MarketUserError")
    );

    let missing_country_code = proxy.process_request(json_graphql_request(
        r#"
        mutation BackupRegionUpdateMissingCountryCode {
          backupRegionUpdate(region: {}) { backupRegion { id } userErrors { field code } }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        missing_country_code.body["errors"][0]["extensions"]["code"],
        json!("missingRequiredInputObjectAttribute")
    );

    let mut access_request = json_graphql_request(
        r#"
        mutation BackupRegionUpdateIdempotent {
          backupRegionUpdate(region: { countryCode: CA }) { backupRegion { id } userErrors { field message code } }
        }
        "#,
        json!({}),
    );
    access_request.headers.insert(
        "X-Shopify-Access-Token".to_string(),
        "shpat_delegate_proxy_1".to_string(),
    );
    let access_denied = proxy.process_request(access_request);
    assert_eq!(
        access_denied.body["data"]["backupRegionUpdate"],
        json!(null)
    );
    assert_eq!(
        access_denied.body["errors"][0]["extensions"]["code"],
        json!("ACCESS_DENIED")
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
          locationDeactivate(locationId: $source, destinationLocationId: $destination) {
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
          locationDeactivate(locationId: $source, destinationLocationId: $destination) {
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
        json!({ "id": "gid://shopify/FulfillmentOrder/9656703910194" }),
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

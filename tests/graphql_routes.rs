use std::sync::{Arc, Mutex};

use pretty_assertions::assert_eq;
use serde_json::{json, Value};
use shopify_draft_proxy::graphql::OperationType;
use shopify_draft_proxy::operation_registry::{
    CapabilityDomain, CapabilityExecution, OperationRegistryEntry,
};
use shopify_draft_proxy::proxy::{Config, DraftProxy, ProductRecord, ReadMode, Request};

fn snapshot_proxy() -> DraftProxy {
    configured_proxy(ReadMode::Snapshot, None)
}

fn configured_proxy(
    read_mode: ReadMode,
    unsupported_mutation_mode: Option<shopify_draft_proxy::proxy::UnsupportedMutationMode>,
) -> DraftProxy {
    DraftProxy::new(Config {
        read_mode,
        unsupported_mutation_mode,
        bulk_operation_run_mutation_max_input_file_size_bytes: None,
        port: 0,
        shopify_admin_origin: "https://shopify.com".to_string(),
        snapshot_path: None,
    })
}

fn graphql_request(method: &str, body: &str) -> Request {
    Request {
        method: method.to_string(),
        path: "/admin/api/2026-04/graphql.json".to_string(),
        headers: Default::default(),
        body: body.to_string(),
    }
}

fn json_graphql_request(query: &str, variables: serde_json::Value) -> Request {
    graphql_request(
        "POST",
        &json!({ "query": query, "variables": variables }).to_string(),
    )
}

fn registry_entry(
    name: &str,
    operation_type: OperationType,
    execution: CapabilityExecution,
    implemented: bool,
) -> OperationRegistryEntry {
    OperationRegistryEntry {
        name: name.to_string(),
        operation_type,
        domain: CapabilityDomain::Products,
        execution,
        implemented,
        match_names: vec![name.to_string()],
        runtime_tests: vec!["tests/graphql_routes.rs".to_string()],
        support_notes: None,
    }
}

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
                "message": "Location has an ongoing relocation."
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

#[test]
fn b2b_fixture_backed_reads_cover_customer_since_and_assignment_nodes() {
    let mut proxy = configured_proxy(ReadMode::LiveHybrid, None);

    let company = proxy.process_request(json_graphql_request(
        r#"
        query B2BCustomerSinceCompanyRead($companyId: ID!) {
          company(id: $companyId) {
            name
            customerSince
          }
        }
        "#,
        json!({ "companyId": "gid://shopify/Company/7681462450" }),
    ));
    assert_eq!(
        company.body["data"]["company"],
        json!({
            "name": "HAR-760 customerSince 1778017011251",
            "customerSince": "2024-01-01T00:00:00Z"
        })
    );

    let nodes = proxy.process_request(json_graphql_request(
        r#"
        query B2BContactLocationAssignmentsNodeRead($ids: [ID!]!) {
          nodes(ids: $ids) {
            ... on CompanyAddress { id address1 city countryCode }
            ... on CompanyContactRoleAssignment {
              id
              companyContact { id title }
              role { id name }
              companyLocation { id name }
            }
            ... on CompanyContact { id title }
            ... on CompanyContactRole { id name }
            ... on CompanyLocation { id name }
          }
        }
        "#,
        json!({
            "ids": [
                "gid://shopify/CompanyAddress/9348383026",
                "gid://shopify/CompanyContactRoleAssignment/44647547186",
                "gid://shopify/CompanyContact/10149003570",
                "gid://shopify/CompanyLocation/8247738674",
                "gid://shopify/CompanyContactRole/10668638514"
            ]
        }),
    ));

    assert_eq!(
        nodes.body["data"]["nodes"],
        json!([
            {
                "id": "gid://shopify/CompanyAddress/9348383026",
                "address1": "446 Assignment Way",
                "city": "Toronto",
                "countryCode": "CA"
            },
            {
                "id": "gid://shopify/CompanyContactRoleAssignment/44647547186",
                "companyContact": {
                    "id": "gid://shopify/CompanyContact/10149003570",
                    "title": "Lead buyer"
                },
                "role": {
                    "id": "gid://shopify/CompanyContactRole/10668638514",
                    "name": "Location admin"
                },
                "companyLocation": {
                    "id": "gid://shopify/CompanyLocation/8247738674",
                    "name": "HAR-446 B2B assignment 1778015458844 Single assignment updated"
                }
            },
            {
                "id": "gid://shopify/CompanyContact/10149003570",
                "title": "Lead buyer"
            },
            {
                "id": "gid://shopify/CompanyLocation/8247738674",
                "name": "HAR-446 B2B assignment 1778015458844 Single assignment updated"
            },
            {
                "id": "gid://shopify/CompanyContactRole/10668638514",
                "name": "Location admin"
            }
        ])
    );
}

#[test]
fn bulk_operation_query_status_and_cancel_reads_stage_local_operations() {
    let mut proxy = snapshot_proxy();

    let empty = proxy.process_request(json_graphql_request(
        r#"
        query BulkOperationStatusParityRead($unknownId: ID!, $first: Int, $runningQuery: String, $runningMutation: String) {
          unknown: bulkOperation(id: $unknownId) { id status }
          runningQueries: bulkOperations(first: $first, query: $runningQuery) { edges { cursor node { id } } nodes { id } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } }
          runningMutations: bulkOperations(first: $first, query: $runningMutation) { edges { cursor node { id } } nodes { id } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } }
          currentMutation: currentBulkOperation(type: MUTATION) { id }
        }
        "#,
        json!({
            "unknownId": "gid://shopify/BulkOperation/unknown",
            "first": 5,
            "runningQuery": "status:RUNNING type:QUERY",
            "runningMutation": "status:RUNNING type:MUTATION"
        }),
    ));
    assert_eq!(empty.body["data"]["unknown"], Value::Null);
    assert_eq!(empty.body["data"]["runningQueries"]["nodes"], json!([]));
    assert_eq!(empty.body["data"]["runningQueries"]["edges"], json!([]));
    assert_eq!(
        empty.body["data"]["runningQueries"]["pageInfo"]["hasNextPage"],
        json!(false)
    );
    assert_eq!(empty.body["data"]["currentMutation"], Value::Null);

    let run = proxy.process_request(json_graphql_request(
        r#"
        mutation BulkOperationRunQueryGroupObjectsTrue($query: String!) {
          bulkOperationRunQuery(query: $query, groupObjects: true) {
            bulkOperation { id status type errorCode createdAt completedAt objectCount rootObjectCount fileSize url partialDataUrl query }
            userErrors { field message }
          }
        }
        "#,
        json!({ "query": "#graphql\n{\n  products {\n    edges {\n      node {\n        id\n        title\n      }\n    }\n  }\n}" }),
    ));
    let id = run.body["data"]["bulkOperationRunQuery"]["bulkOperation"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(
        run.body["data"]["bulkOperationRunQuery"]["userErrors"],
        json!([])
    );
    assert_eq!(
        run.body["data"]["bulkOperationRunQuery"]["bulkOperation"]["status"],
        json!("CREATED")
    );

    let read = proxy.process_request(json_graphql_request(
        r#"
        query BulkOperationByIdParity($id: ID!) {
          bulkOperation(id: $id) { id status type errorCode createdAt completedAt objectCount rootObjectCount fileSize url partialDataUrl query }
        }
        "#,
        json!({ "id": id }),
    ));
    assert_eq!(
        read.body["data"]["bulkOperation"]["status"],
        json!("COMPLETED")
    );
    assert_eq!(read.body["data"]["bulkOperation"]["type"], json!("QUERY"));
    assert_eq!(
        read.body["data"]["bulkOperation"]["objectCount"],
        json!("1432")
    );

    let cancel = proxy.process_request(json_graphql_request(
        r#"
        mutation BulkOperationCancelParity($id: ID!) {
          bulkOperationCancel(id: $id) {
            bulkOperation { id status type errorCode createdAt completedAt objectCount rootObjectCount fileSize url partialDataUrl query }
            userErrors { field message }
          }
        }
        "#,
        json!({ "id": "gid://shopify/BulkOperation/7689772990770" }),
    ));
    assert_eq!(
        cancel.body["data"]["bulkOperationCancel"]["bulkOperation"]["status"],
        json!("CANCELING")
    );
    assert_eq!(
        cancel.body["data"]["bulkOperationCancel"]["userErrors"],
        json!([])
    );
}

#[test]
fn bulk_operation_unported_read_shapes_fall_back_to_upstream_transport() {
    let forwarded = Arc::new(Mutex::new(Vec::<Request>::new()));
    let captured = Arc::clone(&forwarded);
    let mut proxy =
        configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport(move |request| {
            captured.lock().unwrap().push(request);
            shopify_draft_proxy::proxy::Response {
                status: 200,
                headers: Default::default(),
                body: json!({
                    "data": null,
                    "errors": [{
                        "message": "you must provide one of first or last",
                        "path": ["bulkOperations"]
                    }]
                }),
            }
        });

    let response = proxy.process_request(json_graphql_request(
        r#"
        query BulkOperationsMissingWindowValidation {
          bulkOperations { nodes { id } }
        }
        "#,
        json!({}),
    ));

    assert_eq!(response.status, 200);
    assert_eq!(
        response.body["errors"][0]["message"],
        json!("you must provide one of first or last")
    );
    assert_eq!(response.body["data"], Value::Null);
    assert_eq!(forwarded.lock().unwrap().len(), 1);
}

#[test]
fn customer_create_stages_record_for_downstream_customer_reads_and_counts() {
    let mut proxy = snapshot_proxy();
    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation CustomerCreateParityPlan($input: CustomerInput!) {
          customerCreate(input: $input) {
            customer {
              id firstName lastName displayName email locale note verifiedEmail taxExempt taxExemptions tags state canDelete
              loyalty: metafield(namespace: "custom", key: "loyalty") { id namespace key type value }
              metafields(first: 5) { nodes { id namespace key type value } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } }
              defaultEmailAddress { emailAddress }
              defaultPhoneNumber { phoneNumber }
              defaultAddress { address1 city province country zip formattedArea }
              createdAt updatedAt
            }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "input": {
                "email": "hermes-customer-create@example.com",
                "firstName": "Hermes",
                "lastName": "Create",
                "locale": "en",
                "note": "customer create parity probe",
                "phone": "+14155550123",
                "tags": ["parity", "create"],
                "taxExempt": true
            }
        }),
    ));
    let customer = &create.body["data"]["customerCreate"]["customer"];
    let id = customer["id"].as_str().unwrap();
    assert!(id.starts_with("gid://shopify/Customer/"));
    assert_eq!(customer["displayName"], json!("Hermes Create"));
    assert_eq!(customer["tags"], json!(["create", "parity"]));
    assert_eq!(
        create.body["data"]["customerCreate"]["userErrors"],
        json!([])
    );

    let read = proxy.process_request(json_graphql_request(
        r#"
        query CustomerMutationDownstream($id: ID!, $query: String!, $first: Int!) {
          customer(id: $id) { id firstName lastName displayName email tags defaultEmailAddress { emailAddress } defaultPhoneNumber { phoneNumber } }
          customers(first: $first, query: $query, sortKey: UPDATED_AT, reverse: true) { nodes { id email } pageInfo { hasNextPage hasPreviousPage } }
          customersCount { count precision }
        }
        "#,
        json!({ "id": id, "query": "__customer_parity_no_match__", "first": 5 }),
    ));
    assert_eq!(read.body["data"]["customer"]["id"], json!(id));
    assert_eq!(
        read.body["data"]["customer"]["email"],
        json!("hermes-customer-create@example.com")
    );
    assert_eq!(
        read.body["data"]["customers"],
        json!({
            "nodes": [],
            "pageInfo": { "hasNextPage": false, "hasPreviousPage": false }
        })
    );
    assert_eq!(
        read.body["data"]["customersCount"],
        json!({ "count": 177, "precision": "EXACT" })
    );
}

#[test]
fn customer_update_and_delete_stage_known_fixture_customer_reads() {
    let mut proxy = snapshot_proxy();
    let update = proxy.process_request(json_graphql_request(
        r#"
        mutation CustomerUpdateParityPlan($input: CustomerInput!) {
          customerUpdate(input: $input) {
            customer { id firstName lastName displayName email note taxExempt taxExemptions tags defaultPhoneNumber { phoneNumber } loyalty: metafield(namespace: "custom", key: "loyalty") { id namespace key type value } metafields(first: 5) { nodes { id namespace key type value } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "input": {
                "id": "gid://shopify/Customer/9102966915305",
                "firstName": "Hermes",
                "lastName": "Updated",
                "note": "customer update parity probe",
                "tags": ["parity", "updated"],
                "taxExempt": false,
                "taxExemptions": ["CA_BC_RESELLER_EXEMPTION"],
                "metafields": [{ "namespace": "custom", "key": "loyalty", "type": "single_line_text_field", "value": "gold" }]
            }
        }),
    ));
    assert_eq!(
        update.body["data"]["customerUpdate"]["customer"]["displayName"],
        json!("Hermes Updated")
    );
    assert_eq!(
        update.body["data"]["customerUpdate"]["customer"]["loyalty"]["value"],
        json!("gold")
    );
    assert_eq!(
        update.body["data"]["customerUpdate"]["customer"]["defaultPhoneNumber"]["phoneNumber"],
        json!("+14155550123")
    );

    let delete = proxy.process_request(json_graphql_request(
        r#"
        mutation CustomerDeleteParityPlan($input: CustomerDeleteInput!) {
          customerDelete(input: $input) { deletedCustomerId shop { id } userErrors { field message } }
        }
        "#,
        json!({ "input": { "id": "gid://shopify/Customer/9102966915305" } }),
    ));
    assert_eq!(
        delete.body["data"]["customerDelete"],
        json!({
            "deletedCustomerId": "gid://shopify/Customer/9102966915305",
            "shop": { "id": "gid://shopify/Shop/1?shopify-draft-proxy=synthetic" },
            "userErrors": []
        })
    );
    let read = proxy.process_request(json_graphql_request(
        "query($id: ID!) { customer(id: $id) { id email } }",
        json!({ "id": "gid://shopify/Customer/9102966915305" }),
    ));
    assert_eq!(read.body["data"]["customer"], Value::Null);
}

#[test]
fn customer_delete_order_precondition_blocks_only_when_order_exists() {
    let mut proxy = snapshot_proxy();

    let create_query = r#"
        mutation CustomerDeleteOrderPreconditionCustomerCreate($input: CustomerInput!) {
          customerCreate(input: $input) {
            customer { id email displayName }
            userErrors { field message }
          }
        }
        "#;
    let create = proxy.process_request(json_graphql_request(
        create_query,
        json!({
            "input": {
                "email": "har-773-blocked@example.test",
                "firstName": "Blocked",
                "lastName": "Delete"
            }
        }),
    ));
    assert_eq!(
        create.body["data"]["customerCreate"]["userErrors"],
        json!([])
    );
    let customer_id = create.body["data"]["customerCreate"]["customer"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let order = proxy.process_request(json_graphql_request(
        r#"
        mutation CustomerDeleteOrderPreconditionOrderCreate($order: OrderCreateOrderInput!) {
          orderCreate(order: $order) {
            order { id customer { id email displayName } }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "order": {
                "email": "har-773-order@example.test",
                "customerId": customer_id,
                "currency": "CAD",
                "lineItems": [{ "title": "HAR-773 blocking line", "quantity": 1 }]
            }
        }),
    ));
    assert_eq!(order.body["data"]["orderCreate"]["userErrors"], json!([]));

    let blocked = proxy.process_request(json_graphql_request(
        r#"
        mutation CustomerDeleteOrderPreconditionDelete($input: CustomerDeleteInput!) {
          customerDelete(input: $input) { deletedCustomerId userErrors { field message } }
        }
        "#,
        json!({ "input": { "id": customer_id } }),
    ));
    assert_eq!(
        blocked.body["data"]["customerDelete"],
        json!({
            "deletedCustomerId": null,
            "userErrors": [{ "field": ["id"], "message": "Customer can’t be deleted because they have associated orders" }]
        })
    );

    let read = proxy.process_request(json_graphql_request(
        r#"
        query CustomerDeleteOrderPreconditionRead($id: ID!) {
          customer(id: $id) {
            id email displayName
            orders(first: 5) { nodes { id customer { id email displayName } } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } }
          }
        }
        "#,
        json!({ "id": customer_id }),
    ));
    assert_eq!(read.body["data"]["customer"]["id"], json!(customer_id));
    assert_eq!(
        read.body["data"]["customer"]["orders"]["nodes"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn customer_create_supports_consent_precondition_shapes_without_synthesizing_missing_contacts() {
    let mut proxy = snapshot_proxy();
    let phone_only = proxy.process_request(json_graphql_request(
        r#"
        mutation CustomerInputInlineConsentCreate($input: CustomerInput!) {
          customerCreate(input: $input) {
            customer { id email defaultEmailAddress { emailAddress } defaultPhoneNumber { phoneNumber } }
            userErrors { field message }
          }
        }
        "#,
        json!({ "input": { "firstName": "Hermes", "lastName": "ConsentPhoneOnly", "phone": "+141****6021" } }),
    ));
    assert_eq!(
        phone_only.body["data"]["customerCreate"]["userErrors"],
        json!([])
    );
    let phone_customer = &phone_only.body["data"]["customerCreate"]["customer"];
    assert_eq!(phone_customer["email"], Value::Null);
    assert_eq!(phone_customer["defaultEmailAddress"], Value::Null);
    assert_eq!(
        phone_customer["defaultPhoneNumber"]["phoneNumber"],
        json!("+141****6021")
    );

    let email_only = proxy.process_request(json_graphql_request(
        r#"
        mutation CustomerInputInlineConsentCreate($input: CustomerInput!) {
          customerCreate(input: $input) {
            customer { id email defaultEmailAddress { emailAddress } defaultPhoneNumber { phoneNumber } }
            userErrors { field message }
          }
        }
        "#,
        json!({ "input": { "firstName": "Hermes", "lastName": "ConsentEmailOnly", "email": "hermes-consent-email-only-1777943566021@example.com" } }),
    ));
    let email_customer = &email_only.body["data"]["customerCreate"]["customer"];
    assert_eq!(
        email_only.body["data"]["customerCreate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        email_customer["email"],
        json!("hermes-consent-email-only-1777943566021@example.com")
    );
    assert_eq!(
        email_customer["defaultEmailAddress"]["emailAddress"],
        json!("hermes-consent-email-only-1777943566021@example.com")
    );
    assert_eq!(email_customer["defaultPhoneNumber"], Value::Null);
}

#[test]
fn customer_by_identifier_supports_id_for_input_validation_downstream_reads() {
    let mut proxy = snapshot_proxy();
    let update = proxy.process_request(json_graphql_request(
        r#"
        mutation CustomerUpdateParityPlan($input: CustomerInput!) {
          customerUpdate(input: $input) {
            customer { id firstName defaultPhoneNumber { phoneNumber } tags }
            userErrors { field message }
          }
        }
        "#,
        json!({ "input": { "id": "gid://shopify/Customer/9102966915305", "firstName": "", "lastName": "", "phone": "", "tags": ["Zulu", "alpha", "spaced tag"] } }),
    ));
    let id = update.body["data"]["customerUpdate"]["customer"]["id"]
        .as_str()
        .unwrap();
    let read = proxy.process_request(json_graphql_request(
        r#"
        query CustomerInputValidationDownstreamRead($id: ID!, $identifier: CustomerIdentifierInput!) {
          customer(id: $id) { id defaultPhoneNumber { phoneNumber } tags }
          customerByIdentifier(identifier: $identifier) { id defaultPhoneNumber { phoneNumber } tags }
        }
        "#,
        json!({ "id": id, "identifier": { "id": id } }),
    ));
    assert_eq!(read.body["data"]["customerByIdentifier"]["id"], json!(id));
    assert_eq!(
        read.body["data"]["customerByIdentifier"]["defaultPhoneNumber"],
        Value::Null
    );
    assert_eq!(
        read.body["data"]["customerByIdentifier"]["tags"],
        json!(["Zulu", "alpha", "spaced tag"])
    );
}

#[test]
fn customer_set_id_and_unknown_identifier_guards_do_not_stage_or_log() {
    let mut proxy = snapshot_proxy();
    let id_not_allowed = proxy.process_request(json_graphql_request(
        r#"
        mutation CustomerSetIdNotAllowed($input: CustomerSetInput!, $identifier: CustomerSetIdentifiers) {
          customerSet(input: $input, identifier: $identifier) {
            customer { id email }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "identifier": { "email": "customer-set-id-not-allowed@example.com" },
            "input": {
                "id": "gid://shopify/Customer/999999999999998",
                "email": "customer-set-id-not-allowed@example.com",
                "firstName": "IdNotAllowed"
            }
        }),
    ));
    assert_eq!(
        id_not_allowed.body["data"]["customerSet"],
        json!({
            "customer": null,
            "userErrors": [{
                "field": ["input"],
                "message": "The id field is not allowed if identifier is provided.",
                "code": "ID_NOT_ALLOWED"
            }]
        })
    );

    let unknown_id = proxy.process_request(json_graphql_request(
        r#"
        mutation CustomerSetUnknownIdErrors($input: CustomerSetInput!, $identifier: CustomerSetIdentifiers) {
          customerSet(input: $input, identifier: $identifier) {
            customer { id email }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "identifier": { "id": "gid://shopify/Customer/999999999" },
            "input": { "email": "buyer@example.com" }
        }),
    ));
    assert_eq!(
        unknown_id.body["data"]["customerSet"],
        json!({
            "customer": null,
            "userErrors": [{
                "field": ["input"],
                "message": "Resource matching the identifier was not found.",
                "code": "INVALID"
            }]
        })
    );
    assert_eq!(proxy.get_log_snapshot()["entries"], json!([]));
    assert_eq!(
        proxy.get_state_snapshot()["stagedState"]["products"],
        json!({})
    );
}

#[test]
fn quantity_pricing_by_variant_update_returns_seeded_variant_ids_for_b2b_quantity_rules() {
    let mut proxy = snapshot_proxy();

    let response = proxy.process_request(json_graphql_request(
        r#"
        mutation QuantityPricingByVariantUpdate($priceListId: ID!, $input: QuantityPricingByVariantUpdateInput!) {
          quantityPricingByVariantUpdate(priceListId: $priceListId, input: $input) {
            productVariants { id }
            userErrors { field code message }
          }
        }
        "#,
        json!({
            "priceListId": "gid://shopify/PriceList/31575376178",
            "input": {
                "pricesToAdd": [{
                    "variantId": "gid://shopify/ProductVariant/49875425296690",
                    "price": { "amount": "20.00", "currencyCode": "CAD" }
                }],
                "quantityRulesToAdd": [{
                    "variantId": "gid://shopify/ProductVariant/49875425296690",
                    "minimum": 1,
                    "maximum": 20,
                    "increment": 1
                }],
                "quantityPriceBreaksToAdd": [{
                    "variantId": "gid://shopify/ProductVariant/49875425296690",
                    "minimumQuantity": 10,
                    "price": { "amount": "18.00", "currencyCode": "CAD" }
                }]
            }
        }),
    ));

    assert_eq!(
        response.body["data"]["quantityPricingByVariantUpdate"],
        json!({
            "productVariants": [{ "id": "gid://shopify/ProductVariant/49875425296690" }],
            "userErrors": []
        })
    );
}

#[test]
fn delegate_access_token_create_validates_and_stages_synthetic_secret() {
    let mut proxy = snapshot_proxy();

    let empty = proxy.process_request(json_graphql_request(
        r#"
        mutation DelegateAccessTokenCreateEmptyScopeValidation {
          delegateAccessTokenCreate(input: { delegateAccessScope: [] }) {
            delegateAccessToken { accessToken accessScopes createdAt expiresIn }
            userErrors { field message code }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        empty.body["data"]["delegateAccessTokenCreate"],
        json!({
            "delegateAccessToken": null,
            "userErrors": [{ "field": null, "message": "The access scope can't be empty.", "code": "EMPTY_ACCESS_SCOPE" }]
        })
    );

    let negative_expires = proxy.process_request(json_graphql_request(
        r#"
        mutation DelegateAccessTokenCreateNegativeExpiresValidation {
          delegateAccessTokenCreate(input: { delegateAccessScope: ["read_products"], expiresIn: -1 }) {
            delegateAccessToken { accessToken accessScopes createdAt expiresIn }
            userErrors { field message code }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        negative_expires.body["data"]["delegateAccessTokenCreate"],
        json!({
            "delegateAccessToken": null,
            "userErrors": [{ "field": null, "message": "The expires_in value must be greater than 0.", "code": "NEGATIVE_EXPIRES_IN" }]
        })
    );

    let unknown_scope = proxy.process_request(json_graphql_request(
        r#"
        mutation DelegateAccessTokenCreateUnknownScopeValidation {
          delegateAccessTokenCreate(input: { delegateAccessScope: ["fake_scope"] }) {
            delegateAccessToken { accessToken accessScopes createdAt expiresIn }
            userErrors { field message code }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        unknown_scope.body["data"]["delegateAccessTokenCreate"],
        json!({
            "delegateAccessToken": null,
            "userErrors": [{ "field": null, "message": "The access scope is invalid: fake_scope", "code": "UNKNOWN_SCOPES" }]
        })
    );

    let happy = proxy.process_request(json_graphql_request(
        r#"
        mutation DelegateAccessTokenCreateHappyValidation {
          aliasCreate: delegateAccessTokenCreate(input: { delegateAccessScope: ["read_products"], expiresIn: 300 }) {
            delegateAccessToken { accessToken accessScopes createdAt expiresIn }
            userErrors { field message code }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        happy.body["data"]["aliasCreate"]["delegateAccessToken"]["accessScopes"],
        json!(["read_products"])
    );
    assert!(
        happy.body["data"]["aliasCreate"]["delegateAccessToken"]["accessToken"]
            .as_str()
            .is_some_and(|token| token.starts_with("shpat_delegate_proxy_"))
    );
    assert_eq!(
        happy.body["data"]["aliasCreate"]["delegateAccessToken"]["createdAt"],
        json!("2026-04-28T02:10:00.000Z")
    );
    assert_eq!(
        happy.body["data"]["aliasCreate"]["delegateAccessToken"]["expiresIn"],
        json!(300)
    );
    assert_eq!(happy.body["data"]["aliasCreate"]["userErrors"], json!([]));
}

#[test]
fn delegate_access_token_create_shop_payload_expires_parent_and_destroy_lifecycle() {
    let mut proxy = snapshot_proxy();

    let expires_after_parent = proxy.process_request(json_graphql_request(
        r#"
        mutation DelegateAccessTokenCreateExpiresAfterParent {
          delegateAccessTokenCreate(input: { delegateAccessScope: ["read_products"], expiresIn: 99999999 }) {
            delegateAccessToken { accessToken accessScopes createdAt expiresIn }
            userErrors { field message code }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        expires_after_parent.body["data"]["delegateAccessTokenCreate"],
        json!({
            "delegateAccessToken": null,
            "userErrors": [{
                "field": null,
                "message": "The delegate token can't expire after the parent token.",
                "code": "EXPIRES_AFTER_PARENT"
            }]
        })
    );

    let missing = proxy.process_request(json_graphql_request(
        r#"
        mutation DelegateAccessTokenDestroyCodes($token: String!) {
          delegateAccessTokenDestroy(accessToken: $token) {
            status
            shop { id name }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "token": "shpat_does_not_exist" }),
    ));
    assert_eq!(
        missing.body["data"]["delegateAccessTokenDestroy"],
        json!({
            "status": false,
            "shop": { "id": "gid://shopify/Shop/92891250994", "name": "harry-test-heelo" },
            "userErrors": [{ "field": null, "message": "Access token does not exist.", "code": "ACCESS_TOKEN_NOT_FOUND" }]
        })
    );

    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation DelegateAccessTokenCreateShopPayload {
          delegateAccessTokenCreate(input: { delegateAccessScope: ["read_products"], expiresIn: 300 }) {
            delegateAccessToken { accessToken }
            shop { id myshopifyDomain currencyCode }
            userErrors { field message code }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        create.body["data"]["delegateAccessTokenCreate"]["shop"],
        json!({
            "id": "gid://shopify/Shop/92891250994",
            "myshopifyDomain": "harry-test-heelo.myshopify.com",
            "currencyCode": "USD"
        })
    );
    assert_eq!(
        create.body["data"]["delegateAccessTokenCreate"]["userErrors"],
        json!([])
    );
    let token = create.body["data"]["delegateAccessTokenCreate"]["delegateAccessToken"]
        ["accessToken"]
        .as_str()
        .unwrap()
        .to_string();

    let destroy = proxy.process_request(json_graphql_request(
        r#"
        mutation DelegateAccessTokenDestroyShopPayload($token: String!) {
          delegateAccessTokenDestroy(accessToken: $token) {
            shop { id }
            status
            userErrors { field message code }
          }
        }
        "#,
        json!({ "token": token }),
    ));
    assert_eq!(
        destroy.body["data"]["delegateAccessTokenDestroy"],
        json!({
            "shop": { "id": "gid://shopify/Shop/92891250994" },
            "status": true,
            "userErrors": []
        })
    );

    let repeat = proxy.process_request(json_graphql_request(
        r#"
        mutation DelegateAccessTokenDestroyShopPayloadUnknown {
          delegateAccessTokenDestroy(accessToken: "shpat_unknown") {
            shop { id }
            status
            userErrors { field message code }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        repeat.body["data"]["delegateAccessTokenDestroy"],
        json!({
            "shop": { "id": "gid://shopify/Shop/92891250994" },
            "status": false,
            "userErrors": [{ "field": null, "message": "Access token does not exist.", "code": "ACCESS_TOKEN_NOT_FOUND" }]
        })
    );

    let mut self_delete = json_graphql_request(
        r#"
        mutation DelegateAccessTokenDestroyCodes($token: String!) {
          delegateAccessTokenDestroy(accessToken: $token) {
            status
            shop { id name }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "token": "shpat_parent_destroy_self" }),
    );
    self_delete.headers.insert(
        "X-Shopify-Access-Token".to_string(),
        "shpat_parent_destroy_self".to_string(),
    );
    let self_delete = proxy.process_request(self_delete);
    assert_eq!(
        self_delete.body["data"]["delegateAccessTokenDestroy"]["userErrors"],
        json!([{ "field": null, "message": "Can only delete delegate tokens.", "code": "CAN_ONLY_DELETE_DELEGATE_TOKENS" }])
    );
}

#[test]
fn app_revoke_access_scopes_validates_atomically_and_updates_current_installation() {
    let mut proxy = snapshot_proxy();

    let unknown = proxy.process_request(json_graphql_request(
        r#"
        mutation AppRevokeAccessScopesFakeScope {
          appRevokeAccessScopes(scopes: ["fake_scope"]) {
            revoked { handle description }
            userErrors { field message code }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        unknown.body["data"]["appRevokeAccessScopes"],
        json!({
            "revoked": [],
            "userErrors": [{
                "field": ["scopes"],
                "message": "The requested list of scopes to revoke includes invalid handles.",
                "code": "UNKNOWN_SCOPES"
            }]
        })
    );

    let mixed = proxy.process_request(json_graphql_request(
        r#"
        mutation AppRevokeAccessScopesMixedFakeScope {
          appRevokeAccessScopes(scopes: ["read_products", "fake_scope"]) {
            revoked { handle description }
            userErrors { field message code }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        mixed.body["data"]["appRevokeAccessScopes"]["revoked"],
        json!([])
    );
    assert_eq!(
        mixed.body["data"]["appRevokeAccessScopes"]["userErrors"],
        json!([
            {
                "field": ["scopes"],
                "message": "Scopes that are declared as required cannot be revoked.",
                "code": "CANNOT_REVOKE_REQUIRED_SCOPES"
            },
            {
                "field": ["scopes"],
                "message": "The requested list of scopes to revoke includes invalid handles.",
                "code": "UNKNOWN_SCOPES"
            }
        ])
    );

    let required = proxy.process_request(json_graphql_request(
        r#"
        mutation AppRevokeAccessScopesRequiredReadProducts {
          appRevokeAccessScopes(scopes: ["read_products"]) {
            revoked { handle description }
            userErrors { field message code }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        required.body["data"]["appRevokeAccessScopes"],
        json!({
            "revoked": [],
            "userErrors": [{
                "field": ["scopes"],
                "message": "Scopes that are declared as required cannot be revoked.",
                "code": "CANNOT_REVOKE_REQUIRED_SCOPES"
            }]
        })
    );

    let missing_source_app = proxy.process_request(json_graphql_request(
        r#"
        mutation AppRevokeAccessScopesErrorCodes {
          appRevokeAccessScopes(scopes: ["write_products"]) {
            revoked { handle description }
            userErrors { field message code }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        missing_source_app.body["data"]["appRevokeAccessScopes"],
        json!({
            "revoked": [],
            "userErrors": [{ "field": ["base"], "message": "Source app is missing.", "code": "MISSING_SOURCE_APP" }]
        })
    );

    let optional = proxy.process_request(json_graphql_request(
        r#"
        mutation AppRevokeAccessScopesOptionalWriteProducts {
          appRevokeAccessScopes(scopes: ["write_products"]) {
            revoked { handle description }
            userErrors { field message code }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        optional.body["data"]["appRevokeAccessScopes"],
        json!({
            "revoked": [{ "handle": "write_products", "description": null }],
            "userErrors": []
        })
    );

    let read = proxy.process_request(json_graphql_request(
        r#"
        query AppAccessScopesLocalRead {
          currentAppInstallation { accessScopes { handle } }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        read.body,
        json!({ "data": { "currentAppInstallation": { "accessScopes": [{ "handle": "read_products" }] } } })
    );
}

#[test]
fn app_purchase_one_time_create_validates_and_stages_selected_fields() {
    let mut proxy = snapshot_proxy();

    let blank = proxy.process_request(json_graphql_request(
        r#"
        mutation AppPurchaseOneTimeCreateValidationBlankName {
          create: appPurchaseOneTimeCreate(name: "   ", returnUrl: "https://app.example.test/return", price: { amount: "5.00", currencyCode: USD }, test: true) {
            appPurchaseOneTime { id }
            confirmationUrl
            userErrors { field message code }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        blank.body["data"]["create"],
        json!({
            "appPurchaseOneTime": null,
            "confirmationUrl": null,
            "userErrors": [{ "field": ["name"], "message": "Name can't be blank", "code": null }]
        })
    );

    let zero_price = proxy.process_request(json_graphql_request(
        r#"
        mutation AppPurchaseOneTimeCreateValidationZeroPrice {
          appPurchaseOneTimeCreate(name: "Pro", returnUrl: "https://app.example.test/return", price: { amount: "0", currencyCode: USD }, test: true) {
            appPurchaseOneTime { id }
            confirmationUrl
            userErrors { field message code }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        zero_price.body["data"]["appPurchaseOneTimeCreate"],
        json!({
            "appPurchaseOneTime": null,
            "confirmationUrl": null,
            "userErrors": [{ "field": ["price"], "message": "Price must be at least 0.50 USD.", "code": "PRICE_TOO_LOW" }]
        })
    );

    let currency_mismatch = proxy.process_request(json_graphql_request(
        r#"
        mutation AppPurchaseOneTimeCreateValidationCurrencyMismatch {
          appPurchaseOneTimeCreate(name: "Pro", returnUrl: "https://app.example.test/return", price: { amount: "5.00", currencyCode: EUR }, test: true) {
            appPurchaseOneTime { id }
            confirmationUrl
            userErrors { field message code }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        currency_mismatch.body["data"]["appPurchaseOneTimeCreate"],
        json!({
            "appPurchaseOneTime": null,
            "confirmationUrl": null,
            "userErrors": [{ "field": ["price"], "message": "Price currency must match shop billing currency USD.", "code": null }]
        })
    );

    let missing_return_url = proxy.process_request(json_graphql_request(
        r#"
        mutation AppPurchaseOneTimeCreateValidationMissingReturnUrl {
          appPurchaseOneTimeCreate(name: "Pro", price: { amount: "5.00", currencyCode: USD }, test: true) {
            appPurchaseOneTime { id }
            confirmationUrl
            userErrors { field message code }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        missing_return_url.body["errors"][0]["extensions"],
        json!({
            "code": "missingRequiredArguments",
            "className": "Field",
            "name": "appPurchaseOneTimeCreate",
            "arguments": "returnUrl"
        })
    );

    let success = proxy.process_request(json_graphql_request(
        r#"
        mutation AppPurchaseOneTimeCreateValidationSuccess {
          appPurchaseOneTimeCreate(name: "HAR-646 valid test", returnUrl: "https://app.example.test/return", price: { amount: "5.00", currencyCode: USD }, test: true) {
            appPurchaseOneTime { id name status test createdAt price { amount currencyCode } }
            confirmationUrl
            userErrors { field message code }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        success.body["data"]["appPurchaseOneTimeCreate"],
        json!({
            "appPurchaseOneTime": {
                "id": "gid://shopify/AppPurchaseOneTime/expected",
                "name": "HAR-646 valid test",
                "status": "ACTIVE",
                "test": true,
                "createdAt": "2024-01-01T00:00:00.000Z",
                "price": { "amount": "5.00", "currencyCode": "USD" }
            },
            "confirmationUrl": "https://app.example.test/local-confirmation",
            "userErrors": []
        })
    );
}

#[test]
fn app_subscription_create_cancel_and_repeat_cancel_stages_status_transitions() {
    let mut proxy = snapshot_proxy();

    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation AppSubscriptionCreateLocalLifecycle($lineItems: [AppSubscriptionLineItemInput!]!) {
          appSubscriptionCreate(
            name: "Local plan"
            returnUrl: "https://app.example.test/return"
            trialDays: 7
            test: true
            lineItems: $lineItems
          ) {
            confirmationUrl
            appSubscription {
              id
              name
              status
              test
              trialDays
              lineItems {
                id
                plan {
                  pricingDetails {
                    __typename
                    ... on AppUsagePricing {
                      cappedAmount { amount currencyCode }
                      balanceUsed { amount currencyCode }
                      interval
                      terms
                    }
                  }
                }
              }
            }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "lineItems": [{
                "plan": {
                    "appUsagePricingDetails": {
                        "cappedAmount": { "amount": 100, "currencyCode": "USD" },
                        "terms": "usage terms"
                    }
                }
            }]
        }),
    ));
    assert_eq!(
        create.body["data"]["appSubscriptionCreate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        create.body["data"]["appSubscriptionCreate"]["appSubscription"],
        json!({
            "id": "gid://shopify/AppSubscription/expected",
            "name": "Local plan",
            "status": "ACTIVE",
            "test": true,
            "trialDays": 7,
            "lineItems": [{
                "id": "gid://shopify/AppSubscriptionLineItem/expected",
                "plan": { "pricingDetails": {
                    "__typename": "AppUsagePricing",
                    "cappedAmount": { "amount": "100", "currencyCode": "USD" },
                    "balanceUsed": { "amount": "0.0", "currencyCode": "USD" },
                    "interval": "EVERY_30_DAYS",
                    "terms": "usage terms"
                }}
            }]
        })
    );

    let cancel = proxy.process_request(json_graphql_request(
        r#"
        mutation AppSubscriptionCancelLocalLifecycle($id: ID!) {
          appSubscriptionCancel(id: $id, prorate: true) {
            appSubscription { id status trialDays }
            userErrors { field message }
          }
        }
        "#,
        json!({ "id": "gid://shopify/AppSubscription/expected" }),
    ));
    assert_eq!(
        cancel.body["data"]["appSubscriptionCancel"],
        json!({
            "appSubscription": { "id": "gid://shopify/AppSubscription/expected", "status": "CANCELLED", "trialDays": 7 },
            "userErrors": []
        })
    );

    let repeat = proxy.process_request(json_graphql_request(
        r#"
        mutation AppSubscriptionCancelLocalLifecycle($id: ID!) {
          appSubscriptionCancel(id: $id, prorate: true) {
            appSubscription { id status trialDays }
            userErrors { field message }
          }
        }
        "#,
        json!({ "id": "gid://shopify/AppSubscription/expected" }),
    ));
    assert_eq!(
        repeat.body["data"]["appSubscriptionCancel"],
        json!({
            "appSubscription": null,
            "userErrors": [{ "field": ["id"], "message": "Cannot transition status via :cancel from :cancelled" }]
        })
    );
}

#[test]
fn app_usage_record_create_caps_idempotency_and_readback_balance() {
    let mut proxy = snapshot_proxy();

    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation AppSubscriptionCreateLocalLifecycle($lineItems: [AppSubscriptionLineItemInput!]!) {
          appSubscriptionCreate(
            name: "Local plan"
            returnUrl: "https://app.example.test/return"
            trialDays: 7
            test: true
            lineItems: $lineItems
          ) {
            appSubscription {
              id
              lineItems {
                id
                plan { pricingDetails { __typename ... on AppUsagePricing { cappedAmount { amount currencyCode } } } }
              }
            }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "lineItems": [{
                "plan": { "appUsagePricingDetails": { "cappedAmount": { "amount": 5, "currencyCode": "USD" }, "terms": "usage terms" } }
            }]
        }),
    ));
    assert_eq!(
        create.body["data"]["appSubscriptionCreate"]["appSubscription"]["lineItems"][0]["id"],
        json!("gid://shopify/AppSubscriptionLineItem/expected")
    );

    let success_query = r#"
        mutation AppUsageRecordCreateCapSuccess($id: ID!) {
          appUsageRecordCreate(
            subscriptionLineItemId: $id
            price: { amount: "3.00", currencyCode: USD }
            description: "first"
            idempotencyKey: "usage-key-cap-1"
          ) {
            appUsageRecord {
              id
              description
              price { amount currencyCode }
              subscriptionLineItem { id plan { pricingDetails { __typename ... on AppUsagePricing { balanceUsed { amount currencyCode } } } } }
            }
            userErrors { field message }
          }
        }
    "#;
    let success = proxy.process_request(json_graphql_request(
        success_query,
        json!({ "id": "gid://shopify/AppSubscriptionLineItem/expected" }),
    ));
    assert_eq!(
        success.body["data"]["appUsageRecordCreate"],
        json!({
            "appUsageRecord": {
                "id": "gid://shopify/AppUsageRecord/expected",
                "description": "first",
                "price": { "amount": "3.00", "currencyCode": "USD" },
                "subscriptionLineItem": {
                    "id": "gid://shopify/AppSubscriptionLineItem/expected",
                    "plan": { "pricingDetails": { "__typename": "AppUsagePricing", "balanceUsed": { "amount": "3.00", "currencyCode": "USD" } } }
                }
            },
            "userErrors": []
        })
    );

    let duplicate = proxy.process_request(json_graphql_request(
        success_query,
        json!({ "id": "gid://shopify/AppSubscriptionLineItem/expected" }),
    ));
    assert_eq!(duplicate.body, success.body);

    let over_cap = proxy.process_request(json_graphql_request(
        r#"
        mutation AppUsageRecordCreateCapOverLimit($id: ID!) {
          appUsageRecordCreate(
            subscriptionLineItemId: $id
            price: { amount: "3.00", currencyCode: USD }
            description: "second"
            idempotencyKey: "usage-key-cap-2"
          ) {
            appUsageRecord { id }
            userErrors { field message }
          }
        }
        "#,
        json!({ "id": "gid://shopify/AppSubscriptionLineItem/expected" }),
    ));
    assert_eq!(
        over_cap.body["data"]["appUsageRecordCreate"],
        json!({
            "appUsageRecord": null,
            "userErrors": [{ "field": [], "message": "Total price exceeds balance remaining" }]
        })
    );

    let long_key = proxy.process_request(json_graphql_request(
        r#"
        mutation AppUsageRecordCreateLongIdempotencyKey($id: ID!, $key: String) {
          appUsageRecordCreate(
            subscriptionLineItemId: $id
            price: { amount: "1.00", currencyCode: USD }
            description: "too long"
            idempotencyKey: $key
          ) {
            appUsageRecord { id }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "id": "gid://shopify/AppSubscriptionLineItem/expected",
            "key": "x".repeat(256)
        }),
    ));
    assert_eq!(
        long_key.body["data"]["appUsageRecordCreate"],
        json!({
            "appUsageRecord": null,
            "userErrors": [{ "field": ["idempotencyKey"], "message": "Idempotency key must be at most 255 characters", "code": null }]
        })
    );

    let readback = proxy.process_request(json_graphql_request(
        r#"
        query AppUsageRecordCreateCapRead {
          currentAppInstallation {
            allSubscriptions(first: 5) {
              nodes {
                lineItems {
                  plan { pricingDetails { __typename ... on AppUsagePricing { balanceUsed { amount currencyCode } } } }
                  usageRecords { nodes { id description price { amount currencyCode } } }
                }
              }
            }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        readback.body["data"]["currentAppInstallation"],
        json!({
            "allSubscriptions": { "nodes": [{
                "lineItems": [{
                    "plan": { "pricingDetails": { "__typename": "AppUsagePricing", "balanceUsed": { "amount": "3.00", "currencyCode": "USD" } } },
                    "usageRecords": { "nodes": [{
                        "id": "gid://shopify/AppUsageRecord/expected",
                        "description": "first",
                        "price": { "amount": "3.00", "currencyCode": "USD" }
                    }] }
                }]
            }] }
        })
    );
}

#[test]
fn app_billing_access_local_lifecycle_reads_nodes_and_uninstall_cascade() {
    let mut proxy = snapshot_proxy();

    let create_subscription = proxy.process_request(json_graphql_request(
        r#"
        mutation AppSubscriptionCreateLocalLifecycle($lineItems: [AppSubscriptionLineItemInput!]!) {
          appSubscriptionCreate(name: "Local plan", returnUrl: "https://app.example.test/return", trialDays: 7, test: true, lineItems: $lineItems) {
            appSubscription { id status trialDays lineItems { id } }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "lineItems": [{
                "plan": { "appUsagePricingDetails": { "cappedAmount": { "amount": 100, "currencyCode": "USD" }, "terms": "usage terms" } }
            }]
        }),
    ));
    assert_eq!(
        create_subscription.body["data"]["appSubscriptionCreate"]["appSubscription"]["id"],
        json!("gid://shopify/AppSubscription/expected")
    );

    let one_time = proxy.process_request(json_graphql_request(
        r#"
        mutation AppPurchaseOneTimeCreateLocalLifecycle {
          appPurchaseOneTimeCreate(name: "Import package", returnUrl: "https://app.example.test/return", price: { amount: 10, currencyCode: USD }, test: true) {
            confirmationUrl
            appPurchaseOneTime { id name status test price { amount currencyCode } }
            userErrors { field message }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        one_time.body["data"]["appPurchaseOneTimeCreate"],
        json!({
            "confirmationUrl": "https://app.example.test/local-confirmation",
            "appPurchaseOneTime": {
                "id": "gid://shopify/AppPurchaseOneTime/expected",
                "name": "Import package",
                "status": "ACTIVE",
                "test": true,
                "price": { "amount": "10", "currencyCode": "USD" }
            },
            "userErrors": []
        })
    );

    let usage = proxy.process_request(json_graphql_request(
        r#"
        mutation AppUsageRecordCreateLocalLifecycle($id: ID!) {
          appUsageRecordCreate(subscriptionLineItemId: $id, price: { amount: "12.5", currencyCode: USD }, description: "metered import", idempotencyKey: "usage-local-1") {
            appUsageRecord { id description price { amount currencyCode } subscriptionLineItem { id } }
            userErrors { field message }
          }
        }
        "#,
        json!({ "id": "gid://shopify/AppSubscriptionLineItem/expected" }),
    ));
    assert_eq!(
        usage.body["data"]["appUsageRecordCreate"]["appUsageRecord"],
        json!({
            "id": "gid://shopify/AppUsageRecord/expected",
            "description": "metered import",
            "price": { "amount": "12.5", "currencyCode": "USD" },
            "subscriptionLineItem": { "id": "gid://shopify/AppSubscriptionLineItem/expected" }
        })
    );

    let expired_trial = proxy.process_request(json_graphql_request(
        r#"
        mutation AppSubscriptionTrialExtendLocalLifecycle($id: ID!) {
          appSubscriptionTrialExtend(id: $id, days: 3) {
            appSubscription { id trialDays }
            userErrors { field message }
          }
        }
        "#,
        json!({ "id": "gid://shopify/AppSubscription/expected" }),
    ));
    assert_eq!(
        expired_trial.body["data"]["appSubscriptionTrialExtend"],
        json!({
            "appSubscription": null,
            "userErrors": [{ "field": ["id"], "message": "The trial can't be extended after expiration." }]
        })
    );

    proxy.process_request(json_graphql_request(
        r#"
        mutation AppSubscriptionCancelLocalLifecycle($id: ID!) {
          appSubscriptionCancel(id: $id, prorate: true) { appSubscription { id status trialDays } userErrors { field message } }
        }
        "#,
        json!({ "id": "gid://shopify/AppSubscription/expected" }),
    ));

    let readback = proxy.process_request(json_graphql_request(
        r#"
        query AppBillingLocalRead {
          currentAppInstallation {
            id
            activeSubscriptions { id }
            allSubscriptions(first: 5) { nodes { id status trialDays lineItems { id usageRecords(first: 5) { nodes { description price { amount currencyCode } } } } } }
            oneTimePurchases(first: 5) { nodes { name status price { amount currencyCode } } }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        readback.body["data"]["currentAppInstallation"],
        json!({
            "id": "gid://shopify/AppInstallation/expected",
            "activeSubscriptions": [],
            "allSubscriptions": { "nodes": [{
                "id": "gid://shopify/AppSubscription/expected",
                "status": "CANCELLED",
                "trialDays": 7,
                "lineItems": [{
                    "id": "gid://shopify/AppSubscriptionLineItem/expected",
                    "usageRecords": { "nodes": [{
                        "description": "metered import",
                        "price": { "amount": "12.5", "currencyCode": "USD" }
                    }] }
                }]
            }] },
            "oneTimePurchases": { "nodes": [{
                "name": "Import package",
                "status": "ACTIVE",
                "price": { "amount": "10", "currencyCode": "USD" }
            }] }
        })
    );

    let node_read = proxy.process_request(json_graphql_request(
        r#"
        query AppBillingNodeRead($id: ID!) {
          node(id: $id) {
            ... on AppPurchaseOneTime { id name status test price { amount currencyCode } }
          }
        }
        "#,
        json!({ "id": "gid://shopify/AppPurchaseOneTime/expected" }),
    ));
    assert_eq!(
        node_read.body["data"]["node"],
        json!({
            "id": "gid://shopify/AppPurchaseOneTime/expected",
            "name": "Import package",
            "status": "ACTIVE",
            "test": true,
            "price": { "amount": "10", "currencyCode": "USD" }
        })
    );

    let uninstall = proxy.process_request(json_graphql_request(
        r#"
        mutation AppUninstallLocalLifecycle { appUninstall { app { id handle } userErrors { field message } } }
        "#,
        json!({}),
    ));
    assert_eq!(
        uninstall.body["data"]["appUninstall"],
        json!({
            "app": { "id": "gid://shopify/App/expected", "handle": "shopify-draft-proxy" },
            "userErrors": []
        })
    );

    let after_uninstall = proxy.process_request(json_graphql_request(
        r#"query AppInstallationIdLocalRead { currentAppInstallation { id } }"#,
        json!({}),
    ));
    assert_eq!(
        after_uninstall.body["data"]["currentAppInstallation"],
        Value::Null
    );
}

#[test]
fn app_subscription_line_item_update_validates_recurring_currency_and_amount() {
    let mut proxy = snapshot_proxy();

    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation AppSubscriptionCreateLocalLifecycle($lineItems: [AppSubscriptionLineItemInput!]!) {
          appSubscriptionCreate(
            name: "Local plan"
            returnUrl: "https://app.example.test/return"
            trialDays: 7
            test: true
            lineItems: $lineItems
          ) {
            confirmationUrl
            appSubscription {
              id
              lineItems {
                id
                plan {
                  pricingDetails {
                    __typename
                    ... on AppUsagePricing { cappedAmount { amount currencyCode } }
                    ... on AppRecurringPricing { price { amount currencyCode } }
                  }
                }
              }
            }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "lineItems": [
                { "plan": { "appUsagePricingDetails": { "cappedAmount": { "amount": 5, "currencyCode": "USD" }, "terms": "usage terms" } } },
                { "plan": { "appRecurringPricingDetails": { "price": { "amount": 1, "currencyCode": "USD" }, "interval": "EVERY_30_DAYS" } } }
            ]
        }),
    ));
    assert_eq!(
        create.body["data"]["appSubscriptionCreate"],
        json!({
            "confirmationUrl": "https://app.example.test/local-confirmation",
            "appSubscription": {
                "id": "gid://shopify/AppSubscription/expected",
                "lineItems": [
                    {
                        "id": "gid://shopify/AppSubscriptionLineItem/usage",
                        "plan": { "pricingDetails": {
                            "__typename": "AppUsagePricing",
                            "cappedAmount": { "amount": "5", "currencyCode": "USD" }
                        }}
                    },
                    {
                        "id": "gid://shopify/AppSubscriptionLineItem/recurring",
                        "plan": { "pricingDetails": {
                            "__typename": "AppRecurringPricing",
                            "price": { "amount": "1", "currencyCode": "USD" }
                        }}
                    }
                ]
            },
            "userErrors": []
        })
    );

    let update = proxy.process_request(json_graphql_request(
        r#"
        mutation AppSubscriptionLineItemUpdateValidation($usageLineItemId: ID!, $recurringLineItemId: ID!) {
          recurring: appSubscriptionLineItemUpdate(id: $recurringLineItemId, cappedAmount: { amount: 10, currencyCode: USD }) {
            appSubscription { id }
            userErrors { field message }
          }
          currencyMismatch: appSubscriptionLineItemUpdate(id: $usageLineItemId, cappedAmount: { amount: 10, currencyCode: EUR }) {
            appSubscription { id }
            userErrors { field message }
          }
          nonIncreasing: appSubscriptionLineItemUpdate(id: $usageLineItemId, cappedAmount: { amount: 3, currencyCode: USD }) {
            appSubscription { id }
            userErrors { field message }
          }
          success: appSubscriptionLineItemUpdate(id: $usageLineItemId, cappedAmount: { amount: 10, currencyCode: USD }) {
            confirmationUrl
            appSubscription {
              id
              lineItems {
                id
                plan {
                  pricingDetails {
                    __typename
                    ... on AppUsagePricing { cappedAmount { amount currencyCode } }
                    ... on AppRecurringPricing { price { amount currencyCode } }
                  }
                }
              }
            }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "usageLineItemId": "gid://shopify/AppSubscriptionLineItem/usage",
            "recurringLineItemId": "gid://shopify/AppSubscriptionLineItem/recurring"
        }),
    ));

    assert_eq!(
        update.body["data"],
        json!({
            "recurring": {
                "appSubscription": null,
                "userErrors": [{ "field": ["cappedAmount"], "message": "Only usage-pricing line items support cappedAmount updates" }]
            },
            "currencyMismatch": {
                "appSubscription": null,
                "userErrors": [{ "field": ["cappedAmount"], "message": "Capped amount currency mismatch. Expected USD" }]
            },
            "nonIncreasing": {
                "appSubscription": null,
                "userErrors": [{ "field": ["cappedAmount"], "message": "The capped amount must be greater than the existing capped amount" }]
            },
            "success": {
                "confirmationUrl": "https://app.example.test/local-confirmation",
                "appSubscription": {
                    "id": "gid://shopify/AppSubscription/expected",
                    "lineItems": [
                        {
                            "id": "gid://shopify/AppSubscriptionLineItem/usage",
                            "plan": { "pricingDetails": {
                                "__typename": "AppUsagePricing",
                                "cappedAmount": { "amount": "5", "currencyCode": "USD" }
                            }}
                        },
                        {
                            "id": "gid://shopify/AppSubscriptionLineItem/recurring",
                            "plan": { "pricingDetails": {
                                "__typename": "AppRecurringPricing",
                                "price": { "amount": "1", "currencyCode": "USD" }
                            }}
                        }
                    ]
                },
                "userErrors": []
            }
        })
    );
}

#[test]
fn app_subscription_trial_extend_validates_days_unknown_and_inactive_status() {
    let mut proxy = snapshot_proxy();

    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation AppSubscriptionCreatePendingLocalLifecycle($lineItems: [AppSubscriptionLineItemInput!]!) {
          appSubscriptionCreate(
            name: "Local plan"
            returnUrl: "https://app.example.test/return"
            trialDays: 7
            test: false
            lineItems: $lineItems
          ) {
            appSubscription { id status trialDays }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "lineItems": [{
                "plan": {
                    "appUsagePricingDetails": {
                        "cappedAmount": { "amount": 100, "currencyCode": "USD" },
                        "terms": "usage terms"
                    }
                }
            }]
        }),
    ));
    assert_eq!(
        create.body["data"]["appSubscriptionCreate"],
        json!({
            "appSubscription": {
                "id": "gid://shopify/AppSubscription/expected",
                "status": "PENDING",
                "trialDays": 7
            },
            "userErrors": []
        })
    );

    let trial_extend_query = r#"
        mutation AppSubscriptionTrialExtendValidation($id: ID!, $days: Int!) {
          appSubscriptionTrialExtend(id: $id, days: $days) {
            appSubscription { id trialDays }
            userErrors { field message code }
          }
        }
    "#;

    let days_zero = proxy.process_request(json_graphql_request(
        trial_extend_query,
        json!({ "id": "gid://shopify/AppSubscription/expected", "days": 0 }),
    ));
    assert_eq!(
        days_zero.body["data"]["appSubscriptionTrialExtend"],
        json!({
            "appSubscription": null,
            "userErrors": [{ "field": ["days"], "message": "Days must be greater than 0", "code": null }]
        })
    );

    let days_too_large = proxy.process_request(json_graphql_request(
        trial_extend_query,
        json!({ "id": "gid://shopify/AppSubscription/expected", "days": 1001 }),
    ));
    assert_eq!(
        days_too_large.body["data"]["appSubscriptionTrialExtend"],
        json!({
            "appSubscription": null,
            "userErrors": [{ "field": ["days"], "message": "Days must be less than or equal to 1000", "code": null }]
        })
    );

    let unknown = proxy.process_request(json_graphql_request(
        trial_extend_query,
        json!({ "id": "gid://shopify/AppSubscription/unknown", "days": 5 }),
    ));
    assert_eq!(
        unknown.body["data"]["appSubscriptionTrialExtend"],
        json!({
            "appSubscription": null,
            "userErrors": [{ "field": ["id"], "message": "The app subscription wasn't found.", "code": "SUBSCRIPTION_NOT_FOUND" }]
        })
    );

    let pending = proxy.process_request(json_graphql_request(
        trial_extend_query,
        json!({ "id": "gid://shopify/AppSubscription/expected", "days": 5 }),
    ));
    assert_eq!(
        pending.body["data"]["appSubscriptionTrialExtend"],
        json!({
            "appSubscription": null,
            "userErrors": [{ "field": ["id"], "message": "The trial can't be extended on inactive app subscriptions.", "code": "SUBSCRIPTION_NOT_ACTIVE" }]
        })
    );
}

#[test]
fn app_subscription_create_activates_test_charge_and_reads_back_current_installation() {
    let mut proxy = snapshot_proxy();

    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation AppSubscriptionCreateActivationReadback {
          subscription: appSubscriptionCreate(
            name: "Activation readback plan"
            returnUrl: "https://app.example.test/return"
            trialDays: 7
            test: true
            lineItems: [
              { plan: { appRecurringPricingDetails: { price: { amount: "10.00", currencyCode: USD }, interval: EVERY_30_DAYS } } }
            ]
          ) {
            confirmationUrl
            appSubscription { id status test trialDays currentPeriodEnd }
            userErrors { field message }
          }
        }
        "#,
        json!({}),
    ));
    let subscription_id = create.body["data"]["subscription"]["appSubscription"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(
        create.body["data"]["subscription"],
        json!({
            "confirmationUrl": "https://app.example.test/local-confirmation",
            "appSubscription": {
                "id": subscription_id,
                "status": "ACTIVE",
                "test": true,
                "trialDays": 7,
                "currentPeriodEnd": "2024-02-07T00:00:00.000Z"
            },
            "userErrors": []
        })
    );

    let read = proxy.process_request(json_graphql_request(
        r#"
        query AppSubscriptionActivationRead {
          installation: currentAppInstallation {
            activeSubscriptions { id status currentPeriodEnd }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        read.body,
        json!({
            "data": {
                "installation": {
                    "activeSubscriptions": [{
                        "id": subscription_id,
                        "status": "ACTIVE",
                        "currentPeriodEnd": "2024-02-07T00:00:00.000Z"
                    }]
                }
            }
        })
    );
}

#[test]
fn fulfillment_service_lifecycle_stages_location_reads_deletes_and_validates() {
    let mut proxy = snapshot_proxy();
    let invalid = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateBlank($name: String!, $callbackUrl: URL) {
          fulfillmentServiceCreate(
            name: $name
            callbackUrl: $callbackUrl
            trackingSupport: true
            inventoryManagement: true
            requiresShippingMethod: true
          ) {
            fulfillmentService { id }
            userErrors { field message }
          }
        }
        "#,
        json!({ "name": "", "callbackUrl": "https://example.com/fulfillment-service/moeomiux" }),
    ));
    assert_eq!(
        invalid.body["data"]["fulfillmentServiceCreate"],
        json!({
            "fulfillmentService": null,
            "userErrors": [
                { "field": ["name"], "message": "Name can't be blank" },
                { "field": ["callbackUrl"], "message": "Callback url is not allowed" }
            ]
        })
    );

    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateFs($name: String!) {
          fulfillmentServiceCreate(name: $name, trackingSupport: true, inventoryManagement: true, requiresShippingMethod: true) {
            fulfillmentService {
              id handle serviceName callbackUrl trackingSupport inventoryManagement requiresShippingMethod type
              location { id name isFulfillmentService fulfillsOnlineOrders shipsInventory }
            }
            userErrors { field message }
          }
        }
        "#,
        json!({ "name": "Hermes FS moeompnx" }),
    ));
    let service_id = create.body["data"]["fulfillmentServiceCreate"]["fulfillmentService"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let location_id = create.body["data"]["fulfillmentServiceCreate"]["fulfillmentService"]
        ["location"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(
        create.body["data"]["fulfillmentServiceCreate"]["fulfillmentService"],
        json!({
            "id": service_id,
            "handle": "hermes-fs-moeompnx",
            "serviceName": "Hermes FS moeompnx",
            "callbackUrl": null,
            "trackingSupport": true,
            "inventoryManagement": true,
            "requiresShippingMethod": true,
            "type": "THIRD_PARTY",
            "location": {
                "id": location_id,
                "name": "Hermes FS moeompnx",
                "isFulfillmentService": true,
                "fulfillsOnlineOrders": true,
                "shipsInventory": false
            }
        })
    );

    let read = proxy.process_request(json_graphql_request(
        r#"
        query FulfillmentServiceAfterCreate($id: ID!, $locationId: ID!) {
          fulfillmentService(id: $id) {
            id handle serviceName callbackUrl trackingSupport inventoryManagement requiresShippingMethod type
            location { id name isFulfillmentService fulfillsOnlineOrders shipsInventory }
          }
          location(id: $locationId) { id name isFulfillmentService fulfillsOnlineOrders shipsInventory }
        }
        "#,
        json!({ "id": service_id, "locationId": location_id }),
    ));
    assert_eq!(
        read.body["data"]["fulfillmentService"],
        create.body["data"]["fulfillmentServiceCreate"]["fulfillmentService"]
    );
    assert_eq!(
        read.body["data"]["location"],
        create.body["data"]["fulfillmentServiceCreate"]["fulfillmentService"]["location"]
    );

    let update = proxy.process_request(json_graphql_request(
        r#"
        mutation UpdateFs($id: ID!, $name: String!) {
          fulfillmentServiceUpdate(id: $id, name: $name, trackingSupport: false, inventoryManagement: false, requiresShippingMethod: false) {
            fulfillmentService {
              id handle serviceName callbackUrl trackingSupport inventoryManagement requiresShippingMethod type
              location { id name isFulfillmentService fulfillsOnlineOrders shipsInventory }
            }
            userErrors { field message }
          }
        }
        "#,
        json!({ "id": service_id, "name": "Hermes FS Updated moeompnx" }),
    ));
    assert_eq!(
        update.body["data"]["fulfillmentServiceUpdate"]["fulfillmentService"]["serviceName"],
        json!("Hermes FS Updated moeompnx")
    );
    assert_eq!(
        update.body["data"]["fulfillmentServiceUpdate"]["fulfillmentService"]["location"]["name"],
        json!("Hermes FS Updated moeompnx")
    );

    let delete = proxy.process_request(json_graphql_request(
        r#"
        mutation DeleteFs($id: ID!) {
          fulfillmentServiceDelete(id: $id, inventoryAction: DELETE) { deletedId userErrors { field message } }
        }
        "#,
        json!({ "id": service_id }),
    ));
    assert_eq!(
        delete.body["data"]["fulfillmentServiceDelete"],
        json!({ "deletedId": service_id.replace("?id=true", ""), "userErrors": [] })
    );

    let after_delete = proxy.process_request(json_graphql_request(
        r#"
        query Loc($id: ID!) { location(id: $id) { id name isFulfillmentService isActive } }
        "#,
        json!({ "id": location_id }),
    ));
    assert_eq!(after_delete.body["data"]["location"], json!(null));

    let unknown_update = proxy.process_request(json_graphql_request(
        r#"
        mutation UnknownUpdate($id: ID!) {
          fulfillmentServiceUpdate(id: $id, name: "Nope") { fulfillmentService { id } userErrors { field message } }
        }
        "#,
        json!({ "id": "gid://shopify/FulfillmentService/999999999999" }),
    ));
    assert_eq!(
        unknown_update.body["data"]["fulfillmentServiceUpdate"],
        json!({
            "fulfillmentService": null,
            "userErrors": [{ "field": ["id"], "message": "Fulfillment service could not be found." }]
        })
    );
}

#[test]
fn fulfillment_service_uniqueness_rejects_name_handle_and_reserved_collisions() {
    let mut proxy = snapshot_proxy();

    let create_query = r#"
        mutation FulfillmentServiceUniquenessCreate($name: String!) {
          fulfillmentServiceCreate(
            name: $name
            trackingSupport: true
            inventoryManagement: true
            requiresShippingMethod: true
          ) {
            fulfillmentService {
              id handle serviceName callbackUrl trackingSupport inventoryManagement requiresShippingMethod type
              location { id name isFulfillmentService fulfillsOnlineOrders shipsInventory }
            }
            userErrors { field message }
          }
        }
    "#;
    let update_query = r#"
        mutation FulfillmentServiceUniquenessUpdate($id: ID!, $name: String!) {
          fulfillmentServiceUpdate(
            id: $id
            name: $name
            trackingSupport: false
            inventoryManagement: false
            requiresShippingMethod: false
          ) {
            fulfillmentService {
              id handle serviceName callbackUrl trackingSupport inventoryManagement requiresShippingMethod type
              location { id name isFulfillmentService fulfillsOnlineOrders shipsInventory }
            }
            userErrors { field message }
          }
        }
    "#;

    let create_a = proxy.process_request(json_graphql_request(
        create_query,
        json!({ "name": "FS Unique Acme fsuniq-mowo6bal" }),
    ));
    let service_a = &create_a.body["data"]["fulfillmentServiceCreate"]["fulfillmentService"];
    assert!(service_a["id"]
        .as_str()
        .unwrap()
        .starts_with("gid://shopify/FulfillmentService/"));
    assert!(service_a["location"]["id"]
        .as_str()
        .unwrap()
        .starts_with("gid://shopify/Location/"));
    assert_eq!(
        service_a,
        &json!({
            "id": service_a["id"],
            "handle": "fs-unique-acme-fsuniq-mowo6bal",
            "serviceName": "FS Unique Acme fsuniq-mowo6bal",
            "callbackUrl": null,
            "trackingSupport": true,
            "inventoryManagement": true,
            "requiresShippingMethod": true,
            "type": "THIRD_PARTY",
            "location": {
                "id": service_a["location"]["id"],
                "name": "FS Unique Acme fsuniq-mowo6bal",
                "isFulfillmentService": true,
                "fulfillsOnlineOrders": true,
                "shipsInventory": false
            }
        })
    );

    for duplicate_name in [
        "FS Unique Acme fsuniq-mowo6bal",
        "FS UNIQUE ACME FSUNIQ-MOWO6BAL",
    ] {
        let duplicate = proxy.process_request(json_graphql_request(
            create_query,
            json!({ "name": duplicate_name }),
        ));
        assert_eq!(
            duplicate.body["data"]["fulfillmentServiceCreate"],
            json!({
                "fulfillmentService": null,
                "userErrors": [{ "field": ["name"], "message": "Name has already been taken" }]
            })
        );
    }

    let spaced = proxy.process_request(json_graphql_request(
        create_query,
        json!({ "name": "FS Unique AB fsuniq-mowo6bal" }),
    ));
    assert_eq!(
        spaced.body["data"]["fulfillmentServiceCreate"]["fulfillmentService"]["handle"],
        json!("fs-unique-ab-fsuniq-mowo6bal")
    );

    let handle_collision = proxy.process_request(json_graphql_request(
        create_query,
        json!({ "name": "fs-unique-ab-fsuniq-mowo6bal" }),
    ));
    assert_eq!(
        handle_collision.body["data"]["fulfillmentServiceCreate"],
        json!({
            "fulfillmentService": null,
            "userErrors": [{ "field": ["name"], "message": "Name has already been taken" }]
        })
    );

    let diacritic = proxy.process_request(json_graphql_request(
        create_query,
        json!({ "name": "FS Unique Café__3PL fsuniq-mowo6bal!!!" }),
    ));
    assert_eq!(
        diacritic.body["data"]["fulfillmentServiceCreate"]["fulfillmentService"]["handle"],
        json!("fs-unique-cafe__3pl-fsuniq-mowo6bal")
    );

    for reserved_name in ["Manual", "Gift_Card"] {
        let reserved = proxy.process_request(json_graphql_request(
            create_query,
            json!({ "name": reserved_name }),
        ));
        assert_eq!(
            reserved.body["data"]["fulfillmentServiceCreate"],
            json!({
                "fulfillmentService": null,
                "userErrors": [{ "field": ["name"], "message": "Name is reserved" }]
            })
        );
    }

    proxy.process_request(json_graphql_request(
        create_query,
        json!({ "name": "FS Unique Source fsuniq-mowo6bal" }),
    ));
    let target = proxy.process_request(json_graphql_request(
        create_query,
        json!({ "name": "FS Unique Target fsuniq-mowo6bal" }),
    ));
    let target_id = target.body["data"]["fulfillmentServiceCreate"]["fulfillmentService"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let update_duplicate = proxy.process_request(json_graphql_request(
        update_query,
        json!({ "id": target_id, "name": "FS Unique Source fsuniq-mowo6bal" }),
    ));
    assert_eq!(
        update_duplicate.body["data"]["fulfillmentServiceUpdate"],
        json!({
            "fulfillmentService": null,
            "userErrors": [{ "field": ["name"], "message": "Name has already been taken" }]
        })
    );

    let update_reserved = proxy.process_request(json_graphql_request(
        update_query,
        json!({ "id": target_id, "name": "Manual" }),
    ));
    assert_eq!(
        update_reserved.body["data"]["fulfillmentServiceUpdate"],
        json!({
            "fulfillmentService": null,
            "userErrors": [{ "field": ["name"], "message": "Name is reserved" }]
        })
    );
}

#[test]
fn carrier_service_lifecycle_stages_reads_filters_deletes_and_validates() {
    let mut proxy = snapshot_proxy();
    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation CarrierServiceCreateProbe($input: DeliveryCarrierServiceCreateInput!) {
          carrierServiceCreate(input: $input) {
            carrierService { id name formattedName callbackUrl active supportsServiceDiscovery }
            userErrors { field message }
          }
        }
        "#,
        json!({ "input": {
            "name": "Hermes Carrier Local",
            "callbackUrl": "https://mock.shop/carrier-service-rates",
            "supportsServiceDiscovery": true,
            "active": false
        }}),
    ));
    let id = create.body["data"]["carrierServiceCreate"]["carrierService"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(id.starts_with("gid://shopify/DeliveryCarrierService/"));
    assert_eq!(
        create.body["data"]["carrierServiceCreate"]["carrierService"]["formattedName"],
        json!("Hermes Carrier Local (Rates provided by app)")
    );

    let update = proxy.process_request(json_graphql_request(
        r#"
        mutation CarrierServiceUpdateProbe($input: DeliveryCarrierServiceUpdateInput!) {
          carrierServiceUpdate(input: $input) {
            carrierService { id name formattedName callbackUrl active supportsServiceDiscovery }
            userErrors { field message }
          }
        }
        "#,
        json!({ "input": {
            "id": id,
            "name": "Hermes Carrier Updated",
            "callbackUrl": "https://mock.shop/carrier-service-rates-updated",
            "supportsServiceDiscovery": false,
            "active": true
        }}),
    ));
    assert_eq!(
        update.body["data"]["carrierServiceUpdate"]["carrierService"]["name"],
        json!("Hermes Carrier Updated")
    );

    let downstream = proxy.process_request(json_graphql_request(
        r#"
        query CarrierServiceAfterUpdate($id: ID!, $first: Int!, $activeQuery: String) {
          carrierService(id: $id) { id name formattedName callbackUrl active supportsServiceDiscovery }
          active: carrierServices(first: $first, query: $activeQuery, sortKey: ID) {
            nodes { id name formattedName callbackUrl active supportsServiceDiscovery }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
        }
        "#,
        json!({ "id": id, "first": 5, "activeQuery": "active:true" }),
    ));
    assert_eq!(
        downstream.body["data"]["carrierService"]["active"],
        json!(true)
    );
    assert_eq!(
        downstream.body["data"]["active"]["nodes"]
            .as_array()
            .unwrap()
            .len(),
        1
    );

    let delete = proxy.process_request(json_graphql_request(
        r#"
        mutation CarrierServiceDeleteProbe($id: ID!) {
          carrierServiceDelete(id: $id) { deletedId userErrors { field message } }
        }
        "#,
        json!({ "id": id }),
    ));
    assert_eq!(
        delete.body["data"]["carrierServiceDelete"]["userErrors"],
        json!([])
    );

    let missing = proxy.process_request(json_graphql_request(
        r#"
        mutation CarrierServiceDeleteProbe($id: ID!) {
          carrierServiceDelete(id: $id) { deletedId userErrors { field message } }
        }
        "#,
        json!({ "id": "gid://shopify/DeliveryCarrierService/999999999999" }),
    ));
    assert_eq!(
        missing.body["data"]["carrierServiceDelete"]["userErrors"][0]["message"],
        json!("The carrier or app could not be found.")
    );
}

#[test]
fn delivery_settings_roots_return_read_only_settings_with_aliases_and_selected_fields() {
    let mut proxy = snapshot_proxy();
    let response = proxy.process_request(json_graphql_request(
        r#"
        query DeliverySettingsRead {
          deliverySettingsAlias: deliverySettings {
            legacyModeProfiles
            legacyModeBlocked { blocked reasons }
          }
          deliveryPromiseSettingsAlias: deliveryPromiseSettings {
            deliveryDatesEnabled
            processingTime
          }
        }
        "#,
        json!({}),
    ));

    assert_eq!(response.status, 200);
    assert_eq!(
        response.body,
        json!({
            "data": {
                "deliverySettingsAlias": {
                    "legacyModeProfiles": false,
                    "legacyModeBlocked": { "blocked": false, "reasons": null }
                },
                "deliveryPromiseSettingsAlias": {
                    "deliveryDatesEnabled": false,
                    "processingTime": null
                }
            }
        })
    );
}

#[test]
fn shipping_package_lifecycle_stages_state_defaults_deletes_and_log_order() {
    let mut proxy = snapshot_proxy();
    let update_query = r#"
        mutation ShippingPackageUpdateLocalRuntime($id: ID!, $shippingPackage: CustomShippingPackageInput!) {
          shippingPackageUpdate(id: $id, shippingPackage: $shippingPackage) { userErrors { field message } }
        }
    "#;
    let make_default_query = r#"
        mutation ShippingPackageMakeDefaultLocalRuntime($id: ID!) {
          shippingPackageMakeDefault(id: $id) { userErrors { field message } }
        }
    "#;
    let delete_query = r#"
        mutation ShippingPackageDeleteLocalRuntime($id: ID!) {
          shippingPackageDelete(id: $id) { deletedId userErrors { field message } }
        }
    "#;

    let update = proxy.process_request(json_graphql_request(
        update_query,
        json!({
            "id": "gid://shopify/ShippingPackage/1",
            "shippingPackage": {
                "name": "Updated box",
                "type": "BOX",
                "default": true,
                "weight": { "value": 2.5, "unit": "POUNDS" },
                "dimensions": { "length": 12, "width": 9, "height": 5, "unit": "INCHES" }
            }
        }),
    ));
    assert_eq!(
        update.body["data"]["shippingPackageUpdate"],
        json!({ "userErrors": [] })
    );
    assert_eq!(
        proxy.get_state_snapshot()["stagedState"]["shippingPackages"]
            ["gid://shopify/ShippingPackage/1"]["updatedAt"],
        json!("2024-01-01T00:00:01.000Z")
    );

    let make_default = proxy.process_request(json_graphql_request(
        make_default_query,
        json!({ "id": "gid://shopify/ShippingPackage/2" }),
    ));
    assert_eq!(
        make_default.body["data"]["shippingPackageMakeDefault"],
        json!({ "userErrors": [] })
    );
    let state = proxy.get_state_snapshot();
    assert_eq!(
        state["stagedState"]["shippingPackages"]["gid://shopify/ShippingPackage/1"]["default"],
        json!(false)
    );
    assert_eq!(
        state["stagedState"]["shippingPackages"]["gid://shopify/ShippingPackage/2"]["default"],
        json!(true)
    );

    let restore = proxy.process_request(json_graphql_request(
        update_query,
        json!({
            "id": "gid://shopify/ShippingPackage/1",
            "shippingPackage": { "default": true }
        }),
    ));
    assert_eq!(
        restore.body["data"]["shippingPackageUpdate"],
        json!({ "userErrors": [] })
    );
    let state = proxy.get_state_snapshot();
    assert_eq!(
        state["stagedState"]["shippingPackages"]["gid://shopify/ShippingPackage/1"]["default"],
        json!(true)
    );
    assert_eq!(
        state["stagedState"]["shippingPackages"]["gid://shopify/ShippingPackage/2"]["default"],
        json!(false)
    );

    let delete = proxy.process_request(json_graphql_request(
        delete_query,
        json!({ "id": "gid://shopify/ShippingPackage/1" }),
    ));
    assert_eq!(
        delete.body["data"]["shippingPackageDelete"],
        json!({ "deletedId": "gid://shopify/ShippingPackage/1", "userErrors": [] })
    );
    let state = proxy.get_state_snapshot();
    assert_eq!(
        state["stagedState"]["deletedShippingPackageIds"]["gid://shopify/ShippingPackage/1"],
        json!(true)
    );
    assert!(state["stagedState"]["shippingPackages"]
        .get("gid://shopify/ShippingPackage/1")
        .is_none());

    let log = proxy.get_log_snapshot();
    assert_eq!(
        log["entries"][0]["operationName"],
        json!("shippingPackageUpdate")
    );
    assert_eq!(
        log["entries"][1]["operationName"],
        json!("shippingPackageMakeDefault")
    );
    assert_eq!(
        log["entries"][2]["operationName"],
        json!("shippingPackageUpdate")
    );
    assert_eq!(
        log["entries"][3]["operationName"],
        json!("shippingPackageDelete")
    );
    assert_eq!(log["entries"][3]["status"], json!("staged"));
}

#[test]
fn shipping_package_update_rejects_flat_rate_packages_without_staging_state() {
    let mut proxy = snapshot_proxy();
    let update_query = r#"
        mutation ShippingPackageUpdateFlatRate($id: ID!, $shippingPackage: CustomShippingPackageInput!) {
          shippingPackageUpdate(id: $id, shippingPackage: $shippingPackage) { userErrors { field message code } }
        }
    "#;

    let response = proxy.process_request(json_graphql_request(
        update_query,
        json!({
            "id": "gid://shopify/ShippingPackage/10",
            "shippingPackage": {
                "dimensions": { "length": 999, "width": 8, "height": 4, "unit": "CENTIMETERS" }
            }
        }),
    ));

    assert_eq!(
        response.body["data"]["shippingPackageUpdate"],
        json!({
            "userErrors": [{
                "field": ["shippingPackage"],
                "message": "Custom shipping box is not updatable",
                "code": "CUSTOM_SHIPPING_BOX_NOT_UPDATABLE"
            }]
        })
    );
    assert_eq!(
        proxy.get_state_snapshot()["stagedState"]["shippingPackages"],
        json!({})
    );
}

#[test]
fn product_create_preserves_parity_fields_and_downstream_read() {
    let mut proxy = snapshot_proxy();
    let create_query = r#"
        mutation ProductCreateParityPlan($product: ProductCreateInput!) {
          productCreate(product: $product) {
            product {
              id
              title
              handle
              status
              vendor
              productType
              tags
              descriptionHtml
              templateSuffix
              seo { title description }
            }
            userErrors { field message }
          }
        }
    "#;
    let variables = json!({
        "product": {
            "title": "Hermes Product Conformance 1776299742511",
            "status": "DRAFT",
            "vendor": "HERMES",
            "productType": "ACCESSORIES",
            "tags": ["conformance", "product-mutation", "1776299742511"],
            "descriptionHtml": "<p>Hermes product mutation conformance 1776299742511</p>",
            "templateSuffix": "product-mutation-parity",
            "seo": {
                "title": "Hermes Product 1776299742511",
                "description": "Hermes product mutation conformance 1776299742511"
            }
        }
    });

    let create = proxy.process_request(json_graphql_request(create_query, variables));
    let id = create.body["data"]["productCreate"]["product"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    assert_eq!(
        create.body["data"]["productCreate"]["product"],
        json!({
            "id": id,
            "title": "Hermes Product Conformance 1776299742511",
            "handle": "hermes-product-conformance-1776299742511",
            "status": "DRAFT",
            "vendor": "HERMES",
            "productType": "ACCESSORIES",
            "tags": ["1776299742511", "conformance", "product-mutation"],
            "descriptionHtml": "<p>Hermes product mutation conformance 1776299742511</p>",
            "templateSuffix": "product-mutation-parity",
            "seo": {
                "title": "Hermes Product 1776299742511",
                "description": "Hermes product mutation conformance 1776299742511"
            }
        })
    );

    let read_query = r#"
        query ProductCreateDownstreamRead($id: ID!) {
          product(id: $id) {
            id
            title
            handle
            status
            vendor
            productType
            tags
            descriptionHtml
            templateSuffix
            seo { title description }
          }
        }
    "#;
    let read = proxy.process_request(json_graphql_request(read_query, json!({ "id": id })));
    assert_eq!(
        read.body["data"]["product"],
        create.body["data"]["productCreate"]["product"]
    );
}

#[test]
fn admin_graphql_path_is_post_only() {
    let mut proxy = snapshot_proxy();

    let response = proxy.process_request(graphql_request("GET", ""));

    assert_eq!(response.status, 405);
    assert_eq!(
        response.body,
        json!({ "errors": [{ "message": "Method not allowed" }] })
    );
}

#[test]
fn admin_graphql_rejects_non_json_or_missing_query_bodies() {
    let mut proxy = snapshot_proxy();

    let non_json = proxy.process_request(graphql_request("POST", "not json"));
    assert_eq!(non_json.status, 400);
    assert_eq!(
        non_json.body,
        json!({ "errors": [{ "message": "Expected JSON body with a string `query`" }] })
    );

    let missing_query = proxy.process_request(graphql_request("POST", r#"{"variables":{}}"#));
    assert_eq!(missing_query.status, 400);
    assert_eq!(
        missing_query.body,
        json!({ "errors": [{ "message": "Expected JSON body with a string `query`" }] })
    );
}

#[test]
fn admin_graphql_reports_parse_and_dispatch_errors_with_existing_envelopes() {
    let mut proxy = snapshot_proxy();

    let parse_error = proxy.process_request(graphql_request("POST", r#"{"query":""}"#));
    assert_eq!(parse_error.status, 400);
    assert_eq!(
        parse_error.body,
        json!({ "errors": [{ "message": "Could not parse GraphQL operation" }] })
    );

    let unknown_query = proxy.process_request(graphql_request(
        "POST",
        r#"{"query":"query Named { definitelyUnknownRoot { id } }"}"#,
    ));
    assert_eq!(unknown_query.status, 400);
    assert_eq!(
        unknown_query.body,
        json!({ "errors": [{ "message": "No domain dispatcher implemented for root field: definitelyUnknownRoot" }] })
    );

    let unknown_mutation = proxy.process_request(graphql_request(
        "POST",
        r#"{"query":"mutation { definitelyUnknownMutation { ok } }"}"#,
    ));
    assert_eq!(unknown_mutation.status, 400);
    assert_eq!(
        unknown_mutation.body,
        json!({ "errors": [{ "message": "No mutation dispatcher implemented for root field: definitelyUnknownMutation" }] })
    );
}

#[test]
fn admin_graphql_routes_by_root_field_not_alias_or_fragment_definition() {
    let mut proxy = snapshot_proxy();

    let aliased_query = proxy.process_request(graphql_request(
        "POST",
        r#"{"query":"query Named { visibleAlias: definitelyUnknownRoot { id } }"}"#,
    ));
    assert_eq!(aliased_query.status, 400);
    assert_eq!(
        aliased_query.body,
        json!({ "errors": [{ "message": "No domain dispatcher implemented for root field: definitelyUnknownRoot" }] })
    );

    let fragment_before_operation = proxy.process_request(graphql_request(
        "POST",
        r#"{"query":"fragment Fields on Product { id } query Named { definitelyUnknownRoot { ...Fields } }"}"#,
    ));
    assert_eq!(fragment_before_operation.status, 400);
    assert_eq!(
        fragment_before_operation.body,
        json!({ "errors": [{ "message": "No domain dispatcher implemented for root field: definitelyUnknownRoot" }] })
    );
}

#[test]
fn live_hybrid_forwards_unknown_queries_to_upstream_transport() {
    let forwarded = Arc::new(Mutex::new(Vec::<Request>::new()));
    let captured = Arc::clone(&forwarded);
    let mut proxy = configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport(move |request| {
        captured.lock().unwrap().push(request);
        shopify_draft_proxy::proxy::Response {
            status: 202,
            headers: [("x-test-upstream".to_string(), "domain-read".to_string())].into(),
            body: json!({ "data": { "currentAppInstallation": { "id": "gid://shopify/AppInstallation/42" } } }),
        }
    });

    let response = proxy.process_request(Request {
        method: "POST".to_string(),
        path: "/admin/api/2026-04/graphql.json".to_string(),
        headers: [(
            "authorization".to_string(),
            "Bearer passthrough-token".to_string(),
        )]
        .into(),
        body: json!({ "query": "{ currentAppInstallation { id } }" }).to_string(),
    });

    assert_eq!(response.status, 202);
    assert_eq!(
        response.body,
        json!({ "data": { "currentAppInstallation": { "id": "gid://shopify/AppInstallation/42" } } })
    );
    assert_eq!(
        response.headers.get("x-test-upstream"),
        Some(&"domain-read".to_string())
    );
    let forwarded = forwarded.lock().unwrap();
    assert_eq!(forwarded.len(), 1);
    assert_eq!(
        forwarded[0].headers.get("authorization"),
        Some(&"Bearer passthrough-token".to_string())
    );
    assert_eq!(
        forwarded[0].body,
        json!({ "query": "{ currentAppInstallation { id } }" }).to_string()
    );
}

#[test]
fn unknown_mutation_passthrough_observability_and_reject_mode_are_preserved() {
    let hits = Arc::new(Mutex::new(0usize));
    let hit_counter = Arc::clone(&hits);
    let mut passthrough = configured_proxy(
        ReadMode::LiveHybrid,
        Some(shopify_draft_proxy::proxy::UnsupportedMutationMode::Passthrough),
    )
    .with_upstream_transport(move |_request| {
        *hit_counter.lock().unwrap() += 1;
        shopify_draft_proxy::proxy::Response {
            status: 200,
            headers: Default::default(),
            body: json!({ "data": { "definitelyUnsupportedMutation": { "ok": true } } }),
        }
    });

    let passthrough_response = passthrough.process_request(graphql_request(
        "POST",
        &json!({ "query": "mutation { definitelyUnsupportedMutation { ok } }" }).to_string(),
    ));

    assert_eq!(passthrough_response.status, 200);
    assert_eq!(
        passthrough_response.body,
        json!({ "data": { "definitelyUnsupportedMutation": { "ok": true } } })
    );
    assert_eq!(*hits.lock().unwrap(), 1);
    assert_eq!(
        passthrough.get_log_snapshot(),
        json!({
            "entries": [{
                "id": "log-1",
                "operationName": "definitelyUnsupportedMutation",
                "status": "proxied",
                "path": "/admin/api/2026-04/graphql.json",
                "query": "mutation { definitelyUnsupportedMutation { ok } }",
                "variables": {},
                "interpreted": {
                    "operationType": "mutation",
                    "rootFields": ["definitelyUnsupportedMutation"],
                    "primaryRootField": "definitelyUnsupportedMutation",
                    "capability": {
                        "operationName": "definitelyUnsupportedMutation",
                        "domain": "unknown",
                        "execution": "passthrough"
                    }
                },
                "notes": "Mutation passthrough placeholder until supported local staging is implemented."
            }]
        })
    );

    let reject_hits = Arc::new(Mutex::new(0usize));
    let reject_counter = Arc::clone(&reject_hits);
    let mut reject = configured_proxy(
        ReadMode::LiveHybrid,
        Some(shopify_draft_proxy::proxy::UnsupportedMutationMode::Reject),
    )
    .with_upstream_transport(move |_request| {
        *reject_counter.lock().unwrap() += 1;
        shopify_draft_proxy::proxy::Response {
            status: 500,
            headers: Default::default(),
            body: json!({ "errors": [{ "message": "should not hit upstream" }] }),
        }
    });

    let reject_response = reject.process_request(graphql_request(
        "POST",
        &json!({ "query": "mutation { definitelyUnsupportedMutation { ok } }" }).to_string(),
    ));

    assert_eq!(reject_response.status, 400);
    assert_eq!(
        reject_response.body,
        json!({ "errors": [{ "message": "Unsupported mutation rejected by configuration: definitelyUnsupportedMutation" }] })
    );
    assert_eq!(*reject_hits.lock().unwrap(), 0);
}

#[test]
fn standard_proxy_construction_attaches_default_registry_for_core_roots() {
    let mut proxy = snapshot_proxy();

    let product = proxy.process_request(graphql_request(
        "POST",
        r#"{"query":"query { product(id: \"gid://shopify/Product/1\") { id } }"}"#,
    ));

    assert_eq!(product.status, 200);
    assert_eq!(product.body, json!({ "data": { "product": null } }));
}

#[test]
fn product_read_serializes_seeded_base_product_by_id() {
    let mut proxy = snapshot_proxy().with_base_products(vec![ProductRecord {
        id: "gid://shopify/Product/1".to_string(),
        title: "Seeded product".to_string(),
        handle: "seeded-product".to_string(),
        status: "ACTIVE".to_string(),
        description_html: String::new(),
        vendor: String::new(),
        product_type: String::new(),
        tags: Vec::new(),
        template_suffix: String::new(),
        seo_title: String::new(),
        seo_description: String::new(),
    }]);

    let product = proxy.process_request(graphql_request(
        "POST",
        r#"{"query":"query { product(id: \"gid://shopify/Product/1\") { id title handle status } }"}"#,
    ));

    assert_eq!(product.status, 200);
    assert_eq!(
        product.body,
        json!({
            "data": {
                "product": {
                    "id": "gid://shopify/Product/1",
                    "title": "Seeded product",
                    "handle": "seeded-product",
                    "status": "ACTIVE"
                }
            }
        })
    );
}

#[test]
fn product_read_serializes_only_requested_scalar_fields() {
    let mut proxy = snapshot_proxy().with_base_products(vec![ProductRecord {
        id: "gid://shopify/Product/1".to_string(),
        title: "Seeded product".to_string(),
        handle: "seeded-product".to_string(),
        status: "ACTIVE".to_string(),
        description_html: String::new(),
        vendor: String::new(),
        product_type: String::new(),
        tags: Vec::new(),
        template_suffix: String::new(),
        seo_title: String::new(),
        seo_description: String::new(),
    }]);

    let product = proxy.process_request(graphql_request(
        "POST",
        r#"{"query":"query { product(id: \"gid://shopify/Product/1\") { handle } }"}"#,
    ));

    assert_eq!(product.status, 200);
    assert_eq!(
        product.body,
        json!({
            "data": {
                "product": {
                    "handle": "seeded-product"
                }
            }
        })
    );
}

#[test]
fn product_read_preserves_root_alias() {
    let mut proxy = snapshot_proxy().with_base_products(vec![ProductRecord {
        id: "gid://shopify/Product/1".to_string(),
        title: "Seeded product".to_string(),
        handle: "seeded-product".to_string(),
        status: "ACTIVE".to_string(),
        description_html: String::new(),
        vendor: String::new(),
        product_type: String::new(),
        tags: Vec::new(),
        template_suffix: String::new(),
        seo_title: String::new(),
        seo_description: String::new(),
    }]);

    let product = proxy.process_request(graphql_request(
        "POST",
        r#"{"query":"query { selectedProduct: product(id: \"gid://shopify/Product/1\") { id title } }"}"#,
    ));

    assert_eq!(product.status, 200);
    assert_eq!(
        product.body,
        json!({
            "data": {
                "selectedProduct": {
                    "id": "gid://shopify/Product/1",
                    "title": "Seeded product"
                }
            }
        })
    );
}

#[test]
fn collection_publishable_mutations_stage_publication_state_for_downstream_reads() {
    let mut proxy = snapshot_proxy();
    let variables = json!({
        "id": "gid://shopify/Collection/468787757289",
        "input": [{ "publicationId": "gid://shopify/Publication/82090459369" }],
        "publicationId": "gid://shopify/Publication/82090459369"
    });
    let publish_response = proxy.process_request(json_graphql_request(
        r#"
        mutation CollectionPublishablePublish($id: ID!, $input: [PublicationInput!]!, $publicationId: ID!) {
          publishablePublish(id: $id, input: $input) {
            publishable { ... on Collection { id title handle publishedOnCurrentPublication publishedOnPublication(publicationId: $publicationId) availablePublicationsCount { count precision } resourcePublicationsCount { count precision } } }
            userErrors { field message }
          }
        }
        "#,
        variables.clone(),
    ));
    assert_eq!(
        publish_response.body["data"]["publishablePublish"]["publishable"],
        json!({
            "id": "gid://shopify/Collection/468787757289",
            "title": "Hermes Collection Conformance 1777078204269",
            "handle": "hermes-collection-conformance-1777078204269",
            "publishedOnCurrentPublication": false,
            "publishedOnPublication": true,
            "availablePublicationsCount": { "count": 1, "precision": "EXACT" },
            "resourcePublicationsCount": { "count": 1, "precision": "EXACT" }
        })
    );
    assert_eq!(
        publish_response.body["data"]["publishablePublish"]["userErrors"],
        json!([])
    );

    let read_query = r#"
        query CollectionPublicationRead($id: ID!, $publicationId: ID!) {
          collection(id: $id) {
            id title handle publishedOnCurrentPublication publishedOnPublication(publicationId: $publicationId)
            availablePublicationsCount { count precision }
            resourcePublicationsCount { count precision }
          }
        }
    "#;
    let read_after_publish =
        proxy.process_request(json_graphql_request(read_query, variables.clone()));
    assert_eq!(
        read_after_publish.body["data"]["collection"],
        publish_response.body["data"]["publishablePublish"]["publishable"]
    );

    let unpublish_response = proxy.process_request(json_graphql_request(
        r#"
        mutation CollectionPublishableUnpublish($id: ID!, $input: [PublicationInput!]!, $publicationId: ID!) {
          publishableUnpublish(id: $id, input: $input) {
            publishable { ... on Collection { id title handle publishedOnCurrentPublication publishedOnPublication(publicationId: $publicationId) availablePublicationsCount { count precision } resourcePublicationsCount { count precision } } }
            userErrors { field message }
          }
        }
        "#,
        variables.clone(),
    ));
    assert_eq!(
        unpublish_response.body["data"]["publishableUnpublish"]["publishable"],
        json!({
            "id": "gid://shopify/Collection/468787757289",
            "title": "Hermes Collection Conformance 1777078204269",
            "handle": "hermes-collection-conformance-1777078204269",
            "publishedOnCurrentPublication": false,
            "publishedOnPublication": false,
            "availablePublicationsCount": { "count": 0, "precision": "EXACT" },
            "resourcePublicationsCount": { "count": 0, "precision": "EXACT" }
        })
    );
    let read_after_unpublish = proxy.process_request(json_graphql_request(read_query, variables));
    assert_eq!(
        read_after_unpublish.body["data"]["collection"],
        unpublish_response.body["data"]["publishableUnpublish"]["publishable"]
    );
}

#[test]
fn product_publishable_mutations_return_captured_aggregate_shape() {
    let mut proxy = snapshot_proxy();
    for (root, query) in [
        (
            "publishablePublish",
            r#"
            mutation PublishablePublishProductParity($id: ID!, $input: [PublicationInput!]!) {
              publishablePublish(id: $id, input: $input) {
                publishable { ... on Product { id publishedOnCurrentPublication availablePublicationsCount { count precision } resourcePublicationsCount { count precision } } }
                userErrors { field message }
              }
            }
            "#,
        ),
        (
            "publishableUnpublish",
            r#"
            mutation PublishableUnpublishProductParity($id: ID!, $input: [PublicationInput!]!) {
              publishableUnpublish(id: $id, input: $input) {
                publishable { ... on Product { id publishedOnCurrentPublication availablePublicationsCount { count precision } resourcePublicationsCount { count precision } } }
                userErrors { field message }
              }
            }
            "#,
        ),
        (
            "publishablePublishToCurrentChannel",
            r#"
            mutation PublishablePublishToCurrentChannelProductParity($id: ID!) {
              publishablePublishToCurrentChannel(id: $id) {
                publishable { ... on Product { id publishedOnCurrentPublication availablePublicationsCount { count precision } resourcePublicationsCount { count precision } } }
                userErrors { field message }
              }
            }
            "#,
        ),
        (
            "publishableUnpublishToCurrentChannel",
            r#"
            mutation PublishableUnpublishToCurrentChannelProductParity($id: ID!) {
              publishableUnpublishToCurrentChannel(id: $id) {
                publishable { ... on Product { id publishedOnCurrentPublication availablePublicationsCount { count precision } resourcePublicationsCount { count precision } } }
                userErrors { field message }
              }
            }
            "#,
        ),
    ] {
        let response = proxy.process_request(json_graphql_request(
            query,
            json!({
                "id": "gid://shopify/Product/9264105488617",
                "input": [{ "publicationId": "gid://shopify/Publication/82090459369" }]
            }),
        ));
        assert_eq!(
            response.body["data"][root]["publishable"],
            json!({
                "id": "gid://shopify/Product/9264105488617",
                "publishedOnCurrentPublication": false,
                "availablePublicationsCount": { "count": 0, "precision": "EXACT" },
                "resourcePublicationsCount": { "count": 0, "precision": "EXACT" }
            })
        );
        assert_eq!(response.body["data"][root]["userErrors"], json!([]));
    }
}

#[test]
fn product_create_blank_title_user_errors_match_public_shape_and_selected_fields() {
    let mut proxy = snapshot_proxy();

    let public_shape = proxy.process_request(json_graphql_request(
        r#"
        mutation ProductUserErrorShapeProductCreate($product: ProductCreateInput!) {
          productCreate(product: $product) {
            product { id }
            userErrors { field message }
          }
        }
        "#,
        json!({ "product": { "title": "" } }),
    ));
    assert_eq!(
        public_shape.body["data"]["productCreate"],
        json!({
            "product": null,
            "userErrors": [{ "field": ["title"], "message": "Title can't be blank" }]
        })
    );

    let local_code_projection = proxy.process_request(json_graphql_request(
        r#"
        mutation ProductCreateBlankTitleWithCode($product: ProductCreateInput!) {
          productCreate(product: $product) {
            product { id }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "product": { "title": "" } }),
    ));
    assert_eq!(
        local_code_projection.body["data"]["productCreate"],
        json!({
            "product": null,
            "userErrors": [{ "field": ["title"], "message": "Title can't be blank", "code": "BLANK" }]
        })
    );
}

#[test]
fn product_create_serializes_only_requested_payload_fields() {
    let mut proxy = snapshot_proxy();

    let create = proxy.process_request(graphql_request(
        "POST",
        r#"{"query":"mutation { productCreate(product: { title: \"Selection product\" }) { product { title } } }"}"#,
    ));

    assert_eq!(create.status, 200);
    assert_eq!(
        create.body,
        json!({
            "data": {
                "productCreate": {
                    "product": {
                        "title": "Selection product"
                    }
                }
            }
        })
    );
}

#[test]
fn product_create_stages_extended_product_scalars_visible_to_product_read() {
    let mut proxy = snapshot_proxy();

    let create = proxy.process_request(graphql_request(
        "POST",
        r#"{"query":"mutation { productCreate(product: { title: \"Extended product\", descriptionHtml: \"<p>Rich</p>\", vendor: \"Hermes\", productType: \"Accessory\", tags: [\"alpha\", \"beta\"] }) { product { title descriptionHtml vendor productType tags } userErrors { field message code } } }"}"#,
    ));

    assert_eq!(create.status, 200);
    assert_eq!(
        create.body,
        json!({
            "data": {
                "productCreate": {
                    "product": {
                        "title": "Extended product",
                        "descriptionHtml": "<p>Rich</p>",
                        "vendor": "Hermes",
                        "productType": "Accessory",
                        "tags": ["alpha", "beta"]
                    },
                    "userErrors": []
                }
            }
        })
    );

    let read_back = proxy.process_request(graphql_request(
        "POST",
        r#"{"query":"query { product(id: \"gid://shopify/Product/1?shopify-draft-proxy=synthetic\") { descriptionHtml vendor productType tags } }"}"#,
    ));

    assert_eq!(read_back.status, 200);
    assert_eq!(
        read_back.body,
        json!({
            "data": {
                "product": {
                    "descriptionHtml": "<p>Rich</p>",
                    "vendor": "Hermes",
                    "productType": "Accessory",
                    "tags": ["alpha", "beta"]
                }
            }
        })
    );
}

#[test]
fn product_update_stages_scalar_changes_visible_to_product_read() {
    let mut proxy = snapshot_proxy().with_base_products(vec![ProductRecord {
        id: "gid://shopify/Product/1".to_string(),
        title: "Original product".to_string(),
        handle: "original-product".to_string(),
        status: "ACTIVE".to_string(),
        description_html: "<p>Original</p>".to_string(),
        vendor: "Original vendor".to_string(),
        product_type: "Original type".to_string(),
        tags: vec!["old".to_string()],
        template_suffix: String::new(),
        seo_title: String::new(),
        seo_description: String::new(),
    }]);

    let update = proxy.process_request(graphql_request(
        "POST",
        r#"{"query":"mutation { productUpdate(product: { id: \"gid://shopify/Product/1\", title: \"Updated product\", handle: \"updated-product\", status: DRAFT, descriptionHtml: \"<p>Updated</p>\", vendor: \"Hermes\", productType: \"Accessory\", tags: [\"alpha\", \"beta\"] }) { product { id title handle status descriptionHtml vendor productType tags } userErrors { field message code } } }"}"#,
    ));

    assert_eq!(update.status, 200);
    assert_eq!(
        update.body,
        json!({
            "data": {
                "productUpdate": {
                    "product": {
                        "id": "gid://shopify/Product/1",
                        "title": "Updated product",
                        "handle": "updated-product",
                        "status": "DRAFT",
                        "descriptionHtml": "<p>Updated</p>",
                        "vendor": "Hermes",
                        "productType": "Accessory",
                        "tags": ["alpha", "beta"]
                    },
                    "userErrors": []
                }
            }
        })
    );

    let read_back = proxy.process_request(graphql_request(
        "POST",
        r#"{"query":"query { product(id: \"gid://shopify/Product/1\") { title handle status descriptionHtml vendor productType tags } }"}"#,
    ));

    assert_eq!(read_back.status, 200);
    assert_eq!(
        read_back.body,
        json!({
            "data": {
                "product": {
                    "title": "Updated product",
                    "handle": "updated-product",
                    "status": "DRAFT",
                    "descriptionHtml": "<p>Updated</p>",
                    "vendor": "Hermes",
                    "productType": "Accessory",
                    "tags": ["alpha", "beta"]
                }
            }
        })
    );
}

#[test]
fn products_connection_reflects_staged_creates_and_deletes() {
    let mut proxy = snapshot_proxy().with_base_products(vec![ProductRecord {
        id: "gid://shopify/Product/base".to_string(),
        title: "Base product".to_string(),
        handle: "base-product".to_string(),
        status: "ACTIVE".to_string(),
        description_html: String::new(),
        vendor: String::new(),
        product_type: String::new(),
        tags: Vec::new(),
        template_suffix: String::new(),
        seo_title: String::new(),
        seo_description: String::new(),
    }]);

    let create = proxy.process_request(graphql_request(
        "POST",
        r#"{"query":"mutation { productCreate(product: { title: \"Created product\", handle: \"created-product\" }) { product { id } userErrors { field message code } } }"}"#,
    ));
    assert_eq!(create.status, 200);

    let list_after_create = proxy.process_request(graphql_request(
        "POST",
        r#"{"query":"query { products(first: 10) { nodes { id title handle } } }"}"#,
    ));
    assert_eq!(list_after_create.status, 200);
    assert_eq!(
        list_after_create.body,
        json!({
            "data": {
                "products": {
                    "nodes": [
                        {
                            "id": "gid://shopify/Product/base",
                            "title": "Base product",
                            "handle": "base-product"
                        },
                        {
                            "id": "gid://shopify/Product/1?shopify-draft-proxy=synthetic",
                            "title": "Created product",
                            "handle": "created-product"
                        }
                    ]
                }
            }
        })
    );

    let delete = proxy.process_request(graphql_request(
        "POST",
        r#"{"query":"mutation { productDelete(input: { id: \"gid://shopify/Product/base\" }) { deletedProductId userErrors { field message code } } }"}"#,
    ));
    assert_eq!(delete.status, 200);

    let list_after_delete = proxy.process_request(graphql_request(
        "POST",
        r#"{"query":"query { products(first: 10) { nodes { id title handle } } }"}"#,
    ));
    assert_eq!(list_after_delete.status, 200);
    assert_eq!(
        list_after_delete.body,
        json!({
            "data": {
                "products": {
                    "nodes": [
                        {
                            "id": "gid://shopify/Product/1?shopify-draft-proxy=synthetic",
                            "title": "Created product",
                            "handle": "created-product"
                        }
                    ]
                }
            }
        })
    );
}

#[test]
fn products_connection_applies_first_limit_after_overlaying_state() {
    let mut proxy = snapshot_proxy().with_base_products(vec![
        ProductRecord {
            id: "gid://shopify/Product/1".to_string(),
            title: "First product".to_string(),
            handle: "first-product".to_string(),
            status: "ACTIVE".to_string(),
            description_html: String::new(),
            vendor: String::new(),
            product_type: String::new(),
            tags: Vec::new(),
            template_suffix: String::new(),
            seo_title: String::new(),
            seo_description: String::new(),
        },
        ProductRecord {
            id: "gid://shopify/Product/2".to_string(),
            title: "Second product".to_string(),
            handle: "second-product".to_string(),
            status: "ACTIVE".to_string(),
            description_html: String::new(),
            vendor: String::new(),
            product_type: String::new(),
            tags: Vec::new(),
            template_suffix: String::new(),
            seo_title: String::new(),
            seo_description: String::new(),
        },
    ]);

    let first_only = proxy.process_request(graphql_request(
        "POST",
        r#"{"query":"query { products(first: 1) { nodes { id title } } }"}"#,
    ));

    assert_eq!(first_only.status, 200);
    assert_eq!(
        first_only.body,
        json!({
            "data": {
                "products": {
                    "nodes": [
                        {
                            "id": "gid://shopify/Product/1",
                            "title": "First product"
                        }
                    ]
                }
            }
        })
    );
}

#[test]
fn products_connection_serializes_edges_and_page_info_for_selected_window() {
    let mut proxy = snapshot_proxy().with_base_products(vec![
        ProductRecord {
            id: "gid://shopify/Product/1".to_string(),
            title: "First product".to_string(),
            handle: "first-product".to_string(),
            status: "ACTIVE".to_string(),
            description_html: String::new(),
            vendor: String::new(),
            product_type: String::new(),
            tags: Vec::new(),
            template_suffix: String::new(),
            seo_title: String::new(),
            seo_description: String::new(),
        },
        ProductRecord {
            id: "gid://shopify/Product/2".to_string(),
            title: "Second product".to_string(),
            handle: "second-product".to_string(),
            status: "ACTIVE".to_string(),
            description_html: String::new(),
            vendor: String::new(),
            product_type: String::new(),
            tags: Vec::new(),
            template_suffix: String::new(),
            seo_title: String::new(),
            seo_description: String::new(),
        },
    ]);

    let connection = proxy.process_request(graphql_request(
        "POST",
        r#"{"query":"query { products(first: 2) { edges { cursor node { id title } } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } }"}"#,
    ));

    assert_eq!(connection.status, 200);
    assert_eq!(
        connection.body,
        json!({
            "data": {
                "products": {
                    "edges": [
                        {
                            "cursor": "gid://shopify/Product/1",
                            "node": {
                                "id": "gid://shopify/Product/1",
                                "title": "First product"
                            }
                        },
                        {
                            "cursor": "gid://shopify/Product/2",
                            "node": {
                                "id": "gid://shopify/Product/2",
                                "title": "Second product"
                            }
                        }
                    ],
                    "pageInfo": {
                        "hasNextPage": false,
                        "hasPreviousPage": false,
                        "startCursor": "gid://shopify/Product/1",
                        "endCursor": "gid://shopify/Product/2"
                    }
                }
            }
        })
    );
}

#[test]
fn products_count_reflects_staged_creates_and_deletes() {
    let mut proxy = snapshot_proxy().with_base_products(vec![ProductRecord {
        id: "gid://shopify/Product/base".to_string(),
        title: "Base product".to_string(),
        handle: "base-product".to_string(),
        status: "ACTIVE".to_string(),
        description_html: String::new(),
        vendor: String::new(),
        product_type: String::new(),
        tags: Vec::new(),
        template_suffix: String::new(),
        seo_title: String::new(),
        seo_description: String::new(),
    }]);

    let create = proxy.process_request(graphql_request(
        "POST",
        r#"{"query":"mutation { productCreate(product: { title: \"Created product\" }) { product { id } } }"}"#,
    ));
    assert_eq!(create.status, 200);

    let count_after_create = proxy.process_request(graphql_request(
        "POST",
        r#"{"query":"query { productsCount { count precision } }"}"#,
    ));
    assert_eq!(count_after_create.status, 200);
    assert_eq!(
        count_after_create.body,
        json!({
            "data": {
                "productsCount": {
                    "count": 2,
                    "precision": "EXACT"
                }
            }
        })
    );

    let delete = proxy.process_request(graphql_request(
        "POST",
        r#"{"query":"mutation { productDelete(input: { id: \"gid://shopify/Product/base\" }) { deletedProductId } }"}"#,
    ));
    assert_eq!(delete.status, 200);

    let count_after_delete = proxy.process_request(graphql_request(
        "POST",
        r#"{"query":"query { productsCount { count precision } }"}"#,
    ));
    assert_eq!(count_after_delete.status, 200);
    assert_eq!(
        count_after_delete.body,
        json!({
            "data": {
                "productsCount": {
                    "count": 1,
                    "precision": "EXACT"
                }
            }
        })
    );
}

#[test]
fn product_by_identifier_finds_staged_product_by_handle() {
    let mut proxy = snapshot_proxy();

    let create = proxy.process_request(graphql_request(
        "POST",
        r#"{"query":"mutation { productCreate(product: { title: \"Identifier product\", handle: \"identifier-product\" }) { product { id } } }"}"#,
    ));
    assert_eq!(create.status, 200);

    let by_handle = proxy.process_request(graphql_request(
        "POST",
        r#"{"query":"query { productByIdentifier(identifier: { handle: \"identifier-product\" }) { id title handle } }"}"#,
    ));

    assert_eq!(by_handle.status, 200);
    assert_eq!(
        by_handle.body,
        json!({
            "data": {
                "productByIdentifier": {
                    "id": "gid://shopify/Product/1?shopify-draft-proxy=synthetic",
                    "title": "Identifier product",
                    "handle": "identifier-product"
                }
            }
        })
    );
}

#[test]
fn product_by_identifier_preserves_root_alias() {
    let mut proxy = snapshot_proxy().with_base_products(vec![ProductRecord {
        id: "gid://shopify/Product/base".to_string(),
        title: "Base product".to_string(),
        handle: "base-product".to_string(),
        status: "ACTIVE".to_string(),
        description_html: String::new(),
        vendor: String::new(),
        product_type: String::new(),
        tags: Vec::new(),
        template_suffix: String::new(),
        seo_title: String::new(),
        seo_description: String::new(),
    }]);

    let by_handle = proxy.process_request(graphql_request(
        "POST",
        r#"{"query":"query { byHandle: productByIdentifier(identifier: { handle: \"base-product\" }) { id title handle } }"}"#,
    ));

    assert_eq!(by_handle.status, 200);
    assert_eq!(
        by_handle.body,
        json!({
            "data": {
                "byHandle": {
                    "id": "gid://shopify/Product/base",
                    "title": "Base product",
                    "handle": "base-product"
                }
            }
        })
    );
}

#[test]
fn product_by_identifier_supports_multiple_aliases_in_one_query() {
    let mut proxy = snapshot_proxy().with_base_products(vec![ProductRecord {
        id: "gid://shopify/Product/base".to_string(),
        title: "Base product".to_string(),
        handle: "base-product".to_string(),
        status: "ACTIVE".to_string(),
        description_html: String::new(),
        vendor: String::new(),
        product_type: String::new(),
        tags: Vec::new(),
        template_suffix: String::new(),
        seo_title: String::new(),
        seo_description: String::new(),
    }]);

    let create = proxy.process_request(graphql_request(
        "POST",
        r#"{"query":"mutation { productCreate(product: { title: \"Created product\", handle: \"created-product\" }) { product { id } } }"}"#,
    ));
    assert_eq!(create.status, 200);

    let by_identifiers = proxy.process_request(graphql_request(
        "POST",
        r#"{"query":"query { byId: productByIdentifier(identifier: { id: \"gid://shopify/Product/base\" }) { id title } byHandle: productByIdentifier(identifier: { handle: \"created-product\" }) { id handle } missing: productByIdentifier(identifier: { handle: \"missing-product\" }) { id } }"}"#,
    ));

    assert_eq!(by_identifiers.status, 200);
    assert_eq!(
        by_identifiers.body,
        json!({
            "data": {
                "byId": {
                    "id": "gid://shopify/Product/base",
                    "title": "Base product"
                },
                "byHandle": {
                    "id": "gid://shopify/Product/1?shopify-draft-proxy=synthetic",
                    "handle": "created-product"
                },
                "missing": null
            }
        })
    );
}

#[test]
fn products_and_products_count_preserve_root_aliases() {
    let mut proxy = snapshot_proxy().with_base_products(vec![
        ProductRecord {
            id: "gid://shopify/Product/1".to_string(),
            title: "First product".to_string(),
            handle: "first-product".to_string(),
            status: "ACTIVE".to_string(),
            description_html: String::new(),
            vendor: String::new(),
            product_type: String::new(),
            tags: Vec::new(),
            template_suffix: String::new(),
            seo_title: String::new(),
            seo_description: String::new(),
        },
        ProductRecord {
            id: "gid://shopify/Product/2".to_string(),
            title: "Second product".to_string(),
            handle: "second-product".to_string(),
            status: "ACTIVE".to_string(),
            description_html: String::new(),
            vendor: String::new(),
            product_type: String::new(),
            tags: Vec::new(),
            template_suffix: String::new(),
            seo_title: String::new(),
            seo_description: String::new(),
        },
    ]);

    let response = proxy.process_request(graphql_request(
        "POST",
        r#"{"query":"query { listedProducts: products(first: 1) { nodes { id title } } localProductCount: productsCount { count precision } }"}"#,
    ));

    assert_eq!(response.status, 200);
    assert_eq!(
        response.body,
        json!({
            "data": {
                "listedProducts": {
                    "nodes": [
                        {
                            "id": "gid://shopify/Product/1",
                            "title": "First product"
                        }
                    ]
                },
                "localProductCount": {
                    "count": 2,
                    "precision": "EXACT"
                }
            }
        })
    );
}

#[test]
fn product_roots_support_multiple_aliases_in_one_query() {
    let mut proxy = snapshot_proxy().with_base_products(vec![
        ProductRecord {
            id: "gid://shopify/Product/1".to_string(),
            title: "First product".to_string(),
            handle: "first-product".to_string(),
            status: "ACTIVE".to_string(),
            description_html: String::new(),
            vendor: String::new(),
            product_type: String::new(),
            tags: Vec::new(),
            template_suffix: String::new(),
            seo_title: String::new(),
            seo_description: String::new(),
        },
        ProductRecord {
            id: "gid://shopify/Product/2".to_string(),
            title: "Second product".to_string(),
            handle: "second-product".to_string(),
            status: "DRAFT".to_string(),
            description_html: String::new(),
            vendor: String::new(),
            product_type: String::new(),
            tags: Vec::new(),
            template_suffix: String::new(),
            seo_title: String::new(),
            seo_description: String::new(),
        },
    ]);

    let response = proxy.process_request(graphql_request(
        "POST",
        r#"{"query":"query { first: product(id: \"gid://shopify/Product/1\") { title } second: product(id: \"gid://shopify/Product/2\") { status } missing: product(id: \"gid://shopify/Product/missing\") { id } }"}"#,
    ));

    assert_eq!(response.status, 200);
    assert_eq!(
        response.body,
        json!({
            "data": {
                "first": { "title": "First product" },
                "second": { "status": "DRAFT" },
                "missing": null
            }
        })
    );
}

#[test]
fn product_mutations_preserve_root_alias_response_keys() {
    let mut proxy = snapshot_proxy().with_base_products(vec![ProductRecord {
        id: "gid://shopify/Product/1".to_string(),
        title: "Seeded product".to_string(),
        handle: "seeded-product".to_string(),
        status: "ACTIVE".to_string(),
        description_html: String::new(),
        vendor: String::new(),
        product_type: String::new(),
        tags: Vec::new(),
        template_suffix: String::new(),
        seo_title: String::new(),
        seo_description: String::new(),
    }]);

    let create = proxy.process_request(graphql_request(
        "POST",
        r#"{"query":"mutation { createResult: productCreate(product: { title: \"Alias product\" }) { product { id title } userErrors { field message code } } }"}"#,
    ));
    assert_eq!(create.status, 200);
    assert_eq!(
        create.body,
        json!({
            "data": {
                "createResult": {
                    "product": {
                        "id": "gid://shopify/Product/1?shopify-draft-proxy=synthetic",
                        "title": "Alias product"
                    },
                    "userErrors": []
                }
            }
        })
    );

    let update = proxy.process_request(graphql_request(
        "POST",
        r#"{"query":"mutation { updateResult: productUpdate(product: { id: \"gid://shopify/Product/1\", title: \"Updated alias\" }) { product { id title } userErrors { field message code } } }"}"#,
    ));
    assert_eq!(update.status, 200);
    assert_eq!(
        update.body,
        json!({
            "data": {
                "updateResult": {
                    "product": {
                        "id": "gid://shopify/Product/1",
                        "title": "Updated alias"
                    },
                    "userErrors": []
                }
            }
        })
    );

    let delete = proxy.process_request(graphql_request(
        "POST",
        r#"{"query":"mutation { deleteResult: productDelete(product: { id: \"gid://shopify/Product/1\" }) { deletedProductId userErrors { field message code } } }"}"#,
    ));
    assert_eq!(delete.status, 200);
    assert_eq!(
        delete.body,
        json!({
            "data": {
                "deleteResult": {
                    "deletedProductId": "gid://shopify/Product/1",
                    "userErrors": []
                }
            }
        })
    );
}

#[test]
fn saved_search_roots_support_defaults_filtering_pagination_edges_and_aliases() {
    let mut proxy = snapshot_proxy();

    let response = proxy.process_request(graphql_request(
        "POST",
        r#"{"query":"query { ord: orderSavedSearches(first: 2) { nodes { id name } edges { cursor node { id } } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } draftOrderSavedSearches(first: 1) { nodes { name resourceType } } productSavedSearches(first: 2) { nodes { id } edges { cursor node { id } } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } filtered: orderSavedSearches(query: \"financial_status\") { nodes { name query } } }"}"#,
    ));

    assert_eq!(response.status, 200);
    assert_eq!(
        response.body,
        json!({
            "data": {
                "ord": {
                    "nodes": [
                        { "id": "gid://shopify/SavedSearch/3634391515442", "name": "Unfulfilled" },
                        { "id": "gid://shopify/SavedSearch/3634391548210", "name": "Unpaid" }
                    ],
                    "edges": [
                        { "cursor": "cursor:gid://shopify/SavedSearch/3634391515442", "node": { "id": "gid://shopify/SavedSearch/3634391515442" } },
                        { "cursor": "cursor:gid://shopify/SavedSearch/3634391548210", "node": { "id": "gid://shopify/SavedSearch/3634391548210" } }
                    ],
                    "pageInfo": {
                        "hasNextPage": true,
                        "hasPreviousPage": false,
                        "startCursor": "cursor:gid://shopify/SavedSearch/3634391515442",
                        "endCursor": "cursor:gid://shopify/SavedSearch/3634391548210"
                    }
                },
                "draftOrderSavedSearches": {
                    "nodes": [
                        { "name": "Open and invoice sent", "resourceType": "DRAFT_ORDER" }
                    ]
                },
                "productSavedSearches": {
                    "nodes": [],
                    "edges": [],
                    "pageInfo": {
                        "hasNextPage": false,
                        "hasPreviousPage": false,
                        "startCursor": null,
                        "endCursor": null
                    }
                },
                "filtered": {
                    "nodes": [
                        { "name": "Unpaid", "query": "status:open financial_status:unpaid" }
                    ]
                }
            }
        })
    );
}

#[test]
fn segment_create_update_query_grammar_stages_and_reads_generic_node() {
    let mut proxy = snapshot_proxy();
    let create_query = r#"
        mutation SegmentCreateQueryGrammar($name: String!, $query: String!) {
          segmentCreate(name: $name, query: $query) {
            segment { id name query creationDate lastEditDate }
            userErrors { field message }
          }
        }
    "#;

    let created = proxy.process_request(json_graphql_request(
        create_query,
        json!({
            "name": "Query grammar update setup segment-query-grammar-local",
            "query": "number_of_orders >= 1"
        }),
    ));
    let created_segment = &created.body["data"]["segmentCreate"]["segment"];
    let segment_id = created_segment["id"].as_str().unwrap().to_string();
    assert!(segment_id.starts_with("gid://shopify/Segment/"));
    assert_eq!(
        created.body["data"]["segmentCreate"],
        json!({
            "segment": {
                "id": segment_id,
                "name": "Query grammar update setup segment-query-grammar-local",
                "query": "number_of_orders >= 1",
                "creationDate": created_segment["creationDate"],
                "lastEditDate": created_segment["lastEditDate"],
            },
            "userErrors": []
        })
    );

    let updated = proxy.process_request(json_graphql_request(
        r#"
        mutation SegmentUpdateQueryGrammar($id: ID!, $query: String) {
          segmentUpdate(id: $id, query: $query) {
            segment { id name query creationDate lastEditDate }
            userErrors { field message }
          }
        }
        "#,
        json!({ "id": segment_id, "query": "customer_countries CONTAINS 'CA'" }),
    ));
    assert_eq!(
        updated.body["data"]["segmentUpdate"]["segment"]["query"],
        json!("customer_countries CONTAINS 'CA'")
    );

    let node = proxy.process_request(json_graphql_request(
        r#"
        query SegmentNodeRead($id: ID!) {
          node(id: $id) { ... on Segment { id name query creationDate lastEditDate } }
        }
        "#,
        json!({ "id": segment_id }),
    ));
    assert_eq!(
        node.body["data"]["node"]["query"],
        json!("customer_countries CONTAINS 'CA'")
    );

    let malformed = proxy.process_request(json_graphql_request(
        create_query,
        json!({
            "name": "Query grammar malformed segment-query-grammar-local",
            "query": "not a valid segment query ???"
        }),
    ));
    assert_eq!(
        malformed.body["data"]["segmentCreate"]["segment"],
        Value::Null
    );
    assert_eq!(
        malformed.body["data"]["segmentCreate"]["userErrors"],
        json!([
            { "field": ["query"], "message": "Query Line 1 Column 6: 'valid' is unexpected." },
            { "field": ["query"], "message": "Query Line 1 Column 4: 'a' filter cannot be found." }
        ])
    );
}

#[test]
fn customer_segment_members_query_create_validates_stages_and_reads_node() {
    let mut proxy = snapshot_proxy();
    let create_query = r#"
        mutation CustomerSegmentMembersQueryCreateValidationAndShape($input: CustomerSegmentMembersQueryInput!) {
          customerSegmentMembersQueryCreate(input: $input) {
            customerSegmentMembersQuery { id currentCount done }
            userErrors { field code message }
          }
        }
    "#;

    let both = proxy.process_request(json_graphql_request(
        create_query,
        json!({ "input": { "segmentId": "gid://shopify/Segment/1", "query": "number_of_orders > 0" } }),
    ));
    assert_eq!(
        both.body["data"]["customerSegmentMembersQueryCreate"],
        json!({
            "customerSegmentMembersQuery": null,
            "userErrors": [{
                "field": ["input"],
                "code": "INVALID",
                "message": "Providing both segment_id and query is not supported."
            }]
        })
    );

    let neither = proxy.process_request(json_graphql_request(create_query, json!({ "input": {} })));
    assert_eq!(
        neither.body["data"]["customerSegmentMembersQueryCreate"],
        json!({
            "customerSegmentMembersQuery": null,
            "userErrors": [{
                "field": ["input"],
                "code": "INVALID",
                "message": "You must provide one of segment_id or query."
            }]
        })
    );

    let created = proxy.process_request(json_graphql_request(
        create_query,
        json!({ "input": { "query": "number_of_orders > 0" } }),
    ));
    let created_query =
        &created.body["data"]["customerSegmentMembersQueryCreate"]["customerSegmentMembersQuery"];
    let query_id = created_query["id"].as_str().unwrap().to_string();
    assert!(query_id.starts_with("gid://shopify/CustomerSegmentMembersQuery/"));
    assert_eq!(
        created.body["data"]["customerSegmentMembersQueryCreate"],
        json!({
            "customerSegmentMembersQuery": {
                "id": query_id,
                "currentCount": 0,
                "done": false
            },
            "userErrors": []
        })
    );

    let lookup = proxy.process_request(json_graphql_request(
        r#"
        query CustomerSegmentMembersQueryLookupValidationAndShape($id: ID!) {
          customerSegmentMembersQuery(id: $id) { id currentCount done }
        }
        "#,
        json!({ "id": query_id }),
    ));
    assert_eq!(
        lookup.body["data"]["customerSegmentMembersQuery"],
        json!({ "id": query_id, "currentCount": 0, "done": false })
    );

    let node = proxy.process_request(json_graphql_request(
        r#"
        query CustomerSegmentMembersQueryNodeRead($id: ID!) {
          node(id: $id) { ... on CustomerSegmentMembersQuery { id currentCount done } }
        }
        "#,
        json!({ "id": query_id }),
    ));
    assert_eq!(
        node.body["data"]["node"],
        json!({ "id": query_id, "currentCount": 0, "done": false })
    );
}

#[test]
fn saved_search_create_stages_and_reads_back_selection_aware_results() {
    let mut proxy = snapshot_proxy();

    let create = proxy.process_request(graphql_request(
        "POST",
        r#"{"query":"mutation CreateSearch($input: SavedSearchCreateInput!) { made: savedSearchCreate(input: $input) { savedSearch { id legacyResourceId name query resourceType filters { key value } } userErrors { field message code } } }","variables":{"input":{"name":"Promo products","query":"tag:promo vendor:acme","resourceType":"PRODUCT"}}}"#,
    ));

    assert_eq!(create.status, 200);
    assert_eq!(
        create.body,
        json!({
            "data": {
                "made": {
                    "savedSearch": {
                        "id": "gid://shopify/SavedSearch/1?shopify-draft-proxy=synthetic",
                        "legacyResourceId": "1",
                        "name": "Promo products",
                        "query": "tag:promo vendor:acme",
                        "resourceType": "PRODUCT",
                        "filters": [
                            { "key": "tag", "value": "promo" },
                            { "key": "vendor", "value": "acme" }
                        ]
                    },
                    "userErrors": []
                }
            }
        })
    );

    let read = proxy.process_request(graphql_request(
        "POST",
        r#"{"query":"query { productSavedSearches(first: 5) { nodes { id name query resourceType filters { key value } } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } }"}"#,
    ));

    assert_eq!(read.status, 200);
    assert_eq!(
        read.body,
        json!({
            "data": {
                "productSavedSearches": {
                    "nodes": [
                        {
                            "id": "gid://shopify/SavedSearch/1?shopify-draft-proxy=synthetic",
                            "name": "Promo products",
                            "query": "tag:promo vendor:acme",
                            "resourceType": "PRODUCT",
                            "filters": [
                                { "key": "tag", "value": "promo" },
                                { "key": "vendor", "value": "acme" }
                            ]
                        }
                    ],
                    "pageInfo": {
                        "hasNextPage": false,
                        "hasPreviousPage": false,
                        "startCursor": "cursor:gid://shopify/SavedSearch/1?shopify-draft-proxy=synthetic",
                        "endCursor": "cursor:gid://shopify/SavedSearch/1?shopify-draft-proxy=synthetic"
                    }
                }
            }
        })
    );
}

#[test]
fn saved_search_reserved_names_are_rejected_and_failed_update_preserves_existing_name() {
    let mut proxy = snapshot_proxy();

    for (resource_type, name) in [
        ("PRODUCT", "All products"),
        ("PRODUCT", "ALL PRODUCTS"),
        ("ORDER", "All"),
        ("DRAFT_ORDER", "All Drafts"),
        ("FILE", "All Files"),
        ("COLLECTION", "All collections"),
        ("DISCOUNT_REDEEM_CODE", "All codes"),
    ] {
        let create = proxy.process_request(json_graphql_request(
            r#"
            mutation SavedSearchLocalStagingCreate($input: SavedSearchCreateInput!) {
              savedSearchCreate(input: $input) {
                savedSearch { id name query resourceType }
                userErrors { field message }
              }
            }
            "#,
            json!({ "input": { "resourceType": resource_type, "name": name, "query": "vendor:Acme" } }),
        ));
        assert_eq!(
            create.body["data"]["savedSearchCreate"],
            json!({
                "savedSearch": null,
                "userErrors": [{ "field": ["input", "name"], "message": "Name has already been taken" }]
            })
        );
    }

    let positive = proxy.process_request(json_graphql_request(
        r#"
        mutation SavedSearchLocalStagingCreate($input: SavedSearchCreateInput!) {
          savedSearchCreate(input: $input) {
            savedSearch { id name query resourceType searchTerms filters { key value } }
            userErrors { field message }
          }
        }
        "#,
        json!({ "input": { "resourceType": "PRODUCT", "name": "All products v2", "query": "vendor:Acme" } }),
    ));
    let id = positive.body["data"]["savedSearchCreate"]["savedSearch"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(
        positive.body["data"]["savedSearchCreate"]["userErrors"],
        json!([])
    );

    let update = proxy.process_request(json_graphql_request(
        r#"
        mutation SavedSearchNameUniquenessUpdateConflict($input: SavedSearchUpdateInput!) {
          savedSearchUpdate(input: $input) {
            savedSearch { id name query resourceType searchTerms filters { key value } }
            userErrors { field message }
          }
        }
        "#,
        json!({ "input": { "id": id, "name": "All products", "query": "vendor:Changed" } }),
    ));
    assert_eq!(
        update.body["data"]["savedSearchUpdate"],
        json!({
            "savedSearch": {
                "id": positive.body["data"]["savedSearchCreate"]["savedSearch"]["id"].clone(),
                "name": "All products v2",
                "query": "vendor:Changed",
                "resourceType": "PRODUCT",
                "searchTerms": "",
                "filters": [{ "key": "vendor", "value": "Changed" }]
            },
            "userErrors": [{ "field": ["input", "name"], "message": "Name has already been taken" }]
        })
    );

    let create_a = proxy.process_request(json_graphql_request(
        r#"
        mutation SavedSearchLocalStagingCreate($input: SavedSearchCreateInput!) {
          savedSearchCreate(input: $input) { savedSearch { id name query resourceType } userErrors { field message } }
        }
        "#,
        json!({ "input": { "resourceType": "PRODUCT", "name": "Conflict A", "query": "title:a" } }),
    ));
    let create_b = proxy.process_request(json_graphql_request(
        r#"
        mutation SavedSearchLocalStagingCreate($input: SavedSearchCreateInput!) {
          savedSearchCreate(input: $input) { savedSearch { id name query resourceType } userErrors { field message } }
        }
        "#,
        json!({ "input": { "resourceType": "PRODUCT", "name": "Conflict B", "query": "title:b" } }),
    ));
    let duplicate = proxy.process_request(json_graphql_request(
        r#"
        mutation SavedSearchLocalStagingCreate($input: SavedSearchCreateInput!) {
          savedSearchCreate(input: $input) { savedSearch { id name query resourceType } userErrors { field message } }
        }
        "#,
        json!({ "input": { "resourceType": "PRODUCT", "name": "Conflict A", "query": "title:duplicate" } }),
    ));
    assert_eq!(
        duplicate.body["data"]["savedSearchCreate"],
        json!({
            "savedSearch": null,
            "userErrors": [{ "field": ["input", "name"], "message": "Name has already been taken" }]
        })
    );
    let b_id = create_b.body["data"]["savedSearchCreate"]["savedSearch"]["id"]
        .as_str()
        .unwrap();
    let rename_conflict = proxy.process_request(json_graphql_request(
        r#"
        mutation SavedSearchNameUniquenessUpdateConflict($input: SavedSearchUpdateInput!) {
          savedSearchUpdate(input: $input) { savedSearch { id name query resourceType filters { key value } } userErrors { field message } }
        }
        "#,
        json!({ "input": { "id": b_id, "name": "Conflict A", "query": "title:rename-conflict" } }),
    ));
    assert_eq!(
        rename_conflict.body["data"]["savedSearchUpdate"],
        json!({
            "savedSearch": {
                "id": create_b.body["data"]["savedSearchCreate"]["savedSearch"]["id"].clone(),
                "name": "Conflict B",
                "query": "title:rename-conflict",
                "resourceType": "PRODUCT",
                "filters": [{ "key": "title", "value": "rename-conflict" }]
            },
            "userErrors": [{ "field": ["input", "name"], "message": "Name has already been taken" }]
        })
    );
    assert_eq!(
        create_a.body["data"]["savedSearchCreate"]["userErrors"],
        json!([])
    );
}

#[test]
fn saved_search_multi_root_create_delete_and_filter_projection() {
    let mut proxy = snapshot_proxy();

    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation SavedSearchFilterProjection($product: SavedSearchCreateInput!, $collection: SavedSearchCreateInput!, $exists: SavedSearchCreateInput!, $bounded: SavedSearchCreateInput!, $grammar: SavedSearchCreateInput!) {
          product: savedSearchCreate(input: $product) { savedSearch { id name query resourceType searchTerms filters { __typename key value } } userErrors { field message } }
          collection: savedSearchCreate(input: $collection) { savedSearch { id name query resourceType searchTerms filters { key value } } userErrors { field message } }
          exists: savedSearchCreate(input: $exists) { savedSearch { id name query resourceType searchTerms filters { key value } } userErrors { field message } }
          bounded: savedSearchCreate(input: $bounded) { savedSearch { id name query resourceType searchTerms filters { key value } } userErrors { field message } }
          grammar: savedSearchCreate(input: $grammar) { savedSearch { id name query resourceType searchTerms filters { key value } } userErrors { field message } }
        }
        "#,
        json!({
            "product": { "resourceType": "PRODUCT", "name": "Recorder Product", "query": "1778109773860 title:Recorder" },
            "collection": { "resourceType": "COLLECTION", "name": "Collection Search", "query": "Collection Search" },
            "exists": { "resourceType": "PRODUCT", "name": "Exists Search", "query": "sku:*" },
            "bounded": { "resourceType": "PRODUCT", "name": "Bounded Search", "query": "inventory_total:>2 inventory_total:<10" },
            "grammar": { "resourceType": "PRODUCT", "name": "Grammar Search", "query": "title:\"Alpha\" OR (status:ACTIVE tag:\"tagged\") -vendor:Archived" }
        }),
    ));
    assert_eq!(create.status, 200);
    assert_eq!(
        create.body["data"]["product"],
        json!({
            "savedSearch": {
                "id": "gid://shopify/SavedSearch/1?shopify-draft-proxy=synthetic",
                "name": "Recorder Product",
                "query": "1778109773860 title:Recorder",
                "resourceType": "PRODUCT",
                "searchTerms": "1778109773860",
                "filters": [{ "__typename": "SearchFilter", "key": "title", "value": "Recorder" }]
            },
            "userErrors": []
        })
    );
    assert_eq!(
        create.body["data"]["collection"]["savedSearch"],
        json!({
            "id": "gid://shopify/SavedSearch/2?shopify-draft-proxy=synthetic",
            "name": "Collection Search",
            "query": "Collection Search",
            "resourceType": "COLLECTION",
            "searchTerms": "Collection Search",
            "filters": []
        })
    );
    assert_eq!(
        create.body["data"]["exists"]["savedSearch"]["filters"],
        json!([{ "key": "sku", "value": "true" }])
    );
    assert_eq!(
        create.body["data"]["bounded"]["savedSearch"]["filters"],
        json!([
            { "key": "inventory_total_min", "value": "2" },
            { "key": "inventory_total_max", "value": "10" }
        ])
    );
    assert_eq!(
        create.body["data"]["grammar"]["savedSearch"],
        json!({
            "id": "gid://shopify/SavedSearch/5?shopify-draft-proxy=synthetic",
            "name": "Grammar Search",
            "query": "title:\"Alpha\" OR (status:ACTIVE tag:\"tagged\") -vendor:Archived",
            "resourceType": "PRODUCT",
            "searchTerms": "title:\"Alpha\" OR (status:ACTIVE tag:\"tagged\")",
            "filters": [{ "key": "vendor_not", "value": "Archived" }]
        })
    );

    let read = proxy.process_request(json_graphql_request(
        r#"
        query SavedSearchResourceReads {
          products: productSavedSearches(first: 10) { nodes { id name searchTerms filters { key value } } pageInfo { hasNextPage hasPreviousPage } }
          collections: collectionSavedSearches(first: 10) { nodes { id name searchTerms filters { key value } } pageInfo { hasNextPage hasPreviousPage } }
        }
        "#,
        json!({}),
    ));
    assert_eq!(read.status, 200);
    assert_eq!(
        read.body["data"]["products"]["nodes"]
            .as_array()
            .unwrap()
            .len(),
        4
    );
    assert_eq!(
        read.body["data"]["collections"]["nodes"]
            .as_array()
            .unwrap()
            .len(),
        1
    );

    let delete = proxy.process_request(json_graphql_request(
        r#"
        mutation SavedSearchCleanup($first: SavedSearchDeleteInput!, $second: SavedSearchDeleteInput!, $missing: SavedSearchDeleteInput!) {
          first: savedSearchDelete(input: $first) { deletedSavedSearchId userErrors { field message } }
          second: savedSearchDelete(input: $second) { deletedSavedSearchId userErrors { field message } }
          missing: savedSearchDelete(input: $missing) { deletedSavedSearchId userErrors { field message } }
        }
        "#,
        json!({
            "first": { "id": "gid://shopify/SavedSearch/1?shopify-draft-proxy=synthetic" },
            "second": { "id": "gid://shopify/SavedSearch/2?shopify-draft-proxy=synthetic" },
            "missing": { "id": "gid://shopify/SavedSearch/missing" }
        }),
    ));
    assert_eq!(delete.status, 200);
    assert_eq!(
        delete.body["data"],
        json!({
            "first": { "deletedSavedSearchId": "gid://shopify/SavedSearch/1?shopify-draft-proxy=synthetic", "userErrors": [] },
            "second": { "deletedSavedSearchId": "gid://shopify/SavedSearch/2?shopify-draft-proxy=synthetic", "userErrors": [] },
            "missing": { "deletedSavedSearchId": null, "userErrors": [{ "field": ["input", "id"], "message": "Saved Search does not exist" }] }
        })
    );
}

#[test]
fn saved_search_required_input_omissions_return_top_level_graphql_errors() {
    let mut proxy = snapshot_proxy();

    let missing_name = proxy.process_request(json_graphql_request(
        r#"
        mutation SavedSearchCreateMissingName {
          savedSearchCreate(input: { resourceType: PRODUCT }) {
            savedSearch { id name query resourceType }
            userErrors { field message }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(missing_name.status, 200);
    assert_eq!(missing_name.body.get("data"), None);
    assert_eq!(
        missing_name.body["errors"],
        json!([
            {
                "message": "Argument 'name' on InputObject 'SavedSearchCreateInput' is required. Expected type String!",
                "locations": [{ "line": 2, "column": 28 }],
                "path": ["mutation SavedSearchCreateMissingName", "savedSearchCreate", "input", "name"],
                "extensions": {
                    "code": "missingRequiredInputObjectAttribute",
                    "argumentName": "name",
                    "argumentType": "String!",
                    "inputObjectType": "SavedSearchCreateInput"
                }
            },
            {
                "message": "Argument 'query' on InputObject 'SavedSearchCreateInput' is required. Expected type String!",
                "locations": [{ "line": 2, "column": 28 }],
                "path": ["mutation SavedSearchCreateMissingName", "savedSearchCreate", "input", "query"],
                "extensions": {
                    "code": "missingRequiredInputObjectAttribute",
                    "argumentName": "query",
                    "argumentType": "String!",
                    "inputObjectType": "SavedSearchCreateInput"
                }
            }
        ])
    );

    let missing_resource_type = proxy.process_request(json_graphql_request(
        r#"
        mutation SavedSearchCreateMissingResourceType {
          savedSearchCreate(input: { name: "Missing resource type", query: "tag:missing-resource-type" }) {
            savedSearch { id name query resourceType }
            userErrors { field message }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(missing_resource_type.body.get("data"), None);
    assert_eq!(
        missing_resource_type.body["errors"][0],
        json!({
            "message": "Argument 'resourceType' on InputObject 'SavedSearchCreateInput' is required. Expected type SearchResultType!",
            "locations": [{ "line": 2, "column": 28 }],
            "path": ["mutation SavedSearchCreateMissingResourceType", "savedSearchCreate", "input", "resourceType"],
            "extensions": {
                "code": "missingRequiredInputObjectAttribute",
                "argumentName": "resourceType",
                "argumentType": "SearchResultType!",
                "inputObjectType": "SavedSearchCreateInput"
            }
        })
    );

    let missing_id = proxy.process_request(json_graphql_request(
        r#"
        mutation SavedSearchUpdateMissingId {
          savedSearchUpdate(input: { name: "Missing id" }) {
            savedSearch { id name }
            userErrors { field message }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(missing_id.body.get("data"), None);
    assert_eq!(
        missing_id.body["errors"][0],
        json!({
            "message": "Argument 'id' on InputObject 'SavedSearchUpdateInput' is required. Expected type ID!",
            "locations": [{ "line": 2, "column": 28 }],
            "path": ["mutation SavedSearchUpdateMissingId", "savedSearchUpdate", "input", "id"],
            "extensions": {
                "code": "missingRequiredInputObjectAttribute",
                "argumentName": "id",
                "argumentType": "ID!",
                "inputObjectType": "SavedSearchUpdateInput"
            }
        })
    );
}

#[test]
fn saved_search_required_variable_omissions_return_invalid_variable_errors() {
    let mut proxy = snapshot_proxy();

    let missing_resource_type = proxy.process_request(json_graphql_request(
        r#"
        mutation SavedSearchCreateVariableMissingResourceType($input: SavedSearchCreateInput!) {
          savedSearchCreate(input: $input) { savedSearch { id } userErrors { field message } }
        }
        "#,
        json!({ "input": { "name": "Variable missing resource type ssri-mowc", "query": "tag:variable-required" } }),
    ));
    assert_eq!(missing_resource_type.body.get("data"), None);
    assert_eq!(
        missing_resource_type.body["errors"][0],
        json!({
            "message": "Variable $input of type SavedSearchCreateInput! was provided invalid value for resourceType (Expected value to not be null)",
            "locations": [{ "line": 1, "column": 55 }],
            "extensions": {
                "code": "INVALID_VARIABLE",
                "value": { "name": "Variable missing resource type ssri-mowc", "query": "tag:variable-required" },
                "problems": [{ "path": ["resourceType"], "explanation": "Expected value to not be null" }]
            }
        })
    );

    let missing_name = proxy.process_request(json_graphql_request(
        r#"
        mutation SavedSearchCreateVariableMissingName($input: SavedSearchCreateInput!) {
          savedSearchCreate(input: $input) { savedSearch { id } userErrors { field message } }
        }
        "#,
        json!({ "input": { "resourceType": "PRODUCT", "query": "tag:variable-required" } }),
    ));
    assert_eq!(missing_name.body.get("data"), None);
    assert_eq!(
        missing_name.body["errors"][0],
        json!({
            "message": "Variable $input of type SavedSearchCreateInput! was provided invalid value for name (Expected value to not be null)",
            "locations": [{ "line": 1, "column": 47 }],
            "extensions": {
                "code": "INVALID_VARIABLE",
                "value": { "resourceType": "PRODUCT", "query": "tag:variable-required" },
                "problems": [{ "path": ["name"], "explanation": "Expected value to not be null" }]
            }
        })
    );
}

#[test]
fn product_mutation_error_payloads_preserve_root_alias_response_keys() {
    let mut proxy = snapshot_proxy();

    let create = proxy.process_request(graphql_request(
        "POST",
        r#"{"query":"mutation { failedCreate: productCreate(product: { title: \" \" }) { product { id } userErrors { field message code } } }"}"#,
    ));
    assert_eq!(create.status, 200);
    assert_eq!(
        create.body,
        json!({
            "data": {
                "failedCreate": {
                    "product": null,
                    "userErrors": [{
                        "field": ["title"],
                        "message": "Title can't be blank",
                        "code": "BLANK"
                    }]
                }
            }
        })
    );

    let update = proxy.process_request(graphql_request(
        "POST",
        r#"{"query":"mutation { failedUpdate: productUpdate(product: { id: \"gid://shopify/Product/missing\", title: \"Missing\" }) { product { id } userErrors { field message code } } }"}"#,
    ));
    assert_eq!(update.status, 200);
    assert_eq!(
        update.body,
        json!({
            "data": {
                "failedUpdate": {
                    "product": null,
                    "userErrors": [{
                        "field": ["id"],
                        "message": "Product does not exist",
                        "code": "NOT_FOUND"
                    }]
                }
            }
        })
    );

    let delete = proxy.process_request(graphql_request(
        "POST",
        r#"{"query":"mutation { failedDelete: productDelete(input: { id: \"gid://shopify/Product/missing\" }) { deletedProductId userErrors { field message code } } }"}"#,
    ));
    assert_eq!(delete.status, 200);
    assert_eq!(
        delete.body,
        json!({
            "data": {
                "failedDelete": {
                    "deletedProductId": null,
                    "userErrors": [{
                        "field": ["id"],
                        "message": "Product does not exist",
                        "code": "NOT_FOUND"
                    }]
                }
            }
        })
    );
}

#[test]
fn product_delete_stages_downstream_no_data_for_product_read() {
    let mut proxy = snapshot_proxy().with_base_products(vec![ProductRecord {
        id: "gid://shopify/Product/1".to_string(),
        title: "Deletable product".to_string(),
        handle: "deletable-product".to_string(),
        status: "ACTIVE".to_string(),
        description_html: String::new(),
        vendor: String::new(),
        product_type: String::new(),
        tags: Vec::new(),
        template_suffix: String::new(),
        seo_title: String::new(),
        seo_description: String::new(),
    }]);

    let delete = proxy.process_request(graphql_request(
        "POST",
        r#"{"query":"mutation { productDelete(input: { id: \"gid://shopify/Product/1\" }) { deletedProductId userErrors { field message code } } }"}"#,
    ));

    assert_eq!(delete.status, 200);
    assert_eq!(
        delete.body,
        json!({
            "data": {
                "productDelete": {
                    "deletedProductId": "gid://shopify/Product/1",
                    "userErrors": []
                }
            }
        })
    );

    let read_back = proxy.process_request(graphql_request(
        "POST",
        r#"{"query":"query { product(id: \"gid://shopify/Product/1\") { id title } }"}"#,
    ));

    assert_eq!(read_back.status, 200);
    assert_eq!(read_back.body, json!({ "data": { "product": null } }));
}

#[test]
fn product_create_stages_product_visible_to_product_read() {
    let mut proxy = snapshot_proxy();

    let create = proxy.process_request(graphql_request(
        "POST",
        r#"{"query":"mutation { productCreate(product: { title: \"Rust staged product\", handle: \"rust-staged-product\", status: ACTIVE }) { product { id title handle status } userErrors { field message code } } }"}"#,
    ));

    assert_eq!(create.status, 200);
    assert_eq!(
        create.body,
        json!({
            "data": {
                "productCreate": {
                    "product": {
                        "id": "gid://shopify/Product/1?shopify-draft-proxy=synthetic",
                        "title": "Rust staged product",
                        "handle": "rust-staged-product",
                        "status": "ACTIVE"
                    },
                    "userErrors": []
                }
            }
        })
    );

    let read_back = proxy.process_request(graphql_request(
        "POST",
        r#"{"query":"query { product(id: \"gid://shopify/Product/1?shopify-draft-proxy=synthetic\") { id title handle status } }"}"#,
    ));

    assert_eq!(read_back.status, 200);
    assert_eq!(
        read_back.body,
        json!({
            "data": {
                "product": {
                    "id": "gid://shopify/Product/1?shopify-draft-proxy=synthetic",
                    "title": "Rust staged product",
                    "handle": "rust-staged-product",
                    "status": "ACTIVE"
                }
            }
        })
    );
}

#[test]
fn product_read_resolves_id_from_request_variables() {
    let mut proxy = snapshot_proxy().with_base_products(vec![ProductRecord {
        id: "gid://shopify/Product/variable-id".to_string(),
        title: "Variable product".to_string(),
        handle: "variable-product".to_string(),
        status: "DRAFT".to_string(),
        description_html: String::new(),
        vendor: String::new(),
        product_type: String::new(),
        tags: Vec::new(),
        template_suffix: String::new(),
        seo_title: String::new(),
        seo_description: String::new(),
    }]);

    let product = proxy.process_request(graphql_request(
        "POST",
        r#"{"query":"query ProductById($id: ID!) { product(id: $id) { id title handle status } }","variables":{"id":"gid://shopify/Product/variable-id"}}"#,
    ));

    assert_eq!(product.status, 200);
    assert_eq!(
        product.body,
        json!({
            "data": {
                "product": {
                    "id": "gid://shopify/Product/variable-id",
                    "title": "Variable product",
                    "handle": "variable-product",
                    "status": "DRAFT"
                }
            }
        })
    );
}

#[test]
fn product_create_resolves_input_from_request_variables() {
    let mut proxy = snapshot_proxy();

    let create = proxy.process_request(graphql_request(
        "POST",
        r#"{"query":"mutation ProductCreate($product: ProductCreateInput!) { productCreate(product: $product) { product { id title handle status } userErrors { field message code } } }","variables":{"product":{"title":"Variable staged product","handle":"variable-staged-product","status":"ARCHIVED"}}}"#,
    ));

    assert_eq!(create.status, 200);
    assert_eq!(
        create.body,
        json!({
            "data": {
                "productCreate": {
                    "product": {
                        "id": "gid://shopify/Product/1?shopify-draft-proxy=synthetic",
                        "title": "Variable staged product",
                        "handle": "variable-staged-product",
                        "status": "ARCHIVED"
                    },
                    "userErrors": []
                }
            }
        })
    );
}

#[test]
fn admin_graphql_uses_proxy_owned_registry_for_capability_classification() {
    let mut proxy = snapshot_proxy().with_registry(vec![
        registry_entry(
            "knownProducts",
            OperationType::Query,
            CapabilityExecution::OverlayRead,
            true,
        ),
        registry_entry(
            "knownProductCreate",
            OperationType::Mutation,
            CapabilityExecution::StageLocally,
            true,
        ),
        registry_entry(
            "knownButUnimplemented",
            OperationType::Query,
            CapabilityExecution::OverlayRead,
            false,
        ),
    ]);

    let known_query = proxy.process_request(graphql_request(
        "POST",
        r#"{"query":"query { knownProducts(first: 1) { nodes { id } } }"}"#,
    ));
    assert_eq!(known_query.status, 501);
    assert_eq!(
        known_query.body,
        json!({ "errors": [{ "message": "No Rust overlay-read dispatcher implemented for root field: knownProducts" }] })
    );

    let known_mutation = proxy.process_request(graphql_request(
        "POST",
        r#"{"query":"mutation { knownProductCreate(input: {}) { product { id } } }"}"#,
    ));
    assert_eq!(known_mutation.status, 501);
    assert_eq!(
        known_mutation.body,
        json!({ "errors": [{ "message": "No Rust stage-locally dispatcher implemented for root field: knownProductCreate" }] })
    );

    let unimplemented = proxy.process_request(graphql_request(
        "POST",
        r#"{"query":"query { knownButUnimplemented { id } }"}"#,
    ));
    assert_eq!(unimplemented.status, 400);
    assert_eq!(
        unimplemented.body,
        json!({ "errors": [{ "message": "No domain dispatcher implemented for root field: knownButUnimplemented" }] })
    );
}

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

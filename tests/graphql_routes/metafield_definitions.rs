use super::common::*;
use pretty_assertions::assert_eq;

fn create_definition(proxy: &mut DraftProxy, namespace: &str, key: &str, pin: bool) -> Value {
    proxy
        .process_request(json_graphql_request(
            r#"
        mutation MetafieldDefinitionCreateForPinLimit($definition: MetafieldDefinitionInput!) {
          metafieldDefinitionCreate(definition: $definition) {
            createdDefinition { id key pinnedPosition }
            userErrors { field message code }
          }
        }
        "#,
            json!({
                "definition": {
                    "ownerType": "PRODUCT",
                    "namespace": namespace,
                    "key": key,
                    "name": format!("Pin limit {key}"),
                    "type": "single_line_text_field",
                    "pin": pin
                }
            }),
        ))
        .body["data"]["metafieldDefinitionCreate"]
        .clone()
}

fn pin_definition(proxy: &mut DraftProxy, namespace: &str, key: &str) -> Value {
    proxy
        .process_request(json_graphql_request(
            r#"
        mutation MetafieldDefinitionPinForLimit($identifier: MetafieldDefinitionIdentifierInput!) {
          metafieldDefinitionPin(identifier: $identifier) {
            pinnedDefinition { id key pinnedPosition }
            userErrors { field message code }
          }
        }
        "#,
            json!({"identifier": {"ownerType": "PRODUCT", "namespace": namespace, "key": key}}),
        ))
        .body["data"]["metafieldDefinitionPin"]
        .clone()
}

fn unpin_definition(proxy: &mut DraftProxy, namespace: &str, key: &str) -> Value {
    proxy
        .process_request(json_graphql_request(
            r#"
        mutation MetafieldDefinitionUnpinForLimit($identifier: MetafieldDefinitionIdentifierInput!) {
          metafieldDefinitionUnpin(identifier: $identifier) {
            unpinnedDefinition { id key pinnedPosition }
            userErrors { field message code }
          }
        }
        "#,
            json!({"identifier": {"ownerType": "PRODUCT", "namespace": namespace, "key": key}}),
        ))
        .body["data"]["metafieldDefinitionUnpin"]
        .clone()
}

fn read_definition(proxy: &mut DraftProxy, namespace: &str, key: &str) -> Value {
    proxy
        .process_request(json_graphql_request(
            r#"
        query MetafieldDefinitionPinningRead($identifier: MetafieldDefinitionIdentifierInput!) {
          metafieldDefinition(identifier: $identifier) {
            key
            pinnedPosition
          }
        }
        "#,
            json!({"identifier": {"ownerType": "PRODUCT", "namespace": namespace, "key": key}}),
        ))
        .body["data"]["metafieldDefinition"]
        .clone()
}

fn create_definition_for_resource_limit(
    proxy: &mut DraftProxy,
    owner_type: &str,
    namespace: &str,
    key: &str,
) -> Value {
    proxy
        .process_request(json_graphql_request(
            r#"
        mutation MetafieldDefinitionCreateForResourceLimit($definition: MetafieldDefinitionInput!) {
          metafieldDefinitionCreate(definition: $definition) {
            createdDefinition { id namespace key ownerType }
            userErrors { field message code }
          }
        }
        "#,
            json!({
                "definition": {
                    "ownerType": owner_type,
                    "namespace": namespace,
                    "key": key,
                    "name": format!("Resource limit {key}"),
                    "type": "single_line_text_field"
                }
            }),
        ))
        .body["data"]["metafieldDefinitionCreate"]
        .clone()
}

fn create_app_definition_for_resource_limit(
    proxy: &mut DraftProxy,
    api_client_id: &str,
    key: &str,
) -> Value {
    let mut request = json_graphql_request(
        r#"
        mutation MetafieldDefinitionCreateForAppResourceLimit($definition: MetafieldDefinitionInput!) {
          metafieldDefinitionCreate(definition: $definition) {
            createdDefinition { id namespace key ownerType }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "definition": {
                "ownerType": "PRODUCT",
                "namespace": "$app:resource_limit",
                "key": key,
                "name": format!("App resource limit {key}"),
                "type": "single_line_text_field"
            }
        }),
    );
    request.headers.insert(
        "x-shopify-draft-proxy-api-client-id".to_string(),
        api_client_id.to_string(),
    );
    proxy.process_request(request).body["data"]["metafieldDefinitionCreate"].clone()
}

fn update_definition_pin(proxy: &mut DraftProxy, namespace: &str, key: &str) -> Value {
    proxy.process_request(json_graphql_request(
        r#"
        mutation MetafieldDefinitionUpdateForPinLimit($definition: MetafieldDefinitionUpdateInput!) {
          metafieldDefinitionUpdate(definition: $definition) {
            updatedDefinition { id key pinnedPosition }
            userErrors { field message code }
            validationJob { id }
          }
        }
        "#,
        json!({
            "definition": {
                "ownerType": "PRODUCT",
                "namespace": namespace,
                "key": key,
                "pin": true
            }
        }),
    )).body["data"]["metafieldDefinitionUpdate"].clone()
}

fn standard_enable_pin(proxy: &mut DraftProxy) -> Value {
    proxy.process_request(json_graphql_request(
        r#"
        mutation StandardMetafieldDefinitionEnablePinLimit($ownerType: MetafieldOwnerType!, $id: ID!) {
          standardMetafieldDefinitionEnable(ownerType: $ownerType, id: $id, pin: true) {
            createdDefinition { id namespace key pinnedPosition __shopifyDraftProxyStandardTemplateId }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "ownerType": "PRODUCT",
            "id": "gid://shopify/StandardMetafieldDefinitionTemplate/1"
        }),
    )).body["data"]["standardMetafieldDefinitionEnable"].clone()
}

#[test]
fn metafield_definition_pin_and_unpin_require_backed_definition_records() {
    let mut proxy = snapshot_proxy();

    let pin_missing = pin_definition(&mut proxy, "missing_pin_catalog", "pin_01");
    assert_eq!(pin_missing["pinnedDefinition"], Value::Null);
    assert_eq!(
        pin_missing["userErrors"],
        json!([{
            "field": null,
            "message": "Definition not found.",
            "code": "NOT_FOUND"
        }])
    );

    let unpin_missing = unpin_definition(&mut proxy, "missing_pin_catalog", "pin_01");
    assert_eq!(unpin_missing["unpinnedDefinition"], Value::Null);
    assert_eq!(
        unpin_missing["userErrors"],
        json!([{
            "field": null,
            "message": "Definition not found.",
            "code": "NOT_FOUND"
        }])
    );
}

#[test]
fn metafield_definition_unpin_compacts_product_positions_across_namespaces() {
    let mut proxy = snapshot_proxy();

    assert_eq!(
        create_definition(&mut proxy, "har1423_compact_a", "pin_01", true)["createdDefinition"]
            ["pinnedPosition"],
        json!(1)
    );
    assert_eq!(
        create_definition(&mut proxy, "har1423_compact_b", "pin_02", true)["createdDefinition"]
            ["pinnedPosition"],
        json!(2)
    );

    let unpinned = unpin_definition(&mut proxy, "har1423_compact_a", "pin_01");
    assert_eq!(unpinned["userErrors"], json!([]));
    assert_eq!(
        read_definition(&mut proxy, "har1423_compact_b", "pin_02")["pinnedPosition"],
        json!(1)
    );
    assert_eq!(
        create_definition(&mut proxy, "har1423_compact_c", "pin_03", true)["createdDefinition"]
            ["pinnedPosition"],
        json!(2)
    );
}

#[test]
fn metafield_definition_pin_unpin_and_limit_reads_stage_local_positions() {
    let mut proxy = snapshot_proxy();
    let namespace = "har1423_pin_read";
    assert_eq!(
        create_definition(&mut proxy, namespace, "pin_a", false)["userErrors"],
        json!([])
    );
    assert_eq!(
        create_definition(&mut proxy, namespace, "pin_b", false)["userErrors"],
        json!([])
    );

    let pin_a = proxy.process_request(json_graphql_request(
        r#"
        mutation MetafieldDefinitionPinByIdentifier($identifier: MetafieldDefinitionIdentifierInput!) {
          metafieldDefinitionPin(identifier: $identifier) {
            pinnedDefinition { id key pinnedPosition }
            userErrors { field message code }
          }
        }
        "#,
        json!({"identifier": {"ownerType": "PRODUCT", "namespace": namespace, "key": "pin_a"}}),
    ));
    assert_eq!(
        pin_a.body["data"]["metafieldDefinitionPin"]["userErrors"],
        json!([])
    );
    assert_eq!(
        pin_a.body["data"]["metafieldDefinitionPin"]["pinnedDefinition"]["pinnedPosition"],
        json!(1)
    );

    let pin_b = proxy.process_request(json_graphql_request(
        r#"
        mutation MetafieldDefinitionPinByIdentifier($identifier: MetafieldDefinitionIdentifierInput!) {
          metafieldDefinitionPin(identifier: $identifier) { pinnedDefinition { id key pinnedPosition } userErrors { field message code } }
        }
        "#,
        json!({"identifier": {"ownerType": "PRODUCT", "namespace": namespace, "key": "pin_b"}}),
    ));
    assert_eq!(
        pin_b.body["data"]["metafieldDefinitionPin"]["pinnedDefinition"]["pinnedPosition"],
        json!(2)
    );

    let read = proxy.process_request(json_graphql_request(
        r#"
        query MetafieldDefinitionPinningRead($namespace: String!) {
          byIdentifier: metafieldDefinition(identifier: { ownerType: PRODUCT, namespace: $namespace, key: "pin_a" }) { id key pinnedPosition }
          pinned: metafieldDefinitions(ownerType: PRODUCT, first: 10, namespace: $namespace, sortKey: PINNED_POSITION, pinnedStatus: PINNED) { nodes { key pinnedPosition } }
        }
        "#,
        json!({"namespace": namespace}),
    ));
    assert_eq!(
        read.body["data"]["byIdentifier"]["pinnedPosition"],
        json!(1)
    );
    assert_eq!(
        read.body["data"]["pinned"]["nodes"],
        json!([
            {"key": "pin_b", "pinnedPosition": 2},
            {"key": "pin_a", "pinnedPosition": 1}
        ])
    );

    let unpin_a = proxy.process_request(json_graphql_request(
        r#"
        mutation MetafieldDefinitionUnpinByIdentifier($identifier: MetafieldDefinitionIdentifierInput!) {
          metafieldDefinitionUnpin(identifier: $identifier) { unpinnedDefinition { id key pinnedPosition } userErrors { field message code } }
        }
        "#,
        json!({"identifier": {"ownerType": "PRODUCT", "namespace": namespace, "key": "pin_a"}}),
    ));
    assert_eq!(
        unpin_a.body["data"]["metafieldDefinitionUnpin"]["unpinnedDefinition"]["pinnedPosition"],
        Value::Null
    );
}

#[test]
fn metafield_definition_pin_limit_is_twenty_for_pin_create_update_and_standard_enable() {
    let namespace = "har1423_pin_limit";

    let mut pin_proxy = snapshot_proxy();
    for index in 1..=21 {
        let key = format!("pin_{index:02}");
        let created = create_definition(&mut pin_proxy, namespace, &key, false);
        assert_eq!(created["userErrors"], json!([]));
    }
    for index in 1..=20 {
        let key = format!("pin_{index:02}");
        let pinned = pin_definition(&mut pin_proxy, namespace, &key);
        assert_eq!(pinned["userErrors"], json!([]));
        if index == 20 {
            assert_eq!(pinned["pinnedDefinition"]["pinnedPosition"], json!(20));
        }
    }
    let over_cap_pin = pin_definition(&mut pin_proxy, namespace, "pin_21");
    assert_eq!(over_cap_pin["pinnedDefinition"], Value::Null);
    assert_eq!(
        over_cap_pin["userErrors"],
        json!([{
            "field": null,
            "message": "Limit of 20 pinned definitions.",
            "code": "PINNED_LIMIT_REACHED"
        }])
    );
    let constrained = pin_proxy.process_request(json_graphql_request(
        r#"
        mutation MetafieldDefinitionConstrainedAtPinLimit($namespace: String!, $categoryId: String!) {
          constrainedCreate: metafieldDefinitionCreate(definition: { ownerType: PRODUCT, namespace: $namespace, key: "constrained", name: "Constrained", type: "single_line_text_field", constraints: { key: "category", values: [$categoryId] } }) { createdDefinition { id } userErrors { field message code } }
          constrainedPin: metafieldDefinitionPin(identifier: { ownerType: PRODUCT, namespace: $namespace, key: "constrained" }) { pinnedDefinition { id } userErrors { field message code } }
        }
        "#,
        json!({"namespace": namespace, "categoryId": "gid://shopify/TaxonomyCategory/sg-4-17-2-17"}),
    ));
    assert_eq!(
        constrained.body["data"]["constrainedPin"]["userErrors"][0]["code"],
        json!("UNSUPPORTED_PINNING")
    );

    let mut create_proxy = snapshot_proxy();
    for index in 1..=20 {
        let key = format!("pin_{index:02}");
        let created = create_definition(&mut create_proxy, "har1423_create_limit", &key, true);
        assert_eq!(created["userErrors"], json!([]));
        if index == 20 {
            assert_eq!(created["createdDefinition"]["pinnedPosition"], json!(20));
        }
    }
    let over_cap_create =
        create_definition(&mut create_proxy, "har1423_create_limit", "pin_21", true);
    assert_eq!(over_cap_create["createdDefinition"], Value::Null);
    assert_eq!(
        over_cap_create["userErrors"][0]["message"],
        json!("Limit of 20 pinned definitions.")
    );
    assert_eq!(
        over_cap_create["userErrors"][0]["code"],
        json!("PINNED_LIMIT_REACHED")
    );

    let mut update_proxy = snapshot_proxy();
    for index in 1..=21 {
        let key = format!("pin_{index:02}");
        assert_eq!(
            create_definition(&mut update_proxy, "har1423_update_limit", &key, false)["userErrors"],
            json!([])
        );
    }
    for index in 1..=20 {
        let key = format!("pin_{index:02}");
        let updated = update_definition_pin(&mut update_proxy, "har1423_update_limit", &key);
        assert_eq!(updated["userErrors"], json!([]));
        if index == 20 {
            assert_eq!(updated["updatedDefinition"]["pinnedPosition"], json!(20));
        }
    }
    let over_cap_update =
        update_definition_pin(&mut update_proxy, "har1423_update_limit", "pin_21");
    assert_eq!(over_cap_update["updatedDefinition"], Value::Null);
    assert_eq!(
        over_cap_update["userErrors"][0]["message"],
        json!("Limit of 20 pinned definitions.")
    );
    assert_eq!(
        over_cap_update["userErrors"][0]["code"],
        json!("PINNED_LIMIT_REACHED")
    );

    let mut standard_proxy = snapshot_proxy();
    for index in 1..=20 {
        let key = format!("pin_{index:02}");
        assert_eq!(
            create_definition(&mut standard_proxy, "har1423_standard_limit", &key, true)
                ["userErrors"],
            json!([])
        );
    }
    let over_cap_standard = standard_enable_pin(&mut standard_proxy);
    assert_eq!(over_cap_standard["createdDefinition"], Value::Null);
    assert_eq!(
        over_cap_standard["userErrors"][0]["message"],
        json!("Limit of 20 pinned definitions.")
    );
    assert_eq!(
        over_cap_standard["userErrors"][0]["code"],
        json!("PINNED_LIMIT_REACHED")
    );
}

#[test]
fn metafield_definition_create_resource_type_limit_is_scoped_by_owner_and_app_namespace() {
    let mut proxy = snapshot_proxy();

    for index in 0..256 {
        let created = create_definition_for_resource_limit(
            &mut proxy,
            "PRODUCT",
            "resource_limit_merchant",
            &format!("key_{index:03}"),
        );
        assert_eq!(created["userErrors"], json!([]));
    }

    let over_limit = create_definition_for_resource_limit(
        &mut proxy,
        "PRODUCT",
        "resource_limit_merchant",
        "key_256",
    );
    assert_eq!(over_limit["createdDefinition"], Value::Null);
    assert_eq!(
        over_limit["userErrors"],
        json!([{
            "field": ["definition"],
            "message": "You can only have 256 definitions per resource type.",
            "code": "RESOURCE_TYPE_LIMIT_EXCEEDED"
        }])
    );

    let read_rejected = proxy.process_request(json_graphql_request(
        r#"
        query RejectedMetafieldDefinitionRead($identifier: MetafieldDefinitionIdentifierInput!) {
          metafieldDefinition(identifier: $identifier) { id }
        }
        "#,
        json!({"identifier": {"ownerType": "PRODUCT", "namespace": "resource_limit_merchant", "key": "key_256"}}),
    ));
    assert_eq!(
        read_rejected.body["data"]["metafieldDefinition"],
        Value::Null
    );

    let customer_created = create_definition_for_resource_limit(
        &mut proxy,
        "CUSTOMER",
        "resource_limit_merchant",
        "customer_key",
    );
    assert_eq!(customer_created["userErrors"], json!([]));

    // App-reserved namespaces (`app--<id>--…`) can only be written by the app
    // that owns them: Shopify rejects cross-app definition writes with a top-level
    // ACCESS_DENIED (see `metafield-definition-app-namespace-resolution` parity
    // capture). Authenticate as each app via the api-client-id header so the
    // `$app:` namespace resolves to that app's own reserved namespace, proving the
    // resource-type limit is bucketed per app independently of the merchant bucket.
    let app_one_created =
        create_app_definition_for_resource_limit(&mut proxy, "111", "app_one_key");
    assert_eq!(app_one_created["userErrors"], json!([]));
    assert_eq!(
        app_one_created["createdDefinition"]["namespace"],
        json!("app--111--resource_limit")
    );

    for index in 0..256 {
        let created =
            create_app_definition_for_resource_limit(&mut proxy, "222", &format!("key_{index:03}"));
        assert_eq!(created["userErrors"], json!([]));
    }
    let over_app_limit = create_app_definition_for_resource_limit(&mut proxy, "222", "key_256");
    assert_eq!(over_app_limit["createdDefinition"], Value::Null);
    assert_eq!(
        over_app_limit["userErrors"][0]["code"],
        json!("RESOURCE_TYPE_LIMIT_EXCEEDED")
    );

    // A second namespace under the same app shares that app's bucket and is still
    // well under the limit, so it succeeds.
    let app_one_second_namespace_created =
        create_app_definition_for_resource_limit(&mut proxy, "111", "default_key");
    assert_eq!(app_one_second_namespace_created["userErrors"], json!([]));

    let app_three_created =
        create_app_definition_for_resource_limit(&mut proxy, "333", "app_three_key");
    assert_eq!(app_three_created["userErrors"], json!([]));
    assert_eq!(
        app_three_created["createdDefinition"]["namespace"],
        json!("app--333--resource_limit")
    );

    let standard_enabled = standard_enable_pin(&mut proxy);
    assert_eq!(standard_enabled["userErrors"], json!([]));
    assert_eq!(
        standard_enabled["createdDefinition"]["namespace"],
        json!("descriptors")
    );
    assert_eq!(
        standard_enabled["createdDefinition"].get("__shopifyDraftProxyStandardTemplateId"),
        None
    );

    let mut standard_exclusion_proxy = snapshot_proxy();
    let standard_first = standard_enable_pin(&mut standard_exclusion_proxy);
    assert_eq!(standard_first["userErrors"], json!([]));
    for index in 0..256 {
        let created = create_definition_for_resource_limit(
            &mut standard_exclusion_proxy,
            "PRODUCT",
            "resource_limit_after_standard",
            &format!("key_{index:03}"),
        );
        assert_eq!(created["userErrors"], json!([]));
    }
    let standard_exclusion_over_limit = create_definition_for_resource_limit(
        &mut standard_exclusion_proxy,
        "PRODUCT",
        "resource_limit_after_standard",
        "key_256",
    );
    assert_eq!(
        standard_exclusion_over_limit["userErrors"][0]["code"],
        json!("RESOURCE_TYPE_LIMIT_EXCEEDED")
    );
}

#[test]
fn metafield_definition_capability_eligibility_matches_shopify() {
    let mut proxy = snapshot_proxy();
    let namespace = "capability_eligibility_rust";

    let invalid_unique = proxy.process_request(json_graphql_request(
        r#"
        mutation InvalidUniqueCapability($namespace: String!) {
          metafieldDefinitionCreate(definition: {
            ownerType: PRODUCT
            namespace: $namespace
            key: "json_unique"
            name: "JSON Unique"
            type: "json"
            capabilities: { uniqueValues: { enabled: true } }
          }) {
            createdDefinition { id }
            userErrors { field message code }
          }
          rejectedRead: metafieldDefinition(identifier: { ownerType: PRODUCT, namespace: $namespace, key: "json_unique" }) {
            id
          }
        }
        "#,
        json!({ "namespace": namespace }),
    ));
    assert_eq!(
        invalid_unique.body["data"]["metafieldDefinitionCreate"],
        json!({
            "createdDefinition": null,
            "userErrors": [{
                "field": ["definition"],
                "message": "The capability unique_values is not valid for this definition.",
                "code": "INVALID_CAPABILITY"
            }]
        })
    );
    assert_eq!(invalid_unique.body["data"]["rejectedRead"], Value::Null);

    let invalid_smart = proxy.process_request(json_graphql_request(
        r#"
        mutation InvalidSmartCapability($namespace: String!) {
          metafieldDefinitionCreate(definition: {
            ownerType: CUSTOMER
            namespace: $namespace
            key: "customer_smart"
            name: "Customer Smart"
            type: "single_line_text_field"
            capabilities: { smartCollectionCondition: { enabled: true } }
          }) {
            createdDefinition { id }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "namespace": namespace }),
    ));
    assert_eq!(
        invalid_smart.body["data"]["metafieldDefinitionCreate"],
        json!({
            "createdDefinition": null,
            "userErrors": [{
                "field": ["definition"],
                "message": "The capability smart_collection_condition is not valid for this definition.",
                "code": "INVALID_CAPABILITY"
            }]
        })
    );

    let id_create = proxy.process_request(json_graphql_request(
        r#"
        mutation IdCapabilityAutoEnable($namespace: String!) {
          metafieldDefinitionCreate(definition: {
            ownerType: PRODUCT
            namespace: $namespace
            key: "external_id"
            name: "External ID"
            type: "id"
          }) {
            createdDefinition {
              type { name category }
              capabilities {
                adminFilterable { enabled eligible status }
                smartCollectionCondition { enabled eligible }
                uniqueValues { enabled eligible }
              }
            }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "namespace": namespace }),
    ));
    assert_eq!(
        id_create.body["data"]["metafieldDefinitionCreate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        id_create.body["data"]["metafieldDefinitionCreate"]["createdDefinition"],
        json!({
            "type": { "name": "id", "category": "ID" },
            "capabilities": {
                "adminFilterable": { "enabled": false, "eligible": true, "status": "NOT_FILTERABLE" },
                "smartCollectionCondition": { "enabled": false, "eligible": false },
                "uniqueValues": { "enabled": true, "eligible": true }
            }
        })
    );

    let id_disabled = proxy.process_request(json_graphql_request(
        r#"
        mutation IdCapabilityExplicitDisable($namespace: String!) {
          metafieldDefinitionCreate(definition: {
            ownerType: PRODUCT
            namespace: $namespace
            key: "external_id_disabled"
            name: "External ID disabled"
            type: "id"
            capabilities: { uniqueValues: { enabled: false } }
          }) {
            createdDefinition { capabilities { uniqueValues { enabled eligible } } }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "namespace": namespace }),
    ));
    assert_eq!(
        id_disabled.body["data"]["metafieldDefinitionCreate"]["createdDefinition"]["capabilities"]
            ["uniqueValues"],
        json!({ "enabled": false, "eligible": true })
    );

    let json_base = proxy.process_request(json_graphql_request(
        r#"
        mutation JsonCapabilityReadback($namespace: String!) {
          metafieldDefinitionCreate(definition: {
            ownerType: PRODUCT
            namespace: $namespace
            key: "json_payload"
            name: "JSON Payload"
            type: "json"
          }) {
            createdDefinition {
              capabilities {
                adminFilterable { enabled eligible status }
                smartCollectionCondition { enabled eligible }
                uniqueValues { enabled eligible }
              }
            }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "namespace": namespace }),
    ));
    assert_eq!(
        json_base.body["data"]["metafieldDefinitionCreate"]["createdDefinition"]["capabilities"],
        json!({
            "adminFilterable": { "enabled": false, "eligible": false, "status": "NOT_FILTERABLE" },
            "smartCollectionCondition": { "enabled": false, "eligible": false },
            "uniqueValues": { "enabled": false, "eligible": false }
        })
    );

    let update_unique = proxy.process_request(json_graphql_request(
        r#"
        mutation InvalidUpdateUniqueCapability($namespace: String!) {
          metafieldDefinitionUpdate(definition: {
            ownerType: PRODUCT
            namespace: $namespace
            key: "json_payload"
            capabilities: { uniqueValues: { enabled: true } }
          }) {
            updatedDefinition { id }
            userErrors { field message code }
            validationJob { id }
          }
        }
        "#,
        json!({ "namespace": namespace }),
    ));
    assert_eq!(
        update_unique.body["data"]["metafieldDefinitionUpdate"],
        json!({
            "updatedDefinition": null,
            "userErrors": [{
                "field": ["definition"],
                "message": "The capability unique_values is not valid for this definition.",
                "code": "INVALID_CAPABILITY"
            }],
            "validationJob": null
        })
    );
}

#[test]
fn metafield_definition_admin_filterable_cap_is_fifty_per_owner_type() {
    let mut proxy = snapshot_proxy();
    let namespace = "admin_filter_cap_rust";

    for index in 1..=50 {
        let key = format!("admin_{index:02}");
        let created = proxy.process_request(json_graphql_request(
            r#"
            mutation AdminFilterableCreate($definition: MetafieldDefinitionInput!) {
              metafieldDefinitionCreate(definition: $definition) {
                createdDefinition {
                  key
                  capabilities { adminFilterable { enabled eligible status } }
                }
                userErrors { field message code }
              }
            }
            "#,
            json!({
                "definition": {
                    "ownerType": "PRODUCT",
                    "namespace": namespace,
                    "key": key,
                    "name": format!("Admin Filter {index:02}"),
                    "type": "single_line_text_field",
                    "capabilities": { "adminFilterable": { "enabled": true } }
                }
            }),
        ));
        assert_eq!(
            created.body["data"]["metafieldDefinitionCreate"]["userErrors"],
            json!([])
        );
        if index == 50 {
            assert_eq!(
                created.body["data"]["metafieldDefinitionCreate"]["createdDefinition"]
                    ["capabilities"]["adminFilterable"],
                json!({ "enabled": true, "eligible": true, "status": "FILTERABLE" })
            );
        }
    }

    let over_limit = proxy.process_request(json_graphql_request(
        r#"
        mutation AdminFilterableLimit($definition: MetafieldDefinitionInput!) {
          metafieldDefinitionCreate(definition: $definition) {
            createdDefinition { id }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "definition": {
                "ownerType": "PRODUCT",
                "namespace": namespace,
                "key": "admin_51",
                "name": "Admin Filter 51",
                "type": "single_line_text_field",
                "capabilities": { "adminFilterable": { "enabled": true } }
            }
        }),
    ));
    assert_eq!(
        over_limit.body["data"]["metafieldDefinitionCreate"],
        json!({
            "createdDefinition": null,
            "userErrors": [{
                "field": ["definition"],
                "message": "You can only use 50 product metafield definitions to filter the product list. To add a new filter, disable filtering on an existing one.",
                "code": "OWNER_TYPE_LIMIT_EXCEEDED_FOR_USE_AS_ADMIN_FILTERS"
            }]
        })
    );

    let customer_allowed = proxy.process_request(json_graphql_request(
        r#"
        mutation CustomerAdminFilterableCreate($definition: MetafieldDefinitionInput!) {
          metafieldDefinitionCreate(definition: $definition) {
            createdDefinition { ownerType capabilities { adminFilterable { enabled status } } }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "definition": {
                "ownerType": "CUSTOMER",
                "namespace": namespace,
                "key": "customer_admin",
                "name": "Customer admin",
                "type": "single_line_text_field",
                "capabilities": { "adminFilterable": { "enabled": true } }
            }
        }),
    ));
    assert_eq!(
        customer_allowed.body["data"]["metafieldDefinitionCreate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        customer_allowed.body["data"]["metafieldDefinitionCreate"]["createdDefinition"]
            ["capabilities"]["adminFilterable"],
        json!({ "enabled": true, "status": "FILTERABLE" })
    );
}

#[test]
fn standard_metafield_definition_enable_rejects_ineligible_capabilities() {
    let mut proxy = snapshot_proxy();

    let invalid_unique = proxy.process_request(json_graphql_request(
        r#"
        mutation StandardInvalidUnique {
          standardMetafieldDefinitionEnable(
            ownerType: PRODUCT
            id: "gid://shopify/StandardMetafieldDefinitionTemplate/10004"
            capabilities: { uniqueValues: { enabled: true } }
          ) {
            createdDefinition { namespace key }
            userErrors { field message code }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        invalid_unique.body["data"]["standardMetafieldDefinitionEnable"],
        json!({
            "createdDefinition": null,
            "userErrors": [{
                "field": null,
                "message": "The capability unique_values is not valid for this definition.",
                "code": "INVALID_CAPABILITY"
            }]
        })
    );

    let invalid_smart = proxy.process_request(json_graphql_request(
        r#"
        mutation StandardInvalidSmart {
          standardMetafieldDefinitionEnable(
            ownerType: PRODUCT
            id: "gid://shopify/StandardMetafieldDefinitionTemplate/2"
            capabilities: { smartCollectionCondition: { enabled: true } }
          ) {
            createdDefinition { namespace key }
            userErrors { field message code }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        invalid_smart.body["data"]["standardMetafieldDefinitionEnable"]["userErrors"],
        json!([{
            "field": null,
            "message": "The capability smart_collection_condition is not valid for this definition.",
            "code": "INVALID_CAPABILITY"
        }])
    );

    let enabled_admin_filter = proxy.process_request(json_graphql_request(
        r#"
        mutation StandardAdminFilterable {
          standardMetafieldDefinitionEnable(
            ownerType: PRODUCT
            id: "gid://shopify/StandardMetafieldDefinitionTemplate/1"
            capabilities: { adminFilterable: { enabled: true } }
          ) {
            createdDefinition {
              namespace
              key
              capabilities {
                adminFilterable { enabled eligible status }
                smartCollectionCondition { enabled eligible }
                uniqueValues { enabled eligible }
              }
            }
            userErrors { field message code }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        enabled_admin_filter.body["data"]["standardMetafieldDefinitionEnable"],
        json!({
            "createdDefinition": {
                "namespace": "descriptors",
                "key": "subtitle",
                "capabilities": {
                    "adminFilterable": { "enabled": true, "eligible": true, "status": "FILTERABLE" },
                    "smartCollectionCondition": { "enabled": false, "eligible": true },
                    "uniqueValues": { "enabled": false, "eligible": true }
                }
            },
            "userErrors": []
        })
    );
}

#[test]
fn metafield_definition_capability_create_stays_local_and_logs_raw_mutation() {
    let mut proxy = snapshot_proxy();
    let query = r#"
        mutation CapabilityLogProof($definition: MetafieldDefinitionInput!) {
          metafieldDefinitionCreate(definition: $definition) {
            createdDefinition {
              namespace
              key
              capabilities { adminFilterable { enabled status } }
            }
            userErrors { field message code }
          }
        }
        "#;
    let request = json_graphql_request(
        query,
        json!({
            "definition": {
                "ownerType": "PRODUCT",
                "namespace": "capability_log_rust",
                "key": "admin_filter",
                "name": "Admin filter",
                "type": "single_line_text_field",
                "capabilities": { "adminFilterable": { "enabled": true } }
            }
        }),
    );
    let response = proxy.process_request(request);
    assert_eq!(
        response.body["data"]["metafieldDefinitionCreate"]["userErrors"],
        json!([])
    );

    let log = proxy.process_request(Request {
        method: "GET".to_string(),
        path: "/__meta/log".to_string(),
        headers: Default::default(),
        body: String::new(),
    });
    assert_eq!(log.status, 200);
    assert_eq!(log.body["entries"][0]["status"], json!("staged"));
    assert_eq!(
        log.body["entries"][0]["interpreted"]["rootFields"],
        json!(["metafieldDefinitionCreate"])
    );
    assert_eq!(
        log.body["entries"][0]["interpreted"]["primaryRootField"],
        json!("metafieldDefinitionCreate")
    );
    assert_eq!(
        log.body["entries"][0]["stagedResourceIds"]
            .as_array()
            .map(Vec::len),
        Some(1)
    );
    assert_eq!(log.body["entries"][0]["query"], json!(query));
    assert_eq!(
        log.body["entries"][0]["variables"]["definition"]["capabilities"]["adminFilterable"]
            ["enabled"],
        json!(true)
    );
    assert!(log.body["entries"][0]["rawBody"]
        .as_str()
        .unwrap()
        .contains("CapabilityLogProof"));
}

#[test]
fn metafield_definition_lifecycle_mutations_validate_and_stage_real_inputs() {
    let mut proxy = snapshot_proxy();

    let invalid = proxy.process_request(json_graphql_request(
        r#"
        mutation MetafieldDefinitionCreateValidation($definition: MetafieldDefinitionInput!) {
          metafieldDefinitionCreate(definition: $definition) {
            createdDefinition { id }
            userErrors { __typename field message code }
          }
        }
        "#,
        json!({
            "definition": {
                "namespace": "ab",
                "key": "x",
                "ownerType": "PRODUCT",
                "name": "X",
                "type": "single_line_text_field"
            }
        }),
    ));
    assert_eq!(
        invalid.body["data"]["metafieldDefinitionCreate"],
        json!({
            "createdDefinition": null,
            "userErrors": [
                {
                    "__typename": "MetafieldDefinitionCreateUserError",
                    "field": ["definition", "namespace"],
                    "message": "Namespace is too short (minimum is 3 characters)",
                    "code": "TOO_SHORT"
                },
                {
                    "__typename": "MetafieldDefinitionCreateUserError",
                    "field": ["definition", "key"],
                    "message": "Key is too short (minimum is 2 characters)",
                    "code": "TOO_SHORT"
                }
            ]
        })
    );

    let created = proxy.process_request(json_graphql_request(
        r#"
        mutation MetafieldDefinitionCreateRealInput($definition: MetafieldDefinitionInput!) {
          metafieldDefinitionCreate(definition: $definition) {
            createdDefinition {
              id
              namespace
              key
              name
              ownerType
              type { name category }
              access { admin storefront customerAccount }
              validations { name value }
              constraints { key values(first: 5) { nodes { value } } }
              pinnedPosition
            }
            userErrors { __typename field message code }
          }
        }
        "#,
        json!({
            "definition": {
                "namespace": "customer_loyalty",
                "key": "tier",
                "ownerType": "CUSTOMER",
                "name": "Loyalty tier",
                "type": "json",
                "access": { "admin": "MERCHANT_READ_WRITE" },
                "validations": [{ "name": "schema", "value": "{\"type\":\"string\"}" }]
            }
        }),
    ));
    let created_definition =
        &created.body["data"]["metafieldDefinitionCreate"]["createdDefinition"];
    assert_eq!(created.status, 200);
    assert!(created_definition["id"]
        .as_str()
        .unwrap()
        .contains("gid://shopify/MetafieldDefinition/"));
    assert_eq!(
        created_definition,
        &json!({
            "id": created_definition["id"].clone(),
            "namespace": "customer_loyalty",
            "key": "tier",
            "name": "Loyalty tier",
            "ownerType": "CUSTOMER",
            "type": { "name": "json", "category": "JSON" },
            "access": { "admin": "PUBLIC_READ_WRITE", "storefront": "NONE", "customerAccount": "NONE" },
            "validations": [{ "name": "schema", "value": "{\"type\":\"string\"}" }],
            "constraints": { "key": null, "values": { "nodes": [] } },
            "pinnedPosition": null
        })
    );

    let read = proxy.process_request(json_graphql_request(
        r#"
        query MetafieldDefinitionReadAfterCreate($namespace: String!) {
          definition: metafieldDefinition(identifier: { ownerType: CUSTOMER, namespace: $namespace, key: "tier" }) {
            namespace
            key
            ownerType
            type { name }
            validations { name value }
          }
        }
        "#,
        json!({ "namespace": "customer_loyalty" }),
    ));
    assert_eq!(
        read.body["data"]["definition"],
        json!({
            "namespace": "customer_loyalty",
            "key": "tier",
            "ownerType": "CUSTOMER",
            "type": { "name": "json" },
            "validations": [{ "name": "schema", "value": "{\"type\":\"string\"}" }]
        })
    );

    let updated = proxy.process_request(json_graphql_request(
        r#"
        mutation MetafieldDefinitionUpdateLocal($definition: MetafieldDefinitionUpdateInput!) {
          metafieldDefinitionUpdate(definition: $definition) {
            updatedDefinition {
              namespace
              key
              ownerType
              name
              description
              validations { name value }
              constraints { key values(first: 5) { nodes { value } } }
            }
            userErrors { __typename field message code }
            validationJob { __typename id done query { __typename } }
          }
        }
        "#,
        json!({
            "definition": {
                "namespace": "customer_loyalty",
                "key": "tier",
                "ownerType": "CUSTOMER",
                "name": "Updated tier",
                "description": "Readable tier",
                "validations": [{ "name": "max", "value": "32" }],
                "constraintsUpdates": {
                    "key": "category",
                    "values": [{ "create": "gid://shopify/TaxonomyCategory/ap-2" }]
                }
            }
        }),
    ));
    assert_eq!(
        updated.body["data"]["metafieldDefinitionUpdate"]["updatedDefinition"],
        json!({
            "namespace": "customer_loyalty",
            "key": "tier",
            "ownerType": "CUSTOMER",
            "name": "Updated tier",
            "description": "Readable tier",
            "validations": [{ "name": "max", "value": "32" }],
                "constraints": {
                    "key": "category",
                    "values": {
                    "nodes": [{ "value": "ap-2" }]
                    }
                }
        })
    );
    assert_eq!(
        updated.body["data"]["metafieldDefinitionUpdate"]["validationJob"]["__typename"],
        json!("Job")
    );
    assert_eq!(
        updated.body["data"]["metafieldDefinitionUpdate"]["validationJob"]["done"],
        json!(false)
    );

    let empty_constraints = proxy.process_request(json_graphql_request(
        r#"
        mutation MetafieldDefinitionUpdateEmptyConstraintValues($definition: MetafieldDefinitionUpdateInput!) {
          metafieldDefinitionUpdate(definition: $definition) {
            updatedDefinition { id }
            userErrors { __typename field message code }
          }
        }
        "#,
        json!({
            "definition": {
                "namespace": "customer_loyalty",
                "key": "tier",
                "ownerType": "CUSTOMER",
                "constraintsUpdates": { "key": "category", "values": [] }
            }
        }),
    ));
    assert_eq!(
        empty_constraints.body["data"]["metafieldDefinitionUpdate"],
        json!({
            "updatedDefinition": null,
            "userErrors": [{
                "__typename": "MetafieldDefinitionUpdateUserError",
                "field": ["definition"],
                "message": "Cannot change the constraint key without providing values.",
                "code": "INVALID_INPUT"
            }]
        })
    );
}

#[test]
fn metafield_definition_delete_enforces_type_guards_without_associated_values() {
    let mut proxy = snapshot_proxy();

    let cases = [
        (
            "id_no_values",
            "uid",
            "id",
            false,
            "ID_TYPE_DELETION_ERROR",
            "Deleting an id type metafield definition requires deletion of its associated metafields.",
        ),
        (
            "reference_no_values",
            "target",
            "product_reference",
            true,
            "REFERENCE_TYPE_DELETION_ERROR",
            "Deleting a reference type metafield definition requires deletion of its associated metafields.",
        ),
        (
            "list_reference_no_values",
            "targets",
            "list.product_reference",
            false,
            "REFERENCE_TYPE_DELETION_ERROR",
            "Deleting a reference type metafield definition requires deletion of its associated metafields.",
        ),
    ];

    for (namespace, key, metafield_type, include_false_flag, expected_code, expected_message) in
        cases
    {
        let create = proxy.process_request(json_graphql_request(
            r#"
            mutation CreateDefinition($definition: MetafieldDefinitionInput!) {
              metafieldDefinitionCreate(definition: $definition) {
                createdDefinition { id namespace key type { name } }
                userErrors { __typename field message code }
              }
            }
            "#,
            json!({
                "definition": {
                    "name": format!("Delete guard {key}"),
                    "namespace": namespace,
                    "key": key,
                    "ownerType": "PRODUCT",
                    "type": metafield_type
                }
            }),
        ));
        assert_eq!(
            create.body["data"]["metafieldDefinitionCreate"]["userErrors"],
            json!([])
        );

        let delete_query = if include_false_flag {
            r#"
            mutation DeleteDefinition($namespace: String!, $key: String!) {
              metafieldDefinitionDelete(
                identifier: { ownerType: PRODUCT, namespace: $namespace, key: $key }
                deleteAllAssociatedMetafields: false
              ) {
                deletedDefinitionId
                deletedDefinition { ownerType namespace key }
                userErrors { __typename field message code }
              }
            }
            "#
        } else {
            r#"
            mutation DeleteDefinition($namespace: String!, $key: String!) {
              metafieldDefinitionDelete(
                identifier: { ownerType: PRODUCT, namespace: $namespace, key: $key }
              ) {
                deletedDefinitionId
                deletedDefinition { ownerType namespace key }
                userErrors { __typename field message code }
              }
            }
            "#
        };
        let guarded_delete = proxy.process_request(json_graphql_request(
            delete_query,
            json!({ "namespace": namespace, "key": key }),
        ));
        assert_eq!(
            guarded_delete.body["data"]["metafieldDefinitionDelete"],
            json!({
                "deletedDefinitionId": null,
                "deletedDefinition": null,
                "userErrors": [{
                    "__typename": "MetafieldDefinitionDeleteUserError",
                    "field": null,
                    "message": expected_message,
                    "code": expected_code
                }]
            })
        );
    }
}

#[test]
fn metafield_definition_delete_keeps_type_guard_exceptions_without_associated_values() {
    let mut proxy = snapshot_proxy();

    let text_create = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateTextDefinition {
          metafieldDefinitionCreate(
            definition: {
              name: "Delete text target"
              namespace: "delete_text_no_values"
              key: "label"
              ownerType: PRODUCT
              type: "single_line_text_field"
            }
          ) {
            createdDefinition { id namespace key type { name } }
            userErrors { __typename field message code }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        text_create.body["data"]["metafieldDefinitionCreate"]["userErrors"],
        json!([])
    );

    let text_delete = proxy.process_request(json_graphql_request(
        r#"
        mutation DeleteTextDefinition {
          metafieldDefinitionDelete(
            identifier: { ownerType: PRODUCT, namespace: "delete_text_no_values", key: "label" }
          ) {
            deletedDefinitionId
            deletedDefinition { ownerType namespace key }
            userErrors { __typename field message code }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        text_delete.body["data"]["metafieldDefinitionDelete"]["deletedDefinition"],
        json!({
            "ownerType": "PRODUCT",
            "namespace": "delete_text_no_values",
            "key": "label"
        })
    );
    assert_eq!(
        text_delete.body["data"]["metafieldDefinitionDelete"]["userErrors"],
        json!([])
    );

    let reference_create = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateReferenceDefinition {
          metafieldDefinitionCreate(
            definition: {
              name: "Delete reference with flag"
              namespace: "delete_reference_no_values_with_flag"
              key: "target"
              ownerType: PRODUCT
              type: "product_reference"
            }
          ) {
            createdDefinition { id namespace key type { name } }
            userErrors { __typename field message code }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        reference_create.body["data"]["metafieldDefinitionCreate"]["userErrors"],
        json!([])
    );

    let reference_delete = proxy.process_request(json_graphql_request(
        r#"
        mutation DeleteReferenceDefinition {
          metafieldDefinitionDelete(
            identifier: { ownerType: PRODUCT, namespace: "delete_reference_no_values_with_flag", key: "target" }
            deleteAllAssociatedMetafields: true
          ) {
            deletedDefinitionId
            deletedDefinition { ownerType namespace key }
            userErrors { __typename field message code }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        reference_delete.body["data"]["metafieldDefinitionDelete"]["deletedDefinition"],
        json!({
            "ownerType": "PRODUCT",
            "namespace": "delete_reference_no_values_with_flag",
            "key": "target"
        })
    );
    assert_eq!(
        reference_delete.body["data"]["metafieldDefinitionDelete"]["userErrors"],
        json!([])
    );
}

#[test]
fn metafield_definition_delete_enforces_reference_guards_and_removes_associated_values() {
    let mut proxy = snapshot_proxy();
    let namespace = "delete_reference_guard";
    let owner_id = "gid://shopify/Product/10173064872242";

    let seed = proxy.process_request(request_with_body(
        "POST",
        "/__meta/seed",
        &json!({
            "products": [{
                "id": owner_id,
                "title": "Delete reference guard product",
                "handle": "delete-reference-guard-product",
                "status": "ACTIVE"
            }]
        })
        .to_string(),
    ));
    assert_eq!(seed.status, 200);

    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateReferenceDefinition($namespace: String!) {
          metafieldDefinitionCreate(
            definition: {
              name: "Delete target"
              namespace: $namespace
              key: "target"
              ownerType: PRODUCT
              type: "product_reference"
            }
          ) {
            createdDefinition { id namespace key type { name } }
            userErrors { __typename field message code }
          }
        }
        "#,
        json!({ "namespace": namespace }),
    ));
    assert_eq!(
        create.body["data"]["metafieldDefinitionCreate"]["userErrors"],
        json!([])
    );

    let set = proxy.process_request(json_graphql_request(
        r#"
        mutation MetafieldDefinitionLifecycleMetafieldsSet($metafields: [MetafieldsSetInput!]!) {
          metafieldsSet(metafields: $metafields) {
            metafields { namespace key value }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "metafields": [{
                "ownerId": owner_id,
                "namespace": namespace,
                "key": "target",
                "type": "product_reference",
                "value": owner_id
            }]
        }),
    ));
    assert_eq!(set.body["data"]["metafieldsSet"]["userErrors"], json!([]));

    let guarded_delete = proxy.process_request(json_graphql_request(
        r#"
        mutation DeleteReferenceDefinitionGuard($namespace: String!) {
          metafieldDefinitionDelete(
            identifier: { ownerType: PRODUCT, namespace: $namespace, key: "target" }
            deleteAllAssociatedMetafields: false
          ) {
            deletedDefinitionId
            userErrors { __typename field message code }
          }
        }
        "#,
        json!({ "namespace": namespace }),
    ));
    assert_eq!(
        guarded_delete.body["data"]["metafieldDefinitionDelete"],
        json!({
            "deletedDefinitionId": null,
            "userErrors": [{
                "__typename": "MetafieldDefinitionDeleteUserError",
                "field": null,
                "message": "Deleting a reference type metafield definition requires deletion of its associated metafields.",
                "code": "REFERENCE_TYPE_DELETION_ERROR"
            }]
        })
    );

    let delete_all = proxy.process_request(json_graphql_request(
        r#"
        mutation DeleteReferenceDefinitionAll($namespace: String!) {
          metafieldDefinitionDelete(
            identifier: { ownerType: PRODUCT, namespace: $namespace, key: "target" }
            deleteAllAssociatedMetafields: true
          ) {
            deletedDefinitionId
            deletedDefinition { ownerType namespace key }
            userErrors { __typename field message code }
          }
        }
        "#,
        json!({ "namespace": namespace }),
    ));
    assert_eq!(
        delete_all.body["data"]["metafieldDefinitionDelete"]["deletedDefinition"],
        json!({
            "ownerType": "PRODUCT",
            "namespace": namespace,
            "key": "target"
        })
    );
    assert_eq!(
        delete_all.body["data"]["metafieldDefinitionDelete"]["userErrors"],
        json!([])
    );

    let read = proxy.process_request(json_graphql_request(
        r#"
        query MetafieldDefinitionLifecycleReadProductMetafield($id: ID!, $namespace: String!, $key: String!) {
          product(id: $id) {
            metafield(namespace: $namespace, key: $key) { namespace key value }
          }
        }
        "#,
        json!({
            "id": owner_id,
            "namespace": namespace,
            "key": "target"
        }),
    ));
    assert_eq!(read.body["data"]["product"]["metafield"], Value::Null);
}

#[test]
fn metafield_definition_validation_update_gates_later_metafields_set() {
    let mut proxy = snapshot_proxy();
    let namespace = "validation_affects_values";
    let owner_id = "gid://shopify/Product/10173064872242";

    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation MetafieldDefinitionValidationAffectsValuesCreate($definition: MetafieldDefinitionInput!) {
          metafieldDefinitionCreate(definition: $definition) {
            createdDefinition { id namespace key ownerType validations { name value } }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "definition": {
                "namespace": namespace,
                "key": "headline",
                "ownerType": "PRODUCT",
                "name": "Headline",
                "type": "single_line_text_field"
            }
        }),
    ));
    assert_eq!(
        create.body["data"]["metafieldDefinitionCreate"]["userErrors"],
        json!([])
    );

    let before_update = proxy.process_request(json_graphql_request(
        r#"
        mutation MetafieldDefinitionValidationAffectsValuesSet($metafields: [MetafieldsSetInput!]!) {
          metafieldsSet(metafields: $metafields) {
            metafields { namespace key value }
            userErrors { field message code elementIndex }
          }
        }
        "#,
        json!({
            "metafields": [{
                "ownerId": owner_id,
                "namespace": namespace,
                "key": "headline",
                "type": "single_line_text_field",
                "value": "unbounded headline"
            }]
        }),
    ));
    assert_eq!(
        before_update.body["data"]["metafieldsSet"]["userErrors"],
        json!([])
    );

    let update = proxy.process_request(json_graphql_request(
        r#"
        mutation MetafieldDefinitionValidationAffectsValuesUpdate($definition: MetafieldDefinitionUpdateInput!) {
          metafieldDefinitionUpdate(definition: $definition) {
            updatedDefinition { namespace key validations { name value } }
            validationJob { __typename done }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "definition": {
                "namespace": namespace,
                "key": "headline",
                "ownerType": "PRODUCT",
                "validations": [{ "name": "max", "value": "5" }]
            }
        }),
    ));
    assert_eq!(
        update.body["data"]["metafieldDefinitionUpdate"]["updatedDefinition"]["validations"],
        json!([{ "name": "max", "value": "5" }])
    );

    let rejected = proxy.process_request(json_graphql_request(
        r#"
        mutation MetafieldDefinitionValidationAffectsValuesSet($metafields: [MetafieldsSetInput!]!) {
          metafieldsSet(metafields: $metafields) {
            metafields { namespace key value }
            userErrors { field message code elementIndex }
          }
        }
        "#,
        json!({
            "metafields": [{
                "ownerId": owner_id,
                "namespace": namespace,
                "key": "headline",
                "type": "single_line_text_field",
                "value": "too long"
            }]
        }),
    ));
    assert_eq!(
        rejected.body["data"]["metafieldsSet"],
        json!({
            "metafields": [],
            "userErrors": [{
                "field": ["metafields", "0", "value"],
                "message": "Value is too long.",
                "code": "INVALID_VALUE",
                "elementIndex": null
            }]
        })
    );

    let accepted = proxy.process_request(json_graphql_request(
        r#"
        mutation MetafieldDefinitionValidationAffectsValuesSet($metafields: [MetafieldsSetInput!]!) {
          metafieldsSet(metafields: $metafields) {
            metafields { namespace key value }
            userErrors { field message code elementIndex }
          }
        }
        "#,
        json!({
            "metafields": [{
                "ownerId": owner_id,
                "namespace": namespace,
                "key": "headline",
                "type": "single_line_text_field",
                "value": "short"
            }]
        }),
    ));
    assert_eq!(
        accepted.body["data"]["metafieldsSet"]["metafields"][0]["value"],
        json!("short")
    );

    let read = proxy.process_request(json_graphql_request(
        r#"
        query MetafieldDefinitionValidationAffectsValuesRead($id: ID!, $namespace: String!, $key: String!) {
          product(id: $id) {
            metafield(namespace: $namespace, key: $key) { namespace key value }
          }
        }
        "#,
        json!({
            "id": owner_id,
            "namespace": namespace,
            "key": "headline"
        }),
    ));
    assert_eq!(
        read.body["data"]["product"]["metafield"],
        json!({
            "namespace": namespace,
            "key": "headline",
            "value": "short"
        })
    );
}

#[test]
fn metafield_definition_validations_gate_metafields_set_for_non_product_owners() {
    let mut proxy = snapshot_proxy();
    let cases = [
        ("CUSTOMER", "gid://shopify/Customer/1", "customer"),
        ("ORDER", "gid://shopify/Order/1", "order"),
        ("COMPANY", "gid://shopify/Company/1", "company"),
        ("COLLECTION", "gid://shopify/Collection/1", "collection"),
        (
            "PRODUCTVARIANT",
            "gid://shopify/ProductVariant/1",
            "variant",
        ),
    ];

    for (owner_type, owner_id, suffix) in cases {
        let namespace = format!("validation_non_product_{suffix}");
        let key = "headline";
        let create = proxy.process_request(json_graphql_request(
            r#"
            mutation NonProductMetafieldDefinitionValidationCreate($definition: MetafieldDefinitionInput!) {
              metafieldDefinitionCreate(definition: $definition) {
                createdDefinition { namespace key ownerType validations { name value } }
                userErrors { field message code }
              }
            }
            "#,
            json!({
                "definition": {
                    "namespace": namespace,
                    "key": key,
                    "ownerType": owner_type,
                    "name": format!("{owner_type} Headline"),
                    "type": "single_line_text_field",
                    "validations": [{ "name": "max", "value": "5" }]
                }
            }),
        ));
        assert_eq!(
            create.body["data"]["metafieldDefinitionCreate"]["userErrors"],
            json!([]),
            "{owner_type} definition should be created"
        );

        let rejected = proxy.process_request(json_graphql_request(
            r#"
            mutation NonProductMetafieldDefinitionValidationSet($metafields: [MetafieldsSetInput!]!) {
              metafieldsSet(metafields: $metafields) {
                metafields { namespace key value }
                userErrors { field message code elementIndex }
              }
            }
            "#,
            json!({
                "metafields": [{
                    "ownerId": owner_id,
                    "namespace": namespace,
                    "key": key,
                    "type": "single_line_text_field",
                    "value": "too long"
                }]
            }),
        ));
        assert_eq!(
            rejected.body["data"]["metafieldsSet"],
            json!({
                "metafields": [],
                "userErrors": [{
                    "field": ["metafields", "0", "value"],
                    "message": "Value is too long.",
                    "code": "INVALID_VALUE",
                    "elementIndex": null
                }]
            }),
            "{owner_type} definition validation should reject over-limit values"
        );

        let accepted = proxy.process_request(json_graphql_request(
            r#"
            mutation NonProductMetafieldDefinitionValidationSet($metafields: [MetafieldsSetInput!]!) {
              metafieldsSet(metafields: $metafields) {
                metafields { namespace key value }
                userErrors { field message code elementIndex }
              }
            }
            "#,
            json!({
                "metafields": [{
                    "ownerId": owner_id,
                    "namespace": namespace,
                    "key": key,
                    "type": "single_line_text_field",
                    "value": "short"
                }]
            }),
        ));
        assert_eq!(
            accepted.body["data"]["metafieldsSet"]["userErrors"],
            json!([]),
            "{owner_type} definition validation should allow valid values"
        );
        assert_eq!(
            accepted.body["data"]["metafieldsSet"]["metafields"][0]["value"],
            json!("short")
        );
    }
}

#[test]
fn standard_metafield_definition_enable_uses_template_registry_and_errors() {
    let mut proxy = snapshot_proxy();

    let missing_selector = proxy.process_request(json_graphql_request(
        r#"
        mutation StandardMetafieldDefinitionEnableValidation($ownerType: MetafieldOwnerType!) {
          standardMetafieldDefinitionEnable(ownerType: $ownerType) {
            createdDefinition { id }
            userErrors { __typename field message code }
          }
        }
        "#,
        json!({ "ownerType": "PRODUCT" }),
    ));
    assert_eq!(
        missing_selector.body["data"]["standardMetafieldDefinitionEnable"],
        json!({
            "createdDefinition": null,
            "userErrors": [{
                "__typename": "StandardMetafieldDefinitionEnableUserError",
                "field": null,
                "message": "A namespace and key or standard metafield definition template id must be provided.",
                "code": "TEMPLATE_NOT_FOUND"
            }]
        })
    );

    let unknown = proxy.process_request(json_graphql_request(
        r#"
        mutation StandardMetafieldDefinitionEnableValidation($ownerType: MetafieldOwnerType!, $namespace: String, $key: String) {
          standardMetafieldDefinitionEnable(ownerType: $ownerType, namespace: $namespace, key: $key) {
            createdDefinition { id }
            userErrors { __typename field message code }
          }
        }
        "#,
        json!({
            "ownerType": "PRODUCT",
            "namespace": "codex_missing_standard",
            "key": "codex_missing_key"
        }),
    ));
    assert_eq!(
        unknown.body["data"]["standardMetafieldDefinitionEnable"]["userErrors"][0]["code"],
        json!("TEMPLATE_NOT_FOUND")
    );

    let enabled = proxy.process_request(json_graphql_request(
        r#"
        mutation StandardMetafieldDefinitionEnableSuccess($ownerType: MetafieldOwnerType!, $id: ID!) {
          standardMetafieldDefinitionEnable(ownerType: $ownerType, id: $id) {
            createdDefinition {
              namespace
              key
              ownerType
              name
              description
              type { name category }
              validations { name value }
            }
            userErrors { __typename field message code }
          }
        }
        "#,
        json!({
            "ownerType": "PRODUCT",
            "id": "gid://shopify/StandardMetafieldDefinitionTemplate/1"
        }),
    ));
    assert_eq!(
        enabled.body["data"]["standardMetafieldDefinitionEnable"],
        json!({
            "createdDefinition": {
                "namespace": "descriptors",
                "key": "subtitle",
                "ownerType": "PRODUCT",
                "name": "Product subtitle",
                "description": "Used as a shorthand for a product name",
                "type": { "name": "single_line_text_field", "category": "TEXT" },
                "validations": [{ "name": "max", "value": "70" }]
            },
            "userErrors": []
        })
    );
}

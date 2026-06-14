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
            createdDefinition { id namespace key pinnedPosition }
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

    let pin_a = proxy.process_request(json_graphql_request(
        r#"
        mutation MetafieldDefinitionPinByIdentifier($identifier: MetafieldDefinitionIdentifierInput!) {
          metafieldDefinitionPin(identifier: $identifier) {
            pinnedDefinition { id key pinnedPosition }
            userErrors { field message code }
          }
        }
        "#,
        json!({"identifier": {"ownerType": "PRODUCT", "namespace": "metafield_definition_pin_moyouov1", "key": "pin_a"}}),
    ));
    assert_eq!(
        pin_a.body["data"]["metafieldDefinitionPin"]["userErrors"],
        json!([])
    );
    assert_eq!(
        pin_a.body["data"]["metafieldDefinitionPin"]["pinnedDefinition"]["pinnedPosition"],
        json!(3)
    );

    let pin_b = proxy.process_request(json_graphql_request(
        r#"
        mutation MetafieldDefinitionPinByIdentifier($identifier: MetafieldDefinitionIdentifierInput!) {
          metafieldDefinitionPin(identifier: $identifier) { pinnedDefinition { id key pinnedPosition } userErrors { field message code } }
        }
        "#,
        json!({"identifier": {"ownerType": "PRODUCT", "namespace": "metafield_definition_pin_moyouov1", "key": "pin_b"}}),
    ));
    assert_eq!(
        pin_b.body["data"]["metafieldDefinitionPin"]["pinnedDefinition"]["pinnedPosition"],
        json!(4)
    );

    let read = proxy.process_request(json_graphql_request(
        r#"
        query MetafieldDefinitionPinningRead($namespace: String!) {
          byIdentifier: metafieldDefinition(identifier: { ownerType: PRODUCT, namespace: $namespace, key: "pin_a" }) { id key pinnedPosition }
          pinned: metafieldDefinitions(ownerType: PRODUCT, first: 10, namespace: $namespace, sortKey: PINNED_POSITION, pinnedStatus: PINNED) { nodes { key pinnedPosition } }
        }
        "#,
        json!({"namespace": "metafield_definition_pin_moyouov1"}),
    ));
    assert_eq!(
        read.body["data"]["byIdentifier"]["pinnedPosition"],
        json!(3)
    );
    assert_eq!(
        read.body["data"]["pinned"]["nodes"],
        json!([
            {"key": "pin_b", "pinnedPosition": 4},
            {"key": "pin_a", "pinnedPosition": 3}
        ])
    );

    let unpin_a = proxy.process_request(json_graphql_request(
        r#"
        mutation MetafieldDefinitionUnpinByIdentifier($identifier: MetafieldDefinitionIdentifierInput!) {
          metafieldDefinitionUnpin(identifier: $identifier) { unpinnedDefinition { id key pinnedPosition } userErrors { field message code } }
        }
        "#,
        json!({"identifier": {"ownerType": "PRODUCT", "namespace": "metafield_definition_pin_moyouov1", "key": "pin_a"}}),
    ));
    assert_eq!(
        unpin_a.body["data"]["metafieldDefinitionUnpin"]["unpinnedDefinition"]["pinnedPosition"],
        Value::Null
    );
}

#[test]
fn metafield_definition_pin_limit_is_fifty_for_pin_create_update_and_standard_enable() {
    let namespace = "har1423_pin_limit";

    let mut pin_proxy = snapshot_proxy();
    for index in 1..=51 {
        let key = format!("pin_{index:02}");
        let created = create_definition(&mut pin_proxy, namespace, &key, false);
        assert_eq!(created["userErrors"], json!([]));
    }
    for index in 1..=50 {
        let key = format!("pin_{index:02}");
        let pinned = pin_definition(&mut pin_proxy, namespace, &key);
        assert_eq!(pinned["userErrors"], json!([]));
        if index == 50 {
            assert_eq!(pinned["pinnedDefinition"]["pinnedPosition"], json!(50));
        }
    }
    let over_cap_pin = pin_definition(&mut pin_proxy, namespace, "pin_51");
    assert_eq!(over_cap_pin["pinnedDefinition"], Value::Null);
    assert_eq!(
        over_cap_pin["userErrors"],
        json!([{
            "field": null,
            "message": "Limit of 50 pinned definitions.",
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
    for index in 1..=50 {
        let key = format!("pin_{index:02}");
        let created = create_definition(&mut create_proxy, "har1423_create_limit", &key, true);
        assert_eq!(created["userErrors"], json!([]));
        if index == 50 {
            assert_eq!(created["createdDefinition"]["pinnedPosition"], json!(50));
        }
    }
    let over_cap_create =
        create_definition(&mut create_proxy, "har1423_create_limit", "pin_51", true);
    assert_eq!(over_cap_create["createdDefinition"], Value::Null);
    assert_eq!(
        over_cap_create["userErrors"][0]["message"],
        json!("Limit of 50 pinned definitions.")
    );
    assert_eq!(
        over_cap_create["userErrors"][0]["code"],
        json!("PINNED_LIMIT_REACHED")
    );

    let mut update_proxy = snapshot_proxy();
    for index in 1..=51 {
        let key = format!("pin_{index:02}");
        assert_eq!(
            create_definition(&mut update_proxy, "har1423_update_limit", &key, false)["userErrors"],
            json!([])
        );
    }
    for index in 1..=50 {
        let key = format!("pin_{index:02}");
        let updated = update_definition_pin(&mut update_proxy, "har1423_update_limit", &key);
        assert_eq!(updated["userErrors"], json!([]));
        if index == 50 {
            assert_eq!(updated["updatedDefinition"]["pinnedPosition"], json!(50));
        }
    }
    let over_cap_update =
        update_definition_pin(&mut update_proxy, "har1423_update_limit", "pin_51");
    assert_eq!(over_cap_update["updatedDefinition"], Value::Null);
    assert_eq!(
        over_cap_update["userErrors"][0]["message"],
        json!("Limit of 50 pinned definitions.")
    );
    assert_eq!(
        over_cap_update["userErrors"][0]["code"],
        json!("PINNED_LIMIT_REACHED")
    );

    let mut standard_proxy = snapshot_proxy();
    for index in 1..=50 {
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
        json!("Limit of 50 pinned definitions.")
    );
    assert_eq!(
        over_cap_standard["userErrors"][0]["code"],
        json!("PINNED_LIMIT_REACHED")
    );
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
fn metafield_definition_delete_enforces_reference_guards_and_removes_associated_values() {
    let mut proxy = snapshot_proxy();
    let namespace = "delete_reference_guard";

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
                "ownerId": "gid://shopify/Product/10173064872242",
                "namespace": namespace,
                "key": "target",
                "type": "product_reference",
                "value": "gid://shopify/Product/10178790424882"
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
            "id": "gid://shopify/Product/10173064872242",
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

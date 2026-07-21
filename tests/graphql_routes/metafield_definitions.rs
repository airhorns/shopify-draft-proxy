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

fn upstream_definition(id: &str, namespace: &str, key: &str, name: &str) -> Value {
    json!({
        "id": id,
        "name": name,
        "namespace": namespace,
        "key": key,
        "ownerType": "PRODUCT",
        "type": { "name": "single_line_text_field", "category": "TEXT" },
        "description": null,
        "validations": [],
        "access": {
            "admin": "MERCHANT_READ_WRITE",
            "storefront": "PUBLIC_READ",
            "customerAccount": "NONE"
        },
        "capabilities": {
            "adminFilterable": { "enabled": false, "eligible": true, "status": "NOT_FILTERABLE" },
            "smartCollectionCondition": { "enabled": false, "eligible": true },
            "uniqueValues": { "enabled": false, "eligible": true }
        },
        "constraints": null,
        "pinnedPosition": null,
        "validationStatus": "VALID",
        "metafieldsCount": 0
    })
}

fn upstream_definition_with_options(
    id: &str,
    namespace: &str,
    key: &str,
    name: &str,
    pinned_position: Option<i64>,
    admin_filterable: bool,
) -> Value {
    let mut definition = upstream_definition(id, namespace, key, name);
    definition["pinnedPosition"] = pinned_position.map_or(Value::Null, |position| json!(position));
    definition["capabilities"]["adminFilterable"] = if admin_filterable {
        json!({ "enabled": true, "eligible": true, "status": "FILTERABLE" })
    } else {
        json!({ "enabled": false, "eligible": true, "status": "NOT_FILTERABLE" })
    };
    definition
}

fn update_definition_admin_filterable(proxy: &mut DraftProxy, namespace: &str, key: &str) -> Value {
    proxy
        .process_request(json_graphql_request(
            r#"
            mutation UpdateDefinitionAdminFilterable($definition: MetafieldDefinitionUpdateInput!) {
              metafieldDefinitionUpdate(definition: $definition) {
                updatedDefinition { id capabilities { adminFilterable { enabled } } }
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
                    "capabilities": { "adminFilterable": { "enabled": true } }
                }
            }),
        ))
        .body["data"]["metafieldDefinitionUpdate"]
        .clone()
}

fn metafield_definition_upstream_response(definitions: &[Value], body: &Value) -> Response {
    let variables = &body["variables"];
    match body["operationName"].as_str().unwrap_or_default() {
        "MetafieldDefinitionHydrateByIdentifier" => {
            let identifier = &variables["identifier"];
            let definition = definitions
                .iter()
                .find(|definition| {
                    definition["ownerType"] == identifier["ownerType"]
                        && definition["namespace"] == identifier["namespace"]
                        && definition["key"] == identifier["key"]
                })
                .cloned()
                .unwrap_or(Value::Null);
            return Response {
                status: 200,
                headers: Default::default(),
                body: json!({ "data": { "metafieldDefinition": definition } }),
            };
        }
        "MetafieldDefinitionHydrateById" => {
            let definition = definitions
                .iter()
                .find(|definition| definition["id"] == variables["id"])
                .cloned()
                .unwrap_or(Value::Null);
            return Response {
                status: 200,
                headers: Default::default(),
                body: json!({ "data": { "metafieldDefinition": definition } }),
            };
        }
        _ => {}
    }

    let owner_type = variables["ownerType"].as_str().unwrap_or("PRODUCT");
    let namespace_filter = variables["namespace"].as_str();
    let key_filter = variables["key"].as_str();
    let pinned_status = variables["pinnedStatus"].as_str();
    let query = variables["query"].as_str();
    let mut filtered = definitions
        .iter()
        .filter(|definition| {
            definition["ownerType"].as_str() == Some(owner_type)
                && namespace_filter
                    .is_none_or(|namespace| definition["namespace"].as_str() == Some(namespace))
                && key_filter.is_none_or(|key| definition["key"].as_str() == Some(key))
                && match pinned_status {
                    Some("PINNED") => !definition["pinnedPosition"].is_null(),
                    Some("UNPINNED") => definition["pinnedPosition"].is_null(),
                    _ => true,
                }
                && match query {
                    Some("-namespace:app--*") => !definition["namespace"]
                        .as_str()
                        .unwrap_or_default()
                        .starts_with("app--"),
                    Some(query) if query.starts_with("namespace:app--") && query.ends_with('*') => {
                        definition["namespace"].as_str().is_some_and(|namespace| {
                            namespace.starts_with(&query[10..query.len() - 1])
                        })
                    }
                    _ => true,
                }
        })
        .cloned()
        .collect::<Vec<_>>();
    match variables["sortKey"].as_str() {
        Some("NAME") => {
            filtered.sort_by(|left, right| left["name"].as_str().cmp(&right["name"].as_str()))
        }
        Some("PINNED_POSITION") => filtered.sort_by_key(|definition| {
            std::cmp::Reverse(definition["pinnedPosition"].as_i64().unwrap_or_default())
        }),
        _ => filtered.sort_by(|left, right| left["id"].as_str().cmp(&right["id"].as_str())),
    }
    if variables["reverse"].as_bool() == Some(true) {
        filtered.reverse();
    }
    let after = variables["after"].as_str();
    let start = after
        .and_then(|cursor| {
            filtered
                .iter()
                .position(|definition| definition["id"].as_str() == Some(cursor))
        })
        .map_or(0, |index| index + 1);
    let first = variables["first"].as_u64().map(|value| value as usize);
    let last = variables["last"].as_u64().map(|value| value as usize);
    let mut nodes = filtered.iter().skip(start).cloned().collect::<Vec<_>>();
    if let Some(first) = first {
        nodes.truncate(first);
    } else if let Some(last) = last {
        let keep_from = nodes.len().saturating_sub(last);
        nodes = nodes.split_off(keep_from);
    }
    let edges = nodes
        .iter()
        .map(|node| json!({ "cursor": node["id"], "node": node }))
        .collect::<Vec<_>>();
    let start_cursor = nodes.first().map_or(Value::Null, |node| node["id"].clone());
    let end_cursor = nodes.last().map_or(Value::Null, |node| node["id"].clone());
    Response {
        status: 200,
        headers: Default::default(),
        body: json!({
            "data": {
                "metafieldDefinitions": {
                    "nodes": nodes,
                    "edges": edges,
                    "pageInfo": {
                        "hasNextPage": start + first.unwrap_or(filtered.len()) < filtered.len(),
                        "hasPreviousPage": start > 0,
                        "startCursor": start_cursor,
                        "endCursor": end_cursor
                    }
                }
            }
        }),
    }
}

fn live_hybrid_proxy_with_upstream_definitions(definitions: Vec<Value>) -> DraftProxy {
    configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport(move |request| {
        let body: Value = serde_json::from_str(&request.body).unwrap();
        let query = body["query"].as_str().unwrap_or_default();
        assert!(
            query.contains("metafieldDefinition"),
            "supported metafield definition mutation should not be forwarded upstream: {}",
            request.body
        );
        metafield_definition_upstream_response(&definitions, &body)
    })
}

fn tracking_live_hybrid_definition_proxy(
    definitions: Vec<Value>,
) -> (DraftProxy, Arc<Mutex<Vec<Value>>>) {
    let requests = Arc::new(Mutex::new(Vec::new()));
    let captured_requests = Arc::clone(&requests);
    let proxy =
        configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport(move |request| {
            let body: Value = serde_json::from_str(&request.body).unwrap();
            captured_requests.lock().unwrap().push(body.clone());
            metafield_definition_upstream_response(&definitions, &body)
        });
    (proxy, requests)
}

#[test]
fn live_hybrid_definition_create_and_reads_merge_real_catalog_with_staged_changes() {
    let namespace = "partial_catalog";
    let mut real_definition = upstream_definition(
        "gid://shopify/MetafieldDefinition/900001",
        namespace,
        "real",
        "Real definition",
    );
    real_definition["metafieldsCount"] = json!(7);
    let other_definition = upstream_definition(
        "gid://shopify/MetafieldDefinition/900002",
        "unrelated_catalog",
        "other",
        "Other definition",
    );
    let upstream_requests = Arc::new(Mutex::new(Vec::new()));
    let mut proxy = configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport({
        let real_definition = real_definition.clone();
        let other_definition = other_definition.clone();
        let upstream_requests = Arc::clone(&upstream_requests);
        move |request| {
            upstream_requests.lock().unwrap().push(request.body.clone());
            let body: Value = serde_json::from_str(&request.body).unwrap();
            metafield_definition_upstream_response(
                &[real_definition.clone(), other_definition.clone()],
                &body,
            )
        }
    });
    let config = proxy.process_request(request_with_body("GET", "/__meta/config", ""));
    assert_eq!(config.body["runtime"]["readMode"], json!("live-hybrid"));

    let duplicate = proxy.process_request(json_graphql_request(
        r#"
        mutation DuplicateRealDefinition($definition: MetafieldDefinitionInput!) {
          metafieldDefinitionCreate(definition: $definition) {
            createdDefinition { id namespace key }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "definition": {
                "ownerType": "PRODUCT",
                "namespace": namespace,
                "key": "real",
                "name": "Duplicate real definition",
                "type": "single_line_text_field"
            }
        }),
    ));
    assert_eq!(
        upstream_requests.lock().unwrap().len(),
        1,
        "duplicate create should stop after one exact identity probe"
    );

    assert_eq!(
        duplicate.body["data"]["metafieldDefinitionCreate"],
        json!({
            "createdDefinition": null,
            "userErrors": [{
                "field": ["definition", "key"],
                "message": "Key is in use for Product metafields on the 'partial_catalog' namespace.",
                "code": "TAKEN"
            }]
        })
    );

    let staged = create_definition_for_resource_limit(&mut proxy, "PRODUCT", namespace, "local");
    assert_eq!(staged["userErrors"], json!([]));
    let local_id = staged["createdDefinition"]["id"].clone();

    let read = proxy.process_request(json_graphql_request(
        r#"
        query ReadMergedDefinitions($namespace: String!) {
          realDetail: metafieldDefinition(identifier: { ownerType: PRODUCT, namespace: $namespace, key: "real" }) {
            id
            namespace
            key
            name
            metafieldsCount
          }
          localDetail: metafieldDefinition(identifier: { ownerType: PRODUCT, namespace: $namespace, key: "local" }) {
            id
            namespace
            key
            name
            metafieldsCount
          }
          catalog: metafieldDefinitions(ownerType: PRODUCT, namespace: $namespace, first: 10, sortKey: NAME) {
            nodes { id namespace key name metafieldsCount }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
          ownerCatalog: metafieldDefinitions(ownerType: PRODUCT, first: 10, sortKey: NAME) {
            nodes { id namespace key name metafieldsCount }
          }
        }
        "#,
        json!({ "namespace": namespace }),
    ));

    assert_eq!(
        read.body["data"]["realDetail"],
        json!({
            "id": "gid://shopify/MetafieldDefinition/900001",
            "namespace": namespace,
            "key": "real",
            "name": "Real definition",
            "metafieldsCount": 7
        })
    );
    assert_eq!(
        read.body["data"]["localDetail"],
        json!({
            "id": local_id,
            "namespace": namespace,
            "key": "local",
            "name": "Resource limit local",
            "metafieldsCount": 0
        })
    );
    assert_eq!(
        read.body["data"]["catalog"]["nodes"],
        json!([
            {
                "id": "gid://shopify/MetafieldDefinition/900001",
                "namespace": namespace,
                "key": "real",
                "name": "Real definition",
                "metafieldsCount": 7
            },
            {
                "id": local_id,
                "namespace": namespace,
                "key": "local",
                "name": "Resource limit local",
                "metafieldsCount": 0
            }
        ])
    );
    assert_eq!(
        read.body["data"]["ownerCatalog"]["nodes"],
        json!([
            {
                "id": "gid://shopify/MetafieldDefinition/900002",
                "namespace": "unrelated_catalog",
                "key": "other",
                "name": "Other definition",
                "metafieldsCount": 0
            },
            {
                "id": "gid://shopify/MetafieldDefinition/900001",
                "namespace": namespace,
                "key": "real",
                "name": "Real definition",
                "metafieldsCount": 7
            },
            {
                "id": local_id,
                "namespace": namespace,
                "key": "local",
                "name": "Resource limit local",
                "metafieldsCount": 0
            }
        ])
    );
    assert_eq!(
        read.body["data"]["catalog"]["pageInfo"],
        json!({
            "hasNextPage": false,
            "hasPreviousPage": false,
            "startCursor": "gid://shopify/MetafieldDefinition/900001",
            "endCursor": local_id
        })
    );
    assert!(
        upstream_requests
            .lock()
            .unwrap()
            .iter()
            .all(|body| !body.contains("DuplicateRealDefinition")),
        "supported mutation should not be forwarded upstream"
    );

    let delete = proxy.process_request(json_graphql_request(
        r#"
        mutation DeleteRealDefinition($id: ID!) {
          metafieldDefinitionDelete(id: $id, deleteAllAssociatedMetafields: true) {
            deletedDefinitionId
            userErrors { field message code }
          }
        }
        "#,
        json!({ "id": "gid://shopify/MetafieldDefinition/900001" }),
    ));
    assert_eq!(
        delete.body["data"]["metafieldDefinitionDelete"],
        json!({
            "deletedDefinitionId": "gid://shopify/MetafieldDefinition/900001",
            "userErrors": []
        })
    );

    let read_after_delete = proxy.process_request(json_graphql_request(
        r#"
        query ReadDefinitionsAfterDelete($namespace: String!) {
          realDetail: metafieldDefinition(identifier: { ownerType: PRODUCT, namespace: $namespace, key: "real" }) {
            id
          }
          catalog: metafieldDefinitions(ownerType: PRODUCT, namespace: $namespace, first: 10, sortKey: NAME) {
            nodes { id namespace key name }
          }
        }
        "#,
        json!({ "namespace": namespace }),
    ));
    assert_eq!(read_after_delete.body["data"]["realDetail"], Value::Null);
    assert_eq!(
        read_after_delete.body["data"]["catalog"]["nodes"],
        json!([
            {
                "id": local_id,
                "namespace": namespace,
                "key": "local",
                "name": "Resource limit local"
            }
        ])
    );
}

#[test]
fn live_hybrid_definition_validations_count_upstream_owner_catalog() {
    let resource_definitions = (0..256)
        .map(|index| {
            upstream_definition(
                &format!("gid://shopify/MetafieldDefinition/91{index:04}"),
                "resource_existing",
                &format!("resource_{index:03}"),
                &format!("Resource {index:03}"),
            )
        })
        .collect::<Vec<_>>();
    let mut resource_proxy = live_hybrid_proxy_with_upstream_definitions(resource_definitions);
    let resource_over_limit = create_definition_for_resource_limit(
        &mut resource_proxy,
        "PRODUCT",
        "resource_new",
        "resource_over",
    );
    assert_eq!(
        resource_over_limit,
        json!({
            "createdDefinition": null,
            "userErrors": [{
                "field": ["definition"],
                "message": "Stores can only have 256 definitions for each store resource.",
                "code": "RESOURCE_TYPE_LIMIT_EXCEEDED"
            }]
        })
    );

    let admin_definitions = (0..50)
        .map(|index| {
            upstream_definition_with_options(
                &format!("gid://shopify/MetafieldDefinition/92{index:04}"),
                "admin_filter_existing",
                &format!("admin_{index:02}"),
                &format!("Admin {index:02}"),
                None,
                true,
            )
        })
        .collect::<Vec<_>>();
    let mut admin_proxy = live_hybrid_proxy_with_upstream_definitions(admin_definitions);
    let admin_over_limit = admin_proxy.process_request(json_graphql_request(
        r#"
        mutation AdminFilterableLimitFromUpstream($definition: MetafieldDefinitionInput!) {
          metafieldDefinitionCreate(definition: $definition) {
            createdDefinition { id }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "definition": {
                "ownerType": "PRODUCT",
                "namespace": "admin_filter_new",
                "key": "admin_over",
                "name": "Admin over",
                "type": "single_line_text_field",
                "capabilities": { "adminFilterable": { "enabled": true } }
            }
        }),
    ));
    assert_eq!(
        admin_over_limit.body["data"]["metafieldDefinitionCreate"],
        json!({
            "createdDefinition": null,
            "userErrors": [{
                "field": ["definition"],
                "message": "You can only use 50 product metafield definitions to filter the product list. To add a new filter, disable filtering on an existing one.",
                "code": "OWNER_TYPE_LIMIT_EXCEEDED_FOR_USE_AS_ADMIN_FILTERS"
            }]
        })
    );

    let pinned_definitions = (1..=50)
        .map(|position| {
            upstream_definition_with_options(
                &format!("gid://shopify/MetafieldDefinition/93{position:04}"),
                "pinned_existing",
                &format!("pinned_{position:02}"),
                &format!("Pinned {position:02}"),
                Some(position),
                false,
            )
        })
        .collect::<Vec<_>>();
    let mut pin_proxy = live_hybrid_proxy_with_upstream_definitions(pinned_definitions);
    let pin_over_limit = create_definition(&mut pin_proxy, "pinned_new", "pinned_over", true);
    assert_eq!(
        pin_over_limit,
        json!({
            "createdDefinition": null,
            "userErrors": [{
                "field": ["definition"],
                "message": "Limit of 50 pinned definitions.",
                "code": "PINNED_LIMIT_REACHED"
            }]
        })
    );
}

#[test]
fn live_hybrid_definition_create_uses_bounded_probes_past_large_unrelated_catalogs() {
    let upstream_requests = Arc::new(Mutex::new(Vec::<Value>::new()));
    let captured_requests = Arc::clone(&upstream_requests);
    let mut proxy = configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport(
        move |request| {
            let body: Value = serde_json::from_str(&request.body).unwrap();
            captured_requests.lock().unwrap().push(body.clone());
            match body["operationName"].as_str().unwrap_or_default() {
                "MetafieldDefinitionHydrateByIdentifier" => Response {
                    status: 200,
                    headers: Default::default(),
                    body: json!({ "data": { "metafieldDefinition": null } }),
                },
                "MetafieldDefinitionsHydrateResourceScope" => {
                    assert_eq!(
                        body["variables"]["query"],
                        json!("-namespace:app--*"),
                        "merchant validation must exclude more than 5,000 unrelated app definitions at the query boundary"
                    );
                    let after = body["variables"]["after"].as_str();
                    let start = if after.is_some() { 250 } else { 0 };
                    let take = if after.is_some() { 6 } else { 250 };
                    let nodes = (start..start + take)
                        .map(|index| {
                            upstream_definition(
                                &format!("gid://shopify/MetafieldDefinition/{}", 9_500_000 + index),
                                "merchant_existing",
                                &format!("existing_{index:03}"),
                                "Existing merchant definition",
                            )
                        })
                        .collect::<Vec<_>>();
                    Response {
                        status: 200,
                        headers: Default::default(),
                        body: json!({
                            "data": {
                                "metafieldDefinitions": {
                                    "nodes": nodes,
                                    "pageInfo": {
                                        "hasNextPage": after.is_none(),
                                        "endCursor": if after.is_none() { json!("merchant-page-1") } else { Value::Null }
                                    }
                                }
                            }
                        }),
                    }
                }
                operation_name => panic!("unexpected prerequisite operation: {operation_name}"),
            }
        },
    );

    let response = create_definition_for_resource_limit(
        &mut proxy,
        "PRODUCT",
        "merchant_after_large_app_catalog",
        "over_limit",
    );
    let requests = upstream_requests.lock().unwrap();
    assert!(
        requests.len() <= 3,
        "one create must use at most three bounded prerequisite requests; observed {} owner-catalog pages",
        requests.len()
    );
    assert_eq!(
        response["userErrors"][0]["code"],
        json!("RESOURCE_TYPE_LIMIT_EXCEEDED"),
        "the merchant threshold must remain authoritative after more than 5,000 unrelated definitions"
    );
}

#[test]
fn live_hybrid_definition_mutation_budgets_stay_fixed_near_limits_in_large_catalogs() {
    let unrelated_app_definitions = (0..5_001)
        .map(|index| {
            upstream_definition(
                &format!("gid://shopify/MetafieldDefinition/{:07}", 8_000_000 + index),
                "app--999999--unrelated",
                &format!("unrelated_{index:04}"),
                "Unrelated app definition",
            )
        })
        .collect::<Vec<_>>();
    let large_catalog = |mut relevant: Vec<Value>| {
        relevant.extend(unrelated_app_definitions.iter().cloned());
        relevant
    };

    for (existing_count, expected_code) in [
        (255usize, None),
        (256usize, Some("RESOURCE_TYPE_LIMIT_EXCEEDED")),
    ] {
        let relevant = (0..existing_count)
            .map(|index| {
                upstream_definition(
                    &format!("gid://shopify/MetafieldDefinition/{:07}", 8_100_000 + index),
                    "merchant_resource_limit",
                    &format!("resource_{index:03}"),
                    "Merchant resource definition",
                )
            })
            .collect::<Vec<_>>();
        let (mut proxy, requests) = tracking_live_hybrid_definition_proxy(large_catalog(relevant));
        let created = create_definition_for_resource_limit(
            &mut proxy,
            "PRODUCT",
            "merchant_resource_limit_new",
            "target",
        );
        if let Some(expected_code) = expected_code {
            assert_eq!(created["createdDefinition"], Value::Null);
            assert_eq!(created["userErrors"][0]["code"], json!(expected_code));
        } else {
            assert_eq!(created["userErrors"], json!([]));
        }
        assert_eq!(
            requests.lock().unwrap().len(),
            3,
            "resource threshold {existing_count} uses one identity probe and two threshold pages"
        );
    }

    for (admin_filterable_count, expected_code) in [
        (49usize, None),
        (
            50usize,
            Some("OWNER_TYPE_LIMIT_EXCEEDED_FOR_USE_AS_ADMIN_FILTERS"),
        ),
    ] {
        let target = upstream_definition(
            "gid://shopify/MetafieldDefinition/8200000",
            "merchant_admin_limit",
            "target",
            "Admin filter target",
        );
        let mut relevant = vec![target];
        relevant.extend((0..admin_filterable_count).map(|index| {
            upstream_definition_with_options(
                &format!("gid://shopify/MetafieldDefinition/{:07}", 8_200_001 + index),
                "merchant_admin_limit",
                &format!("admin_{index:02}"),
                "Admin filter definition",
                None,
                true,
            )
        }));
        let (mut proxy, requests) = tracking_live_hybrid_definition_proxy(large_catalog(relevant));
        let updated =
            update_definition_admin_filterable(&mut proxy, "merchant_admin_limit", "target");
        if let Some(expected_code) = expected_code {
            assert_eq!(updated["updatedDefinition"], Value::Null);
            assert_eq!(updated["userErrors"][0]["code"], json!(expected_code));
        } else {
            assert_eq!(updated["userErrors"], json!([]));
        }
        assert_eq!(
            requests.lock().unwrap().len(),
            2,
            "admin-filter threshold {admin_filterable_count} uses one identity probe and one merchant threshold page"
        );
    }

    for (pinned_count, expected_code) in [(49usize, None), (50usize, Some("PINNED_LIMIT_REACHED"))]
    {
        let target = upstream_definition(
            "gid://shopify/MetafieldDefinition/8300000",
            "pinned_large_catalog",
            "target",
            "Pin target",
        );
        let mut relevant = vec![target];
        relevant.extend((0..pinned_count).map(|index| {
            upstream_definition_with_options(
                &format!("gid://shopify/MetafieldDefinition/{:07}", 8_300_001 + index),
                "pinned_large_catalog",
                &format!("pinned_{index:02}"),
                "Pinned definition",
                Some(index as i64 + 1),
                false,
            )
        }));
        let (mut proxy, requests) = tracking_live_hybrid_definition_proxy(large_catalog(relevant));
        let pinned = pin_definition(&mut proxy, "pinned_large_catalog", "target");
        if let Some(expected_code) = expected_code {
            assert_eq!(pinned["pinnedDefinition"], Value::Null);
            assert_eq!(pinned["userErrors"][0]["code"], json!(expected_code));
        } else {
            assert_eq!(pinned["userErrors"], json!([]));
            assert_eq!(pinned["pinnedDefinition"]["pinnedPosition"], json!(50));
        }
        assert_eq!(
            requests.lock().unwrap().len(),
            2,
            "pin threshold {pinned_count} uses one identity probe and one pinned-owner probe"
        );
    }

    for (pinned_count, expected_code) in [(49usize, None), (50usize, Some("LIMIT_EXCEEDED"))] {
        let relevant = (0..pinned_count)
            .map(|index| {
                upstream_definition_with_options(
                    &format!("gid://shopify/MetafieldDefinition/{:07}", 8_350_001 + index),
                    "standard_pin_large_catalog",
                    &format!("pinned_{index:02}"),
                    "Pinned definition",
                    Some(index as i64 + 1),
                    false,
                )
            })
            .collect::<Vec<_>>();
        let (mut proxy, requests) = tracking_live_hybrid_definition_proxy(large_catalog(relevant));
        let enabled = standard_enable_pin(&mut proxy);
        if let Some(expected_code) = expected_code {
            assert_eq!(enabled["createdDefinition"], Value::Null);
            assert_eq!(enabled["userErrors"][0]["code"], json!(expected_code));
        } else {
            assert_eq!(enabled["userErrors"], json!([]));
            assert_eq!(enabled["createdDefinition"]["pinnedPosition"], json!(50));
        }
        assert_eq!(
            requests.lock().unwrap().len(),
            2,
            "standard enable at pin threshold {pinned_count} uses one identity probe and one pinned-owner probe"
        );
    }

    let pinned_target = upstream_definition_with_options(
        "gid://shopify/MetafieldDefinition/8400020",
        "pinned_large_catalog",
        "target",
        "Pinned target",
        Some(50),
        false,
    );
    let pinned_context = (0..49)
        .map(|index| {
            upstream_definition_with_options(
                &format!("gid://shopify/MetafieldDefinition/{:07}", 8_400_001 + index),
                "pinned_large_catalog",
                &format!("pinned_{index:02}"),
                "Pinned definition",
                Some(index as i64 + 1),
                false,
            )
        })
        .collect::<Vec<_>>();

    let mut unpin_catalog = vec![pinned_target.clone()];
    unpin_catalog.extend(pinned_context.iter().cloned());
    let (mut unpin_proxy, unpin_requests) =
        tracking_live_hybrid_definition_proxy(large_catalog(unpin_catalog));
    assert_eq!(
        unpin_definition(&mut unpin_proxy, "pinned_large_catalog", "target")["userErrors"],
        json!([])
    );
    assert_eq!(
        unpin_requests.lock().unwrap().len(),
        2,
        "unpin at the owner cap uses one identity probe and one pinned-owner probe"
    );

    let mut delete_catalog = vec![pinned_target];
    delete_catalog.extend(pinned_context);
    let (mut delete_proxy, delete_requests) =
        tracking_live_hybrid_definition_proxy(large_catalog(delete_catalog));
    let deleted = delete_proxy.process_request(json_graphql_request(
        r#"
        mutation DeletePinnedDefinitionAtLargeCatalogLimit($identifier: MetafieldDefinitionIdentifierInput!) {
          metafieldDefinitionDelete(identifier: $identifier, deleteAllAssociatedMetafields: true) {
            deletedDefinitionId
            userErrors { field message code }
          }
        }
        "#,
        json!({ "identifier": {
            "ownerType": "PRODUCT",
            "namespace": "pinned_large_catalog",
            "key": "target"
        }}),
    ));
    assert_eq!(
        deleted.body["data"]["metafieldDefinitionDelete"]["userErrors"],
        json!([])
    );
    assert_eq!(
        delete_requests.lock().unwrap().len(),
        2,
        "pinned delete at the owner cap uses one identity probe and one pinned-owner probe"
    );
}

#[test]
fn live_hybrid_definition_mutation_targets_use_fixed_cold_and_warm_request_budgets() {
    let target = upstream_definition(
        "gid://shopify/MetafieldDefinition/9600001",
        "bounded_targets",
        "target",
        "Bounded target",
    );

    let (mut update_proxy, update_requests) =
        tracking_live_hybrid_definition_proxy(vec![target.clone()]);
    for name in ["Updated once", "Updated twice"] {
        let response = update_proxy.process_request(json_graphql_request(
            r#"
            mutation UpdateBoundedTarget($definition: MetafieldDefinitionUpdateInput!) {
              metafieldDefinitionUpdate(definition: $definition) {
                updatedDefinition { id name }
                userErrors { field message code }
                validationJob { id }
              }
            }
            "#,
            json!({
                "definition": {
                    "ownerType": "PRODUCT",
                    "namespace": "bounded_targets",
                    "key": "target",
                    "name": name
                }
            }),
        ));
        assert_eq!(
            response.body["data"]["metafieldDefinitionUpdate"]["userErrors"],
            json!([])
        );
    }
    assert_eq!(
        update_requests.lock().unwrap().len(),
        1,
        "a cold update uses one exact target probe and the warm update reuses it"
    );

    let (mut pin_proxy, pin_requests) = tracking_live_hybrid_definition_proxy(vec![target.clone()]);
    assert_eq!(
        pin_definition(&mut pin_proxy, "bounded_targets", "target")["userErrors"],
        json!([])
    );
    assert!(
        pin_requests.lock().unwrap().len() <= 2,
        "a cold pin uses only its exact target and bounded pinned-owner context"
    );

    let mut pinned_target = target.clone();
    pinned_target["pinnedPosition"] = json!(1);
    let (mut unpin_proxy, unpin_requests) =
        tracking_live_hybrid_definition_proxy(vec![pinned_target]);
    assert_eq!(
        unpin_definition(&mut unpin_proxy, "bounded_targets", "target")["userErrors"],
        json!([])
    );
    assert!(
        unpin_requests.lock().unwrap().len() <= 2,
        "a cold unpin uses only its exact target and bounded pinned-owner context"
    );

    let (mut delete_proxy, delete_requests) =
        tracking_live_hybrid_definition_proxy(vec![target.clone()]);
    let deleted = delete_proxy.process_request(json_graphql_request(
        r#"
        mutation DeleteBoundedTarget($identifier: MetafieldDefinitionIdentifierInput!) {
          metafieldDefinitionDelete(identifier: $identifier, deleteAllAssociatedMetafields: true) {
            deletedDefinitionId
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "identifier": {
                "ownerType": "PRODUCT",
                "namespace": "bounded_targets",
                "key": "target"
            }
        }),
    ));
    assert_eq!(
        deleted.body["data"]["metafieldDefinitionDelete"]["userErrors"],
        json!([])
    );
    assert_eq!(
        delete_requests.lock().unwrap().len(),
        1,
        "a cold unpinned delete needs only one exact target probe"
    );

    let (mut create_proxy, create_requests) = tracking_live_hybrid_definition_proxy(vec![target]);
    assert_eq!(
        create_definition_for_resource_limit(
            &mut create_proxy,
            "PRODUCT",
            "bounded_targets",
            "created",
        )["userErrors"],
        json!([])
    );
    assert!(
        create_requests.lock().unwrap().len() <= 2,
        "a below-limit create uses one exact target probe and one bounded resource-scope page"
    );

    for requests in [
        update_requests,
        pin_requests,
        unpin_requests,
        delete_requests,
        create_requests,
    ] {
        assert!(requests.lock().unwrap().iter().all(|body| {
            matches!(
                body["operationName"].as_str(),
                Some(
                    "MetafieldDefinitionHydrateByIdentifier"
                        | "MetafieldDefinitionsHydrateResourceScope"
                        | "MetafieldDefinitionsHydratePinnedOwner"
                )
            )
        }));
    }
}

#[test]
fn live_hybrid_definition_read_windows_overlay_tombstones_without_catalog_hydration() {
    let definitions = (0..300)
        .map(|index| {
            upstream_definition(
                &format!("gid://shopify/MetafieldDefinition/{:07}", 9_700_000 + index),
                "bounded_window",
                &format!("key_{index:03}"),
                &format!("Window {index:03}"),
            )
        })
        .collect::<Vec<_>>();
    let (mut proxy, requests) = tracking_live_hybrid_definition_proxy(definitions);

    let deleted = proxy.process_request(json_graphql_request(
        r#"
        mutation DeleteWindowHead($identifier: MetafieldDefinitionIdentifierInput!) {
          metafieldDefinitionDelete(identifier: $identifier, deleteAllAssociatedMetafields: true) {
            deletedDefinitionId
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "identifier": {
                "ownerType": "PRODUCT",
                "namespace": "bounded_window",
                "key": "key_000"
            }
        }),
    ));
    assert_eq!(
        deleted.body["data"]["metafieldDefinitionDelete"]["userErrors"],
        json!([])
    );

    let read_window = |proxy: &mut DraftProxy| {
        proxy.process_request(json_graphql_request(
            r#"
            query ReadBoundedDefinitionWindow($namespace: String!) {
              metafieldDefinitions(ownerType: PRODUCT, namespace: $namespace, first: 2, sortKey: ID) {
                edges { cursor node { id key } }
                nodes { id key }
                pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
              }
            }
            "#,
            json!({ "namespace": "bounded_window" }),
        ))
    };
    let first = read_window(&mut proxy);
    assert_eq!(
        first.body["data"]["metafieldDefinitions"]["nodes"],
        json!([
            { "id": "gid://shopify/MetafieldDefinition/9700001", "key": "key_001" },
            { "id": "gid://shopify/MetafieldDefinition/9700002", "key": "key_002" }
        ])
    );
    assert_eq!(
        first.body["data"]["metafieldDefinitions"]["pageInfo"],
        json!({
            "hasNextPage": true,
            "hasPreviousPage": false,
            "startCursor": "gid://shopify/MetafieldDefinition/9700001",
            "endCursor": "gid://shopify/MetafieldDefinition/9700002"
        })
    );
    let requests_after_first_read = requests.lock().unwrap().len();
    assert_eq!(requests_after_first_read, 2);
    let second = read_window(&mut proxy);
    assert_eq!(second.body, first.body);
    assert_eq!(
        requests.lock().unwrap().len(),
        requests_after_first_read,
        "the exact argument-keyed window is reused while staged state is unchanged"
    );
    assert!(requests
        .lock()
        .unwrap()
        .iter()
        .all(|body| { body["operationName"] != json!("MetafieldDefinitionsHydrateOwnerCatalog") }));

    let pinned = proxy.process_request(json_graphql_request(
        r#"
        query ReadPinnedDefinitionWindow($namespace: String!) {
          metafieldDefinitions(
            ownerType: PRODUCT
            namespace: $namespace
            pinnedStatus: PINNED
            first: 2
            sortKey: PINNED_POSITION
          ) {
            nodes { id key }
          }
        }
        "#,
        json!({ "namespace": "bounded_window" }),
    ));
    assert_eq!(
        pinned.body["data"]["metafieldDefinitions"]["nodes"],
        json!([])
    );
    let requests = requests.lock().unwrap();
    let unfiltered_window = &requests[1];
    assert_eq!(
        unfiltered_window["operationName"],
        json!("MetafieldDefinitionsHydrateWindow"),
        "{requests:#?}"
    );
    assert!(unfiltered_window["query"]
        .as_str()
        .unwrap_or_default()
        .contains("pinnedStatus: $pinnedStatus"));
    assert_eq!(unfiltered_window["variables"]["pinnedStatus"], json!("ANY"));
    let pinned_window = requests.last().expect("pinned definition window request");
    assert_eq!(
        pinned_window["operationName"],
        json!("MetafieldDefinitionsHydrateWindow")
    );
    assert!(pinned_window["query"]
        .as_str()
        .unwrap_or_default()
        .contains("pinnedStatus: $pinnedStatus"));
}

#[test]
fn live_hybrid_definition_max_window_refills_once_after_tombstone() {
    let definitions = (0..301)
        .map(|index| {
            upstream_definition(
                &format!("gid://shopify/MetafieldDefinition/{:07}", 9_800_000 + index),
                "bounded_max_window",
                &format!("key_{index:03}"),
                &format!("Window {index:03}"),
            )
        })
        .collect::<Vec<_>>();
    let (mut proxy, requests) = tracking_live_hybrid_definition_proxy(definitions);

    let deleted = proxy.process_request(json_graphql_request(
        r#"
        mutation DeleteMaximumWindowHead($identifier: MetafieldDefinitionIdentifierInput!) {
          metafieldDefinitionDelete(identifier: $identifier, deleteAllAssociatedMetafields: true) {
            deletedDefinitionId
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "identifier": {
                "ownerType": "PRODUCT",
                "namespace": "bounded_max_window",
                "key": "key_000"
            }
        }),
    ));
    assert_eq!(
        deleted.body["data"]["metafieldDefinitionDelete"]["userErrors"],
        json!([])
    );

    let read = proxy.process_request(json_graphql_request(
        r#"
        query ReadMaximumDefinitionWindow($namespace: String!) {
          metafieldDefinitions(ownerType: PRODUCT, namespace: $namespace, first: 250, sortKey: ID) {
            nodes { id key }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
        }
        "#,
        json!({ "namespace": "bounded_max_window" }),
    ));
    let nodes = read.body["data"]["metafieldDefinitions"]["nodes"]
        .as_array()
        .expect("maximum definition window nodes");
    assert_eq!(nodes.len(), 250);
    assert_eq!(nodes.first().unwrap()["key"], json!("key_001"));
    assert_eq!(nodes.last().unwrap()["key"], json!("key_250"));
    assert_eq!(
        requests.lock().unwrap().len(),
        3,
        "one exact delete probe plus two bounded window pages"
    );
    assert!(requests
        .lock()
        .unwrap()
        .iter()
        .all(|body| { body["operationName"] != json!("MetafieldDefinitionsHydrateOwnerCatalog") }));
}

#[test]
fn live_hybrid_definition_scoped_observations_round_trip_and_reset_discards_overlay() {
    let real_definition = upstream_definition(
        "gid://shopify/MetafieldDefinition/9900001",
        "round_trip_window",
        "real",
        "Round trip real",
    );
    let (mut proxy, _) = tracking_live_hybrid_definition_proxy(vec![real_definition.clone()]);
    let created = create_definition(&mut proxy, "round_trip_window", "local", true);
    assert_eq!(created["userErrors"], json!([]));

    let read_window = |proxy: &mut DraftProxy| {
        proxy.process_request(json_graphql_request(
            r#"
            query ReadRoundTrippedDefinitionWindow($namespace: String!) {
              metafieldDefinitions(ownerType: PRODUCT, namespace: $namespace, first: 10, sortKey: ID) {
                nodes { id key pinnedPosition }
                pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
              }
            }
            "#,
            json!({ "namespace": "round_trip_window" }),
        ))
    };
    let before_dump = read_window(&mut proxy);
    assert_eq!(
        before_dump.body["data"]["metafieldDefinitions"]["nodes"]
            .as_array()
            .unwrap()
            .len(),
        2
    );

    let missing_id_read = proxy.process_request(json_graphql_request(
        r#"
        query ObserveMissingMetafieldDefinitionId($id: ID!) {
          metafieldDefinition(id: $id) { id }
        }
        "#,
        json!({ "id": "gid://shopify/MetafieldDefinition/9999999" }),
    ));
    assert_eq!(
        missing_id_read.body["data"]["metafieldDefinition"],
        Value::Null
    );

    let dump = proxy.process_request(request_with_body("POST", "/__meta/dump", "{}"));
    assert_eq!(dump.status, 200);
    let base_state = &dump.body["state"]["baseState"];
    assert!(!base_state["metafieldDefinitionObservedIdentities"]
        .as_array()
        .unwrap()
        .is_empty());
    assert_eq!(
        base_state["metafieldDefinitionObservedIds"],
        json!(["gid://shopify/MetafieldDefinition/9999999"])
    );
    assert!(!base_state["metafieldDefinitionResourceScopes"]
        .as_array()
        .unwrap()
        .is_empty());
    assert_eq!(
        base_state["metafieldDefinitionPinnedOwnerScopes"],
        json!(["PRODUCT"])
    );
    assert!(!base_state["metafieldDefinitionWindows"]
        .as_object()
        .unwrap()
        .is_empty());

    let reset_upstream_requests = Arc::new(Mutex::new(Vec::<Value>::new()));
    let captured_requests = Arc::clone(&reset_upstream_requests);
    let mut restored =
        configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport(move |request| {
            let body: Value = serde_json::from_str(&request.body).unwrap();
            captured_requests.lock().unwrap().push(body.clone());
            metafield_definition_upstream_response(std::slice::from_ref(&real_definition), &body)
        });
    let restore = restored.process_request(request_with_body(
        "POST",
        "/__meta/restore",
        &dump.body.to_string(),
    ));
    assert_eq!(restore.status, 200);
    assert_eq!(read_window(&mut restored).body, before_dump.body);
    assert_eq!(
        restored
            .process_request(json_graphql_request(
                r#"
                query ReadObservedMissingMetafieldDefinitionId($id: ID!) {
                  metafieldDefinition(id: $id) { id }
                }
                "#,
                json!({ "id": "gid://shopify/MetafieldDefinition/9999999" }),
            ))
            .body["data"]["metafieldDefinition"],
        Value::Null
    );
    assert!(reset_upstream_requests.lock().unwrap().is_empty());

    let reset = restored.process_request(request_with_body("POST", "/__meta/reset", ""));
    assert_eq!(reset.status, 200);
    assert_eq!(log_snapshot(&restored), json!({ "entries": [] }));
    assert!(
        state_snapshot(&restored)["stagedState"]["metafieldDefinitions"]
            .as_object()
            .is_none_or(serde_json::Map::is_empty)
    );
    let after_reset = read_window(&mut restored);
    let reset_nodes = after_reset.body["data"]["metafieldDefinitions"]["nodes"]
        .as_array()
        .expect("upstream window after reset");
    assert_eq!(reset_nodes.len(), 1);
    assert_eq!(reset_nodes[0]["key"], json!("real"));
    assert_eq!(reset_upstream_requests.lock().unwrap().len(), 1);
}

#[test]
fn metafield_definition_rejections_are_atomic_and_commit_replays_raw_mutations_in_order() {
    let runtime_upstream_calls = Arc::new(Mutex::new(0usize));
    let observed_runtime_upstream_calls = Arc::clone(&runtime_upstream_calls);
    let replayed = Arc::new(Mutex::new(Vec::<Request>::new()));
    let observed_replayed = Arc::clone(&replayed);
    let mut proxy = snapshot_proxy()
        .with_upstream_transport(move |_| {
            *observed_runtime_upstream_calls.lock().unwrap() += 1;
            panic!("supported definition mutations must not use the runtime upstream transport")
        })
        .with_commit_transport(move |request| {
            observed_replayed.lock().unwrap().push(request);
            Response {
                status: 200,
                headers: Default::default(),
                body: json!({
                    "data": {
                        "metafieldDefinitionCreate": {
                            "createdDefinition": {
                                "id": "gid://shopify/MetafieldDefinition/9900100"
                            }
                        }
                    }
                }),
            }
        });
    let namespace = "ordered_commit";
    let create_query = r#"
        mutation CommitOrderedDefinitionCreate($definition: MetafieldDefinitionInput!) {
          metafieldDefinitionCreate(definition: $definition) {
            createdDefinition { id key }
            userErrors { field message code }
          }
        }
        "#;
    let definition = json!({
        "ownerType": "PRODUCT",
        "namespace": namespace,
        "key": "target",
        "name": "Commit target",
        "type": "single_line_text_field"
    });
    let create = proxy.process_request(json_graphql_request(
        create_query,
        json!({ "definition": definition }),
    ));
    assert_eq!(
        create.body["data"]["metafieldDefinitionCreate"]["userErrors"],
        json!([])
    );

    let state_before_rejection = state_snapshot(&proxy);
    let log_before_rejection = log_snapshot(&proxy);
    let duplicate = proxy.process_request(json_graphql_request(
        r#"
        mutation RejectedDuplicateDefinitionDoesNotLog($definition: MetafieldDefinitionInput!) {
          metafieldDefinitionCreate(definition: $definition) {
            createdDefinition { id }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "definition": {
            "ownerType": "PRODUCT",
            "namespace": namespace,
            "key": "target",
            "name": "Rejected duplicate",
            "type": "single_line_text_field"
        }}),
    ));
    assert_eq!(
        duplicate.body["data"]["metafieldDefinitionCreate"]["userErrors"][0]["code"],
        json!("TAKEN")
    );
    assert_eq!(state_snapshot(&proxy), state_before_rejection);
    assert_eq!(log_snapshot(&proxy), log_before_rejection);

    let update_query = r#"
        mutation CommitOrderedDefinitionUpdate($definition: MetafieldDefinitionUpdateInput!) {
          metafieldDefinitionUpdate(definition: $definition) {
            updatedDefinition { id name }
            userErrors { field message code }
            validationJob { id }
          }
        }
        "#;
    let update = proxy.process_request(json_graphql_request(
        update_query,
        json!({ "definition": {
            "ownerType": "PRODUCT",
            "namespace": namespace,
            "key": "target",
            "name": "Updated commit target"
        }}),
    ));
    assert_eq!(
        update.body["data"]["metafieldDefinitionUpdate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        pin_definition(&mut proxy, namespace, "target")["userErrors"],
        json!([])
    );
    assert_eq!(
        unpin_definition(&mut proxy, namespace, "target")["userErrors"],
        json!([])
    );
    let delete_query = r#"
        mutation CommitOrderedDefinitionDelete($identifier: MetafieldDefinitionIdentifierInput!) {
          metafieldDefinitionDelete(identifier: $identifier, deleteAllAssociatedMetafields: true) {
            deletedDefinitionId
            userErrors { field message code }
          }
        }
        "#;
    let delete = proxy.process_request(json_graphql_request(
        delete_query,
        json!({ "identifier": {
            "ownerType": "PRODUCT",
            "namespace": namespace,
            "key": "target"
        }}),
    ));
    assert_eq!(
        delete.body["data"]["metafieldDefinitionDelete"]["userErrors"],
        json!([])
    );
    assert_eq!(*runtime_upstream_calls.lock().unwrap(), 0);

    let staged_log = log_snapshot(&proxy);
    let entries = staged_log["entries"].as_array().unwrap();
    assert_eq!(entries.len(), 5);
    assert!(entries
        .iter()
        .all(|entry| entry["status"] == json!("staged")));
    for (entry, operation_name) in entries.iter().zip([
        "CommitOrderedDefinitionCreate",
        "CommitOrderedDefinitionUpdate",
        "MetafieldDefinitionPinForLimit",
        "MetafieldDefinitionUnpinForLimit",
        "CommitOrderedDefinitionDelete",
    ]) {
        assert!(entry["rawBody"].as_str().unwrap().contains(operation_name));
    }

    let commit = proxy.process_request(request_with_body("POST", "/__meta/commit", ""));
    assert_eq!(commit.status, 200);
    assert_eq!(commit.body["ok"], json!(true));
    assert_eq!(commit.body["committed"], json!(5));
    assert_eq!(commit.body["failed"], json!(0));
    let replayed = replayed.lock().unwrap();
    assert_eq!(replayed.len(), 5);
    for (request, operation_name) in replayed.iter().zip([
        "CommitOrderedDefinitionCreate",
        "CommitOrderedDefinitionUpdate",
        "MetafieldDefinitionPinForLimit",
        "MetafieldDefinitionUnpinForLimit",
        "CommitOrderedDefinitionDelete",
    ]) {
        assert!(request.body.contains(operation_name));
    }
}

#[test]
fn metafield_definition_create_duplicate_and_cross_owner_keys_are_owner_scoped() {
    let mut proxy = snapshot_proxy();
    let namespace = "owner_scope";

    let create = |proxy: &mut DraftProxy, owner_type: &str, name: &str| {
        proxy
            .process_request(json_graphql_request(
                r#"
        mutation MetafieldDefinitionCreateScoped($definition: MetafieldDefinitionInput!) {
          metafieldDefinitionCreate(definition: $definition) {
            createdDefinition { id namespace key ownerType name pinnedPosition }
            userErrors { __typename field message code }
          }
        }
        "#,
                json!({
                    "definition": {
                        "ownerType": owner_type,
                        "namespace": namespace,
                        "key": "spec",
                        "name": name,
                        "type": "single_line_text_field"
                    }
                }),
            ))
            .body["data"]["metafieldDefinitionCreate"]
            .clone()
    };

    let product = create(&mut proxy, "PRODUCT", "Product spec");
    let product_id = product["createdDefinition"]["id"].clone();
    assert_eq!(product["userErrors"], json!([]));

    let duplicate = create(&mut proxy, "PRODUCT", "Duplicate product spec");
    assert_eq!(
        duplicate,
        json!({
            "createdDefinition": null,
            "userErrors": [{
                "__typename": "MetafieldDefinitionCreateUserError",
                "field": ["definition", "key"],
                "message": "Key is in use for Product metafields on the 'owner_scope' namespace.",
                "code": "TAKEN"
            }]
        })
    );

    let customer = create(&mut proxy, "CUSTOMER", "Customer spec");
    let customer_id = customer["createdDefinition"]["id"].clone();
    assert_eq!(customer["userErrors"], json!([]));
    assert_ne!(product_id, customer_id);

    let read = proxy.process_request(json_graphql_request(
        r#"
        query MetafieldDefinitionReadScoped($namespace: String!) {
          productByIdentifier: metafieldDefinition(identifier: { ownerType: PRODUCT, namespace: $namespace, key: "spec" }) {
            id ownerType namespace key name pinnedPosition
          }
          customerByIdentifier: metafieldDefinition(identifier: { ownerType: CUSTOMER, namespace: $namespace, key: "spec" }) {
            id ownerType namespace key name pinnedPosition
          }
          productCatalog: metafieldDefinitions(ownerType: PRODUCT, namespace: $namespace, first: 10) {
            nodes { id ownerType namespace key name pinnedPosition }
          }
          customerCatalog: metafieldDefinitions(ownerType: CUSTOMER, namespace: $namespace, first: 10) {
            nodes { id ownerType namespace key name pinnedPosition }
          }
        }
        "#,
        json!({ "namespace": namespace }),
    ));
    assert_eq!(
        read.body["data"],
        json!({
            "productByIdentifier": {
                "id": product_id,
                "ownerType": "PRODUCT",
                "namespace": namespace,
                "key": "spec",
                "name": "Product spec",
                "pinnedPosition": null
            },
            "customerByIdentifier": {
                "id": customer_id,
                "ownerType": "CUSTOMER",
                "namespace": namespace,
                "key": "spec",
                "name": "Customer spec",
                "pinnedPosition": null
            },
            "productCatalog": {
                "nodes": [{
                    "id": product_id,
                    "ownerType": "PRODUCT",
                    "namespace": namespace,
                    "key": "spec",
                    "name": "Product spec",
                    "pinnedPosition": null
                }]
            },
            "customerCatalog": {
                "nodes": [{
                    "id": customer_id,
                    "ownerType": "CUSTOMER",
                    "namespace": namespace,
                    "key": "spec",
                    "name": "Customer spec",
                    "pinnedPosition": null
                }]
            }
        })
    );

    let customer_update = proxy.process_request(json_graphql_request(
        r#"
        mutation MetafieldDefinitionUpdateScoped($definition: MetafieldDefinitionUpdateInput!) {
          metafieldDefinitionUpdate(definition: $definition) {
            updatedDefinition { id ownerType namespace key name }
            userErrors { field message code }
            validationJob { id }
          }
        }
        "#,
        json!({
            "definition": {
                "ownerType": "CUSTOMER",
                "namespace": namespace,
                "key": "spec",
                "name": "Updated customer spec"
            }
        }),
    ));
    assert_eq!(
        customer_update.body["data"]["metafieldDefinitionUpdate"]["updatedDefinition"],
        json!({
            "id": customer_id,
            "ownerType": "CUSTOMER",
            "namespace": namespace,
            "key": "spec",
            "name": "Updated customer spec"
        })
    );

    let pinned = proxy.process_request(json_graphql_request(
        r#"
        mutation MetafieldDefinitionPinScoped($identifier: MetafieldDefinitionIdentifierInput!) {
          metafieldDefinitionPin(identifier: $identifier) {
            pinnedDefinition { id ownerType namespace key name pinnedPosition }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "identifier": { "ownerType": "PRODUCT", "namespace": namespace, "key": "spec" } }),
    ));
    assert_eq!(
        pinned.body["data"]["metafieldDefinitionPin"]["pinnedDefinition"],
        json!({
            "id": product_id,
            "ownerType": "PRODUCT",
            "namespace": namespace,
            "key": "spec",
            "name": "Product spec",
            "pinnedPosition": 1
        })
    );

    let unpinned = proxy.process_request(json_graphql_request(
        r#"
        mutation MetafieldDefinitionUnpinScoped($identifier: MetafieldDefinitionIdentifierInput!) {
          metafieldDefinitionUnpin(identifier: $identifier) {
            unpinnedDefinition { id ownerType namespace key name pinnedPosition }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "identifier": { "ownerType": "PRODUCT", "namespace": namespace, "key": "spec" } }),
    ));
    assert_eq!(
        unpinned.body["data"]["metafieldDefinitionUnpin"]["unpinnedDefinition"]["pinnedPosition"],
        json!(null)
    );

    let deleted = proxy.process_request(json_graphql_request(
        r#"
        mutation MetafieldDefinitionDeleteScoped($identifier: MetafieldDefinitionIdentifierInput!) {
          metafieldDefinitionDelete(identifier: $identifier) {
            deletedDefinition { ownerType namespace key }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "identifier": { "ownerType": "PRODUCT", "namespace": namespace, "key": "spec" } }),
    ));
    assert_eq!(
        deleted.body["data"]["metafieldDefinitionDelete"],
        json!({
            "deletedDefinition": {
                "ownerType": "PRODUCT",
                "namespace": namespace,
                "key": "spec"
            },
            "userErrors": []
        })
    );

    let after_delete = proxy.process_request(json_graphql_request(
        r#"
        query MetafieldDefinitionReadAfterScopedDelete($namespace: String!) {
          productByIdentifier: metafieldDefinition(identifier: { ownerType: PRODUCT, namespace: $namespace, key: "spec" }) { id }
          customerByIdentifier: metafieldDefinition(identifier: { ownerType: CUSTOMER, namespace: $namespace, key: "spec" }) { id ownerType name }
        }
        "#,
        json!({ "namespace": namespace }),
    ));
    assert_eq!(
        after_delete.body["data"]["productByIdentifier"],
        json!(null)
    );
    assert_eq!(
        after_delete.body["data"]["customerByIdentifier"],
        json!({
            "id": customer_id,
            "ownerType": "CUSTOMER",
            "name": "Updated customer spec"
        })
    );
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

fn standard_enable_subtitle(proxy: &mut DraftProxy, pin: Option<bool>) -> Value {
    proxy
        .process_request(json_graphql_request(
            r#"
        mutation StandardMetafieldDefinitionEnableSubtitle($pin: Boolean) {
          standardMetafieldDefinitionEnable(
            ownerType: PRODUCT
            id: "gid://shopify/StandardMetafieldDefinitionTemplate/1"
            pin: $pin
          ) {
            createdDefinition {
              id
              namespace
              key
              pinnedPosition
              capabilities {
                adminFilterable { enabled eligible status }
                smartCollectionCondition { enabled eligible }
                uniqueValues { enabled eligible }
              }
              access { admin storefront customerAccount }
            }
            userErrors { field message code }
          }
        }
        "#,
            json!({ "pin": pin }),
        ))
        .body["data"]["standardMetafieldDefinitionEnable"]
        .clone()
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
fn standard_metafield_definition_reenable_preserves_id_and_merges_update_params() {
    let mut proxy = snapshot_proxy();

    let initial = standard_enable_subtitle(&mut proxy, None);
    assert_eq!(initial["userErrors"], json!([]));
    let initial_id = initial["createdDefinition"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let reenabled = proxy.process_request(json_graphql_request(
        r#"
        mutation StandardMetafieldDefinitionReenableMerge {
          standardMetafieldDefinitionEnable(
            ownerType: PRODUCT
            namespace: "descriptors"
            key: "subtitle"
            access: { admin: MERCHANT_READ_WRITE, storefront: PUBLIC_READ }
            capabilities: { adminFilterable: { enabled: true } }
          ) {
            createdDefinition {
              id
              namespace
              key
              pinnedPosition
              access { admin storefront customerAccount }
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
    let payload = &reenabled.body["data"]["standardMetafieldDefinitionEnable"];
    assert_eq!(payload["userErrors"], json!([]));
    assert_eq!(payload["createdDefinition"]["id"], json!(initial_id));
    assert_eq!(
        payload["createdDefinition"]["access"],
        json!({
            "admin": "PUBLIC_READ_WRITE",
            "storefront": "PUBLIC_READ",
            "customerAccount": "NONE"
        })
    );
    assert_eq!(
        payload["createdDefinition"]["capabilities"]["adminFilterable"],
        json!({ "enabled": true, "eligible": true, "status": "FILTERABLE" })
    );

    let read = proxy.process_request(json_graphql_request(
        r#"
        query StandardMetafieldDefinitionReenableRead($id: ID!) {
          byOriginalId: metafieldDefinition(id: $id) {
            id
            namespace
            key
            capabilities { adminFilterable { enabled status } }
          }
          byIdentifier: metafieldDefinition(
            identifier: { ownerType: PRODUCT, namespace: "descriptors", key: "subtitle" }
          ) {
            id
            namespace
            key
            capabilities { adminFilterable { enabled status } }
          }
        }
        "#,
        json!({ "id": initial_id }),
    ));
    assert_eq!(
        read.body["data"]["byOriginalId"],
        read.body["data"]["byIdentifier"]
    );
    assert_eq!(read.body["data"]["byOriginalId"]["id"], json!(initial_id));
    assert_eq!(
        read.body["data"]["byOriginalId"]["capabilities"]["adminFilterable"],
        json!({ "enabled": true, "status": "FILTERABLE" })
    );
}

#[test]
fn standard_template_metafield_definition_update_rejects_immutable_field_edits() {
    let mut proxy = snapshot_proxy();

    let initial = standard_enable_subtitle(&mut proxy, None);
    assert_eq!(initial["userErrors"], json!([]));

    let rejected = proxy.process_request(json_graphql_request(
        r#"
        mutation StandardTemplateDefinitionImmutableUpdate($definition: MetafieldDefinitionUpdateInput!) {
          metafieldDefinitionUpdate(definition: $definition) {
            updatedDefinition { id name description validations { name value } }
            userErrors { __typename field message code }
            validationJob { id }
          }
        }
        "#,
        json!({
            "definition": {
                "ownerType": "PRODUCT",
                "namespace": "descriptors",
                "key": "subtitle",
                "name": "Renamed subtitle",
                "description": "Changed description",
                "validations": [{ "name": "max", "value": "80" }]
            }
        }),
    ));
    assert_eq!(
        rejected.body["data"]["metafieldDefinitionUpdate"],
        json!({
            "updatedDefinition": null,
            "userErrors": [
                {
                    "__typename": "MetafieldDefinitionUpdateUserError",
                    "field": ["definition", "name"],
                    "message": "Name cannot be changed in a standard definition.",
                    "code": "INVALID_INPUT"
                },
                {
                    "__typename": "MetafieldDefinitionUpdateUserError",
                    "field": ["definition", "description"],
                    "message": "Description cannot be changed in a standard definition.",
                    "code": "INVALID_INPUT"
                },
                {
                    "__typename": "MetafieldDefinitionUpdateUserError",
                    "field": ["definition", "validations"],
                    "message": "Validations cannot be changed in a standard definition.",
                    "code": "INVALID_INPUT"
                }
            ],
            "validationJob": null
        })
    );

    let read_after_reject = proxy.process_request(json_graphql_request(
        r#"
        query StandardTemplateDefinitionReadAfterRejectedUpdate {
          metafieldDefinition(identifier: { ownerType: PRODUCT, namespace: "descriptors", key: "subtitle" }) {
            name
            description
            validations { name value }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        read_after_reject.body["data"]["metafieldDefinition"],
        json!({
            "name": "Product subtitle",
            "description": "Used as a shorthand for a product name",
            "validations": [{ "name": "max", "value": "70" }]
        })
    );

    let allowed = proxy.process_request(json_graphql_request(
        r#"
        mutation StandardTemplateDefinitionNonImmutableUpdate($definition: MetafieldDefinitionUpdateInput!) {
          metafieldDefinitionUpdate(definition: $definition) {
            updatedDefinition {
              namespace
              key
              pinnedPosition
              access { admin storefront customerAccount }
              capabilities { adminFilterable { enabled eligible status } }
            }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "definition": {
                "ownerType": "PRODUCT",
                "namespace": "descriptors",
                "key": "subtitle",
                "pin": true,
                "access": { "admin": "MERCHANT_READ_WRITE", "storefront": "PUBLIC_READ" },
                "capabilities": { "adminFilterable": { "enabled": true } }
            }
        }),
    ));
    assert_eq!(
        allowed.body["data"]["metafieldDefinitionUpdate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        allowed.body["data"]["metafieldDefinitionUpdate"]["updatedDefinition"],
        json!({
            "namespace": "descriptors",
            "key": "subtitle",
            "pinnedPosition": 1,
            "access": {
                "admin": "PUBLIC_READ_WRITE",
                "storefront": "PUBLIC_READ",
                "customerAccount": "NONE"
            },
            "capabilities": {
                "adminFilterable": {
                    "enabled": true,
                    "eligible": true,
                    "status": "FILTERABLE"
                }
            }
        })
    );
}

#[test]
fn standard_metafield_definition_reenable_pin_over_cap_uses_next_position() {
    let namespace = "standard_reenable_pin_cap";
    let mut proxy = snapshot_proxy();

    for index in 1..=50 {
        let key = format!("pin_{index:02}");
        let created = create_definition(&mut proxy, namespace, &key, true);
        assert_eq!(created["userErrors"], json!([]));
    }

    let initial = standard_enable_subtitle(&mut proxy, None);
    assert_eq!(initial["userErrors"], json!([]));
    assert_eq!(initial["createdDefinition"]["pinnedPosition"], Value::Null);
    let initial_id = initial["createdDefinition"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let reenabled = standard_enable_subtitle(&mut proxy, Some(true));
    assert_eq!(reenabled["userErrors"], json!([]));
    assert_eq!(reenabled["createdDefinition"]["id"], json!(initial_id));
    assert_eq!(reenabled["createdDefinition"]["pinnedPosition"], json!(51));
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

    let unpin_b = proxy.process_request(json_graphql_request(
        r#"
        mutation MetafieldDefinitionUnpinByIdentifier($identifier: MetafieldDefinitionIdentifierInput!) {
          metafieldDefinitionUnpin(identifier: $identifier) { unpinnedDefinition { id key pinnedPosition } userErrors { field message code } }
        }
        "#,
        json!({"identifier": {"ownerType": "PRODUCT", "namespace": namespace, "key": "pin_b"}}),
    ));
    assert_eq!(
        unpin_b.body["data"]["metafieldDefinitionUnpin"]["unpinnedDefinition"]["pinnedPosition"],
        Value::Null
    );

    let unpinned_read = proxy.process_request(json_graphql_request(
        r#"
        query MetafieldDefinitionUnpinnedPositionRead($namespace: String!) {
          metafieldDefinitions(ownerType: PRODUCT, first: 10, namespace: $namespace, sortKey: PINNED_POSITION) {
            nodes { key pinnedPosition }
          }
        }
        "#,
        json!({"namespace": namespace}),
    ));
    assert_eq!(
        unpinned_read.body["data"]["metafieldDefinitions"]["nodes"],
        json!([
            {"key": "pin_b", "pinnedPosition": null},
            {"key": "pin_a", "pinnedPosition": null}
        ])
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
        json!("LIMIT_EXCEEDED")
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
            "message": "Stores can only have 256 definitions for each store resource.",
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

    let mut standard_count_proxy = snapshot_proxy();
    let standard_first = standard_enable_pin(&mut standard_count_proxy);
    assert_eq!(standard_first["userErrors"], json!([]));
    for index in 0..255 {
        let created = create_definition_for_resource_limit(
            &mut standard_count_proxy,
            "PRODUCT",
            "resource_limit_after_standard",
            &format!("key_{index:03}"),
        );
        assert_eq!(created["userErrors"], json!([]));
    }
    let standard_count_over_limit = create_definition_for_resource_limit(
        &mut standard_count_proxy,
        "PRODUCT",
        "resource_limit_after_standard",
        "key_255",
    );
    assert_eq!(
        standard_count_over_limit["userErrors"][0]["code"],
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

    let rejected_read = proxy.process_request(json_graphql_request(
        r#"
        query RejectedCapabilityRead($namespace: String!) {
          rejectedRead: metafieldDefinition(identifier: { ownerType: PRODUCT, namespace: $namespace, key: "json_unique" }) {
            id
          }
        }
        "#,
        json!({ "namespace": namespace }),
    ));
    assert_eq!(rejected_read.body["data"]["rejectedRead"], Value::Null);

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
        id_disabled.body["data"]["metafieldDefinitionCreate"],
        json!({
            "createdDefinition": null,
            "userErrors": [{
                "field": ["definition"],
                "message": "Capability unique_values is required for type id but is disabled",
                "code": "CAPABILITY_REQUIRED_BUT_DISABLED"
            }]
        })
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
fn standard_metafield_definition_enable_public_hidden_arguments_match_live_branches() {
    let mut proxy = snapshot_proxy();

    let visible = proxy.process_request(json_graphql_request(
        r#"
        mutation StandardMetafieldDefinitionEnableVisibleStorefront {
          standardMetafieldDefinitionEnable(
            ownerType: PRODUCT
            id: "gid://shopify/StandardMetafieldDefinitionTemplate/3"
            visibleToStorefrontApi: false
          ) {
            createdDefinition {
              namespace
              key
              access { storefront }
            }
            userErrors { field message code }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        visible.body["data"]["standardMetafieldDefinitionEnable"],
        json!({
            "createdDefinition": {
                "namespace": "facts",
                "key": "isbn",
                "access": { "storefront": "NONE" }
            },
            "userErrors": []
        })
    );

    let condition = proxy.process_request(json_graphql_request(
        r#"
        mutation StandardMetafieldDefinitionEnableDeprecatedCondition {
          standardMetafieldDefinitionEnable(
            ownerType: PRODUCT
            id: "gid://shopify/StandardMetafieldDefinitionTemplate/2"
            useAsCollectionCondition: true
          ) {
            createdDefinition { namespace key }
            userErrors { field message code }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        condition.body["data"]["standardMetafieldDefinitionEnable"],
        json!({
            "createdDefinition": null,
            "userErrors": [{
                "field": null,
                "message": "The capability smart_collection_condition is not valid for this definition.",
                "code": "INVALID_CAPABILITY"
            }]
        })
    );

    let legacy_admin_filter = proxy.process_request(json_graphql_request(
        r#"
        mutation StandardMetafieldDefinitionEnableLegacyAdminFilter {
          standardMetafieldDefinitionEnable(
            ownerType: PRODUCT
            id: "gid://shopify/StandardMetafieldDefinitionTemplate/3"
            useAsAdminFilter: true
            visibleToStorefrontApi: false
          ) {
            createdDefinition {
              namespace
              key
              access { storefront }
              capabilities { adminFilterable { enabled eligible status } }
            }
            userErrors { field message code }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        legacy_admin_filter.body["errors"][0]["extensions"],
        json!({
            "code": "argumentNotAccepted",
            "name": "standardMetafieldDefinitionEnable",
            "typeName": "Field",
            "argumentName": "useAsAdminFilter"
        })
    );

    let unstructured_setup = proxy.process_request(json_graphql_request(
        r#"
        mutation StandardMetafieldDefinitionEnableMetafieldsSetSubtitle {
          metafieldsSet(
            metafields: [{
              ownerId: "gid://shopify/Product/1"
              namespace: "descriptors"
              key: "subtitle"
              type: "single_line_text_field"
              value: "Existing subtitle"
            }]
          ) {
            metafields { namespace key }
            userErrors { field message code }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        unstructured_setup.body["data"]["metafieldsSet"]["userErrors"],
        json!([])
    );

    let force_false = proxy.process_request(json_graphql_request(
        r#"
        mutation StandardMetafieldDefinitionEnableForceFalse {
          standardMetafieldDefinitionEnable(
            ownerType: PRODUCT
            id: "gid://shopify/StandardMetafieldDefinitionTemplate/1"
            forceEnable: false
          ) {
            createdDefinition { namespace key }
            userErrors { field message code }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        force_false.body["errors"][0]["extensions"],
        json!({
            "code": "argumentNotAccepted",
            "name": "standardMetafieldDefinitionEnable",
            "typeName": "Field",
            "argumentName": "forceEnable"
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
fn metafield_definition_create_input_validation_matches_live_branches_and_runtime_reserved_guards()
{
    let mut proxy = snapshot_proxy();
    let create = |proxy: &mut DraftProxy, definition: Value| {
        proxy
            .process_request(json_graphql_request(
                r#"
        mutation MetafieldDefinitionCreateInputValidation($definition: MetafieldDefinitionInput!) {
          metafieldDefinitionCreate(definition: $definition) {
            createdDefinition { id }
            userErrors { field message code }
          }
        }
        "#,
                json!({ "definition": definition }),
            ))
            .body["data"]["metafieldDefinitionCreate"]
            .clone()
    };

    assert_eq!(
        create(
            &mut proxy,
            json!({
                "namespace": "my space",
                "key": "valid_key",
                "ownerType": "PRODUCT",
                "name": "X",
                "type": "single_line_text_field"
            }),
        )["userErrors"],
        json!([{
            "field": ["definition", "namespace"],
            "message": "Namespace contains one or more invalid characters.",
            "code": "INVALID_CHARACTER"
        }])
    );
    assert_eq!(
        create(
            &mut proxy,
            json!({
                "namespace": "loyalty",
                "key": "bad.key!",
                "ownerType": "PRODUCT",
                "name": "X",
                "type": "single_line_text_field"
            }),
        )["userErrors"],
        json!([{
            "field": ["definition", "key"],
            "message": "Key contains one or more invalid characters.",
            "code": "INVALID_CHARACTER"
        }])
    );

    let unknown_type = create(
        &mut proxy,
        json!({
            "namespace": "loyalty",
            "key": "tier",
            "ownerType": "PRODUCT",
            "name": "Tier",
            "type": "totally_made_up_type"
        }),
    );
    let unknown_type_message = unknown_type["userErrors"][0]["message"]
        .as_str()
        .expect("unknown type message");
    assert_eq!(
        unknown_type["userErrors"][0]["field"],
        json!(["definition", "type"])
    );
    assert_eq!(unknown_type["userErrors"][0]["code"], json!("INCLUSION"));
    assert!(unknown_type_message.contains("jurisdiction"));
    assert!(unknown_type_message.contains("product_taxonomy_disclosure_reference"));
    assert!(unknown_type_message.contains("list.disclosure_reference"));

    assert_eq!(
        create(
            &mut proxy,
            json!({
                "namespace": "loyalty",
                "key": "disclosures",
                "ownerType": "PRODUCT",
                "name": "Disclosures",
                "type": "list.disclosure_reference"
            }),
        ),
        json!({
            "createdDefinition": null,
            "userErrors": [{
                "field": ["definition"],
                "message": "The disclosure_reference type can only be used in standard definitions provided by Shopify.",
                "code": null
            }]
        })
    );

    for namespace in ["shopify_standard", "protected"] {
        let reserved = create(
            &mut proxy,
            json!({
                "namespace": namespace,
                "key": "xx",
                "ownerType": "PRODUCT",
                "name": "X",
                "type": "single_line_text_field"
            }),
        );
        assert_eq!(reserved["createdDefinition"], Value::Null);
        assert_eq!(
            reserved["userErrors"],
            json!([{
                "field": ["definition", "namespace"],
                "message": format!("Namespace {namespace} is reserved."),
                "code": "RESERVED_NAMESPACE_KEY"
            }])
        );
    }

    let name_too_long = create(
        &mut proxy,
        json!({
            "namespace": "loyalty",
            "key": "longname",
            "ownerType": "PRODUCT",
            "name": "N".repeat(256),
            "type": "single_line_text_field"
        }),
    );
    assert_eq!(
        name_too_long["userErrors"],
        json!([{
            "field": ["definition", "name"],
            "message": "Name is too long (maximum is 255 characters)",
            "code": "TOO_LONG"
        }])
    );
}

#[test]
fn metafield_definition_update_validates_name_description_length_without_mutating() {
    let mut proxy = snapshot_proxy();
    let namespace = "length_update";
    let key = "season";

    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation MetafieldDefinitionUpdateLengthSetup($definition: MetafieldDefinitionInput!) {
          metafieldDefinitionCreate(definition: $definition) {
            createdDefinition { id namespace key name description }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "definition": {
                "namespace": namespace,
                "key": key,
                "ownerType": "PRODUCT",
                "name": "Season",
                "description": "Original description",
                "type": "single_line_text_field"
            }
        }),
    ));
    assert_eq!(
        create.body["data"]["metafieldDefinitionCreate"]["userErrors"],
        json!([])
    );

    let log_len_after_create = log_snapshot(&proxy)["entries"]
        .as_array()
        .expect("log entries after create")
        .len();

    let update = |proxy: &mut DraftProxy, definition: Value| {
        proxy
            .process_request(json_graphql_request(
                r#"
        mutation MetafieldDefinitionUpdateLength($definition: MetafieldDefinitionUpdateInput!) {
          metafieldDefinitionUpdate(definition: $definition) {
            updatedDefinition { namespace key name description }
            userErrors { __typename field message code }
            validationJob { id }
          }
        }
        "#,
                json!({ "definition": definition }),
            ))
            .body["data"]["metafieldDefinitionUpdate"]
            .clone()
    };
    let read_definition = |proxy: &mut DraftProxy| {
        proxy
            .process_request(json_graphql_request(
                r#"
        query MetafieldDefinitionUpdateLengthRead($identifier: MetafieldDefinitionIdentifierInput!) {
          metafieldDefinition(identifier: $identifier) {
            namespace
            key
            name
            description
          }
        }
        "#,
                json!({
                    "identifier": {
                        "ownerType": "PRODUCT",
                        "namespace": namespace,
                        "key": key
                    }
                }),
            ))
            .body["data"]["metafieldDefinition"]
            .clone()
    };

    let too_long_name = update(
        &mut proxy,
        json!({
            "ownerType": "PRODUCT",
            "namespace": namespace,
            "key": key,
            "name": "N".repeat(256)
        }),
    );
    assert_eq!(
        too_long_name,
        json!({
            "updatedDefinition": null,
            "userErrors": [{
                "__typename": "MetafieldDefinitionUpdateUserError",
                "field": ["definition", "name"],
                "message": "Name is too long (maximum is 255 characters)",
                "code": "TOO_LONG"
            }],
            "validationJob": null
        })
    );
    assert_eq!(
        read_definition(&mut proxy),
        json!({
            "namespace": namespace,
            "key": key,
            "name": "Season",
            "description": "Original description"
        })
    );
    assert_eq!(
        log_snapshot(&proxy)["entries"].as_array().unwrap().len(),
        log_len_after_create
    );

    let too_long_description = update(
        &mut proxy,
        json!({
            "ownerType": "PRODUCT",
            "namespace": namespace,
            "key": key,
            "description": "D".repeat(256)
        }),
    );
    assert_eq!(
        too_long_description,
        json!({
            "updatedDefinition": null,
            "userErrors": [{
                "__typename": "MetafieldDefinitionUpdateUserError",
                "field": ["definition", "description"],
                "message": "Description is too long (maximum is 255 characters)",
                "code": "TOO_LONG"
            }],
            "validationJob": null
        })
    );
    assert_eq!(
        read_definition(&mut proxy),
        json!({
            "namespace": namespace,
            "key": key,
            "name": "Season",
            "description": "Original description"
        })
    );
    assert_eq!(
        log_snapshot(&proxy)["entries"].as_array().unwrap().len(),
        log_len_after_create
    );

    let boundary_name = "N".repeat(255);
    let boundary_description = "D".repeat(255);
    let boundary_update = update(
        &mut proxy,
        json!({
            "ownerType": "PRODUCT",
            "namespace": namespace,
            "key": key,
            "name": boundary_name,
            "description": boundary_description
        }),
    );
    assert_eq!(boundary_update["userErrors"], json!([]));
    assert_eq!(
        boundary_update["updatedDefinition"],
        json!({
            "namespace": namespace,
            "key": key,
            "name": "N".repeat(255),
            "description": "D".repeat(255)
        })
    );

    let mut restored = proxy
        .process_request(request_with_body("POST", "/__meta/dump", "{}"))
        .body;
    let definitions = restored["state"]["stagedState"]["metafieldDefinitions"]
        .as_object_mut()
        .expect("staged metafield definitions");
    let restored_definition = definitions
        .values_mut()
        .find(|definition| {
            definition["namespace"].as_str() == Some(namespace)
                && definition["key"].as_str() == Some(key)
                && definition["ownerType"].as_str() == Some("PRODUCT")
        })
        .expect("created definition in staged state");
    restored_definition["name"] = json!("L".repeat(256));
    let restore = proxy.process_request(request_with_body(
        "POST",
        "/__meta/restore",
        &restored.to_string(),
    ));
    assert_eq!(restore.status, 200);

    let untouched_over_length_name = update(
        &mut proxy,
        json!({
            "ownerType": "PRODUCT",
            "namespace": namespace,
            "key": key,
            "description": "Only description touched"
        }),
    );
    assert_eq!(untouched_over_length_name["userErrors"], json!([]));
    assert_eq!(
        untouched_over_length_name["updatedDefinition"],
        json!({
            "namespace": namespace,
            "key": key,
            "name": "L".repeat(256),
            "description": "Only description touched"
        })
    );
}

#[test]
fn metafield_definition_create_rejects_type_unsupported_validation_options() {
    let mut proxy = snapshot_proxy();
    let create = |proxy: &mut DraftProxy, definition: Value| {
        proxy
            .process_request(json_graphql_request(
                r#"
        mutation MetafieldDefinitionCreateValidationOptions($definition: MetafieldDefinitionInput!) {
          metafieldDefinitionCreate(definition: $definition) {
            createdDefinition {
              id
              namespace
              key
              type { name category }
              validations { name value }
            }
            userErrors { __typename field message code }
          }
        }
        "#,
                json!({ "definition": definition }),
            ))
            .body["data"]["metafieldDefinitionCreate"]
            .clone()
    };

    for (key, name, expected_message) in [
        (
            "not_real_option",
            "not_a_real_option",
            "Validations value for option not_a_real_option contains an invalid value: 'not_a_real_option' isn't supported for single_line_text_field.",
        ),
        (
            "pattern_option",
            "pattern",
            "Validations value for option pattern contains an invalid value: 'pattern' isn't supported for single_line_text_field.",
        ),
    ] {
        assert_eq!(
            create(
                &mut proxy,
                json!({
                    "namespace": "validation_options_create",
                    "key": key,
                    "ownerType": "PRODUCT",
                    "name": "Unsupported validation option",
                    "type": "single_line_text_field",
                    "validations": [{ "name": name, "value": "x" }]
                }),
            ),
            json!({
                "createdDefinition": null,
                "userErrors": [{
                    "__typename": "MetafieldDefinitionCreateUserError",
                    "field": ["definition", "validations"],
                    "message": expected_message,
                    "code": "INVALID_OPTION"
                }]
            })
        );
    }

    assert_eq!(
        create(
            &mut proxy,
            json!({
                "namespace": "validation_options_create",
                "key": "decimal_bad_min",
                "ownerType": "PRODUCT",
                "name": "Invalid decimal min",
                "type": "number_decimal",
                "validations": [{ "name": "min", "value": "not-a-number" }]
            }),
        ),
        json!({
            "createdDefinition": null,
            "userErrors": [{
                "__typename": "MetafieldDefinitionCreateUserError",
                "field": ["definition", "validations"],
                "message": "Validations value for option min must be a decimal.",
                "code": "INVALID_OPTION"
            }]
        })
    );

    let valid_decimal = create(
        &mut proxy,
        json!({
            "namespace": "validation_options_create",
            "key": "decimal_valid_range",
            "ownerType": "PRODUCT",
            "name": "Valid decimal range",
            "type": "number_decimal",
            "validations": [{ "name": "min", "value": "1.5" }, { "name": "max", "value": "9.9" }]
        }),
    );
    assert_eq!(valid_decimal["userErrors"], json!([]));
    assert_eq!(
        valid_decimal["createdDefinition"]["validations"],
        json!([{ "name": "min", "value": "1.5" }, { "name": "max", "value": "9.9" }])
    );
}

#[test]
fn metafield_definition_update_rejects_type_unsupported_validation_options() {
    let mut proxy = snapshot_proxy();
    let create = |proxy: &mut DraftProxy, definition: Value| {
        proxy
            .process_request(json_graphql_request(
                r#"
        mutation MetafieldDefinitionCreateUpdateTarget($definition: MetafieldDefinitionInput!) {
          metafieldDefinitionCreate(definition: $definition) {
            createdDefinition { id namespace key validations { name value } }
            userErrors { field message code }
          }
        }
        "#,
                json!({ "definition": definition }),
            ))
            .body["data"]["metafieldDefinitionCreate"]
            .clone()
    };
    let update = |proxy: &mut DraftProxy, definition: Value| {
        proxy
            .process_request(json_graphql_request(
                r#"
        mutation MetafieldDefinitionUpdateValidationOptions($definition: MetafieldDefinitionUpdateInput!) {
          metafieldDefinitionUpdate(definition: $definition) {
            updatedDefinition { id namespace key validations { name value } }
            userErrors { __typename field message code }
            validationJob { id }
          }
        }
        "#,
                json!({ "definition": definition }),
            ))
            .body["data"]["metafieldDefinitionUpdate"]
            .clone()
    };

    assert_eq!(
        create(
            &mut proxy,
            json!({
                "namespace": "validation_options_update",
                "key": "unsupported",
                "ownerType": "PRODUCT",
                "name": "Unsupported update target",
                "type": "single_line_text_field"
            }),
        )["userErrors"],
        json!([])
    );
    assert_eq!(
        update(
            &mut proxy,
            json!({
                "namespace": "validation_options_update",
                "key": "unsupported",
                "ownerType": "PRODUCT",
                "validations": [{ "name": "not_a_real_option", "value": "x" }]
            }),
        ),
        json!({
            "updatedDefinition": null,
            "userErrors": [{
                "__typename": "MetafieldDefinitionUpdateUserError",
                "field": ["definition", "validations"],
                "message": "Validations value for option not_a_real_option contains an invalid value: 'not_a_real_option' isn't supported for single_line_text_field.",
                "code": "INVALID_OPTION"
            }],
            "validationJob": null
        })
    );

    assert_eq!(
        create(
            &mut proxy,
            json!({
                "namespace": "validation_options_update",
                "key": "decimal",
                "ownerType": "PRODUCT",
                "name": "Decimal update target",
                "type": "number_decimal",
                "validations": [{ "name": "min", "value": "1.5" }, { "name": "max", "value": "9.9" }]
            }),
        )["userErrors"],
        json!([])
    );
    assert_eq!(
        update(
            &mut proxy,
            json!({
                "namespace": "validation_options_update",
                "key": "decimal",
                "ownerType": "PRODUCT",
                "validations": [{ "name": "min", "value": "not-a-number" }]
            }),
        ),
        json!({
            "updatedDefinition": null,
            "userErrors": [{
                "__typename": "MetafieldDefinitionUpdateUserError",
                "field": ["definition", "validations"],
                "message": "Validations value for option min must be a decimal.",
                "code": "INVALID_OPTION"
            }],
            "validationJob": null
        })
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
fn metafields_set_uses_matching_definition_type_when_input_type_is_omitted() {
    let mut proxy = snapshot_proxy();

    let product = proxy.process_request(json_graphql_request(
        r#"
        mutation MetafieldsSetDefinitionTypeProduct($product: ProductCreateInput!) {
          productCreate(product: $product) {
            product { id }
            userErrors { field message  }
          }
        }
        "#,
        json!({ "product": { "title": "Definition typed metafield owner" } }),
    ));
    assert_eq!(
        product.body["data"]["productCreate"]["userErrors"],
        json!([])
    );
    let owner_id = product.body["data"]["productCreate"]["product"]["id"]
        .as_str()
        .expect("productCreate should stage a product id")
        .to_string();

    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation MetafieldsSetDefinitionTypeCreate($definition: MetafieldDefinitionInput!) {
          metafieldDefinitionCreate(definition: $definition) {
            createdDefinition { id namespace key type { name } }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "definition": {
                "ownerType": "PRODUCT",
                "namespace": "custom",
                "key": "specs",
                "name": "Specs",
                "type": "multi_line_text_field"
            }
        }),
    ));
    assert_eq!(
        create.body["data"]["metafieldDefinitionCreate"]["userErrors"],
        json!([])
    );

    let set = proxy.process_request(json_graphql_request(
        r#"
        mutation MetafieldsSetDefinitionType($metafields: [MetafieldsSetInput!]!) {
          metafieldsSet(metafields: $metafields) {
            metafields { id namespace key type value }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "metafields": [{
                "ownerId": owner_id.clone(),
                "namespace": "custom",
                "key": "specs",
                "value": "hello world"
            }]
        }),
    ));
    assert_eq!(set.body["data"]["metafieldsSet"]["userErrors"], json!([]));
    assert_eq!(
        set.body["data"]["metafieldsSet"]["metafields"][0]["type"],
        json!("multi_line_text_field")
    );

    let read = proxy.process_request(json_graphql_request(
        r#"
        query MetafieldsSetDefinitionTypeRead($id: ID!) {
          product(id: $id) {
            metafield(namespace: "custom", key: "specs") {
              namespace
              key
              type
              value
            }
          }
        }
        "#,
        json!({ "id": owner_id }),
    ));
    assert_eq!(
        read.body["data"]["product"]["metafield"],
        json!({
            "namespace": "custom",
            "key": "specs",
            "type": "multi_line_text_field",
            "value": "hello world"
        })
    );
}

#[test]
fn metafields_set_without_type_still_rejects_when_no_definition_matches() {
    let mut proxy = snapshot_proxy();

    let set = proxy.process_request(json_graphql_request(
        r#"
        mutation MetafieldsSetMissingDefinitionType($metafields: [MetafieldsSetInput!]!) {
          metafieldsSet(metafields: $metafields) {
            metafields { id namespace key type value }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "metafields": [{
                "ownerId": "gid://shopify/Product/123",
                "namespace": "custom",
                "key": "specs",
                "value": "hello world"
            }]
        }),
    ));
    assert_eq!(set.body["data"]["metafieldsSet"]["metafields"], json!([]));
    assert_eq!(
        set.body["data"]["metafieldsSet"]["userErrors"],
        json!([{
            "field": ["metafields", "0", "type"],
            "message": "Type can't be blank",
            "code": "BLANK"
        }])
    );
}

#[test]
fn metafield_definition_delete_enforces_reference_guards_and_removes_associated_values() {
    let mut proxy = snapshot_proxy();
    let namespace = "delete_reference_guard";

    // Create a real product locally so the `product_reference` value resolves
    // against staged resource state. The seeded reference target this previously
    // relied on was removed with `/__meta/seed`; metafieldsSet validates reference
    // values against staged/base/hydrated resources, so the target must genuinely
    // exist rather than being injected.
    let create_target = proxy.process_request(json_graphql_request(
        r#"
        mutation DeleteReferenceGuardTarget($product: ProductCreateInput!) {
          productCreate(product: $product) {
            product { id }
            userErrors { field message }
          }
        }
        "#,
        json!({ "product": { "title": "Delete guard reference target" } }),
    ));
    assert_eq!(
        create_target.body["data"]["productCreate"]["userErrors"],
        json!([])
    );
    let owner_id = create_target.body["data"]["productCreate"]["product"]["id"]
        .as_str()
        .expect("productCreate should stage a product id")
        .to_string();

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
                "ownerId": owner_id.clone(),
                "namespace": namespace,
                "key": "target",
                "type": "product_reference",
                "value": owner_id.clone()
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
fn metafield_definition_delete_rejects_reserved_namespace_without_delete_all_flag() {
    let mut proxy = snapshot_proxy();

    let product = proxy.process_request(json_graphql_request(
        r#"
        mutation ReservedNamespaceGuardProductCreate {
          productCreate(product: { title: "Reserved namespace guard product" }) {
            product { id }
            userErrors { field message  }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        product.body["data"]["productCreate"]["userErrors"],
        json!([])
    );
    let owner_id = product.body["data"]["productCreate"]["product"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let app_request = |query: &str, variables: serde_json::Value| {
        let mut request = json_graphql_request(query, variables);
        request.headers.insert(
            "x-shopify-draft-proxy-api-client-id".to_string(),
            "347082227713".to_string(),
        );
        request
    };

    let create_definition = proxy.process_request(app_request(
        r#"
        mutation ReservedNamespaceDefinitionCreate($definition: MetafieldDefinitionInput!) {
          metafieldDefinitionCreate(definition: $definition) {
            createdDefinition { id namespace key ownerType type { name } }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "definition": {
                "ownerType": "PRODUCT",
                "namespace": "$app:settings",
                "key": "config",
                "name": "Reserved config",
                "type": "single_line_text_field"
            }
        }),
    ));
    assert_eq!(
        create_definition.body["data"]["metafieldDefinitionCreate"]["createdDefinition"]
            ["namespace"],
        json!("app--347082227713--settings")
    );
    assert_eq!(
        create_definition.body["data"]["metafieldDefinitionCreate"]["userErrors"],
        json!([])
    );

    let set = proxy.process_request(app_request(
        r#"
        mutation ReservedNamespaceAssociatedMetafieldSet($metafields: [MetafieldsSetInput!]!) {
          metafieldsSet(metafields: $metafields) {
            metafields { ownerType namespace key value }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "metafields": [{
                "ownerId": owner_id.clone(),
                "namespace": "$app:settings",
                "key": "config",
                "type": "single_line_text_field",
                "value": "enabled"
            }]
        }),
    ));
    assert_eq!(set.body["data"]["metafieldsSet"]["userErrors"], json!([]));

    let guarded_delete = proxy.process_request(app_request(
        r#"
        mutation ReservedNamespaceDefinitionDeleteNoFlag {
          metafieldDefinitionDelete(
            identifier: { ownerType: PRODUCT, namespace: "$app:settings", key: "config" }
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
        guarded_delete.body["data"]["metafieldDefinitionDelete"],
        json!({
            "deletedDefinitionId": null,
            "deletedDefinition": null,
            "userErrors": [{
                "__typename": "MetafieldDefinitionDeleteUserError",
                "field": null,
                "message": "Deleting a definition in a reserved namespace must have deleteAllAssociatedMetafields set to true.",
                "code": "RESERVED_NAMESPACE_ORPHANED_METAFIELDS"
            }]
        })
    );

    let read_after_guard = proxy.process_request(app_request(
        r#"
        query ReservedNamespaceDefinitionReadAfterGuard {
          metafieldDefinition(identifier: { ownerType: PRODUCT, namespace: "$app:settings", key: "config" }) {
            namespace
            key
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        read_after_guard.body["data"]["metafieldDefinition"],
        json!({
            "namespace": "app--347082227713--settings",
            "key": "config"
        })
    );

    let delete_all = proxy.process_request(app_request(
        r#"
        mutation ReservedNamespaceDefinitionDeleteAll {
          metafieldDefinitionDelete(
            identifier: { ownerType: PRODUCT, namespace: "$app:settings", key: "config" }
            deleteAllAssociatedMetafields: true
          ) {
            deletedDefinition { ownerType namespace key }
            userErrors { field message code }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        delete_all.body["data"]["metafieldDefinitionDelete"]["deletedDefinition"],
        json!({
            "ownerType": "PRODUCT",
            "namespace": "app--347082227713--settings",
            "key": "config"
        })
    );
    assert_eq!(
        delete_all.body["data"]["metafieldDefinitionDelete"]["userErrors"],
        json!([])
    );

    let read_metafield_after_delete_all = proxy.process_request(app_request(
        r#"
        query ReservedNamespaceMetafieldReadAfterDeleteAll($id: ID!) {
          product(id: $id) {
            metafield(namespace: "$app:settings", key: "config") { namespace key value }
          }
        }
        "#,
        json!({ "id": owner_id }),
    ));
    assert_eq!(
        read_metafield_after_delete_all.body["data"]["product"]["metafield"],
        Value::Null
    );
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
fn metafields_set_and_owner_reads_project_matching_definition() {
    let mut proxy = snapshot_proxy();
    let owner_id = "gid://shopify/Product/10173064872243";

    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation MetafieldDefinitionAssociationCreate($definition: MetafieldDefinitionInput!) {
          metafieldDefinitionCreate(definition: $definition) {
            createdDefinition { id namespace key ownerType type { name } }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "definition": {
                "ownerType": "PRODUCT",
                "namespace": "custom",
                "key": "specs",
                "name": "Specs",
                "type": "multi_line_text_field"
            }
        }),
    ));
    assert_eq!(
        create.body["data"]["metafieldDefinitionCreate"]["userErrors"],
        json!([])
    );
    let created_definition = &create.body["data"]["metafieldDefinitionCreate"]["createdDefinition"];

    let set = proxy.process_request(json_graphql_request(
        r#"
        mutation MetafieldDefinitionAssociationSet($metafields: [MetafieldsSetInput!]!) {
          metafieldsSet(metafields: $metafields) {
            metafields {
              namespace
              key
              definition { id namespace key type { name } }
            }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "metafields": [
                {
                    "ownerId": owner_id,
                    "namespace": "custom",
                    "key": "specs",
                    "type": "multi_line_text_field",
                    "value": "hi"
                },
                {
                    "ownerId": owner_id,
                    "namespace": "unscoped",
                    "key": "note",
                    "type": "single_line_text_field",
                    "value": "loose"
                }
            ]
        }),
    ));
    assert_eq!(set.body["data"]["metafieldsSet"]["userErrors"], json!([]));
    assert_eq!(
        set.body["data"]["metafieldsSet"]["metafields"],
        json!([
            {
                "namespace": "custom",
                "key": "specs",
                "definition": {
                    "id": created_definition["id"].clone(),
                    "namespace": "custom",
                    "key": "specs",
                    "type": { "name": "multi_line_text_field" }
                }
            },
            {
                "namespace": "unscoped",
                "key": "note",
                "definition": null
            }
        ])
    );

    let read = proxy.process_request(json_graphql_request(
        r#"
        query MetafieldDefinitionAssociationRead($id: ID!) {
          product(id: $id) {
            defined: metafield(namespace: "custom", key: "specs") {
              namespace
              key
              definition { id namespace key type { name } }
            }
            undefined: metafield(namespace: "unscoped", key: "note") {
              namespace
              key
              definition { id }
            }
          }
        }
        "#,
        json!({ "id": owner_id }),
    ));
    assert_eq!(
        read.body["data"]["product"]["defined"],
        json!({
            "namespace": "custom",
            "key": "specs",
            "definition": {
                "id": created_definition["id"].clone(),
                "namespace": "custom",
                "key": "specs",
                "type": { "name": "multi_line_text_field" }
            }
        })
    );
    assert_eq!(
        read.body["data"]["product"]["undefined"],
        json!({
            "namespace": "unscoped",
            "key": "note",
            "definition": null
        })
    );
}

#[test]
fn metafield_definition_validation_rules_gate_metafields_set_values() {
    let mut proxy = snapshot_proxy();
    let owner_id = "gid://shopify/Product/10173064872244";
    let namespace = "definition_rule_matrix";
    let create_query = r#"
        mutation DefinitionRuleMatrixCreate($definition: MetafieldDefinitionInput!) {
          metafieldDefinitionCreate(definition: $definition) {
            createdDefinition { namespace key ownerType type { name } validations { name value } }
            userErrors { field message code }
          }
        }
        "#;

    let definitions = vec![
        json!({"namespace": namespace, "key": "quantity_min", "ownerType": "PRODUCT", "name": "Quantity min", "type": "number_integer", "validations": [{"name": "min", "value": "2"}]}),
        json!({"namespace": namespace, "key": "quantity_max", "ownerType": "PRODUCT", "name": "Quantity max", "type": "number_integer", "validations": [{"name": "max", "value": "5"}]}),
        json!({"namespace": namespace, "key": "sku", "ownerType": "PRODUCT", "name": "SKU", "type": "single_line_text_field", "validations": [{"name": "regex", "value": "^[A-Z]{3}$"}]}),
        json!({"namespace": namespace, "key": "color", "ownerType": "PRODUCT", "name": "Color", "type": "single_line_text_field", "validations": [{"name": "choices", "value": "[\"red\",\"blue\"]"}]}),
        json!({"namespace": namespace, "key": "rating", "ownerType": "PRODUCT", "name": "Rating", "type": "rating", "validations": [{"name": "scale_min", "value": "1.0"}, {"name": "scale_max", "value": "5.0"}]}),
        json!({"namespace": namespace, "key": "launch_date", "ownerType": "PRODUCT", "name": "Launch date", "type": "date", "validations": [{"name": "min", "value": "2026-01-01"}, {"name": "max", "value": "2026-12-31"}]}),
        json!({"namespace": namespace, "key": "starts_at", "ownerType": "PRODUCT", "name": "Starts at", "type": "date_time", "validations": [{"name": "min", "value": "2026-01-01T00:00:00+00:00"}, {"name": "max", "value": "2026-12-31T23:59:59+00:00"}]}),
    ];

    for definition in definitions {
        let created = proxy.process_request(json_graphql_request(
            create_query,
            json!({"definition": definition}),
        ));
        assert_eq!(
            created.body["data"]["metafieldDefinitionCreate"]["userErrors"],
            json!([])
        );
    }

    let set_query = r#"
        mutation DefinitionRuleMatrixSet($metafields: [MetafieldsSetInput!]!) {
          metafieldsSet(metafields: $metafields) {
            metafields { namespace key type value }
            userErrors { field message code elementIndex }
          }
        }
        "#;
    let rejected = proxy.process_request(json_graphql_request(
        set_query,
        json!({"metafields": [
            {"ownerId": owner_id, "namespace": namespace, "key": "quantity_min", "type": "number_integer", "value": "1"},
            {"ownerId": owner_id, "namespace": namespace, "key": "quantity_max", "type": "number_integer", "value": "6"},
            {"ownerId": owner_id, "namespace": namespace, "key": "sku", "type": "single_line_text_field", "value": "abc"},
            {"ownerId": owner_id, "namespace": namespace, "key": "color", "type": "single_line_text_field", "value": "green"},
            {"ownerId": owner_id, "namespace": namespace, "key": "rating", "type": "rating", "value": json!({"value": "6.0", "scale_min": "1.0", "scale_max": "10.0"}).to_string()},
            {"ownerId": owner_id, "namespace": namespace, "key": "launch_date", "type": "date", "value": "2027-01-01"},
            {"ownerId": owner_id, "namespace": namespace, "key": "starts_at", "type": "date_time", "value": "2027-01-01T00:00:00+00:00"}
        ]}),
    ));
    assert_eq!(
        rejected.body["data"]["metafieldsSet"]["metafields"],
        json!([])
    );
    let errors = rejected.body["data"]["metafieldsSet"]["userErrors"]
        .as_array()
        .unwrap();
    assert_eq!(errors.len(), 7);
    let expected_messages = [
        "Value has a minimum of 2.",
        "Value has a maximum of 5.",
        "Value does not match the required pattern.",
        "Value must be one of the allowed choices.",
        "Value has a maximum of 5.0.",
        "Value has a maximum date of 2026-12-31.",
        "Value has a maximum date-time of 2026-12-31T23:59:59+00:00.",
    ];
    for (index, error) in errors.iter().enumerate() {
        assert_eq!(
            error["field"],
            json!(["metafields", index.to_string(), "value"])
        );
        assert_eq!(error["code"], json!("INVALID_VALUE"));
        assert_eq!(error["message"], json!(expected_messages[index]));
        assert_eq!(error["elementIndex"], Value::Null);
    }

    let accepted = proxy.process_request(json_graphql_request(
        set_query,
        json!({"metafields": [
            {"ownerId": owner_id, "namespace": namespace, "key": "quantity_min", "type": "number_integer", "value": "2"},
            {"ownerId": owner_id, "namespace": namespace, "key": "quantity_max", "type": "number_integer", "value": "5"},
            {"ownerId": owner_id, "namespace": namespace, "key": "sku", "type": "single_line_text_field", "value": "ABC"},
            {"ownerId": owner_id, "namespace": namespace, "key": "color", "type": "single_line_text_field", "value": "red"},
            {"ownerId": owner_id, "namespace": namespace, "key": "rating", "type": "rating", "value": json!({"value": "4.0", "scale_min": "1.0", "scale_max": "10.0"}).to_string()},
            {"ownerId": owner_id, "namespace": namespace, "key": "launch_date", "type": "date", "value": "2026-06-25"},
            {"ownerId": owner_id, "namespace": namespace, "key": "starts_at", "type": "date_time", "value": "2026-06-25T10:11:12Z"}
        ]}),
    ));
    assert_eq!(
        accepted.body["data"]["metafieldsSet"]["userErrors"],
        json!([])
    );
    assert_eq!(
        accepted.body["data"]["metafieldsSet"]["metafields"]
            .as_array()
            .unwrap()
            .len(),
        7
    );
}

#[test]
fn metafield_definition_metaobject_reference_validation_checks_target_definition() {
    let mut proxy = snapshot_proxy();
    let title_field = json!({"key": "title", "name": "Title", "type": "single_line_text_field", "required": false});
    let create_definition_query = r#"
        mutation CreateMetaobjectDefinitionForMetafieldReference($definition: MetaobjectDefinitionCreateInput!) {
          metaobjectDefinitionCreate(definition: $definition) {
            metaobjectDefinition { id type }
            userErrors { field message code elementKey elementIndex }
          }
        }
        "#;
    let create_metaobject_definition = |proxy: &mut DraftProxy, meta_type: &str| -> String {
        let response = proxy.process_request(json_graphql_request(
            create_definition_query,
            json!({"definition": {
                "type": meta_type,
                "name": meta_type,
                "displayNameKey": "title",
                "fieldDefinitions": [title_field.clone()]
            }}),
        ));
        assert_eq!(
            response.body["data"]["metaobjectDefinitionCreate"]["userErrors"],
            json!([])
        );
        response.body["data"]["metaobjectDefinitionCreate"]["metaobjectDefinition"]["id"]
            .as_str()
            .unwrap()
            .to_string()
    };
    let allowed_definition_id = create_metaobject_definition(&mut proxy, "allowed_reference_type");
    create_metaobject_definition(&mut proxy, "disallowed_reference_type");

    let create_metaobject_query = r#"
        mutation CreateMetaobjectForMetafieldReference($metaobject: MetaobjectCreateInput!) {
          metaobjectCreate(metaobject: $metaobject) {
            metaobject { id type }
            userErrors { field message code elementKey elementIndex }
          }
        }
        "#;
    let create_metaobject = |proxy: &mut DraftProxy, meta_type: &str, handle: &str| -> String {
        let response = proxy.process_request(json_graphql_request(
            create_metaobject_query,
            json!({"metaobject": {
                "type": meta_type,
                "handle": handle,
                "fields": [{"key": "title", "value": handle}]
            }}),
        ));
        assert_eq!(
            response.body["data"]["metaobjectCreate"]["userErrors"],
            json!([])
        );
        response.body["data"]["metaobjectCreate"]["metaobject"]["id"]
            .as_str()
            .unwrap()
            .to_string()
    };
    let allowed_metaobject_id =
        create_metaobject(&mut proxy, "allowed_reference_type", "allowed-reference");
    let disallowed_metaobject_id = create_metaobject(
        &mut proxy,
        "disallowed_reference_type",
        "disallowed-reference",
    );

    let namespace = "reference_definition_rule";
    let definition = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateMetaobjectReferenceMetafieldDefinition($definition: MetafieldDefinitionInput!) {
          metafieldDefinitionCreate(definition: $definition) {
            createdDefinition { namespace key ownerType type { name } validations { name value } }
            userErrors { field message code }
          }
        }
        "#,
        json!({"definition": {
            "namespace": namespace,
            "key": "linked",
            "ownerType": "PRODUCT",
            "name": "Linked metaobject",
            "type": "metaobject_reference",
            "validations": [{"name": "metaobject_definition_id", "value": allowed_definition_id}]
        }}),
    ));
    assert_eq!(
        definition.body["data"]["metafieldDefinitionCreate"]["userErrors"],
        json!([])
    );

    let set_query = r#"
        mutation SetMetaobjectReferenceMetafield($metafields: [MetafieldsSetInput!]!) {
          metafieldsSet(metafields: $metafields) {
            metafields { namespace key type value }
            userErrors { field message code elementIndex }
          }
        }
        "#;
    let rejected = proxy.process_request(json_graphql_request(
        set_query,
        json!({"metafields": [{
            "ownerId": "gid://shopify/Product/10173064872245",
            "namespace": namespace,
            "key": "linked",
            "type": "metaobject_reference",
            "value": disallowed_metaobject_id
        }]}),
    ));
    assert_eq!(
        rejected.body["data"]["metafieldsSet"],
        json!({
            "metafields": [],
            "userErrors": [{
                "field": ["metafields", "0", "value"],
                "message": "Value must belong to the configured metaobject definition.",
                "code": "INVALID_VALUE",
                "elementIndex": null
            }]
        })
    );

    let accepted = proxy.process_request(json_graphql_request(
        set_query,
        json!({"metafields": [{
            "ownerId": "gid://shopify/Product/10173064872245",
            "namespace": namespace,
            "key": "linked",
            "type": "metaobject_reference",
            "value": allowed_metaobject_id
        }]}),
    ));
    assert_eq!(
        accepted.body["data"]["metafieldsSet"]["userErrors"],
        json!([])
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

#[test]
fn standard_metafield_definition_enable_supports_shopify_material_template() {
    let mut proxy = snapshot_proxy();

    let enabled = proxy.process_request(json_graphql_request(
        r#"
        mutation StandardMetafieldDefinitionEnableMaterial {
          standardMetafieldDefinitionEnable(ownerType: PRODUCT, namespace: "shopify", key: "material") {
            createdDefinition {
              namespace
              key
              ownerType
              name
              type { name category }
              validations { name value }
              constraints {
                key
                values(first: 5) { nodes { value } }
              }
            }
            userErrors { __typename field message code }
          }
        }
        "#,
        json!({}),
    ));

    assert_eq!(
        enabled.body["data"]["standardMetafieldDefinitionEnable"]["userErrors"],
        json!([])
    );
    assert_eq!(
        enabled.body["data"]["standardMetafieldDefinitionEnable"]["createdDefinition"]["namespace"],
        json!("shopify")
    );
    assert_eq!(
        enabled.body["data"]["standardMetafieldDefinitionEnable"]["createdDefinition"],
        json!({
            "namespace": "shopify",
            "key": "material",
            "ownerType": "PRODUCT",
            "name": "Material",
            "type": { "name": "list.metaobject_reference", "category": "REFERENCE" },
            "validations": [{
                "name": "metaobject_definition_id",
                "value": "gid://shopify/MetaobjectDefinition/standard-material?shopify-draft-proxy=synthetic"
            }],
            "constraints": {
                "key": "category",
                "values": {
                    "nodes": [
                        { "value": "aa-2" },
                        { "value": "aa-2-14-6" },
                        { "value": "aa-2-14-6-2" },
                        { "value": "aa-2-14-6-3" },
                        { "value": "aa-2-14-6-4" }
                    ]
                }
            }
        })
    );
}

#[test]
fn standard_metafield_definition_enable_accepts_catalog_template_id_for_fabric() {
    let mut proxy = snapshot_proxy();

    let enabled = proxy.process_request(json_graphql_request(
        r#"
        mutation StandardMetafieldDefinitionEnableFabricById {
          standardMetafieldDefinitionEnable(
            ownerType: PRODUCT
            id: "gid://shopify/StandardMetafieldDefinitionTemplate/12777"
          ) {
            createdDefinition {
              id
              namespace
              key
              name
              type { name category }
              validations { name value }
              constraints { key values(first: 5) { nodes { value } } }
            }
            userErrors { field message code }
          }
        }
        "#,
        json!({}),
    ));

    assert_eq!(
        enabled.body["data"]["standardMetafieldDefinitionEnable"]["userErrors"],
        json!([])
    );
    assert_eq!(
        enabled.body["data"]["standardMetafieldDefinitionEnable"]["createdDefinition"]["namespace"],
        json!("shopify")
    );
    assert_eq!(
        enabled.body["data"]["standardMetafieldDefinitionEnable"]["createdDefinition"]["key"],
        json!("fabric")
    );
    assert_eq!(
        enabled.body["data"]["standardMetafieldDefinitionEnable"]["createdDefinition"]["name"],
        json!("Fabric")
    );
    assert_eq!(
        enabled.body["data"]["standardMetafieldDefinitionEnable"]["createdDefinition"]["type"],
        json!({ "name": "list.metaobject_reference", "category": "REFERENCE" })
    );
    assert_eq!(
        enabled.body["data"]["standardMetafieldDefinitionEnable"]["createdDefinition"]
            ["validations"],
        json!([{
            "name": "metaobject_definition_id",
            "value": "gid://shopify/MetaobjectDefinition/standard-fabric?shopify-draft-proxy=synthetic"
        }])
    );
    assert_eq!(
        enabled.body["data"]["standardMetafieldDefinitionEnable"]["createdDefinition"]
            ["constraints"],
        json!({ "key": "category", "values": { "nodes": [] } })
    );
}

#[test]
fn metafield_definition_validation_names_are_checked_against_type() {
    let mut proxy = snapshot_proxy();
    let create_definition = r#"
        mutation CreateInvalidValidationDefinition($definition: MetafieldDefinitionInput!) {
          metafieldDefinitionCreate(definition: $definition) {
            createdDefinition { id }
            userErrors { field message code }
          }
        }
        "#;

    for (key, metafield_type, validation_name) in [
        ("text_scale_max", "single_line_text_field", "scale_max"),
        ("boolean_max", "boolean", "max"),
    ] {
        let response = proxy.process_request(json_graphql_request(
            create_definition,
            json!({"definition": {
                "ownerType": "PRODUCT",
                "namespace": "validation_name_matrix",
                "key": key,
                "name": key,
                "type": metafield_type,
                "validations": [{"name": validation_name, "value": "5"}]
            }}),
        ));
        assert_eq!(response.status, 200, "{key}");
        assert_eq!(
            response.body["data"]["metafieldDefinitionCreate"]["createdDefinition"],
            Value::Null,
            "{key}"
        );
        assert_eq!(
            response.body["data"]["metafieldDefinitionCreate"]["userErrors"],
            json!([{
                "field": ["definition", "validations"],
                "message": format!(
                    "Validations value for option {validation_name} contains an invalid value: '{validation_name}' isn't supported for {metafield_type}."
                ),
                "code": "INVALID_OPTION"
            }]),
            "{key}"
        );
    }
}

#[test]
fn metafield_definition_access_grants_validation_uses_parsed_input_not_raw_query_text() {
    let mut proxy = snapshot_proxy();

    let valid_with_sniff_text = proxy.process_request(json_graphql_request(
        r#"
        mutation ValidMetafieldDefinitionNameMentionsGrants {
          metafieldDefinitionCreate(
            definition: {
              ownerType: PRODUCT
              namespace: "grant_text"
              key: "valid"
              name: "access: { grants:"
              type: "single_line_text_field"
            }
          ) {
            createdDefinition { namespace key name }
            userErrors { field message code }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        valid_with_sniff_text.body["data"]["metafieldDefinitionCreate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        valid_with_sniff_text.body["data"]["metafieldDefinitionCreate"]["createdDefinition"],
        json!({
            "namespace": "grant_text",
            "key": "valid",
            "name": "access: { grants:"
        })
    );

    let compact_invalid = proxy.process_request(json_graphql_request(
        r#"mutation CompactGrantsLocation { metafieldDefinitionCreate(definition: { ownerType: PRODUCT, namespace: "grant_text", key: "compact", name: "Compact", type: "single_line_text_field", access:{grants:[{grantee:"gid://shopify/App/1",access:READ_WRITE}]} }) { createdDefinition { id } userErrors { field message code } } }"#,
        json!({}),
    ));
    assert_eq!(
        compact_invalid.body["errors"],
        json!([{
            "message": "InputObject 'MetafieldAccessInput' doesn't accept argument 'grants'",
            "locations": [{ "line": 1, "column": 192 }],
            "path": ["mutation CompactGrantsLocation", "metafieldDefinitionCreate", "definition", "access", "grants"],
            "extensions": {
                "code": "argumentNotAccepted",
                "name": "MetafieldAccessInput",
                "typeName": "InputObject",
                "argumentName": "grants"
            }
        }])
    );

    let variable_invalid = proxy.process_request(json_graphql_request(
        r#"
        mutation VariableGrantsValidation($definition: MetafieldDefinitionInput!) {
          metafieldDefinitionCreate(definition: $definition) {
            createdDefinition { id }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "definition": {
                "ownerType": "PRODUCT",
                "namespace": "grant_text",
                "key": "variable",
                "name": "Variable",
                "type": "single_line_text_field",
                "access": {
                    "grants": [{
                        "grantee": "gid://shopify/App/1",
                        "access": "READ_WRITE"
                    }]
                }
            }
        }),
    ));
    assert_eq!(
        variable_invalid.body["errors"][0]["extensions"]["code"],
        json!("INVALID_VARIABLE")
    );
    assert_eq!(
        variable_invalid.body["errors"][0]["extensions"]["problems"],
        json!([{
            "path": ["access", "grants"],
            "explanation": "Field is not defined on MetafieldAccessInput"
        }])
    );
    assert!(variable_invalid.body.get("data").is_none());
}

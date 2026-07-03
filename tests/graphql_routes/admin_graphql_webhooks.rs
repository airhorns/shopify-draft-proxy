use super::common::*;
use pretty_assertions::assert_eq;

#[test]
fn ported_gleam_event_empty_read_shapes_match_draft_proxy_tests() {
    let mut proxy = snapshot_proxy();
    let response = proxy.process_request(json_graphql_request(
        r#"
        query EventEmptyRead($eventId: ID!, $first: Int!, $query: String!) {
          myEvent: event(id: "gid://shopify/Event/1") { id }
          event(id: $eventId) { id action message }
          events(first: $first, query: $query, sortKey: ID, reverse: true) {
            nodes { id action message }
            edges { cursor node { id action message } }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
          nodeOnlyEvents: events(first: 5) { nodes { id } }
          eventsCount(query: $query) { count precision }
          looseCount: eventsCount { count whatever }
        }
        "#,
        json!({
            "eventId": "gid://shopify/BasicEvent/999999999999",
            "first": 2,
            "query": "id:999999999999"
        }),
    ));

    assert_eq!(response.status, 200);
    assert_eq!(
        response.body,
        json!({
            "data": {
                "myEvent": null,
                "event": null,
                "events": {
                    "nodes": [],
                    "edges": [],
                    "pageInfo": {
                        "hasNextPage": false,
                        "hasPreviousPage": false,
                        "startCursor": null,
                        "endCursor": null
                    }
                },
                "nodeOnlyEvents": {
                    "nodes": []
                },
                "eventsCount": {
                    "count": 0,
                    "precision": "EXACT"
                },
                "looseCount": {
                    "count": 0,
                    "whatever": null
                }
            }
        })
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
fn admin_graphql_reports_base_validation_errors_before_dispatch() {
    let mut proxy = snapshot_proxy();

    for version in ["2025-01", "2026-04"] {
        let path = format!("/admin/api/{version}/graphql.json");
        let parse_error =
            proxy.process_request(request_with_body("POST", &path, r#"{"query":""}"#));
        assert_eq!(parse_error.status, 200, "{version} parse error status");
        assert_eq!(
            parse_error.body,
            json!({
                "errors": [{
                    "message": "syntax error, unexpected end of file at [1, 1]",
                    "locations": [{ "line": 1, "column": 1 }],
                    "extensions": { "code": "PARSE_ERROR" }
                }]
            }),
            "{version} parse error body"
        );

        let missing_variable = proxy.process_request(request_with_body(
            "POST",
            &path,
            r#"{"query":"query Named($id: ID!) { product(id: $id) { id } }","variables":{}}"#,
        ));
        assert_eq!(
            missing_variable.status, 200,
            "{version} missing variable status"
        );
        assert_eq!(
            missing_variable.body["errors"][0]["message"],
            json!("Variable $id of type ID! was provided invalid value"),
            "{version} missing variable message"
        );
        assert_eq!(
            missing_variable.body["errors"][0]["extensions"]["code"],
            json!("INVALID_VARIABLE"),
            "{version} missing variable code"
        );

        let unknown_query = proxy.process_request(request_with_body(
            "POST",
            &path,
            r#"{"query":"query Named { definitelyUnknownRoot { id } }"}"#,
        ));
        assert_eq!(unknown_query.status, 200, "{version} unknown query status");
        assert_eq!(
            unknown_query.body,
            json!({
                "errors": [{
                    "message": "Field 'definitelyUnknownRoot' doesn't exist on type 'QueryRoot'",
                    "locations": [{ "line": 1, "column": 15 }],
                    "path": ["query Named", "definitelyUnknownRoot"],
                    "extensions": {
                        "code": "undefinedField",
                        "typeName": "QueryRoot",
                        "fieldName": "definitelyUnknownRoot"
                    }
                }]
            }),
            "{version} unknown query body"
        );

        let selection_mismatch = proxy.process_request(request_with_body(
            "POST",
            &path,
            r#"{"query":"query Named { shop }"}"#,
        ));
        assert_eq!(
            selection_mismatch.status, 200,
            "{version} selection mismatch status"
        );
        assert_eq!(
            selection_mismatch.body["errors"][0]["extensions"]["code"],
            json!("selectionMismatch"),
            "{version} selection mismatch code"
        );

        let unknown_mutation = proxy.process_request(request_with_body(
            "POST",
            &path,
            r#"{"query":"mutation { definitelyUnknownMutation { ok } }"}"#,
        ));
        assert_eq!(
            unknown_mutation.status, 200,
            "{version} unknown mutation status"
        );
        assert_eq!(
            unknown_mutation.body,
            json!({
                "errors": [{
                    "message": "Field 'definitelyUnknownMutation' doesn't exist on type 'Mutation'",
                    "locations": [{ "line": 1, "column": 12 }],
                    "path": ["mutation", "definitelyUnknownMutation"],
                    "extensions": {
                        "code": "undefinedField",
                        "typeName": "Mutation",
                        "fieldName": "definitelyUnknownMutation"
                    }
                }]
            }),
            "{version} unknown mutation body"
        );

        let product_create_arity = proxy.process_request(request_with_body(
            "POST",
            &path,
            r#"{"query":"mutation { productCreate { product { id } userErrors { message } } }"}"#,
        ));
        assert_eq!(
            product_create_arity.status, 200,
            "{version} productCreate arity status"
        );
        assert_eq!(
            product_create_arity.body["data"],
            json!({ "productCreate": null }),
            "{version} productCreate arity data"
        );
        assert_eq!(
            product_create_arity.body["errors"][0]["extensions"]["code"],
            json!("INVALID_FIELD_ARGUMENTS"),
            "{version} productCreate arity code"
        );
    }
}

#[test]
fn admin_graphql_rejects_unknown_api_versions() {
    let mut proxy = snapshot_proxy();

    let unknown_version = proxy.process_request(request_with_body(
        "POST",
        "/admin/api/banana/graphql.json",
        &json!({ "query": "{ shop { id } }" }).to_string(),
    ));

    assert_eq!(unknown_version.status, 404);
    assert_eq!(
        unknown_version.body,
        json!({ "errors": [{ "message": "Not found" }] })
    );
}

#[test]
fn admin_graphql_routes_by_root_field_not_alias_or_fragment_definition() {
    let mut proxy = snapshot_proxy();

    let aliased_query = proxy.process_request(request_with_body(
        "POST",
        "/admin/api/2025-01/graphql.json",
        r#"{"query":"query Named { visibleAlias: definitelyUnknownRoot { id } }"}"#,
    ));
    assert_eq!(aliased_query.status, 200);
    assert_eq!(
        aliased_query.body["errors"][0]["message"],
        json!("Field 'definitelyUnknownRoot' doesn't exist on type 'QueryRoot'")
    );
    assert_eq!(
        aliased_query.body["errors"][0]["path"],
        json!(["query Named", "visibleAlias"])
    );
    assert_eq!(
        aliased_query.body["errors"][0]["extensions"],
        json!({
            "code": "undefinedField",
            "typeName": "QueryRoot",
            "fieldName": "definitelyUnknownRoot"
        })
    );

    let fragment_before_operation = proxy.process_request(request_with_body(
        "POST",
        "/admin/api/2025-01/graphql.json",
        r#"{"query":"fragment Fields on Product { id } query Named { definitelyUnknownRoot { ...Fields } }"}"#,
    ));
    assert_eq!(fragment_before_operation.status, 200);
    assert_eq!(
        fragment_before_operation.body["errors"][0]["message"],
        json!("Field 'definitelyUnknownRoot' doesn't exist on type 'QueryRoot'")
    );
    assert_eq!(
        fragment_before_operation.body["errors"][0]["path"],
        json!(["query Named", "definitelyUnknownRoot"])
    );
    assert_eq!(
        fragment_before_operation.body["errors"][0]["extensions"],
        json!({
            "code": "undefinedField",
            "typeName": "QueryRoot",
            "fieldName": "definitelyUnknownRoot"
        })
    );
}

#[test]
fn live_hybrid_forwards_unknown_queries_to_upstream_transport() {
    let forwarded = Arc::new(Mutex::new(Vec::<Request>::new()));
    let captured = Arc::clone(&forwarded);
    let mut proxy =
        configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport(move |request| {
            captured.lock().unwrap().push(request);
            shopify_draft_proxy::proxy::Response {
                status: 202,
                headers: [("x-test-upstream".to_string(), "domain-read".to_string())].into(),
                body: json!({ "data": { "shop": { "id": "gid://shopify/Shop/42" } } }),
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
        body: json!({ "query": "{ shop { id } }" }).to_string(),
    });

    assert_eq!(response.status, 202);
    assert_eq!(
        response.body,
        json!({ "data": { "shop": { "id": "gid://shopify/Shop/42" } } })
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
        json!({ "query": "{ shop { id } }" }).to_string()
    );
}

#[test]
fn unknown_mutation_passthrough_observability_and_reject_mode_are_preserved() {
    let unsupported_mutation =
        "mutation { urlRedirectCreate(urlRedirect: { path: \"/old\", target: \"/new\" }) { urlRedirect { id } userErrors { message } } }";
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
            body: json!({ "data": { "urlRedirectCreate": { "urlRedirect": { "id": "gid://shopify/UrlRedirect/1" }, "userErrors": [] } } }),
        }
    });

    let passthrough_response = passthrough.process_request(graphql_request(
        "POST",
        &json!({ "query": unsupported_mutation }).to_string(),
    ));

    assert_eq!(passthrough_response.status, 200);
    assert_eq!(
        passthrough_response.body,
        json!({ "data": { "urlRedirectCreate": { "urlRedirect": { "id": "gid://shopify/UrlRedirect/1" }, "userErrors": [] } } })
    );
    assert_eq!(*hits.lock().unwrap(), 1);
    assert_eq!(
        log_snapshot(&passthrough),
        json!({
            "entries": [{
                "id": "log-1",
                "operationName": "urlRedirectCreate",
                "status": "proxied",
                "path": "/admin/api/2026-04/graphql.json",
                "query": unsupported_mutation,
                "variables": {},
                "interpreted": {
                    "operationType": "mutation",
                    "rootFields": ["urlRedirectCreate"],
                    "primaryRootField": "urlRedirectCreate",
                    "capability": {
                        "operationName": "urlRedirectCreate",
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
        &json!({ "query": unsupported_mutation }).to_string(),
    ));

    assert_eq!(reject_response.status, 400);
    assert_eq!(
        reject_response.body,
        json!({ "errors": [{ "message": "Unsupported mutation rejected by configuration: urlRedirectCreate" }] })
    );
    assert_eq!(*reject_hits.lock().unwrap(), 0);
}

#[test]
fn webhook_subscription_create_update_delete_and_reads_stage_locally() {
    let mut proxy = snapshot_proxy();

    let create = proxy.process_request(json_graphql_request(
        "# RustWebhookLocalRuntime\nmutation { webhookSubscriptionCreate(topic: ORDERS_CREATE, webhookSubscription: { callbackUrl: \"https://hooks.example.com/orders\", format: JSON }) { webhookSubscription { id topic format callbackUrl endpoint { __typename ... on WebhookHttpEndpoint { callbackUrl } } } userErrors { field message } } }",
        json!({}),
    ));
    assert_eq!(create.status, 200);
    assert_eq!(
        create.body["data"]["webhookSubscriptionCreate"]["userErrors"],
        json!([])
    );
    let webhook_id = create.body["data"]["webhookSubscriptionCreate"]["webhookSubscription"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(
        create.body["data"]["webhookSubscriptionCreate"]["webhookSubscription"],
        json!({
            "id": webhook_id,
            "topic": "ORDERS_CREATE",
            "format": "JSON",
            "callbackUrl": "https://hooks.example.com/orders",
            "endpoint": {
                "__typename": "WebhookHttpEndpoint",
                "callbackUrl": "https://hooks.example.com/orders"
            }
        })
    );

    let read = proxy.process_request(json_graphql_request(
        "# RustWebhookLocalRuntime\nquery($id: ID!) { webhookSubscription(id: $id) { id topic callbackUrl } webhookSubscriptions(first: 10) { nodes { id topic callbackUrl } pageInfo { hasNextPage hasPreviousPage } } webhookSubscriptionsCount { count } }",
        json!({ "id": webhook_id }),
    ));
    assert_eq!(
        read.body["data"]["webhookSubscription"],
        json!({ "id": webhook_id, "topic": "ORDERS_CREATE", "callbackUrl": "https://hooks.example.com/orders" })
    );
    assert_eq!(
        read.body["data"]["webhookSubscriptions"]["nodes"],
        json!([{ "id": webhook_id, "topic": "ORDERS_CREATE", "callbackUrl": "https://hooks.example.com/orders" }])
    );
    assert_eq!(
        read.body["data"]["webhookSubscriptionsCount"],
        json!({ "count": 1 })
    );

    let update = proxy.process_request(json_graphql_request(
        "# RustWebhookLocalRuntime\nmutation($id: ID!) { webhookSubscriptionUpdate(id: $id, webhookSubscription: { callbackUrl: \"https://hooks.example.com/updated\", format: JSON }) { webhookSubscription { id callbackUrl endpoint { __typename ... on WebhookHttpEndpoint { callbackUrl } } } userErrors { field message } } }",
        json!({ "id": webhook_id }),
    ));
    assert_eq!(update.status, 200);
    assert_eq!(
        update.body["data"]["webhookSubscriptionUpdate"]["webhookSubscription"],
        json!({
            "id": webhook_id,
            "callbackUrl": "https://hooks.example.com/updated",
            "endpoint": {
                "__typename": "WebhookHttpEndpoint",
                "callbackUrl": "https://hooks.example.com/updated"
            }
        })
    );

    let delete = proxy.process_request(json_graphql_request(
        "# RustWebhookLocalRuntime\nmutation($id: ID!) { webhookSubscriptionDelete(id: $id) { deletedWebhookSubscriptionId userErrors { field message } } }",
        json!({ "id": webhook_id }),
    ));
    assert_eq!(
        delete.body["data"]["webhookSubscriptionDelete"],
        json!({ "deletedWebhookSubscriptionId": webhook_id, "userErrors": [] })
    );

    let read_after_delete = proxy.process_request(json_graphql_request(
        "# RustWebhookLocalRuntime\nquery($id: ID!) { webhookSubscription(id: $id) { id } webhookSubscriptions(first: 10) { nodes { id } } webhookSubscriptionsCount { count } }",
        json!({ "id": webhook_id }),
    ));
    assert_eq!(
        read_after_delete.body["data"]["webhookSubscription"],
        Value::Null
    );
    assert_eq!(
        read_after_delete.body["data"]["webhookSubscriptions"]["nodes"],
        json!([])
    );
    assert_eq!(
        read_after_delete.body["data"]["webhookSubscriptionsCount"],
        json!({ "count": 0 })
    );

    let log_roots: Vec<Value> = log_snapshot(&proxy)["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["interpreted"]["primaryRootField"].clone())
        .collect();
    assert_eq!(
        log_roots,
        vec![
            json!("webhookSubscriptionCreate"),
            json!("webhookSubscriptionUpdate"),
            json!("webhookSubscriptionDelete")
        ]
    );
}

#[test]
fn webhook_subscription_api_version_projects_and_survives_update() {
    let mut proxy = snapshot_proxy();
    let expected_api_version = json!({
        "handle": "2026-07",
        "displayName": "2026-07 (Release candidate)",
        "supported": false
    });

    let mut create_request = json_graphql_request(
        r#"# RustWebhookLocalRuntime
mutation {
  webhookSubscriptionCreate(
    topic: ORDERS_CREATE
    webhookSubscription: {
      callbackUrl: "https://hooks.example.com/orders-api-version"
      format: JSON
    }
  ) {
    webhookSubscription {
      id
      topic
      apiVersion { handle displayName supported }
    }
    userErrors { field message }
  }
}"#,
        json!({}),
    );
    create_request.headers.insert(
        "x-shopify-draft-proxy-api-version".to_string(),
        "2026-07".to_string(),
    );
    create_request.headers.insert(
        "x-shopify-draft-proxy-api-client-id".to_string(),
        "347082227713".to_string(),
    );

    let create = proxy.process_request(create_request);
    assert_eq!(create.status, 200);
    assert_eq!(
        create.body["data"]["webhookSubscriptionCreate"]["userErrors"],
        json!([])
    );
    let webhook_id = create.body["data"]["webhookSubscriptionCreate"]["webhookSubscription"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(
        create.body["data"]["webhookSubscriptionCreate"]["webhookSubscription"],
        json!({
            "id": webhook_id,
            "topic": "ORDERS_CREATE",
            "apiVersion": expected_api_version
        })
    );

    let read_after_create = proxy.process_request(json_graphql_request(
        r#"# RustWebhookLocalRuntime
query($id: ID!) {
  detail: webhookSubscription(id: $id) {
    id
    apiVersion { handle displayName supported }
  }
  webhookSubscriptions(first: 10) {
    nodes {
      id
      apiVersion { handle displayName supported }
    }
  }
}"#,
        json!({ "id": webhook_id }),
    ));
    assert_eq!(
        read_after_create.body["data"]["detail"],
        json!({ "id": webhook_id, "apiVersion": expected_api_version })
    );
    assert_eq!(
        read_after_create.body["data"]["webhookSubscriptions"]["nodes"],
        json!([{ "id": webhook_id, "apiVersion": expected_api_version }])
    );

    let mut update_request = json_graphql_request(
        r#"# RustWebhookLocalRuntime
mutation($id: ID!) {
  webhookSubscriptionUpdate(
    id: $id
    webhookSubscription: {
      callbackUrl: "https://hooks.example.com/orders-api-version-updated"
    }
  ) {
    webhookSubscription {
      id
      callbackUrl
      apiVersion { handle displayName supported }
    }
    userErrors { field message }
  }
}"#,
        json!({ "id": webhook_id }),
    );
    update_request.headers.insert(
        "x-shopify-draft-proxy-api-version".to_string(),
        "2026-04".to_string(),
    );

    let update = proxy.process_request(update_request);
    assert_eq!(
        update.body["data"]["webhookSubscriptionUpdate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        update.body["data"]["webhookSubscriptionUpdate"]["webhookSubscription"],
        json!({
            "id": webhook_id,
            "callbackUrl": "https://hooks.example.com/orders-api-version-updated",
            "apiVersion": expected_api_version
        })
    );

    let read_after_update = proxy.process_request(json_graphql_request(
        r#"# RustWebhookLocalRuntime
query($id: ID!) {
  webhookSubscription(id: $id) {
    id
    callbackUrl
    apiVersion { handle displayName supported }
  }
}"#,
        json!({ "id": webhook_id }),
    ));
    assert_eq!(
        read_after_update.body["data"]["webhookSubscription"],
        json!({
            "id": webhook_id,
            "callbackUrl": "https://hooks.example.com/orders-api-version-updated",
            "apiVersion": expected_api_version
        })
    );
}

#[test]
fn webhook_subscription_payload_fields_round_trip_through_create_update_and_reads() {
    let mut proxy = snapshot_proxy();

    let create = proxy.process_request(json_graphql_request(
        r#"# RustWebhookLocalRuntime
mutation {
  webhookSubscriptionCreate(
    topic: SHOP_UPDATE
    webhookSubscription: {
      uri: "https://hooks.example.com/payload-fields"
      format: JSON
      includeFields: ["id", "name"]
      metafieldNamespaces: ["custom"]
      metafields: [{ namespace: "custom", key: "color" }]
      filter: "customer_id:123"
    }
  ) {
    webhookSubscription {
      id
      topic
      format
      includeFields
      metafieldNamespaces
      metafields { namespace key }
      filter
      createdAt
      updatedAt
    }
    userErrors { field message }
  }
}"#,
        json!({}),
    ));
    assert_eq!(create.status, 200);
    assert_eq!(
        create.body["data"]["webhookSubscriptionCreate"]["userErrors"],
        json!([])
    );
    let webhook_id = create.body["data"]["webhookSubscriptionCreate"]["webhookSubscription"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let created_at = create.body["data"]["webhookSubscriptionCreate"]["webhookSubscription"]
        ["createdAt"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(created_at, "2024-01-01T00:00:01.000Z");
    assert_eq!(
        create.body["data"]["webhookSubscriptionCreate"]["webhookSubscription"],
        json!({
            "id": webhook_id,
            "topic": "SHOP_UPDATE",
            "format": "JSON",
            "includeFields": ["id", "name"],
            "metafieldNamespaces": ["custom"],
            "metafields": [{ "namespace": "custom", "key": "color" }],
            "filter": "customer_id:123",
            "createdAt": created_at,
            "updatedAt": created_at
        })
    );

    let read_after_create = proxy.process_request(json_graphql_request(
        r#"# RustWebhookLocalRuntime
query($id: ID!) {
  webhookSubscription(id: $id) {
    id
    includeFields
    metafieldNamespaces
    metafields { namespace key }
    filter
    createdAt
    updatedAt
  }
  webhookSubscriptions(first: 10) {
    nodes {
      id
      includeFields
      metafieldNamespaces
      metafields { namespace key }
      filter
      createdAt
      updatedAt
    }
  }
}"#,
        json!({ "id": webhook_id }),
    ));
    assert_eq!(
        read_after_create.body["data"]["webhookSubscription"],
        json!({
            "id": webhook_id,
            "includeFields": ["id", "name"],
            "metafieldNamespaces": ["custom"],
            "metafields": [{ "namespace": "custom", "key": "color" }],
            "filter": "customer_id:123",
            "createdAt": created_at,
            "updatedAt": created_at
        })
    );
    assert_eq!(
        read_after_create.body["data"]["webhookSubscriptions"]["nodes"],
        json!([{
            "id": webhook_id,
            "includeFields": ["id", "name"],
            "metafieldNamespaces": ["custom"],
            "metafields": [{ "namespace": "custom", "key": "color" }],
            "filter": "customer_id:123",
            "createdAt": created_at,
            "updatedAt": created_at
        }])
    );

    let update_omitted_fields = proxy.process_request(json_graphql_request(
        r#"# RustWebhookLocalRuntime
mutation($id: ID!) {
  webhookSubscriptionUpdate(
    id: $id
    webhookSubscription: { callbackUrl: "https://hooks.example.com/payload-fields-updated" }
  ) {
    webhookSubscription {
      id
      includeFields
      metafieldNamespaces
      metafields { namespace key }
      filter
      createdAt
      updatedAt
    }
    userErrors { field message }
  }
}"#,
        json!({ "id": webhook_id }),
    ));
    assert_eq!(
        update_omitted_fields.body["data"]["webhookSubscriptionUpdate"]["userErrors"],
        json!([])
    );
    let omitted_update_updated_at = update_omitted_fields.body["data"]["webhookSubscriptionUpdate"]
        ["webhookSubscription"]["updatedAt"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(omitted_update_updated_at, "2024-01-01T00:00:02.000Z");
    assert_eq!(
        update_omitted_fields.body["data"]["webhookSubscriptionUpdate"]["webhookSubscription"],
        json!({
            "id": webhook_id,
            "includeFields": ["id", "name"],
            "metafieldNamespaces": ["custom"],
            "metafields": [{ "namespace": "custom", "key": "color" }],
            "filter": "customer_id:123",
            "createdAt": created_at,
            "updatedAt": omitted_update_updated_at
        })
    );
    assert_ne!(created_at, omitted_update_updated_at);

    let update_empty_filter = proxy.process_request(json_graphql_request(
        r#"# RustWebhookLocalRuntime
mutation($id: ID!) {
  webhookSubscriptionUpdate(id: $id, webhookSubscription: { filter: "" }) {
    webhookSubscription {
      id
      includeFields
      metafieldNamespaces
      metafields { namespace key }
      filter
      createdAt
      updatedAt
    }
    userErrors { field message }
  }
}"#,
        json!({ "id": webhook_id }),
    ));
    let empty_filter_updated_at = update_empty_filter.body["data"]["webhookSubscriptionUpdate"]
        ["webhookSubscription"]["updatedAt"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(empty_filter_updated_at, "2024-01-01T00:00:03.000Z");
    assert_eq!(
        update_empty_filter.body["data"]["webhookSubscriptionUpdate"]["webhookSubscription"],
        json!({
            "id": webhook_id,
            "includeFields": ["id", "name"],
            "metafieldNamespaces": ["custom"],
            "metafields": [{ "namespace": "custom", "key": "color" }],
            "filter": "",
            "createdAt": created_at,
            "updatedAt": empty_filter_updated_at
        })
    );
    assert!(omitted_update_updated_at < empty_filter_updated_at);

    let create_defaults = proxy.process_request(json_graphql_request(
        r#"# RustWebhookLocalRuntime
mutation {
  webhookSubscriptionCreate(
    topic: ORDERS_CREATE
    webhookSubscription: { uri: "https://hooks.example.com/payload-field-defaults" }
  ) {
    webhookSubscription {
      id
      includeFields
      metafieldNamespaces
      metafields { namespace key }
      filter
      createdAt
      updatedAt
    }
    userErrors { field message }
  }
}"#,
        json!({}),
    ));
    let default_subscription =
        &create_defaults.body["data"]["webhookSubscriptionCreate"]["webhookSubscription"];
    let default_webhook_id = default_subscription["id"].as_str().unwrap().to_string();
    assert_eq!(default_subscription["includeFields"], json!([]));
    assert_eq!(default_subscription["metafieldNamespaces"], json!([]));
    assert_eq!(default_subscription["metafields"], json!([]));
    assert_eq!(default_subscription["filter"], Value::Null);
    assert!(default_subscription["createdAt"].as_str().is_some());
    assert_eq!(
        default_subscription["updatedAt"],
        default_subscription["createdAt"]
    );

    let read_defaults = proxy.process_request(json_graphql_request(
        r#"# RustWebhookLocalRuntime
query($id: ID!) {
  webhookSubscription(id: $id) {
    id
    metafields { namespace key }
  }
  webhookSubscriptions(first: 10, uri: "https://hooks.example.com/payload-field-defaults") {
    nodes {
      id
      metafields { namespace key }
    }
  }
}"#,
        json!({ "id": default_webhook_id }),
    ));
    assert_eq!(
        read_defaults.body["data"]["webhookSubscription"],
        json!({ "id": default_webhook_id, "metafields": [] })
    );
    assert_eq!(
        read_defaults.body["data"]["webhookSubscriptions"]["nodes"],
        json!([{ "id": default_webhook_id, "metafields": [] }])
    );
}

#[test]
fn dedicated_cloud_webhook_subscription_metafields_round_trip() {
    let mut proxy = snapshot_proxy();

    let pubsub_create = proxy.process_request(json_graphql_request(
        r#"# RustWebhookLocalRuntime
mutation {
  pubSubWebhookSubscriptionCreate(
    topic: SHOP_UPDATE
    webhookSubscription: {
      pubSubProject: "valid-project"
      pubSubTopic: "topic-1"
      metafields: [{ namespace: "custom", key: "tier" }]
    }
  ) {
    webhookSubscription {
      id
      metafields { namespace key }
    }
    userErrors { field message }
  }
}"#,
        json!({}),
    ));
    assert_eq!(
        pubsub_create.body["data"]["pubSubWebhookSubscriptionCreate"]["userErrors"],
        json!([])
    );
    let pubsub_id = pubsub_create.body["data"]["pubSubWebhookSubscriptionCreate"]
        ["webhookSubscription"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(
        pubsub_create.body["data"]["pubSubWebhookSubscriptionCreate"]["webhookSubscription"],
        json!({
            "id": pubsub_id,
            "metafields": [{ "namespace": "custom", "key": "tier" }]
        })
    );

    let pubsub_update = proxy.process_request(json_graphql_request(
        r#"# RustWebhookLocalRuntime
mutation($id: ID!) {
  pubSubWebhookSubscriptionUpdate(
    id: $id
    webhookSubscription: {
      pubSubProject: "valid-project"
      pubSubTopic: "topic-2"
      metafields: [{ namespace: "custom", key: "color" }]
    }
  ) {
    webhookSubscription {
      id
      metafields { namespace key }
    }
    userErrors { field message }
  }
}"#,
        json!({ "id": pubsub_id }),
    ));
    assert_eq!(
        pubsub_update.body["data"]["pubSubWebhookSubscriptionUpdate"]["webhookSubscription"],
        json!({
            "id": pubsub_id,
            "metafields": [{ "namespace": "custom", "key": "color" }]
        })
    );

    let eventbridge_create = proxy.process_request(json_graphql_request(
        r#"# RustWebhookLocalRuntime
mutation {
  eventBridgeWebhookSubscriptionCreate(
    topic: SHOP_UPDATE
    webhookSubscription: {
      arn: "arn:aws:events:us-east-1::event-source/aws.partner/shopify.com/347082227713/metafields-source"
      metafields: [{ namespace: "custom", key: "delivery" }]
    }
  ) {
    webhookSubscription {
      id
      metafields { namespace key }
    }
    userErrors { field message }
  }
}"#,
        json!({}),
    ));
    assert_eq!(
        eventbridge_create.body["data"]["eventBridgeWebhookSubscriptionCreate"]["userErrors"],
        json!([])
    );
    let eventbridge_id = eventbridge_create.body["data"]["eventBridgeWebhookSubscriptionCreate"]
        ["webhookSubscription"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(
        eventbridge_create.body["data"]["eventBridgeWebhookSubscriptionCreate"]
            ["webhookSubscription"],
        json!({
            "id": eventbridge_id,
            "metafields": [{ "namespace": "custom", "key": "delivery" }]
        })
    );

    let eventbridge_update = proxy.process_request(json_graphql_request(
        r#"# RustWebhookLocalRuntime
mutation($id: ID!) {
  eventBridgeWebhookSubscriptionUpdate(
    id: $id
    webhookSubscription: {
      arn: "arn:aws:events:us-east-1::event-source/aws.partner/shopify.com/347082227713/metafields-source-updated"
      metafields: [{ namespace: "custom", key: "delivery_updated" }]
    }
  ) {
    webhookSubscription {
      id
      metafields { namespace key }
    }
    userErrors { field message }
  }
}"#,
        json!({ "id": eventbridge_id }),
    ));
    assert_eq!(
        eventbridge_update.body["data"]["eventBridgeWebhookSubscriptionUpdate"]
            ["webhookSubscription"],
        json!({
            "id": eventbridge_id,
            "metafields": [{ "namespace": "custom", "key": "delivery_updated" }]
        })
    );
}

#[test]
fn webhook_subscription_endpoint_uri_variants_validate_cloud_destinations() {
    let mut proxy = snapshot_proxy();

    let eventbridge = proxy.process_request(json_graphql_request(
        "# RustWebhookLocalRuntime\nmutation { webhookSubscriptionCreate(topic: ORDERS_CREATE, webhookSubscription: { uri: \"arn:aws:events:us-east-1:1234:event-bus/default\", format: JSON }) { webhookSubscription { id callbackUrl endpoint { __typename ... on WebhookEventBridgeEndpoint { arn } } } userErrors { field message } } }",
        json!({}),
    ));
    assert_eq!(eventbridge.status, 200);
    assert_eq!(
        eventbridge.body["data"]["webhookSubscriptionCreate"],
        json!({
            "webhookSubscription": null,
            "userErrors": [
                {"field": ["webhookSubscription", "callbackUrl"], "message": "Address is invalid"},
                {"field": ["webhookSubscription", "callbackUrl"], "message": "Address is not a valid AWS ARN"}
            ]
        })
    );

    let pubsub = proxy.process_request(json_graphql_request(
        "# RustWebhookLocalRuntime\nmutation { webhookSubscriptionCreate(topic: ORDERS_CREATE, webhookSubscription: { uri: \"pubsub://my-project:my-topic\", format: JSON }) { webhookSubscription { id callbackUrl endpoint { __typename ... on WebhookPubSubEndpoint { pubSubProject pubSubTopic } } } userErrors { field message } } }",
        json!({}),
    ));
    assert_eq!(pubsub.status, 200);
    assert_eq!(
        pubsub.body["data"]["webhookSubscriptionCreate"]["webhookSubscription"]["endpoint"],
        json!({
            "__typename": "WebhookPubSubEndpoint",
            "pubSubProject": "my-project",
            "pubSubTopic": "my-topic"
        })
    );
}

#[test]
fn webhook_subscription_validation_guards_match_old_gleam_cases() {
    let mut proxy = snapshot_proxy();

    let blank = proxy.process_request(json_graphql_request(
        "# RustWebhookLocalRuntime\nmutation { webhookSubscriptionCreate(topic: ORDERS_CREATE, webhookSubscription: { uri: \"\", format: JSON }) { webhookSubscription { id } userErrors { field message } } }",
        json!({}),
    ));
    assert_eq!(
        blank.body["data"]["webhookSubscriptionCreate"],
        json!({
            "webhookSubscription": null,
            "userErrors": [{
                "field": ["webhookSubscription", "callbackUrl"],
                "message": "Address can't be blank"
            }]
        })
    );

    let trimmed = proxy.process_request(json_graphql_request(
        "# RustWebhookLocalRuntime\nmutation { webhookSubscriptionCreate(topic: SHOP_UPDATE, webhookSubscription: { uri: \"  https://example.com/h  \", format: JSON, name: \"OrderHook\" }) { webhookSubscription { id uri name } userErrors { field message } } }",
        json!({}),
    ));
    assert_eq!(trimmed.status, 200);
    assert_eq!(
        trimmed.body["data"]["webhookSubscriptionCreate"]["webhookSubscription"]["uri"],
        json!("https://example.com/h")
    );

    let pubsub_without_topic = proxy.process_request(json_graphql_request(
        "# RustWebhookLocalRuntime\nmutation { webhookSubscriptionCreate(topic: ORDERS_CREATE, webhookSubscription: { uri: \"pubsub://my-project\", format: JSON }) { webhookSubscription { id } userErrors { field message } } }",
        json!({}),
    ));
    assert_eq!(
        pubsub_without_topic.body["data"]["webhookSubscriptionCreate"],
        json!({
            "webhookSubscription": null,
            "userErrors": [
                {
                    "field": ["webhookSubscription", "callbackUrl"],
                    "message": "Address protocol pubsub:// is not supported"
                },
                {
                    "field": ["webhookSubscription", "callbackUrl"],
                    "message": "Address is not a valid GCP pub/sub format. Format should be pubsub://project:topic"
                }
            ]
        })
    );

    let duplicate_uri = proxy.process_request(json_graphql_request(
        "# RustWebhookLocalRuntime\nmutation { webhookSubscriptionCreate(topic: SHOP_UPDATE, webhookSubscription: { uri: \"https://example.com/h\", format: JSON }) { webhookSubscription { id } userErrors { field message } } }",
        json!({}),
    ));
    assert_eq!(
        duplicate_uri.body["data"]["webhookSubscriptionCreate"],
        json!({
            "webhookSubscription": null,
            "userErrors": [{
                "field": ["webhookSubscription", "callbackUrl"],
                "message": "Address for this topic has already been taken"
            }]
        })
    );

    let duplicate_name = proxy.process_request(json_graphql_request(
        "# RustWebhookLocalRuntime\nmutation { webhookSubscriptionCreate(topic: SHOP_UPDATE, webhookSubscription: { uri: \"https://example.com/other\", name: \"orderhook\" }) { webhookSubscription { id name } userErrors { field message } } }",
        json!({}),
    ));
    assert_eq!(
        duplicate_name.body["data"]["webhookSubscriptionCreate"],
        json!({
            "webhookSubscription": null,
            "userErrors": [{
                "field": ["webhookSubscription", "name"],
                "message": "Name already exists, no duplicate allowed"
            }]
        })
    );

    let bad_name = proxy.process_request(json_graphql_request(
        "# RustWebhookLocalRuntime\nmutation { webhookSubscriptionCreate(topic: SHOP_UPDATE, webhookSubscription: { uri: \"https://example.com/bad-name\", name: \"has spaces\" }) { webhookSubscription { id } userErrors { field message } } }",
        json!({}),
    ));
    assert_eq!(
        bad_name.body["data"]["webhookSubscriptionCreate"],
        json!({
            "webhookSubscription": null,
            "userErrors": [{
                "field": ["webhookSubscription", "name"],
                "message": "Name name field can only contain alphanumeric characters, underscores, and hyphens"
            }]
        })
    );
}

#[test]
fn webhook_subscription_filter_byte_size_validation_matches_shopify_ordering() {
    let mut proxy = snapshot_proxy();
    let at_limit_filter = format!("id:{}", "1".repeat(65_532));
    let over_limit_filter = format!("id:{}", "1".repeat(65_533));
    let malformed_over_limit_filter = "totally bogus syntax ".repeat(3_500);

    assert_eq!(at_limit_filter.len(), 65_535);
    assert_eq!(over_limit_filter.len(), 65_536);
    assert!(malformed_over_limit_filter.len() > 65_535);

    let create_mutation = r#"
        mutation WebhookSubscriptionFilterByteSizeCreate(
          $topic: WebhookSubscriptionTopic!
          $webhookSubscription: WebhookSubscriptionInput!
        ) {
          webhookSubscriptionCreate(topic: $topic, webhookSubscription: $webhookSubscription) {
            webhookSubscription { id filter }
            userErrors { field message }
          }
        }
    "#;
    let update_mutation = r#"
        mutation WebhookSubscriptionFilterByteSizeUpdate(
          $id: ID!
          $webhookSubscription: WebhookSubscriptionInput!
        ) {
          webhookSubscriptionUpdate(id: $id, webhookSubscription: $webhookSubscription) {
            webhookSubscription { id filter }
            userErrors { field message }
          }
        }
    "#;
    let detail_query = r#"
        query WebhookSubscriptionFilterByteSizeDetail($id: ID!) {
          webhookSubscription(id: $id) { id filter }
        }
    "#;
    let filter_too_large_error = json!({
        "webhookSubscription": null,
        "userErrors": [{
            "field": ["webhookSubscription"],
            "message": "The specified filter exceeds the maximum allowed size."
        }]
    });

    let at_limit_create = proxy.process_request(json_graphql_request(
        create_mutation,
        json!({
            "topic": "ORDERS_CREATE",
            "webhookSubscription": {
                "uri": "https://example.com/filter-at-limit",
                "format": "JSON",
                "filter": at_limit_filter
            }
        }),
    ));
    assert_eq!(
        at_limit_create.body["data"]["webhookSubscriptionCreate"]["userErrors"],
        json!([])
    );
    assert!(
        at_limit_create.body["data"]["webhookSubscriptionCreate"]["webhookSubscription"]["id"]
            .as_str()
            .is_some()
    );

    let over_limit_create = proxy.process_request(json_graphql_request(
        create_mutation,
        json!({
            "topic": "ORDERS_CREATE",
            "webhookSubscription": {
                "uri": "https://example.com/filter-over-limit",
                "format": "JSON",
                "filter": over_limit_filter
            }
        }),
    ));
    assert_eq!(
        over_limit_create.body["data"]["webhookSubscriptionCreate"],
        filter_too_large_error
    );

    let malformed_over_limit_create = proxy.process_request(json_graphql_request(
        create_mutation,
        json!({
            "topic": "ORDERS_CREATE",
            "webhookSubscription": {
                "uri": "https://example.com/filter-over-limit-malformed",
                "format": "JSON",
                "filter": malformed_over_limit_filter
            }
        }),
    ));
    assert_eq!(
        malformed_over_limit_create.body["data"]["webhookSubscriptionCreate"],
        filter_too_large_error,
        "byte-size validation should take precedence over syntax validation"
    );

    let malformed_sub_limit_create = proxy.process_request(json_graphql_request(
        create_mutation,
        json!({
            "topic": "ORDERS_CREATE",
            "webhookSubscription": {
                "uri": "https://example.com/filter-sub-limit-malformed",
                "format": "JSON",
                "filter": "totally bogus syntax"
            }
        }),
    ));
    assert_eq!(
        malformed_sub_limit_create.body["data"]["webhookSubscriptionCreate"],
        json!({
            "webhookSubscription": null,
            "userErrors": [{
                "field": ["webhookSubscription"],
                "message": "The specified filter is invalid, please ensure you specify the field(s) you wish to filter on."
            }]
        })
    );

    let update_base = proxy.process_request(json_graphql_request(
        create_mutation,
        json!({
            "topic": "ORDERS_CREATE",
            "webhookSubscription": {
                "uri": "https://example.com/filter-update-base",
                "format": "JSON",
                "filter": "id:1"
            }
        }),
    ));
    let update_base_id = update_base.body["data"]["webhookSubscriptionCreate"]
        ["webhookSubscription"]["id"]
        .as_str()
        .expect("update setup should create a subscription")
        .to_string();

    let over_limit_update = proxy.process_request(json_graphql_request(
        update_mutation,
        json!({
            "id": update_base_id,
            "webhookSubscription": {
                "uri": "https://example.com/filter-update-base",
                "format": "JSON",
                "filter": format!("id:{}", "2".repeat(65_533))
            }
        }),
    ));
    assert_eq!(
        over_limit_update.body["data"]["webhookSubscriptionUpdate"],
        filter_too_large_error
    );

    let detail_after_rejected_update = proxy.process_request(json_graphql_request(
        detail_query,
        json!({ "id": update_base_id }),
    ));
    assert_eq!(
        detail_after_rejected_update.body["data"]["webhookSubscription"]["filter"],
        json!("id:1")
    );
}

#[test]
fn webhook_subscription_rejects_unknown_topic_before_staging() {
    let mut proxy = snapshot_proxy();

    let inline_unknown_topic = proxy.process_request(json_graphql_request(
        r#"# RustWebhookLocalRuntime
        mutation WebhookSubscriptionBogusTopic {
          webhookSubscriptionCreate(topic: NOT_A_REAL_TOPIC, webhookSubscription: { uri: "https://hooks.example.com/bogus", format: JSON }) {
            webhookSubscription { id topic uri }
            userErrors { field message }
          }
        }"#,
        json!({}),
    ));
    assert_eq!(inline_unknown_topic.status, 200);
    assert_eq!(
        inline_unknown_topic.body,
        json!({
            "errors": [{
                "message": "Argument 'topic' on Field 'webhookSubscriptionCreate' has an invalid value (NOT_A_REAL_TOPIC). Expected type 'WebhookSubscriptionTopic!'.",
                "locations": [{ "line": 3, "column": 11 }],
                "path": ["mutation WebhookSubscriptionBogusTopic", "webhookSubscriptionCreate", "topic"],
                "extensions": {
                    "code": "argumentLiteralsIncompatible",
                    "typeName": "Field",
                    "argumentName": "topic"
                }
            }]
        })
    );
    let count = proxy.process_request(json_graphql_request(
        "# RustWebhookLocalRuntime\nquery { webhookSubscriptionsCount { count } }",
        json!({}),
    ));
    assert_eq!(
        count.body["data"]["webhookSubscriptionsCount"],
        json!({ "count": 0 })
    );
    assert_eq!(log_snapshot(&proxy), json!({ "entries": [] }));

    let variable_unknown_topic = proxy.process_request(json_graphql_request(
        r#"# RustWebhookLocalRuntime
        mutation WebhookSubscriptionCreateParity(
          $topic: WebhookSubscriptionTopic!
          $webhookSubscription: WebhookSubscriptionInput!
        ) {
          webhookSubscriptionCreate(topic: $topic, webhookSubscription: $webhookSubscription) {
            webhookSubscription { id topic }
            userErrors { field message }
          }
        }"#,
        json!({
            "topic": "NOT_A_REAL_TOPIC",
            "webhookSubscription": {
                "uri": "https://hooks.example.com/bogus-variable"
            }
        }),
    ));
    assert_eq!(variable_unknown_topic.status, 200);
    assert_eq!(
        variable_unknown_topic.body["errors"][0]["extensions"]["code"],
        json!("INVALID_VARIABLE")
    );
    assert_eq!(
        variable_unknown_topic.body["errors"][0]["extensions"]["value"],
        json!("NOT_A_REAL_TOPIC")
    );
    assert!(
        variable_unknown_topic.body["errors"][0]["extensions"]["problems"][0]["explanation"]
            .as_str()
            .is_some_and(|message| message.contains("SHOP_UPDATE")
                && message.contains("CHECKOUT_AND_ACCOUNTS_CONFIGURATIONS_UPDATE"))
    );
    let count = proxy.process_request(json_graphql_request(
        "# RustWebhookLocalRuntime\nquery { webhookSubscriptionsCount { count } }",
        json!({}),
    ));
    assert_eq!(
        count.body["data"]["webhookSubscriptionsCount"],
        json!({ "count": 0 })
    );
    assert_eq!(log_snapshot(&proxy), json!({ "entries": [] }));
}

#[test]
fn webhook_subscription_duplicate_scope_includes_format_filter_and_api_permission() {
    let mut proxy = snapshot_proxy();

    let create = |proxy: &mut DraftProxy, webhook_subscription: Value| {
        proxy.process_request(json_graphql_request(
            r#"# RustWebhookLocalRuntime
            mutation($webhookSubscription: WebhookSubscriptionInput!) {
              webhookSubscriptionCreate(topic: SHOP_UPDATE, webhookSubscription: $webhookSubscription) {
                webhookSubscription { id topic uri format filter }
                userErrors { field message }
              }
            }"#,
            json!({ "webhookSubscription": webhook_subscription }),
        ))
    };

    let first = create(
        &mut proxy,
        json!({
            "uri": "https://hooks.example.com/same-uri",
            "format": "JSON",
            "filter": "orders_count:1"
        }),
    );
    assert_eq!(
        first.body["data"]["webhookSubscriptionCreate"]["userErrors"],
        json!([])
    );

    let different_format = create(
        &mut proxy,
        json!({
            "uri": "https://hooks.example.com/same-uri",
            "format": "XML",
            "filter": "orders_count:1"
        }),
    );
    assert_eq!(
        different_format.body["data"]["webhookSubscriptionCreate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        different_format.body["data"]["webhookSubscriptionCreate"]["webhookSubscription"]["format"],
        json!("XML")
    );

    let different_filter = create(
        &mut proxy,
        json!({
            "uri": "https://hooks.example.com/same-uri",
            "format": "JSON",
            "filter": "orders_count:2"
        }),
    );
    assert_eq!(
        different_filter.body["data"]["webhookSubscriptionCreate"]["userErrors"],
        json!([])
    );

    let exact_duplicate = create(
        &mut proxy,
        json!({
            "uri": "https://hooks.example.com/same-uri",
            "format": "JSON",
            "filter": "orders_count:1"
        }),
    );
    assert_eq!(
        exact_duplicate.body["data"]["webhookSubscriptionCreate"],
        json!({
            "webhookSubscription": null,
            "userErrors": [{
                "field": ["webhookSubscription", "callbackUrl"],
                "message": "Address for this topic has already been taken"
            }]
        })
    );

    let count = proxy.process_request(json_graphql_request(
        "# RustWebhookLocalRuntime\nquery { webhookSubscriptionsCount { count } }",
        json!({}),
    ));
    assert_eq!(
        count.body["data"]["webhookSubscriptionsCount"],
        json!({ "count": 3 })
    );
    assert_eq!(log_snapshot(&proxy)["entries"].as_array().unwrap().len(), 3);
}

#[test]
fn dedicated_pubsub_missing_required_fields_return_coercion_errors_before_staging() {
    let mut proxy = snapshot_proxy();

    let missing_topic = proxy.process_request(json_graphql_request(
        r#"# RustWebhookLocalRuntime
        mutation PubSubWebhookSubscriptionCreateMissingTopic(
          $topic: WebhookSubscriptionTopic!
          $webhookSubscription: PubSubWebhookSubscriptionInput!
        ) {
          pubSubWebhookSubscriptionCreate(topic: $topic, webhookSubscription: $webhookSubscription) {
            webhookSubscription { id }
            userErrors { field message }
          }
        }"#,
        json!({
            "topic": "SHOP_UPDATE",
            "webhookSubscription": {
                "pubSubProject": "valid-project"
            }
        }),
    ));
    assert_eq!(missing_topic.status, 200);
    assert_eq!(
        missing_topic.body,
        json!({
            "errors": [{
                "message": "Variable $webhookSubscription of type PubSubWebhookSubscriptionInput! was provided invalid value for pubSubTopic (Expected value to not be null)",
                "locations": [{ "line": 4, "column": 11 }],
                "extensions": {
                    "code": "INVALID_VARIABLE",
                    "value": { "pubSubProject": "valid-project" },
                    "problems": [{
                        "path": ["pubSubTopic"],
                        "explanation": "Expected value to not be null"
                    }]
                }
            }]
        })
    );

    let missing_both = proxy.process_request(json_graphql_request(
        r#"# RustWebhookLocalRuntime
        mutation PubSubWebhookSubscriptionCreateMissingBoth(
          $topic: WebhookSubscriptionTopic!
          $webhookSubscription: PubSubWebhookSubscriptionInput!
        ) {
          pubSubWebhookSubscriptionCreate(topic: $topic, webhookSubscription: $webhookSubscription) {
            webhookSubscription { id }
            userErrors { field message }
          }
        }"#,
        json!({
            "topic": "SHOP_UPDATE",
            "webhookSubscription": {}
        }),
    ));
    assert_eq!(
        missing_both.body["errors"][0]["extensions"]["problems"],
        json!([
            { "path": ["pubSubProject"], "explanation": "Expected value to not be null" },
            { "path": ["pubSubTopic"], "explanation": "Expected value to not be null" }
        ])
    );

    let inline_missing_topic = proxy.process_request(json_graphql_request(
        r#"# RustWebhookLocalRuntime
        mutation {
          pubSubWebhookSubscriptionCreate(topic: SHOP_UPDATE, webhookSubscription: { pubSubProject: "valid-project" }) {
            webhookSubscription { id }
            userErrors { field message }
          }
        }"#,
        json!({}),
    ));
    assert_eq!(inline_missing_topic.status, 200);
    assert_eq!(
        inline_missing_topic.body["errors"][0]["extensions"]["code"],
        json!("missingRequiredInputObjectAttribute")
    );
    assert_eq!(
        inline_missing_topic.body["errors"][0]["extensions"]["argumentName"],
        json!("pubSubTopic")
    );

    let create = proxy.process_request(json_graphql_request(
        "# RustWebhookLocalRuntime\nmutation { pubSubWebhookSubscriptionCreate(topic: SHOP_UPDATE, webhookSubscription: { pubSubProject: \"valid-project\", pubSubTopic: \"topic-1\" }) { webhookSubscription { id } userErrors { field message } } }",
        json!({}),
    ));
    let id = create.body["data"]["pubSubWebhookSubscriptionCreate"]["webhookSubscription"]["id"]
        .as_str()
        .unwrap();

    let update_missing_project = proxy.process_request(json_graphql_request(
        r#"# RustWebhookLocalRuntime
        mutation PubSubWebhookSubscriptionUpdateMissingProject($id: ID!, $webhookSubscription: PubSubWebhookSubscriptionInput!) {
          pubSubWebhookSubscriptionUpdate(id: $id, webhookSubscription: $webhookSubscription) {
            webhookSubscription { id }
            userErrors { field message }
          }
        }"#,
        json!({
            "id": id,
            "webhookSubscription": {
                "pubSubTopic": "topic-1"
            }
        }),
    ));
    assert_eq!(
        update_missing_project.body["errors"][0]["extensions"]["problems"],
        json!([{ "path": ["pubSubProject"], "explanation": "Expected value to not be null" }])
    );

    assert_eq!(
        proxy
            .process_request(json_graphql_request(
                "# RustWebhookLocalRuntime\nquery { webhookSubscriptionsCount { count } }",
                json!({}),
            ))
            .body["data"]["webhookSubscriptionsCount"],
        json!({ "count": 1 }),
        "only the valid control create should stage a record"
    );
    assert_eq!(
        log_snapshot(&proxy)["entries"].as_array().unwrap().len(),
        1,
        "coercion errors should not append mutation log entries"
    );
}

#[test]
fn webhook_subscription_uri_and_format_validation_ports_old_gleam_edges() {
    let assert_rejected = |uri: &str,
                           format_value: &str,
                           topic: &str,
                           expected_messages: Vec<&str>| {
        let mut proxy = snapshot_proxy();
        let response = proxy.process_request(json_graphql_request(
            &format!(
                "# RustWebhookLocalRuntime\nmutation {{ webhookSubscriptionCreate(topic: {topic}, webhookSubscription: {{ uri: \"{uri}\", format: {format_value} }}) {{ webhookSubscription {{ id }} userErrors {{ field message }} }} }}"
            ),
            json!({}),
        ));
        assert_eq!(response.status, 200);
        assert_eq!(
            response.body["data"]["webhookSubscriptionCreate"]["webhookSubscription"],
            Value::Null
        );
        let messages: Vec<&str> = response.body["data"]["webhookSubscriptionCreate"]["userErrors"]
            .as_array()
            .unwrap()
            .iter()
            .map(|error| error["message"].as_str().unwrap())
            .collect();
        assert_eq!(messages, expected_messages, "unexpected errors for {uri}");
    };

    assert_rejected(
        "pubsub://-bad:topic",
        "JSON",
        "SHOP_UPDATE",
        vec![
            "Address is invalid",
            "Address is not a valid GCP project id.",
        ],
    );
    assert_rejected(
        "pubsub://valid-project:goog-prefixed",
        "JSON",
        "SHOP_UPDATE",
        vec!["Address is invalid", "Address is not a valid GCP topic id."],
    );
    assert_rejected(
        "pubsub://valid-project:go",
        "JSON",
        "SHOP_UPDATE",
        vec!["Address is invalid", "Address is not a valid GCP topic id."],
    );
    assert_rejected(
        "pubsub://valid-project:bad/topic",
        "JSON",
        "SHOP_UPDATE",
        vec!["Address is invalid", "Address is not a valid GCP topic id."],
    );
    assert_rejected(
        "arn:aws:events:bogus",
        "JSON",
        "SHOP_UPDATE",
        vec!["Address is invalid", "Address is not a valid AWS ARN"],
    );
    assert_rejected(
        "https://admin.shopify.com/hook",
        "JSON",
        "SHOP_UPDATE",
        vec!["Address cannot be a Shopify or an internal domain"],
    );
    assert_rejected(
        "https://192.168.1.10/hook",
        "JSON",
        "SHOP_UPDATE",
        vec!["Address cannot be a Shopify or an internal domain"],
    );
    assert_rejected(
        "pubsub://valid-project:topic",
        "XML",
        "SHOP_UPDATE",
        vec!["Format can only be used with format: 'json'"],
    );
    assert_rejected(
        "https://hooks.example.com/returns",
        "XML",
        "RETURNS_APPROVE",
        vec!["Format 'xml' is invalid for this webhook topic. Allowed formats: json"],
    );

    let long_uri = format!("https://example.com/{}", "a".repeat(65_516));
    assert_rejected(
        &long_uri,
        "JSON",
        "SHOP_UPDATE",
        vec!["Address is too big (maximum is 64 KB)"],
    );
}

#[test]
fn webhook_eventbridge_cloud_delivery_rejects_non_json_format_without_staging() {
    let mut proxy = snapshot_proxy();

    fn partner_arn(region: &str, source: &str) -> String {
        format!(
            "arn:aws:events:{region}::event-source/aws.partner/shopify.com/347082227713/{source}"
        )
    }

    fn request_with_api_client(query: &str, variables: Value) -> Request {
        let mut request = json_graphql_request(query, variables);
        request.headers.insert(
            "x-shopify-draft-proxy-api-client-id".to_string(),
            "347082227713".to_string(),
        );
        request
    }

    let dedicated_create_mutation = r#"
        mutation RustWebhookLocalRuntimeEventBridgeXmlCreate($webhookSubscription: EventBridgeWebhookSubscriptionInput!) {
          eventBridgeWebhookSubscriptionCreate(topic: SHOP_UPDATE, webhookSubscription: $webhookSubscription) {
            webhookSubscription { id format }
            userErrors { field message }
          }
        }
    "#;
    let dedicated_update_mutation = r#"
        mutation RustWebhookLocalRuntimeEventBridgeXmlUpdate($id: ID!, $webhookSubscription: EventBridgeWebhookSubscriptionInput!) {
          eventBridgeWebhookSubscriptionUpdate(id: $id, webhookSubscription: $webhookSubscription) {
            webhookSubscription { id format }
            userErrors { field message }
          }
        }
    "#;
    let unified_create_mutation = r#"
        mutation RustWebhookLocalRuntimeUnifiedArnXmlCreate($webhookSubscription: WebhookSubscriptionInput!) {
          webhookSubscriptionCreate(topic: SHOP_UPDATE, webhookSubscription: $webhookSubscription) {
            webhookSubscription { id format }
            userErrors { field message }
          }
        }
    "#;
    let unified_update_mutation = r#"
        mutation RustWebhookLocalRuntimeUnifiedArnXmlUpdate($id: ID!, $webhookSubscription: WebhookSubscriptionInput!) {
          webhookSubscriptionUpdate(id: $id, webhookSubscription: $webhookSubscription) {
            webhookSubscription { id format }
            userErrors { field message }
          }
        }
    "#;
    let expected_rejection = json!({
        "webhookSubscription": null,
        "userErrors": [{
            "field": ["webhookSubscription", "format"],
            "message": "Format can only be used with format: 'json'"
        }]
    });

    let dedicated_create_xml = proxy.process_request(request_with_api_client(
        dedicated_create_mutation,
        json!({"webhookSubscription": {"arn": partner_arn("us-east-1", "xml-dedicated-create"), "format": "XML"}}),
    ));
    assert_eq!(
        dedicated_create_xml.body["data"]["eventBridgeWebhookSubscriptionCreate"],
        expected_rejection
    );

    let unified_create_xml = proxy.process_request(request_with_api_client(
        unified_create_mutation,
        json!({"webhookSubscription": {"uri": partner_arn("us-east-1", "xml-unified-create"), "format": "XML"}}),
    ));
    assert_eq!(
        unified_create_xml.body["data"]["webhookSubscriptionCreate"],
        expected_rejection
    );

    let dedicated_setup = proxy.process_request(request_with_api_client(
        dedicated_create_mutation,
        json!({"webhookSubscription": {"arn": partner_arn("us-east-1", "json-dedicated-setup"), "format": "JSON"}}),
    ));
    let dedicated_id = dedicated_setup.body["data"]["eventBridgeWebhookSubscriptionCreate"]
        ["webhookSubscription"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(
        dedicated_setup.body["data"]["eventBridgeWebhookSubscriptionCreate"]["userErrors"],
        json!([])
    );

    let dedicated_update_xml = proxy.process_request(request_with_api_client(
        dedicated_update_mutation,
        json!({
            "id": dedicated_id,
            "webhookSubscription": {"arn": partner_arn("us-west-2", "xml-dedicated-update"), "format": "XML"}
        }),
    ));
    assert_eq!(
        dedicated_update_xml.body["data"]["eventBridgeWebhookSubscriptionUpdate"],
        expected_rejection
    );

    let unified_setup = proxy.process_request(request_with_api_client(
        unified_create_mutation,
        json!({"webhookSubscription": {"uri": partner_arn("us-east-1", "json-unified-setup"), "format": "JSON"}}),
    ));
    let unified_id = unified_setup.body["data"]["webhookSubscriptionCreate"]["webhookSubscription"]
        ["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(
        unified_setup.body["data"]["webhookSubscriptionCreate"]["userErrors"],
        json!([])
    );

    let unified_update_xml = proxy.process_request(request_with_api_client(
        unified_update_mutation,
        json!({
            "id": unified_id,
            "webhookSubscription": {"uri": partner_arn("us-west-2", "xml-unified-update"), "format": "XML"}
        }),
    ));
    assert_eq!(
        unified_update_xml.body["data"]["webhookSubscriptionUpdate"],
        expected_rejection
    );

    assert_eq!(
        proxy
            .process_request(json_graphql_request(
                "# RustWebhookLocalRuntime\nquery { webhookSubscriptionsCount { count } }",
                json!({}),
            ))
            .body["data"]["webhookSubscriptionsCount"],
        json!({ "count": 2 }),
        "only the valid JSON setup creates should stage records"
    );
    assert_eq!(
        log_snapshot(&proxy)["entries"].as_array().unwrap().len(),
        2,
        "rejected cloud-format create/update attempts should not append mutation log entries"
    );
}

#[test]
fn pubsub_gcp_project_and_topic_char_rules_match_shopify() {
    let mut proxy = snapshot_proxy();

    let dedicated_numeric_project = proxy.process_request(json_graphql_request(
        r#"# RustWebhookLocalRuntime
        mutation {
          pubSubWebhookSubscriptionCreate(
            topic: SHOP_UPDATE
            webhookSubscription: { pubSubProject: "123456789012", pubSubTopic: "valid-topic" }
          ) {
            webhookSubscription {
              id
              uri
              endpoint {
                __typename
                ... on WebhookPubSubEndpoint { pubSubProject pubSubTopic }
              }
            }
            userErrors { field message }
          }
        }"#,
        json!({}),
    ));
    assert_eq!(dedicated_numeric_project.status, 200);
    assert_eq!(
        dedicated_numeric_project.body["data"]["pubSubWebhookSubscriptionCreate"]["userErrors"],
        json!([])
    );
    let dedicated_id = dedicated_numeric_project.body["data"]["pubSubWebhookSubscriptionCreate"]
        ["webhookSubscription"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(
        dedicated_numeric_project.body["data"]["pubSubWebhookSubscriptionCreate"]
            ["webhookSubscription"]["endpoint"],
        json!({
            "__typename": "WebhookPubSubEndpoint",
            "pubSubProject": "123456789012",
            "pubSubTopic": "valid-topic"
        })
    );

    let dedicated_digit_leading_topic = proxy.process_request(json_graphql_request(
        r#"# RustWebhookLocalRuntime
        mutation {
          pubSubWebhookSubscriptionCreate(
            topic: SHOP_UPDATE
            webhookSubscription: { pubSubProject: "valid-project", pubSubTopic: "1topic" }
          ) {
            webhookSubscription { id }
            userErrors { field message }
          }
        }"#,
        json!({}),
    ));
    assert_eq!(dedicated_digit_leading_topic.status, 200);
    assert_eq!(
        dedicated_digit_leading_topic.body["data"]["pubSubWebhookSubscriptionCreate"],
        json!({
            "webhookSubscription": null,
            "userErrors": [{
                "field": ["webhookSubscription", "pubSubTopic"],
                "message": "Google Cloud Pub/Sub topic ID is not valid"
            }]
        })
    );

    let unified_percent_topic = proxy.process_request(json_graphql_request(
        r#"# RustWebhookLocalRuntime
        mutation {
          webhookSubscriptionCreate(
            topic: SHOP_UPDATE
            webhookSubscription: { uri: "pubsub://valid-project:my%25topic", format: JSON }
          ) {
            webhookSubscription {
              id
              uri
              endpoint {
                __typename
                ... on WebhookPubSubEndpoint { pubSubProject pubSubTopic }
              }
            }
            userErrors { field message }
          }
        }"#,
        json!({}),
    ));
    assert_eq!(unified_percent_topic.status, 200);
    assert_eq!(
        unified_percent_topic.body["data"]["webhookSubscriptionCreate"]["userErrors"],
        json!([])
    );
    let unified_id = unified_percent_topic.body["data"]["webhookSubscriptionCreate"]
        ["webhookSubscription"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(
        unified_percent_topic.body["data"]["webhookSubscriptionCreate"]["webhookSubscription"]
            ["endpoint"],
        json!({
            "__typename": "WebhookPubSubEndpoint",
            "pubSubProject": "valid-project",
            "pubSubTopic": "my%25topic"
        })
    );

    let dedicated_update_percent_topic = proxy.process_request(json_graphql_request(
        r#"# RustWebhookLocalRuntime
        mutation($id: ID!) {
          pubSubWebhookSubscriptionUpdate(
            id: $id
            webhookSubscription: { pubSubProject: "123456789012", pubSubTopic: "next%25topic" }
          ) {
            webhookSubscription {
              id
              uri
              endpoint {
                __typename
                ... on WebhookPubSubEndpoint { pubSubProject pubSubTopic }
              }
            }
            userErrors { field message }
          }
        }"#,
        json!({ "id": dedicated_id }),
    ));
    assert_eq!(dedicated_update_percent_topic.status, 200);
    assert_eq!(
        dedicated_update_percent_topic.body["data"]["pubSubWebhookSubscriptionUpdate"]
            ["userErrors"],
        json!([])
    );
    assert_eq!(
        dedicated_update_percent_topic.body["data"]["pubSubWebhookSubscriptionUpdate"]
            ["webhookSubscription"]["endpoint"],
        json!({
            "__typename": "WebhookPubSubEndpoint",
            "pubSubProject": "123456789012",
            "pubSubTopic": "next%25topic"
        })
    );

    let unified_update_numeric_project = proxy.process_request(json_graphql_request(
        r#"# RustWebhookLocalRuntime
        mutation($id: ID!) {
          webhookSubscriptionUpdate(
            id: $id
            webhookSubscription: { uri: "pubsub://123456789012:valid-topic", format: JSON }
          ) {
            webhookSubscription {
              id
              uri
              endpoint {
                __typename
                ... on WebhookPubSubEndpoint { pubSubProject pubSubTopic }
              }
            }
            userErrors { field message }
          }
        }"#,
        json!({ "id": unified_id }),
    ));
    assert_eq!(unified_update_numeric_project.status, 200);
    assert_eq!(
        unified_update_numeric_project.body["data"]["webhookSubscriptionUpdate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        unified_update_numeric_project.body["data"]["webhookSubscriptionUpdate"]
            ["webhookSubscription"]["endpoint"],
        json!({
            "__typename": "WebhookPubSubEndpoint",
            "pubSubProject": "123456789012",
            "pubSubTopic": "valid-topic"
        })
    );

    let unified_update_digit_leading_topic = proxy.process_request(json_graphql_request(
        r#"# RustWebhookLocalRuntime
        mutation($id: ID!) {
          webhookSubscriptionUpdate(
            id: $id
            webhookSubscription: { uri: "pubsub://valid-project:1topic", format: JSON }
          ) {
            webhookSubscription { id }
            userErrors { field message }
          }
        }"#,
        json!({ "id": unified_id }),
    ));
    assert_eq!(unified_update_digit_leading_topic.status, 200);
    assert_eq!(
        unified_update_digit_leading_topic.body["data"]["webhookSubscriptionUpdate"],
        json!({
            "webhookSubscription": null,
            "userErrors": [
                {
                    "field": ["webhookSubscription", "callbackUrl"],
                    "message": "Address is invalid"
                },
                {
                    "field": ["webhookSubscription", "callbackUrl"],
                    "message": "Address is not a valid GCP topic id."
                }
            ]
        })
    );

    let detail = proxy.process_request(json_graphql_request(
        r#"# RustWebhookLocalRuntime
        query($id: ID!) {
          webhookSubscription(id: $id) {
            id
            endpoint {
              __typename
              ... on WebhookPubSubEndpoint { pubSubProject pubSubTopic }
            }
          }
        }"#,
        json!({ "id": unified_id }),
    ));
    assert_eq!(
        detail.body["data"]["webhookSubscription"]["endpoint"],
        json!({
            "__typename": "WebhookPubSubEndpoint",
            "pubSubProject": "123456789012",
            "pubSubTopic": "valid-topic"
        })
    );

    let list = proxy.process_request(json_graphql_request(
        r#"# RustWebhookLocalRuntime
        query {
          webhookSubscriptions(first: 5) {
            nodes {
              id
              endpoint {
                __typename
                ... on WebhookPubSubEndpoint { pubSubProject pubSubTopic }
              }
            }
          }
        }"#,
        json!({}),
    ));
    assert_eq!(
        list.body["data"]["webhookSubscriptions"]["nodes"]
            .as_array()
            .unwrap()
            .len(),
        2
    );

    let count = proxy.process_request(json_graphql_request(
        "# RustWebhookLocalRuntime\nquery { webhookSubscriptionsCount { count } }",
        json!({}),
    ));
    assert_eq!(
        count.body["data"]["webhookSubscriptionsCount"],
        json!({ "count": 2 })
    );
    assert_eq!(
        log_snapshot(&proxy)["entries"]
            .as_array()
            .unwrap()
            .len(),
        4,
        "two accepted creates plus two accepted updates should be logged; rejected mutations should not"
    );
}

#[test]
fn dedicated_pubsub_webhook_update_uses_old_gleam_field_path_errors() {
    let mut proxy = snapshot_proxy();
    let create = proxy.process_request(json_graphql_request(
        "# RustWebhookLocalRuntime\nmutation { pubSubWebhookSubscriptionCreate(topic: SHOP_UPDATE, webhookSubscription: { pubSubProject: \"valid-project\", pubSubTopic: \"topic-1\" }) { webhookSubscription { id } userErrors { field message } } }",
        json!({}),
    ));
    let id = create.body["data"]["pubSubWebhookSubscriptionCreate"]["webhookSubscription"]["id"]
        .as_str()
        .unwrap();

    let update = proxy.process_request(json_graphql_request(
        "# RustWebhookLocalRuntime\nmutation($id: ID!) { pubSubWebhookSubscriptionUpdate(id: $id, webhookSubscription: { pubSubProject: \"valid-project\", pubSubTopic: \"goog-prefixed\" }) { webhookSubscription { id uri } userErrors { field message } } }",
        json!({ "id": id }),
    ));

    assert_eq!(update.status, 200);
    assert_eq!(
        update.body["data"]["pubSubWebhookSubscriptionUpdate"],
        json!({
            "webhookSubscription": null,
            "userErrors": [{
                "field": ["webhookSubscription", "pubSubTopic"],
                "message": "Google Cloud Pub/Sub topic ID is not valid"
            }]
        })
    );
}

#[test]
fn webhook_subscriptions_connection_filters_sorts_and_counts_like_old_gleam_helpers() {
    let mut proxy = snapshot_proxy();

    for (topic, uri, format) in [
        ("ORDERS_CREATE", "https://hook-1.example.com", "JSON"),
        ("ORDERS_PAID", "https://hook-2.example.com", "XML"),
        ("PRODUCTS_CREATE", "https://hook-3.example.com", "JSON"),
    ] {
        let create = proxy.process_request(json_graphql_request(
            &format!(
                "# RustWebhookLocalRuntime\nmutation {{ webhookSubscriptionCreate(topic: {topic}, webhookSubscription: {{ uri: \"{uri}\", format: {format} }}) {{ webhookSubscription {{ id }} userErrors {{ message }} }} }}"
            ),
            json!({}),
        ));
        assert_eq!(create.status, 200);
        assert_eq!(
            create.body["data"]["webhookSubscriptionCreate"]["userErrors"],
            json!([])
        );
    }

    let topic_filter = proxy.process_request(json_graphql_request(
        "# RustWebhookLocalRuntime\nquery { webhookSubscriptions(first: 5, topics: [ORDERS_PAID]) { nodes { topic uri } } }",
        json!({}),
    ));
    assert_eq!(
        topic_filter.body["data"]["webhookSubscriptions"]["nodes"],
        json!([{ "topic": "ORDERS_PAID", "uri": "https://hook-2.example.com" }])
    );

    let query_filter = proxy.process_request(json_graphql_request(
        "# RustWebhookLocalRuntime\nquery { webhookSubscriptions(first: 5, query: \"topic:orders AND format:JSON\") { nodes { topic format } } }",
        json!({}),
    ));
    assert_eq!(
        query_filter.body["data"]["webhookSubscriptions"]["nodes"],
        json!([{ "topic": "ORDERS_CREATE", "format": "JSON" }])
    );

    let legacy_id_filter = proxy.process_request(json_graphql_request(
        "# RustWebhookLocalRuntime\nquery { webhookSubscriptions(first: 5, query: \"id:1\") { nodes { legacyResourceId uri } } }",
        json!({}),
    ));
    assert_eq!(
        legacy_id_filter.body["data"]["webhookSubscriptions"]["nodes"],
        json!([{ "legacyResourceId": "1", "uri": "https://hook-1.example.com" }])
    );

    let format_arg_filter = proxy.process_request(json_graphql_request(
        "# RustWebhookLocalRuntime\nquery { webhookSubscriptions(first: 5, format: JSON) { nodes { topic format } } }",
        json!({}),
    ));
    assert_eq!(
        format_arg_filter.body["data"]["webhookSubscriptions"]["nodes"],
        json!([
            { "topic": "ORDERS_CREATE", "format": "JSON" },
            { "topic": "PRODUCTS_CREATE", "format": "JSON" }
        ])
    );

    let uri_arg_filter = proxy.process_request(json_graphql_request(
        "# RustWebhookLocalRuntime\nquery { webhookSubscriptions(first: 5, uri: \"https://hook-2.example.com\") { nodes { topic uri } } }",
        json!({}),
    ));
    assert_eq!(
        uri_arg_filter.body["data"]["webhookSubscriptions"]["nodes"],
        json!([{ "topic": "ORDERS_PAID", "uri": "https://hook-2.example.com" }])
    );

    let negated_format = proxy.process_request(json_graphql_request(
        "# RustWebhookLocalRuntime\nquery { webhookSubscriptions(first: 5, query: \"-format:JSON\") { nodes { topic format } } }",
        json!({}),
    ));
    assert_eq!(
        negated_format.body["data"]["webhookSubscriptions"]["nodes"],
        json!([{ "topic": "ORDERS_PAID", "format": "XML" }])
    );

    let reverse_topic = proxy.process_request(json_graphql_request(
        "# RustWebhookLocalRuntime\nquery { webhookSubscriptions(first: 5, sortKey: TOPIC, reverse: true) { nodes { topic } } }",
        json!({}),
    ));
    assert_eq!(
        reverse_topic.body["data"]["webhookSubscriptions"]["nodes"],
        json!([
            { "topic": "PRODUCTS_CREATE" },
            { "topic": "ORDERS_PAID" },
            { "topic": "ORDERS_CREATE" }
        ])
    );

    let count = proxy.process_request(json_graphql_request(
        "# RustWebhookLocalRuntime\nquery { webhookSubscriptionsCount(limit: 1) { count precision } }",
        json!({}),
    ));
    assert_eq!(
        count.body["data"]["webhookSubscriptionsCount"],
        json!({ "count": 1, "precision": "AT_LEAST" })
    );
}

#[test]
fn webhook_subscription_dedicated_pubsub_and_eventbridge_roots_stage_records() {
    let mut proxy = snapshot_proxy();

    let pubsub = proxy.process_request(json_graphql_request(
        "# RustWebhookLocalRuntime\nmutation { pubSubWebhookSubscriptionCreate(topic: SHOP_UPDATE, webhookSubscription: { pubSubProject: \"valid-project\", pubSubTopic: \"topic-1\" }) { webhookSubscription { id topic callbackUrl uri endpoint { __typename ... on WebhookPubSubEndpoint { pubSubProject pubSubTopic } } } userErrors { field message } } }",
        json!({}),
    ));
    assert_eq!(pubsub.status, 200);
    let pubsub_id = pubsub.body["data"]["pubSubWebhookSubscriptionCreate"]["webhookSubscription"]
        ["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(
        pubsub.body["data"]["pubSubWebhookSubscriptionCreate"]["webhookSubscription"],
        json!({
            "id": pubsub_id,
            "topic": "SHOP_UPDATE",
            "uri": "pubsub://valid-project:topic-1",
            "endpoint": {
                "__typename": "WebhookPubSubEndpoint",
                "pubSubProject": "valid-project",
                "pubSubTopic": "topic-1"
            }
        })
    );

    let eventbridge = proxy.process_request(json_graphql_request(
        "# RustWebhookLocalRuntime\nmutation { eventBridgeWebhookSubscriptionCreate(topic: SHOP_UPDATE, webhookSubscription: { arn: \"arn:aws:events:us-east-1::event-source/aws.partner/shopify.com/347082227713/source\" }) { webhookSubscription { id topic callbackUrl uri endpoint { __typename ... on WebhookEventBridgeEndpoint { arn } } } userErrors { field message } } }",
        json!({}),
    ));
    assert_eq!(eventbridge.status, 200);
    let eventbridge_id = eventbridge.body["data"]["eventBridgeWebhookSubscriptionCreate"]
        ["webhookSubscription"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(
        eventbridge.body["data"]["eventBridgeWebhookSubscriptionCreate"]["webhookSubscription"],
        json!({
            "id": eventbridge_id,
            "topic": "SHOP_UPDATE",
            "uri": "arn:aws:events:us-east-1::event-source/aws.partner/shopify.com/347082227713/source",
            "endpoint": {
                "__typename": "WebhookEventBridgeEndpoint",
                "arn": "arn:aws:events:us-east-1::event-source/aws.partner/shopify.com/347082227713/source"
            }
        })
    );

    let read = proxy.process_request(json_graphql_request(
        r#"# RustWebhookLocalRuntime
query($pubsubId: ID!, $eventbridgeId: ID!) {
  pubsub: webhookSubscription(id: $pubsubId) {
    id
    callbackUrl
    uri
    endpoint {
      __typename
      ... on WebhookPubSubEndpoint { pubSubProject pubSubTopic }
    }
  }
  eventbridge: webhookSubscription(id: $eventbridgeId) {
    id
    callbackUrl
    uri
    endpoint {
      __typename
      ... on WebhookEventBridgeEndpoint { arn }
    }
  }
  webhookSubscriptions(first: 10) {
    nodes {
      id
      callbackUrl
      uri
      endpoint {
        __typename
        ... on WebhookPubSubEndpoint { pubSubProject pubSubTopic }
        ... on WebhookEventBridgeEndpoint { arn }
      }
    }
  }
}"#,
        json!({ "pubsubId": pubsub_id, "eventbridgeId": eventbridge_id }),
    ));
    assert_eq!(
        read.body["data"]["pubsub"],
        json!({
            "id": pubsub_id,
            "uri": "pubsub://valid-project:topic-1",
            "endpoint": {
                "__typename": "WebhookPubSubEndpoint",
                "pubSubProject": "valid-project",
                "pubSubTopic": "topic-1"
            }
        })
    );
    assert_eq!(
        read.body["data"]["eventbridge"],
        json!({
            "id": eventbridge_id,
            "uri": "arn:aws:events:us-east-1::event-source/aws.partner/shopify.com/347082227713/source",
            "endpoint": {
                "__typename": "WebhookEventBridgeEndpoint",
                "arn": "arn:aws:events:us-east-1::event-source/aws.partner/shopify.com/347082227713/source"
            }
        })
    );
    assert_eq!(
        read.body["data"]["webhookSubscriptions"]["nodes"],
        json!([
            {
                "id": pubsub_id,
                "uri": "pubsub://valid-project:topic-1",
                "endpoint": {
                    "__typename": "WebhookPubSubEndpoint",
                    "pubSubProject": "valid-project",
                    "pubSubTopic": "topic-1"
                }
            },
            {
                "id": eventbridge_id,
                "uri": "arn:aws:events:us-east-1::event-source/aws.partner/shopify.com/347082227713/source",
                "endpoint": {
                    "__typename": "WebhookEventBridgeEndpoint",
                    "arn": "arn:aws:events:us-east-1::event-source/aws.partner/shopify.com/347082227713/source"
                }
            }
        ])
    );

    let pubsub_update = proxy.process_request(json_graphql_request(
        r#"# RustWebhookLocalRuntime
mutation($id: ID!) {
  pubSubWebhookSubscriptionUpdate(
    id: $id
    webhookSubscription: { pubSubProject: "valid-project", pubSubTopic: "topic-2" }
  ) {
    webhookSubscription {
      id
      callbackUrl
      uri
      endpoint {
        __typename
        ... on WebhookPubSubEndpoint { pubSubProject pubSubTopic }
      }
    }
    userErrors { field message }
  }
}"#,
        json!({ "id": pubsub_id }),
    ));
    assert_eq!(
        pubsub_update.body["data"]["pubSubWebhookSubscriptionUpdate"]["webhookSubscription"],
        json!({
            "id": pubsub_id,
            "uri": "pubsub://valid-project:topic-2",
            "endpoint": {
                "__typename": "WebhookPubSubEndpoint",
                "pubSubProject": "valid-project",
                "pubSubTopic": "topic-2"
            }
        })
    );

    let eventbridge_update = proxy.process_request(json_graphql_request(
        r#"# RustWebhookLocalRuntime
mutation($id: ID!) {
  eventBridgeWebhookSubscriptionUpdate(
    id: $id
    webhookSubscription: {
      arn: "arn:aws:events:us-east-1::event-source/aws.partner/shopify.com/347082227713/source-updated"
    }
  ) {
    webhookSubscription {
      id
      callbackUrl
      uri
      endpoint {
        __typename
        ... on WebhookEventBridgeEndpoint { arn }
      }
    }
    userErrors { field message }
  }
}"#,
        json!({ "id": eventbridge_id }),
    ));
    assert_eq!(
        eventbridge_update.body["data"]["eventBridgeWebhookSubscriptionUpdate"]
            ["webhookSubscription"],
        json!({
            "id": eventbridge_id,
            "uri": "arn:aws:events:us-east-1::event-source/aws.partner/shopify.com/347082227713/source-updated",
            "endpoint": {
                "__typename": "WebhookEventBridgeEndpoint",
                "arn": "arn:aws:events:us-east-1::event-source/aws.partner/shopify.com/347082227713/source-updated"
            }
        })
    );
}

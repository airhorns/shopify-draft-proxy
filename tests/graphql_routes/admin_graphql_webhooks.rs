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
          whatever
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
                },
                "whatever": null
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

    let log_roots: Vec<Value> = proxy.get_log_snapshot()["entries"]
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
      filter: "customer_id:123"
    }
  ) {
    webhookSubscription {
      id
      topic
      format
      includeFields
      metafieldNamespaces
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
    assert_eq!(
        create.body["data"]["webhookSubscriptionCreate"]["webhookSubscription"],
        json!({
            "id": webhook_id,
            "topic": "SHOP_UPDATE",
            "format": "JSON",
            "includeFields": ["id", "name"],
            "metafieldNamespaces": ["custom"],
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
    filter
    createdAt
    updatedAt
  }
  webhookSubscriptions(first: 10) {
    nodes {
      id
      includeFields
      metafieldNamespaces
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
    assert_eq!(
        update_omitted_fields.body["data"]["webhookSubscriptionUpdate"]["webhookSubscription"],
        json!({
            "id": webhook_id,
            "includeFields": ["id", "name"],
            "metafieldNamespaces": ["custom"],
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
    assert_eq!(
        update_empty_filter.body["data"]["webhookSubscriptionUpdate"]["webhookSubscription"],
        json!({
            "id": webhook_id,
            "includeFields": ["id", "name"],
            "metafieldNamespaces": ["custom"],
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
    assert_eq!(default_subscription["includeFields"], json!([]));
    assert_eq!(default_subscription["metafieldNamespaces"], json!([]));
    assert_eq!(default_subscription["filter"], Value::Null);
    assert!(default_subscription["createdAt"].as_str().is_some());
    assert_eq!(
        default_subscription["updatedAt"],
        default_subscription["createdAt"]
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
    assert_eq!(proxy.get_log_snapshot(), json!({ "entries": [] }));

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
    assert_eq!(proxy.get_log_snapshot(), json!({ "entries": [] }));
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
    assert_eq!(
        proxy.get_log_snapshot()["entries"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
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
                "locations": [{ "line": 6, "column": 11 }],
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
        proxy.get_log_snapshot()["entries"]
            .as_array()
            .unwrap()
            .len(),
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
        "# RustWebhookLocalRuntime\nmutation { pubSubWebhookSubscriptionCreate(topic: SHOP_UPDATE, webhookSubscription: { pubSubProject: \"valid-project\", pubSubTopic: \"topic-1\" }) { webhookSubscription { id topic uri endpoint { __typename ... on WebhookPubSubEndpoint { pubSubProject pubSubTopic } } } userErrors { field message } } }",
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
        "# RustWebhookLocalRuntime\nmutation { eventBridgeWebhookSubscriptionCreate(topic: SHOP_UPDATE, webhookSubscription: { arn: \"arn:aws:events:us-east-1::event-source/aws.partner/shopify.com/347082227713/source\" }) { webhookSubscription { id topic uri endpoint { __typename ... on WebhookEventBridgeEndpoint { arn } } } userErrors { field message } } }",
        json!({}),
    ));
    assert_eq!(eventbridge.status, 200);
    assert_eq!(
        eventbridge.body["data"]["eventBridgeWebhookSubscriptionCreate"]["webhookSubscription"]
            ["endpoint"],
        json!({
            "__typename": "WebhookEventBridgeEndpoint",
            "arn": "arn:aws:events:us-east-1::event-source/aws.partner/shopify.com/347082227713/source"
        })
    );

    let read = proxy.process_request(json_graphql_request(
        "# RustWebhookLocalRuntime\nquery($id: ID!) { webhookSubscription(id: $id) { id uri endpoint { __typename ... on WebhookPubSubEndpoint { pubSubProject pubSubTopic } } } }",
        json!({ "id": pubsub_id }),
    ));
    assert_eq!(
        read.body["data"]["webhookSubscription"]["endpoint"],
        json!({
            "__typename": "WebhookPubSubEndpoint",
            "pubSubProject": "valid-project",
            "pubSubTopic": "topic-1"
        })
    );
}

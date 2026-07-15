use super::common::*;

#[test]
fn operation_name_selects_query_and_does_not_execute_other_operations() {
    let mut proxy = snapshot_proxy();
    let query = r#"
        query ValidButUnselectedFirst($id: ID!) {
          product(id: $id) {
            id
          }
        }

        query SelectedSecond($first: Int = 1) {
          products(first: $first) {
            nodes { id title }
          }
        }
    "#;

    let response = proxy.process_request(graphql_request(
        "POST",
        &json!({
            "query": query,
            "operationName": "SelectedSecond",
            "variables": {}
        })
        .to_string(),
    ));

    assert_eq!(response.status, 200);
    assert_eq!(response.body.get("errors"), None);
    assert!(response.body["data"].get("products").is_some());
    assert!(response.body["data"].get("product").is_none());
}

#[test]
fn missing_and_unknown_operation_name_return_graphql_errors_without_execution() {
    let mut proxy = snapshot_proxy();
    let query = r#"
        mutation First {
          productCreate(product: { title: "Should not stage" }) {
            product { id title }
            userErrors { field message }
          }
        }

        mutation Second {
          productCreate(product: { title: "Also should not stage" }) {
            product { id title }
            userErrors { field message }
          }
        }
    "#;

    let missing = proxy.process_request(graphql_request(
        "POST",
        &json!({ "query": query, "variables": {} }).to_string(),
    ));
    assert_eq!(missing.status, 200);
    assert_eq!(
        missing.body,
        json!({ "errors": [{ "message": "An operation name is required" }] })
    );

    let unknown = proxy.process_request(graphql_request(
        "POST",
        &json!({
            "query": query,
            "operationName": "Missing",
            "variables": {}
        })
        .to_string(),
    ));
    assert_eq!(unknown.status, 200);
    assert_eq!(
        unknown.body,
        json!({ "errors": [{ "message": "No operation named \"Missing\"" }] })
    );

    assert_eq!(log_snapshot(&proxy)["entries"], json!([]));
}

#[test]
fn operation_name_selects_mutation_logs_selected_root_and_preserves_raw_body_for_commit() {
    let replayed = Arc::new(Mutex::new(Vec::new()));
    let replayed_for_transport = Arc::clone(&replayed);
    let mut proxy = snapshot_proxy().with_commit_transport(move |request| {
        replayed_for_transport.lock().unwrap().push(request.body);
        Response {
            status: 200,
            headers: Default::default(),
            body: json!({ "data": { "productCreate": { "product": { "id": "gid://shopify/Product/authoritative" } } } }),
        }
    });
    let query = r#"
        mutation First {
          productCreate(product: { title: "Wrong title" }) {
            product { id title }
            userErrors { field message }
          }
        }

        mutation Second {
          productCreate(product: { title: "Right title" }) {
            product { id title }
            userErrors { field message }
          }
        }
    "#;
    let body = json!({
        "query": query,
        "operationName": "Second",
        "variables": {}
    })
    .to_string();

    let response = proxy.process_request(graphql_request("POST", &body));

    assert_eq!(response.status, 200);
    assert_eq!(
        response.body["data"]["productCreate"]["product"]["title"],
        json!("Right title")
    );

    let log = log_snapshot(&proxy);
    assert_eq!(log["entries"].as_array().unwrap().len(), 1);
    let entry = &log["entries"][0];
    assert_eq!(entry["interpreted"]["rootFields"], json!(["productCreate"]));
    assert_eq!(
        entry["interpreted"]["primaryRootField"],
        json!("productCreate")
    );
    assert!(entry["query"]
        .as_str()
        .unwrap_or_default()
        .contains("Second"));
    assert!(!entry["query"]
        .as_str()
        .unwrap_or_default()
        .contains("Wrong title"));
    assert_eq!(
        serde_json::from_str::<Value>(entry["rawBody"].as_str().unwrap()).unwrap(),
        serde_json::from_str::<Value>(&body).unwrap()
    );

    let commit = proxy.process_request(request_with_body("POST", "/__meta/commit", ""));
    assert_eq!(commit.status, 200);
    let replayed = replayed.lock().unwrap();
    assert_eq!(replayed.len(), 1);
    assert_eq!(
        serde_json::from_str::<Value>(&replayed[0]).unwrap(),
        serde_json::from_str::<Value>(&body).unwrap()
    );
}

#[test]
fn cost_bearing_upstream_responses_preserve_omitted_selected_nested_fields() {
    let upstream_body = json!({
        "data": {
            "orders": {
                "nodes": [{
                    "id": "gid://shopify/Order/omitted-note",
                    "tags": ["authoritative"]
                }]
            }
        },
        "extensions": {
            "cost": {
                "requestedQueryCost": 2,
                "actualQueryCost": 2
            }
        }
    });
    let mut proxy = configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport({
        let upstream_body = upstream_body.clone();
        move |_| Response {
            status: 200,
            headers: Default::default(),
            body: upstream_body.clone(),
        }
    });

    let response = proxy.process_request(json_graphql_request(
        r#"
        query AuthoritativeOrderOmission {
          orders(first: 1) {
            nodes { id note tags }
          }
        }
        "#,
        json!({}),
    ));

    assert_eq!(response.status, 200);
    assert_eq!(response.body, upstream_body);

    let mut untrusted =
        configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport(|_| Response {
            status: 200,
            headers: Default::default(),
            body: json!({
                "data": {
                    "orders": {
                        "nodes": [{
                            "id": "gid://shopify/Order/local-gap",
                            "tags": []
                        }]
                    }
                }
            }),
        });
    let response = untrusted.process_request(json_graphql_request(
        r#"query UntrustedOrderOmission { orders(first: 1) { nodes { id note tags } } }"#,
        json!({}),
    ));
    assert!(response.body["errors"].as_array().is_some_and(|errors| {
        errors
            .iter()
            .any(|error| error["message"] == json!("Local resolver did not implement `Order.note`"))
    }));
}

#[test]
fn variable_defaults_apply_for_scalar_input_and_list_while_null_stays_explicit() {
    let mut proxy = snapshot_proxy();

    let input_default = proxy.process_request(graphql_request(
        "POST",
        &json!({
            "query": r#"
                mutation InputDefault($product: ProductCreateInput! = { title: "Input default" }) {
                  productCreate(product: $product) {
                    product { id title }
                    userErrors { field message }
                  }
                }
            "#,
            "variables": {}
        })
        .to_string(),
    ));
    assert_eq!(input_default.status, 200);
    assert_eq!(
        input_default.body["data"]["productCreate"]["product"]["title"],
        json!("Input default")
    );
    assert_eq!(
        input_default.body["data"]["productCreate"]["userErrors"],
        json!([])
    );

    let scalar_and_list_defaults = proxy.process_request(graphql_request(
        "POST",
        &json!({
            "query": r#"
                mutation ScalarAndListDefaults(
                  $title: String = "Scalar default",
                  $tags: [String!] = ["red", "blue"]
                ) {
                  productCreate(product: { title: $title, tags: $tags }) {
                    product { id title tags }
                    userErrors { field message }
                  }
                }
            "#,
            "variables": {}
        })
        .to_string(),
    ));
    assert_eq!(scalar_and_list_defaults.status, 200);
    let product = &scalar_and_list_defaults.body["data"]["productCreate"]["product"];
    assert_eq!(product["title"], json!("Scalar default"));
    assert_eq!(product["tags"], json!(["blue", "red"]));

    let explicit_null = proxy.process_request(graphql_request(
        "POST",
        &json!({
            "query": r#"
                mutation InputDefault($product: ProductCreateInput! = { title: "Input default" }) {
                  productCreate(product: $product) {
                    product { id title }
                    userErrors { field message }
                  }
                }
            "#,
            "variables": { "product": null }
        })
        .to_string(),
    ));
    assert_eq!(explicit_null.status, 200);
    assert_eq!(explicit_null.body["data"], Value::Null);
    assert_eq!(
        explicit_null.body["errors"][0]["message"],
        json!("Variable $product of type ProductCreateInput! was provided invalid value")
    );

    assert_eq!(log_snapshot(&proxy)["entries"].as_array().unwrap().len(), 2);
}

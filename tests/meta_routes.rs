use pretty_assertions::assert_eq;
use serde_json::json;
use shopify_draft_proxy::proxy::{Config, DraftProxy, ProductRecord, ReadMode, Request};

fn snapshot_proxy() -> DraftProxy {
    DraftProxy::new(Config {
        read_mode: ReadMode::Snapshot,
        unsupported_mutation_mode: None,
        bulk_operation_run_mutation_max_input_file_size_bytes: None,
        port: 0,
        shopify_admin_origin: "https://shopify.com".to_string(),
        snapshot_path: None,
    })
}

fn request(method: &str, path: &str) -> Request {
    request_with_body(method, path, "")
}

fn request_with_body(method: &str, path: &str, body: &str) -> Request {
    Request {
        method: method.to_string(),
        path: path.to_string(),
        headers: Default::default(),
        body: body.to_string(),
    }
}

fn graphql_request(body: &str) -> Request {
    request_with_body("POST", "/admin/api/2026-04/graphql.json", body)
}

#[test]
fn serves_meta_route_response_shapes() {
    let mut proxy = snapshot_proxy();

    let health = proxy.process_request(request("GET", "/__meta/health"));
    assert_eq!(health.status, 200);
    assert_eq!(
        health.body,
        json!({ "ok": true, "message": "shopify-draft-proxy is running" })
    );

    let config = proxy.process_request(request("GET", "/__meta/config"));
    assert_eq!(config.status, 200);
    assert_eq!(
        config.body,
        json!({
            "runtime": {
                "readMode": "snapshot",
                "unsupportedMutationMode": "passthrough",
                "bulkOperationRunMutationMaxInputFileSizeBytes": 104857600
            },
            "proxy": { "port": 0, "shopifyAdminOrigin": "https://shopify.com" },
            "snapshot": { "enabled": false, "path": null }
        })
    );

    let log = proxy.process_request(request("GET", "/__meta/log"));
    assert_eq!(log.status, 200);
    assert_eq!(log.body, json!({ "entries": [] }));

    let state = proxy.process_request(request("GET", "/__meta/state"));
    assert_eq!(state.status, 200);
    assert!(state.body.get("baseState").is_some());
    assert!(state.body.get("stagedState").is_some());

    let reset = proxy.process_request(request("POST", "/__meta/reset"));
    assert_eq!(reset.status, 200);
    assert_eq!(reset.body, json!({ "ok": true, "message": "state reset" }));
}

#[test]
fn records_supported_product_mutations_in_meta_log_with_raw_replay_inputs() {
    let mut proxy = snapshot_proxy().with_base_products(vec![ProductRecord {
        id: "gid://shopify/Product/base".to_string(),
        title: "Base product".to_string(),
        handle: "base-product".to_string(),
        status: "ACTIVE".to_string(),
        description_html: String::new(),
        vendor: String::new(),
        product_type: String::new(),
        tags: Vec::new(),
    }]);

    let create_query =
        "mutation { productCreate(product: { title: \"Created product\" }) { product { id } } }";
    let create = proxy.process_request(graphql_request(
        &json!({ "query": create_query }).to_string(),
    ));
    assert_eq!(create.status, 200);

    let update_query = "mutation { productUpdate(product: { id: \"gid://shopify/Product/base\", title: \"Updated product\" }) { product { id } } }";
    let update = proxy.process_request(graphql_request(
        &json!({ "query": update_query, "variables": { "unused": true } }).to_string(),
    ));
    assert_eq!(update.status, 200);

    let log = proxy.process_request(request("GET", "/__meta/log"));
    assert_eq!(log.status, 200);
    assert_eq!(
        log.body,
        json!({
            "entries": [
                {
                    "id": "log-1",
                    "operationName": null,
                    "path": "/admin/api/2026-04/graphql.json",
                    "query": create_query,
                    "variables": {},
                    "stagedResourceIds": ["gid://shopify/Product/1?shopify-draft-proxy=synthetic"],
                    "status": "staged",
                    "interpreted": {
                        "operationType": "mutation",
                        "rootFields": ["productCreate"],
                        "primaryRootField": "productCreate"
                    }
                },
                {
                    "id": "log-2",
                    "operationName": null,
                    "path": "/admin/api/2026-04/graphql.json",
                    "query": update_query,
                    "variables": {"unused": true},
                    "stagedResourceIds": ["gid://shopify/Product/base"],
                    "status": "staged",
                    "interpreted": {
                        "operationType": "mutation",
                        "rootFields": ["productUpdate"],
                        "primaryRootField": "productUpdate"
                    }
                }
            ]
        })
    );
}

#[test]
fn meta_reset_clears_log_and_staged_product_overlay() {
    let mut proxy = snapshot_proxy().with_base_products(vec![ProductRecord {
        id: "gid://shopify/Product/base".to_string(),
        title: "Base product".to_string(),
        handle: "base-product".to_string(),
        status: "ACTIVE".to_string(),
        description_html: String::new(),
        vendor: String::new(),
        product_type: String::new(),
        tags: Vec::new(),
    }]);

    let update = proxy.process_request(graphql_request(
        r#"{"query":"mutation { productUpdate(product: { id: \"gid://shopify/Product/base\", title: \"Updated product\" }) { product { id } } }"}"#,
    ));
    assert_eq!(update.status, 200);

    let reset = proxy.process_request(request("POST", "/__meta/reset"));
    assert_eq!(reset.status, 200);

    let log = proxy.process_request(request("GET", "/__meta/log"));
    assert_eq!(log.body, json!({ "entries": [] }));

    let read_back = proxy.process_request(graphql_request(
        r#"{"query":"query { product(id: \"gid://shopify/Product/base\") { title } }"}"#,
    ));
    assert_eq!(
        read_back.body,
        json!({ "data": { "product": { "title": "Base product" } } })
    );
}

#[test]
fn rejects_missing_paths_and_wrong_methods_with_existing_error_envelopes() {
    let mut proxy = snapshot_proxy();

    let missing = proxy.process_request(request("GET", "/missing"));
    assert_eq!(missing.status, 404);
    assert_eq!(
        missing.body,
        json!({ "errors": [{ "message": "Not found" }] })
    );

    let wrong_method = proxy.process_request(request("POST", "/__meta/health"));
    assert_eq!(wrong_method.status, 405);
    assert_eq!(
        wrong_method.body,
        json!({ "errors": [{ "message": "Method not allowed" }] })
    );
}

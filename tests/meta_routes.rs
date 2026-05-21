use std::sync::{Arc, Mutex};

use pretty_assertions::assert_eq;
use serde_json::{json, Value};
use shopify_draft_proxy::proxy::{Config, DraftProxy, ProductRecord, ReadMode, Request, Response};

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

fn ok_transport_response(body: Value) -> Response {
    Response {
        status: 200,
        headers: Default::default(),
        body,
    }
}

fn error_transport_response(status: u16, body: Value) -> Response {
    Response {
        status,
        headers: Default::default(),
        body,
    }
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
        template_suffix: String::new(),
        seo_title: String::new(),
        seo_description: String::new(),
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
fn meta_state_exposes_staged_products_saved_searches_and_deleted_ids() {
    let mut proxy = snapshot_proxy().with_base_products(vec![ProductRecord {
        id: "gid://shopify/Product/base".to_string(),
        title: "Base product".to_string(),
        handle: "base-product".to_string(),
        status: "ACTIVE".to_string(),
        description_html: "<p>Base</p>".to_string(),
        vendor: "Base vendor".to_string(),
        product_type: "Base type".to_string(),
        tags: vec!["base".to_string()],
        template_suffix: String::new(),
        seo_title: String::new(),
        seo_description: String::new(),
    }]);

    let create_product = proxy.process_request(graphql_request(
        &json!({ "query": "mutation { productCreate(product: { title: \"Created product\", handle: \"created-product\", tags: [\"new\"] }) { product { id } } }" }).to_string(),
    ));
    assert_eq!(create_product.status, 200);

    let delete_base = proxy.process_request(graphql_request(
        &json!({ "query": "mutation { productDelete(product: { id: \"gid://shopify/Product/base\" }) { deletedProductId } }" }).to_string(),
    ));
    assert_eq!(delete_base.status, 200);

    let create_saved_search = proxy.process_request(graphql_request(
        &json!({ "query": "mutation { savedSearchCreate(input: { name: \"Promo products\", query: \"tag:promo\", resourceType: PRODUCT }) { savedSearch { id } } }" }).to_string(),
    ));
    assert_eq!(create_saved_search.status, 200);

    let state = proxy.process_request(request("GET", "/__meta/state"));
    assert_eq!(state.status, 200);
    assert_eq!(
        state.body,
        json!({
            "baseState": {
                "products": {
                    "gid://shopify/Product/base": {
                        "id": "gid://shopify/Product/base",
                        "title": "Base product",
                        "handle": "base-product",
                        "status": "ACTIVE",
                        "descriptionHtml": "<p>Base</p>",
                        "vendor": "Base vendor",
                        "productType": "Base type",
                        "tags": ["base"],
                        "templateSuffix": "",
                        "seo": { "title": "", "description": "" }
                    }
                },
                "savedSearches": {}
            },
            "stagedState": {
                "products": {
                    "gid://shopify/Product/1?shopify-draft-proxy=synthetic": {
                        "id": "gid://shopify/Product/1?shopify-draft-proxy=synthetic",
                        "title": "Created product",
                        "handle": "created-product",
                        "status": "ACTIVE",
                        "descriptionHtml": "",
                        "vendor": "",
                        "productType": "",
                        "tags": ["new"],
                        "templateSuffix": "",
                        "seo": { "title": "", "description": "" }
                    }
                },
                "deletedProductIds": ["gid://shopify/Product/base"],
                "shippingPackages": {},
                "deletedShippingPackageIds": {},
                "delegatedAccessTokens": {},
                "customers": {},
                "deletedCustomerIds": [],
                "savedSearches": {
                    "gid://shopify/SavedSearch/2?shopify-draft-proxy=synthetic": {
                        "id": "gid://shopify/SavedSearch/2?shopify-draft-proxy=synthetic",
                        "name": "Promo products",
                        "query": "tag:promo",
                        "resourceType": "PRODUCT"
                    }
                }
            }
        })
    );
}

#[test]
fn meta_dump_and_restore_round_trip_staged_rust_state() {
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
    let create_product_query =
        "mutation { productCreate(product: { title: \"Created product\", handle: \"created-product\" }) { product { id } } }";
    assert_eq!(
        proxy
            .process_request(graphql_request(
                &json!({ "query": create_product_query }).to_string()
            ))
            .status,
        200
    );
    let create_saved_search_query = "mutation { savedSearchCreate(input: { name: \"Promo products\", query: \"tag:promo\", resourceType: PRODUCT }) { savedSearch { id } } }";
    assert_eq!(
        proxy
            .process_request(graphql_request(
                &json!({ "query": create_saved_search_query }).to_string()
            ))
            .status,
        200
    );

    let dump = proxy.process_request(request_with_body(
        "POST",
        "/__meta/dump",
        &json!({ "createdAt": "2026-05-21T00:00:00.000Z" }).to_string(),
    ));
    assert_eq!(dump.status, 200);
    assert_eq!(
        dump.body["schema"],
        json!("shopify-draft-proxy-rust-state/v1")
    );
    assert_eq!(dump.body["createdAt"], json!("2026-05-21T00:00:00.000Z"));
    assert_eq!(dump.body["log"]["entries"].as_array().unwrap().len(), 2);

    let mut restored = snapshot_proxy();
    let restore = restored.process_request(request_with_body(
        "POST",
        "/__meta/restore",
        &dump.body.to_string(),
    ));
    assert_eq!(restore.status, 200);
    assert_eq!(
        restore.body,
        json!({ "ok": true, "message": "state restored" })
    );

    let restored_product_read = restored.process_request(graphql_request(
        &json!({ "query": "{ productByIdentifier(identifier: { handle: \"created-product\" }) { id title handle } }" }).to_string(),
    ));
    assert_eq!(restored_product_read.status, 200);
    assert_eq!(
        restored_product_read.body,
        json!({
            "data": {
                "productByIdentifier": {
                    "id": "gid://shopify/Product/1?shopify-draft-proxy=synthetic",
                    "title": "Created product",
                    "handle": "created-product"
                }
            }
        })
    );

    let restored_saved_search_read = restored.process_request(graphql_request(
        &json!({ "query": "{ productSavedSearches(query: \"Promo\") { nodes { id name query resourceType } } }" }).to_string(),
    ));
    assert_eq!(restored_saved_search_read.status, 200);
    assert_eq!(
        restored_saved_search_read.body,
        json!({
            "data": {
                "productSavedSearches": {
                    "nodes": [{
                        "id": "gid://shopify/SavedSearch/2?shopify-draft-proxy=synthetic",
                        "name": "Promo products",
                        "query": "tag:promo",
                        "resourceType": "PRODUCT"
                    }]
                }
            }
        })
    );

    let restored_log = restored.process_request(request("GET", "/__meta/log"));
    assert_eq!(restored_log.body, dump.body["log"]);

    let next_create = restored.process_request(graphql_request(
        &json!({ "query": "mutation { productCreate(product: { title: \"Next product\" }) { product { id } } }" }).to_string(),
    ));
    assert_eq!(
        next_create.body["data"]["productCreate"]["product"]["id"],
        json!("gid://shopify/Product/3?shopify-draft-proxy=synthetic")
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
        template_suffix: String::new(),
        seo_title: String::new(),
        seo_description: String::new(),
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
fn commit_replays_staged_mutations_in_order_and_marks_entries_committed() {
    let replayed = Arc::new(Mutex::new(Vec::<Request>::new()));
    let replayed_for_transport = Arc::clone(&replayed);
    let mut proxy = snapshot_proxy()
        .with_base_products(vec![ProductRecord {
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
        }])
        .with_commit_transport(move |request| {
            replayed_for_transport.lock().unwrap().push(request);
            ok_transport_response(json!({ "data": { "ok": true } }))
        });

    let create_query =
        "mutation { productCreate(product: { title: \"Created product\" }) { product { id } } }";
    let update_query = "mutation { productUpdate(product: { id: \"gid://shopify/Product/base\", title: \"Updated product\" }) { product { id } } }";
    assert_eq!(
        proxy
            .process_request(graphql_request(
                &json!({ "query": create_query }).to_string()
            ))
            .status,
        200
    );
    assert_eq!(
        proxy
            .process_request(graphql_request(
                &json!({ "query": update_query, "variables": { "title": "Updated product" } })
                    .to_string(),
            ))
            .status,
        200
    );

    let commit = proxy.process_request(request("POST", "/__meta/commit"));
    assert_eq!(commit.status, 200);
    assert_eq!(
        commit.body,
        json!({ "ok": true, "committed": 2, "failed": 0 })
    );

    let replayed = replayed.lock().unwrap();
    assert_eq!(replayed.len(), 2);
    assert_eq!(replayed[0].method, "POST");
    assert_eq!(replayed[0].path, "/admin/api/2026-04/graphql.json");
    assert_eq!(
        serde_json::from_str::<Value>(&replayed[0].body).unwrap(),
        json!({ "query": create_query, "variables": {} })
    );
    assert_eq!(replayed[1].path, "/admin/api/2026-04/graphql.json");
    assert_eq!(
        serde_json::from_str::<Value>(&replayed[1].body).unwrap(),
        json!({ "query": update_query, "variables": { "title": "Updated product" } })
    );

    let log = proxy.process_request(request("GET", "/__meta/log"));
    assert_eq!(log.body["entries"][0]["status"], json!("committed"));
    assert_eq!(log.body["entries"][1]["status"], json!("committed"));
}

#[test]
fn commit_stops_on_first_upstream_failure_and_persists_failed_status() {
    let attempts = Arc::new(Mutex::new(0usize));
    let attempts_for_transport = Arc::clone(&attempts);
    let mut proxy = snapshot_proxy().with_commit_transport(move |_request| {
        let mut attempts = attempts_for_transport.lock().unwrap();
        *attempts += 1;
        if *attempts == 1 {
            error_transport_response(500, json!({ "errors": [{ "message": "upstream failed" }] }))
        } else {
            ok_transport_response(json!({ "data": { "ok": true } }))
        }
    });

    let first_query =
        "mutation { productCreate(product: { title: \"First product\" }) { product { id } } }";
    let second_query =
        "mutation { productCreate(product: { title: \"Second product\" }) { product { id } } }";
    assert_eq!(
        proxy
            .process_request(graphql_request(
                &json!({ "query": first_query }).to_string()
            ))
            .status,
        200
    );
    assert_eq!(
        proxy
            .process_request(graphql_request(
                &json!({ "query": second_query }).to_string()
            ))
            .status,
        200
    );

    let commit = proxy.process_request(request("POST", "/__meta/commit"));
    assert_eq!(commit.status, 502);
    assert_eq!(
        commit.body,
        json!({
            "ok": false,
            "committed": 0,
            "failed": 1,
            "error": "Upstream commit failed for log-1 with status 500"
        })
    );
    assert_eq!(*attempts.lock().unwrap(), 1);

    let log = proxy.process_request(request("GET", "/__meta/log"));
    assert_eq!(log.body["entries"][0]["status"], json!("failed"));
    assert_eq!(log.body["entries"][1]["status"], json!("staged"));
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

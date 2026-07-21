use std::sync::{Arc, Mutex};

use pretty_assertions::assert_eq;
use serde_json::{json, Value};
use shopify_draft_proxy::proxy::{
    Config, DraftProxy, ProductRecord, ReadMode, Request, Response, UnsupportedMutationMode,
};

const COMMIT_GRAPHQL_PATH: &str = "/admin/api/2026-04/graphql.json";
const SYNTHETIC_PRODUCT_VARIANT_ONE: &str =
    "gid://shopify/ProductVariant/1?shopify-draft-proxy=synthetic";
const SYNTHETIC_PRODUCT_VARIANT_TWO: &str =
    "gid://shopify/ProductVariant/2?shopify-draft-proxy=synthetic";
const SYNTHETIC_SAVED_SEARCH_ONE: &str =
    "gid://shopify/SavedSearch/1?shopify-draft-proxy=synthetic";
const SYNTHETIC_SAVED_SEARCH_TWO: &str =
    "gid://shopify/SavedSearch/2?shopify-draft-proxy=synthetic";
const CANONICAL_SAVED_SEARCH_ONE: &str = "gid://shopify/SavedSearch/1";
const CANONICAL_SAVED_SEARCH_TWO: &str = "gid://shopify/SavedSearch/2";
const AUTHORITATIVE_SAVED_SEARCH_ONE: &str = "gid://shopify/SavedSearch/12345";
const AUTHORITATIVE_SAVED_SEARCH_TWO: &str = "gid://shopify/SavedSearch/67890";

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

fn request_with_headers(
    method: &str,
    path: &str,
    headers: impl IntoIterator<Item = (&'static str, &'static str)>,
) -> Request {
    Request {
        method: method.to_string(),
        path: path.to_string(),
        headers: headers
            .into_iter()
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect(),
        body: String::new(),
    }
}

fn config_snapshot(proxy: &DraftProxy) -> Value {
    meta_snapshot(proxy, "/__meta/config")
}

fn log_snapshot(proxy: &DraftProxy) -> Value {
    meta_snapshot(proxy, "/__meta/log")
}

fn state_snapshot(proxy: &DraftProxy) -> Value {
    meta_snapshot(proxy, "/__meta/state")
}

fn meta_snapshot(proxy: &DraftProxy, path: &str) -> Value {
    let mut proxy = proxy.clone();
    let response = proxy.process_request(request("GET", path));
    assert_eq!(response.status, 200);
    response.body
}

fn graphql_request(body: &str) -> Request {
    request_with_body("POST", "/admin/api/2026-04/graphql.json", body)
}

fn base_product() -> ProductRecord {
    ProductRecord {
        id: "gid://shopify/Product/base".to_string(),
        created_at: "2024-01-01T00:00:00.000Z".to_string(),
        updated_at: "2024-01-01T00:00:00.000Z".to_string(),
        title: "Base product".to_string(),
        handle: "base-product".to_string(),
        status: "ACTIVE".to_string(),
        description_html: String::new(),
        vendor: String::new(),
        product_type: String::new(),
        tags: vec!["base".to_string()],
        template_suffix: String::new(),
        seo_title: String::new(),
        seo_description: String::new(),
        ..ProductRecord::default()
    }
}

fn expected_local_staged_log(
    id: &str,
    query: &str,
    variables: Value,
    root_field: &str,
    domain: &str,
    staged_resource_ids: Value,
) -> Value {
    let raw_body = expected_raw_graphql_body(query, &variables);
    json!({
        "id": id,
        "operationName": null,
        "path": "/admin/api/2026-04/graphql.json",
        "query": query,
        "variables": variables,
        "rawBody": raw_body,
        "stagedResourceIds": staged_resource_ids,
        "status": "staged",
        "interpreted": {
            "operationType": "mutation",
            "operationName": root_field,
            "rootFields": [root_field],
            "primaryRootField": root_field,
            "capability": {
                "operationName": root_field,
                "domain": domain,
                "execution": "stage-locally"
            }
        },
        "notes": "Supported mutation staged locally; commit replays the original raw mutation."
    })
}

fn expected_raw_graphql_body(query: &str, variables: &Value) -> String {
    if variables == &json!({}) {
        json!({ "query": query }).to_string()
    } else {
        json!({ "query": query, "variables": variables }).to_string()
    }
}

fn assert_single_local_staged_log(
    proxy: &DraftProxy,
    query: &str,
    variables: Value,
    root_field: &str,
    domain: &str,
    staged_resource_ids: Value,
) {
    assert_eq!(
        log_snapshot(proxy),
        json!({
            "entries": [
                expected_local_staged_log(
                    "log-1",
                    query,
                    variables,
                    root_field,
                    domain,
                    staged_resource_ids
                )
            ]
        })
    );
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

fn restore_log_entries(proxy: &mut DraftProxy, entries: Value) {
    let dump = proxy.process_request(request_with_body("POST", "/__meta/dump", ""));
    assert_eq!(dump.status, 200);

    let mut restored = dump.body;
    restored["log"]["entries"] = entries;
    restored["nextSyntheticId"] = json!(10);

    let restore = proxy.process_request(request_with_body(
        "POST",
        "/__meta/restore",
        &restored.to_string(),
    ));
    assert_eq!(restore.status, 200, "restore body: {}", restore.body);
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
fn draft_proxy_route_and_snapshot_helpers_match_current_behavior() {
    let mut default_proxy = DraftProxy::new(Config::default());
    let expected_default_config = json!({
        "runtime": {
            "readMode": "snapshot",
            "unsupportedMutationMode": "passthrough",
            "bulkOperationRunMutationMaxInputFileSizeBytes": 104857600
        },
        "proxy": { "port": 4000, "shopifyAdminOrigin": "https://shopify.com" },
        "snapshot": { "enabled": false, "path": null }
    });
    assert_eq!(config_snapshot(&default_proxy), expected_default_config);
    assert_eq!(
        default_proxy
            .process_request(request("GET", "/__meta/config"))
            .body,
        expected_default_config
    );

    let snapshot_proxy = DraftProxy::new(Config {
        read_mode: ReadMode::LiveHybrid,
        unsupported_mutation_mode: Some(UnsupportedMutationMode::Passthrough),
        bulk_operation_run_mutation_max_input_file_size_bytes: Some(104_857_600),
        port: 4001,
        shopify_admin_origin: "https://example.myshopify.com".to_string(),
        snapshot_path: Some("/tmp/snap.json".to_string()),
    });
    assert_eq!(
        config_snapshot(&snapshot_proxy),
        json!({
            "runtime": {
                "readMode": "live-hybrid",
                "unsupportedMutationMode": "passthrough",
                "bulkOperationRunMutationMaxInputFileSizeBytes": 104857600
            },
            "proxy": { "port": 4001, "shopifyAdminOrigin": "https://example.myshopify.com" },
            "snapshot": { "enabled": true, "path": "/tmp/snap.json" }
        })
    );

    let log_json = log_snapshot(&default_proxy);
    assert_eq!(log_json, json!({ "entries": [] }));
    assert_eq!(
        default_proxy
            .process_request(request("GET", "/__meta/log"))
            .body,
        log_json
    );

    let state_json = state_snapshot(&default_proxy);
    assert_eq!(
        default_proxy
            .process_request(request("GET", "/__meta/state"))
            .body,
        state_json
    );

    let mut helper_proxy = DraftProxy::new(Config::default());
    let create = helper_proxy.process_request(graphql_request(
        &json!({ "query": "mutation { productCreate(product: { title: \"Snapshot helper product\" }) { product { id } userErrors { message } } }" }).to_string(),
    ));
    assert_eq!(create.status, 200);
    assert_eq!(
        helper_proxy
            .process_request(request("GET", "/__meta/log"))
            .body,
        log_snapshot(&helper_proxy)
    );
    assert_eq!(
        helper_proxy
            .process_request(request("GET", "/__meta/state"))
            .body,
        state_snapshot(&helper_proxy)
    );

    let route_guards = [
        ("POST", "/__meta/health", 405),
        ("POST", "/__meta/config", 405),
        ("GET", "/__meta/reset", 405),
        ("GET", "/__meta/commit", 405),
        ("GET", "/totally-unknown", 404),
        ("GET", "/admin/api/2026-04/graphql.json", 405),
    ];
    for (method, path, expected_status) in route_guards {
        let response = default_proxy.process_request(request(method, path));
        assert_eq!(
            response.status, expected_status,
            "{method} {path} should keep old draft_proxy route status"
        );
    }

    assert_eq!(
        default_proxy
            .process_request(request_with_body(
                "POST",
                "/admin/api/2026-04/graphql.json",
                "not-json"
            ))
            .status,
        400
    );
    assert_eq!(
        default_proxy
            .process_request(request_with_body(
                "POST",
                "/admin/api/banana/graphql.json",
                &json!({ "query": "{ shop { id } }" }).to_string()
            ))
            .status,
        404
    );
    assert_eq!(
        default_proxy
            .process_request(request("POST", "/__meta/reset"))
            .body,
        json!({ "ok": true, "message": "state reset" })
    );

    let empty_commit = default_proxy.process_request(request("POST", "/__meta/commit"));
    assert_eq!(empty_commit.status, 200);
    assert_eq!(
        empty_commit.body,
        json!({ "ok": true, "committed": 0, "failed": 0, "stopIndex": null, "attempts": [] })
    );

    let dump = default_proxy.process_request(request_with_body(
        "POST",
        "/__meta/dump",
        &json!({ "createdAt": "2026-04-29T12:00:00.000Z" }).to_string(),
    ));
    assert_eq!(dump.status, 200);
    assert_eq!(
        dump.body["schema"],
        json!("shopify-draft-proxy-rust-state/v1")
    );
    assert_eq!(dump.body["createdAt"], json!("2026-04-29T12:00:00.000Z"));
    assert_eq!(dump.body["log"], json!({ "entries": [] }));
    assert_eq!(dump.body["nextSyntheticId"], json!(1));
    assert!(dump.body["state"]["baseState"].is_object());
    assert!(dump.body["state"]["stagedState"].is_object());
}

#[test]
fn records_supported_product_mutations_in_meta_log_with_raw_replay_inputs() {
    let mut proxy = snapshot_proxy().with_base_products(vec![base_product()]);

    let create_query = "mutation CreateForCommit { productCreate(product: { title: \"Created product\" }) { product { id } } }";
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
                expected_local_staged_log(
                    "log-1",
                    create_query,
                    json!({}),
                    "productCreate",
                    "products",
                    json!([
                        "gid://shopify/Product/1?shopify-draft-proxy=synthetic",
                        "gid://shopify/ProductVariant/2?shopify-draft-proxy=synthetic"
                    ])
                ),
                expected_local_staged_log(
                    "log-2",
                    update_query,
                    json!({"unused": true}),
                    "productUpdate",
                    "products",
                    json!(["gid://shopify/Product/base"])
                )
            ]
        })
    );
}

#[test]
fn product_mutation_outcomes_finalize_exactly_one_log_draft() {
    let create_query = "mutation { productCreate(product: { title: \"Created product\" }) { product { id title } userErrors { field message } } }";
    let mut create_proxy = snapshot_proxy();
    let create = create_proxy.process_request(graphql_request(
        &json!({ "query": create_query }).to_string(),
    ));
    assert_eq!(create.status, 200);
    assert_eq!(
        create.body["data"]["productCreate"]["product"]["title"],
        json!("Created product")
    );
    assert_single_local_staged_log(
        &create_proxy,
        create_query,
        json!({}),
        "productCreate",
        "products",
        json!([
            "gid://shopify/Product/1?shopify-draft-proxy=synthetic",
            "gid://shopify/ProductVariant/2?shopify-draft-proxy=synthetic"
        ]),
    );

    let update_query = "mutation { productUpdate(product: { id: \"gid://shopify/Product/base\", title: \"Updated product\" }) { product { id title } userErrors { field message } } }";
    let mut update_proxy = snapshot_proxy().with_base_products(vec![base_product()]);
    let update = update_proxy.process_request(graphql_request(
        &json!({ "query": update_query }).to_string(),
    ));
    assert_eq!(update.status, 200);
    assert_eq!(
        update.body["data"]["productUpdate"]["product"],
        json!({"id": "gid://shopify/Product/base", "title": "Updated product"})
    );
    assert_single_local_staged_log(
        &update_proxy,
        update_query,
        json!({}),
        "productUpdate",
        "products",
        json!(["gid://shopify/Product/base"]),
    );

    let delete_query = "mutation { productDelete(input: { id: \"gid://shopify/Product/base\" }) { deletedProductId userErrors { field message } } }";
    let mut delete_proxy = snapshot_proxy().with_base_products(vec![base_product()]);
    let delete = delete_proxy.process_request(graphql_request(
        &json!({ "query": delete_query }).to_string(),
    ));
    assert_eq!(delete.status, 200);
    assert_eq!(
        delete.body["data"]["productDelete"]["deletedProductId"],
        json!("gid://shopify/Product/base")
    );
    assert_single_local_staged_log(
        &delete_proxy,
        delete_query,
        json!({}),
        "productDelete",
        "products",
        json!(["gid://shopify/Product/base"]),
    );

    let status_query = "mutation { productChangeStatus(productId: \"gid://shopify/Product/base\", status: DRAFT) { product { id status } userErrors { field message } } }";
    let mut status_proxy = snapshot_proxy().with_base_products(vec![base_product()]);
    let status = status_proxy.process_request(graphql_request(
        &json!({ "query": status_query }).to_string(),
    ));
    assert_eq!(status.status, 200);
    assert_eq!(
        status.body["data"]["productChangeStatus"]["product"],
        json!({"id": "gid://shopify/Product/base", "status": "DRAFT"})
    );
    assert_single_local_staged_log(
        &status_proxy,
        status_query,
        json!({}),
        "productChangeStatus",
        "products",
        json!(["gid://shopify/Product/base"]),
    );

    let tags_query = "mutation { tagsAdd(id: \"gid://shopify/Product/base\", tags: [\"new\"]) { node { ... on Product { id tags } } userErrors { field message } } }";
    let mut tags_proxy = snapshot_proxy().with_base_products(vec![base_product()]);
    let tags =
        tags_proxy.process_request(graphql_request(&json!({ "query": tags_query }).to_string()));
    assert_eq!(tags.status, 200);
    assert_eq!(
        tags.body["data"]["tagsAdd"]["node"],
        json!({"id": "gid://shopify/Product/base", "tags": ["base", "new"]})
    );
    assert_single_local_staged_log(
        &tags_proxy,
        tags_query,
        json!({}),
        "tagsAdd",
        "products",
        json!(["gid://shopify/Product/base"]),
    );

    let tags_remove_query = "mutation { tagsRemove(id: \"gid://shopify/Product/base\", tags: [\"base\"]) { node { ... on Product { id tags } } userErrors { field message } } }";
    let mut tags_remove_proxy = snapshot_proxy().with_base_products(vec![base_product()]);
    let tags_remove = tags_remove_proxy.process_request(graphql_request(
        &json!({ "query": tags_remove_query }).to_string(),
    ));
    assert_eq!(tags_remove.status, 200);
    assert_eq!(
        tags_remove.body["data"]["tagsRemove"]["node"],
        json!({"id": "gid://shopify/Product/base", "tags": []})
    );
    assert_single_local_staged_log(
        &tags_remove_proxy,
        tags_remove_query,
        json!({}),
        "tagsRemove",
        "products",
        json!(["gid://shopify/Product/base"]),
    );

    let product_set_query = "mutation ProductDeleteAsyncSourceCreate($input: ProductSetInput!, $synchronous: Boolean!) { productSet(input: $input, synchronous: $synchronous) { product { id title status } userErrors { field message } } }";
    let product_set_variables = json!({
        "input": { "title": "Async delete source", "status": "DRAFT" },
        "synchronous": true
    });
    let mut product_set_proxy = snapshot_proxy();
    let product_set = product_set_proxy.process_request(graphql_request(
        &json!({ "query": product_set_query, "variables": product_set_variables.clone() })
            .to_string(),
    ));
    assert_eq!(product_set.status, 200);
    assert_eq!(
        product_set.body,
        json!({
            "data": {
                "productSet": {
                    "product": {
                        "id": "gid://shopify/Product/1?shopify-draft-proxy=synthetic",
                        "status": "DRAFT",
                        "title": "Async delete source"
                    },
                    "userErrors": []
                }
            }
        })
    );
    assert_single_local_staged_log(
        &product_set_proxy,
        product_set_query,
        product_set_variables,
        "productSet",
        "products",
        json!(["gid://shopify/Product/1?shopify-draft-proxy=synthetic"]),
    );
}

#[test]
fn multi_root_mutation_executes_serially_and_logs_the_original_operation_once() {
    let mut proxy = snapshot_proxy().with_base_products(vec![base_product()]);
    let query = r#"
        mutation SerialProductMutations($product: ProductUpdateInput!, $tags: [String!]!) {
          rename: productUpdate(product: $product) {
            product { id title tags }
            userErrors { field message }
          }
          tagIt: tagsAdd(id: "gid://shopify/Product/base", tags: $tags) {
            node { ... on Product { id title tags } }
            userErrors { field message }
          }
        }
    "#;
    let variables = json!({
        "product": {
            "id": "gid://shopify/Product/base",
            "title": "Renamed product"
        },
        "tags": ["new"]
    });

    let response = proxy.process_request(graphql_request(
        &json!({ "query": query, "variables": variables }).to_string(),
    ));

    assert_eq!(response.status, 200);
    assert_eq!(
        response.body["data"]["rename"]["product"],
        json!({
            "id": "gid://shopify/Product/base",
            "title": "Renamed product",
            "tags": ["base"]
        })
    );
    assert_eq!(
        response.body["data"]["tagIt"]["node"],
        json!({
            "id": "gid://shopify/Product/base",
            "title": "Renamed product",
            "tags": ["base", "new"]
        })
    );

    let log = log_snapshot(&proxy);
    let entries = log["entries"].as_array().expect("mutation log entries");
    assert_eq!(entries.len(), 1);
    assert_eq!(
        entries[0]["interpreted"]["rootFields"],
        json!(["productUpdate", "tagsAdd"])
    );
    assert_eq!(entries[0]["query"], json!(query));
    assert_eq!(entries[0]["status"], json!("staged"));
}

#[test]
fn mixed_supported_unsupported_mutation_is_rejected_without_writes() {
    let forwarded = Arc::new(Mutex::new(Vec::<Value>::new()));
    let captured = Arc::clone(&forwarded);
    let mut proxy = DraftProxy::new(Config {
        read_mode: ReadMode::LiveHybrid,
        unsupported_mutation_mode: Some(UnsupportedMutationMode::Passthrough),
        bulk_operation_run_mutation_max_input_file_size_bytes: None,
        port: 0,
        shopify_admin_origin: "https://shopify.com".to_string(),
        snapshot_path: None,
    })
    .with_base_products(vec![base_product()])
    .with_upstream_transport(move |request| {
        let body: Value = serde_json::from_str(&request.body).expect("upstream body");
        captured.lock().unwrap().push(body.clone());
        Response {
            status: 200,
            headers: Default::default(),
            body: json!({
                "data": {
                    "redirect": {
                        "urlRedirect": { "id": "gid://shopify/UrlRedirect/1" },
                        "userErrors": []
                    }
                }
            }),
        }
    });

    let query = r#"
        mutation MixedSupportedUnsupported {
          productUpdate(product: { id: "gid://shopify/Product/base", title: "Safe local update" }) {
            product { id title }
            userErrors { field message }
          }
          redirect: urlRedirectCreate(urlRedirect: { path: "/old", target: "/new" }) {
            urlRedirect { id }
            userErrors { message }
          }
        }
    "#;
    let response = proxy.process_request(graphql_request(&json!({ "query": query }).to_string()));

    assert_eq!(response.status, 200);
    assert_eq!(response.body["data"]["productUpdate"], Value::Null);
    assert_eq!(response.body["data"]["redirect"], Value::Null);
    assert!(response.body["errors"].as_array().is_some_and(|errors| {
        !errors.is_empty()
            && errors.iter().all(|error| {
                error["message"]
                    == json!(
                    "A mutation operation cannot mix locally staged and passthrough root fields."
                )
            })
    }));

    let forwarded = forwarded.lock().unwrap();
    assert!(forwarded.is_empty());

    let log = log_snapshot(&proxy);
    let entries = log["entries"].as_array().expect("mutation log entries");
    assert!(entries.is_empty());

    let read = proxy.process_request(graphql_request(
        &json!({ "query": "{ product(id: \"gid://shopify/Product/base\") { title } }" })
            .to_string(),
    ));
    assert_eq!(read.body["data"]["product"]["title"], json!("Base product"));
}

#[test]
fn saved_search_mutation_outcomes_finalize_exactly_one_log_draft() {
    let create_query = "mutation { savedSearchCreate(input: { name: \"Promo orders\", query: \"tag:promo\", resourceType: ORDER }) { savedSearch { id name query resourceType } userErrors { field message } } }";
    let mut create_proxy = snapshot_proxy();
    let create = create_proxy.process_request(graphql_request(
        &json!({ "query": create_query }).to_string(),
    ));
    assert_eq!(create.status, 200);
    assert_eq!(
        create.body["data"]["savedSearchCreate"]["savedSearch"]["name"],
        json!("Promo orders")
    );
    assert_single_local_staged_log(
        &create_proxy,
        create_query,
        json!({}),
        "savedSearchCreate",
        "saved_searches",
        json!(["gid://shopify/SavedSearch/1?shopify-draft-proxy=synthetic"]),
    );

    let update_query = "mutation { savedSearchUpdate(input: { id: \"gid://shopify/SavedSearch/default-order-open?shopify-draft-proxy=synthetic\", name: \"Open orders\", query: \"status:open\" }) { savedSearch { id name query resourceType } userErrors { field message } } }";
    let mut update_proxy = snapshot_proxy();
    let update = update_proxy.process_request(graphql_request(
        &json!({ "query": update_query }).to_string(),
    ));
    assert_eq!(update.status, 200);
    assert_eq!(
        update.body["data"]["savedSearchUpdate"]["savedSearch"]["name"],
        json!("Open orders")
    );
    assert_single_local_staged_log(
        &update_proxy,
        update_query,
        json!({}),
        "savedSearchUpdate",
        "saved_searches",
        json!(["gid://shopify/SavedSearch/default-order-open?shopify-draft-proxy=synthetic"]),
    );

    let delete_query = "mutation { savedSearchDelete(input: { id: \"gid://shopify/SavedSearch/default-order-open?shopify-draft-proxy=synthetic\" }) { deletedSavedSearchId userErrors { field message } } }";
    let mut delete_proxy = snapshot_proxy();
    let delete = delete_proxy.process_request(graphql_request(
        &json!({ "query": delete_query }).to_string(),
    ));
    assert_eq!(delete.status, 200);
    assert_eq!(
        delete.body["data"]["savedSearchDelete"]["deletedSavedSearchId"],
        json!("gid://shopify/SavedSearch/default-order-open?shopify-draft-proxy=synthetic")
    );
    assert_single_local_staged_log(
        &delete_proxy,
        delete_query,
        json!({}),
        "savedSearchDelete",
        "saved_searches",
        json!(["gid://shopify/SavedSearch/default-order-open?shopify-draft-proxy=synthetic"]),
    );
}

#[test]
fn log_draft_enforcement_supported_domains_record_entries() {
    let cases = [
        (
            "admin_platform",
            "backupRegionUpdate",
            "mutation { backupRegionUpdate(region: { countryCode: CA }) { backupRegion { id } userErrors { message } } }",
        ),
        (
            "apps",
            "appUninstall",
            "mutation { appUninstall { app { id } userErrors { message } } }",
        ),
        (
            "bulk_operations",
            "bulkOperationRunQuery",
            "mutation BulkOperationRunQueryParity { bulkOperationRunQuery(query: \"{ products { edges { node { id } } } }\", groupObjects: false) { bulkOperation { id } userErrors { message } } }",
        ),
        (
            "functions",
            "taxAppConfigure",
            "mutation { taxAppConfigure(ready: true) { taxAppConfiguration { state } userErrors { message } } }",
        ),
        (
            "gift_cards",
            "giftCardCreate",
            "mutation GiftCardCreateNotify { giftCardCreate(input: { initialValue: \"5.00\" }) { giftCard { id } userErrors { message } } }",
        ),
        (
            "localization",
            "shopLocaleEnable",
            "# RustLogDraftEnforcement\nmutation { shopLocaleEnable(locale: \"fr\") { shopLocale { locale } userErrors { message } } }",
        ),
        (
            "marketing",
            "marketingActivityCreateExternal",
            "# RustLogDraftEnforcement\nmutation { marketingActivityCreateExternal(input: { title: \"Launch\", remoteId: \"remote-1\", remoteUrl: \"https://example.com/launch\", tactic: NEWSLETTER, marketingChannelType: EMAIL, urlParameterValue: \"utm_campaign=launch\", utm: { campaign: \"launch\", source: \"email\", medium: \"newsletter\" } }) { marketingActivity { id } userErrors { message } } }",
        ),
        (
            "metafield_definitions",
            "standardMetafieldDefinitionEnable",
            "# RustLogDraftEnforcement\nmutation { standardMetafieldDefinitionEnable(ownerType: PRODUCT, id: \"gid://shopify/StandardMetafieldDefinitionTemplate/missing\") { createdDefinition { id } userErrors { message } } }",
        ),
        (
            "saved_searches",
            "savedSearchCreate",
            "mutation { savedSearchCreate(input: { resourceType: ORDER, name: \"X\", query: \"tag:x\" }) { savedSearch { id } userErrors { message } } }",
        ),
        (
            "segments",
            "segmentCreate",
            "mutation SegmentCreateQueryGrammar { segmentCreate(name: \"VIPs\", query: \"number_of_orders >= 5\") { segment { id name } userErrors { field } } }",
        ),
        (
            "webhooks",
            "webhookSubscriptionCreate",
            "# RustWebhookLocalRuntime\nmutation { webhookSubscriptionCreate(topic: ORDERS_CREATE, webhookSubscription: { uri: \"https://hooks.example.com/orders\", format: JSON }) { webhookSubscription { id } userErrors { message } } }",
        ),
    ];

    for (domain, root, query) in cases {
        let mut proxy = snapshot_proxy();
        if root == "backupRegionUpdate" {
            let setup = proxy.process_request(graphql_request(
                &json!({
                    "query": "mutation { marketCreate(input: { name: \"Canada\", enabled: true, regions: [{ countryCode: \"CA\" }] }) { market { id } userErrors { message } } }"
                })
                .to_string(),
            ));
            assert_eq!(
                setup.body["data"]["marketCreate"]["userErrors"],
                json!([]),
                "backupRegionUpdate log enforcement setup should stage a covering market"
            );
        }
        let mut request = graphql_request(&json!({ "query": query }).to_string());
        if root == "taxAppConfigure" {
            request.headers.insert(
                "x-shopify-draft-proxy-access-scopes".to_string(),
                "write_taxes".to_string(),
            );
            request.headers.insert(
                "x-shopify-draft-proxy-tax-calculations-app".to_string(),
                "true".to_string(),
            );
        }
        let response = proxy.process_request(request);
        assert_eq!(
            response.status, 200,
            "log-draft enforcement case {domain} should return HTTP 200; body={}",
            response.body
        );

        let log = log_snapshot(&proxy);
        let entries = log["entries"]
            .as_array()
            .unwrap_or_else(|| panic!("{domain} log entries should be an array: {log}"));
        assert!(
            !entries.is_empty(),
            "log-draft enforcement case {domain}/{root} should record at least one log entry; response body={}",
            response.body
        );
        let last = entries.last().unwrap();
        assert_eq!(
            last["status"],
            json!("staged"),
            "{domain}/{root} should record a staged mutation log entry"
        );
        assert_eq!(
            last["interpreted"]["primaryRootField"],
            json!(root),
            "{domain}/{root} should keep the staged root field in log metadata"
        );
    }
}

#[test]
fn meta_state_exposes_staged_products_saved_searches_and_deleted_ids() {
    let mut proxy = snapshot_proxy().with_base_products(vec![ProductRecord {
        id: "gid://shopify/Product/base".to_string(),
        created_at: "2024-01-01T00:00:00.000Z".to_string(),
        updated_at: "2024-01-01T00:00:00.000Z".to_string(),
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
        ..ProductRecord::default()
    }]);

    let create_product = proxy.process_request(graphql_request(
        &json!({ "query": "mutation { productCreate(product: { title: \"Created product\", handle: \"created-product\", tags: [\"new\"] }) { product { id } } }" }).to_string(),
    ));
    assert_eq!(create_product.status, 200);

    let delete_base = proxy.process_request(graphql_request(
        &json!({ "query": "mutation { productDelete(input: { id: \"gid://shopify/Product/base\" }) { deletedProductId } }" }).to_string(),
    ));
    assert_eq!(delete_base.status, 200);

    let create_saved_search = proxy.process_request(graphql_request(
        &json!({ "query": "mutation { savedSearchCreate(input: { name: \"Promo products\", query: \"tag:promo\", resourceType: PRODUCT }) { savedSearch { id } } }" }).to_string(),
    ));
    assert_eq!(create_saved_search.status, 200);

    let state = proxy.process_request(request("GET", "/__meta/state"));
    assert_eq!(state.status, 200);
    assert_eq!(state.body["stagedState"]["collections"], json!({}));
    assert_eq!(state.body["stagedState"]["deletedCollectionIds"], json!([]));
    assert_eq!(
        state.body["stagedState"]["deletedCollectionHandles"],
        json!([])
    );
    assert_eq!(state.body["stagedState"]["collectionJobs"], json!({}));
    let mut state_body = state.body.clone();
    state_body["stagedState"]
        .as_object_mut()
        .expect("stagedState is object")
        .remove("collections");
    state_body["stagedState"]
        .as_object_mut()
        .expect("stagedState is object")
        .remove("deletedCollectionIds");
    state_body["stagedState"]
        .as_object_mut()
        .expect("stagedState is object")
        .remove("deletedCollectionHandles");
    state_body["stagedState"]
        .as_object_mut()
        .expect("stagedState is object")
        .remove("collectionJobs");
    state_body["stagedState"]
        .as_object_mut()
        .expect("stagedState is object")
        .remove("fulfillmentOrderCursors");
    let mut expected: Value = serde_json::from_str(
        r##"
            {
                "baseState": {
                    "availableLocales": null,
                    "giftCardCompleteQueries": [],
                    "giftCardConfiguration": null,
                    "giftCards": {},
                    "inactiveInventoryLevels": [],
                    "inventoryItemCursors": {},
                    "inventoryItemsCatalogHydrated": false,
                    "inventoryLevelCursors": {},
                    "inventoryLevelIds": [],
                    "inventoryLevelOrder": [],
                    "inventoryLevels": [],
                    "inventoryQuantityUpdatedAt": [],
                    "localizationProductIds": [],
                    "orderCountBaselines": {},
                    "orderOrder": [],
                    "orders": {},
                    "draftOrderCountBaselines": {},
                    "draftOrderOrder": [],
                    "draftOrders": {},
                    "metafieldDefinitionNamespaces": [],
                    "metafieldDefinitionOwnerCatalogs": [],
                    "metafieldDefinitions": {},
                    "productOrder": [
                        "gid://shopify/Product/base"
                    ],
                    "productVariantOrder": [],
                    "productVariants": {},
                    "products": {
                        "gid://shopify/Product/base": {
                            "collections": {
                                "edges": [],
                                "nodes": [],
                                "pageInfo": {
                                    "endCursor": null,
                                    "hasNextPage": false,
                                    "hasPreviousPage": false,
                                    "startCursor": null
                                }
                            },
                            "createdAt": "2024-01-01T00:00:00.000Z",
                            "descriptionHtml": "<p>Base</p>",
                            "extraFields": {},
                            "handle": "base-product",
                            "id": "gid://shopify/Product/base",
                            "media": {
                                "edges": [],
                                "nodes": [],
                                "pageInfo": {
                                    "endCursor": null,
                                    "hasNextPage": false,
                                    "hasPreviousPage": false,
                                    "startCursor": null
                                }
                            },
                            "productType": "Base type",
                            "seo": {
                                "description": "",
                                "title": ""
                            },
                            "status": "ACTIVE",
                            "tags": [
                                "base"
                            ],
                            "templateSuffix": "",
                            "title": "Base product",
                            "totalInventory": 0,
                            "tracksInventory": false,
                            "updatedAt": "2024-01-01T00:00:00.000Z",
                            "variants": {
                                "edges": [],
                                "nodes": [],
                                "pageInfo": {
                                    "endCursor": null,
                                    "hasNextPage": false,
                                    "hasPreviousPage": false,
                                    "startCursor": null
                                }
                            },
                            "vendor": "Base vendor"
                        }
                    },
                    "publicationCount": null,
                    "publicationIds": [],
                    "savedSearchOrder": [],
                    "savedSearches": {},
                    "segmentOrder": [],
                    "segments": {},
                    "shop": null,
                    "shopLocales": null,
                    "shopPolicies": {},
                    "shopPolicyOrder": [],
                    "storefrontLocalizations": {},
                    "storefrontLocationCursors": {},
                    "storefrontLocationOrder": [],
                    "storefrontLocations": {},
                    "storefrontMenuOrder": [],
                    "storefrontMenus": {},
                    "storefrontPaymentSettings": null,
                    "storefrontProductTags": null,
                    "storefrontProductTypes": null,
                    "storefrontPublicApiVersions": [],
                    "storefrontShop": null,
                    "deliveryProfileOrder": [],
                    "deliveryProfiles": {},
                    "deliveryPromiseCompleteNodeIds": [],
                    "deliveryPromiseParticipantBaselineOrders": {},
                    "deliveryPromiseParticipantCompleteScopes": [],
                    "deliveryPromiseParticipantCursorIds": {},
                    "deliveryPromiseParticipantNextCursors": {},
                    "deliveryPromiseParticipantOrder": [],
                    "deliveryPromiseParticipantPreviousCursors": {},
                    "deliveryPromiseParticipants": {},
                    "deliveryPromiseProviderCompleteLocationIds": [],
                    "deliveryPromiseProviderOrder": [],
                    "deliveryPromiseProviders": {},
                    "discountOrder": [],
                    "discounts": {},
                    "discountCountBaselines": {},
                    "bulkOperationOrder": [],
                    "bulkOperations": {},
                    "bulkOperationsObserved": false,
                    "locationOrder": [],
                    "locations": {}
                },
                "stagedState": {
                    "abandonments": {},
                    "createdPublicationIds": [],
                    "currentChannelPublicationId": null,
                    "currentChannelPublicationResolved": false,
                    "customerAddressOrder": {},
                    "customerAddressOwners": {},
                    "customerAddresses": {},
                    "customerDataErasureRequests": {},
                    "customerMergeRequests": {},
                    "customerOrders": {},
                    "customerCountBaselines": {},
                    "customers": {},
                    "customersCountBase": null,
                    "delegatedAccessTokens": {},
                    "deletedCustomerIds": [],
                    "deletedDeliveryCustomizationIds": [],
                    "deletedDeliveryProfileIds": [],
                    "deletedDeliveryPromiseParticipantIds": [],
                    "deletedDeliveryPromiseProviderIds": [],
                    "deletedDiscountIds": [],
                    "deletedLocationIds": [],
                    "deletedMetafieldDefinitions": [],
                    "deletedOrderIds": [],
                    "deletedOwnerMetafields": [],
                    "deletedProductFeedIds": [],
                    "deletedProductIds": [
                        "gid://shopify/Product/base"
                    ],
                    "deletedProductVariantIds": [],
                    "deletedSavedSearchIds": [],
                    "deletedSegmentIds": [],
                    "deletedShippingPackageIds": {},
                    "deletedShopPolicyIds": [],
                    "deliveryCustomizationOrder": [],
                    "deliveryCustomizations": {},
                    "deliveryProfileOrder": [],
                    "deliveryProfiles": {},
                    "deliveryPromiseParticipantOrder": [],
                    "deliveryPromiseParticipants": {},
                    "deliveryPromiseProviderOrder": [],
                    "deliveryPromiseProviders": {},
                    "discountCodeIndex": {},
                    "discountRedeemCodeBulkCreations": {},
                    "discounts": {},
                    "draftOrderTags": {},
                    "giftCards": {},
                    "installedApps": {},
                    "locallyCreatedCustomerIds": [],
                    "locationLimitReached": false,
                    "locationOrder": [],
                    "locations": {},
                    "mergedCustomerIds": {},
                    "nextDraftOrderId": 1,
                    "nextStoreCreditAccountId": 1,
                    "nextStoreCreditTransactionId": 1,
                    "observedShippingLocationOrder": [],
                    "observedShippingLocations": {},
                    "orders": {},
                    "ownerMetafields": {},
                    "productFeedOrder": [],
                    "productFeeds": {},
                    "productOrder": [
                        "gid://shopify/Product/1?shopify-draft-proxy=synthetic"
                    ],
                    "productVariantOrder": [
                        "gid://shopify/ProductVariant/2?shopify-draft-proxy=synthetic"
                    ],
                    "productVariants": {
                        "gid://shopify/ProductVariant/2?shopify-draft-proxy=synthetic": {
                            "barcode": null,
                            "compareAtPrice": null,
                            "id": "gid://shopify/ProductVariant/2?shopify-draft-proxy=synthetic",
                            "inventoryItem": {
                                "id": "gid://shopify/InventoryItem/3?shopify-draft-proxy=synthetic",
                                "requiresShipping": true,
                                "tracked": false
                            },
                            "inventoryPolicy": "DENY",
                            "inventoryQuantity": 0,
                            "position": 1,
                            "price": "0.00",
                            "productId": "gid://shopify/Product/1?shopify-draft-proxy=synthetic",
                            "selectedOptions": [
                                {
                                    "name": "Title",
                                    "value": "Default Title"
                                }
                            ],
                            "sku": null,
                            "taxable": true,
                            "title": "Default Title"
                        }
                    },
                    "products": {
                        "gid://shopify/Product/1?shopify-draft-proxy=synthetic": {
                            "collections": {
                                "edges": [],
                                "nodes": [],
                                "pageInfo": {
                                    "endCursor": null,
                                    "hasNextPage": false,
                                    "hasPreviousPage": false,
                                    "startCursor": null
                                }
                            },
                            "createdAt": "2024-01-01T00:00:01.000Z",
                            "descriptionHtml": "",
                            "extraFields": {},
                            "handle": "created-product",
                            "id": "gid://shopify/Product/1?shopify-draft-proxy=synthetic",
                            "media": {
                                "edges": [],
                                "nodes": [],
                                "pageInfo": {
                                    "endCursor": null,
                                    "hasNextPage": false,
                                    "hasPreviousPage": false,
                                    "startCursor": null
                                }
                            },
                            "productType": "",
                            "seo": {
                                "description": "",
                                "title": ""
                            },
                            "status": "ACTIVE",
                            "tags": [
                                "new"
                            ],
                            "templateSuffix": "",
                            "title": "Created product",
                            "totalInventory": 0,
                            "tracksInventory": false,
                            "updatedAt": "2024-01-01T00:00:01.000Z",
                            "variants": {
                                "edges": [
                                    {
                                        "cursor": "gid://shopify/ProductVariant/2?shopify-draft-proxy=synthetic",
                                        "node": {
                                            "barcode": null,
                                            "compareAtPrice": null,
                                            "id": "gid://shopify/ProductVariant/2?shopify-draft-proxy=synthetic",
                                            "inventoryItem": {
                                                "id": "gid://shopify/InventoryItem/3?shopify-draft-proxy=synthetic",
                                                "requiresShipping": true,
                                                "tracked": false
                                            },
                                            "inventoryPolicy": "DENY",
                                            "inventoryQuantity": 0,
                                            "position": 1,
                                            "price": "0.00",
                                            "productId": "gid://shopify/Product/1?shopify-draft-proxy=synthetic",
                                            "selectedOptions": [
                                                {
                                                    "name": "Title",
                                                    "value": "Default Title"
                                                }
                                            ],
                                            "sku": null,
                                            "taxable": true,
                                            "title": "Default Title"
                                        }
                                    }
                                ],
                                "nodes": [
                                    {
                                        "barcode": null,
                                        "compareAtPrice": null,
                                        "id": "gid://shopify/ProductVariant/2?shopify-draft-proxy=synthetic",
                                        "inventoryItem": {
                                            "id": "gid://shopify/InventoryItem/3?shopify-draft-proxy=synthetic",
                                            "requiresShipping": true,
                                            "tracked": false
                                        },
                                        "inventoryPolicy": "DENY",
                                        "inventoryQuantity": 0,
                                        "position": 1,
                                        "price": "0.00",
                                        "productId": "gid://shopify/Product/1?shopify-draft-proxy=synthetic",
                                        "selectedOptions": [
                                            {
                                                "name": "Title",
                                                "value": "Default Title"
                                            }
                                        ],
                                        "sku": null,
                                        "taxable": true,
                                        "title": "Default Title"
                                    }
                                ],
                                "pageInfo": {
                                    "endCursor": null,
                                    "hasNextPage": false,
                                    "hasPreviousPage": false,
                                    "startCursor": null
                                }
                            },
                            "vendor": ""
                        }
                    },
                    "publicationIds": [],
                    "publications": {},
                    "resourcePublications": {},
                    "returns": {},
                    "returnsByOrder": {},
                    "reverseDeliveries": {},
                    "reverseFulfillmentOrders": {},
                    "revokedAppAccessScopes": {},
                    "savedSearchOrder": [
                        "gid://shopify/SavedSearch/4?shopify-draft-proxy=synthetic"
                    ],
                    "savedSearches": {
                        "gid://shopify/SavedSearch/4?shopify-draft-proxy=synthetic": {
                            "apiClientId": "shopify-draft-proxy-local-app",
                            "filters": [
                                {
                                    "key": "tag",
                                    "value": "promo"
                                }
                            ],
                            "id": "gid://shopify/SavedSearch/4?shopify-draft-proxy=synthetic",
                            "legacyResourceId": "4",
                            "name": "Promo products",
                            "query": "tag:promo",
                            "resourceType": "PRODUCT",
                            "searchTerms": ""
                        }
                    },
                    "segmentOrder": [],
                    "segments": {},
                    "shippingPackages": {},
                    "shopPolicies": {},
                    "shopPolicyOrder": [],
                    "storeCreditAccountOrder": [],
                    "storeCreditAccounts": {},
                    "storeCreditTransactionOrder": [],
                    "storeCreditTransactions": {},
                    "storefrontCartLineOrder": {},
                    "storefrontCartLines": {},
                    "storefrontCartOrder": [],
                    "storefrontCarts": {},
                    "storefrontCustomerAccessTokens": {},
                    "storefrontCustomerEmailIndex": {},
                    "nextStorefrontCartAppliedGiftCardId": 1,
                    "nextStorefrontCartDeliveryAddressId": 1,
                    "nextStorefrontCartId": 1,
                    "nextStorefrontCartLineId": 1,
                    "nextStorefrontCartMetafieldId": 1,
                    "nextStorefrontCustomerAccessTokenId": 1,
                    "nextStorefrontCustomerResetTokenId": 1,
                    "taggableResources": {},
                    "uninstalledAppIds": []
                }
            }
        "##,
    )
    .expect("expected meta state JSON parses");
    expected["baseState"]["availableLocales"] = state.body["baseState"]["availableLocales"].clone();
    expected["baseState"]["shop"] = state.body["baseState"]["shop"].clone();
    expected["baseState"]["shopLocales"] = state.body["baseState"]["shopLocales"].clone();
    assert_eq!(state_body, expected);
}

#[test]
fn meta_dump_and_restore_round_trip_staged_rust_state() {
    let mut proxy = snapshot_proxy().with_base_products(vec![ProductRecord {
        id: "gid://shopify/Product/base".to_string(),
        created_at: "2024-01-01T00:00:00.000Z".to_string(),
        updated_at: "2024-01-01T00:00:00.000Z".to_string(),
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
        ..ProductRecord::default()
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
        &json!({ "query": "{ productSavedSearches(first: 10) { nodes { id name query resourceType } } }" }).to_string(),
    ));
    assert_eq!(restored_saved_search_read.status, 200);
    assert_eq!(
        restored_saved_search_read.body,
        json!({
            "data": {
                "productSavedSearches": {
                    "nodes": [{
                        "id": "gid://shopify/SavedSearch/4?shopify-draft-proxy=synthetic",
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
        json!("gid://shopify/Product/5?shopify-draft-proxy=synthetic")
    );
}

#[test]
fn restore_state_round_trips_dumped_staged_counter_fields() {
    let mut dump = snapshot_proxy()
        .process_request(request_with_body(
            "POST",
            "/__meta/dump",
            &json!({ "createdAt": "2026-06-26T00:00:00.000Z" }).to_string(),
        ))
        .body;
    let staged_state = dump["state"]["stagedState"].as_object_mut().unwrap();
    staged_state.insert("nextStoreCreditAccountId".to_string(), json!(17));
    staged_state.insert("nextStoreCreditTransactionId".to_string(), json!(23));
    staged_state.insert("nextDraftOrderId".to_string(), json!(29));
    staged_state.insert("nextCustomerPaymentMethodId".to_string(), json!(31));
    staged_state.insert("nextOrderCustomerOrderId".to_string(), json!(37));
    staged_state.insert("nextDraftOrderBulkTagJobId".to_string(), json!(41));
    staged_state.insert("nextInventoryQuantityTimestamp".to_string(), json!(43));
    staged_state.insert("nextB2bCompanyId".to_string(), json!(47));
    staged_state.insert("nextB2bContactId".to_string(), json!(53));
    staged_state.insert("nextB2bContactRoleAssignmentId".to_string(), json!(59));
    staged_state.insert("nextStorefrontCartId".to_string(), json!(61));
    staged_state.insert("nextStorefrontCartLineId".to_string(), json!(67));
    staged_state.insert("nextStorefrontCartAppliedGiftCardId".to_string(), json!(71));
    staged_state.insert("nextStorefrontCartMetafieldId".to_string(), json!(73));
    staged_state.insert("nextStorefrontCartDeliveryAddressId".to_string(), json!(79));
    staged_state.insert(
        "orderCustomerOrders".to_string(),
        json!({
            "gid://shopify/Order/37": {
                "id": "gid://shopify/Order/37"
            }
        }),
    );
    staged_state.insert(
        "b2bCompanies".to_string(),
        json!({
            "gid://shopify/Company/47": {
                "id": "gid://shopify/Company/47"
            }
        }),
    );
    let counter_fields = [
        "nextStoreCreditAccountId",
        "nextStoreCreditTransactionId",
        "nextDraftOrderId",
        "nextCustomerPaymentMethodId",
        "nextOrderCustomerOrderId",
        "nextDraftOrderBulkTagJobId",
        "nextInventoryQuantityTimestamp",
        "nextB2bCompanyId",
        "nextB2bContactId",
        "nextB2bContactRoleAssignmentId",
        "nextStorefrontCartId",
        "nextStorefrontCartLineId",
        "nextStorefrontCartAppliedGiftCardId",
        "nextStorefrontCartMetafieldId",
        "nextStorefrontCartDeliveryAddressId",
    ];
    let expected_counters = counter_fields
        .iter()
        .map(|field| (*field, dump["state"]["stagedState"][field].clone()))
        .collect::<Vec<_>>();

    let mut restored = snapshot_proxy();
    let restore = restored.process_request(request_with_body(
        "POST",
        "/__meta/restore",
        &dump.to_string(),
    ));
    assert_eq!(restore.status, 200);

    let restored_dump = restored.process_request(request_with_body(
        "POST",
        "/__meta/dump",
        &json!({ "createdAt": "2026-06-26T00:00:01.000Z" }).to_string(),
    ));
    assert_eq!(restored_dump.status, 200);
    for (field, expected) in expected_counters {
        assert_eq!(
            restored_dump.body["state"]["stagedState"][field], expected,
            "staged counter {field} should round-trip through restore"
        );
    }
}

#[test]
fn restore_state_advances_order_refund_transaction_and_bulk_job_counters() {
    let mut proxy = snapshot_proxy();
    let create_order_query = r#"
        mutation CreateRestorableOrder($order: OrderCreateOrderInput!) {
          orderCreate(order: $order) {
            order {
              id
              lineItems(first: 5) { nodes { id title quantity } }
              transactions { id kind status gateway }
            }
            userErrors { field message }
          }
        }
    "#;
    let first_order = proxy.process_request(graphql_request(
        &json!({
            "query": create_order_query,
            "variables": {
                "order": {
                    "email": "restore-counters-first@example.test",
                    "currency": "CAD",
                    "lineItems": [{
                        "title": "Restore counter item",
                        "quantity": 2,
                        "priceSet": { "shopMoney": { "amount": "10.00", "currencyCode": "CAD" } }
                    }],
                    "transactions": [{
                        "kind": "SALE",
                        "status": "SUCCESS",
                        "gateway": "manual",
                        "amountSet": { "shopMoney": { "amount": "20.00", "currencyCode": "CAD" } }
                    }]
                }
            }
        })
        .to_string(),
    ));
    assert_eq!(first_order.status, 200);
    assert_eq!(
        first_order.body["data"]["orderCreate"]["userErrors"],
        json!([])
    );
    let first_order_id = first_order.body["data"]["orderCreate"]["order"]["id"].clone();
    let first_line_item_id =
        first_order.body["data"]["orderCreate"]["order"]["lineItems"]["nodes"][0]["id"].clone();
    let parent_transaction_id =
        first_order.body["data"]["orderCreate"]["order"]["transactions"][0]["id"].clone();

    let refund_query = r#"
        mutation RefundRestoredOrder($input: RefundInput!) {
          refundCreate(input: $input) {
            refund {
              id
              refundLineItems(first: 5) { nodes { id quantity } }
              transactions(first: 5) { nodes { id kind status } }
            }
            order { id }
            userErrors { field message }
          }
        }
    "#;
    let first_refund = proxy.process_request(graphql_request(
        &json!({
            "query": refund_query,
            "variables": {
                "input": {
                    "orderId": first_order_id,
                    "refundLineItems": [{
                        "lineItemId": first_line_item_id,
                        "quantity": 1,
                        "restockType": "RETURN"
                    }],
                    "transactions": [{
                        "parentId": parent_transaction_id,
                        "kind": "REFUND",
                        "gateway": "manual",
                        "orderId": first_order_id,
                        "amount": "10.00"
                    }]
                }
            }
        })
        .to_string(),
    ));
    assert_eq!(first_refund.status, 200);
    assert_eq!(
        first_refund.body["data"]["refundCreate"]["refund"]["id"],
        json!("gid://shopify/Refund/1")
    );
    assert_eq!(
        first_refund.body["data"]["refundCreate"]["refund"]["refundLineItems"]["nodes"][0]["id"],
        json!("gid://shopify/RefundLineItem/1")
    );
    assert_eq!(
        first_refund.body["data"]["refundCreate"]["refund"]["transactions"]["nodes"][0]["id"],
        json!("gid://shopify/OrderTransaction/4")
    );

    let create_draft = proxy.process_request(graphql_request(
        &json!({
            "query": r#"
                mutation CreateTaggedDraft {
                  draftOrderCreate(input: {
                    email: "restore-bulk-tags@example.test",
                    tags: ["one"],
                    lineItems: [{ title: "Restore bulk tag item", quantity: 1, originalUnitPrice: "4.00" }]
                  }) {
                    draftOrder { id tags }
                    userErrors { field message }
                  }
                }
            "#
        })
        .to_string(),
    ));
    assert_eq!(create_draft.status, 200);
    let draft_order_id = create_draft.body["data"]["draftOrderCreate"]["draftOrder"]["id"].clone();
    let first_bulk_job = proxy.process_request(graphql_request(
        &json!({
            "query": r#"
                mutation AddDraftTags($ids: [ID!]!, $tags: [String!]!) {
                  draftOrderBulkAddTags(ids: $ids, tags: $tags) {
                    job { id done }
                    userErrors { field message }
                  }
                }
            "#,
            "variables": { "ids": [draft_order_id], "tags": ["two"] }
        })
        .to_string(),
    ));
    assert_eq!(first_bulk_job.status, 200);
    assert_eq!(
        first_bulk_job.body["data"]["draftOrderBulkAddTags"]["job"]["id"],
        json!("gid://shopify/Job/1")
    );

    let dump = proxy.process_request(request_with_body(
        "POST",
        "/__meta/dump",
        &json!({ "createdAt": "2026-06-21T00:00:00.000Z" }).to_string(),
    ));
    assert_eq!(dump.status, 200);
    assert_eq!(
        dump.body["state"]["stagedState"]["nextDraftOrderBulkTagJobId"],
        json!(2)
    );

    let mut restored = snapshot_proxy();
    let restore = restored.process_request(request_with_body(
        "POST",
        "/__meta/restore",
        &dump.body.to_string(),
    ));
    assert_eq!(restore.status, 200);

    let second_order = restored.process_request(graphql_request(
        &json!({
            "query": create_order_query,
            "variables": {
                "order": {
                    "email": "restore-counters-second@example.test",
                    "currency": "CAD",
                    "financialStatus": "PENDING",
                    "lineItems": [{
                        "title": "Second restore counter item",
                        "quantity": 1,
                        "priceSet": { "shopMoney": { "amount": "7.00", "currencyCode": "CAD" } }
                    }]
                }
            }
        })
        .to_string(),
    ));
    assert_eq!(second_order.status, 200);
    assert_eq!(
        second_order.body["data"]["orderCreate"]["order"]["id"],
        json!("gid://shopify/Order/2")
    );
    assert!(state_snapshot(&restored)["stagedState"]["orders"]
        .as_object()
        .unwrap()
        .contains_key(first_order_id.as_str().unwrap()));
    let second_order_id = second_order.body["data"]["orderCreate"]["order"]["id"].clone();

    let mark_paid = restored.process_request(graphql_request(
        &json!({
            "query": r#"
                mutation MarkRestoredOrderPaid($input: OrderMarkAsPaidInput!) {
                  orderMarkAsPaid(input: $input) {
                    order { id transactions { id kind status } }
                    userErrors { field message }
                  }
                }
            "#,
            "variables": { "input": { "id": second_order_id } }
        })
        .to_string(),
    ));
    assert_eq!(mark_paid.status, 200);
    assert_eq!(
        mark_paid.body["data"]["orderMarkAsPaid"]["order"]["transactions"][0]["id"],
        json!("gid://shopify/OrderTransaction/5")
    );

    let second_refund = restored.process_request(graphql_request(
        &json!({
            "query": refund_query,
            "variables": {
                "input": {
                    "orderId": first_order_id,
                    "refundLineItems": [{
                        "lineItemId": first_line_item_id,
                        "quantity": 1,
                        "restockType": "RETURN"
                    }],
                    "transactions": [{
                        "parentId": parent_transaction_id,
                        "kind": "REFUND",
                        "gateway": "manual",
                        "orderId": first_order_id,
                        "amount": "10.00"
                    }]
                }
            }
        })
        .to_string(),
    ));
    assert_eq!(second_refund.status, 200);
    assert_eq!(
        second_refund.body["data"]["refundCreate"]["refund"]["id"],
        json!("gid://shopify/Refund/2")
    );
    assert_eq!(
        second_refund.body["data"]["refundCreate"]["refund"]["refundLineItems"]["nodes"][0]["id"],
        json!("gid://shopify/RefundLineItem/2")
    );
    assert_eq!(
        second_refund.body["data"]["refundCreate"]["refund"]["transactions"]["nodes"][0]["id"],
        json!("gid://shopify/OrderTransaction/6")
    );

    let second_bulk_job = restored.process_request(graphql_request(
        &json!({
            "query": r#"
                mutation RemoveDraftTags($ids: [ID!]!, $tags: [String!]!) {
                  draftOrderBulkRemoveTags(ids: $ids, tags: $tags) {
                    job { id done }
                    userErrors { field message }
                  }
                }
            "#,
            "variables": {
                "ids": [create_draft.body["data"]["draftOrderCreate"]["draftOrder"]["id"].clone()],
                "tags": ["one"]
            }
        })
        .to_string(),
    ));
    assert_eq!(second_bulk_job.status, 200);
    assert_eq!(
        second_bulk_job.body["data"]["draftOrderBulkRemoveTags"]["job"]["id"],
        json!("gid://shopify/Job/2")
    );
}

#[test]
fn restore_state_round_trips_customer_payment_method_records_and_counter() {
    let mut proxy = snapshot_proxy();
    let create_query = r#"
        mutation CreatePaymentMethod($sessionId: String!) {
          customerPaymentMethodCreditCardCreate(
            customerId: "gid://shopify/Customer/8801"
            sessionId: $sessionId
            billingAddress: {
              firstName: "Ada"
              lastName: "Lovelace"
              address1: "1 Main St"
              city: "New York"
              zip: "10001"
              country: "US"
              province: "NY"
            }
          ) {
            customerPaymentMethod {
              id
              customer { id }
              instrument {
                __typename
                ... on CustomerCreditCard {
                  billingAddress { address1 city zip countryCode provinceCode }
                }
              }
            }
            processing
            userErrors { field message }
          }
        }
    "#;
    let first = proxy.process_request(graphql_request(
        &json!({ "query": create_query, "variables": { "sessionId": "session-one" } }).to_string(),
    ));
    assert_eq!(first.status, 200);
    assert_eq!(
        first.body["data"]["customerPaymentMethodCreditCardCreate"]["userErrors"],
        json!([])
    );
    let first_id = first.body["data"]["customerPaymentMethodCreditCardCreate"]
        ["customerPaymentMethod"]["id"]
        .clone();
    assert_eq!(first_id, json!("gid://shopify/CustomerPaymentMethod/1"));

    let dump = proxy.process_request(request_with_body(
        "POST",
        "/__meta/dump",
        &json!({ "createdAt": "2026-06-22T00:00:00.000Z" }).to_string(),
    ));
    assert_eq!(dump.status, 200);
    assert_eq!(
        dump.body["state"]["stagedState"]["customerPaymentMethods"]
            ["gid://shopify/CustomerPaymentMethod/1"]["customer"]["id"],
        json!("gid://shopify/Customer/8801")
    );
    assert_eq!(
        dump.body["state"]["stagedState"]["customerPaymentMethodCustomerIndex"]
            ["gid://shopify/Customer/8801"],
        json!([
            "gid://shopify/CustomerPaymentMethod/base-card",
            "gid://shopify/CustomerPaymentMethod/base-paypal",
            "gid://shopify/CustomerPaymentMethod/base-shop-pay",
            "gid://shopify/CustomerPaymentMethod/1"
        ])
    );
    assert_eq!(
        dump.body["state"]["stagedState"]["nextCustomerPaymentMethodId"],
        json!(2)
    );

    let mut restored = snapshot_proxy();
    let restore = restored.process_request(request_with_body(
        "POST",
        "/__meta/restore",
        &dump.body.to_string(),
    ));
    assert_eq!(restore.status, 200);

    let read = restored.process_request(graphql_request(
        &json!({
            "query": r#"
                query ReadPaymentMethod($id: ID!) {
                  customerPaymentMethod(id: $id, showRevoked: true) {
                    id
                    customer { id }
                    instrument {
                      __typename
                      ... on CustomerCreditCard {
                        billingAddress { address1 city zip countryCode provinceCode }
                      }
                    }
                  }
                }
            "#,
            "variables": { "id": first_id }
        })
        .to_string(),
    ));
    assert_eq!(read.status, 200);
    assert_eq!(
        read.body["data"]["customerPaymentMethod"]["id"],
        json!("gid://shopify/CustomerPaymentMethod/1")
    );
    assert_eq!(
        read.body["data"]["customerPaymentMethod"]["instrument"]["billingAddress"]["address1"],
        json!("1 Main St")
    );

    let second = restored.process_request(graphql_request(
        &json!({ "query": create_query, "variables": { "sessionId": "session-two" } }).to_string(),
    ));
    assert_eq!(second.status, 200);
    assert_eq!(
        second.body["data"]["customerPaymentMethodCreditCardCreate"]["customerPaymentMethod"]["id"],
        json!("gid://shopify/CustomerPaymentMethod/2")
    );
}

#[test]
fn restore_state_round_trips_order_customer_and_b2b_records() {
    let mut proxy = snapshot_proxy();
    let create_customer = proxy.process_request(graphql_request(
        &json!({
            "query": r#"
                mutation CreateOrderCustomer {
                  customerCreate(input: { email: "roundtrip-customer@example.test", firstName: "Round", lastName: "Trip" }) {
                    customer { id email displayName }
                    userErrors { field message }
                  }
                }
            "#
        })
        .to_string(),
    ));
    assert_eq!(create_customer.status, 200);
    assert_eq!(
        create_customer.body["data"]["customerCreate"]["userErrors"],
        json!([])
    );
    let customer_id = create_customer.body["data"]["customerCreate"]["customer"]["id"].clone();

    let create_order_query = r#"
        mutation CreateOrder($order: OrderCreateOrderInput!) {
          orderCreate(order: $order) {
            order { id customer { id } }
            userErrors { field message }
          }
        }
    "#;
    let regular_order = proxy.process_request(graphql_request(
        &json!({
            "query": create_order_query,
            "variables": {
                "order": {
                    "email": "roundtrip-order@example.test",
                    "customerId": customer_id,
                    "lineItems": [{
                        "title": "Roundtrip item",
                        "quantity": 1,
                        "priceSet": { "shopMoney": { "amount": "10.00", "currencyCode": "USD" } }
                    }]
                }
            }
        })
        .to_string(),
    ));
    assert_eq!(regular_order.status, 200);
    assert_eq!(
        regular_order.body["data"]["orderCreate"]["userErrors"],
        json!([])
    );
    let regular_order_id = regular_order.body["data"]["orderCreate"]["order"]["id"].clone();

    let company = proxy.process_request(graphql_request(
        &json!({
            "query": r#"
                mutation CreateOrderCustomerCompany {
                  companyCreate(input: { company: { name: "Roundtrip Buyer Company" } }) {
                    company { id name }
                    userErrors { field message }
                  }
                }
            "#
        })
        .to_string(),
    ));
    assert_eq!(company.status, 200);
    let company_id = company.body["data"]["companyCreate"]["company"]["id"].clone();
    let assign = proxy.process_request(graphql_request(
        &json!({
            "query": r#"
                mutation AssignOrderCustomerContact($companyId: ID!, $customerId: ID!) {
                  companyAssignCustomerAsContact(companyId: $companyId, customerId: $customerId) {
                    companyContact { id customer { id } company { id name } }
                    userErrors { field message }
                  }
                }
            "#,
            "variables": { "companyId": company_id.clone(), "customerId": customer_id }
        })
        .to_string(),
    ));
    assert_eq!(assign.status, 200);
    assert_eq!(
        assign.body["data"]["companyAssignCustomerAsContact"]["userErrors"],
        json!([])
    );

    let company_location = proxy.process_request(graphql_request(
        &json!({
            "query": r#"
                mutation CreateOrderCustomerCompanyLocation($companyId: ID!) {
                  companyLocationCreate(companyId: $companyId, input: { name: "Roundtrip HQ" }) {
                    companyLocation { id }
                    userErrors { field message code }
                  }
                }
            "#,
            "variables": { "companyId": company_id }
        })
        .to_string(),
    ));
    assert_eq!(company_location.status, 200);
    assert_eq!(
        company_location.body["data"]["companyLocationCreate"]["userErrors"],
        json!([])
    );
    let company_location_id =
        company_location.body["data"]["companyLocationCreate"]["companyLocation"]["id"].clone();

    let b2b_order = proxy.process_request(graphql_request(
        &json!({
            "query": create_order_query,
            "variables": {
                "order": {
                    "email": "roundtrip-b2b-order@example.test",
                    "companyLocationId": company_location_id,
                    "lineItems": [{
                        "title": "Roundtrip B2B item",
                        "quantity": 1,
                        "priceSet": { "shopMoney": { "amount": "10.00", "currencyCode": "USD" } }
                    }]
                }
            }
        })
        .to_string(),
    ));
    assert_eq!(b2b_order.status, 200);
    let b2b_order_id = b2b_order.body["data"]["orderCreate"]["order"]["id"].clone();

    let cancelled_order = proxy.process_request(graphql_request(
        &json!({
            "query": create_order_query,
            "variables": {
                "order": {
                    "email": "roundtrip-cancelled@example.test",
                    "lineItems": [{
                        "title": "Roundtrip cancelled item",
                        "quantity": 1,
                        "priceSet": { "shopMoney": { "amount": "12.00", "currencyCode": "USD" } }
                    }]
                }
            }
        })
        .to_string(),
    ));
    assert_eq!(cancelled_order.status, 200);
    let cancelled_order_id = cancelled_order.body["data"]["orderCreate"]["order"]["id"].clone();
    let cancel = proxy.process_request(graphql_request(
        &json!({
            "query": r#"
                mutation CancelOrderCustomerOrder($orderId: ID!) {
                  orderCancel(orderId: $orderId, restock: false, reason: OTHER) {
                    job { id done }
                    orderCancelUserErrors { field message code }
                    userErrors { field message }
                  }
                }
            "#,
            "variables": { "orderId": cancelled_order_id }
        })
        .to_string(),
    ));
    assert_eq!(cancel.status, 200);
    assert_eq!(cancel.body["data"]["orderCancel"]["userErrors"], json!([]));

    let dump = proxy.process_request(request_with_body(
        "POST",
        "/__meta/dump",
        &json!({ "createdAt": "2026-06-22T00:00:00.000Z" }).to_string(),
    ));
    assert_eq!(dump.status, 200);
    assert_eq!(
        dump.body["state"]["stagedState"]["orders"][regular_order_id.as_str().unwrap()]["id"],
        regular_order_id
    );
    assert_eq!(
        dump.body["state"]["stagedState"]["orderCustomerOrders"][b2b_order_id.as_str().unwrap()]
            ["id"],
        b2b_order_id
    );
    assert_eq!(
        dump.body["state"]["stagedState"]["orderCustomerB2bOrderIds"],
        json!([b2b_order_id])
    );
    assert_eq!(
        dump.body["state"]["stagedState"]["orderCustomerContactCustomerIds"],
        json!([customer_id])
    );

    let mut restored = snapshot_proxy();
    let restore = restored.process_request(request_with_body(
        "POST",
        "/__meta/restore",
        &dump.body.to_string(),
    ));
    assert_eq!(restore.status, 200);

    let set = restored.process_request(graphql_request(
        &json!({
            "query": r#"
                mutation SetOrderCustomer($orderId: ID!, $customerId: ID!) {
                  orderCustomerSet(orderId: $orderId, customerId: $customerId) {
                    order { id customer { id email displayName } }
                    userErrors { field message code }
                  }
                }
            "#,
            "variables": { "orderId": regular_order_id, "customerId": customer_id }
        })
        .to_string(),
    ));
    assert_eq!(set.status, 200);
    assert_eq!(
        set.body["data"]["orderCustomerSet"]["userErrors"],
        json!([])
    );

    let b2b_not_permitted = restored.process_request(graphql_request(
        &json!({
            "query": r#"
                mutation SetB2bOrderCustomer($orderId: ID!, $customerId: ID!) {
                  orderCustomerSet(orderId: $orderId, customerId: $customerId) {
                    order { id customer { id } }
                    userErrors { field message code }
                  }
                }
            "#,
            "variables": { "orderId": b2b_order_id, "customerId": customer_id }
        })
        .to_string(),
    ));
    assert_eq!(
        b2b_not_permitted.body["data"]["orderCustomerSet"]["userErrors"][0]["code"],
        json!("NOT_PERMITTED")
    );

    let b2b_remove = restored.process_request(graphql_request(
        &json!({
            "query": r#"
                mutation RemoveB2bOrderCustomer($orderId: ID!) {
                  orderCustomerRemove(orderId: $orderId) {
                    order { id customer { id } }
                    userErrors { field message code }
                  }
                }
            "#,
            "variables": { "orderId": b2b_order_id }
        })
        .to_string(),
    ));
    assert_eq!(
        b2b_remove.body["data"]["orderCustomerRemove"]["userErrors"][0]["code"],
        json!("INVALID")
    );

    let cancelled_remove = restored.process_request(graphql_request(
        &json!({
            "query": r#"
                mutation RemoveCancelledOrderCustomer($orderId: ID!) {
                  orderCustomerRemove(orderId: $orderId) {
                    order { id customer { id } }
                    userErrors { field message code }
                  }
                }
            "#,
            "variables": { "orderId": cancelled_order_id }
        })
        .to_string(),
    ));
    assert_eq!(
        cancelled_remove.body["data"]["orderCustomerRemove"]["userErrors"],
        json!([])
    );

    let next_order = restored.process_request(graphql_request(
        &json!({
            "query": create_order_query,
            "variables": {
                "order": {
                    "email": "roundtrip-next@example.test",
                    "lineItems": [{
                        "title": "Roundtrip next item",
                        "quantity": 1,
                        "priceSet": { "shopMoney": { "amount": "14.00", "currencyCode": "USD" } }
                    }]
                }
            }
        })
        .to_string(),
    ));
    assert_ne!(
        next_order.body["data"]["orderCreate"]["order"]["id"],
        regular_order_id
    );
}

#[test]
fn restore_state_rejects_malformed_rust_dumps() {
    let mut proxy = snapshot_proxy();
    let dump = proxy.process_request(request_with_body(
        "POST",
        "/__meta/dump",
        &json!({ "createdAt": "2026-05-21T00:00:00.000Z" }).to_string(),
    ));
    assert_eq!(dump.status, 200);

    fn reject_restore(body: String, expected_message: &str) {
        let mut proxy = snapshot_proxy();
        let response = proxy.process_request(request_with_body("POST", "/__meta/restore", &body));
        assert_eq!(
            response.status, 400,
            "restore body should be rejected: {body}; response={}",
            response.body
        );
        assert_eq!(
            response.body["errors"][0]["message"],
            json!(expected_message)
        );
    }

    reject_restore("not-json".to_string(), "Invalid Rust state dump JSON");

    let mut missing_schema = dump.body.clone();
    missing_schema.as_object_mut().unwrap().remove("schema");
    reject_restore(
        missing_schema.to_string(),
        "Unsupported Rust state dump schema",
    );

    let mut wrong_schema = dump.body.clone();
    wrong_schema["schema"] = json!("some/other/schema");
    reject_restore(
        wrong_schema.to_string(),
        "Unsupported Rust state dump schema",
    );

    let mut missing_state = dump.body.clone();
    missing_state.as_object_mut().unwrap().remove("state");
    reject_restore(
        missing_state.to_string(),
        "Rust state dump is missing state",
    );

    let mut missing_base_state = dump.body.clone();
    missing_base_state["state"]
        .as_object_mut()
        .unwrap()
        .remove("baseState");
    reject_restore(
        missing_base_state.to_string(),
        "Rust state dump is missing state.baseState",
    );

    let mut missing_base_products = dump.body.clone();
    missing_base_products["state"]["baseState"]
        .as_object_mut()
        .unwrap()
        .remove("products");
    reject_restore(
        missing_base_products.to_string(),
        "Rust state dump is missing state.baseState.products",
    );

    let mut missing_base_product_order = dump.body.clone();
    missing_base_product_order["state"]["baseState"]
        .as_object_mut()
        .unwrap()
        .remove("productOrder");
    reject_restore(
        missing_base_product_order.to_string(),
        "Rust state dump is missing state.baseState.productOrder",
    );

    let mut missing_base_saved_search_order = dump.body.clone();
    missing_base_saved_search_order["state"]["baseState"]
        .as_object_mut()
        .unwrap()
        .remove("savedSearchOrder");
    reject_restore(
        missing_base_saved_search_order.to_string(),
        "Rust state dump is missing state.baseState.savedSearchOrder",
    );

    let mut missing_staged_state = dump.body.clone();
    missing_staged_state["state"]
        .as_object_mut()
        .unwrap()
        .remove("stagedState");
    reject_restore(
        missing_staged_state.to_string(),
        "Rust state dump is missing state.stagedState",
    );

    let mut missing_staged_products = dump.body.clone();
    missing_staged_products["state"]["stagedState"]
        .as_object_mut()
        .unwrap()
        .remove("products");
    reject_restore(
        missing_staged_products.to_string(),
        "Rust state dump is missing state.stagedState.products",
    );

    let mut missing_staged_product_order = dump.body.clone();
    missing_staged_product_order["state"]["stagedState"]
        .as_object_mut()
        .unwrap()
        .remove("productOrder");
    reject_restore(
        missing_staged_product_order.to_string(),
        "Rust state dump is missing state.stagedState.productOrder",
    );

    let mut missing_staged_deleted_ids = dump.body.clone();
    missing_staged_deleted_ids["state"]["stagedState"]
        .as_object_mut()
        .unwrap()
        .remove("deletedProductIds");
    reject_restore(
        missing_staged_deleted_ids.to_string(),
        "Rust state dump is missing state.stagedState.deletedProductIds",
    );

    let mut missing_staged_saved_search_order = dump.body.clone();
    missing_staged_saved_search_order["state"]["stagedState"]
        .as_object_mut()
        .unwrap()
        .remove("savedSearchOrder");
    reject_restore(
        missing_staged_saved_search_order.to_string(),
        "Rust state dump is missing state.stagedState.savedSearchOrder",
    );

    let mut missing_staged_deleted_saved_search_ids = dump.body.clone();
    missing_staged_deleted_saved_search_ids["state"]["stagedState"]
        .as_object_mut()
        .unwrap()
        .remove("deletedSavedSearchIds");
    reject_restore(
        missing_staged_deleted_saved_search_ids.to_string(),
        "Rust state dump is missing state.stagedState.deletedSavedSearchIds",
    );

    let mut missing_log_entries = dump.body.clone();
    missing_log_entries["log"]
        .as_object_mut()
        .unwrap()
        .remove("entries");
    reject_restore(
        missing_log_entries.to_string(),
        "Rust state dump is missing log.entries",
    );

    let mut zero_synthetic_id = dump.body.clone();
    zero_synthetic_id["nextSyntheticId"] = json!(0);
    reject_restore(
        zero_synthetic_id.to_string(),
        "Invalid Rust synthetic identity",
    );
}

#[test]
fn meta_reset_clears_log_and_staged_product_overlay() {
    let mut proxy = snapshot_proxy().with_base_products(vec![ProductRecord {
        id: "gid://shopify/Product/base".to_string(),
        created_at: "2024-01-01T00:00:00.000Z".to_string(),
        updated_at: "2024-01-01T00:00:00.000Z".to_string(),
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
        ..ProductRecord::default()
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

    let fresh_create = proxy.process_request(graphql_request(
        &json!({ "query": "mutation { productCreate(product: { title: \"Fresh product\" }) { product { id } userErrors { message } } }" }).to_string(),
    ));
    assert_eq!(fresh_create.status, 200);
    assert_eq!(
        fresh_create.body["data"]["productCreate"]["product"]["id"],
        json!("gid://shopify/Product/1?shopify-draft-proxy=synthetic")
    );
}

#[test]
fn commit_replays_staged_mutations_in_order_and_marks_entries_committed() {
    let replayed = Arc::new(Mutex::new(Vec::<Request>::new()));
    let replayed_for_transport = Arc::clone(&replayed);
    let mut proxy = snapshot_proxy()
        .with_base_products(vec![ProductRecord {
            id: "gid://shopify/Product/base".to_string(),
            created_at: "2024-01-01T00:00:00.000Z".to_string(),
            updated_at: "2024-01-01T00:00:00.000Z".to_string(),
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
            ..ProductRecord::default()
        }])
        .with_commit_transport(move |request| {
            replayed_for_transport.lock().unwrap().push(request);
            ok_transport_response(json!({ "data": { "ok": true } }))
        });

    let create_query = "mutation CreateForCommit { productCreate(product: { title: \"Created product\" }) { product { id } } }";
    let update_query = "mutation { productUpdate(product: { id: \"gid://shopify/Product/base\", title: \"Updated product\" }) { product { id } } }";
    let create_body =
        json!({ "query": create_query, "operationName": "CreateForCommit" }).to_string();
    let update_body =
        json!({ "query": update_query, "variables": { "title": "Updated product" } }).to_string();
    assert_eq!(
        proxy.process_request(graphql_request(&create_body)).status,
        200
    );
    assert_eq!(
        proxy.process_request(graphql_request(&update_body)).status,
        200
    );

    let commit = proxy.process_request(request_with_headers(
        "POST",
        "/__meta/commit",
        [
            ("authorization", "Bearer commit-token"),
            ("x-shopify-access-token", "shpat_commit"),
        ],
    ));
    assert_eq!(commit.status, 200);
    assert_eq!(
        commit.body,
        json!({
            "ok": true,
            "committed": 2,
            "failed": 0,
            "stopIndex": null,
            "attempts": [
                {
                    "index": 0,
                    "logId": "log-1",
                    "status": "committed",
                    "request": { "method": "POST", "path": "/admin/api/2026-04/graphql.json" },
                    "response": { "status": 200, "body": { "data": { "ok": true } } },
                    "mappedIds": {},
                    "unresolvedIds": [
                        {
                            "syntheticId": "gid://shopify/Product/1?shopify-draft-proxy=synthetic",
                            "operation": "productCreate",
                            "responsePath": ["product", "id"],
                            "reason": "authoritative mutation payload was missing"
                        },
                        {
                            "syntheticId": "gid://shopify/ProductVariant/2?shopify-draft-proxy=synthetic",
                            "operation": "productCreate",
                            "responsePath": ["product", "variants", "nodes", "id"],
                            "reason": "authoritative mutation payload was missing"
                        }
                    ]
                },
                {
                    "index": 1,
                    "logId": "log-2",
                    "status": "committed",
                    "request": { "method": "POST", "path": "/admin/api/2026-04/graphql.json" },
                    "response": { "status": 200, "body": { "data": { "ok": true } } },
                    "mappedIds": {}
                }
            ]
        })
    );

    let replayed = replayed.lock().unwrap();
    assert_eq!(replayed.len(), 2);
    assert_eq!(replayed[0].method, "POST");
    assert_eq!(replayed[0].path, "/admin/api/2026-04/graphql.json");
    assert_eq!(replayed[0].headers["authorization"], "Bearer commit-token");
    assert_eq!(
        replayed[0].headers["x-shopify-access-token"],
        "shpat_commit"
    );
    assert_eq!(
        serde_json::from_str::<Value>(&replayed[0].body).unwrap(),
        json!({ "query": create_query, "operationName": "CreateForCommit" })
    );
    assert_eq!(replayed[1].path, "/admin/api/2026-04/graphql.json");
    assert_eq!(
        serde_json::from_str::<Value>(&replayed[1].body).unwrap(),
        json!({ "query": update_query, "variables": { "title": "Updated product" } })
    );

    let log = proxy.process_request(request("GET", "/__meta/log"));
    assert_eq!(log.body["entries"][0]["status"], json!("committed"));
    assert_eq!(log.body["entries"][1]["status"], json!("committed"));

    let second_commit = proxy.process_request(request("POST", "/__meta/commit"));
    assert_eq!(second_commit.status, 200);
    assert_eq!(
        second_commit.body,
        json!({ "ok": true, "committed": 0, "failed": 0, "stopIndex": null, "attempts": [] })
    );
    assert_eq!(
        replayed.len(),
        2,
        "already committed entries should not be replayed again"
    );
}

#[test]
fn commit_rewrites_later_replay_bodies_with_authoritative_ids() {
    let replayed = Arc::new(Mutex::new(Vec::<Request>::new()));
    let replayed_for_transport = Arc::clone(&replayed);
    let attempts = Arc::new(Mutex::new(0usize));
    let attempts_for_transport = Arc::clone(&attempts);
    let mut proxy = snapshot_proxy().with_commit_transport(move |request| {
        replayed_for_transport.lock().unwrap().push(request);
        let mut attempts = attempts_for_transport.lock().unwrap();
        *attempts += 1;
        if *attempts == 1 {
            ok_transport_response(json!({
                "data": {
                    "productCreate": {
                        "product": { "id": "gid://shopify/Product/999" },
                        "userErrors": []
                    }
                }
            }))
        } else {
            ok_transport_response(json!({ "data": { "productUpdate": { "product": { "id": "gid://shopify/Product/999" }, "userErrors": [] } } }))
        }
    });

    let create_query =
        "mutation { productCreate(product: { title: \"Created product\" }) { product { id } } }";
    let create = proxy.process_request(graphql_request(
        &json!({ "query": create_query }).to_string(),
    ));
    assert_eq!(create.status, 200);
    let synthetic_id = create.body["data"]["productCreate"]["product"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let update_query = "mutation UpdateProduct($product: ProductUpdateInput!) { productUpdate(product: $product) { product { id title } userErrors { field message } } }";
    let update_body = json!({
        "query": update_query,
        "variables": {
            "product": {
                "id": synthetic_id,
                "title": "Authoritative update"
            }
        }
    })
    .to_string();
    let update = proxy.process_request(graphql_request(&update_body));
    assert_eq!(update.status, 200);

    let commit = proxy.process_request(request("POST", "/__meta/commit"));
    assert_eq!(commit.status, 200);
    assert_eq!(
        commit.body["attempts"][0]["mappedIds"],
        json!({ synthetic_id.clone(): "gid://shopify/Product/999" })
    );

    let replayed = replayed.lock().unwrap();
    assert_eq!(replayed.len(), 2);
    let update_replay = serde_json::from_str::<Value>(&replayed[1].body).unwrap();
    assert_eq!(
        update_replay["variables"]["product"]["id"],
        json!("gid://shopify/Product/999")
    );

    let log = proxy.process_request(request("GET", "/__meta/log"));
    assert!(
        log.body["entries"][1]["rawBody"]
            .as_str()
            .unwrap()
            .contains(&synthetic_id),
        "the persisted original raw mutation should not be rewritten"
    );
}

#[test]
fn commit_replay_maps_bulk_created_ids_from_the_created_payload_field() {
    let replayed = Arc::new(Mutex::new(Vec::<Request>::new()));
    let replayed_for_transport = Arc::clone(&replayed);
    let attempts = Arc::new(Mutex::new(0usize));
    let attempts_for_transport = Arc::clone(&attempts);
    let mut proxy = snapshot_proxy()
        .with_base_products(vec![base_product()])
        .with_commit_transport(move |request| {
            replayed_for_transport.lock().unwrap().push(request);
            let mut attempts = attempts_for_transport.lock().unwrap();
            *attempts += 1;
            if *attempts == 1 {
                ok_transport_response(json!({
                    "data": {
                        "bulkAlias": {
                            "aParent": {
                                "aVariants": {
                                    "aNodes": [
                                        { "seenId": "gid://shopify/ProductVariant/700" },
                                        { "seenId": "gid://shopify/ProductVariant/701" },
                                        { "seenId": "gid://shopify/ProductVariant/702" }
                                    ]
                                }
                            },
                            "zCreated": [
                                { "createdId": "gid://shopify/ProductVariant/701" },
                                { "createdId": "gid://shopify/ProductVariant/702" }
                            ],
                            "userErrors": []
                        }
                    }
                }))
            } else {
                ok_transport_response(json!({
                    "data": {
                        "updateAlias": {
                            "updatedVariants": [
                                { "id": "gid://shopify/ProductVariant/701" },
                                { "id": "gid://shopify/ProductVariant/702" }
                            ],
                            "userErrors": []
                        }
                    }
                }))
            }
        });

    let create_query = r#"
        mutation BulkCreateForCommit($productId: ID!, $variants: [ProductVariantsBulkInput!]!) {
          bulkAlias: productVariantsBulkCreate(productId: $productId, variants: $variants) {
            aParent: product {
              aVariants: variants(first: 10) {
                aNodes: nodes { seenId: id }
              }
            }
            zCreated: productVariants { createdId: id }
            userErrors { field message }
          }
        }
    "#;
    let create_body = json!({
        "query": create_query,
        "variables": {
            "productId": "gid://shopify/Product/base",
            "variants": [
                {
                    "price": "10.00",
                    "optionValues": [{ "optionName": "Title", "name": "Blue" }]
                },
                {
                    "price": "20.00",
                    "optionValues": [{ "optionName": "Title", "name": "Green" }]
                }
            ]
        }
    })
    .to_string();
    let create = proxy.process_request(graphql_request(&create_body));
    assert_eq!(create.status, 200);
    assert_eq!(create.body["data"]["bulkAlias"]["userErrors"], json!([]));
    let synthetic_ids = create.body["data"]["bulkAlias"]["zCreated"]
        .as_array()
        .expect("bulk create should return created variants")
        .iter()
        .map(|variant| {
            variant["createdId"]
                .as_str()
                .expect("created variant id should be selected")
                .to_string()
        })
        .collect::<Vec<_>>();

    let update_query = r#"
        mutation UpdateBulkCreatedForCommit($productId: ID!, $variants: [ProductVariantsBulkInput!]!) {
          updateAlias: productVariantsBulkUpdate(productId: $productId, variants: $variants) {
            updatedVariants: productVariants { id }
            userErrors { field message }
          }
        }
    "#;
    let update_body = json!({
        "query": update_query,
        "variables": {
            "productId": "gid://shopify/Product/base",
            "variants": [
                { "id": synthetic_ids[0], "price": "11.00" },
                { "id": synthetic_ids[1], "price": "21.00" }
            ]
        }
    })
    .to_string();
    let update = proxy.process_request(graphql_request(&update_body));
    assert_eq!(update.status, 200);
    assert_eq!(update.body["data"]["updateAlias"]["userErrors"], json!([]));

    let log_before_commit = proxy.process_request(request("GET", "/__meta/log"));
    let commit = proxy.process_request(request("POST", "/__meta/commit"));
    assert_eq!(commit.status, 200);
    assert_eq!(
        commit.body["attempts"][0]["mappedIds"],
        json!({
            synthetic_ids[0].clone(): "gid://shopify/ProductVariant/701",
            synthetic_ids[1].clone(): "gid://shopify/ProductVariant/702"
        })
    );

    let replayed = replayed.lock().unwrap();
    assert_eq!(replayed.len(), 2);
    let update_replay = serde_json::from_str::<Value>(&replayed[1].body)
        .expect("later bulk update replay should remain valid JSON");
    assert_eq!(
        update_replay["variables"]["variants"],
        json!([
            { "id": "gid://shopify/ProductVariant/701", "price": "11.00" },
            { "id": "gid://shopify/ProductVariant/702", "price": "21.00" }
        ])
    );

    let log_after_commit = proxy.process_request(request("GET", "/__meta/log"));
    assert_eq!(
        log_after_commit.body["entries"][0]["rawBody"],
        log_before_commit.body["entries"][0]["rawBody"]
    );
    assert_eq!(
        log_after_commit.body["entries"][1]["rawBody"],
        log_before_commit.body["entries"][1]["rawBody"]
    );
    assert_eq!(
        log_after_commit.body["entries"]
            .as_array()
            .unwrap()
            .iter()
            .map(|entry| entry["status"].clone())
            .collect::<Vec<_>>(),
        vec![json!("committed"), json!("committed")]
    );
}

#[test]
fn commit_replay_reports_incomplete_bulk_create_id_mappings_without_guessing() {
    let replayed = Arc::new(Mutex::new(Vec::<Request>::new()));
    let replayed_for_transport = Arc::clone(&replayed);
    let attempts = Arc::new(Mutex::new(0usize));
    let attempts_for_transport = Arc::clone(&attempts);
    let mut proxy = snapshot_proxy().with_commit_transport(move |request| {
        replayed_for_transport.lock().unwrap().push(request);
        let mut attempts = attempts_for_transport.lock().unwrap();
        *attempts += 1;
        if *attempts == 1 {
            ok_transport_response(json!({
                "data": {
                    "bulkAlias": {
                        "createdVariants": [
                            { "authoritativeId": "gid://shopify/ProductVariant/701" }
                        ],
                        "userErrors": []
                    }
                }
            }))
        } else {
            ok_transport_response(json!({
                "data": { "updateAlias": { "userErrors": [] } }
            }))
        }
    });
    let create_body = json!({
        "query": r#"
            mutation MissingBulkCreateIds(
              $productId: ID!
              $variants: [ProductVariantsBulkInput!]!
            ) {
              bulkAlias: productVariantsBulkCreate(
                productId: $productId
                variants: $variants
              ) {
                createdVariants: productVariants { authoritativeId: id }
                userErrors { field message }
              }
            }
        "#,
        "variables": {
            "productId": "gid://shopify/Product/base",
            "variants": [
                { "price": "10.00" },
                { "price": "20.00" }
            ]
        }
    })
    .to_string();
    let update_body = json!({
        "query": r#"
            mutation ReplayAfterMissingIds($variants: [ProductVariantsBulkInput!]!) {
              updateAlias: productVariantsBulkUpdate(
                productId: "gid://shopify/Product/base"
                variants: $variants
              ) {
                userErrors { field message }
              }
            }
        "#,
        "variables": {
            "variants": [
                { "id": SYNTHETIC_PRODUCT_VARIANT_ONE, "price": "11.00" },
                { "id": SYNTHETIC_PRODUCT_VARIANT_TWO, "price": "21.00" }
            ]
        }
    })
    .to_string();
    restore_log_entries(
        &mut proxy,
        json!([
            {
                "id": "log-1",
                "path": COMMIT_GRAPHQL_PATH,
                "rawBody": create_body,
                "stagedResourceIds": [
                    SYNTHETIC_PRODUCT_VARIANT_ONE,
                    SYNTHETIC_PRODUCT_VARIANT_TWO
                ],
                "status": "staged"
            },
            {
                "id": "log-2",
                "path": COMMIT_GRAPHQL_PATH,
                "rawBody": update_body,
                "stagedResourceIds": [],
                "status": "staged"
            }
        ]),
    );

    let log_before_commit = proxy.process_request(request("GET", "/__meta/log"));
    let commit = proxy.process_request(request("POST", "/__meta/commit"));
    assert_eq!(commit.status, 200);
    assert_eq!(commit.body["attempts"][0]["mappedIds"], json!({}));
    assert_eq!(
        commit.body["attempts"][0]["unresolvedIds"],
        json!([
            {
                "syntheticId": SYNTHETIC_PRODUCT_VARIANT_ONE,
                "operation": "productVariantsBulkCreate",
                "responsePath": ["productVariants", "id"],
                "reason": "authoritative response path did not contain one unambiguous ID per staged input"
            },
            {
                "syntheticId": SYNTHETIC_PRODUCT_VARIANT_TWO,
                "operation": "productVariantsBulkCreate",
                "responsePath": ["productVariants", "id"],
                "reason": "authoritative response path did not contain one unambiguous ID per staged input"
            }
        ])
    );

    let replayed = replayed.lock().unwrap();
    assert_eq!(replayed.len(), 2);
    let update_replay = serde_json::from_str::<Value>(&replayed[1].body)
        .expect("later replay should remain valid JSON");
    assert_eq!(
        update_replay["variables"]["variants"][0]["id"],
        json!(SYNTHETIC_PRODUCT_VARIANT_ONE)
    );
    assert_eq!(
        update_replay["variables"]["variants"][1]["id"],
        json!(SYNTHETIC_PRODUCT_VARIANT_TWO)
    );

    let log_after_commit = proxy.process_request(request("GET", "/__meta/log"));
    assert_eq!(
        log_after_commit.body["entries"][0]["rawBody"],
        log_before_commit.body["entries"][0]["rawBody"]
    );
    assert_eq!(
        log_after_commit.body["entries"][1]["rawBody"],
        log_before_commit.body["entries"][1]["rawBody"]
    );
    assert_eq!(
        log_after_commit.body["entries"]
            .as_array()
            .unwrap()
            .iter()
            .map(|entry| (entry["id"].clone(), entry["status"].clone()))
            .collect::<Vec<_>>(),
        vec![
            (json!("log-1"), json!("committed")),
            (json!("log-2"), json!("committed"))
        ]
    );
}

#[test]
fn commit_replay_raw_body_rewrites_all_mapped_synthetic_ids() {
    let replayed = Arc::new(Mutex::new(Vec::<Request>::new()));
    let replayed_for_transport = Arc::clone(&replayed);
    let attempts = Arc::new(Mutex::new(0usize));
    let attempts_for_transport = Arc::clone(&attempts);
    let mut proxy = snapshot_proxy().with_commit_transport(move |request| {
        replayed_for_transport.lock().unwrap().push(request);
        let mut attempts = attempts_for_transport.lock().unwrap();
        *attempts += 1;
        if *attempts == 1 {
            ok_transport_response(json!({
                "data": {
                    "firstCreate": {
                        "savedSearch": { "id": AUTHORITATIVE_SAVED_SEARCH_ONE },
                        "userErrors": []
                    },
                    "secondCreate": {
                        "savedSearch": { "id": AUTHORITATIVE_SAVED_SEARCH_TWO },
                        "userErrors": []
                    }
                }
            }))
        } else {
            ok_transport_response(json!({ "data": { "savedSearchUpdate": { "userErrors": [] } } }))
        }
    });

    let replay_raw_body = json!({
        "query": "mutation UpdateSavedSearches($ids: [ID!]!) { savedSearchUpdate(ids: $ids) { savedSearch { id } } }",
        "variables": { "ids": [SYNTHETIC_SAVED_SEARCH_ONE, SYNTHETIC_SAVED_SEARCH_TWO] }
    })
    .to_string();
    restore_log_entries(
        &mut proxy,
        json!([
            {
                "id": "log-1",
                "path": COMMIT_GRAPHQL_PATH,
                "rawBody": json!({ "query": "mutation { firstCreate: savedSearchCreate(input: { name: \"One\", query: \"status:open\" }) { savedSearch { id } } secondCreate: savedSearchCreate(input: { name: \"Two\", query: \"status:closed\" }) { savedSearch { id } } }" }).to_string(),
                "stagedResourceIds": [
                    SYNTHETIC_SAVED_SEARCH_ONE,
                    { "nested": [SYNTHETIC_SAVED_SEARCH_TWO] }
                ],
                "status": "staged"
            },
            {
                "id": "log-2",
                "path": COMMIT_GRAPHQL_PATH,
                "query": "mutation { ignored }",
                "variables": { "ignored": true },
                "rawBody": replay_raw_body,
                "stagedResourceIds": [],
                "status": "staged"
            }
        ]),
    );

    let commit = proxy.process_request(request("POST", "/__meta/commit"));
    assert_eq!(commit.status, 200);
    assert_eq!(
        commit.body["attempts"][0]["mappedIds"],
        json!({
            SYNTHETIC_SAVED_SEARCH_ONE: AUTHORITATIVE_SAVED_SEARCH_ONE,
            SYNTHETIC_SAVED_SEARCH_TWO: AUTHORITATIVE_SAVED_SEARCH_TWO
        })
    );

    let replayed = replayed.lock().unwrap();
    assert_eq!(replayed.len(), 2);
    let replay_body = &replayed[1].body;
    let parsed = serde_json::from_str::<Value>(replay_body).expect("replay body should be JSON");
    assert_eq!(
        parsed["variables"]["ids"],
        json!([
            AUTHORITATIVE_SAVED_SEARCH_ONE,
            AUTHORITATIVE_SAVED_SEARCH_TWO
        ])
    );
    assert!(!replay_body.contains(SYNTHETIC_SAVED_SEARCH_ONE));
    assert!(!replay_body.contains(SYNTHETIC_SAVED_SEARCH_TWO));
    assert!(!replay_body.contains("ignored"));

    let log = proxy.process_request(request("GET", "/__meta/log"));
    assert!(
        log.body["entries"][1]["rawBody"]
            .as_str()
            .unwrap()
            .contains(SYNTHETIC_SAVED_SEARCH_ONE),
        "commit replay should not mutate the persisted raw mutation body"
    );
}

#[test]
fn commit_replay_rewrites_canonical_aliases_in_variables_and_inline_literals() {
    let replayed = Arc::new(Mutex::new(Vec::<Request>::new()));
    let replayed_for_transport = Arc::clone(&replayed);
    let attempts = Arc::new(Mutex::new(0usize));
    let attempts_for_transport = Arc::clone(&attempts);
    let mut proxy = snapshot_proxy().with_commit_transport(move |request| {
        replayed_for_transport.lock().unwrap().push(request);
        let mut attempts = attempts_for_transport.lock().unwrap();
        *attempts += 1;
        if *attempts == 1 {
            ok_transport_response(json!({
                "data": {
                    "firstCreate": {
                        "savedSearch": { "id": AUTHORITATIVE_SAVED_SEARCH_ONE },
                        "userErrors": []
                    },
                    "secondCreate": {
                        "savedSearch": { "id": AUTHORITATIVE_SAVED_SEARCH_TWO },
                        "userErrors": []
                    }
                }
            }))
        } else {
            ok_transport_response(json!({ "data": { "savedSearchUpdate": { "userErrors": [] } } }))
        }
    });

    let query = format!(
        r#"
        mutation {{
          # GraphQL text should keep {CANONICAL_SAVED_SEARCH_ONE}
          savedSearchUpdate(input: {{ id: "{CANONICAL_SAVED_SEARCH_ONE}", name: "Updated", query: "status:open" }}) {{
            savedSearch {{ id }}
            userErrors {{ field message }}
          }}
          second: savedSearchUpdate(input: {{ id: "{CANONICAL_SAVED_SEARCH_TWO}", name: "Updated two", query: "status:closed" }}) {{
            savedSearch {{ id }}
            userErrors {{ field message }}
          }}
        }}
        "#
    );
    restore_log_entries(
        &mut proxy,
        json!([
            {
                "id": "log-1",
                "path": COMMIT_GRAPHQL_PATH,
                "rawBody": json!({ "query": "mutation { firstCreate: savedSearchCreate(input: { name: \"One\", query: \"status:open\" }) { savedSearch { id } } secondCreate: savedSearchCreate(input: { name: \"Two\", query: \"status:closed\" }) { savedSearch { id } } }" }).to_string(),
                "stagedResourceIds": [SYNTHETIC_SAVED_SEARCH_ONE, SYNTHETIC_SAVED_SEARCH_TWO],
                "status": "staged"
            },
            {
                "id": "log-2",
                "path": COMMIT_GRAPHQL_PATH,
                "rawBody": json!({
                    "query": query,
                    "variables": {
                        "id": CANONICAL_SAVED_SEARCH_ONE,
                        "nested": {
                            "ids": [CANONICAL_SAVED_SEARCH_TWO, SYNTHETIC_SAVED_SEARCH_ONE],
                            "text": format!("note mentions {CANONICAL_SAVED_SEARCH_ONE} but is not an ID value")
                        }
                    }
                }).to_string(),
                "stagedResourceIds": [],
                "status": "staged"
            }
        ]),
    );

    let commit = proxy.process_request(request("POST", "/__meta/commit"));
    assert_eq!(commit.status, 200);

    let replayed = replayed.lock().unwrap();
    assert_eq!(replayed.len(), 2);
    let parsed =
        serde_json::from_str::<Value>(&replayed[1].body).expect("replayed body should be JSON");
    let query = parsed["query"].as_str().expect("query should be preserved");
    assert!(query.contains(&format!(
        "savedSearchUpdate(input: {{ id: \"{AUTHORITATIVE_SAVED_SEARCH_ONE}\""
    )));
    assert!(query.contains(&format!(
        "second: savedSearchUpdate(input: {{ id: \"{AUTHORITATIVE_SAVED_SEARCH_TWO}\""
    )));
    assert!(query.contains(&format!(
        "# GraphQL text should keep {CANONICAL_SAVED_SEARCH_ONE}"
    )));
    assert_eq!(
        parsed["variables"]["id"],
        json!(AUTHORITATIVE_SAVED_SEARCH_ONE)
    );
    assert_eq!(
        parsed["variables"]["nested"]["ids"],
        json!([
            AUTHORITATIVE_SAVED_SEARCH_TWO,
            AUTHORITATIVE_SAVED_SEARCH_ONE
        ])
    );
    assert_eq!(
        parsed["variables"]["nested"]["text"],
        json!(format!(
            "note mentions {CANONICAL_SAVED_SEARCH_ONE} but is not an ID value"
        ))
    );
}

#[test]
fn commit_replay_preserves_nonmatching_canonical_type_and_tail_values() {
    let replayed = Arc::new(Mutex::new(Vec::<Request>::new()));
    let replayed_for_transport = Arc::clone(&replayed);
    let attempts = Arc::new(Mutex::new(0usize));
    let attempts_for_transport = Arc::clone(&attempts);
    let mut proxy = snapshot_proxy().with_commit_transport(move |request| {
        replayed_for_transport.lock().unwrap().push(request);
        let mut attempts = attempts_for_transport.lock().unwrap();
        *attempts += 1;
        if *attempts == 1 {
            ok_transport_response(json!({
                "data": {
                    "savedSearchCreate": {
                        "savedSearch": { "id": AUTHORITATIVE_SAVED_SEARCH_ONE },
                        "userErrors": []
                    }
                }
            }))
        } else {
            ok_transport_response(json!({ "data": { "savedSearchUpdate": { "userErrors": [] } } }))
        }
    });
    let wrong_type_same_tail = "gid://shopify/Product/1";
    let same_type_wrong_tail = "gid://shopify/SavedSearch/42";
    let longer_tail = format!("{CANONICAL_SAVED_SEARCH_ONE}0");
    let query = format!(
        r#"
        mutation {{
          wrongType: savedSearchUpdate(input: {{ id: "{wrong_type_same_tail}", name: "Wrong type" }}) {{ savedSearch {{ id }} }}
          wrongTail: savedSearchUpdate(input: {{ id: "{same_type_wrong_tail}", name: "Wrong tail" }}) {{ savedSearch {{ id }} }}
          longer: savedSearchUpdate(input: {{ id: "{longer_tail}", name: "Longer tail" }}) {{ savedSearch {{ id }} }}
        }}
        "#
    );
    restore_log_entries(
        &mut proxy,
        json!([
            {
                "id": "log-1",
                "path": COMMIT_GRAPHQL_PATH,
                "rawBody": json!({ "query": "mutation { savedSearchCreate(input: { name: \"One\", query: \"status:open\" }) { savedSearch { id } } }" }).to_string(),
                "stagedResourceIds": [SYNTHETIC_SAVED_SEARCH_ONE],
                "status": "staged"
            },
            {
                "id": "log-2",
                "path": COMMIT_GRAPHQL_PATH,
                "rawBody": json!({
                    "query": query,
                    "variables": {
                        "wrongType": wrong_type_same_tail,
                        "wrongTail": same_type_wrong_tail,
                        "longer": longer_tail
                    }
                }).to_string(),
                "stagedResourceIds": [],
                "status": "staged"
            }
        ]),
    );

    let commit = proxy.process_request(request("POST", "/__meta/commit"));
    assert_eq!(commit.status, 200);

    let replayed = replayed.lock().unwrap();
    assert_eq!(replayed.len(), 2);
    let body = &replayed[1].body;
    let parsed = serde_json::from_str::<Value>(body).expect("replayed body should be JSON");
    let replay_query = parsed["query"].as_str().expect("query should be preserved");
    assert!(replay_query.contains(wrong_type_same_tail));
    assert!(replay_query.contains(same_type_wrong_tail));
    assert!(replay_query.contains(&longer_tail));
    assert_eq!(
        parsed["variables"]["wrongType"],
        json!(wrong_type_same_tail)
    );
    assert_eq!(
        parsed["variables"]["wrongTail"],
        json!(same_type_wrong_tail)
    );
    assert_eq!(parsed["variables"]["longer"], json!(longer_tail));
    assert!(!body.contains(AUTHORITATIVE_SAVED_SEARCH_ONE));
}

#[test]
fn commit_replay_legacy_log_entries_fall_back_to_query_and_variables() {
    let replayed = Arc::new(Mutex::new(Vec::<Request>::new()));
    let replayed_for_transport = Arc::clone(&replayed);
    let mut proxy = snapshot_proxy().with_commit_transport(move |request| {
        replayed_for_transport.lock().unwrap().push(request);
        ok_transport_response(json!({ "data": { "savedSearchCreate": { "userErrors": [] } } }))
    });
    let legacy_query = "mutation LegacyCommit($input: SavedSearchCreateInput!) { savedSearchCreate(input: $input) { savedSearch { id } } }";
    let legacy_variables = json!({ "input": { "name": "Open orders", "query": "status:open" } });
    restore_log_entries(
        &mut proxy,
        json!([{
            "id": "log-1",
            "path": COMMIT_GRAPHQL_PATH,
            "query": legacy_query,
            "variables": legacy_variables,
            "stagedResourceIds": [],
            "status": "staged"
        }]),
    );

    let commit = proxy.process_request(request("POST", "/__meta/commit"));
    assert_eq!(commit.status, 200);

    let replayed = replayed.lock().unwrap();
    assert_eq!(replayed.len(), 1);
    let parsed =
        serde_json::from_str::<Value>(&replayed[0].body).expect("fallback body should be JSON");
    assert_eq!(parsed["query"], json!(legacy_query));
    assert_eq!(parsed["variables"], legacy_variables);
}

#[test]
fn commit_replay_maps_multiple_ids_and_keeps_existing_mapping() {
    let attempts = Arc::new(Mutex::new(0usize));
    let attempts_for_transport = Arc::clone(&attempts);
    let mut proxy = snapshot_proxy().with_commit_transport(move |_request| {
        let mut attempts = attempts_for_transport.lock().unwrap();
        *attempts += 1;
        if *attempts == 1 {
            ok_transport_response(json!({
                "data": {
                    "firstCreate": {
                        "savedSearch": { "id": AUTHORITATIVE_SAVED_SEARCH_ONE },
                        "userErrors": []
                    },
                    "secondCreate": {
                        "savedSearch": { "id": AUTHORITATIVE_SAVED_SEARCH_TWO },
                        "userErrors": []
                    }
                }
            }))
        } else {
            ok_transport_response(json!({ "id": AUTHORITATIVE_SAVED_SEARCH_TWO }))
        }
    });
    restore_log_entries(
        &mut proxy,
        json!([
            {
                "id": "log-1",
                "path": COMMIT_GRAPHQL_PATH,
                "rawBody": json!({ "query": "mutation { firstCreate: savedSearchCreate(input: { name: \"One\", query: \"status:open\" }) { savedSearch { id } } secondCreate: savedSearchCreate(input: { name: \"Two\", query: \"status:closed\" }) { savedSearch { id } } }" }).to_string(),
                "stagedResourceIds": [
                    SYNTHETIC_SAVED_SEARCH_ONE,
                    { "nested": [SYNTHETIC_SAVED_SEARCH_TWO, "gid://shopify/SavedSearch/non-synthetic"] }
                ],
                "status": "staged"
            },
            {
                "id": "log-2",
                "path": COMMIT_GRAPHQL_PATH,
                "rawBody": json!({ "query": "mutation { savedSearchUpdate(input: { id: \"gid://shopify/SavedSearch/1\", name: \"Again\" }) { savedSearch { id } } }" }).to_string(),
                "stagedResourceIds": [SYNTHETIC_SAVED_SEARCH_ONE],
                "status": "staged"
            }
        ]),
    );

    let commit = proxy.process_request(request("POST", "/__meta/commit"));
    assert_eq!(commit.status, 200);
    assert_eq!(commit.body["committed"], json!(2));
    assert_eq!(
        commit.body["attempts"][0]["mappedIds"],
        json!({
            SYNTHETIC_SAVED_SEARCH_ONE: AUTHORITATIVE_SAVED_SEARCH_ONE,
            SYNTHETIC_SAVED_SEARCH_TWO: AUTHORITATIVE_SAVED_SEARCH_TWO
        })
    );
    assert_eq!(commit.body["attempts"][1]["mappedIds"], json!({}));
}

#[test]
fn commit_replay_authoritative_id_mapping_skips_non_synthetic_and_wrong_type_ids() {
    let mut proxy = snapshot_proxy().with_commit_transport(move |_request| {
        ok_transport_response(json!({
            "data": {
                "webhookSubscriptionCreate": {
                    "webhookSubscription": {
                        "id": "gid://shopify/WebhookSubscription/99"
                    }
                }
            }
        }))
    });
    restore_log_entries(
        &mut proxy,
        json!([{
            "id": "log-1",
            "path": COMMIT_GRAPHQL_PATH,
            "rawBody": json!({ "query": "mutation { savedSearchCreate(input: { name: \"One\", query: \"status:open\" }) { savedSearch { id } } }" }).to_string(),
            "stagedResourceIds": [
                "gid://shopify/SavedSearch/ordinary",
                SYNTHETIC_SAVED_SEARCH_ONE
            ],
            "status": "staged"
        }]),
    );

    let commit = proxy.process_request(request("POST", "/__meta/commit"));
    assert_eq!(commit.status, 200);
    assert_eq!(commit.body["attempts"][0]["mappedIds"], json!({}));
}

#[test]
fn commit_replay_graphql_error_detection_matches_top_level_error_semantics() {
    let cases = vec![
        (
            "non-empty error array",
            json!({ "errors": [{ "message": "boom" }] }),
            502,
            false,
        ),
        (
            "error object",
            json!({ "errors": { "message": "boom" } }),
            502,
            false,
        ),
        ("empty error array", json!({ "errors": [] }), 200, true),
        ("null errors", json!({ "errors": null }), 200, true),
        (
            "absent errors",
            json!({ "data": { "ok": true } }),
            200,
            true,
        ),
    ];

    for (label, response_body, expected_status, expected_ok) in cases {
        let mut proxy = snapshot_proxy()
            .with_commit_transport(move |_request| ok_transport_response(response_body.clone()));
        restore_log_entries(
            &mut proxy,
            json!([{
                "id": "log-1",
                "path": COMMIT_GRAPHQL_PATH,
                "rawBody": json!({ "query": "mutation { savedSearchCreate(input: { name: \"One\", query: \"status:open\" }) { savedSearch { id } } }" }).to_string(),
                "stagedResourceIds": [],
                "status": "staged"
            }]),
        );

        let commit = proxy.process_request(request("POST", "/__meta/commit"));
        assert_eq!(commit.status, expected_status, "{label}");
        assert_eq!(commit.body["ok"], json!(expected_ok), "{label}");
    }
}

#[test]
fn commit_stops_on_first_transport_failure_and_persists_failed_status() {
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
            "stopIndex": 0,
            "attempts": [{
                "index": 0,
                "logId": "log-1",
                "status": "failed",
                "request": { "method": "POST", "path": "/admin/api/2026-04/graphql.json" },
                "response": { "status": 500, "body": { "errors": [{ "message": "upstream failed" }] } },
                "error": "Upstream commit failed for log-1 with status 500"
            }],
            "error": "Upstream commit failed for log-1 with status 500"
        })
    );
    assert_eq!(*attempts.lock().unwrap(), 1);

    let log = proxy.process_request(request("GET", "/__meta/log"));
    assert_eq!(log.body["entries"][0]["status"], json!("failed"));
    assert_eq!(log.body["entries"][1]["status"], json!("staged"));
}

#[test]
fn commit_stops_on_graphql_errors_after_committing_prior_entries() {
    let attempts = Arc::new(Mutex::new(0usize));
    let attempts_for_transport = Arc::clone(&attempts);
    let mut proxy = snapshot_proxy().with_commit_transport(move |_request| {
        let mut attempts = attempts_for_transport.lock().unwrap();
        *attempts += 1;
        match *attempts {
            1 => ok_transport_response(json!({ "data": { "ok": true } })),
            2 => ok_transport_response(json!({
                "data": null,
                "errors": [{ "message": "GraphQL validation failed" }]
            })),
            _ => ok_transport_response(json!({ "data": { "ok": true } })),
        }
    });

    for title in ["First product", "Second product", "Third product"] {
        let query = format!(
            "mutation {{ productCreate(product: {{ title: \"{title}\" }}) {{ product {{ id }} }} }}"
        );
        assert_eq!(
            proxy
                .process_request(graphql_request(&json!({ "query": query }).to_string()))
                .status,
            200
        );
    }

    let commit = proxy.process_request(request("POST", "/__meta/commit"));
    assert_eq!(commit.status, 502);
    assert_eq!(commit.body["ok"], json!(false));
    assert_eq!(commit.body["committed"], json!(1));
    assert_eq!(commit.body["failed"], json!(1));
    assert_eq!(commit.body["stopIndex"], json!(1));
    assert_eq!(
        commit.body["error"],
        json!("Upstream commit failed for log-2 with GraphQL errors")
    );
    assert_eq!(commit.body["attempts"].as_array().unwrap().len(), 2);
    assert_eq!(*attempts.lock().unwrap(), 2);

    let log = proxy.process_request(request("GET", "/__meta/log"));
    assert_eq!(log.body["entries"][0]["status"], json!("committed"));
    assert_eq!(log.body["entries"][1]["status"], json!("failed"));
    assert_eq!(log.body["entries"][2]["status"], json!("staged"));
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

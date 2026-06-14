use std::sync::{Arc, Mutex};

use pretty_assertions::assert_eq;
use serde_json::{json, Value};
use shopify_draft_proxy::proxy::{
    Config, DraftProxy, ProductRecord, ReadMode, Request, Response, UnsupportedMutationMode,
};

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
        proxy.get_log_snapshot(),
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
fn ported_gleam_draft_proxy_route_and_snapshot_helpers_match_old_proxy_tests() {
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
    assert_eq!(default_proxy.get_config_snapshot(), expected_default_config);
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
        snapshot_proxy.get_config_snapshot(),
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

    let log_snapshot = default_proxy.get_log_snapshot();
    assert_eq!(log_snapshot, json!({ "entries": [] }));
    assert_eq!(
        default_proxy
            .process_request(request("GET", "/__meta/log"))
            .body,
        log_snapshot
    );

    let state_snapshot = default_proxy.get_state_snapshot();
    assert_eq!(
        default_proxy
            .process_request(request("GET", "/__meta/state"))
            .body,
        state_snapshot
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
        helper_proxy.get_log_snapshot()
    );
    assert_eq!(
        helper_proxy
            .process_request(request("GET", "/__meta/state"))
            .body,
        helper_proxy.get_state_snapshot()
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
            .process_request(graphql_request(
                &json!({ "query": "mutation { eventDelete(id: \"x\") { ok } }" }).to_string()
            ))
            .status,
        400
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
                expected_local_staged_log(
                    "log-1",
                    create_query,
                    json!({}),
                    "productCreate",
                    "products",
                    json!(["gid://shopify/Product/1?shopify-draft-proxy=synthetic"])
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
    let create_query = "mutation { productCreate(product: { title: \"Created product\" }) { product { id title } userErrors { field message code } } }";
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
        json!(["gid://shopify/Product/1?shopify-draft-proxy=synthetic"]),
    );

    let update_query = "mutation { productUpdate(product: { id: \"gid://shopify/Product/base\", title: \"Updated product\" }) { product { id title } userErrors { field message code } } }";
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

    let delete_query = "mutation { productDelete(input: { id: \"gid://shopify/Product/base\" }) { deletedProductId userErrors { field message code } } }";
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
        product_set.body["data"]["productSet"]["product"]["title"],
        json!("Async delete source")
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
fn saved_search_mutation_outcomes_finalize_exactly_one_log_draft() {
    let create_query = "mutation { savedSearchCreate(input: { name: \"Promo orders\", query: \"tag:promo\", resourceType: ORDER }) { savedSearch { id name query resourceType } userErrors { field message code } } }";
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

    let update_query = "mutation { savedSearchUpdate(input: { id: \"gid://shopify/SavedSearch/3634391580978\", name: \"Open orders\", query: \"status:open\" }) { savedSearch { id name query resourceType } userErrors { field message } } }";
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
        json!(["gid://shopify/SavedSearch/3634391580978"]),
    );

    let delete_query = "mutation { savedSearchDelete(input: { id: \"gid://shopify/SavedSearch/3634391580978\" }) { deletedSavedSearchId userErrors { field message } } }";
    let mut delete_proxy = snapshot_proxy();
    let delete = delete_proxy.process_request(graphql_request(
        &json!({ "query": delete_query }).to_string(),
    ));
    assert_eq!(delete.status, 200);
    assert_eq!(
        delete.body["data"]["savedSearchDelete"]["deletedSavedSearchId"],
        json!("gid://shopify/SavedSearch/3634391580978")
    );
    assert_single_local_staged_log(
        &delete_proxy,
        delete_query,
        json!({}),
        "savedSearchDelete",
        "saved_searches",
        json!(["gid://shopify/SavedSearch/3634391580978"]),
    );
}

#[test]
fn ported_gleam_log_draft_enforcement_supported_domains_record_entries() {
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
            "mutation { taxAppConfigure(ready: true) { taxAppConfiguration { id } userErrors { message } } }",
        ),
        (
            "gift_cards",
            "giftCardCreate",
            "mutation GiftCardCreateNotify { giftCardCreate(input: { initialValue: { amount: \"5.00\", currencyCode: CAD } }) { giftCard { id } userErrors { message } } }",
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
        let response =
            proxy.process_request(graphql_request(&json!({ "query": query }).to_string()));
        assert_eq!(
            response.status, 200,
            "ported Gleam log-draft enforcement case {domain} should return HTTP 200; body={}",
            response.body
        );

        let log = proxy.get_log_snapshot();
        let entries = log["entries"]
            .as_array()
            .unwrap_or_else(|| panic!("{domain} log entries should be an array: {log}"));
        assert!(
            !entries.is_empty(),
            "ported Gleam log-draft enforcement case {domain}/{root} should record at least one log entry; response body={}",
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
                        "createdAt": "2024-01-01T00:00:00.000Z",
                        "updatedAt": "2024-01-01T00:00:00.000Z",
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
                "productOrder": ["gid://shopify/Product/base"],
                "savedSearches": {},
                "savedSearchOrder": []
            },
            "stagedState": {
                "products": {
                    "gid://shopify/Product/1?shopify-draft-proxy=synthetic": {
                        "id": "gid://shopify/Product/1?shopify-draft-proxy=synthetic",
                        "createdAt": "2024-01-01T00:00:01.000Z",
                        "updatedAt": "2024-01-01T00:00:01.000Z",
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
                "productOrder": ["gid://shopify/Product/1?shopify-draft-proxy=synthetic"],
                "deletedProductIds": ["gid://shopify/Product/base"],
                "shippingPackages": {},
                "deletedShippingPackageIds": {},
                "delegatedAccessTokens": {},
                "customers": {},
                "deletedCustomerIds": [],
                "discounts": {},
                "discountCodeIndex": {},
                "deletedDiscountIds": [],
                "discountRedeemCodeBulkCreations": {},
                "customerOrders": {},
                "taggableResources": {},
                "orders": {},
                "returns": {},
                "returnsByOrder": {},
                "reverseDeliveries": {},
                "reverseFulfillmentOrders": {},
                "locations": {},
                "locationOrder": [],
                "locationLimitReached": false,
                "savedSearches": {
                    "gid://shopify/SavedSearch/2?shopify-draft-proxy=synthetic": {
                        "id": "gid://shopify/SavedSearch/2?shopify-draft-proxy=synthetic",
                        "name": "Promo products",
                        "query": "tag:promo",
                        "resourceType": "PRODUCT"
                    }
                },
                "savedSearchOrder": ["gid://shopify/SavedSearch/2?shopify-draft-proxy=synthetic"],
                "deletedSavedSearchIds": []
            }
        })
    );
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
fn ported_gleam_restore_state_rejects_malformed_rust_dumps() {
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
        }])
        .with_commit_transport(move |request| {
            replayed_for_transport.lock().unwrap().push(request);
            ok_transport_response(json!({ "data": { "ok": true } }))
        });

    let create_query =
        "mutation { productCreate(product: { title: \"Created product\" }) { product { id } } }";
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
                    "mappedIds": {}
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

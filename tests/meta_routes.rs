use pretty_assertions::assert_eq;
use serde_json::json;
use shopify_draft_proxy::proxy::{Config, DraftProxy, ReadMode, Request};

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
    Request {
        method: method.to_string(),
        path: path.to_string(),
        headers: Default::default(),
        body: String::new(),
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

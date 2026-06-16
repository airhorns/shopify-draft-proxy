#![allow(unused_imports)]
pub(super) use std::sync::{Arc, Mutex};

pub(super) use serde_json::{json, Value};
pub(super) use shopify_draft_proxy::graphql::OperationType;
pub(super) use shopify_draft_proxy::operation_registry::{
    CapabilityDomain, CapabilityExecution, OperationRegistryEntry,
};
pub(super) use shopify_draft_proxy::proxy::{
    Config, DraftProxy, ProductRecord, ReadMode, Request, Response,
};

pub(super) fn snapshot_proxy() -> DraftProxy {
    configured_proxy(ReadMode::Snapshot, None)
}

pub(super) fn configured_proxy(
    read_mode: ReadMode,
    unsupported_mutation_mode: Option<shopify_draft_proxy::proxy::UnsupportedMutationMode>,
) -> DraftProxy {
    configured_proxy_with_bulk_mutation_max(read_mode, unsupported_mutation_mode, None)
}

pub(super) fn configured_proxy_with_bulk_mutation_max(
    read_mode: ReadMode,
    unsupported_mutation_mode: Option<shopify_draft_proxy::proxy::UnsupportedMutationMode>,
    bulk_operation_run_mutation_max_input_file_size_bytes: Option<u64>,
) -> DraftProxy {
    DraftProxy::new(Config {
        read_mode,
        unsupported_mutation_mode,
        bulk_operation_run_mutation_max_input_file_size_bytes,
        port: 0,
        shopify_admin_origin: "https://shopify.com".to_string(),
        snapshot_path: None,
    })
}

pub(super) fn graphql_request(method: &str, body: &str) -> Request {
    Request {
        method: method.to_string(),
        path: "/admin/api/2026-04/graphql.json".to_string(),
        headers: Default::default(),
        body: body.to_string(),
    }
}

pub(super) fn request_with_body(method: &str, path: &str, body: &str) -> Request {
    Request {
        method: method.to_string(),
        path: path.to_string(),
        headers: Default::default(),
        body: body.to_string(),
    }
}

pub(super) fn json_graphql_request(query: &str, variables: serde_json::Value) -> Request {
    graphql_request(
        "POST",
        &json!({ "query": query, "variables": variables }).to_string(),
    )
}

pub(super) fn product_fixture(path: &str) -> Value {
    serde_json::from_str(path).expect("product fixture must parse")
}

pub(super) fn create_legacy_variant(
    proxy: &mut DraftProxy,
    product_id: &str,
    sku: &str,
    price: &str,
) -> Value {
    let response = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateLegacyVariantForTest($input: ProductVariantInput!) {
          productVariantCreate(input: $input) {
            productVariant { id sku price inventoryItem { id } }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "input": {
                "productId": product_id,
                "title": sku,
                "sku": sku,
                "price": price
            }
        }),
    ));
    assert_eq!(response.status, 200);
    assert_eq!(
        response.body["data"]["productVariantCreate"]["userErrors"],
        json!([])
    );
    response.body["data"]["productVariantCreate"]["productVariant"].clone()
}

pub(super) fn registry_entry(
    name: &str,
    operation_type: OperationType,
    execution: CapabilityExecution,
    implemented: bool,
) -> OperationRegistryEntry {
    OperationRegistryEntry {
        name: name.to_string(),
        operation_type,
        domain: CapabilityDomain::Products,
        execution,
        implemented,
        match_names: vec![name.to_string()],
        runtime_tests: vec!["tests/graphql_routes.rs".to_string()],
        support_notes: None,
    }
}

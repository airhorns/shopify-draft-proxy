#![allow(unused_imports)]
pub(super) use std::sync::{Arc, Mutex};

pub(super) use serde_json::{json, Value};
pub(super) use shopify_draft_proxy::graphql::OperationType;
pub(super) use shopify_draft_proxy::operation_registry::{
    CapabilityDomain, CapabilityExecution, OperationRegistryEntry,
};
pub(super) use shopify_draft_proxy::proxy::{
    Config, DraftProxy, ProductRecord, ProductVariantInventoryItem, ProductVariantRecord,
    ProductVariantSelectedOption, ReadMode, Request,
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

pub(super) fn json_graphql_request(query: &str, variables: serde_json::Value) -> Request {
    graphql_request(
        "POST",
        &json!({ "query": query, "variables": variables }).to_string(),
    )
}

pub(super) fn product_fixture(path: &str) -> Value {
    serde_json::from_str(path).expect("product fixture must parse")
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

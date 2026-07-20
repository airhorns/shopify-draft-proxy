#![allow(unused_imports)]
pub(super) use std::sync::{Arc, Mutex};

pub(super) use serde_json::{json, Value};
pub(super) use shopify_draft_proxy::graphql::OperationType;
pub(super) use shopify_draft_proxy::operation_registry::{
    ApiSurface, CapabilityDomain, OperationRegistryEntry,
};
pub(super) use shopify_draft_proxy::proxy::{
    Config, DraftProxy, ProductRecord, ReadMode, Request, Response,
};

pub(super) fn snapshot_proxy() -> DraftProxy {
    configured_proxy(ReadMode::Snapshot, None)
}

pub(super) fn create_metafield_product_owner(proxy: &mut DraftProxy, title: &str) -> String {
    let response = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateMetafieldProductOwner($product: ProductCreateInput!) {
          productCreate(product: $product) {
            product { id }
            userErrors { field message }
          }
        }
        "#,
        json!({ "product": { "title": title } }),
    ));
    assert_eq!(response.status, 200);
    assert_eq!(
        response.body["data"]["productCreate"]["userErrors"],
        json!([])
    );
    response.body["data"]["productCreate"]["product"]["id"]
        .as_str()
        .expect("productCreate should return a metafield owner id")
        .to_string()
}

pub(super) fn create_metafield_product_and_variant_owners(
    proxy: &mut DraftProxy,
    title: &str,
) -> (String, String) {
    let response = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateMetafieldProductAndVariantOwners($product: ProductCreateInput!) {
          productCreate(product: $product) {
            product { id variants(first: 1) { nodes { id } } }
            userErrors { field message }
          }
        }
        "#,
        json!({ "product": { "title": title } }),
    ));
    assert_eq!(response.status, 200);
    assert_eq!(
        response.body["data"]["productCreate"]["userErrors"],
        json!([])
    );
    let product = &response.body["data"]["productCreate"]["product"];
    (
        product["id"]
            .as_str()
            .expect("productCreate should return a metafield owner id")
            .to_string(),
        product["variants"]["nodes"][0]["id"]
            .as_str()
            .expect("productCreate should return its default variant id")
            .to_string(),
    )
}

pub(super) fn create_metafield_collection_owner(proxy: &mut DraftProxy, title: &str) -> String {
    let response = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateMetafieldCollectionOwner($input: CollectionInput!) {
          collectionCreate(input: $input) {
            collection { id }
            userErrors { field message }
          }
        }
        "#,
        json!({ "input": { "title": title } }),
    ));
    assert_eq!(response.status, 200);
    assert_eq!(
        response.body["data"]["collectionCreate"]["userErrors"],
        json!([])
    );
    response.body["data"]["collectionCreate"]["collection"]["id"]
        .as_str()
        .expect("collectionCreate should return a metafield owner id")
        .to_string()
}

pub(super) fn create_metafield_customer_owner(proxy: &mut DraftProxy, email: &str) -> String {
    let response = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateMetafieldCustomerOwner($input: CustomerInput!) {
          customerCreate(input: $input) {
            customer { id }
            userErrors { field message }
          }
        }
        "#,
        json!({ "input": { "email": email } }),
    ));
    assert_eq!(response.status, 200);
    assert_eq!(
        response.body["data"]["customerCreate"]["userErrors"],
        json!([])
    );
    response.body["data"]["customerCreate"]["customer"]["id"]
        .as_str()
        .expect("customerCreate should return a metafield owner id")
        .to_string()
}

pub(super) fn utc_time(unix_seconds: i64) -> time::OffsetDateTime {
    time::OffsetDateTime::from_unix_timestamp(unix_seconds)
        .expect("test timestamp should be representable")
}

pub(super) fn snapshot_proxy_with_clock(clock: Arc<Mutex<time::OffsetDateTime>>) -> DraftProxy {
    snapshot_proxy().with_clock(move || *clock.lock().unwrap())
}

pub(super) fn set_clock(clock: &Arc<Mutex<time::OffsetDateTime>>, unix_seconds: i64) {
    *clock.lock().unwrap() = utc_time(unix_seconds);
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

pub(super) fn log_snapshot(proxy: &DraftProxy) -> Value {
    meta_snapshot(proxy, "/__meta/log")
}

pub(super) fn state_snapshot(proxy: &DraftProxy) -> Value {
    meta_snapshot(proxy, "/__meta/state")
}

pub(super) fn restore_state_with(proxy: &mut DraftProxy, mutate: impl FnOnce(&mut Value)) {
    let dump = proxy.process_request(request_with_body("POST", "/__meta/dump", ""));
    assert_eq!(dump.status, 200);
    let mut restored = dump.body;
    mutate(&mut restored["state"]);
    let restore = proxy.process_request(request_with_body(
        "POST",
        "/__meta/restore",
        &restored.to_string(),
    ));
    assert_eq!(restore.status, 200);
}

pub(super) fn restore_shop_domain_context(
    proxy: &mut DraftProxy,
    myshopify_domain: &str,
    primary_domain_host: &str,
) -> String {
    let domain_id = "gid://shopify/Domain/1000".to_string();
    restore_state_with(proxy, |state| {
        state["baseState"]["shop"] = json!({
            "id": "gid://shopify/Shop/domain-context",
            "name": "Domain context shop",
            "myshopifyDomain": myshopify_domain,
            "primaryDomain": {
                "id": domain_id,
                "host": primary_domain_host,
                "url": format!("https://{primary_domain_host}"),
                "sslEnabled": true
            },
            "domains": [{
                "id": domain_id,
                "host": primary_domain_host,
                "url": format!("https://{primary_domain_host}"),
                "sslEnabled": true
            }]
        });
    });
    domain_id
}

fn meta_snapshot(proxy: &DraftProxy, path: &str) -> Value {
    let mut proxy = proxy.clone();
    let response = proxy.process_request(request_with_body("GET", path, ""));
    assert_eq!(response.status, 200);
    response.body
}

pub(super) fn json_graphql_request(query: &str, variables: serde_json::Value) -> Request {
    graphql_request(
        "POST",
        &json!({ "query": query, "variables": variables }).to_string(),
    )
}

pub(super) fn omit_user_error_code_selection(query: &str) -> String {
    let query = query
        .replace("field message code", "field message")
        .replace("field code message", "field message")
        .replace("code field message", "field message");
    let mut output = String::new();
    let mut in_plain_user_errors = false;
    for line in query.lines() {
        let trimmed = line.trim();
        if in_plain_user_errors && trimmed == "code" {
            continue;
        }
        output.push_str(line);
        output.push('\n');
        if trimmed.starts_with("userErrors") && trimmed.ends_with('{') {
            in_plain_user_errors = true;
        } else if in_plain_user_errors && trimmed == "}" {
            in_plain_user_errors = false;
        }
    }
    output
}

pub(super) fn strip_user_error_codes(value: &Value) -> Value {
    match value {
        Value::Array(items) => Value::Array(items.iter().map(strip_user_error_codes).collect()),
        Value::Object(object) => {
            let mut stripped = serde_json::Map::new();
            for (key, child) in object {
                if key == "userErrors" {
                    stripped.insert(key.clone(), strip_code_from_user_errors(child));
                } else {
                    stripped.insert(key.clone(), strip_user_error_codes(child));
                }
            }
            Value::Object(stripped)
        }
        _ => value.clone(),
    }
}

fn strip_code_from_user_errors(value: &Value) -> Value {
    match value {
        Value::Array(items) => Value::Array(
            items
                .iter()
                .map(|item| match item {
                    Value::Object(object) => {
                        let mut stripped = object.clone();
                        stripped.remove("code");
                        Value::Object(stripped)
                    }
                    _ => item.clone(),
                })
                .collect(),
        ),
        _ => strip_user_error_codes(value),
    }
}

pub(super) fn restore_shop_currency(proxy: &mut DraftProxy, currency_code: &str) {
    restore_state_with(proxy, |state| {
        state["baseState"]["shop"]["currencyCode"] = json!(currency_code);
    });
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
        mutation CreateVariantForTest($productId: ID!, $variants: [ProductVariantsBulkInput!]!) {
          productVariantsBulkCreate(productId: $productId, variants: $variants) {
            productVariants { id sku price inventoryItem { id } }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "productId": product_id,
            "variants": [{
                "inventoryItem": { "sku": sku },
                "optionValues": [{ "optionName": "Title", "name": sku }],
                "price": price
            }]
        }),
    ));
    assert_eq!(response.status, 200);
    assert_eq!(
        response.body["data"]["productVariantsBulkCreate"]["userErrors"],
        json!([])
    );
    response.body["data"]["productVariantsBulkCreate"]["productVariants"][0].clone()
}

pub(super) fn registry_entry(
    name: &str,
    operation_type: OperationType,
    implemented: bool,
) -> OperationRegistryEntry {
    OperationRegistryEntry {
        api_surface: ApiSurface::Admin,
        name: name.to_string(),
        operation_type,
        domain: CapabilityDomain::Products,
        implemented,
        runtime_tests: vec!["tests/graphql_routes.rs".to_string()],
        commit_id_mappings: Vec::new(),
    }
}

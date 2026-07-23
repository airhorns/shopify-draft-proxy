#![allow(dead_code)]

#[path = "graphql_routes/common.rs"]
mod common;

use common::*;
use pretty_assertions::assert_eq;

fn product_set_hydrated_product_node(id: &str, handle: &str) -> Value {
    json!({
        "id": id,
        "title": "Hydrated product",
        "handle": handle,
        "status": "ACTIVE",
        "vendor": "Hydrated Vendor",
        "productType": "Hydrated Type",
        "tags": ["hydrated"],
        "totalInventory": 5,
        "tracksInventory": true,
        "createdAt": "2024-01-01T00:00:00Z",
        "updatedAt": "2024-01-02T00:00:00Z",
        "descriptionHtml": "<p>Hydrated</p>",
        "onlineStorePreviewUrl": "https://example.myshopify.com/products/hydrated",
        "templateSuffix": null,
        "seo": { "title": "Hydrated SEO", "description": "Hydrated description" },
        "options": [{
            "id": "gid://shopify/ProductOption/101",
            "name": "Color",
            "position": 1,
            "values": ["Blue"],
            "optionValues": [{
                "id": "gid://shopify/ProductOptionValue/201",
                "name": "Blue",
                "hasVariants": true
            }]
        }],
        "variants": {
            "nodes": [{
                "id": "gid://shopify/ProductVariant/301",
                "title": "Blue",
                "sku": "HYD-BLUE",
                "barcode": "HYD-BC",
                "price": "12.00",
                "compareAtPrice": null,
                "taxable": true,
                "inventoryPolicy": "DENY",
                "inventoryQuantity": 5,
                "selectedOptions": [{ "name": "Color", "value": "Blue" }],
                "inventoryItem": {
                    "id": "gid://shopify/InventoryItem/401",
                    "tracked": false,
                    "requiresShipping": true
                }
            }]
        },
        "collections": {
            "nodes": [{
                "id": "gid://shopify/Collection/501",
                "title": "Hydrated Collection",
                "handle": "hydrated-collection"
            }],
            "pageInfo": {
                "hasNextPage": false,
                "hasPreviousPage": false
            }
        }
    })
}

#[test]
fn product_set_live_hybrid_hydrates_unobserved_existing_id_before_update() {
    let product_id = "gid://shopify/Product/7001";
    let upstream_calls = Arc::new(Mutex::new(Vec::<Value>::new()));
    let captured_calls = Arc::clone(&upstream_calls);
    let mut proxy = configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport({
        let product = product_set_hydrated_product_node(product_id, "hydrated-id-product");
        move |request| {
            let body = serde_json::from_str::<Value>(&request.body).unwrap();
            captured_calls.lock().unwrap().push(body.clone());
            assert_eq!(body["operationName"], json!("ProductSetTargetHydrateById"));
            Response {
                status: 200,
                headers: Default::default(),
                body: json!({ "data": { "nodes": [product.clone()] } }),
            }
        }
    });
    assert_eq!(
        proxy
            .process_request(request_with_body("GET", "/__meta/config", ""))
            .body["runtime"]["readMode"],
        json!("live-hybrid")
    );

    let response = proxy.process_request(json_graphql_request(
        r#"
        mutation ProductSetHydrateColdId($identifier: ProductSetIdentifiers, $input: ProductSetInput!, $synchronous: Boolean!) {
          productSet(identifier: $identifier, input: $input, synchronous: $synchronous) {
            product {
              id
              title
              handle
              status
              vendor
              onlineStorePreviewUrl
              options { id name values optionValues { id name hasVariants } }
              variants(first: 10) {
                nodes { id title sku inventoryItem { id tracked } }
              }
            }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "synchronous": true,
            "identifier": { "id": product_id },
            "input": { "title": "Hydrated ID update" }
        }),
    ));

    assert_eq!(response.status, 200);
    assert_eq!(response.body["data"]["productSet"]["userErrors"], json!([]));
    let product = &response.body["data"]["productSet"]["product"];
    assert_eq!(product["id"], json!(product_id));
    assert_eq!(product["title"], json!("Hydrated ID update"));
    assert_eq!(product["handle"], json!("hydrated-id-product"));
    assert_eq!(product["status"], json!("ACTIVE"));
    assert_eq!(product["vendor"], json!("Hydrated Vendor"));
    assert_eq!(
        product["onlineStorePreviewUrl"],
        json!("https://example.myshopify.com/products/hydrated")
    );
    assert_eq!(
        product["options"][0]["id"],
        json!("gid://shopify/ProductOption/101")
    );
    assert_eq!(product["options"][0]["values"], json!(["Blue"]));
    assert_eq!(
        product["variants"]["nodes"][0]["id"],
        json!("gid://shopify/ProductVariant/301")
    );
    assert_eq!(
        product["variants"]["nodes"][0]["inventoryItem"]["tracked"],
        json!(false)
    );

    let calls = upstream_calls.lock().unwrap();
    assert_eq!(calls.len(), 1);
    assert!(calls[0]["query"]
        .as_str()
        .is_some_and(|query| query.contains("query ProductSetTargetHydrateById")));
    assert!(
        !calls[0]["query"]
            .as_str()
            .unwrap_or_default()
            .contains("productSet("),
        "hydration must not forward the original productSet mutation"
    );
    assert_eq!(calls[0]["variables"]["ids"], json!([product_id]));
    assert_eq!(
        log_snapshot(&proxy)["entries"][0]["stagedResourceIds"],
        json!([product_id])
    );
}

#[test]
fn product_set_live_hybrid_hydrates_unobserved_existing_handle_before_update() {
    let product_id = "gid://shopify/Product/7002";
    let handle = "hydrated-handle-product";
    let upstream_calls = Arc::new(Mutex::new(Vec::<Value>::new()));
    let captured_calls = Arc::clone(&upstream_calls);
    let mut proxy = configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport({
        let product = product_set_hydrated_product_node(product_id, handle);
        move |request| {
            let body = serde_json::from_str::<Value>(&request.body).unwrap();
            captured_calls.lock().unwrap().push(body.clone());
            assert_eq!(
                body["operationName"],
                json!("ProductSetTargetHydrateByHandle")
            );
            Response {
                status: 200,
                headers: Default::default(),
                body: json!({ "data": { "productByIdentifier": product.clone() } }),
            }
        }
    });

    let response = proxy.process_request(json_graphql_request(
        r#"
        mutation ProductSetHydrateColdHandle($identifier: ProductSetIdentifiers, $input: ProductSetInput!, $synchronous: Boolean!) {
          productSet(identifier: $identifier, input: $input, synchronous: $synchronous) {
            product {
              id
              title
              handle
              vendor
              options { id name values optionValues { id name hasVariants } }
              variants(first: 10) {
                nodes { id sku selectedOptions { name value } }
              }
            }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "synchronous": true,
            "identifier": { "handle": handle },
            "input": { "title": "Hydrated handle update", "handle": handle }
        }),
    ));

    assert_eq!(response.status, 200);
    assert_eq!(response.body["data"]["productSet"]["userErrors"], json!([]));
    let product = &response.body["data"]["productSet"]["product"];
    assert_eq!(product["id"], json!(product_id));
    assert_eq!(product["title"], json!("Hydrated handle update"));
    assert_eq!(product["handle"], json!(handle));
    assert_eq!(product["vendor"], json!("Hydrated Vendor"));
    assert_eq!(
        product["options"][0]["id"],
        json!("gid://shopify/ProductOption/101")
    );
    assert_eq!(
        product["variants"]["nodes"][0]["selectedOptions"],
        json!([{ "name": "Color", "value": "Blue" }])
    );

    let calls = upstream_calls.lock().unwrap();
    assert_eq!(calls.len(), 1);
    assert!(calls[0]["query"]
        .as_str()
        .is_some_and(|query| query.contains("query ProductSetTargetHydrateByHandle")));
    assert!(
        !calls[0]["query"]
            .as_str()
            .unwrap_or_default()
            .contains("productSet("),
        "hydration must not forward the original productSet mutation"
    );
    assert_eq!(calls[0]["variables"]["handle"], json!(handle));
    assert_eq!(
        log_snapshot(&proxy)["entries"][0]["stagedResourceIds"],
        json!([product_id])
    );
}

#[test]
fn product_set_live_hybrid_preserves_missing_identifier_behaviors_after_hydration_miss() {
    let missing_id = "gid://shopify/Product/7999";
    let id_calls = Arc::new(Mutex::new(Vec::<Value>::new()));
    let captured_id_calls = Arc::clone(&id_calls);
    let mut id_proxy = configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport({
        move |request| {
            let body = serde_json::from_str::<Value>(&request.body).unwrap();
            captured_id_calls.lock().unwrap().push(body);
            Response {
                status: 200,
                headers: Default::default(),
                body: json!({ "data": { "nodes": [Value::Null] } }),
            }
        }
    });

    let missing_id_response = id_proxy.process_request(json_graphql_request(
        r#"
        mutation ProductSetMissingId($identifier: ProductSetIdentifiers, $input: ProductSetInput!, $synchronous: Boolean!) {
          productSet(identifier: $identifier, input: $input, synchronous: $synchronous) {
            product { id }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "synchronous": true,
            "identifier": { "id": missing_id },
            "input": { "title": "Missing ID update" }
        }),
    ));
    assert_eq!(missing_id_response.status, 200);
    assert_eq!(
        missing_id_response.body["data"]["productSet"],
        json!({
            "product": null,
            "userErrors": [{
                "field": ["input", "id"],
                "message": "Product does not exist",
                "code": "PRODUCT_DOES_NOT_EXIST"
            }]
        })
    );
    assert_eq!(id_calls.lock().unwrap().len(), 1);
    assert_eq!(log_snapshot(&id_proxy)["entries"], json!([]));
    assert_eq!(
        state_snapshot(&id_proxy)["stagedState"]["products"],
        json!({})
    );

    let missing_handle = "not-present-for-product-set";
    let handle_calls = Arc::new(Mutex::new(Vec::<Value>::new()));
    let captured_handle_calls = Arc::clone(&handle_calls);
    let mut handle_proxy = configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport({
        move |request| {
            let body = serde_json::from_str::<Value>(&request.body).unwrap();
            captured_handle_calls.lock().unwrap().push(body);
            Response {
                status: 200,
                headers: Default::default(),
                body: json!({ "data": { "productByIdentifier": Value::Null } }),
            }
        }
    });

    let missing_handle_response = handle_proxy.process_request(json_graphql_request(
        r#"
        mutation ProductSetMissingHandle($identifier: ProductSetIdentifiers, $input: ProductSetInput!, $synchronous: Boolean!) {
          productSet(identifier: $identifier, input: $input, synchronous: $synchronous) {
            product { id title handle }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "synchronous": true,
            "identifier": { "handle": missing_handle },
            "input": { "title": "Missing handle create", "handle": missing_handle }
        }),
    ));
    assert_eq!(missing_handle_response.status, 200);
    assert_eq!(
        missing_handle_response.body["data"]["productSet"]["userErrors"],
        json!([])
    );
    assert_eq!(
        missing_handle_response.body["data"]["productSet"]["product"]["handle"],
        json!(missing_handle)
    );
    let created_id = missing_handle_response.body["data"]["productSet"]["product"]["id"]
        .as_str()
        .unwrap_or_default();
    assert!(
        created_id.starts_with("gid://shopify/Product/")
            && created_id.ends_with("?shopify-draft-proxy=synthetic")
    );
    assert_eq!(handle_calls.lock().unwrap().len(), 1);
    assert_eq!(
        log_snapshot(&handle_proxy)["entries"][0]["stagedResourceIds"],
        json!([created_id])
    );
}

#[test]
fn product_set_preserves_an_observed_null_preview_url() {
    let product_id = "gid://shopify/Product/7101";
    let mut hydrated = product_set_hydrated_product_node(product_id, "preview-unavailable");
    hydrated["onlineStorePreviewUrl"] = Value::Null;
    let mut proxy = configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport({
        move |_| Response {
            status: 200,
            headers: Default::default(),
            body: json!({ "data": { "nodes": [hydrated.clone()] } }),
        }
    });

    let response = proxy.process_request(json_graphql_request(
        r#"
        mutation ProductSetObservedNullPreview($identifier: ProductSetIdentifiers, $input: ProductSetInput!) {
          productSet(identifier: $identifier, input: $input, synchronous: true) {
            product { id onlineStorePreviewUrl }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "identifier": { "id": product_id },
            "input": { "title": "Preview remains unavailable" }
        }),
    ));

    assert_eq!(response.status, 200);
    assert_eq!(response.body["data"]["productSet"]["userErrors"], json!([]));
    assert_eq!(
        response.body["data"]["productSet"]["product"]["onlineStorePreviewUrl"],
        Value::Null
    );
}

#[test]
fn snapshot_product_set_uses_null_preview_url_and_delete_hides_the_product() {
    let mut proxy = snapshot_proxy();
    let created = proxy.process_request(json_graphql_request(
        r#"
        mutation ProductSetSnapshotPreviewBoundary($input: ProductSetInput!) {
          productSet(input: $input, synchronous: true) {
            product { id status onlineStorePreviewUrl }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "input": { "title": "Snapshot preview boundary", "status": "DRAFT" } }),
    ));

    assert_eq!(created.status, 200);
    let product = &created.body["data"]["productSet"]["product"];
    let product_id = product["id"]
        .as_str()
        .expect("productSet should return an id");
    assert_eq!(product["status"], json!("DRAFT"));
    assert_eq!(product["onlineStorePreviewUrl"], Value::Null);

    let read = proxy.process_request(json_graphql_request(
        "query ProductSetSnapshotPreviewRead($id: ID!) { product(id: $id) { id onlineStorePreviewUrl } }",
        json!({ "id": product_id }),
    ));
    assert_eq!(
        read.body["data"]["product"]["onlineStorePreviewUrl"],
        Value::Null
    );

    let deleted = proxy.process_request(json_graphql_request(
        "mutation ProductSetSnapshotPreviewDelete($input: ProductDeleteInput!) { productDelete(input: $input) { deletedProductId userErrors { field message } } }",
        json!({ "input": { "id": product_id } }),
    ));
    assert_eq!(
        deleted.body["data"]["productDelete"]["deletedProductId"],
        json!(product_id)
    );

    let deleted_read = proxy.process_request(json_graphql_request(
        "query ProductSetDeletedPreviewRead($id: ID!) { product(id: $id) { id onlineStorePreviewUrl } }",
        json!({ "id": product_id }),
    ));
    assert_eq!(deleted_read.body["data"]["product"], Value::Null);
}

#[test]
fn product_duplicate_does_not_copy_the_source_products_authoritative_preview_url() {
    let source_id = "gid://shopify/Product/7201";
    let mut proxy = configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport({
        let source = product_set_hydrated_product_node(source_id, "duplicate-preview-source");
        move |_| Response {
            status: 200,
            headers: Default::default(),
            body: json!({ "data": { "nodes": [source.clone()] } }),
        }
    });

    let response = proxy.process_request(json_graphql_request(
        r#"
        mutation ProductDuplicatePreviewBoundary($productId: ID!, $newTitle: String!) {
          productDuplicate(productId: $productId, newTitle: $newTitle, synchronous: true) {
            newProduct { id onlineStorePreviewUrl }
            userErrors { field message }
          }
        }
        "#,
        json!({ "productId": source_id, "newTitle": "Duplicate preview boundary" }),
    ));

    assert_eq!(response.status, 200);
    assert_eq!(
        response.body["data"]["productDuplicate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        response.body["data"]["productDuplicate"]["newProduct"]["onlineStorePreviewUrl"],
        Value::Null
    );
}

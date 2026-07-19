#![allow(dead_code)]

#[path = "graphql_routes/common.rs"]
mod common;

use common::*;
use pretty_assertions::assert_eq;

fn product(id: &str, title: &str, handle: &str) -> ProductRecord {
    ProductRecord {
        id: id.to_string(),
        title: title.to_string(),
        handle: handle.to_string(),
        status: "DRAFT".to_string(),
        ..ProductRecord::default()
    }
}

fn product_create(proxy: &mut DraftProxy, title: &str, handle: Option<&str>) -> Value {
    let mut product = json!({ "title": title, "status": "DRAFT" });
    if let Some(handle) = handle {
        product["handle"] = json!(handle);
    }
    proxy
        .process_request(json_graphql_request(
            r#"
            mutation ProductHandleCreate($product: ProductCreateInput!) {
              productCreate(product: $product) {
                product { id title handle }
                userErrors { field message }
              }
            }
            "#,
            json!({ "product": product }),
        ))
        .body["data"]["productCreate"]
        .clone()
}

fn product_update(proxy: &mut DraftProxy, input: Value) -> Value {
    proxy
        .process_request(json_graphql_request(
            r#"
            mutation ProductHandleUpdate($product: ProductUpdateInput!) {
              productUpdate(product: $product) {
                product { id title handle vendor }
                userErrors { field message }
              }
            }
            "#,
            json!({ "product": input }),
        ))
        .body["data"]["productUpdate"]
        .clone()
}

fn product_set(proxy: &mut DraftProxy, identifier: Value, input: Value) -> Value {
    proxy
        .process_request(json_graphql_request(
            r#"
            mutation ProductHandleSet(
              $identifier: ProductSetIdentifiers
              $input: ProductSetInput!
            ) {
              productSet(identifier: $identifier, input: $input, synchronous: true) {
                product { id title handle vendor }
                userErrors { field message }
              }
            }
            "#,
            json!({ "identifier": identifier, "input": input }),
        ))
        .body["data"]["productSet"]
        .clone()
}

#[test]
fn product_create_normalizes_and_reserves_handles_across_base_and_staged_products() {
    let mut proxy = snapshot_proxy().with_base_products(vec![
        product(
            "gid://shopify/Product/base-title",
            "Repeated title 41",
            "repeated-title-41",
        ),
        product(
            "gid://shopify/Product/base-fallback",
            "Fallback owner",
            "product",
        ),
    ]);

    let first_generated = product_create(&mut proxy, "Repeated title 41", None);
    let second_generated = product_create(&mut proxy, "Repeated title 41", None);
    let unicode_explicit = product_create(
        &mut proxy,
        "Unicode explicit",
        Some("  Mixed CASE / 東京 100 % "),
    );
    let punctuation_fallback = product_create(&mut proxy, "Punctuation explicit", Some("%%%"));
    let collision = product_create(&mut proxy, "Collision", Some(" REPEATED title 41 "));

    assert_eq!(
        first_generated["product"]["handle"],
        json!("repeated-title-42")
    );
    assert_eq!(
        second_generated["product"]["handle"],
        json!("repeated-title-43")
    );
    assert_eq!(
        unicode_explicit["product"]["handle"],
        json!("mixed-case-東京-100")
    );
    assert_eq!(
        punctuation_fallback["product"]["handle"],
        json!("product-1")
    );
    assert_eq!(collision["product"], Value::Null);
    assert_eq!(
        collision["userErrors"],
        json!([{
            "field": ["input", "handle"],
            "message": "Handle ' REPEATED title 41 ' already in use. Please provide a new handle."
        }])
    );
}

#[test]
fn product_update_normalizes_rejects_collisions_and_keeps_sticky_handle() {
    let target_id = "gid://shopify/Product/update-target";
    let mut proxy = snapshot_proxy().with_base_products(vec![
        product(target_id, "Sticky title", "sticky-handle"),
        product(
            "gid://shopify/Product/collision-owner",
            "Collision owner",
            "taken-handle",
        ),
    ]);

    let normalized = product_update(
        &mut proxy,
        json!({ "id": target_id, "handle": "  New / 東京  " }),
    );
    let title_only = product_update(
        &mut proxy,
        json!({ "id": target_id, "title": "Renamed sticky title" }),
    );
    let sparse = product_update(
        &mut proxy,
        json!({ "id": target_id, "vendor": "Sticky vendor" }),
    );
    let blank = product_update(&mut proxy, json!({ "id": target_id, "handle": "   " }));
    let collision = product_update(
        &mut proxy,
        json!({ "id": target_id, "handle": " TAKEN handle " }),
    );

    assert_eq!(normalized["product"]["handle"], json!("new-東京"));
    assert_eq!(title_only["product"]["handle"], json!("new-東京"));
    assert_eq!(sparse["product"]["handle"], json!("new-東京"));
    assert_eq!(blank["product"]["handle"], json!("renamed-sticky-title"));
    assert_eq!(
        collision["product"]["handle"],
        json!("renamed-sticky-title")
    );
    assert_eq!(
        collision["userErrors"],
        json!([{
            "field": ["input", "handle"],
            "message": "Handle ' TAKEN handle ' already in use. Please provide a new handle."
        }])
    );
}

#[test]
fn product_set_uses_generated_and_explicit_handle_reservations() {
    let mut proxy = snapshot_proxy();
    let first = product_set(
        &mut proxy,
        Value::Null,
        json!({ "title": "Product set 900", "status": "DRAFT" }),
    );
    let second = product_set(
        &mut proxy,
        Value::Null,
        json!({ "title": "Product set 900", "status": "DRAFT" }),
    );
    let first_id = first["product"]["id"]
        .as_str()
        .expect("productSet create should return a product id");
    let normalized = product_set(
        &mut proxy,
        json!({ "id": first_id }),
        json!({ "handle": "  Set / 東京  " }),
    );
    let blank = product_set(
        &mut proxy,
        json!({ "id": first_id }),
        json!({ "handle": "   " }),
    );
    let collision = product_set(
        &mut proxy,
        json!({ "id": first_id }),
        json!({ "handle": " PRODUCT set 901 " }),
    );

    assert_eq!(first["product"]["handle"], json!("product-set-900"));
    assert_eq!(second["product"]["handle"], json!("product-set-901"));
    assert_eq!(normalized["product"]["handle"], json!("set-東京"));
    assert_eq!(blank["product"]["handle"], json!("product-set-900"));
    assert_eq!(collision["product"]["handle"], json!("product-set-900"));
    assert_eq!(
        collision["userErrors"],
        json!([{
            "field": ["input", "handle"],
            "message": "Handle ' PRODUCT set 901 ' already in use. Please provide a new handle."
        }])
    );
}

#[test]
fn product_duplicate_reserves_the_next_generated_handle() {
    let mut proxy = snapshot_proxy();
    let source = product_create(&mut proxy, "Source product", Some("source-product"));
    let _owner = product_create(&mut proxy, "Duplicate title 7", None);
    let source_id = source["product"]["id"]
        .as_str()
        .expect("productCreate should return a product id");

    let duplicate = proxy.process_request(json_graphql_request(
        r#"
        mutation ProductHandleDuplicate($productId: ID!, $newTitle: String!) {
          productDuplicate(productId: $productId, newTitle: $newTitle) {
            newProduct { id title handle }
            userErrors { field message }
          }
        }
        "#,
        json!({ "productId": source_id, "newTitle": "Duplicate title 7" }),
    ));

    assert_eq!(duplicate.status, 200);
    assert_eq!(
        duplicate.body["data"]["productDuplicate"]["newProduct"]["handle"],
        json!("duplicate-title-8")
    );
    assert_eq!(
        duplicate.body["data"]["productDuplicate"]["userErrors"],
        json!([])
    );
}

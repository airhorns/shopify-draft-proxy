use pretty_assertions::assert_eq;
use serde_json::{json, Value};
use shopify_draft_proxy::proxy::{Config, DraftProxy, ReadMode, Request};

fn snapshot_proxy() -> DraftProxy {
    DraftProxy::new(Config {
        read_mode: ReadMode::Snapshot,
        unsupported_mutation_mode: None,
        port: 0,
        shopify_admin_origin: "https://shopify.com".to_string(),
        snapshot_path: None,
        bulk_operation_run_mutation_max_input_file_size_bytes: None,
    })
}

fn json_graphql_request(query: &str, variables: Value) -> Request {
    Request {
        method: "POST".to_string(),
        path: "/admin/api/2026-04/graphql.json".to_string(),
        headers: Default::default(),
        body: json!({ "query": query, "variables": variables }).to_string(),
    }
}

fn json_string(value: &Value, context: &str) -> String {
    value
        .as_str()
        .unwrap_or_else(|| panic!("{context} should be a string, got {value}"))
        .to_string()
}

fn create_product_with_variant(proxy: &mut DraftProxy, title: &str, sku: &str) -> (String, String) {
    let product = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateEntitledProduct($product: ProductCreateInput!) {
          productCreate(product: $product) {
            product { id }
            userErrors { field message }
          }
        }
        "#,
        json!({ "product": { "title": title } }),
    ));
    assert_eq!(
        product.body["data"]["productCreate"]["userErrors"],
        json!([])
    );
    let product_id = json_string(
        &product.body["data"]["productCreate"]["product"]["id"],
        "entitled product id",
    );
    let variant = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateEntitledVariant($input: ProductVariantInput!) {
          productVariantCreate(input: $input) {
            productVariant { id }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "input": {
                "productId": product_id,
                "title": sku,
                "sku": sku,
                "price": "10.00"
            }
        }),
    ));
    assert_eq!(
        variant.body["data"]["productVariantCreate"]["userErrors"],
        json!([])
    );
    let variant_id = json_string(
        &variant.body["data"]["productVariantCreate"]["productVariant"]["id"],
        "entitled variant id",
    );
    (product_id, variant_id)
}

fn create_collection(proxy: &mut DraftProxy, title: &str) -> String {
    let collection = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateEntitledCollection($input: CollectionInput!) {
          collectionCreate(input: $input) {
            collection { id }
            userErrors { field message }
          }
        }
        "#,
        json!({ "input": { "title": title } }),
    ));
    assert_eq!(
        collection.body["data"]["collectionCreate"]["userErrors"],
        json!([])
    );
    json_string(
        &collection.body["data"]["collectionCreate"]["collection"]["id"],
        "entitled collection id",
    )
}

fn create_bxgy_discount(proxy: &mut DraftProxy, title: &str, code: &str, items: Value) -> String {
    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateEntitledBxgyDiscount($input: DiscountCodeBxgyInput!) {
          discountCodeBxgyCreate(bxgyCodeDiscount: $input) {
            codeDiscountNode { id }
            userErrors { field message code extraInfo }
          }
        }
        "#,
        json!({
            "input": {
                "title": title,
                "code": code,
                "startsAt": "2026-04-25T00:00:00Z",
                "context": { "all": "ALL" },
                "customerBuys": { "value": { "quantity": "1" }, "items": items },
                "customerGets": {
                    "value": {
                        "discountOnQuantity": {
                            "quantity": "1",
                            "effect": { "percentage": 0.5 }
                        }
                    },
                    "items": items
                }
            }
        }),
    ));
    assert_eq!(
        create.body["data"]["discountCodeBxgyCreate"]["userErrors"],
        json!([])
    );
    json_string(
        &create.body["data"]["discountCodeBxgyCreate"]["codeDiscountNode"]["id"],
        "entitled bxgy discount id",
    )
}

#[test]
fn discount_entitlement_connections_resolve_resources_and_connection_windows() {
    let mut proxy = snapshot_proxy();
    let (alpha_product_id, alpha_variant_id) =
        create_product_with_variant(&mut proxy, "Entitled Alpha", "ENTITLED-ALPHA");
    let (bravo_product_id, bravo_variant_id) =
        create_product_with_variant(&mut proxy, "Entitled Bravo", "ENTITLED-BRAVO");
    let alpha_collection_id = create_collection(&mut proxy, "Entitled Alpha Collection");
    let bravo_collection_id = create_collection(&mut proxy, "Entitled Bravo Collection");
    let product_discount_id = create_bxgy_discount(
        &mut proxy,
        "Product and variant entitlements",
        "ENTITLEPRODUCTS",
        json!({
            "products": {
                "productsToAdd": [alpha_product_id, bravo_product_id],
                "productVariantsToAdd": [alpha_variant_id, bravo_variant_id]
            }
        }),
    );
    let collection_discount_id = create_bxgy_discount(
        &mut proxy,
        "Collection entitlements",
        "ENTITLECOLLECTIONS",
        json!({ "collections": { "add": [alpha_collection_id, bravo_collection_id] } }),
    );

    let read = proxy.process_request(json_graphql_request(
        r#"
        query EntitlementConnections(
          $productDiscountId: ID!
          $collectionDiscountId: ID!
          $productAfter: String!
          $productBefore: String!
          $variantAfter: String!
          $variantBefore: String!
          $collectionAfter: String!
          $collectionBefore: String!
        ) {
          productDiscount: discountNode(id: $productDiscountId) {
            discount {
              __typename
              ... on DiscountCodeBxgy {
                customerBuys {
                  items {
                    __typename
                    ... on DiscountProducts {
                      products(first: 1) { nodes { id title } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } }
                      productVariants(first: 1) { nodes { id sku product { id title } } }
                    }
                  }
                }
                customerGets {
                  items {
                    __typename
                    ... on DiscountProducts {
                      firstProducts: products(first: 1) {
                        nodes { id title }
                        edges { cursor node { id title } }
                        pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
                      }
                      afterProducts: products(first: 1, after: $productAfter) { nodes { id title } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } }
                      beforeProducts: products(last: 1, before: $productBefore) { nodes { id title } }
                      reverseProducts: products(first: 2, reverse: true) { nodes { id title } }
                      firstVariants: productVariants(first: 1) {
                        nodes { id title sku product { id title } }
                        edges { cursor node { id sku } }
                        pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
                      }
                      afterVariants: productVariants(first: 1, after: $variantAfter) { nodes { id sku } }
                      beforeVariants: productVariants(last: 1, before: $variantBefore) { nodes { id sku } }
                      reverseVariants: productVariants(first: 2, reverse: true) { nodes { id sku } }
                    }
                  }
                }
              }
            }
          }
          collectionDiscount: discountNode(id: $collectionDiscountId) {
            discount {
              __typename
              ... on DiscountCodeBxgy {
                customerBuys {
                  items {
                    __typename
                    ... on DiscountCollections {
                      collections(first: 1) { nodes { id title handle } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } }
                    }
                  }
                }
                customerGets {
                  items {
                    __typename
                    ... on DiscountCollections {
                      firstCollections: collections(first: 1) {
                        nodes { id title handle }
                        edges { cursor node { id title } }
                        pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
                      }
                      afterCollections: collections(first: 1, after: $collectionAfter) { nodes { id title } }
                      beforeCollections: collections(last: 1, before: $collectionBefore) { nodes { id title } }
                      reverseCollections: collections(first: 2, reverse: true) { nodes { id title } }
                    }
                  }
                }
              }
            }
          }
          catalog: discountNodes(first: 10) {
            nodes {
              id
              discount {
                __typename
                ... on DiscountCodeBxgy {
                  customerBuys {
                    items {
                      __typename
                      ... on DiscountProducts { products(first: 2) { nodes { id title } } }
                      ... on DiscountCollections { collections(first: 2) { nodes { id title } } }
                    }
                  }
                  customerGets {
                    items {
                      __typename
                      ... on DiscountProducts { products(first: 2) { nodes { id title } } }
                      ... on DiscountCollections { collections(first: 2) { nodes { id title } } }
                    }
                  }
                }
              }
            }
          }
        }
        "#,
        json!({
            "productDiscountId": product_discount_id,
            "collectionDiscountId": collection_discount_id,
            "productAfter": alpha_product_id,
            "productBefore": bravo_product_id,
            "variantAfter": alpha_variant_id,
            "variantBefore": bravo_variant_id,
            "collectionAfter": alpha_collection_id,
            "collectionBefore": bravo_collection_id
        }),
    ));
    assert_eq!(read.status, 200);
    let products = &read.body["data"]["productDiscount"]["discount"]["customerGets"]["items"];
    assert_eq!(
        products["firstProducts"],
        json!({
            "nodes": [{ "id": alpha_product_id, "title": "Entitled Alpha" }],
            "edges": [{ "cursor": alpha_product_id, "node": { "id": alpha_product_id, "title": "Entitled Alpha" } }],
            "pageInfo": {
                "hasNextPage": true,
                "hasPreviousPage": false,
                "startCursor": alpha_product_id,
                "endCursor": alpha_product_id
            }
        })
    );
    assert_eq!(
        products["afterProducts"]["nodes"],
        json!([{ "id": bravo_product_id, "title": "Entitled Bravo" }])
    );
    assert_eq!(
        products["beforeProducts"]["nodes"],
        json!([{ "id": alpha_product_id, "title": "Entitled Alpha" }])
    );
    assert_eq!(
        products["reverseProducts"]["nodes"],
        json!([
            { "id": bravo_product_id, "title": "Entitled Bravo" },
            { "id": alpha_product_id, "title": "Entitled Alpha" }
        ])
    );
    assert_eq!(
        products["firstVariants"]["nodes"],
        json!([{
            "id": alpha_variant_id,
            "title": "ENTITLED-ALPHA",
            "sku": "ENTITLED-ALPHA",
            "product": { "id": alpha_product_id, "title": "Entitled Alpha" }
        }])
    );
    assert_eq!(
        products["firstVariants"]["edges"],
        json!([{ "cursor": alpha_variant_id, "node": { "id": alpha_variant_id, "sku": "ENTITLED-ALPHA" } }])
    );
    assert_eq!(
        products["afterVariants"]["nodes"],
        json!([{ "id": bravo_variant_id, "sku": "ENTITLED-BRAVO" }])
    );
    assert_eq!(
        products["beforeVariants"]["nodes"],
        json!([{ "id": alpha_variant_id, "sku": "ENTITLED-ALPHA" }])
    );
    assert_eq!(
        products["reverseVariants"]["nodes"],
        json!([
            { "id": bravo_variant_id, "sku": "ENTITLED-BRAVO" },
            { "id": alpha_variant_id, "sku": "ENTITLED-ALPHA" }
        ])
    );
    let product_buys = &read.body["data"]["productDiscount"]["discount"]["customerBuys"]["items"];
    assert_eq!(
        product_buys["products"]["nodes"],
        json!([{ "id": alpha_product_id, "title": "Entitled Alpha" }])
    );
    assert_eq!(
        product_buys["productVariants"]["nodes"],
        json!([{ "id": alpha_variant_id, "sku": "ENTITLED-ALPHA", "product": { "id": alpha_product_id, "title": "Entitled Alpha" } }])
    );

    let collections = &read.body["data"]["collectionDiscount"]["discount"]["customerGets"]["items"];
    assert_eq!(
        collections["firstCollections"],
        json!({
            "nodes": [{ "id": alpha_collection_id, "title": "Entitled Alpha Collection", "handle": "entitled-alpha-collection" }],
            "edges": [{ "cursor": alpha_collection_id, "node": { "id": alpha_collection_id, "title": "Entitled Alpha Collection" } }],
            "pageInfo": {
                "hasNextPage": true,
                "hasPreviousPage": false,
                "startCursor": alpha_collection_id,
                "endCursor": alpha_collection_id
            }
        })
    );
    assert_eq!(
        collections["afterCollections"]["nodes"],
        json!([{ "id": bravo_collection_id, "title": "Entitled Bravo Collection" }])
    );
    assert_eq!(
        collections["beforeCollections"]["nodes"],
        json!([{ "id": alpha_collection_id, "title": "Entitled Alpha Collection" }])
    );
    assert_eq!(
        collections["reverseCollections"]["nodes"],
        json!([
            { "id": bravo_collection_id, "title": "Entitled Bravo Collection" },
            { "id": alpha_collection_id, "title": "Entitled Alpha Collection" }
        ])
    );
    assert_eq!(
        read.body["data"]["collectionDiscount"]["discount"]["customerBuys"]["items"]["collections"]
            ["nodes"],
        json!([{ "id": alpha_collection_id, "title": "Entitled Alpha Collection", "handle": "entitled-alpha-collection" }])
    );

    let catalog_nodes = read.body["data"]["catalog"]["nodes"].as_array().unwrap();
    let product_catalog = catalog_nodes
        .iter()
        .find(|node| node.get("id") == Some(&json!(product_discount_id)))
        .expect("product entitlement discount should be in catalog");
    assert_eq!(
        product_catalog["discount"]["customerGets"]["items"]["products"]["nodes"],
        json!([
            { "id": alpha_product_id, "title": "Entitled Alpha" },
            { "id": bravo_product_id, "title": "Entitled Bravo" }
        ])
    );
    assert_eq!(
        product_catalog["discount"]["customerBuys"]["items"]["products"]["nodes"],
        json!([
            { "id": alpha_product_id, "title": "Entitled Alpha" },
            { "id": bravo_product_id, "title": "Entitled Bravo" }
        ])
    );
    let collection_catalog = catalog_nodes
        .iter()
        .find(|node| node.get("id") == Some(&json!(collection_discount_id)))
        .expect("collection entitlement discount should be in catalog");
    assert_eq!(
        collection_catalog["discount"]["customerGets"]["items"]["collections"]["nodes"],
        json!([
            { "id": alpha_collection_id, "title": "Entitled Alpha Collection" },
            { "id": bravo_collection_id, "title": "Entitled Bravo Collection" }
        ])
    );
    assert_eq!(
        collection_catalog["discount"]["customerBuys"]["items"]["collections"]["nodes"],
        json!([
            { "id": alpha_collection_id, "title": "Entitled Alpha Collection" },
            { "id": bravo_collection_id, "title": "Entitled Bravo Collection" }
        ])
    );
}

#[test]
fn discount_entitlement_connections_follow_staged_resource_updates_and_deletes() {
    let mut proxy = snapshot_proxy();
    let (product_id, variant_id) =
        create_product_with_variant(&mut proxy, "Entitled Mutable", "ENTITLED-MUTABLE");
    let collection_id = create_collection(&mut proxy, "Entitled Mutable Collection");
    let product_discount_id = create_bxgy_discount(
        &mut proxy,
        "Mutable product entitlements",
        "MUTABLEPRODUCTS",
        json!({
            "products": {
                "productsToAdd": [product_id],
                "productVariantsToAdd": [variant_id]
            }
        }),
    );
    let collection_discount_id = create_bxgy_discount(
        &mut proxy,
        "Mutable collection entitlements",
        "MUTABLECOLLECTIONS",
        json!({ "collections": { "add": [collection_id] } }),
    );

    let product_update = proxy.process_request(json_graphql_request(
        r#"
        mutation UpdateEntitledProduct($product: ProductUpdateInput!) {
          productUpdate(product: $product) { product { id title } userErrors { field message } }
        }
        "#,
        json!({ "product": { "id": product_id, "title": "Entitled Mutable Updated" } }),
    ));
    assert_eq!(
        product_update.body["data"]["productUpdate"]["userErrors"],
        json!([])
    );
    let variant_update = proxy.process_request(json_graphql_request(
        r#"
        mutation UpdateEntitledVariant($input: ProductVariantInput!) {
          productVariantUpdate(input: $input) { productVariant { id title sku } userErrors { field message } }
        }
        "#,
        json!({ "input": { "id": variant_id, "title": "Entitled Variant Updated", "sku": "ENTITLED-MUTABLE-UPDATED" } }),
    ));
    assert_eq!(
        variant_update.body["data"]["productVariantUpdate"]["userErrors"],
        json!([])
    );
    let collection_update = proxy.process_request(json_graphql_request(
        r#"
        mutation UpdateEntitledCollection($input: CollectionInput!) {
          collectionUpdate(input: $input) { collection { id title } userErrors { field message } }
        }
        "#,
        json!({ "input": { "id": collection_id, "title": "Entitled Collection Updated" } }),
    ));
    assert_eq!(
        collection_update.body["data"]["collectionUpdate"]["userErrors"],
        json!([])
    );

    let read_after_updates = proxy.process_request(json_graphql_request(
        r#"
        query EntitlementUpdates($productDiscountId: ID!, $collectionDiscountId: ID!) {
          productDiscount: discountNode(id: $productDiscountId) {
            discount { __typename ... on DiscountCodeBxgy { customerGets { items { __typename ... on DiscountProducts { products(first: 1) { nodes { id title } } productVariants(first: 1) { nodes { id title sku product { id title } } } } } } } }
          }
          collectionDiscount: discountNode(id: $collectionDiscountId) {
            discount { __typename ... on DiscountCodeBxgy { customerGets { items { __typename ... on DiscountCollections { collections(first: 1) { nodes { id title } } } } } } }
          }
          catalog: discountNodes(first: 10) {
            nodes { id discount { __typename ... on DiscountCodeBxgy { customerGets { items { __typename ... on DiscountProducts { products(first: 1) { nodes { id title } } productVariants(first: 1) { nodes { id sku } } } ... on DiscountCollections { collections(first: 1) { nodes { id title } } } } } } } }
          }
        }
        "#,
        json!({ "productDiscountId": product_discount_id, "collectionDiscountId": collection_discount_id }),
    ));
    assert_eq!(
        read_after_updates.body["data"]["productDiscount"]["discount"]["customerGets"]["items"]
            ["products"]["nodes"],
        json!([{ "id": product_id, "title": "Entitled Mutable Updated" }])
    );
    assert_eq!(
        read_after_updates.body["data"]["productDiscount"]["discount"]["customerGets"]["items"]
            ["productVariants"]["nodes"],
        json!([{
            "id": variant_id,
            "title": "Entitled Variant Updated",
            "sku": "ENTITLED-MUTABLE-UPDATED",
            "product": { "id": product_id, "title": "Entitled Mutable Updated" }
        }])
    );
    assert_eq!(
        read_after_updates.body["data"]["collectionDiscount"]["discount"]["customerGets"]["items"]
            ["collections"]["nodes"],
        json!([{ "id": collection_id, "title": "Entitled Collection Updated" }])
    );
    let catalog_nodes = read_after_updates.body["data"]["catalog"]["nodes"]
        .as_array()
        .unwrap();
    let product_catalog = catalog_nodes
        .iter()
        .find(|node| node.get("id") == Some(&json!(product_discount_id)))
        .expect("product entitlement discount should be in catalog after updates");
    assert_eq!(
        product_catalog["discount"]["customerGets"]["items"]["products"]["nodes"][0]["title"],
        json!("Entitled Mutable Updated")
    );
    assert_eq!(
        product_catalog["discount"]["customerGets"]["items"]["productVariants"]["nodes"][0]["sku"],
        json!("ENTITLED-MUTABLE-UPDATED")
    );
    let collection_catalog = catalog_nodes
        .iter()
        .find(|node| node.get("id") == Some(&json!(collection_discount_id)))
        .expect("collection entitlement discount should be in catalog after updates");
    assert_eq!(
        collection_catalog["discount"]["customerGets"]["items"]["collections"]["nodes"][0]["title"],
        json!("Entitled Collection Updated")
    );

    let variant_delete = proxy.process_request(json_graphql_request(
        r#"
        mutation DeleteEntitledVariant($id: ID!) {
          productVariantDelete(id: $id) { deletedProductVariantId userErrors { field message } }
        }
        "#,
        json!({ "id": variant_id }),
    ));
    assert_eq!(
        variant_delete.body["data"]["productVariantDelete"]["userErrors"],
        json!([])
    );
    let product_delete = proxy.process_request(json_graphql_request(
        r#"
        mutation DeleteEntitledProduct($input: ProductDeleteInput!) {
          productDelete(input: $input) { deletedProductId userErrors { field message } }
        }
        "#,
        json!({ "input": { "id": product_id } }),
    ));
    assert_eq!(
        product_delete.body["data"]["productDelete"]["deletedProductId"],
        json!(product_id)
    );
    let collection_delete = proxy.process_request(json_graphql_request(
        r#"
        mutation DeleteEntitledCollection($input: CollectionDeleteInput!) {
          collectionDelete(input: $input) { deletedCollectionId userErrors { field message } }
        }
        "#,
        json!({ "input": { "id": collection_id } }),
    ));
    assert_eq!(
        collection_delete.body["data"]["collectionDelete"]["deletedCollectionId"],
        json!(collection_id)
    );

    let read_after_deletes = proxy.process_request(json_graphql_request(
        r#"
        query EntitlementDeletes($productDiscountId: ID!, $collectionDiscountId: ID!) {
          productDiscount: discountNode(id: $productDiscountId) {
            discount { __typename ... on DiscountCodeBxgy { customerGets { items { __typename ... on DiscountProducts { products(first: 1) { nodes { id } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } productVariants(first: 1) { nodes { id } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } } } } } }
          }
          collectionDiscount: discountNode(id: $collectionDiscountId) {
            discount { __typename ... on DiscountCodeBxgy { customerGets { items { __typename ... on DiscountCollections { collections(first: 1) { nodes { id } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } } } } } }
          }
          catalog: discountNodes(first: 10) {
            nodes { id discount { __typename ... on DiscountCodeBxgy { customerGets { items { __typename ... on DiscountProducts { products(first: 1) { nodes { id } } productVariants(first: 1) { nodes { id } } } ... on DiscountCollections { collections(first: 1) { nodes { id } } } } } } } }
          }
        }
        "#,
        json!({ "productDiscountId": product_discount_id, "collectionDiscountId": collection_discount_id }),
    ));
    let deleted_product_items =
        &read_after_deletes.body["data"]["productDiscount"]["discount"]["customerGets"]["items"];
    assert_eq!(
        deleted_product_items["products"],
        json!({
            "nodes": [],
            "pageInfo": {
                "hasNextPage": false,
                "hasPreviousPage": false,
                "startCursor": Value::Null,
                "endCursor": Value::Null
            }
        })
    );
    assert_eq!(
        deleted_product_items["productVariants"],
        json!({
            "nodes": [],
            "pageInfo": {
                "hasNextPage": false,
                "hasPreviousPage": false,
                "startCursor": Value::Null,
                "endCursor": Value::Null
            }
        })
    );
    assert_eq!(
        read_after_deletes.body["data"]["collectionDiscount"]["discount"]["customerGets"]["items"]
            ["collections"],
        json!({
            "nodes": [],
            "pageInfo": {
                "hasNextPage": false,
                "hasPreviousPage": false,
                "startCursor": Value::Null,
                "endCursor": Value::Null
            }
        })
    );
    let catalog_nodes = read_after_deletes.body["data"]["catalog"]["nodes"]
        .as_array()
        .unwrap();
    let product_catalog = catalog_nodes
        .iter()
        .find(|node| node.get("id") == Some(&json!(product_discount_id)))
        .expect("product entitlement discount should stay in catalog after deletes");
    assert_eq!(
        product_catalog["discount"]["customerGets"]["items"]["products"]["nodes"],
        json!([])
    );
    assert_eq!(
        product_catalog["discount"]["customerGets"]["items"]["productVariants"]["nodes"],
        json!([])
    );
    let collection_catalog = catalog_nodes
        .iter()
        .find(|node| node.get("id") == Some(&json!(collection_discount_id)))
        .expect("collection entitlement discount should stay in catalog after deletes");
    assert_eq!(
        collection_catalog["discount"]["customerGets"]["items"]["collections"]["nodes"],
        json!([])
    );
}

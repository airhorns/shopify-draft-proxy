#![allow(dead_code)]

#[path = "graphql_routes/common.rs"]
mod common;

use common::*;

fn observed_product(id: usize) -> Value {
    json!({
        "__typename": "Product",
        "id": format!("gid://shopify/Product/{id}"),
        "title": format!("Product {id}"),
        "handle": format!("product-{id}"),
        "status": "ACTIVE",
        "collections": {
            "nodes": [],
            "pageInfo": {
                "hasNextPage": false,
                "hasPreviousPage": false
            }
        }
    })
}

fn observed_member_product(id: usize, collection_id: &str) -> Value {
    let mut product = observed_product(id);
    product["collections"]["nodes"] = json!([{
        "id": collection_id,
        "title": "Large collection",
        "handle": "large-collection"
    }]);
    product
}

fn observed_collection(
    collection_id: &str,
    total: usize,
    product_ids: impl IntoIterator<Item = usize>,
) -> Value {
    let product_ids = product_ids.into_iter().collect::<Vec<_>>();
    let has_next_page = product_ids.last().is_some_and(|last| *last < total);
    json!({
        "__typename": "Collection",
        "id": collection_id,
        "title": "Large collection",
        "handle": "large-collection",
        "sortOrder": "MANUAL",
        "productsCount": { "count": total, "precision": "EXACT" },
        "products": {
            "edges": product_ids.iter().map(|id| json!({
                "cursor": format!("opaque-{id}"),
                "node": observed_product(*id)
            })).collect::<Vec<_>>(),
            "pageInfo": {
                "hasNextPage": has_next_page,
                "hasPreviousPage": product_ids.first().is_some_and(|first| *first > 1),
                "startCursor": product_ids.first().map(|id| format!("opaque-{id}")),
                "endCursor": product_ids.last().map(|id| format!("opaque-{id}"))
            }
        }
    })
}

#[test]
fn collection_add_preserves_partial_upstream_membership_count() {
    let collection_id = "gid://shopify/Collection/large";
    let new_product_id = "gid://shopify/Product/16";
    let first_page = (1..=12).map(observed_product).collect::<Vec<_>>();
    let upstream_calls = Arc::new(Mutex::new(Vec::<Value>::new()));
    let captured_calls = Arc::clone(&upstream_calls);
    let mut proxy = configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport({
        let first_page = first_page.clone();
        move |request| {
            let body = serde_json::from_str::<Value>(&request.body)
                .expect("upstream GraphQL request parses");
            captured_calls.lock().unwrap().push(body.clone());
            if body["operationName"] != json!("CollectionMembershipTargetsHydrate") {
                assert!(body["query"]
                    .as_str()
                    .unwrap_or_default()
                    .contains("AddedMemberAfterRestore"));
                return Response {
                    status: 200,
                    headers: Default::default(),
                    body: json!({
                        "data": {
                            "collection": {
                                "id": collection_id,
                                "productsCount": { "count": 15, "precision": "EXACT" },
                                "hasAdded": false
                            },
                            "added": {
                                "id": new_product_id,
                                "collections": { "nodes": [] }
                            },
                            "untouched": observed_member_product(13, collection_id)
                        }
                    }),
                };
            }
            assert_eq!(
                body["operationName"],
                json!("CollectionMembershipTargetsHydrate"),
                "unexpected upstream request: {body:#}"
            );
            assert_eq!(
                body["variables"]["collectionId"],
                json!(collection_id)
            );
            assert_eq!(body["variables"]["productIds"], json!([new_product_id]));
            assert_eq!(body["variables"]["first"], json!(12));
            Response {
                status: 200,
                headers: Default::default(),
                body: json!({
                    "data": {
                        "collection": {
                                "__typename": "Collection",
                                "id": collection_id,
                                "title": "Large collection",
                                "handle": "large-collection",
                                "sortOrder": "MANUAL",
                                "productsCount": { "count": 15, "precision": "EXACT" },
                                "products": {
                                    "edges": first_page.iter().cloned().enumerate().map(|(index, node)| json!({
                                        "cursor": format!("opaque-{}", index + 1),
                                        "node": node
                                    })).collect::<Vec<_>>(),
                                    "pageInfo": {
                                        "hasNextPage": true,
                                        "hasPreviousPage": false,
                                        "startCursor": "opaque-1",
                                        "endCursor": "opaque-12"
                                    }
                                }
                            },
                            "nodes": [{
                                "id": new_product_id,
                                "title": "Aardvark",
                                "handle": "aardvark",
                                "status": "ACTIVE",
                                "collections": {
                                    "nodes": [],
                                    "pageInfo": {
                                        "hasNextPage": false,
                                        "hasPreviousPage": false
                                    }
                                }
                            }]
                    }
                }),
            }
        }
    });

    let response = proxy.process_request(json_graphql_request(
        r#"
        mutation CollectionAddPreservesPartialBaseline($id: ID!, $productIds: [ID!]!) {
          collectionAddProducts(id: $id, productIds: $productIds) {
            collection {
              id
              products(first: 10, sortKey: MANUAL) {
                nodes { id }
                pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
              }
              productsCount { count precision }
            }
            userErrors { field message }
          }
        }
        "#,
        json!({ "id": collection_id, "productIds": [new_product_id] }),
    ));

    assert_eq!(response.status, 200);
    assert_eq!(
        response.body["data"]["collectionAddProducts"]["userErrors"],
        json!([])
    );
    assert_eq!(
        response.body["data"]["collectionAddProducts"]["collection"]["productsCount"],
        json!({ "count": 15, "precision": "EXACT" })
    );
    assert_eq!(
        response.body["data"]["collectionAddProducts"]["collection"]["products"]["nodes"],
        json!((1..=10)
            .map(|id| json!({ "id": format!("gid://shopify/Product/{id}") }))
            .collect::<Vec<_>>())
    );
    assert_eq!(
        response.body["data"]["collectionAddProducts"]["collection"]["products"]["pageInfo"]
            ["hasNextPage"],
        json!(true)
    );

    let dump = proxy.process_request(request_with_body("POST", "/__meta/dump", ""));
    assert_eq!(dump.status, 200);
    let mut snapshot_restored = snapshot_proxy();
    let snapshot_restore = snapshot_restored.process_request(request_with_body(
        "POST",
        "/__meta/restore",
        &dump.body.to_string(),
    ));
    assert_eq!(snapshot_restore.status, 200);
    let snapshot_sorted = snapshot_restored.process_request(json_graphql_request(
        "query SnapshotSorted($id: ID!) { collection(id: $id) { products(first: 1, sortKey: TITLE) { nodes { id title } pageInfo { hasNextPage } } } }",
        json!({ "id": collection_id }),
    ));
    assert_eq!(
        snapshot_sorted.body["data"]["collection"]["products"]["nodes"],
        json!([{ "id": new_product_id, "title": "Aardvark" }])
    );
    assert_eq!(
        snapshot_sorted.body["data"]["collection"]["products"]["pageInfo"]["hasNextPage"],
        json!(true)
    );
    let restore = proxy.process_request(request_with_body(
        "POST",
        "/__meta/restore",
        &dump.body.to_string(),
    ));
    assert_eq!(restore.status, 200);

    let read = proxy.process_request(json_graphql_request(
        r#"
        query AddedMemberAfterRestore($id: ID!, $productId: ID!) {
          collection(id: $id) {
            productsCount { count precision }
            hasAdded: hasProduct(id: $productId)
          }
          added: product(id: $productId) {
            collections(first: 10) { nodes { id } }
          }
          untouched: product(id: "gid://shopify/Product/13") { id }
        }
        "#,
        json!({ "id": collection_id, "productId": new_product_id }),
    ));
    assert_eq!(
        read.body["data"]["collection"]["productsCount"],
        json!({ "count": 16, "precision": "EXACT" })
    );
    assert_eq!(read.body["data"]["collection"]["hasAdded"], json!(true));
    assert_eq!(
        read.body["data"]["added"]["collections"]["nodes"],
        json!([{ "id": collection_id }])
    );

    let calls = upstream_calls.lock().unwrap();
    assert_eq!(calls.len(), 2);
    assert!(!calls[0]["query"]
        .as_str()
        .unwrap_or_default()
        .contains("collectionAddProducts("));
    assert_eq!(log_snapshot(&proxy)["entries"].as_array().unwrap().len(), 1);
}

#[test]
fn collection_remove_probes_a_later_member_and_refills_the_requested_window() {
    let collection_id = "gid://shopify/Collection/large";
    let removed_product_id = "gid://shopify/Product/13";
    let upstream_calls = Arc::new(Mutex::new(Vec::<Value>::new()));
    let captured_calls = Arc::clone(&upstream_calls);
    let replayed = Arc::new(Mutex::new(Vec::<Request>::new()));
    let captured_replay = Arc::clone(&replayed);
    let mut proxy = configured_proxy(ReadMode::LiveHybrid, None)
        .with_upstream_transport(move |request| {
            let body = serde_json::from_str::<Value>(&request.body)
                .expect("upstream GraphQL request parses");
            captured_calls.lock().unwrap().push(body.clone());
            match body["operationName"].as_str() {
                Some("CollectionMembershipTargetsHydrate") => Response {
                    status: 200,
                    headers: Default::default(),
                    body: json!({
                        "data": {
                            "collection": observed_collection(collection_id, 15, 1..=12),
                            "nodes": [observed_member_product(13, collection_id)]
                        }
                    }),
                },
                Some("CollectionMembershipWindowHydrate") => {
                    assert_eq!(body["variables"]["after"], json!("opaque-13"));
                    assert_eq!(body["variables"]["first"], json!(3));
                    Response {
                        status: 200,
                        headers: Default::default(),
                        body: json!({
                            "data": {
                                "collection": {
                                    "products": observed_collection(collection_id, 15, 14..=15)["products"].clone()
                                }
                            }
                        }),
                    }
                }
                _ => {
                    assert!(body["query"]
                        .as_str()
                        .unwrap_or_default()
                        .contains("CollectionPageBoundary"));
                    Response {
                        status: 200,
                        headers: Default::default(),
                        body: json!({
                            "data": {
                                "collection": {
                                    "products": observed_collection(collection_id, 15, 11..=13)["products"].clone(),
                                    "productsCount": { "count": 15, "precision": "EXACT" },
                                    "hasThirteen": true
                                }
                            }
                        }),
                    }
                }
            }
        })
        .with_commit_transport(move |request| {
            captured_replay.lock().unwrap().push(request);
            Response {
                status: 200,
                headers: Default::default(),
                body: json!({ "data": { "collectionRemoveProducts": { "job": null, "userErrors": [] } } }),
            }
        });

    let mutation = json_graphql_request(
        r#"
        mutation RemoveLaterMember($id: ID!, $productIds: [ID!]!) {
          collectionRemoveProducts(id: $id, productIds: $productIds) {
            job { id done }
            userErrors { field message }
          }
        }
        "#,
        json!({ "id": collection_id, "productIds": [removed_product_id] }),
    );
    let remove = proxy.process_request(mutation.clone());
    assert_eq!(
        remove.body["data"]["collectionRemoveProducts"]["userErrors"],
        json!([])
    );

    let read = proxy.process_request(json_graphql_request(
        r#"
        query CollectionPageBoundary($id: ID!, $after: String!, $removed: ID!) {
          collection(id: $id) {
            products(first: 3, after: $after, sortKey: MANUAL) {
              edges { cursor node { id } }
              pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
            }
            productsCount { count precision }
            hasThirteen: hasProduct(id: $removed)
          }
          removed: product(id: $removed) {
            collections(first: 10) { nodes { id } }
          }
        }
        "#,
        json!({ "id": collection_id, "after": "opaque-10", "removed": removed_product_id }),
    ));
    assert_eq!(
        read.body["data"]["collection"]["products"]["edges"],
        json!([
            { "cursor": "opaque-11", "node": { "id": "gid://shopify/Product/11" } },
            { "cursor": "opaque-12", "node": { "id": "gid://shopify/Product/12" } },
            { "cursor": "opaque-14", "node": { "id": "gid://shopify/Product/14" } }
        ])
    );
    assert_eq!(
        read.body["data"]["collection"]["products"]["pageInfo"],
        json!({
            "hasNextPage": true,
            "hasPreviousPage": true,
            "startCursor": "opaque-11",
            "endCursor": "opaque-14"
        })
    );
    assert_eq!(
        read.body["data"]["collection"]["productsCount"],
        json!({ "count": 14, "precision": "EXACT" })
    );
    assert_eq!(read.body["data"]["collection"]["hasThirteen"], json!(false));
    assert_eq!(
        read.body["data"]["removed"]["collections"]["nodes"],
        json!([])
    );
    assert_eq!(upstream_calls.lock().unwrap().len(), 3);

    let commit = proxy.process_request(request_with_body("POST", "/__meta/commit", ""));
    assert_eq!(commit.body["committed"], json!(1));
    let replayed = replayed.lock().unwrap();
    assert_eq!(replayed.len(), 1);
    assert_eq!(replayed[0].body, mutation.body);
}

#[test]
fn unresolved_membership_hydration_is_atomic_and_does_not_log_success() {
    let collection_id = "gid://shopify/Collection/large";
    let mut proxy =
        configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport(move |_| Response {
            status: 200,
            headers: Default::default(),
            body: json!({
                "data": {
                    "collection": observed_collection(collection_id, 15, 1..=12),
                    "nodes": []
                }
            }),
        });

    let response = proxy.process_request(json_graphql_request(
        r#"
        mutation UnresolvedAdd($id: ID!, $productIds: [ID!]!) {
          collectionAddProducts(id: $id, productIds: $productIds) {
            collection { id }
            userErrors { field message }
          }
        }
        "#,
        json!({ "id": collection_id, "productIds": ["gid://shopify/Product/16"] }),
    ));
    assert_eq!(
        response.body["data"]["collectionAddProducts"],
        json!({
            "collection": null,
            "userErrors": [{
                "field": ["productIds"],
                "message": "Collection membership could not be resolved"
            }]
        })
    );
    let state = state_snapshot(&proxy);
    assert_eq!(state["stagedState"]["collections"], json!({}));
    assert_eq!(state["stagedState"]["collectionMemberships"], json!({}));
    assert_eq!(log_snapshot(&proxy), json!({ "entries": [] }));
}

#[test]
fn collection_add_accepts_complete_legacy_membership_evidence_after_probe_failure() {
    let collection_id = "gid://shopify/Collection/legacy";
    let first_product_id = "gid://shopify/Product/16";
    let second_product_id = "gid://shopify/Product/17";
    let upstream_calls = Arc::new(Mutex::new(Vec::<Value>::new()));
    let captured_calls = Arc::clone(&upstream_calls);
    let mut proxy =
        configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport(move |request| {
            let body = serde_json::from_str::<Value>(&request.body)
                .expect("upstream GraphQL request parses");
            captured_calls.lock().unwrap().push(body.clone());
            if body["operationName"] == json!("CollectionMembershipTargetsHydrate") {
                return Response {
                    status: 502,
                    headers: Default::default(),
                    body: json!({ "errors": [{ "message": "probe unavailable" }] }),
                };
            }
            assert_eq!(body["operationName"], json!("ProductsHydrateNodes"));
            assert_eq!(
                body["variables"]["ids"],
                json!([collection_id, first_product_id, second_product_id])
            );
            Response {
                status: 200,
                headers: Default::default(),
                body: json!({
                    "data": {
                        "nodes": [
                            observed_collection(collection_id, 0, std::iter::empty()),
                            observed_product(17),
                            observed_product(16)
                        ]
                    }
                }),
            }
        });

    let response = proxy.process_request(json_graphql_request(
        r#"
        mutation LegacyEvidenceAdd($id: ID!, $productIds: [ID!]!) {
          collectionAddProducts(id: $id, productIds: $productIds) {
            collection {
              id
              products(first: 10) { nodes { id } }
              productsCount { count precision }
            }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "id": collection_id,
            "productIds": [first_product_id, second_product_id]
        }),
    ));

    assert_eq!(
        response.body["data"]["collectionAddProducts"]["userErrors"],
        json!([])
    );
    assert_eq!(
        response.body["data"]["collectionAddProducts"]["collection"]["products"]["nodes"],
        json!([{ "id": first_product_id }, { "id": second_product_id }])
    );
    assert_eq!(
        response.body["data"]["collectionAddProducts"]["collection"]["productsCount"],
        json!({ "count": 0, "precision": "EXACT" })
    );
    let calls = upstream_calls.lock().unwrap();
    assert_eq!(calls.len(), 2);
    assert!(calls.iter().all(|call| !call["query"]
        .as_str()
        .unwrap_or_default()
        .contains("collectionAddProducts(")));
    assert_eq!(log_snapshot(&proxy)["entries"].as_array().unwrap().len(), 1);
}

#[test]
fn collection_reorder_moves_a_later_member_without_losing_untouched_members() {
    let collection_id = "gid://shopify/Collection/large";
    let upstream_calls = Arc::new(Mutex::new(Vec::<Value>::new()));
    let captured_calls = Arc::clone(&upstream_calls);
    let mut proxy = configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport(
        move |request| {
            let body = serde_json::from_str::<Value>(&request.body)
                .expect("upstream GraphQL request parses");
            captured_calls.lock().unwrap().push(body.clone());
            match body["operationName"].as_str() {
                Some("CollectionMembershipTargetsHydrate") => {
                    assert_eq!(body["variables"]["first"], json!(12));
                    assert_eq!(
                        body["variables"]["productIds"],
                        json!(["gid://shopify/Product/12"])
                    );
                    Response {
                        status: 200,
                        headers: Default::default(),
                        body: json!({
                            "data": {
                                "collection": observed_collection(collection_id, 20, 1..=12),
                                "nodes": [observed_member_product(12, collection_id)]
                            }
                        }),
                    }
                }
                Some("CollectionMembershipWindowHydrate") => {
                    assert_eq!(body["variables"]["after"], json!("opaque-12"));
                    assert_eq!(body["variables"]["first"], json!(3));
                    Response {
                        status: 200,
                        headers: Default::default(),
                        body: json!({
                            "data": {
                                "collection": {
                                    "products": observed_collection(collection_id, 20, 13..=15)["products"].clone()
                                }
                            }
                        }),
                    }
                }
                _ if body["query"]
                    .as_str()
                    .unwrap_or_default()
                    .contains("ReorderedFirstPage") => Response {
                    status: 200,
                    headers: Default::default(),
                    body: json!({
                        "data": {
                            "collection": {
                                "products": observed_collection(collection_id, 20, 1..=10)["products"].clone(),
                                "productsCount": { "count": 20, "precision": "EXACT" },
                                "moved": true,
                                "untouched": true
                            },
                            "moved": observed_member_product(12, collection_id),
                            "untouched": observed_member_product(13, collection_id)
                        }
                    }),
                },
                _ if body["query"]
                    .as_str()
                    .unwrap_or_default()
                    .contains("ReorderedSecondPage") => Response {
                    status: 200,
                    headers: Default::default(),
                    body: json!({
                        "data": {
                            "collection": {
                                "products": observed_collection(collection_id, 20, 10..=12)["products"].clone()
                            }
                        }
                    }),
                },
                _ => panic!("unexpected upstream request: {body:#}"),
            }
        },
    );

    let reorder = proxy.process_request(json_graphql_request(
        r#"
        mutation ReorderLaterMember($id: ID!, $moves: [MoveInput!]!) {
          collectionReorderProducts(id: $id, moves: $moves) {
            job { id done }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "id": collection_id,
            "moves": [{ "id": "gid://shopify/Product/12", "newPosition": "1" }]
        }),
    ));
    assert_eq!(
        reorder.body["data"]["collectionReorderProducts"]["userErrors"],
        json!([])
    );

    let first = proxy.process_request(json_graphql_request(
        r#"
        query ReorderedFirstPage($id: ID!) {
          collection(id: $id) {
            products(first: 10, sortKey: MANUAL) {
              edges { cursor node { id } }
              pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
            }
            productsCount { count precision }
            moved: hasProduct(id: "gid://shopify/Product/12")
            untouched: hasProduct(id: "gid://shopify/Product/13")
          }
          moved: product(id: "gid://shopify/Product/12") {
            collections(first: 10) { nodes { id } }
          }
          untouched: product(id: "gid://shopify/Product/13") {
            collections(first: 10) { nodes { id } }
          }
        }
        "#,
        json!({ "id": collection_id }),
    ));
    let first_edges = first.body["data"]["collection"]["products"]["edges"]
        .as_array()
        .unwrap();
    assert_eq!(
        first_edges[0]["node"]["id"],
        json!("gid://shopify/Product/1")
    );
    assert_eq!(
        first_edges[1]["node"]["id"],
        json!("gid://shopify/Product/12")
    );
    assert_eq!(
        first_edges[9]["node"]["id"],
        json!("gid://shopify/Product/9")
    );
    assert_eq!(first_edges[9]["cursor"], json!("opaque-9"));
    assert_eq!(
        first.body["data"]["collection"]["productsCount"],
        json!({ "count": 20, "precision": "EXACT" })
    );
    assert_eq!(first.body["data"]["collection"]["moved"], json!(true));
    assert_eq!(first.body["data"]["collection"]["untouched"], json!(true));
    assert_eq!(
        first.body["data"]["moved"]["collections"]["nodes"][0]["id"],
        json!(collection_id)
    );
    assert_eq!(
        first.body["data"]["untouched"]["collections"]["nodes"][0]["id"],
        json!(collection_id)
    );

    let second = proxy.process_request(json_graphql_request(
        r#"
        query ReorderedSecondPage($id: ID!, $after: String!) {
          collection(id: $id) {
            products(first: 3, after: $after, sortKey: MANUAL) {
              edges { cursor node { id } }
              pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
            }
          }
        }
        "#,
        json!({ "id": collection_id, "after": "opaque-9" }),
    ));
    assert_eq!(
        second.body["data"]["collection"]["products"]["edges"],
        json!([
            { "cursor": "opaque-10", "node": { "id": "gid://shopify/Product/10" } },
            { "cursor": "opaque-11", "node": { "id": "gid://shopify/Product/11" } },
            { "cursor": "opaque-13", "node": { "id": "gid://shopify/Product/13" } }
        ])
    );
    assert_eq!(
        second.body["data"]["collection"]["products"]["pageInfo"],
        json!({
            "hasNextPage": true,
            "hasPreviousPage": true,
            "startCursor": "opaque-10",
            "endCursor": "opaque-13"
        })
    );
    assert_eq!(upstream_calls.lock().unwrap().len(), 4);
    assert_eq!(log_snapshot(&proxy)["entries"].as_array().unwrap().len(), 1);
}

#[test]
fn collection_add_overlays_the_new_member_on_a_sorted_upstream_window() {
    let collection_id = "gid://shopify/Collection/large";
    let upstream_calls = Arc::new(Mutex::new(Vec::<Value>::new()));
    let captured_calls = Arc::clone(&upstream_calls);
    let mut proxy = configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport(
        move |request| {
            let body = serde_json::from_str::<Value>(&request.body)
                .expect("upstream GraphQL request parses");
            captured_calls.lock().unwrap().push(body.clone());
            if body["operationName"] == json!("CollectionMembershipTargetsHydrate") {
                let mut target = observed_product(16);
                target["title"] = json!("Aardvark");
                return Response {
                    status: 200,
                    headers: Default::default(),
                    body: json!({
                        "data": {
                            "collection": observed_collection(collection_id, 15, 1..=12),
                            "nodes": [target]
                        }
                    }),
                };
            }
            assert!(body["query"]
                .as_str()
                .unwrap_or_default()
                .contains("SortedMembershipWindow"));
            Response {
                status: 200,
                headers: Default::default(),
                body: json!({
                    "data": {
                        "collection": {
                            "products": observed_collection(collection_id, 15, 1..=3)["products"].clone()
                        }
                    }
                }),
            }
        },
    );

    let add = proxy.process_request(json_graphql_request(
        "mutation AddSorted($id: ID!, $ids: [ID!]!) { collectionAddProducts(id: $id, productIds: $ids) { collection { id } userErrors { field message } } }",
        json!({ "id": collection_id, "ids": ["gid://shopify/Product/16"] }),
    ));
    assert_eq!(
        add.body["data"]["collectionAddProducts"]["userErrors"],
        json!([])
    );

    let read = proxy.process_request(json_graphql_request(
        r#"
        query SortedMembershipWindow($id: ID!) {
          collection(id: $id) {
            products(first: 3, sortKey: TITLE) {
              edges { cursor node { id title } }
              pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
            }
          }
        }
        "#,
        json!({ "id": collection_id }),
    ));
    let edges = read.body["data"]["collection"]["products"]["edges"]
        .as_array()
        .unwrap();
    assert_eq!(
        edges[0]["node"],
        json!({
            "id": "gid://shopify/Product/16",
            "title": "Aardvark"
        })
    );
    assert_ne!(edges[0]["cursor"], json!("gid://shopify/Product/16"));
    assert_eq!(edges[1]["node"]["id"], json!("gid://shopify/Product/1"));
    assert_eq!(edges[2]["node"]["id"], json!("gid://shopify/Product/2"));
    assert_eq!(
        read.body["data"]["collection"]["products"]["pageInfo"]["hasNextPage"],
        json!(true)
    );
    assert_eq!(upstream_calls.lock().unwrap().len(), 2);
}

#[test]
fn snapshot_membership_mutations_use_complete_local_evidence_and_reset_cleanly() {
    let upstream_calls = Arc::new(Mutex::new(0usize));
    let captured_calls = Arc::clone(&upstream_calls);
    let products = (1..=13)
        .map(|id| ProductRecord {
            id: format!("gid://shopify/Product/{id}"),
            title: format!("Product {id}"),
            handle: format!("product-{id}"),
            status: "ACTIVE".to_string(),
            ..ProductRecord::default()
        })
        .collect::<Vec<_>>();
    let mut proxy = snapshot_proxy()
        .with_base_products(products)
        .with_upstream_transport(move |request| {
            *captured_calls.lock().unwrap() += 1;
            panic!("snapshot called upstream: {}", request.body)
        });

    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateLargeLocal($input: CollectionInput!) {
          collectionCreate(input: $input) {
            collection { id }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "input": {
                "title": "Large local",
                "sortOrder": "MANUAL",
                "products": (1..=12).map(|id| format!("gid://shopify/Product/{id}")).collect::<Vec<_>>()
            }
        }),
    ));
    let collection_id = create.body["data"]["collectionCreate"]["collection"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let add = proxy.process_request(json_graphql_request(
        "mutation Add($id: ID!, $ids: [ID!]!) { collectionAddProducts(id: $id, productIds: $ids) { userErrors { field message } } }",
        json!({ "id": collection_id, "ids": ["gid://shopify/Product/13"] }),
    ));
    assert_eq!(
        add.body["data"]["collectionAddProducts"]["userErrors"],
        json!([])
    );
    let remove = proxy.process_request(json_graphql_request(
        "mutation Remove($id: ID!, $ids: [ID!]!) { collectionRemoveProducts(id: $id, productIds: $ids) { userErrors { field message } } }",
        json!({ "id": collection_id, "ids": ["gid://shopify/Product/12"] }),
    ));
    assert_eq!(
        remove.body["data"]["collectionRemoveProducts"]["userErrors"],
        json!([])
    );
    let reorder = proxy.process_request(json_graphql_request(
        "mutation Reorder($id: ID!, $moves: [MoveInput!]!) { collectionReorderProducts(id: $id, moves: $moves) { userErrors { field message } } }",
        json!({ "id": collection_id, "moves": [{ "id": "gid://shopify/Product/13", "newPosition": "0" }] }),
    ));
    assert_eq!(
        reorder.body["data"]["collectionReorderProducts"]["userErrors"],
        json!([])
    );

    let read = proxy.process_request(json_graphql_request(
        r#"
        query LocalMembership($id: ID!) {
          collection(id: $id) {
            products(first: 20, sortKey: MANUAL) { nodes { id } }
            productsCount { count precision }
            hasAdded: hasProduct(id: "gid://shopify/Product/13")
            hasRemoved: hasProduct(id: "gid://shopify/Product/12")
          }
          added: product(id: "gid://shopify/Product/13") { collections(first: 10) { nodes { id } } }
          removed: product(id: "gid://shopify/Product/12") { collections(first: 10) { nodes { id } } }
        }
        "#,
        json!({ "id": collection_id }),
    ));
    assert_eq!(
        read.body["data"]["collection"]["products"]["nodes"],
        json!((std::iter::once(13).chain(1..=11))
            .map(|id| json!({ "id": format!("gid://shopify/Product/{id}") }))
            .collect::<Vec<_>>())
    );
    assert_eq!(
        read.body["data"]["collection"]["productsCount"],
        json!({ "count": 12, "precision": "EXACT" })
    );
    assert_eq!(read.body["data"]["collection"]["hasAdded"], json!(true));
    assert_eq!(read.body["data"]["collection"]["hasRemoved"], json!(false));
    assert_eq!(
        read.body["data"]["added"]["collections"]["nodes"][0]["id"],
        json!(collection_id)
    );
    assert_eq!(
        read.body["data"]["removed"]["collections"]["nodes"],
        json!([])
    );
    assert_eq!(*upstream_calls.lock().unwrap(), 0);
    assert_eq!(log_snapshot(&proxy)["entries"].as_array().unwrap().len(), 4);

    let dump = proxy.process_request(request_with_body("POST", "/__meta/dump", ""));
    assert_eq!(dump.status, 200);
    let mut restored = snapshot_proxy();
    let restore = restored.process_request(request_with_body(
        "POST",
        "/__meta/restore",
        &dump.body.to_string(),
    ));
    assert_eq!(restore.status, 200);
    let restored_read = restored.process_request(json_graphql_request(
        "query RestoredMembership($id: ID!) { collection(id: $id) { products(first: 3, sortKey: MANUAL) { nodes { id } } productsCount { count precision } } }",
        json!({ "id": collection_id }),
    ));
    assert_eq!(
        restored_read.body["data"]["collection"]["products"]["nodes"],
        json!([
            { "id": "gid://shopify/Product/13" },
            { "id": "gid://shopify/Product/1" },
            { "id": "gid://shopify/Product/2" }
        ])
    );
    assert_eq!(
        restored_read.body["data"]["collection"]["productsCount"],
        json!({ "count": 12, "precision": "EXACT" })
    );

    let reset = proxy.process_request(request_with_body("POST", "/__meta/reset", ""));
    assert_eq!(reset.status, 200);
    assert_eq!(
        state_snapshot(&proxy)["stagedState"]["collectionMemberships"],
        json!({})
    );
    assert_eq!(log_snapshot(&proxy), json!({ "entries": [] }));
    assert_eq!(*upstream_calls.lock().unwrap(), 0);
}

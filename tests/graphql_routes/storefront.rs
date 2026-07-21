use super::common::*;
use shopify_draft_proxy::proxy::UnsupportedMutationMode;

fn storefront_graphql_request(query: &str, variables: Value) -> Request {
    request_with_body(
        "POST",
        "/api/2026-04/graphql.json",
        &json!({
            "query": query,
            "variables": variables
        })
        .to_string(),
    )
}

fn storefront_product_fixture(
    id: &str,
    title: &str,
    handle: &str,
    publication_id: Option<&str>,
) -> ProductRecord {
    let mut product = ProductRecord {
        id: id.to_string(),
        created_at: "2024-01-01T00:00:00.000Z".to_string(),
        updated_at: "2024-01-02T00:00:00.000Z".to_string(),
        title: title.to_string(),
        handle: handle.to_string(),
        status: "ACTIVE".to_string(),
        description_html: format!("<p>{title} description</p>"),
        vendor: "Hermes".to_string(),
        product_type: "Accessory".to_string(),
        tags: vec!["storefront".to_string(), "catalog".to_string()],
        seo_title: format!("{title} SEO"),
        seo_description: format!("{title} SEO description"),
        total_inventory: 7,
        tracks_inventory: true,
        ..ProductRecord::default()
    };
    if let Some(publication_id) = publication_id {
        product.extra_fields.insert(
            "productPublications".to_string(),
            json!([{ "publicationId": publication_id, "publishedAt": "2024-01-03T00:00:00.000Z" }]),
        );
        product
            .extra_fields
            .insert("publishedAt".to_string(), json!("2024-01-03T00:00:00.000Z"));
    }
    product
}

fn restore_storefront_current_publication(proxy: &mut DraftProxy, publication_id: &str) {
    restore_state_with(proxy, |state| {
        state["baseState"]["publicationIds"] = json!([publication_id]);
        state["baseState"]["publicationCount"] = json!(1);
        state["stagedState"]["currentChannelPublicationId"] = json!(publication_id);
        state["stagedState"]["currentChannelPublicationResolved"] = json!(true);
    });
}

fn publish_to_current_storefront_channel(proxy: &mut DraftProxy, product_id: &str) {
    let publish = proxy.process_request(json_graphql_request(
        r#"
        mutation PublishToCurrentStorefrontChannel($id: ID!) {
          publishablePublishToCurrentChannel(id: $id) {
            publishable { ... on Product { id } ... on Collection { id } }
            userErrors { field message }
          }
        }
        "#,
        json!({ "id": product_id }),
    ));
    assert_eq!(publish.status, 200);
    assert_eq!(
        publish.body["data"]["publishablePublishToCurrentChannel"]["userErrors"],
        json!([])
    );
}

fn unpublish_from_current_storefront_channel(proxy: &mut DraftProxy, product_id: &str) {
    let unpublish = proxy.process_request(json_graphql_request(
        r#"
        mutation UnpublishFromCurrentStorefrontChannel($id: ID!) {
          publishableUnpublishToCurrentChannel(id: $id) {
            publishable { ... on Product { id } ... on Collection { id } }
            userErrors { field message }
          }
        }
        "#,
        json!({ "id": product_id }),
    ));
    assert_eq!(unpublish.status, 200);
    assert_eq!(
        unpublish.body["data"]["publishableUnpublishToCurrentChannel"]["userErrors"],
        json!([])
    );
}

fn add_storefront_inventory_location(proxy: &mut DraftProxy, name: &str) -> String {
    let response = proxy.process_request(json_graphql_request(
        r#"
        mutation AddStorefrontInventoryLocation($input: LocationAddInput!) {
          locationAdd(input: $input) {
            location { id }
            userErrors { field message }
          }
        }
        "#,
        json!({ "input": { "name": name, "address": { "countryCode": "US" } } }),
    ));
    assert_eq!(response.status, 200);
    assert_eq!(
        response.body["data"]["locationAdd"]["userErrors"],
        json!([])
    );
    response.body["data"]["locationAdd"]["location"]["id"]
        .as_str()
        .expect("location add should return id")
        .to_string()
}

fn stage_storefront_cart_variant(
    proxy: &mut DraftProxy,
    inventory_quantity: i64,
) -> (String, String, String) {
    let publication_id = "gid://shopify/Publication/storefront-cart-tests";
    restore_storefront_current_publication(proxy, publication_id);
    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateCartMerchandise($product: ProductCreateInput!) {
          productCreate(product: $product) {
            product {
              id
              variants(first: 1) {
                nodes { id inventoryItem { id } }
              }
            }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "product": {
                "title": "Storefront Cart Test Product",
                "status": "ACTIVE",
                "productOptions": [{ "name": "Color", "values": [{ "name": "Blue" }] }]
            }
        }),
    ));
    assert_eq!(
        create.body["data"]["productCreate"]["userErrors"],
        json!([]),
        "{}",
        create.body
    );
    let product_id = create.body["data"]["productCreate"]["product"]["id"]
        .as_str()
        .expect("cart test product id")
        .to_string();
    let variant_id = create.body["data"]["productCreate"]["product"]["variants"]["nodes"][0]["id"]
        .as_str()
        .expect("cart test variant id")
        .to_string();
    let inventory_item_id = create.body["data"]["productCreate"]["product"]["variants"]["nodes"][0]
        ["inventoryItem"]["id"]
        .as_str()
        .expect("cart test inventory item id")
        .to_string();

    let update = proxy.process_request(json_graphql_request(
        r#"
        mutation UpdateCartMerchandise($productId: ID!, $variants: [ProductVariantsBulkInput!]!) {
          productVariantsBulkUpdate(productId: $productId, variants: $variants) {
            productVariants { id price compareAtPrice inventoryItem { id tracked requiresShipping } }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "productId": product_id,
            "variants": [{
                "id": variant_id,
                "price": "12.50",
                "compareAtPrice": "15.00",
                "inventoryItem": { "tracked": true, "requiresShipping": true }
            }]
        }),
    ));
    assert_eq!(
        update.body["data"]["productVariantsBulkUpdate"]["userErrors"],
        json!([]),
        "{}",
        update.body
    );
    let location_name = format!("Storefront inventory {product_id}");
    let location_id = add_storefront_inventory_location(proxy, &location_name);
    let inventory = proxy.process_request(json_graphql_request(
        r#"
        mutation SetCartMerchandiseInventory($input: InventorySetQuantitiesInput!) {
          inventorySetQuantities(input: $input) { userErrors { field message code } }
        }
        "#,
        json!({
            "input": {
                "name": "available",
                "reason": "correction",
                "quantities": [{
                    "inventoryItemId": inventory_item_id,
                    "locationId": location_id,
                    "quantity": inventory_quantity,
                    "changeFromQuantity": null
                }]
            }
        }),
    ));
    assert_eq!(
        inventory.body["data"]["inventorySetQuantities"]["userErrors"],
        json!([]),
        "{}",
        inventory.body
    );
    publish_to_current_storefront_channel(proxy, &product_id);
    (product_id, variant_id, location_id)
}

struct StorefrontCartDeliveryProfileFixture {
    id: String,
    location_group_id: String,
    zone_id: String,
    standard_method_id: String,
    standard_rate_id: String,
}

fn stage_storefront_cart_delivery_profile(
    proxy: &mut DraftProxy,
    variant_id: &str,
    location_id: &str,
) -> StorefrontCartDeliveryProfileFixture {
    let response = proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/shipping-fulfillments/delivery-profile-lifecycle-create.graphql"
        ),
        json!({
            "profile": {
                "name": "Storefront cart delivery profile",
                "variantsToAssociate": [variant_id],
                "locationGroupsToCreate": [{
                    "locations": [location_id],
                    "zonesToCreate": [{
                        "name": "Domestic",
                        "countries": [{ "code": "US", "includeAllProvinces": true }],
                        "methodDefinitionsToCreate": [
                            {
                                "name": "Conformance Standard",
                                "description": "Captured fixed storefront cart delivery rate",
                                "active": true,
                                "rateDefinition": { "price": { "amount": "7.25", "currencyCode": "USD" } }
                            },
                            {
                                "name": "Conformance Express",
                                "description": "Captured expedited storefront cart delivery rate",
                                "active": true,
                                "rateDefinition": { "price": { "amount": "12.00", "currencyCode": "USD" } }
                            }
                        ]
                    }]
                }]
            }
        }),
    ));
    assert_eq!(response.status, 200, "{}", response.body);
    assert_eq!(
        response.body["data"]["deliveryProfileCreate"]["userErrors"],
        json!([]),
        "{}",
        response.body
    );
    let profile = &response.body["data"]["deliveryProfileCreate"]["profile"];
    StorefrontCartDeliveryProfileFixture {
        id: profile["id"]
            .as_str()
            .expect("delivery profile id")
            .to_string(),
        location_group_id: profile["profileLocationGroups"][0]["locationGroup"]["id"]
            .as_str()
            .expect("delivery location group id")
            .to_string(),
        zone_id: profile["profileLocationGroups"][0]["locationGroupZones"]["nodes"][0]["zone"]
            ["id"]
            .as_str()
            .expect("delivery zone id")
            .to_string(),
        standard_method_id: profile["profileLocationGroups"][0]["locationGroupZones"]["nodes"][0]
            ["methodDefinitions"]["nodes"][0]["id"]
            .as_str()
            .expect("delivery method id")
            .to_string(),
        standard_rate_id: profile["profileLocationGroups"][0]["locationGroupZones"]["nodes"][0]
            ["methodDefinitions"]["nodes"][0]["rateProvider"]["id"]
            .as_str()
            .expect("delivery rate id")
            .to_string(),
    }
}

#[test]
fn storefront_graphql_route_proxies_request_with_storefront_token_header() {
    let observed_requests = Arc::new(Mutex::new(Vec::<Request>::new()));
    let observed_for_proxy = Arc::clone(&observed_requests);
    let mut proxy = configured_proxy(
        ReadMode::LiveHybrid,
        Some(UnsupportedMutationMode::Passthrough),
    )
    .with_upstream_transport(move |request| {
        observed_for_proxy.lock().unwrap().push(request);
        Response {
            status: 200,
            headers: Default::default(),
            body: json!({
                "data": {
                    "shop": {
                        "name": "Storefront cassette shop"
                    }
                }
            }),
        }
    });

    let request_body = json!({
        "query": "query StorefrontShopNameProxyParity { shop { name } }",
        "variables": {}
    })
    .to_string();
    let response = proxy.process_request(Request {
        method: "POST".to_string(),
        path: "/api/2025-01/graphql.json".to_string(),
        headers: [(
            "x-shopify-storefront-access-token".to_string(),
            "shpat_storefront_token".to_string(),
        )]
        .into(),
        body: request_body.clone(),
    });

    assert_eq!(response.status, 200);
    assert_eq!(
        response.body["data"]["shop"]["name"],
        json!("Storefront cassette shop")
    );

    let observed = observed_requests.lock().unwrap();
    assert_eq!(observed.len(), 1);
    assert_eq!(observed[0].method, "POST");
    assert_eq!(observed[0].path, "/api/2025-01/graphql.json");
    assert_eq!(
        observed[0].headers.get("x-shopify-storefront-access-token"),
        Some(&"shpat_storefront_token".to_string())
    );
    assert_eq!(observed[0].body, request_body);
}

#[test]
fn storefront_graphql_route_rejects_wrong_method_and_unsupported_version() {
    let mut proxy = configured_proxy(
        ReadMode::LiveHybrid,
        Some(UnsupportedMutationMode::Passthrough),
    )
    .with_upstream_transport(|_| panic!("invalid Storefront routes should not call upstream"));

    let wrong_method = proxy.process_request(Request {
        method: "GET".to_string(),
        path: "/api/2025-01/graphql.json".to_string(),
        headers: Default::default(),
        body: String::new(),
    });
    assert_eq!(wrong_method.status, 405);

    let unsupported_version = proxy.process_request(Request {
        method: "POST".to_string(),
        path: "/api/2024-10/graphql.json".to_string(),
        headers: Default::default(),
        body: json!({ "query": "{ shop { name } }" }).to_string(),
    });
    assert_eq!(unsupported_version.status, 404);

    let admin_only_version = proxy.process_request(Request {
        method: "POST".to_string(),
        path: "/api/2025-10/graphql.json".to_string(),
        headers: Default::default(),
        body: json!({ "query": "{ shop { name } }" }).to_string(),
    });
    assert_eq!(admin_only_version.status, 404);
}

#[test]
fn storefront_graphql_route_preserves_private_and_public_storefront_headers() {
    let observed_requests = Arc::new(Mutex::new(Vec::<Request>::new()));
    let observed_for_proxy = Arc::clone(&observed_requests);
    let mut proxy = configured_proxy(
        ReadMode::LiveHybrid,
        Some(UnsupportedMutationMode::Passthrough),
    )
    .with_upstream_transport(move |request| {
        observed_for_proxy.lock().unwrap().push(request);
        Response {
            status: 200,
            headers: Default::default(),
            body: json!({ "data": { "shop": { "name": "Storefront shop" } } }),
        }
    });

    let body = json!({
        "query": "query StorefrontShopName { shop { name } }",
        "variables": {}
    })
    .to_string();
    let response = proxy.process_request(Request {
        method: "POST".to_string(),
        path: "/api/2025-01/graphql.json".to_string(),
        headers: [
            (
                "X-Shopify-Storefront-Access-Token".to_string(),
                "public-token".to_string(),
            ),
            (
                "Shopify-Storefront-Private-Token".to_string(),
                "private-token".to_string(),
            ),
            (
                "Shopify-Storefront-Buyer-IP".to_string(),
                "203.0.113.9".to_string(),
            ),
        ]
        .into(),
        body: body.clone(),
    });

    assert_eq!(response.status, 200);
    let observed = observed_requests.lock().unwrap();
    assert_eq!(observed.len(), 1);
    assert_eq!(observed[0].method, "POST");
    assert_eq!(observed[0].path, "/api/2025-01/graphql.json");
    assert_eq!(observed[0].body, body);
    assert_eq!(
        observed[0].headers.get("X-Shopify-Storefront-Access-Token"),
        Some(&"public-token".to_string())
    );
    assert_eq!(
        observed[0].headers.get("Shopify-Storefront-Private-Token"),
        Some(&"private-token".to_string())
    );
    assert_eq!(
        observed[0].headers.get("Shopify-Storefront-Buyer-IP"),
        Some(&"203.0.113.9".to_string())
    );
}

#[test]
fn storefront_graphql_route_uses_storefront_schema_validation_not_admin_validation() {
    let observed_requests = Arc::new(Mutex::new(Vec::<Request>::new()));
    let observed_for_proxy = Arc::clone(&observed_requests);
    let mut proxy = configured_proxy(
        ReadMode::LiveHybrid,
        Some(UnsupportedMutationMode::Passthrough),
    )
    .with_upstream_transport(move |request| {
        observed_for_proxy.lock().unwrap().push(request);
        Response {
            status: 200,
            headers: Default::default(),
            body: json!({ "data": { "cartCompletionAttempt": null } }),
        }
    });

    let response = proxy.process_request(Request {
        method: "POST".to_string(),
        path: "/api/2026-04/graphql.json".to_string(),
        headers: Default::default(),
        body: json!({
            "query": "query StorefrontCartCompletionAttempt { cartCompletionAttempt(attemptId: \"attempt\") { __typename } }",
            "variables": {}
        })
        .to_string(),
    });

    assert_eq!(response.status, 200);
    assert_eq!(response.body["data"]["cartCompletionAttempt"], Value::Null);
    assert_eq!(observed_requests.lock().unwrap().len(), 1);
}

#[test]
fn storefront_catalog_live_hybrid_without_local_catalog_state_stays_passthrough() {
    let observed_requests = Arc::new(Mutex::new(Vec::<Request>::new()));
    let observed_for_proxy = Arc::clone(&observed_requests);
    let mut proxy = configured_proxy(
        ReadMode::LiveHybrid,
        Some(UnsupportedMutationMode::Passthrough),
    )
    .with_upstream_transport(move |request| {
        observed_for_proxy.lock().unwrap().push(request);
        Response {
            status: 200,
            headers: Default::default(),
            body: json!({
                "data": {
                    "product": {
                        "id": "gid://shopify/Product/upstream",
                        "title": "Upstream Storefront Product"
                    }
                }
            }),
        }
    });

    let body = json!({
        "query": "query StorefrontProductPassthrough { product(id: \"gid://shopify/Product/upstream\") { id title } }",
        "variables": {}
    })
    .to_string();
    let response = proxy.process_request(Request {
        method: "POST".to_string(),
        path: "/api/2026-04/graphql.json".to_string(),
        headers: Default::default(),
        body: body.clone(),
    });

    assert_eq!(response.status, 200);
    assert_eq!(
        response.body["data"]["product"]["title"],
        json!("Upstream Storefront Product")
    );
    let observed = observed_requests.lock().unwrap();
    assert_eq!(observed.len(), 1);
    assert_eq!(observed[0].body, body);
}

#[test]
fn storefront_graphql_route_rejects_roots_missing_from_storefront_schema_before_upstream() {
    let mut proxy = configured_proxy(
        ReadMode::LiveHybrid,
        Some(UnsupportedMutationMode::Passthrough),
    )
    .with_upstream_transport(|_| {
        panic!("Storefront schema validation should fail before upstream")
    });

    let response = proxy.process_request(Request {
        method: "POST".to_string(),
        path: "/api/2026-04/graphql.json".to_string(),
        headers: Default::default(),
        body: json!({
            "query": "query AdminOnlyRoot { productsCount { count } }",
            "variables": {}
        })
        .to_string(),
    });

    assert_eq!(response.status, 200);
    assert_eq!(
        response.body["errors"][0]["extensions"]["code"],
        json!("undefinedField")
    );
    assert_eq!(
        response.body["errors"][0]["extensions"]["fieldName"],
        json!("productsCount")
    );
}

#[test]
fn storefront_graphql_snapshot_mode_returns_schema_shaped_empty_connections_and_enforces_nullability(
) {
    let mut proxy = configured_proxy(ReadMode::Snapshot, Some(UnsupportedMutationMode::Reject))
        .with_upstream_transport(|_| panic!("snapshot Storefront reads should not call upstream"));

    let response = proxy.process_request(Request {
        method: "POST".to_string(),
        path: "/api/2026-04/graphql.json".to_string(),
        headers: Default::default(),
        body: json!({
            "query": "query StorefrontSnapshot($includeNodes: Boolean!) { products(first: 1) { items: nodes @include(if: $includeNodes) { id } ...EmptyPage } } fragment EmptyPage on ProductConnection { nodes: pageInfo { next: hasNextPage previous: hasPreviousPage } }",
            "variables": { "includeNodes": true }
        })
        .to_string(),
    });

    assert_eq!(response.status, 200);
    assert_eq!(response.body["data"]["products"]["items"], json!([]));
    assert_eq!(
        response.body["data"]["products"]["nodes"],
        json!({ "next": false, "previous": false })
    );

    let missing_shop = proxy.process_request(Request {
        method: "POST".to_string(),
        path: "/api/2026-04/graphql.json".to_string(),
        headers: Default::default(),
        body: json!({ "query": "query MissingSnapshotShop { shop { name } }" }).to_string(),
    });
    assert_eq!(missing_shop.status, 200);
    assert_eq!(missing_shop.body["data"], Value::Null);
    assert_eq!(
        missing_shop.body["errors"][0]["message"],
        json!("Storefront snapshot has no value for non-null root `QueryRoot.shop`")
    );
}

#[test]
fn storefront_graphql_snapshot_mode_rejects_mutations_without_upstream() {
    let mut proxy = configured_proxy(ReadMode::Snapshot, Some(UnsupportedMutationMode::Reject))
        .with_upstream_transport(|_| {
            panic!("snapshot Storefront mutations should not call upstream")
        });

    let response = proxy.process_request(Request {
        method: "POST".to_string(),
        path: "/api/2026-04/graphql.json".to_string(),
        headers: Default::default(),
        body: json!({
            "query": "mutation StorefrontBillingAddress { cartBillingAddressUpdate(cartId: \"gid://shopify/Cart/1\", billingAddress: null) { cart { id } } }",
            "variables": {}
        })
        .to_string(),
    });

    assert_eq!(response.status, 501);
    assert_eq!(
        response.body,
        json!({ "errors": [{ "message": "Storefront API mutations are not locally implemented in snapshot mode" }] })
    );
}

#[test]
fn storefront_catalog_roots_read_visible_products_from_shared_state() {
    let current_publication_id = "gid://shopify/Publication/current-storefront";
    let visible_product_id = "gid://shopify/Product/visible-storefront";
    let unpublished_product_id = "gid://shopify/Product/unpublished-storefront";
    let draft_product_id = "gid://shopify/Product/draft-storefront";
    let mut draft_product = storefront_product_fixture(
        draft_product_id,
        "Draft Storefront Product",
        "draft-storefront-product",
        Some(current_publication_id),
    );
    draft_product.status = "DRAFT".to_string();
    let mut proxy = configured_proxy(ReadMode::Snapshot, Some(UnsupportedMutationMode::Reject))
        .with_upstream_transport(|_| panic!("snapshot Storefront catalog should not call upstream"))
        .with_base_products(vec![
            storefront_product_fixture(
                visible_product_id,
                "Visible Storefront Product",
                "visible-storefront-product",
                Some(current_publication_id),
            ),
            storefront_product_fixture(
                unpublished_product_id,
                "Unpublished Storefront Product",
                "unpublished-storefront-product",
                None,
            ),
            draft_product,
        ]);
    restore_state_with(&mut proxy, |state| {
        state["baseState"]["publicationIds"] = json!([current_publication_id]);
        state["baseState"]["publicationCount"] = json!(1);
        state["stagedState"]["currentChannelPublicationId"] = json!(current_publication_id);
        state["stagedState"]["currentChannelPublicationResolved"] = json!(true);
    });

    let response = proxy.process_request(storefront_graphql_request(
        r#"
        query StorefrontCatalogCore($id: ID!, $handle: String!) {
          byId: product(id: $id) {
            id
            title
            handle
            description
            descriptionHtml
            availableForSale
            totalInventory
            vendor
            productType
            tags
            publishedAt
            seo { title description }
          }
          byHandle: productByHandle(handle: $handle) {
            id
            title
            handle
          }
          products(first: 10, sortKey: TITLE) {
            nodes {
              id
              title
              handle
            }
            pageInfo { hasNextPage hasPreviousPage }
          }
        }
        "#,
        json!({
            "id": visible_product_id,
            "handle": "visible-storefront-product"
        }),
    ));

    assert_eq!(response.status, 200);
    assert_eq!(
        response.body["data"],
        json!({
            "byId": {
                "id": visible_product_id,
                "title": "Visible Storefront Product",
                "handle": "visible-storefront-product",
                "description": "Visible Storefront Product description",
                "descriptionHtml": "<p>Visible Storefront Product description</p>",
                "availableForSale": true,
                "totalInventory": 7,
                "vendor": "Hermes",
                "productType": "Accessory",
                "tags": ["storefront", "catalog"],
                "publishedAt": "2024-01-03T00:00:00.000Z",
                "seo": {
                    "title": "Visible Storefront Product SEO",
                    "description": "Visible Storefront Product SEO description"
                }
            },
            "byHandle": {
                "id": visible_product_id,
                "title": "Visible Storefront Product",
                "handle": "visible-storefront-product"
            },
            "products": {
                "nodes": [{
                    "id": visible_product_id,
                    "title": "Visible Storefront Product",
                    "handle": "visible-storefront-product"
                }],
                "pageInfo": {
                    "hasNextPage": false,
                    "hasPreviousPage": false
                }
            }
        })
    );
}

#[test]
fn storefront_catalog_enrichment_roots_use_visible_shared_state_with_pagination_and_fragments() {
    let publication_id = "gid://shopify/Publication/storefront-enrichment";
    let source_id = "gid://shopify/Product/storefront-enrichment-source";
    let candidate_id = "gid://shopify/Product/storefront-enrichment-candidate";
    let outsider_id = "gid://shopify/Product/storefront-enrichment-outsider";
    let mut source = storefront_product_fixture(
        source_id,
        "Source product",
        "source-product",
        Some(publication_id),
    );
    source.product_type = "Shirt".to_string();
    source.tags = vec!["shared".to_string(), "source".to_string()];
    let mut candidate = storefront_product_fixture(
        candidate_id,
        "Best candidate",
        "best-candidate",
        Some(publication_id),
    );
    candidate.product_type = "Shirt".to_string();
    candidate.tags = vec!["shared".to_string(), "candidate".to_string()];
    let mut outsider = storefront_product_fixture(
        outsider_id,
        "Outside candidate",
        "outside-candidate",
        Some(publication_id),
    );
    outsider.vendor = "Other vendor".to_string();
    outsider.product_type = "Other".to_string();
    outsider.tags = vec!["other".to_string()];
    let mut hidden = storefront_product_fixture(
        "gid://shopify/Product/storefront-enrichment-hidden",
        "Hidden candidate",
        "hidden-candidate",
        None,
    );
    hidden.tags = vec!["hidden".to_string()];

    let mut proxy = configured_proxy(ReadMode::Snapshot, Some(UnsupportedMutationMode::Reject))
        .with_upstream_transport(|_| panic!("snapshot enrichment roots must stay local"))
        .with_base_products(vec![source, candidate, outsider, hidden]);
    restore_storefront_current_publication(&mut proxy, publication_id);

    let response = proxy.process_request(storefront_graphql_request(
        r#"
        query StorefrontEnrichmentRoots($sourceId: ID!, $missingId: ID!) {
          tags: productTags(first: 2) {
            edges { cursor node }
            nodes
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
          types: productTypes(first: 2) { nodes }
          recommendations: productRecommendations(productId: $sourceId, intent: RELATED) {
            ...RecommendationFields
          }
          recommendationsByHandle: productRecommendations(productHandle: "source-product", intent: RELATED) {
            ...RecommendationFields
          }
          missing: productRecommendations(productId: $missingId, intent: RELATED) { id }
        }
        fragment RecommendationFields on Product { id title handle productType tags }
        "#,
        json!({
            "sourceId": source_id,
            "missingId": "gid://shopify/Product/missing"
        }),
    ));

    assert_eq!(response.status, 200);
    assert_eq!(
        response.body["data"]["tags"]["nodes"],
        json!(["candidate", "other"]),
        "{:#?}",
        response.body
    );
    assert_eq!(
        response.body["data"]["tags"]["edges"][0]["node"],
        json!("candidate")
    );
    assert_eq!(
        response.body["data"]["tags"]["pageInfo"]["hasNextPage"],
        json!(true)
    );
    assert_eq!(
        response.body["data"]["types"]["nodes"],
        json!(["Other", "Shirt"])
    );
    assert_eq!(
        response.body["data"]["recommendations"][0]["id"],
        json!(candidate_id)
    );
    assert_eq!(
        response.body["data"]["recommendations"][1]["id"],
        json!(outsider_id)
    );
    assert_eq!(
        response.body["data"]["recommendationsByHandle"],
        response.body["data"]["recommendations"]
    );
    assert_eq!(response.body["data"]["missing"], Value::Null);
    assert!(!response.body.to_string().contains("hidden-candidate"));
}

#[test]
fn storefront_catalog_enrichment_projects_staged_media_metafields_and_selling_plans() {
    let publication_id = "gid://shopify/Publication/storefront-enrichment";
    let mut proxy = configured_proxy(ReadMode::Snapshot, Some(UnsupportedMutationMode::Reject))
        .with_upstream_transport(|_| panic!("staged Storefront enrichment must stay local"));
    restore_state_with(&mut proxy, |state| {
        state["baseState"]["shop"] = json!({ "currencyCode": "CAD" });
        state["baseState"]["publicationIds"] = json!([publication_id]);
        state["baseState"]["publicationCount"] = json!(1);
        state["stagedState"]["currentChannelPublicationId"] = json!(publication_id);
        state["stagedState"]["currentChannelPublicationResolved"] = json!(true);
    });

    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateStorefrontEnrichment($product: ProductCreateInput!, $media: [CreateMediaInput!]) {
          productCreate(product: $product, media: $media) {
            product { id handle variants(first: 1) { nodes { id } } }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "product": {
                "title": "Staged enrichment product",
                "handle": "staged-enrichment-product",
                "status": "ACTIVE",
                "vendor": "Hermes",
                "productType": "Subscription",
                "tags": ["staged", "enrichment"],
                "productOptions": [{ "name": "Color", "values": [{ "name": "Blue" }] }]
            },
            "media": [{
                "alt": "Staged enrichment image",
                "mediaContentType": "IMAGE",
                "originalSource": "https://placehold.co/640x480/png?text=enrichment"
            }]
        }),
    ));
    assert_eq!(create.status, 200);
    assert_eq!(
        create.body["data"]["productCreate"]["userErrors"],
        json!([])
    );
    let product_id = create.body["data"]["productCreate"]["product"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let variant_id = create.body["data"]["productCreate"]["product"]["variants"]["nodes"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    publish_to_current_storefront_channel(&mut proxy, &product_id);

    stage_metafield_definition(
        &mut proxy,
        "PRODUCT",
        "custom",
        "visible",
        "single_line_text_field",
        "PUBLIC_READ",
    );
    stage_metafield_definition(
        &mut proxy,
        "PRODUCT",
        "custom",
        "hidden",
        "single_line_text_field",
        "NONE",
    );
    stage_metafield_definition(
        &mut proxy,
        "PRODUCTVARIANT",
        "custom",
        "variant_visible",
        "single_line_text_field",
        "PUBLIC_READ",
    );
    stage_metafields_set(
        &mut proxy,
        &product_id,
        json!([
            { "namespace": "custom", "key": "visible", "type": "single_line_text_field", "value": "public value" },
            { "namespace": "custom", "key": "hidden", "type": "single_line_text_field", "value": "private value" }
        ]),
    );
    stage_metafields_set(
        &mut proxy,
        &variant_id,
        json!([
            { "namespace": "custom", "key": "variant_visible", "type": "single_line_text_field", "value": "variant value" }
        ]),
    );

    let selling_plan = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateStorefrontSellingPlan($input: SellingPlanGroupInput!, $resources: SellingPlanGroupResourceInput!) {
          sellingPlanGroupCreate(input: $input, resources: $resources) {
            sellingPlanGroup { id }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "input": {
                "name": "Subscribe and save",
                "merchantCode": "subscribe-save",
                "options": ["Delivery frequency"],
                "position": 1,
                "sellingPlansToCreate": [{
                    "name": "Monthly",
                    "description": "Monthly delivery",
                    "options": ["Every month"],
                    "position": 1,
                    "category": "SUBSCRIPTION",
                    "billingPolicy": { "recurring": { "interval": "MONTH", "intervalCount": 1 } },
                    "deliveryPolicy": { "recurring": { "interval": "MONTH", "intervalCount": 1 } },
                    "pricingPolicies": [{
                        "fixed": {
                            "adjustmentType": "PERCENTAGE",
                            "adjustmentValue": { "percentage": 15 }
                        }
                    }]
                }]
            },
            "resources": { "productIds": [product_id] }
        }),
    ));
    assert_eq!(selling_plan.status, 200);
    assert_eq!(
        selling_plan.body["data"]["sellingPlanGroupCreate"]["userErrors"],
        json!([])
    );

    let update = proxy.process_request(json_graphql_request(
        r#"
        mutation PriceStorefrontVariant($productId: ID!, $variants: [ProductVariantsBulkInput!]!) {
          productVariantsBulkUpdate(productId: $productId, variants: $variants) {
            productVariants { id price compareAtPrice }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "productId": product_id,
            "variants": [{ "id": variant_id, "price": "149.00", "compareAtPrice": "179.00" }]
        }),
    ));
    assert_eq!(update.status, 200);
    assert_eq!(
        update.body["data"]["productVariantsBulkUpdate"]["userErrors"],
        json!([])
    );

    let response = proxy.process_request(storefront_graphql_request(
        r#"
        query StagedStorefrontEnrichment($id: ID!) {
          product(id: $id) {
            featuredImage { id url altText width height }
            images(first: 1) { nodes { id width height } }
            media(first: 1) { nodes { __typename id mediaContentType previewImage { url width height } } }
            visible: metafield(namespace: "custom", key: "visible") { key value }
            hidden: metafield(namespace: "custom", key: "hidden") { key value }
            selected: metafields(identifiers: [
              { namespace: "custom", key: "visible" }
              { namespace: "custom", key: "hidden" }
            ]) { key value }
            sellingPlanGroups(first: 1) {
              nodes { name sellingPlans(first: 1) { nodes { name recurringDeliveries } } }
            }
            variants(first: 1) {
              nodes {
                image { width height }
                metafield(namespace: "custom", key: "variant_visible") { key value }
                sellingPlanAllocations(first: 1) {
                  nodes {
                    checkoutChargeAmount { amount currencyCode }
                    remainingBalanceChargeAmount { amount currencyCode }
                    sellingPlan { name }
                  }
                }
              }
            }
          }
        }
        "#,
        json!({ "id": product_id }),
    ));

    assert_eq!(response.status, 200);
    let product = &response.body["data"]["product"];
    assert_eq!(
        product["featuredImage"]["width"],
        json!(640),
        "{:#?}",
        response.body
    );
    assert_eq!(product["featuredImage"]["height"], json!(480));
    assert_eq!(
        product["media"]["nodes"][0]["mediaContentType"],
        json!("IMAGE")
    );
    assert_eq!(
        product["visible"],
        json!({ "key": "visible", "value": "public value" })
    );
    assert_eq!(product["hidden"], Value::Null);
    assert_eq!(product["selected"][1], Value::Null);
    assert_eq!(
        product["variants"]["nodes"][0]["metafield"]["value"],
        json!("variant value")
    );
    assert_eq!(
        product["variants"]["nodes"][0]["sellingPlanAllocations"]["nodes"][0]
            ["checkoutChargeAmount"],
        json!({ "amount": "126.65", "currencyCode": "CAD" })
    );
    assert_eq!(
        product["sellingPlanGroups"]["nodes"][0]["sellingPlans"]["nodes"][0]["recurringDeliveries"],
        json!(true)
    );
}

#[test]
fn storefront_catalog_enrichment_hydrates_taxonomy_and_isolates_extended_contexts() {
    let observed = Arc::new(Mutex::new(Vec::<Value>::new()));
    let observed_for_transport = Arc::clone(&observed);
    let mut proxy = configured_proxy(
        ReadMode::LiveHybrid,
        Some(UnsupportedMutationMode::Passthrough),
    )
    .with_upstream_transport(move |request| {
        let body: Value = serde_json::from_str(&request.body).unwrap();
        observed_for_transport.lock().unwrap().push(body.clone());
        let query = body["query"].as_str().unwrap_or_default();
        if query.contains("StorefrontEnrichmentTaxonomyHydrate") {
            return Response {
                status: 200,
                headers: Default::default(),
                body: json!({
                    "data": {
                        "productTags": {
                            "edges": [
                                { "cursor": "QWxwaGE=", "node": "Alpha" },
                                { "cursor": "QmV0YQ==", "node": "Beta" }
                            ],
                            "nodes": ["Alpha", "Beta"],
                            "pageInfo": {
                                "hasNextPage": false,
                                "hasPreviousPage": false,
                                "startCursor": "QWxwaGE=",
                                "endCursor": "QmV0YQ=="
                            }
                        },
                        "productTypes": {
                            "edges": [{ "cursor": "U2hpcnQ=", "node": "Shirt" }],
                            "nodes": ["Shirt"],
                            "pageInfo": {
                                "hasNextPage": false,
                                "hasPreviousPage": false,
                                "startCursor": "U2hpcnQ=",
                                "endCursor": "U2hpcnQ="
                            }
                        }
                    }
                }),
            };
        }
        if query.contains("StorefrontEnrichmentContextHydrate") {
            let country = body["variables"]["country"].as_str().unwrap_or("AE");
            let (currency, market_id, handle) = if country == "DK" {
                ("DKK", "gid://shopify/Market/denmark", "denmark")
            } else {
                ("CAD", "gid://shopify/Market/international", "international")
            };
            return Response {
                status: 200,
                headers: Default::default(),
                body: json!({
                    "data": {
                        "localization": {
                            "country": { "isoCode": country, "currency": { "isoCode": currency } },
                            "language": { "isoCode": "EN" },
                            "market": { "id": market_id, "handle": handle }
                        }
                    }
                }),
            };
        }
        panic!("unexpected Storefront enrichment hydrate: {query}");
    });

    let taxonomy = proxy.process_request(storefront_graphql_request(
        r#"
        query HydratedStorefrontTaxonomy {
          productTags(first: 1) {
            edges { cursor node }
            nodes
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
          productTypes(first: 5) { nodes }
        }
        "#,
        json!({}),
    ));
    assert_eq!(taxonomy.status, 200);
    assert_eq!(
        taxonomy.body["data"]["productTags"]["nodes"],
        json!(["Alpha"])
    );
    assert_eq!(
        taxonomy.body["data"]["productTags"]["pageInfo"]["hasNextPage"],
        json!(true)
    );
    assert_eq!(
        taxonomy.body["data"]["productTypes"]["nodes"],
        json!(["Shirt"])
    );

    let context_query = r#"
        query HydratedStorefrontContext(
          $country: CountryCode
          $language: LanguageCode
          $preferredLocationId: ID
          $buyer: BuyerInput
        ) @inContext(
          country: $country
          language: $language
          preferredLocationId: $preferredLocationId
          buyer: $buyer
        ) {
          localization {
            country { isoCode currency { isoCode } }
            language { isoCode }
            market { id handle }
          }
        }
    "#;
    let default_context = proxy.process_request(storefront_graphql_request(
        context_query,
        json!({
            "country": null,
            "language": null,
            "preferredLocationId": null,
            "buyer": null
        }),
    ));
    let denmark_context = proxy.process_request(storefront_graphql_request(
        context_query,
        json!({
            "country": "DK",
            "language": "EN",
            "preferredLocationId": null,
            "buyer": null
        }),
    ));
    let preferred_context = proxy.process_request(storefront_graphql_request(
        context_query,
        json!({
            "country": "DK",
            "language": "EN",
            "preferredLocationId": "gid://shopify/Location/local-synthetic",
            "buyer": null
        }),
    ));
    assert_eq!(
        default_context.body["data"]["localization"]["country"]["currency"]["isoCode"],
        json!("CAD")
    );
    assert_eq!(
        denmark_context.body["data"]["localization"]["country"]["currency"]["isoCode"],
        json!("DKK")
    );
    assert_eq!(preferred_context.body["data"], denmark_context.body["data"]);

    let invalid_buyer = proxy.process_request(storefront_graphql_request(
        context_query,
        json!({
            "country": "DK",
            "language": "EN",
            "preferredLocationId": null,
            "buyer": {
                "customerAccessToken": "invalid-token",
                "companyLocationId": "gid://shopify/CompanyLocation/1"
            }
        }),
    ));
    assert_eq!(
        invalid_buyer.body,
        json!({ "errors": [{ "message": "The token provided is not valid" }] })
    );

    let observed = observed.lock().unwrap();
    assert_eq!(observed.len(), 3);
    assert!(observed[0]["query"]
        .as_str()
        .unwrap()
        .contains("StorefrontEnrichmentTaxonomyHydrate"));
    assert_eq!(observed[1]["variables"]["preferredLocationId"], Value::Null);
    assert_eq!(observed[2]["variables"]["country"], json!("DK"));
    assert_eq!(observed[2]["variables"]["preferredLocationId"], Value::Null);
}

#[test]
fn storefront_catalog_reflects_admin_staged_lifecycle_and_variant_inventory() {
    let current_publication_id = "gid://shopify/Publication/current-storefront";
    let mut proxy = configured_proxy(ReadMode::Snapshot, Some(UnsupportedMutationMode::Reject))
        .with_upstream_transport(|_| {
            panic!("snapshot Storefront catalog should not call upstream")
        });
    restore_storefront_current_publication(&mut proxy, current_publication_id);
    restore_state_with(&mut proxy, |state| {
        state["baseState"]["shop"] = json!({ "currencyCode": "USD" });
    });

    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation StorefrontCatalogCreate($product: ProductCreateInput!) {
          productCreate(product: $product) {
            product { id title handle }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "product": {
                "title": "Stage Storefront Tee",
                "handle": "stage-storefront-tee",
                "status": "ACTIVE",
                "descriptionHtml": "<p>Stage catalog body</p>",
                "vendor": "Hermes",
                "productType": "Tee",
                "tags": ["new", "storefront"]
            }
        }),
    ));
    assert_eq!(create.status, 200);
    assert_eq!(
        create.body["data"]["productCreate"]["userErrors"],
        json!([])
    );
    let product_id = create.body["data"]["productCreate"]["product"]["id"]
        .as_str()
        .expect("productCreate should return id")
        .to_string();
    publish_to_current_storefront_channel(&mut proxy, &product_id);

    let variant = create_legacy_variant(&mut proxy, &product_id, "STAGE-TEE", "12.50");
    let variant_id = variant["id"]
        .as_str()
        .expect("variant create should return id")
        .to_string();
    let inventory_item_id = variant["inventoryItem"]["id"]
        .as_str()
        .expect("variant create should return inventory item id")
        .to_string();

    let update_variant = proxy.process_request(json_graphql_request(
        r#"
        mutation StorefrontVariantUpdate($productId: ID!, $variants: [ProductVariantsBulkInput!]!) {
          productVariantsBulkUpdate(productId: $productId, variants: $variants) {
            productVariants { id sku barcode price compareAtPrice selectedOptions { name value } inventoryItem { tracked requiresShipping } }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "productId": product_id,
            "variants": [{
                "id": variant_id,
                "barcode": "stage-barcode",
                "price": "14.25",
                "compareAtPrice": "18.00",
                "optionValues": [{ "optionName": "Title", "name": "Storefront Red" }],
                "inventoryItem": {
                    "sku": "STAGE-RED",
                    "tracked": true,
                    "requiresShipping": false
                }
            }]
        }),
    ));
    assert_eq!(update_variant.status, 200);
    assert_eq!(
        update_variant.body["data"]["productVariantsBulkUpdate"]["userErrors"],
        json!([])
    );

    let location_id = add_storefront_inventory_location(&mut proxy, "Storefront inventory");
    let set_inventory = proxy.process_request(json_graphql_request(
        r#"
        mutation StorefrontInventorySet($input: InventorySetQuantitiesInput!) {
          inventorySetQuantities(input: $input) {
            userErrors { field message }
          }
        }
        "#,
        json!({
            "input": {
                "name": "available",
                "reason": "correction",
                "quantities": [{
                    "inventoryItemId": inventory_item_id,
                    "locationId": location_id,
                    "quantity": 5,
                    "changeFromQuantity": null
                }]
            }
        }),
    ));
    assert_eq!(set_inventory.status, 200);
    assert_eq!(
        set_inventory.body["data"]["inventorySetQuantities"]["userErrors"],
        json!([])
    );

    let storefront = proxy.process_request(storefront_graphql_request(
        r#"
        query StorefrontCatalogAfterAdminWrites($handle: String!) {
          productByHandle(handle: $handle) {
            id
            title
            handle
            availableForSale
            priceRange { minVariantPrice { amount currencyCode } maxVariantPrice { amount currencyCode } }
            compareAtPriceRange { minVariantPrice { amount currencyCode } maxVariantPrice { amount currencyCode } }
            variants(first: 5) {
              nodes {
                id
                title
                sku
                barcode
                availableForSale
                quantityAvailable
                requiresShipping
                price { amount currencyCode }
                compareAtPrice { amount currencyCode }
                selectedOptions { name value }
              }
            }
          }
        }
        "#,
        json!({ "handle": "stage-storefront-tee" }),
    ));
    assert_eq!(storefront.status, 200);
    assert_eq!(
        storefront.body["data"]["productByHandle"],
        json!({
            "id": product_id,
            "title": "Stage Storefront Tee",
            "handle": "stage-storefront-tee",
            "availableForSale": true,
            "priceRange": {
                "minVariantPrice": { "amount": "14.25", "currencyCode": "USD" },
                "maxVariantPrice": { "amount": "14.25", "currencyCode": "USD" }
            },
            "compareAtPriceRange": {
                "minVariantPrice": { "amount": "18.0", "currencyCode": "USD" },
                "maxVariantPrice": { "amount": "18.0", "currencyCode": "USD" }
            },
            "variants": {
                "nodes": [{
                    "id": variant_id,
                    "title": "Storefront Red",
                    "sku": "STAGE-RED",
                    "barcode": "stage-barcode",
                    "availableForSale": true,
                    "quantityAvailable": 5,
                    "requiresShipping": false,
                    "price": { "amount": "14.25", "currencyCode": "USD" },
                    "compareAtPrice": { "amount": "18.0", "currencyCode": "USD" },
                    "selectedOptions": [{ "name": "Title", "value": "Storefront Red" }]
                }]
            }
        })
    );

    let update_product = proxy.process_request(json_graphql_request(
        r#"
        mutation StorefrontCatalogUpdate($product: ProductUpdateInput!) {
          productUpdate(product: $product) {
            product { id title handle }
            userErrors { field message }
          }
        }
        "#,
        json!({ "product": { "id": product_id, "title": "Updated Storefront Tee", "handle": "updated-storefront-tee" } }),
    ));
    assert_eq!(update_product.status, 200);
    assert_eq!(
        update_product.body["data"]["productUpdate"]["userErrors"],
        json!([])
    );

    let updated = proxy.process_request(storefront_graphql_request(
        r#"
        query StorefrontUpdatedHandle($oldHandle: String!, $newHandle: String!) {
          old: productByHandle(handle: $oldHandle) { id }
          new: productByHandle(handle: $newHandle) { id title handle }
        }
        "#,
        json!({
            "oldHandle": "stage-storefront-tee",
            "newHandle": "updated-storefront-tee"
        }),
    ));
    assert_eq!(updated.status, 200);
    assert_eq!(updated.body["data"]["old"], Value::Null);
    assert_eq!(
        updated.body["data"]["new"],
        json!({
            "id": product_id,
            "title": "Updated Storefront Tee",
            "handle": "updated-storefront-tee"
        })
    );

    unpublish_from_current_storefront_channel(&mut proxy, &product_id);
    let unpublished = proxy.process_request(storefront_graphql_request(
        r#"
        query StorefrontUnpublishedProduct($id: ID!) {
          product(id: $id) { id }
          products(first: 10) { nodes { id } }
        }
        "#,
        json!({ "id": product_id }),
    ));
    assert_eq!(unpublished.status, 200);
    assert_eq!(unpublished.body["data"]["product"], Value::Null);
    assert_eq!(unpublished.body["data"]["products"]["nodes"], json!([]));

    publish_to_current_storefront_channel(&mut proxy, &product_id);
    let delete = proxy.process_request(json_graphql_request(
        r#"
        mutation StorefrontCatalogDelete($id: ID!) {
          productDelete(input: { id: $id }) {
            deletedProductId
            userErrors { field message }
          }
        }
        "#,
        json!({ "id": product_id }),
    ));
    assert_eq!(delete.status, 200);
    assert_eq!(
        delete.body["data"]["productDelete"]["userErrors"],
        json!([])
    );

    let deleted = proxy.process_request(storefront_graphql_request(
        r#"
        query StorefrontDeletedProduct($id: ID!) {
          product(id: $id) { id }
          products(first: 10) { nodes { id } }
        }
        "#,
        json!({ "id": product_id }),
    ));
    assert_eq!(deleted.status, 200);
    assert_eq!(deleted.body["data"]["product"], Value::Null);
    assert_eq!(deleted.body["data"]["products"]["nodes"], json!([]));
}

#[test]
fn storefront_catalog_uses_explicit_known_publication_when_current_context_unresolved() {
    let publication_id = "gid://shopify/Publication/online-store";
    let mut proxy = configured_proxy(ReadMode::Snapshot, Some(UnsupportedMutationMode::Reject))
        .with_upstream_transport(|_| {
            panic!("snapshot Storefront catalog should not call upstream")
        });
    restore_state_with(&mut proxy, |state| {
        state["baseState"]["publicationIds"] = json!([publication_id]);
        state["baseState"]["publicationCount"] = json!(1);
    });

    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation StorefrontExplicitPublicationCreate($product: ProductCreateInput!) {
          productCreate(product: $product) {
            product { id title handle }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "product": {
                "title": "Explicit Storefront Product",
                "handle": "explicit-storefront-product",
                "status": "ACTIVE"
            }
        }),
    ));
    assert_eq!(create.status, 200);
    assert_eq!(
        create.body["data"]["productCreate"]["userErrors"],
        json!([])
    );
    let product_id = create.body["data"]["productCreate"]["product"]["id"]
        .as_str()
        .expect("productCreate should return id")
        .to_string();

    let hidden_before_publish = proxy.process_request(storefront_graphql_request(
        r#"
        query StorefrontExplicitPublicationHidden($handle: String!) {
          productByHandle(handle: $handle) { id }
        }
        "#,
        json!({ "handle": "explicit-storefront-product" }),
    ));
    assert_eq!(hidden_before_publish.status, 200);
    assert_eq!(
        hidden_before_publish.body["data"]["productByHandle"],
        Value::Null
    );

    let publish = proxy.process_request(json_graphql_request(
        r#"
        mutation StorefrontExplicitPublicationPublish($id: ID!, $input: [PublicationInput!]!) {
          publishablePublish(id: $id, input: $input) {
            publishable { ... on Product { id } }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "id": product_id,
            "input": [{ "publicationId": publication_id }]
        }),
    ));
    assert_eq!(publish.status, 200);
    assert_eq!(
        publish.body["data"]["publishablePublish"]["userErrors"],
        json!([])
    );

    let visible = proxy.process_request(storefront_graphql_request(
        r#"
        query StorefrontExplicitPublicationVisible($handle: String!) {
          productByHandle(handle: $handle) {
            id
            title
            handle
          }
          products(first: 5) { nodes { id } }
        }
        "#,
        json!({ "handle": "explicit-storefront-product" }),
    ));
    assert_eq!(visible.status, 200);
    assert_eq!(
        visible.body["data"]["productByHandle"],
        json!({
            "id": product_id,
            "title": "Explicit Storefront Product",
            "handle": "explicit-storefront-product"
        })
    );
    assert_eq!(
        visible.body["data"]["products"]["nodes"],
        json!([{ "id": product_id }])
    );

    let unpublish = proxy.process_request(json_graphql_request(
        r#"
        mutation StorefrontExplicitPublicationUnpublish($id: ID!, $input: [PublicationInput!]!) {
          publishableUnpublish(id: $id, input: $input) {
            publishable { ... on Product { id } }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "id": product_id,
            "input": [{ "publicationId": publication_id }]
        }),
    ));
    assert_eq!(unpublish.status, 200);
    assert_eq!(
        unpublish.body["data"]["publishableUnpublish"]["userErrors"],
        json!([])
    );

    let hidden_after_unpublish = proxy.process_request(storefront_graphql_request(
        r#"
        query StorefrontExplicitPublicationUnpublished($handle: String!) {
          productByHandle(handle: $handle) { id }
          products(first: 5) { nodes { id } }
        }
        "#,
        json!({ "handle": "explicit-storefront-product" }),
    ));
    assert_eq!(hidden_after_unpublish.status, 200);
    assert_eq!(
        hidden_after_unpublish.body["data"]["productByHandle"],
        Value::Null
    );
    assert_eq!(
        hidden_after_unpublish.body["data"]["products"]["nodes"],
        json!([])
    );
}

#[test]
fn storefront_products_connection_search_sort_window_and_fragments_use_visible_catalog() {
    let current_publication_id = "gid://shopify/Publication/current-storefront";
    let mut alpha = storefront_product_fixture(
        "gid://shopify/Product/alpha-storefront",
        "Alpha Jacket",
        "alpha-jacket",
        Some(current_publication_id),
    );
    alpha.vendor = "Northwind".to_string();
    alpha.product_type = "Jackets".to_string();
    alpha.created_at = "2024-01-01T00:00:00.000Z".to_string();
    alpha.updated_at = "2024-01-03T00:00:00.000Z".to_string();
    let mut beta = storefront_product_fixture(
        "gid://shopify/Product/beta-storefront",
        "Beta Jacket",
        "beta-jacket",
        Some(current_publication_id),
    );
    beta.vendor = "Southwind".to_string();
    beta.product_type = "Jackets".to_string();
    beta.created_at = "2024-01-02T00:00:00.000Z".to_string();
    beta.updated_at = "2024-01-02T00:00:00.000Z".to_string();
    let mut gamma = storefront_product_fixture(
        "gid://shopify/Product/gamma-storefront",
        "Gamma Shirt",
        "gamma-shirt",
        Some(current_publication_id),
    );
    gamma.vendor = "Northwind".to_string();
    gamma.product_type = "Shirts".to_string();
    gamma.created_at = "2024-01-03T00:00:00.000Z".to_string();
    gamma.updated_at = "2024-01-01T00:00:00.000Z".to_string();
    let draft = {
        let mut product = storefront_product_fixture(
            "gid://shopify/Product/draft-filter-storefront",
            "Draft Northwind",
            "draft-northwind",
            Some(current_publication_id),
        );
        product.status = "DRAFT".to_string();
        product
    };
    let mut proxy = configured_proxy(ReadMode::Snapshot, Some(UnsupportedMutationMode::Reject))
        .with_base_products(vec![alpha, beta, gamma, draft])
        .with_upstream_transport(|_| {
            panic!("snapshot Storefront catalog should not call upstream")
        });
    restore_state_with(&mut proxy, |state| {
        state["baseState"]["shop"] = json!({ "currencyCode": "USD" });
    });
    restore_storefront_current_publication(&mut proxy, current_publication_id);
    create_legacy_variant(
        &mut proxy,
        "gid://shopify/Product/alpha-storefront",
        "ALPHA-PRICE",
        "30.00",
    );
    create_legacy_variant(
        &mut proxy,
        "gid://shopify/Product/beta-storefront",
        "BETA-PRICE",
        "10.00",
    );
    create_legacy_variant(
        &mut proxy,
        "gid://shopify/Product/gamma-storefront",
        "GAMMA-PRICE",
        "20.00",
    );

    let response = proxy.process_request(storefront_graphql_request(
        r#"
        query StorefrontConnectionWindows($query: String!, $after: String!) {
          northwind: products(first: 2, query: $query, sortKey: TITLE, reverse: true) {
            nodes {
              ...ProductCard
            }
            edges { cursor node { id } }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
          pageAfterAlpha: products(first: 2, sortKey: TITLE, after: $after) {
            nodes { id title }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
          priceSorted: products(first: 3, sortKey: PRICE) {
            nodes {
              handle
              priceRange { minVariantPrice { amount } }
            }
          }
          byId: product(id: "gid://shopify/Product/alpha-storefront") {
            ... on Product {
              aliasTitle: title
              handle
            }
          }
        }

        fragment ProductCard on Product {
          id
          title
          handle
          vendor
        }
        "#,
        json!({
            "query": "vendor:Northwind",
            "after": "gid://shopify/Product/alpha-storefront"
        }),
    ));

    assert_eq!(response.status, 200);
    assert_eq!(
        response.body["data"]["northwind"]["nodes"],
        json!([
            {
                "id": "gid://shopify/Product/gamma-storefront",
                "title": "Gamma Shirt",
                "handle": "gamma-shirt",
                "vendor": "Northwind"
            },
            {
                "id": "gid://shopify/Product/alpha-storefront",
                "title": "Alpha Jacket",
                "handle": "alpha-jacket",
                "vendor": "Northwind"
            }
        ])
    );
    assert_eq!(
        response.body["data"]["northwind"]["pageInfo"],
        json!({
            "hasNextPage": false,
            "hasPreviousPage": false,
            "startCursor": "gid://shopify/Product/gamma-storefront",
            "endCursor": "gid://shopify/Product/alpha-storefront"
        })
    );
    assert_eq!(
        response.body["data"]["pageAfterAlpha"]["nodes"],
        json!([
            { "id": "gid://shopify/Product/beta-storefront", "title": "Beta Jacket" },
            { "id": "gid://shopify/Product/gamma-storefront", "title": "Gamma Shirt" }
        ])
    );
    assert_eq!(
        response.body["data"]["pageAfterAlpha"]["pageInfo"],
        json!({
            "hasNextPage": false,
            "hasPreviousPage": true,
            "startCursor": "gid://shopify/Product/beta-storefront",
            "endCursor": "gid://shopify/Product/gamma-storefront"
        })
    );
    assert_eq!(
        response.body["data"]["priceSorted"]["nodes"],
        json!([
            {
                "handle": "beta-jacket",
                "priceRange": { "minVariantPrice": { "amount": "10.0" } }
            },
            {
                "handle": "gamma-shirt",
                "priceRange": { "minVariantPrice": { "amount": "20.0" } }
            },
            {
                "handle": "alpha-jacket",
                "priceRange": { "minVariantPrice": { "amount": "30.0" } }
            }
        ])
    );
    assert_eq!(
        response.body["data"]["byId"],
        json!({
            "aliasTitle": "Alpha Jacket",
            "handle": "alpha-jacket"
        })
    );
}

#[test]
fn storefront_cart_lifecycle_stages_locally_with_aliases_fragments_and_state_round_trip() {
    let mut proxy = configured_proxy(ReadMode::Snapshot, Some(UnsupportedMutationMode::Reject))
        .with_upstream_transport(|_| {
            panic!("supported Storefront carts must never write upstream")
        });
    restore_state_with(&mut proxy, |state| {
        state["baseState"]["shop"] = json!({ "currencyCode": "CAD" });
    });
    let (_, variant_id, _) = stage_storefront_cart_variant(&mut proxy, 5);

    let create = proxy.process_request(storefront_graphql_request(
        r#"
        mutation CreateLocalCart($input: CartInput) {
          created: cartCreate(input: $input) {
            cart { ...CartSummary }
            userErrors { field message code }
            warnings { code message target }
          }
        }
        fragment CartSummary on Cart {
          id createdAt updatedAt checkoutUrl totalQuantity note
          attributes { key value }
          lines(first: 10) {
            nodes {
              id quantity attributes { key value }
              merchandise { ... on ProductVariant { id title price { amount currencyCode } } }
              cost { amountPerQuantity { amount currencyCode } subtotalAmount { amount currencyCode } totalAmount { amount currencyCode } }
            }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
          cost { subtotalAmount { amount currencyCode } totalAmount { amount currencyCode } totalTaxAmount { amount currencyCode } }
        }
        "#,
        json!({
            "input": {
                "attributes": [{ "key": "channel", "value": "runtime" }],
                "note": "Initial note",
                "lines": [{
                    "merchandiseId": variant_id,
                    "quantity": 2,
                    "attributes": [{ "key": "engraving", "value": "A" }]
                }]
            }
        }),
    ));
    assert_eq!(create.status, 200, "{}", create.body);
    assert_eq!(
        create.body["data"]["created"]["userErrors"],
        json!([]),
        "{}",
        create.body,
    );
    assert_eq!(
        create.body["data"]["created"]["warnings"],
        json!([]),
        "{}",
        create.body,
    );
    let cart = &create.body["data"]["created"]["cart"];
    let cart_id = cart["id"].as_str().expect("cart id").to_string();
    assert!(cart_id.starts_with("gid://shopify/Cart/"));
    assert!(cart_id.contains("?key="));
    assert_eq!(cart["totalQuantity"], json!(2));
    assert_eq!(
        cart["cost"]["subtotalAmount"],
        json!({ "amount": "25.0", "currencyCode": "CAD" })
    );
    assert_eq!(
        cart["lines"]["nodes"][0]["merchandise"]["id"],
        json!(variant_id)
    );
    let line_id = cart["lines"]["nodes"][0]["id"]
        .as_str()
        .expect("cart line id")
        .to_string();

    let merge = proxy.process_request(storefront_graphql_request(
        r#"
        mutation MergeLocalCartLine($cartId: ID!, $lines: [CartLineInput!]!) {
          cartLinesAdd(cartId: $cartId, lines: $lines) {
            cart { totalQuantity lines(first: 10) { nodes { id quantity attributes { key value } } } }
            userErrors { field message code }
            warnings { code message target }
          }
        }
        "#,
        json!({
            "cartId": cart_id,
            "lines": [{
                "merchandiseId": variant_id,
                "quantity": 1,
                "attributes": [{ "key": "engraving", "value": "A" }]
            }]
        }),
    ));
    assert_eq!(
        merge.body["data"]["cartLinesAdd"]["cart"]["totalQuantity"],
        json!(3)
    );
    assert_eq!(
        merge.body["data"]["cartLinesAdd"]["cart"]["lines"]["nodes"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        merge.body["data"]["cartLinesAdd"]["cart"]["lines"]["nodes"][0]["id"],
        json!(line_id)
    );

    let attributes = proxy.process_request(storefront_graphql_request(
        r#"
        mutation ReplaceLocalCartAttributes($cartId: ID!, $attributes: [AttributeInput!]!) {
          cartAttributesUpdate(cartId: $cartId, attributes: $attributes) {
            cart { id attributes { key value } }
            userErrors { field message code }
            warnings { code message target }
          }
        }
        "#,
        json!({
            "cartId": cart_id,
            "attributes": [
                { "key": "gift", "value": "yes" },
                { "key": "channel", "value": "updated" },
                { "key": "gift", "value": "no" }
            ]
        }),
    ));
    assert_eq!(
        attributes.body["data"]["cartAttributesUpdate"]["cart"]["attributes"],
        json!([{ "key": "gift", "value": "no" }, { "key": "channel", "value": "updated" }])
    );

    let note = proxy.process_request(storefront_graphql_request(
        r#"
        mutation UpdateLocalCartNote($cartId: ID!, $note: String!) {
          cartNoteUpdate(cartId: $cartId, note: $note) {
            cart { id note }
            userErrors { field message code }
            warnings { code message target }
          }
        }
        "#,
        json!({ "cartId": cart_id, "note": "Updated note" }),
    ));
    assert_eq!(
        note.body["data"]["cartNoteUpdate"]["cart"]["note"],
        json!("Updated note")
    );

    let read = proxy.process_request(storefront_graphql_request(
        r#"
        query ReadLocalCart($id: ID!) {
          current: cart(id: $id) { ...CartRead }
        }
        fragment CartRead on Cart {
          id totalQuantity note attributes { key value }
          lines(first: 10) { nodes { id quantity } }
        }
        "#,
        json!({ "id": cart_id }),
    ));
    assert_eq!(read.body["data"]["current"]["totalQuantity"], json!(3));
    assert_eq!(read.body["data"]["current"]["note"], json!("Updated note"));

    let log = proxy.process_request(request_with_body("GET", "/__meta/log", ""));
    let cart_log = log.body["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["interpreted"]["primaryRootField"] == json!("cartCreate"))
        .expect("cart create log entry");
    assert_eq!(cart_log["status"], json!("handled"));
    assert_eq!(
        cart_log["interpreted"]["capability"]["execution"],
        json!("stage-locally")
    );
    assert!(!cart_log.to_string().contains(cart_id.as_str()));
    assert_eq!(cart_log["query"], json!("<redacted:storefront-cart-query>"));

    let dump = proxy.process_request(request_with_body("POST", "/__meta/dump", ""));
    assert_eq!(dump.status, 200);
    assert!(!dump.body.to_string().contains(cart_id.as_str()));
    assert_eq!(
        dump.body["state"]["stagedState"]["storefrontCarts"]
            .as_object()
            .unwrap()
            .len(),
        1
    );

    let mut restored = configured_proxy(ReadMode::Snapshot, Some(UnsupportedMutationMode::Reject))
        .with_upstream_transport(|_| panic!("restored Storefront carts must stay local"));
    let restore = restored.process_request(request_with_body(
        "POST",
        "/__meta/restore",
        &dump.body.to_string(),
    ));
    assert_eq!(restore.status, 200);
    let restored_read = restored.process_request(storefront_graphql_request(
        r#"query ReadRestoredCart($id: ID!) { cart(id: $id) { id totalQuantity note } }"#,
        json!({ "id": cart_id }),
    ));
    assert_eq!(
        restored_read.body["data"]["cart"]["totalQuantity"],
        json!(3)
    );
    assert_eq!(
        restored_read.body["data"]["cart"]["note"],
        json!("Updated note")
    );

    assert_eq!(
        restored
            .process_request(request_with_body("POST", "/__meta/reset", ""))
            .status,
        200
    );
    let after_reset = restored.process_request(storefront_graphql_request(
        r#"query ReadResetCart($id: ID!) { cart(id: $id) { id } }"#,
        json!({ "id": cart_id }),
    ));
    assert_eq!(after_reset.body["data"]["cart"], Value::Null);
}

#[test]
fn storefront_money_projection_uses_observed_currency_and_nulls_without_evidence() {
    let mut eur_proxy = configured_proxy(ReadMode::Snapshot, Some(UnsupportedMutationMode::Reject))
        .with_upstream_transport(|_| panic!("snapshot EUR Storefront reads must stay local"));
    restore_state_with(&mut eur_proxy, |state| {
        state["baseState"]["shop"] = json!({ "currencyCode": "EUR" });
    });
    let (eur_product_id, eur_variant_id, _) = stage_storefront_cart_variant(&mut eur_proxy, 5);

    let eur_product = eur_proxy.process_request(storefront_graphql_request(
        r#"
        query StorefrontEurProduct($id: ID!) {
          product(id: $id) {
            priceRange { minVariantPrice { amount currencyCode } }
            variants(first: 1) { nodes { price { amount currencyCode } } }
          }
        }
        "#,
        json!({ "id": eur_product_id }),
    ));
    assert_eq!(eur_product.status, 200, "{}", eur_product.body);
    assert_eq!(
        eur_product.body["data"]["product"]["priceRange"]["minVariantPrice"]["currencyCode"],
        json!("EUR"),
        "{}",
        eur_product.body,
    );
    assert_eq!(
        eur_product.body["data"]["product"]["variants"]["nodes"][0]["price"]["currencyCode"],
        json!("EUR"),
        "{}",
        eur_product.body,
    );

    let eur_cart = eur_proxy.process_request(storefront_graphql_request(
        r#"
        mutation StorefrontEurCart($input: CartInput) {
          cartCreate(input: $input) {
            cart {
              cost { subtotalAmount { amount currencyCode } totalAmount { amount currencyCode } }
              lines(first: 1) {
                nodes {
                  merchandise { ... on ProductVariant { price { amount currencyCode } } }
                  cost { amountPerQuantity { amount currencyCode } totalAmount { amount currencyCode } }
                }
              }
            }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "input": {
                "lines": [{ "merchandiseId": eur_variant_id, "quantity": 2 }]
            }
        }),
    ));
    assert_eq!(eur_cart.status, 200, "{}", eur_cart.body);
    let eur_cart_value = &eur_cart.body["data"]["cartCreate"]["cart"];
    for currency_path in [
        &eur_cart_value["cost"]["subtotalAmount"]["currencyCode"],
        &eur_cart_value["cost"]["totalAmount"]["currencyCode"],
        &eur_cart_value["lines"]["nodes"][0]["merchandise"]["price"]["currencyCode"],
        &eur_cart_value["lines"]["nodes"][0]["cost"]["amountPerQuantity"]["currencyCode"],
        &eur_cart_value["lines"]["nodes"][0]["cost"]["totalAmount"]["currencyCode"],
    ] {
        assert_eq!(currency_path, &json!("EUR"), "{}", eur_cart.body);
    }

    let mut cart_state_proxy =
        configured_proxy(ReadMode::Snapshot, Some(UnsupportedMutationMode::Reject))
            .with_upstream_transport(|_| {
                panic!("snapshot cart-state currency reads must stay local")
            });
    let (_, currency_unknown_variant_id, _) =
        stage_storefront_cart_variant(&mut cart_state_proxy, 5);
    let (_, cad_variant_id, _) = stage_storefront_cart_variant(&mut cart_state_proxy, 5);
    restore_state_with(&mut cart_state_proxy, |state| {
        state["stagedState"]["productVariants"][&cad_variant_id]["currencyCode"] = json!("CAD");
    });
    let cart_state_currency = cart_state_proxy.process_request(storefront_graphql_request(
        r#"
        mutation StorefrontCartStateCurrency($input: CartInput) {
          cartCreate(input: $input) {
            cart {
              cost { totalAmount { currencyCode } }
              lines(first: 2) {
                nodes {
                  merchandise { ... on ProductVariant { price { currencyCode } } }
                  cost { totalAmount { currencyCode } }
                }
              }
            }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "input": {
                "lines": [
                    { "merchandiseId": currency_unknown_variant_id, "quantity": 1 },
                    { "merchandiseId": cad_variant_id, "quantity": 1 }
                ]
            }
        }),
    ));
    assert_eq!(
        cart_state_currency.status, 200,
        "{}",
        cart_state_currency.body
    );
    let cart_state_value = &cart_state_currency.body["data"]["cartCreate"]["cart"];
    for currency_path in [
        &cart_state_value["cost"]["totalAmount"]["currencyCode"],
        &cart_state_value["lines"]["nodes"][0]["merchandise"]["price"]["currencyCode"],
        &cart_state_value["lines"]["nodes"][0]["cost"]["totalAmount"]["currencyCode"],
        &cart_state_value["lines"]["nodes"][1]["merchandise"]["price"]["currencyCode"],
        &cart_state_value["lines"]["nodes"][1]["cost"]["totalAmount"]["currencyCode"],
    ] {
        assert_eq!(currency_path, &json!("CAD"), "{}", cart_state_currency.body);
    }

    let market_id = "gid://shopify/Market/context-eur";
    let catalog_id = "gid://shopify/MarketCatalog/context-eur";
    let price_list_id = "gid://shopify/PriceList/context-eur";
    let mut contextual_proxy =
        configured_proxy(ReadMode::Snapshot, Some(UnsupportedMutationMode::Reject))
            .with_upstream_transport(|_| {
                panic!("snapshot contextual Storefront reads must stay local")
            });
    restore_state_with(&mut contextual_proxy, |state| {
        state["baseState"]["shop"] = json!({ "currencyCode": "CAD" });
        state["baseState"]["storefrontLocalizations"] = json!({
            "country=*;language=*": {
                "country": { "isoCode": "CA", "currency": { "isoCode": "CAD" } },
                "language": { "isoCode": "EN" },
                "market": { "id": "gid://shopify/Market/primary", "handle": "primary" }
            }
        });
        state["stagedState"]["markets"] = json!({
            (market_id): {
                "id": market_id,
                "handle": "europe",
                "status": "ACTIVE",
                "conditions": {
                    "regionsCondition": {
                        "regions": { "nodes": [{ "code": "DE" }] }
                    }
                },
                "currencySettings": {
                    "baseCurrency": { "currencyCode": "EUR" }
                }
            }
        });
        state["stagedState"]["catalogs"] = json!({
            (catalog_id): {
                "id": catalog_id,
                "status": "ACTIVE",
                "marketIds": [market_id],
                "priceListId": price_list_id
            }
        });
        state["stagedState"]["priceLists"] = json!({
            (price_list_id): {
                "id": price_list_id,
                "currency": "EUR",
                "prices": { "edges": [] }
            }
        });
    });
    let (contextual_product_id, contextual_variant_id, _) =
        stage_storefront_cart_variant(&mut contextual_proxy, 5);

    let contextual_product = contextual_proxy.process_request(storefront_graphql_request(
        r#"
        query StorefrontContextualProduct($id: ID!) @inContext(country: DE) {
          product(id: $id) {
            priceRange { minVariantPrice { amount currencyCode } }
            variants(first: 1) { nodes { price { amount currencyCode } } }
          }
        }
        "#,
        json!({ "id": contextual_product_id }),
    ));
    assert_eq!(
        contextual_product.status, 200,
        "{}",
        contextual_product.body
    );
    assert_eq!(
        contextual_product.body["data"]["product"]["priceRange"]["minVariantPrice"]["currencyCode"],
        json!("EUR"),
        "{}",
        contextual_product.body,
    );
    assert_eq!(
        contextual_product.body["data"]["product"]["variants"]["nodes"][0]["price"]["currencyCode"],
        json!("EUR"),
        "{}",
        contextual_product.body,
    );

    let contextual_cart = contextual_proxy.process_request(storefront_graphql_request(
        r#"
        mutation StorefrontContextualCart($input: CartInput) {
          cartCreate(input: $input) {
            cart {
              cost { subtotalAmount { amount currencyCode } totalAmount { amount currencyCode } }
              lines(first: 1) {
                nodes { cost { amountPerQuantity { amount currencyCode } totalAmount { amount currencyCode } } }
              }
            }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "input": {
                "buyerIdentity": { "countryCode": "DE" },
                "lines": [{ "merchandiseId": contextual_variant_id, "quantity": 1 }]
            }
        }),
    ));
    assert_eq!(contextual_cart.status, 200, "{}", contextual_cart.body);
    let contextual_cart_value = &contextual_cart.body["data"]["cartCreate"]["cart"];
    for currency_path in [
        &contextual_cart_value["cost"]["subtotalAmount"]["currencyCode"],
        &contextual_cart_value["cost"]["totalAmount"]["currencyCode"],
        &contextual_cart_value["lines"]["nodes"][0]["cost"]["amountPerQuantity"]["currencyCode"],
        &contextual_cart_value["lines"]["nodes"][0]["cost"]["totalAmount"]["currencyCode"],
    ] {
        assert_eq!(currency_path, &json!("EUR"), "{}", contextual_cart.body);
    }

    let mut unavailable_proxy =
        configured_proxy(ReadMode::Snapshot, Some(UnsupportedMutationMode::Reject))
            .with_upstream_transport(|_| {
                panic!("snapshot unavailable Storefront reads must stay local")
            });
    let (unavailable_product_id, unavailable_variant_id, _) =
        stage_storefront_cart_variant(&mut unavailable_proxy, 5);
    let unavailable_product = unavailable_proxy.process_request(storefront_graphql_request(
        r#"
        query StorefrontUnavailableCurrencyProduct($id: ID!) {
          product(id: $id) { priceRange { minVariantPrice { amount currencyCode } } }
        }
        "#,
        json!({ "id": unavailable_product_id }),
    ));
    assert_eq!(
        unavailable_product.status, 200,
        "{}",
        unavailable_product.body
    );
    assert!(!unavailable_product.body.to_string().contains("USD"));
    assert_eq!(unavailable_product.body["data"], Value::Null);
    assert_eq!(
        unavailable_product.body["errors"][0]["message"],
        json!("Storefront snapshot has no value for non-null root `QueryRoot.product`")
    );
    assert_eq!(
        unavailable_product.body["errors"][0]["path"],
        json!(["product", "priceRange", "minVariantPrice"])
    );

    let unavailable_cart = unavailable_proxy.process_request(storefront_graphql_request(
        r#"
        mutation StorefrontUnavailableCurrencyCart($input: CartInput) {
          cartCreate(input: $input) {
            cart { cost { totalAmount { amount currencyCode } } }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "input": {
                "lines": [{ "merchandiseId": unavailable_variant_id, "quantity": 1 }]
            }
        }),
    ));
    assert_eq!(unavailable_cart.status, 200, "{}", unavailable_cart.body);
    assert!(!unavailable_cart.body.to_string().contains("USD"));
    assert_eq!(unavailable_cart.body["data"], Value::Null);
    assert_eq!(
        unavailable_cart.body["errors"][0]["message"],
        json!("Storefront snapshot has no value for non-null root `QueryRoot.cartCreate`")
    );
    assert_eq!(
        unavailable_cart.body["errors"][0]["path"],
        json!(["cartCreate", "cart", "cost", "totalAmount"])
    );
}

#[test]
fn storefront_cart_validations_warnings_limits_and_stale_branches_match_capture() {
    let mut proxy = configured_proxy(ReadMode::Snapshot, Some(UnsupportedMutationMode::Reject))
        .with_upstream_transport(|_| panic!("supported Storefront cart branches must stay local"));
    let (_, variant_id, _) = stage_storefront_cart_variant(&mut proxy, 5);
    let create = proxy.process_request(storefront_graphql_request(
        r#"
        mutation CreateValidationCart($input: CartInput) {
          cartCreate(input: $input) {
            cart { id lines(first: 10) { nodes { id quantity } } }
            userErrors { field message code }
            warnings { code message target }
          }
        }
        "#,
        json!({ "input": { "lines": [{ "merchandiseId": variant_id, "quantity": 2 }] } }),
    ));
    let cart_id = create.body["data"]["cartCreate"]["cart"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let line_id = create.body["data"]["cartCreate"]["cart"]["lines"]["nodes"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let capped = proxy.process_request(storefront_graphql_request(
        r#"
        mutation CapCartQuantity($cartId: ID!, $lines: [CartLineUpdateInput!]!) {
          cartLinesUpdate(cartId: $cartId, lines: $lines) {
            cart { totalQuantity lines(first: 10) { nodes { id quantity } } }
            userErrors { field message code }
            warnings { code message target }
          }
        }
        "#,
        json!({ "cartId": cart_id, "lines": [{ "id": line_id, "quantity": 10 }] }),
    ));
    assert_eq!(
        capped.body["data"]["cartLinesUpdate"]["cart"]["totalQuantity"],
        json!(5)
    );
    assert_eq!(
        capped.body["data"]["cartLinesUpdate"]["warnings"][0]["code"],
        json!("MERCHANDISE_NOT_ENOUGH_STOCK")
    );
    assert_eq!(
        capped.body["data"]["cartLinesUpdate"]["warnings"][0]["target"],
        json!(line_id)
    );

    let zero = proxy.process_request(storefront_graphql_request(
        r#"
        mutation IgnoreZeroQuantity($cartId: ID!, $lines: [CartLineInput!]!) {
          cartLinesAdd(cartId: $cartId, lines: $lines) {
            cart { totalQuantity }
            userErrors { field message code }
            warnings { code message target }
          }
        }
        "#,
        json!({ "cartId": cart_id, "lines": [{ "merchandiseId": variant_id, "quantity": 0 }] }),
    ));
    assert_eq!(
        zero.body["data"]["cartLinesAdd"]["cart"]["totalQuantity"],
        json!(5)
    );
    assert_eq!(zero.body["data"]["cartLinesAdd"]["userErrors"], json!([]));

    for (variables, expected_field, expected_code) in [
        (
            json!({ "cartId": cart_id, "lines": [{ "merchandiseId": "gid://shopify/ProductVariant/0", "quantity": 1 }] }),
            json!(["lines", "0", "merchandiseId"]),
            "INVALID",
        ),
        (
            json!({ "cartId": cart_id, "lines": [{ "merchandiseId": variant_id, "sellingPlanId": "gid://shopify/SellingPlan/0", "quantity": 1 }] }),
            json!(["lines", "0", "sellingPlanId"]),
            "SELLING_PLAN_NOT_APPLICABLE",
        ),
    ] {
        let response = proxy.process_request(storefront_graphql_request(
            r#"
            mutation InvalidCartLine($cartId: ID!, $lines: [CartLineInput!]!) {
              cartLinesAdd(cartId: $cartId, lines: $lines) {
                cart { id }
                userErrors { field message code }
                warnings { code message target }
              }
            }
            "#,
            variables,
        ));
        assert_eq!(response.body["data"]["cartLinesAdd"]["cart"], Value::Null);
        assert_eq!(
            response.body["data"]["cartLinesAdd"]["userErrors"][0]["field"],
            expected_field
        );
        assert_eq!(
            response.body["data"]["cartLinesAdd"]["userErrors"][0]["code"],
            json!(expected_code)
        );
    }

    let remove = proxy.process_request(storefront_graphql_request(
        r#"
        mutation RemoveValidationLine($cartId: ID!, $lineIds: [ID!]!) {
          cartLinesRemove(cartId: $cartId, lineIds: $lineIds) {
            cart { totalQuantity }
            userErrors { field message code }
            warnings { code message target }
          }
        }
        "#,
        json!({ "cartId": cart_id, "lineIds": [line_id] }),
    ));
    assert_eq!(
        remove.body["data"]["cartLinesRemove"]["cart"]["totalQuantity"],
        json!(0)
    );
    let stale = proxy.process_request(storefront_graphql_request(
        r#"
        mutation UpdateStaleLine($cartId: ID!, $lines: [CartLineUpdateInput!]!) {
          cartLinesUpdate(cartId: $cartId, lines: $lines) {
            cart { totalQuantity }
            userErrors { field message code }
            warnings { code message target }
          }
        }
        "#,
        json!({ "cartId": cart_id, "lines": [{ "id": line_id, "quantity": 1 }] }),
    ));
    assert_eq!(
        stale.body["data"]["cartLinesUpdate"]["userErrors"][0]["field"],
        json!(["lines", "0", "id"])
    );
    assert_eq!(
        stale.body["data"]["cartLinesUpdate"]["userErrors"][0]["code"],
        json!("INVALID_MERCHANDISE_LINE")
    );

    let note = proxy.process_request(storefront_graphql_request(
        r#"
        mutation RejectLongNote($cartId: ID!, $note: String!) {
          cartNoteUpdate(cartId: $cartId, note: $note) {
            cart { id }
            userErrors { field message code }
            warnings { code message target }
          }
        }
        "#,
        json!({ "cartId": cart_id, "note": "n".repeat(5001) }),
    ));
    assert_eq!(note.body["data"]["cartNoteUpdate"]["cart"], Value::Null);
    assert_eq!(
        note.body["data"]["cartNoteUpdate"]["userErrors"][0]["code"],
        json!("NOTE_TOO_LONG")
    );

    let too_many = proxy.process_request(storefront_graphql_request(
        r#"
        mutation RejectTooManyAttributes($cartId: ID!, $attributes: [AttributeInput!]!) {
          cartAttributesUpdate(cartId: $cartId, attributes: $attributes) {
            cart { id }
            userErrors { field message code }
            warnings { code message target }
          }
        }
        "#,
        json!({
            "cartId": cart_id,
            "attributes": (0..251).map(|index| json!({ "key": format!("key-{index}"), "value": "value" })).collect::<Vec<_>>()
        }),
    ));
    assert_eq!(
        too_many.body["errors"][0]["extensions"]["code"],
        json!("MAX_INPUT_SIZE_EXCEEDED")
    );
    assert_eq!(
        too_many.body["errors"][0]["path"],
        json!(["cartAttributesUpdate", "attributes"])
    );

    let missing = proxy.process_request(storefront_graphql_request(
        r#"query MissingCart($id: ID!) { cart(id: $id) { id } }"#,
        json!({ "id": "gid://shopify/Cart/missing?key=missing" }),
    ));
    assert_eq!(missing.body["data"]["cart"], Value::Null);

    let missing_update = proxy.process_request(storefront_graphql_request(
        r#"
        mutation UpdateMissingCartNote($cartId: ID!, $note: String!) {
          cartNoteUpdate(cartId: $cartId, note: $note) {
            cart { id note totalQuantity }
            userErrors { field message code }
            warnings { code message target }
          }
        }
        "#,
        json!({ "cartId": "gid://shopify/Cart/missing?key=missing", "note": "replacement" }),
    ));
    assert_eq!(
        missing_update.body["data"]["cartNoteUpdate"]["userErrors"][0],
        json!({
            "field": ["cartId"],
            "message": "The specified cart does not exist.",
            "code": "INVALID"
        })
    );
    assert_ne!(
        missing_update.body["data"]["cartNoteUpdate"]["cart"]["id"],
        json!("gid://shopify/Cart/missing?key=missing")
    );
    assert_eq!(
        missing_update.body["data"]["cartNoteUpdate"]["cart"]["note"],
        json!("replacement")
    );
}

#[test]
fn storefront_cart_adjustments_reuse_shared_state_and_round_trip_without_upstream_writes() {
    let mut proxy = configured_proxy(ReadMode::Snapshot, Some(UnsupportedMutationMode::Reject))
        .with_clock(|| utc_time(1_704_067_200))
        .with_upstream_transport(|_| {
            panic!("supported Storefront cart adjustments must never write upstream")
        });
    let (_, variant_id, _) = stage_storefront_cart_variant(&mut proxy, 5);
    restore_state_with(&mut proxy, |state| {
        state["baseState"]["shop"] = json!({
            "id": "gid://shopify/Shop/cart-adjustments",
            "name": "Cart adjustments",
            "currencyCode": "CAD"
        });
    });

    let create_discount = |proxy: &mut DraftProxy,
                           title: &str,
                           code: &str,
                           starts_at: &str,
                           ends_at: Option<&str>,
                           minimum: &str| {
        let mut input = json!({
            "title": title,
            "code": code,
            "startsAt": starts_at,
            "combinesWith": {
                "productDiscounts": false,
                "orderDiscounts": true,
                "shippingDiscounts": false
            },
            "context": { "all": "ALL" },
            "minimumRequirement": {
                "subtotal": { "greaterThanOrEqualToSubtotal": minimum }
            },
            "customerGets": {
                "value": { "percentage": 0.2 },
                "items": { "all": true }
            }
        });
        if let Some(ends_at) = ends_at {
            input["endsAt"] = json!(ends_at);
        }
        let response = proxy.process_request(json_graphql_request(
            include_str!(
                "../../config/parity-requests/storefront/storefront-cart-discount-create-admin.graphql"
            ),
            json!({ "input": input }),
        ));
        assert_eq!(
            response.body["data"]["discountCodeBasicCreate"]["userErrors"],
            json!([]),
            "{}",
            response.body
        );
    };
    create_discount(
        &mut proxy,
        "Active cart discount",
        "CARTACTIVE",
        "2023-12-01T00:00:00Z",
        None,
        "1.00",
    );
    create_discount(
        &mut proxy,
        "Expired cart discount",
        "CARTEXPIRED",
        "2023-01-01T00:00:00Z",
        Some("2023-06-01T00:00:00Z"),
        "1.00",
    );
    create_discount(
        &mut proxy,
        "Inapplicable cart discount",
        "CARTINAPPLICABLE",
        "2023-12-01T00:00:00Z",
        None,
        "1000.00",
    );

    let gift_card = proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/storefront/storefront-cart-gift-card-create-admin.graphql"
        ),
        json!({
            "input": {
                "initialValue": "40.00",
                "code": "cartgiftcard123",
                "note": "Cart adjustment test"
            }
        }),
    ));
    assert_eq!(
        gift_card.body["data"]["giftCardCreate"]["userErrors"],
        json!([]),
        "{}",
        gift_card.body
    );

    let customer = proxy.process_request(storefront_graphql_request(
        include_str!(
            "../../config/parity-requests/storefront/storefront-customer-auth-create.graphql"
        ),
        json!({
            "input": {
                "email": "cart-buyer@example.com",
                "password": "CartBuyer123!",
                "firstName": "Cart",
                "lastName": "Buyer"
            }
        }),
    ));
    let customer_id = customer.body["data"]["customerCreate"]["customer"]["id"]
        .as_str()
        .expect("customer id")
        .to_string();
    let token = proxy.process_request(storefront_graphql_request(
        include_str!(
            "../../config/parity-requests/storefront/storefront-customer-auth-token-create.graphql"
        ),
        json!({
            "input": { "email": "cart-buyer@example.com", "password": "CartBuyer123!" }
        }),
    ));
    let customer_access_token = token.body["data"]["customerAccessTokenCreate"]
        ["customerAccessToken"]["accessToken"]
        .as_str()
        .expect("customer access token")
        .to_string();

    let create = proxy.process_request(storefront_graphql_request(
        include_str!("../../config/parity-requests/storefront/storefront-cart-create.graphql"),
        json!({
            "input": {
                "lines": [{ "merchandiseId": variant_id, "quantity": 2 }]
            }
        }),
    ));
    assert_eq!(create.status, 200, "{}", create.body);
    let cart_id = create.body["data"]["cartCreate"]["cart"]["id"]
        .as_str()
        .expect("cart id")
        .to_string();

    let buyer = proxy.process_request(storefront_graphql_request(
        include_str!(
            "../../config/parity-requests/storefront/storefront-cart-buyer-identity-update.graphql"
        ),
        json!({
            "cartId": cart_id,
            "buyerIdentity": {
                "countryCode": "CA",
                "email": "cart-buyer@example.com",
                "phone": "+12025550123",
                "customerAccessToken": customer_access_token
            }
        }),
    ));
    assert_eq!(
        buyer.body["data"]["cartBuyerIdentityUpdate"]["userErrors"],
        json!([]),
        "{}",
        buyer.body
    );
    assert_eq!(
        buyer.body["data"]["cartBuyerIdentityUpdate"]["cart"]["buyerIdentity"]["customer"]["id"],
        json!(customer_id)
    );
    let invalid_buyer = proxy.process_request(storefront_graphql_request(
        include_str!(
            "../../config/parity-requests/storefront/storefront-cart-buyer-identity-update.graphql"
        ),
        json!({
            "cartId": cart_id,
            "buyerIdentity": { "countryCode": "CA", "customerAccessToken": "invalid-token" }
        }),
    ));
    assert_eq!(
        invalid_buyer.body["data"]["cartBuyerIdentityUpdate"]["userErrors"][0],
        json!({
            "field": ["buyerIdentity", "customerAccessToken"],
            "message": "Customer is invalid",
            "code": "INVALID"
        })
    );
    assert_eq!(
        invalid_buyer.body["data"]["cartBuyerIdentityUpdate"]["cart"]["buyerIdentity"]["customer"]
            ["id"],
        json!(customer_id)
    );

    let discounts = proxy.process_request(storefront_graphql_request(
        include_str!(
            "../../config/parity-requests/storefront/storefront-cart-discount-codes-update.graphql"
        ),
        json!({
            "cartId": cart_id,
            "discountCodes": [
                "CARTACTIVE",
                "cartactive",
                "CARTEXPIRED",
                "CARTINAPPLICABLE",
                " NOT-A-REAL-CODE "
            ]
        }),
    ));
    let discount_payload = &discounts.body["data"]["cartDiscountCodesUpdate"];
    assert_eq!(
        discount_payload["userErrors"],
        json!([]),
        "{}",
        discounts.body
    );
    assert_eq!(
        discount_payload["cart"]["discountCodes"]
            .as_array()
            .expect("discount codes")
            .len(),
        4
    );
    assert_eq!(
        discount_payload["cart"]["discountCodes"][0]["applicable"],
        json!(true)
    );
    assert_eq!(
        discount_payload["warnings"]
            .as_array()
            .expect("discount warnings")
            .iter()
            .map(|warning| warning["code"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec![
            "DISCOUNT_CURRENTLY_INACTIVE",
            "DISCOUNT_PURCHASE_NOT_IN_RANGE",
            "DISCOUNT_NOT_FOUND"
        ]
    );
    assert_eq!(
        discount_payload["cart"]["cost"]["totalAmount"]["amount"],
        json!("20.0")
    );

    let gift_add = proxy.process_request(storefront_graphql_request(
        include_str!(
            "../../config/parity-requests/storefront/storefront-cart-gift-card-codes-add.graphql"
        ),
        json!({
            "cartId": cart_id,
            "giftCardCodes": ["CARTGIFTCARD123", "NOT-A-REAL-GIFT-CARD"]
        }),
    ));
    let applied_id = gift_add.body["data"]["cartGiftCardCodesAdd"]["cart"]["appliedGiftCards"][0]
        ["id"]
        .as_str()
        .expect("applied gift card id")
        .to_string();
    assert_eq!(
        gift_add.body["data"]["cartGiftCardCodesAdd"]["cart"]["cost"]["totalAmount"]["amount"],
        json!("0.0")
    );
    let gift_remove = proxy.process_request(storefront_graphql_request(
        include_str!(
            "../../config/parity-requests/storefront/storefront-cart-gift-card-codes-remove.graphql"
        ),
        json!({ "cartId": cart_id, "appliedGiftCardIds": [applied_id] }),
    ));
    assert_eq!(
        gift_remove.body["data"]["cartGiftCardCodesRemove"]["cart"]["appliedGiftCards"],
        json!([])
    );
    let gift_update = proxy.process_request(storefront_graphql_request(
        include_str!(
            "../../config/parity-requests/storefront/storefront-cart-gift-card-codes-update.graphql"
        ),
        json!({
            "cartId": cart_id,
            "giftCardCodes": ["cartgiftcard123", "CARTGIFTCARD123"]
        }),
    ));
    assert_eq!(
        gift_update.body["data"]["cartGiftCardCodesUpdate"]["cart"]["appliedGiftCards"]
            .as_array()
            .expect("applied gift cards")
            .len(),
        1
    );

    let metafields = proxy.process_request(storefront_graphql_request(
        include_str!(
            "../../config/parity-requests/storefront/storefront-cart-metafields-set.graphql"
        ),
        json!({
            "metafields": [
                {
                    "ownerId": cart_id,
                    "key": "custom.note",
                    "type": "single_line_text_field",
                    "value": "Cart note value"
                },
                {
                    "ownerId": cart_id,
                    "key": "custom.count",
                    "type": "number_integer",
                    "value": "2"
                }
            ]
        }),
    ));
    assert_eq!(
        metafields.body["data"]["cartMetafieldsSet"]["userErrors"],
        json!([]),
        "{}",
        metafields.body
    );
    assert_eq!(
        metafields.body["data"]["cartMetafieldsSet"]["metafields"]
            .as_array()
            .expect("cart metafields")
            .len(),
        2
    );
    let invalid_metafield = proxy.process_request(storefront_graphql_request(
        include_str!(
            "../../config/parity-requests/storefront/storefront-cart-metafields-set.graphql"
        ),
        json!({
            "metafields": [{
                "ownerId": cart_id,
                "key": "custom.count",
                "type": "number_integer",
                "value": "not-a-number"
            }]
        }),
    ));
    assert_eq!(
        invalid_metafield.body["data"]["cartMetafieldsSet"]["userErrors"][0],
        json!({
            "field": ["metafields", "0", "value"],
            "message": "Value must be an integer.",
            "code": "INVALID_VALUE",
            "elementIndex": null
        })
    );
    let deleted = proxy.process_request(storefront_graphql_request(
        include_str!(
            "../../config/parity-requests/storefront/storefront-cart-metafield-delete.graphql"
        ),
        json!({ "input": { "ownerId": cart_id, "key": "custom.note" } }),
    ));
    assert!(deleted.body["data"]["cartMetafieldDelete"]["deletedId"].is_string());

    let dump = proxy.process_request(request_with_body("POST", "/__meta/dump", ""));
    assert_eq!(dump.status, 200);
    let mut restored = configured_proxy(ReadMode::Snapshot, Some(UnsupportedMutationMode::Reject))
        .with_clock(|| utc_time(1_704_067_200))
        .with_upstream_transport(|_| panic!("restored cart adjustments must stay local"));
    assert_eq!(
        restored
            .process_request(request_with_body(
                "POST",
                "/__meta/restore",
                &dump.body.to_string()
            ))
            .status,
        200
    );
    let restored_read = restored.process_request(storefront_graphql_request(
        include_str!("../../config/parity-requests/storefront/storefront-cart-read.graphql"),
        json!({ "id": cart_id }),
    ));
    assert_eq!(
        restored_read.body["data"]["cart"]["buyerIdentity"]["customer"]["id"],
        json!(customer_id)
    );
    assert_eq!(restored_read.body["data"]["cart"]["metafield"], Value::Null);
    assert_eq!(
        restored_read.body["data"]["cart"]["metafields"]
            .as_array()
            .expect("restored metafields")
            .len(),
        1
    );
    assert_eq!(
        restored
            .process_request(request_with_body("POST", "/__meta/reset", ""))
            .status,
        200
    );
    let after_reset = restored.process_request(storefront_graphql_request(
        r#"query ReadResetAdjustedCart($id: ID!) { cart(id: $id) { id } }"#,
        json!({ "id": cart_id }),
    ));
    assert_eq!(after_reset.body["data"]["cart"], Value::Null);
}

#[test]
fn storefront_cart_state_is_isolated_between_proxy_instances() {
    let mut first = configured_proxy(ReadMode::Snapshot, Some(UnsupportedMutationMode::Reject))
        .with_upstream_transport(|_| panic!("first cart proxy must stay local"));
    let mut second = configured_proxy(ReadMode::Snapshot, Some(UnsupportedMutationMode::Reject))
        .with_upstream_transport(|_| panic!("second cart proxy must stay local"));
    let create_query = r#"
      mutation CreateIsolatedCart($input: CartInput) {
        cartCreate(input: $input) {
          cart { id note }
          userErrors { field message code }
          warnings { code message target }
        }
      }
    "#;
    let first_create = first.process_request(storefront_graphql_request(
        create_query,
        json!({ "input": { "note": "first" } }),
    ));
    let second_create = second.process_request(storefront_graphql_request(
        create_query,
        json!({ "input": { "note": "second" } }),
    ));
    let first_id = first_create.body["data"]["cartCreate"]["cart"]["id"]
        .as_str()
        .unwrap();
    let second_id = second_create.body["data"]["cartCreate"]["cart"]["id"]
        .as_str()
        .unwrap();
    assert_eq!(
        first_id, second_id,
        "synthetic IDs should be deterministic per instance"
    );

    let read_query = r#"query ReadIsolatedCart($id: ID!) { cart(id: $id) { id note } }"#;
    let first_read = first.process_request(storefront_graphql_request(
        read_query,
        json!({ "id": first_id }),
    ));
    let second_read = second.process_request(storefront_graphql_request(
        read_query,
        json!({ "id": second_id }),
    ));
    assert_eq!(first_read.body["data"]["cart"]["note"], json!("first"));
    assert_eq!(second_read.body["data"]["cart"]["note"], json!("second"));
}

#[test]
fn storefront_cart_delivery_lifecycle_stages_rates_selection_and_state_round_trip() {
    let mut proxy = configured_proxy(ReadMode::Snapshot, Some(UnsupportedMutationMode::Reject))
        .with_upstream_transport(|_| panic!("Storefront cart delivery must stay local"));
    restore_state_with(&mut proxy, |state| {
        state["baseState"]["shop"] = json!({ "currencyCode": "USD" });
    });
    let (_, variant_id, location_id) = stage_storefront_cart_variant(&mut proxy, 5);
    stage_storefront_cart_delivery_profile(&mut proxy, &variant_id, &location_id);

    let create = proxy.process_request(storefront_graphql_request(
        r#"
        mutation CreateDeliveryCart($input: CartInput) {
          cartCreate(input: $input) {
            cart { id checkoutUrl totalQuantity }
            userErrors { field message code }
            warnings { code message target }
          }
        }
        "#,
        json!({
            "input": {
                "buyerIdentity": { "countryCode": "US" },
                "lines": [{ "merchandiseId": variant_id, "quantity": 2 }]
            }
        }),
    ));
    assert_eq!(create.status, 200, "{}", create.body);
    let cart_id = create.body["data"]["cartCreate"]["cart"]["id"]
        .as_str()
        .expect("delivery cart id")
        .to_string();
    let checkout_url = create.body["data"]["cartCreate"]["cart"]["checkoutUrl"]
        .as_str()
        .expect("checkout url")
        .to_string();
    assert!(checkout_url.starts_with("https://shopify.com/cart/c/"));
    assert!(checkout_url.contains("?key="));

    let add = proxy.process_request(storefront_graphql_request(
        include_str!(
            "../../config/parity-requests/storefront/storefront-cart-delivery-addresses-add.graphql"
        ),
        json!({
            "cartId": cart_id,
            "addresses": [
                {
                    "address": { "deliveryAddress": {
                        "firstName": "Conformance",
                        "lastName": "Buyer",
                        "address1": "123 Example Street",
                        "city": "New York",
                        "provinceCode": "NY",
                        "countryCode": "US",
                        "zip": "10001"
                    } },
                    "selected": true,
                    "oneTimeUse": false,
                    "validationStrategy": "COUNTRY_CODE_ONLY"
                },
                {
                    "address": { "deliveryAddress": {
                        "firstName": "Conformance",
                        "lastName": "Buyer",
                        "address1": "456 Example Street",
                        "city": "New York",
                        "provinceCode": "NY",
                        "countryCode": "US",
                        "zip": "10001"
                    } },
                    "selected": false,
                    "oneTimeUse": true,
                    "validationStrategy": "COUNTRY_CODE_ONLY"
                }
            ]
        }),
    ));
    assert_eq!(add.status, 200, "{}", add.body);
    let payload = &add.body["data"]["cartDeliveryAddressesAdd"];
    assert_eq!(payload["userErrors"], json!([]));
    assert_eq!(payload["warnings"], json!([]));
    let addresses = payload["cart"]["delivery"]["addresses"]
        .as_array()
        .expect("delivery addresses");
    assert_eq!(addresses.len(), 2);
    assert_eq!(addresses[0]["selected"], json!(true));
    assert_eq!(addresses[0]["oneTimeUse"], json!(false));
    assert_eq!(addresses[1]["selected"], json!(false));
    assert_eq!(addresses[1]["oneTimeUse"], json!(true));
    let first_address_id = addresses[0]["id"].as_str().unwrap().to_string();
    let second_address_id = addresses[1]["id"].as_str().unwrap().to_string();

    let groups = &payload["cart"]["deliveryGroups"];
    assert_eq!(groups["nodes"].as_array().unwrap().len(), 1);
    assert_eq!(groups["edges"].as_array().unwrap().len(), 1);
    assert_eq!(groups["nodes"][0]["groupType"], json!("ONE_TIME_PURCHASE"));
    assert_eq!(
        groups["nodes"][0]["deliveryOptions"][0]["code"],
        json!("Conformance Standard")
    );
    assert_eq!(
        groups["nodes"][0]["deliveryOptions"][0]["estimatedCost"],
        json!({
            "amount": "7.25",
            "currencyCode": "USD"
        })
    );
    assert_eq!(
        groups["nodes"][0]["deliveryOptions"][1]["code"],
        json!("Conformance Express")
    );
    assert_eq!(
        groups["nodes"][0]["selectedDeliveryOption"]["code"],
        json!("Conformance Standard")
    );
    let standard_cost = proxy.process_request(storefront_graphql_request(
        r#"query DeliveryCartCost($id: ID!) { cart(id: $id) { cost { totalAmount { amount currencyCode } } } }"#,
        json!({ "id": cart_id }),
    ));
    assert_eq!(
        standard_cost.body["data"]["cart"]["cost"]["totalAmount"]["amount"],
        json!("32.25")
    );
    assert_eq!(
        groups["nodes"][0]["cartLines"]["nodes"][0]["quantity"],
        json!(2)
    );
    let group_id = groups["nodes"][0]["id"].as_str().unwrap().to_string();
    let express_handle = groups["nodes"][0]["deliveryOptions"][1]["handle"]
        .as_str()
        .unwrap()
        .to_string();

    let select = proxy.process_request(storefront_graphql_request(
        include_str!(
            "../../config/parity-requests/storefront/storefront-cart-selected-delivery-options-update.graphql"
        ),
        json!({
            "cartId": cart_id,
            "selectedDeliveryOptions": [{
                "deliveryGroupId": group_id,
                "deliveryOptionHandle": express_handle
            }]
        }),
    ));
    assert_eq!(select.status, 200, "{}", select.body);
    assert_eq!(
        select.body["data"]["cartSelectedDeliveryOptionsUpdate"]["cart"]["deliveryGroups"]["nodes"]
            [0]["selectedDeliveryOption"]["code"],
        json!("Conformance Express")
    );
    let express_cost = proxy.process_request(storefront_graphql_request(
        r#"query SelectedDeliveryCartCost($id: ID!) { cart(id: $id) { cost { totalAmount { amount currencyCode } } } }"#,
        json!({ "id": cart_id }),
    ));
    assert_eq!(
        express_cost.body["data"]["cart"]["cost"]["totalAmount"]["amount"],
        json!("37.0")
    );

    let update = proxy.process_request(storefront_graphql_request(
        include_str!(
            "../../config/parity-requests/storefront/storefront-cart-delivery-addresses-update.graphql"
        ),
        json!({
            "cartId": cart_id,
            "addresses": [{ "id": second_address_id, "selected": true, "oneTimeUse": false }]
        }),
    ));
    let updated_addresses =
        &update.body["data"]["cartDeliveryAddressesUpdate"]["cart"]["delivery"]["addresses"];
    assert_eq!(updated_addresses[0]["id"], json!(first_address_id));
    assert_eq!(updated_addresses[0]["selected"], json!(false));
    assert_eq!(updated_addresses[1]["id"], json!(second_address_id));
    assert_eq!(updated_addresses[1]["selected"], json!(true));
    assert_eq!(updated_addresses[1]["oneTimeUse"], json!(false));
    assert_eq!(
        update.body["data"]["cartDeliveryAddressesUpdate"]["cart"]["deliveryGroups"]["nodes"][0]
            ["selectedDeliveryOption"]["code"],
        json!("Conformance Standard")
    );

    let dump = proxy.process_request(request_with_body("POST", "/__meta/dump", ""));
    assert_eq!(dump.status, 200);
    assert!(!dump.body.to_string().contains(&cart_id));
    let mut restored = configured_proxy(ReadMode::Snapshot, Some(UnsupportedMutationMode::Reject))
        .with_upstream_transport(|_| panic!("restored cart delivery must stay local"));
    assert_eq!(
        restored
            .process_request(request_with_body(
                "POST",
                "/__meta/restore",
                &dump.body.to_string()
            ))
            .status,
        200
    );
    let restored_read = restored.process_request(storefront_graphql_request(
        include_str!(
            "../../config/parity-requests/storefront/storefront-cart-delivery-read.graphql"
        ),
        json!({ "id": cart_id }),
    ));
    assert_eq!(restored_read.status, 200, "{}", restored_read.body);
    assert_eq!(
        restored_read.body["data"]["cart"]["checkoutUrl"],
        json!(checkout_url)
    );
    assert_eq!(
        restored_read.body["data"]["cart"]["delivery"]["addresses"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        restored_read.body["data"]["cart"]["deliveryGroups"]["nodes"][0]["selectedDeliveryOption"]
            ["code"],
        json!("Conformance Standard")
    );

    assert_eq!(
        restored
            .process_request(request_with_body("POST", "/__meta/reset", ""))
            .status,
        200
    );
    let after_reset = restored.process_request(storefront_graphql_request(
        r#"query CartAfterDeliveryReset($id: ID!) { cart(id: $id) { id } }"#,
        json!({ "id": cart_id }),
    ));
    assert_eq!(after_reset.body["data"]["cart"], Value::Null);
}

#[test]
fn storefront_cart_strict_address_validation_uses_country_metadata() {
    let mut proxy = configured_proxy(ReadMode::Snapshot, Some(UnsupportedMutationMode::Reject))
        .with_upstream_transport(|_| panic!("Storefront cart address validation must stay local"));
    restore_state_with(&mut proxy, |state| {
        state["baseState"]["shop"] = json!({ "currencyCode": "USD" });
    });
    let create_cart = |proxy: &mut DraftProxy, country_code: &str| {
        let response = proxy.process_request(storefront_graphql_request(
            include_str!("../../config/parity-requests/storefront/storefront-cart-create.graphql"),
            json!({ "input": { "buyerIdentity": { "countryCode": country_code } } }),
        ));
        response.body["data"]["cartCreate"]["cart"]["id"]
            .as_str()
            .unwrap_or_else(|| panic!("strict address cart create failed: {}", response.body))
            .to_string()
    };
    let add_addresses = |proxy: &mut DraftProxy, cart_id: &str, addresses: Value| {
        proxy.process_request(storefront_graphql_request(
            include_str!(
                "../../config/parity-requests/storefront/storefront-cart-delivery-addresses-add.graphql"
            ),
            json!({ "cartId": cart_id, "addresses": addresses }),
        ))
    };

    let emirates_cart_id = create_cart(&mut proxy, "AE");
    let emirates_required = add_addresses(
        &mut proxy,
        &emirates_cart_id,
        json!([{
            "address": { "deliveryAddress": { "countryCode": "AE" } },
            "validationStrategy": "STRICT"
        }]),
    );
    assert_eq!(
        emirates_required.body["data"]["cartDeliveryAddressesAdd"]["userErrors"],
        json!([
            { "field": ["addresses", "0", "address", "deliveryAddress", "lastName"], "message": "A last name is required in order to continue.", "code": "ADDRESS_FIELD_IS_REQUIRED" },
            { "field": ["addresses", "0", "address", "deliveryAddress", "address1"], "message": "An address is required in order to continue.", "code": "ADDRESS_FIELD_IS_REQUIRED" },
            { "field": ["addresses", "0", "address", "deliveryAddress", "provinceCode"], "message": "The specified country requires a zone.", "code": "ADDRESS_FIELD_IS_REQUIRED" },
            { "field": ["addresses", "0", "address", "deliveryAddress", "city"], "message": "A city is required in order to continue.", "code": "ADDRESS_FIELD_IS_REQUIRED" }
        ])
    );
    assert_eq!(
        emirates_required.body["data"]["cartDeliveryAddressesAdd"]["cart"]["delivery"]["addresses"],
        json!([])
    );

    let emirates_invalid_zone = add_addresses(
        &mut proxy,
        &emirates_cart_id,
        json!([{
            "address": { "deliveryAddress": {
                "firstName": "Cart", "lastName": "Buyer", "address1": "1 Example Street",
                "city": "Dubai", "provinceCode": "ZZ", "countryCode": "AE"
            } },
            "validationStrategy": "STRICT"
        }]),
    );
    assert_eq!(
        emirates_invalid_zone.body["data"]["cartDeliveryAddressesAdd"]["userErrors"],
        json!([{
            "field": ["addresses", "0", "address", "deliveryAddress", "provinceCode"],
            "message": "The specified country requires a zone.",
            "code": "ADDRESS_FIELD_IS_REQUIRED"
        }])
    );
    assert_eq!(
        emirates_invalid_zone.body["data"]["cartDeliveryAddressesAdd"]["cart"]["delivery"]
            ["addresses"],
        json!([])
    );

    let emirates_valid = add_addresses(
        &mut proxy,
        &emirates_cart_id,
        json!([{
            "address": { "deliveryAddress": {
                "firstName": "Cart", "lastName": "Buyer", "address1": "1 Example Street",
                "city": "Dubai", "provinceCode": "du", "countryCode": "AE"
            } },
            "validationStrategy": "STRICT"
        }]),
    );
    assert_eq!(
        emirates_valid.body["data"]["cartDeliveryAddressesAdd"]["userErrors"],
        json!([])
    );
    let emirates_address = &emirates_valid.body["data"]["cartDeliveryAddressesAdd"]["cart"]
        ["delivery"]["addresses"][0]["address"];
    assert_eq!(emirates_address["provinceCode"], json!("DU"));
    assert_eq!(emirates_address["zip"], Value::Null);

    let singapore_cart_id = create_cart(&mut proxy, "SG");
    let singapore_valid = add_addresses(
        &mut proxy,
        &singapore_cart_id,
        json!([{
            "address": { "deliveryAddress": {
                "lastName": "Buyer", "address1": "1 Example Street", "countryCode": "SG",
                "zip": "018989"
            } },
            "validationStrategy": "STRICT"
        }]),
    );
    assert_eq!(
        singapore_valid.body["data"]["cartDeliveryAddressesAdd"]["userErrors"],
        json!([])
    );
    let singapore_address = &singapore_valid.body["data"]["cartDeliveryAddressesAdd"]["cart"]
        ["delivery"]["addresses"][0]["address"];
    assert_eq!(singapore_address["city"], Value::Null);
    assert_eq!(singapore_address["provinceCode"], Value::Null);
    assert_eq!(singapore_address["zip"], json!("018989"));

    let lenient_cart_id = create_cart(&mut proxy, "AU");
    let lenient = add_addresses(
        &mut proxy,
        &lenient_cart_id,
        json!([{
            "address": { "deliveryAddress": { "countryCode": "AU" } }
        }]),
    );
    assert_eq!(
        lenient.body["data"]["cartDeliveryAddressesAdd"]["userErrors"],
        json!([])
    );
    assert_eq!(
        lenient.body["data"]["cartDeliveryAddressesAdd"]["cart"]["delivery"]["addresses"][0]
            ["address"]["countryCode"],
        json!("AU")
    );

    let australia_cart_id = create_cart(&mut proxy, "AU");
    let australia_required = add_addresses(
        &mut proxy,
        &australia_cart_id,
        json!([{
            "address": { "deliveryAddress": { "countryCode": "AU" } },
            "validationStrategy": "STRICT"
        }]),
    );
    assert_eq!(
        australia_required.body["data"]["cartDeliveryAddressesAdd"]["userErrors"],
        json!([
            { "field": ["addresses", "0", "address", "deliveryAddress", "lastName"], "message": "A last name is required in order to continue.", "code": "ADDRESS_FIELD_IS_REQUIRED" },
            { "field": ["addresses", "0", "address", "deliveryAddress", "address1"], "message": "An address is required in order to continue.", "code": "ADDRESS_FIELD_IS_REQUIRED" },
            { "field": ["addresses", "0", "address", "deliveryAddress", "provinceCode"], "message": "The specified country requires a zone.", "code": "ADDRESS_FIELD_IS_REQUIRED" },
            { "field": ["addresses", "0", "address", "deliveryAddress", "zip"], "message": "Country specified requires a postal code in order to continue.", "code": "ADDRESS_FIELD_IS_REQUIRED" },
            { "field": ["addresses", "0", "address", "deliveryAddress", "city"], "message": "A city is required in order to continue.", "code": "ADDRESS_FIELD_IS_REQUIRED" }
        ])
    );

    let australia_postal_normalized = add_addresses(
        &mut proxy,
        &australia_cart_id,
        json!([{
            "address": { "deliveryAddress": {
                "firstName": "Cart", "lastName": "Buyer", "address1": "1 Example Street",
                "city": "Sydney", "provinceCode": "ZZ", "countryCode": "AU", "zip": "2000"
            } },
            "validationStrategy": "STRICT"
        }]),
    );
    assert_eq!(
        australia_postal_normalized.body["data"]["cartDeliveryAddressesAdd"]["userErrors"],
        json!([])
    );
    assert_eq!(
        australia_postal_normalized.body["data"]["cartDeliveryAddressesAdd"]["cart"]["delivery"]
            ["addresses"][0]["address"]["provinceCode"],
        json!("NSW")
    );
}

#[test]
fn storefront_cart_delivery_validates_ownership_inputs_and_stale_options() {
    let mut proxy = configured_proxy(ReadMode::Snapshot, Some(UnsupportedMutationMode::Reject))
        .with_upstream_transport(|_| panic!("Storefront cart delivery validation must stay local"));
    restore_state_with(&mut proxy, |state| {
        state["baseState"]["shop"] = json!({ "currencyCode": "USD" });
    });
    let (_, variant_id, location_id) = stage_storefront_cart_variant(&mut proxy, 5);
    stage_storefront_cart_delivery_profile(&mut proxy, &variant_id, &location_id);
    let create_cart = |proxy: &mut DraftProxy, country_code: &str| {
        proxy.process_request(storefront_graphql_request(
            r#"mutation CreateDeliveryValidationCart($input: CartInput) { cartCreate(input: $input) { cart { id totalQuantity lines(first: 1) { nodes { __typename quantity } } } userErrors { field message code } warnings { code message target } } }"#,
            json!({ "input": { "buyerIdentity": { "countryCode": country_code }, "lines": [{ "merchandiseId": variant_id, "quantity": 1 }] } }),
        ))
    };
    let create = create_cart(&mut proxy, "US");
    let cart_id = create.body["data"]["cartCreate"]["cart"]["id"]
        .as_str()
        .unwrap_or_else(|| panic!("delivery validation cart create failed: {}", create.body))
        .to_string();

    let missing_country = proxy.process_request(storefront_graphql_request(
        include_str!("../../config/parity-requests/storefront/storefront-cart-delivery-addresses-add.graphql"),
        json!({ "cartId": cart_id, "addresses": [{ "address": { "deliveryAddress": { "address1": "123 Example Street", "city": "New York" } }, "selected": true }] }),
    ));
    assert_eq!(
        missing_country.body["data"]["cartDeliveryAddressesAdd"]["userErrors"],
        json!([{
            "field": ["addresses", "0", "address", "deliveryAddress", "countryCode"],
            "message": "invalid value",
            "code": "INVALID"
        }])
    );

    let strict = proxy.process_request(storefront_graphql_request(
        include_str!("../../config/parity-requests/storefront/storefront-cart-delivery-addresses-add.graphql"),
        json!({ "cartId": cart_id, "addresses": [{ "address": { "deliveryAddress": { "countryCode": "US" } }, "selected": true, "validationStrategy": "STRICT" }] }),
    ));
    assert_eq!(
        strict.body["data"]["cartDeliveryAddressesAdd"]["userErrors"],
        json!([
            { "field": ["addresses", "0", "address", "deliveryAddress", "lastName"], "message": "A last name is required in order to continue.", "code": "ADDRESS_FIELD_IS_REQUIRED" },
            { "field": ["addresses", "0", "address", "deliveryAddress", "address1"], "message": "An address is required in order to continue.", "code": "ADDRESS_FIELD_IS_REQUIRED" },
            { "field": ["addresses", "0", "address", "deliveryAddress", "provinceCode"], "message": "The specified country requires a zone.", "code": "ADDRESS_FIELD_IS_REQUIRED" },
            { "field": ["addresses", "0", "address", "deliveryAddress", "zip"], "message": "Country specified requires a postal code in order to continue.", "code": "ADDRESS_FIELD_IS_REQUIRED" },
            { "field": ["addresses", "0", "address", "deliveryAddress", "city"], "message": "A city is required in order to continue.", "code": "ADDRESS_FIELD_IS_REQUIRED" }
        ])
    );

    let add = proxy.process_request(storefront_graphql_request(
        include_str!("../../config/parity-requests/storefront/storefront-cart-delivery-addresses-add.graphql"),
        json!({ "cartId": cart_id, "addresses": [{ "address": { "deliveryAddress": { "lastName": "Buyer", "address1": "123 Example Street", "city": "New York", "provinceCode": "NY", "countryCode": "US", "zip": "10001" } }, "selected": true }] }),
    ));
    let address_id = add.body["data"]["cartDeliveryAddressesAdd"]["cart"]["delivery"]["addresses"]
        [0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let group_id = add.body["data"]["cartDeliveryAddressesAdd"]["cart"]["deliveryGroups"]["nodes"]
        [0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let other = create_cart(&mut proxy, "CA");
    assert_eq!(
        other.body["data"]["cartCreate"]["cart"]["totalQuantity"],
        json!(0)
    );
    assert_eq!(
        other.body["data"]["cartCreate"]["cart"]["lines"]["nodes"][0]["quantity"],
        json!(0)
    );
    assert_eq!(
        other.body["data"]["cartCreate"]["warnings"][0]["code"],
        json!("MERCHANDISE_OUT_OF_STOCK")
    );
    let other_cart_id = other.body["data"]["cartCreate"]["cart"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    for (document, variables, root, field) in [
        (
            include_str!("../../config/parity-requests/storefront/storefront-cart-delivery-addresses-update.graphql"),
            json!({ "cartId": other_cart_id, "addresses": [{ "id": address_id, "selected": true }] }),
            "cartDeliveryAddressesUpdate",
            json!(["addresses", "0"]),
        ),
        (
            include_str!("../../config/parity-requests/storefront/storefront-cart-delivery-addresses-remove.graphql"),
            json!({ "cartId": other_cart_id, "addressIds": [address_id] }),
            "cartDeliveryAddressesRemove",
            json!(["addressIds", "0"]),
        ),
    ] {
        let response = proxy.process_request(storefront_graphql_request(document, variables));
        assert_eq!(response.body["data"][root]["userErrors"][0]["field"], field);
        assert_eq!(response.body["data"][root]["userErrors"][0]["code"], json!("INVALID_DELIVERY_ADDRESS_ID"));
        assert_eq!(response.body["data"][root]["warnings"], json!([]));
    }

    let invalid_option = proxy.process_request(storefront_graphql_request(
        include_str!("../../config/parity-requests/storefront/storefront-cart-selected-delivery-options-update.graphql"),
        json!({ "cartId": cart_id, "selectedDeliveryOptions": [{ "deliveryGroupId": group_id, "deliveryOptionHandle": "invalid-delivery-option-handle" }] }),
    ));
    assert_eq!(
        invalid_option.body["data"]["cartSelectedDeliveryOptionsUpdate"]["cart"],
        Value::Null
    );
    assert_eq!(
        invalid_option.body["data"]["cartSelectedDeliveryOptionsUpdate"]["userErrors"],
        json!([{
            "field": ["selectedDeliveryOptions"],
            "message": "The delivery option with handle invalid-delivery-option-handle is not valid.",
            "code": "INVALID_DELIVERY_OPTION"
        }])
    );

    let too_many = proxy.process_request(storefront_graphql_request(
        include_str!("../../config/parity-requests/storefront/storefront-cart-delivery-addresses-add.graphql"),
        json!({ "cartId": cart_id, "addresses": (0..251).map(|_| json!({ "address": { "deliveryAddress": { "countryCode": "US" } } })).collect::<Vec<_>>() }),
    ));
    assert_eq!(
        too_many.body["data"]["cartDeliveryAddressesAdd"],
        Value::Null
    );
    assert_eq!(
        too_many.body["errors"][0]["extensions"]["code"],
        json!("MAX_INPUT_SIZE_EXCEEDED")
    );
    assert_eq!(
        too_many.body["errors"][0]["path"],
        json!(["cartDeliveryAddressesAdd", "addresses"])
    );
}

#[test]
fn storefront_cart_delivery_recalculates_from_admin_shipping_and_address_context() {
    let mut proxy = configured_proxy(ReadMode::Snapshot, Some(UnsupportedMutationMode::Reject))
        .with_upstream_transport(|_| {
            panic!("Storefront cart delivery context changes must stay local")
        });
    restore_state_with(&mut proxy, |state| {
        state["baseState"]["shop"] = json!({ "currencyCode": "USD" });
    });
    let (_, variant_id, location_id) = stage_storefront_cart_variant(&mut proxy, 5);
    let profile = stage_storefront_cart_delivery_profile(&mut proxy, &variant_id, &location_id);
    let create = proxy.process_request(storefront_graphql_request(
        r#"mutation CreateDeliveryContextCart($input: CartInput) { cartCreate(input: $input) { cart { id } userErrors { field message code } warnings { code message target } } }"#,
        json!({ "input": { "buyerIdentity": { "countryCode": "US" }, "lines": [{ "merchandiseId": variant_id, "quantity": 2 }] } }),
    ));
    let cart_id = create.body["data"]["cartCreate"]["cart"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let add = proxy.process_request(storefront_graphql_request(
        include_str!("../../config/parity-requests/storefront/storefront-cart-delivery-addresses-add.graphql"),
        json!({ "cartId": cart_id, "addresses": [{ "address": { "deliveryAddress": { "lastName": "Buyer", "address1": "123 Example Street", "city": "New York", "provinceCode": "NY", "countryCode": "US", "zip": "10001" } }, "selected": true }] }),
    ));
    let address_id = add.body["data"]["cartDeliveryAddressesAdd"]["cart"]["delivery"]["addresses"]
        [0]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let admin_update = proxy.process_request(json_graphql_request(
        include_str!("../../config/parity-requests/shipping-fulfillments/delivery-profile-lifecycle-update.graphql"),
        json!({
            "id": profile.id,
            "profile": {
                "locationGroupsToUpdate": [{
                    "id": profile.location_group_id,
                    "zonesToUpdate": [{
                        "id": profile.zone_id,
                        "methodDefinitionsToUpdate": [{
                            "id": profile.standard_method_id,
                            "name": "Conformance Standard Updated",
                            "description": "Captured updated storefront cart delivery rate",
                            "active": true,
                            "rateDefinition": {
                                "id": profile.standard_rate_id,
                                "price": { "amount": "8.50", "currencyCode": "USD" }
                            }
                        }]
                    }]
                }]
            }
        }),
    ));
    assert_eq!(admin_update.status, 200, "{}", admin_update.body);
    assert_eq!(
        admin_update.body["data"]["deliveryProfileUpdate"]["userErrors"],
        json!([])
    );
    let after_profile_update = proxy.process_request(storefront_graphql_request(
        include_str!(
            "../../config/parity-requests/storefront/storefront-cart-delivery-read.graphql"
        ),
        json!({ "id": cart_id }),
    ));
    assert_eq!(
        after_profile_update.body["data"]["cart"]["deliveryGroups"]["nodes"][0]["deliveryOptions"]
            [0]["code"],
        json!("Conformance Standard Updated")
    );
    assert_eq!(
        after_profile_update.body["data"]["cart"]["deliveryGroups"]["nodes"][0]["deliveryOptions"]
            [0]["estimatedCost"]["amount"],
        json!("8.5")
    );
    let updated_cost = proxy.process_request(storefront_graphql_request(
        r#"query UpdatedDeliveryCartCost($id: ID!) { cart(id: $id) { cost { totalAmount { amount currencyCode } } } }"#,
        json!({ "id": cart_id }),
    ));
    assert_eq!(
        updated_cost.body["data"]["cart"]["cost"]["totalAmount"]["amount"],
        json!("33.5")
    );

    let backup_location = proxy.process_request(json_graphql_request(
        r#"
        mutation AddDeliveryBackupLocation($input: LocationAddInput!) {
          locationAdd(input: $input) {
            location { id }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "input": { "name": "Storefront inventory backup", "address": { "countryCode": "US" } } }),
    ));
    assert_eq!(
        backup_location.body["data"]["locationAdd"]["userErrors"],
        json!([])
    );
    let disable_location = proxy.process_request(json_graphql_request(
        r#"
        mutation DisableDeliveryLocation($id: ID!, $input: LocationEditInput!) {
          locationEdit(id: $id, input: $input) {
            location { id fulfillsOnlineOrders }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "id": location_id, "input": { "fulfillsOnlineOrders": false } }),
    ));
    assert_eq!(
        disable_location.body["data"]["locationEdit"]["userErrors"],
        json!([])
    );
    let unavailable_location_read = proxy.process_request(storefront_graphql_request(
        include_str!(
            "../../config/parity-requests/storefront/storefront-cart-delivery-read.graphql"
        ),
        json!({ "id": cart_id }),
    ));
    assert_eq!(
        unavailable_location_read.body["data"]["cart"]["deliveryGroups"]["nodes"],
        json!([])
    );

    let restore_location = proxy.process_request(json_graphql_request(
        r#"
        mutation RestoreDeliveryLocation($id: ID!, $input: LocationEditInput!) {
          locationEdit(id: $id, input: $input) {
            location { id fulfillsOnlineOrders }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "id": location_id, "input": { "fulfillsOnlineOrders": true } }),
    ));
    assert_eq!(
        restore_location.body["data"]["locationEdit"]["userErrors"],
        json!([])
    );
    let restored_location_read = proxy.process_request(storefront_graphql_request(
        include_str!(
            "../../config/parity-requests/storefront/storefront-cart-delivery-read.graphql"
        ),
        json!({ "id": cart_id }),
    ));
    assert_eq!(
        restored_location_read.body["data"]["cart"]["deliveryGroups"]["nodes"][0]
            ["deliveryOptions"][0]["code"],
        json!("Conformance Standard Updated")
    );

    let non_deliverable = proxy.process_request(storefront_graphql_request(
        include_str!("../../config/parity-requests/storefront/storefront-cart-delivery-addresses-update.graphql"),
        json!({
            "cartId": cart_id,
            "addresses": [{
                "id": address_id,
                "address": { "deliveryAddress": { "lastName": "Buyer", "address1": "123 Example Street", "city": "Ottawa", "provinceCode": "ON", "countryCode": "CA", "zip": "K1A 0B1" } },
                "selected": true
            }]
        }),
    ));
    assert_eq!(
        non_deliverable.body["data"]["cartDeliveryAddressesUpdate"]["cart"]["totalQuantity"],
        json!(0)
    );
    assert_eq!(
        non_deliverable.body["data"]["cartDeliveryAddressesUpdate"]["cart"]["deliveryGroups"]
            ["nodes"],
        json!([])
    );
    assert_eq!(
        non_deliverable.body["data"]["cartDeliveryAddressesUpdate"]["warnings"][0]["code"],
        json!("MERCHANDISE_OUT_OF_STOCK")
    );
}

#[test]
fn storefront_cart_mutations_cannot_mix_with_passthrough_roots() {
    let mut proxy = configured_proxy(
        ReadMode::LiveHybrid,
        Some(UnsupportedMutationMode::Passthrough),
    )
    .with_upstream_transport(|_| {
        panic!("mixed Storefront cart mutations must never reach upstream")
    });
    let response = proxy.process_request(storefront_graphql_request(
        r#"
        mutation MixedCartMutation($input: CartInput, $cartId: ID!) {
          cartCreate(input: $input) { cart { id } userErrors { message } }
          cartPrepareForCompletion(cartId: $cartId) { userErrors { message } }
        }
        "#,
        json!({ "input": { "note": "must not stage" }, "cartId": "gid://shopify/Cart/missing?key=missing" }),
    ));
    assert_eq!(response.status, 400);
    assert_eq!(
        response.body["errors"][0]["message"],
        json!("Storefront cart mutations cannot be mixed with unsupported Storefront roots")
    );
    let state = state_snapshot(&proxy);
    assert_eq!(state["stagedState"]["storefrontCarts"], json!({}));
}

#[test]
fn storefront_customer_auth_lifecycle_stages_locally_and_redacts_meta() {
    let mut proxy = configured_proxy(ReadMode::Snapshot, Some(UnsupportedMutationMode::Reject))
        .with_upstream_transport(|_| panic!("Storefront customer auth must stay local"));

    let create = proxy.process_request(storefront_graphql_request(
        r#"
        mutation StorefrontCustomerCreate($input: CustomerCreateInput!) {
          customerCreate(input: $input) {
            customer { id email firstName lastName acceptsMarketing numberOfOrders tags addresses(first: 2) { nodes { id } pageInfo { hasNextPage hasPreviousPage } } }
            customerUserErrors { field message code }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "input": {
                "email": "storefront-auth@example.test",
                "password": "CodexPass123!",
                "firstName": "Storefront",
                "lastName": "Auth",
                "acceptsMarketing": true
            }
        }),
    ));
    assert_eq!(create.status, 200, "{}", create.body);
    let customer_id = create.body["data"]["customerCreate"]["customer"]["id"]
        .as_str()
        .expect("created customer id")
        .to_string();
    assert_eq!(
        create.body["data"]["customerCreate"]["customer"]["email"],
        json!("storefront-auth@example.test")
    );
    assert_eq!(
        create.body["data"]["customerCreate"]["customer"]["numberOfOrders"],
        json!("0")
    );
    assert_eq!(
        create.body["data"]["customerCreate"]["customerUserErrors"],
        json!([])
    );

    let bad_token = proxy.process_request(storefront_graphql_request(
        r#"
        mutation StorefrontCustomerBadToken($input: CustomerAccessTokenCreateInput!) {
          customerAccessTokenCreate(input: $input) {
            customerAccessToken { accessToken expiresAt }
            customerUserErrors { field message code }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "input": {
                "email": "storefront-auth@example.test",
                "password": "wrong"
            }
        }),
    ));
    assert_eq!(
        bad_token.body["data"]["customerAccessTokenCreate"]["customerUserErrors"],
        json!([{
            "field": null,
            "message": "Unidentified customer",
            "code": "UNIDENTIFIED_CUSTOMER"
        }])
    );

    let token_create = proxy.process_request(storefront_graphql_request(
        r#"
        mutation StorefrontCustomerToken($input: CustomerAccessTokenCreateInput!) {
          customerAccessTokenCreate(input: $input) {
            customerAccessToken { accessToken expiresAt }
            customerUserErrors { field message code }
          }
        }
        "#,
        json!({
            "input": {
                "email": "storefront-auth@example.test",
                "password": "CodexPass123!"
            }
        }),
    ));
    let access_token = token_create.body["data"]["customerAccessTokenCreate"]
        ["customerAccessToken"]["accessToken"]
        .as_str()
        .expect("access token")
        .to_string();
    assert!(access_token.starts_with("sdp_ca_"));

    let read = proxy.process_request(storefront_graphql_request(
        r#"
        query StorefrontCustomerRead($token: String!) {
          customer(customerAccessToken: $token) { id email displayName acceptsMarketing }
        }
        "#,
        json!({ "token": access_token }),
    ));
    assert_eq!(read.body["data"]["customer"]["id"], json!(customer_id));
    assert_eq!(
        read.body["data"]["customer"]["displayName"],
        json!("Storefront Auth")
    );

    let renew = proxy.process_request(storefront_graphql_request(
        r#"
        mutation StorefrontCustomerRenew($token: String!) {
          customerAccessTokenRenew(customerAccessToken: $token) {
            customerAccessToken { accessToken expiresAt }
            userErrors { field message }
          }
        }
        "#,
        json!({ "token": access_token }),
    ));
    assert_eq!(
        renew.body["data"]["customerAccessTokenRenew"]["customerAccessToken"]["accessToken"],
        json!(access_token)
    );

    let delete = proxy.process_request(storefront_graphql_request(
        r#"
        mutation StorefrontCustomerDeleteToken($token: String!) {
          customerAccessTokenDelete(customerAccessToken: $token) {
            deletedAccessToken
            deletedCustomerAccessTokenId
            userErrors { field message }
          }
        }
        "#,
        json!({ "token": access_token }),
    ));
    assert_eq!(
        delete.body["data"]["customerAccessTokenDelete"]["deletedAccessToken"],
        json!(access_token)
    );
    assert!(
        delete.body["data"]["customerAccessTokenDelete"]["deletedCustomerAccessTokenId"]
            .as_str()
            .unwrap_or_default()
            .starts_with("gid://shopify/CustomerAccessToken/")
    );

    let read_after_delete = proxy.process_request(storefront_graphql_request(
        r#"query($token: String!) { customer(customerAccessToken: $token) { id } }"#,
        json!({ "token": access_token }),
    ));
    assert_eq!(read_after_delete.body["data"]["customer"], Value::Null);

    let delete_again = proxy.process_request(storefront_graphql_request(
        r#"
        mutation StorefrontCustomerDeleteTokenAgain($token: String!) {
          customerAccessTokenDelete(customerAccessToken: $token) {
            deletedAccessToken
            userErrors { field message }
          }
        }
        "#,
        json!({ "token": access_token }),
    ));
    assert_eq!(
        delete_again.body["data"]["customerAccessTokenDelete"],
        Value::Null
    );
    assert_eq!(
        delete_again.body["errors"][0]["extensions"]["code"],
        json!("ACCESS_DENIED")
    );
    assert_eq!(delete_again.body["errors"][0]["locations"], json!([]));

    let log = log_snapshot(&proxy);
    for entry in log["entries"].as_array().expect("log entries") {
        assert_eq!(
            entry["rawBody"],
            json!("<redacted:storefront-customer-auth-request>")
        );
        assert_eq!(
            entry["query"],
            json!("<redacted:storefront-customer-auth-query>")
        );
    }
    assert_eq!(
        log["entries"][0]["variables"]["input"]["password"],
        json!("<redacted:storefront-customer-auth>")
    );
    assert_eq!(
        log["entries"][2]["variables"]["input"]["password"],
        json!("<redacted:storefront-customer-auth>")
    );
    assert_eq!(
        log["entries"][3]["variables"]["token"],
        json!("<redacted:storefront-customer-auth>")
    );

    let state = state_snapshot(&proxy);
    assert_ne!(
        state["stagedState"]["customers"][customer_id.as_str()]["__storefrontPasswordFingerprint"],
        json!("CodexPass123!")
    );
    let token_state = state["stagedState"]["storefrontCustomerAccessTokens"]
        .as_object()
        .expect("token state");
    assert_eq!(token_state.len(), 1);
    assert!(!token_state.contains_key(&access_token));
    assert!(token_state
        .values()
        .all(|record| record.get("accessToken").is_none()));
}

#[test]
fn storefront_customer_profile_addresses_orders_and_restore_share_state() {
    let mut proxy = configured_proxy(ReadMode::Snapshot, Some(UnsupportedMutationMode::Reject))
        .with_upstream_transport(|request| {
            panic!(
                "Storefront customer profile/address/order behavior must stay local: {}",
                request.body
            )
        });

    let (customer_id, access_token) = create_storefront_customer_token(
        &mut proxy,
        "storefront-profile@example.test",
        "Original",
        "Customer",
    );

    let denied_email_update = proxy.process_request(storefront_graphql_request(
        r#"
        mutation UpdateProfile($token: String!, $customer: CustomerUpdateInput!) {
          profile: customerUpdate(customerAccessToken: $token, customer: $customer) {
            customer {
              id
              email
              firstName
              lastName
              displayName
              phone
              acceptsMarketing
            }
            customerAccessToken { accessToken expiresAt }
            customerUserErrors { field message code }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "token": access_token,
            "customer": {
                "email": "storefront-profile-updated@example.test",
                "firstName": "Denied",
                "acceptsMarketing": true
            }
        }),
    ));
    assert_eq!(
        denied_email_update.status, 200,
        "{}",
        denied_email_update.body
    );
    assert_eq!(
        denied_email_update.body["errors"],
        Value::Null,
        "{}",
        denied_email_update.body
    );
    assert_eq!(
        denied_email_update.body["data"]["profile"]["customer"],
        Value::Null
    );
    assert_eq!(
        denied_email_update.body["data"]["profile"]["customerAccessToken"],
        Value::Null
    );
    assert_eq!(
        denied_email_update.body["data"]["profile"]["customerUserErrors"],
        json!([{
            "field": ["customer", "email"],
            "message": "CustomerUpdate access denied",
            "code": "INVALID"
        }])
    );

    let update = proxy.process_request(storefront_graphql_request(
        r#"
        mutation UpdateProfile($token: String!, $customer: CustomerUpdateInput!) {
          profile: customerUpdate(customerAccessToken: $token, customer: $customer) {
            customer {
              id
              email
              firstName
              lastName
              displayName
              phone
              acceptsMarketing
            }
            customerAccessToken { accessToken expiresAt }
            customerUserErrors { field message code }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "token": access_token,
            "customer": {
                "firstName": "Updated",
                "lastName": "Profile",
                "phone": "+16135550123",
                "acceptsMarketing": true
            }
        }),
    ));
    assert_eq!(update.status, 200, "{}", update.body);
    assert_eq!(update.body["errors"], Value::Null, "{}", update.body);
    assert_eq!(
        update.body["data"]["profile"]["customer"],
        json!({
            "id": customer_id,
            "email": "storefront-profile@example.test",
            "firstName": "Updated",
            "lastName": "Profile",
            "displayName": "Updated Profile",
            "phone": "+16135550123",
            "acceptsMarketing": true
        })
    );
    assert_eq!(
        update.body["data"]["profile"]["customerAccessToken"],
        Value::Null
    );
    assert_eq!(
        update.body["data"]["profile"]["customerUserErrors"],
        json!([])
    );

    let create_address = proxy.process_request(storefront_graphql_request(
        r#"
        mutation CreateAddress($token: String!, $address: MailingAddressInput!) {
          customerAddressCreate(customerAccessToken: $token, address: $address) {
            customerAddress {
              id
              firstName
              lastName
              address1
              city
              province
              country
              countryCodeV2
              zip
              phone
              name
              formattedArea
            }
            customerUserErrors { field message code }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "token": access_token,
            "address": {
                "address1": "1 Main St",
                "city": "Ottawa",
                "province": "Ontario",
                "country": "Canada",
                "zip": "K1A 0B1",
                "phone": "+1 (613) 555-0199"
            }
        }),
    ));
    assert_eq!(create_address.status, 200, "{}", create_address.body);
    assert_eq!(
        create_address.body["errors"],
        Value::Null,
        "{}",
        create_address.body
    );
    assert_eq!(
        create_address.body["data"]["customerAddressCreate"]["customerUserErrors"],
        json!([])
    );
    let first_address_id = create_address.body["data"]["customerAddressCreate"]["customerAddress"]
        ["id"]
        .as_str()
        .expect("address id")
        .to_string();
    assert_eq!(
        create_address.body["data"]["customerAddressCreate"]["customerAddress"]["name"],
        json!("Updated Profile")
    );

    let second_address = proxy.process_request(storefront_graphql_request(
        r#"
        mutation CreateSecondAddress($token: String!, $address: MailingAddressInput!) {
          customerAddressCreate(customerAccessToken: $token, address: $address) {
            customerAddress { id address1 city country }
            customerUserErrors { field message code }
          }
        }
        "#,
        json!({
            "token": access_token,
            "address": {
                "firstName": "Second",
                "lastName": "Address",
                "address1": "2 Side St",
                "city": "Toronto",
                "country": "Canada"
            }
        }),
    ));
    let second_address_id = second_address.body["data"]["customerAddressCreate"]["customerAddress"]
        ["id"]
        .as_str()
        .expect("second address id")
        .to_string();

    let make_default = proxy.process_request(storefront_graphql_request(
        r#"
        mutation MakeDefault($token: String!, $addressId: ID!) {
          customerDefaultAddressUpdate(customerAccessToken: $token, addressId: $addressId) {
            customer {
              id
              defaultAddress { id address1 city }
              addresses(first: 5) { nodes { id address1 city } }
            }
            customerUserErrors { field message code }
          }
        }
        "#,
        json!({ "token": access_token, "addressId": second_address_id }),
    ));
    assert_eq!(make_default.status, 200, "{}", make_default.body);
    assert_eq!(
        make_default.body["data"]["customerDefaultAddressUpdate"]["customer"]["defaultAddress"]
            ["id"],
        json!(second_address_id)
    );

    let update_address = proxy.process_request(storefront_graphql_request(
        r#"
        mutation UpdateAddress($token: String!, $id: ID!, $address: MailingAddressInput!) {
          customerAddressUpdate(customerAccessToken: $token, id: $id, address: $address) {
            customerAddress { id address1 city country }
            customerUserErrors { field message code }
          }
        }
        "#,
        json!({
            "token": access_token,
            "id": first_address_id,
            "address": { "address1": "10 Main St", "city": "Gatineau", "country": "Canada" }
        }),
    ));
    assert_eq!(
        update_address.body["data"]["customerAddressUpdate"]["customerAddress"]["address1"],
        json!("10 Main St")
    );

    let order = proxy.process_request(graphql_request(
        "POST",
        &json!({
            "query": r#"
            mutation SeedCustomerOrder($order: OrderCreateOrderInput!) {
              orderCreate(order: $order) {
                order { id name email customer { id email } }
                userErrors { field message code }
              }
            }
            "#,
            "variables": {
                "order": {
                    "email": "storefront-order@example.test",
                    "customerId": customer_id,
                    "currency": "CAD",
                    "lineItems": [{ "title": "Storefront visible item", "quantity": 1 }]
                }
            }
        })
        .to_string(),
    ));
    assert_eq!(order.status, 200, "{}", order.body);
    assert_eq!(order.body["data"]["orderCreate"]["userErrors"], json!([]));
    assert_eq!(
        order.body["data"]["orderCreate"]["order"]["customer"]["email"],
        json!("storefront-order@example.test")
    );
    let order_id = order.body["data"]["orderCreate"]["order"]["id"]
        .as_str()
        .expect("order id")
        .to_string();

    let read = proxy.process_request(storefront_graphql_request(
        r#"
        query ReadCustomer($token: String!) {
          customer(customerAccessToken: $token) {
            id
            email
            firstName
            lastName
            defaultAddress { id address1 city }
            addresses(first: 5) {
              nodes { id address1 city }
              pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
            }
            orders(first: 5) {
              nodes { id name email }
              pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
            }
          }
        }
        "#,
        json!({ "token": access_token }),
    ));
    assert_eq!(read.status, 200, "{}", read.body);
    assert_eq!(
        read.body["data"]["customer"]["email"],
        json!("storefront-order@example.test")
    );
    assert_eq!(
        read.body["data"]["customer"]["defaultAddress"]["id"],
        json!(second_address_id)
    );
    assert_eq!(
        read.body["data"]["customer"]["addresses"]["nodes"][0]["address1"],
        json!("10 Main St")
    );
    assert_eq!(
        read.body["data"]["customer"]["orders"]["nodes"][0]["id"],
        json!(order_id)
    );
    assert_eq!(
        read.body["data"]["customer"]["orders"]["nodes"][0]["email"],
        json!("storefront-order@example.test")
    );

    let admin_read = proxy.process_request(graphql_request(
        "POST",
        &json!({
            "query": r#"
            query AdminReadCustomer($id: ID!) {
              customer(id: $id) {
                id
                email
                firstName
                lastName
                defaultAddress { id address1 city }
                addressesV2(first: 5) { nodes { id address1 city } }
                orders(first: 5) { nodes { id name email } }
              }
            }
            "#,
            "variables": { "id": customer_id }
        })
        .to_string(),
    ));
    assert_eq!(
        admin_read.body["data"]["customer"]["defaultAddress"]["id"],
        json!(second_address_id)
    );
    assert_eq!(
        admin_read.body["data"]["customer"]["addressesV2"]["nodes"][0]["address1"],
        json!("10 Main St")
    );
    assert_eq!(
        admin_read.body["data"]["customer"]["orders"]["nodes"][0]["id"],
        json!(order_id)
    );

    let deleted_default = proxy.process_request(storefront_graphql_request(
        r#"
        mutation DeleteDefault($token: String!, $id: ID!) {
          customerAddressDelete(customerAccessToken: $token, id: $id) {
            deletedCustomerAddressId
            customerUserErrors { field message code }
          }
        }
        "#,
        json!({ "token": access_token, "id": second_address_id }),
    ));
    assert_eq!(
        deleted_default.body["data"]["customerAddressDelete"]["deletedCustomerAddressId"],
        json!(second_address_id)
    );
    let after_delete = proxy.process_request(storefront_graphql_request(
        r#"
        query AfterDefaultDelete($token: String!) {
          customer(customerAccessToken: $token) {
            defaultAddress { id }
            addresses(first: 5) { nodes { id } }
          }
        }
        "#,
        json!({ "token": access_token }),
    ));
    assert_eq!(
        after_delete.body["data"]["customer"]["defaultAddress"]["id"],
        json!(first_address_id)
    );
    assert_eq!(
        after_delete.body["data"]["customer"]["addresses"]["nodes"]
            .as_array()
            .unwrap()
            .len(),
        1
    );

    let invalid_token = proxy.process_request(storefront_graphql_request(
        r#"
        mutation InvalidToken($token: String!) {
          customerAddressCreate(customerAccessToken: $token, address: { address1: "3 Lost St" }) {
            customerAddress { id }
            customerUserErrors { field message code }
            userErrors { field message }
          }
        }
        "#,
        json!({ "token": "not-a-token" }),
    ));
    assert_eq!(
        invalid_token.body["data"]["customerAddressCreate"],
        Value::Null
    );
    assert_eq!(
        invalid_token.body["errors"],
        json!([{
            "message": "Access denied for customerAddressCreate field. Required access: `unauthenticated_write_customers` access scope. Also: Requires valid customer access token.",
            "path": ["customerAddressCreate"],
            "locations": [],
            "extensions": {
                "code": "ACCESS_DENIED",
                "documentation": "https://shopify.dev/api/usage/access-scopes",
                "requiredAccess": "`unauthenticated_write_customers` access scope. Also: Requires valid customer access token."
            }
        }])
    );

    let dump = proxy.process_request(request_with_body("POST", "/__meta/dump", ""));
    assert_eq!(dump.status, 200);
    let mut restored = configured_proxy(ReadMode::Snapshot, Some(UnsupportedMutationMode::Reject))
        .with_upstream_transport(|_| {
            panic!("restored Storefront customer state should stay local")
        });
    let restore = restored.process_request(request_with_body(
        "POST",
        "/__meta/restore",
        &dump.body.to_string(),
    ));
    assert_eq!(restore.status, 200);
    let restored_read = restored.process_request(storefront_graphql_request(
        r#"
        query RestoredCustomer($token: String!) {
          customer(customerAccessToken: $token) {
            id
            defaultAddress { id }
            orders(first: 5) { nodes { id } }
          }
        }
        "#,
        json!({ "token": access_token }),
    ));
    assert_eq!(
        restored_read.body["data"]["customer"]["defaultAddress"]["id"],
        json!(first_address_id)
    );
    assert_eq!(
        restored_read.body["data"]["customer"]["orders"]["nodes"][0]["id"],
        json!(order_id)
    );

    let reset = restored.process_request(request_with_body("POST", "/__meta/reset", ""));
    assert_eq!(reset.status, 200);
    let after_reset = restored.process_request(storefront_graphql_request(
        r#"query AfterReset($token: String!) { customer(customerAccessToken: $token) { id } }"#,
        json!({ "token": access_token }),
    ));
    assert_eq!(after_reset.body["data"]["customer"], Value::Null);
}

#[test]
fn storefront_customer_order_projection_preserves_authoritative_fields_across_admin_updates() {
    let mut proxy = configured_proxy(ReadMode::Snapshot, Some(UnsupportedMutationMode::Reject))
        .with_upstream_transport(|request| {
            panic!(
                "Storefront order projection must remain local: {}",
                request.body
            )
        });
    let (customer_id, access_token) = create_storefront_customer_token(
        &mut proxy,
        "storefront-order-projection@example.test",
        "Order",
        "Projection",
    );

    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateProjectedOrder($order: OrderCreateOrderInput!) {
          orderCreate(order: $order) {
            order { id name email phone }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "order": {
                "customerId": customer_id,
                "name": "IMPORT-9-X2-2026",
                "email": "order-before-update@example.test",
                "phone": "+33123456789",
                "currency": "EUR",
                "financialStatus": "PAID",
                "fulfillmentStatus": "FULFILLED",
                "processedAt": "2026-01-02T03:04:05Z",
                "lineItems": [{
                    "title": "Authoritative projection item",
                    "quantity": 1,
                    "priceSet": {
                        "shopMoney": { "amount": "31.25", "currencyCode": "EUR" }
                    }
                }]
            }
        }),
    ));
    assert_eq!(create.status, 200, "{}", create.body);
    assert_eq!(create.body["data"]["orderCreate"]["userErrors"], json!([]));
    let order_id = create.body["data"]["orderCreate"]["order"]["id"]
        .as_str()
        .expect("created order id")
        .to_string();

    let read_order = |proxy: &mut DraftProxy| {
        proxy.process_request(storefront_graphql_request(
            r#"
            query ReadProjectedOrder($token: String!) {
              customer(customerAccessToken: $token) {
                orders(first: 5) {
                  nodes {
                    id
                    name
                    email
                    phone
                    currencyCode
                    financialStatus
                    fulfillmentStatus
                    orderNumber
                    processedAt
                    subtotalPriceV2 { amount currencyCode }
                    totalPrice { amount currencyCode }
                    totalPriceV2 { amount currencyCode }
                  }
                }
              }
            }
            "#,
            json!({ "token": access_token }),
        ))
    };

    let before_update = read_order(&mut proxy);
    assert_eq!(before_update.status, 200, "{}", before_update.body);
    let before = &before_update.body["data"]["customer"]["orders"]["nodes"][0];
    assert_eq!(before["id"], json!(order_id));
    assert_eq!(before["name"], json!("IMPORT-9-X2-2026"));
    assert_eq!(before["email"], json!("order-before-update@example.test"));
    assert_eq!(before["phone"], json!("+33123456789"));
    assert_eq!(before["currencyCode"], json!("EUR"));
    assert_eq!(before["financialStatus"], json!("PAID"));
    assert_eq!(before["fulfillmentStatus"], json!("FULFILLED"));
    assert_eq!(before["orderNumber"], json!(1));
    assert_ne!(before["orderNumber"], json!(922026));
    assert_eq!(before["processedAt"], json!("2026-01-02T03:04:05Z"));
    assert_eq!(
        before["subtotalPriceV2"],
        json!({ "amount": "31.25", "currencyCode": "EUR" })
    );
    assert_eq!(before["totalPrice"], before["totalPriceV2"]);
    assert_eq!(
        before["totalPriceV2"],
        json!({ "amount": "31.25", "currencyCode": "EUR" })
    );

    let update = proxy.process_request(json_graphql_request(
        r#"
        mutation UpdateProjectedOrder($input: OrderInput!) {
          orderUpdate(input: $input) {
            order { id name email phone }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "input": {
                "id": order_id,
                "email": "order-after-update@example.test",
                "phone": "+49891234567"
            }
        }),
    ));
    assert_eq!(update.status, 200, "{}", update.body);
    assert_eq!(update.body["data"]["orderUpdate"]["userErrors"], json!([]));

    let after_update = read_order(&mut proxy);
    assert_eq!(after_update.status, 200, "{}", after_update.body);
    let after = &after_update.body["data"]["customer"]["orders"]["nodes"][0];
    assert_eq!(after["email"], json!("order-after-update@example.test"));
    assert_eq!(after["phone"], json!("+49891234567"));
    for field in [
        "id",
        "name",
        "currencyCode",
        "financialStatus",
        "fulfillmentStatus",
        "orderNumber",
        "processedAt",
        "subtotalPriceV2",
        "totalPrice",
        "totalPriceV2",
    ] {
        assert_eq!(
            after[field], before[field],
            "Admin orderUpdate must preserve authoritative Storefront Order.{field}"
        );
    }
}

#[test]
fn storefront_customer_password_update_rotates_access_tokens() {
    let mut proxy = configured_proxy(ReadMode::Snapshot, Some(UnsupportedMutationMode::Reject))
        .with_upstream_transport(|_| panic!("Storefront password update must stay local"));

    let (customer_id, access_token) = create_storefront_customer_token(
        &mut proxy,
        "storefront-password@example.test",
        "Password",
        "Rotation",
    );

    let update = proxy.process_request(storefront_graphql_request(
        r#"
        mutation RotatePassword($token: String!, $customer: CustomerUpdateInput!) {
          customerUpdate(customerAccessToken: $token, customer: $customer) {
            customer { id email }
            customerAccessToken { accessToken expiresAt }
            customerUserErrors { field message code }
          }
        }
        "#,
        json!({
            "token": access_token,
            "customer": { "password": "NewCodexPass123!" }
        }),
    ));
    assert_eq!(update.status, 200, "{}", update.body);
    assert_eq!(
        update.body["data"]["customerUpdate"]["customer"]["id"],
        json!(customer_id)
    );
    let rotated_token = update.body["data"]["customerUpdate"]["customerAccessToken"]["accessToken"]
        .as_str()
        .expect("rotated token")
        .to_string();
    assert_ne!(rotated_token, access_token);

    let old_read = proxy.process_request(storefront_graphql_request(
        r#"query OldToken($token: String!) { customer(customerAccessToken: $token) { id } }"#,
        json!({ "token": access_token }),
    ));
    assert_eq!(old_read.body["data"]["customer"], Value::Null);

    let new_read = proxy.process_request(storefront_graphql_request(
        r#"query NewToken($token: String!) { customer(customerAccessToken: $token) { id } }"#,
        json!({ "token": rotated_token }),
    ));
    assert_eq!(new_read.body["data"]["customer"]["id"], json!(customer_id));

    let old_password_login = proxy.process_request(storefront_graphql_request(
        r#"
        mutation OldPassword($input: CustomerAccessTokenCreateInput!) {
          customerAccessTokenCreate(input: $input) {
            customerAccessToken { accessToken }
            customerUserErrors { field message code }
          }
        }
        "#,
        json!({
            "input": {
                "email": "storefront-password@example.test",
                "password": "CodexPass123!"
            }
        }),
    ));
    assert_eq!(
        old_password_login.body["data"]["customerAccessTokenCreate"]["customerUserErrors"][0]
            ["code"],
        json!("UNIDENTIFIED_CUSTOMER")
    );

    let new_password_login = proxy.process_request(storefront_graphql_request(
        r#"
        mutation NewPassword($input: CustomerAccessTokenCreateInput!) {
          customerAccessTokenCreate(input: $input) {
            customerAccessToken { accessToken }
            customerUserErrors { field message code }
          }
        }
        "#,
        json!({
            "input": {
                "email": "storefront-password@example.test",
                "password": "NewCodexPass123!"
            }
        }),
    ));
    assert!(
        new_password_login.body["data"]["customerAccessTokenCreate"]["customerAccessToken"]
            ["accessToken"]
            .as_str()
            .unwrap_or_default()
            .starts_with("sdp_ca_")
    );
}

#[test]
fn storefront_customer_reads_admin_profile_and_address_changes() {
    let mut proxy = configured_proxy(ReadMode::Snapshot, Some(UnsupportedMutationMode::Reject))
        .with_upstream_transport(|_| panic!("cross-surface customer reads must stay local"));

    let (customer_id, access_token) = create_storefront_customer_token(
        &mut proxy,
        "storefront-admin-visible@example.test",
        "Storefront",
        "Visible",
    );

    let admin_update = proxy.process_request(graphql_request(
        "POST",
        &json!({
            "query": r#"
            mutation AdminProfileUpdate($input: CustomerInput!) {
              customerUpdate(input: $input) {
                customer { id firstName lastName email }
                userErrors { field message }
              }
            }
            "#,
            "variables": {
                "input": {
                    "id": customer_id,
                    "firstName": "Admin",
                    "lastName": "Visible",
                    "email": "storefront-admin-updated@example.test"
                }
            }
        })
        .to_string(),
    ));
    assert_eq!(admin_update.status, 200, "{}", admin_update.body);
    assert_eq!(
        admin_update.body["data"]["customerUpdate"]["userErrors"],
        json!([])
    );

    let admin_address = proxy.process_request(graphql_request(
        "POST",
        &json!({
            "query": r#"
            mutation AdminAddressCreate($customerId: ID!, $address: MailingAddressInput!) {
              customerAddressCreate(customerId: $customerId, address: $address, setAsDefault: true) {
                address { id address1 city country }
                userErrors { field message }
              }
            }
            "#,
            "variables": {
                "customerId": customer_id,
                "address": {
                    "address1": "50 Admin Way",
                    "city": "Montreal",
                    "country": "Canada"
                }
            }
        })
        .to_string(),
    ));
    assert_eq!(admin_address.status, 200, "{}", admin_address.body);
    assert_eq!(
        admin_address.body["data"]["customerAddressCreate"]["userErrors"],
        json!([])
    );
    let address_id = admin_address.body["data"]["customerAddressCreate"]["address"]["id"]
        .as_str()
        .expect("admin address id")
        .to_string();

    let storefront_read = proxy.process_request(storefront_graphql_request(
        r#"
        query StorefrontSeesAdminChanges($token: String!) {
          customer(customerAccessToken: $token) {
            id
            email
            firstName
            lastName
            defaultAddress { id address1 city }
            addresses(first: 5) { nodes { id address1 city } }
          }
        }
        "#,
        json!({ "token": access_token }),
    ));
    assert_eq!(storefront_read.status, 200, "{}", storefront_read.body);
    assert_eq!(
        storefront_read.body["data"]["customer"]["email"],
        json!("storefront-admin-updated@example.test")
    );
    assert_eq!(
        storefront_read.body["data"]["customer"]["firstName"],
        json!("Admin")
    );
    assert_eq!(
        storefront_read.body["data"]["customer"]["defaultAddress"]["id"],
        json!(address_id)
    );
    assert_eq!(
        storefront_read.body["data"]["customer"]["addresses"]["nodes"][0]["address1"],
        json!("50 Admin Way")
    );
}

#[test]
fn storefront_customer_activation_recovery_and_reset_are_local_only() {
    let mut proxy = configured_proxy(ReadMode::Snapshot, Some(UnsupportedMutationMode::Reject))
        .with_upstream_transport(|_| panic!("Storefront customer auth must stay local"));

    let admin_create = proxy.process_request(json_graphql_request(
        r#"
        mutation AdminCreateDisabledCustomer($input: CustomerInput!) {
          customerCreate(input: $input) {
            customer { id email state }
            userErrors { field message }
          }
        }
        "#,
        json!({ "input": { "email": "storefront-activate@example.test" } }),
    ));
    let customer_id = admin_create.body["data"]["customerCreate"]["customer"]["id"]
        .as_str()
        .expect("admin customer id")
        .to_string();
    assert_eq!(
        admin_create.body["data"]["customerCreate"]["customer"]["state"],
        json!("DISABLED")
    );

    let activation = proxy.process_request(json_graphql_request(
        r#"
        mutation AdminGenerateActivation($customerId: ID!) {
          customerGenerateAccountActivationUrl(customerId: $customerId) {
            accountActivationUrl
            userErrors { field message }
          }
        }
        "#,
        json!({ "customerId": customer_id }),
    ));
    let activation_url = activation.body["data"]["customerGenerateAccountActivationUrl"]
        ["accountActivationUrl"]
        .as_str()
        .expect("activation URL")
        .to_string();

    let invalid = proxy.process_request(storefront_graphql_request(
        r#"
        mutation StorefrontActivateInvalid($id: ID!, $input: CustomerActivateInput!) {
          customerActivate(id: $id, input: $input) {
            customer { id }
            customerAccessToken { accessToken expiresAt }
            customerUserErrors { field message code }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "id": customer_id,
            "input": {
                "activationToken": "bad-token",
                "password": "CodexPass123!"
            }
        }),
    ));
    assert_eq!(
        invalid.body["data"]["customerActivate"]["customerUserErrors"],
        json!([{
            "field": ["input"],
            "message": "Invalid activation token",
            "code": "TOKEN_INVALID"
        }]),
        "{}",
        invalid.body
    );
    assert_eq!(
        invalid.body["data"]["customerActivate"]["userErrors"],
        json!([{ "field": null, "message": "Invalid activation token" }])
    );

    let activated = proxy.process_request(storefront_graphql_request(
        r#"
        mutation StorefrontActivateByUrl($activationUrl: URL!, $password: String!) {
          customerActivateByUrl(activationUrl: $activationUrl, password: $password) {
            customer { id email }
            customerAccessToken { accessToken expiresAt }
            customerUserErrors { field message code }
          }
        }
        "#,
        json!({
            "activationUrl": activation_url,
            "password": "CodexPass123!"
        }),
    ));
    let activation_token = activated.body["data"]["customerActivateByUrl"]["customerAccessToken"]
        ["accessToken"]
        .as_str()
        .expect("activation token")
        .to_string();
    assert_eq!(
        activated.body["data"]["customerActivateByUrl"]["customer"]["id"],
        json!(customer_id)
    );

    let recover = proxy.process_request(storefront_graphql_request(
        r#"
        mutation StorefrontRecover($email: String!) {
          customerRecover(email: $email) {
            customerUserErrors { field message code }
            userErrors { field message }
          }
        }
        "#,
        json!({ "email": "storefront-activate@example.test" }),
    ));
    assert_eq!(
        recover.body["data"]["customerRecover"]["customerUserErrors"],
        json!([])
    );
    let reset_token = format!(
        "sdp-reset-{}-1",
        customer_id.rsplit('/').next().expect("customer id tail")
    );

    let reset = proxy.process_request(storefront_graphql_request(
        r#"
        mutation StorefrontReset($id: ID!, $input: CustomerResetInput!) {
          customerReset(id: $id, input: $input) {
            customer { id email }
            customerAccessToken { accessToken expiresAt }
            customerUserErrors { field message code }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "id": customer_id,
            "input": {
                "resetToken": reset_token,
                "password": "NewCodexPass123!"
            }
        }),
    ));
    let reset_access_token = reset.body["data"]["customerReset"]["customerAccessToken"]
        ["accessToken"]
        .as_str()
        .expect("reset access token")
        .to_string();
    assert_ne!(activation_token, reset_access_token);
    assert_eq!(
        reset.body["data"]["customerReset"]["customer"]["email"],
        json!("storefront-activate@example.test")
    );

    let invalid_reset = proxy.process_request(storefront_graphql_request(
        r#"
        mutation StorefrontResetInvalid($id: ID!, $input: CustomerResetInput!) {
          customerReset(id: $id, input: $input) {
            customer { id }
            customerAccessToken { accessToken }
            customerUserErrors { field message code }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "id": customer_id,
            "input": {
                "resetToken": "bad-token",
                "password": "AnotherCodexPass123!"
            }
        }),
    ));
    assert_eq!(
        invalid_reset.body["data"]["customerReset"]["customerUserErrors"],
        json!([{
            "field": ["input"],
            "message": "Invalid reset token",
            "code": "TOKEN_INVALID"
        }])
    );
    assert_eq!(
        invalid_reset.body["data"]["customerReset"]["userErrors"],
        json!([{ "field": null, "message": "Invalid reset token" }])
    );

    let invalid_reset_url = proxy.process_request(storefront_graphql_request(
        r#"
        mutation StorefrontResetByUrlInvalid($resetUrl: URL!, $password: String!) {
          customerResetByUrl(resetUrl: $resetUrl, password: $password) {
            customer { id }
            customerAccessToken { accessToken }
            customerUserErrors { field message code }
          }
        }
        "#,
        json!({
            "resetUrl": "https://example.test/account/reset/bad-token",
            "password": "AnotherCodexPass123!"
        }),
    ));
    assert_eq!(
        invalid_reset_url.body["data"]["customerResetByUrl"],
        Value::Null
    );
    assert_eq!(
        invalid_reset_url.body["errors"][0]["extensions"]["code"],
        json!("NOT_FOUND")
    );
    assert_eq!(invalid_reset_url.body["errors"][0]["locations"], json!([]));

    let old_password = proxy.process_request(storefront_graphql_request(
        r#"
        mutation OldPassword($input: CustomerAccessTokenCreateInput!) {
          customerAccessTokenCreate(input: $input) {
            customerAccessToken { accessToken }
            customerUserErrors { field message code }
          }
        }
        "#,
        json!({
            "input": {
                "email": "storefront-activate@example.test",
                "password": "CodexPass123!"
            }
        }),
    ));
    assert_eq!(
        old_password.body["data"]["customerAccessTokenCreate"]["customerUserErrors"][0]["code"],
        json!("UNIDENTIFIED_CUSTOMER")
    );

    let new_password = proxy.process_request(storefront_graphql_request(
        r#"
        mutation NewPassword($input: CustomerAccessTokenCreateInput!) {
          customerAccessTokenCreate(input: $input) {
            customerAccessToken { accessToken expiresAt }
            customerUserErrors { field message code }
          }
        }
        "#,
        json!({
            "input": {
                "email": "storefront-activate@example.test",
                "password": "NewCodexPass123!"
            }
        }),
    ));
    assert!(
        new_password.body["data"]["customerAccessTokenCreate"]["customerAccessToken"]
            ["accessToken"]
            .as_str()
            .unwrap_or_default()
            .starts_with("sdp_ca_")
    );
}

#[test]
fn storefront_customer_tokens_survive_dump_restore_expire_and_reset_without_cleartext() {
    let clock = Arc::new(Mutex::new(utc_time(1_800_000_000)));
    let mut proxy = configured_proxy(ReadMode::Snapshot, Some(UnsupportedMutationMode::Reject))
        .with_clock({
            let clock = Arc::clone(&clock);
            move || *clock.lock().unwrap()
        })
        .with_upstream_transport(|_| panic!("Storefront customer auth must stay local"));

    let create = proxy.process_request(storefront_graphql_request(
        r#"
        mutation CreateStorefrontCustomer($input: CustomerCreateInput!) {
          customerCreate(input: $input) {
            customer { id email }
            customerUserErrors { field message code }
          }
        }
        "#,
        json!({
            "input": {
                "email": "storefront-expiry@example.test",
                "password": "CodexPass123!"
            }
        }),
    ));
    let customer_id = create.body["data"]["customerCreate"]["customer"]["id"]
        .as_str()
        .expect("created customer id")
        .to_string();

    let token_create = proxy.process_request(storefront_graphql_request(
        r#"
        mutation CreateStorefrontToken($input: CustomerAccessTokenCreateInput!) {
          customerAccessTokenCreate(input: $input) {
            customerAccessToken { accessToken expiresAt }
            customerUserErrors { field message code }
          }
        }
        "#,
        json!({
            "input": {
                "email": "storefront-expiry@example.test",
                "password": "CodexPass123!"
            }
        }),
    ));
    let access_token = token_create.body["data"]["customerAccessTokenCreate"]
        ["customerAccessToken"]["accessToken"]
        .as_str()
        .expect("access token")
        .to_string();
    let expires_at = token_create.body["data"]["customerAccessTokenCreate"]["customerAccessToken"]
        ["expiresAt"]
        .as_str()
        .expect("expires at")
        .to_string();

    let dump = proxy.process_request(request_with_body("POST", "/__meta/dump", ""));
    assert_eq!(dump.status, 200);
    let dumped_state = &dump.body["state"];
    assert_eq!(
        dumped_state["stagedState"]["storefrontCustomerAccessTokens"]
            .as_object()
            .expect("token map")
            .len(),
        1
    );
    assert!(!dumped_state.to_string().contains(access_token.as_str()));
    assert!(
        dumped_state["stagedState"]["customers"][customer_id.as_str()]
            ["__storefrontPasswordFingerprint"]
            .as_str()
            .is_some()
    );

    let mut restored = configured_proxy(ReadMode::Snapshot, Some(UnsupportedMutationMode::Reject))
        .with_clock({
            let clock = Arc::clone(&clock);
            move || *clock.lock().unwrap()
        })
        .with_upstream_transport(|_| panic!("restored Storefront customer auth must stay local"));
    let restore = restored.process_request(request_with_body(
        "POST",
        "/__meta/restore",
        &dump.body.to_string(),
    ));
    assert_eq!(restore.status, 200);

    let restored_read = restored.process_request(storefront_graphql_request(
        r#"query ReadRestoredCustomer($token: String!) { customer(customerAccessToken: $token) { id email } }"#,
        json!({ "token": access_token }),
    ));
    assert_eq!(
        restored_read.body["data"]["customer"]["id"],
        json!(customer_id)
    );

    set_clock(&clock, 1_800_000_000 + 43 * 24 * 60 * 60);
    let expired_read = restored.process_request(storefront_graphql_request(
        r#"query ReadExpiredCustomer($token: String!) { customer(customerAccessToken: $token) { id } }"#,
        json!({ "token": access_token }),
    ));
    assert_eq!(expired_read.body["data"]["customer"], Value::Null);

    let expired_renew = restored.process_request(storefront_graphql_request(
        r#"
        mutation RenewExpiredToken($token: String!) {
          customerAccessTokenRenew(customerAccessToken: $token) {
            customerAccessToken { accessToken expiresAt }
            userErrors { field message }
          }
        }
        "#,
        json!({ "token": access_token }),
    ));
    assert_eq!(
        expired_renew.body["data"]["customerAccessTokenRenew"]["customerAccessToken"],
        Value::Null
    );
    assert_eq!(
        expired_renew.body["data"]["customerAccessTokenRenew"]["userErrors"],
        json!([{ "field": ["customerAccessToken"], "message": "access token does not exist" }])
    );

    let reset = restored.process_request(request_with_body("POST", "/__meta/reset", ""));
    assert_eq!(reset.status, 200);
    let after_reset = restored.process_request(storefront_graphql_request(
        r#"query ReadAfterReset($token: String!) { customer(customerAccessToken: $token) { id } }"#,
        json!({ "token": access_token }),
    ));
    assert_eq!(after_reset.body["data"]["customer"], Value::Null);
    let state_after_reset = state_snapshot(&restored);
    assert_eq!(
        state_after_reset["stagedState"]["storefrontCustomerAccessTokens"],
        json!({})
    );
    assert_eq!(
        state_after_reset["stagedState"]["nextStorefrontCustomerAccessTokenId"],
        json!(1)
    );
    assert_eq!(expires_at, "2027-02-26T08:00:00Z");
}

#[test]
fn storefront_first_slice_hydrates_and_projects_local_roots_with_context() {
    let observed_requests = Arc::new(Mutex::new(Vec::<Request>::new()));
    let observed_for_proxy = Arc::clone(&observed_requests);
    let mut proxy = configured_proxy(
        ReadMode::LiveHybrid,
        Some(UnsupportedMutationMode::Passthrough),
    )
    .with_upstream_transport(move |request| {
        observed_for_proxy.lock().unwrap().push(request);
        Response {
            status: 200,
            headers: Default::default(),
            body: json!({
                "data": {
                    "shop": {
                        "id": "gid://shopify/Shop/1",
                        "name": "Hydrated Storefront Shop",
                        "description": "A captured storefront",
                        "moneyFormat": "${{amount}}",
                        "primaryDomain": {
                            "host": "storefront.example",
                            "url": "https://storefront.example",
                            "sslEnabled": true
                        },
                        "brand": {
                            "shortDescription": "Storefront brand",
                            "slogan": "Local reads",
                            "logo": null,
                            "squareLogo": null,
                            "coverImage": null
                        },
                        "privacyPolicy": {
                            "id": "gid://shopify/ShopPolicy/1",
                            "title": "Privacy Policy",
                            "body": "Privacy body",
                            "handle": "privacy-policy",
                            "url": "https://storefront.example/policies/privacy-policy"
                        },
                        "paymentSettings": {
                            "acceptedCardBrands": ["VISA", "MASTERCARD"],
                            "cardVaultUrl": "https://elb.deposit.shopifycs.com/sessions",
                            "countryCode": "CA",
                            "currencyCode": "CAD",
                            "enabledPresentmentCurrencies": ["CAD", "USD"],
                            "shopifyPaymentsAccountId": "acct_storefront",
                            "supportedDigitalWallets": ["APPLE_PAY", "SHOPIFY_PAY"]
                        }
                    },
                    "localization": {
                        "country": {
                            "isoCode": "CA",
                            "name": "Canada",
                            "unitSystem": "METRIC",
                            "currency": {
                                "isoCode": "CAD",
                                "name": "Canadian Dollar",
                                "symbol": "$"
                            },
                            "defaultLanguage": {
                                "isoCode": "EN",
                                "name": "English",
                                "endonymName": "English"
                            },
                            "availableLanguages": [{
                                "isoCode": "FR",
                                "name": "French",
                                "endonymName": "français"
                            }],
                            "market": {
                                "id": "gid://shopify/Market/1",
                                "handle": "canada"
                            }
                        },
                        "language": {
                            "isoCode": "FR",
                            "name": "French",
                            "endonymName": "français"
                        },
                        "market": {
                            "id": "gid://shopify/Market/1",
                            "handle": "canada"
                        },
                        "availableCountries": [],
                        "availableLanguages": []
                    },
                    "locations": {
                        "edges": [
                            {
                                "cursor": "cursor-location-1",
                                "node": {
                                    "id": "gid://shopify/Location/1",
                                    "name": "Toronto pickup",
                                    "address": {
                                        "address1": "1 Queen St",
                                        "address2": null,
                                        "city": "Toronto",
                                        "country": "Canada",
                                        "countryCode": "CA",
                                        "formatted": ["1 Queen St", "Toronto ON", "Canada"],
                                        "latitude": 43.65,
                                        "longitude": -79.38,
                                        "phone": null,
                                        "province": "Ontario",
                                        "provinceCode": "ON",
                                        "zip": "M5H"
                                    }
                                }
                            },
                            {
                                "cursor": "cursor-location-2",
                                "node": {
                                    "id": "gid://shopify/Location/2",
                                    "name": "Montreal pickup",
                                    "address": {
                                        "address1": "2 Rue Sainte-Catherine",
                                        "address2": null,
                                        "city": "Montreal",
                                        "country": "Canada",
                                        "countryCode": "CA",
                                        "formatted": ["2 Rue Sainte-Catherine", "Montreal QC", "Canada"],
                                        "latitude": 45.5,
                                        "longitude": -73.56,
                                        "phone": null,
                                        "province": "Quebec",
                                        "provinceCode": "QC",
                                        "zip": "H3B"
                                    }
                                }
                            }
                        ],
                        "nodes": [
                            {
                                "id": "gid://shopify/Location/1",
                                "name": "Toronto pickup",
                                "address": {
                                    "address1": "1 Queen St",
                                    "address2": null,
                                    "city": "Toronto",
                                    "country": "Canada",
                                    "countryCode": "CA",
                                    "formatted": ["1 Queen St", "Toronto ON", "Canada"],
                                    "latitude": 43.65,
                                    "longitude": -79.38,
                                    "phone": null,
                                    "province": "Ontario",
                                    "provinceCode": "ON",
                                    "zip": "M5H"
                                }
                            },
                            {
                                "id": "gid://shopify/Location/2",
                                "name": "Montreal pickup",
                                "address": {
                                    "address1": "2 Rue Sainte-Catherine",
                                    "address2": null,
                                    "city": "Montreal",
                                    "country": "Canada",
                                    "countryCode": "CA",
                                    "formatted": ["2 Rue Sainte-Catherine", "Montreal QC", "Canada"],
                                    "latitude": 45.5,
                                    "longitude": -73.56,
                                    "phone": null,
                                    "province": "Quebec",
                                    "provinceCode": "QC",
                                    "zip": "H3B"
                                }
                            }
                        ],
                        "pageInfo": {
                            "hasNextPage": false,
                            "hasPreviousPage": false,
                            "startCursor": "cursor-location-1",
                            "endCursor": "cursor-location-2"
                        }
                    },
                    "paymentSettings": {
                        "acceptedCardBrands": ["VISA", "MASTERCARD"],
                        "cardVaultUrl": "https://elb.deposit.shopifycs.com/sessions",
                        "countryCode": "CA",
                        "currencyCode": "CAD",
                        "enabledPresentmentCurrencies": ["CAD", "USD"],
                        "shopifyPaymentsAccountId": "acct_storefront",
                        "supportedDigitalWallets": ["APPLE_PAY", "SHOPIFY_PAY"]
                    },
                    "publicApiVersions": [
                        {
                            "handle": "2026-04",
                            "displayName": "2026-04",
                            "supported": true
                        }
                    ]
                }
            }),
        }
    });

    let response = proxy.process_request(Request {
        method: "POST".to_string(),
        path: "/api/2026-04/graphql.json".to_string(),
        headers: [(
            "X-Shopify-Storefront-Access-Token".to_string(),
            "storefront-token".to_string(),
        )]
        .into(),
        body: json!({
            "query": r#"
                query StorefrontFirstSlice($includeVersions: Boolean!) @inContext(country: CA, language: FR) {
                  sfShop: shop {
                    ...ShopFields
                    paymentSettings { currencyCode supportedDigitalWallets }
                  }
                  localization {
                    country { isoCode name }
                    language { isoCode name endonymName }
                    market { id handle }
                  }
                  locations(first: 1, sortKey: NAME) {
                    edges { cursor node { id name address { city countryCode formatted } } }
                    pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
                  }
                  paymentSettings { currencyCode countryCode acceptedCardBrands }
                  publicApiVersions @include(if: $includeVersions) { handle displayName supported }
                }

                fragment ShopFields on Shop {
                  name
                  primaryDomain { host }
                  privacyPolicy { title handle }
                  brand { shortDescription }
                }
            "#,
            "variables": { "includeVersions": true }
        })
        .to_string(),
    });

    assert_eq!(response.status, 200);
    assert_eq!(
        response.body["data"]["sfShop"]["name"],
        json!("Hydrated Storefront Shop")
    );
    assert_eq!(
        response.body["data"]["sfShop"]["primaryDomain"],
        json!({ "host": "storefront.example" })
    );
    assert_eq!(
        response.body["data"]["sfShop"]["brand"],
        json!({ "shortDescription": "Storefront brand" })
    );
    assert_eq!(
        response.body["data"]["localization"]["country"],
        json!({ "isoCode": "CA", "name": "Canada" })
    );
    assert_eq!(
        response.body["data"]["localization"]["language"],
        json!({ "isoCode": "FR", "name": "French", "endonymName": "français" })
    );
    assert_eq!(
        response.body["data"]["locations"]["edges"][0]["cursor"],
        json!("cursor-location-2")
    );
    assert_eq!(
        response.body["data"]["locations"]["pageInfo"],
        json!({
            "hasNextPage": true,
            "hasPreviousPage": false,
            "startCursor": "cursor-location-2",
            "endCursor": "cursor-location-2"
        })
    );
    assert_eq!(
        response.body["data"]["paymentSettings"],
        json!({
            "currencyCode": "CAD",
            "countryCode": "CA",
            "acceptedCardBrands": ["VISA", "MASTERCARD"]
        })
    );
    assert_eq!(
        response.body["data"]["publicApiVersions"],
        json!([{ "handle": "2026-04", "displayName": "2026-04", "supported": true }])
    );

    let observed = observed_requests.lock().unwrap();
    assert_eq!(observed.len(), 1);
    assert_eq!(observed[0].method, "POST");
    assert_eq!(observed[0].path, "/api/2026-04/graphql.json");
    assert_eq!(
        observed[0].headers.get("X-Shopify-Storefront-Access-Token"),
        Some(&"storefront-token".to_string())
    );
    let hydrate_body: Value = serde_json::from_str(&observed[0].body).unwrap();
    let hydrate_query = hydrate_body["query"].as_str().unwrap();
    assert!(hydrate_query.contains("StorefrontFirstSliceHydrateWithContext"));
    assert!(!hydrate_query.contains("contactInformation"));
    assert!(!hydrate_query.contains("legalNotice"));
    assert!(!hydrate_query.contains("termsOfSale"));
    assert_eq!(hydrate_body["variables"]["country"], json!("CA"));
    assert_eq!(hydrate_body["variables"]["language"], json!("FR"));
}

#[test]
fn storefront_first_slice_snapshot_returns_empty_non_null_collections_without_invented_context() {
    let mut proxy = configured_proxy(ReadMode::Snapshot, Some(UnsupportedMutationMode::Reject))
        .with_upstream_transport(|_| {
            panic!("snapshot Storefront first-slice reads should not call upstream")
        });

    let response = proxy.process_request(Request {
        method: "POST".to_string(),
        path: "/api/2026-04/graphql.json".to_string(),
        headers: Default::default(),
        body: json!({
            "query": r#"
                query StorefrontFirstSliceEmptyCollections {
                  locations(first: 2) {
                    nodes { id name }
                    pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
                  }
                  publicApiVersions { handle displayName supported }
                }
            "#,
            "variables": {}
        })
        .to_string(),
    });

    assert_eq!(response.status, 200);
    assert_eq!(response.body["data"]["publicApiVersions"], json!([]));
    assert_eq!(response.body["data"]["locations"]["nodes"], json!([]));
    assert_eq!(
        response.body["data"]["locations"]["pageInfo"],
        json!({
            "hasNextPage": false,
            "hasPreviousPage": false,
            "startCursor": null,
            "endCursor": null
        })
    );
}

#[test]
fn storefront_shop_can_observe_admin_hydrated_store_state_without_storefront_upstream() {
    let mut proxy = configured_proxy(
        ReadMode::LiveHybrid,
        Some(UnsupportedMutationMode::Passthrough),
    )
    .with_upstream_transport(|request| {
        if request.path.starts_with("/api/") {
            panic!("admin-backed Storefront shop selection should not call Storefront upstream");
        }
        Response {
            status: 200,
            headers: Default::default(),
            body: json!({
                "data": {
                    "shop": {
                        "id": "gid://shopify/Shop/admin-observed",
                        "name": "Admin observed shop",
                        "primaryDomain": {
                            "id": "gid://shopify/Domain/1",
                            "host": "admin-observed.example",
                            "url": "https://admin-observed.example",
                            "sslEnabled": true
                        },
                        "currencyCode": "USD",
                        "enabledPresentmentCurrencies": ["USD", "CAD"],
                        "paymentSettings": {
                            "supportedDigitalWallets": ["APPLE_PAY"]
                        },
                        "shopPolicies": [{
                            "id": "gid://shopify/ShopPolicy/privacy",
                            "title": "Privacy Policy",
                            "body": "Admin privacy body",
                            "type": "PRIVACY_POLICY",
                            "url": "https://admin-observed.example/policies/privacy-policy",
                            "createdAt": "2024-01-01T00:00:00Z",
                            "updatedAt": "2024-01-02T00:00:00Z"
                        }]
                    }
                }
            }),
        }
    });

    let admin = proxy.process_request(json_graphql_request(
        r#"
        query AdminShopHydrate {
          shop {
            id
            name
            primaryDomain { id host url sslEnabled }
            currencyCode
            enabledPresentmentCurrencies
            paymentSettings { supportedDigitalWallets }
            shopPolicies { id title body type url createdAt updatedAt }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(admin.status, 200);

    let storefront = proxy.process_request(Request {
        method: "POST".to_string(),
        path: "/api/2026-04/graphql.json".to_string(),
        headers: Default::default(),
        body: json!({
            "query": r#"
                query StorefrontAdminObservedShop {
                  shop {
                    id
                    name
                    primaryDomain { host url sslEnabled }
                    paymentSettings { currencyCode enabledPresentmentCurrencies supportedDigitalWallets }
                    privacyPolicy { id title body handle url }
                  }
                }
            "#,
            "variables": {}
        })
        .to_string(),
    });

    assert_eq!(storefront.status, 200);
    assert_eq!(
        storefront.body["data"]["shop"],
        json!({
            "id": "gid://shopify/Shop/admin-observed",
            "name": "Admin observed shop",
            "primaryDomain": {
                "host": "admin-observed.example",
                "url": "https://admin-observed.example",
                "sslEnabled": true
            },
            "paymentSettings": {
                "currencyCode": "USD",
                "enabledPresentmentCurrencies": ["USD", "CAD"],
                "supportedDigitalWallets": ["APPLE_PAY"]
            },
            "privacyPolicy": {
                "id": "gid://shopify/ShopPolicy/privacy",
                "title": "Privacy Policy",
                "body": "Admin privacy body",
                "handle": "privacy-policy",
                "url": "https://admin-observed.example/policies/privacy-policy"
            }
        })
    );
}

#[test]
fn storefront_metaobjects_resolve_public_active_admin_staged_entries() {
    let mut proxy = configured_proxy(
        ReadMode::LiveHybrid,
        Some(UnsupportedMutationMode::Passthrough),
    )
    .with_upstream_transport(|_| {
        panic!("staged Storefront custom-data reads should stay local in live-hybrid mode")
    });

    stage_storefront_metaobject_definition(
        &mut proxy,
        "codex_storefront_public",
        "PUBLIC_READ",
        true,
    );
    let entry = stage_storefront_metaobject(
        &mut proxy,
        "codex_storefront_public",
        "visible-entry",
        "ACTIVE",
        "Visible Storefront Entry",
    );

    let response = proxy.process_request(storefront_graphql_request(
        r#"
        query StorefrontPublicMetaobjects($handle: MetaobjectHandleInput!) {
          byHandle: metaobject(handle: $handle) {
            ...StorefrontMetaobjectFields
            title: field(key: "title") { key type value }
          }
          entries: metaobjects(type: "codex_storefront_public", first: 2, sortKey: "updated_at") {
            edges { cursor node { ...StorefrontMetaobjectFields } }
            nodes { ...StorefrontMetaobjectFields }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
        }

        fragment StorefrontMetaobjectFields on Metaobject {
          id
          handle
          type
          updatedAt
          fields { key type value }
        }
        "#,
        json!({
            "handle": {
                "type": "codex_storefront_public",
                "handle": "visible-entry"
            }
        }),
    ));

    assert_eq!(response.status, 200);
    let expected_node = json!({
        "id": entry["id"],
        "handle": "visible-entry",
        "type": "codex_storefront_public",
        "updatedAt": entry["updatedAt"],
        "fields": [
            { "key": "body", "type": "multi_line_text_field", "value": "Body for Visible Storefront Entry" },
            { "key": "title", "type": "single_line_text_field", "value": "Visible Storefront Entry" }
        ]
    });
    assert_eq!(response.body["data"]["byHandle"]["id"], entry["id"]);
    assert_eq!(
        response.body["data"]["byHandle"]["title"],
        json!({ "key": "title", "type": "single_line_text_field", "value": "Visible Storefront Entry" })
    );
    assert_eq!(
        response.body["data"]["entries"]["nodes"],
        json!([expected_node])
    );
    assert_eq!(
        response.body["data"]["entries"]["edges"][0]["node"]["handle"],
        json!("visible-entry")
    );
    assert_eq!(
        response.body["data"]["entries"]["pageInfo"]["hasNextPage"],
        json!(false)
    );
}

#[test]
fn storefront_metaobject_fields_resolve_visible_nested_references() {
    let mut proxy = configured_proxy(ReadMode::Snapshot, Some(UnsupportedMutationMode::Reject))
        .with_upstream_transport(|_| panic!("snapshot Storefront reads should stay local"));

    stage_storefront_metaobject_definition(
        &mut proxy,
        "codex_storefront_reference_target",
        "PUBLIC_READ",
        true,
    );
    let visible_target = stage_storefront_metaobject(
        &mut proxy,
        "codex_storefront_reference_target",
        "visible-target",
        "ACTIVE",
        "Visible Target",
    );
    let draft_target = stage_storefront_metaobject(
        &mut proxy,
        "codex_storefront_reference_target",
        "draft-target",
        "DRAFT",
        "Draft Target",
    );
    stage_storefront_reference_definition(&mut proxy, "codex_storefront_reference_source");
    stage_storefront_reference_metaobject(
        &mut proxy,
        visible_target["id"].as_str().unwrap(),
        draft_target["id"].as_str().unwrap(),
    );

    let response = proxy.process_request(storefront_graphql_request(
        r#"
        query StorefrontReferenceFields {
          source: metaobject(handle: {
            type: "codex_storefront_reference_source",
            handle: "source-entry"
          }) {
            featured: field(key: "featured") {
              key
              type
              value
              reference { ... on Metaobject { handle type } }
            }
            related: field(key: "related") {
              key
              type
              references(first: 5) {
                nodes { ... on Metaobject { handle type } }
                pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
              }
            }
          }
        }
        "#,
        json!({}),
    ));

    assert_eq!(response.status, 200);
    assert_eq!(
        response.body["data"]["source"]["featured"]["reference"],
        json!({
            "handle": "visible-target",
            "type": "codex_storefront_reference_target"
        })
    );
    assert_eq!(
        response.body["data"]["source"]["related"]["references"]["nodes"],
        json!([{
            "handle": "visible-target",
            "type": "codex_storefront_reference_target"
        }])
    );
    assert_eq!(
        response.body["data"]["source"]["related"]["references"]["pageInfo"],
        json!({
            "hasNextPage": false,
            "hasPreviousPage": false,
            "startCursor": visible_target["id"],
            "endCursor": visible_target["id"]
        })
    );
}

#[test]
fn storefront_metaobjects_hide_non_public_draft_and_deleted_entries() {
    let mut proxy = configured_proxy(ReadMode::Snapshot, Some(UnsupportedMutationMode::Reject))
        .with_upstream_transport(|_| panic!("snapshot Storefront reads should stay local"));

    stage_storefront_metaobject_definition(
        &mut proxy,
        "codex_storefront_public",
        "PUBLIC_READ",
        true,
    );
    let active = stage_storefront_metaobject(
        &mut proxy,
        "codex_storefront_public",
        "active-entry",
        "ACTIVE",
        "Active Entry",
    );
    stage_storefront_metaobject(
        &mut proxy,
        "codex_storefront_public",
        "draft-entry",
        "DRAFT",
        "Draft Entry",
    );
    stage_storefront_metaobject_definition(&mut proxy, "codex_storefront_private", "NONE", true);
    stage_storefront_metaobject(
        &mut proxy,
        "codex_storefront_private",
        "private-entry",
        "ACTIVE",
        "Private Entry",
    );

    let before_delete = proxy.process_request(storefront_graphql_request(
        r#"
        query StorefrontVisibility {
          active: metaobject(handle: { type: "codex_storefront_public", handle: "active-entry" }) { id handle }
          draft: metaobject(handle: { type: "codex_storefront_public", handle: "draft-entry" }) { id handle }
          privateEntry: metaobject(handle: { type: "codex_storefront_private", handle: "private-entry" }) { id handle }
          publicEntries: metaobjects(type: "codex_storefront_public", first: 10) { nodes { handle } }
          privateEntries: metaobjects(type: "codex_storefront_private", first: 10) { nodes { handle } }
        }
        "#,
        json!({}),
    ));

    assert_eq!(before_delete.status, 200);
    assert_eq!(
        before_delete.body["data"]["active"]["handle"],
        json!("active-entry")
    );
    assert_eq!(before_delete.body["data"]["draft"], Value::Null);
    assert_eq!(before_delete.body["data"]["privateEntry"], Value::Null);
    assert_eq!(
        before_delete.body["data"]["publicEntries"]["nodes"],
        json!([{ "handle": "active-entry" }])
    );
    assert_eq!(
        before_delete.body["data"]["privateEntries"]["nodes"],
        json!([])
    );

    let delete = proxy.process_request(json_graphql_request(
        r#"
        mutation DeleteMetaobject($id: ID!) {
          metaobjectDelete(id: $id) {
            deletedId
            userErrors { field message code }
          }
        }
        "#,
        json!({ "id": active["id"] }),
    ));
    assert_eq!(delete.status, 200);
    assert_eq!(
        delete.body["data"]["metaobjectDelete"]["userErrors"],
        json!([])
    );

    let after_delete = proxy.process_request(storefront_graphql_request(
        r#"
        query StorefrontAfterDelete {
          active: metaobject(handle: { type: "codex_storefront_public", handle: "active-entry" }) { id handle }
          publicEntries: metaobjects(type: "codex_storefront_public", first: 10) { nodes { handle } }
        }
        "#,
        json!({}),
    ));

    assert_eq!(after_delete.status, 200);
    assert_eq!(after_delete.body["data"]["active"], Value::Null);
    assert_eq!(
        after_delete.body["data"]["publicEntries"]["nodes"],
        json!([])
    );
}

#[test]
fn storefront_shop_metafields_require_storefront_definition_access() {
    let mut proxy = configured_proxy(ReadMode::Snapshot, Some(UnsupportedMutationMode::Reject))
        .with_upstream_transport(|_| panic!("snapshot Storefront reads should stay local"));
    restore_state_with(&mut proxy, |state| {
        state["baseState"]["shop"] = json!({
            "id": "gid://shopify/Shop/storefront-metafields",
            "name": "Storefront metafields shop"
        });
    });

    stage_metafield_definition(
        &mut proxy,
        "SHOP",
        "custom",
        "visible",
        "single_line_text_field",
        "PUBLIC_READ",
    );
    stage_metafield_definition(
        &mut proxy,
        "SHOP",
        "custom",
        "hidden",
        "single_line_text_field",
        "NONE",
    );
    stage_metafields_set(
        &mut proxy,
        "gid://shopify/Shop/storefront-metafields",
        json!([
            {
                "namespace": "custom",
                "key": "visible",
                "type": "single_line_text_field",
                "value": "Visible tagline"
            },
            {
                "namespace": "custom",
                "key": "hidden",
                "type": "single_line_text_field",
                "value": "Hidden tagline"
            }
        ]),
    );

    let response = proxy.process_request(storefront_graphql_request(
        r#"
        query StorefrontShopMetafields {
          shop {
            name
            visible: metafield(namespace: "custom", key: "visible") {
              namespace
              key
              type
              value
              list
              description
            }
            hidden: metafield(namespace: "custom", key: "hidden") { key value }
            selected: metafields(identifiers: [
              { namespace: "custom", key: "visible" },
              { namespace: "custom", key: "hidden" },
              { namespace: "custom", key: "missing" }
            ]) {
              key
              value
            }
          }
        }
        "#,
        json!({}),
    ));

    assert_eq!(response.status, 200);
    assert_eq!(
        response.body["data"]["shop"]["visible"],
        json!({
            "namespace": "custom",
            "key": "visible",
            "type": "single_line_text_field",
            "value": "Visible tagline",
            "list": false,
            "description": null
        })
    );
    assert_eq!(response.body["data"]["shop"]["hidden"], Value::Null);
    assert_eq!(
        response.body["data"]["shop"]["selected"],
        json!([{ "key": "visible", "value": "Visible tagline" }, null, null])
    );
}

#[test]
fn storefront_shop_metafields_use_staged_shop_owner_without_hydration() {
    let mut proxy = configured_proxy(
        ReadMode::LiveHybrid,
        Some(UnsupportedMutationMode::Passthrough),
    )
    .with_upstream_transport(|request| {
        let body: Value = serde_json::from_str(&request.body).expect("upstream JSON body");
        match body["operationName"].as_str().unwrap_or_default() {
            "MetafieldDefinitionHydrateByIdentifier" => Response {
                status: 200,
                headers: Default::default(),
                body: json!({ "data": { "metafieldDefinition": null } }),
            },
            "MetafieldDefinitionsHydrateResourceScope" => Response {
                status: 200,
                headers: Default::default(),
                body: json!({
                    "data": {
                        "metafieldDefinitions": {
                            "nodes": [],
                            "pageInfo": { "hasNextPage": false, "endCursor": null }
                        }
                    }
                }),
            },
            _ => Response {
                status: 502,
                headers: Default::default(),
                body: json!({ "errors": [{ "message": "No setup hydration configured" }] }),
            },
        }
    });

    stage_metafield_definition(
        &mut proxy,
        "SHOP",
        "custom",
        "visible",
        "single_line_text_field",
        "PUBLIC_READ",
    );
    stage_metafield_definition(
        &mut proxy,
        "SHOP",
        "custom",
        "hidden",
        "single_line_text_field",
        "NONE",
    );
    stage_metafields_set(
        &mut proxy,
        "gid://shopify/Shop/storefront-metafields-no-hydrate",
        json!([
            {
                "namespace": "custom",
                "key": "visible",
                "type": "single_line_text_field",
                "value": "Visible tagline"
            },
            {
                "namespace": "custom",
                "key": "hidden",
                "type": "single_line_text_field",
                "value": "Hidden tagline"
            }
        ]),
    );
    let mut proxy = proxy.with_upstream_transport(|_| {
        panic!("staged Storefront shop metafields should not require first-slice hydration")
    });

    let response = proxy.process_request(storefront_graphql_request(
        r#"
        query StorefrontShopMetafieldsWithoutHydration {
          shop {
            visible: metafield(namespace: "custom", key: "visible") {
              namespace
              key
              type
              value
              list
            }
            hidden: metafield(namespace: "custom", key: "hidden") { key value }
            selected: metafields(identifiers: [
              { namespace: "custom", key: "visible" },
              { namespace: "custom", key: "hidden" },
              { namespace: "custom", key: "missing" }
            ]) {
              namespace
              key
              type
              value
            }
          }
        }
        "#,
        json!({}),
    ));

    assert_eq!(response.status, 200);
    assert_eq!(
        response.body["data"]["shop"],
        json!({
            "visible": {
                "namespace": "custom",
                "key": "visible",
                "type": "single_line_text_field",
                "value": "Visible tagline",
                "list": false
            },
            "hidden": null,
            "selected": [
                {
                    "namespace": "custom",
                    "key": "visible",
                    "type": "single_line_text_field",
                    "value": "Visible tagline"
                },
                null,
                null
            ]
        })
    );
}

#[test]
fn storefront_graphql_passthrough_does_not_enter_admin_staging_or_commit() {
    let observed_requests = Arc::new(Mutex::new(Vec::<Request>::new()));
    let observed_for_proxy = Arc::clone(&observed_requests);
    let commit_requests = Arc::new(Mutex::new(Vec::<Request>::new()));
    let commit_for_proxy = Arc::clone(&commit_requests);
    let mut proxy = configured_proxy(
        ReadMode::LiveHybrid,
        Some(UnsupportedMutationMode::Passthrough),
    )
    .with_upstream_transport(move |request| {
        observed_for_proxy.lock().unwrap().push(request);
        Response {
            status: 200,
            headers: Default::default(),
            body: json!({ "data": { "cartBillingAddressUpdate": { "cart": { "id": "gid://shopify/Cart/1" } } } }),
        }
    })
    .with_commit_transport(move |request| {
        commit_for_proxy.lock().unwrap().push(request);
        Response {
            status: 200,
            headers: Default::default(),
            body: json!({ "data": { "unexpected": true } }),
        }
    });

    let body = json!({
        "query": "mutation StorefrontMutationShape { cartBillingAddressUpdate(cartId: \"gid://shopify/Cart/1\", billingAddress: null) { cart { id } } }",
        "variables": {}
    })
    .to_string();
    let response = proxy.process_request(Request {
        method: "POST".to_string(),
        path: "/api/2026-04/graphql.json".to_string(),
        headers: Default::default(),
        body: body.clone(),
    });
    assert_eq!(response.status, 200);

    let observed = observed_requests.lock().unwrap();
    assert_eq!(observed.len(), 1);
    assert_eq!(observed[0].path, "/api/2026-04/graphql.json");
    assert_eq!(observed[0].body, body);

    let log = proxy.process_request(request_with_body("GET", "/__meta/log", ""));
    assert_eq!(log.status, 200);
    assert_eq!(log.body["entries"].as_array().unwrap().len(), 1);
    assert_eq!(log.body["entries"][0]["apiSurface"], json!("storefront"));
    assert_eq!(log.body["entries"][0]["status"], json!("proxied"));
    assert_eq!(
        log.body["entries"][0]["interpreted"]["capability"]["execution"],
        json!("passthrough")
    );

    let commit = proxy.process_request(request_with_body("POST", "/__meta/commit", ""));
    assert_eq!(commit.status, 200);
    assert_eq!(commit.body["committed"], json!(0));
    assert_eq!(commit.body["attempts"], json!([]));
    assert!(commit_requests.lock().unwrap().is_empty());
}

#[test]
fn storefront_content_roots_project_staged_admin_content() {
    let mut proxy = configured_proxy(
        ReadMode::LiveHybrid,
        Some(UnsupportedMutationMode::Passthrough),
    )
    .with_upstream_transport(|request| {
        if request.path.starts_with("/api/") {
            panic!("staged Storefront content should not call Storefront upstream");
        }
        Response {
            status: 599,
            headers: Default::default(),
            body: json!({ "errors": [{ "message": "unexpected upstream call" }] }),
        }
    });

    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation StageStorefrontContent($blog: BlogCreateInput!, $page: PageCreateInput!) {
          madeBlog: blogCreate(blog: $blog) { blog { id handle title } userErrors { field message code } }
          madePage: pageCreate(page: $page) { page { id handle title body bodySummary isPublished createdAt updatedAt } userErrors { field message code } }
        }
        "#,
        json!({
            "blog": { "title": "Storefront Content Blog", "handle": "storefront-content-blog" },
            "page": { "title": "Storefront Content Page", "handle": "storefront-content-page", "body": "<p>Visible page body</p>", "isPublished": true }
        }),
    ));
    assert_eq!(create.status, 200);
    assert_eq!(create.body["data"]["madeBlog"]["userErrors"], json!([]));
    assert_eq!(create.body["data"]["madePage"]["userErrors"], json!([]));
    let blog_id = create.body["data"]["madeBlog"]["blog"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let page_id = create.body["data"]["madePage"]["page"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let article = proxy.process_request(json_graphql_request(
        r#"
        mutation StageStorefrontArticle($article: ArticleCreateInput!) {
          madeArticle: articleCreate(article: $article) {
            article { id handle title body summary tags isPublished publishedAt author { name } blog { id handle title } }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "article": {
            "title": "Storefront Content Article",
            "handle": "storefront-content-article",
            "body": "<p>Visible article body</p>",
            "summary": "Visible article summary",
            "tags": ["sf-content", "read-after-write"],
            "author": { "name": "Storefront Author" },
            "blogId": blog_id,
            "isPublished": true
        }}),
    ));
    assert_eq!(article.status, 200);
    assert_eq!(article.body["data"]["madeArticle"]["userErrors"], json!([]));
    let article_id = article.body["data"]["madeArticle"]["article"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let storefront = proxy.process_request(storefront_graphql_request(
        r#"
        query StorefrontContentRead($blogHandle: String!, $pageId: ID!, $pageHandle: String!, $articleId: ID!, $articleHandle: String!) {
          byId: article(id: $articleId) {
            ...ArticleFields
            blog {
              id
              handle
              title
              articleByHandle(handle: $articleHandle) { id title handle }
              articles(first: 2, query: "tag:sf-content", sortKey: TITLE) {
                nodes { id title handle }
                pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
              }
              authors { name }
            }
          }
          allArticles: articles(first: 1, query: "author:Storefront", sortKey: TITLE) {
            edges { cursor node { id title handle } }
            nodes { id title }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
          byBlog: blog(handle: $blogHandle) { id handle title }
          oldBlog: blogByHandle(handle: $blogHandle) { id handle title }
          blogs(first: 2, query: "handle:storefront-content-blog") { nodes { id handle title } }
          byPage: page(id: $pageId) { id handle title body bodySummary seo { title description } }
          oldPage: pageByHandle(handle: $pageHandle) { id handle title }
          pages(first: 2, query: "title:Storefront") { nodes { id handle title } }
          sitemap(type: PAGE) {
            pagesCount { count precision }
            resources(page: 1) { hasNextPage items { handle updatedAt ... on SitemapResource { title } } }
          }
        }

        fragment ArticleFields on Article {
          id
          handle
          title
          content
          contentHtml
          excerpt
          excerptHtml
          tags
          publishedAt
          author { name }
          authorV2 { name }
          seo { title description }
        }
        "#,
        json!({
            "blogHandle": "storefront-content-blog",
            "pageId": page_id,
            "pageHandle": "storefront-content-page",
            "articleId": article_id,
            "articleHandle": "storefront-content-article"
        }),
    ));

    assert_eq!(storefront.status, 200);
    assert_eq!(storefront.body["errors"], Value::Null);
    assert_eq!(storefront.body["data"]["byId"]["id"], json!(article_id));
    assert_eq!(
        storefront.body["data"]["byId"]["content"],
        json!("Visible article body")
    );
    assert_eq!(
        storefront.body["data"]["byId"]["contentHtml"],
        json!("<p>Visible article body</p>")
    );
    assert_eq!(
        storefront.body["data"]["byId"]["excerpt"],
        json!("Visible article summary")
    );
    assert_eq!(
        storefront.body["data"]["byId"]["blog"]["articleByHandle"]["id"],
        json!(article_id)
    );
    assert_eq!(
        storefront.body["data"]["allArticles"]["nodes"],
        json!([{ "id": article_id, "title": "Storefront Content Article" }])
    );
    assert_eq!(storefront.body["data"]["byBlog"]["id"], json!(blog_id));
    assert_eq!(storefront.body["data"]["oldBlog"]["id"], json!(blog_id));
    assert_eq!(
        storefront.body["data"]["blogs"]["nodes"][0]["handle"],
        json!("storefront-content-blog")
    );
    assert_eq!(storefront.body["data"]["byPage"]["id"], json!(page_id));
    assert_eq!(
        storefront.body["data"]["byPage"]["bodySummary"],
        json!("Visible page body")
    );
    assert_eq!(
        storefront.body["data"]["oldPage"]["handle"],
        json!("storefront-content-page")
    );
    assert_eq!(
        storefront.body["data"]["sitemap"]["pagesCount"],
        json!({ "count": 1, "precision": "EXACT" })
    );
    assert_eq!(
        storefront.body["data"]["sitemap"]["resources"]["items"][0]["handle"],
        json!("storefront-content-page")
    );
}

#[test]
fn storefront_content_visibility_delete_and_redirect_boundaries_use_staged_state() {
    let mut proxy = configured_proxy(ReadMode::Snapshot, Some(UnsupportedMutationMode::Reject))
        .with_upstream_transport(|_| panic!("snapshot Storefront content should stay local"));

    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation StageStorefrontVisibility {
          blogCreate(blog: { title: "Visibility Blog" }) { blog { id } userErrors { field message code } }
          visible: pageCreate(page: { title: "Visible Storefront Page", body: "<p>visible</p>", isPublished: true }) { page { id handle } userErrors { field message code } }
          hidden: pageCreate(page: { title: "Hidden Storefront Page", body: "<p>hidden</p>", isPublished: false }) { page { id handle } userErrors { field message code } }
        }
        "#,
        json!({}),
    ));
    assert_eq!(create.status, 200);
    let visible_page_id = create.body["data"]["visible"]["page"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let hidden_page_handle = create.body["data"]["hidden"]["page"]["handle"]
        .as_str()
        .unwrap()
        .to_string();

    let before_delete = proxy.process_request(storefront_graphql_request(
        r#"
        query StorefrontVisibility($visibleId: ID!, $hiddenHandle: String!) {
          visible: page(id: $visibleId) { id handle title }
          hidden: pageByHandle(handle: $hiddenHandle) { id handle title }
          pages(first: 10) { nodes { id title } }
        }
        "#,
        json!({ "visibleId": visible_page_id, "hiddenHandle": hidden_page_handle }),
    ));
    assert_eq!(
        before_delete.body["data"]["visible"]["id"],
        json!(visible_page_id)
    );
    assert_eq!(before_delete.body["data"]["hidden"], Value::Null);
    assert_eq!(
        before_delete.body["data"]["pages"]["nodes"],
        json!([{ "id": visible_page_id, "title": "Visible Storefront Page" }])
    );

    let delete = proxy.process_request(json_graphql_request(
        r#"
        mutation DeleteVisiblePage($id: ID!) {
          pageDelete(id: $id) { deletedPageId userErrors { field message code } }
        }
        "#,
        json!({ "id": visible_page_id }),
    ));
    assert_eq!(
        delete.body["data"]["pageDelete"]["deletedPageId"],
        json!(visible_page_id)
    );

    let after_delete = proxy.process_request(storefront_graphql_request(
        r#"
        query StorefrontVisibilityAfterDelete($visibleId: ID!) {
          visible: page(id: $visibleId) { id handle title }
          pages(first: 10) { nodes { id title } pageInfo { hasNextPage hasPreviousPage } }
          urlRedirects(first: 2, query: "path:/pages/old") {
            nodes { id path target }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
        }
        "#,
        json!({ "visibleId": visible_page_id }),
    ));
    assert_eq!(after_delete.body["data"]["visible"], Value::Null);
    assert_eq!(after_delete.body["data"]["pages"]["nodes"], json!([]));
    assert_eq!(
        after_delete.body["data"]["urlRedirects"]["nodes"],
        json!([])
    );
}

#[test]
fn storefront_menu_projects_restored_captured_base_state_without_snapshot_fabrication() {
    let mut proxy = configured_proxy(ReadMode::Snapshot, Some(UnsupportedMutationMode::Reject))
        .with_upstream_transport(|_| panic!("snapshot Storefront menu should not call upstream"));

    let empty = proxy.process_request(storefront_graphql_request(
        r#"
        query MissingMenu {
          menu(handle: "main-menu") { id handle title itemsCount items { id title } }
        }
        "#,
        json!({}),
    ));
    assert_eq!(empty.status, 200);
    assert_eq!(empty.body["data"]["menu"], Value::Null);

    restore_state_with(&mut proxy, |state| {
        state["baseState"]["storefrontMenus"] = json!({
            "gid://shopify/Menu/main": {
                "id": "gid://shopify/Menu/main",
                "handle": "main-menu",
                "title": "Main menu",
                "itemsCount": 1,
                "items": [{
                    "id": "gid://shopify/MenuItem/main-1",
                    "title": "Visible page",
                    "type": "PAGE",
                    "url": "https://example.myshopify.com/pages/visible-page",
                    "resourceId": "gid://shopify/Page/visible",
                    "tags": [],
                    "items": [],
                    "resource": {
                        "__typename": "Page",
                        "id": "gid://shopify/Page/visible",
                        "handle": "visible-page",
                        "title": "Visible page"
                    }
                }]
            }
        });
        state["baseState"]["storefrontMenuOrder"] = json!(["gid://shopify/Menu/main"]);
    });

    let response = proxy.process_request(storefront_graphql_request(
        r#"
        query CapturedMenu {
          menu(handle: "main-menu") {
            id
            handle
            title
            itemsCount
            items {
              id
              title
              type
              url
              resourceId
              tags
              items { id title }
              resource { __typename ... on Page { id handle title } }
            }
          }
        }
        "#,
        json!({}),
    ));

    assert_eq!(response.status, 200);
    assert_eq!(response.body["data"]["menu"]["handle"], json!("main-menu"));
    assert_eq!(response.body["data"]["menu"]["itemsCount"], json!(1));
    assert_eq!(
        response.body["data"]["menu"]["items"][0]["resource"],
        json!({
            "__typename": "Page",
            "id": "gid://shopify/Page/visible",
            "handle": "visible-page",
            "title": "Visible page"
        })
    );
}

fn create_storefront_customer_token(
    proxy: &mut DraftProxy,
    email: &str,
    first_name: &str,
    last_name: &str,
) -> (String, String) {
    let create = proxy.process_request(storefront_graphql_request(
        r#"
        mutation CreateStorefrontCustomer($input: CustomerCreateInput!) {
          customerCreate(input: $input) {
            customer { id email }
            customerUserErrors { field message code }
          }
        }
        "#,
        json!({
            "input": {
                "email": email,
                "password": "CodexPass123!",
                "firstName": first_name,
                "lastName": last_name
            }
        }),
    ));
    assert_eq!(create.status, 200, "{}", create.body);
    assert_eq!(
        create.body["data"]["customerCreate"]["customerUserErrors"],
        json!([])
    );
    let customer_id = create.body["data"]["customerCreate"]["customer"]["id"]
        .as_str()
        .expect("customer id")
        .to_string();

    let token = proxy.process_request(storefront_graphql_request(
        r#"
        mutation CreateStorefrontCustomerToken($input: CustomerAccessTokenCreateInput!) {
          customerAccessTokenCreate(input: $input) {
            customerAccessToken { accessToken expiresAt }
            customerUserErrors { field message code }
          }
        }
        "#,
        json!({
            "input": {
                "email": email,
                "password": "CodexPass123!"
            }
        }),
    ));
    assert_eq!(token.status, 200, "{}", token.body);
    assert_eq!(
        token.body["data"]["customerAccessTokenCreate"]["customerUserErrors"],
        json!([])
    );
    let access_token = token.body["data"]["customerAccessTokenCreate"]["customerAccessToken"]
        ["accessToken"]
        .as_str()
        .expect("customer access token")
        .to_string();
    (customer_id, access_token)
}

fn stage_storefront_metaobject_definition(
    proxy: &mut DraftProxy,
    meta_type: &str,
    storefront_access: &str,
    publishable_enabled: bool,
) -> Value {
    let response = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateDefinition($definition: MetaobjectDefinitionCreateInput!) {
          metaobjectDefinitionCreate(definition: $definition) {
            metaobjectDefinition {
              id
              type
              access { storefront }
              capabilities { publishable { enabled } }
              fieldDefinitions { key type { name } required }
            }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "definition": {
                "type": meta_type,
                "name": meta_type.replace('_', " "),
                "access": { "storefront": storefront_access },
                "capabilities": { "publishable": { "enabled": publishable_enabled } },
                "displayNameKey": "title",
                "fieldDefinitions": [
                    {
                        "key": "title",
                        "name": "Title",
                        "type": "single_line_text_field",
                        "required": true
                    },
                    {
                        "key": "body",
                        "name": "Body",
                        "type": "multi_line_text_field",
                        "required": false
                    }
                ]
            }
        }),
    ));
    assert_eq!(response.status, 200);
    assert_eq!(
        response.body["data"]["metaobjectDefinitionCreate"]["userErrors"],
        json!([])
    );
    response.body["data"]["metaobjectDefinitionCreate"]["metaobjectDefinition"].clone()
}

fn stage_storefront_metaobject(
    proxy: &mut DraftProxy,
    meta_type: &str,
    handle: &str,
    status: &str,
    title: &str,
) -> Value {
    let response = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateMetaobject($metaobject: MetaobjectCreateInput!) {
          metaobjectCreate(metaobject: $metaobject) {
            metaobject {
              id
              handle
              type
              updatedAt
              capabilities { publishable { status } }
              fields { key type value jsonValue }
            }
            userErrors { field message code elementKey elementIndex }
          }
        }
        "#,
        json!({
            "metaobject": {
                "type": meta_type,
                "handle": handle,
                "capabilities": { "publishable": { "status": status } },
                "fields": [
                    { "key": "title", "value": title },
                    { "key": "body", "value": format!("Body for {title}") }
                ]
            }
        }),
    ));
    assert_eq!(response.status, 200);
    assert_eq!(
        response.body["data"]["metaobjectCreate"]["userErrors"],
        json!([])
    );
    response.body["data"]["metaobjectCreate"]["metaobject"].clone()
}

fn stage_storefront_reference_definition(proxy: &mut DraftProxy, meta_type: &str) -> Value {
    let response = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateReferenceDefinition($definition: MetaobjectDefinitionCreateInput!) {
          metaobjectDefinitionCreate(definition: $definition) {
            metaobjectDefinition {
              id
              type
              access { storefront }
              fieldDefinitions { key type { name } }
            }
            userErrors { field message code elementKey elementIndex }
          }
        }
        "#,
        json!({
            "definition": {
                "type": meta_type,
                "name": "codex storefront reference source",
                "access": { "storefront": "PUBLIC_READ" },
                "capabilities": { "publishable": { "enabled": true } },
                "displayNameKey": "title",
                "fieldDefinitions": [
                    {
                        "key": "title",
                        "name": "Title",
                        "type": "single_line_text_field",
                        "required": true
                    },
                    {
                        "key": "featured",
                        "name": "Featured",
                        "type": "metaobject_reference",
                        "required": false
                    },
                    {
                        "key": "related",
                        "name": "Related",
                        "type": "list.metaobject_reference",
                        "required": false
                    }
                ]
            }
        }),
    ));
    assert_eq!(response.status, 200);
    assert_eq!(
        response.body["data"]["metaobjectDefinitionCreate"]["userErrors"],
        json!([])
    );
    response.body["data"]["metaobjectDefinitionCreate"]["metaobjectDefinition"].clone()
}

fn stage_storefront_reference_metaobject(
    proxy: &mut DraftProxy,
    visible_target_id: &str,
    draft_target_id: &str,
) -> Value {
    let response = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateReferenceMetaobject($metaobject: MetaobjectCreateInput!) {
          metaobjectCreate(metaobject: $metaobject) {
            metaobject {
              id
              handle
              type
              fields { key type value jsonValue }
            }
            userErrors { field message code elementKey elementIndex }
          }
        }
        "#,
        json!({
            "metaobject": {
                "type": "codex_storefront_reference_source",
                "handle": "source-entry",
                "capabilities": { "publishable": { "status": "ACTIVE" } },
                "fields": [
                    { "key": "title", "value": "Source Entry" },
                    { "key": "featured", "value": visible_target_id },
                    { "key": "related", "value": json!([visible_target_id, draft_target_id]).to_string() }
                ]
            }
        }),
    ));
    assert_eq!(response.status, 200);
    assert_eq!(
        response.body["data"]["metaobjectCreate"]["userErrors"],
        json!([])
    );
    response.body["data"]["metaobjectCreate"]["metaobject"].clone()
}

fn stage_metafield_definition(
    proxy: &mut DraftProxy,
    owner_type: &str,
    namespace: &str,
    key: &str,
    field_type: &str,
    storefront_access: &str,
) -> Value {
    let response = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateMetafieldDefinition($definition: MetafieldDefinitionInput!) {
          metafieldDefinitionCreate(definition: $definition) {
            createdDefinition {
              id
              ownerType
              namespace
              key
              type { name }
              access { storefront }
            }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "definition": {
                "ownerType": owner_type,
                "namespace": namespace,
                "key": key,
                "name": key.replace('_', " "),
                "type": field_type,
                "access": { "storefront": storefront_access }
            }
        }),
    ));
    assert_eq!(response.status, 200);
    assert_eq!(
        response.body["data"]["metafieldDefinitionCreate"]["userErrors"],
        json!([])
    );
    response.body["data"]["metafieldDefinitionCreate"]["createdDefinition"].clone()
}

fn stage_metafields_set(proxy: &mut DraftProxy, owner_id: &str, metafields: Value) -> Value {
    let metafields = metafields
        .as_array()
        .expect("test metafields must be an array")
        .iter()
        .map(|metafield| {
            let mut metafield = metafield.clone();
            metafield["ownerId"] = json!(owner_id);
            metafield
        })
        .collect::<Vec<_>>();
    let response = proxy.process_request(json_graphql_request(
        r#"
        mutation SetMetafields($metafields: [MetafieldsSetInput!]!) {
          metafieldsSet(metafields: $metafields) {
            metafields { id namespace key type value }
            userErrors { field message code elementIndex }
          }
        }
        "#,
        json!({ "metafields": metafields }),
    ));
    assert_eq!(response.status, 200);
    assert_eq!(
        response.body["data"]["metafieldsSet"]["userErrors"],
        json!([])
    );
    response.body["data"]["metafieldsSet"]["metafields"].clone()
}

#[test]
fn storefront_collections_project_shared_admin_lifecycle_and_product_connections() {
    let publication_id = "gid://shopify/Publication/storefront-collections";
    let mut alpha = storefront_product_fixture(
        "gid://shopify/Product/storefront-collection-alpha",
        "Alpha Collection Product",
        "alpha-collection-product",
        Some(publication_id),
    );
    alpha.vendor = "Hermes North".to_string();
    alpha.tags = vec!["alpha".to_string(), "storefront-collections".to_string()];
    let mut beta = storefront_product_fixture(
        "gid://shopify/Product/storefront-collection-beta",
        "Beta Collection Product",
        "beta-collection-product",
        Some(publication_id),
    );
    beta.vendor = "Hermes South".to_string();
    beta.tags = vec!["beta".to_string(), "storefront-collections".to_string()];
    beta.total_inventory = 0;
    let mut gamma = storefront_product_fixture(
        "gid://shopify/Product/storefront-collection-gamma",
        "Gamma Collection Product",
        "gamma-collection-product",
        Some(publication_id),
    );
    gamma.vendor = "Hermes North".to_string();
    gamma.tags = vec!["gamma".to_string(), "storefront-collections".to_string()];

    let mut proxy = configured_proxy(ReadMode::Snapshot, Some(UnsupportedMutationMode::Reject))
        .with_base_products(vec![alpha, beta, gamma])
        .with_upstream_transport(|_| {
            panic!("snapshot Storefront collection reads should not call upstream")
        });
    restore_storefront_current_publication(&mut proxy, publication_id);
    stage_metafield_definition(
        &mut proxy,
        "COLLECTION",
        "storefront_collections",
        "visible",
        "single_line_text_field",
        "PUBLIC_READ",
    );

    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateStorefrontCollections($primary: CollectionInput!, $secondary: CollectionInput!) {
          primary: collectionCreate(input: $primary) {
            collection { id title handle }
            userErrors { field message }
          }
          secondary: collectionCreate(input: $secondary) {
            collection { id title handle }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "primary": {
                "title": "Storefront Collection Alpha",
                "handle": "storefront-collection-alpha",
                "descriptionHtml": "<p>Storefront collection description</p>",
                "sortOrder": "MANUAL",
                "products": [
                    "gid://shopify/Product/storefront-collection-alpha",
                    "gid://shopify/Product/storefront-collection-beta",
                    "gid://shopify/Product/storefront-collection-gamma"
                ],
                "image": {
                    "src": "https://placehold.co/64x64/png",
                    "altText": "Storefront collection image"
                },
                "seo": {
                    "title": "Storefront Collection SEO",
                    "description": "Storefront Collection SEO description"
                },
                "metafields": [{
                    "namespace": "storefront_collections",
                    "key": "visible",
                    "type": "single_line_text_field",
                    "value": "Visible collection metafield"
                }]
            },
            "secondary": {
                "title": "Storefront Collection Beta",
                "handle": "storefront-collection-beta",
                "sortOrder": "MANUAL"
            }
        }),
    ));
    assert_eq!(create.status, 200);
    assert_eq!(create.body["data"]["primary"]["userErrors"], json!([]));
    assert_eq!(create.body["data"]["secondary"]["userErrors"], json!([]));
    let primary_id = create.body["data"]["primary"]["collection"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let secondary_id = create.body["data"]["secondary"]["collection"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    publish_to_current_storefront_channel(&mut proxy, &primary_id);
    publish_to_current_storefront_channel(&mut proxy, &secondary_id);

    let initial = proxy.process_request(storefront_graphql_request(
        r#"
        query StorefrontCollectionProjection(
          $id: ID!
          $handle: String!
          $query: String!
          $namespace: String!
        ) {
          byId: collection(id: $id) {
            ...CollectionCard
          }
          byHandleArgument: collection(handle: $handle) {
            id
            aliasedTitle: title
            handle
          }
          deprecatedByHandle: collectionByHandle(handle: $handle) {
            id
            title
            handle
          }
          firstPage: collections(first: 1, query: $query, sortKey: TITLE) {
            edges { cursor node { id title handle } }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
          reverseCatalog: collections(first: 2, query: $query, sortKey: TITLE, reverse: true) {
            nodes { id title handle }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
          missing: collection(id: "gid://shopify/Collection/missing") { id }
          empty: collections(first: 2, query: "missing-storefront-collection") {
            nodes { id }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
        }

        fragment CollectionCard on Collection {
          __typename
          id
          title
          handle
          description
          truncatedDescription: description(truncateAt: 12)
          descriptionHtml
          updatedAt
          image { url altText }
          seo { title description }
          metafield(namespace: $namespace, key: "visible") {
            namespace
            key
            type
            value
          }
          metafields(identifiers: [
            { namespace: $namespace, key: "visible" }
            { namespace: $namespace, key: "missing" }
          ]) {
            namespace
            key
            value
          }
          products(first: 2, sortKey: COLLECTION_DEFAULT) {
            edges {
              cursor
              node {
                __typename
                id
                title
                handle
                availableForSale
                totalInventory
                vendor
                productType
                tags
              }
            }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
          productsReverse: products(first: 2, sortKey: MANUAL, reverse: true) {
            nodes { id title handle }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
          availableProducts: products(first: 3, filters: [{ available: true }]) {
            nodes { id title availableForSale }
          }
          taggedProducts: products(first: 3, filters: [{ tag: "storefront-collections" }]) {
            nodes { id title tags }
          }
        }
        "#,
        json!({
            "id": primary_id,
            "handle": "storefront-collection-alpha",
            "query": "Storefront Collection",
            "namespace": "storefront_collections"
        }),
    ));
    assert_eq!(initial.status, 200);
    assert_eq!(
        initial.body["data"]["byId"]["__typename"],
        json!("Collection")
    );
    assert_eq!(
        initial.body["data"]["byId"]["title"],
        json!("Storefront Collection Alpha")
    );
    assert_eq!(
        initial.body["data"]["byId"]["description"],
        json!("Storefront collection description")
    );
    assert_eq!(
        initial.body["data"]["byId"]["truncatedDescription"],
        json!("Storefron...")
    );
    assert_eq!(
        initial.body["data"]["byId"]["descriptionHtml"],
        json!("<p>Storefront collection description</p>")
    );
    assert_eq!(
        initial.body["data"]["byId"]["image"],
        json!({
            "url": "https://placehold.co/64x64/png",
            "altText": "Storefront collection image"
        })
    );
    assert_eq!(
        initial.body["data"]["byId"]["seo"],
        json!({
            "title": "Storefront Collection SEO",
            "description": "Storefront Collection SEO description"
        })
    );
    assert_eq!(
        initial.body["data"]["byId"]["metafield"],
        json!({
            "namespace": "storefront_collections",
            "key": "visible",
            "type": "single_line_text_field",
            "value": "Visible collection metafield"
        })
    );
    assert_eq!(
        initial.body["data"]["byId"]["metafields"],
        json!([
            {
                "namespace": "storefront_collections",
                "key": "visible",
                "value": "Visible collection metafield"
            },
            null
        ])
    );
    assert_eq!(
        initial.body["data"]["byId"]["products"]["edges"]
            .as_array()
            .unwrap()
            .iter()
            .map(|edge| edge["node"]["title"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec!["Alpha Collection Product", "Beta Collection Product"]
    );
    assert_eq!(
        initial.body["data"]["byId"]["products"]["pageInfo"]["hasNextPage"],
        json!(true)
    );
    assert_eq!(
        initial.body["data"]["byId"]["productsReverse"]["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|node| node["title"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec!["Gamma Collection Product", "Beta Collection Product"]
    );
    assert_eq!(
        initial.body["data"]["byId"]["availableProducts"]["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|node| node["title"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec!["Alpha Collection Product", "Gamma Collection Product"]
    );
    assert_eq!(
        initial.body["data"]["byId"]["taggedProducts"]["nodes"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
    assert_eq!(
        initial.body["data"]["byHandleArgument"]["aliasedTitle"],
        json!("Storefront Collection Alpha")
    );
    assert_eq!(
        initial.body["data"]["deprecatedByHandle"]["id"],
        json!(primary_id)
    );
    assert_eq!(
        initial.body["data"]["firstPage"]["pageInfo"]["hasNextPage"],
        json!(true)
    );
    assert_eq!(
        initial.body["data"]["reverseCatalog"]["nodes"][0]["id"],
        json!(secondary_id)
    );
    assert_eq!(initial.body["data"]["missing"], Value::Null);
    assert_eq!(initial.body["data"]["empty"]["nodes"], json!([]));

    let reorder = proxy.process_request(json_graphql_request(
        r#"
        mutation ReorderStorefrontCollection($id: ID!, $moves: [MoveInput!]!) {
          collectionReorderProducts(id: $id, moves: $moves) {
            job { id done }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "id": primary_id,
            "moves": [{ "id": "gid://shopify/Product/storefront-collection-gamma", "newPosition": "0" }]
        }),
    ));
    assert_eq!(
        reorder.body["data"]["collectionReorderProducts"]["userErrors"],
        json!([])
    );

    let update = proxy.process_request(json_graphql_request(
        r#"
        mutation UpdateStorefrontCollection($collection: CollectionInput!, $product: ProductUpdateInput!) {
          collectionUpdate(input: $collection) {
            collection { id title handle image { url altText } seo { title description } }
            userErrors { field message }
          }
          productUpdate(product: $product) {
            product { id title handle }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "collection": {
                "id": primary_id,
                "title": "Updated Storefront Collection",
                "handle": "updated-storefront-collection",
                "image": { "src": "https://placehold.co/80x80/png", "altText": "Updated collection image" },
                "seo": { "title": "Updated SEO", "description": "Updated SEO description" }
            },
            "product": {
                "id": "gid://shopify/Product/storefront-collection-beta",
                "title": "Updated Beta Collection Product",
                "handle": "updated-beta-collection-product"
            }
        }),
    ));
    assert_eq!(
        update.body["data"]["collectionUpdate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        update.body["data"]["productUpdate"]["userErrors"],
        json!([])
    );

    let lifecycle_read = |proxy: &mut DraftProxy| {
        proxy.process_request(storefront_graphql_request(
            r#"
            query StorefrontCollectionLifecycle($id: ID!) {
              collection(id: $id) {
                id
                title
                handle
                image { url altText }
                seo { title description }
                products(first: 5, sortKey: COLLECTION_DEFAULT) {
                  nodes { id title handle }
                }
              }
            }
            "#,
            json!({ "id": primary_id }),
        ))
    };
    let updated = lifecycle_read(&mut proxy);
    assert_eq!(
        updated.body["data"]["collection"]["title"],
        json!("Updated Storefront Collection")
    );
    assert_eq!(
        updated.body["data"]["collection"]["image"]["url"],
        json!("https://placehold.co/80x80/png")
    );
    assert_eq!(
        updated.body["data"]["collection"]["products"]["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|node| node["title"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec![
            "Gamma Collection Product",
            "Alpha Collection Product",
            "Updated Beta Collection Product"
        ]
    );

    let remove = proxy.process_request(json_graphql_request(
        r#"
        mutation RemoveStorefrontCollectionProduct($id: ID!, $productIds: [ID!]!) {
          collectionRemoveProducts(id: $id, productIds: $productIds) {
            job { id done }
            userErrors { field message }
          }
        }
        "#,
        json!({ "id": primary_id, "productIds": ["gid://shopify/Product/storefront-collection-beta"] }),
    ));
    assert_eq!(
        remove.body["data"]["collectionRemoveProducts"]["userErrors"],
        json!([])
    );
    let removed = lifecycle_read(&mut proxy);
    assert_eq!(
        removed.body["data"]["collection"]["products"]["nodes"]
            .as_array()
            .unwrap()
            .len(),
        2
    );

    let add = proxy.process_request(json_graphql_request(
        r#"
        mutation AddStorefrontCollectionProduct($id: ID!, $productIds: [ID!]!) {
          collectionAddProducts(id: $id, productIds: $productIds) {
            collection { id }
            userErrors { field message }
          }
        }
        "#,
        json!({ "id": primary_id, "productIds": ["gid://shopify/Product/storefront-collection-beta"] }),
    ));
    assert_eq!(
        add.body["data"]["collectionAddProducts"]["userErrors"],
        json!([])
    );
    assert_eq!(
        lifecycle_read(&mut proxy).body["data"]["collection"]["products"]["nodes"]
            .as_array()
            .unwrap()
            .len(),
        3
    );

    unpublish_from_current_storefront_channel(
        &mut proxy,
        "gid://shopify/Product/storefront-collection-gamma",
    );
    let product_unpublished = lifecycle_read(&mut proxy);
    assert_eq!(
        product_unpublished.body["data"]["collection"]["products"]["nodes"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    publish_to_current_storefront_channel(
        &mut proxy,
        "gid://shopify/Product/storefront-collection-gamma",
    );
    assert_eq!(
        lifecycle_read(&mut proxy).body["data"]["collection"]["products"]["nodes"]
            .as_array()
            .unwrap()
            .len(),
        3
    );

    let delete_product = proxy.process_request(json_graphql_request(
        r#"
        mutation DeleteStorefrontCollectionProduct($input: ProductDeleteInput!) {
          productDelete(input: $input) { deletedProductId userErrors { field message } }
        }
        "#,
        json!({ "input": { "id": "gid://shopify/Product/storefront-collection-alpha" } }),
    ));
    assert_eq!(
        delete_product.body["data"]["productDelete"]["userErrors"],
        json!([])
    );
    let product_deleted = lifecycle_read(&mut proxy);
    assert_eq!(
        product_deleted.body["data"]["collection"]["products"]["nodes"]
            .as_array()
            .unwrap()
            .len(),
        2
    );

    unpublish_from_current_storefront_channel(&mut proxy, &primary_id);
    let collection_unpublished = lifecycle_read(&mut proxy);
    assert_eq!(
        collection_unpublished.body["data"]["collection"],
        Value::Null
    );
    let delete_collection = proxy.process_request(json_graphql_request(
        r#"
        mutation DeleteStorefrontCollection($input: CollectionDeleteInput!) {
          collectionDelete(input: $input) { deletedCollectionId userErrors { field message } }
        }
        "#,
        json!({ "input": { "id": primary_id } }),
    ));
    assert_eq!(
        delete_collection.body["data"]["collectionDelete"]["userErrors"],
        json!([])
    );
    assert_eq!(
        lifecycle_read(&mut proxy).body["data"]["collection"],
        Value::Null
    );
}

#[test]
fn storefront_collections_live_hybrid_hydrates_once_and_snapshot_absence_stays_local() {
    let calls = Arc::new(Mutex::new(Vec::<Request>::new()));
    let observed = Arc::clone(&calls);
    let mut proxy = configured_proxy(
        ReadMode::LiveHybrid,
        Some(UnsupportedMutationMode::Passthrough),
    )
    .with_upstream_transport(move |request| {
        observed.lock().unwrap().push(request);
        Response {
            status: 200,
            headers: Default::default(),
            body: json!({
                "data": {
                    "collection": {
                        "id": "gid://shopify/Collection/hydrated-storefront",
                        "title": "Hydrated Storefront Collection",
                        "handle": "hydrated-storefront-collection",
                        "description": "Hydrated description",
                        "descriptionHtml": "<p>Hydrated description</p>",
                        "updatedAt": "2026-07-16T00:00:00Z",
                        "image": null,
                        "seo": { "title": "Hydrated SEO", "description": "Hydrated SEO description" },
                        "products": {
                            "edges": [
                                { "cursor": "alpha-cursor", "node": {
                                    "id": "gid://shopify/Product/hydrated-alpha",
                                    "title": "Hydrated Alpha",
                                    "handle": "hydrated-alpha",
                                    "availableForSale": true,
                                    "totalInventory": 4,
                                    "vendor": "Hermes North",
                                    "productType": "Fixture",
                                    "tags": ["alpha", "hydrated"],
                                    "priceRange": {
                                        "minVariantPrice": { "amount": "10.0", "currencyCode": "CAD" },
                                        "maxVariantPrice": { "amount": "10.0", "currencyCode": "CAD" }
                                    }
                                } },
                                { "cursor": "beta-cursor", "node": {
                                    "id": "gid://shopify/Product/hydrated-beta",
                                    "title": "Hydrated Beta",
                                    "handle": "hydrated-beta",
                                    "availableForSale": false,
                                    "totalInventory": 0,
                                    "vendor": "Hermes South",
                                    "productType": "Fixture",
                                    "tags": ["beta", "hydrated"],
                                    "priceRange": {
                                        "minVariantPrice": { "amount": "20.0", "currencyCode": "CAD" },
                                        "maxVariantPrice": { "amount": "20.0", "currencyCode": "CAD" }
                                    }
                                } }
                            ]
                        },
                        "productsReverse": { "nodes": [
                            { "id": "gid://shopify/Product/hydrated-gamma", "title": "Hydrated Gamma", "handle": "hydrated-gamma" },
                            { "id": "gid://shopify/Product/hydrated-beta", "title": "Hydrated Beta", "handle": "hydrated-beta" }
                        ] },
                        "productsByTitle": { "nodes": [
                            { "id": "gid://shopify/Product/hydrated-alpha", "title": "Hydrated Alpha", "handle": "hydrated-alpha" },
                            { "id": "gid://shopify/Product/hydrated-beta", "title": "Hydrated Beta", "handle": "hydrated-beta" },
                            { "id": "gid://shopify/Product/hydrated-gamma", "title": "Hydrated Gamma", "handle": "hydrated-gamma" }
                        ] },
                        "availableProducts": { "nodes": [
                            { "id": "gid://shopify/Product/hydrated-alpha", "title": "Hydrated Alpha", "availableForSale": true },
                            { "id": "gid://shopify/Product/hydrated-gamma", "title": "Hydrated Gamma", "availableForSale": true }
                        ] }
                    }
                }
            }),
        }
    });
    let query = r#"
        query HydrateStorefrontCollection($id: ID!) {
          collection(id: $id) {
            id
            title
            handle
            description
            descriptionHtml
            updatedAt
            image { url altText }
            seo { title description }
            products(first: 2) {
              edges {
                node {
                  id title handle availableForSale totalInventory vendor productType tags
                  priceRange {
                    minVariantPrice { amount currencyCode }
                    maxVariantPrice { amount currencyCode }
                  }
                }
              }
            }
            productsReverse: products(first: 2, sortKey: MANUAL, reverse: true) {
              nodes { id title handle }
            }
            productsByTitle: products(first: 3, sortKey: TITLE) {
              nodes { id title handle }
            }
            availableProducts: products(first: 3, filters: [{ available: true }]) {
              nodes { id title availableForSale }
            }
          }
        }
    "#;
    let variables = json!({ "id": "gid://shopify/Collection/hydrated-storefront" });
    let first = proxy.process_request(storefront_graphql_request(query, variables.clone()));
    assert_eq!(first.status, 200);
    assert_eq!(
        first.body["data"]["collection"]["title"],
        json!("Hydrated Storefront Collection")
    );
    assert_eq!(
        first.body["data"]["collection"]["products"]["edges"]
            .as_array()
            .unwrap()
            .iter()
            .map(|edge| edge["node"]["title"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec!["Hydrated Alpha", "Hydrated Beta"]
    );
    assert_eq!(
        first.body["data"]["collection"]["products"]["edges"][0]["node"],
        json!({
            "id": "gid://shopify/Product/hydrated-alpha",
            "title": "Hydrated Alpha",
            "handle": "hydrated-alpha",
            "availableForSale": true,
            "totalInventory": 4,
            "vendor": "Hermes North",
            "productType": "Fixture",
            "tags": ["alpha", "hydrated"],
            "priceRange": {
                "minVariantPrice": { "amount": "10.0", "currencyCode": "CAD" },
                "maxVariantPrice": { "amount": "10.0", "currencyCode": "CAD" }
            }
        })
    );
    assert_eq!(
        first.body["data"]["collection"]["productsReverse"]["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|node| node["title"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec!["Hydrated Gamma", "Hydrated Beta"]
    );
    let second = proxy.process_request(storefront_graphql_request(query, variables));
    assert_eq!(
        second.body["data"]["collection"]["title"],
        json!("Hydrated Storefront Collection")
    );
    assert_eq!(calls.lock().unwrap().len(), 1);

    let mut snapshot = configured_proxy(ReadMode::Snapshot, Some(UnsupportedMutationMode::Reject))
        .with_upstream_transport(|_| panic!("snapshot collection absence must not hydrate"));
    let absent = snapshot.process_request(storefront_graphql_request(
        r#"
        query AbsentStorefrontCollections {
          collection(id: "gid://shopify/Collection/missing") { id }
          collectionByHandle(handle: "missing") { id }
          collections(first: 2) {
            nodes { id }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(absent.status, 200);
    assert_eq!(absent.body["data"]["collection"], Value::Null);
    assert_eq!(absent.body["data"]["collectionByHandle"], Value::Null);
    assert_eq!(
        absent.body["data"]["collections"],
        json!({
            "nodes": [],
            "pageInfo": {
                "hasNextPage": false,
                "hasPreviousPage": false,
                "startCursor": null,
                "endCursor": null
            }
        })
    );
}

struct StorefrontDiscoveryFixture {
    product_id: String,
    collection_id: String,
    article_id: String,
    page_id: String,
}

fn stage_storefront_discovery_fixture(proxy: &mut DraftProxy) -> StorefrontDiscoveryFixture {
    let catalog = proxy.process_request(json_graphql_request(
        r#"
        mutation StageStorefrontDiscoveryCatalog(
          $product: ProductCreateInput!
          $collection: CollectionInput!
        ) {
          productCreate(product: $product) {
            product { id title handle }
            userErrors { field message }
          }
          collectionCreate(input: $collection) {
            collection { id title handle }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "product": {
                "title": "Aurora Discovery Product",
                "handle": "aurora-discovery-product",
                "status": "ACTIVE",
                "vendor": "Hermes Discovery",
                "productType": "Discovery Fixture",
                "tags": ["aurora", "discovery"],
                "productOptions": [{ "name": "Color", "values": [{ "name": "Blue" }] }]
            },
            "collection": {
                "title": "Aurora Discovery Collection",
                "handle": "aurora-discovery-collection"
            }
        }),
    ));
    assert_eq!(catalog.status, 200);
    assert_eq!(
        catalog.body["data"]["productCreate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        catalog.body["data"]["collectionCreate"]["userErrors"],
        json!([])
    );
    let product_id = catalog.body["data"]["productCreate"]["product"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let collection_id = catalog.body["data"]["collectionCreate"]["collection"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    publish_to_current_storefront_channel(proxy, &product_id);
    publish_to_current_storefront_channel(proxy, &collection_id);

    let content = proxy.process_request(json_graphql_request(
        r#"
        mutation StageStorefrontDiscoveryContent($blog: BlogCreateInput!, $page: PageCreateInput!) {
          blogCreate(blog: $blog) {
            blog { id }
            userErrors { field message code }
          }
          pageCreate(page: $page) {
            page { id title handle }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "blog": {
                "title": "Aurora Discovery Blog",
                "handle": "aurora-discovery-blog"
            },
            "page": {
                "title": "Aurora Discovery Page",
                "handle": "aurora-discovery-page",
                "body": "<p>Aurora discovery page body</p>",
                "isPublished": true
            }
        }),
    ));
    assert_eq!(content.status, 200);
    assert_eq!(content.body["data"]["blogCreate"]["userErrors"], json!([]));
    assert_eq!(content.body["data"]["pageCreate"]["userErrors"], json!([]));
    let blog_id = content.body["data"]["blogCreate"]["blog"]["id"]
        .as_str()
        .unwrap();
    let page_id = content.body["data"]["pageCreate"]["page"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let article = proxy.process_request(json_graphql_request(
        r#"
        mutation StageStorefrontDiscoveryArticle($article: ArticleCreateInput!) {
          articleCreate(article: $article) {
            article { id title handle }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "article": {
            "title": "Aurora Discovery Article",
            "handle": "aurora-discovery-article",
            "body": "<p>Aurora discovery article body</p>",
            "summary": "Aurora discovery article summary",
            "tags": ["aurora", "discovery"],
            "author": { "name": "Discovery Author" },
            "blogId": blog_id,
            "isPublished": true
        }}),
    ));
    assert_eq!(article.status, 200);
    assert_eq!(
        article.body["data"]["articleCreate"]["userErrors"],
        json!([])
    );
    let article_id = article.body["data"]["articleCreate"]["article"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    StorefrontDiscoveryFixture {
        product_id,
        collection_id,
        article_id,
        page_id,
    }
}

#[test]
fn storefront_node_and_nodes_dispatch_supported_visible_types_and_preserve_slots() {
    let publication_id = "gid://shopify/Publication/storefront-discovery";
    let mut proxy = configured_proxy(ReadMode::Snapshot, Some(UnsupportedMutationMode::Reject))
        .with_upstream_transport(|_| panic!("snapshot Storefront discovery must stay local"));
    restore_storefront_current_publication(&mut proxy, publication_id);
    let fixture = stage_storefront_discovery_fixture(&mut proxy);

    let response = proxy.process_request(storefront_graphql_request(
        r#"
        query StorefrontNodeDiscovery($productId: ID!, $ids: [ID!]!) {
          aliasedNode: node(id: $productId) {
            ...NodeIdentity
            ... on Product { aliasedTitle: title handle }
          }
          aliasedNodes: nodes(ids: $ids) {
            ...NodeIdentity
            ... on Product { title handle }
            ... on Collection { title handle }
            ... on Article { title handle }
            ... on Page { title handle }
          }
        }

        fragment NodeIdentity on Node { __typename id }
        "#,
        json!({
            "productId": fixture.product_id,
            "ids": [
                fixture.page_id,
                "gid://shopify/Product/missing",
                fixture.collection_id,
                fixture.article_id,
                fixture.product_id,
                fixture.page_id
            ]
        }),
    ));

    assert_eq!(response.status, 200);
    assert_eq!(response.body["errors"], Value::Null);
    assert_eq!(
        response.body["data"]["aliasedNode"]["__typename"],
        json!("Product")
    );
    assert_eq!(
        response.body["data"]["aliasedNode"]["aliasedTitle"],
        json!("Aurora Discovery Product")
    );
    let nodes = response.body["data"]["aliasedNodes"].as_array().unwrap();
    assert_eq!(nodes.len(), 6);
    assert_eq!(nodes[0]["__typename"], json!("Page"));
    assert_eq!(nodes[1], Value::Null);
    assert_eq!(nodes[2]["__typename"], json!("Collection"));
    assert_eq!(nodes[3]["__typename"], json!("Article"));
    assert_eq!(nodes[4]["__typename"], json!("Product"));
    assert_eq!(nodes[5]["id"], nodes[0]["id"]);
}

#[test]
fn storefront_discovery_rejects_malformed_global_ids_like_shopify() {
    let mut proxy = configured_proxy(ReadMode::Snapshot, Some(UnsupportedMutationMode::Reject))
        .with_upstream_transport(|_| panic!("invalid snapshot node must not call upstream"));
    let response = proxy.process_request(storefront_graphql_request(
        r#"query MalformedStorefrontNode { node(id: "not-a-gid") { id } }"#,
        json!({}),
    ));

    assert_eq!(response.status, 200);
    assert_eq!(
        response.body["errors"][0]["message"],
        json!("Invalid global id 'not-a-gid'")
    );
    assert_eq!(
        response.body["errors"][0]["extensions"],
        json!({ "code": "argumentLiteralsIncompatible", "typeName": "CoercionError" })
    );
    assert_eq!(response.body.get("data"), None);
}

#[test]
fn storefront_discovery_parity_document_with_operation_name_stays_local() {
    let publication_id = "gid://shopify/Publication/storefront-discovery-parity-document";
    let mut proxy = configured_proxy(
        ReadMode::LiveHybrid,
        Some(UnsupportedMutationMode::Passthrough),
    )
    .with_upstream_transport(|_| panic!("staged Storefront discovery must stay local"));
    restore_state_with(&mut proxy, |state| {
        state["baseState"]["shop"] = json!({ "currencyCode": "USD" });
    });
    restore_storefront_current_publication(&mut proxy, publication_id);
    let fixture = stage_storefront_discovery_fixture(&mut proxy);
    let document =
        include_str!("../../config/parity-requests/storefront/storefront-discovery-read.graphql");
    let response = proxy.process_request(request_with_body(
        "POST",
        "/api/2026-04/graphql.json",
        &json!({
            "query": document,
            "operationName": "StorefrontDiscoveryRead",
            "variables": {
                "productId": fixture.product_id.clone(),
                "nodeIds": [
                    fixture.page_id.clone(),
                    "gid://shopify/Product/missing",
                    fixture.collection_id.clone(),
                    fixture.article_id.clone(),
                    fixture.product_id.clone(),
                    fixture.page_id.clone()
                ],
                "query": "Aurora Discovery",
                "prefixQuery": "Aurora Disc",
                "emptyQuery": "zz-no-storefront-discovery-result",
                "suggestionsQuery": "aur",
                "tag": "discovery",
                "after": null
            }
        })
        .to_string(),
    ));

    assert_eq!(response.status, 200);
    assert_eq!(response.body["errors"], Value::Null, "{}", response.body);
    assert_eq!(response.body["data"]["mixed"]["totalCount"], json!(1));
    assert_eq!(response.body["data"]["prefixLast"]["totalCount"], json!(3));
    assert_eq!(response.body["data"]["aliasedNodes"][1], Value::Null);
}

#[test]
fn storefront_search_and_predictive_search_use_effective_visible_state() {
    let publication_id = "gid://shopify/Publication/storefront-discovery-search";
    let mut proxy = configured_proxy(
        ReadMode::LiveHybrid,
        Some(UnsupportedMutationMode::Passthrough),
    )
    .with_upstream_transport(|_| panic!("staged Storefront discovery must stay local"));
    restore_storefront_current_publication(&mut proxy, publication_id);
    let fixture = stage_storefront_discovery_fixture(&mut proxy);

    let query = r#"
      query StorefrontSearchDiscovery($after: String) {
        search(
          first: 2
          after: $after
          query: "Aurora Discovery"
          prefix: LAST
          unavailableProducts: SHOW
        ) {
          totalCount
          edges {
            cursor
            node {
              __typename
              ... on Product { id title handle }
              ... on Article { id title handle }
              ... on Page { id title handle }
            }
          }
          nodes { __typename ... on Product { id } ... on Article { id } ... on Page { id } }
          pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          productFilters { id label type presentation values { id label count input } }
        }
        explicitTypes: search(
          first: 10
          query: "Aurora Discovery"
          prefix: LAST
          types: [PRODUCT, ARTICLE, PAGE]
          unavailableProducts: SHOW
        ) {
          totalCount
          nodes { __typename ... on Product { id } ... on Article { id } ... on Page { id } }
        }
        predictive: predictiveSearch(
          query: "Aurora Disc"
          limit: 10
          limitScope: EACH
          searchableFields: [TITLE, TAG, PRODUCT_TYPE, VENDOR]
          types: [PRODUCT, COLLECTION, ARTICLE, PAGE, QUERY]
          unavailableProducts: SHOW
        ) {
          products { id title handle }
          collections { id title handle }
          articles { id title handle }
          pages { id title handle }
          queries { text styledText trackingParameters }
        }
      }
    "#;
    let first = proxy.process_request(storefront_graphql_request(query, json!({ "after": null })));
    assert_eq!(first.status, 200);
    assert_eq!(first.body["errors"], Value::Null, "{}", first.body);
    assert_eq!(first.body["data"]["search"]["totalCount"], json!(3));
    assert_eq!(
        first.body["data"]["search"]["nodes"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        first.body["data"]["search"]["pageInfo"]["hasNextPage"],
        json!(true)
    );
    assert_eq!(first.body["data"]["explicitTypes"]["totalCount"], json!(1));
    assert_eq!(
        first.body["data"]["explicitTypes"]["nodes"][0]["__typename"],
        json!("Product")
    );
    assert_eq!(
        first.body["data"]["predictive"]["products"][0]["id"],
        json!(fixture.product_id)
    );
    assert_eq!(
        first.body["data"]["predictive"]["collections"][0]["id"],
        json!(fixture.collection_id)
    );
    assert_eq!(
        first.body["data"]["predictive"]["articles"][0]["id"],
        json!(fixture.article_id)
    );
    assert_eq!(
        first.body["data"]["predictive"]["pages"][0]["id"],
        json!(fixture.page_id)
    );

    let cursor = first.body["data"]["search"]["pageInfo"]["endCursor"]
        .as_str()
        .unwrap();
    let second = proxy.process_request(storefront_graphql_request(
        query,
        json!({ "after": cursor }),
    ));
    assert_eq!(second.body["data"]["search"]["totalCount"], json!(3));
    assert_eq!(
        second.body["data"]["search"]["nodes"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        second.body["data"]["search"]["pageInfo"]["hasPreviousPage"],
        json!(true)
    );

    let delete = proxy.process_request(json_graphql_request(
        r#"
        mutation RemoveStorefrontDiscoveryArticle($articleId: ID!) {
          articleDelete(id: $articleId) { deletedArticleId userErrors { field message code } }
        }
        "#,
        json!({ "articleId": fixture.article_id }),
    ));
    assert_eq!(delete.status, 200);
    assert_eq!(
        delete.body["data"]["articleDelete"]["userErrors"],
        json!([])
    );
    let hide_page = proxy.process_request(json_graphql_request(
        r#"
        mutation HideStorefrontDiscoveryPage($pageId: ID!, $page: PageUpdateInput!) {
          pageUpdate(id: $pageId, page: $page) { page { id isPublished } userErrors { field message code } }
        }
        "#,
        json!({ "pageId": fixture.page_id, "page": { "isPublished": false } }),
    ));
    assert_eq!(hide_page.status, 200);
    assert_eq!(
        hide_page.body["data"]["pageUpdate"]["userErrors"],
        json!([])
    );
    unpublish_from_current_storefront_channel(&mut proxy, &fixture.product_id);
    unpublish_from_current_storefront_channel(&mut proxy, &fixture.collection_id);

    let hidden = proxy.process_request(storefront_graphql_request(query, json!({ "after": null })));
    assert_eq!(hidden.body["data"]["search"]["totalCount"], json!(0));
    assert_eq!(hidden.body["data"]["search"]["nodes"], json!([]));
    assert_eq!(hidden.body["data"]["predictive"]["products"], json!([]));
    assert_eq!(hidden.body["data"]["predictive"]["collections"], json!([]));
    assert_eq!(hidden.body["data"]["predictive"]["articles"], json!([]));
    assert_eq!(hidden.body["data"]["predictive"]["pages"], json!([]));
}

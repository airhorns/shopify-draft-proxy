use super::common::*;
use shopify_draft_proxy::proxy::UnsupportedMutationMode;

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
            body: json!({ "data": { "cart": null } }),
        }
    });

    let response = proxy.process_request(Request {
        method: "POST".to_string(),
        path: "/api/2026-04/graphql.json".to_string(),
        headers: Default::default(),
        body: json!({
            "query": "query StorefrontCart { cart(id: \"gid://shopify/Cart/1\") { id } }",
            "variables": {}
        })
        .to_string(),
    });

    assert_eq!(response.status, 200);
    assert_eq!(response.body["data"]["cart"], Value::Null);
    assert_eq!(observed_requests.lock().unwrap().len(), 1);
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
            "query": "query StorefrontSnapshot { products(first: 1) { nodes { id } pageInfo { hasNextPage hasPreviousPage } } }",
            "variables": {}
        })
        .to_string(),
    });

    assert_eq!(response.status, 200);
    assert_eq!(response.body["data"]["products"]["nodes"], json!([]));
    assert_eq!(
        response.body["data"]["products"]["pageInfo"],
        json!({ "hasNextPage": false, "hasPreviousPage": false })
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
            "query": "mutation StorefrontCartCreate { cartCreate { cart { id } } }",
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
    assert!(hydrate_body["query"]
        .as_str()
        .unwrap()
        .contains("StorefrontFirstSliceHydrateWithContext"));
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
            body: json!({ "data": { "cartCreate": { "cart": { "id": "gid://shopify/Cart/1" } } } }),
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
        "query": "mutation StorefrontMutationShape { cartCreate { cart { id } } }",
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

fn storefront_graphql_request(query: &str, variables: serde_json::Value) -> Request {
    Request {
        method: "POST".to_string(),
        path: "/api/2026-04/graphql.json".to_string(),
        headers: Default::default(),
        body: json!({ "query": query, "variables": variables }).to_string(),
    }
}

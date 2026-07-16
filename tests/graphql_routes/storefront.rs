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

fn storefront_graphql_request(query: &str, variables: serde_json::Value) -> Request {
    Request {
        method: "POST".to_string(),
        path: "/api/2026-04/graphql.json".to_string(),
        headers: Default::default(),
        body: json!({ "query": query, "variables": variables }).to_string(),
    }
}

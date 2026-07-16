use super::common::*;
use pretty_assertions::assert_eq;

#[test]
fn fixed_amount_discount_hydrates_shop_currency_before_serialization() {
    let upstream_queries = Arc::new(Mutex::new(Vec::<String>::new()));
    let captured_queries = Arc::clone(&upstream_queries);
    let mut proxy =
        configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport(move |request| {
            let body: Value = serde_json::from_str(&request.body).expect("upstream body parses");
            let query = body["query"].as_str().unwrap_or_default().to_string();
            captured_queries.lock().unwrap().push(query.clone());
            let data = if query.contains("DraftProxyShopPricingHydrate") {
                json!({
                    "shop": {
                        "currencyCode": "CAD",
                        "taxesIncluded": false,
                        "taxShipping": false
                    }
                })
            } else {
                json!({ "codeDiscountNodeByCode": null })
            };
            Response {
                status: 200,
                headers: Default::default(),
                body: json!({ "data": data }),
            }
        });

    let response = proxy.process_request(json_graphql_request(
        r#"
        mutation DiscountCurrencyHydrate($input: DiscountCodeBasicInput!) {
          discountCodeBasicCreate(basicCodeDiscount: $input) {
            codeDiscountNode {
              codeDiscount {
                ... on DiscountCodeBasic {
                  minimumRequirement {
                    ... on DiscountMinimumSubtotal {
                      greaterThanOrEqualToSubtotal { amount currencyCode }
                    }
                  }
                }
              }
            }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "input": {
                "title": "Currency hydrate",
                "code": "CURRENCYHYDRATE",
                "startsAt": "2026-01-01T00:00:00Z",
                "context": { "all": "ALL" },
                "customerGets": { "value": { "percentage": 0.1 }, "items": { "all": true } },
                "minimumRequirement": { "subtotal": { "greaterThanOrEqualToSubtotal": "1.00" } }
            }
        }),
    ));

    assert_eq!(
        response.body["data"]["discountCodeBasicCreate"]["codeDiscountNode"]["codeDiscount"]
            ["minimumRequirement"]["greaterThanOrEqualToSubtotal"]["currencyCode"],
        json!("CAD")
    );
    assert!(upstream_queries
        .lock()
        .unwrap()
        .iter()
        .any(|query| query.contains("DraftProxyShopPricingHydrate")));
}

fn json_string(value: &Value, context: &str) -> String {
    value
        .as_str()
        .unwrap_or_else(|| panic!("{context} should be a string, got {value}"))
        .to_string()
}

fn query_location(query: &str, needle: &str) -> Value {
    let offset = query
        .find(needle)
        .unwrap_or_else(|| panic!("query should contain {needle:?}"));
    let mut line = 1usize;
    let mut column = 1usize;
    for (index, ch) in query.char_indices() {
        if index == offset {
            return json!([{ "line": line, "column": column }]);
        }
        if ch == '\n' {
            line += 1;
            column = 1;
        } else {
            column += 1;
        }
    }
    json!([{ "line": line, "column": column }])
}

fn upstream_code_basic_fixed_amount_discount(
    redeem_code_id: &str,
    title: &str,
    status: &str,
) -> Value {
    json!({
        "__typename": "DiscountCodeBasic",
        "title": title,
        "status": status,
        "summary": "$5.00 off entire order • Minimum purchase of $50.00",
        "startsAt": "2026-04-27T19:31:14Z",
        "endsAt": null,
        "createdAt": "2026-04-20T19:31:14Z",
        "updatedAt": "2026-05-01T00:00:00Z",
        "asyncUsageCount": 7,
        "usageLimit": 100,
        "recurringCycleLimit": null,
        "discountClasses": ["ORDER"],
        "combinesWith": {
            "productDiscounts": false,
            "orderDiscounts": true,
            "shippingDiscounts": false
        },
        "context": {
            "__typename": "DiscountBuyerSelectionAll",
            "all": "ALL"
        },
        "customerGets": {
            "value": {
                "__typename": "DiscountAmount",
                "amount": {
                    "amount": "5.0",
                    "currencyCode": "USD"
                },
                "appliesOnEachItem": false
            },
            "items": {
                "__typename": "AllDiscountItems",
                "allItems": true
            },
            "appliesOnOneTimePurchase": true,
            "appliesOnSubscription": false
        },
        "minimumRequirement": {
            "__typename": "DiscountMinimumSubtotal",
            "greaterThanOrEqualToSubtotal": {
                "amount": "50.0",
                "currencyCode": "USD"
            }
        },
        "appliesOncePerCustomer": true,
        "codes": {
            "nodes": [{
                "id": redeem_code_id,
                "code": "UPSTREAM-FIXED-5",
                "asyncUsageCount": 3
            }]
        }
    })
}

fn upstream_automatic_basic_discount(title: &str, status: &str) -> Value {
    json!({
        "__typename": "DiscountAutomaticBasic",
        "title": title,
        "status": status,
        "summary": "10% off entire order",
        "startsAt": "2026-04-21T19:31:14Z",
        "endsAt": null,
        "createdAt": "2026-04-21T19:31:14Z",
        "updatedAt": "2026-05-02T00:00:00Z",
        "asyncUsageCount": 5,
        "discountClasses": ["ORDER"],
        "combinesWith": {
            "productDiscounts": false,
            "orderDiscounts": true,
            "shippingDiscounts": false
        },
        "context": {
            "__typename": "DiscountBuyerSelectionAll",
            "all": "ALL"
        },
        "customerGets": {
            "value": {
                "__typename": "DiscountPercentage",
                "percentage": 0.1
            },
            "items": {
                "__typename": "AllDiscountItems",
                "allItems": true
            },
            "appliesOnOneTimePurchase": true,
            "appliesOnSubscription": false
        },
        "minimumRequirement": null,
        "appliesOncePerCustomer": false
    })
}

fn upstream_discount_metafields(discount_id: &str) -> Value {
    json!({
        "nodes": [{
            "id": format!("{discount_id}/Metafield/1"),
            "namespace": "custom",
            "key": "campaign",
            "type": "single_line_text_field",
            "value": "summer"
        }],
        "pageInfo": {
            "hasNextPage": false,
            "hasPreviousPage": false,
            "startCursor": null,
            "endCursor": null
        }
    })
}

fn upstream_discount_connection(nodes: Vec<Value>) -> Value {
    let edges = nodes
        .iter()
        .map(|node| json!({ "cursor": node["id"], "node": node }))
        .collect::<Vec<_>>();
    let start_cursor = nodes
        .first()
        .and_then(|node| node.get("id"))
        .cloned()
        .unwrap_or(Value::Null);
    let end_cursor = nodes
        .last()
        .and_then(|node| node.get("id"))
        .cloned()
        .unwrap_or(Value::Null);
    json!({
        "nodes": nodes,
        "edges": edges,
        "pageInfo": {
            "hasNextPage": false,
            "hasPreviousPage": false,
            "startCursor": start_cursor,
            "endCursor": end_cursor
        }
    })
}

fn assert_full_discount_config_hydrate_request(body: &str) {
    let request_body: Value =
        serde_json::from_str(body).expect("discount hydrate request body should parse");
    let query = request_body["query"]
        .as_str()
        .expect("discount hydrate request should carry a query string");
    for field in [
        "customerGets",
        "customerBuys",
        "minimumRequirement",
        "usageLimit",
        "usesPerOrderLimit",
        "recurringCycleLimit",
        "combinesWith",
        "discountClasses",
        "destinationSelection",
        "maximumShippingPrice",
        "appliesOncePerCustomer",
        "context",
        "summary",
        "metafields",
    ] {
        assert!(
            query.contains(field),
            "discount hydrate query should select {field}, got: {query}"
        );
    }
}

fn assert_code_only_bounded_discount_hydrate_request(request_body: &Value) {
    let query = request_body["query"]
        .as_str()
        .expect("discount hydrate request should carry a query string");
    assert!(
        query.contains("codeDiscountNode"),
        "code-discount hydrate should read the code branch, got: {query}"
    );
    assert!(
        !query.contains("automaticDiscountNode"),
        "code-discount hydrate should not read the automatic branch, got: {query}"
    );
    assert!(
        !query.contains("first: 250"),
        "discount hydrate should not fetch unbounded 250-item windows by default, got: {query}"
    );
}

fn discount_context_hydrate_node(id: &str) -> Value {
    let tail = id.rsplit('/').next().unwrap_or(id);
    if id.contains("/Customer/") {
        json!({
            "__typename": "Customer",
            "id": id,
            "displayName": format!("Customer {tail}"),
            "email": format!("customer-{tail}@example.com")
        })
    } else {
        json!({
            "__typename": "Segment",
            "id": id,
            "name": format!("Segment {tail}"),
            "query": format!("customer_tag = 'segment-{tail}'"),
            "creationDate": "2026-04-01T00:00:00Z",
            "lastEditDate": "2026-04-02T00:00:00Z"
        })
    }
}

fn assert_synthetic_gid(id: &str, resource_type: &str) {
    assert!(
        id.starts_with(&format!("gid://shopify/{resource_type}/")),
        "{id} should be a {resource_type} gid"
    );
    assert!(
        id.contains("shopify-draft-proxy=synthetic"),
        "{id} should be synthetic"
    );
}

fn assert_datetime_string(value: &Value, context: &str) {
    let timestamp = value
        .as_str()
        .unwrap_or_else(|| panic!("{context} should be a string, got {value}"));
    assert!(
        timestamp.contains('T') && timestamp.ends_with('Z'),
        "{context} should be an ISO-8601 DateTime-shaped string, got {timestamp}"
    );
}

fn upstream_gift_card_fixture(id: &str, currency: &str) -> Value {
    json!({
        "__typename": "GiftCard",
        "id": id,
        "legacyResourceId": id.trim_start_matches("gid://shopify/GiftCard/"),
        "lastCharacters": "9876",
        "maskedCode": "•••• •••• •••• 9876",
        "enabled": true,
        "deactivatedAt": null,
        "disabledAt": null,
        "expiresOn": "2028-01-31",
        "note": "real upstream note",
        "templateSuffix": null,
        "createdAt": "2026-06-01T12:00:00Z",
        "updatedAt": "2026-06-02T12:00:00Z",
        "initialValue": { "amount": "25.0", "currencyCode": currency },
        "balance": { "amount": "25.0", "currencyCode": currency },
        "customer": { "id": "gid://shopify/Customer/424242", "email": "gift-real@example.com" },
        "recipientAttributes": null,
        "transactions": {
            "nodes": [],
            "edges": [],
            "pageInfo": {
                "hasNextPage": false,
                "hasPreviousPage": false,
                "startCursor": null,
                "endCursor": null
            }
        }
    })
}

fn gift_card_hydrate_query_from_body(body: &str) -> String {
    let request_body: Value =
        serde_json::from_str(body).expect("gift-card hydrate request body should parse");
    request_body["query"]
        .as_str()
        .expect("gift-card hydrate should include query")
        .to_string()
}

fn assert_gift_card_hydrate_omits_transactions(query: &str, context: &str) {
    assert!(
        !query.contains("transactions("),
        "{context} should not hydrate gift-card transactions, got: {query}"
    );
    assert!(
        !query.contains("first: 250"),
        "{context} should not fetch a 250-item transaction window, got: {query}"
    );
}

fn assert_gift_card_hydrate_includes_transactions(query: &str, context: &str) {
    assert!(
        query.contains("transactions(first: 250)"),
        "{context} should hydrate gift-card transactions, got: {query}"
    );
    assert!(
        query.contains("pageInfo"),
        "{context} should hydrate transaction pageInfo with transaction nodes, got: {query}"
    );
}

fn live_hybrid_gift_card_hydrate_query_for_request(query: &str, variables: Value) -> String {
    let upstream_id = variables["id"]
        .as_str()
        .expect("test variables should include id")
        .to_string();
    let captured_bodies = Arc::new(Mutex::new(Vec::new()));
    let captured_for_proxy = Arc::clone(&captured_bodies);
    let mut proxy =
        configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport(move |request| {
            captured_for_proxy
                .lock()
                .unwrap()
                .push(request.body.clone());
            let hydrate_query = gift_card_hydrate_query_from_body(&request.body);
            let mut gift_card = upstream_gift_card_fixture(&upstream_id, "USD");
            if !hydrate_query.contains("transactions(") {
                gift_card
                    .as_object_mut()
                    .expect("fixture should be an object")
                    .remove("transactions");
            }
            Response {
                status: 200,
                headers: Default::default(),
                body: json!({
                    "data": {
                        "giftCard": gift_card,
                        "giftCardConfiguration": {
                            "issueLimit": { "amount": "3000.0", "currencyCode": "USD" },
                            "purchaseLimit": { "amount": "14000.0", "currencyCode": "USD" }
                        }
                    }
                }),
            }
        });

    let response = proxy.process_request(json_graphql_request(query, variables));
    assert_eq!(response.status, 200);
    let captured_bodies = captured_bodies.lock().unwrap();
    assert_eq!(
        captured_bodies.len(),
        1,
        "request should issue exactly one gift-card hydrate"
    );
    gift_card_hydrate_query_from_body(&captured_bodies[0])
}

fn legacy_gift_card_fixture(id: &str) -> Value {
    let mut card = upstream_gift_card_fixture(id, "CAD");
    card["lastCharacters"] = json!("2053");
    card["maskedCode"] = json!("•••• •••• •••• 2053");
    card["initialValue"] = json!({ "amount": "5.0", "currencyCode": "CAD" });
    card["balance"] = card["initialValue"].clone();
    card["expiresOn"] = json!("2027-04-26");
    card["note"] = json!("legacy gift card fixture");
    card["customer"] = json!({
        "id": "gid://shopify/Customer/10552623464754",
        "email": "gift-card-customer@example.com",
        "defaultEmailAddress": { "emailAddress": "gift-card-customer@example.com" },
        "defaultPhoneNumber": null
    });
    card["recipientAttributes"] = json!({
        "message": "recipient message",
        "preferredName": "recipient",
        "sendNotificationAt": null,
        "recipient": {
            "id": "gid://shopify/Customer/10552623464754",
            "email": "gift-card-customer@example.com",
            "defaultEmailAddress": { "emailAddress": "gift-card-customer@example.com" },
            "defaultPhoneNumber": null
        }
    });
    card
}

fn restore_proxy_state(proxy: &mut DraftProxy, update: impl FnOnce(&mut Value)) {
    let dump = proxy
        .process_request(request_with_body(
            "POST",
            "/__meta/dump",
            &json!({ "createdAt": "2026-06-16T00:00:00.000Z" }).to_string(),
        ))
        .body;
    let mut restored = dump.clone();
    update(&mut restored);
    let restore = proxy.process_request(request_with_body(
        "POST",
        "/__meta/restore",
        &restored.to_string(),
    ));
    assert_eq!(restore.status, 200);
}

fn seed_legacy_gift_card_base_state(proxy: &mut DraftProxy) {
    let mut cards = serde_json::Map::new();
    for id in [
        "gid://shopify/GiftCard/har694-active",
        "gid://shopify/GiftCard/1?shopify-draft-proxy=synthetic",
        "gid://shopify/GiftCard/654773256498",
        "gid://shopify/GiftCard/654865301810",
        "gid://shopify/GiftCard/654808252722",
        "gid://shopify/GiftCard/trial-assignment",
        "gid://shopify/GiftCard/trial-update-card",
        "gid://shopify/GiftCard/timezone-credit",
        "gid://shopify/GiftCard/timezone-debit",
        "gid://shopify/GiftCard/timezone-customer-notification",
        "gid://shopify/GiftCard/timezone-recipient-notification",
        "gid://shopify/GiftCard/disabled-entitlement-card",
    ] {
        let mut card = legacy_gift_card_fixture(id);
        if id == "gid://shopify/GiftCard/1?shopify-draft-proxy=synthetic" {
            card["note"] = json!("HAR-766 no-op current note");
            card["expiresOn"] = json!("2030-01-01");
            card["templateSuffix"] = json!("birthday");
            card["initialValue"] = json!({ "amount": "12.5", "currencyCode": "CAD" });
            card["balance"] = card["initialValue"].clone();
            card["recipientAttributes"] = Value::Null;
        }
        cards.insert(id.to_string(), card);
    }
    for id in [
        "gid://shopify/GiftCard/har694-deactivated",
        "gid://shopify/GiftCard/deactivated",
        "gid://shopify/GiftCard/654808318258",
        "gid://shopify/GiftCard/654904197426",
    ] {
        let mut card = legacy_gift_card_fixture(id);
        card["enabled"] = json!(false);
        card["deactivatedAt"] = json!("2026-04-29T09:31:13Z");
        cards.insert(id.to_string(), card);
    }
    for id in [
        "gid://shopify/GiftCard/654808285490",
        "gid://shopify/GiftCard/654904295730",
    ] {
        let mut card = legacy_gift_card_fixture(id);
        card["expiresOn"] = json!("2020-01-01");
        cards.insert(id.to_string(), card);
    }
    for id in [
        "gid://shopify/GiftCard/timezone-credit",
        "gid://shopify/GiftCard/timezone-debit",
        "gid://shopify/GiftCard/timezone-customer-notification",
        "gid://shopify/GiftCard/timezone-recipient-notification",
    ] {
        let mut card = legacy_gift_card_fixture(id);
        card["expiresOn"] = json!("2026-06-14");
        cards.insert(id.to_string(), card);
    }
    let mut boundary = legacy_gift_card_fixture("gid://shopify/GiftCard/654867595570");
    boundary["initialValue"] = json!({ "amount": "3000.0", "currencyCode": "CAD" });
    boundary["balance"] = boundary["initialValue"].clone();
    cards.insert("gid://shopify/GiftCard/654867595570".to_string(), boundary);
    let mut no_customer = legacy_gift_card_fixture("gid://shopify/GiftCard/654904230194");
    no_customer["customer"] = Value::Null;
    cards.insert(
        "gid://shopify/GiftCard/654904230194".to_string(),
        no_customer,
    );
    let mut no_contact = legacy_gift_card_fixture("gid://shopify/GiftCard/654904262962");
    no_contact["recipientAttributes"] = json!({
        "message": null,
        "preferredName": null,
        "sendNotificationAt": null,
        "recipient": {
            "id": "gid://shopify/Customer/654904262962",
            "email": null,
            "phone": null,
            "defaultEmailAddress": null,
            "defaultPhoneNumber": null
        }
    });
    cards.insert(
        "gid://shopify/GiftCard/654904262962".to_string(),
        no_contact,
    );

    restore_proxy_state(proxy, |restored| {
        restored["state"]["baseState"]["shop"]["currencyCode"] = json!("CAD");
        restored["state"]["baseState"]["giftCards"] = Value::Object(cards);
        restored["state"]["baseState"]["giftCardConfiguration"] = json!({
            "issueLimit": { "amount": "3000.0", "currencyCode": "CAD" },
            "purchaseLimit": { "amount": "14000.0", "currencyCode": "CAD" }
        });
    });
}

fn set_gift_card_trial_shop(proxy: &mut DraftProxy) {
    restore_proxy_state(proxy, |restored| {
        restored["state"]["baseState"]["shop"]["plan"] = json!({
            "partnerDevelopment": false,
            "publicDisplayName": "Trial",
            "shopifyPlus": false
        });
    });
}

fn set_gift_cards_unavailable(proxy: &mut DraftProxy) {
    restore_proxy_state(proxy, |restored| {
        restored["state"]["baseState"]["shop"]["entitlements"]["giftCards"] =
            json!({ "enabled": false });
    });
}

fn stage_market(proxy: &mut DraftProxy, name: &str, country_code: &str) -> String {
    let response = proxy.process_request(json_graphql_request(
        r#"mutation StageMarketForLocalization($input: MarketCreateInput!) {
          marketCreate(input: $input) {
            market { id }
            userErrors { field message code }
          }
        }"#,
        json!({ "input": { "name": name, "regions": [{ "countryCode": country_code }] } }),
    ));
    assert_eq!(response.status, 200);
    assert_eq!(
        response.body["data"]["marketCreate"]["userErrors"],
        json!([])
    );
    json_string(
        &response.body["data"]["marketCreate"]["market"]["id"],
        "staged market id",
    )
}

fn stage_web_presence(proxy: &mut DraftProxy, subfolder_suffix: &str) -> String {
    restore_shop_domain_context(
        proxy,
        "localization-web-presence.myshopify.com",
        "localization-web-presence.example",
    );
    let response = proxy.process_request(json_graphql_request(
        r#"mutation StageWebPresenceForLocalization($input: WebPresenceCreateInput!) {
          webPresenceCreate(input: $input) {
            webPresence { id }
            userErrors { field message code }
          }
        }"#,
        json!({ "input": { "defaultLocale": "en", "subfolderSuffix": subfolder_suffix } }),
    ));
    assert_eq!(response.status, 200);
    assert_eq!(
        response.body["data"]["webPresenceCreate"]["userErrors"],
        json!([])
    );
    json_string(
        &response.body["data"]["webPresenceCreate"]["webPresence"]["id"],
        "staged web presence id",
    )
}

fn function_metadata_record(
    id: &str,
    title: &str,
    handle: &str,
    api_type: &str,
    app_key: &str,
    app_id_tail: &str,
) -> Value {
    json!({
        "id": id,
        "title": title,
        "handle": handle,
        "apiType": api_type,
        "description": format!("{title} fixture function"),
        "appKey": app_key,
        "app": {
            "__typename": "App",
            "id": format!("gid://shopify/App/{app_id_tail}"),
            "title": format!("{title} App"),
            "handle": format!("{handle}-app"),
            "apiKey": app_key
        }
    })
}

fn test_function_metadata() -> Vec<Value> {
    vec![
        function_metadata_record(
            "gid://shopify/ShopifyFunction/validation-local",
            "Validation Local",
            "validation-local",
            "VALIDATION",
            "validation-app-key",
            "validation-app",
        ),
        function_metadata_record(
            "gid://shopify/ShopifyFunction/cart-transform-local",
            "Cart Transform Local",
            "cart-transform-local",
            "CART_TRANSFORM",
            "cart-app-key",
            "cart-app",
        ),
        function_metadata_record(
            "gid://shopify/ShopifyFunction/validation-owned",
            "Owned Validation",
            "validation-owned",
            "VALIDATION",
            "validation-app-key",
            "validation-app",
        ),
        function_metadata_record(
            "gid://shopify/ShopifyFunction/cart-owned",
            "Owned Cart Transform",
            "cart-owned",
            "CART_TRANSFORM",
            "cart-app-key",
            "cart-app",
        ),
        function_metadata_record(
            "gid://shopify/ShopifyFunction/validation-alpha",
            "Validation Alpha",
            "validation-alpha",
            "VALIDATION",
            "validation-app-key",
            "validation-app",
        ),
        function_metadata_record(
            "gid://shopify/ShopifyFunction/conformance-validation",
            "Conformance Validation",
            "conformance-validation",
            "VALIDATION",
            "validation-app-key",
            "validation-app",
        ),
        function_metadata_record(
            "019dd44b-127f-724b-a49c-70fc98ff4d72",
            "Conformance Cart Transform",
            "conformance-cart-transform",
            "CART_TRANSFORM",
            "cart-app-key",
            "cart-app",
        ),
        function_metadata_record(
            "gid://shopify/ShopifyFunction/fulfillment-constraint-local",
            "Fulfillment Constraint Local",
            "fulfillment-constraint-local",
            "FULFILLMENT_CONSTRAINT_RULE",
            "fulfillment-app-key",
            "fulfillment-app",
        ),
        function_metadata_record(
            "gid://shopify/ShopifyFunction/non-catalog-validation",
            "Non Catalog Validation",
            "non-catalog-validation",
            "VALIDATION",
            "non-catalog-app-key",
            "non-catalog-app",
        ),
    ]
}

fn test_function_metadata_by_id_or_handle(id: Option<&str>, handle: Option<&str>) -> Option<Value> {
    test_function_metadata().into_iter().find(|function| {
        id.is_some_and(|id| function["id"].as_str() == Some(id))
            || handle.is_some_and(|handle| function["handle"].as_str() == Some(handle))
    })
}

fn function_metadata_proxy() -> DraftProxy {
    function_metadata_proxy_with_hits(Arc::new(Mutex::new(Vec::new())))
}

fn tax_app_graphql_request(query: &str, variables: serde_json::Value) -> Request {
    let mut request = json_graphql_request(query, variables);
    request.headers.insert(
        "x-shopify-draft-proxy-access-scopes".to_string(),
        "write_taxes".to_string(),
    );
    request.headers.insert(
        "x-shopify-draft-proxy-tax-calculations-app".to_string(),
        "true".to_string(),
    );
    request
}

fn function_metadata_proxy_with_hits(hits: Arc<Mutex<Vec<Value>>>) -> DraftProxy {
    configured_proxy(
        ReadMode::LiveHybrid,
        Some(shopify_draft_proxy::proxy::UnsupportedMutationMode::Passthrough),
    )
    .with_upstream_transport(move |request| {
        let body: Value =
            serde_json::from_str(&request.body).expect("function hydrate body should parse");
        hits.lock().unwrap().push(body.clone());
        let operation_name = body["operationName"].as_str().unwrap_or_default();
        let query = body["query"].as_str().unwrap_or_default();
        let response_body = match operation_name {
            "FunctionHydrateById" => {
                let id = body["variables"]["id"].as_str().unwrap_or_default();
                json!({
                    "data": {
                        "shopifyFunction": test_function_metadata_by_id_or_handle(Some(id), None)
                    }
                })
            }
            "FunctionHydrateByHandle" => {
                let handle = body["variables"]["handle"].as_str().unwrap_or_default();
                let nodes = test_function_metadata_by_id_or_handle(None, Some(handle))
                    .into_iter()
                    .collect::<Vec<_>>();
                json!({ "data": { "shopifyFunctions": { "nodes": nodes } } })
            }
            _ if query.contains("cartTransforms") => {
                json!({ "data": { "cartTransforms": { "nodes": [] } } })
            }
            _ if query.contains("validations") => {
                json!({ "data": { "validations": { "nodes": [] } } })
            }
            _ if query.contains("fulfillmentConstraintRules") => {
                json!({ "data": { "fulfillmentConstraintRules": [] } })
            }
            _ => json!({
                "errors": [{
                    "message": format!("unexpected function metadata upstream request: {body}")
                }]
            }),
        };
        Response {
            status: 200,
            headers: Default::default(),
            body: response_body,
        }
    })
}

fn function_fulfillment_constraint_rule_proxy_with_hits(
    hits: Arc<Mutex<Vec<Value>>>,
) -> DraftProxy {
    let upstream_function = function_metadata_record(
        "gid://shopify/ShopifyFunction/upstream-fulfillment-constraint",
        "Upstream Fulfillment Constraint",
        "upstream-fulfillment-constraint",
        "FULFILLMENT_CONSTRAINT_RULE",
        "upstream-fulfillment-key",
        "upstream-fulfillment-app",
    );
    let upstream_rule = json!({
        "id": "gid://shopify/FulfillmentConstraintRule/upstream-rule",
        "deliveryMethodTypes": ["SHIPPING"],
        "function": upstream_function.clone(),
        "metafields": {
            "nodes": [{
                "id": "gid://shopify/Metafield/upstream-fulfillment-config",
                "namespace": "custom",
                "key": "config",
                "type": "json",
                "value": "{\"mode\":\"upstream\"}",
                "ownerType": "FULFILLMENT_CONSTRAINT_RULE",
                "createdAt": "2026-01-01T00:00:00Z",
                "updatedAt": "2026-01-01T00:00:00Z"
            }]
        }
    });
    configured_proxy(
        ReadMode::LiveHybrid,
        Some(shopify_draft_proxy::proxy::UnsupportedMutationMode::Passthrough),
    )
    .with_upstream_transport(move |request| {
        let body: Value = serde_json::from_str(&request.body)
            .expect("fulfillment constraint rule hydrate body should parse");
        hits.lock().unwrap().push(body.clone());
        let operation_name = body["operationName"].as_str().unwrap_or_default();
        let query = body["query"].as_str().unwrap_or_default();
        let response_body = match operation_name {
            "FunctionHydrateByHandle" => {
                let handle = body["variables"]["handle"].as_str().unwrap_or_default();
                let nodes = test_function_metadata_by_id_or_handle(None, Some(handle))
                    .into_iter()
                    .collect::<Vec<_>>();
                json!({ "data": { "shopifyFunctions": { "nodes": nodes } } })
            }
            "FunctionHydrateById" => {
                let id = body["variables"]["id"].as_str().unwrap_or_default();
                let function = test_function_metadata_by_id_or_handle(Some(id), None)
                    .or_else(|| {
                        (upstream_function["id"].as_str() == Some(id))
                            .then(|| upstream_function.clone())
                    })
                    .unwrap_or(Value::Null);
                json!({ "data": { "shopifyFunction": function } })
            }
            _ if query.contains("validations") => {
                json!({ "data": { "validations": { "nodes": [] } } })
            }
            _ if query.contains("cartTransforms") => {
                json!({ "data": { "cartTransforms": { "nodes": [] } } })
            }
            _ if query.contains("shopifyFunctions") => json!({
                "data": {
                    "shopifyFunctions": {
                        "nodes": [upstream_function.clone()]
                    }
                }
            }),
            _ if query.contains("fulfillmentConstraintRules") => {
                json!({ "data": { "fulfillmentConstraintRules": [upstream_rule.clone()] } })
            }
            _ => json!({
                "errors": [{
                    "message": format!("unexpected fulfillment constraint rule upstream request: {body}")
                }]
            }),
        };
        Response {
            status: 200,
            headers: Default::default(),
            body: response_body,
        }
    })
}

fn discount_app_test_function() -> Value {
    json!({
        "id": "gid://shopify/ShopifyFunction/discount-function",
        "title": "Discount Function",
        "handle": "discount-function",
        "apiType": "discount",
        "description": "Local discount function",
        "appKey": "discount-app-key",
        "app": {
            "id": "gid://shopify/App/discount-app",
            "title": "Discount App",
            "handle": "discount-app",
            "apiKey": "discount-app-key"
        }
    })
}

fn discount_app_function_upstream_response(
    request: Request,
    activation_available: bool,
) -> Response {
    let body = serde_json::from_str::<Value>(&request.body)
        .expect("discount app upstream body should parse");
    let query = body["query"].as_str().unwrap_or_default();
    let function = discount_app_test_function();
    let response_body = if query.contains("ShopifyFunctionByHandle") {
        json!({ "data": { "shopifyFunctions": { "nodes": [function] } } })
    } else if query.contains("ShopifyFunctionById") {
        let mut function = function;
        function
            .as_object_mut()
            .expect("test Function metadata should be an object")
            .remove("handle");
        json!({ "data": { "shopifyFunction": function } })
    } else if query.contains("ShopifyFunctionAvailabilityForDiscountActivation") {
        let nodes = if activation_available {
            vec![function]
        } else {
            Vec::new()
        };
        json!({ "data": { "shopifyFunctions": { "nodes": nodes } } })
    } else {
        json!({
            "errors": [{
                "message": format!("unexpected discount app upstream request: {body}")
            }]
        })
    };

    Response {
        status: 200,
        headers: Default::default(),
        body: response_body,
    }
}

struct DiscountConnectionSeed {
    code_zulu_id: String,
    code_bravo_id: String,
    automatic_yankee_id: String,
    automatic_alpha_id: String,
}

fn seed_discount_connection_mechanics(proxy: &mut DraftProxy) -> DiscountConnectionSeed {
    let setup = proxy.process_request(json_graphql_request(
        r#"
        mutation DiscountConnectionMechanicsSetup {
          codeZulu: discountCodeBasicCreate(basicCodeDiscount: { title: "Zulu code connection", code: "CONNECTION-ZULU", startsAt: "2026-06-04T00:00:00Z", endsAt: "2026-12-04T00:00:00Z", context: { all: "ALL" }, customerGets: { value: { percentage: 0.1 }, items: { all: true } } }) {
            codeDiscountNode { id }
            userErrors { field message code extraInfo }
          }
          codeBravo: discountCodeBasicCreate(basicCodeDiscount: { title: "Bravo code connection", code: "CONNECTION-BRAVO", startsAt: "2026-06-02T00:00:00Z", endsAt: "2026-12-02T00:00:00Z", context: { all: "ALL" }, customerGets: { value: { percentage: 0.1 }, items: { all: true } } }) {
            codeDiscountNode { id }
            userErrors { field message code extraInfo }
          }
          automaticYankee: discountAutomaticBasicCreate(automaticBasicDiscount: { title: "Yankee automatic connection", startsAt: "2026-06-03T00:00:00Z", endsAt: "2026-12-03T00:00:00Z" }) {
            automaticDiscountNode { id }
            userErrors { field message code extraInfo }
          }
          automaticAlpha: discountAutomaticBasicCreate(automaticBasicDiscount: { title: "Alpha automatic connection", startsAt: "2026-06-01T00:00:00Z", endsAt: "2026-12-01T00:00:00Z" }) {
            automaticDiscountNode { id }
            userErrors { field message code extraInfo }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(setup.status, 200);
    for key in ["codeZulu", "codeBravo", "automaticYankee", "automaticAlpha"] {
        assert_eq!(setup.body["data"][key]["userErrors"], json!([]));
    }

    let seed = DiscountConnectionSeed {
        code_zulu_id: json_string(
            &setup.body["data"]["codeZulu"]["codeDiscountNode"]["id"],
            "zulu code discount id",
        ),
        code_bravo_id: json_string(
            &setup.body["data"]["codeBravo"]["codeDiscountNode"]["id"],
            "bravo code discount id",
        ),
        automatic_yankee_id: json_string(
            &setup.body["data"]["automaticYankee"]["automaticDiscountNode"]["id"],
            "yankee automatic discount id",
        ),
        automatic_alpha_id: json_string(
            &setup.body["data"]["automaticAlpha"]["automaticDiscountNode"]["id"],
            "alpha automatic discount id",
        ),
    };

    restore_proxy_state(proxy, |state| {
        let discounts = state["state"]["stagedState"]["discounts"]
            .as_object_mut()
            .expect("staged discounts should be dump-restorable");
        let mut stamp = |id: &str, created_at: &str, updated_at: &str| {
            let record = discounts
                .get_mut(id)
                .unwrap_or_else(|| panic!("missing staged discount {id}"));
            record["createdAt"] = json!(created_at);
            record["updatedAt"] = json!(updated_at);
        };
        stamp(
            &seed.code_zulu_id,
            "2026-06-04T00:00:00Z",
            "2026-06-14T00:00:00Z",
        );
        stamp(
            &seed.code_bravo_id,
            "2026-06-02T00:00:00Z",
            "2026-06-12T00:00:00Z",
        );
        stamp(
            &seed.automatic_yankee_id,
            "2026-06-03T00:00:00Z",
            "2026-06-13T00:00:00Z",
        );
        stamp(
            &seed.automatic_alpha_id,
            "2026-06-01T00:00:00Z",
            "2026-06-11T00:00:00Z",
        );
    });

    seed
}

fn discount_connection_body_selection(root: &str) -> &'static str {
    match root {
        "discountNodes" => {
            r#"discount { __typename ... on DiscountCodeBasic { title } ... on DiscountAutomaticBasic { title } }"#
        }
        "codeDiscountNodes" => r#"codeDiscount { __typename ... on DiscountCodeBasic { title } }"#,
        "automaticDiscountNodes" => {
            r#"automaticDiscount { __typename ... on DiscountAutomaticBasic { title } }"#
        }
        other => panic!("unexpected discount connection root {other}"),
    }
}

fn discount_connection_body_field(root: &str) -> &'static str {
    match root {
        "discountNodes" => "discount",
        "codeDiscountNodes" => "codeDiscount",
        "automaticDiscountNodes" => "automaticDiscount",
        other => panic!("unexpected discount connection root {other}"),
    }
}

fn discount_connection_node_titles(connection: &Value, root: &str) -> Vec<String> {
    let body_field = discount_connection_body_field(root);
    connection["nodes"]
        .as_array()
        .unwrap_or_else(|| panic!("{root} nodes should be an array"))
        .iter()
        .map(|node| json_string(&node[body_field]["title"], "discount connection node title"))
        .collect()
}

fn discount_connection_edge_titles(connection: &Value, root: &str) -> Vec<String> {
    let body_field = discount_connection_body_field(root);
    connection["edges"]
        .as_array()
        .unwrap_or_else(|| panic!("{root} edges should be an array"))
        .iter()
        .map(|edge| {
            json_string(
                &edge["node"][body_field]["title"],
                "discount connection edge node title",
            )
        })
        .collect()
}

fn read_discount_connection_titles(
    proxy: &mut DraftProxy,
    root: &str,
    sort_key: &str,
    reverse: bool,
) -> Vec<String> {
    let query = format!(
        r#"
        query DiscountConnectionSortRead {{
          root: {root}(first: 10, sortKey: {sort_key}, reverse: {reverse}) {{
            nodes {{ id {} }}
          }}
        }}
        "#,
        discount_connection_body_selection(root)
    );
    let response = proxy.process_request(json_graphql_request(&query, json!({})));
    assert_eq!(
        response.status, 200,
        "{root} sortKey {sort_key} reverse {reverse} returned {:?}",
        response.body
    );
    discount_connection_node_titles(&response.body["data"]["root"], root)
}

fn cad_snapshot_proxy() -> DraftProxy {
    let mut proxy = snapshot_proxy();
    restore_shop_currency(&mut proxy, "CAD");
    proxy
}

fn snapshot_proxy_with_gift_card_fixed_validation_clock() -> DraftProxy {
    snapshot_proxy_with_clock(Arc::new(Mutex::new(utc_time(1_777_455_062))))
}

fn cad_snapshot_proxy_with_gift_card_fixed_validation_clock() -> DraftProxy {
    let mut proxy = snapshot_proxy_with_gift_card_fixed_validation_clock();
    restore_shop_currency(&mut proxy, "CAD");
    proxy
}

fn assert_starts_at_required_error(data: &Value, alias: &str, node_field: &str, input_arg: &str) {
    assert_eq!(data[alias][node_field], json!(null));
    assert_eq!(
        data[alias]["userErrors"],
        json!([{
            "field": [input_arg, "startsAt"],
            "message": "Starts at can't be blank",
            "code": "BLANK",
            "extraInfo": null
        }])
    );
}

fn create_discount_ref_product(proxy: &mut DraftProxy) -> (String, String) {
    let product = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateDiscountRefProduct($product: ProductCreateInput!) {
          productCreate(product: $product) {
            product { id }
            userErrors { field message  }
          }
        }
        "#,
        json!({ "product": { "title": "Discount reference product" } }),
    ));
    assert_eq!(
        product.body["data"]["productCreate"]["userErrors"],
        json!([])
    );
    let product_id = json_string(
        &product.body["data"]["productCreate"]["product"]["id"],
        "discount reference product id",
    );
    let variant = create_legacy_variant(proxy, &product_id, "DISCOUNT-REF-VARIANT", "10.00");
    let variant_id = json_string(&variant["id"], "discount reference variant id");
    (product_id, variant_id)
}

fn create_discount_ref_collection(proxy: &mut DraftProxy) -> String {
    let collection = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateDiscountRefCollection($input: CollectionInput!) {
          collectionCreate(input: $input) {
            collection { id }
            userErrors { field message }
          }
        }
        "#,
        json!({ "input": { "title": "Discount reference collection" } }),
    ));
    assert_eq!(
        collection.body["data"]["collectionCreate"]["userErrors"],
        json!([])
    );
    json_string(
        &collection.body["data"]["collectionCreate"]["collection"]["id"],
        "discount reference collection id",
    )
}

fn basic_code_discount_input(title: &str, code: &str, items: Value) -> Value {
    json!({
        "title": title,
        "code": code,
        "startsAt": "2026-04-25T00:00:00Z",
        "context": { "all": "ALL" },
        "customerGets": {
            "value": { "percentage": 0.1 },
            "items": items
        }
    })
}

fn create_basic_code_discount(proxy: &mut DraftProxy, title: &str, code: &str) -> String {
    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateBasicCodeDiscount($input: DiscountCodeBasicInput!) {
          discountCodeBasicCreate(basicCodeDiscount: $input) {
            codeDiscountNode { id }
            userErrors { field message code extraInfo }
          }
        }
        "#,
        json!({ "input": basic_code_discount_input(title, code, json!({ "all": true })) }),
    ));
    assert_eq!(
        create.body["data"]["discountCodeBasicCreate"]["userErrors"],
        json!([])
    );
    json_string(
        &create.body["data"]["discountCodeBasicCreate"]["codeDiscountNode"]["id"],
        "basic code discount id",
    )
}

fn bxgy_code_discount_input(title: &str, code: &str, product_id: &str) -> Value {
    json!({
        "title": title,
        "code": code,
        "startsAt": "2026-04-25T00:00:00Z",
        "context": { "all": "ALL" },
        "customerBuys": {
            "value": { "quantity": "1" },
            "items": { "products": { "productsToAdd": [product_id] } }
        },
        "customerGets": {
            "value": {
                "discountOnQuantity": {
                    "quantity": "1",
                    "effect": { "percentage": 0.5 }
                }
            },
            "items": { "products": { "productsToAdd": [product_id] } }
        }
    })
}

fn create_bxgy_code_discount(
    proxy: &mut DraftProxy,
    title: &str,
    code: &str,
    product_id: &str,
) -> String {
    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateBxgyCodeDiscount($input: DiscountCodeBxgyInput!) {
          discountCodeBxgyCreate(bxgyCodeDiscount: $input) {
            codeDiscountNode { id }
            userErrors { field message code extraInfo }
          }
        }
        "#,
        json!({ "input": bxgy_code_discount_input(title, code, product_id) }),
    ));
    assert_eq!(
        create.body["data"]["discountCodeBxgyCreate"]["userErrors"],
        json!([])
    );
    json_string(
        &create.body["data"]["discountCodeBxgyCreate"]["codeDiscountNode"]["id"],
        "bxgy code discount id",
    )
}

fn create_free_shipping_code_discount(proxy: &mut DraftProxy, title: &str, code: &str) -> String {
    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateFreeShippingCodeDiscount($input: DiscountCodeFreeShippingInput!) {
          discountCodeFreeShippingCreate(freeShippingCodeDiscount: $input) {
            codeDiscountNode { id }
            userErrors { field message code extraInfo }
          }
        }
        "#,
        json!({ "input": {
            "title": title,
            "code": code,
            "startsAt": "2026-04-25T00:00:00Z",
            "destination": { "all": true }
        }}),
    ));
    assert_eq!(
        create.body["data"]["discountCodeFreeShippingCreate"]["userErrors"],
        json!([])
    );
    json_string(
        &create.body["data"]["discountCodeFreeShippingCreate"]["codeDiscountNode"]["id"],
        "free shipping code discount id",
    )
}

#[test]
fn discount_broad_bulk_roots_stage_locally_without_runtime_upstream_forwarding() {
    let clock = Arc::new(Mutex::new(utc_time(1_783_080_000)));
    let upstream_requests = Arc::new(Mutex::new(Vec::<String>::new()));
    let upstream_requests_for_transport = Arc::clone(&upstream_requests);
    let mut proxy = configured_proxy(ReadMode::LiveHybrid, None)
        .with_clock({
            let clock = Arc::clone(&clock);
            move || *clock.lock().unwrap()
        })
        .with_upstream_transport(move |request| {
            upstream_requests_for_transport
                .lock()
                .unwrap()
                .push(request.body.clone());
            Response {
                status: 200,
                headers: Default::default(),
                body: json!({
                    "data": {
                        "discountCodeBulkActivate": { "job": { "done": true }, "userErrors": [] },
                        "discountCodeBulkDeactivate": { "job": { "done": true }, "userErrors": [] },
                        "discountCodeBulkDelete": { "job": { "done": true }, "userErrors": [] },
                        "discountAutomaticBulkDelete": { "job": { "done": true }, "userErrors": [] }
                    }
                }),
            }
        });

    let deactivate_id =
        create_basic_code_discount(&mut proxy, "Bulk deactivate code", "BULK-DEACTIVATE");
    let delete_id = create_basic_code_discount(&mut proxy, "Bulk delete code", "BULK-DELETE");
    let activate = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateExpiredBulkCode($input: DiscountCodeBasicInput!) {
          discountCodeBasicCreate(basicCodeDiscount: $input) {
            codeDiscountNode { id }
            userErrors { field message code extraInfo }
          }
        }
        "#,
        json!({ "input": {
            "title": "Bulk activate code",
            "code": "BULK-ACTIVATE",
            "startsAt": "2026-06-01T00:00:00Z",
            "endsAt": "2026-06-02T00:00:00Z",
            "context": { "all": "ALL" },
            "customerGets": { "value": { "percentage": 0.1 }, "items": { "all": true } }
        }}),
    ));
    assert_eq!(
        activate.body["data"]["discountCodeBasicCreate"]["userErrors"],
        json!([])
    );
    let activate_id = json_string(
        &activate.body["data"]["discountCodeBasicCreate"]["codeDiscountNode"]["id"],
        "bulk activate code id",
    );
    let automatic = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateBulkAutomatic($input: DiscountAutomaticBasicInput!) {
          discountAutomaticBasicCreate(automaticBasicDiscount: $input) {
            automaticDiscountNode { id }
            userErrors { field message code extraInfo }
          }
        }
        "#,
        json!({ "input": {
            "title": "Bulk delete automatic",
            "startsAt": "2026-06-01T00:00:00Z",
            "context": { "all": "ALL" },
            "customerGets": { "value": { "percentage": 0.1 }, "items": { "all": true } }
        }}),
    ));
    assert_eq!(
        automatic.body["data"]["discountAutomaticBasicCreate"]["userErrors"],
        json!([])
    );
    let automatic_id = json_string(
        &automatic.body["data"]["discountAutomaticBasicCreate"]["automaticDiscountNode"]["id"],
        "bulk automatic id",
    );
    upstream_requests.lock().unwrap().clear();

    for (root, id) in [
        ("discountCodeBulkActivate", activate_id.clone()),
        ("discountCodeBulkDeactivate", deactivate_id.clone()),
        ("discountCodeBulkDelete", delete_id.clone()),
        ("discountAutomaticBulkDelete", automatic_id.clone()),
    ] {
        let mutation = format!(
            r#"
            mutation BroadBulk($ids: [ID!]!) {{
              {root}(ids: $ids) {{
                job {{ done }}
                userErrors {{ field message code extraInfo }}
              }}
            }}
            "#
        );
        let response =
            proxy.process_request(json_graphql_request(&mutation, json!({ "ids": [id] })));
        assert_eq!(response.status, 200, "{root} returned {:?}", response.body);
        assert_eq!(response.body["data"][root]["job"]["done"], json!(true));
        assert_eq!(response.body["data"][root]["userErrors"], json!([]));
    }

    let forwarded = upstream_requests.lock().unwrap().clone();
    assert!(
        forwarded.is_empty(),
        "broad discount bulk roots must not forward runtime mutation documents upstream: {forwarded:?}"
    );

    let read = proxy.process_request(json_graphql_request(
        r#"
        query BroadBulkRead($activateId: ID!, $deactivateId: ID!, $deleteId: ID!, $automaticId: ID!) {
          activated: codeDiscountNode(id: $activateId) {
            codeDiscount { ... on DiscountCodeBasic { status } }
          }
          deactivated: codeDiscountNode(id: $deactivateId) {
            codeDiscount { ... on DiscountCodeBasic { status } }
          }
          deletedCode: codeDiscountNode(id: $deleteId) { id }
          deletedAutomatic: automaticDiscountNode(id: $automaticId) { id }
        }
        "#,
        json!({
            "activateId": activate_id,
            "deactivateId": deactivate_id,
            "deleteId": delete_id,
            "automaticId": automatic_id
        }),
    ));
    assert_eq!(
        read.body["data"]["activated"]["codeDiscount"]["status"],
        json!("ACTIVE")
    );
    assert_eq!(
        read.body["data"]["deactivated"]["codeDiscount"]["status"],
        json!("EXPIRED")
    );
    assert_eq!(read.body["data"]["deletedCode"], json!(null));
    assert_eq!(read.body["data"]["deletedAutomatic"], json!(null));

    let log = log_snapshot(&proxy);
    let roots: Vec<_> = log["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| {
            entry["interpreted"]["primaryRootField"]
                .as_str()
                .unwrap()
                .to_string()
        })
        .collect();
    for root in [
        "discountCodeBulkActivate",
        "discountCodeBulkDeactivate",
        "discountCodeBulkDelete",
        "discountAutomaticBulkDelete",
    ] {
        assert!(
            roots.iter().any(|logged| logged == root),
            "{root} missing from log"
        );
    }
}

#[test]
fn discount_broad_bulk_selector_validation_matches_captured_shopify_branches() {
    let mut proxy = snapshot_proxy().with_upstream_transport(|request| {
        panic!(
            "discount broad bulk validation should not forward upstream: {}",
            request.body
        )
    });
    let response = proxy.process_request(json_graphql_request(
        r#"
        mutation DiscountBulkSelectorValidation(
          $codeIds: [ID!]
          $automaticIds: [ID!]
          $search: String
          $savedSearchId: ID
          $codeFieldSearch: String!
          $codeClassSearch: String!
          $unknownFieldSearch: String!
        ) {
          codeActivateEmpty: discountCodeBulkActivate { userErrors { field message code extraInfo } }
          codeActivateBlank: discountCodeBulkActivate(search: "") { userErrors { field message code extraInfo } }
          codeActivateTooMany: discountCodeBulkActivate(ids: $codeIds, search: $search) { userErrors { field message code extraInfo } }
          codeActivateSavedSearch: discountCodeBulkActivate(savedSearchId: $savedSearchId) { userErrors { field message code extraInfo } }
          codeDeactivateEmpty: discountCodeBulkDeactivate { userErrors { field message code extraInfo } }
          codeDeactivateBlank: discountCodeBulkDeactivate(search: "") { userErrors { field message code extraInfo } }
          codeDeactivateTooMany: discountCodeBulkDeactivate(ids: $codeIds, search: $search) { userErrors { field message code extraInfo } }
          codeDeactivateSavedSearch: discountCodeBulkDeactivate(savedSearchId: $savedSearchId) { userErrors { field message code extraInfo } }
          codeDeleteEmpty: discountCodeBulkDelete { userErrors { field message code extraInfo } }
          codeDeleteBlank: discountCodeBulkDelete(search: "") { userErrors { field message code extraInfo } }
          codeDeleteTooMany: discountCodeBulkDelete(ids: $codeIds, search: $search) { userErrors { field message code extraInfo } }
          codeDeleteSavedSearch: discountCodeBulkDelete(savedSearchId: $savedSearchId) { userErrors { field message code extraInfo } }
          codeDeleteCodeField: discountCodeBulkDelete(search: $codeFieldSearch) { userErrors { field message code extraInfo } }
          codeDeleteClassField: discountCodeBulkDelete(search: $codeClassSearch) { userErrors { field message code extraInfo } }
          codeDeleteUnknownField: discountCodeBulkDelete(search: $unknownFieldSearch) { userErrors { field message code extraInfo } }
          automaticDeleteEmpty: discountAutomaticBulkDelete { userErrors { field message code extraInfo } }
          automaticDeleteBlank: discountAutomaticBulkDelete(search: "") { userErrors { field message code extraInfo } }
          automaticDeleteTooMany: discountAutomaticBulkDelete(ids: $automaticIds, search: $search) { userErrors { field message code extraInfo } }
          automaticDeleteSavedSearch: discountAutomaticBulkDelete(savedSearchId: $savedSearchId) { userErrors { field message code extraInfo } }
          automaticDeleteUnknownField: discountAutomaticBulkDelete(search: $unknownFieldSearch) { userErrors { field message code extraInfo } }
        }
        "#,
        json!({
            "codeIds": ["gid://shopify/DiscountCodeNode/0"],
            "automaticIds": ["gid://shopify/DiscountAutomaticNode/0"],
            "search": "status:active",
            "savedSearchId": "gid://shopify/SavedSearch/0",
            "codeFieldSearch": "code:BULK",
            "codeClassSearch": "discount_class:order",
            "unknownFieldSearch": "frobnicate:true"
        }),
    ));
    assert_eq!(response.status, 200);
    let data = &response.body["data"];
    let code_missing = json!([{
        "field": null,
        "message": "Missing expected argument key: 'ids', 'search' or 'saved_search_id'.",
        "code": "MISSING_ARGUMENT",
        "extraInfo": null
    }]);
    let code_blank = json!([{
        "field": ["search"],
        "message": "'Search' can't be blank.",
        "code": "BLANK",
        "extraInfo": null
    }]);
    let code_too_many = json!([{
        "field": null,
        "message": "Only one of 'ids', 'search' or 'saved_search_id' is allowed.",
        "code": "TOO_MANY_ARGUMENTS",
        "extraInfo": null
    }]);
    let code_saved_search = json!([{
        "field": ["savedSearchId"],
        "message": "Invalid 'saved_search_id'.",
        "code": "INVALID",
        "extraInfo": null
    }]);
    for alias in [
        "codeActivateEmpty",
        "codeDeactivateEmpty",
        "codeDeleteEmpty",
    ] {
        assert_eq!(data[alias]["userErrors"], code_missing, "{alias}");
    }
    for alias in [
        "codeActivateBlank",
        "codeDeactivateBlank",
        "codeDeleteBlank",
    ] {
        assert_eq!(data[alias]["userErrors"], code_blank, "{alias}");
    }
    for alias in [
        "codeActivateTooMany",
        "codeDeactivateTooMany",
        "codeDeleteTooMany",
    ] {
        assert_eq!(data[alias]["userErrors"], code_too_many, "{alias}");
    }
    for alias in [
        "codeActivateSavedSearch",
        "codeDeactivateSavedSearch",
        "codeDeleteSavedSearch",
    ] {
        assert_eq!(data[alias]["userErrors"], code_saved_search, "{alias}");
    }
    assert_eq!(
        data["codeDeleteCodeField"]["userErrors"],
        json!([{ "field": ["search"], "message": "Invalid search field(s): code. Check the query syntax.", "code": "INVALID", "extraInfo": null }])
    );
    assert_eq!(
        data["codeDeleteClassField"]["userErrors"],
        json!([{ "field": ["search"], "message": "Invalid search field(s): discount_class. Check the query syntax.", "code": "INVALID", "extraInfo": null }])
    );
    assert_eq!(
        data["codeDeleteUnknownField"]["userErrors"],
        json!([{ "field": ["search"], "message": "Invalid search field(s): frobnicate. Check the query syntax.", "code": "INVALID", "extraInfo": null }])
    );
    assert_eq!(
        data["automaticDeleteEmpty"]["userErrors"],
        json!([{ "field": null, "message": "One of IDs, search argument or saved search ID is required.", "code": "MISSING_ARGUMENT", "extraInfo": null }])
    );
    assert_eq!(data["automaticDeleteBlank"]["userErrors"], json!([]));
    assert_eq!(
        data["automaticDeleteTooMany"]["userErrors"],
        json!([{ "field": null, "message": "Only one of IDs, search argument or saved search ID is allowed.", "code": "TOO_MANY_ARGUMENTS", "extraInfo": null }])
    );
    assert_eq!(
        data["automaticDeleteSavedSearch"]["userErrors"],
        json!([{ "field": ["savedSearchId"], "message": "Invalid savedSearchId.", "code": "INVALID", "extraInfo": null }])
    );
    assert_eq!(data["automaticDeleteUnknownField"]["userErrors"], json!([]));
}

#[test]
fn discount_broad_bulk_search_and_saved_search_target_effective_local_catalog() {
    let clock = Arc::new(Mutex::new(utc_time(1_783_080_000)));
    let active_saved_search_id = "gid://shopify/SavedSearch/bulk-active".to_string();
    let scheduled_saved_search_id = "gid://shopify/SavedSearch/bulk-scheduled".to_string();
    let upstream_requests = Arc::new(Mutex::new(Vec::<String>::new()));
    let upstream_requests_for_transport = Arc::clone(&upstream_requests);
    let active_saved_search_id_for_transport = active_saved_search_id.clone();
    let scheduled_saved_search_id_for_transport = scheduled_saved_search_id.clone();
    let clock_for_proxy = Arc::clone(&clock);
    let mut proxy = configured_proxy(ReadMode::LiveHybrid, None)
        .with_clock(move || *clock_for_proxy.lock().unwrap())
        .with_upstream_transport(move |request| {
            upstream_requests_for_transport
                .lock()
                .unwrap()
                .push(request.body.clone());
            if request.body.contains("mutation") {
                panic!(
                    "discount broad bulk search/saved-search selectors should not forward upstream mutations: {}",
                    request.body
                );
            }
            if request.body.contains("codeDiscountSavedSearches") {
                return Response {
                    status: 200,
                    headers: Default::default(),
                    body: json!({
                        "data": {
                            "codeDiscountSavedSearches": {
                                "nodes": [
                                    {
                                        "id": active_saved_search_id_for_transport.clone(),
                                        "name": "Bulk active discounts",
                                        "query": "status:active",
                                        "resourceType": "PRICE_RULE"
                                    },
                                    {
                                        "id": scheduled_saved_search_id_for_transport.clone(),
                                        "name": "Bulk scheduled discounts",
                                        "query": "status:scheduled",
                                        "resourceType": "PRICE_RULE"
                                    }
                                ]
                            }
                        }
                    }),
                };
            }
            Response {
                status: 200,
                headers: Default::default(),
                body: json!({
                    "data": {
                        "codeNode": null,
                        "automaticNode": null,
                        "deactivated": null,
                        "activated": null,
                        "deletedAutomatic": null,
                        "discountNodes": { "nodes": [], "pageInfo": { "hasNextPage": false, "hasPreviousPage": false } }
                    }
                }),
            }
        });

    let deactivate_id = create_basic_code_discount(
        &mut proxy,
        "Bulk search deactivate code",
        "BULK-SEARCH-DEACTIVATE",
    );
    let activate = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateExpiredBulkSearchCode($input: DiscountCodeBasicInput!) {
          discountCodeBasicCreate(basicCodeDiscount: $input) {
            codeDiscountNode { id }
            userErrors { field message code extraInfo }
          }
        }
        "#,
        json!({ "input": {
            "title": "Bulk saved-search activate code",
            "code": "BULK-SAVED-ACTIVATE",
            "startsAt": "2026-08-01T00:00:00Z",
            "context": { "all": "ALL" },
            "customerGets": { "value": { "percentage": 0.1 }, "items": { "all": true } }
        }}),
    ));
    let activate_id = json_string(
        &activate.body["data"]["discountCodeBasicCreate"]["codeDiscountNode"]["id"],
        "bulk saved-search activate id",
    );
    let automatic = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateBulkSearchAutomatic($input: DiscountAutomaticBasicInput!) {
          discountAutomaticBasicCreate(automaticBasicDiscount: $input) {
            automaticDiscountNode { id }
            userErrors { field message code extraInfo }
          }
        }
        "#,
        json!({ "input": {
            "title": "Bulk saved-search automatic",
            "startsAt": "2026-06-01T00:00:00Z",
            "context": { "all": "ALL" },
            "customerGets": { "value": { "percentage": 0.1 }, "items": { "all": true } }
        }}),
    ));
    let automatic_id = json_string(
        &automatic.body["data"]["discountAutomaticBasicCreate"]["automaticDiscountNode"]["id"],
        "bulk saved-search automatic id",
    );
    let saved_search = proxy.process_request(json_graphql_request(
        r#"
        query HydrateDiscountSavedSearch {
          codeDiscountSavedSearches(first: 10) {
            nodes { id query resourceType }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        saved_search.body["data"]["codeDiscountSavedSearches"]["nodes"][0]["id"],
        json!(active_saved_search_id)
    );
    assert_eq!(
        saved_search.body["data"]["codeDiscountSavedSearches"]["nodes"][1]["id"],
        json!(scheduled_saved_search_id)
    );
    upstream_requests.lock().unwrap().clear();

    let activate = proxy.process_request(json_graphql_request(
        r#"
        mutation ActivateBulkSavedSearch($savedSearchId: ID!) {
          discountCodeBulkActivate(savedSearchId: $savedSearchId) {
            job { done }
            userErrors { field message code extraInfo }
          }
        }
        "#,
        json!({ "savedSearchId": scheduled_saved_search_id }),
    ));
    assert_eq!(
        activate.body["data"]["discountCodeBulkActivate"]["userErrors"],
        json!([])
    );

    let deactivate = proxy.process_request(json_graphql_request(
        r#"
        mutation DeactivateBulkSearch($search: String!) {
          discountCodeBulkDeactivate(search: $search) {
            job { done }
            userErrors { field message code extraInfo }
          }
        }
        "#,
        json!({ "search": "status:active" }),
    ));
    assert_eq!(
        deactivate.body["data"]["discountCodeBulkDeactivate"]["userErrors"],
        json!([])
    );

    let automatic_delete = proxy.process_request(json_graphql_request(
        r#"
        mutation DeleteAutomaticBulkSavedSearch($savedSearchId: ID!) {
          discountAutomaticBulkDelete(savedSearchId: $savedSearchId) {
            job { done }
            userErrors { field message code extraInfo }
          }
        }
        "#,
        json!({ "savedSearchId": active_saved_search_id }),
    ));
    assert_eq!(
        automatic_delete.body["data"]["discountAutomaticBulkDelete"]["userErrors"],
        json!([])
    );

    let code_delete = proxy.process_request(json_graphql_request(
        r#"
        mutation DeleteCodeBulkSearch($search: String!) {
          discountCodeBulkDelete(search: $search) {
            job { done }
            userErrors { field message code extraInfo }
          }
        }
        "#,
        json!({ "search": "status:expired" }),
    ));
    assert_eq!(
        code_delete.body["data"]["discountCodeBulkDelete"]["userErrors"],
        json!([])
    );

    let read = proxy.process_request(json_graphql_request(
        r#"
        query ReadBulkSearchSelectors($deactivateId: ID!, $activateId: ID!, $automaticId: ID!) {
          deactivated: codeDiscountNode(id: $deactivateId) { id }
          activated: codeDiscountNode(id: $activateId) { id }
          deletedAutomatic: automaticDiscountNode(id: $automaticId) { id }
          discountNodes(first: 5) { nodes { id } }
        }
        "#,
        json!({
            "deactivateId": deactivate_id,
            "activateId": activate_id,
            "automaticId": automatic_id
        }),
    ));
    assert_eq!(read.body["data"]["deactivated"], json!(null));
    assert_eq!(read.body["data"]["activated"], json!(null));
    assert_eq!(read.body["data"]["deletedAutomatic"], json!(null));
    assert_eq!(read.body["data"]["discountNodes"]["nodes"], json!([]));
    let upstream_requests = upstream_requests.lock().unwrap();
    for root in [
        "discountCodeBulkActivate",
        "discountCodeBulkDeactivate",
        "discountCodeBulkDelete",
        "discountAutomaticBulkDelete",
    ] {
        assert!(
            upstream_requests.iter().all(|body| !body.contains(root)),
            "{root} should not be forwarded upstream"
        );
    }
}

#[test]
fn discount_code_status_recomputes_from_the_proxy_clock_on_reads_and_filters() {
    let clock = Arc::new(Mutex::new(utc_time(1_783_080_000)));
    let mut proxy = snapshot_proxy_with_clock(Arc::clone(&clock));

    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateClockedDiscount($input: DiscountCodeBasicInput!) {
          discountCodeBasicCreate(basicCodeDiscount: $input) {
            codeDiscountNode {
              id
              codeDiscount {
                ... on DiscountCodeBasic {
                  status
                  startsAt
                  endsAt
                  createdAt
                  updatedAt
                }
              }
            }
            userErrors { field message code extraInfo }
          }
        }
        "#,
        json!({ "input": {
            "title": "Clocked status",
            "code": "CLOCKED-STATUS",
            "startsAt": "2026-07-02T12:00:00Z",
            "endsAt": "2026-07-04T12:00:00Z",
            "context": { "all": "ALL" },
            "customerGets": { "value": { "percentage": 0.1 }, "items": { "all": true } }
        }}),
    ));
    assert_eq!(
        create.body["data"]["discountCodeBasicCreate"]["userErrors"],
        json!([])
    );
    let node = &create.body["data"]["discountCodeBasicCreate"]["codeDiscountNode"];
    let id = json_string(&node["id"], "clocked discount id");
    assert_eq!(node["codeDiscount"]["status"], json!("ACTIVE"));
    assert_eq!(
        node["codeDiscount"]["createdAt"],
        json!("2026-07-03T12:00:00Z")
    );
    assert_eq!(
        node["codeDiscount"]["updatedAt"],
        json!("2026-07-03T12:00:00Z")
    );

    set_clock(&clock, 1_783_252_800);
    let read = proxy.process_request(json_graphql_request(
        r#"
        query ReadClockedDiscount($id: ID!, $query: String!) {
          codeDiscountNode(id: $id) {
            codeDiscount {
              ... on DiscountCodeBasic { status }
            }
          }
          discountNodes(query: $query) {
            nodes {
              id
              discount {
                ... on DiscountCodeBasic { status }
              }
            }
          }
        }
        "#,
        json!({ "id": id, "query": "status:expired" }),
    ));
    assert_eq!(
        read.body["data"]["codeDiscountNode"]["codeDiscount"]["status"],
        json!("EXPIRED")
    );
    assert_eq!(
        read.body["data"]["discountNodes"]["nodes"],
        json!([{
            "id": id,
            "discount": { "status": "EXPIRED" }
        }])
    );
}

#[test]
fn discount_mutation_timestamps_advance_with_the_proxy_clock() {
    let clock = Arc::new(Mutex::new(utc_time(1_783_080_000)));
    let mut proxy = snapshot_proxy_with_clock(Arc::clone(&clock));

    let create_id = create_basic_code_discount(&mut proxy, "Clocked timestamp", "CLOCKED-TIME");
    let created = proxy.process_request(json_graphql_request(
        r#"
        query ReadCreatedDiscount($id: ID!) {
          codeDiscountNode(id: $id) {
            codeDiscount {
              ... on DiscountCodeBasic { createdAt updatedAt status startsAt endsAt }
            }
          }
        }
        "#,
        json!({ "id": create_id }),
    ));
    let created_discount = &created.body["data"]["codeDiscountNode"]["codeDiscount"];
    assert_eq!(created_discount["createdAt"], json!("2026-07-03T12:00:00Z"));
    assert_eq!(created_discount["updatedAt"], json!("2026-07-03T12:00:00Z"));
    assert_eq!(created_discount["status"], json!("ACTIVE"));

    set_clock(&clock, 1_783_166_400);
    let update = proxy.process_request(json_graphql_request(
        r#"
        mutation UpdateClockedDiscount($id: ID!, $input: DiscountCodeBasicInput!) {
          discountCodeBasicUpdate(id: $id, basicCodeDiscount: $input) {
            codeDiscountNode {
              codeDiscount { ... on DiscountCodeBasic { createdAt updatedAt status } }
            }
            userErrors { field message code extraInfo }
          }
        }
        "#,
        json!({ "id": create_id, "input": {
            "title": "Clocked timestamp updated",
            "startsAt": "2026-04-25T00:00:00Z",
            "context": { "all": "ALL" },
            "customerGets": { "value": { "percentage": 0.2 }, "items": { "all": true } }
        }}),
    ));
    let updated_discount =
        &update.body["data"]["discountCodeBasicUpdate"]["codeDiscountNode"]["codeDiscount"];
    assert_eq!(updated_discount["createdAt"], json!("2026-07-03T12:00:00Z"));
    assert_eq!(updated_discount["updatedAt"], json!("2026-07-04T12:00:00Z"));
    assert!(updated_discount["updatedAt"].as_str() > created_discount["updatedAt"].as_str());

    set_clock(&clock, 1_783_252_800);
    let deactivate = proxy.process_request(json_graphql_request(
        r#"
        mutation DeactivateClockedDiscount($id: ID!) {
          discountCodeDeactivate(id: $id) {
            codeDiscountNode {
              codeDiscount { ... on DiscountCodeBasic { updatedAt status endsAt } }
            }
            userErrors { field message code extraInfo }
          }
        }
        "#,
        json!({ "id": create_id }),
    ));
    let deactivated_discount =
        &deactivate.body["data"]["discountCodeDeactivate"]["codeDiscountNode"]["codeDiscount"];
    assert_eq!(
        deactivated_discount["updatedAt"],
        json!("2026-07-05T12:00:00Z")
    );
    assert_eq!(deactivated_discount["status"], json!("EXPIRED"));
    assert_eq!(
        deactivated_discount["endsAt"],
        json!("2026-07-05T12:00:00Z")
    );
    assert!(deactivated_discount["updatedAt"].as_str() > updated_discount["updatedAt"].as_str());
}

#[test]
fn discount_stage_locally_roots_dispatch_by_root_field_not_operation_name_or_alias() {
    let hits = Arc::new(Mutex::new(0usize));
    let hit_counter = Arc::clone(&hits);
    let mut proxy = configured_proxy(
        ReadMode::LiveHybrid,
        Some(shopify_draft_proxy::proxy::UnsupportedMutationMode::Passthrough),
    )
    .with_upstream_transport(move |request| {
        // The discount mutation itself must always stage locally and never
        // passthrough. Reads may still hydrate a baseline for overlay results.
        assert!(
            !request.body.contains("mutation"),
            "discount mutations must not be forwarded, got: {}",
            request.body
        );
        *hit_counter.lock().unwrap() += 1;
        let data = if request.body.contains("discountNodesCount") {
            json!({ "activeCount": { "count": 0, "precision": "EXACT" } })
        } else {
            assert!(request.body.contains("codeDiscountNodeByCode"));
            json!({ "codeDiscountNodeByCode": null })
        };
        shopify_draft_proxy::proxy::Response {
            status: 200,
            headers: Default::default(),
            body: json!({ "data": data }),
        }
    });

    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateDiscount($input: DiscountCodeBasicInput!) {
          createdDiscount: discountCodeBasicCreate(basicCodeDiscount: $input) {
            codeDiscountNode {
              id
              codeDiscount {
                __typename
                ... on DiscountCodeBasic {
                  title
                  status
                  discountClasses
                  combinesWith { productDiscounts orderDiscounts shippingDiscounts }
                  codes(first: 1) { nodes { code } }
                }
              }
            }
            userErrors { field message code extraInfo }
          }
        }
        "#,
        json!({ "input": {
            "title": "Normal operation discount",
            "code": "NORMAL1404",
            "startsAt": "2026-04-27T19:31:14Z",
            "combinesWith": { "productDiscounts": false, "orderDiscounts": true, "shippingDiscounts": false },
            "context": { "all": "ALL" },
            "customerGets": { "value": { "percentage": 0.1 }, "items": { "all": true } }
        }}),
    ));

    assert_eq!(create.status, 200);
    // Exactly one upstream call: the duplicate-code uniqueness read-through. The
    // create mutation stages locally (asserted in the transport above).
    assert_eq!(*hits.lock().unwrap(), 1);
    let id = create.body["data"]["createdDiscount"]["codeDiscountNode"]["id"]
        .as_str()
        .expect("discount create should return a staged id")
        .to_string();
    assert!(id.contains("shopify-draft-proxy=synthetic"));
    assert_eq!(
        create.body["data"]["createdDiscount"]["codeDiscountNode"]["codeDiscount"]["title"],
        json!("Normal operation discount")
    );
    assert_eq!(
        create.body["data"]["createdDiscount"]["codeDiscountNode"]["codeDiscount"]["codes"]
            ["nodes"][0]["code"],
        json!("NORMAL1404")
    );
    assert_eq!(
        create.body["data"]["createdDiscount"]["userErrors"],
        json!([])
    );

    let read = proxy.process_request(json_graphql_request(
        r#"
        query ReadDiscount($id: ID!, $code: String!) {
          byId: discountNode(id: $id) { id discount { __typename ... on DiscountCodeBasic { title status } } }
          byCode: codeDiscountNodeByCode(code: $code) { id codeDiscount { __typename ... on DiscountCodeBasic { title status } } }
          activeCount: discountNodesCount(query: "status:active") { count precision }
        }
        "#,
        json!({ "id": id, "code": "NORMAL1404" }),
    ));
    assert_eq!(read.status, 200);
    assert_eq!(
        read.body["data"]["byId"]["discount"]["title"],
        json!("Normal operation discount")
    );
    assert_eq!(
        read.body["data"]["byCode"]["id"],
        read.body["data"]["byId"]["id"]
    );
    assert_eq!(
        read.body["data"]["activeCount"],
        json!({ "count": 1, "precision": "EXACT" })
    );
    assert_eq!(
        log_snapshot(&proxy)["entries"][0]["rawBody"]
            .as_str()
            .unwrap()
            .contains("mutation CreateDiscount"),
        true
    );
}

fn starts_at_required_variables(starts_at: Option<Value>) -> Value {
    let mut variables = json!({
        "basicCode": {
            "title": "StartsAt required code basic",
            "code": "STARTSAT-BASIC",
            "combinesWith": { "productDiscounts": false, "orderDiscounts": true, "shippingDiscounts": false },
            "context": { "all": "ALL" },
            "customerGets": { "value": { "percentage": 0.1 }, "items": { "all": true } }
        },
        "bxgyCode": {
            "title": "StartsAt required code BXGY",
            "code": "STARTSAT-BXGY",
            "combinesWith": { "productDiscounts": true, "orderDiscounts": false, "shippingDiscounts": false },
            "context": { "all": "ALL" },
            "customerBuys": { "value": { "quantity": "1" }, "items": { "products": { "productsToAdd": ["gid://shopify/Product/10180236017970"] } } },
            "customerGets": { "value": { "discountOnQuantity": { "quantity": "1", "effect": { "percentage": 1 } } }, "items": { "products": { "productsToAdd": ["gid://shopify/Product/10180236017970"] } } }
        },
        "freeShippingCode": {
            "title": "StartsAt required code free shipping",
            "code": "STARTSAT-SHIP",
            "combinesWith": { "productDiscounts": false, "orderDiscounts": true, "shippingDiscounts": false },
            "context": { "all": "ALL" },
            "destination": { "all": true }
        },
        "automaticBasic": {
            "title": "StartsAt required automatic basic",
            "combinesWith": { "productDiscounts": false, "orderDiscounts": true, "shippingDiscounts": false },
            "context": { "all": "ALL" },
            "customerGets": { "value": { "percentage": 0.1 }, "items": { "all": true } }
        },
        "automaticBxgy": {
            "title": "StartsAt required automatic BXGY",
            "combinesWith": { "productDiscounts": true, "orderDiscounts": false, "shippingDiscounts": false },
            "context": { "all": "ALL" },
            "customerBuys": { "value": { "quantity": "1" }, "items": { "products": { "productsToAdd": ["gid://shopify/Product/10180236017970"] } } },
            "customerGets": { "value": { "discountOnQuantity": { "quantity": "1", "effect": { "percentage": 1 } } }, "items": { "products": { "productsToAdd": ["gid://shopify/Product/10180236017970"] } } }
        },
        "automaticFreeShipping": {
            "title": "StartsAt required automatic free shipping",
            "combinesWith": { "productDiscounts": false, "orderDiscounts": true, "shippingDiscounts": false },
            "context": { "all": "ALL" },
            "destination": { "all": true }
        }
    });
    if let Some(starts_at) = starts_at {
        for key in [
            "basicCode",
            "bxgyCode",
            "freeShippingCode",
            "automaticBasic",
            "automaticBxgy",
            "automaticFreeShipping",
        ] {
            variables[key]["startsAt"] = starts_at.clone();
        }
    }
    variables
}

#[test]
fn discount_native_create_requires_starts_at_for_all_roots() {
    let mut proxy = snapshot_proxy();
    let query = r#"
        mutation DiscountStartsAtRequiredValidation(
          $basicCode: DiscountCodeBasicInput!
          $bxgyCode: DiscountCodeBxgyInput!
          $freeShippingCode: DiscountCodeFreeShippingInput!
          $automaticBasic: DiscountAutomaticBasicInput!
          $automaticBxgy: DiscountAutomaticBxgyInput!
          $automaticFreeShipping: DiscountAutomaticFreeShippingInput!
        ) {
          basicCode: discountCodeBasicCreate(basicCodeDiscount: $basicCode) { codeDiscountNode { id } userErrors { field message code extraInfo } }
          bxgyCode: discountCodeBxgyCreate(bxgyCodeDiscount: $bxgyCode) { codeDiscountNode { id } userErrors { field message code extraInfo } }
          freeShippingCode: discountCodeFreeShippingCreate(freeShippingCodeDiscount: $freeShippingCode) { codeDiscountNode { id } userErrors { field message code extraInfo } }
          automaticBasic: discountAutomaticBasicCreate(automaticBasicDiscount: $automaticBasic) { automaticDiscountNode { id } userErrors { field message code extraInfo } }
          automaticBxgy: discountAutomaticBxgyCreate(automaticBxgyDiscount: $automaticBxgy) { automaticDiscountNode { id } userErrors { field message code extraInfo } }
          automaticFreeShipping: discountAutomaticFreeShippingCreate(freeShippingAutomaticDiscount: $automaticFreeShipping) { automaticDiscountNode { id } userErrors { field message code extraInfo } }
        }
    "#;

    for variables in [
        starts_at_required_variables(None),
        starts_at_required_variables(Some(Value::Null)),
    ] {
        let response = proxy.process_request(json_graphql_request(query, variables));
        assert_eq!(response.status, 200);
        let data = &response.body["data"];
        assert_starts_at_required_error(data, "basicCode", "codeDiscountNode", "basicCodeDiscount");
        assert_starts_at_required_error(data, "bxgyCode", "codeDiscountNode", "bxgyCodeDiscount");
        assert_starts_at_required_error(
            data,
            "freeShippingCode",
            "codeDiscountNode",
            "freeShippingCodeDiscount",
        );
        assert_starts_at_required_error(
            data,
            "automaticBasic",
            "automaticDiscountNode",
            "automaticBasicDiscount",
        );
        assert_starts_at_required_error(
            data,
            "automaticBxgy",
            "automaticDiscountNode",
            "automaticBxgyDiscount",
        );
        assert_starts_at_required_error(
            data,
            "automaticFreeShipping",
            "automaticDiscountNode",
            "freeShippingAutomaticDiscount",
        );
    }
}

#[test]
fn discount_native_update_preserves_existing_starts_at_when_omitted() {
    let mut proxy = snapshot_proxy();
    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateDiscount($input: DiscountCodeBasicInput!) {
          discountCodeBasicCreate(basicCodeDiscount: $input) {
            codeDiscountNode { id codeDiscount { __typename ... on DiscountCodeBasic { startsAt } } }
            userErrors { field message code extraInfo }
          }
        }
        "#,
        json!({ "input": {
            "title": "Preserve startsAt",
            "code": "PRESERVE-STARTS-AT",
            "startsAt": "2026-04-27T19:31:14Z",
            "context": { "all": "ALL" },
            "customerGets": { "value": { "percentage": 0.1 }, "items": { "all": true } }
        }}),
    ));
    assert_eq!(
        create.body["data"]["discountCodeBasicCreate"]["userErrors"],
        json!([])
    );
    let id = json_string(
        &create.body["data"]["discountCodeBasicCreate"]["codeDiscountNode"]["id"],
        "created code discount id",
    );

    let update = proxy.process_request(json_graphql_request(
        r#"
        mutation UpdateDiscount($id: ID!, $input: DiscountCodeBasicInput!) {
          discountCodeBasicUpdate(id: $id, basicCodeDiscount: $input) {
            codeDiscountNode { id codeDiscount { __typename ... on DiscountCodeBasic { title startsAt } } }
            userErrors { field message code extraInfo }
          }
        }
        "#,
        json!({ "id": id, "input": {
            "title": "Preserved startsAt renamed",
            "code": "PRESERVE-STARTS-AT",
            "context": { "all": "ALL" },
            "customerGets": { "value": { "percentage": 0.2 }, "items": { "all": true } }
        }}),
    ));
    assert_eq!(
        update.body["data"]["discountCodeBasicUpdate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        update.body["data"]["discountCodeBasicUpdate"]["codeDiscountNode"]["codeDiscount"]["title"],
        json!("Preserved startsAt renamed")
    );
    assert_eq!(
        update.body["data"]["discountCodeBasicUpdate"]["codeDiscountNode"]["codeDiscount"]
            ["startsAt"],
        json!("2026-04-27T19:31:14Z")
    );
}

#[test]
fn discount_code_basic_update_rejects_taken_code_and_invalid_item_refs() {
    let mut proxy = snapshot_proxy();
    let (product_id, variant_id) = create_discount_ref_product(&mut proxy);
    let collection_id = create_discount_ref_collection(&mut proxy);
    let first_id = create_basic_code_discount(&mut proxy, "Reference first", "REFSAVE10");
    let second_id = create_basic_code_discount(&mut proxy, "Reference second", "REFSAVE20");

    let update = r#"
        mutation UpdateBasicCodeDiscount($id: ID!, $input: DiscountCodeBasicInput!) {
          discountCodeBasicUpdate(id: $id, basicCodeDiscount: $input) {
            codeDiscountNode { id }
            userErrors { field message code extraInfo }
          }
        }
    "#;
    let taken = proxy.process_request(json_graphql_request(
        update,
        json!({
            "id": second_id,
            "input": basic_code_discount_input("Reference second taken", "REFSAVE10", json!({ "all": true }))
        }),
    ));
    assert_eq!(
        taken.body["data"]["discountCodeBasicUpdate"]["codeDiscountNode"],
        json!(null)
    );
    assert_eq!(
        taken.body["data"]["discountCodeBasicUpdate"]["userErrors"],
        json!([{
            "field": ["basicCodeDiscount", "code"],
            "message": "Code must be unique. Please try a different code.",
            "code": "TAKEN",
            "extraInfo": null
        }])
    );

    let own_code = proxy.process_request(json_graphql_request(
        update,
        json!({
            "id": first_id.clone(),
            "input": basic_code_discount_input("Reference first unchanged code", "REFSAVE10", json!({ "all": true }))
        }),
    ));
    assert_eq!(
        own_code.body["data"]["discountCodeBasicUpdate"]["codeDiscountNode"]["id"],
        json!(first_id)
    );
    assert_eq!(
        own_code.body["data"]["discountCodeBasicUpdate"]["userErrors"],
        json!([])
    );

    let invalid_product_zero = proxy.process_request(json_graphql_request(
        update,
        json!({
            "id": first_id,
            "input": basic_code_discount_input(
                "Reference invalid product zero",
                "REFSAVE10",
                json!({ "products": { "productsToAdd": ["gid://shopify/Product/0"] } })
            )
        }),
    ));
    assert_eq!(
        invalid_product_zero.body["data"]["discountCodeBasicUpdate"]["codeDiscountNode"],
        json!(null)
    );
    assert_eq!(
        invalid_product_zero.body["data"]["discountCodeBasicUpdate"]["userErrors"],
        json!([{
            "field": ["basicCodeDiscount", "customerGets", "items", "products", "productsToAdd"],
            "message": "Product with id: 0 is invalid",
            "code": "INVALID",
            "extraInfo": null
        }])
    );

    let invalid_product_unknown = proxy.process_request(json_graphql_request(
        update,
        json!({
            "id": own_code.body["data"]["discountCodeBasicUpdate"]["codeDiscountNode"]["id"],
            "input": basic_code_discount_input(
                "Reference invalid product unknown",
                "REFSAVE10",
                json!({ "products": { "productsToAdd": ["gid://shopify/Product/999999"] } })
            )
        }),
    ));
    assert_eq!(
        invalid_product_unknown.body["data"]["discountCodeBasicUpdate"]["userErrors"],
        json!([{
            "field": ["basicCodeDiscount", "customerGets", "items", "products", "productsToAdd"],
            "message": "Product with id: 999999 is invalid",
            "code": "INVALID",
            "extraInfo": null
        }])
    );

    let invalid_variant = proxy.process_request(json_graphql_request(
        update,
        json!({
            "id": own_code.body["data"]["discountCodeBasicUpdate"]["codeDiscountNode"]["id"],
            "input": basic_code_discount_input(
                "Reference invalid variant",
                "REFSAVE10",
                json!({ "products": { "productVariantsToAdd": ["gid://shopify/ProductVariant/999998"] } })
            )
        }),
    ));
    assert_eq!(
        invalid_variant.body["data"]["discountCodeBasicUpdate"]["userErrors"],
        json!([{
            "field": ["basicCodeDiscount", "customerGets", "items", "products", "productVariantsToAdd"],
            "message": "Product variant with id: 999998 is invalid",
            "code": "INVALID",
            "extraInfo": null
        }])
    );

    let invalid_collection = proxy.process_request(json_graphql_request(
        update,
        json!({
            "id": own_code.body["data"]["discountCodeBasicUpdate"]["codeDiscountNode"]["id"],
            "input": basic_code_discount_input(
                "Reference invalid collection",
                "REFSAVE10",
                json!({ "collections": { "add": ["gid://shopify/Collection/999997"] } })
            )
        }),
    ));
    assert_eq!(
        invalid_collection.body["data"]["discountCodeBasicUpdate"]["userErrors"],
        json!([{
            "field": ["basicCodeDiscount", "customerGets", "items", "collections", "add"],
            "message": "Collection with id: 999997 is invalid",
            "code": "INVALID",
            "extraInfo": null
        }])
    );

    let conflict = proxy.process_request(json_graphql_request(
        update,
        json!({
            "id": own_code.body["data"]["discountCodeBasicUpdate"]["codeDiscountNode"]["id"],
            "input": basic_code_discount_input(
                "Reference conflict",
                "REFSAVE10",
                json!({
                    "products": { "productsToAdd": [product_id], "productVariantsToAdd": [variant_id] },
                    "collections": { "add": [collection_id] }
                })
            )
        }),
    ));
    assert_eq!(
        conflict.body["data"]["discountCodeBasicUpdate"]["codeDiscountNode"],
        json!(null)
    );
    assert_eq!(
        conflict.body["data"]["discountCodeBasicUpdate"]["userErrors"],
        json!([{
            "field": ["basicCodeDiscount", "customerGets", "items", "collections", "add"],
            "message": "Cannot entitle collections in combination with product variants or products",
            "code": "CONFLICT",
            "extraInfo": null
        }])
    );
}

#[test]
fn discount_code_update_uniqueness_applies_to_bxgy_and_free_shipping_roots() {
    let mut proxy = snapshot_proxy();
    let (product_id, _) = create_discount_ref_product(&mut proxy);

    let bxgy_first =
        create_bxgy_code_discount(&mut proxy, "BXGY reference first", "BXGYREF10", &product_id);
    let bxgy_second = create_bxgy_code_discount(
        &mut proxy,
        "BXGY reference second",
        "BXGYREF20",
        &product_id,
    );
    let bxgy_update = proxy.process_request(json_graphql_request(
        r#"
        mutation UpdateBxgyCodeDiscount($id: ID!, $input: DiscountCodeBxgyInput!) {
          discountCodeBxgyUpdate(id: $id, bxgyCodeDiscount: $input) {
            codeDiscountNode { id }
            userErrors { field message code extraInfo }
          }
        }
        "#,
        json!({
            "id": bxgy_second,
            "input": bxgy_code_discount_input("BXGY reference second taken", "BXGYREF10", &product_id)
        }),
    ));
    assert_synthetic_gid(&bxgy_first, "DiscountCodeNode");
    assert_eq!(
        bxgy_update.body["data"]["discountCodeBxgyUpdate"]["codeDiscountNode"],
        json!(null)
    );
    assert_eq!(
        bxgy_update.body["data"]["discountCodeBxgyUpdate"]["userErrors"],
        json!([{
            "field": ["bxgyCodeDiscount", "code"],
            "message": "Code must be unique. Please try a different code.",
            "code": "TAKEN",
            "extraInfo": null
        }])
    );

    let shipping_first =
        create_free_shipping_code_discount(&mut proxy, "Shipping reference first", "SHIPREF10");
    let shipping_second =
        create_free_shipping_code_discount(&mut proxy, "Shipping reference second", "SHIPREF20");
    let shipping_update = proxy.process_request(json_graphql_request(
        r#"
        mutation UpdateFreeShippingCodeDiscount($id: ID!, $input: DiscountCodeFreeShippingInput!) {
          discountCodeFreeShippingUpdate(id: $id, freeShippingCodeDiscount: $input) {
            codeDiscountNode { id }
            userErrors { field message code extraInfo }
          }
        }
        "#,
        json!({
            "id": shipping_second,
            "input": {
                "title": "Shipping reference second taken",
                "code": "SHIPREF10",
                "startsAt": "2026-04-25T00:00:00Z",
                "destination": { "all": true }
            }
        }),
    ));
    assert_synthetic_gid(&shipping_first, "DiscountCodeNode");
    assert_eq!(
        shipping_update.body["data"]["discountCodeFreeShippingUpdate"]["codeDiscountNode"],
        json!(null)
    );
    assert_eq!(
        shipping_update.body["data"]["discountCodeFreeShippingUpdate"]["userErrors"],
        json!([{
            "field": ["freeShippingCodeDiscount", "code"],
            "message": "Code must be unique. Please try a different code.",
            "code": "TAKEN",
            "extraInfo": null
        }])
    );
}

#[test]
fn discount_automatic_update_rejects_invalid_item_refs() {
    let mut proxy = snapshot_proxy();
    let (product_id, _) = create_discount_ref_product(&mut proxy);
    let collection_id = create_discount_ref_collection(&mut proxy);

    let automatic_basic = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateAutomaticBasic($input: DiscountAutomaticBasicInput!) {
          discountAutomaticBasicCreate(automaticBasicDiscount: $input) {
            automaticDiscountNode { id }
            userErrors { field message code extraInfo }
          }
        }
        "#,
        json!({ "input": {
            "title": "Automatic basic reference",
            "startsAt": "2026-04-25T00:00:00Z",
            "customerGets": { "value": { "percentage": 0.1 }, "items": { "all": true } }
        }}),
    ));
    assert_eq!(
        automatic_basic.body["data"]["discountAutomaticBasicCreate"]["userErrors"],
        json!([])
    );
    let automatic_basic_id = json_string(
        &automatic_basic.body["data"]["discountAutomaticBasicCreate"]["automaticDiscountNode"]
            ["id"],
        "automatic basic discount id",
    );

    let automatic_basic_update = proxy.process_request(json_graphql_request(
        r#"
        mutation UpdateAutomaticBasic($id: ID!, $input: DiscountAutomaticBasicInput!) {
          discountAutomaticBasicUpdate(id: $id, automaticBasicDiscount: $input) {
            automaticDiscountNode { id }
            userErrors { field message code extraInfo }
          }
        }
        "#,
        json!({
            "id": automatic_basic_id,
            "input": {
                "title": "Automatic basic invalid product",
                "startsAt": "2026-04-25T00:00:00Z",
                "customerGets": {
                    "value": { "percentage": 0.1 },
                    "items": { "products": { "productsToAdd": ["gid://shopify/Product/0"] } }
                }
            }
        }),
    ));
    assert_eq!(
        automatic_basic_update.body["data"]["discountAutomaticBasicUpdate"]
            ["automaticDiscountNode"],
        json!(null)
    );
    assert_eq!(
        automatic_basic_update.body["data"]["discountAutomaticBasicUpdate"]["userErrors"],
        json!([{
            "field": ["automaticBasicDiscount", "customerGets", "items", "products", "productsToAdd"],
            "message": "Product with id: 0 is invalid",
            "code": "INVALID",
            "extraInfo": null
        }])
    );

    let automatic_bxgy = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateAutomaticBxgy($input: DiscountAutomaticBxgyInput!) {
          discountAutomaticBxgyCreate(automaticBxgyDiscount: $input) {
            automaticDiscountNode { id }
            userErrors { field message code extraInfo }
          }
        }
        "#,
        json!({ "input": {
            "title": "Automatic BXGY reference",
            "startsAt": "2026-04-25T00:00:00Z",
            "customerBuys": {
                "value": { "quantity": "1" },
                "items": { "products": { "productsToAdd": [product_id.clone()] } }
            },
            "customerGets": {
                "value": {
                    "discountOnQuantity": {
                        "quantity": "1",
                        "effect": { "percentage": 0.5 }
                    }
                },
                "items": { "products": { "productsToAdd": [product_id.clone()] } }
            }
        }}),
    ));
    assert_eq!(
        automatic_bxgy.body["data"]["discountAutomaticBxgyCreate"]["userErrors"],
        json!([])
    );
    let automatic_bxgy_id = json_string(
        &automatic_bxgy.body["data"]["discountAutomaticBxgyCreate"]["automaticDiscountNode"]["id"],
        "automatic bxgy discount id",
    );

    let automatic_bxgy_update = proxy.process_request(json_graphql_request(
        r#"
        mutation UpdateAutomaticBxgy($id: ID!, $input: DiscountAutomaticBxgyInput!) {
          discountAutomaticBxgyUpdate(id: $id, automaticBxgyDiscount: $input) {
            automaticDiscountNode { id }
            userErrors { field message code extraInfo }
          }
        }
        "#,
        json!({
            "id": automatic_bxgy_id,
            "input": {
                "title": "Automatic BXGY conflict",
                "startsAt": "2026-04-25T00:00:00Z",
                "customerBuys": {
                    "value": { "quantity": "1" },
                    "items": { "products": { "productsToAdd": [product_id.clone()] } }
                },
                "customerGets": {
                    "value": {
                        "discountOnQuantity": {
                            "quantity": "1",
                            "effect": { "percentage": 0.5 }
                        }
                    },
                    "items": {
                        "products": { "productsToAdd": [product_id] },
                        "collections": { "add": [collection_id] }
                    }
                }
            }
        }),
    ));
    assert_eq!(
        automatic_bxgy_update.body["data"]["discountAutomaticBxgyUpdate"]["automaticDiscountNode"],
        json!(null)
    );
    assert_eq!(
        automatic_bxgy_update.body["data"]["discountAutomaticBxgyUpdate"]["userErrors"],
        json!([{
            "field": ["automaticBxgyDiscount", "customerGets", "items", "collections", "add"],
            "message": "Cannot entitle collections in combination with product variants or products",
            "code": "CONFLICT",
            "extraInfo": null
        }])
    );
}

#[test]
fn discount_code_app_title_validation_matches_shopify() {
    let mut proxy =
        configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport(|request| {
            let body = serde_json::from_str::<Value>(&request.body)
                .expect("upstream function lookup request should parse");
            assert!(
                body["query"]
                    .as_str()
                    .is_some_and(|query| query.contains("ShopifyFunctionByHandle")),
                "expected app discount Function lookup, got {body}"
            );
            Response {
                status: 200,
                headers: Default::default(),
                body: json!({
                    "data": {
                        "shopifyFunctions": {
                            "nodes": [{
                                "id": "gid://shopify/ShopifyFunction/discount-function",
                                "title": "Discount Function",
                                "handle": "discount-function",
                                "apiType": "DISCOUNT",
                                "description": "Local discount function",
                                "appKey": "discount-app-key",
                                "app": {
                                    "id": "gid://shopify/App/discount-app",
                                    "title": "Discount App",
                                    "handle": "discount-app",
                                    "apiKey": "discount-app-key"
                                }
                            }]
                        }
                    }
                }),
            }
        });

    let long_title = "x".repeat(256);
    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation CodeAppTitleCreate(
          $blank: DiscountCodeAppInput!
          $omitted: DiscountCodeAppInput!
          $long: DiscountCodeAppInput!
          $automatic: DiscountAutomaticAppInput!
        ) {
          blank: discountCodeAppCreate(codeAppDiscount: $blank) {
            codeAppDiscount { discountId title }
            userErrors { field message code extraInfo }
          }
          omitted: discountCodeAppCreate(codeAppDiscount: $omitted) {
            codeAppDiscount { discountId title }
            userErrors { field message code extraInfo }
          }
          long: discountCodeAppCreate(codeAppDiscount: $long) {
            codeAppDiscount { discountId title }
            userErrors { field message code extraInfo }
          }
          automatic: discountAutomaticAppCreate(automaticAppDiscount: $automatic) {
            automaticAppDiscount { discountId title }
            userErrors { field message code extraInfo }
          }
        }
        "#,
        json!({
            "blank": {
                "title": " ",
                "code": "APP-BLANK-TITLE",
                "startsAt": "2026-05-05T00:00:00Z",
                "functionHandle": "discount-function",
                "discountClasses": ["ORDER"]
            },
            "omitted": {
                "code": "APP-OMITTED-TITLE",
                "startsAt": "2026-05-05T00:00:00Z",
                "functionHandle": "discount-function",
                "discountClasses": ["ORDER"]
            },
            "long": {
                "title": long_title,
                "code": "APP-LONG-TITLE",
                "startsAt": "2026-05-05T00:00:00Z",
                "functionHandle": "discount-function",
                "discountClasses": ["ORDER"]
            },
            "automatic": {
                "title": "Automatic setup",
                "startsAt": "2026-05-05T00:00:00Z",
                "functionHandle": "discount-function",
                "discountClasses": ["ORDER"]
            }
        }),
    ));
    assert_eq!(create.status, 200);
    assert_eq!(
        create.body["data"]["blank"],
        json!({
            "codeAppDiscount": null,
            "userErrors": [{
                "field": ["codeAppDiscount", "title"],
                "message": "can't be blank",
                "code": "INVALID",
                "extraInfo": null
            }]
        })
    );
    assert_eq!(
        create.body["data"]["omitted"],
        json!({
            "codeAppDiscount": null,
            "userErrors": [{
                "field": ["codeAppDiscount", "title"],
                "message": "Required argument not found.",
                "code": "INVALID",
                "extraInfo": null
            }]
        })
    );
    assert_eq!(
        create.body["data"]["long"],
        json!({
            "codeAppDiscount": null,
            "userErrors": [{
                "field": ["codeAppDiscount", "title"],
                "message": "is too long (maximum is 255 characters)",
                "code": "INVALID",
                "extraInfo": null
            }]
        })
    );
    assert_eq!(create.body["data"]["automatic"]["userErrors"], json!([]));

    let setup = proxy.process_request(json_graphql_request(
        r#"
        mutation CodeAppTitleUpdateSetup($input: DiscountCodeAppInput!) {
          discountCodeAppCreate(codeAppDiscount: $input) {
            codeAppDiscount { discountId title }
            userErrors { field message code extraInfo }
          }
        }
        "#,
        json!({
            "input": {
                "title": "Code app setup",
                "code": "APP-TITLE-SETUP",
                "startsAt": "2026-05-05T00:00:00Z",
                "functionHandle": "discount-function",
                "discountClasses": ["ORDER"]
            }
        }),
    ));
    assert_eq!(setup.status, 200);
    assert_eq!(
        setup.body["data"]["discountCodeAppCreate"]["userErrors"],
        json!([])
    );
    let code_id = json_string(
        &setup.body["data"]["discountCodeAppCreate"]["codeAppDiscount"]["discountId"],
        "code app discount id",
    );
    let automatic_id = json_string(
        &create.body["data"]["automatic"]["automaticAppDiscount"]["discountId"],
        "automatic app discount id",
    );

    let update = proxy.process_request(json_graphql_request(
        r#"
        mutation CodeAppTitleUpdate(
          $codeId: ID!
          $automaticId: ID!
          $blank: DiscountCodeAppInput!
          $omitted: DiscountCodeAppInput!
          $long: DiscountCodeAppInput!
          $automaticBlank: DiscountAutomaticAppInput!
        ) {
          blank: discountCodeAppUpdate(id: $codeId, codeAppDiscount: $blank) {
            codeAppDiscount { discountId title }
            userErrors { field message code extraInfo }
          }
          omitted: discountCodeAppUpdate(id: $codeId, codeAppDiscount: $omitted) {
            codeAppDiscount { discountId title }
            userErrors { field message code extraInfo }
          }
          long: discountCodeAppUpdate(id: $codeId, codeAppDiscount: $long) {
            codeAppDiscount { discountId title }
            userErrors { field message code extraInfo }
          }
          automaticBlank: discountAutomaticAppUpdate(id: $automaticId, automaticAppDiscount: $automaticBlank) {
            automaticAppDiscount { discountId title }
            userErrors { field message code extraInfo }
          }
        }
        "#,
        json!({
            "codeId": code_id,
            "automaticId": automatic_id,
            "blank": {
                "title": "",
                "code": "APP-TITLE-UP-BLANK",
                "startsAt": "2026-05-05T00:00:00Z",
                "functionHandle": "discount-function",
                "discountClasses": ["ORDER"]
            },
            "omitted": {
                "code": "APP-TITLE-UP-OMITTED",
                "startsAt": "2026-05-05T00:00:00Z",
                "functionHandle": "discount-function",
                "discountClasses": ["ORDER"]
            },
            "long": {
                "title": "y".repeat(256),
                "code": "APP-TITLE-UP-LONG",
                "startsAt": "2026-05-05T00:00:00Z",
                "functionHandle": "discount-function",
                "discountClasses": ["ORDER"]
            },
            "automaticBlank": {
                "title": "",
                "startsAt": "2026-05-05T00:00:00Z",
                "functionHandle": "discount-function",
                "discountClasses": ["ORDER"]
            }
        }),
    ));
    assert_eq!(update.status, 200);
    assert_eq!(
        update.body["data"]["blank"],
        json!({
            "codeAppDiscount": null,
            "userErrors": [{
                "field": ["codeAppDiscount", "title"],
                "message": "can't be blank",
                "code": "INVALID",
                "extraInfo": null
            }]
        })
    );
    assert_eq!(update.body["data"]["omitted"]["userErrors"], json!([]));
    assert_eq!(
        update.body["data"]["omitted"]["codeAppDiscount"]["title"],
        json!("Code app setup")
    );
    assert_eq!(
        update.body["data"]["long"],
        json!({
            "codeAppDiscount": null,
            "userErrors": [{
                "field": ["codeAppDiscount", "title"],
                "message": "is too long (maximum is 255 characters)",
                "code": "INVALID",
                "extraInfo": null
            }]
        })
    );
    assert_eq!(
        update.body["data"]["automaticBlank"],
        json!({
            "automaticAppDiscount": null,
            "userErrors": [{
                "field": ["automaticAppDiscount", "title"],
                "message": "Title can't be blank.",
                "code": "INVALID",
                "extraInfo": null
            }]
        })
    );
}

#[test]
fn discount_app_lifecycle_stages_updates_reads_and_deletes_without_local_runtime_fixture() {
    let mut proxy = configured_proxy(ReadMode::LiveHybrid, None)
        .with_upstream_transport(|request| discount_app_function_upstream_response(request, true));

    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation DiscountAppLifecycle($codeInput: DiscountCodeAppInput!, $automaticInput: DiscountAutomaticAppInput!) {
          discountCodeAppCreate(codeAppDiscount: $codeInput) {
            codeAppDiscount {
              __typename
              discountId
              title
              status
              usageLimit
              combinesWith { orderDiscounts productDiscounts shippingDiscounts }
              codes(first: 5) { nodes { code } }
              appDiscountType { functionId title description }
            }
            userErrors { field message code extraInfo }
          }
          discountAutomaticAppCreate(automaticAppDiscount: $automaticInput) {
            automaticAppDiscount {
              __typename
              discountId
              title
              status
              recurringCycleLimit
              appDiscountType { functionId title description }
            }
            userErrors { field message code extraInfo }
          }
        }
        "#,
        json!({
            "codeInput": {
                "title": "App lifecycle code",
                "code": "APP-LIFE",
                "startsAt": "2026-05-05T00:00:00Z",
                "functionHandle": "discount-function",
                "usageLimit": 10,
                "combinesWith": {
                    "orderDiscounts": true,
                    "productDiscounts": false,
                    "shippingDiscounts": true
                },
                "discountClasses": ["ORDER"]
            },
            "automaticInput": {
                "title": "App lifecycle automatic",
                "startsAt": "2026-05-05T00:00:00Z",
                "functionHandle": "discount-function",
                "recurringCycleLimit": 0,
                "discountClasses": ["ORDER"]
            }
        }),
    ));
    assert_eq!(create.status, 200);
    assert_eq!(
        create.body["data"]["discountCodeAppCreate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        create.body["data"]["discountAutomaticAppCreate"]["userErrors"],
        json!([])
    );
    let code_id = json_string(
        &create.body["data"]["discountCodeAppCreate"]["codeAppDiscount"]["discountId"],
        "code app lifecycle id",
    );
    let automatic_id = json_string(
        &create.body["data"]["discountAutomaticAppCreate"]["automaticAppDiscount"]["discountId"],
        "automatic app lifecycle id",
    );
    assert_synthetic_gid(&code_id, "DiscountCodeNode");
    assert_synthetic_gid(&automatic_id, "DiscountAutomaticNode");
    assert_eq!(
        create.body["data"]["discountCodeAppCreate"]["codeAppDiscount"]["appDiscountType"]
            ["functionId"],
        json!("discount-function")
    );
    assert_eq!(
        create.body["data"]["discountCodeAppCreate"]["codeAppDiscount"]["codes"],
        json!({ "nodes": [{ "code": "APP-LIFE" }] })
    );

    let update = proxy.process_request(json_graphql_request(
        r#"
        mutation DiscountAppLifecycleUpdate(
          $codeId: ID!
          $codeInput: DiscountCodeAppInput!
          $automaticId: ID!
          $automaticInput: DiscountAutomaticAppInput!
        ) {
          discountCodeAppUpdate(id: $codeId, codeAppDiscount: $codeInput) {
            codeAppDiscount { discountId title codes(first: 5) { nodes { code } } }
            userErrors { field message code extraInfo }
          }
          discountAutomaticAppUpdate(id: $automaticId, automaticAppDiscount: $automaticInput) {
            automaticAppDiscount { discountId title recurringCycleLimit }
            userErrors { field message code extraInfo }
          }
        }
        "#,
        json!({
            "codeId": code_id,
            "codeInput": {
                "title": "App lifecycle code updated",
                "code": "APP-LIFE-UP",
                "startsAt": "2026-05-05T00:00:00Z",
                "functionHandle": "discount-function",
                "discountClasses": ["ORDER"]
            },
            "automaticId": automatic_id,
            "automaticInput": {
                "title": "App lifecycle automatic updated",
                "startsAt": "2026-05-05T00:00:00Z",
                "functionHandle": "discount-function",
                "recurringCycleLimit": 2,
                "discountClasses": ["ORDER"]
            }
        }),
    ));
    assert_eq!(
        update.body["data"]["discountCodeAppUpdate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        update.body["data"]["discountCodeAppUpdate"]["codeAppDiscount"]["title"],
        json!("App lifecycle code updated")
    );
    assert_eq!(
        update.body["data"]["discountCodeAppUpdate"]["codeAppDiscount"]["codes"],
        json!({ "nodes": [{ "code": "APP-LIFE-UP" }] })
    );
    assert_eq!(
        update.body["data"]["discountAutomaticAppUpdate"]["automaticAppDiscount"]
            ["recurringCycleLimit"],
        json!(2)
    );

    let deactivate = proxy.process_request(json_graphql_request(
        r#"
        mutation DiscountAutomaticAppLifecycleDeactivate($id: ID!) {
          discountAutomaticDeactivate(id: $id) {
            automaticDiscountNode {
              id
              automaticDiscount { __typename ... on DiscountAutomaticApp { title status endsAt appDiscountType { functionId } } }
            }
            userErrors { field message code extraInfo }
          }
        }
        "#,
        json!({ "id": automatic_id }),
    ));
    assert_eq!(
        deactivate.body["data"]["discountAutomaticDeactivate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        deactivate.body["data"]["discountAutomaticDeactivate"]["automaticDiscountNode"]
            ["automaticDiscount"]["status"],
        json!("EXPIRED")
    );
    assert_datetime_string(
        &deactivate.body["data"]["discountAutomaticDeactivate"]["automaticDiscountNode"]
            ["automaticDiscount"]["endsAt"],
        "automatic app deactivate endsAt",
    );

    let activate = proxy.process_request(json_graphql_request(
        r#"
        mutation DiscountAutomaticAppLifecycleActivate($id: ID!) {
          discountAutomaticActivate(id: $id) {
            automaticDiscountNode {
              id
              automaticDiscount { __typename ... on DiscountAutomaticApp { title status endsAt appDiscountType { functionId } } }
            }
            userErrors { field message code extraInfo }
          }
        }
        "#,
        json!({ "id": automatic_id }),
    ));
    assert_eq!(
        activate.body["data"]["discountAutomaticActivate"]["automaticDiscountNode"]
            ["automaticDiscount"]["status"],
        json!("ACTIVE")
    );
    assert_eq!(
        activate.body["data"]["discountAutomaticActivate"]["automaticDiscountNode"]
            ["automaticDiscount"]["endsAt"],
        json!(null)
    );

    let read = proxy.process_request(json_graphql_request(
        r#"
        query DiscountAppLifecycleRead($codeId: ID!, $automaticId: ID!, $code: String!) {
          codeDiscountNode(id: $codeId) {
            codeDiscount { ... on DiscountCodeApp { title appDiscountType { functionId } codes(first: 5) { nodes { code } } } }
          }
          codeDiscountNodeByCode(code: $code) { id }
          automaticDiscountNode(id: $automaticId) {
            automaticDiscount { ... on DiscountAutomaticApp { title recurringCycleLimit appDiscountType { functionId } } }
          }
          appCount: discountNodesCount(query: "type:app") { count precision }
        }
        "#,
        json!({ "codeId": code_id, "automaticId": automatic_id, "code": "APP-LIFE-UP" }),
    ));
    assert_eq!(
        read.body["data"]["codeDiscountNode"]["codeDiscount"]["title"],
        json!("App lifecycle code updated")
    );
    assert_eq!(
        read.body["data"]["codeDiscountNodeByCode"]["id"],
        json!(code_id)
    );
    assert_eq!(
        read.body["data"]["automaticDiscountNode"]["automaticDiscount"]["title"],
        json!("App lifecycle automatic updated")
    );
    assert_eq!(
        read.body["data"]["appCount"],
        json!({ "count": 2, "precision": "EXACT" })
    );

    let delete = proxy.process_request(json_graphql_request(
        r#"
        mutation DiscountAutomaticAppLifecycleDelete($id: ID!) {
          discountAutomaticDelete(id: $id) {
            deletedAutomaticDiscountId
            userErrors { field message code extraInfo }
          }
        }
        "#,
        json!({ "id": automatic_id }),
    ));
    assert_eq!(
        delete.body["data"]["discountAutomaticDelete"],
        json!({
            "deletedAutomaticDiscountId": automatic_id,
            "userErrors": []
        })
    );
    let read_after_delete = proxy.process_request(json_graphql_request(
        r#"query DiscountAutomaticAppReadAfterDelete($id: ID!) {
          automaticDiscountNode(id: $id) { id }
          discountNodesCount(query: "type:app") { count precision }
        }"#,
        json!({ "id": automatic_id }),
    ));
    assert_eq!(
        read_after_delete.body["data"]["automaticDiscountNode"],
        json!(null)
    );
    assert_eq!(
        read_after_delete.body["data"]["discountNodesCount"],
        json!({ "count": 1, "precision": "EXACT" })
    );
}

#[test]
fn discount_nodes_count_lone_read_uses_upstream_baseline_after_staged_app_create() {
    let mut proxy =
        configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport(|request| {
            let body = serde_json::from_str::<Value>(&request.body)
                .expect("discount count upstream body should parse");
            let query = body["query"].as_str().unwrap_or_default();
            if query.contains("discountNodesCount") {
                return Response {
                    status: 200,
                    headers: Default::default(),
                    body: json!({
                        "data": {
                            "exact": { "count": 2, "precision": "EXACT" },
                            "limited": { "count": 2, "precision": "EXACT" }
                        }
                    }),
                };
            }
            discount_app_function_upstream_response(request, true)
        });

    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateAppDiscount($input: DiscountAutomaticAppInput!) {
          discountAutomaticAppCreate(automaticAppDiscount: $input) {
            automaticAppDiscount { discountId title }
            userErrors { field message code extraInfo }
          }
        }
        "#,
        json!({
            "input": {
                "title": "Count baseline app create",
                "startsAt": "2026-05-05T00:00:00Z",
                "functionHandle": "discount-function",
                "discountClasses": ["ORDER"]
            }
        }),
    ));
    assert_eq!(create.status, 200);
    assert_eq!(
        create.body["data"]["discountAutomaticAppCreate"]["userErrors"],
        json!([])
    );

    let read = proxy.process_request(json_graphql_request(
        r#"
        query CountOnlyAfterCreate {
          exact: discountNodesCount(query: "type:app") { count precision }
          limited: discountNodesCount(query: "type:app", limit: 2) { count precision }
        }
        "#,
        json!({}),
    ));

    assert_eq!(read.status, 200);
    assert_eq!(
        read.body["data"]["exact"],
        json!({ "count": 3, "precision": "EXACT" })
    );
    assert_eq!(
        read.body["data"]["limited"],
        json!({ "count": 2, "precision": "AT_LEAST" })
    );
}

#[test]
fn discount_nodes_count_lone_read_uses_upstream_baseline_after_staged_app_delete() {
    let upstream_id = "gid://shopify/DiscountAutomaticNode/901";
    let mut proxy =
        configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport(move |request| {
            let body = serde_json::from_str::<Value>(&request.body)
                .expect("discount count upstream body should parse");
            let query = body["query"].as_str().unwrap_or_default();
            if query.contains("automaticDiscountNode") {
                return Response {
                    status: 200,
                    headers: Default::default(),
                    body: json!({
                        "data": {
                            "automaticDiscountNode": {
                                "__typename": "DiscountAutomaticNode",
                                "id": upstream_id,
                                "automaticDiscount": {
                                    "__typename": "DiscountAutomaticApp",
                                    "title": "Upstream app discount",
                                    "status": "ACTIVE",
                                    "startsAt": "2026-05-01T00:00:00Z",
                                    "endsAt": null,
                                    "createdAt": "2026-05-01T00:00:00Z",
                                    "updatedAt": "2026-05-01T00:00:00Z",
                                    "discountClasses": ["ORDER"],
                                    "appDiscountType": {
                                        "functionId": "gid://shopify/ShopifyFunction/discount-function"
                                    }
                                }
                            }
                        }
                    }),
                };
            }
            if query.contains("discountNodesCount") {
                return Response {
                    status: 200,
                    headers: Default::default(),
                    body: json!({
                        "data": {
                            "discountNodesCount": { "count": 2, "precision": "EXACT" }
                        }
                    }),
                };
            }
            discount_app_function_upstream_response(request, true)
        });

    let hydrate = proxy.process_request(json_graphql_request(
        r#"
        query HydrateAutomaticAppDiscount($id: ID!) {
          automaticDiscountNode(id: $id) {
            id
            automaticDiscount {
              __typename
              ... on DiscountAutomaticApp {
                title
                status
                startsAt
                endsAt
                createdAt
                updatedAt
                discountClasses
                appDiscountType { functionId }
              }
            }
          }
        }
        "#,
        json!({ "id": upstream_id }),
    ));
    assert_eq!(hydrate.status, 200);
    assert_eq!(
        hydrate.body["data"]["automaticDiscountNode"]["id"],
        json!(upstream_id)
    );

    let delete = proxy.process_request(json_graphql_request(
        r#"
        mutation DeleteUpstreamAppDiscount($id: ID!) {
          discountAutomaticDelete(id: $id) {
            deletedAutomaticDiscountId
            userErrors { field message code extraInfo }
          }
        }
        "#,
        json!({ "id": upstream_id }),
    ));
    assert_eq!(
        delete.body["data"]["discountAutomaticDelete"],
        json!({
            "deletedAutomaticDiscountId": upstream_id,
            "userErrors": []
        })
    );

    let read = proxy.process_request(json_graphql_request(
        r#"
        query CountOnlyAfterDelete {
          discountNodesCount(query: "type:app") { count precision }
        }
        "#,
        json!({}),
    ));

    assert_eq!(read.status, 200);
    assert_eq!(
        read.body["data"]["discountNodesCount"],
        json!({ "count": 1, "precision": "EXACT" })
    );
}

#[test]
fn discount_app_function_id_hydrate_matches_live_schema_without_function_handle() {
    let hits = Arc::new(Mutex::new(Vec::new()));
    let hits_for_transport = hits.clone();
    let mut proxy =
        configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport(move |request| {
            let body = serde_json::from_str::<Value>(&request.body)
                .expect("discount app upstream body should parse");
            hits_for_transport.lock().unwrap().push(body.clone());
            discount_app_function_upstream_response(request, true)
        });

    let response = proxy.process_request(json_graphql_request(
        r#"
        mutation DiscountAppFunctionId($input: DiscountCodeAppInput!) {
          discountCodeAppCreate(codeAppDiscount: $input) {
            codeAppDiscount {
              discountId
              appDiscountType { functionId title description }
            }
            userErrors { field message code extraInfo }
          }
        }
        "#,
        json!({
            "input": {
                "title": "Function id app discount",
                "code": "APP-FUNCTION-ID",
                "startsAt": "2026-05-05T00:00:00Z",
                "functionId": "gid://shopify/ShopifyFunction/discount-function",
                "discountClasses": ["ORDER"]
            }
        }),
    ));

    assert_eq!(response.status, 200);
    assert_eq!(
        response.body["data"]["discountCodeAppCreate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        response.body["data"]["discountCodeAppCreate"]["codeAppDiscount"]["appDiscountType"]
            ["functionId"],
        json!("gid://shopify/ShopifyFunction/discount-function")
    );

    let hits = hits.lock().unwrap();
    let hydrate = hits
        .iter()
        .find(|body| {
            body["query"]
                .as_str()
                .is_some_and(|query| query.contains("ShopifyFunctionById"))
        })
        .expect("functionId app discount create should hydrate the Function by id");
    let query = hydrate["query"]
        .as_str()
        .expect("Function hydrate query should be a string");
    assert!(
        !query.contains("\n    handle\n"),
        "live 2026-04 ShopifyFunction does not expose handle: {query}"
    );
}

#[test]
fn discount_app_activation_fails_when_backing_function_is_unavailable() {
    let mut proxy = configured_proxy(ReadMode::LiveHybrid, None)
        .with_upstream_transport(|request| discount_app_function_upstream_response(request, false));

    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation DiscountActivationFailureCreate(
          $codeInput: DiscountCodeAppInput!
          $automaticInput: DiscountAutomaticAppInput!
        ) {
          discountCodeAppCreate(codeAppDiscount: $codeInput) {
            codeAppDiscount { discountId }
            userErrors { field message code extraInfo }
          }
          discountAutomaticAppCreate(automaticAppDiscount: $automaticInput) {
            automaticAppDiscount { discountId }
            userErrors { field message code extraInfo }
          }
        }
        "#,
        json!({
            "codeInput": {
                "title": "Activation failure code app",
                "code": "ACTIVATEFAILBASE",
                "startsAt": "2026-05-05T00:00:00Z",
                "functionHandle": "discount-function",
                "discountClasses": ["ORDER"]
            },
            "automaticInput": {
                "title": "Activation failure automatic app",
                "startsAt": "2026-05-05T00:00:00Z",
                "functionHandle": "discount-function",
                "discountClasses": ["ORDER"]
            }
        }),
    ));
    let code_id = json_string(
        &create.body["data"]["discountCodeAppCreate"]["codeAppDiscount"]["discountId"],
        "code activation failure id",
    );
    let automatic_id = json_string(
        &create.body["data"]["discountAutomaticAppCreate"]["automaticAppDiscount"]["discountId"],
        "automatic activation failure id",
    );

    let response = proxy.process_request(json_graphql_request(
        r#"
        mutation DiscountActivationFailure($codeId: ID!, $automaticId: ID!) {
          code: discountCodeActivate(id: $codeId) {
            codeDiscountNode { id }
            userErrors { field message code extraInfo }
          }
          automatic: discountAutomaticActivate(id: $automaticId) {
            automaticDiscountNode { id }
            userErrors { field message code extraInfo }
          }
        }
        "#,
        json!({ "codeId": code_id, "automaticId": automatic_id }),
    ));

    let expected_error = json!([{
        "field": ["base"],
        "message": "Discount could not be activated.",
        "code": "INTERNAL_ERROR",
        "extraInfo": null
    }]);
    assert_eq!(
        response.body["data"]["code"]["codeDiscountNode"],
        json!(null)
    );
    assert_eq!(response.body["data"]["code"]["userErrors"], expected_error);
    assert_eq!(
        response.body["data"]["automatic"]["automaticDiscountNode"],
        json!(null)
    );
    assert_eq!(
        response.body["data"]["automatic"]["userErrors"],
        expected_error
    );
}

#[test]
fn discount_generic_handler_validates_input_and_handles_lifecycle_by_arguments() {
    let mut proxy = snapshot_proxy();

    let invalid = proxy.process_request(json_graphql_request(
        r#"
        mutation AnyName($input: DiscountCodeBasicInput!) {
          discountCodeBasicCreate(basicCodeDiscount: $input) {
            codeDiscountNode { id }
            userErrors { field message code extraInfo }
          }
        }
        "#,
        json!({ "input": {
            "title": " ",
            "startsAt": "2026-04-27T19:31:14Z",
            "context": { "all": "ALL" },
            "customerSelection": { "all": true },
            "minimumRequirement": {
                "quantity": { "greaterThanOrEqualToQuantity": "1" },
                "subtotal": { "greaterThanOrEqualToSubtotal": "1.00" }
            },
            "customerGets": {
                "value": {
                    "percentage": 1.5
                },
                "items": { "all": true }
            }
        }}),
    ));
    assert_eq!(invalid.status, 200);
    assert_eq!(
        invalid.body["data"]["discountCodeBasicCreate"]["codeDiscountNode"],
        json!(null)
    );
    assert!(
        invalid.body["data"]["discountCodeBasicCreate"]["userErrors"]
            .as_array()
            .unwrap()
            .iter()
            .any(|error| error["field"] == json!(["basicCodeDiscount", "code"]))
    );
    assert!(
        invalid.body["data"]["discountCodeBasicCreate"]["userErrors"]
            .as_array()
            .unwrap()
            .iter()
            .any(|error| error["field"] == json!(["basicCodeDiscount", "context"]))
    );

    let created = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateDiscount($input: DiscountCodeBasicInput!) {
          discountCodeBasicCreate(basicCodeDiscount: $input) {
            codeDiscountNode { id codeDiscount { __typename ... on DiscountCodeBasic { title status endsAt } } }
            userErrors { field message code extraInfo }
          }
        }
        "#,
        json!({ "input": {
            "title": "Lifecycle discount",
            "code": "LIFE1404",
            "startsAt": "2026-04-27T19:31:14Z",
            "context": { "all": "ALL" },
            "customerGets": { "value": { "percentage": 0.1 }, "items": { "all": true } }
        }}),
    ));
    let id = created.body["data"]["discountCodeBasicCreate"]["codeDiscountNode"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(
        created.body["data"]["discountCodeBasicCreate"]["codeDiscountNode"]["codeDiscount"]
            ["status"],
        json!("ACTIVE")
    );

    let deactivated = proxy.process_request(json_graphql_request(
        r#"
        mutation Whatever($id: ID!) {
          discountCodeDeactivate(id: $id) {
            codeDiscountNode { id codeDiscount { __typename ... on DiscountCodeBasic { status endsAt } } }
            userErrors { field message code extraInfo }
          }
        }
        "#,
        json!({ "id": id }),
    ));
    assert_eq!(
        deactivated.body["data"]["discountCodeDeactivate"]["codeDiscountNode"]["codeDiscount"]
            ["status"],
        json!("EXPIRED")
    );

    let deleted = proxy.process_request(json_graphql_request(
        r#"
        mutation DeleteIt($id: ID!) {
          discountCodeDelete(id: $id) { deletedCodeDiscountId userErrors { field message code extraInfo } }
        }
        "#,
        json!({ "id": deactivated.body["data"]["discountCodeDeactivate"]["codeDiscountNode"]["id"].clone() }),
    ));
    assert_eq!(
        deleted.body["data"]["discountCodeDelete"]["userErrors"],
        json!([])
    );

    let missing_activate = proxy.process_request(json_graphql_request(
        r#"
        mutation Missing($id: ID!) {
          discountCodeActivate(id: $id) { codeDiscountNode { id } userErrors { field message code extraInfo } }
        }
        "#,
        json!({ "id": "gid://shopify/DiscountCodeNode/not-found" }),
    ));
    assert_eq!(
        missing_activate.body["data"]["discountCodeActivate"]["userErrors"][0]["field"],
        json!(["id"])
    );
}

fn discount_minimum_requirement_conflict_input(code: Option<&str>) -> Value {
    let mut input = json!({
        "title": "Minimum requirement conflict",
        "startsAt": "2026-04-27T19:31:14Z",
        "context": { "all": "ALL" },
        "customerGets": {
            "value": { "percentage": 0.1 },
            "items": { "all": true }
        },
        "minimumRequirement": {
            "quantity": { "greaterThanOrEqualToQuantity": "2" },
            "subtotal": { "greaterThanOrEqualToSubtotal": "10.00" }
        }
    });
    if let Some(code) = code {
        input
            .as_object_mut()
            .unwrap()
            .insert("code".to_string(), json!(code));
    }
    input
}

fn discount_minimum_requirement_conflict_errors(input_arg: &str) -> Value {
    json!([
        {
            "field": [
                input_arg,
                "minimumRequirement",
                "subtotal",
                "greaterThanOrEqualToSubtotal"
            ],
            "message": "Minimum subtotal cannot be defined when minimum quantity is.",
            "code": "CONFLICT",
            "extraInfo": null
        },
        {
            "field": [
                input_arg,
                "minimumRequirement",
                "quantity",
                "greaterThanOrEqualToQuantity"
            ],
            "message": "Minimum quantity cannot be defined when minimum subtotal is.",
            "code": "CONFLICT",
            "extraInfo": null
        }
    ])
}

fn discount_minimum_requirement_bound_error(
    input_arg: &str,
    requirement: &str,
    value_field: &str,
    message: &str,
) -> Value {
    json!([{
        "field": [input_arg, "minimumRequirement", requirement, value_field],
        "message": message,
        "code": "LESS_THAN",
        "extraInfo": null
    }])
}

#[test]
fn discount_minimum_requirement_conflict_errors_use_concrete_paths() {
    let mut proxy = snapshot_proxy();

    let setup = proxy.process_request(json_graphql_request(
        r#"
        mutation MinimumRequirementConflictSetup(
          $codeInput: DiscountCodeBasicInput!
          $automaticInput: DiscountAutomaticBasicInput!
        ) {
          codeSetup: discountCodeBasicCreate(basicCodeDiscount: $codeInput) {
            codeDiscountNode { id }
            userErrors { field message code extraInfo }
          }
          automaticSetup: discountAutomaticBasicCreate(automaticBasicDiscount: $automaticInput) {
            automaticDiscountNode { id }
            userErrors { field message code extraInfo }
          }
        }
        "#,
        json!({
            "codeInput": {
                "title": "Minimum requirement code setup",
                "code": "MINREQSETUP",
                "startsAt": "2026-04-27T19:31:14Z",
                "context": { "all": "ALL" },
                "customerGets": { "value": { "percentage": 0.1 }, "items": { "all": true } }
            },
            "automaticInput": {
                "title": "Minimum requirement automatic setup",
                "startsAt": "2026-04-27T19:31:14Z",
                "customerGets": { "value": { "percentage": 0.1 }, "items": { "all": true } }
            }
        }),
    ));
    assert_eq!(setup.status, 200);
    assert_eq!(setup.body["data"]["codeSetup"]["userErrors"], json!([]));
    assert_eq!(
        setup.body["data"]["automaticSetup"]["userErrors"],
        json!([])
    );
    let code_id = setup.body["data"]["codeSetup"]["codeDiscountNode"]["id"]
        .as_str()
        .unwrap();
    let automatic_id = setup.body["data"]["automaticSetup"]["automaticDiscountNode"]["id"]
        .as_str()
        .unwrap();

    let invalid = proxy.process_request(json_graphql_request(
        r#"
        mutation MinimumRequirementConflicts(
          $codeId: ID!
          $automaticId: ID!
          $codeCreateInput: DiscountCodeBasicInput!
          $codeUpdateInput: DiscountCodeBasicInput!
          $automaticCreateInput: DiscountAutomaticBasicInput!
          $automaticUpdateInput: DiscountAutomaticBasicInput!
        ) {
          codeCreate: discountCodeBasicCreate(basicCodeDiscount: $codeCreateInput) {
            codeDiscountNode { id }
            userErrors { field message code extraInfo }
          }
          codeUpdate: discountCodeBasicUpdate(id: $codeId, basicCodeDiscount: $codeUpdateInput) {
            codeDiscountNode { id }
            userErrors { field message code extraInfo }
          }
          automaticCreate: discountAutomaticBasicCreate(automaticBasicDiscount: $automaticCreateInput) {
            automaticDiscountNode { id }
            userErrors { field message code extraInfo }
          }
          automaticUpdate: discountAutomaticBasicUpdate(id: $automaticId, automaticBasicDiscount: $automaticUpdateInput) {
            automaticDiscountNode { id }
            userErrors { field message code extraInfo }
          }
        }
        "#,
        json!({
            "codeId": code_id,
            "automaticId": automatic_id,
            "codeCreateInput": discount_minimum_requirement_conflict_input(Some("MINREQCREATE")),
            "codeUpdateInput": discount_minimum_requirement_conflict_input(Some("MINREQUPDATE")),
            "automaticCreateInput": discount_minimum_requirement_conflict_input(None),
            "automaticUpdateInput": discount_minimum_requirement_conflict_input(None)
        }),
    ));
    assert_eq!(invalid.status, 200);

    let basic_errors = discount_minimum_requirement_conflict_errors("basicCodeDiscount");
    let automatic_errors = discount_minimum_requirement_conflict_errors("automaticBasicDiscount");
    assert_eq!(
        invalid.body["data"]["codeCreate"]["codeDiscountNode"],
        json!(null)
    );
    assert_eq!(
        invalid.body["data"]["codeCreate"]["userErrors"],
        basic_errors
    );
    assert_eq!(
        invalid.body["data"]["codeUpdate"]["codeDiscountNode"],
        json!(null)
    );
    assert_eq!(
        invalid.body["data"]["codeUpdate"]["userErrors"],
        basic_errors
    );
    assert_eq!(
        invalid.body["data"]["automaticCreate"]["automaticDiscountNode"],
        json!(null)
    );
    assert_eq!(
        invalid.body["data"]["automaticCreate"]["userErrors"],
        automatic_errors
    );
    assert_eq!(
        invalid.body["data"]["automaticUpdate"]["automaticDiscountNode"],
        json!(null)
    );
    assert_eq!(
        invalid.body["data"]["automaticUpdate"]["userErrors"],
        automatic_errors
    );
}

#[test]
fn discount_minimum_requirement_bounds_use_concrete_paths() {
    let mut proxy = snapshot_proxy();

    let response = proxy.process_request(json_graphql_request(
        r#"
        mutation MinimumRequirementBounds(
          $quantityLimit: DiscountCodeBasicInput!
          $subtotalLimit: DiscountCodeBasicInput!
          $automaticQuantityLimit: DiscountAutomaticBasicInput!
        ) {
          quantityLimit: discountCodeBasicCreate(basicCodeDiscount: $quantityLimit) {
            codeDiscountNode { id }
            userErrors { field message code extraInfo }
          }
          subtotalLimit: discountCodeBasicCreate(basicCodeDiscount: $subtotalLimit) {
            codeDiscountNode { id }
            userErrors { field message code extraInfo }
          }
          automaticQuantityLimit: discountAutomaticBasicCreate(automaticBasicDiscount: $automaticQuantityLimit) {
            automaticDiscountNode { id }
            userErrors { field message code extraInfo }
          }
        }
        "#,
        json!({
            "quantityLimit": {
                "title": "Minimum quantity limit",
                "code": "MINREQQTY",
                "startsAt": "2026-04-27T19:31:14Z",
                "context": { "all": "ALL" },
                "customerGets": { "value": { "percentage": 0.1 }, "items": { "all": true } },
                "minimumRequirement": {
                    "quantity": { "greaterThanOrEqualToQuantity": "9999999999" }
                }
            },
            "subtotalLimit": {
                "title": "Minimum subtotal limit",
                "code": "MINREQSUB",
                "startsAt": "2026-04-27T19:31:14Z",
                "context": { "all": "ALL" },
                "customerGets": { "value": { "percentage": 0.1 }, "items": { "all": true } },
                "minimumRequirement": {
                    "subtotal": {
                        "greaterThanOrEqualToSubtotal": "1000000000000000001.00"
                    }
                }
            },
            "automaticQuantityLimit": {
                "title": "Automatic minimum quantity limit",
                "startsAt": "2026-04-27T19:31:14Z",
                "customerGets": { "value": { "percentage": 0.1 }, "items": { "all": true } },
                "minimumRequirement": {
                    "quantity": { "greaterThanOrEqualToQuantity": "9999999999" }
                }
            }
        }),
    ));
    assert_eq!(response.status, 200);
    assert_eq!(
        response.body["data"]["quantityLimit"]["codeDiscountNode"],
        json!(null)
    );
    assert_eq!(
        response.body["data"]["quantityLimit"]["userErrors"],
        discount_minimum_requirement_bound_error(
            "basicCodeDiscount",
            "quantity",
            "greaterThanOrEqualToQuantity",
            "Minimum quantity must be less than 2147483647"
        )
    );
    assert_eq!(
        response.body["data"]["subtotalLimit"]["codeDiscountNode"],
        json!(null)
    );
    assert_eq!(
        response.body["data"]["subtotalLimit"]["userErrors"],
        discount_minimum_requirement_bound_error(
            "basicCodeDiscount",
            "subtotal",
            "greaterThanOrEqualToSubtotal",
            "Minimum subtotal must be less than 1000000000000000000"
        )
    );
    assert_eq!(
        response.body["data"]["automaticQuantityLimit"]["automaticDiscountNode"],
        json!(null)
    );
    assert_eq!(
        response.body["data"]["automaticQuantityLimit"]["userErrors"],
        discount_minimum_requirement_bound_error(
            "automaticBasicDiscount",
            "quantity",
            "greaterThanOrEqualToQuantity",
            "Minimum quantity must be less than 2147483647"
        )
    );
}

#[test]
fn discount_basic_customer_gets_value_bounds_match_captured_shopify_behavior() {
    let mut proxy = snapshot_proxy();

    let create = r#"
        mutation DiscountValueBounds(
          $percentageHigh: DiscountCodeBasicInput!
          $percentageNegative: DiscountCodeBasicInput!
          $percentageZero: DiscountCodeBasicInput!
          $amountNegative: DiscountCodeBasicInput!
          $amountZero: DiscountCodeBasicInput!
          $amountHigh: DiscountCodeBasicInput!
        ) {
          percentageHigh: discountCodeBasicCreate(basicCodeDiscount: $percentageHigh) { codeDiscountNode { id } userErrors { field message code extraInfo } }
          percentageNegative: discountCodeBasicCreate(basicCodeDiscount: $percentageNegative) { codeDiscountNode { id } userErrors { field message code extraInfo } }
          percentageZero: discountCodeBasicCreate(basicCodeDiscount: $percentageZero) { codeDiscountNode { id } userErrors { field message code extraInfo } }
          amountNegative: discountCodeBasicCreate(basicCodeDiscount: $amountNegative) { codeDiscountNode { id } userErrors { field message code extraInfo } }
          amountZero: discountCodeBasicCreate(basicCodeDiscount: $amountZero) { codeDiscountNode { id } userErrors { field message code extraInfo } }
          amountHigh: discountCodeBasicCreate(basicCodeDiscount: $amountHigh) { codeDiscountNode { id } userErrors { field message code extraInfo } }
        }
    "#;
    let base = json!({
        "startsAt": "2026-04-25T00:00:00Z",
        "combinesWith": { "productDiscounts": false, "orderDiscounts": true, "shippingDiscounts": false },
        "context": { "all": "ALL" },
        "customerGets": { "items": { "all": true } }
    });
    let input = |title: &str, code: &str, value: Value| {
        let mut input = base.clone();
        input["title"] = json!(title);
        input["code"] = json!(code);
        input["customerGets"]["value"] = value;
        input
    };

    let response = proxy.process_request(json_graphql_request(
        create,
        json!({
            "percentageHigh": input("Percentage high", "PCTHIGH1440", json!({ "percentage": 1.5 })),
            "percentageNegative": input("Percentage negative", "PCTNEG1440", json!({ "percentage": -0.1 })),
            "percentageZero": input("Percentage zero", "PCTZERO1440", json!({ "percentage": 0 })),
            "amountNegative": input("Amount negative", "AMTNEG1440", json!({ "discountAmount": { "amount": "-5", "appliesOnEachItem": false } })),
            "amountZero": input("Amount zero", "AMTZERO1440", json!({ "discountAmount": { "amount": "0", "appliesOnEachItem": false } })),
            "amountHigh": input("Amount high", "AMTHIGH1440", json!({ "discountAmount": { "amount": "1000000000000000000", "appliesOnEachItem": false } }))
        }),
    ));

    assert_eq!(
        response.body["data"]["percentageHigh"]["userErrors"],
        json!([{
            "field": ["basicCodeDiscount", "customerGets", "value", "percentage"],
            "message": "Value must be between 0.0 and 1.0",
            "code": "VALUE_OUTSIDE_RANGE",
            "extraInfo": null
        }])
    );
    assert_eq!(
        response.body["data"]["percentageNegative"]["userErrors"],
        response.body["data"]["percentageHigh"]["userErrors"]
    );
    assert!(
        response.body["data"]["percentageZero"]["codeDiscountNode"]["id"]
            .as_str()
            .unwrap()
            .contains("shopify-draft-proxy=synthetic")
    );
    assert_eq!(
        response.body["data"]["percentageZero"]["userErrors"],
        json!([])
    );
    assert_eq!(
        response.body["data"]["amountNegative"]["userErrors"],
        json!([{
            "field": ["basicCodeDiscount", "customerGets", "value", "discountAmount", "amount"],
            "message": "Value must be less than or equal to 0",
            "code": "LESS_THAN_OR_EQUAL_TO",
            "extraInfo": null
        }])
    );
    assert!(
        response.body["data"]["amountZero"]["codeDiscountNode"]["id"]
            .as_str()
            .unwrap()
            .contains("shopify-draft-proxy=synthetic")
    );
    assert_eq!(response.body["data"]["amountZero"]["userErrors"], json!([]));
    assert_eq!(
        response.body["data"]["amountHigh"]["userErrors"],
        json!([{
            "field": ["basicCodeDiscount", "customerGets", "value", "discountAmount", "amount"],
            "message": "Value must be greater than -1000000000000000000",
            "code": "LESS_THAN",
            "extraInfo": null
        }])
    );
}

#[test]
fn discount_automatic_basic_customer_gets_value_bounds_match_captured_shopify_behavior() {
    let mut proxy = snapshot_proxy();

    let setup = proxy.process_request(json_graphql_request(
        r#"
        mutation Setup($input: DiscountAutomaticBasicInput!) {
          discountAutomaticBasicCreate(automaticBasicDiscount: $input) {
            automaticDiscountNode { id }
            userErrors { field message code extraInfo }
          }
        }
        "#,
        json!({ "input": {
            "title": "Automatic shared bounds setup",
            "startsAt": "2026-04-25T00:00:00Z",
            "context": { "all": "ALL" },
            "customerGets": { "value": { "percentage": 0.1 }, "items": { "all": true } }
        }}),
    ));
    assert_eq!(
        setup.body["data"]["discountAutomaticBasicCreate"]["userErrors"],
        json!([])
    );
    let id = setup.body["data"]["discountAutomaticBasicCreate"]["automaticDiscountNode"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let base = json!({
        "startsAt": "2026-04-25T00:00:00Z",
        "combinesWith": { "productDiscounts": false, "orderDiscounts": true, "shippingDiscounts": false },
        "context": { "all": "ALL" },
        "customerGets": { "items": { "all": true } }
    });
    let input = |title: &str, value: Value| {
        let mut input = base.clone();
        input["title"] = json!(title);
        input["customerGets"]["value"] = value;
        input
    };

    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation Create(
          $percentageHigh: DiscountAutomaticBasicInput!
          $percentageNegative: DiscountAutomaticBasicInput!
          $percentageZero: DiscountAutomaticBasicInput!
          $amountNegative: DiscountAutomaticBasicInput!
          $amountZero: DiscountAutomaticBasicInput!
        ) {
          percentageHigh: discountAutomaticBasicCreate(automaticBasicDiscount: $percentageHigh) {
            automaticDiscountNode { id }
            userErrors { field message code extraInfo }
          }
          percentageNegative: discountAutomaticBasicCreate(automaticBasicDiscount: $percentageNegative) {
            automaticDiscountNode { id }
            userErrors { field message code extraInfo }
          }
          percentageZero: discountAutomaticBasicCreate(automaticBasicDiscount: $percentageZero) {
            automaticDiscountNode { id }
            userErrors { field message code extraInfo }
          }
          amountNegative: discountAutomaticBasicCreate(automaticBasicDiscount: $amountNegative) {
            automaticDiscountNode { id }
            userErrors { field message code extraInfo }
          }
          amountZero: discountAutomaticBasicCreate(automaticBasicDiscount: $amountZero) {
            automaticDiscountNode { id }
            userErrors { field message code extraInfo }
          }
        }
        "#,
        json!({
            "percentageHigh": input("Automatic bounds create percentage high", json!({ "percentage": 1.5 })),
            "percentageNegative": input("Automatic bounds create percentage negative", json!({ "percentage": -0.1 })),
            "percentageZero": input("Automatic bounds create percentage zero", json!({ "percentage": 0 })),
            "amountNegative": input("Automatic bounds create amount negative", json!({ "discountAmount": { "amount": "-1", "appliesOnEachItem": false } })),
            "amountZero": input("Automatic bounds create amount zero", json!({ "discountAmount": { "amount": "0", "appliesOnEachItem": false } }))
        }),
    ));

    let update = proxy.process_request(json_graphql_request(
        r#"
        mutation Update(
          $id: ID!
          $percentageHigh: DiscountAutomaticBasicInput!
          $percentageNegative: DiscountAutomaticBasicInput!
          $percentageZero: DiscountAutomaticBasicInput!
          $amountNegative: DiscountAutomaticBasicInput!
          $amountZero: DiscountAutomaticBasicInput!
        ) {
          percentageHigh: discountAutomaticBasicUpdate(id: $id, automaticBasicDiscount: $percentageHigh) {
            automaticDiscountNode { id }
            userErrors { field message code extraInfo }
          }
          percentageNegative: discountAutomaticBasicUpdate(id: $id, automaticBasicDiscount: $percentageNegative) {
            automaticDiscountNode { id }
            userErrors { field message code extraInfo }
          }
          percentageZero: discountAutomaticBasicUpdate(id: $id, automaticBasicDiscount: $percentageZero) {
            automaticDiscountNode { id }
            userErrors { field message code extraInfo }
          }
          amountNegative: discountAutomaticBasicUpdate(id: $id, automaticBasicDiscount: $amountNegative) {
            automaticDiscountNode { id }
            userErrors { field message code extraInfo }
          }
          amountZero: discountAutomaticBasicUpdate(id: $id, automaticBasicDiscount: $amountZero) {
            automaticDiscountNode { id }
            userErrors { field message code extraInfo }
          }
        }
        "#,
        json!({
            "id": id,
            "percentageHigh": input("Automatic bounds update percentage high", json!({ "percentage": 1.5 })),
            "percentageNegative": input("Automatic bounds update percentage negative", json!({ "percentage": -0.1 })),
            "percentageZero": input("Automatic bounds update percentage zero", json!({ "percentage": 0 })),
            "amountNegative": input("Automatic bounds update amount negative", json!({ "discountAmount": { "amount": "-1", "appliesOnEachItem": false } })),
            "amountZero": input("Automatic bounds update amount zero", json!({ "discountAmount": { "amount": "0", "appliesOnEachItem": false } }))
        }),
    ));

    let percentage_error = json!([{
        "field": ["automaticBasicDiscount", "customerGets", "value", "percentage"],
        "message": "Value must be between 0.0 and 1.0",
        "code": "VALUE_OUTSIDE_RANGE",
        "extraInfo": null
    }]);
    let amount_error = json!([{
        "field": ["automaticBasicDiscount", "customerGets", "value", "discountAmount", "amount"],
        "message": "Value must be less than 0",
        "code": "GREATER_THAN",
        "extraInfo": null
    }]);

    for root in ["percentageHigh", "percentageNegative", "percentageZero"] {
        assert_eq!(
            create.body["data"][root]["automaticDiscountNode"],
            json!(null)
        );
        assert_eq!(create.body["data"][root]["userErrors"], percentage_error);
        assert_eq!(
            update.body["data"][root]["automaticDiscountNode"],
            json!(null)
        );
        assert_eq!(update.body["data"][root]["userErrors"], percentage_error);
    }

    for root in ["amountNegative", "amountZero"] {
        assert_eq!(
            create.body["data"][root]["automaticDiscountNode"],
            json!(null)
        );
        assert_eq!(create.body["data"][root]["userErrors"], amount_error);
        assert_eq!(
            update.body["data"][root]["automaticDiscountNode"],
            json!(null)
        );
        assert_eq!(update.body["data"][root]["userErrors"], amount_error);
    }
}

#[test]
fn discount_basic_non_numeric_decimal_variable_fails_before_resolver_execution() {
    let mut proxy = snapshot_proxy();

    let response = proxy.process_request(json_graphql_request(
        r#"
        mutation DiscountValueBoundsNonNumeric($input: DiscountCodeBasicInput!) {
          discountCodeBasicCreate(basicCodeDiscount: $input) {
            codeDiscountNode { id }
            userErrors { field message code extraInfo }
          }
        }
        "#,
        json!({ "input": {
            "title": "Value Bounds NonNumeric",
            "code": "VALUEBOUNDSNAN1440",
            "startsAt": "2026-04-25T00:00:00Z",
            "combinesWith": { "productDiscounts": false, "orderDiscounts": true, "shippingDiscounts": false },
            "context": { "all": "ALL" },
            "customerGets": {
                "value": { "discountAmount": { "amount": "abc", "appliesOnEachItem": false } },
                "items": { "all": true }
            }
        }}),
    ));

    assert_eq!(response.body["data"], Value::Null);
    assert_eq!(
        response.body["errors"][0]["message"],
        json!(
            "Variable $input of type DiscountCodeBasicInput! was provided invalid value for customerGets.value.discountAmount.amount (invalid decimal 'abc')"
        )
    );
    assert_eq!(
        response.body["errors"][0]["extensions"]["code"],
        json!("INVALID_VARIABLE")
    );
    assert_eq!(
        response.body["errors"][0]["extensions"]["problems"],
        json!([{
            "path": ["customerGets", "value", "discountAmount", "amount"],
            "explanation": "invalid decimal 'abc'",
            "message": "invalid decimal 'abc'"
        }])
    );
    assert_eq!(log_snapshot(&proxy)["entries"], json!([]));
}

#[test]
fn discount_lifecycle_unknown_ids_use_type_specific_not_found_messages() {
    let mut proxy = snapshot_proxy();

    let response = proxy.process_request(json_graphql_request(
        r#"
        mutation DiscountLifecycleUnknowns {
          codeActivate: discountCodeActivate(id: "gid://shopify/DiscountCodeNode/0") {
            codeDiscountNode { id }
            userErrors { field message code extraInfo }
          }
          codeDeactivate: discountCodeDeactivate(id: "gid://shopify/DiscountCodeNode/0") {
            codeDiscountNode { id }
            userErrors { field message code extraInfo }
          }
          codeDelete: discountCodeDelete(id: "gid://shopify/DiscountCodeNode/0") {
            deletedCodeDiscountId
            userErrors { field message code extraInfo }
          }
          automaticActivate: discountAutomaticActivate(id: "gid://shopify/DiscountAutomaticNode/0") {
            automaticDiscountNode { id }
            userErrors { field message code extraInfo }
          }
          automaticDeactivate: discountAutomaticDeactivate(id: "gid://shopify/DiscountAutomaticNode/0") {
            automaticDiscountNode { id }
            userErrors { field message code extraInfo }
          }
          automaticDelete: discountAutomaticDelete(id: "gid://shopify/DiscountAutomaticNode/0") {
            deletedAutomaticDiscountId
            userErrors { field message code extraInfo }
          }
        }
        "#,
        json!({}),
    ));

    assert_eq!(response.status, 200);
    assert_eq!(
        response.body["data"]["codeActivate"]["codeDiscountNode"],
        json!(null)
    );
    assert_eq!(
        response.body["data"]["codeDeactivate"]["codeDiscountNode"],
        json!(null)
    );
    assert_eq!(
        response.body["data"]["codeDelete"]["deletedCodeDiscountId"],
        json!(null)
    );
    assert_eq!(
        response.body["data"]["automaticActivate"]["automaticDiscountNode"],
        json!(null)
    );
    assert_eq!(
        response.body["data"]["automaticDeactivate"]["automaticDiscountNode"],
        json!(null)
    );
    assert_eq!(
        response.body["data"]["automaticDelete"]["deletedAutomaticDiscountId"],
        json!(null)
    );

    for response_key in ["codeActivate", "codeDeactivate", "codeDelete"] {
        assert_eq!(
            response.body["data"][response_key]["userErrors"],
            json!([{ "field": ["id"], "message": "Code discount does not exist.", "code": "INVALID", "extraInfo": null }])
        );
    }
    for response_key in [
        "automaticActivate",
        "automaticDeactivate",
        "automaticDelete",
    ] {
        assert_eq!(
            response.body["data"][response_key]["userErrors"],
            json!([{ "field": ["id"], "message": "Automatic discount does not exist.", "code": "INVALID", "extraInfo": null }])
        );
    }
}

#[test]
fn discount_lifecycle_cross_kind_ids_are_not_found_and_do_not_mutate_records() {
    let mut proxy = snapshot_proxy();

    let setup = proxy.process_request(json_graphql_request(
        r#"
        mutation DiscountLifecycleCrossKindSetup($codeInput: DiscountCodeBasicInput!, $automaticInput: DiscountAutomaticBasicInput!) {
          codeSetup: discountCodeBasicCreate(basicCodeDiscount: $codeInput) {
            codeDiscountNode { id codeDiscount { ... on DiscountCodeBasic { status codesCount { count precision } } } }
            userErrors { field message code extraInfo }
          }
          automaticSetup: discountAutomaticBasicCreate(automaticBasicDiscount: $automaticInput) {
            automaticDiscountNode { id automaticDiscount { ... on DiscountAutomaticBasic { status } } }
            userErrors { field message code extraInfo }
          }
        }
        "#,
        json!({
            "codeInput": {
                "title": "Cross-kind code lifecycle",
                "code": "CROSS-KIND-CODE",
                "startsAt": "2026-04-01T00:00:00Z",
                "context": { "all": "ALL" },
                "customerGets": { "value": { "percentage": 0.1 }, "items": { "all": true } }
            },
            "automaticInput": {
                "title": "Cross-kind automatic lifecycle",
                "startsAt": "2026-04-01T00:00:00Z",
                "customerGets": { "value": { "percentage": 0.1 }, "items": { "all": true } }
            }
        }),
    ));
    assert_eq!(setup.status, 200);
    assert_eq!(setup.body["data"]["codeSetup"]["userErrors"], json!([]));
    assert_eq!(
        setup.body["data"]["automaticSetup"]["userErrors"],
        json!([])
    );
    let code_id = json_string(
        &setup.body["data"]["codeSetup"]["codeDiscountNode"]["id"],
        "cross-kind code id",
    );
    let automatic_id = json_string(
        &setup.body["data"]["automaticSetup"]["automaticDiscountNode"]["id"],
        "cross-kind automatic id",
    );

    let transitions = proxy.process_request(json_graphql_request(
        r#"
        mutation DiscountLifecycleCrossKindTransitions($codeId: ID!, $automaticId: ID!) {
          codeActivateAutomatic: discountCodeActivate(id: $automaticId) {
            codeDiscountNode { id }
            userErrors { field message code extraInfo }
          }
          codeDeactivateAutomatic: discountCodeDeactivate(id: $automaticId) {
            codeDiscountNode { id }
            userErrors { field message code extraInfo }
          }
          automaticActivateCode: discountAutomaticActivate(id: $codeId) {
            automaticDiscountNode { id }
            userErrors { field message code extraInfo }
          }
          automaticDeactivateCode: discountAutomaticDeactivate(id: $codeId) {
            automaticDiscountNode { id }
            userErrors { field message code extraInfo }
          }
          bulkAddAutomatic: discountRedeemCodeBulkAdd(discountId: $automaticId, codes: [{ code: "CROSSKIND1" }]) {
            bulkCreation { done codesCount importedCount failedCount }
            userErrors { field message code extraInfo }
          }
        }
        "#,
        json!({ "codeId": code_id, "automaticId": automatic_id }),
    ));
    assert_eq!(transitions.status, 200);
    for response_key in ["codeActivateAutomatic", "codeDeactivateAutomatic"] {
        assert_eq!(
            transitions.body["data"][response_key]["codeDiscountNode"],
            json!(null)
        );
        assert_eq!(
            transitions.body["data"][response_key]["userErrors"],
            json!([{ "field": ["id"], "message": "Code discount does not exist.", "code": "INVALID", "extraInfo": null }])
        );
    }
    for response_key in ["automaticActivateCode", "automaticDeactivateCode"] {
        assert_eq!(
            transitions.body["data"][response_key]["automaticDiscountNode"],
            json!(null)
        );
        assert_eq!(
            transitions.body["data"][response_key]["userErrors"],
            json!([{ "field": ["id"], "message": "Automatic discount does not exist.", "code": "INVALID", "extraInfo": null }])
        );
    }
    assert_eq!(
        transitions.body["data"]["bulkAddAutomatic"]["bulkCreation"],
        json!(null)
    );
    assert_eq!(
        transitions.body["data"]["bulkAddAutomatic"]["userErrors"],
        json!([{ "field": ["discountId"], "message": "Code discount does not exist.", "code": "INVALID", "extraInfo": null }])
    );

    let deletes = proxy.process_request(json_graphql_request(
        r#"
        mutation DiscountLifecycleCrossKindDeletes($codeId: ID!, $automaticId: ID!) {
          codeDeleteAutomatic: discountCodeDelete(id: $automaticId) {
            deletedCodeDiscountId
            userErrors { field message code extraInfo }
          }
          automaticDeleteCode: discountAutomaticDelete(id: $codeId) {
            deletedAutomaticDiscountId
            userErrors { field message code extraInfo }
          }
        }
        "#,
        json!({ "codeId": code_id, "automaticId": automatic_id }),
    ));
    assert_eq!(deletes.status, 200);
    assert_eq!(
        deletes.body["data"]["codeDeleteAutomatic"]["deletedCodeDiscountId"],
        json!(null)
    );
    assert_eq!(
        deletes.body["data"]["codeDeleteAutomatic"]["userErrors"],
        json!([{ "field": ["id"], "message": "Code discount does not exist.", "code": "INVALID", "extraInfo": null }])
    );
    assert_eq!(
        deletes.body["data"]["automaticDeleteCode"]["deletedAutomaticDiscountId"],
        json!(null)
    );
    assert_eq!(
        deletes.body["data"]["automaticDeleteCode"]["userErrors"],
        json!([{ "field": ["id"], "message": "Automatic discount does not exist.", "code": "INVALID", "extraInfo": null }])
    );

    let read = proxy.process_request(json_graphql_request(
        r#"
        query DiscountLifecycleCrossKindRead($codeId: ID!, $automaticId: ID!, $rejectedCode: String!) {
          codeDiscountNode(id: $codeId) {
            id
            codeDiscount { ... on DiscountCodeBasic { status codesCount { count precision } } }
          }
          automaticDiscountNode(id: $automaticId) {
            id
            automaticDiscount { ... on DiscountAutomaticBasic { status } }
          }
          rejectedCodeLookup: codeDiscountNodeByCode(code: $rejectedCode) { id }
        }
        "#,
        json!({ "codeId": code_id, "automaticId": automatic_id, "rejectedCode": "CROSSKIND1" }),
    ));
    assert_eq!(read.status, 200);
    assert_eq!(
        read.body["data"]["codeDiscountNode"]["codeDiscount"]["status"],
        json!("ACTIVE")
    );
    assert_eq!(
        read.body["data"]["codeDiscountNode"]["codeDiscount"]["codesCount"],
        json!({ "count": 1, "precision": "EXACT" })
    );
    assert_eq!(
        read.body["data"]["automaticDiscountNode"]["automaticDiscount"]["status"],
        json!("ACTIVE")
    );
    assert_eq!(read.body["data"]["rejectedCodeLookup"], json!(null));
}

#[test]
fn discount_redeem_code_bulk_add_rejects_code_app_discount_ids() {
    let mut proxy = configured_proxy(ReadMode::LiveHybrid, None)
        .with_upstream_transport(|request| discount_app_function_upstream_response(request, true));

    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation DiscountRedeemCodeBulkAddCodeAppSetup($input: DiscountCodeAppInput!) {
          discountCodeAppCreate(codeAppDiscount: $input) {
            codeAppDiscount {
              discountId
              codes(first: 5) { nodes { code } }
            }
            userErrors { field message code extraInfo }
          }
        }
        "#,
        json!({
            "input": {
                "title": "Code app bulk add target",
                "code": "APP-BULK-BASE",
                "startsAt": "2026-05-05T00:00:00Z",
                "functionHandle": "discount-function",
                "discountClasses": ["ORDER"]
            }
        }),
    ));
    assert_eq!(create.status, 200);
    assert_eq!(
        create.body["data"]["discountCodeAppCreate"]["userErrors"],
        json!([])
    );
    let code_app_id = json_string(
        &create.body["data"]["discountCodeAppCreate"]["codeAppDiscount"]["discountId"],
        "code app discount id",
    );

    let bulk_add = proxy.process_request(json_graphql_request(
        r#"
        mutation DiscountRedeemCodeBulkAddCodeApp($discountId: ID!) {
          discountRedeemCodeBulkAdd(discountId: $discountId, codes: [{ code: "APP-BULK-ADDED" }]) {
            bulkCreation { done codesCount importedCount failedCount }
            userErrors { field message code extraInfo }
          }
        }
        "#,
        json!({ "discountId": code_app_id }),
    ));
    assert_eq!(
        bulk_add.body["data"]["discountRedeemCodeBulkAdd"]["bulkCreation"],
        json!(null)
    );
    assert_eq!(
        bulk_add.body["data"]["discountRedeemCodeBulkAdd"]["userErrors"],
        json!([{ "field": ["discountId"], "message": "Code discount does not exist.", "code": "INVALID", "extraInfo": null }])
    );

    let read = proxy.process_request(json_graphql_request(
        r#"
        query DiscountRedeemCodeBulkAddCodeAppRead($discountId: ID!, $rejectedCode: String!) {
          codeDiscountNode(id: $discountId) {
            id
            codeDiscount { ... on DiscountCodeApp { codes(first: 5) { nodes { code } } } }
          }
          rejectedCodeLookup: codeDiscountNodeByCode(code: $rejectedCode) { id }
        }
        "#,
        json!({ "discountId": code_app_id, "rejectedCode": "APP-BULK-ADDED" }),
    ));
    assert_eq!(
        read.body["data"]["codeDiscountNode"]["codeDiscount"]["codes"]["nodes"],
        json!([{ "code": "APP-BULK-BASE" }])
    );
    assert_eq!(read.body["data"]["rejectedCodeLookup"], json!(null));
}

#[test]
fn discount_activate_deactivate_noops_preserve_captured_timestamp_shapes() {
    let mut proxy = snapshot_proxy();

    let setup = proxy.process_request(json_graphql_request(
        r#"
        mutation DiscountNoopTransitionSetup {
          codeActive: discountCodeBasicCreate(basicCodeDiscount: { title: "Noop active code", code: "NOOP-ACTIVE-CODE", startsAt: "2026-04-01T00:00:00Z", context: { all: "ALL" }, customerGets: { value: { percentage: 0.1 }, items: { all: true } } }) {
            codeDiscountNode { id codeDiscount { __typename ... on DiscountCodeBasic { startsAt endsAt status updatedAt } } }
            userErrors { field message code extraInfo }
          }
          codeExpired: discountCodeBasicCreate(basicCodeDiscount: { title: "Noop expired code", code: "NOOP-EXPIRED-CODE", startsAt: "2020-01-01T00:00:00Z", endsAt: "2020-01-02T00:00:00Z", context: { all: "ALL" }, customerGets: { value: { percentage: 0.1 }, items: { all: true } } }) {
            codeDiscountNode { id codeDiscount { __typename ... on DiscountCodeBasic { startsAt endsAt status updatedAt } } }
            userErrors { field message code extraInfo }
          }
          automaticActive: discountAutomaticBasicCreate(automaticBasicDiscount: { title: "Noop active automatic", startsAt: "2026-04-01T00:00:00Z" }) {
            automaticDiscountNode { id automaticDiscount { __typename ... on DiscountAutomaticBasic { startsAt endsAt status updatedAt } } }
            userErrors { field message code extraInfo }
          }
          automaticExpired: discountAutomaticBasicCreate(automaticBasicDiscount: { title: "Noop expired automatic", startsAt: "2020-01-01T00:00:00Z", endsAt: "2020-01-02T00:00:00Z" }) {
            automaticDiscountNode { id automaticDiscount { __typename ... on DiscountAutomaticBasic { startsAt endsAt status updatedAt } } }
            userErrors { field message code extraInfo }
          }
        }
        "#,
        json!({}),
    ));
    for response_key in [
        "codeActive",
        "codeExpired",
        "automaticActive",
        "automaticExpired",
    ] {
        assert_eq!(setup.body["data"][response_key]["userErrors"], json!([]));
    }
    let code_active_id = json_string(
        &setup.body["data"]["codeActive"]["codeDiscountNode"]["id"],
        "active code discount id",
    );
    let code_expired_id = json_string(
        &setup.body["data"]["codeExpired"]["codeDiscountNode"]["id"],
        "expired code discount id",
    );
    let automatic_active_id = json_string(
        &setup.body["data"]["automaticActive"]["automaticDiscountNode"]["id"],
        "active automatic discount id",
    );
    let automatic_expired_id = json_string(
        &setup.body["data"]["automaticExpired"]["automaticDiscountNode"]["id"],
        "expired automatic discount id",
    );
    for id in [&code_active_id, &code_expired_id] {
        assert_synthetic_gid(id, "DiscountCodeNode");
    }
    for id in [&automatic_active_id, &automatic_expired_id] {
        assert_synthetic_gid(id, "DiscountAutomaticNode");
    }

    let code_activate = proxy.process_request(json_graphql_request(
        r#"
        mutation DiscountCodeActivateNoopIdempotence($id: ID!) {
          discountCodeActivate(id: $id) {
            codeDiscountNode {
              id
              codeDiscount { __typename ... on DiscountCodeBasic { startsAt endsAt status updatedAt } }
            }
            userErrors { field message code extraInfo }
          }
        }
        "#,
        json!({ "id": code_active_id }),
    ));
    assert_eq!(
        code_activate.body["data"]["discountCodeActivate"]["codeDiscountNode"]["codeDiscount"],
        setup.body["data"]["codeActive"]["codeDiscountNode"]["codeDiscount"]
    );
    assert_eq!(
        code_activate.body["data"]["discountCodeActivate"]["userErrors"],
        json!([])
    );

    let code_deactivate = proxy.process_request(json_graphql_request(
        r#"
        mutation DiscountCodeDeactivateNoopIdempotence($id: ID!) {
          discountCodeDeactivate(id: $id) {
            codeDiscountNode {
              id
              codeDiscount { __typename ... on DiscountCodeBasic { startsAt endsAt status updatedAt } }
            }
            userErrors { field message code extraInfo }
          }
        }
        "#,
        json!({ "id": code_expired_id }),
    ));
    assert_eq!(
        code_deactivate.body["data"]["discountCodeDeactivate"]["codeDiscountNode"]["codeDiscount"],
        setup.body["data"]["codeExpired"]["codeDiscountNode"]["codeDiscount"]
    );
    assert_eq!(
        code_deactivate.body["data"]["discountCodeDeactivate"]["userErrors"],
        json!([])
    );

    let automatic_activate = proxy.process_request(json_graphql_request(
        r#"
        mutation DiscountAutomaticActivateNoopIdempotence($id: ID!) {
          discountAutomaticActivate(id: $id) {
            automaticDiscountNode {
              id
              automaticDiscount { __typename ... on DiscountAutomaticBasic { startsAt endsAt status updatedAt } }
            }
            userErrors { field message code extraInfo }
          }
        }
        "#,
        json!({ "id": automatic_active_id }),
    ));
    assert_eq!(
        automatic_activate.body["data"]["discountAutomaticActivate"]["automaticDiscountNode"]
            ["automaticDiscount"],
        setup.body["data"]["automaticActive"]["automaticDiscountNode"]["automaticDiscount"]
    );
    assert_eq!(
        automatic_activate.body["data"]["discountAutomaticActivate"]["userErrors"],
        json!([])
    );

    let automatic_deactivate = proxy.process_request(json_graphql_request(
        r#"
        mutation DiscountAutomaticDeactivateNoopIdempotence($id: ID!) {
          discountAutomaticDeactivate(id: $id) {
            automaticDiscountNode {
              id
              automaticDiscount { __typename ... on DiscountAutomaticBasic { startsAt endsAt status updatedAt } }
            }
            userErrors { field message code extraInfo }
          }
        }
        "#,
        json!({ "id": automatic_expired_id }),
    ));
    assert_eq!(
        automatic_deactivate.body["data"]["discountAutomaticDeactivate"]["automaticDiscountNode"]
            ["automaticDiscount"],
        setup.body["data"]["automaticExpired"]["automaticDiscountNode"]["automaticDiscount"]
    );
    assert_eq!(
        automatic_deactivate.body["data"]["discountAutomaticDeactivate"]["userErrors"],
        json!([])
    );
}

#[test]
fn discount_automatic_basic_buyer_context_lifecycle_stages_selected_context_reads() {
    let mut proxy = snapshot_proxy();

    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation DiscountAutomaticBasicBuyerContextCreate($input: DiscountAutomaticBasicInput!) {
          discountAutomaticBasicCreate(automaticBasicDiscount: $input) {
            automaticDiscountNode { id automaticDiscount { __typename ... on DiscountAutomaticBasic { title status context { __typename ... on DiscountCustomers { customers { __typename id } } ... on DiscountCustomerSegments { segments { __typename id name } } } } } }
            userErrors { field message code extraInfo }
          }
        }
        "#,
        json!({ "input": { "title": "HAR-390 automatic customer context 1777346878525", "startsAt": "2026-04-25T00:00:00Z", "context": { "customers": { "add": ["gid://shopify/Customer/10548596015410"] } } } }),
    ));
    let discount_id = json_string(
        &create.body["data"]["discountAutomaticBasicCreate"]["automaticDiscountNode"]["id"],
        "automatic discount id",
    );
    assert_synthetic_gid(&discount_id, "DiscountAutomaticNode");
    assert_eq!(
        create.body["data"]["discountAutomaticBasicCreate"]["automaticDiscountNode"]
            ["automaticDiscount"],
        json!({
            "__typename": "DiscountAutomaticBasic",
            "title": "HAR-390 automatic customer context 1777346878525",
            "status": "ACTIVE",
            "context": {
                "__typename": "DiscountCustomers",
                "customers": [{
                    "__typename": "Customer",
                    "id": "gid://shopify/Customer/10548596015410"
                }]
            }
        })
    );

    let update = proxy.process_request(json_graphql_request(
        r#"
        mutation DiscountAutomaticBasicBuyerContextUpdate($id: ID!, $input: DiscountAutomaticBasicInput!) {
          discountAutomaticBasicUpdate(id: $id, automaticBasicDiscount: $input) {
            automaticDiscountNode { id automaticDiscount { __typename ... on DiscountAutomaticBasic { title status context { __typename ... on DiscountCustomerSegments { segments { __typename id } } } } } }
            userErrors { field message code extraInfo }
          }
        }
        "#,
        json!({ "id": discount_id.clone(), "input": { "title": "HAR-390 automatic segment context 1777346878525", "context": { "customerSegments": { "add": ["gid://shopify/Segment/647746715954"] } } } }),
    ));
    assert_eq!(
        update.body["data"]["discountAutomaticBasicUpdate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        update.body["data"]["discountAutomaticBasicUpdate"]["automaticDiscountNode"]
            ["automaticDiscount"]["context"],
        json!({
            "__typename": "DiscountCustomerSegments",
            "segments": [{
                "__typename": "Segment",
                "id": "gid://shopify/Segment/647746715954"
            }]
        })
    );

    let read = proxy.process_request(json_graphql_request(
        r#"
        query DiscountAutomaticBasicBuyerContextRead($id: ID!) {
          automaticDiscountNode(id: $id) {
            id
            automaticDiscount { __typename ... on DiscountAutomaticBasic { title context { __typename ... on DiscountCustomerSegments { segments { __typename id } } } } }
          }
        }
        "#,
        json!({ "id": discount_id.clone() }),
    ));
    assert_eq!(
        read.body["data"]["automaticDiscountNode"]["automaticDiscount"],
        json!({
            "__typename": "DiscountAutomaticBasic",
            "title": "HAR-390 automatic segment context 1777346878525",
            "context": {
                "__typename": "DiscountCustomerSegments",
                "segments": [{
                    "__typename": "Segment",
                    "id": "gid://shopify/Segment/647746715954"
                }]
            }
        })
    );

    let delete = proxy.process_request(json_graphql_request(
        r#"
        mutation DiscountAutomaticBasicBuyerContextDelete($id: ID!) {
          discountAutomaticDelete(id: $id) { deletedAutomaticDiscountId userErrors { field message code extraInfo } }
        }
        "#,
        json!({ "id": discount_id.clone() }),
    ));
    assert_eq!(
        delete.body["data"]["discountAutomaticDelete"],
        json!({ "deletedAutomaticDiscountId": discount_id, "userErrors": [] })
    );
}

#[test]
fn discount_automatic_nodes_read_returns_empty_connection_without_staged_discounts() {
    let mut proxy = snapshot_proxy();

    let response = proxy.process_request(json_graphql_request(
        r#"
        query DiscountAutomaticNodesRead($first: Int!, $query: String) {
          automaticDiscountNodes(first: $first, query: $query) {
            nodes {
              id
              automaticDiscount {
                __typename
                ... on DiscountAutomaticBasic { title status summary startsAt endsAt createdAt updatedAt asyncUsageCount discountClasses combinesWith { productDiscounts orderDiscounts shippingDiscounts } }
                ... on DiscountAutomaticBxgy { title status summary startsAt endsAt createdAt updatedAt asyncUsageCount discountClasses combinesWith { productDiscounts orderDiscounts shippingDiscounts } }
              }
            }
            edges { cursor node { id automaticDiscount { __typename ... on DiscountAutomaticBasic { title status } ... on DiscountAutomaticBxgy { title status } } } }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
        }
        "#,
        json!({ "first": 5, "query": null }),
    ));

    assert_eq!(response.status, 200);
    assert_eq!(
        response.body["data"]["automaticDiscountNodes"]["nodes"],
        json!([])
    );
    assert_eq!(
        response.body["data"]["automaticDiscountNodes"]["edges"],
        json!([])
    );
    assert_eq!(
        response.body["data"]["automaticDiscountNodes"]["pageInfo"],
        json!({
            "hasNextPage": false,
            "hasPreviousPage": false,
            "startCursor": null,
            "endCursor": null
        })
    );
}

#[test]
fn discount_live_hybrid_catalog_merges_upstream_and_staged_discounts() {
    let upstream_code_id = "gid://shopify/DiscountCodeNode/990001";
    let upstream_redeem_code_id = "gid://shopify/DiscountRedeemCode/990002";
    let upstream_automatic_id = "gid://shopify/DiscountAutomaticNode/990003";
    let upstream_code = upstream_code_basic_fixed_amount_discount(
        upstream_redeem_code_id,
        "Upstream code mixed",
        "ACTIVE",
    );
    let upstream_automatic =
        upstream_automatic_basic_discount("Upstream automatic mixed", "ACTIVE");
    let upstream_code_node = json!({
        "id": upstream_code_id,
        "codeDiscount": upstream_code,
        "metafields": upstream_discount_metafields(upstream_code_id)
    });
    let upstream_automatic_node = json!({
        "id": upstream_automatic_id,
        "automaticDiscount": upstream_automatic,
        "metafields": upstream_discount_metafields(upstream_automatic_id)
    });
    let upstream_admin_code_node = json!({
        "id": upstream_code_id,
        "discount": upstream_code_node["codeDiscount"].clone()
    });
    let upstream_admin_automatic_node = json!({
        "id": upstream_automatic_id,
        "discount": upstream_automatic_node["automaticDiscount"].clone()
    });
    let upstream_calls = Arc::new(Mutex::new(Vec::<String>::new()));
    let captured_calls = Arc::clone(&upstream_calls);
    let code_node_for_transport = upstream_code_node.clone();
    let automatic_node_for_transport = upstream_automatic_node.clone();
    let admin_code_node_for_transport = upstream_admin_code_node.clone();
    let admin_automatic_node_for_transport = upstream_admin_automatic_node.clone();
    let mut proxy =
        configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport(move |request| {
            let body: Value =
                serde_json::from_str(&request.body).expect("upstream GraphQL body parses");
            let query = body["query"].as_str().unwrap_or_default().to_string();
            captured_calls.lock().unwrap().push(query.clone());
            if query.contains("DiscountUniquenessCheck") {
                return Response {
                    status: 200,
                    headers: Default::default(),
                    body: json!({ "data": { "codeDiscountNodeByCode": null } }),
                };
            }
            let mut data = serde_json::Map::new();
            if query.contains("all: discountNodes") {
                data.insert(
                    "all".to_string(),
                    upstream_discount_connection(vec![
                        admin_code_node_for_transport.clone(),
                        admin_automatic_node_for_transport.clone(),
                    ]),
                );
            }
            if query.contains("codeOnly: codeDiscountNodes") {
                data.insert(
                    "codeOnly".to_string(),
                    upstream_discount_connection(vec![code_node_for_transport.clone()]),
                );
            }
            if query.contains("automaticOnly: automaticDiscountNodes") {
                data.insert(
                    "automaticOnly".to_string(),
                    upstream_discount_connection(vec![automatic_node_for_transport.clone()]),
                );
            }
            if query.contains("count: discountNodesCount") {
                data.insert(
                    "count".to_string(),
                    json!({ "count": 2, "precision": "EXACT" }),
                );
            }
            if query.contains("byCode: codeDiscountNodeByCode") {
                data.insert("byCode".to_string(), code_node_for_transport.clone());
            }
            if query.contains("byAutomatic: automaticDiscountNode") {
                data.insert(
                    "byAutomatic".to_string(),
                    automatic_node_for_transport.clone(),
                );
            }
            if !data.is_empty() {
                return Response {
                    status: 200,
                    headers: Default::default(),
                    body: json!({ "data": data }),
                };
            }
            Response {
                status: 200,
                headers: Default::default(),
                body: json!({
                    "errors": [{
                        "message": format!("unexpected mixed discount upstream request: {body}")
                    }]
                }),
            }
        });

    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation DiscountMixedCatalogLocalCreate {
          discountCodeBasicCreate(basicCodeDiscount: {
            title: "Local staged mixed",
            code: "MIXED-LOCAL",
            startsAt: "2026-04-22T00:00:00Z",
            context: { all: "ALL" },
            customerGets: { value: { percentage: 0.2 }, items: { all: true } }
          }) {
            codeDiscountNode { id codeDiscount { __typename ... on DiscountCodeBasic { title status } } }
            userErrors { field message code extraInfo }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        create.body["data"]["discountCodeBasicCreate"]["userErrors"],
        json!([])
    );
    let staged_id = json_string(
        &create.body["data"]["discountCodeBasicCreate"]["codeDiscountNode"]["id"],
        "staged mixed discount id",
    );

    let read = proxy.process_request(json_graphql_request(
        r#"
        query DiscountMixedCatalogRead($active: String!, $upstreamCode: String!, $upstreamAutomaticId: ID!) {
          all: discountNodes(first: 2, sortKey: TITLE, query: $active) {
            nodes {
              id
              discount {
                __typename
                ... on DiscountCodeBasic { title status }
                ... on DiscountAutomaticBasic { title status }
              }
            }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
          codeOnly: codeDiscountNodes(first: 5, sortKey: TITLE, query: $active) {
            nodes {
              id
              codeDiscount { __typename ... on DiscountCodeBasic { title status codes(first: 5) { nodes { code } } } }
            }
          }
          automaticOnly: automaticDiscountNodes(first: 5, sortKey: ID, query: $active) {
            nodes {
              id
              automaticDiscount { __typename ... on DiscountAutomaticBasic { title status } }
            }
          }
          count: discountNodesCount(query: $active) { count precision }
          byCode: codeDiscountNodeByCode(code: $upstreamCode) {
            id
            codeDiscount { __typename ... on DiscountCodeBasic { title status codes(first: 5) { nodes { code } } } }
          }
          byAutomatic: automaticDiscountNode(id: $upstreamAutomaticId) {
            id
            automaticDiscount { __typename ... on DiscountAutomaticBasic { title status } }
          }
        }
        "#,
        json!({
            "active": "status:active",
            "upstreamCode": "UPSTREAM-FIXED-5",
            "upstreamAutomaticId": upstream_automatic_id
        }),
    ));
    assert_eq!(read.status, 200);
    assert_eq!(
        discount_connection_node_titles(&read.body["data"]["all"], "discountNodes"),
        vec!["Local staged mixed", "Upstream automatic mixed"]
    );
    assert_eq!(
        read.body["data"]["all"]["pageInfo"],
        json!({
            "hasNextPage": true,
            "hasPreviousPage": false,
            "startCursor": staged_id,
            "endCursor": upstream_automatic_id
        })
    );
    assert_eq!(
        discount_connection_node_titles(&read.body["data"]["codeOnly"], "codeDiscountNodes"),
        vec!["Local staged mixed", "Upstream code mixed"]
    );
    assert_eq!(
        discount_connection_node_titles(
            &read.body["data"]["automaticOnly"],
            "automaticDiscountNodes"
        ),
        vec!["Upstream automatic mixed"]
    );
    assert_eq!(
        read.body["data"]["count"],
        json!({ "count": 3, "precision": "EXACT" })
    );
    assert_eq!(
        read.body["data"]["byCode"]["codeDiscount"]["title"],
        json!("Upstream code mixed")
    );
    assert_eq!(
        read.body["data"]["byAutomatic"]["automaticDiscount"]["title"],
        json!("Upstream automatic mixed")
    );

    let second_page = proxy.process_request(json_graphql_request(
        r#"
        query DiscountMixedCatalogSecondPage($active: String!, $after: String!) {
          all: discountNodes(first: 2, after: $after, sortKey: TITLE, query: $active) {
            nodes {
              id
              discount {
                __typename
                ... on DiscountCodeBasic { title status }
                ... on DiscountAutomaticBasic { title status }
              }
            }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
        }
        "#,
        json!({ "active": "status:active", "after": upstream_automatic_id }),
    ));
    assert_eq!(
        discount_connection_node_titles(&second_page.body["data"]["all"], "discountNodes"),
        vec!["Upstream code mixed"]
    );
    assert_eq!(
        second_page.body["data"]["all"]["pageInfo"],
        json!({
            "hasNextPage": false,
            "hasPreviousPage": true,
            "startCursor": upstream_code_id,
            "endCursor": upstream_code_id
        })
    );

    let update_automatic = proxy.process_request(json_graphql_request(
        r#"
        mutation DiscountMixedCatalogAutomaticUpdate($id: ID!) {
          discountAutomaticBasicUpdate(id: $id, automaticBasicDiscount: {
            title: "Updated upstream automatic mixed",
            startsAt: "2026-04-21T19:31:14Z"
          }) {
            automaticDiscountNode {
              id
              automaticDiscount { __typename ... on DiscountAutomaticBasic { title status } }
            }
            userErrors { field message code extraInfo }
          }
        }
        "#,
        json!({ "id": upstream_automatic_id }),
    ));
    assert_eq!(
        update_automatic.body["data"]["discountAutomaticBasicUpdate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        update_automatic.body["data"]["discountAutomaticBasicUpdate"]["automaticDiscountNode"]
            ["automaticDiscount"]["title"],
        json!("Updated upstream automatic mixed")
    );

    let delete_code = proxy.process_request(json_graphql_request(
        r#"
        mutation DiscountMixedCatalogCodeDelete($id: ID!) {
          discountCodeDelete(id: $id) {
            deletedCodeDiscountId
            userErrors { field message code extraInfo }
          }
        }
        "#,
        json!({ "id": upstream_code_id }),
    ));
    assert_eq!(
        delete_code.body["data"]["discountCodeDelete"],
        json!({ "deletedCodeDiscountId": upstream_code_id, "userErrors": [] })
    );

    let after_mutations = proxy.process_request(json_graphql_request(
        r#"
        query DiscountMixedCatalogAfterMutations($active: String!, $upstreamCode: String!) {
          codeOnly: codeDiscountNodes(first: 5, sortKey: TITLE, query: $active) {
            nodes {
              id
              codeDiscount { __typename ... on DiscountCodeBasic { title status } }
            }
          }
          automaticOnly: automaticDiscountNodes(first: 5, sortKey: ID, query: $active) {
            nodes {
              id
              automaticDiscount { __typename ... on DiscountAutomaticBasic { title status } }
            }
          }
          count: discountNodesCount(query: $active) { count precision }
          byCode: codeDiscountNodeByCode(code: $upstreamCode) {
            id
            codeDiscount { __typename ... on DiscountCodeBasic { title status } }
          }
        }
        "#,
        json!({ "active": "status:active", "upstreamCode": "UPSTREAM-FIXED-5" }),
    ));
    assert_eq!(
        discount_connection_node_titles(
            &after_mutations.body["data"]["codeOnly"],
            "codeDiscountNodes"
        ),
        vec!["Local staged mixed"]
    );
    assert_eq!(
        discount_connection_node_titles(
            &after_mutations.body["data"]["automaticOnly"],
            "automaticDiscountNodes"
        ),
        vec!["Updated upstream automatic mixed"]
    );
    assert_eq!(
        after_mutations.body["data"]["count"],
        json!({ "count": 2, "precision": "EXACT" })
    );
    assert_eq!(after_mutations.body["data"]["byCode"], json!(null));

    let calls = upstream_calls.lock().unwrap();
    assert!(
        calls
            .iter()
            .filter(|query| query.contains("codeOnly: codeDiscountNodes"))
            .count()
            >= 2,
        "post-mutation discount catalog read should hydrate the upstream catalog, got {calls:?}"
    );
}

#[test]
fn discount_nodes_connection_windows_edges_page_info_and_count_limit() {
    let mut proxy = snapshot_proxy();
    let seed = seed_discount_connection_mechanics(&mut proxy);

    let first_page = proxy.process_request(json_graphql_request(
        r#"
        query DiscountNodesWindowFirst {
          discountNodes(first: 2, sortKey: TITLE) {
            nodes {
              id
              discount { __typename ... on DiscountCodeBasic { title } ... on DiscountAutomaticBasic { title } }
            }
            edges {
              cursor
              node {
                id
                discount { __typename ... on DiscountCodeBasic { title } ... on DiscountAutomaticBasic { title } }
              }
            }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
          limitedCount: discountNodesCount(limit: 2) { count precision }
          exactCount: discountNodesCount(limit: 4) { count precision }
        }
        "#,
        json!({}),
    ));
    assert_eq!(first_page.status, 200);
    let first_connection = &first_page.body["data"]["discountNodes"];
    assert_eq!(
        discount_connection_node_titles(first_connection, "discountNodes"),
        vec!["Alpha automatic connection", "Bravo code connection"]
    );
    assert_eq!(
        discount_connection_edge_titles(first_connection, "discountNodes"),
        vec!["Alpha automatic connection", "Bravo code connection"]
    );
    assert_eq!(
        first_connection["pageInfo"],
        json!({
            "hasNextPage": true,
            "hasPreviousPage": false,
            "startCursor": seed.automatic_alpha_id,
            "endCursor": seed.code_bravo_id
        })
    );
    assert_eq!(
        first_page.body["data"]["limitedCount"],
        json!({ "count": 2, "precision": "AT_LEAST" })
    );
    assert_eq!(
        first_page.body["data"]["exactCount"],
        json!({ "count": 4, "precision": "EXACT" })
    );

    let second_page = proxy.process_request(json_graphql_request(
        r#"
        query DiscountNodesWindowAfter($after: String!) {
          discountNodes(first: 2, after: $after, sortKey: TITLE) {
            nodes {
              id
              discount { __typename ... on DiscountCodeBasic { title } ... on DiscountAutomaticBasic { title } }
            }
            edges {
              cursor
              node {
                id
                discount { __typename ... on DiscountCodeBasic { title } ... on DiscountAutomaticBasic { title } }
              }
            }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
        }
        "#,
        json!({ "after": seed.code_bravo_id }),
    ));
    assert_eq!(second_page.status, 200);
    let second_connection = &second_page.body["data"]["discountNodes"];
    assert_eq!(
        discount_connection_node_titles(second_connection, "discountNodes"),
        vec!["Yankee automatic connection", "Zulu code connection"]
    );
    assert_eq!(
        second_connection["pageInfo"],
        json!({
            "hasNextPage": false,
            "hasPreviousPage": true,
            "startCursor": seed.automatic_yankee_id,
            "endCursor": seed.code_zulu_id
        })
    );
}

#[test]
fn discount_list_roots_honor_all_sort_keys_and_reverse() {
    let mut proxy = snapshot_proxy();
    seed_discount_connection_mechanics(&mut proxy);

    for sort_key in ["CREATED_AT", "ENDS_AT", "STARTS_AT", "TITLE", "UPDATED_AT"] {
        assert_eq!(
            read_discount_connection_titles(&mut proxy, "discountNodes", sort_key, false),
            vec![
                "Alpha automatic connection",
                "Bravo code connection",
                "Yankee automatic connection",
                "Zulu code connection"
            ],
            "{sort_key} should sort all discount nodes by the requested value"
        );
        assert_eq!(
            read_discount_connection_titles(&mut proxy, "discountNodes", sort_key, true),
            vec![
                "Zulu code connection",
                "Yankee automatic connection",
                "Bravo code connection",
                "Alpha automatic connection"
            ],
            "{sort_key} reverse should reverse all discount nodes"
        );
        assert_eq!(
            read_discount_connection_titles(&mut proxy, "codeDiscountNodes", sort_key, false),
            vec!["Bravo code connection", "Zulu code connection"],
            "{sort_key} should sort code discount nodes"
        );
        assert_eq!(
            read_discount_connection_titles(&mut proxy, "codeDiscountNodes", sort_key, true),
            vec!["Zulu code connection", "Bravo code connection"],
            "{sort_key} reverse should reverse code discount nodes"
        );
        if sort_key == "CREATED_AT" {
            assert_eq!(
                read_discount_connection_titles(
                    &mut proxy,
                    "automaticDiscountNodes",
                    sort_key,
                    false,
                ),
                vec!["Alpha automatic connection", "Yankee automatic connection"],
                "{sort_key} should sort automatic discount nodes"
            );
            assert_eq!(
                read_discount_connection_titles(
                    &mut proxy,
                    "automaticDiscountNodes",
                    sort_key,
                    true,
                ),
                vec!["Yankee automatic connection", "Alpha automatic connection"],
                "{sort_key} reverse should reverse automatic discount nodes"
            );
        }
    }

    for sort_key in ["ID", "RELEVANCE"] {
        assert_eq!(
            read_discount_connection_titles(&mut proxy, "discountNodes", sort_key, false),
            vec![
                "Zulu code connection",
                "Bravo code connection",
                "Yankee automatic connection",
                "Alpha automatic connection"
            ],
            "{sort_key} should use id order for all discount nodes"
        );
        assert_eq!(
            read_discount_connection_titles(&mut proxy, "discountNodes", sort_key, true),
            vec![
                "Alpha automatic connection",
                "Yankee automatic connection",
                "Bravo code connection",
                "Zulu code connection"
            ],
            "{sort_key} reverse should reverse id order for all discount nodes"
        );
        assert_eq!(
            read_discount_connection_titles(&mut proxy, "codeDiscountNodes", sort_key, false),
            vec!["Zulu code connection", "Bravo code connection"],
            "{sort_key} should use id order for code discount nodes"
        );
        assert_eq!(
            read_discount_connection_titles(&mut proxy, "codeDiscountNodes", sort_key, true),
            vec!["Bravo code connection", "Zulu code connection"],
            "{sort_key} reverse should reverse id order for code discount nodes"
        );
        if sort_key == "ID" {
            assert_eq!(
                read_discount_connection_titles(
                    &mut proxy,
                    "automaticDiscountNodes",
                    sort_key,
                    false,
                ),
                vec!["Yankee automatic connection", "Alpha automatic connection"],
                "{sort_key} should use id order for automatic discount nodes"
            );
            assert_eq!(
                read_discount_connection_titles(
                    &mut proxy,
                    "automaticDiscountNodes",
                    sort_key,
                    true,
                ),
                vec!["Alpha automatic connection", "Yankee automatic connection"],
                "{sort_key} reverse should reverse id order for automatic discount nodes"
            );
        }
    }
}

#[test]
fn discount_redeem_code_connection_windows_edges_and_page_info() {
    let mut proxy = snapshot_proxy();
    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation DiscountRedeemCodeWindowCreate($input: DiscountCodeBasicInput!) {
          discountCodeBasicCreate(basicCodeDiscount: $input) {
            codeDiscountNode { id }
            userErrors { field message code extraInfo }
          }
        }
        "#,
        json!({ "input": {
            "title": "Redeem code connection window",
            "code": "CONNECTION-CODE-BASE",
            "startsAt": "2026-06-10T00:00:00Z",
            "context": { "all": "ALL" },
            "customerGets": { "value": { "percentage": 0.1 }, "items": { "all": true } }
        }}),
    ));
    assert_eq!(
        create.body["data"]["discountCodeBasicCreate"]["userErrors"],
        json!([])
    );
    let discount_id = json_string(
        &create.body["data"]["discountCodeBasicCreate"]["codeDiscountNode"]["id"],
        "redeem code window discount id",
    );

    let add = proxy.process_request(json_graphql_request(
        r#"
        mutation DiscountRedeemCodeWindowAdd($discountId: ID!) {
          discountRedeemCodeBulkAdd(discountId: $discountId, codes: [
            { code: "CONNECTION-CODE-A" },
            { code: "CONNECTION-CODE-B" },
            { code: "CONNECTION-CODE-C" }
          ]) {
            bulkCreation { codesCount }
            userErrors { field message code extraInfo }
          }
        }
        "#,
        json!({ "discountId": discount_id.clone() }),
    ));
    assert_eq!(
        add.body["data"]["discountRedeemCodeBulkAdd"]["userErrors"],
        json!([])
    );

    let first_page = proxy.process_request(json_graphql_request(
        r#"
        query DiscountRedeemCodeWindowFirst($discountId: ID!) {
          codeDiscountNode(id: $discountId) {
            codeDiscount {
              ... on DiscountCodeBasic {
                codes(first: 2) {
                  nodes { id code }
                  edges { cursor node { code } }
                  pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
                }
              }
            }
          }
        }
        "#,
        json!({ "discountId": discount_id.clone() }),
    ));
    let first_codes = &first_page.body["data"]["codeDiscountNode"]["codeDiscount"]["codes"];
    assert_eq!(
        first_codes["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|node| json_string(&node["code"], "redeem code"))
            .collect::<Vec<_>>(),
        vec!["CONNECTION-CODE-BASE", "CONNECTION-CODE-A"]
    );
    assert_eq!(
        first_codes["edges"]
            .as_array()
            .unwrap()
            .iter()
            .map(|edge| json_string(&edge["node"]["code"], "redeem code edge"))
            .collect::<Vec<_>>(),
        vec!["CONNECTION-CODE-BASE", "CONNECTION-CODE-A"]
    );
    assert_eq!(first_codes["pageInfo"]["hasNextPage"], json!(true));
    assert_eq!(first_codes["pageInfo"]["hasPreviousPage"], json!(false));
    assert_eq!(
        first_codes["pageInfo"]["startCursor"],
        first_codes["edges"][0]["cursor"]
    );
    assert_eq!(
        first_codes["pageInfo"]["endCursor"],
        first_codes["edges"][1]["cursor"]
    );
    let after = json_string(
        &first_codes["pageInfo"]["endCursor"],
        "redeem code first page end cursor",
    );

    let second_page = proxy.process_request(json_graphql_request(
        r#"
        query DiscountRedeemCodeWindowAfter($discountId: ID!, $after: String!) {
          codeDiscountNode(id: $discountId) {
            codeDiscount {
              ... on DiscountCodeBasic {
                codes(first: 2, after: $after) {
                  nodes { code }
                  edges { cursor node { code } }
                  pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
                }
              }
            }
          }
        }
        "#,
        json!({ "discountId": discount_id, "after": after }),
    ));
    let second_codes = &second_page.body["data"]["codeDiscountNode"]["codeDiscount"]["codes"];
    assert_eq!(
        second_codes["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|node| json_string(&node["code"], "redeem code"))
            .collect::<Vec<_>>(),
        vec!["CONNECTION-CODE-B", "CONNECTION-CODE-C"]
    );
    assert_eq!(second_codes["pageInfo"]["hasNextPage"], json!(false));
    assert_eq!(second_codes["pageInfo"]["hasPreviousPage"], json!(true));
}

#[test]
fn functions_metadata_local_staging_updates_deletes_and_reads_validation_cart_and_function_roots() {
    let mut proxy = function_metadata_proxy();
    let stage = r#"mutation StageFunctionMetadata($validation: ValidationCreateInput!, $cartFunctionHandle: String!, $cartBlockOnFailure: Boolean!, $ready: Boolean!) { validationCreate(validation: $validation) { validation { id title enabled blockOnFailure shopifyFunction { id title handle apiType } } userErrors { field message code } } cartTransformCreate(functionHandle: $cartFunctionHandle, blockOnFailure: $cartBlockOnFailure) { cartTransform { id blockOnFailure functionId } userErrors { field message code } } taxAppConfigure(ready: $ready) { taxAppConfiguration { state } userErrors { field message code } } }"#;
    let missing_validation_delete = r#"mutation DeleteFunctionValidation($id: ID!) { validationDelete(id: $id) { deletedId userErrors { field message code } } }"#;
    let missing_validation_response = proxy.process_request(json_graphql_request(
        missing_validation_delete,
        json!({ "id": "gid://shopify/Validation/999999999999" }),
    ));
    assert_eq!(
        missing_validation_response.body["data"]["validationDelete"],
        json!({
            "deletedId": null,
            "userErrors": [{ "field": ["id"], "message": "Extension not found.", "code": "NOT_FOUND" }]
        })
    );

    let missing_cart_delete = r#"mutation DeleteFunctionCartTransform($id: ID!) { cartTransformDelete(id: $id) { deletedId userErrors { field message code } } }"#;
    let missing_cart_response = proxy.process_request(json_graphql_request(
        missing_cart_delete,
        json!({ "id": "gid://shopify/CartTransform/999999999999" }),
    ));
    assert_eq!(
        missing_cart_response.body["data"]["cartTransformDelete"],
        json!({
            "deletedId": null,
            "userErrors": [{ "field": ["id"], "message": "Could not find cart transform with id: gid://shopify/CartTransform/999999999999", "code": "NOT_FOUND" }]
        })
    );

    let stage_response = proxy.process_request(tax_app_graphql_request(stage, json!({
        "validation": { "functionHandle": "validation-local", "title": "Local validation", "enable": true, "blockOnFailure": true },
        "cartFunctionHandle": "cart-transform-local",
        "cartBlockOnFailure": true,
        "ready": true
    })));
    let validation_id = stage_response.body["data"]["validationCreate"]["validation"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let cart_transform_id = stage_response.body["data"]["cartTransformCreate"]["cartTransform"]
        ["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_synthetic_gid(&validation_id, "Validation");
    assert_synthetic_gid(&cart_transform_id, "CartTransform");
    assert_eq!(
        stage_response.body["data"]["validationCreate"]["validation"]["shopifyFunction"],
        json!({
            "id": "gid://shopify/ShopifyFunction/validation-local",
            "title": "Validation Local",
            "handle": "validation-local",
            "apiType": "VALIDATION"
        })
    );
    assert_eq!(
        stage_response.body["data"]["cartTransformCreate"]["cartTransform"],
        json!({
            "id": cart_transform_id,
            "blockOnFailure": true,
            "functionId": "gid://shopify/ShopifyFunction/cart-transform-local"
        })
    );

    let update = r#"mutation UpdateFunctionValidation($id: ID!, $validation: ValidationUpdateInput!) { validationUpdate(id: $id, validation: $validation) { validation { id title enabled blockOnFailure shopifyFunction { handle } } userErrors { field message code } } }"#;
    let update_response = proxy.process_request(json_graphql_request(update, json!({
        "id": validation_id,
        "validation": { "title": "Updated validation", "enable": false, "blockOnFailure": false }
    })));
    assert_eq!(
        update_response.body["data"]["validationUpdate"]["validation"]["id"],
        json!(validation_id)
    );
    assert_eq!(
        update_response.body["data"]["validationUpdate"]["validation"]["title"],
        json!("Updated validation")
    );
    assert_eq!(
        update_response.body["data"]["validationUpdate"]["validation"]["enabled"],
        json!(false)
    );
    assert_eq!(
        update_response.body["data"]["validationUpdate"]["validation"]["blockOnFailure"],
        json!(false)
    );
    assert_eq!(
        update_response.body["data"]["validationUpdate"]["validation"]["shopifyFunction"]["handle"],
        json!("validation-local")
    );

    let read = r#"query ReadFunctionMetadata($validationId: ID!) { validation(id: $validationId) { id title enabled blockOnFailure shopifyFunction { id title handle apiType } } validations(first: 5) { nodes { id title enabled blockOnFailure } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } cartTransforms(first: 5) { nodes { id blockOnFailure functionId } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } validationFunctions: shopifyFunctions(first: 5, apiType: "VALIDATION") { nodes { id title handle apiType } } cartFunctions: shopifyFunctions(first: 5, apiType: "CART_TRANSFORM") { nodes { id title handle apiType } } cartFunction: shopifyFunction(id: "gid://shopify/ShopifyFunction/cart-transform-local") { id title handle apiType } }"#;
    let read_response = proxy.process_request(json_graphql_request(
        read,
        json!({ "validationId": validation_id }),
    ));
    assert_eq!(
        read_response.body["data"]["validation"]["title"],
        json!("Updated validation")
    );
    assert_eq!(
        read_response.body["data"]["validations"]["nodes"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        read_response.body["data"]["cartTransforms"]["nodes"][0]["id"],
        json!(cart_transform_id)
    );
    assert_eq!(
        read_response.body["data"]["validationFunctions"]["nodes"][0]["handle"],
        json!("validation-local")
    );
    assert_eq!(
        read_response.body["data"]["cartFunctions"]["nodes"][0]["handle"],
        json!("cart-transform-local")
    );
    assert_eq!(
        read_response.body["data"]["cartFunction"]["apiType"],
        json!("CART_TRANSFORM")
    );

    let node_read = r#"query CartTransformNodeRead($id: ID!) { node(id: $id) { ... on CartTransform { id blockOnFailure functionId } } }"#;
    let node_response = proxy.process_request(json_graphql_request(
        node_read,
        json!({ "id": cart_transform_id }),
    ));
    assert_eq!(
        node_response.body["data"]["node"],
        read_response.body["data"]["cartTransforms"]["nodes"][0]
    );

    let delete_validation = r#"mutation DeleteFunctionValidation($id: ID!) { validationDelete(id: $id) { deletedId userErrors { field message code } } }"#;
    let validation_delete_response = proxy.process_request(json_graphql_request(
        delete_validation,
        json!({ "id": validation_id }),
    ));
    assert_eq!(
        validation_delete_response.body["data"]["validationDelete"],
        json!({ "deletedId": validation_id, "userErrors": [] })
    );

    let delete_cart_transform = r#"mutation DeleteFunctionCartTransform($id: ID!) { cartTransformDelete(id: $id) { deletedId userErrors { field message code } } }"#;
    let cart_delete_response = proxy.process_request(json_graphql_request(
        delete_cart_transform,
        json!({ "id": cart_transform_id }),
    ));
    assert_eq!(
        cart_delete_response.body["data"]["cartTransformDelete"],
        json!({ "deletedId": cart_transform_id, "userErrors": [] })
    );

    let empty_read = r#"query ReadDeletedFunctionMetadata($validationId: ID!) { validation(id: $validationId) { id } validations(first: 5) { nodes { id } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } cartTransforms(first: 5) { nodes { id } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } }"#;
    let empty_response = proxy.process_request(json_graphql_request(
        empty_read,
        json!({ "validationId": validation_id }),
    ));
    assert_eq!(empty_response.body["data"]["validation"], Value::Null);
    assert_eq!(
        empty_response.body["data"]["validations"]["nodes"],
        json!([])
    );
    assert_eq!(
        empty_response.body["data"]["cartTransforms"]["nodes"],
        json!([])
    );
}

#[test]
fn functions_validation_reads_reject_fabricated_output_fields() {
    let mut proxy = function_metadata_proxy();

    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation StageValidationForOutputFieldValidation {
          validationCreate(validation: { functionHandle: "validation-local", title: "Local validation", enable: true, blockOnFailure: true }) {
            validation { id }
            userErrors { field message code }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        create.body["data"]["validationCreate"]["userErrors"],
        json!([])
    );
    let validation_id = create.body["data"]["validationCreate"]["validation"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let invalid = proxy.process_request(json_graphql_request(
        r#"
        query InvalidValidationOutputFields($id: ID!) {
          validation(id: $id) {
            functionId
            functionHandle
            createdAt
            updatedAt
            enable
          }
          validations(first: 5) {
            nodes {
              functionId
              functionHandle
              createdAt
              updatedAt
              enable
            }
          }
          node(id: $id) {
            ... on Validation {
              functionId
              functionHandle
              createdAt
              updatedAt
              enable
            }
          }
        }
        "#,
        json!({ "id": validation_id }),
    ));
    assert_eq!(invalid.status, 200);
    let errors = invalid.body["errors"].as_array().unwrap();
    for field_name in [
        "functionId",
        "functionHandle",
        "createdAt",
        "updatedAt",
        "enable",
    ] {
        assert!(
            errors.iter().any(|error| {
                error["message"]
                    == format!("Field '{field_name}' doesn't exist on type 'Validation'")
                    && error["extensions"]["typeName"] == "Validation"
                    && error["extensions"]["fieldName"] == field_name
            }),
            "missing undefined-field error for Validation.{field_name}: {errors:#?}"
        );
    }
    assert!(invalid.body.get("data").is_none());

    let valid = proxy.process_request(json_graphql_request(
        r#"
        query ValidValidationOutputFields($id: ID!) {
          validation(id: $id) {
            id
            title
            enabled
            blockOnFailure
            shopifyFunction { id }
          }
          validations(first: 5) {
            nodes {
              id
              title
              enabled
              blockOnFailure
              shopifyFunction { id }
            }
          }
          node(id: $id) {
            ... on Validation {
              id
              title
              enabled
              blockOnFailure
              shopifyFunction { id }
            }
          }
        }
        "#,
        json!({ "id": validation_id }),
    ));
    assert_eq!(valid.status, 200);
    assert!(valid.body.get("errors").is_none());
    assert_eq!(
        valid.body["data"]["validation"],
        json!({
            "id": validation_id,
            "title": "Local validation",
            "enabled": true,
            "blockOnFailure": true,
            "shopifyFunction": { "id": "gid://shopify/ShopifyFunction/validation-local" }
        })
    );
    assert_eq!(
        valid.body["data"]["validations"]["nodes"][0],
        valid.body["data"]["validation"]
    );
    assert_eq!(valid.body["data"]["node"], valid.body["data"]["validation"]);
}

#[test]
fn tax_app_configure_stages_configuration_state() {
    let mut proxy = function_metadata_proxy();

    let configure = proxy.process_request(tax_app_graphql_request(
        r#"
        mutation ConfigureTaxApp($ready: Boolean!) {
          taxAppConfigure(ready: $ready) {
            taxAppConfiguration { state }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "ready": true }),
    ));
    assert_eq!(
        configure.body["data"]["taxAppConfigure"]["userErrors"],
        json!([])
    );
    assert_eq!(
        configure.body["data"]["taxAppConfigure"]["taxAppConfiguration"],
        json!({ "state": "READY" })
    );

    let update = proxy.process_request(tax_app_graphql_request(
        r#"
        mutation UpdateTaxAppReadiness($ready: Boolean!) {
          taxAppConfigure(ready: $ready) {
            taxAppConfiguration { state }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "ready": false }),
    ));
    assert_eq!(
        update.body["data"]["taxAppConfigure"]["taxAppConfiguration"]["state"],
        json!("PENDING")
    );
}

#[test]
fn tax_app_configure_requires_tax_calculations_app_authority() {
    let mut proxy = function_metadata_proxy();

    let response = proxy.process_request(json_graphql_request(
        r#"
        mutation ConfigureTaxAppWithoutAuthority($ready: Boolean!) {
          taxAppConfigure(ready: $ready) {
            taxAppConfiguration { state }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "ready": true }),
    ));

    assert_eq!(response.body["data"]["taxAppConfigure"], Value::Null);
    let errors = response.body["errors"].as_array().unwrap();
    assert_eq!(errors.len(), 1);
    assert_eq!(
        errors[0]["message"],
        json!("Access denied for taxAppConfigure field. Required access: `write_taxes` access scope. Also: The caller must be a tax calculations app.")
    );
    assert_eq!(errors[0]["extensions"]["code"], json!("ACCESS_DENIED"));
    assert_eq!(
        errors[0]["extensions"]["requiredAccess"],
        json!("`write_taxes` access scope. Also: The caller must be a tax calculations app.")
    );
    assert_eq!(errors[0]["path"], json!(["taxAppConfigure"]));
}

#[test]
fn functions_owner_metadata_stages_validation_cart_tax_and_downstream_reads() {
    let mut proxy = function_metadata_proxy();

    let stage = proxy.process_request(tax_app_graphql_request(
        r#"
        mutation StageOwnedFunctionMetadata($validation: ValidationCreateInput!, $cartFunctionHandle: String!, $cartBlockOnFailure: Boolean!, $ready: Boolean!) {
          validationCreate(validation: $validation) { validation { id title enabled blockOnFailure shopifyFunction { id title handle apiType description appKey app { __typename id title handle apiKey } } } userErrors { field message code } }
          cartTransformCreate(functionHandle: $cartFunctionHandle, blockOnFailure: $cartBlockOnFailure) { cartTransform { id blockOnFailure functionId } userErrors { field message code } }
          taxAppConfigure(ready: $ready) { taxAppConfiguration { state } userErrors { field message code } }
        }
        "#,
        json!({
            "validation": { "functionId": "gid://shopify/ShopifyFunction/validation-owned", "title": "Owned validation", "enable": true, "blockOnFailure": true },
            "cartFunctionHandle": "cart-owned",
            "cartBlockOnFailure": true,
            "ready": true
        }),
    ));
    let validation_id = stage.body["data"]["validationCreate"]["validation"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_synthetic_gid(&validation_id, "Validation");
    assert_eq!(
        stage.body["data"]["validationCreate"]["validation"]["shopifyFunction"]["app"]["apiKey"],
        json!("validation-app-key")
    );
    assert_eq!(
        stage.body["data"]["cartTransformCreate"]["cartTransform"]["functionId"],
        json!("gid://shopify/ShopifyFunction/cart-owned")
    );
    assert_eq!(
        stage.body["data"]["taxAppConfigure"]["taxAppConfiguration"]["state"],
        json!("READY")
    );

    let update = proxy.process_request(json_graphql_request(
        r#"
        mutation UpdateOwnedFunctionValidation($id: ID!, $validation: ValidationUpdateInput!) {
          validationUpdate(id: $id, validation: $validation) { validation { id title enabled blockOnFailure shopifyFunction { id handle appKey app { title apiKey } } } userErrors { field message code } }
        }
        "#,
        json!({ "id": validation_id, "validation": { "title": "Owned validation renamed" } }),
    ));
    assert_eq!(
        update.body["data"]["validationUpdate"]["validation"]["title"],
        json!("Owned validation renamed")
    );
    assert_eq!(
        update.body["data"]["validationUpdate"]["validation"]["enabled"],
        json!(false)
    );
    assert_eq!(
        update.body["data"]["validationUpdate"]["validation"]["shopifyFunction"]["app"]["apiKey"],
        json!("validation-app-key")
    );

    let read = proxy.process_request(json_graphql_request(
        r#"
        query ReadOwnedFunctionMetadata($validationId: ID!) {
          validation(id: $validationId) { id title enabled blockOnFailure shopifyFunction { id title handle apiType description appKey app { __typename id title handle apiKey } } }
          validationFunctions: shopifyFunctions(first: 5, apiType: "VALIDATION") { nodes { id title handle apiType appKey app { title apiKey } } }
          cartFunction: shopifyFunction(id: "gid://shopify/ShopifyFunction/cart-owned") { id title handle apiType appKey app { __typename title apiKey } }
        }
        "#,
        json!({ "validationId": validation_id }),
    ));
    assert_eq!(
        read.body["data"]["validation"]["title"],
        json!("Owned validation renamed")
    );
    assert_eq!(
        read.body["data"]["validationFunctions"]["nodes"][0]["app"]["apiKey"],
        json!("validation-app-key")
    );
    assert_eq!(
        read.body["data"]["cartFunction"]["app"]["apiKey"],
        json!("cart-app-key")
    );
}

#[test]
fn functions_validation_create_errors_return_null_and_do_not_stage_records() {
    let mut proxy = function_metadata_proxy();

    let response = proxy.process_request(json_graphql_request(
        r#"
        mutation FunctionsValidationCreateErrorShape(
          $unknownFunctionId: String!
          $cartFunctionId: String!
          $cartFunctionHandle: String!
        ) {
          unknownFunction: validationCreate(validation: { functionId: $unknownFunctionId, title: "Unknown" }) {
            validation { id }
            userErrors { code field message }
          }
          apiMismatch: validationCreate(validation: { functionId: $cartFunctionId, title: "Wrong API" }) {
            validation { id }
            userErrors { code field message }
          }
          missingIdentifier: validationCreate(validation: {}) {
            validation { id }
            userErrors { code field message }
          }
          multipleIdentifiers: validationCreate(validation: { functionId: $cartFunctionId, functionHandle: $cartFunctionHandle }) {
            validation { id }
            userErrors { code field message }
          }
        }
        "#,
        json!({
            "unknownFunctionId": "01900000-0000-7000-8000-000000000000",
            "cartFunctionId": "019dd44b-127f-724b-a49c-70fc98ff4d72",
            "cartFunctionHandle": "conformance-cart-transform"
        }),
    ));

    assert_eq!(
        response.body["data"],
        json!({
            "unknownFunction": {
                "validation": null,
                "userErrors": [{ "code": "NOT_FOUND", "field": ["validation", "functionId"], "message": "Extension not found." }]
            },
            "apiMismatch": {
                "validation": null,
                "userErrors": [{ "code": "FUNCTION_DOES_NOT_IMPLEMENT", "field": ["validation", "functionId"], "message": "Unexpected Function API. The provided function must implement one of the following extension targets: [%{targets}]." }]
            },
            "missingIdentifier": {
                "validation": null,
                "userErrors": [{ "code": "MISSING_FUNCTION_IDENTIFIER", "field": ["validation", "functionHandle"], "message": "Either function_id or function_handle must be provided." }]
            },
            "multipleIdentifiers": {
                "validation": null,
                "userErrors": [{ "code": "MULTIPLE_FUNCTION_IDENTIFIERS", "field": ["validation"], "message": "Only one of function_id or function_handle can be provided, not both." }]
            }
        })
    );

    let read = proxy.process_request(json_graphql_request(
        r#"query FunctionsValidationCreateErrorRead { validations(first: 5) { nodes { id } } }"#,
        json!({}),
    ));
    assert_eq!(read.body["data"]["validations"]["nodes"], json!([]));
}

#[test]
fn functions_validation_create_title_fallback_uses_hydrated_function_title() {
    let mut proxy = function_metadata_proxy();

    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation ValidationCreateTitleFallbacks {
          omitted: validationCreate(validation: { functionHandle: "conformance-validation" }) {
            validation { id title shopifyFunction { title handle } }
            userErrors { field message code }
          }
          explicitNull: validationCreate(validation: { functionHandle: "conformance-validation", title: null }) {
            validation { id title shopifyFunction { title handle } }
            userErrors { field message code }
          }
          emptyString: validationCreate(validation: { functionHandle: "conformance-validation", title: "" }) {
            validation { id title shopifyFunction { title handle } }
            userErrors { field message code }
          }
        }
        "#,
        json!({}),
    ));

    for alias in ["omitted", "explicitNull", "emptyString"] {
        assert_eq!(
            create.body["data"][alias]["userErrors"],
            json!([]),
            "{alias} should not return userErrors"
        );
    }
    assert_eq!(
        create.body["data"]["omitted"]["validation"]["title"],
        json!("Conformance Validation")
    );
    assert_eq!(
        create.body["data"]["explicitNull"]["validation"]["title"],
        json!("Conformance Validation")
    );
    assert_eq!(
        create.body["data"]["emptyString"]["validation"]["title"],
        json!("")
    );
    assert_eq!(
        create.body["data"]["omitted"]["validation"]["shopifyFunction"],
        json!({ "title": "Conformance Validation", "handle": "conformance-validation" })
    );
    let omitted_id = json_string(
        &create.body["data"]["omitted"]["validation"]["id"],
        "omitted validation id",
    );
    assert_synthetic_gid(&omitted_id, "Validation");

    let read = proxy.process_request(json_graphql_request(
        r#"
        query ValidationTitleFallbackRead($id: ID!) {
          validation(id: $id) { id title }
          validations(first: 3) { nodes { title } }
        }
        "#,
        json!({ "id": omitted_id }),
    ));

    assert_eq!(
        read.body["data"]["validation"]["title"],
        json!("Conformance Validation")
    );
    assert_eq!(
        read.body["data"]["validations"]["nodes"],
        json!([
            { "title": "Conformance Validation" },
            { "title": "Conformance Validation" },
            { "title": "" }
        ])
    );
}

#[test]
fn functions_handle_lookup_uses_narrow_query_and_reuses_metadata() {
    let upstream_hits = Arc::new(Mutex::new(Vec::new()));
    let mut proxy = function_metadata_proxy_with_hits(Arc::clone(&upstream_hits));

    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation RepeatedHandleFunctionValidation {
          first: validationCreate(validation: { functionHandle: "validation-alpha", title: "First" }) {
            validation { id shopifyFunction { id handle apiType } }
            userErrors { field message code }
          }
          second: validationCreate(validation: { functionHandle: "validation-alpha", title: "Second" }) {
            validation { id shopifyFunction { id handle apiType } }
            userErrors { field message code }
          }
        }
        "#,
        json!({}),
    ));

    assert_eq!(create.status, 200);
    assert_eq!(create.body["data"]["first"]["userErrors"], json!([]));
    assert_eq!(create.body["data"]["second"]["userErrors"], json!([]));

    let follow_up = proxy.process_request(json_graphql_request(
        r#"
        mutation ReusedHandleFunctionValidation {
          validationCreate(validation: { functionHandle: "validation-alpha", title: "Third" }) {
            validation { id shopifyFunction { handle } }
            userErrors { field message code }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(follow_up.status, 200);
    assert_eq!(
        follow_up.body["data"]["validationCreate"]["userErrors"],
        json!([])
    );

    let hits = upstream_hits.lock().unwrap();
    let handle_hits = hits
        .iter()
        .filter(|body| body["operationName"].as_str() == Some("FunctionHydrateByHandle"))
        .collect::<Vec<_>>();
    assert_eq!(
        handle_hits.len(),
        1,
        "repeated same-handle validation creates should reuse hydrated function metadata"
    );
    let query = handle_hits[0]["query"].as_str().unwrap_or_default();
    assert!(
        query.contains("shopifyFunctions(first: 1, handle: $handle)"),
        "single-handle lookup should query Shopify by handle instead of scanning the catalog: {query}"
    );
    assert!(
        !query.contains("first: 100"),
        "single-handle lookup should not scan the first 100 Shopify functions: {query}"
    );
    assert_eq!(
        handle_hits[0]["variables"],
        json!({ "handle": "validation-alpha", "apiType": "VALIDATION" })
    );
}

#[test]
fn functions_cold_reads_forward_and_hydrate_non_catalog_function_metadata() {
    let upstream_hits = Arc::new(Mutex::new(0usize));
    let hit_counter = Arc::clone(&upstream_hits);
    let mut upstream_function =
        test_function_metadata_by_id_or_handle(None, Some("non-catalog-validation")).unwrap();
    upstream_function["apiType"] = json!("cart_checkout_validation");
    let mut proxy = configured_proxy(
        ReadMode::LiveHybrid,
        Some(shopify_draft_proxy::proxy::UnsupportedMutationMode::Passthrough),
    )
    .with_upstream_transport(move |request| {
        let body: Value =
            serde_json::from_str(&request.body).expect("cold function read body should parse");
        *hit_counter.lock().unwrap() += 1;
        assert!(
            body["query"]
                .as_str()
                .unwrap_or_default()
                .contains("shopifyFunctions"),
            "cold function read should forward the original shopifyFunctions query, got {body}"
        );
        Response {
            status: 200,
            headers: Default::default(),
            body: json!({
                "data": {
                    "validationFunctions": {
                        "nodes": [upstream_function.clone()]
                    }
                }
            }),
        }
    });

    let cold_read = proxy.process_request(json_graphql_request(
        r#"
        query ColdNonCatalogFunctionRead {
          validationFunctions: shopifyFunctions(first: 5, apiType: "VALIDATION") {
            nodes { id title handle apiType app { id apiKey } }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        cold_read.body["data"]["validationFunctions"]["nodes"][0]["handle"],
        json!("non-catalog-validation")
    );
    assert_eq!(*upstream_hits.lock().unwrap(), 1);

    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateFromHydratedNonCatalogFunction {
          validationCreate(validation: { functionHandle: "non-catalog-validation", title: "Hydrated non-catalog validation", enable: true }) {
            validation { id title shopifyFunction { id handle apiType app { apiKey } } }
            userErrors { field message code }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        create.body["data"]["validationCreate"]["userErrors"],
        json!([])
    );
    assert_synthetic_gid(
        create.body["data"]["validationCreate"]["validation"]["id"]
            .as_str()
            .unwrap(),
        "Validation",
    );
    assert_eq!(
        create.body["data"]["validationCreate"]["validation"]["shopifyFunction"]["handle"],
        json!("non-catalog-validation")
    );
    assert_eq!(
        create.body["data"]["validationCreate"]["validation"]["shopifyFunction"]["app"]["apiKey"],
        json!("non-catalog-app-key")
    );
    assert_eq!(
        *upstream_hits.lock().unwrap(),
        1,
        "create should reuse the hydrated function metadata without another upstream read"
    );

    let local_read = proxy.process_request(json_graphql_request(
        r#"
        query LocalReadAfterHydratedFunction {
          validationFunctions: shopifyFunctions(first: 5, apiType: "VALIDATION") {
            nodes { handle apiType app { apiKey } }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        local_read.body["data"]["validationFunctions"]["nodes"][0],
        json!({
            "handle": "non-catalog-validation",
            "apiType": "cart_checkout_validation",
            "app": { "apiKey": "non-catalog-app-key" }
        })
    );
    assert_eq!(*upstream_hits.lock().unwrap(), 1);
}

#[test]
fn functions_hydrated_raw_api_types_remain_public_while_filters_use_canonical_keys() {
    let upstream_hits = Arc::new(Mutex::new(0usize));
    let hit_counter = Arc::clone(&upstream_hits);
    let upstream_functions = vec![
        function_metadata_record(
            "gid://shopify/ShopifyFunction/raw-validation",
            "Raw Validation",
            "raw-validation",
            "cart_checkout_validation",
            "raw-validation-app-key",
            "raw-validation-app",
        ),
        function_metadata_record(
            "gid://shopify/ShopifyFunction/raw-cart",
            "Raw Cart",
            "raw-cart",
            "cart_transform",
            "raw-cart-app-key",
            "raw-cart-app",
        ),
        function_metadata_record(
            "gid://shopify/ShopifyFunction/raw-discount",
            "Raw Discount",
            "raw-discount",
            "discount",
            "raw-discount-app-key",
            "raw-discount-app",
        ),
        function_metadata_record(
            "gid://shopify/ShopifyFunction/raw-payment",
            "Raw Payment",
            "raw-payment",
            "payment_customization",
            "raw-payment-app-key",
            "raw-payment-app",
        ),
    ];
    let upstream_response_functions = upstream_functions.clone();
    let mut proxy = configured_proxy(
        ReadMode::LiveHybrid,
        Some(shopify_draft_proxy::proxy::UnsupportedMutationMode::Passthrough),
    )
    .with_upstream_transport(move |request| {
        let body: Value =
            serde_json::from_str(&request.body).expect("cold function read body should parse");
        *hit_counter.lock().unwrap() += 1;
        assert!(
            body["query"]
                .as_str()
                .unwrap_or_default()
                .contains("shopifyFunctions"),
            "cold function read should forward the original shopifyFunctions query, got {body}"
        );
        Response {
            status: 200,
            headers: Default::default(),
            body: json!({
                "data": {
                    "validationFunctions": { "nodes": [upstream_response_functions[0].clone()] },
                    "cartFunctions": { "nodes": [upstream_response_functions[1].clone()] },
                    "discountFunctions": { "nodes": [upstream_response_functions[2].clone()] },
                    "paymentFunctions": { "nodes": [upstream_response_functions[3].clone()] }
                }
            }),
        }
    });

    let query = r#"
        query RawFunctionApiTypes {
          validationFunctions: shopifyFunctions(first: 5, apiType: "VALIDATION") {
            nodes { handle apiType }
          }
          cartFunctions: shopifyFunctions(first: 5, apiType: "CART_TRANSFORM") {
            nodes { handle apiType }
          }
          discountFunctions: shopifyFunctions(first: 5, apiType: "DISCOUNT") {
            nodes { handle apiType }
          }
          paymentFunctions: shopifyFunctions(first: 5, apiType: "PAYMENT_CUSTOMIZATION") {
            nodes { handle apiType }
          }
        }
    "#;
    let cold_read = proxy.process_request(json_graphql_request(query, json!({})));
    assert_eq!(cold_read.status, 200);
    assert_eq!(
        cold_read.body["data"]["validationFunctions"]["nodes"][0]["apiType"],
        json!("cart_checkout_validation")
    );
    assert_eq!(*upstream_hits.lock().unwrap(), 1);

    let local_read = proxy.process_request(json_graphql_request(query, json!({})));
    assert_eq!(local_read.status, 200);
    assert_eq!(
        local_read.body["data"],
        json!({
            "validationFunctions": { "nodes": [{ "handle": "raw-validation", "apiType": "cart_checkout_validation" }] },
            "cartFunctions": { "nodes": [{ "handle": "raw-cart", "apiType": "cart_transform" }] },
            "discountFunctions": { "nodes": [{ "handle": "raw-discount", "apiType": "discount" }] },
            "paymentFunctions": { "nodes": [{ "handle": "raw-payment", "apiType": "payment_customization" }] }
        })
    );
    assert_eq!(
        *upstream_hits.lock().unwrap(),
        1,
        "local function read should use hydrated metadata without another upstream request"
    );
}

#[test]
fn functions_shopify_functions_without_api_type_returns_all_local_metadata_and_windows() {
    let upstream_hits = Arc::new(Mutex::new(0usize));
    let hit_counter = Arc::clone(&upstream_hits);
    let upstream_functions = vec![
        function_metadata_record(
            "gid://shopify/ShopifyFunction/unfiltered-validation",
            "Unfiltered Validation",
            "unfiltered-validation",
            "cart_checkout_validation",
            "validation-app-key",
            "validation-app",
        ),
        function_metadata_record(
            "gid://shopify/ShopifyFunction/unfiltered-cart",
            "Unfiltered Cart",
            "unfiltered-cart",
            "cart_transform",
            "cart-app-key",
            "cart-app",
        ),
        function_metadata_record(
            "gid://shopify/ShopifyFunction/unfiltered-payment",
            "Unfiltered Payment",
            "unfiltered-payment",
            "payment_customization",
            "payment-app-key",
            "payment-app",
        ),
    ];
    let mut proxy = configured_proxy(
        ReadMode::LiveHybrid,
        Some(shopify_draft_proxy::proxy::UnsupportedMutationMode::Passthrough),
    )
    .with_upstream_transport(move |request| {
        let body: Value =
            serde_json::from_str(&request.body).expect("cold function read body should parse");
        *hit_counter.lock().unwrap() += 1;
        assert!(
            body["query"]
                .as_str()
                .unwrap_or_default()
                .contains("shopifyFunctions"),
            "cold function read should forward the original shopifyFunctions query, got {body}"
        );
        Response {
            status: 200,
            headers: Default::default(),
            body: json!({
                "data": {
                    "cartFunction": upstream_functions[1].clone(),
                    "shopifyFunctions": {
                        "nodes": upstream_functions.clone(),
                        "pageInfo": {
                            "hasNextPage": false,
                            "hasPreviousPage": false,
                            "startCursor": "upstream-start",
                            "endCursor": "upstream-end"
                        }
                    },
                    "validationFunction": upstream_functions[0].clone()
                }
            }),
        }
    });

    let hydrate = proxy.process_request(json_graphql_request(
        r#"
        query HydrateUnfilteredFunctions {
          shopifyFunctions(first: 10) {
            nodes { id title handle apiType app { apiKey } }
          }
          validationFunction: shopifyFunction(id: "gid://shopify/ShopifyFunction/unfiltered-validation") {
            id
            handle
            apiType
          }
          cartFunction: shopifyFunction(id: "gid://shopify/ShopifyFunction/unfiltered-cart") {
            id
            handle
            apiType
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(hydrate.status, 200);
    assert_eq!(
        hydrate.body["data"]["shopifyFunctions"]["nodes"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
    assert_eq!(*upstream_hits.lock().unwrap(), 1);

    let first_page = proxy.process_request(json_graphql_request(
        r#"
        query LocalUnfilteredFunctionsFirstPage {
          shopifyFunctions(first: 2) {
            nodes { handle apiType }
            edges { cursor node { handle } }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
          cartFunctions: shopifyFunctions(first: 10, apiType: "CART_TRANSFORM") {
            nodes { handle apiType }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(first_page.status, 200);
    assert_eq!(
        first_page.body["data"]["shopifyFunctions"]["nodes"],
        json!([
            { "handle": "unfiltered-validation", "apiType": "cart_checkout_validation" },
            { "handle": "unfiltered-cart", "apiType": "cart_transform" }
        ])
    );
    assert_eq!(
        first_page.body["data"]["shopifyFunctions"]["edges"],
        json!([
            {
                "cursor": "cursor:gid://shopify/ShopifyFunction/unfiltered-validation",
                "node": { "handle": "unfiltered-validation" }
            },
            {
                "cursor": "cursor:gid://shopify/ShopifyFunction/unfiltered-cart",
                "node": { "handle": "unfiltered-cart" }
            }
        ])
    );
    assert_eq!(
        first_page.body["data"]["shopifyFunctions"]["pageInfo"],
        json!({
            "hasNextPage": true,
            "hasPreviousPage": false,
            "startCursor": "cursor:gid://shopify/ShopifyFunction/unfiltered-validation",
            "endCursor": "cursor:gid://shopify/ShopifyFunction/unfiltered-cart"
        })
    );
    assert_eq!(
        first_page.body["data"]["cartFunctions"]["nodes"],
        json!([{ "handle": "unfiltered-cart", "apiType": "cart_transform" }])
    );

    let second_page = proxy.process_request(json_graphql_request(
        r#"
        query LocalUnfilteredFunctionsSecondPage($after: String!) {
          shopifyFunctions(first: 2, after: $after) {
            nodes { handle apiType }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
        }
        "#,
        json!({
            "after": first_page.body["data"]["shopifyFunctions"]["pageInfo"]["endCursor"]
        }),
    ));
    assert_eq!(
        second_page.body["data"]["shopifyFunctions"]["nodes"],
        json!([{ "handle": "unfiltered-payment", "apiType": "payment_customization" }])
    );
    assert_eq!(
        second_page.body["data"]["shopifyFunctions"]["pageInfo"],
        json!({
            "hasNextPage": false,
            "hasPreviousPage": true,
            "startCursor": "cursor:gid://shopify/ShopifyFunction/unfiltered-payment",
            "endCursor": "cursor:gid://shopify/ShopifyFunction/unfiltered-payment"
        })
    );
    assert_eq!(
        *upstream_hits.lock().unwrap(),
        1,
        "local function reads after hydration should not make another upstream request"
    );
}

#[test]
fn functions_live_hybrid_reads_merge_upstream_records_after_one_local_validation_lifecycle() {
    let upstream_hits = Arc::new(Mutex::new(Vec::<Value>::new()));
    let hit_log = Arc::clone(&upstream_hits);
    let upstream_validation_function = json!({
        "id": "gid://shopify/ShopifyFunction/upstream-validation",
        "title": "Upstream Validation Function",
        "handle": "upstream-validation",
        "apiType": "cart_checkout_validation",
        "description": "Real validation function",
        "appKey": "upstream-validation-key",
        "app": {
            "__typename": "App",
            "id": "gid://shopify/App/upstream-validation-app",
            "title": "Upstream Validation App",
            "handle": "upstream-validation-app",
            "apiKey": "upstream-validation-key"
        }
    });
    let upstream_cart_function = json!({
        "id": "gid://shopify/ShopifyFunction/upstream-cart",
        "title": "Upstream Cart Function",
        "handle": "upstream-cart",
        "apiType": "cart_transform",
        "description": "Real cart transform function",
        "appKey": "upstream-cart-key",
        "app": {
            "__typename": "App",
            "id": "gid://shopify/App/upstream-cart-app",
            "title": "Upstream Cart App",
            "handle": "upstream-cart-app",
            "apiKey": "upstream-cart-key"
        }
    });
    let upstream_validation = json!({
        "id": "gid://shopify/Validation/upstream-validation",
        "title": "Upstream validation",
        "enabled": true,
        "blockOnFailure": true,
        "shopifyFunction": upstream_validation_function.clone(),
        "metafields": { "nodes": [] }
    });
    let upstream_cart_transform = json!({
        "id": "gid://shopify/CartTransform/upstream-cart-transform",
        "functionId": "gid://shopify/ShopifyFunction/upstream-cart",
        "blockOnFailure": false,
        "metafield": {
            "namespace": "bundles",
            "key": "config",
            "type": "json",
            "value": "{\"mode\":\"upstream\"}",
            "ownerType": "CARTTRANSFORM"
        },
        "metafields": {
            "nodes": [{
                "namespace": "bundles",
                "key": "config",
                "type": "json",
                "value": "{\"mode\":\"upstream\"}",
                "ownerType": "CARTTRANSFORM"
            }]
        }
    });
    let mut proxy = configured_proxy(
        ReadMode::LiveHybrid,
        Some(shopify_draft_proxy::proxy::UnsupportedMutationMode::Passthrough),
    )
    .with_upstream_transport(move |request| {
        let body: Value =
            serde_json::from_str(&request.body).expect("function overlay body should parse");
        hit_log.lock().unwrap().push(body.clone());
        let operation_name = body["operationName"].as_str().unwrap_or_default();
        let query = body["query"].as_str().unwrap_or_default();
        let response_body = match operation_name {
            "FunctionHydrateByHandle" => {
                let handle = body["variables"]["handle"].as_str().unwrap_or_default();
                let nodes = test_function_metadata_by_id_or_handle(None, Some(handle))
                    .into_iter()
                    .collect::<Vec<_>>();
                json!({ "data": { "shopifyFunctions": { "nodes": nodes } } })
            }
            "FunctionHydrateById" => {
                let id = body["variables"]["id"].as_str().unwrap_or_default();
                let function = [
                    upstream_validation_function.clone(),
                    upstream_cart_function.clone(),
                ]
                .into_iter()
                .find(|function| function["id"].as_str() == Some(id))
                .unwrap_or(Value::Null);
                json!({ "data": { "shopifyFunction": function } })
            }
            "FunctionValidationHydrateById" => {
                json!({ "data": { "validation": upstream_validation.clone() } })
            }
            _ if query.contains("validations") => json!({
                "data": { "validations": { "nodes": [upstream_validation.clone()] } }
            }),
            _ if query.contains("cartTransforms") => json!({
                "data": {
                    "cartTransforms": {
                        "nodes": [upstream_cart_transform.clone()],
                        "pageInfo": {
                            "hasNextPage": false,
                            "hasPreviousPage": false,
                            "startCursor": null,
                            "endCursor": null
                        }
                    }
                }
            }),
            _ if query.contains("shopifyFunctions") => json!({
                "data": {
                    "shopifyFunctions": {
                        "nodes": [
                            upstream_validation_function.clone(),
                            upstream_cart_function.clone()
                        ]
                    }
                }
            }),
            _ if query.contains("shopifyFunction") => json!({
                "data": { "shopifyFunction": upstream_cart_function.clone() }
            }),
            _ => json!({
                "errors": [{
                    "message": format!("unexpected function overlay upstream request: {body}")
                }]
            }),
        };
        Response {
            status: 200,
            headers: Default::default(),
            body: response_body,
        }
    });

    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation StageOnlyOneValidation {
          validationCreate(validation: { functionHandle: "validation-local", title: "Local validation", enable: true }) {
            validation { id title shopifyFunction { handle } }
            userErrors { field message code }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        create.body["data"]["validationCreate"]["userErrors"],
        json!([])
    );
    let staged_validation_id = json_string(
        &create.body["data"]["validationCreate"]["validation"]["id"],
        "staged validation id",
    );

    let read = proxy.process_request(json_graphql_request(
        r#"
        query ReadFunctionsOverlayAfterValidationStage($stagedValidationId: ID!) {
          stagedValidation: validation(id: $stagedValidationId) {
            id
            title
            shopifyFunction { handle app { apiKey } }
          }
          baseValidation: validation(id: "gid://shopify/Validation/upstream-validation") {
            id
            title
            shopifyFunction { handle app { apiKey } }
          }
          validations(first: 2, reverse: true) {
            nodes { id title shopifyFunction { handle app { apiKey } } }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
          cartTransforms(first: 5) {
            nodes {
              id
              functionId
              blockOnFailure
              metafield(namespace: "bundles", key: "config") { value ownerType }
            }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
          allFunctions: shopifyFunctions(first: 10) {
            nodes { id handle apiType app { apiKey } }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
          cartFunctions: shopifyFunctions(first: 10, apiType: "CART_TRANSFORM") {
            nodes { id handle apiType app { apiKey } }
          }
          upstreamCartFunction: shopifyFunction(id: "gid://shopify/ShopifyFunction/upstream-cart") {
            id
            handle
            apiType
            app { apiKey }
          }
        }
        "#,
        json!({ "stagedValidationId": staged_validation_id.clone() }),
    ));
    assert_eq!(read.status, 200);
    assert_eq!(
        read.body["data"]["stagedValidation"]["title"],
        json!("Local validation")
    );
    assert_eq!(
        read.body["data"]["baseValidation"]["shopifyFunction"]["app"]["apiKey"],
        json!("upstream-validation-key")
    );
    assert_eq!(
        read.body["data"]["validations"]["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|node| json_string(&node["title"], "validation title"))
            .collect::<Vec<_>>(),
        vec!["Local validation", "Upstream validation"]
    );
    assert_eq!(
        read.body["data"]["validations"]["pageInfo"]["hasPreviousPage"],
        json!(false)
    );
    assert_eq!(
        read.body["data"]["cartTransforms"]["nodes"][0]["metafield"],
        json!({ "value": "{\"mode\":\"upstream\"}", "ownerType": "CARTTRANSFORM" })
    );
    let mut function_handles = read.body["data"]["allFunctions"]["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|node| json_string(&node["handle"], "function handle"))
        .collect::<Vec<_>>();
    function_handles.sort();
    assert_eq!(
        function_handles,
        vec!["upstream-cart", "upstream-validation", "validation-local"]
    );
    assert_eq!(
        read.body["data"]["cartFunctions"]["nodes"],
        json!([{
            "id": "gid://shopify/ShopifyFunction/upstream-cart",
            "handle": "upstream-cart",
            "apiType": "cart_transform",
            "app": { "apiKey": "upstream-cart-key" }
        }])
    );
    assert_eq!(
        read.body["data"]["upstreamCartFunction"]["app"]["apiKey"],
        json!("upstream-cart-key")
    );
    assert!(
        upstream_hits
            .lock()
            .unwrap()
            .iter()
            .any(|body| body["query"]
                .as_str()
                .unwrap_or_default()
                .contains("cartTransforms")),
        "post-stage read should still hydrate unrelated cart transforms"
    );

    let dump = proxy.process_request(request_with_body("POST", "/__meta/dump", ""));
    assert_eq!(dump.status, 200);
    assert_eq!(
        dump.body["state"]["baseState"]["functionValidations"]
            ["gid://shopify/Validation/upstream-validation"]["shopifyFunction"]["app"]["apiKey"],
        json!("upstream-validation-key")
    );
    let dumped_cart_transform_metafield = &dump.body["state"]["baseState"]
        ["functionCartTransforms"]["gid://shopify/CartTransform/upstream-cart-transform"]
        ["metafield"];
    assert_eq!(
        dumped_cart_transform_metafield["value"],
        json!("{\"mode\":\"upstream\"}")
    );
    assert_eq!(
        dumped_cart_transform_metafield["ownerType"],
        json!("CARTTRANSFORM")
    );
    assert_eq!(
        dump.body["state"]["stagedState"]["functionValidations"][staged_validation_id.as_str()]
            ["title"],
        json!("Local validation")
    );
    assert_eq!(
        dump.body["state"]["stagedState"]["functionValidationsDirty"],
        json!(true)
    );

    let mut restored = snapshot_proxy();
    let restore = restored.process_request(request_with_body(
        "POST",
        "/__meta/restore",
        &dump.body.to_string(),
    ));
    assert_eq!(restore.status, 200);
    let restored_read = restored.process_request(json_graphql_request(
        r#"
        query RestoredFunctionsOverlay($stagedValidationId: ID!) {
          stagedValidation: validation(id: $stagedValidationId) {
            id
            title
          }
          baseValidation: validation(id: "gid://shopify/Validation/upstream-validation") {
            id
            title
            shopifyFunction { handle app { apiKey } }
          }
          cartTransforms(first: 5) {
            nodes {
              id
              metafield(namespace: "bundles", key: "config") { value ownerType }
            }
          }
          allFunctions: shopifyFunctions(first: 10) {
            nodes { handle apiType app { apiKey } }
          }
        }
        "#,
        json!({ "stagedValidationId": staged_validation_id.clone() }),
    ));
    assert_eq!(restored_read.status, 200);
    assert_eq!(
        restored_read.body["data"]["stagedValidation"]["title"],
        json!("Local validation")
    );
    assert_eq!(
        restored_read.body["data"]["baseValidation"]["shopifyFunction"]["app"]["apiKey"],
        json!("upstream-validation-key")
    );
    assert_eq!(
        restored_read.body["data"]["cartTransforms"]["nodes"][0]["metafield"],
        json!({ "value": "{\"mode\":\"upstream\"}", "ownerType": "CARTTRANSFORM" })
    );
    let mut restored_function_handles = restored_read.body["data"]["allFunctions"]["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|node| json_string(&node["handle"], "restored function handle"))
        .collect::<Vec<_>>();
    restored_function_handles.sort();
    assert_eq!(
        restored_function_handles,
        vec!["upstream-cart", "upstream-validation", "validation-local"]
    );

    let delete_base = restored.process_request(json_graphql_request(
        r#"
        mutation DeleteRestoredBaseValidation {
          validationDelete(id: "gid://shopify/Validation/upstream-validation") {
            deletedId
            userErrors { field message code }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(delete_base.status, 200);
    assert_eq!(
        delete_base.body["data"]["validationDelete"],
        json!({
            "deletedId": "gid://shopify/Validation/upstream-validation",
            "userErrors": []
        })
    );
    let tombstone_dump = restored.process_request(request_with_body("POST", "/__meta/dump", ""));
    assert_eq!(tombstone_dump.status, 200);
    assert!(
        tombstone_dump.body["state"]["stagedState"]["deletedFunctionValidationIds"]
            .as_array()
            .unwrap()
            .contains(&json!("gid://shopify/Validation/upstream-validation")),
        "restored base validation delete should dump an authoritative tombstone"
    );

    let mut tombstone_restored = snapshot_proxy();
    let tombstone_restore = tombstone_restored.process_request(request_with_body(
        "POST",
        "/__meta/restore",
        &tombstone_dump.body.to_string(),
    ));
    assert_eq!(tombstone_restore.status, 200);
    let tombstone_read = tombstone_restored.process_request(json_graphql_request(
        r#"
        query ReadRestoredFunctionTombstone {
          baseValidation: validation(id: "gid://shopify/Validation/upstream-validation") {
            id
          }
          validations(first: 10) {
            nodes { title }
          }
          cartTransforms(first: 5) {
            nodes { id }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(tombstone_read.status, 200);
    assert_eq!(tombstone_read.body["data"]["baseValidation"], Value::Null);
    assert_eq!(
        tombstone_read.body["data"]["validations"]["nodes"],
        json!([{ "title": "Local validation" }])
    );
    assert_eq!(
        tombstone_read.body["data"]["cartTransforms"]["nodes"][0]["id"],
        json!("gid://shopify/CartTransform/upstream-cart-transform")
    );
}

#[test]
fn functions_create_uses_authenticated_app_identity_for_ownership() {
    let mut proxy = function_metadata_proxy();

    let mut owned_request = json_graphql_request(
        r#"
        mutation CreateValidationForAuthenticatedApp {
          validationCreate(validation: { functionId: "gid://shopify/ShopifyFunction/validation-owned", title: "Owned validation" }) {
            validation { id shopifyFunction { id app { id apiKey } } }
            userErrors { field message code }
          }
        }
        "#,
        json!({}),
    );
    owned_request.headers.insert(
        "x-shopify-draft-proxy-api-client-id".to_string(),
        "validation-app".to_string(),
    );
    let owned = proxy.process_request(owned_request);
    assert_eq!(
        owned.body["data"]["validationCreate"]["userErrors"],
        json!([])
    );
    assert_synthetic_gid(
        owned.body["data"]["validationCreate"]["validation"]["id"]
            .as_str()
            .unwrap(),
        "Validation",
    );

    let mut foreign_request = json_graphql_request(
        r#"
        mutation RejectForeignCartTransformFunction {
          cartTransformCreate(functionId: "gid://shopify/ShopifyFunction/cart-owned") {
            cartTransform { id }
            userErrors { field message code }
          }
        }
        "#,
        json!({}),
    );
    foreign_request.headers.insert(
        "x-shopify-draft-proxy-api-client-id".to_string(),
        "foreign-app".to_string(),
    );
    let foreign = proxy.process_request(foreign_request);
    assert_eq!(
        foreign.body["data"]["cartTransformCreate"],
        json!({
            "cartTransform": null,
            "userErrors": [{
                "field": ["functionId"],
                "message": "Function gid://shopify/ShopifyFunction/cart-owned not found. Ensure that it is released in the current app (foreign-app), and that the app is installed.",
                "code": "FUNCTION_NOT_FOUND"
            }]
        })
    );

    let mut foreign_read = json_graphql_request(
        r#"
        query ForeignAppCannotReadCachedFunctionMetadata {
          validationFunctions: shopifyFunctions(first: 5, apiType: "VALIDATION") {
            nodes { id handle app { id apiKey } }
          }
          ownedFunction: shopifyFunction(id: "gid://shopify/ShopifyFunction/validation-owned") {
            id
            handle
          }
        }
        "#,
        json!({}),
    );
    foreign_read.headers.insert(
        "x-shopify-draft-proxy-api-client-id".to_string(),
        "foreign-app".to_string(),
    );
    let foreign_read = proxy.process_request(foreign_read);
    assert_eq!(
        foreign_read.body["data"]["validationFunctions"]["nodes"],
        json!([])
    );
    assert_eq!(foreign_read.body["data"]["ownedFunction"], Value::Null);
}

#[test]
fn functions_cart_transform_metafield_compare_digest_round_trips_through_metafields_set() {
    let mut proxy = function_metadata_proxy();

    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateCartTransformWithMetafield {
          cartTransformCreate(
            functionHandle: "cart-transform-local"
            metafields: [{ namespace: "bundles", key: "config", type: "json", value: "{\"enabled\":true}" }]
          ) {
            cartTransform {
              id
              metafield(namespace: "bundles", key: "config") { id namespace key type value compareDigest ownerType }
            }
            userErrors { field message code }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        create.body["data"]["cartTransformCreate"]["userErrors"],
        json!([])
    );
    let cart_transform_id = create.body["data"]["cartTransformCreate"]["cartTransform"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_synthetic_gid(&cart_transform_id, "CartTransform");

    let read = proxy.process_request(json_graphql_request(
        r#"
        query ReadCartTransformMetafieldDigest {
          cartTransforms(first: 5) {
            nodes {
              id
              metafield(namespace: "bundles", key: "config") { id namespace key type value compareDigest ownerType }
            }
          }
        }
        "#,
        json!({}),
    ));
    let metafield = &read.body["data"]["cartTransforms"]["nodes"][0]["metafield"];
    assert_eq!(metafield["value"], json!("{\"enabled\":true}"));
    assert_eq!(metafield["ownerType"], json!("CARTTRANSFORM"));
    let initial_digest = metafield["compareDigest"].as_str().unwrap().to_string();
    let metafield_id = metafield["id"].as_str().unwrap().to_string();
    assert!(
        metafield_id.starts_with("gid://shopify/Metafield/"),
        "{metafield_id} should be a Metafield gid"
    );
    assert_ne!(metafield_id, "gid://shopify/Metafield/43125986558258");
    assert_ne!(metafield_id, "gid://shopify/Metafield/43125986591026");

    let update = proxy.process_request(json_graphql_request(
        r#"
        mutation UpdateCartTransformMetafieldWithCompareDigest($metafields: [MetafieldsSetInput!]!) {
          metafieldsSet(metafields: $metafields) {
            metafields { id namespace key type value compareDigest ownerType }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "metafields": [{
                "ownerId": cart_transform_id,
                "namespace": "bundles",
                "key": "config",
                "type": "json",
                "value": "{\"enabled\":false}",
                "compareDigest": initial_digest
            }]
        }),
    ));
    assert_eq!(
        update.body["data"]["metafieldsSet"]["userErrors"],
        json!([])
    );
    assert_eq!(
        update.body["data"]["metafieldsSet"]["metafields"][0]["id"],
        json!(metafield_id)
    );
    assert_eq!(
        update.body["data"]["metafieldsSet"]["metafields"][0]["value"],
        json!("{\"enabled\":false}")
    );
    let updated_digest = update.body["data"]["metafieldsSet"]["metafields"][0]["compareDigest"]
        .as_str()
        .unwrap()
        .to_string();
    assert_ne!(updated_digest, initial_digest);

    let after = proxy.process_request(json_graphql_request(
        r#"
        query ReadCartTransformMetafieldAfterCAS {
          cartTransforms(first: 5) {
            nodes {
              metafield(namespace: "bundles", key: "config") { id value compareDigest ownerType }
            }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        after.body["data"]["cartTransforms"]["nodes"][0]["metafield"],
        json!({
            "id": metafield_id,
            "value": "{\"enabled\":false}",
            "compareDigest": updated_digest,
            "ownerType": "CARTTRANSFORM"
        })
    );
}

#[test]
fn functions_validation_metafields_accept_shared_registry_types_and_reject_unknowns() {
    let mut proxy = function_metadata_proxy();

    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation ValidationMetafieldSharedTypes(
          $validBoolean: ValidationCreateInput!
          $invalidBooleanValue: ValidationCreateInput!
          $unknownType: ValidationCreateInput!
        ) {
          validBoolean: validationCreate(validation: $validBoolean) {
            validation {
              id
              metafields(first: 5) {
                nodes { namespace key type value }
              }
            }
            userErrors { field message code }
          }
          invalidBooleanValue: validationCreate(validation: $invalidBooleanValue) {
            validation { id }
            userErrors { field message code }
          }
          unknownType: validationCreate(validation: $unknownType) {
            validation { id }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "validBoolean": {
                "functionHandle": "validation-local",
                "title": "Boolean validation metafield",
                "metafields": [{
                    "namespace": "custom",
                    "key": "enabled",
                    "type": "boolean",
                    "value": "true"
                }]
            },
            "invalidBooleanValue": {
                "functionHandle": "validation-local",
                "title": "Invalid boolean validation metafield",
                "metafields": [{
                    "namespace": "custom",
                    "key": "enabled",
                    "type": "boolean",
                    "value": "maybe"
                }]
            },
            "unknownType": {
                "functionHandle": "validation-local",
                "title": "Unknown validation metafield",
                "metafields": [{
                    "namespace": "custom",
                    "key": "enabled",
                    "type": "draft_proxy_unknown",
                    "value": "true"
                }]
            }
        }),
    ));
    assert_eq!(create.body["data"]["validBoolean"]["userErrors"], json!([]));
    let validation_id = json_string(
        &create.body["data"]["validBoolean"]["validation"]["id"],
        "validation id",
    );
    assert_synthetic_gid(&validation_id, "Validation");
    assert_eq!(
        create.body["data"]["validBoolean"]["validation"]["metafields"]["nodes"],
        json!([{
            "namespace": "custom",
            "key": "enabled",
            "type": "boolean",
            "value": "true"
        }])
    );

    let invalid_boolean = &create.body["data"]["invalidBooleanValue"];
    assert_eq!(invalid_boolean["validation"], Value::Null);
    assert_eq!(
        invalid_boolean["userErrors"][0]["field"],
        json!(["validation", "metafields", "0"])
    );
    assert_eq!(
        invalid_boolean["userErrors"][0]["code"],
        json!("INVALID_VALUE")
    );

    assert_eq!(
        create.body["data"]["unknownType"],
        json!({
            "validation": null,
            "userErrors": [{
                "field": ["validation", "metafields", "0"],
                "message": "The type is invalid.",
                "code": "INVALID_TYPE"
            }]
        })
    );

    let update = proxy.process_request(json_graphql_request(
        r#"
        mutation ValidationMetafieldSharedTypesUpdate($id: ID!) {
          validationUpdate(
            id: $id
            validation: {
              metafields: [{
                namespace: "custom"
                key: "enabled"
                type: "boolean"
                value: "false"
              }]
            }
          ) {
            validation {
              id
              metafields(first: 5) {
                nodes { namespace key type value }
              }
            }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "id": validation_id }),
    ));
    assert_eq!(
        update.body["data"]["validationUpdate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        update.body["data"]["validationUpdate"]["validation"]["metafields"]["nodes"],
        json!([{
            "namespace": "custom",
            "key": "enabled",
            "type": "boolean",
            "value": "false"
        }])
    );
}

#[test]
fn functions_validation_max_cap_update_defaults_and_metafield_rejection_preserve_state() {
    let mut proxy = function_metadata_proxy();

    let mut stage = String::from("mutation ValidationCapAndDefaultsStage {");
    stage.push_str(
        r#" subject: validationCreate(validation: { functionHandle: "validation-alpha", title: "Subject", enable: false, blockOnFailure: true }) { validation { id enabled blockOnFailure title } userErrors { field message code } }"#,
    );
    for index in 1..=25 {
        stage.push_str(&format!(
            r#" active{index}: validationCreate(validation: {{ functionHandle: "validation-alpha", title: "Active {index}", enable: true }}) {{ validation {{ id enabled blockOnFailure }} userErrors {{ field message code }} }}"#
        ));
    }
    stage.push_str(
        r#" maxActive: validationCreate(validation: { functionHandle: "validation-alpha", title: "Max", enable: true }) { validation { id } userErrors { field message code } } }"#,
    );

    let stage_response = proxy.process_request(json_graphql_request(&stage, json!({})));
    assert_eq!(
        stage_response.body["data"]["maxActive"],
        json!({
            "validation": null,
            "userErrors": [{ "field": [], "message": "Cannot have more than 25 active validation functions.", "code": "MAX_VALIDATIONS_ACTIVATED" }]
        })
    );
    let subject_id = stage_response.body["data"]["subject"]["validation"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_synthetic_gid(&subject_id, "Validation");

    let update_default = proxy.process_request(json_graphql_request(
        r#"mutation ValidationUpdateDefaults($id: ID!) { validationUpdate(id: $id, validation: { title: "Renamed" }) { validation { id title enabled blockOnFailure } userErrors { field message code } } }"#,
        json!({ "id": subject_id }),
    ));
    assert_eq!(
        update_default.body["data"]["validationUpdate"]["validation"],
        json!({
            "id": subject_id,
            "title": "Renamed",
            "enabled": false,
            "blockOnFailure": false
        })
    );

    let rejected_metafield = proxy.process_request(json_graphql_request(
        r#"mutation ValidationMetafieldsInvalidUpdate($id: ID!) { validationUpdate(id: $id, validation: { metafields: [{ namespace: "custom", type: "single_line_text_field", value: "loose" }] }) { validation { id } userErrors { field message code } } }"#,
        json!({ "id": subject_id }),
    ));
    assert_eq!(
        rejected_metafield.body["data"]["validationUpdate"],
        json!({
            "validation": null,
            "userErrors": [{ "field": ["validation", "metafields", "0"], "message": "presence", "code": null }]
        })
    );

    let read = proxy.process_request(json_graphql_request(
        r#"query ValidationAfterRejectedMetafield($id: ID!) { validation(id: $id) { title enabled blockOnFailure metafields(first: 5) { nodes { namespace key value } } } }"#,
        json!({ "id": subject_id }),
    ));
    assert_eq!(
        read.body["data"]["validation"],
        json!({
            "title": "Renamed",
            "enabled": false,
            "blockOnFailure": false,
            "metafields": { "nodes": [] }
        })
    );
}

#[test]
fn functions_cart_transform_create_validates_identifier_api_conflict_and_metafields() {
    let mut proxy = function_metadata_proxy();

    let unknown_id = proxy.process_request(json_graphql_request(
        r#"mutation CartTransformUnknownId { cartTransformCreate(functionId: "00000000-0000-0000-0000-000000000000") { cartTransform { id functionId } userErrors { field message code } } }"#,
        json!({}),
    ));
    assert_eq!(
        unknown_id.body["data"]["cartTransformCreate"],
        json!({
            "cartTransform": null,
            "userErrors": [{ "field": ["functionId"], "message": "Function 00000000-0000-0000-0000-000000000000 not found. Ensure that it is released in the current app (gid://shopify/App/local), and that the app is installed.", "code": "FUNCTION_NOT_FOUND" }]
        })
    );
    let read_after_unknown_id = proxy.process_request(json_graphql_request(
        r#"query CartTransformsAfterUnknownId { cartTransforms(first: 5) { nodes { id functionId } } }"#,
        json!({}),
    ));
    assert_eq!(
        read_after_unknown_id.body["data"]["cartTransforms"],
        json!({ "nodes": [] })
    );

    let unknown_handle = proxy.process_request(json_graphql_request(
        r#"mutation CartTransformUnknownHandle { cartTransformCreate(functionHandle: "missing-cart-transform") { cartTransform { id functionId } userErrors { field message code } } }"#,
        json!({}),
    ));
    assert_eq!(
        unknown_handle.body["data"]["cartTransformCreate"],
        json!({
            "cartTransform": null,
            "userErrors": [{ "field": ["functionHandle"], "message": "Could not find function with handle: missing-cart-transform.", "code": "FUNCTION_NOT_FOUND" }]
        })
    );
    let read_after_unknown_handle = proxy.process_request(json_graphql_request(
        r#"query CartTransformsAfterUnknownHandle { cartTransforms(first: 5) { nodes { id functionId } } }"#,
        json!({}),
    ));
    assert_eq!(
        read_after_unknown_handle.body["data"]["cartTransforms"],
        json!({ "nodes": [] })
    );

    let api_mismatch = proxy.process_request(json_graphql_request(
        r#"mutation CartTransformApiMismatch { cartTransformCreate(functionHandle: "conformance-validation") { cartTransform { id } userErrors { field message code } } }"#,
        json!({}),
    ));
    assert_eq!(
        api_mismatch.body["data"]["cartTransformCreate"],
        json!({
            "cartTransform": null,
            "userErrors": [{ "field": ["functionHandle"], "message": "Unexpected Function API. The provided function must implement one of the following extension targets: [purchase.cart-transform.run, cart.transform.run].", "code": "FUNCTION_DOES_NOT_IMPLEMENT" }]
        })
    );

    let invalid_metafield = proxy.process_request(json_graphql_request(
        r#"mutation CartTransformInvalidMetafield { cartTransformCreate(functionId: "019dd44b-127f-724b-a49c-70fc98ff4d72", metafields: [{ namespace: "bundles", key: "config", type: "json", value: "not-json" }]) { cartTransform { id } userErrors { field message code } } }"#,
        json!({}),
    ));
    assert_eq!(
        invalid_metafield.body["data"]["cartTransformCreate"],
        json!({
            "cartTransform": null,
            "userErrors": [{ "field": ["metafields", "0", "value"], "message": "is invalid JSON: unexpected token 'not-json' at line 1 column 1.", "code": "INVALID_METAFIELDS" }]
        })
    );

    let create = proxy.process_request(json_graphql_request(
        r#"mutation CartTransformCreateSetup { cartTransformCreate(functionId: "019dd44b-127f-724b-a49c-70fc98ff4d72", blockOnFailure: false) { cartTransform { id functionId blockOnFailure } userErrors { field message code } } }"#,
        json!({}),
    ));
    let cart_transform_id = create.body["data"]["cartTransformCreate"]["cartTransform"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_synthetic_gid(&cart_transform_id, "CartTransform");
    assert_eq!(
        create.body["data"]["cartTransformCreate"]["cartTransform"],
        json!({
            "id": cart_transform_id,
            "functionId": "019dd44b-127f-724b-a49c-70fc98ff4d72",
            "blockOnFailure": false
        })
    );

    let conflict = proxy.process_request(json_graphql_request(
        r#"mutation CartTransformCreateConflict { cartTransformCreate(functionId: "019dd44b-127f-724b-a49c-70fc98ff4d72", blockOnFailure: false) { cartTransform { id functionId } userErrors { field message code } } }"#,
        json!({}),
    ));
    assert_eq!(
        conflict.body["data"]["cartTransformCreate"],
        json!({
            "cartTransform": null,
            "userErrors": [{ "field": ["functionId"], "message": "Could not enable cart transform because it is already registered", "code": "FUNCTION_ALREADY_REGISTERED" }]
        })
    );

    let cap_conflict = proxy.process_request(json_graphql_request(
        r#"mutation CartTransformCreateCapConflict { cartTransformCreate(functionId: "gid://shopify/ShopifyFunction/cart-transform-local", blockOnFailure: true) { cartTransform { id functionId } userErrors { field message code } } }"#,
        json!({}),
    ));
    assert_eq!(
        cap_conflict.body["data"]["cartTransformCreate"],
        json!({
            "cartTransform": null,
            "userErrors": [{ "field": ["base"], "message": "The maximum number of cart transforms per shop has been reached.", "code": "MAXIMUM_CART_TRANSFORMS" }]
        })
    );
}

#[test]
fn functions_fulfillment_constraint_rules_stage_locally_and_read_after_write() {
    let hydrate_requests = Arc::new(Mutex::new(Vec::new()));
    let mut proxy = function_metadata_proxy_with_hits(Arc::clone(&hydrate_requests));

    let create_query = r#"
        mutation CreateFulfillmentConstraintRule {
          fulfillmentConstraintRuleCreate(
            functionHandle: "fulfillment-constraint-local"
            deliveryMethodTypes: [SHIPPING, LOCAL]
            metafields: [{ namespace: "custom", key: "config", type: "json", value: "{\"mode\":\"local\"}" }]
          ) {
            fulfillmentConstraintRule {
              id
              deliveryMethodTypes
              function { id handle apiType }
              metafields(first: 5) { nodes { namespace key type value ownerType } }
            }
            userErrors { code field message }
          }
        }
    "#;
    let create = proxy.process_request(json_graphql_request(create_query, json!({})));
    assert_eq!(create.status, 200);
    assert_eq!(hydrate_requests.lock().unwrap().len(), 1);
    assert_eq!(
        create.body["data"]["fulfillmentConstraintRuleCreate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        create.body["data"]["fulfillmentConstraintRuleCreate"]["fulfillmentConstraintRule"]["id"],
        json!("gid://shopify/FulfillmentConstraintRule/1")
    );
    assert_eq!(
        create.body["data"]["fulfillmentConstraintRuleCreate"]["fulfillmentConstraintRule"]
            ["deliveryMethodTypes"],
        json!(["SHIPPING", "LOCAL"])
    );
    assert_eq!(
        create.body["data"]["fulfillmentConstraintRuleCreate"]["fulfillmentConstraintRule"]
            ["function"],
        json!({
            "id": "gid://shopify/ShopifyFunction/fulfillment-constraint-local",
            "handle": "fulfillment-constraint-local",
            "apiType": "FULFILLMENT_CONSTRAINT_RULE"
        })
    );
    assert_eq!(
        create.body["data"]["fulfillmentConstraintRuleCreate"]["fulfillmentConstraintRule"]
            ["metafields"]["nodes"][0]["ownerType"],
        json!("FULFILLMENT_CONSTRAINT_RULE")
    );

    let log = log_snapshot(&proxy);
    assert_eq!(log["entries"].as_array().unwrap().len(), 1);
    assert_eq!(
        log["entries"][0]["interpreted"]["rootFields"],
        json!(["fulfillmentConstraintRuleCreate"])
    );
    assert_eq!(
        log["entries"][0]["rawBody"]
            .as_str()
            .unwrap()
            .contains("CreateFulfillmentConstraintRule"),
        true
    );

    let read = proxy.process_request(json_graphql_request(
        r#"
        query ReadFulfillmentConstraintRules {
          fulfillmentConstraintRules {
            id
            deliveryMethodTypes
            function { handle apiType }
            metafield(namespace: "custom", key: "config") { namespace key value }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        read.body["data"]["fulfillmentConstraintRules"][0],
        json!({
            "id": "gid://shopify/FulfillmentConstraintRule/1",
            "deliveryMethodTypes": ["SHIPPING", "LOCAL"],
            "function": {
                "handle": "fulfillment-constraint-local",
                "apiType": "FULFILLMENT_CONSTRAINT_RULE"
            },
            "metafield": {
                "namespace": "custom",
                "key": "config",
                "value": "{\"mode\":\"local\"}"
            }
        })
    );

    let node_read = proxy.process_request(json_graphql_request(
        r#"
        query ReadFulfillmentConstraintRuleNode($id: ID!) {
          node(id: $id) {
            ... on FulfillmentConstraintRule {
              id
              deliveryMethodTypes
              function { handle }
            }
          }
        }
        "#,
        json!({ "id": "gid://shopify/FulfillmentConstraintRule/1" }),
    ));
    assert_eq!(
        node_read.body["data"]["node"],
        json!({
            "id": "gid://shopify/FulfillmentConstraintRule/1",
            "deliveryMethodTypes": ["SHIPPING", "LOCAL"],
            "function": { "handle": "fulfillment-constraint-local" }
        })
    );

    let update = proxy.process_request(json_graphql_request(
        r#"
        mutation UpdateFulfillmentConstraintRule($id: ID!) {
          fulfillmentConstraintRuleUpdate(id: $id, deliveryMethodTypes: [PICK_UP]) {
            fulfillmentConstraintRule { id deliveryMethodTypes function { handle } }
            userErrors { code field message }
          }
        }
        "#,
        json!({ "id": "gid://shopify/FulfillmentConstraintRule/1" }),
    ));
    assert_eq!(
        update.body["data"]["fulfillmentConstraintRuleUpdate"],
        json!({
            "fulfillmentConstraintRule": {
                "id": "gid://shopify/FulfillmentConstraintRule/1",
                "deliveryMethodTypes": ["PICK_UP"],
                "function": { "handle": "fulfillment-constraint-local" }
            },
            "userErrors": []
        })
    );

    let delete = proxy.process_request(json_graphql_request(
        r#"
        mutation DeleteFulfillmentConstraintRule($id: ID!) {
          fulfillmentConstraintRuleDelete(id: $id) {
            success
            userErrors { code field message }
          }
        }
        "#,
        json!({ "id": "gid://shopify/FulfillmentConstraintRule/1" }),
    ));
    assert_eq!(
        delete.body["data"]["fulfillmentConstraintRuleDelete"],
        json!({ "success": true, "userErrors": [] })
    );

    let empty_read = proxy.process_request(json_graphql_request(
        r#"query ReadDeletedFulfillmentConstraintRules { fulfillmentConstraintRules { id } }"#,
        json!({}),
    ));
    assert_eq!(
        empty_read.body["data"]["fulfillmentConstraintRules"],
        json!([])
    );
    assert_eq!(hydrate_requests.lock().unwrap().len(), 2);
    assert_eq!(log_snapshot(&proxy)["entries"].as_array().unwrap().len(), 3);

    let upstream_hydrate_requests = Arc::new(Mutex::new(Vec::new()));
    let mut upstream_proxy = function_fulfillment_constraint_rule_proxy_with_hits(Arc::clone(
        &upstream_hydrate_requests,
    ));
    let create = upstream_proxy.process_request(json_graphql_request(
        r#"
        mutation StageFulfillmentConstraintRuleBesideUpstream {
          fulfillmentConstraintRuleCreate(
            functionHandle: "fulfillment-constraint-local"
            deliveryMethodTypes: [LOCAL]
          ) {
            fulfillmentConstraintRule { id deliveryMethodTypes function { handle } }
            userErrors { code field message }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        create.body["data"]["fulfillmentConstraintRuleCreate"]["userErrors"],
        json!([])
    );
    let staged_rule_id = json_string(
        &create.body["data"]["fulfillmentConstraintRuleCreate"]["fulfillmentConstraintRule"]["id"],
        "staged fulfillment constraint rule id",
    );
    let read = upstream_proxy.process_request(json_graphql_request(
        r#"
        query ReadFulfillmentConstraintRulesWithUpstreamBase {
          fulfillmentConstraintRules {
            id
            deliveryMethodTypes
            function { handle apiType app { apiKey } }
            metafield(namespace: "custom", key: "config") { value ownerType }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(read.status, 200);
    let rules = read.body["data"]["fulfillmentConstraintRules"]
        .as_array()
        .expect("fulfillment constraint rules should be a list");
    assert_eq!(rules.len(), 2);
    assert_eq!(
        rules
            .iter()
            .map(|rule| json_string(&rule["id"], "fulfillment constraint rule id"))
            .collect::<Vec<_>>(),
        vec![
            "gid://shopify/FulfillmentConstraintRule/upstream-rule".to_string(),
            staged_rule_id.clone()
        ]
    );
    assert_eq!(
        rules[0]["function"],
        json!({
            "handle": "upstream-fulfillment-constraint",
            "apiType": "FULFILLMENT_CONSTRAINT_RULE",
            "app": { "apiKey": "upstream-fulfillment-key" }
        })
    );
    assert_eq!(
        rules[0]["metafield"],
        json!({ "value": "{\"mode\":\"upstream\"}", "ownerType": "FULFILLMENT_CONSTRAINT_RULE" })
    );
    assert_eq!(
        rules[1]["function"],
        json!({
            "handle": "fulfillment-constraint-local",
            "apiType": "FULFILLMENT_CONSTRAINT_RULE",
            "app": { "apiKey": "fulfillment-app-key" }
        })
    );
    assert!(
        upstream_hydrate_requests
            .lock()
            .unwrap()
            .iter()
            .any(|body| body["query"]
                .as_str()
                .unwrap_or_default()
                .contains("fulfillmentConstraintRules")),
        "post-stage read should hydrate upstream fulfillment constraint rules"
    );
    let delete_base = upstream_proxy.process_request(json_graphql_request(
        r#"
        mutation DeleteBaseFulfillmentConstraintRule {
          fulfillmentConstraintRuleDelete(
            id: "gid://shopify/FulfillmentConstraintRule/upstream-rule"
          ) {
            success
            userErrors { code field message }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        delete_base.body["data"]["fulfillmentConstraintRuleDelete"],
        json!({ "success": true, "userErrors": [] })
    );
    let tombstone_read = upstream_proxy.process_request(json_graphql_request(
        r#"
        query ReadFulfillmentConstraintRulesAfterBaseDelete {
          fulfillmentConstraintRules { id deliveryMethodTypes function { handle } }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        tombstone_read.body["data"]["fulfillmentConstraintRules"],
        json!([{
            "id": staged_rule_id,
            "deliveryMethodTypes": ["LOCAL"],
            "function": { "handle": "fulfillment-constraint-local" }
        }])
    );

    let cross_root_hydrate_requests = Arc::new(Mutex::new(Vec::new()));
    let mut cross_root_proxy = function_fulfillment_constraint_rule_proxy_with_hits(Arc::clone(
        &cross_root_hydrate_requests,
    ));
    let create_validation = cross_root_proxy.process_request(json_graphql_request(
        r#"
        mutation StageValidationBeforeCombinedFunctionRead {
          validationCreate(validation: {
            functionHandle: "validation-local"
            title: "Local validation"
            enable: true
          }) {
            validation { id title }
            userErrors { code field message }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        create_validation.body["data"]["validationCreate"]["userErrors"],
        json!([])
    );
    let read = cross_root_proxy.process_request(json_graphql_request(
        r#"
        query ReadValidationOverlayAndFulfillmentConstraintRules {
          validations(first: 10) {
            nodes { id title }
          }
          fulfillmentConstraintRules {
            id
            deliveryMethodTypes
            function { handle app { apiKey } }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(read.status, 200);
    assert_eq!(
        read.body["data"]["validations"]["nodes"][0]["title"],
        json!("Local validation")
    );
    assert_eq!(
        read.body["data"]["fulfillmentConstraintRules"],
        json!([{
            "id": "gid://shopify/FulfillmentConstraintRule/upstream-rule",
            "deliveryMethodTypes": ["SHIPPING"],
            "function": {
                "handle": "upstream-fulfillment-constraint",
                "app": { "apiKey": "upstream-fulfillment-key" }
            }
        }])
    );
    assert!(
        cross_root_hydrate_requests
            .lock()
            .unwrap()
            .iter()
            .any(|body| body["query"]
                .as_str()
                .unwrap_or_default()
                .contains("fulfillmentConstraintRules")),
        "combined read opened by validation overlay should hydrate fulfillmentConstraintRules"
    );
}

#[test]
fn functions_fulfillment_constraint_rule_reads_reject_fabricated_output_fields() {
    let mut proxy = function_metadata_proxy();

    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation StageFulfillmentConstraintRuleForOutputFieldValidation {
          fulfillmentConstraintRuleCreate(
            functionHandle: "fulfillment-constraint-local"
            deliveryMethodTypes: [SHIPPING]
          ) {
            fulfillmentConstraintRule { id }
            userErrors { code field message }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        create.body["data"]["fulfillmentConstraintRuleCreate"]["userErrors"],
        json!([])
    );
    let rule_id = create.body["data"]["fulfillmentConstraintRuleCreate"]
        ["fulfillmentConstraintRule"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let invalid = proxy.process_request(json_graphql_request(
        r#"
        query InvalidFulfillmentConstraintRuleOutputFields($id: ID!) {
          fulfillmentConstraintRules {
            functionId
            functionHandle
            shopifyFunction { id }
          }
          node(id: $id) {
            ... on FulfillmentConstraintRule {
              functionId
              functionHandle
              shopifyFunction { id }
            }
          }
        }
        "#,
        json!({ "id": rule_id }),
    ));
    assert_eq!(invalid.status, 200);
    let errors = invalid.body["errors"].as_array().unwrap();
    for field_name in ["functionId", "functionHandle", "shopifyFunction"] {
        assert!(
            errors.iter().any(|error| {
                error["message"]
                    == format!(
                        "Field '{field_name}' doesn't exist on type 'FulfillmentConstraintRule'"
                    )
                    && error["extensions"]["typeName"] == "FulfillmentConstraintRule"
                    && error["extensions"]["fieldName"] == field_name
            }),
            "missing undefined-field error for FulfillmentConstraintRule.{field_name}: {errors:#?}"
        );
    }
    assert!(invalid.body.get("data").is_none());

    let valid = proxy.process_request(json_graphql_request(
        r#"
        query ValidFulfillmentConstraintRuleOutputFields($id: ID!) {
          direct: fulfillmentConstraintRules {
            id
            deliveryMethodTypes
            function { id handle apiType }
          }
          node(id: $id) {
            ... on FulfillmentConstraintRule {
              id
              deliveryMethodTypes
              function { id handle apiType }
            }
          }
        }
        "#,
        json!({ "id": rule_id }),
    ));
    assert_eq!(valid.status, 200);
    assert!(valid.body.get("errors").is_none());
    let expected_rule = json!({
        "id": "gid://shopify/FulfillmentConstraintRule/1",
        "deliveryMethodTypes": ["SHIPPING"],
        "function": {
            "id": "gid://shopify/ShopifyFunction/fulfillment-constraint-local",
            "handle": "fulfillment-constraint-local",
            "apiType": "FULFILLMENT_CONSTRAINT_RULE"
        }
    });
    assert_eq!(valid.body["data"]["direct"][0], expected_rule);
    assert_eq!(valid.body["data"]["node"], expected_rule);
}

#[test]
fn functions_fulfillment_constraint_rules_return_shopify_like_user_errors() {
    let mut proxy = function_metadata_proxy();

    let response = proxy.process_request(json_graphql_request(
        r#"
        mutation FulfillmentConstraintRuleUserErrors {
          missing: fulfillmentConstraintRuleCreate(deliveryMethodTypes: [SHIPPING]) {
            fulfillmentConstraintRule { id }
            userErrors { code field message }
          }
          multiple: fulfillmentConstraintRuleCreate(
            functionId: "gid://shopify/ShopifyFunction/fulfillment-constraint-local"
            functionHandle: "fulfillment-constraint-local"
            deliveryMethodTypes: [SHIPPING]
          ) {
            fulfillmentConstraintRule { id }
            userErrors { code field message }
          }
          emptyDelivery: fulfillmentConstraintRuleCreate(
            functionHandle: "fulfillment-constraint-local"
            deliveryMethodTypes: []
          ) {
            fulfillmentConstraintRule { id }
            userErrors { code field message }
          }
          unknownId: fulfillmentConstraintRuleCreate(
            functionId: "gid://shopify/ShopifyFunction/999999999999"
            deliveryMethodTypes: [SHIPPING]
          ) {
            fulfillmentConstraintRule { id }
            userErrors { code field message }
          }
          unknownHandle: fulfillmentConstraintRuleCreate(
            functionHandle: "definitely-missing-fulfillment-constraint"
            deliveryMethodTypes: [SHIPPING]
          ) {
            fulfillmentConstraintRule { id }
            userErrors { code field message }
          }
          wrongApi: fulfillmentConstraintRuleCreate(
            functionHandle: "conformance-validation"
            deliveryMethodTypes: [SHIPPING]
          ) {
            fulfillmentConstraintRule { id }
            userErrors { code field message }
          }
          deleteUnknown: fulfillmentConstraintRuleDelete(
            id: "gid://shopify/FulfillmentConstraintRule/999999999999"
          ) {
            success
            userErrors { code field message }
          }
        }
        "#,
        json!({}),
    ));

    assert_eq!(
        response.body["data"],
        json!({
            "missing": {
                "fulfillmentConstraintRule": null,
                "userErrors": [{
                    "code": "MISSING_FUNCTION_IDENTIFIER",
                    "field": ["functionHandle"],
                    "message": "Either function_id or function_handle must be provided."
                }]
            },
            "multiple": {
                "fulfillmentConstraintRule": null,
                "userErrors": [{
                    "code": "MULTIPLE_FUNCTION_IDENTIFIERS",
                    "field": ["functionHandle"],
                    "message": "Only one of function_id or function_handle can be provided, not both."
                }]
            },
            "emptyDelivery": {
                "fulfillmentConstraintRule": null,
                "userErrors": [{
                    "code": "INPUT_INVALID",
                    "field": ["deliveryMethodTypes"],
                    "message": "Delivery method types cannot be empty."
                }]
            },
            "unknownId": {
                "fulfillmentConstraintRule": null,
                "userErrors": [{
                    "code": "FUNCTION_NOT_FOUND",
                    "field": ["functionId"],
                    "message": "Function gid://shopify/ShopifyFunction/999999999999 not found. Ensure that it is released in the current app (gid://shopify/App/local), and that the app is installed."
                }]
            },
            "unknownHandle": {
                "fulfillmentConstraintRule": null,
                "userErrors": [{
                    "code": "FUNCTION_NOT_FOUND",
                    "field": ["functionHandle"],
                    "message": "Function definitely-missing-fulfillment-constraint not found. Ensure that it is released in the current app (gid://shopify/App/local), and that the app is installed."
                }]
            },
            "wrongApi": {
                "fulfillmentConstraintRule": null,
                "userErrors": [{
                    "code": "FUNCTION_DOES_NOT_IMPLEMENT",
                    "field": ["functionHandle"],
                    "message": "Unexpected Function API. The provided function must implement one of the following extension targets: [purchase.fulfillment-constraint-rule.run, cart.fulfillment-constraints.generate.run]."
                }]
            },
            "deleteUnknown": {
                "success": false,
                "userErrors": [{
                    "code": "NOT_FOUND",
                    "field": ["id"],
                    "message": "Could not find FulfillmentConstraintRule with id: gid://shopify/FulfillmentConstraintRule/999999999999"
                }]
            }
        })
    );

    let read = proxy.process_request(json_graphql_request(
        r#"query FulfillmentConstraintRuleErrorsDoNotStage { fulfillmentConstraintRules { id } }"#,
        json!({}),
    ));
    assert_eq!(read.body["data"]["fulfillmentConstraintRules"], json!([]));
}

#[test]
fn functions_fulfillment_constraint_rule_update_rejects_removed_function_arguments() {
    let mut proxy = function_metadata_proxy();

    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation StageFulfillmentConstraintRuleForUpdateErrors {
          fulfillmentConstraintRuleCreate(
            functionHandle: "fulfillment-constraint-local"
            deliveryMethodTypes: [SHIPPING]
          ) {
            fulfillmentConstraintRule { id deliveryMethodTypes function { handle } }
            userErrors { code field message }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        create.body["data"]["fulfillmentConstraintRuleCreate"]["userErrors"],
        json!([])
    );
    let rule_id = create.body["data"]["fulfillmentConstraintRuleCreate"]
        ["fulfillmentConstraintRule"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let update = proxy.process_request(json_graphql_request(
        r#"
        mutation FulfillmentConstraintRuleUpdateUnknownFunction($id: ID!) {
          unknownId: fulfillmentConstraintRuleUpdate(
            id: $id
            functionId: "gid://shopify/ShopifyFunction/999999999999"
            deliveryMethodTypes: [SHIPPING]
          ) {
            fulfillmentConstraintRule { id }
            userErrors { code field message }
          }
          unknownHandle: fulfillmentConstraintRuleUpdate(
            id: $id
            functionHandle: "definitely-missing-fulfillment-constraint"
            deliveryMethodTypes: [SHIPPING]
          ) {
            fulfillmentConstraintRule { id }
            userErrors { code field message }
          }
        }
        "#,
        json!({ "id": rule_id }),
    ));

    assert!(update.body.get("data").is_none());
    let errors = update.body["errors"].as_array().unwrap();
    assert_eq!(errors.len(), 2);
    for argument in ["functionId", "functionHandle"] {
        assert!(
            errors.iter().any(|error| {
                error["message"]
                    == format!(
                        "Field 'fulfillmentConstraintRuleUpdate' doesn't accept argument '{argument}'"
                    )
                    && error["extensions"]["code"] == "argumentNotAccepted"
                    && error["extensions"]["argumentName"] == argument
            }),
            "missing schema error for removed argument {argument}: {errors:#?}"
        );
    }
    assert_eq!(log_snapshot(&proxy)["entries"].as_array().unwrap().len(), 1);

    let read = proxy.process_request(json_graphql_request(
        r#"query FulfillmentConstraintRuleAfterUnknownFunctionUpdate { fulfillmentConstraintRules { id deliveryMethodTypes function { handle } } }"#,
        json!({}),
    ));
    assert_eq!(
        read.body["data"]["fulfillmentConstraintRules"],
        json!([{
            "id": rule_id,
            "deliveryMethodTypes": ["SHIPPING"],
            "function": { "handle": "fulfillment-constraint-local" }
        }])
    );
}

#[test]
fn localization_locale_and_translation_lifecycle_stages_reads_and_clears_locale_translations() {
    let mut proxy = snapshot_proxy();
    let resource_id = create_fallback_localization_product(&mut proxy);
    let title_digest = fallback_product_title_digest();

    let initial = proxy.process_request(json_graphql_request(
        r#"query LocalizationLocaleTranslationRead($first: Int!, $resourceType: TranslatableResourceType!, $ids: [ID!]!) {
          availableLocalesExcerpt: availableLocales { isoCode name }
          allShopLocales: shopLocales { locale name primary published }
          publishedShopLocales: shopLocales(published: true) { locale name primary published }
          resources: translatableResources(first: $first, resourceType: $resourceType) { nodes { resourceId translatableContent { key value digest locale type } translations(locale: "fr") { key value locale outdated market { id } } } pageInfo { hasNextPage hasPreviousPage } }
          byIds: translatableResourcesByIds(first: $first, resourceIds: $ids) { nodes { resourceId } edges { cursor node { resourceId } } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } }
          missing: translatableResource(resourceId: "gid://shopify/Product/999999999999999") { resourceId }
        }"#,
        json!({ "first": 3, "resourceType": "PRODUCT", "ids": ["gid://shopify/Product/999999999999999"] }),
    ));
    assert_eq!(
        initial.body["data"]["allShopLocales"][0]["locale"],
        json!("en")
    );
    assert!(initial.body["data"]["availableLocalesExcerpt"]
        .as_array()
        .unwrap()
        .iter()
        .any(|locale| locale["isoCode"] == json!("fr") && locale["name"] == json!("French")));
    assert_eq!(initial.body["data"]["missing"], Value::Null);

    let enable = proxy.process_request(json_graphql_request(
        r#"mutation LocalizationShopLocaleEnable($locale: String!) { shopLocaleEnable(locale: $locale) { shopLocale { locale name primary published } userErrors { field message } } }"#,
        json!({ "locale": "fr" }),
    ));
    assert_eq!(
        enable.body["data"]["shopLocaleEnable"]["shopLocale"]["locale"],
        json!("fr")
    );
    assert_eq!(
        enable.body["data"]["shopLocaleEnable"]["userErrors"],
        json!([])
    );

    let registered = proxy.process_request(json_graphql_request(
        r#"mutation LocalizationTranslationsRegister($resourceId: ID!, $translations: [TranslationInput!]!) { translationsRegister(resourceId: $resourceId, translations: $translations) { translations { key value locale outdated market { id } } userErrors { field message code } } }"#,
        json!({ "resourceId": resource_id.clone(), "translations": [{ "locale": "fr", "key": "title", "value": "Titre local", "translatableContentDigest": title_digest }] }),
    ));
    assert_eq!(
        registered.body["data"]["translationsRegister"]["translations"][0]["value"],
        json!("Titre local")
    );

    let downstream = proxy.process_request(json_graphql_request(
        r#"query LocalizationTranslationsRead($resourceId: ID!) { translatableResource(resourceId: $resourceId) { resourceId translations(locale: "fr") { key value locale outdated market { id } } } }"#,
        json!({ "resourceId": resource_id.clone() }),
    ));
    assert_eq!(
        downstream.body["data"]["translatableResource"]["translations"][0]["value"],
        json!("Titre local")
    );

    let disable = proxy.process_request(json_graphql_request(
        r#"mutation LocalizationShopLocaleDisable($locale: String!) { shopLocaleDisable(locale: $locale) { locale userErrors { field message } } }"#,
        json!({ "locale": "fr" }),
    ));
    assert_eq!(
        disable.body["data"]["shopLocaleDisable"],
        json!({ "locale": "fr", "userErrors": [] })
    );

    let after_disable = proxy.process_request(json_graphql_request(
        r#"query LocalizationTranslationsRead($resourceId: ID!) { translatableResource(resourceId: $resourceId) { resourceId translations(locale: "fr") { key value locale outdated market { id } } } }"#,
        json!({ "resourceId": resource_id }),
    ));
    assert_eq!(
        after_disable.body["data"]["translatableResource"]["translations"],
        json!([])
    );
}

#[test]
fn cold_snapshot_localization_baseline_does_not_seed_web_presence_or_product() {
    let mut proxy = snapshot_proxy();

    let localization = proxy.process_request(json_graphql_request(
        r#"
        query ColdLocalizationBaseline($ids: [ID!]!) {
          shopLocales { locale marketWebPresences { id subfolderSuffix } }
          resources: translatableResources(first: 5, resourceType: PRODUCT) {
            nodes { resourceId }
          }
          byIds: translatableResourcesByIds(first: 5, resourceIds: $ids) {
            nodes { resourceId }
          }
        }
        "#,
        json!({ "ids": ["gid://shopify/Product/9801098789170"] }),
    ));
    assert_eq!(localization.status, 200);
    assert_eq!(
        localization.body["data"]["shopLocales"][0]["marketWebPresences"],
        json!([])
    );
    assert_eq!(localization.body["data"]["resources"]["nodes"], json!([]));
    assert_eq!(localization.body["data"]["byIds"]["nodes"], json!([]));

    let web_presences = proxy.process_request(json_graphql_request(
        r#"
        query ColdWebPresenceBaseline {
          webPresences(first: 5) { nodes { id subfolderSuffix } }
        }
        "#,
        json!({}),
    ));
    assert_eq!(web_presences.status, 200);
    assert_eq!(
        web_presences.body["data"]["webPresences"]["nodes"],
        json!([])
    );

    let dump = proxy.process_request(request_with_body("POST", "/__meta/dump", ""));
    assert_eq!(dump.status, 200);
    assert_eq!(
        dump.body["state"]["baseState"]["localizationProductIds"],
        json!([])
    );
    assert_eq!(
        dump.body["state"]["baseState"]["shopLocales"]["en"]["marketWebPresences"],
        json!([])
    );
    let dump_body = dump.body.to_string();
    assert!(!dump_body.contains("gid://shopify/Product/9801098789170"));
    assert!(!dump_body.contains("gid://shopify/MarketWebPresence/62842765618"));
}

#[test]
fn localization_catalog_reads_are_store_backed_without_ported_document_marker() {
    let mut proxy = snapshot_proxy();

    let baseline = proxy.process_request(json_graphql_request(
        r#"query ArbitraryLocaleCatalogRead {
          locales: availableLocales { isoCode name }
          all: shopLocales { locale name primary published marketWebPresences { id subfolderSuffix } }
          published: shopLocales(published: true) { locale published }
        }"#,
        json!({}),
    ));
    assert_eq!(baseline.status, 200);
    assert_eq!(
        baseline.body["data"]["all"],
        json!([{
            "locale": "en",
            "name": "English",
            "primary": true,
            "published": true,
            "marketWebPresences": []
        }])
    );
    assert!(baseline.body["data"]["locales"]
        .as_array()
        .unwrap()
        .iter()
        .any(|locale| locale["isoCode"] == json!("tr") && locale["name"] == json!("Turkish")));
    assert_eq!(
        baseline.body["data"]["published"],
        json!([{ "locale": "en", "published": true }])
    );

    let known_presence = stage_web_presence(&mut proxy, "fr");
    let lifecycle = proxy.process_request(json_graphql_request(
        r#"mutation LocalizationShopLocaleEnable($known: ID!) {
          enable: shopLocaleEnable(locale: "fr") { shopLocale { locale published } userErrors { field message } }
          update: shopLocaleUpdate(locale: "fr", shopLocale: { published: true, marketWebPresenceIds: [$known] }) { shopLocale { locale name published marketWebPresences { id __typename defaultLocale { locale } } } userErrors { field message } }
        }"#,
        json!({ "known": known_presence }),
    ));
    assert_eq!(lifecycle.status, 200);
    assert_eq!(
        lifecycle.body["data"]["update"]["shopLocale"],
        json!({
            "locale": "fr",
                "name": "French",
                "published": true,
                "marketWebPresences": [{
                    "id": known_presence,
                    "__typename": "MarketWebPresence",
                    "defaultLocale": { "locale": "en" }
                }]
        })
    );

    let after_update = proxy.process_request(json_graphql_request(
        r#"query AnyNameCanReadStagedLocales {
          all: shopLocales { locale name published marketWebPresences { id __typename defaultLocale { locale } } }
          published: shopLocales(published: true) { locale published }
        }"#,
        json!({}),
    ));
    let all = after_update.body["data"]["all"].as_array().unwrap();
    assert!(all
        .iter()
        .any(|locale| locale["locale"] == json!("fr") && locale["published"] == json!(true)));
    assert!(after_update.body["data"]["published"]
        .as_array()
        .unwrap()
        .iter()
        .any(|locale| locale["locale"] == json!("fr")));

    let disabled = proxy.process_request(json_graphql_request(
        r#"mutation LocalizationShopLocaleDisable($locale: String!) { shopLocaleDisable(locale: $locale) { locale userErrors { field message } } }"#,
        json!({ "locale": "fr" }),
    ));
    assert_eq!(
        disabled.body["data"]["shopLocaleDisable"],
        json!({ "locale": "fr", "userErrors": [] })
    );

    let after_disable = proxy.process_request(json_graphql_request(
        r#"query NoMarkerShopLocaleAfterDisable { shopLocales { locale published } }"#,
        json!({}),
    ));
    assert_eq!(
        after_disable.body["data"]["shopLocales"],
        json!([{ "locale": "en", "published": true }])
    );
}

#[test]
fn localization_markets_read_returns_empty_connection_without_source_data() {
    let mut proxy = snapshot_proxy();

    let localization_read = proxy.process_request(json_graphql_request(
        r#"query LocalizationMarketsNoData {
          markets(first: 5) {
            nodes { id name handle status type }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
        }"#,
        json!({}),
    ));
    assert_eq!(localization_read.status, 200);
    assert_eq!(
        localization_read.body["data"]["markets"],
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
    let serialized = serde_json::to_string(&localization_read.body).unwrap();
    assert!(!serialized.contains("gid://shopify/Market/123"));
    assert!(!serialized.contains("gid://shopify/Market/ca"));

    let market_localization_read = proxy.process_request(json_graphql_request(
        r#"query RustMarketLocalizationsLocalRuntimeSourceEmpty {
          markets(first: 5) { nodes { id name handle status type } }
          marketLocalizableResource(resourceId: "gid://shopify/Metafield/localizable") { resourceId }
        }"#,
        json!({}),
    ));
    assert_eq!(market_localization_read.status, 200);
    assert_eq!(
        market_localization_read.body["data"]["markets"]["nodes"],
        json!([])
    );
    assert_eq!(
        market_localization_read.body["data"]["marketLocalizableResource"],
        Value::Null
    );
    let serialized = serde_json::to_string(&market_localization_read.body).unwrap();
    assert!(!serialized.contains("gid://shopify/Market/123"));
    assert!(!serialized.contains("gid://shopify/Market/ca"));
}

#[test]
fn localization_translatable_resources_do_not_fabricate_empty_connections() {
    let mut proxy = snapshot_proxy();

    let read = proxy.process_request(json_graphql_request(
        r#"query LocalizationTranslatableNoCollectionData {
          translatableResources(first: 5, resourceType: COLLECTION) {
            nodes { resourceId }
            edges { cursor node { resourceId } }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
        }"#,
        json!({}),
    ));

    assert_eq!(read.status, 200);
    assert_eq!(
        read.body["data"]["translatableResources"],
        json!({
            "nodes": [],
            "edges": [],
            "pageInfo": {
                "hasNextPage": false,
                "hasPreviousPage": false,
                "startCursor": null,
                "endCursor": null
            }
        })
    );
    let serialized = serde_json::to_string(&read.body).unwrap();
    assert!(!serialized.contains("gid://shopify/Collection/9801098789170"));
}

#[test]
fn localization_translatable_resources_honor_reverse_and_cursor_windowing() {
    let mut proxy = snapshot_proxy();

    let first_product_id = create_fallback_localization_product(&mut proxy);
    let second_product = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateSecondLocalizationProduct($product: ProductCreateInput!) {
          productCreate(product: $product) {
            product { id }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "product": {
                "title": "Second Localization Product",
                "handle": "second-localization-product",
                "descriptionHtml": "<p>Second localization body</p>",
                "productType": "snowboard"
            }
        }),
    ));
    assert_eq!(
        second_product.body["data"]["productCreate"]["userErrors"],
        json!([])
    );
    let second_product_id = json_string(
        &second_product.body["data"]["productCreate"]["product"]["id"],
        "second localization product id",
    );

    let read = proxy.process_request(json_graphql_request(
        r#"
        query TranslatableResourcesReverseWindow($after: String!) {
          forward: translatableResources(first: 1, resourceType: PRODUCT) {
            edges { cursor node { resourceId } }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
          reversed: translatableResources(first: 1, resourceType: PRODUCT, reverse: true) {
            edges { cursor node { resourceId } }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
          reversedAfter: translatableResources(first: 1, resourceType: PRODUCT, reverse: true, after: $after) {
            nodes { resourceId }
            edges { cursor node { resourceId } }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
        }
        "#,
        json!({ "after": second_product_id }),
    ));

    assert_eq!(read.status, 200);
    assert_eq!(
        read.body["data"]["forward"],
        json!({
            "edges": [{
                "cursor": first_product_id,
                "node": { "resourceId": first_product_id }
            }],
            "pageInfo": {
                "hasNextPage": true,
                "hasPreviousPage": false,
                "startCursor": first_product_id,
                "endCursor": first_product_id
            }
        })
    );
    assert_eq!(
        read.body["data"]["reversed"],
        json!({
            "edges": [{
                "cursor": second_product_id,
                "node": { "resourceId": second_product_id }
            }],
            "pageInfo": {
                "hasNextPage": true,
                "hasPreviousPage": false,
                "startCursor": second_product_id,
                "endCursor": second_product_id
            }
        })
    );
    assert_eq!(
        read.body["data"]["reversedAfter"],
        json!({
            "nodes": [{ "resourceId": first_product_id }],
            "edges": [{
                "cursor": first_product_id,
                "node": { "resourceId": first_product_id }
            }],
            "pageInfo": {
                "hasNextPage": false,
                "hasPreviousPage": true,
                "startCursor": first_product_id,
                "endCursor": first_product_id
            }
        })
    );
}

#[test]
fn localization_markets_read_hydrates_from_live_source_and_reuses_observed_market() {
    let upstream_requests = Arc::new(Mutex::new(Vec::new()));
    let captured_requests = Arc::clone(&upstream_requests);
    let mut proxy =
        configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport(move |request| {
            let body =
                serde_json::from_str::<Value>(&request.body).expect("upstream GraphQL body parses");
            // A cold LiveHybrid markets read forwards the client's query verbatim
            // upstream and hydrates the staged markets store from the response as a
            // side effect — it does not synthesize a separate hydration operation.
            // This matches the recorded conformance cassettes (e.g. markets-catalog
            // records a verbatim MarketsCatalogRead upstream call; none synthesize a
            // LocalizationMarketsHydrate operation), so the forwarded document is the
            // original markets read rather than a fabricated one.
            assert!(
                body["query"]
                    .as_str()
                    .is_some_and(|query| query.contains("markets(first")),
                "expected the markets read forwarded verbatim upstream, got {body}"
            );
            captured_requests.lock().unwrap().push(request.clone());
            shopify_draft_proxy::proxy::Response {
                status: 200,
                headers: Default::default(),
                body: json!({
                    "data": {
                        "markets": {
                            "nodes": [{
                                "id": "gid://shopify/Market/97997685042",
                                "name": "Source Market",
                                "handle": "source-market",
                                "status": "DRAFT",
                                "type": "NONE"
                            }]
                        }
                    }
                }),
            }
        });

    let mut request = json_graphql_request(
        r#"query LocalizationMarketsFromSource($first: Int!) {
          markets(first: $first) { nodes { id name handle status type } }
        }"#,
        json!({ "first": 1 }),
    );
    request.path = "/admin/api/2026-04/graphql.json".to_string();
    request.headers.insert(
        "x-shopify-access-token".to_string(),
        "source-token".to_string(),
    );
    let hydrated = proxy.process_request(request.clone());

    assert_eq!(hydrated.status, 200);
    assert_eq!(
        hydrated.body["data"]["markets"]["nodes"],
        json!([{
            "id": "gid://shopify/Market/97997685042",
            "name": "Source Market",
            "handle": "source-market",
            "status": "DRAFT",
            "type": "NONE"
        }])
    );
    {
        let requests = upstream_requests.lock().unwrap();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].path, "/admin/api/2026-04/graphql.json");
        assert_eq!(
            requests[0]
                .headers
                .get("x-shopify-access-token")
                .map(String::as_str),
            Some("source-token")
        );
    }

    let cached = proxy.process_request(request);
    assert_eq!(
        cached.body["data"]["markets"]["nodes"][0]["id"],
        json!("gid://shopify/Market/97997685042")
    );
    assert_eq!(upstream_requests.lock().unwrap().len(), 1);
}

#[test]
fn mixed_localization_read_hydrates_only_cold_markets_for_staged_resources() {
    let upstream_requests = Arc::new(Mutex::new(Vec::<Value>::new()));
    let captured_requests = Arc::clone(&upstream_requests);
    let mut proxy =
        configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport(move |request| {
            let body: Value =
                serde_json::from_str(&request.body).expect("market hydrate body parses");
            captured_requests.lock().unwrap().push(body.clone());
            assert_eq!(body["operationName"], json!("LocalizationMarketsHydrate"));
            assert!(body["query"]
                .as_str()
                .is_some_and(|query| query.contains("query LocalizationMarketsHydrate")));
            assert!(!body["query"]
                .as_str()
                .is_some_and(|query| query.contains("translatableResource")));
            Response {
                status: 200,
                headers: Default::default(),
                body: json!({
                    "data": {
                        "markets": {
                            "nodes": [{
                                "id": "gid://shopify/Market/observed",
                                "name": "Observed market",
                                "handle": "observed-market",
                                "status": "ACTIVE",
                                "type": "REGION"
                            }]
                        }
                    }
                }),
            }
        });

    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation ProductForMixedLocalizationRead($product: ProductCreateInput!) {
          productCreate(product: $product) {
            product { id }
            userErrors { field message }
          }
        }
        "#,
        json!({ "product": { "title": "Local translatable product" } }),
    ));
    let product_id = create.body["data"]["productCreate"]["product"]["id"].clone();

    let response = proxy.process_request(json_graphql_request(
        r#"
        query MixedLocalTranslationAndColdMarkets($resourceId: ID!) {
          translatableResource(resourceId: $resourceId) {
            resourceId
            translatableContent { key value locale type digest }
            translations(locale: "es") { key value locale outdated }
          }
          markets(first: 10) { nodes { id name handle status type } }
        }
        "#,
        json!({ "resourceId": product_id }),
    ));

    assert_eq!(response.status, 200);
    assert_eq!(response.body.get("errors"), None);
    assert_eq!(
        response.body["data"]["translatableResource"]["resourceId"],
        product_id
    );
    assert_eq!(
        response.body["data"]["markets"]["nodes"][0]["name"],
        json!("Observed market")
    );
    assert_eq!(upstream_requests.lock().unwrap().len(), 1);
}

#[test]
fn mixed_localization_read_preserves_staged_resource_when_context_hydration_fails() {
    let upstream_requests = Arc::new(Mutex::new(Vec::<Value>::new()));
    let captured_requests = Arc::clone(&upstream_requests);
    let mut proxy =
        configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport(move |request| {
            let body: Value =
                serde_json::from_str(&request.body).expect("mixed localization body parses");
            captured_requests.lock().unwrap().push(body);
            Response {
                status: 503,
                headers: Default::default(),
                body: json!({ "errors": [{ "message": "context hydration unavailable" }] }),
            }
        });

    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation ProductForFailedMixedLocalizationRead($product: ProductCreateInput!) {
          productCreate(product: $product) {
            product { id }
            userErrors { field message }
          }
        }
        "#,
        json!({ "product": { "title": "Local translatable product" } }),
    ));
    let product_id = create.body["data"]["productCreate"]["product"]["id"].clone();

    let response = proxy.process_request(json_graphql_request(
        r#"
        query MixedLocalTranslationAndFailedContext($resourceId: ID!) {
          translatableResource(resourceId: $resourceId) {
            resourceId
            translations(locale: "es") { key value locale outdated }
          }
          markets(first: 10) { nodes { id name handle status type } }
          allShopLocales: shopLocales { locale name primary published }
        }
        "#,
        json!({ "resourceId": product_id }),
    ));

    assert_eq!(response.status, 200);
    assert_eq!(response.body.get("errors"), None);
    assert_eq!(
        response.body["data"]["translatableResource"],
        json!({ "resourceId": product_id, "translations": [] })
    );
    assert_eq!(response.body["data"]["markets"]["nodes"], json!([]));
    assert_eq!(upstream_requests.lock().unwrap().len(), 1);
}

#[test]
fn localization_source_read_stages_observed_markets_and_shop_locales_for_translation_replay() {
    let upstream_requests = Arc::new(Mutex::new(Vec::new()));
    let captured_requests = Arc::clone(&upstream_requests);
    let title_digest = fallback_product_title_digest();
    let mut proxy = configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport(
        move |request| {
            let body = serde_json::from_str::<Value>(&request.body)
                .expect("upstream GraphQL body parses");
            // The source read forwards the client's `markets`/`shopLocales` document
            // verbatim once and hydrates both staged stores from the single response;
            // it does not synthesize a separate LocalizationMarketsHydrate operation
            // (no conformance cassette records one), so there is no second fallback
            // call to recover from.
            assert_ne!(body["operationName"], json!("LocalizationMarketsHydrate"));
            captured_requests.lock().unwrap().push(request.clone());
            shopify_draft_proxy::proxy::Response {
                status: 200,
                headers: Default::default(),
                body: json!({
                    "data": {
                        "markets": {
                            "nodes": [{
                                "id": "gid://shopify/Market/97997685042",
                                "name": "Captured Market",
                                "handle": "captured-market",
                                "status": "ACTIVE",
                                "type": "REGION"
                            }]
                        },
                        "allShopLocales": [
                            { "locale": "en", "name": "English", "primary": true, "published": true, "marketWebPresences": [] },
                            { "locale": "es", "name": "Spanish", "primary": false, "published": false, "marketWebPresences": [] }
                        ]
                    }
                }),
            }
        },
    );
    let resource_id = create_fallback_localization_product(&mut proxy);

    let source_read = proxy.process_request(json_graphql_request(
        r#"query LocalizationTranslationsMarketScopedRead($resourceId: ID!, $marketsFirst: Int!) {
          translatableResource(resourceId: $resourceId) {
            resourceId
            translations(locale: "es") { key value locale }
          }
          markets(first: $marketsFirst) { nodes { id name handle status type } }
          allShopLocales: shopLocales { locale name primary published marketWebPresences { id } }
        }"#,
        json!({ "resourceId": resource_id.as_str(), "marketsFirst": 1 }),
    ));
    assert_eq!(source_read.status, 200);
    assert_eq!(
        source_read.body["data"]["translatableResource"],
        json!({ "resourceId": resource_id, "translations": [] })
    );
    assert_eq!(
        source_read.body["data"]["markets"]["nodes"][0]["id"],
        json!("gid://shopify/Market/97997685042")
    );
    // One verbatim upstream forward serves the whole multi-root source read and
    // hydrates both the markets and shop-locale stores for the translation replay
    // below.
    assert_eq!(upstream_requests.lock().unwrap().len(), 1);

    let registered = proxy.process_request(json_graphql_request(
        r#"mutation LocalizationTranslationsRegister($resourceId: ID!, $translations: [TranslationInput!]!) {
          translationsRegister(resourceId: $resourceId, translations: $translations) {
            translations { key value locale market { id } }
            userErrors { field message code }
          }
        }"#,
        json!({
            "resourceId": resource_id.as_str(),
            "translations": [{
                "locale": "es",
                "key": "title",
                "value": "Titulo de mercado",
                "marketId": "gid://shopify/Market/97997685042",
                "translatableContentDigest": title_digest
            }]
        }),
    ));
    assert_eq!(registered.status, 200);
    assert_eq!(
        registered.body["data"]["translationsRegister"]["translations"],
        json!([{
            "key": "title",
            "value": "Titulo de mercado",
            "locale": "es",
            "market": { "id": "gid://shopify/Market/97997685042" }
        }])
    );
    assert_eq!(
        registered.body["data"]["translationsRegister"]["userErrors"],
        json!([])
    );
}

#[test]
fn localization_markets_read_merges_locally_staged_markets_with_upstream() {
    let upstream_hits = Arc::new(Mutex::new(0usize));
    let captured_hits = Arc::clone(&upstream_hits);
    let mut proxy =
        configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport(move |_request| {
            *captured_hits.lock().unwrap() += 1;
            shopify_draft_proxy::proxy::Response {
                status: 200,
                headers: Default::default(),
                body: json!({
                    "data": {
                        "markets": {
                            "nodes": [{
                                "__typename": "Market",
                                "id": "gid://shopify/Market/live-ca",
                                "name": "Live Canada",
                                "handle": "live-canada",
                                "status": "ACTIVE",
                                "type": "REGION"
                            }],
                            "edges": [],
                            "pageInfo": {
                                "hasNextPage": false,
                                "hasPreviousPage": false,
                                "startCursor": "gid://shopify/Market/live-ca",
                                "endCursor": "gid://shopify/Market/live-ca"
                            }
                        }
                    }
                }),
            }
        });

    let created = proxy.process_request(json_graphql_request(
        r#"mutation RustMarketCreateLocalRuntimeSourceBacked($input: MarketCreateInput!) {
          marketCreate(input: $input) {
            market { id name handle status }
            userErrors { field message code }
          }
        }"#,
        json!({ "input": { "name": "Canada", "regions": [{ "countryCode": "CA" }] } }),
    ));
    assert_eq!(created.status, 200);
    assert_eq!(
        created.body["data"]["marketCreate"]["userErrors"],
        json!([])
    );
    let market = created.body["data"]["marketCreate"]["market"].clone();

    let read = proxy.process_request(json_graphql_request(
        r#"query LocalizationMarketsStagedRead {
          markets(first: 5) { nodes { id name handle status } }
        }"#,
        json!({}),
    ));
    assert_eq!(read.status, 200);
    assert_eq!(
        read.body["data"]["markets"]["nodes"],
        json!([
            market,
            {
                "id": "gid://shopify/Market/live-ca",
                "name": "Live Canada",
                "handle": "live-canada",
                "status": "ACTIVE"
            }
        ])
    );
    assert_eq!(*upstream_hits.lock().unwrap(), 1);
}

#[test]
fn localization_translations_register_multi_row_round_trip_and_indexed_errors() {
    let mut proxy = snapshot_proxy();
    let resource_id = create_fallback_localization_product(&mut proxy);
    let title_digest = fallback_product_title_digest();
    let body_digest = fallback_product_body_digest();
    let meta_title_digest = localization_content_digest("");

    for locale in ["fr", "es"] {
        let enable = proxy.process_request(json_graphql_request(
            r#"mutation LocalizationShopLocaleEnable($locale: String!) {
              shopLocaleEnable(locale: $locale) { userErrors { field message } }
            }"#,
            json!({ "locale": locale }),
        ));
        assert_eq!(
            enable.body["data"]["shopLocaleEnable"]["userErrors"],
            json!([])
        );
    }

    let registered = proxy.process_request(json_graphql_request(
        r#"mutation LocalizationTranslationsRegister($resourceId: ID!, $translations: [TranslationInput!]!) { translationsRegister(resourceId: $resourceId, translations: $translations) { translations { key value locale outdated updatedAt market { id } } userErrors { field message code } } }"#,
        json!({
            "resourceId": resource_id.as_str(),
            "translations": [
                { "locale": "fr", "key": "title", "value": "Titre local", "translatableContentDigest": title_digest },
                { "locale": "fr", "key": "body_html", "value": "Description locale", "translatableContentDigest": body_digest }
            ]
        }),
    ));
    assert_eq!(
        registered.body["data"]["translationsRegister"]["translations"],
        json!([
            { "key": "title", "value": "Titre local", "locale": "fr", "outdated": false, "updatedAt": registered.body["data"]["translationsRegister"]["translations"][0]["updatedAt"], "market": null },
            { "key": "body_html", "value": "Description locale", "locale": "fr", "outdated": false, "updatedAt": registered.body["data"]["translationsRegister"]["translations"][1]["updatedAt"], "market": null }
        ])
    );
    assert_datetime_string(
        &registered.body["data"]["translationsRegister"]["translations"][0]["updatedAt"],
        "registered title translation updatedAt",
    );
    assert_datetime_string(
        &registered.body["data"]["translationsRegister"]["translations"][1]["updatedAt"],
        "registered body translation updatedAt",
    );
    assert_eq!(
        registered.body["data"]["translationsRegister"]["userErrors"],
        json!([])
    );

    let downstream = proxy.process_request(json_graphql_request(
        r#"query LocalizationTranslationsRead($resourceId: ID!) { translatableResource(resourceId: $resourceId) { resourceId translations(locale: "fr") { key value locale outdated updatedAt market { id } } } }"#,
        json!({ "resourceId": resource_id }),
    ));
    assert_eq!(
        downstream.body["data"]["translatableResource"]["translations"],
        json!([
            { "key": "title", "value": "Titre local", "locale": "fr", "outdated": false, "updatedAt": registered.body["data"]["translationsRegister"]["translations"][0]["updatedAt"], "market": null },
            { "key": "body_html", "value": "Description locale", "locale": "fr", "outdated": false, "updatedAt": registered.body["data"]["translationsRegister"]["translations"][1]["updatedAt"], "market": null }
        ])
    );

    let mixed = proxy.process_request(json_graphql_request(
        r#"mutation LocalizationTranslationsRegister($resourceId: ID!, $translations: [TranslationInput!]!) { translationsRegister(resourceId: $resourceId, translations: $translations) { translations { key value locale outdated updatedAt market { id } } userErrors { field message code } } }"#,
        json!({
            "resourceId": resource_id.as_str(),
            "translations": [
                { "locale": "fr", "key": "meta_title", "value": "Titre SEO", "translatableContentDigest": meta_title_digest },
                { "locale": "fr", "key": "title", "value": "Invalid digest row", "translatableContentDigest": "wrong-title-digest" },
                { "locale": "es", "key": "title", "value": "Titulo local", "translatableContentDigest": title_digest }
            ]
        }),
    ));
    assert_eq!(
        mixed.body["data"]["translationsRegister"]["translations"],
        json!([
            { "key": "meta_title", "value": "Titre SEO", "locale": "fr", "outdated": false, "updatedAt": mixed.body["data"]["translationsRegister"]["translations"][0]["updatedAt"], "market": null },
            { "key": "title", "value": "Titulo local", "locale": "es", "outdated": false, "updatedAt": mixed.body["data"]["translationsRegister"]["translations"][1]["updatedAt"], "market": null }
        ])
    );
    assert_datetime_string(
        &mixed.body["data"]["translationsRegister"]["translations"][0]["updatedAt"],
        "mixed seo translation updatedAt",
    );
    assert_datetime_string(
        &mixed.body["data"]["translationsRegister"]["translations"][1]["updatedAt"],
        "mixed es title translation updatedAt",
    );
    assert_eq!(
        mixed.body["data"]["translationsRegister"]["userErrors"][0]["field"],
        json!(["translations", "1", "translatableContentDigest"])
    );

    let downstream_after_mixed = proxy.process_request(json_graphql_request(
        r#"query LocalizationTranslationsRead($resourceId: ID!) { translatableResource(resourceId: $resourceId) { resourceId translations(locale: "fr") { key value locale outdated updatedAt market { id } } } }"#,
        json!({ "resourceId": resource_id }),
    ));
    assert_eq!(
        downstream_after_mixed.body["data"]["translatableResource"]["translations"],
        json!([
            { "key": "title", "value": "Titre local", "locale": "fr", "outdated": false, "updatedAt": registered.body["data"]["translationsRegister"]["translations"][0]["updatedAt"], "market": null },
            { "key": "body_html", "value": "Description locale", "locale": "fr", "outdated": false, "updatedAt": registered.body["data"]["translationsRegister"]["translations"][1]["updatedAt"], "market": null },
            { "key": "meta_title", "value": "Titre SEO", "locale": "fr", "outdated": false, "updatedAt": mixed.body["data"]["translationsRegister"]["translations"][0]["updatedAt"], "market": null }
        ])
    );
    let original_title_updated_at =
        registered.body["data"]["translationsRegister"]["translations"][0]["updatedAt"].clone();
    let reregister = proxy.process_request(json_graphql_request(
        r#"mutation LocalizationTranslationsRegister($resourceId: ID!, $translations: [TranslationInput!]!) { translationsRegister(resourceId: $resourceId, translations: $translations) { translations { key value locale outdated updatedAt market { id } } userErrors { field message code } } }"#,
        json!({
            "resourceId": resource_id.as_str(),
            "translations": [
                { "locale": "fr", "key": "title", "value": "Titre local rafraichi", "translatableContentDigest": title_digest }
            ]
        }),
    ));
    let refreshed_title_updated_at =
        reregister.body["data"]["translationsRegister"]["translations"][0]["updatedAt"].clone();
    assert_datetime_string(
        &refreshed_title_updated_at,
        "reregistered title translation updatedAt",
    );
    assert_ne!(refreshed_title_updated_at, original_title_updated_at);

    let downstream_after_reregister = proxy.process_request(json_graphql_request(
        r#"query LocalizationTranslationsRead($resourceId: ID!) { translatableResource(resourceId: $resourceId) { resourceId translations(locale: "fr") { key value locale outdated updatedAt market { id } } } }"#,
        json!({ "resourceId": resource_id }),
    ));
    assert_eq!(
        downstream_after_reregister.body["data"]["translatableResource"]["translations"],
        json!([
            { "key": "body_html", "value": "Description locale", "locale": "fr", "outdated": false, "updatedAt": registered.body["data"]["translationsRegister"]["translations"][1]["updatedAt"], "market": null },
            { "key": "meta_title", "value": "Titre SEO", "locale": "fr", "outdated": false, "updatedAt": mixed.body["data"]["translationsRegister"]["translations"][0]["updatedAt"], "market": null },
            { "key": "title", "value": "Titre local rafraichi", "locale": "fr", "outdated": false, "updatedAt": refreshed_title_updated_at, "market": null }
        ])
    );
}

#[test]
fn localization_translation_timestamps_follow_the_injected_clock_after_validation_failures() {
    let clock = Arc::new(Mutex::new(utc_time(1_783_080_000)));
    let mut proxy = snapshot_proxy_with_clock(Arc::clone(&clock));
    let resource_id = create_fallback_localization_product(&mut proxy);
    let title_digest = fallback_product_title_digest();

    let enable = proxy.process_request(json_graphql_request(
        r#"mutation EnableClockedLocale($locale: String!) {
          shopLocaleEnable(locale: $locale) { userErrors { field message } }
        }"#,
        json!({ "locale": "fr" }),
    ));
    assert_eq!(
        enable.body["data"]["shopLocaleEnable"]["userErrors"],
        json!([])
    );

    let register_title = |proxy: &mut DraftProxy, value: &str| {
        proxy.process_request(json_graphql_request(
            r#"mutation LocalizationClockedRegister($resourceId: ID!, $translations: [TranslationInput!]!) {
              translationsRegister(resourceId: $resourceId, translations: $translations) {
                translations { key value locale outdated updatedAt market { id } }
                userErrors { field message code }
              }
            }"#,
            json!({
                "resourceId": resource_id,
                "translations": [{
                    "locale": "fr",
                    "key": "title",
                    "value": value,
                    "translatableContentDigest": title_digest
                }]
            }),
        ))
    };

    let first = register_title(&mut proxy, "Titre horloge");
    assert_eq!(
        first.body["data"]["translationsRegister"]["translations"][0]["updatedAt"],
        json!("2026-07-03T12:00:00Z")
    );

    set_clock(&clock, 1_783_166_400);
    let second = register_title(&mut proxy, "Titre horloge avance");
    let second_translation = &second.body["data"]["translationsRegister"]["translations"][0];
    assert_eq!(
        second_translation["updatedAt"],
        json!("2026-07-04T12:00:00Z")
    );
    assert!(
        second_translation["updatedAt"].as_str()
            > first.body["data"]["translationsRegister"]["translations"][0]["updatedAt"].as_str()
    );

    set_clock(&clock, 1_783_339_200);
    let rejected = register_title(&mut proxy, "");
    assert_eq!(
        rejected.body["data"]["translationsRegister"]["translations"],
        json!([])
    );
    assert_eq!(
        rejected.body["data"]["translationsRegister"]["userErrors"][0]["code"],
        json!("FAILS_RESOURCE_VALIDATION")
    );

    let after_reject = proxy.process_request(json_graphql_request(
        r#"query LocalizationClockedRead($resourceId: ID!) {
          translatableResource(resourceId: $resourceId) {
            translations(locale: "fr") { key value updatedAt }
          }
        }"#,
        json!({ "resourceId": resource_id }),
    ));
    assert_eq!(
        after_reject.body["data"]["translatableResource"]["translations"],
        json!([{
            "key": "title",
            "value": "Titre horloge avance",
            "updatedAt": "2026-07-04T12:00:00Z"
        }])
    );

    set_clock(&clock, 1_783_252_800);
    let third = register_title(&mut proxy, "Titre horloge apres erreur");
    assert_eq!(
        third.body["data"]["translationsRegister"]["translations"][0]["updatedAt"],
        json!("2026-07-05T12:00:00Z")
    );
}

#[test]
fn market_localization_timestamps_follow_the_injected_clock_across_restore_and_reset() {
    let clock = Arc::new(Mutex::new(utc_time(1_783_080_000)));
    let resource_id = "gid://shopify/Metafield/clocked-market-localization";
    let market_id = "gid://shopify/Market/clocked-market";
    let content_digest = localization_content_digest("Clocked source value");
    let upstream_calls = Arc::new(Mutex::new(Vec::new()));
    let mut proxy = configured_proxy(ReadMode::LiveHybrid, None)
        .with_clock({
            let clock = Arc::clone(&clock);
            move || *clock.lock().unwrap()
        })
        .with_upstream_transport({
            let upstream_calls = Arc::clone(&upstream_calls);
            let content_digest = content_digest.clone();
            move |request| {
                upstream_calls
                    .lock()
                    .unwrap()
                    .push(serde_json::from_str::<Value>(&request.body).unwrap());
                Response {
                    status: 200,
                    headers: Default::default(),
                    body: json!({
                        "data": {
                            "marketLocalizableResource": {
                                "resourceId": resource_id,
                                "marketLocalizableContent": [{
                                    "key": "value",
                                    "value": "Clocked source value",
                                    "digest": content_digest
                                }],
                                "marketLocalizations": []
                            },
                            "markets": {
                                "nodes": [{
                                    "id": market_id,
                                    "name": "Canada",
                                    "handle": "canada",
                                    "status": "ACTIVE",
                                    "type": "REGION"
                                }]
                            }
                        }
                    }),
                }
            }
        });

    let register_market_value = |proxy: &mut DraftProxy, value: &str, digest: &str| {
        proxy.process_request(json_graphql_request(
            r#"mutation MarketLocalizationClockedRegister(
              $resourceId: ID!
              $marketLocalizations: [MarketLocalizationRegisterInput!]!
            ) {
              marketLocalizationsRegister(resourceId: $resourceId, marketLocalizations: $marketLocalizations) {
                marketLocalizations { key value updatedAt outdated market { id name } }
                userErrors { field message code }
              }
            }"#,
            json!({
                "resourceId": resource_id,
                "marketLocalizations": [{
                    "marketId": market_id,
                    "key": "value",
                    "value": value,
                    "marketLocalizableContentDigest": digest
                }]
            }),
        ))
    };

    let first = register_market_value(&mut proxy, "Canadian clock", &content_digest);
    let first_localization =
        &first.body["data"]["marketLocalizationsRegister"]["marketLocalizations"][0];
    assert_eq!(
        first_localization["updatedAt"],
        json!("2026-07-03T12:00:00Z")
    );
    assert_eq!(first_localization["market"]["name"], json!("Canada"));

    set_clock(&clock, 1_783_166_400);
    let second = register_market_value(&mut proxy, "Canadian clock advanced", &content_digest);
    let second_localization =
        &second.body["data"]["marketLocalizationsRegister"]["marketLocalizations"][0];
    assert_eq!(
        second_localization["updatedAt"],
        json!("2026-07-04T12:00:00Z")
    );
    assert!(second_localization["updatedAt"].as_str() > first_localization["updatedAt"].as_str());

    set_clock(&clock, 1_783_339_200);
    let rejected = register_market_value(&mut proxy, "", &content_digest);
    assert_eq!(
        rejected.body["data"]["marketLocalizationsRegister"]["marketLocalizations"],
        json!(null)
    );
    assert_eq!(
        rejected.body["data"]["marketLocalizationsRegister"]["userErrors"][0]["code"],
        json!("FAILS_RESOURCE_VALIDATION")
    );

    let after_reject = proxy.process_request(json_graphql_request(
        r#"query MarketLocalizationClockedRead($resourceId: ID!, $marketId: ID!) {
          marketLocalizableResource(resourceId: $resourceId) {
            marketLocalizations(marketId: $marketId) { key value updatedAt market { id name } }
          }
        }"#,
        json!({ "resourceId": resource_id, "marketId": market_id }),
    ));
    assert_eq!(
        after_reject.body["data"]["marketLocalizableResource"]["marketLocalizations"],
        json!([{
            "key": "value",
            "value": "Canadian clock advanced",
            "updatedAt": "2026-07-04T12:00:00Z",
            "market": { "id": market_id, "name": "Canada" }
        }])
    );

    set_clock(&clock, 1_783_252_800);
    let third = register_market_value(&mut proxy, "Canadian clock after error", &content_digest);
    assert_eq!(
        third.body["data"]["marketLocalizationsRegister"]["marketLocalizations"][0]["updatedAt"],
        json!("2026-07-05T12:00:00Z")
    );

    let dump = proxy.process_request(request_with_body("POST", "/__meta/dump", "{}"));
    assert_eq!(dump.status, 200);

    set_clock(&clock, 1_783_425_600);
    let mut restored = configured_proxy(ReadMode::LiveHybrid, None).with_clock({
        let clock = Arc::clone(&clock);
        move || *clock.lock().unwrap()
    });
    let restore = restored.process_request(request_with_body(
        "POST",
        "/__meta/restore",
        &dump.body.to_string(),
    ));
    assert_eq!(restore.status, 200);
    let restored_update =
        register_market_value(&mut restored, "Canadian clock restored", &content_digest);
    assert_eq!(
        restored_update.body["data"]["marketLocalizationsRegister"]["marketLocalizations"][0]
            ["updatedAt"],
        json!("2026-07-07T12:00:00Z")
    );

    set_clock(&clock, 1_783_080_000);
    let reset = restored.process_request(request_with_body("POST", "/__meta/reset", "{}"));
    assert_eq!(reset.status, 200);
    restored = restored.with_upstream_transport({
        let content_digest = content_digest.clone();
        move |_request| Response {
            status: 200,
            headers: Default::default(),
            body: json!({
                "data": {
                    "marketLocalizableResource": {
                        "resourceId": resource_id,
                        "marketLocalizableContent": [{
                            "key": "value",
                            "value": "Clocked source value",
                            "digest": content_digest
                        }],
                        "marketLocalizations": []
                    },
                    "markets": {
                        "nodes": [{
                            "id": market_id,
                            "name": "Canada",
                            "handle": "canada",
                            "status": "ACTIVE",
                            "type": "REGION"
                        }]
                    }
                }
            }),
        }
    });
    let after_reset = register_market_value(&mut restored, "Canadian clock reset", &content_digest);
    assert_eq!(
        after_reset.body["data"]["marketLocalizationsRegister"]["marketLocalizations"][0]
            ["updatedAt"],
        json!("2026-07-03T12:00:00Z")
    );
    assert_eq!(
        upstream_calls.lock().unwrap().len(),
        1,
        "only the initial cold preflight should use the first proxy transport"
    );
}

#[test]
fn localization_translations_remove_empty_keys_is_noop_and_preserves_read_after() {
    let mut proxy = snapshot_proxy();
    let resource_id = create_fallback_localization_product(&mut proxy);
    let title_digest = fallback_product_title_digest();
    let body_digest = fallback_product_body_digest();

    let enable = proxy.process_request(json_graphql_request(
        r#"mutation LocalizationShopLocaleEnable($locale: String!) {
          shopLocaleEnable(locale: $locale) { userErrors { field message } }
        }"#,
        json!({ "locale": "fr" }),
    ));
    assert_eq!(
        enable.body["data"]["shopLocaleEnable"]["userErrors"],
        json!([])
    );

    let registered = proxy.process_request(json_graphql_request(
        r#"mutation LocalizationTranslationsRegister($resourceId: ID!, $translations: [TranslationInput!]!) {
          translationsRegister(resourceId: $resourceId, translations: $translations) {
            translations { key value locale outdated updatedAt market { id } }
            userErrors { field message }
          }
        }"#,
        json!({
            "resourceId": resource_id.as_str(),
            "translations": [
                { "locale": "fr", "key": "title", "value": "Titre local", "translatableContentDigest": title_digest },
                { "locale": "fr", "key": "body_html", "value": "Description locale", "translatableContentDigest": body_digest }
            ]
        }),
    ));
    let expected_translations = json!([
        { "key": "title", "value": "Titre local", "locale": "fr", "outdated": false, "updatedAt": registered.body["data"]["translationsRegister"]["translations"][0]["updatedAt"], "market": null },
        { "key": "body_html", "value": "Description locale", "locale": "fr", "outdated": false, "updatedAt": registered.body["data"]["translationsRegister"]["translations"][1]["updatedAt"], "market": null }
    ]);
    assert_eq!(
        registered.body["data"]["translationsRegister"]["translations"],
        expected_translations
    );

    let remove = proxy.process_request(json_graphql_request(
        r#"mutation LocalizationTranslationsRemoveEmptyKeys($resourceId: ID!, $keys: [String!]!, $locales: [String!]!) {
          translationsRemove(resourceId: $resourceId, translationKeys: $keys, locales: $locales) {
            translations { key value locale outdated updatedAt market { id } }
            userErrors { field message }
          }
        }"#,
        json!({ "resourceId": resource_id.as_str(), "keys": [], "locales": ["fr"] }),
    ));
    assert_eq!(
        remove.body["data"]["translationsRemove"],
        json!({ "translations": null, "userErrors": [] })
    );

    let read_after_remove = proxy.process_request(json_graphql_request(
        r#"query LocalizationTranslationsReadAfterEmptyKeyRemove($resourceId: ID!) {
          translatableResource(resourceId: $resourceId) {
            translations(locale: "fr") { key value locale outdated updatedAt market { id } }
          }
        }"#,
        json!({ "resourceId": resource_id.as_str() }),
    ));
    assert_eq!(
        read_after_remove.body["data"]["translatableResource"]["translations"],
        expected_translations
    );
}

#[test]
fn localization_translatable_content_uses_modeled_source_values_and_round_trips_digests() {
    let mut proxy = snapshot_proxy();

    let product_create = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateLocalizedProduct($product: ProductCreateInput!) {
          productCreate(product: $product) {
            product { id }
            userErrors { field message  }
          }
        }
        "#,
        json!({
            "product": {
                "title": "Localized product",
                "handle": "localized-product",
                "descriptionHtml": "<p>Source body</p>",
                "productType": "Widget",
                "seo": {
                    "title": "Localized SEO title",
                    "description": "Localized SEO description"
                }
            }
        }),
    ));
    assert_eq!(
        product_create.body["data"]["productCreate"]["userErrors"],
        json!([])
    );
    let product_id = product_create.body["data"]["productCreate"]["product"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let collection_create = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateLocalizedCollection($input: CollectionInput!) {
          collectionCreate(input: $input) {
            collection { id }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "input": {
                "title": "Localized Collection",
                "handle": "localized-collection",
                "descriptionHtml": "<p>Collection body</p>"
            }
        }),
    ));
    assert_eq!(
        collection_create.body["data"]["collectionCreate"]["userErrors"],
        json!([])
    );
    let collection_id = collection_create.body["data"]["collectionCreate"]["collection"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let read = proxy.process_request(json_graphql_request(
        r#"
        query ReadTranslatableContent($productId: ID!, $collectionId: ID!) {
          product: translatableResource(resourceId: $productId) {
            resourceId
            translatableContent { key value digest locale type }
          }
          collection: translatableResource(resourceId: $collectionId) {
            resourceId
            translatableContent { key value digest locale type }
          }
          theme: translatableResource(resourceId: "gid://shopify/OnlineStoreTheme/123") {
            resourceId
            translatableContent { key value digest locale type }
          }
          products: translatableResources(first: 5, resourceType: PRODUCT) {
            nodes { resourceId }
          }
        }
        "#,
        json!({ "productId": product_id, "collectionId": collection_id }),
    ));
    assert_eq!(
        read.body["data"]["product"]["translatableContent"],
        json!([
            { "key": "title", "value": "Localized product", "digest": localization_content_digest("Localized product"), "locale": "en", "type": "SINGLE_LINE_TEXT_FIELD" },
            { "key": "body_html", "value": "<p>Source body</p>", "digest": localization_content_digest("<p>Source body</p>"), "locale": "en", "type": "HTML" },
            { "key": "handle", "value": "localized-product", "digest": localization_content_digest("localized-product"), "locale": "en", "type": "URI" },
            { "key": "product_type", "value": "Widget", "digest": localization_content_digest("Widget"), "locale": "en", "type": "SINGLE_LINE_TEXT_FIELD" },
            { "key": "meta_title", "value": "Localized SEO title", "digest": localization_content_digest("Localized SEO title"), "locale": "en", "type": "MULTI_LINE_TEXT_FIELD" },
            { "key": "meta_description", "value": "Localized SEO description", "digest": localization_content_digest("Localized SEO description"), "locale": "en", "type": "MULTI_LINE_TEXT_FIELD" }
        ])
    );
    assert_eq!(
        read.body["data"]["collection"]["translatableContent"],
        json!([
            { "key": "title", "value": "Localized Collection", "digest": localization_content_digest("Localized Collection"), "locale": "en", "type": "SINGLE_LINE_TEXT_FIELD" },
            { "key": "body_html", "value": "<p>Collection body</p>", "digest": localization_content_digest("<p>Collection body</p>"), "locale": "en", "type": "HTML" },
            { "key": "handle", "value": "localized-collection", "digest": localization_content_digest("localized-collection"), "locale": "en", "type": "URI" }
        ])
    );
    assert_eq!(read.body["data"]["theme"]["translatableContent"], json!([]));
    assert!(read.body["data"]["products"]["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .any(|node| node["resourceId"] == json!(product_id)));

    let enable = proxy.process_request(json_graphql_request(
        r#"mutation EnableFrench { shopLocaleEnable(locale: "fr") { userErrors { field message } } }"#,
        json!({}),
    ));
    assert_eq!(
        enable.body["data"]["shopLocaleEnable"]["userErrors"],
        json!([])
    );

    let product_title_digest = content_digest(
        &read.body["data"]["product"]["translatableContent"],
        "title",
    );
    let product_body_digest = content_digest(
        &read.body["data"]["product"]["translatableContent"],
        "body_html",
    );
    let collection_handle_digest = content_digest(
        &read.body["data"]["collection"]["translatableContent"],
        "handle",
    );
    let registered = proxy.process_request(json_graphql_request(
        r#"
        mutation RegisterFromReadDigests(
          $productId: ID!
          $collectionId: ID!
          $productTitleDigest: String!
          $productBodyDigest: String!
          $collectionHandleDigest: String!
        ) {
          product: translationsRegister(
            resourceId: $productId
            translations: [
              { locale: "fr", key: "title", value: "Produit localise", translatableContentDigest: $productTitleDigest }
              { locale: "fr", key: "body_html", value: "Corps localise", translatableContentDigest: $productBodyDigest }
            ]
          ) {
            translations { key value locale outdated market { id } }
            userErrors { field message code }
          }
          collection: translationsRegister(
            resourceId: $collectionId
            translations: [
              { locale: "fr", key: "handle", value: "collection-localisee", translatableContentDigest: $collectionHandleDigest }
            ]
          ) {
            translations { key value locale outdated market { id } }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "productId": product_id,
            "collectionId": collection_id,
            "productTitleDigest": product_title_digest,
            "productBodyDigest": product_body_digest,
            "collectionHandleDigest": collection_handle_digest
        }),
    ));
    assert_eq!(registered.body["data"]["product"]["userErrors"], json!([]));
    assert_eq!(
        registered.body["data"]["product"]["translations"],
        json!([
            { "key": "title", "value": "Produit localise", "locale": "fr", "outdated": false, "market": null },
            { "key": "body_html", "value": "Corps localise", "locale": "fr", "outdated": false, "market": null }
        ])
    );
    assert_eq!(
        registered.body["data"]["collection"]["userErrors"],
        json!([])
    );
    assert_eq!(
        registered.body["data"]["collection"]["translations"],
        json!([
            { "key": "handle", "value": "collection-localisee", "locale": "fr", "outdated": false, "market": null }
        ])
    );
}

#[test]
fn localization_translations_register_rejects_invalid_product_key_without_staging_it() {
    let mut proxy = snapshot_proxy();
    let resource_id = create_fallback_localization_product(&mut proxy);
    let title_digest = fallback_product_title_digest();

    let enable = proxy.process_request(json_graphql_request(
        r#"mutation LocalizationShopLocaleEnable($locale: String!) {
          shopLocaleEnable(locale: $locale) { userErrors { field message } }
        }"#,
        json!({ "locale": "fr" }),
    ));
    assert_eq!(
        enable.body["data"]["shopLocaleEnable"]["userErrors"],
        json!([])
    );

    let registered = proxy.process_request(json_graphql_request(
        r#"mutation LocalizationTranslationsRegister($resourceId: ID!, $translations: [TranslationInput!]!) {
          translationsRegister(resourceId: $resourceId, translations: $translations) {
            translations { key value locale outdated market { id } }
            userErrors { field message code }
          }
        }"#,
        json!({
            "resourceId": resource_id.as_str(),
            "translations": [
                { "locale": "fr", "key": "title", "value": "Titre valide", "translatableContentDigest": title_digest },
                { "locale": "fr", "key": "incorrect_key", "value": "Valeur invalide", "translatableContentDigest": title_digest }
            ]
        }),
    ));
    assert_eq!(
        registered.body["data"]["translationsRegister"]["translations"],
        json!([
            { "key": "title", "value": "Titre valide", "locale": "fr", "outdated": false, "market": null }
        ])
    );
    assert_eq!(
        registered.body["data"]["translationsRegister"]["userErrors"],
        json!([{
            "field": ["translations", "1", "key"],
            "message": "Key incorrect_key is not a valid translatable field",
            "code": "INVALID_KEY_FOR_MODEL"
        }])
    );

    let downstream = proxy.process_request(json_graphql_request(
        r#"query LocalizationTranslationsRead($resourceId: ID!) {
          translatableResource(resourceId: $resourceId) {
            resourceId
            translations(locale: "fr") { key value locale outdated market { id } }
          }
        }"#,
        json!({ "resourceId": resource_id }),
    ));
    assert_eq!(
        downstream.body["data"]["translatableResource"]["translations"],
        json!([
            { "key": "title", "value": "Titre valide", "locale": "fr", "outdated": false, "market": null }
        ])
    );
}

#[test]
fn localization_translatable_roots_are_store_backed_without_operation_markers() {
    let mut proxy = snapshot_proxy();
    let product_id = create_fallback_localization_product(&mut proxy);
    let collection_create = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateLocalizedCollection($input: CollectionInput!) {
          collectionCreate(input: $input) {
            collection { id }
            userErrors { field message }
          }
        }
        "#,
        json!({ "input": { "title": "Collection title" } }),
    ));
    assert_eq!(
        collection_create.body["data"]["collectionCreate"]["userErrors"],
        json!([])
    );
    let collection_id = collection_create.body["data"]["collectionCreate"]["collection"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let product_title_digest = fallback_product_title_digest();
    let product_type_digest = localization_content_digest("snowboard");
    let collection_title_digest = localization_content_digest("Collection title");

    let enable = proxy.process_request(json_graphql_request(
        r#"mutation EnableLocale($locale: String!) {
          shopLocaleEnable(locale: $locale) { userErrors { field message } }
        }"#,
        json!({ "locale": "fr" }),
    ));
    assert_eq!(
        enable.body["data"]["shopLocaleEnable"]["userErrors"],
        json!([])
    );
    let market_id = stage_market(&mut proxy, "Localization Market", "CA");

    let product_register = proxy.process_request(json_graphql_request(
        r#"mutation RegisterProductTranslation($resourceId: ID!, $translations: [TranslationInput!]!) {
          translationsRegister(resourceId: $resourceId, translations: $translations) {
            translations { key value locale market { id } }
            userErrors { field message code }
          }
        }"#,
        json!({
            "resourceId": product_id.as_str(),
            "translations": [
                { "locale": "fr", "key": "title", "value": "Titre produit", "translatableContentDigest": product_title_digest },
                { "locale": "fr", "key": "product_type", "value": "Produit", "translatableContentDigest": product_type_digest, "marketId": market_id }
            ]
        }),
    ));
    assert_eq!(
        product_register.body["data"]["translationsRegister"]["userErrors"],
        json!([])
    );

    let collection_register = proxy.process_request(json_graphql_request(
        r#"mutation RegisterCollectionTranslation($resourceId: ID!, $translations: [TranslationInput!]!) {
          translationsRegister(resourceId: $resourceId, translations: $translations) {
            translations { key value locale market { id } }
            userErrors { field message code }
          }
        }"#,
        json!({
            "resourceId": collection_id.as_str(),
            "translations": [
                { "locale": "fr", "key": "title", "value": "Titre collection", "translatableContentDigest": collection_title_digest }
            ]
        }),
    ));
    assert_eq!(
        collection_register.body["data"]["translationsRegister"]["userErrors"],
        json!([])
    );

    let read = proxy.process_request(json_graphql_request(
        r#"query ArbitraryLocalizationRead($productId: ID!, $ids: [ID!]!, $marketId: ID!) {
          direct: translatableResource(resourceId: $productId) {
            resourceId
            allFr: translations(locale: "fr") { key value locale market { id } }
            marketFr: translations(locale: "fr", marketId: $marketId) { key value locale market { id } }
          }
          byType: translatableResources(first: 2, resourceType: PRODUCT) {
            aliasedNodes: nodes { resourceId translations(locale: "fr") { key value } }
            aliasedEdges: edges { aliasedCursor: cursor node { resourceId } }
            aliasedPage: pageInfo { next: hasNextPage previous: hasPreviousPage }
          }
          byIds: translatableResourcesByIds(first: 3, resourceIds: $ids) {
            nodes { resourceId translations(locale: "fr") { key value } }
            edges { cursor node { resourceId } }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
          missing: translatableResource(resourceId: "gid://shopify/Product/999999999999999") { resourceId }
        }"#,
        json!({
            "productId": product_id.as_str(),
            "ids": [collection_id.as_str(), product_id.as_str()],
            "marketId": market_id
        }),
    ));

    assert_eq!(read.status, 200);
    assert_eq!(
        read.body["data"]["direct"]["allFr"],
        json!([
            { "key": "title", "value": "Titre produit", "locale": "fr", "market": null },
            { "key": "product_type", "value": "Produit", "locale": "fr", "market": { "id": market_id } }
        ])
    );
    assert_eq!(
        read.body["data"]["direct"]["marketFr"],
        json!([{ "key": "product_type", "value": "Produit", "locale": "fr", "market": { "id": market_id } }])
    );
    assert_eq!(
        read.body["data"]["byType"]["aliasedNodes"][0]["translations"],
        json!([
            { "key": "title", "value": "Titre produit" },
            { "key": "product_type", "value": "Produit" }
        ])
    );
    assert_eq!(
        read.body["data"]["byType"]["aliasedEdges"][0]["node"]["resourceId"],
        json!(product_id)
    );
    assert_eq!(
        read.body["data"]["byType"]["aliasedPage"],
        json!({ "next": false, "previous": false })
    );
    assert_eq!(
        read.body["data"]["byIds"]["nodes"][0]["resourceId"],
        json!(collection_id)
    );
    assert_eq!(
        read.body["data"]["byIds"]["nodes"][0]["translations"],
        json!([{ "key": "title", "value": "Titre collection" }])
    );
    assert_eq!(read.body["data"]["missing"], Value::Null);
}

#[test]
fn localization_translations_reject_unknown_supported_product_resource_ids() {
    let mut proxy = snapshot_proxy();
    let unknown_resource_id = "gid://shopify/Product/123";

    let enable = proxy.process_request(json_graphql_request(
        r#"mutation LocalizationShopLocaleEnable($locale: String!) {
          shopLocaleEnable(locale: $locale) { userErrors { field message } }
        }"#,
        json!({ "locale": "fr" }),
    ));
    assert_eq!(
        enable.body["data"]["shopLocaleEnable"]["userErrors"],
        json!([])
    );

    let register = proxy.process_request(json_graphql_request(
        r#"mutation LocalizationTranslationsRegister($resourceId: ID!, $translations: [TranslationInput!]!) {
          translationsRegister(resourceId: $resourceId, translations: $translations) {
            translations { key value locale }
            userErrors { field message code }
          }
        }"#,
        json!({
            "resourceId": unknown_resource_id,
            "translations": [{
                "locale": "fr",
                "key": "title",
                "value": "Bonjour",
                "translatableContentDigest": "digest"
            }]
        }),
    ));
    assert_eq!(
        register.body["data"]["translationsRegister"],
        json!({
            "translations": null,
            "userErrors": [{
                "field": ["resourceId"],
                "message": format!("Resource {unknown_resource_id} does not exist"),
                "code": "RESOURCE_NOT_FOUND"
            }]
        })
    );

    let downstream = proxy.process_request(json_graphql_request(
        r#"query LocalizationTranslationsRead($resourceId: ID!) {
          translatableResource(resourceId: $resourceId) {
            resourceId
            translations(locale: "fr") { key value locale }
          }
        }"#,
        json!({ "resourceId": unknown_resource_id }),
    ));
    assert_eq!(downstream.body["data"]["translatableResource"], Value::Null);

    let remove = proxy.process_request(json_graphql_request(
        r#"mutation LocalizationTranslationsRemove($resourceId: ID!, $keys: [String!]!, $locales: [String!]!) {
          translationsRemove(resourceId: $resourceId, translationKeys: $keys, locales: $locales) {
            translations { key value locale }
            userErrors { field message code }
          }
        }"#,
        json!({ "resourceId": unknown_resource_id, "keys": ["title"], "locales": ["fr"] }),
    ));
    assert_eq!(
        remove.body["data"]["translationsRemove"],
        json!({
            "translations": null,
            "userErrors": [{
                "field": ["resourceId"],
                "message": format!("Resource {unknown_resource_id} does not exist"),
                "code": "RESOURCE_NOT_FOUND"
            }]
        })
    );
}

#[test]
fn localization_translations_reject_missing_collection_and_unresolved_resource_types() {
    let mut proxy = snapshot_proxy();

    let enable = proxy.process_request(json_graphql_request(
        r#"mutation LocalizationShopLocaleEnable($locale: String!) {
          shopLocaleEnable(locale: $locale) { userErrors { field message } }
        }"#,
        json!({ "locale": "fr" }),
    ));
    assert_eq!(
        enable.body["data"]["shopLocaleEnable"]["userErrors"],
        json!([])
    );

    for resource_id in ["gid://shopify/Collection/999999999", "gid://shopify/Menu/1"] {
        let mutation = proxy.process_request(json_graphql_request(
            r#"mutation LocalizationUnknownResourceValidation($resourceId: ID!, $translations: [TranslationInput!]!, $keys: [String!]!, $locales: [String!]!) {
              register: translationsRegister(resourceId: $resourceId, translations: $translations) {
                translations { key value locale }
                userErrors { field message code }
              }
              remove: translationsRemove(resourceId: $resourceId, translationKeys: $keys, locales: $locales) {
                translations { key value locale }
                userErrors { field message code }
              }
            }"#,
            json!({
                "resourceId": resource_id,
                "translations": [{
                    "locale": "fr",
                    "key": "title",
                    "value": "Bonjour",
                    "translatableContentDigest": "digest"
                }],
                "keys": ["title"],
                "locales": ["fr"]
            }),
        ));
        let expected = json!({
            "translations": null,
            "userErrors": [{
                "field": ["resourceId"],
                "message": format!("Resource {resource_id} does not exist"),
                "code": "RESOURCE_NOT_FOUND"
            }]
        });
        assert_eq!(mutation.body["data"]["register"], expected);
        assert_eq!(mutation.body["data"]["remove"], expected);
    }
}

#[test]
fn localization_unknown_resource_and_market_scoped_translation_validation_match_shopify_shapes() {
    let mut proxy = snapshot_proxy();
    let title_digest = fallback_product_title_digest();
    let handle_digest = fallback_product_handle_digest();

    let unknown_resource = proxy.process_request(json_graphql_request(
        r#"mutation LocalizationUnknownResourceValidation($resourceId: ID!, $translations: [TranslationInput!]!, $keys: [String!]!, $locales: [String!]!) {
          register: translationsRegister(resourceId: $resourceId, translations: $translations) { translations { key } userErrors { field message code } }
          remove: translationsRemove(resourceId: $resourceId, translationKeys: $keys, locales: $locales) { translations { key } userErrors { field message code } }
        }"#,
        json!({ "resourceId": "gid://shopify/Product/999999999999999", "translations": [{ "locale": "fr", "key": "title", "value": "Missing", "translatableContentDigest": "missing" }], "keys": ["title"], "locales": ["fr"] }),
    ));
    assert_eq!(
        unknown_resource.body["data"]["register"]["translations"],
        Value::Null
    );
    assert_eq!(
        unknown_resource.body["data"]["register"]["userErrors"][0]["code"],
        json!("RESOURCE_NOT_FOUND")
    );
    assert_eq!(
        unknown_resource.body["data"]["remove"]["userErrors"][0]["field"],
        json!(["resourceId"])
    );

    let primary_disable = proxy.process_request(json_graphql_request(
        r#"mutation LocalizationShopLocaleDisable($locale: String!) { shopLocaleDisable(locale: $locale) { locale userErrors { field message } } }"#,
        json!({ "locale": "en" }),
    ));
    assert_eq!(
        primary_disable.body["data"]["shopLocaleDisable"]["locale"],
        Value::Null
    );
    assert_eq!(
        primary_disable.body["data"]["shopLocaleDisable"]["userErrors"][0]["field"],
        json!(["locale"])
    );

    for locale in ["fr", "es"] {
        let enable = proxy.process_request(json_graphql_request(
            r#"mutation LocalizationShopLocaleEnable($locale: String!) {
              shopLocaleEnable(locale: $locale) { userErrors { field message } }
            }"#,
            json!({ "locale": locale }),
        ));
        assert_eq!(
            enable.body["data"]["shopLocaleEnable"]["userErrors"],
            json!([])
        );
    }
    let resource_id = create_fallback_localization_product(&mut proxy);

    let blank_translation = proxy.process_request(json_graphql_request(
        r#"mutation LocalizationTranslationsRegister($resourceId: ID!, $translations: [TranslationInput!]!) { translationsRegister(resourceId: $resourceId, translations: $translations) { translations { key value locale } userErrors { field message code } } }"#,
        json!({ "resourceId": resource_id.as_str(), "translations": [{ "locale": "fr", "key": "title", "value": "", "translatableContentDigest": title_digest }] }),
    ));
    assert_eq!(
        blank_translation.body["data"]["translationsRegister"]["userErrors"][0]["code"],
        json!("FAILS_RESOURCE_VALIDATION")
    );

    let normalized_handle = proxy.process_request(json_graphql_request(
        r#"mutation LocalizationTranslationsRegister($resourceId: ID!, $translations: [TranslationInput!]!) { translationsRegister(resourceId: $resourceId, translations: $translations) { translations { key value locale outdated market { id } } userErrors { field message code } } }"#,
        json!({ "resourceId": resource_id.as_str(), "translations": [{ "locale": "fr", "key": "handle", "value": "Bad Value With Spaces", "translatableContentDigest": handle_digest }] }),
    ));
    assert_eq!(
        normalized_handle.body["data"]["translationsRegister"]["translations"][0]["value"],
        json!("bad-value-with-spaces")
    );
    let non_ascii_handle = proxy.process_request(json_graphql_request(
        r#"mutation LocalizationTranslationsRegister($resourceId: ID!, $translations: [TranslationInput!]!) { translationsRegister(resourceId: $resourceId, translations: $translations) { translations { key value locale outdated market { id } } userErrors { field message code } } }"#,
        json!({ "resourceId": resource_id.as_str(), "translations": [{ "locale": "fr", "key": "handle", "value": "日本", "translatableContentDigest": handle_digest }] }),
    ));
    assert_eq!(
        non_ascii_handle.body["data"]["translationsRegister"]["userErrors"],
        json!([])
    );
    let non_ascii_value = non_ascii_handle.body["data"]["translationsRegister"]["translations"][0]
        ["value"]
        .as_str()
        .unwrap();
    assert!(non_ascii_value.starts_with("localized-"));
    assert!(!non_ascii_value.contains('/'));
    assert_ne!(
        non_ascii_value,
        "store-localization/generic-dynamic-content-translation"
    );

    let unknown_market = proxy.process_request(json_graphql_request(
        r#"mutation LocalizationTranslationsRegister($resourceId: ID!, $translations: [TranslationInput!]!) { translationsRegister(resourceId: $resourceId, translations: $translations) { translations { key value locale outdated market { id } } userErrors { field message code } } }"#,
        json!({ "resourceId": resource_id.as_str(), "translations": [{ "locale": "es", "key": "title", "value": "Titulo", "translatableContentDigest": title_digest, "marketId": "gid://shopify/Market/424242" }] }),
    ));
    assert_eq!(
        unknown_market.body["data"]["translationsRegister"]["translations"],
        json!([])
    );
    assert_eq!(
        unknown_market.body["data"]["translationsRegister"]["userErrors"][0]["code"],
        json!("MARKET_DOES_NOT_EXIST")
    );

    let market_id = stage_market(&mut proxy, "Spanish Translation Market", "CA");
    let registered = proxy.process_request(json_graphql_request(
        r#"mutation LocalizationTranslationsRegister($resourceId: ID!, $translations: [TranslationInput!]!) { translationsRegister(resourceId: $resourceId, translations: $translations) { translations { key value locale outdated market { id } } userErrors { field message code } } }"#,
        json!({ "resourceId": resource_id.as_str(), "translations": [{ "locale": "es", "key": "title", "value": "Titulo", "translatableContentDigest": title_digest, "marketId": market_id }] }),
    ));
    assert_eq!(
        registered.body["data"]["translationsRegister"]["translations"][0]["market"]["id"],
        json!(market_id)
    );

    let packing_slip = proxy.process_request(json_graphql_request(
        r#"mutation LocalizationTranslationsRegister($resourceId: ID!, $translations: [TranslationInput!]!) {
          translationsRegister(resourceId: $resourceId, translations: $translations) {
            translations { key value locale market { id } }
            userErrors { field message code }
          }
        }"#,
        json!({
            "resourceId": "gid://shopify/PackingSlipTemplate/4834722098",
            "translations": [{
                "locale": "es",
                "key": "body",
                "value": "Cuerpo",
                "translatableContentDigest": "digest-body",
                "marketId": market_id
            }]
        }),
    ));
    assert_eq!(
        packing_slip.body["data"]["translationsRegister"],
        json!({
            "translations": [],
            "userErrors": [{
                "field": ["translations", "0", "key"],
                "message": "Key body cannot be customized for a market; it can only be translated.",
                "code": "RESOURCE_NOT_MARKET_CUSTOMIZABLE"
            }]
        })
    );

    let removed = proxy.process_request(json_graphql_request(
        r#"mutation LocalizationTranslationsMarketScopedRemove($resourceId: ID!, $keys: [String!]!, $locales: [String!]!, $marketIds: [ID!]!) { translationsRemove(resourceId: $resourceId, translationKeys: $keys, locales: $locales, marketIds: $marketIds) { translations { key value locale outdated market { id } } userErrors { field message code } } }"#,
        json!({ "resourceId": resource_id.as_str(), "keys": ["title"], "locales": ["es"], "marketIds": [market_id] }),
    ));
    assert_eq!(
        removed.body["data"]["translationsRemove"]["translations"][0]["market"]["id"],
        json!(market_id)
    );
    assert_eq!(
        removed.body["data"]["translationsRemove"]["userErrors"],
        json!([])
    );
}

#[test]
fn localization_target_existence_is_hydrated_not_sentinel_substring_routed() {
    let market_id = "gid://shopify/Market/999999123";
    let web_presence_id = "gid://shopify/MarketWebPresence/9999999999";
    let title_digest = fallback_product_title_digest();
    let upstream_hits = Arc::new(Mutex::new(Vec::<String>::new()));
    let captured_hits = Arc::clone(&upstream_hits);
    let mut proxy = configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport({
        let market_id = market_id.to_string();
        let web_presence_id = web_presence_id.to_string();
        move |request| {
            captured_hits.lock().unwrap().push(request.body.clone());
            assert!(
                request.body.contains("LocalizationMutationTargetsHydrate"),
                "only localization target hydration should hit upstream, got: {}",
                request.body
            );
            let node = if request.body.contains(&web_presence_id) {
                json!({
                    "__typename": "MarketWebPresence",
                    "id": web_presence_id,
                    "subfolderSuffix": "fr",
                    "domain": null,
                    "rootUrls": [],
                    "defaultLocale": { "locale": "en", "name": "English", "primary": true, "published": true },
                    "alternateLocales": [],
                    "markets": { "nodes": [] }
                })
            } else {
                json!({
                    "__typename": "Market",
                    "id": market_id,
                    "name": "Sentinel-looking market",
                    "handle": "sentinel-looking-market",
                    "status": "ACTIVE",
                    "type": "REGION"
                })
            };
            Response {
                status: 200,
                headers: Default::default(),
                body: json!({ "data": { "nodes": [node] } }),
            }
        }
    });
    let resource_id = create_fallback_localization_product(&mut proxy);

    let enable_es = proxy.process_request(json_graphql_request(
        r#"mutation EnableSpanish { shopLocaleEnable(locale: "es") { userErrors { field message } } }"#,
        json!({}),
    ));
    assert_eq!(
        enable_es.body["data"]["shopLocaleEnable"]["userErrors"],
        json!([])
    );

    let register = proxy.process_request(json_graphql_request(
        r#"mutation RegisterSentinelMarketTranslation($resourceId: ID!, $translations: [TranslationInput!]!) {
          translationsRegister(resourceId: $resourceId, translations: $translations) {
            translations { key value locale market { id } }
            userErrors { field message code }
          }
        }"#,
        json!({
            "resourceId": resource_id.as_str(),
            "translations": [{
                "locale": "es",
                "key": "title",
                "value": "Titulo",
                "translatableContentDigest": title_digest,
                "marketId": market_id
            }]
        }),
    ));
    assert_eq!(
        register.body["data"]["translationsRegister"]["userErrors"],
        json!([])
    );
    assert_eq!(
        register.body["data"]["translationsRegister"]["translations"][0]["market"]["id"],
        json!(market_id)
    );

    let remove = proxy.process_request(json_graphql_request(
        r#"mutation RemoveSentinelMarketTranslation($resourceId: ID!, $keys: [String!]!, $locales: [String!]!, $marketIds: [ID!]!) {
          translationsRemove(resourceId: $resourceId, translationKeys: $keys, locales: $locales, marketIds: $marketIds) {
            translations { key value locale market { id } }
            userErrors { field message code }
          }
        }"#,
        json!({
            "resourceId": resource_id.as_str(),
            "keys": ["title"],
            "locales": ["es"],
            "marketIds": [market_id]
        }),
    ));
    assert_eq!(
        remove.body["data"]["translationsRemove"]["translations"][0]["market"]["id"],
        json!(market_id)
    );
    assert_eq!(
        remove.body["data"]["translationsRemove"]["userErrors"],
        json!([])
    );

    let enable_fr = proxy.process_request(json_graphql_request(
        r#"mutation EnableFrenchWithSentinelWebPresence($id: ID!) {
          shopLocaleEnable(locale: "fr", marketWebPresenceIds: [$id]) {
            shopLocale { locale marketWebPresences { id __typename defaultLocale { locale } } }
            userErrors { field message }
          }
        }"#,
        json!({ "id": web_presence_id }),
    ));
    assert_eq!(
        enable_fr.body["data"]["shopLocaleEnable"]["userErrors"],
        json!([])
    );
    assert_eq!(
        enable_fr.body["data"]["shopLocaleEnable"]["shopLocale"]["marketWebPresences"],
        json!([{ "id": web_presence_id, "__typename": "MarketWebPresence", "defaultLocale": { "locale": "en" } }])
    );

    let hits = upstream_hits.lock().unwrap();
    assert_eq!(hits.len(), 2);
    assert!(hits.iter().any(|body| body.contains(market_id)));
    assert!(hits.iter().any(|body| body.contains(web_presence_id)));
}

#[test]
fn localization_translations_register_validation_order_matches_shopify_precedence() {
    let mut locale_proxy = snapshot_proxy();
    let locale_resource_id = create_fallback_localization_product(&mut locale_proxy);

    let non_enabled_blank = locale_proxy.process_request(json_graphql_request(
        r#"mutation LocalizationTranslationsRegister($resourceId: ID!, $translations: [TranslationInput!]!) {
          translationsRegister(resourceId: $resourceId, translations: $translations) {
            translations { key value locale }
            userErrors { field message code }
          }
        }"#,
        json!({
            "resourceId": locale_resource_id.as_str(),
            "translations": [{
                "locale": "it",
                "key": "title",
                "value": "",
                "translatableContentDigest": "digest"
            }]
        }),
    ));
    assert_eq!(
        non_enabled_blank.body["data"]["translationsRegister"],
        json!({
            "translations": [],
            "userErrors": [{
                "field": ["translations", "0", "locale"],
                "message": "Locale is not a valid locale for the shop",
                "code": "INVALID_LOCALE_FOR_SHOP"
            }]
        })
    );

    let primary_blank = locale_proxy.process_request(json_graphql_request(
        r#"mutation LocalizationTranslationsRegister($resourceId: ID!, $translations: [TranslationInput!]!) {
          translationsRegister(resourceId: $resourceId, translations: $translations) {
            translations { key value locale }
            userErrors { field message code }
          }
        }"#,
        json!({
            "resourceId": locale_resource_id.as_str(),
            "translations": [{
                "locale": "en",
                "key": "title",
                "value": "",
                "translatableContentDigest": "digest"
            }]
        }),
    ));
    assert_eq!(
        primary_blank.body["data"]["translationsRegister"],
        json!({
            "translations": [],
            "userErrors": [{
                "field": ["translations", "0", "locale"],
                "message": "Locale cannot be the same as the shop's primary locale",
                "code": "INVALID_LOCALE_FOR_SHOP"
            }]
        })
    );

    let non_enabled_unknown_market = locale_proxy.process_request(json_graphql_request(
        r#"mutation LocalizationTranslationsRegister($resourceId: ID!, $translations: [TranslationInput!]!) {
          translationsRegister(resourceId: $resourceId, translations: $translations) {
            translations { key value locale market { id } }
            userErrors { field message code }
          }
        }"#,
        json!({
            "resourceId": locale_resource_id.as_str(),
            "translations": [{
                "locale": "it",
                "key": "title",
                "value": "Ciao",
                "translatableContentDigest": "digest",
                "marketId": "gid://shopify/Market/424242"
            }]
        }),
    ));
    assert_eq!(
        non_enabled_unknown_market.body["data"]["translationsRegister"],
        json!({
            "translations": [],
            "userErrors": [{
                "field": ["translations", "0", "marketId"],
                "message": "The market corresponding to the `marketId` argument doesn't exist",
                "code": "MARKET_DOES_NOT_EXIST"
            }]
        })
    );

    let primary_unknown_market = locale_proxy.process_request(json_graphql_request(
        r#"mutation LocalizationTranslationsRegister($resourceId: ID!, $translations: [TranslationInput!]!) {
          translationsRegister(resourceId: $resourceId, translations: $translations) {
            translations { key value locale market { id } }
            userErrors { field message code }
          }
        }"#,
        json!({
            "resourceId": locale_resource_id.as_str(),
            "translations": [{
                "locale": "en",
                "key": "title",
                "value": "Hello",
                "translatableContentDigest": "digest",
                "marketId": "gid://shopify/Market/424242"
            }]
        }),
    ));
    assert_eq!(
        primary_unknown_market.body["data"]["translationsRegister"],
        json!({
            "translations": [],
            "userErrors": [{
                "field": ["translations", "0", "marketId"],
                "message": "The market corresponding to the `marketId` argument doesn't exist",
                "code": "MARKET_DOES_NOT_EXIST"
            }]
        })
    );

    let mut market_proxy = snapshot_proxy();
    let market_resource_id = create_fallback_localization_product(&mut market_proxy);
    let enable = market_proxy.process_request(json_graphql_request(
        r#"mutation LocalizationShopLocaleEnable($locale: String!) {
          shopLocaleEnable(locale: $locale) { userErrors { field message } }
        }"#,
        json!({ "locale": "es" }),
    ));
    assert_eq!(
        enable.body["data"]["shopLocaleEnable"]["userErrors"],
        json!([])
    );
    let unknown_market_blank = market_proxy.process_request(json_graphql_request(
        r#"mutation LocalizationTranslationsRegister($resourceId: ID!, $translations: [TranslationInput!]!) {
          translationsRegister(resourceId: $resourceId, translations: $translations) {
            translations { key value locale market { id } }
            userErrors { field message code }
          }
        }"#,
        json!({
            "resourceId": market_resource_id.as_str(),
            "translations": [{
                "locale": "es",
                "key": "title",
                "value": "",
                "translatableContentDigest": "digest",
                "marketId": "gid://shopify/Market/424242"
            }]
        }),
    ));
    assert_eq!(
        unknown_market_blank.body["data"]["translationsRegister"],
        json!({
            "translations": [],
            "userErrors": [{
                "field": ["translations", "0", "marketId"],
                "message": "The market corresponding to the `marketId` argument doesn't exist",
                "code": "MARKET_DOES_NOT_EXIST"
            }]
        })
    );
}

#[test]
fn localization_translations_register_accepts_market_original_without_shop_level_base() {
    let original_title = fallback_product_title_value();
    let title_digest = fallback_product_title_digest();
    let mut proxy = snapshot_proxy();
    let market_id = stage_market(&mut proxy, "Market Original Translation", "CA");

    let product_create = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateValueMatchProduct($product: ProductCreateInput!) {
          productCreate(product: $product) {
            product { id }
            userErrors { field message  }
          }
        }
        "#,
        json!({
            "product": {
                "title": original_title,
                "handle": "market-value-match-original-product"
            }
        }),
    ));
    assert_eq!(
        product_create.body["data"]["productCreate"]["userErrors"],
        json!([])
    );
    let resource_id = product_create.body["data"]["productCreate"]["product"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let enable = proxy.process_request(json_graphql_request(
        r#"mutation LocalizationShopLocaleEnable($locale: String!) {
          shopLocaleEnable(locale: $locale) { userErrors { field message } }
        }"#,
        json!({ "locale": "es" }),
    ));
    assert_eq!(
        enable.body["data"]["shopLocaleEnable"]["userErrors"],
        json!([])
    );

    let market_original = proxy.process_request(json_graphql_request(
        r#"mutation LocalizationTranslationsRegister($resourceId: ID!, $translations: [TranslationInput!]!) {
          translationsRegister(resourceId: $resourceId, translations: $translations) {
            translations { key value locale market { id } }
            userErrors { field message code }
          }
        }"#,
        json!({
            "resourceId": resource_id.as_str(),
            "translations": [{
                "locale": "es",
                "key": "title",
                "value": original_title,
                "translatableContentDigest": title_digest,
                "marketId": market_id
            }]
        }),
    ));
    assert_eq!(
        market_original.body["data"]["translationsRegister"]["translations"],
        json!([{
            "key": "title",
            "value": original_title,
            "locale": "es",
            "market": { "id": market_id }
        }])
    );
    assert_eq!(
        market_original.body["data"]["translationsRegister"]["userErrors"],
        json!([])
    );

    let downstream_after_original = proxy.process_request(json_graphql_request(
        r#"query LocalizationTranslationsRead($resourceId: ID!, $marketId: ID!) {
          translatableResource(resourceId: $resourceId) {
            resourceId
            translations(locale: "es", marketId: $marketId) { key value locale market { id } }
          }
        }"#,
        json!({ "resourceId": resource_id.as_str(), "marketId": market_id }),
    ));
    assert_eq!(
        downstream_after_original.body["data"]["translatableResource"]["translations"],
        json!([{
            "key": "title",
            "value": original_title,
            "locale": "es",
            "market": { "id": market_id }
        }])
    );

    let accepted = proxy.process_request(json_graphql_request(
        r#"mutation LocalizationTranslationsRegister($resourceId: ID!, $translations: [TranslationInput!]!) {
          translationsRegister(resourceId: $resourceId, translations: $translations) {
            translations { key value locale market { id } }
            userErrors { field message code }
          }
        }"#,
        json!({
            "resourceId": resource_id.as_str(),
            "translations": [{
                "locale": "es",
                "key": "title",
                "value": "Titulo local",
                "translatableContentDigest": title_digest,
                "marketId": market_id
            }]
        }),
    ));
    assert_eq!(
        accepted.body["data"]["translationsRegister"]["translations"],
        json!([{
            "key": "title",
            "value": "Titulo local",
            "locale": "es",
            "market": { "id": market_id }
        }])
    );
    assert_eq!(
        accepted.body["data"]["translationsRegister"]["userErrors"],
        json!([])
    );

    let downstream_after_accept = proxy.process_request(json_graphql_request(
        r#"query LocalizationTranslationsRead($resourceId: ID!, $marketId: ID!) {
          translatableResource(resourceId: $resourceId) {
            translations(locale: "es", marketId: $marketId) { key value locale market { id } }
          }
        }"#,
        json!({ "resourceId": resource_id.as_str(), "marketId": market_id }),
    ));
    assert_eq!(
        downstream_after_accept.body["data"]["translatableResource"]["translations"],
        json!([{
            "key": "title",
            "value": "Titulo local",
            "locale": "es",
            "market": { "id": market_id }
        }])
    );

    let enable_fr = proxy.process_request(json_graphql_request(
        r#"mutation LocalizationShopLocaleEnable($locale: String!) {
          shopLocaleEnable(locale: $locale) { userErrors { field message } }
        }"#,
        json!({ "locale": "fr" }),
    ));
    assert_eq!(
        enable_fr.body["data"]["shopLocaleEnable"]["userErrors"],
        json!([])
    );
    let shop_level = proxy.process_request(json_graphql_request(
        r#"mutation LocalizationTranslationsRegister($resourceId: ID!, $translations: [TranslationInput!]!) {
          translationsRegister(resourceId: $resourceId, translations: $translations) {
            translations { key value locale market { id } }
            userErrors { field message code }
          }
        }"#,
        json!({
            "resourceId": resource_id.as_str(),
            "translations": [{
                "locale": "fr",
                "key": "title",
                "value": original_title,
                "translatableContentDigest": title_digest
            }]
        }),
    ));
    assert_eq!(
        shop_level.body["data"]["translationsRegister"]["translations"],
        json!([{
            "key": "title",
            "value": original_title,
            "locale": "fr",
            "market": null
        }])
    );
    assert_eq!(
        shop_level.body["data"]["translationsRegister"]["userErrors"],
        json!([])
    );
}

#[test]
fn localization_translations_register_rejects_market_value_matching_shop_level_translation() {
    let title_digest = fallback_product_title_digest();
    let mut proxy = snapshot_proxy();
    let resource_id = create_fallback_localization_product(&mut proxy);
    let market_id = stage_market(&mut proxy, "Market Shop-Level Translation", "CA");

    let enable = proxy.process_request(json_graphql_request(
        r#"mutation LocalizationShopLocaleEnable($locale: String!) {
          shopLocaleEnable(locale: $locale) { userErrors { field message } }
        }"#,
        json!({ "locale": "fr" }),
    ));
    assert_eq!(
        enable.body["data"]["shopLocaleEnable"]["userErrors"],
        json!([])
    );

    let shop_level = proxy.process_request(json_graphql_request(
        r#"mutation LocalizationTranslationsRegister($resourceId: ID!, $translations: [TranslationInput!]!) {
          translationsRegister(resourceId: $resourceId, translations: $translations) {
            translations { key value locale market { id } }
            userErrors { field message code }
          }
        }"#,
        json!({
            "resourceId": resource_id.as_str(),
            "translations": [{
                "locale": "fr",
                "key": "title",
                "value": "Titre de base",
                "translatableContentDigest": title_digest
            }]
        }),
    ));
    assert_eq!(
        shop_level.body["data"]["translationsRegister"]["userErrors"],
        json!([])
    );

    let rejected = proxy.process_request(json_graphql_request(
        r#"mutation LocalizationTranslationsRegister($resourceId: ID!, $translations: [TranslationInput!]!) {
          translationsRegister(resourceId: $resourceId, translations: $translations) {
            translations { key value locale market { id } }
            userErrors { field message code }
          }
        }"#,
        json!({
            "resourceId": resource_id.as_str(),
            "translations": [{
                "locale": "fr",
                "key": "title",
                "value": "Titre de base",
                "translatableContentDigest": "invalid-digest-is-suppressed",
                "marketId": market_id
            }]
        }),
    ));
    assert_eq!(
        rejected.body["data"]["translationsRegister"],
        json!({
            "translations": [],
            "userErrors": [{
                "field": ["translations", "0", "value"],
                "message": "Value cannot match original content",
                "code": "FAILS_RESOURCE_VALIDATION"
            }]
        })
    );

    let downstream_after_reject = proxy.process_request(json_graphql_request(
        r#"query LocalizationTranslationsRead($resourceId: ID!, $marketId: ID!) {
          translatableResource(resourceId: $resourceId) {
            translations(locale: "fr", marketId: $marketId) { key value locale market { id } }
          }
        }"#,
        json!({ "resourceId": resource_id.as_str(), "marketId": market_id }),
    ));
    assert_eq!(
        downstream_after_reject.body["data"]["translatableResource"]["translations"],
        json!([])
    );
}

#[test]
fn localization_digest_validation_skips_unobserved_source_content_without_prefix_shortcut() {
    let resource_id = "gid://shopify/Product/1234567890000";
    let upstream_hits = Arc::new(Mutex::new(0usize));
    let hit_counter = Arc::clone(&upstream_hits);
    let mut proxy = configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport({
        let resource_id = resource_id.to_string();
        move |_request| {
            *hit_counter.lock().unwrap() += 1;
            Response {
                status: 200,
                headers: Default::default(),
                body: json!({
                    "data": {
                        "translatableResources": {
                            "nodes": [{ "resourceId": resource_id }],
                            "edges": [],
                            "pageInfo": {
                                "hasNextPage": false,
                                "hasPreviousPage": false,
                                "startCursor": null,
                                "endCursor": null
                            }
                        }
                    }
                }),
            }
        }
    });

    let hydrate = proxy.process_request(json_graphql_request(
        r#"query ObserveResourceIdOnly {
          translatableResources(first: 1, resourceType: PRODUCT) { nodes { resourceId } }
        }"#,
        json!({}),
    ));
    assert_eq!(hydrate.status, 200);
    assert_eq!(
        hydrate.body["data"]["translatableResources"]["nodes"][0]["resourceId"],
        json!(resource_id)
    );

    let enable = proxy.process_request(json_graphql_request(
        r#"mutation EnableLocale($locale: String!) {
          shopLocaleEnable(locale: $locale) { userErrors { field message } }
        }"#,
        json!({ "locale": "fr" }),
    ));
    assert_eq!(
        enable.body["data"]["shopLocaleEnable"]["userErrors"],
        json!([])
    );

    let register = proxy.process_request(json_graphql_request(
        r#"mutation RegisterUnobservedDigest($resourceId: ID!, $translations: [TranslationInput!]!) {
          translationsRegister(resourceId: $resourceId, translations: $translations) {
            translations { key value locale }
            userErrors { field message code }
          }
        }"#,
        json!({
            "resourceId": resource_id,
            "translations": [{
                "locale": "fr",
                "key": "title",
                "value": "Titre local",
                "translatableContentDigest": "invalid-prefix-is-not-a-digest-rule"
            }]
        }),
    ));
    assert_eq!(
        register.body["data"]["translationsRegister"],
        json!({
            "translations": [{
                "key": "title",
                "value": "Titre local",
                "locale": "fr"
            }],
            "userErrors": []
        })
    );
    assert_eq!(*upstream_hits.lock().unwrap(), 1);
}

#[test]
fn localization_translations_register_stages_locally_and_keeps_raw_mutation_for_commit() {
    let upstream_hits = Arc::new(Mutex::new(0usize));
    let hit_counter = Arc::clone(&upstream_hits);
    let title_digest = fallback_product_title_digest();
    let mut proxy = configured_proxy(
        ReadMode::LiveHybrid,
        Some(shopify_draft_proxy::proxy::UnsupportedMutationMode::Passthrough),
    )
    .with_upstream_transport(move |_request| {
        *hit_counter.lock().unwrap() += 1;
        shopify_draft_proxy::proxy::Response {
            status: 500,
            headers: Default::default(),
            body: json!({ "errors": [{ "message": "translationsRegister should stay local" }] }),
        }
    });
    let resource_id = create_fallback_localization_product(&mut proxy);

    let enable = proxy.process_request(json_graphql_request(
        r#"mutation LocalizationShopLocaleEnable($locale: String!) {
          shopLocaleEnable(locale: $locale) { userErrors { field message } }
        }"#,
        json!({ "locale": "fr" }),
    ));
    assert_eq!(
        enable.body["data"]["shopLocaleEnable"]["userErrors"],
        json!([])
    );

    let register = proxy.process_request(json_graphql_request(
        r#"mutation LocalizationTranslationsRegister($resourceId: ID!, $translations: [TranslationInput!]!) {
          translationsRegister(resourceId: $resourceId, translations: $translations) {
            translations { key value locale }
            userErrors { field message code }
          }
        }"#,
        json!({
            "resourceId": resource_id.as_str(),
            "translations": [{
                "locale": "fr",
                "key": "title",
                "value": "Titre local",
                "translatableContentDigest": title_digest
            }]
        }),
    ));
    assert_eq!(register.status, 200);
    assert_eq!(*upstream_hits.lock().unwrap(), 0);
    assert_eq!(
        register.body["data"]["translationsRegister"]["translations"],
        json!([{ "key": "title", "value": "Titre local", "locale": "fr" }])
    );
    assert_eq!(
        register.body["data"]["translationsRegister"]["userErrors"],
        json!([])
    );

    let log = log_snapshot(&proxy);
    assert_eq!(log["entries"].as_array().unwrap().len(), 3);
    assert!(log["entries"][2]["rawBody"]
        .as_str()
        .unwrap()
        .contains("mutation LocalizationTranslationsRegister"));
    assert!(log["entries"][2]["rawBody"]
        .as_str()
        .unwrap()
        .contains("Titre local"));
}

#[test]
fn localization_shop_locale_update_disable_tail_helpers_cover_current_behavior() {
    let mut proxy = snapshot_proxy();
    let known_presence = stage_web_presence(&mut proxy, "it");
    let unknown_presence = "gid://shopify/MarketWebPresence/9999999999";

    let lifecycle = proxy.process_request(json_graphql_request(
        r#"mutation RustLocalizationShopLocaleTailHelpers($known: ID!, $unknown: ID!) {
          enableFr: shopLocaleEnable(locale: "fr") { shopLocale { locale published } userErrors { field message } }
          publishFr: shopLocaleUpdate(locale: "fr", shopLocale: { published: true, marketWebPresenceIds: [$known, $unknown] }) { shopLocale { locale name published marketWebPresences { id __typename defaultLocale { locale } } } userErrors { field message } }
          attachMissing: shopLocaleUpdate(locale: "tr", shopLocale: { marketWebPresenceIds: [$known] }) { shopLocale { locale name published marketWebPresences { id __typename defaultLocale { locale } } } userErrors { field message } }
          missingWithPresenceUnpublish: shopLocaleUpdate(locale: "zz", shopLocale: { published: false, marketWebPresenceIds: [$known] }) { shopLocale { locale } userErrors { field message } }
          missingWithPresencePublish: shopLocaleUpdate(locale: "zz", shopLocale: { published: true, marketWebPresenceIds: [$known] }) { shopLocale { locale } userErrors { field message } }
          missingNoPresence: shopLocaleUpdate(locale: "de", shopLocale: { published: true }) { shopLocale { locale } userErrors { field message } }
          primaryPublish: shopLocaleUpdate(locale: "en", shopLocale: { published: true }) { shopLocale { locale } userErrors { field message } }
          primaryUnpublish: shopLocaleUpdate(locale: "en", shopLocale: { published: false }) { shopLocale { locale } userErrors { field message } }
          disablePrimary: shopLocaleDisable(locale: "en") { locale userErrors { field message } }
          disableUnknown: shopLocaleDisable(locale: "de") { locale userErrors { field message } }
        }"#,
        json!({ "known": known_presence, "unknown": unknown_presence }),
    ));
    assert_eq!(lifecycle.status, 200);
    assert_eq!(
        lifecycle.body["data"]["enableFr"],
        json!({ "shopLocale": { "locale": "fr", "published": false }, "userErrors": [] })
    );
    assert_eq!(
        lifecycle.body["data"]["publishFr"],
        json!({
            "shopLocale": {
                "locale": "fr",
                "name": "French",
                "published": true,
                "marketWebPresences": [{
                    "id": known_presence,
                    "__typename": "MarketWebPresence",
                    "defaultLocale": { "locale": "en" }
                }]
            },
            "userErrors": []
        })
    );
    assert_eq!(
        lifecycle.body["data"]["attachMissing"],
        json!({
            "shopLocale": {
                "locale": "tr",
                "name": "Turkish",
                "published": false,
                "marketWebPresences": [{
                    "id": known_presence,
                    "__typename": "MarketWebPresence",
                    "defaultLocale": { "locale": "en" }
                }]
            },
            "userErrors": []
        })
    );
    assert_eq!(
        lifecycle.body["data"]["missingNoPresence"],
        json!({
            "shopLocale": null,
            "userErrors": [{
                "field": ["locale"],
                "message": "The locale doesn't exist."
            }]
        })
    );
    let missing_locale_error = json!({
        "shopLocale": null,
        "userErrors": [{
            "field": ["locale"],
            "message": "The locale doesn't exist."
        }]
    });
    assert_eq!(
        lifecycle.body["data"]["missingWithPresenceUnpublish"],
        missing_locale_error
    );
    assert_eq!(
        lifecycle.body["data"]["missingWithPresencePublish"],
        missing_locale_error
    );
    assert_eq!(
        lifecycle.body["data"]["primaryUnpublish"],
        json!({
            "shopLocale": null,
            "userErrors": [{
                "field": ["locale"],
                "message": "The primary locale of your store can't be changed through this endpoint."
            }]
        })
    );
    assert_eq!(
        lifecycle.body["data"]["primaryPublish"],
        json!({
            "shopLocale": null,
            "userErrors": [{
                "field": ["locale"],
                "message": "The primary locale of your store can't be changed through this endpoint."
            }]
        })
    );
    assert_eq!(
        lifecycle.body["data"]["disablePrimary"],
        json!({
            "locale": null,
            "userErrors": [{
                "field": ["locale"],
                "message": "The primary locale of your store can't be changed through this endpoint."
            }]
        })
    );
    assert_eq!(
        lifecycle.body["data"]["disableUnknown"],
        json!({
            "locale": null,
            "userErrors": [{
                "field": ["locale"],
                "message": "The locale doesn't exist."
            }]
        })
    );

    let read = proxy.process_request(json_graphql_request(
        r#"query RustLocalizationShopLocaleTailHelpersRead {
          allLocales: shopLocales { locale published marketWebPresences { id __typename defaultLocale { locale } } }
          publishedLocales: shopLocales(published: true) { locale published }
        }"#,
        json!({}),
    ));
    let all_locales = read.body["data"]["allLocales"].as_array().unwrap();
    let staged_fr = all_locales
        .iter()
        .find(|locale| locale["locale"] == json!("fr"))
        .unwrap();
    assert_eq!(staged_fr["published"], json!(true));
    assert_eq!(
        staged_fr["marketWebPresences"],
        json!([{ "id": known_presence, "__typename": "MarketWebPresence", "defaultLocale": { "locale": "en" } }])
    );
    assert!(all_locales
        .iter()
        .any(|locale| locale["locale"] == json!("tr")));
    assert!(!all_locales
        .iter()
        .any(|locale| locale["locale"] == json!("zz")));
    assert!(read.body["data"]["publishedLocales"]
        .as_array()
        .unwrap()
        .iter()
        .any(|locale| locale["locale"] == json!("fr")));

    let disabled = proxy.process_request(json_graphql_request(
        r#"mutation RustLocalizationShopLocaleTailHelpersDisable { shopLocaleDisable(locale: "fr") { locale userErrors { field message } } }"#,
        json!({}),
    ));
    assert_eq!(
        disabled.body["data"]["shopLocaleDisable"],
        json!({ "locale": "fr", "userErrors": [] })
    );

    let after_disable = proxy.process_request(json_graphql_request(
        r#"query RustLocalizationShopLocaleTailHelpersReadAfterDisable { shopLocales { locale published } }"#,
        json!({}),
    ));
    assert!(!after_disable.body["data"]["shopLocales"]
        .as_array()
        .unwrap()
        .iter()
        .any(|locale| locale["locale"] == json!("fr")));
}

#[test]
fn localization_shop_locale_user_errors_reject_code_selection() {
    let mut proxy = snapshot_proxy();

    let mut code_selection_request = json_graphql_request(
        r#"mutation ShopLocaleUserErrorNoCode {
          enable: shopLocaleEnable(locale: "tlh") {
            userErrors { field message code }
          }
          update: shopLocaleUpdate(locale: "fr", shopLocale: { published: false }) {
            userErrors { code }
          }
          disable: shopLocaleDisable(locale: "en") {
            userErrors { field message code }
          }
        }"#,
        json!({}),
    );
    code_selection_request.path = "/admin/api/2025-01/graphql.json".to_string();
    let code_selection = proxy.process_request(code_selection_request);
    assert_eq!(code_selection.status, 200);
    assert!(code_selection.body.get("data").is_none());
    let errors = code_selection.body["errors"].as_array().unwrap();
    assert_eq!(errors.len(), 3);
    for (error, response_key) in errors.iter().zip(["enable", "update", "disable"]) {
        assert_eq!(
            error["message"],
            json!("Field 'code' doesn't exist on type 'UserError'")
        );
        assert_eq!(
            error["path"],
            json!([
                "mutation ShopLocaleUserErrorNoCode",
                response_key,
                "userErrors",
                "code"
            ])
        );
        assert_eq!(
            error["extensions"],
            json!({
                "code": "undefinedField",
                "typeName": "UserError",
                "fieldName": "code"
            })
        );
    }

    let field_message_selection = proxy.process_request(json_graphql_request(
        r#"mutation ShopLocaleUserErrorFieldMessage {
          enable: shopLocaleEnable(locale: "tlh") {
            shopLocale { locale }
            userErrors { field message }
          }
          update: shopLocaleUpdate(locale: "fr", shopLocale: { published: false }) {
            shopLocale { locale }
            userErrors { field message }
          }
          disable: shopLocaleDisable(locale: "en") {
            locale
            userErrors { field message }
          }
        }"#,
        json!({}),
    ));
    assert_eq!(field_message_selection.status, 200);
    assert!(field_message_selection.body.get("errors").is_none());
    assert_eq!(
        field_message_selection.body["data"],
        json!({
            "enable": {
                "shopLocale": null,
                "userErrors": [{
                    "field": ["locale"],
                    "message": "Locale is invalid"
                }]
            },
            "update": {
                "shopLocale": null,
                "userErrors": [{
                    "field": ["locale"],
                    "message": "The locale doesn't exist."
                }]
            },
            "disable": {
                "locale": null,
                "userErrors": [{
                    "field": ["locale"],
                    "message": "The primary locale of your store can't be changed through this endpoint."
                }]
            }
        })
    );
}

#[test]
fn localization_locale_cap_register_guards_and_remove_combinations_match_captured_behavior() {
    let mut proxy = snapshot_proxy();
    let title_digest = fallback_product_title_digest();
    // Stage 20 non-primary locales (the snapshot's primary "en" is excluded from the
    // cap count) so that the 21st enable below trips Shopify's 20-language limit. The
    // `localization-shop-locale-enable-validation` parity scenario proves the cap fires
    // only once 20 alternate locales are already present.
    let locale_codes = [
        "fr", "af", "ak", "sq", "am", "ar", "hy", "as", "az", "bm", "bn", "eu", "be", "bs", "br",
        "bg", "my", "ca", "ckb", "ce",
    ];
    for locale in locale_codes {
        let enable = proxy.process_request(json_graphql_request(
            r#"mutation LocalizationShopLocaleEnable($locale: String!) {
              shopLocaleEnable(locale: $locale) {
                shopLocale { locale }
                userErrors { field message }
              }
            }"#,
            json!({ "locale": locale }),
        ));
        assert_eq!(
            enable.body["data"]["shopLocaleEnable"]["userErrors"],
            json!([])
        );
    }

    let over_limit = proxy.process_request(json_graphql_request(
        r#"mutation LocalizationShopLocaleEnable($locale: String!) {
          shopLocaleEnable(locale: $locale) {
            shopLocale { locale }
            userErrors { field message }
          }
        }"#,
        json!({ "locale": "zh-CN" }),
    ));
    assert_eq!(
        over_limit.body["data"]["shopLocaleEnable"],
        json!({
            "shopLocale": null,
            "userErrors": [{
                "field": null,
                "message": "Your store has reached its 20 language limit. To add Chinese (Simplified), delete one of your other languages."
            }]
        })
    );

    let mut guard_proxy = snapshot_proxy();
    let guard_resource_id = create_fallback_localization_product(&mut guard_proxy);
    let non_enabled = guard_proxy.process_request(json_graphql_request(
        r#"mutation LocalizationTranslationsRegister($resourceId: ID!, $translations: [TranslationInput!]!) {
          translationsRegister(resourceId: $resourceId, translations: $translations) {
            translations { key value locale }
            userErrors { field message code }
          }
        }"#,
        json!({
            "resourceId": guard_resource_id.as_str(),
            "translations": [{
                "locale": "es",
                "key": "title",
                "value": "Titulo local",
                "translatableContentDigest": "digest"
            }]
        }),
    ));
    assert_eq!(
        non_enabled.body["data"]["translationsRegister"],
        json!({
            "translations": [],
            "userErrors": [{
                "field": ["translations", "0", "locale"],
                "message": "Locale is not a valid locale for the shop",
                "code": "INVALID_LOCALE_FOR_SHOP"
            }]
        })
    );

    let primary_locale = guard_proxy.process_request(json_graphql_request(
        r#"mutation LocalizationTranslationsRegister($resourceId: ID!, $translations: [TranslationInput!]!) {
          translationsRegister(resourceId: $resourceId, translations: $translations) {
            translations { key value locale }
            userErrors { field message code }
          }
        }"#,
        json!({
            "resourceId": guard_resource_id.as_str(),
            "translations": [{
                "locale": "en",
                "key": "title",
                "value": "Primary title",
                "translatableContentDigest": "digest"
            }]
        }),
    ));
    assert_eq!(
        primary_locale.body["data"]["translationsRegister"]["userErrors"][0]["code"],
        json!("INVALID_LOCALE_FOR_SHOP")
    );

    let mut remove_proxy = snapshot_proxy();
    let remove_resource_id = create_fallback_localization_product(&mut remove_proxy);
    let body_digest = fallback_product_body_digest();
    let market_id = stage_market(&mut remove_proxy, "Remove Combination Market", "CA");
    for locale in ["es", "fr"] {
        let enable = remove_proxy.process_request(json_graphql_request(
            r#"mutation LocalizationShopLocaleEnable($locale: String!) {
              shopLocaleEnable(locale: $locale) { userErrors { field message } }
            }"#,
            json!({ "locale": locale }),
        ));
        assert_eq!(
            enable.body["data"]["shopLocaleEnable"]["userErrors"],
            json!([])
        );
    }
    let register = remove_proxy.process_request(json_graphql_request(
        r#"mutation LocalizationTranslationsRegister($resourceId: ID!, $translations: [TranslationInput!]!) {
          translationsRegister(resourceId: $resourceId, translations: $translations) {
            translations { key value locale market { id } }
            userErrors { field message code }
          }
        }"#,
        json!({
            "resourceId": remove_resource_id.as_str(),
            "translations": [
                { "locale": "es", "key": "title", "value": "Titulo", "translatableContentDigest": title_digest, "marketId": market_id },
                { "locale": "es", "key": "body_html", "value": "Cuerpo", "translatableContentDigest": body_digest, "marketId": market_id },
                { "locale": "fr", "key": "title", "value": "Titre", "translatableContentDigest": title_digest }
            ]
        }),
    ));
    assert_eq!(
        register.body["data"]["translationsRegister"]["translations"]
            .as_array()
            .unwrap()
            .len(),
        3
    );

    let remove = remove_proxy.process_request(json_graphql_request(
        r#"mutation LocalizationTranslationsMarketScopedRemove($resourceId: ID!, $keys: [String!]!, $locales: [String!]!, $marketIds: [ID!]!) {
          translationsRemove(resourceId: $resourceId, translationKeys: $keys, locales: $locales, marketIds: $marketIds) {
            translations { key value locale market { id } }
            userErrors { field message code }
          }
        }"#,
        json!({
            "resourceId": remove_resource_id.as_str(),
            "keys": ["title", "body_html"],
            "locales": ["es", "fr"],
            "marketIds": [market_id]
        }),
    ));
    let removed = remove.body["data"]["translationsRemove"]["translations"]
        .as_array()
        .unwrap();
    assert_eq!(removed.len(), 2);
    assert!(removed
        .iter()
        .any(|translation| translation["key"] == json!("title")));
    assert!(removed
        .iter()
        .any(|translation| translation["key"] == json!("body_html")));

    let read_after_remove = remove_proxy.process_request(json_graphql_request(
        r#"query LocalizationTranslationsMarketScopedRead($resourceId: ID!) {
          translatableResource(resourceId: $resourceId) {
            translations(locale: "fr") {
              key value locale market { id }
            }
          }
        }"#,
        json!({ "resourceId": remove_resource_id.as_str() }),
    ));
    assert_eq!(
        read_after_remove.body["data"]["translatableResource"]["translations"],
        json!([{
            "key": "title",
            "value": "Titre",
            "locale": "fr",
            "market": null
        }])
    );
}

#[test]
fn gift_card_live_hybrid_cold_reads_forward_upstream_without_local_overlay() {
    let hits = Arc::new(Mutex::new(0usize));
    let hit_counter = Arc::clone(&hits);
    let upstream_id = "gid://shopify/GiftCard/777000111222";
    let mut proxy =
        configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport(move |request| {
            *hit_counter.lock().unwrap() += 1;
            assert!(
                request.body.contains("giftCard")
                    && request.body.contains("giftCards")
                    && request.body.contains("giftCardsCount"),
                "cold gift-card read should forward the original read, got {}",
                request.body
            );
            Response {
                status: 200,
                headers: Default::default(),
                body: json!({
                    "data": {
                        "card": {
                            "id": upstream_id,
                            "note": "real upstream note",
                            "balance": { "amount": "25.0", "currencyCode": "USD" }
                        },
                        "cards": {
                        "nodes": [{
                            "id": upstream_id,
                            "note": "real upstream note"
                        }],
                        "pageInfo": {
                            "hasNextPage": false,
                            "hasPreviousPage": false
                        }
                        },
                        "count": { "count": 1, "precision": "EXACT" },
                        "configuration": {
                            "issueLimit": { "amount": "3000.0", "currencyCode": "USD" },
                            "purchaseLimit": { "amount": "14000.0", "currencyCode": "USD" }
                        }
                    }
                }),
            }
        });

    let response = proxy.process_request(json_graphql_request(
        r#"query GiftCardColdRead($id: ID!) {
          card: giftCard(id: $id) { id note balance { amount currencyCode } }
          cards: giftCards(first: 10) { nodes { id note } pageInfo { hasNextPage hasPreviousPage } }
          count: giftCardsCount { count precision }
          configuration: giftCardConfiguration { issueLimit { amount currencyCode } purchaseLimit { amount currencyCode } }
        }"#,
        json!({ "id": upstream_id }),
    ));

    assert_eq!(response.status, 200);
    assert_eq!(*hits.lock().unwrap(), 1);
    assert_eq!(
        response.body["data"]["card"],
        json!({
            "id": upstream_id,
            "note": "real upstream note",
            "balance": { "amount": "25.0", "currencyCode": "USD" }
        })
    );
    assert_eq!(
        response.body["data"]["cards"]["nodes"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        response.body["data"]["count"],
        json!({ "count": 1, "precision": "EXACT" })
    );
    assert_eq!(
        response.body["data"]["configuration"]["issueLimit"]["currencyCode"],
        json!("USD")
    );
}

#[test]
fn gift_card_live_hybrid_zero_count_baseline_makes_filtered_staged_windows_local() {
    let hits = Arc::new(Mutex::new(0usize));
    let hit_counter = Arc::clone(&hits);
    let mut proxy =
        configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport(move |request| {
            *hit_counter.lock().unwrap() += 1;
            if request.body.contains("GiftCardCreateConfiguration") {
                return Response {
                    status: 200,
                    headers: Default::default(),
                    body: json!({
                        "data": {
                            "giftCardConfiguration": {
                                "issueLimit": { "amount": "3000.0", "currencyCode": "CAD" },
                                "purchaseLimit": { "amount": "14000.0", "currencyCode": "CAD" }
                            }
                        }
                    }),
                };
            }
            assert!(
                request.body.contains("GiftCardConnectionBaseline"),
                "only the initial filtered baseline should forward, got {}",
                request.body
            );
            Response {
                status: 200,
                headers: Default::default(),
                body: json!({
                    "data": {
                        "forward": {
                            "nodes": [],
                            "edges": [],
                            "pageInfo": {
                                "hasNextPage": false,
                                "hasPreviousPage": false,
                                "startCursor": null,
                                "endCursor": null
                            }
                        },
                        "reverse": {
                            "nodes": [],
                            "edges": [],
                            "pageInfo": {
                                "hasNextPage": false,
                                "hasPreviousPage": false,
                                "startCursor": null,
                                "endCursor": null
                            }
                        },
                        "countLimit": { "count": 0, "precision": "EXACT" }
                    }
                }),
            }
        });

    let setup = proxy.process_request(json_graphql_request(
        r#"mutation GiftCardConnectionBaselineSetup {
          first: giftCardCreate(input: { initialValue: "41.01", code: "livebaselinea" }) {
            giftCard { id lastCharacters initialValue { amount currencyCode } }
            userErrors { field message }
          }
          second: giftCardCreate(input: { initialValue: "41.02", code: "livebaselineb" }) {
            giftCard { id lastCharacters initialValue { amount currencyCode } }
            userErrors { field message }
          }
          third: giftCardCreate(input: { initialValue: "41.03", code: "livebaselinec" }) {
            giftCard { id lastCharacters initialValue { amount currencyCode } }
            userErrors { field message }
          }
        }"#,
        json!({}),
    ));
    assert_eq!(setup.body["data"]["first"]["userErrors"], json!([]));
    assert_eq!(setup.body["data"]["second"]["userErrors"], json!([]));
    assert_eq!(setup.body["data"]["third"]["userErrors"], json!([]));

    let first_page = proxy.process_request(json_graphql_request(
        r#"query GiftCardConnectionBaseline($query: String!) {
          forward: giftCards(first: 2, query: $query, sortKey: ID) {
            nodes { id lastCharacters initialValue { amount currencyCode } }
            edges { cursor node { id lastCharacters } }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
          reverse: giftCards(first: 2, query: $query, sortKey: ID, reverse: true) {
            nodes { id lastCharacters }
            edges { cursor node { id lastCharacters } }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
          countLimit: giftCardsCount(query: $query, limit: 2) { count precision }
        }"#,
        json!({ "query": "livebaseline" }),
    ));
    assert_eq!(*hits.lock().unwrap(), 2);
    assert_eq!(
        first_page.body["data"]["forward"]["nodes"]
            .as_array()
            .map(Vec::len),
        Some(2)
    );
    assert_eq!(
        first_page.body["data"]["countLimit"],
        json!({ "count": 2, "precision": "AT_LEAST" })
    );
    let after = json_string(
        &first_page.body["data"]["forward"]["pageInfo"]["endCursor"],
        "gift-card baseline end cursor",
    );

    let window = proxy.process_request(json_graphql_request(
        r#"query GiftCardConnectionBaselineWindow($query: String!, $after: String!) {
          giftCards(first: 1, query: $query, sortKey: ID, after: $after) {
            nodes { id lastCharacters initialValue { amount currencyCode } }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
        }"#,
        json!({ "query": "livebaseline", "after": after }),
    ));
    assert_eq!(
        *hits.lock().unwrap(),
        2,
        "the complete zero-count baseline should keep later windows local"
    );
    assert_eq!(
        window.body["data"]["giftCards"]["nodes"]
            .as_array()
            .map(Vec::len),
        Some(1)
    );
    assert_eq!(
        window.body["data"]["giftCards"]["nodes"][0]["initialValue"]["amount"],
        json!("41.03")
    );
}

#[test]
fn gift_card_snapshot_legacy_seed_id_without_state_returns_not_found() {
    let mut proxy = snapshot_proxy();
    let legacy_id = "gid://shopify/GiftCard/654773256498";

    let response = proxy.process_request(json_graphql_request(
        r#"query GiftCardLegacySeedIdWithoutState($id: ID!, $query: String!) {
          card: giftCard(id: $id) {
            id
            lastCharacters
            balance { amount currencyCode }
            customer { id }
            recipientAttributes { message }
          }
          cards: giftCards(first: 2, query: $query, sortKey: ID) {
            nodes { id lastCharacters }
            pageInfo { hasNextPage hasPreviousPage }
          }
          count: giftCardsCount(query: $query) { count precision }
        }"#,
        json!({ "id": legacy_id, "query": "id:654773256498" }),
    ));

    assert_eq!(response.status, 200);
    assert_eq!(response.body["data"]["card"], Value::Null);
    assert_eq!(
        response.body["data"]["cards"],
        json!({
            "nodes": [],
            "pageInfo": { "hasNextPage": false, "hasPreviousPage": false }
        })
    );
    assert_eq!(
        response.body["data"]["count"],
        json!({ "count": 0, "precision": "EXACT" })
    );
}

#[test]
fn gift_card_connection_returns_edges_cursors_windows_sort_and_reverse() {
    let mut proxy = snapshot_proxy();
    let specs = [
        ("gid://shopify/GiftCard/100", "0100", "2026-06-03T00:00:00Z"),
        ("gid://shopify/GiftCard/200", "0200", "2026-06-01T00:00:00Z"),
        ("gid://shopify/GiftCard/300", "0300", "2026-06-02T00:00:00Z"),
    ];
    restore_proxy_state(&mut proxy, |restored| {
        let mut cards = serde_json::Map::new();
        for (id, last_characters, created_at) in specs {
            let mut card = legacy_gift_card_fixture(id);
            card["lastCharacters"] = json!(last_characters);
            card["createdAt"] = json!(created_at);
            card["updatedAt"] = json!(created_at);
            cards.insert(id.to_string(), card);
        }
        restored["state"]["baseState"]["giftCards"] = Value::Object(cards);
    });

    let response = proxy.process_request(json_graphql_request(
        r#"query GiftCardConnectionMechanics($after: String!, $before: String!, $query: String!) {
          defaultWindow: giftCards(first: 2) {
            edges { cursor node { id lastCharacters } }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
          reverseWindow: giftCards(first: 2, reverse: true) {
            edges { cursor node { id lastCharacters } }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
          afterWindow: giftCards(first: 1, after: $after, reverse: true) {
            edges { cursor node { id } }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
          beforeLastWindow: giftCards(last: 1, before: $before, reverse: true) {
            edges { cursor node { id } }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
          createdOrder: giftCards(first: 3, sortKey: CREATED_AT) {
            nodes { id }
          }
          filtered: giftCards(first: 2, query: $query) {
            edges { cursor node { id } }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
        }"#,
        json!({
            "after": "gid://shopify/GiftCard/300",
            "before": "gid://shopify/GiftCard/100",
            "query": "id:200"
        }),
    ));

    assert_eq!(response.status, 200);
    assert_eq!(
        response.body["data"]["defaultWindow"],
        json!({
            "edges": [
                {
                    "cursor": "gid://shopify/GiftCard/100",
                    "node": { "id": "gid://shopify/GiftCard/100", "lastCharacters": "0100" }
                },
                {
                    "cursor": "gid://shopify/GiftCard/200",
                    "node": { "id": "gid://shopify/GiftCard/200", "lastCharacters": "0200" }
                }
            ],
            "pageInfo": {
                "hasNextPage": true,
                "hasPreviousPage": false,
                "startCursor": "gid://shopify/GiftCard/100",
                "endCursor": "gid://shopify/GiftCard/200"
            }
        })
    );
    assert_eq!(
        response.body["data"]["reverseWindow"],
        json!({
            "edges": [
                {
                    "cursor": "gid://shopify/GiftCard/300",
                    "node": { "id": "gid://shopify/GiftCard/300", "lastCharacters": "0300" }
                },
                {
                    "cursor": "gid://shopify/GiftCard/200",
                    "node": { "id": "gid://shopify/GiftCard/200", "lastCharacters": "0200" }
                }
            ],
            "pageInfo": {
                "hasNextPage": true,
                "hasPreviousPage": false,
                "startCursor": "gid://shopify/GiftCard/300",
                "endCursor": "gid://shopify/GiftCard/200"
            }
        })
    );
    assert_eq!(
        response.body["data"]["afterWindow"],
        json!({
            "edges": [{
                "cursor": "gid://shopify/GiftCard/200",
                "node": { "id": "gid://shopify/GiftCard/200" }
            }],
            "pageInfo": {
                "hasNextPage": true,
                "hasPreviousPage": true,
                "startCursor": "gid://shopify/GiftCard/200",
                "endCursor": "gid://shopify/GiftCard/200"
            }
        })
    );
    assert_eq!(
        response.body["data"]["beforeLastWindow"],
        json!({
            "edges": [{
                "cursor": "gid://shopify/GiftCard/200",
                "node": { "id": "gid://shopify/GiftCard/200" }
            }],
            "pageInfo": {
                "hasNextPage": true,
                "hasPreviousPage": true,
                "startCursor": "gid://shopify/GiftCard/200",
                "endCursor": "gid://shopify/GiftCard/200"
            }
        })
    );
    assert_eq!(
        response.body["data"]["createdOrder"]["nodes"],
        json!([
            { "id": "gid://shopify/GiftCard/200" },
            { "id": "gid://shopify/GiftCard/300" },
            { "id": "gid://shopify/GiftCard/100" }
        ])
    );
    assert_eq!(
        response.body["data"]["filtered"],
        json!({
            "edges": [{
                "cursor": "gid://shopify/GiftCard/200",
                "node": { "id": "gid://shopify/GiftCard/200" }
            }],
            "pageInfo": {
                "hasNextPage": false,
                "hasPreviousPage": false,
                "startCursor": "gid://shopify/GiftCard/200",
                "endCursor": "gid://shopify/GiftCard/200"
            }
        })
    );
}

#[test]
fn gift_card_connection_sorts_disabled_at_from_deactivated_at() {
    let mut proxy = snapshot_proxy();
    let setup = proxy.process_request(json_graphql_request(
        r#"mutation GiftCardDisabledAtSetup {
          first: giftCardCreate(input: { initialValue: "41.01", code: "disabledsortnuqa" }) {
            giftCard { id lastCharacters }
            userErrors { field message }
          }
          second: giftCardCreate(input: { initialValue: "41.02", code: "disabledsortnuqb" }) {
            giftCard { id lastCharacters }
            userErrors { field message }
          }
          third: giftCardCreate(input: { initialValue: "41.03", code: "disabledsortnuqc" }) {
            giftCard { id lastCharacters }
            userErrors { field message }
          }
        }"#,
        json!({}),
    ));
    assert_eq!(setup.status, 200);
    for alias in ["first", "second", "third"] {
        assert_eq!(
            setup.body["data"][alias]["userErrors"],
            json!([]),
            "{alias} setup should succeed"
        );
    }
    let first_id = setup.body["data"]["first"]["giftCard"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let second_id = setup.body["data"]["second"]["giftCard"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let third_id = setup.body["data"]["third"]["giftCard"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let deactivate = proxy.process_request(json_graphql_request(
        r#"mutation GiftCardDisabledAtDeactivate($first: ID!, $second: ID!) {
          first: giftCardDeactivate(id: $first) {
            giftCard { id enabled deactivatedAt }
            userErrors { field message }
          }
          second: giftCardDeactivate(id: $second) {
            giftCard { id enabled deactivatedAt }
            userErrors { field message }
          }
        }"#,
        json!({ "first": first_id, "second": second_id }),
    ));
    assert_eq!(deactivate.status, 200);
    assert_eq!(deactivate.body["data"]["first"]["userErrors"], json!([]));
    assert_eq!(deactivate.body["data"]["second"]["userErrors"], json!([]));
    let first_deactivated_at = deactivate.body["data"]["first"]["giftCard"]["deactivatedAt"]
        .as_str()
        .unwrap()
        .to_string();
    let second_deactivated_at = deactivate.body["data"]["second"]["giftCard"]["deactivatedAt"]
        .as_str()
        .unwrap()
        .to_string();

    let response = proxy.process_request(json_graphql_request(
        r#"query GiftCardDisabledAtConnection($query: String!, $after: String!) {
          disabledOrder: giftCards(first: 3, query: $query, sortKey: DISABLED_AT) {
            nodes { id lastCharacters enabled deactivatedAt }
          }
          reverseWindow: giftCards(first: 2, query: $query, sortKey: DISABLED_AT, reverse: true) {
            edges { cursor node { id lastCharacters enabled deactivatedAt } }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
          afterWindow: giftCards(first: 1, query: $query, sortKey: DISABLED_AT, reverse: true, after: $after) {
            edges { cursor node { id lastCharacters enabled deactivatedAt } }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
        }"#,
        json!({ "query": "nuq", "after": first_id }),
    ));

    assert_eq!(response.status, 200);
    assert_eq!(
        response.body["data"]["disabledOrder"]["nodes"],
        json!([
            { "id": first_id, "lastCharacters": "nuqa", "enabled": false, "deactivatedAt": first_deactivated_at },
            { "id": second_id, "lastCharacters": "nuqb", "enabled": false, "deactivatedAt": second_deactivated_at },
            { "id": third_id, "lastCharacters": "nuqc", "enabled": true, "deactivatedAt": null }
        ])
    );
    assert_eq!(
        response.body["data"]["reverseWindow"],
        json!({
            "edges": [
                {
                    "cursor": second_id,
                    "node": {
                        "id": second_id,
                        "lastCharacters": "nuqb",
                        "enabled": false,
                        "deactivatedAt": second_deactivated_at
                    }
                },
                {
                    "cursor": first_id,
                    "node": {
                        "id": first_id,
                        "lastCharacters": "nuqa",
                        "enabled": false,
                        "deactivatedAt": first_deactivated_at
                    }
                }
            ],
            "pageInfo": {
                "hasNextPage": true,
                "hasPreviousPage": false,
                "startCursor": second_id,
                "endCursor": first_id
            }
        })
    );
    assert_eq!(
        response.body["data"]["afterWindow"],
        json!({
            "edges": [{
                "cursor": third_id,
                "node": { "id": third_id, "lastCharacters": "nuqc", "enabled": true, "deactivatedAt": null }
            }],
            "pageInfo": {
                "hasNextPage": false,
                "hasPreviousPage": true,
                "startCursor": third_id,
                "endCursor": third_id
            }
        })
    );
}

#[test]
fn gift_cards_count_honors_limit_precision_after_query_filtering() {
    let mut proxy = snapshot_proxy();
    restore_proxy_state(&mut proxy, |restored| {
        let mut cards = serde_json::Map::new();
        for id in [
            "gid://shopify/GiftCard/100",
            "gid://shopify/GiftCard/200",
            "gid://shopify/GiftCard/300",
        ] {
            cards.insert(id.to_string(), legacy_gift_card_fixture(id));
        }
        restored["state"]["baseState"]["giftCards"] = Value::Object(cards);
    });

    let response = proxy.process_request(json_graphql_request(
        r#"query GiftCardCountPrecision($query: String!) {
          overLimit: giftCardsCount(limit: 2) { count precision }
          exactAtLimit: giftCardsCount(limit: 3) { count precision }
          filteredExact: giftCardsCount(query: $query, limit: 1) { count precision }
          selectedCountOnly: giftCardsCount(limit: 2) { count }
        }"#,
        json!({ "query": "id:200" }),
    ));

    assert_eq!(response.status, 200);
    assert_eq!(
        response.body["data"]["overLimit"],
        json!({ "count": 2, "precision": "AT_LEAST" })
    );
    assert_eq!(
        response.body["data"]["exactAtLimit"],
        json!({ "count": 3, "precision": "EXACT" })
    );
    assert_eq!(
        response.body["data"]["filteredExact"],
        json!({ "count": 1, "precision": "EXACT" })
    );
    assert_eq!(
        response.body["data"]["selectedCountOnly"],
        json!({ "count": 2 })
    );
}

#[test]
fn ordinary_gift_card_live_hybrid_mutation_hydrates_omit_transactions_by_default() {
    let cases = [
        (
            "update",
            r#"mutation GiftCardPlainUpdateHydrateShape($id: ID!) {
              giftCardUpdate(id: $id, input: { note: "plain update" }) {
                giftCard { id note customer { id } }
                userErrors { field message }
              }
            }"#,
        ),
        (
            "deactivate",
            r#"mutation GiftCardPlainDeactivateHydrateShape($id: ID!) {
              giftCardDeactivate(id: $id) {
                giftCard { id enabled }
                userErrors { field message }
              }
            }"#,
        ),
        (
            "customer notification",
            r#"mutation GiftCardPlainCustomerNotificationHydrateShape($id: ID!) {
              giftCardSendNotificationToCustomer(id: $id) {
                giftCard { id customer { id } }
                userErrors { field code message }
              }
            }"#,
        ),
    ];

    for (context, query) in cases {
        let hydrate_query = live_hybrid_gift_card_hydrate_query_for_request(
            query,
            json!({ "id": format!("gid://shopify/GiftCard/plain-{context}") }),
        );
        assert_gift_card_hydrate_omits_transactions(&hydrate_query, context);
    }
}

#[test]
fn gift_card_live_hybrid_mutation_hydrates_transactions_when_payload_selects_them() {
    let hydrate_query = live_hybrid_gift_card_hydrate_query_for_request(
        r#"mutation GiftCardUpdateTransactionOutputHydrateShape($id: ID!) {
          giftCardUpdate(id: $id, input: { note: "transaction output" }) {
            giftCard {
              id
              transactions(first: 2) {
                nodes { id note }
                pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
              }
            }
            userErrors { field message }
          }
        }"#,
        json!({ "id": "gid://shopify/GiftCard/transaction-output" }),
    );

    assert_gift_card_hydrate_includes_transactions(
        &hydrate_query,
        "transaction-selected mutation output",
    );
}

#[test]
fn gift_card_transaction_mutations_hydrate_transactions_for_history_state() {
    let hydrate_query = live_hybrid_gift_card_hydrate_query_for_request(
        r#"mutation GiftCardCreditHydrateShape($id: ID!) {
          giftCardCredit(id: $id, creditInput: { creditAmount: { amount: "2.00", currencyCode: USD } }) {
            giftCardCreditTransaction { id amount { amount currencyCode } }
            userErrors { field code message }
          }
        }"#,
        json!({ "id": "gid://shopify/GiftCard/credit-history" }),
    );

    assert_gift_card_hydrate_includes_transactions(&hydrate_query, "gift-card credit");
}

#[test]
fn gift_card_transaction_read_after_narrow_mutation_hydrate_uses_upstream_window() {
    let upstream_id = "gid://shopify/GiftCard/read-after-narrow";
    let captured_queries = Arc::new(Mutex::new(Vec::new()));
    let captured_for_proxy = Arc::clone(&captured_queries);
    let mut proxy =
        configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport(move |request| {
            let request_body: Value =
                serde_json::from_str(&request.body).expect("upstream body should parse");
            let query = request_body["query"]
                .as_str()
                .expect("upstream request should include query")
                .to_string();
            captured_for_proxy.lock().unwrap().push(query.clone());

            if query.contains("query GiftCardHydrate") {
                let mut gift_card = upstream_gift_card_fixture(upstream_id, "USD");
                gift_card
                    .as_object_mut()
                    .expect("fixture should be an object")
                    .remove("transactions");
                return Response {
                    status: 200,
                    headers: Default::default(),
                    body: json!({
                        "data": {
                            "giftCard": gift_card,
                            "giftCardConfiguration": {
                                "issueLimit": { "amount": "3000.0", "currencyCode": "USD" },
                                "purchaseLimit": { "amount": "14000.0", "currencyCode": "USD" }
                            }
                        }
                    }),
                };
            }

            assert!(
                query.contains("transactions(first: 1)"),
                "transaction read should forward the selected window upstream, got: {query}"
            );
            let mut gift_card = upstream_gift_card_fixture(upstream_id, "USD");
            gift_card["transactions"] = json!({
                "nodes": [{
                    "__typename": "GiftCardCreditTransaction",
                    "id": "gid://shopify/GiftCardCreditTransaction/upstream-1",
                    "note": "upstream credit",
                    "processedAt": "2026-06-03T12:00:00Z",
                    "amount": { "amount": "2.0", "currencyCode": "USD" }
                }],
                "pageInfo": {
                    "hasNextPage": true,
                    "hasPreviousPage": false,
                    "startCursor": "cursor-1",
                    "endCursor": "cursor-1"
                }
            });
            Response {
                status: 200,
                headers: Default::default(),
                body: json!({ "data": { "card": gift_card } }),
            }
        });

    let update = proxy.process_request(json_graphql_request(
        r#"mutation GiftCardNarrowUpdateBeforeTransactionRead($id: ID!) {
          giftCardUpdate(id: $id, input: { note: "local narrow update" }) {
            giftCard { id note }
            userErrors { field message }
          }
        }"#,
        json!({ "id": upstream_id }),
    ));
    assert_eq!(update.status, 200);
    assert_eq!(
        update.body["data"]["giftCardUpdate"]["userErrors"],
        json!([])
    );

    let read = proxy.process_request(json_graphql_request(
        r#"query GiftCardTransactionReadAfterNarrowHydrate($id: ID!) {
          card: giftCard(id: $id) {
            id
            note
            transactions(first: 1) {
              nodes { id note amount { amount currencyCode } }
              pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
            }
          }
        }"#,
        json!({ "id": upstream_id }),
    ));

    assert_eq!(read.status, 200);
    assert_eq!(
        read.body["data"]["card"],
        json!({
            "id": upstream_id,
            "note": "local narrow update",
            "transactions": {
                "nodes": [{
                    "id": "gid://shopify/GiftCardCreditTransaction/upstream-1",
                    "note": "upstream credit",
                    "amount": { "amount": "2.0", "currencyCode": "USD" }
                }],
                "pageInfo": {
                    "hasNextPage": true,
                    "hasPreviousPage": false,
                    "startCursor": "cursor-1",
                    "endCursor": "cursor-1"
                }
            }
        })
    );
    let captured_queries = captured_queries.lock().unwrap();
    assert_eq!(captured_queries.len(), 2);
    assert_gift_card_hydrate_omits_transactions(&captured_queries[0], "initial update");
    assert!(
        captured_queries[1].contains("transactions(first: 1)"),
        "transaction read should preserve caller-selected window, got: {}",
        captured_queries[1]
    );
}

#[test]
fn gift_card_update_hydrates_non_seed_live_hybrid_card_before_staging() {
    let hits = Arc::new(Mutex::new(0usize));
    let hit_counter = Arc::clone(&hits);
    let upstream_id = "gid://shopify/GiftCard/777000333444";
    let mut proxy =
        configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport(move |request| {
            *hit_counter.lock().unwrap() += 1;
            if request.body.contains("GiftCardHydrate") {
                Response {
                    status: 200,
                    headers: Default::default(),
                    body: json!({
                        "data": {
                            "giftCard": upstream_gift_card_fixture(upstream_id, "USD"),
                            "giftCardConfiguration": {
                                "issueLimit": { "amount": "3000.0", "currencyCode": "USD" },
                                "purchaseLimit": { "amount": "14000.0", "currencyCode": "USD" }
                            }
                        }
                    }),
                }
            } else {
                assert!(
                    request.body.contains("giftCards") && request.body.contains("giftCardsCount"),
                    "post-mutation gift-card list/count read should forward upstream, got {}",
                    request.body
                );
                Response {
                    status: 200,
                    headers: Default::default(),
                    body: json!({
                        "data": {
                            "card": {
                                "id": upstream_id,
                                "note": "real upstream note",
                                "balance": { "amount": "25.0", "currencyCode": "USD" }
                            },
                            "cards": {
                                "nodes": [{
                                    "id": upstream_id,
                                    "note": "real upstream note"
                                }],
                                "pageInfo": {
                                    "hasNextPage": false,
                                    "hasPreviousPage": false
                                }
                            },
                            "count": { "count": 1, "precision": "EXACT" },
                            "configuration": {
                                "issueLimit": { "amount": "3000.0", "currencyCode": "USD" }
                            }
                        }
                    }),
                }
            }
        });

    let update = proxy.process_request(json_graphql_request(
        r#"mutation GiftCardUpdateHydratesRealCard($id: ID!) {
          giftCardUpdate(id: $id, input: { note: "updated local note" }) {
            giftCard {
              id
              note
              balance { amount currencyCode }
              customer { id }
            }
            userErrors { field  message }
          }
        }"#,
        json!({ "id": upstream_id }),
    ));

    assert_eq!(update.status, 200);
    assert_eq!(*hits.lock().unwrap(), 1);
    assert_eq!(
        update.body["data"]["giftCardUpdate"],
        json!({
            "giftCard": {
                "id": upstream_id,
                "note": "updated local note",
                "balance": { "amount": "25.0", "currencyCode": "USD" },
                "customer": { "id": "gid://shopify/Customer/424242" }
            },
            "userErrors": []
        })
    );
    assert!(
        !update
            .body
            .to_string()
            .contains("gid://shopify/Customer/10552623464754"),
        "hydrated real-card mutation must not leak the old baked customer id"
    );

    let read = proxy.process_request(json_graphql_request(
        r#"query GiftCardReadOverlaysStagedRealCard($id: ID!, $query: String!) {
          card: giftCard(id: $id) { id note balance { amount currencyCode } }
          cards: giftCards(first: 10, query: $query) { nodes { id note } pageInfo { hasNextPage hasPreviousPage } }
          count: giftCardsCount(query: $query) { count precision }
          configuration: giftCardConfiguration { issueLimit { amount currencyCode } }
        }"#,
        json!({ "id": upstream_id, "query": "id:777000333444" }),
    ));

    assert_eq!(read.status, 200);
    assert_eq!(*hits.lock().unwrap(), 2);
    assert_eq!(
        read.body["data"]["card"],
        json!({
            "id": upstream_id,
            "note": "updated local note",
            "balance": { "amount": "25.0", "currencyCode": "USD" }
        })
    );
    assert_eq!(
        read.body["data"]["cards"]["nodes"],
        json!([{ "id": upstream_id, "note": "updated local note" }])
    );
    assert_eq!(
        read.body["data"]["count"],
        json!({ "count": 1, "precision": "EXACT" })
    );
    assert_eq!(
        read.body["data"]["configuration"]["issueLimit"],
        json!({ "amount": "3000.0", "currencyCode": "USD" })
    );
}

#[test]
fn gift_card_update_hydrates_legacy_seed_id_live_hybrid_card_before_staging() {
    let hits = Arc::new(Mutex::new(0usize));
    let hit_counter = Arc::clone(&hits);
    let upstream_id = "gid://shopify/GiftCard/654773256498";
    let mut proxy =
        configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport(move |request| {
            *hit_counter.lock().unwrap() += 1;
            assert!(
                request.body.contains("GiftCardHydrate"),
                "legacy id mutation should hydrate upstream before staging, got {}",
                request.body
            );
            Response {
                status: 200,
                headers: Default::default(),
                body: json!({
                    "data": {
                        "giftCard": upstream_gift_card_fixture(upstream_id, "USD"),
                        "giftCardConfiguration": {
                            "issueLimit": { "amount": "3000.0", "currencyCode": "USD" },
                            "purchaseLimit": { "amount": "14000.0", "currencyCode": "USD" }
                        }
                    }
                }),
            }
        });

    let update = proxy.process_request(json_graphql_request(
        r#"mutation GiftCardUpdateHydratesFormerSeedCard($id: ID!) {
          giftCardUpdate(id: $id, input: { note: "updated hydrated seed id" }) {
            giftCard {
              id
              note
              balance { amount currencyCode }
              customer { id }
              recipientAttributes { message }
            }
            userErrors { field  message }
          }
        }"#,
        json!({ "id": upstream_id }),
    ));

    assert_eq!(update.status, 200);
    assert_eq!(*hits.lock().unwrap(), 1);
    assert_eq!(
        update.body["data"]["giftCardUpdate"],
        json!({
            "giftCard": {
                "id": upstream_id,
                "note": "updated hydrated seed id",
                "balance": { "amount": "25.0", "currencyCode": "USD" },
                "customer": { "id": "gid://shopify/Customer/424242" },
                "recipientAttributes": null
            },
            "userErrors": []
        })
    );
    assert!(
        !update
            .body
            .to_string()
            .contains("gid://shopify/Customer/10552623464754"),
        "hydrated former seed id must not leak the old baked customer id"
    );
}

#[test]
fn gift_card_configuration_and_create_limit_use_base_configuration() {
    let mut proxy = snapshot_proxy();
    let dump = proxy
        .process_request(request_with_body(
            "POST",
            "/__meta/dump",
            &json!({ "createdAt": "2026-06-16T00:00:00.000Z" }).to_string(),
        ))
        .body;
    let mut restored = dump.clone();
    restored["state"]["baseState"]["shop"]["currencyCode"] = json!("EUR");
    restored["state"]["baseState"]["giftCardConfiguration"] = json!({
        "issueLimit": { "amount": "2500.0", "currencyCode": "EUR" },
        "purchaseLimit": { "amount": "9000.0", "currencyCode": "EUR" }
    });
    let restore = proxy.process_request(request_with_body(
        "POST",
        "/__meta/restore",
        &restored.to_string(),
    ));
    assert_eq!(restore.status, 200);

    let configuration = proxy.process_request(json_graphql_request(
        r#"query GiftCardConfigurationUsesShopCurrency {
          giftCardConfiguration {
            issueLimit { amount currencyCode }
            purchaseLimit { amount currencyCode }
          }
        }"#,
        json!({}),
    ));

    assert_eq!(
        configuration.body["data"]["giftCardConfiguration"],
        json!({
            "issueLimit": { "amount": "2500.0", "currencyCode": "EUR" },
            "purchaseLimit": { "amount": "9000.0", "currencyCode": "EUR" }
        })
    );

    let response = proxy.process_request(json_graphql_request(
        r#"mutation GiftCardLimitUsesShopCurrency {
          createWithinLimit: giftCardCreate(input: { initialValue: "2400" }) {
            giftCard { id balance { amount currencyCode } }
            giftCardCode
            userErrors { field code message }
          }
          createOverLimit: giftCardCreate(input: { initialValue: "2600" }) {
            giftCard { id }
            giftCardCode
            userErrors { field code message }
          }
        }"#,
        json!({}),
    ));

    assert_eq!(
        response.body["data"],
        json!({
            "createWithinLimit": {
                "giftCard": {
                    "id": "gid://shopify/GiftCard/1?shopify-draft-proxy=synthetic",
                    "balance": { "amount": "2400.0", "currencyCode": "EUR" }
                },
                "giftCardCode": "giftcard00000001",
                "userErrors": []
            },
            "createOverLimit": {
                "giftCard": null,
                "giftCardCode": null,
                "userErrors": [{
                    "field": ["input", "initialValue"],
                    "code": "GIFT_CARD_LIMIT_EXCEEDED",
                    "message": "can't exceed $2,500.00 EUR"
                }]
            }
        })
    );
}

#[test]
fn gift_card_transaction_payload_selection_errors_use_request_locations_and_paths() {
    let mut proxy = cad_snapshot_proxy();
    let named_query = r#"mutation RenamedGiftCardTxn {
  aliasedCredit: giftCardCredit(
    id: "gid://shopify/GiftCard/1"
    creditInput: { creditAmount: { amount: "1.00", currencyCode: CAD } }
  ) {
    badGiftCard: giftCard { id }
    userErrors { field code message }
  }
}"#;
    let named = proxy.process_request(json_graphql_request(named_query, json!({})));
    assert_eq!(named.status, 200);
    assert_eq!(
        named.body,
        json!({
            "errors": [{
                "message": "Field 'giftCard' doesn't exist on type 'GiftCardCreditPayload'",
                "locations": query_location(named_query, "badGiftCard: giftCard"),
                "path": ["mutation RenamedGiftCardTxn", "aliasedCredit", "badGiftCard"],
                "extensions": {
                    "code": "undefinedField",
                    "typeName": "GiftCardCreditPayload",
                    "fieldName": "giftCard"
                }
            }]
        })
    );

    let anonymous_query = r#"mutation {
  giftCardDebit(
    id: "gid://shopify/GiftCard/1"
    debitInput: { debitAmount: { amount: "1.00", currencyCode: CAD } }
  ) {
    giftCard { id }
    userErrors { field code message }
  }
}"#;
    let anonymous = proxy.process_request(json_graphql_request(anonymous_query, json!({})));
    assert_eq!(anonymous.status, 200);
    assert_eq!(
        anonymous.body,
        json!({
            "errors": [{
                "message": "Field 'giftCard' doesn't exist on type 'GiftCardDebitPayload'",
                "locations": query_location(anonymous_query, "giftCard { id }"),
                "path": ["mutation", "giftCardDebit", "giftCard"],
                "extensions": {
                    "code": "undefinedField",
                    "typeName": "GiftCardDebitPayload",
                    "fieldName": "giftCard"
                }
            }]
        })
    );
}

#[test]
fn gift_card_recipient_id_errors_use_request_locations_and_paths() {
    let mut proxy = cad_snapshot_proxy();
    let create_query = r#"mutation CustomRecipientCreate {
  createAlias: giftCardCreate(
    input: {
      initialValue: "10"
      recipientAttributes: { preferredName: "A" }
    }
  ) {
    giftCard { id }
    userErrors { field code message }
  }
}"#;
    let create = proxy.process_request(json_graphql_request(create_query, json!({})));
    assert_eq!(create.status, 200);
    assert_eq!(
        create.body,
        json!({
            "errors": [{
                "message": "Argument 'id' on InputObject 'GiftCardRecipientInput' is required. Expected type ID!",
                "locations": query_location(create_query, "{ preferredName"),
                "path": [
                    "mutation CustomRecipientCreate",
                    "createAlias",
                    "input",
                    "recipientAttributes",
                    "id"
                ],
                "extensions": {
                    "code": "missingRequiredInputObjectAttribute",
                    "argumentName": "id",
                    "argumentType": "ID!",
                    "inputObjectType": "GiftCardRecipientInput"
                }
            }]
        })
    );

    let update_query = r#"mutation {
  updateAlias: giftCardUpdate(
    id: "gid://shopify/GiftCard/1"
    input: { recipientAttributes: { message: "Hi" } }
  ) {
    giftCard { id }
    userErrors { field message }
  }
}"#;
    let update = proxy.process_request(json_graphql_request(update_query, json!({})));
    assert_eq!(update.status, 200);
    assert_eq!(
        update.body,
        json!({
            "errors": [{
                "message": "Argument 'id' on InputObject 'GiftCardRecipientInput' is required. Expected type ID!",
                "locations": query_location(update_query, "{ message"),
                "path": ["mutation", "updateAlias", "input", "recipientAttributes", "id"],
                "extensions": {
                    "code": "missingRequiredInputObjectAttribute",
                    "argumentName": "id",
                    "argumentType": "ID!",
                    "inputObjectType": "GiftCardRecipientInput"
                }
            }]
        })
    );
}

#[test]
fn gift_card_create_live_hybrid_hydrates_configuration_before_limit_validation() {
    let hits = Arc::new(Mutex::new(0usize));
    let hit_counter = Arc::clone(&hits);
    let mut proxy =
        configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport(move |request| {
            *hit_counter.lock().unwrap() += 1;
            assert!(
                request.body.contains("GiftCardCreateConfiguration"),
                "create-only gift-card mutation should hydrate configuration before staging, got {}",
                request.body
            );
            assert!(
                request.body.contains("\"variables\":{}"),
                "configuration hydrate should not send synthetic variables, got {}",
                request.body
            );
            Response {
                status: 200,
                headers: Default::default(),
                body: json!({
                    "data": {
                        "giftCardConfiguration": {
                            "issueLimit": { "amount": "5000.0", "currencyCode": "CAD" },
                            "purchaseLimit": { "amount": "14000.0", "currencyCode": "CAD" }
                        }
                    }
                }),
            }
        });

    let response = proxy.process_request(json_graphql_request(
        r#"mutation GiftCardCreateInitialValueLimit {
          aboveFallbackSuccess: giftCardCreate(input: { initialValue: "4000.0" }) {
            giftCard {
              id
              initialValue { amount currencyCode }
              balance { amount currencyCode }
            }
            userErrors { field code message }
          }
          overByCent: giftCardCreate(input: { initialValue: "5000.01" }) {
            giftCard { id }
            userErrors { field code message }
          }
        }"#,
        json!({}),
    ));

    assert_eq!(response.status, 200);
    assert_eq!(*hits.lock().unwrap(), 1);
    assert_eq!(
        response.body["data"],
        json!({
            "aboveFallbackSuccess": {
                "giftCard": {
                    "id": "gid://shopify/GiftCard/1?shopify-draft-proxy=synthetic",
                    "initialValue": { "amount": "4000.0", "currencyCode": "CAD" },
                    "balance": { "amount": "4000.0", "currencyCode": "CAD" }
                },
                "userErrors": []
            },
            "overByCent": {
                "giftCard": null,
                "userErrors": [{
                    "field": ["input", "initialValue"],
                    "code": "GIFT_CARD_LIMIT_EXCEEDED",
                    "message": "can't exceed $5,000.00 CAD"
                }]
            }
        })
    );
}

#[test]
fn gift_card_create_missing_customer_uses_existence_not_id_substring() {
    let mut proxy = snapshot_proxy();

    let response = proxy.process_request(json_graphql_request(
        r#"mutation GiftCardMissingCustomer {
          giftCardCreate(input: { initialValue: "10", customerId: "gid://shopify/Customer/424242" }) {
            giftCard { id customer { id } }
            giftCardCode
            userErrors { field code message }
          }
        }"#,
        json!({}),
    ));

    assert_eq!(
        response.body["data"]["giftCardCreate"],
        json!({
            "giftCard": null,
            "giftCardCode": null,
            "userErrors": [{
                "field": ["input", "customerId"],
                "code": "CUSTOMER_NOT_FOUND",
                "message": "The customer could not be found."
            }]
        })
    );
    assert_eq!(
        state_snapshot(&proxy)["stagedState"]["giftCards"],
        json!({})
    );
}

#[test]
fn gift_card_recipient_notification_checks_actual_contact_projection() {
    let mut proxy = snapshot_proxy();

    let customer = proxy.process_request(json_graphql_request(
        r#"mutation GiftCardNoContactRecipientCustomer {
          customerCreate(input: { firstName: "No", lastName: "Contact" }) {
            customer { id }
            userErrors { field message  }
          }
        }"#,
        json!({}),
    ));
    assert_eq!(
        customer.body["data"]["customerCreate"]["userErrors"],
        json!([])
    );
    let recipient_id = customer.body["data"]["customerCreate"]["customer"]["id"]
        .as_str()
        .expect("recipient customer id")
        .to_string();

    let create = proxy.process_request(json_graphql_request(
        r#"mutation GiftCardRecipientNoContactCreate($recipientId: ID!) {
          create: giftCardCreate(input: { initialValue: "10", recipientAttributes: { id: $recipientId } }) {
            giftCard { id recipientAttributes { recipient { id } } }
            userErrors { field code message }
          }
        }"#,
        json!({ "recipientId": recipient_id }),
    ));

    assert_eq!(create.body["data"]["create"]["userErrors"], json!([]));
    let gift_card_id = json_string(
        &create.body["data"]["create"]["giftCard"]["id"],
        "created gift card id",
    );

    let response = proxy.process_request(json_graphql_request(
        r#"mutation GiftCardRecipientNoContactNotify($id: ID!) {
          notifyRecipient: giftCardSendNotificationToRecipient(id: $id) {
            giftCard { id }
            userErrors { field code message }
          }
        }"#,
        json!({ "id": gift_card_id }),
    ));

    assert_eq!(
        response.body["data"]["notifyRecipient"],
        json!({
            "giftCard": null,
            "userErrors": [{
                "field": null,
                "code": "INVALID",
                "message": "The recipient has no contact information (e.g. email address or phone number)."
            }]
        })
    );
}

#[test]
fn gift_card_customer_notification_checks_actual_contact_projection() {
    let mut proxy = snapshot_proxy();

    let customer = proxy.process_request(json_graphql_request(
        r#"mutation GiftCardNoContactAssignedCustomer {
          customerCreate(input: { firstName: "No", lastName: "Contact" }) {
            customer { id }
            userErrors { field message  }
          }
        }"#,
        json!({}),
    ));
    assert_eq!(
        customer.body["data"]["customerCreate"]["userErrors"],
        json!([])
    );
    let customer_id = customer.body["data"]["customerCreate"]["customer"]["id"]
        .as_str()
        .expect("customer id")
        .to_string();

    let create = proxy.process_request(json_graphql_request(
        r#"mutation GiftCardCustomerNoContactCreate($customerId: ID!) {
          create: giftCardCreate(input: { initialValue: "10", customerId: $customerId }) {
            giftCard { id customer { id } }
            userErrors { field code message }
          }
        }"#,
        json!({ "customerId": customer_id }),
    ));

    assert_eq!(create.body["data"]["create"]["userErrors"], json!([]));
    let gift_card_id = json_string(
        &create.body["data"]["create"]["giftCard"]["id"],
        "created gift card id",
    );

    let response = proxy.process_request(json_graphql_request(
        r#"mutation GiftCardCustomerNoContactNotify($id: ID!) {
          notifyCustomer: giftCardSendNotificationToCustomer(id: $id) {
            giftCard { id }
            userErrors { field code message }
          }
        }"#,
        json!({ "id": gift_card_id }),
    ));

    assert_eq!(
        response.body["data"]["notifyCustomer"],
        json!({
            "giftCard": null,
            "userErrors": [{
                "field": null,
                "code": "INVALID",
                "message": "The customer has no contact information (e.g. email address or phone number)."
            }]
        })
    );
}

#[test]
fn gift_card_customer_notification_allows_reachable_and_unknown_contact_projection() {
    let mut proxy = snapshot_proxy();
    seed_legacy_gift_card_base_state(&mut proxy);

    restore_proxy_state(&mut proxy, |restored| {
        for (tail, customer) in [
            (
                "customer-contact-unknown",
                json!({ "id": "gid://shopify/Customer/customer-contact-unknown" }),
            ),
            (
                "customer-email",
                json!({ "id": "gid://shopify/Customer/customer-email", "email": "customer-email@example.com" }),
            ),
            (
                "customer-phone",
                json!({ "id": "gid://shopify/Customer/customer-phone", "phone": "+14155550100" }),
            ),
            (
                "customer-default-email",
                json!({
                    "id": "gid://shopify/Customer/customer-default-email",
                    "defaultEmailAddress": { "emailAddress": "customer-default-email@example.com" }
                }),
            ),
            (
                "customer-default-phone",
                json!({
                    "id": "gid://shopify/Customer/customer-default-phone",
                    "defaultPhoneNumber": { "phoneNumber": "+14155550101" }
                }),
            ),
        ] {
            let id = format!("gid://shopify/GiftCard/{tail}");
            let mut card = legacy_gift_card_fixture(&id);
            card["customer"] = customer;
            restored["state"]["baseState"]["giftCards"][id] = card;
        }
    });

    let response = proxy.process_request(json_graphql_request(
        r#"mutation GiftCardCustomerReachableProjection {
          unknown: giftCardSendNotificationToCustomer(id: "gid://shopify/GiftCard/customer-contact-unknown") { giftCard { id } userErrors { field code message } }
          email: giftCardSendNotificationToCustomer(id: "gid://shopify/GiftCard/customer-email") { giftCard { id } userErrors { field code message } }
          phone: giftCardSendNotificationToCustomer(id: "gid://shopify/GiftCard/customer-phone") { giftCard { id } userErrors { field code message } }
          defaultEmail: giftCardSendNotificationToCustomer(id: "gid://shopify/GiftCard/customer-default-email") { giftCard { id } userErrors { field code message } }
          defaultPhone: giftCardSendNotificationToCustomer(id: "gid://shopify/GiftCard/customer-default-phone") { giftCard { id } userErrors { field code message } }
        }"#,
        json!({}),
    ));

    assert_eq!(
        response.body["data"],
        json!({
            "unknown": { "giftCard": { "id": "gid://shopify/GiftCard/customer-contact-unknown" }, "userErrors": [] },
            "email": { "giftCard": { "id": "gid://shopify/GiftCard/customer-email" }, "userErrors": [] },
            "phone": { "giftCard": { "id": "gid://shopify/GiftCard/customer-phone" }, "userErrors": [] },
            "defaultEmail": { "giftCard": { "id": "gid://shopify/GiftCard/customer-default-email" }, "userErrors": [] },
            "defaultPhone": { "giftCard": { "id": "gid://shopify/GiftCard/customer-default-phone" }, "userErrors": [] }
        })
    );
}

#[test]
fn gift_card_update_validation_rejects_deactivated_empty_missing_and_long_inputs_and_allows_note() {
    let mut proxy = snapshot_proxy();
    seed_legacy_gift_card_base_state(&mut proxy);

    let response = proxy.process_request(json_graphql_request(
        r#"mutation GiftCardUpdateValidation($activeId: ID!, $deactivatedId: ID!, $missingCustomerId: ID!, $recipientId: ID!, $tooLongPreferredName: String!, $tooLongMessage: String!, $successNote: String!) {
          deactivatedExpiresOn: giftCardUpdate(id: $deactivatedId, input: { expiresOn: "2099-12-31" }) { giftCard { id enabled expiresOn } userErrors { field  message } }
          emptyInput: giftCardUpdate(id: $activeId, input: {}) { giftCard { id note } userErrors { field  message } }
          missingCustomer: giftCardUpdate(id: $activeId, input: { customerId: $missingCustomerId }) { giftCard { id customer { id } } userErrors { field  message } }
          longRecipientName: giftCardUpdate(id: $activeId, input: { recipientAttributes: { id: $recipientId, preferredName: $tooLongPreferredName } }) { giftCard { id recipientAttributes { preferredName recipient { id } } } userErrors { field  message } }
          longRecipientMessage: giftCardUpdate(id: $activeId, input: { recipientAttributes: { id: $recipientId, message: $tooLongMessage } }) { giftCard { id recipientAttributes { message recipient { id } } } userErrors { field  message } }
          success: giftCardUpdate(id: $activeId, input: { note: $successNote }) { giftCard { id note updatedAt } userErrors { field  message } }
        }"#,
        json!({
            "activeId": "gid://shopify/GiftCard/har694-active",
            "deactivatedId": "gid://shopify/GiftCard/har694-deactivated",
            "missingCustomerId": "gid://shopify/Customer/999999999999",
            "recipientId": "gid://shopify/Customer/10582524297522",
            "tooLongPreferredName": "x".repeat(256),
            "tooLongMessage": "x".repeat(201),
            "successNote": "HAR-694 updated note"
        }),
    ));

    assert_eq!(
        response.body["data"],
        json!({
            "deactivatedExpiresOn": { "giftCard": null, "userErrors": [{ "field": ["input", "expiresOn"], "message": "The gift card is deactivated." }] },
            "emptyInput": { "giftCard": null, "userErrors": [{ "field": ["input"], "message": "At least one argument is required in the input." }] },
            "missingCustomer": { "giftCard": null, "userErrors": [{ "field": ["input", "customerId"], "message": "The customer could not be found." }] },
            "longRecipientName": { "giftCard": null, "userErrors": [{ "field": ["input", "recipientAttributes", "preferredName"], "message": "preferredName is too long (maximum is 255)" }] },
            "longRecipientMessage": { "giftCard": null, "userErrors": [{ "field": ["input", "recipientAttributes", "message"], "message": "message is too long (maximum is 200)" }] },
            "success": { "giftCard": { "id": "gid://shopify/GiftCard/har694-active", "note": "HAR-694 updated note", "updatedAt": "2024-01-01T00:00:01.000Z" }, "userErrors": [] }
        })
    );
}

#[test]
fn gift_card_update_noop_accepts_same_values_and_rejects_empty_input() {
    let mut proxy = snapshot_proxy();
    seed_legacy_gift_card_base_state(&mut proxy);

    let response = proxy.process_request(json_graphql_request(
        r#"mutation GiftCardUpdateNoop($id: ID!, $note: String!, $expiresOn: Date!, $templateSuffix: String!) {
          noteNoop: giftCardUpdate(id: $id, input: { note: $note }) { giftCard { id note updatedAt } userErrors { field  message } }
          expiresNoop: giftCardUpdate(id: $id, input: { expiresOn: $expiresOn }) { giftCard { id expiresOn updatedAt } userErrors { field  message } }
          templateNoop: giftCardUpdate(id: $id, input: { templateSuffix: $templateSuffix }) { giftCard { id templateSuffix updatedAt } userErrors { field  message } }
          emptyInput: giftCardUpdate(id: $id, input: {}) { giftCard { id note } userErrors { field  message } }
        }"#,
        json!({
            "id": "gid://shopify/GiftCard/1?shopify-draft-proxy=synthetic",
            "note": "HAR-766 no-op current note",
            "expiresOn": "2030-01-01",
            "templateSuffix": "birthday"
        }),
    ));

    assert_eq!(
        response.body["data"],
        json!({
            "noteNoop": { "giftCard": { "id": "gid://shopify/GiftCard/1?shopify-draft-proxy=synthetic", "note": "HAR-766 no-op current note", "updatedAt": "2024-01-01T00:00:01.000Z" }, "userErrors": [] },
            "expiresNoop": { "giftCard": { "id": "gid://shopify/GiftCard/1?shopify-draft-proxy=synthetic", "expiresOn": "2030-01-01", "updatedAt": "2024-01-01T00:00:01.000Z" }, "userErrors": [] },
            "templateNoop": { "giftCard": { "id": "gid://shopify/GiftCard/1?shopify-draft-proxy=synthetic", "templateSuffix": "birthday", "updatedAt": "2024-01-01T00:00:01.000Z" }, "userErrors": [] },
            "emptyInput": { "giftCard": null, "userErrors": [{ "field": ["input"], "message": "At least one argument is required in the input." }] }
        })
    );
}

#[test]
fn gift_card_create_and_repeated_updates_use_synthetic_clock_timestamps() {
    let mut proxy = snapshot_proxy();

    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation GiftCardSyntheticTimestampCreate {
          giftCardCreate(input: { initialValue: "10", code: "synthetictime" }) {
            giftCard { id createdAt updatedAt }
            userErrors { field code message }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        create.body["data"]["giftCardCreate"]["userErrors"],
        json!([])
    );
    let gift_card = &create.body["data"]["giftCardCreate"]["giftCard"];
    let gift_card_id = json_string(&gift_card["id"], "created gift card id");
    assert_eq!(gift_card["createdAt"], json!("2024-01-01T00:00:01.000Z"));
    assert_eq!(gift_card["updatedAt"], json!("2024-01-01T00:00:01.000Z"));

    let first_update = proxy.process_request(json_graphql_request(
        r#"
        mutation GiftCardSyntheticTimestampFirstUpdate($id: ID!) {
          giftCardUpdate(id: $id, input: { note: "first synthetic timestamp update" }) {
            giftCard { id note createdAt updatedAt }
            userErrors { field  message }
          }
        }
        "#,
        json!({ "id": gift_card_id.clone() }),
    ));
    assert_eq!(
        first_update.body["data"]["giftCardUpdate"]["userErrors"],
        json!([])
    );
    let first_card = &first_update.body["data"]["giftCardUpdate"]["giftCard"];
    assert_eq!(first_card["createdAt"], json!("2024-01-01T00:00:01.000Z"));
    assert_eq!(first_card["updatedAt"], json!("2024-01-01T00:00:02.000Z"));

    let second_update = proxy.process_request(json_graphql_request(
        r#"
        mutation GiftCardSyntheticTimestampSecondUpdate($id: ID!) {
          giftCardUpdate(id: $id, input: { note: "second synthetic timestamp update" }) {
            giftCard { id note createdAt updatedAt }
            userErrors { field  message }
          }
        }
        "#,
        json!({ "id": gift_card_id.clone() }),
    ));
    assert_eq!(
        second_update.body["data"]["giftCardUpdate"]["userErrors"],
        json!([])
    );
    let second_card = &second_update.body["data"]["giftCardUpdate"]["giftCard"];
    assert_eq!(second_card["createdAt"], json!("2024-01-01T00:00:01.000Z"));
    assert_eq!(second_card["updatedAt"], json!("2024-01-01T00:00:03.000Z"));
    assert!(
        second_card["updatedAt"].as_str().unwrap() > first_card["updatedAt"].as_str().unwrap(),
        "giftCardUpdate.updatedAt should advance between writes"
    );

    let read = proxy.process_request(json_graphql_request(
        r#"
        query GiftCardSyntheticTimestampRead($id: ID!) {
          giftCard(id: $id) { id note createdAt updatedAt }
        }
        "#,
        json!({ "id": gift_card_id }),
    ));
    assert_eq!(
        read.body["data"]["giftCard"],
        json!({
            "id": second_card["id"].clone(),
            "note": "second synthetic timestamp update",
            "createdAt": "2024-01-01T00:00:01.000Z",
            "updatedAt": "2024-01-01T00:00:03.000Z"
        })
    );
}

#[test]
fn gift_card_update_deactivated_multi_field_prioritizes_deactivated_errors() {
    let mut proxy = snapshot_proxy();
    seed_legacy_gift_card_base_state(&mut proxy);

    let response = proxy.process_request(json_graphql_request(
        r#"mutation GiftCardUpdateDeactivatedMultiField($deactivatedId: ID!, $customerId: ID!, $recipientId: ID!) {
          expiresAndCustomer: giftCardUpdate(id: $deactivatedId, input: { expiresOn: "2099-12-31", customerId: $customerId }) { giftCard { id } userErrors { field  message } }
          customerAndRecipient: giftCardUpdate(id: $deactivatedId, input: { customerId: $customerId, recipientAttributes: { id: $recipientId } }) { giftCard { id } userErrors { field  message } }
          customerRecipientAndExpires: giftCardUpdate(id: $deactivatedId, input: { customerId: $customerId, recipientAttributes: { id: $recipientId }, expiresOn: "2099-12-31" }) { giftCard { id } userErrors { field  message } }
        }"#,
        json!({
            "deactivatedId": "gid://shopify/GiftCard/deactivated",
            "customerId": "gid://shopify/Customer/1",
            "recipientId": "gid://shopify/Customer/1"
        }),
    ));

    assert_eq!(
        response.body["data"],
        json!({
            "expiresAndCustomer": { "giftCard": null, "userErrors": [{ "field": ["input", "expiresOn"], "message": "The gift card is deactivated." }] },
            "customerAndRecipient": { "giftCard": null, "userErrors": [{ "field": ["input", "customerId"], "message": "The gift card is deactivated." }] },
            "customerRecipientAndExpires": { "giftCard": null, "userErrors": [{ "field": ["input", "expiresOn"], "message": "The gift card is deactivated." }] }
        })
    );
}

#[test]
fn gift_card_trial_shop_assignment_rejects_customer_and_recipient_assignment() {
    let mut proxy = snapshot_proxy();
    seed_legacy_gift_card_base_state(&mut proxy);
    set_gift_card_trial_shop(&mut proxy);

    let response = proxy.process_request(json_graphql_request(
        r#"mutation GiftCardTrialShopAssignment($customerId: ID!, $recipientId: ID!, $updateGiftCardId: ID!) {
          createCustomerAssignment: giftCardCreate(input: { initialValue: "10", customerId: $customerId }) { giftCard { id } giftCardCode userErrors { field code message } }
          createRecipientAssignment: giftCardCreate(input: { initialValue: "10", recipientAttributes: { id: $recipientId } }) { giftCard { id } giftCardCode userErrors { field code message } }
          updateCustomerAssignment: giftCardUpdate(id: $updateGiftCardId, input: { customerId: $customerId }) { giftCard { id } userErrors { field  message } }
          updateRecipientAssignment: giftCardUpdate(id: $updateGiftCardId, input: { recipientAttributes: { id: $recipientId } }) { giftCard { id } userErrors { field  message } }
        }"#,
        json!({
            "customerId": "gid://shopify/Customer/trial-customer",
            "recipientId": "gid://shopify/Customer/trial-recipient",
            "updateGiftCardId": "gid://shopify/GiftCard/trial-update-card"
        }),
    ));

    assert_eq!(
        response.body["data"],
        json!({
            "createCustomerAssignment": { "giftCard": null, "giftCardCode": null, "userErrors": [{ "field": ["input", "customerId"], "code": "INVALID", "message": "A trial shop cannot assign a customer to a gift card." }] },
            "createRecipientAssignment": { "giftCard": null, "giftCardCode": null, "userErrors": [{ "field": ["input", "recipientAttributes"], "code": "INVALID", "message": "A trial shop cannot assign a recipient to a gift card." }] },
            "updateCustomerAssignment": { "giftCard": null, "userErrors": [{ "field": ["input", "customerId"], "message": "A trial shop cannot assign a customer to a gift card." }] },
            "updateRecipientAssignment": { "giftCard": null, "userErrors": [{ "field": ["input", "recipientAttributes"], "message": "A trial shop cannot assign a recipient to a gift card." }] }
        })
    );
}

#[test]
fn gift_card_notification_trial_shop_rejects_customer_and_recipient_notifications() {
    let mut proxy = snapshot_proxy();
    seed_legacy_gift_card_base_state(&mut proxy);
    set_gift_card_trial_shop(&mut proxy);

    let response = proxy.process_request(json_graphql_request(
        r#"mutation GiftCardNotificationTrialShop($id: ID!) {
          customerNotification: giftCardSendNotificationToCustomer(id: $id) { giftCard { id } userErrors { field code message } }
          recipientNotification: giftCardSendNotificationToRecipient(id: $id) { giftCard { id } userErrors { field code message } }
        }"#,
        json!({ "id": "gid://shopify/GiftCard/trial-update-card" }),
    ));

    let trial_error = json!([{
        "field": null,
        "code": "INVALID",
        "message": "Notifications are not available on trial shops."
    }]);
    assert_eq!(
        response.body["data"],
        json!({
            "customerNotification": { "giftCard": null, "userErrors": trial_error },
            "recipientNotification": { "giftCard": null, "userErrors": trial_error }
        })
    );
    assert_eq!(log_snapshot(&proxy)["entries"], json!([]));
}

#[test]
fn gift_card_notification_base_keyed_state_errors_emit_null_field() {
    let mut proxy = snapshot_proxy();
    seed_legacy_gift_card_base_state(&mut proxy);

    let response = proxy.process_request(json_graphql_request(
        r#"mutation GiftCardNotificationBaseKeyedStateErrors {
          noCustomer: giftCardSendNotificationToCustomer(id: "gid://shopify/GiftCard/654904230194") {
            giftCard { id }
            userErrors { field code message }
          }
          noRecipient: giftCardSendNotificationToRecipient(id: "gid://shopify/GiftCard/1?shopify-draft-proxy=synthetic") {
            giftCard { id }
            userErrors { field code message }
          }
          noContact: giftCardSendNotificationToRecipient(id: "gid://shopify/GiftCard/654904262962") {
            giftCard { id }
            userErrors { field code message }
          }
        }"#,
        json!({}),
    ));

    assert_eq!(
        response.body["data"],
        json!({
            "noCustomer": {
                "giftCard": null,
                "userErrors": [{
                    "field": null,
                    "code": "INVALID",
                    "message": "The gift card has no customer."
                }]
            },
            "noRecipient": {
                "giftCard": null,
                "userErrors": [{
                    "field": null,
                    "code": "INVALID",
                    "message": "The gift card has no recipient."
                }]
            },
            "noContact": {
                "giftCard": null,
                "userErrors": [{
                    "field": null,
                    "code": "INVALID",
                    "message": "The recipient has no contact information (e.g. email address or phone number)."
                }]
            }
        })
    );
    assert_eq!(log_snapshot(&proxy)["entries"], json!([]));
}

#[test]
fn gift_card_create_rejects_fabricated_no_contact_recipient_sentinel() {
    let mut proxy = snapshot_proxy();

    let create = proxy.process_request(json_graphql_request(
        r#"mutation GiftCardCreateNoContactRecipientSentinel($recipientId: ID!) {
          giftCardCreate(input: { initialValue: "10", recipientAttributes: { id: $recipientId } }) {
            giftCard { id }
            giftCardCode
            userErrors { field code message }
          }
        }"#,
        json!({ "recipientId": "gid://shopify/Customer/no-contact-recipient" }),
    ));
    assert_eq!(create.status, 200);
    assert_eq!(
        create.body,
        json!({
            "data": {
                "giftCardCreate": null
            },
            "errors": [{
                "message": "Invalid id: gid://shopify/Customer/no-contact-recipient",
                "locations": [{ "line": 2, "column": 11 }],
                "extensions": { "code": "RESOURCE_NOT_FOUND" },
                "path": ["giftCardCreate"]
            }]
        })
    );
    assert_eq!(
        state_snapshot(&proxy)["stagedState"]["giftCards"],
        json!({})
    );
}

#[test]
fn gift_card_notification_entitlement_wins_before_trial_and_trial_wins_before_card_state() {
    let entitlement_error = json!([{ "field": null, "code": null, "message": "Gift cards are unavailable on your plan." }]);
    let trial_error = json!([{
        "field": null,
        "code": "INVALID",
        "message": "Notifications are not available on trial shops."
    }]);

    let mut disabled_proxy = snapshot_proxy();
    seed_legacy_gift_card_base_state(&mut disabled_proxy);
    set_gift_card_trial_shop(&mut disabled_proxy);
    set_gift_cards_unavailable(&mut disabled_proxy);
    let disabled_response = disabled_proxy.process_request(json_graphql_request(
        r#"mutation GiftCardNotificationEntitlementBeforeTrial {
          entitlementBeforeTrial: giftCardSendNotificationToCustomer(id: "gid://shopify/GiftCard/654773256498") { giftCard { id } userErrors { field code message } }
        }"#,
        json!({}),
    ));
    assert_eq!(
        disabled_response.body["data"],
        json!({
            "entitlementBeforeTrial": { "giftCard": null, "userErrors": entitlement_error }
        })
    );
    assert_eq!(log_snapshot(&disabled_proxy)["entries"], json!([]));

    let mut proxy = snapshot_proxy();
    seed_legacy_gift_card_base_state(&mut proxy);
    set_gift_card_trial_shop(&mut proxy);
    let response = proxy.process_request(json_graphql_request(
        r#"mutation GiftCardNotificationTrialPriority {
          trialBeforeMissing: giftCardSendNotificationToCustomer(id: "gid://shopify/GiftCard/not-restored") { giftCard { id } userErrors { field code message } }
          trialBeforeNotifyDisabled: giftCardSendNotificationToCustomer(id: "gid://shopify/GiftCard/1?shopify-draft-proxy=synthetic") { giftCard { id } userErrors { field code message } }
          trialBeforeExpired: giftCardSendNotificationToCustomer(id: "gid://shopify/GiftCard/654808285490") { giftCard { id } userErrors { field code message } }
          trialBeforeDeactivated: giftCardSendNotificationToCustomer(id: "gid://shopify/GiftCard/654808318258") { giftCard { id } userErrors { field code message } }
          trialBeforeNoCustomer: giftCardSendNotificationToCustomer(id: "gid://shopify/GiftCard/654904230194") { giftCard { id } userErrors { field code message } }
          trialBeforeNoContact: giftCardSendNotificationToRecipient(id: "gid://shopify/GiftCard/654904262962") { giftCard { id } userErrors { field code message } }
        }"#,
        json!({}),
    ));
    assert_eq!(
        response.body["data"],
        json!({
            "trialBeforeMissing": { "giftCard": null, "userErrors": trial_error },
            "trialBeforeNotifyDisabled": { "giftCard": null, "userErrors": trial_error },
            "trialBeforeExpired": { "giftCard": null, "userErrors": trial_error },
            "trialBeforeDeactivated": { "giftCard": null, "userErrors": trial_error },
            "trialBeforeNoCustomer": { "giftCard": null, "userErrors": trial_error },
            "trialBeforeNoContact": { "giftCard": null, "userErrors": trial_error }
        })
    );
    assert_eq!(log_snapshot(&proxy)["entries"], json!([]));
}

#[test]
fn gift_card_notification_uses_hydrated_trial_shop_plan() {
    let mut proxy = snapshot_proxy();
    let dump = proxy
        .process_request(request_with_body(
            "POST",
            "/__meta/dump",
            &json!({ "createdAt": "2026-06-16T00:00:00.000Z" }).to_string(),
        ))
        .body;
    let mut restored = dump.clone();
    restored["state"]["baseState"]["shop"]["plan"] = json!({
        "partnerDevelopment": false,
        "publicDisplayName": "Trial",
        "shopifyPlus": false
    });
    let restore = proxy.process_request(request_with_body(
        "POST",
        "/__meta/restore",
        &restored.to_string(),
    ));
    assert_eq!(restore.status, 200);

    let response = proxy.process_request(json_graphql_request(
        r#"mutation GiftCardNotificationTrialPlan($id: ID!) {
          customerNotification: giftCardSendNotificationToCustomer(id: $id) { giftCard { id } userErrors { field code message } }
          recipientNotification: giftCardSendNotificationToRecipient(id: $id) { giftCard { id } userErrors { field code message } }
        }"#,
        json!({ "id": "gid://shopify/GiftCard/654773256498" }),
    ));

    let trial_error = json!([{
        "field": null,
        "code": "INVALID",
        "message": "Notifications are not available on trial shops."
    }]);
    assert_eq!(
        response.body["data"],
        json!({
            "customerNotification": { "giftCard": null, "userErrors": trial_error },
            "recipientNotification": { "giftCard": null, "userErrors": trial_error }
        })
    );
    assert_eq!(log_snapshot(&proxy)["entries"], json!([]));
}

#[test]
fn gift_card_transaction_validation_rejects_state_currency_dates_and_allows_success_credit() {
    let mut proxy = cad_snapshot_proxy();

    let setup = proxy.process_request(json_graphql_request(
        r#"mutation GiftCardTransactionValidationSetup {
          active: giftCardCreate(input: { initialValue: "5", code: "txnactive1" }) {
            giftCard { id }
            userErrors { field code message }
          }
          expired: giftCardCreate(input: { initialValue: "5", code: "txnexpired1", expiresOn: "2020-01-01" }) {
            giftCard { id }
            userErrors { field code message }
          }
          deactivationTarget: giftCardCreate(input: { initialValue: "5", code: "txndeact1" }) {
            giftCard { id }
            userErrors { field code message }
          }
        }"#,
        json!({}),
    ));
    assert_eq!(setup.status, 200);
    assert_eq!(setup.body["data"]["active"]["userErrors"], json!([]));
    assert_eq!(setup.body["data"]["expired"]["userErrors"], json!([]));
    assert_eq!(
        setup.body["data"]["deactivationTarget"]["userErrors"],
        json!([])
    );
    let active_id = json_string(
        &setup.body["data"]["active"]["giftCard"]["id"],
        "active gift card id",
    );
    let expired_id = json_string(
        &setup.body["data"]["expired"]["giftCard"]["id"],
        "expired gift card id",
    );
    let deactivated_id = json_string(
        &setup.body["data"]["deactivationTarget"]["giftCard"]["id"],
        "deactivated gift card id",
    );

    let deactivate = proxy.process_request(json_graphql_request(
        r#"mutation GiftCardTransactionValidationDeactivate($id: ID!) {
          giftCardDeactivate(id: $id) {
            giftCard { id enabled }
            userErrors { field code message }
          }
        }"#,
        json!({ "id": deactivated_id.clone() }),
    ));
    assert_eq!(
        deactivate.body["data"]["giftCardDeactivate"],
        json!({
            "giftCard": { "id": deactivated_id, "enabled": false },
            "userErrors": []
        })
    );

    let response = proxy.process_request(json_graphql_request(
        r#"mutation GiftCardTransactionValidation($activeId: ID!, $expiredId: ID!, $deactivatedId: ID!, $validCreditInput: GiftCardCreditInput!, $mismatchCreditInput: GiftCardCreditInput!, $futureCreditInput: GiftCardCreditInput!, $preEpochCreditInput: GiftCardCreditInput!, $validDebitInput: GiftCardDebitInput!, $futureDebitInput: GiftCardDebitInput!, $preEpochDebitInput: GiftCardDebitInput!) {
          expiredCredit: giftCardCredit(id: $expiredId, creditInput: $validCreditInput) { giftCardCreditTransaction { id __typename processedAt amount { amount currencyCode } } userErrors { field code message } }
          expiredDebit: giftCardDebit(id: $expiredId, debitInput: $validDebitInput) { giftCardDebitTransaction { id __typename processedAt amount { amount currencyCode } } userErrors { field code message } }
          deactivatedCredit: giftCardCredit(id: $deactivatedId, creditInput: $validCreditInput) { giftCardCreditTransaction { id __typename processedAt amount { amount currencyCode } } userErrors { field code message } }
          mismatchCredit: giftCardCredit(id: $activeId, creditInput: $mismatchCreditInput) { giftCardCreditTransaction { id __typename processedAt amount { amount currencyCode } } userErrors { field code message } }
          futureCredit: giftCardCredit(id: $activeId, creditInput: $futureCreditInput) { giftCardCreditTransaction { id __typename processedAt amount { amount currencyCode } } userErrors { field code message } }
          preEpochCredit: giftCardCredit(id: $activeId, creditInput: $preEpochCreditInput) { giftCardCreditTransaction { id __typename processedAt amount { amount currencyCode } } userErrors { field code message } }
          futureDebit: giftCardDebit(id: $activeId, debitInput: $futureDebitInput) { giftCardDebitTransaction { id __typename processedAt amount { amount currencyCode } } userErrors { field code message } }
          preEpochDebit: giftCardDebit(id: $activeId, debitInput: $preEpochDebitInput) { giftCardDebitTransaction { id __typename processedAt amount { amount currencyCode } } userErrors { field code message } }
          deactivatedDebit: giftCardDebit(id: $deactivatedId, debitInput: $validDebitInput) { giftCardDebitTransaction { id __typename processedAt amount { amount currencyCode } } userErrors { field code message } }
          successCredit: giftCardCredit(id: $activeId, creditInput: $validCreditInput) { giftCardCreditTransaction { id __typename processedAt amount { amount currencyCode } } userErrors { field code message } }
        }"#,
        json!({
            "activeId": active_id,
            "expiredId": expired_id,
            "deactivatedId": deactivated_id,
            "validCreditInput": { "creditAmount": { "amount": "5.00", "currencyCode": "CAD" } },
            "mismatchCreditInput": { "creditAmount": { "amount": "5.00", "currencyCode": "EUR" } },
            "futureCreditInput": { "processedAt": "2030-01-01T00:00:00Z", "creditAmount": { "amount": "5.00", "currencyCode": "CAD" } },
            "preEpochCreditInput": { "processedAt": "1960-01-01T00:00:00Z", "creditAmount": { "amount": "5.00", "currencyCode": "CAD" } },
            "validDebitInput": { "debitAmount": { "amount": "5.00", "currencyCode": "CAD" } },
            "futureDebitInput": { "processedAt": "2030-01-01T00:00:00Z", "debitAmount": { "amount": "5.00", "currencyCode": "CAD" } },
            "preEpochDebitInput": { "processedAt": "1960-01-01T00:00:00Z", "debitAmount": { "amount": "5.00", "currencyCode": "CAD" } }
        }),
    ));

    assert_eq!(
        response.body["data"],
        json!({
            "expiredCredit": { "giftCardCreditTransaction": null, "userErrors": [{ "field": ["id"], "code": "INVALID", "message": "The gift card has expired." }] },
            "expiredDebit": { "giftCardDebitTransaction": null, "userErrors": [{ "field": ["id"], "code": "INVALID", "message": "The gift card has expired." }] },
            "deactivatedCredit": { "giftCardCreditTransaction": null, "userErrors": [{ "field": ["id"], "code": "INVALID", "message": "The gift card is deactivated." }] },
            "mismatchCredit": { "giftCardCreditTransaction": null, "userErrors": [{ "field": ["creditInput", "creditAmount", "currencyCode"], "code": "MISMATCHING_CURRENCY", "message": "The currency provided does not match the currency of the gift card." }] },
            "futureCredit": { "giftCardCreditTransaction": null, "userErrors": [{ "field": ["creditInput", "processedAt"], "code": "INVALID", "message": "The processed date must not be in the future." }] },
            "preEpochCredit": { "giftCardCreditTransaction": null, "userErrors": [{ "field": ["creditInput", "processedAt"], "code": "INVALID", "message": "A valid processed date must be used." }] },
            "futureDebit": { "giftCardDebitTransaction": null, "userErrors": [{ "field": ["debitInput", "processedAt"], "code": "INVALID", "message": "The processed date must not be in the future." }] },
            "preEpochDebit": { "giftCardDebitTransaction": null, "userErrors": [{ "field": ["debitInput", "processedAt"], "code": "INVALID", "message": "A valid processed date must be used." }] },
            "deactivatedDebit": { "giftCardDebitTransaction": null, "userErrors": [{ "field": ["id"], "code": "INVALID", "message": "The gift card is deactivated." }] },
            "successCredit": { "giftCardCreditTransaction": { "id": "gid://shopify/GiftCardCreditTransaction/4", "__typename": "GiftCardCreditTransaction", "processedAt": "2024-01-01T00:00:03.000Z", "amount": { "amount": "5.0", "currencyCode": "CAD" } }, "userErrors": [] }
        })
    );

    let read = proxy.process_request(json_graphql_request(
        r#"query GiftCardTransactionValidationRead($id: ID!) {
          giftCard(id: $id) {
            balance { amount currencyCode }
            transactions(first: 5) {
              nodes { processedAt amount { amount currencyCode } }
            }
          }
        }"#,
        json!({ "id": active_id }),
    ));
    assert_eq!(
        read.body["data"]["giftCard"],
        json!({
            "balance": { "amount": "10.0", "currencyCode": "CAD" },
            "transactions": {
                "nodes": [{
                    "processedAt": "2024-01-01T00:00:03.000Z",
                    "amount": { "amount": "5.0", "currencyCode": "CAD" }
                }]
            }
        })
    );
}

#[test]
fn gift_card_recipient_validation_rejects_length_html_and_send_at_bounds() {
    let mut proxy = snapshot_proxy_with_gift_card_fixed_validation_clock();
    seed_legacy_gift_card_base_state(&mut proxy);

    let setup_customer = proxy.process_request(json_graphql_request(
        r#"mutation GiftCardRecipientValidationCustomer {
          customerCreate(input: { firstName: "Gift", lastName: "Recipient", email: "gift-recipient-validation@example.com" }) {
            customer { id }
            userErrors { field message  }
          }
        }"#,
        json!({}),
    ));
    assert_eq!(setup_customer.status, 200);
    assert_eq!(
        setup_customer.body["data"]["customerCreate"]["userErrors"],
        json!([])
    );
    let recipient_id = setup_customer.body["data"]["customerCreate"]["customer"]["id"]
        .as_str()
        .expect("setup customer id")
        .to_string();

    let response = proxy.process_request(json_graphql_request(
        r#"mutation GiftCardRecipientValidation(
          $activeId: ID!,
          $recipientId: ID!,
          $tooLongPreferredName: String!,
          $tooLongMessage: String!,
          $htmlPreferredName: String!,
          $htmlMessage: String!,
          $futureSendAt: DateTime!,
          $pastSendAt: DateTime!,
          $validSendAt: DateTime!
        ) {
          createLongPreferredName: giftCardCreate(input: { initialValue: "10", recipientAttributes: { id: $recipientId, preferredName: $tooLongPreferredName } }) { giftCard { id recipientAttributes { preferredName } } giftCardCode userErrors { field code message } }
          createLongMessage: giftCardCreate(input: { initialValue: "10", recipientAttributes: { id: $recipientId, message: $tooLongMessage } }) { giftCard { id recipientAttributes { message } } giftCardCode userErrors { field code message } }
          createHtmlPreferredName: giftCardCreate(input: { initialValue: "10", recipientAttributes: { id: $recipientId, preferredName: $htmlPreferredName } }) { giftCard { id recipientAttributes { preferredName } } giftCardCode userErrors { field code message } }
          createHtmlMessage: giftCardCreate(input: { initialValue: "10", recipientAttributes: { id: $recipientId, message: $htmlMessage } }) { giftCard { id recipientAttributes { message } } giftCardCode userErrors { field code message } }
          createFutureSendAt: giftCardCreate(input: { initialValue: "10", recipientAttributes: { id: $recipientId, sendNotificationAt: $futureSendAt } }) { giftCard { id recipientAttributes { sendNotificationAt } } giftCardCode userErrors { field code message } }
          createPastSendAt: giftCardCreate(input: { initialValue: "10", recipientAttributes: { id: $recipientId, sendNotificationAt: $pastSendAt } }) { giftCard { id recipientAttributes { sendNotificationAt } } giftCardCode userErrors { field code message } }
          createValidSendAt: giftCardCreate(input: { initialValue: "10", recipientAttributes: { id: $recipientId, sendNotificationAt: $validSendAt } }) { giftCard { id recipientAttributes { sendNotificationAt } } giftCardCode userErrors { field code message } }
          updateLongPreferredName: giftCardUpdate(id: $activeId, input: { recipientAttributes: { id: $recipientId, preferredName: $tooLongPreferredName } }) { giftCard { id recipientAttributes { preferredName } } userErrors { field  message } }
          updateLongMessage: giftCardUpdate(id: $activeId, input: { recipientAttributes: { id: $recipientId, message: $tooLongMessage } }) { giftCard { id recipientAttributes { message } } userErrors { field  message } }
          updateHtmlPreferredName: giftCardUpdate(id: $activeId, input: { recipientAttributes: { id: $recipientId, preferredName: $htmlPreferredName } }) { giftCard { id recipientAttributes { preferredName } } userErrors { field  message } }
          updateHtmlMessage: giftCardUpdate(id: $activeId, input: { recipientAttributes: { id: $recipientId, message: $htmlMessage } }) { giftCard { id recipientAttributes { message } } userErrors { field  message } }
          updatePastSendAt: giftCardUpdate(id: $activeId, input: { recipientAttributes: { id: $recipientId, sendNotificationAt: $pastSendAt } }) { giftCard { id recipientAttributes { sendNotificationAt } } userErrors { field  message } }
          updateFutureSendAt: giftCardUpdate(id: $activeId, input: { recipientAttributes: { id: $recipientId, sendNotificationAt: $futureSendAt } }) { giftCard { id recipientAttributes { sendNotificationAt } } userErrors { field  message } }
          updateValidSendAt: giftCardUpdate(id: $activeId, input: { recipientAttributes: { id: $recipientId, sendNotificationAt: $validSendAt } }) { giftCard { id recipientAttributes { sendNotificationAt } } userErrors { field  message } }
        }"#,
        json!({
            "activeId": "gid://shopify/GiftCard/1?shopify-draft-proxy=synthetic",
            "recipientId": recipient_id,
            "tooLongPreferredName": "x".repeat(256),
            "tooLongMessage": "x".repeat(201),
            "htmlPreferredName": "<b>Recipient</b>",
            "htmlMessage": "<script>alert(1)</script>",
            "futureSendAt": "2026-10-01T00:00:00Z",
            "pastSendAt": "2026-04-28T09:31:02Z",
            "validSendAt": "2026-07-01T00:00:00Z"
        }),
    ));

    assert_eq!(
        response.body["data"],
        json!({
            "createLongPreferredName": { "giftCard": null, "userErrors": [{ "field": ["input", "recipientAttributes", "preferredName"], "code": "TOO_LONG", "message": "preferredName is too long (maximum is 255)" }], "giftCardCode": null },
            "createLongMessage": { "giftCard": null, "userErrors": [{ "field": ["input", "recipientAttributes", "message"], "code": "TOO_LONG", "message": "message is too long (maximum is 200)" }], "giftCardCode": null },
            "createHtmlPreferredName": { "giftCard": null, "userErrors": [{ "field": ["input", "recipientAttributes", "preferredName"], "code": "INVALID", "message": "Preferred name cannot contain HTML tags" }], "giftCardCode": null },
            "createHtmlMessage": { "giftCard": null, "userErrors": [{ "field": ["input", "recipientAttributes", "message"], "code": "INVALID", "message": "Message cannot contain HTML tags" }], "giftCardCode": null },
            "createFutureSendAt": { "giftCard": null, "userErrors": [{ "field": ["input", "recipientAttributes", "sendNotificationAt"], "code": "INVALID", "message": "Send notification at must be within 90 days from now" }], "giftCardCode": null },
            "createPastSendAt": { "giftCard": null, "userErrors": [{ "field": ["input", "recipientAttributes", "sendNotificationAt"], "code": "INVALID", "message": "Send notification at must be within 90 days from now" }], "giftCardCode": null },
            "createValidSendAt": { "giftCard": { "id": "gid://shopify/GiftCard/2?shopify-draft-proxy=synthetic", "recipientAttributes": { "sendNotificationAt": "2026-07-01T00:00:00Z" } }, "giftCardCode": "giftcard00000002", "userErrors": [] },
            "updateLongPreferredName": { "giftCard": null, "userErrors": [{ "field": ["input", "recipientAttributes", "preferredName"], "message": "preferredName is too long (maximum is 255)" }] },
            "updateLongMessage": { "giftCard": null, "userErrors": [{ "field": ["input", "recipientAttributes", "message"], "message": "message is too long (maximum is 200)" }] },
            "updateHtmlPreferredName": { "giftCard": null, "userErrors": [{ "field": ["input", "recipientAttributes", "preferredName"], "message": "Preferred name cannot contain HTML tags" }] },
            "updateHtmlMessage": { "giftCard": null, "userErrors": [{ "field": ["input", "recipientAttributes", "message"], "message": "Message cannot contain HTML tags" }] },
            "updatePastSendAt": { "giftCard": null, "userErrors": [{ "field": ["input", "recipientAttributes", "sendNotificationAt"], "message": "Send notification at must be within 90 days from now" }] },
            "updateFutureSendAt": { "giftCard": null, "userErrors": [{ "field": ["input", "recipientAttributes", "sendNotificationAt"], "message": "Send notification at must be within 90 days from now" }] },
            "updateValidSendAt": { "giftCard": { "id": "gid://shopify/GiftCard/1?shopify-draft-proxy=synthetic", "recipientAttributes": { "sendNotificationAt": "2026-07-01T00:00:00Z" } }, "userErrors": [] }
        })
    );
}

#[test]
fn gift_card_date_validation_boundaries_follow_the_proxy_clock() {
    let clock = Arc::new(Mutex::new(utc_time(1_780_185_600)));
    let mut proxy = snapshot_proxy_with_clock(Arc::clone(&clock));
    restore_shop_currency(&mut proxy, "CAD");

    let setup_customer = proxy.process_request(json_graphql_request(
        r#"mutation GiftCardClockValidationCustomer {
          customerCreate(input: { firstName: "Clock", lastName: "Boundary", email: "gift-card-clock-boundary@example.com" }) {
            customer { id }
            userErrors { field message }
          }
        }"#,
        json!({}),
    ));
    assert_eq!(
        setup_customer.body["data"]["customerCreate"]["userErrors"],
        json!([])
    );
    let recipient_id = json_string(
        &setup_customer.body["data"]["customerCreate"]["customer"]["id"],
        "clock validation customer id",
    );

    let setup_card = proxy.process_request(json_graphql_request(
        r#"mutation GiftCardClockValidationActive {
          giftCardCreate(input: { initialValue: "10", code: "clockproc" }) {
            giftCard { id }
            userErrors { field code message }
          }
        }"#,
        json!({}),
    ));
    assert_eq!(
        setup_card.body["data"]["giftCardCreate"]["userErrors"],
        json!([])
    );
    let active_id = json_string(
        &setup_card.body["data"]["giftCardCreate"]["giftCard"]["id"],
        "clock validation active gift card id",
    );

    let processed_at_input = json!({
        "processedAt": "2026-06-01T00:00:00Z",
        "creditAmount": { "amount": "1.00", "currencyCode": "CAD" }
    });
    let future_processed_at = proxy.process_request(json_graphql_request(
        r#"mutation GiftCardProcessedAtClock($id: ID!, $input: GiftCardCreditInput!) {
          giftCardCredit(id: $id, creditInput: $input) {
            giftCardCreditTransaction { processedAt }
            userErrors { field code message }
          }
        }"#,
        json!({ "id": active_id.clone(), "input": processed_at_input.clone() }),
    ));
    assert_eq!(
        future_processed_at.body["data"]["giftCardCredit"],
        json!({
            "giftCardCreditTransaction": null,
            "userErrors": [{
                "field": ["creditInput", "processedAt"],
                "code": "INVALID",
                "message": "The processed date must not be in the future."
            }]
        })
    );

    let send_at_input = "2026-08-30T00:00:00Z";
    let too_far_send_at = proxy.process_request(json_graphql_request(
        r#"mutation GiftCardSendAtClock($recipientId: ID!, $sendAt: DateTime!) {
          giftCardCreate(input: { initialValue: "10", code: "clocksendtoo", recipientAttributes: { id: $recipientId, sendNotificationAt: $sendAt } }) {
            giftCard { id recipientAttributes { sendNotificationAt } }
            giftCardCode
            userErrors { field code message }
          }
        }"#,
        json!({ "recipientId": recipient_id.clone(), "sendAt": send_at_input }),
    ));
    assert_eq!(
        too_far_send_at.body["data"]["giftCardCreate"],
        json!({
            "giftCard": null,
            "giftCardCode": null,
            "userErrors": [{
                "field": ["input", "recipientAttributes", "sendNotificationAt"],
                "code": "INVALID",
                "message": "Send notification at must be within 90 days from now"
            }]
        })
    );

    set_clock(&clock, 1_780_272_000);
    let accepted_send_at = proxy.process_request(json_graphql_request(
        r#"mutation GiftCardSendAtClock($recipientId: ID!, $sendAt: DateTime!) {
          giftCardCreate(input: { initialValue: "10", code: "clocksendok", recipientAttributes: { id: $recipientId, sendNotificationAt: $sendAt } }) {
            giftCard { id recipientAttributes { sendNotificationAt } }
            giftCardCode
            userErrors { field code message }
          }
        }"#,
        json!({ "recipientId": recipient_id, "sendAt": send_at_input }),
    ));
    assert_eq!(
        accepted_send_at.body["data"]["giftCardCreate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        accepted_send_at.body["data"]["giftCardCreate"]["giftCard"]["recipientAttributes"]
            ["sendNotificationAt"],
        json!(send_at_input)
    );

    let expiring_setup = proxy.process_request(json_graphql_request(
        r#"mutation GiftCardExpiryClockSetup {
          giftCardCreate(input: { initialValue: "10", code: "clockexpiry", expiresOn: "2026-06-01" }) {
            giftCard { id expiresOn }
            userErrors { field code message }
          }
        }"#,
        json!({}),
    ));
    assert_eq!(
        expiring_setup.body["data"]["giftCardCreate"]["userErrors"],
        json!([])
    );
    let expiring_id = json_string(
        &expiring_setup.body["data"]["giftCardCreate"]["giftCard"]["id"],
        "clock validation expiring gift card id",
    );
    let active_on_expiry_day = proxy.process_request(json_graphql_request(
        r#"mutation GiftCardExpiryClockActive($id: ID!) {
          giftCardCredit(id: $id, creditInput: { creditAmount: { amount: "1.00", currencyCode: CAD } }) {
            giftCardCreditTransaction { __typename }
            userErrors { field code message }
          }
        }"#,
        json!({ "id": expiring_id.clone() }),
    ));
    assert_eq!(
        active_on_expiry_day.body["data"]["giftCardCredit"],
        json!({
            "giftCardCreditTransaction": { "__typename": "GiftCardCreditTransaction" },
            "userErrors": []
        })
    );

    set_clock(&clock, 1_780_358_400);
    let accepted_processed_at = proxy.process_request(json_graphql_request(
        r#"mutation GiftCardProcessedAtClock($id: ID!, $input: GiftCardCreditInput!) {
          giftCardCredit(id: $id, creditInput: $input) {
            giftCardCreditTransaction { processedAt amount { amount currencyCode } }
            userErrors { field code message }
          }
        }"#,
        json!({ "id": active_id, "input": processed_at_input }),
    ));
    assert_eq!(
        accepted_processed_at.body["data"]["giftCardCredit"],
        json!({
            "giftCardCreditTransaction": {
                "processedAt": "2026-06-01T00:00:00Z",
                "amount": { "amount": "1.0", "currencyCode": "CAD" }
            },
            "userErrors": []
        })
    );

    let expired_after_clock_advance = proxy.process_request(json_graphql_request(
        r#"mutation GiftCardExpiryClockExpired($id: ID!) {
          giftCardDebit(id: $id, debitInput: { debitAmount: { amount: "1.00", currencyCode: CAD } }) {
            giftCardDebitTransaction { __typename }
            userErrors { field code message }
          }
        }"#,
        json!({ "id": expiring_id }),
    ));
    assert_eq!(
        expired_after_clock_advance.body["data"]["giftCardDebit"],
        json!({
            "giftCardDebitTransaction": null,
            "userErrors": [{
                "field": ["id"],
                "code": "INVALID",
                "message": "The gift card has expired."
            }]
        })
    );
}

#[test]
fn gift_card_recipient_validation_rejects_unknown_recipient_and_blank_text_fields() {
    let mut proxy = snapshot_proxy();
    seed_legacy_gift_card_base_state(&mut proxy);

    let setup_customer = proxy.process_request(json_graphql_request(
        r#"mutation GiftCardRecipientValidationCustomer {
          customerCreate(input: { firstName: "Gift", lastName: "Recipient", email: "gift-recipient@example.com" }) {
            customer { id }
            userErrors { field message  }
          }
        }"#,
        json!({}),
    ));
    assert_eq!(setup_customer.status, 200);
    assert_eq!(
        setup_customer.body["data"]["customerCreate"]["userErrors"],
        json!([])
    );
    let recipient_id = setup_customer.body["data"]["customerCreate"]["customer"]["id"]
        .as_str()
        .expect("setup customer id")
        .to_string();

    let response = proxy.process_request(json_graphql_request(
        r#"mutation GiftCardRecipientPresence($activeId: ID!, $recipientId: ID!, $missingRecipientId: ID!) {
          createUnknownRecipient: giftCardCreate(input: { initialValue: "10", recipientAttributes: { id: $missingRecipientId } }) {
            giftCard { id }
            giftCardCode
            userErrors { field code message }
          }
          updateUnknownRecipient: giftCardUpdate(id: $activeId, input: { recipientAttributes: { id: $missingRecipientId } }) {
            giftCard { id recipientAttributes { recipient { id } } }
            userErrors { field  message }
          }
          createBlankPreferredName: giftCardCreate(input: { initialValue: "10", recipientAttributes: { id: $recipientId, preferredName: "" } }) {
            giftCard { id recipientAttributes { preferredName } }
            giftCardCode
            userErrors { field code message }
          }
          createBlankMessage: giftCardCreate(input: { initialValue: "10", recipientAttributes: { id: $recipientId, message: "" } }) {
            giftCard { id recipientAttributes { message } }
            giftCardCode
            userErrors { field code message }
          }
          updateBlankPreferredName: giftCardUpdate(id: $activeId, input: { recipientAttributes: { id: $recipientId, preferredName: "" } }) {
            giftCard { id recipientAttributes { preferredName } }
            userErrors { field  message }
          }
          updateBlankMessage: giftCardUpdate(id: $activeId, input: { recipientAttributes: { id: $recipientId, message: "" } }) {
            giftCard { id recipientAttributes { message } }
            userErrors { field  message }
          }
        }"#,
        json!({
            "activeId": "gid://shopify/GiftCard/1?shopify-draft-proxy=synthetic",
            "recipientId": recipient_id,
            "missingRecipientId": "gid://shopify/Customer/999999999999"
        }),
    ));

    let recipient_not_found = json!([{
        "field": ["input", "recipientAttributes", "id"],
        "code": "RECIPIENT_NOT_FOUND",
        "message": "Recipient could not be found"
    }]);
    let recipient_not_found_plain = json!([{
        "field": ["input", "recipientAttributes", "id"],
        "message": "Recipient could not be found"
    }]);
    let blank_preferred_name = json!([{
        "field": ["input", "recipientAttributes", "preferredName"],
        "code": "INVALID",
        "message": "Preferred name can't be blank"
    }]);
    let blank_preferred_name_plain = json!([{
        "field": ["input", "recipientAttributes", "preferredName"],
        "message": "Preferred name can't be blank"
    }]);
    let blank_message = json!([{
        "field": ["input", "recipientAttributes", "message"],
        "code": "INVALID",
        "message": "Message can't be blank"
    }]);
    let blank_message_plain = json!([{
        "field": ["input", "recipientAttributes", "message"],
        "message": "Message can't be blank"
    }]);
    assert_eq!(
        response.body["data"],
        json!({
            "createUnknownRecipient": { "giftCard": null, "giftCardCode": null, "userErrors": recipient_not_found },
            "updateUnknownRecipient": { "giftCard": null, "userErrors": recipient_not_found_plain },
            "createBlankPreferredName": { "giftCard": null, "giftCardCode": null, "userErrors": blank_preferred_name },
            "createBlankMessage": { "giftCard": null, "giftCardCode": null, "userErrors": blank_message },
            "updateBlankPreferredName": { "giftCard": null, "userErrors": blank_preferred_name_plain },
            "updateBlankMessage": { "giftCard": null, "userErrors": blank_message_plain }
        })
    );

    let read_after_rejections = proxy.process_request(json_graphql_request(
        r#"query GiftCardRecipientPresenceRead($activeId: ID!) {
          giftCard(id: $activeId) {
            id
            recipientAttributes { recipient { id } preferredName message }
          }
          giftCards(first: 10, query: "id:999999999999") { nodes { id recipientAttributes { recipient { id } } } }
        }"#,
        json!({ "activeId": "gid://shopify/GiftCard/1?shopify-draft-proxy=synthetic" }),
    ));
    assert_eq!(
        read_after_rejections.body["data"]["giftCard"]["recipientAttributes"],
        Value::Null
    );
    assert_eq!(
        read_after_rejections.body["data"]["giftCards"]["nodes"],
        json!([])
    );
}

#[test]
fn gift_card_mutation_user_error_codes_cover_create_update_credit_and_debit_paths() {
    let mut proxy = cad_snapshot_proxy();

    let response = proxy.process_request(json_graphql_request(
        r#"mutation GiftCardMutationUserErrorCodes {
          setupSmallBalance: giftCardCreate(input: { initialValue: "5", code: "har686smallcard" }) { giftCard { id } userErrors { field code message } }
          zeroInitialValue: giftCardCreate(input: { initialValue: "0" }) { giftCard { id } userErrors { field code message } }
          missingUpdate: giftCardUpdate(id: "gid://shopify/GiftCard/9999999", input: { note: "x" }) { giftCard { id } userErrors { field  message } }
          negativeCredit: giftCardCredit(id: "gid://shopify/GiftCard/1?shopify-draft-proxy=synthetic", creditInput: { creditAmount: { amount: "-1", currencyCode: "CAD" } }) { giftCardCreditTransaction { id } userErrors { field code message } }
          insufficientDebit: giftCardDebit(id: "gid://shopify/GiftCard/1?shopify-draft-proxy=synthetic", debitInput: { debitAmount: { amount: "9999", currencyCode: "CAD" } }) { giftCardDebitTransaction { id } userErrors { field code message } }
        }"#,
        json!({}),
    ));

    assert_eq!(
        response.body["data"],
        json!({
            "setupSmallBalance": { "giftCard": { "id": "gid://shopify/GiftCard/1?shopify-draft-proxy=synthetic" }, "userErrors": [] },
            "zeroInitialValue": {
                "giftCard": null,
                "userErrors": [{ "field": ["input", "initialValue"], "code": "GREATER_THAN", "message": "must be greater than 0" }]
            },
            "missingUpdate": {
                "giftCard": null,
                "userErrors": [{ "field": ["id"], "message": "The gift card could not be found." }]
            },
            "negativeCredit": {
                "giftCardCreditTransaction": null,
                "userErrors": [{ "field": ["creditInput", "creditAmount", "amount"], "code": "NEGATIVE_OR_ZERO_AMOUNT", "message": "A positive amount must be used." }]
            },
            "insufficientDebit": {
                "giftCardDebitTransaction": null,
                "userErrors": [{ "field": ["debitInput", "debitAmount", "amount"], "code": "INSUFFICIENT_FUNDS", "message": "The gift card does not have sufficient funds to satisfy the request." }]
            }
        })
    );
}

#[test]
fn gift_card_create_validation_is_input_driven_under_ordinary_operation_name() {
    let mut proxy = cad_snapshot_proxy();

    let response = proxy.process_request(json_graphql_request(
        r#"mutation IssueGiftCards($validCode: String!, $tooLongCode: String!, $missingCustomerId: ID!) {
          zeroInitialValue: giftCardCreate(input: { initialValue: "0" }) {
            giftCard { id }
            giftCardCode
            userErrors { field code message }
          }
          shortCode: giftCardCreate(input: { initialValue: "10", code: "abc" }) {
            giftCard { id }
            giftCardCode
            userErrors { field code message }
          }
          longCode: giftCardCreate(input: { initialValue: "10", code: $tooLongCode }) {
            giftCard { id }
            giftCardCode
            userErrors { field code message }
          }
          invalidCode: giftCardCreate(input: { initialValue: "10", code: "bad!code" }) {
            giftCard { id }
            giftCardCode
            userErrors { field code message }
          }
          shortCodeMissingCustomer: giftCardCreate(input: { initialValue: "10", code: "abc", customerId: $missingCustomerId }) {
            giftCard { id }
            giftCardCode
            userErrors { field code message }
          }
          missingCustomer: giftCardCreate(input: { initialValue: "10", customerId: $missingCustomerId }) {
            giftCard { id }
            giftCardCode
            userErrors { field code message }
          }
          success: giftCardCreate(input: { initialValue: "10", code: $validCode }) {
            giftCard { id lastCharacters maskedCode initialValue { amount currencyCode } balance { amount currencyCode } }
            giftCardCode
            userErrors { field code message }
          }
          duplicate: giftCardCreate(input: { initialValue: "10", code: $validCode }) {
            giftCard { id }
            giftCardCode
            userErrors { field code message }
          }
          autoGenerated: giftCardCreate(input: { initialValue: "10" }) {
            giftCard { id lastCharacters maskedCode initialValue { amount currencyCode } balance { amount currencyCode } }
            giftCardCode
            userErrors { field code message }
          }
        }"#,
        json!({
            "validCode": "ParityOkMowpZlrz",
            "tooLongCode": "x".repeat(21),
            "missingCustomerId": "gid://shopify/Customer/999999999"
        }),
    ));

    assert_eq!(response.status, 200);
    assert_eq!(
        response.body["data"],
        json!({
            "zeroInitialValue": {
                "giftCard": null,
                "giftCardCode": null,
                "userErrors": [{ "field": ["input", "initialValue"], "code": "GREATER_THAN", "message": "must be greater than 0" }]
            },
            "shortCode": {
                "giftCard": null,
                "giftCardCode": null,
                "userErrors": [{ "field": ["input", "code"], "code": "TOO_SHORT", "message": "Code must be at least 8 characters long" }]
            },
            "longCode": {
                "giftCard": null,
                "giftCardCode": null,
                "userErrors": [{ "field": ["input", "code"], "code": "TOO_LONG", "message": "Code must be at most 20 characters long" }]
            },
            "invalidCode": {
                "giftCard": null,
                "giftCardCode": null,
                "userErrors": [{ "field": ["input", "code"], "code": "INVALID", "message": "Code can only contain letters(a-z) and numbers(0-9)" }]
            },
            "shortCodeMissingCustomer": {
                "giftCard": null,
                "giftCardCode": null,
                "userErrors": [{ "field": ["input", "customerId"], "code": "CUSTOMER_NOT_FOUND", "message": "The customer could not be found." }]
            },
            "missingCustomer": {
                "giftCard": null,
                "giftCardCode": null,
                "userErrors": [{ "field": ["input", "customerId"], "code": "CUSTOMER_NOT_FOUND", "message": "The customer could not be found." }]
            },
            "success": {
                "giftCard": {
                    "id": "gid://shopify/GiftCard/1?shopify-draft-proxy=synthetic",
                    "lastCharacters": "zlrz",
                    "maskedCode": "•••• •••• •••• zlrz",
                    "initialValue": { "amount": "10.0", "currencyCode": "CAD" },
                    "balance": { "amount": "10.0", "currencyCode": "CAD" }
                },
                "giftCardCode": "parityokmowpzlrz",
                "userErrors": []
            },
            "duplicate": {
                "giftCard": null,
                "giftCardCode": null,
                "userErrors": [{ "field": ["input", "code"], "code": null, "message": "Code has already been taken" }]
            },
            "autoGenerated": {
                "giftCard": {
                    "id": "gid://shopify/GiftCard/2?shopify-draft-proxy=synthetic",
                    "lastCharacters": "0002",
                    "maskedCode": "•••• •••• •••• 0002",
                    "initialValue": { "amount": "10.0", "currencyCode": "CAD" },
                    "balance": { "amount": "10.0", "currencyCode": "CAD" }
                },
                "giftCardCode": "giftcard00000002",
                "userErrors": []
            }
        })
    );
}

#[test]
fn gift_card_create_omitted_optional_fields_are_null_and_supplied_values_round_trip() {
    let mut proxy = snapshot_proxy_with_gift_card_fixed_validation_clock();

    let plain_create = proxy.process_request(json_graphql_request(
        r#"mutation GiftCardCreatePlain {
          plain: giftCardCreate(input: { initialValue: "25" }) {
            giftCard {
              id
              note
              expiresOn
              customer { id }
              templateSuffix
              recipientAttributes {
                message
                preferredName
                sendNotificationAt
                recipient { id }
              }
            }
            giftCardCode
            userErrors { field code message }
          }
        }"#,
        json!({}),
    ));
    assert_eq!(plain_create.status, 200);
    assert_eq!(
        plain_create.body["data"]["plain"],
        json!({
            "giftCard": {
                "id": "gid://shopify/GiftCard/1?shopify-draft-proxy=synthetic",
                "note": null,
                "expiresOn": null,
                "customer": null,
                "templateSuffix": null,
                "recipientAttributes": null
            },
            "giftCardCode": "giftcard00000001",
            "userErrors": []
        })
    );

    let plain_read = proxy.process_request(json_graphql_request(
        r#"query GiftCardCreatePlainRead($id: ID!) {
          giftCard(id: $id) {
            id
            note
            expiresOn
            customer { id }
            templateSuffix
            recipientAttributes {
              message
              preferredName
              sendNotificationAt
              recipient { id }
            }
          }
        }"#,
        json!({ "id": "gid://shopify/GiftCard/1?shopify-draft-proxy=synthetic" }),
    ));
    assert_eq!(plain_read.status, 200);
    assert_eq!(
        plain_read.body["data"]["giftCard"],
        json!({
            "id": "gid://shopify/GiftCard/1?shopify-draft-proxy=synthetic",
            "note": null,
            "expiresOn": null,
            "customer": null,
            "templateSuffix": null,
            "recipientAttributes": null
        })
    );

    let setup_recipient = proxy.process_request(json_graphql_request(
        r#"mutation GiftCardCreateSuppliedRecipient {
          customerCreate(input: { firstName: "Requested", lastName: "Recipient", email: "requested-recipient@example.com" }) {
            customer { id }
            userErrors { field message  }
          }
        }"#,
        json!({}),
    ));
    assert_eq!(
        setup_recipient.body["data"]["customerCreate"]["userErrors"],
        json!([])
    );
    let recipient_id = setup_recipient.body["data"]["customerCreate"]["customer"]["id"]
        .as_str()
        .expect("setup recipient id")
        .to_string();

    let supplied_create = proxy.process_request(json_graphql_request(
        r#"mutation GiftCardCreateSupplied($recipientId: ID!, $sendAt: DateTime!) {
          supplied: giftCardCreate(input: {
            initialValue: "30"
            note: "Requested gift card note"
            expiresOn: "2028-01-31"
            recipientAttributes: {
              id: $recipientId
              preferredName: "Requested Recipient"
              message: "Requested recipient message"
              sendNotificationAt: $sendAt
            }
          }) {
            giftCard {
              id
              note
              expiresOn
              customer { id }
              templateSuffix
              recipientAttributes {
                message
                preferredName
                sendNotificationAt
                recipient { id }
              }
            }
            giftCardCode
            userErrors { field code message }
          }
        }"#,
        json!({
            "recipientId": recipient_id.clone(),
            "sendAt": "2026-07-01T00:00:00Z"
        }),
    ));
    assert_eq!(supplied_create.status, 200);
    let supplied_card = json!({
        "id": "gid://shopify/GiftCard/3?shopify-draft-proxy=synthetic",
        "note": "Requested gift card note",
        "expiresOn": "2028-01-31",
        "customer": null,
        "templateSuffix": null,
        "recipientAttributes": {
            "message": "Requested recipient message",
            "preferredName": "Requested Recipient",
            "sendNotificationAt": "2026-07-01T00:00:00Z",
            "recipient": { "id": recipient_id }
        }
    });
    assert_eq!(
        supplied_create.body["data"]["supplied"],
        json!({
            "giftCard": supplied_card,
            "giftCardCode": "giftcard00000003",
            "userErrors": []
        })
    );
    assert!(
        !supplied_create
            .body
            .to_string()
            .contains("no-contact-recipient"),
        "real recipient create without customerId must not use the fabricated no-contact sentinel"
    );

    let supplied_read = proxy.process_request(json_graphql_request(
        r#"query GiftCardCreateSuppliedRead($id: ID!) {
          giftCard(id: $id) {
            id
            note
            expiresOn
            customer { id }
            templateSuffix
            recipientAttributes {
              message
              preferredName
              sendNotificationAt
              recipient { id }
            }
          }
        }"#,
        json!({ "id": "gid://shopify/GiftCard/3?shopify-draft-proxy=synthetic" }),
    ));
    assert_eq!(supplied_read.status, 200);
    assert_eq!(supplied_read.body["data"]["giftCard"], supplied_card);
    assert!(
        !supplied_read
            .body
            .to_string()
            .contains("no-contact-recipient"),
        "real recipient readback must not use the fabricated no-contact sentinel"
    );
}

#[test]
fn gift_card_create_released_schema_rejects_missing_initial_value_and_initial_amount() {
    let mut proxy = snapshot_proxy();

    let inline_missing = proxy.process_request(json_graphql_request(
        r#"mutation ReleasedMissingInline {
          missing: giftCardCreate(input: { note: "x" }) {
            giftCard { id }
            userErrors { field code message }
          }
        }"#,
        json!({}),
    ));
    assert_eq!(
        inline_missing.body,
        json!({
            "errors": [{
                "message": "Argument 'initialValue' on InputObject 'GiftCardCreateInput' is required. Expected type Decimal!",
                "locations": [{ "line": 2, "column": 42 }],
                "path": ["mutation ReleasedMissingInline", "missing", "input", "initialValue"],
                "extensions": {
                    "code": "missingRequiredInputObjectAttribute",
                    "argumentName": "initialValue",
                    "argumentType": "Decimal!",
                    "inputObjectType": "GiftCardCreateInput"
                }
            }]
        })
    );

    let variable_missing = proxy.process_request(json_graphql_request(
        r#"mutation ReleasedMissingVariable($input: GiftCardCreateInput!) {
          missing: giftCardCreate(input: $input) {
            giftCard { id }
            userErrors { field code message }
          }
        }"#,
        json!({ "input": { "note": "x" } }),
    ));
    assert_eq!(
        variable_missing.body,
        json!({
            "errors": [{
                "message": "Variable $input of type GiftCardCreateInput! was provided invalid value for initialValue (Expected value to not be null)",
                "locations": [{ "line": 1, "column": 34 }],
                "extensions": {
                    "code": "INVALID_VARIABLE",
                    "value": { "note": "x" },
                    "problems": [{ "path": ["initialValue"], "explanation": "Expected value to not be null" }]
                }
            }]
        })
    );

    let variable_initial_amount = proxy.process_request(json_graphql_request(
        r#"mutation ReleasedInitialAmount($input: GiftCardCreateInput!) {
          money: giftCardCreate(input: $input) {
            giftCard { id }
            userErrors { field code message }
          }
        }"#,
        json!({
            "input": {
                "initialValue": "10",
                "initialAmount": { "amount": "10", "currencyCode": "USD" }
            }
        }),
    ));
    assert_eq!(
        variable_initial_amount.body,
        json!({
            "errors": [{
                "message": "Variable $input of type GiftCardCreateInput! was provided invalid value for initialAmount (Field is not defined on GiftCardCreateInput)",
                "locations": [{ "line": 1, "column": 32 }],
                "extensions": {
                    "code": "INVALID_VARIABLE",
                    "value": {
                        "initialAmount": { "amount": "10", "currencyCode": "USD" },
                        "initialValue": "10"
                    },
                    "problems": [{ "path": ["initialAmount"], "explanation": "Field is not defined on GiftCardCreateInput" }]
                }
            }]
        })
    );

    let variable_multiple_errors = proxy.process_request(json_graphql_request(
        r#"mutation ReleasedMultipleVariableErrors($input: GiftCardCreateInput!) {
          money: giftCardCreate(input: $input) {
            giftCard { id }
            userErrors { field code message }
          }
        }"#,
        json!({
            "input": {
                "note": "x",
                "initialAmount": { "amount": "10", "currencyCode": "USD" }
            }
        }),
    ));
    assert_eq!(
        variable_multiple_errors.body,
        json!({
            "errors": [{
                "message": "Variable $input of type GiftCardCreateInput! was provided invalid value for initialAmount (Field is not defined on GiftCardCreateInput), initialValue (Expected value to not be null)",
                "locations": [{ "line": 1, "column": 41 }],
                "extensions": {
                    "code": "INVALID_VARIABLE",
                    "value": {
                        "initialAmount": { "amount": "10", "currencyCode": "USD" },
                        "note": "x"
                    },
                    "problems": [
                        { "path": ["initialAmount"], "explanation": "Field is not defined on GiftCardCreateInput" },
                        { "path": ["initialValue"], "explanation": "Expected value to not be null" }
                    ]
                }
            }]
        })
    );

    let inline_initial_amount = proxy.process_request(json_graphql_request(
        r#"mutation ReleasedInitialAmountInline {
          money: giftCardCreate(input: { initialValue: "10", initialAmount: { amount: "10", currencyCode: USD } }) {
            giftCard { id }
            userErrors { field code message }
          }
        }"#,
        json!({}),
    ));
    assert_eq!(
        inline_initial_amount.body,
        json!({
            "errors": [{
                "message": "InputObject 'GiftCardCreateInput' doesn't accept argument 'initialAmount'",
                "locations": [{ "line": 2, "column": 62 }],
                "path": ["mutation ReleasedInitialAmountInline", "money", "input", "initialAmount"],
                "extensions": {
                    "code": "argumentNotAccepted",
                    "name": "GiftCardCreateInput",
                    "typeName": "InputObject",
                    "argumentName": "initialAmount"
                }
            }]
        })
    );

    assert_eq!(log_snapshot(&proxy)["entries"], json!([]));
}

#[test]
fn gift_card_roots_accept_ordinary_operation_names_without_501s() {
    let mut proxy = cad_snapshot_proxy();

    let create = proxy.process_request(json_graphql_request(
        r#"mutation IssueLocalGiftCard {
          issue: giftCardCreate(input: { initialValue: "12.50" }) {
            giftCard { id balance { amount currencyCode } }
            giftCardCode
            userErrors { field code message }
          }
        }"#,
        json!({}),
    ));
    assert_eq!(create.status, 200);
    assert_eq!(
        create.body["data"]["issue"],
        json!({
            "giftCard": {
                "id": "gid://shopify/GiftCard/1?shopify-draft-proxy=synthetic",
                "balance": { "amount": "12.5", "currencyCode": "CAD" }
            },
            "giftCardCode": "giftcard00000001",
            "userErrors": []
        })
    );

    let read = proxy.process_request(json_graphql_request(
        r#"query ReadLocalGiftCard($id: ID!, $query: String!) {
          card: giftCard(id: $id) { id balance { amount currencyCode } }
          cards: giftCards(first: 5, query: $query, sortKey: ID) { nodes { id balance { amount currencyCode } } }
          count: giftCardsCount(query: $query) { count precision }
          config: giftCardConfiguration { issueLimit { amount currencyCode } }
        }"#,
        json!({
            "id": "gid://shopify/GiftCard/1?shopify-draft-proxy=synthetic",
            "query": "id:1"
        }),
    ));
    assert_eq!(read.status, 200);
    assert_eq!(
        read.body["data"],
        json!({
            "card": {
                "id": "gid://shopify/GiftCard/1?shopify-draft-proxy=synthetic",
                "balance": { "amount": "12.5", "currencyCode": "CAD" }
            },
            "cards": {
                "nodes": [{
                    "id": "gid://shopify/GiftCard/1?shopify-draft-proxy=synthetic",
                    "balance": { "amount": "12.5", "currencyCode": "CAD" }
                }]
            },
            "count": { "count": 1, "precision": "EXACT" },
            "config": { "issueLimit": { "amount": "3000.0", "currencyCode": "CAD" } }
        })
    );

    let validations = proxy.process_request(json_graphql_request(
        r#"mutation ValidateLocalGiftCards {
          emptyUpdate: giftCardUpdate(id: "gid://shopify/GiftCard/1?shopify-draft-proxy=synthetic", input: {}) {
            giftCard { id }
            userErrors { field  message }
          }
          missingUpdate: giftCardUpdate(id: "gid://shopify/GiftCard/999999999", input: { note: "x" }) {
            giftCard { id }
            userErrors { field  message }
          }
          negativeCredit: giftCardCredit(id: "gid://shopify/GiftCard/1?shopify-draft-proxy=synthetic", creditInput: { creditAmount: { amount: "-1", currencyCode: CAD } }) {
            giftCardCreditTransaction { id }
            userErrors { field code message }
          }
          insufficientDebit: giftCardDebit(id: "gid://shopify/GiftCard/1?shopify-draft-proxy=synthetic", debitInput: { debitAmount: { amount: "999", currencyCode: CAD } }) {
            giftCardDebitTransaction { id }
            userErrors { field code message }
          }
          missingDeactivate: giftCardDeactivate(id: "gid://shopify/GiftCard/999999999") {
            giftCard { id }
            userErrors { field code message }
          }
          notifyDisabled: giftCardSendNotificationToCustomer(id: "gid://shopify/GiftCard/1?shopify-draft-proxy=synthetic") {
            giftCard { id }
            userErrors { field code message }
          }
          missingRecipientNotify: giftCardSendNotificationToRecipient(id: "gid://shopify/GiftCard/999999999") {
            giftCard { id }
            userErrors { field code message }
          }
        }"#,
        json!({}),
    ));
    assert_eq!(validations.status, 200);
    assert_eq!(
        validations.body["data"],
        json!({
            "emptyUpdate": {
                "giftCard": null,
                "userErrors": [{ "field": ["input"], "message": "At least one argument is required in the input." }]
            },
            "missingUpdate": {
                "giftCard": null,
                "userErrors": [{ "field": ["id"], "message": "The gift card could not be found." }]
            },
            "negativeCredit": {
                "giftCardCreditTransaction": null,
                "userErrors": [{ "field": ["creditInput", "creditAmount", "amount"], "code": "NEGATIVE_OR_ZERO_AMOUNT", "message": "A positive amount must be used." }]
            },
            "insufficientDebit": {
                "giftCardDebitTransaction": null,
                "userErrors": [{ "field": ["debitInput", "debitAmount", "amount"], "code": "INSUFFICIENT_FUNDS", "message": "The gift card does not have sufficient funds to satisfy the request." }]
            },
            "missingDeactivate": {
                "giftCard": null,
                "userErrors": [{ "field": ["id"], "code": "GIFT_CARD_NOT_FOUND", "message": "The gift card could not be found." }]
            },
            "notifyDisabled": {
                "giftCard": null,
                "userErrors": [{ "field": null, "code": "INVALID", "message": "The gift card has no customer." }]
            },
            "missingRecipientNotify": {
                "giftCard": null,
                "userErrors": [{ "field": ["id"], "code": "GIFT_CARD_NOT_FOUND", "message": "The gift card could not be found." }]
            }
        })
    );

    let transactions = proxy.process_request(json_graphql_request(
        r#"mutation AdjustLocalGiftCard {
          credit: giftCardCredit(id: "gid://shopify/GiftCard/1?shopify-draft-proxy=synthetic", creditInput: { creditAmount: { amount: "2.50", currencyCode: CAD }, note: "manual credit" }) {
            giftCardCreditTransaction { __typename amount { amount currencyCode } giftCard { balance { amount currencyCode } } }
            userErrors { field code message }
          }
          debit: giftCardDebit(id: "gid://shopify/GiftCard/1?shopify-draft-proxy=synthetic", debitInput: { debitAmount: { amount: "3.00", currencyCode: CAD }, note: "manual debit" }) {
            giftCardDebitTransaction { __typename amount { amount currencyCode } giftCard { balance { amount currencyCode } } }
            userErrors { field code message }
          }
          deactivate: giftCardDeactivate(id: "gid://shopify/GiftCard/1?shopify-draft-proxy=synthetic") {
            giftCard { id enabled balance { amount currencyCode } }
            userErrors { field code message }
          }
        }"#,
        json!({}),
    ));
    assert_eq!(transactions.status, 200);
    assert_eq!(
        transactions.body["data"],
        json!({
            "credit": {
                "giftCardCreditTransaction": {
                    "__typename": "GiftCardCreditTransaction",
                    "amount": { "amount": "2.5", "currencyCode": "CAD" },
                    "giftCard": { "balance": { "amount": "15.0", "currencyCode": "CAD" } }
                },
                "userErrors": []
            },
            "debit": {
                "giftCardDebitTransaction": {
                    "__typename": "GiftCardDebitTransaction",
                    "amount": { "amount": "-3.0", "currencyCode": "CAD" },
                    "giftCard": { "balance": { "amount": "12.0", "currencyCode": "CAD" } }
                },
                "userErrors": []
            },
            "deactivate": {
                "giftCard": {
                    "id": "gid://shopify/GiftCard/1?shopify-draft-proxy=synthetic",
                    "enabled": false,
                    "balance": { "amount": "12.0", "currencyCode": "CAD" }
                },
                "userErrors": []
            }
        })
    );
}

#[test]
fn gift_card_credit_debit_preserve_optional_transaction_notes() {
    let mut proxy = cad_snapshot_proxy();

    let create = proxy.process_request(json_graphql_request(
        r#"mutation IssueLocalGiftCard {
          giftCardCreate(input: { initialValue: "20.00" }) {
            giftCard { id }
            userErrors { field code message }
          }
        }"#,
        json!({}),
    ));
    assert_eq!(create.status, 200);
    let gift_card_id = create.body["data"]["giftCardCreate"]["giftCard"]["id"].clone();

    let transactions = proxy.process_request(json_graphql_request(
        r#"mutation AdjustGiftCardNotes($id: ID!) {
          creditWithoutNote: giftCardCredit(id: $id, creditInput: { creditAmount: { amount: "2.00", currencyCode: CAD } }) {
            giftCardCreditTransaction { note amount { amount currencyCode } }
            userErrors { field code message }
          }
          debitWithoutNote: giftCardDebit(id: $id, debitInput: { debitAmount: { amount: "1.00", currencyCode: CAD } }) {
            giftCardDebitTransaction { note amount { amount currencyCode } }
            userErrors { field code message }
          }
          creditWithNote: giftCardCredit(id: $id, creditInput: { creditAmount: { amount: "3.00", currencyCode: CAD }, note: "manual credit" }) {
            giftCardCreditTransaction { note amount { amount currencyCode } }
            userErrors { field code message }
          }
          debitWithNote: giftCardDebit(id: $id, debitInput: { debitAmount: { amount: "4.00", currencyCode: CAD }, note: "manual debit" }) {
            giftCardDebitTransaction { note amount { amount currencyCode } }
            userErrors { field code message }
          }
        }"#,
        json!({ "id": gift_card_id }),
    ));
    assert_eq!(transactions.status, 200);
    assert_eq!(
        transactions.body["data"],
        json!({
            "creditWithoutNote": {
                "giftCardCreditTransaction": { "note": null, "amount": { "amount": "2.0", "currencyCode": "CAD" } },
                "userErrors": []
            },
            "debitWithoutNote": {
                "giftCardDebitTransaction": { "note": null, "amount": { "amount": "-1.0", "currencyCode": "CAD" } },
                "userErrors": []
            },
            "creditWithNote": {
                "giftCardCreditTransaction": { "note": "manual credit", "amount": { "amount": "3.0", "currencyCode": "CAD" } },
                "userErrors": []
            },
            "debitWithNote": {
                "giftCardDebitTransaction": { "note": "manual debit", "amount": { "amount": "-4.0", "currencyCode": "CAD" } },
                "userErrors": []
            }
        })
    );

    let readback = proxy.process_request(json_graphql_request(
        r#"query GiftCardTransactionNoteReadback($id: ID!) {
          giftCard(id: $id) {
            transactions(first: 5) {
              nodes {
                note
                amount { amount currencyCode }
              }
            }
          }
        }"#,
        json!({ "id": gift_card_id }),
    ));
    assert_eq!(readback.status, 200);
    assert_eq!(
        readback.body["data"]["giftCard"]["transactions"]["nodes"],
        json!([
            { "note": null, "amount": { "amount": "2.0", "currencyCode": "CAD" } },
            { "note": null, "amount": { "amount": "-1.0", "currencyCode": "CAD" } },
            { "note": "manual credit", "amount": { "amount": "3.0", "currencyCode": "CAD" } },
            { "note": "manual debit", "amount": { "amount": "-4.0", "currencyCode": "CAD" } }
        ])
    );
}

#[test]
fn gift_card_lifecycle_stages_update_transactions_deactivate_and_downstream_reads() {
    let mut proxy = snapshot_proxy();
    seed_legacy_gift_card_base_state(&mut proxy);

    let empty = proxy.process_request(json_graphql_request(
        r#"query GiftCardReadEvidence($unknownId: ID!, $query: String!) {
          missingGiftCard: giftCard(id: $unknownId) { id }
          filteredEmptyGiftCards: giftCards(first: 2, query: $query, sortKey: ID) {
            nodes { id lastCharacters }
            pageInfo { hasNextPage hasPreviousPage }
          }
          filteredEmptyGiftCardsCount: giftCardsCount(query: $query) { count precision }
          giftCardConfiguration { issueLimit { amount currencyCode } purchaseLimit { amount currencyCode } }
        }"#,
        json!({
            "unknownId": "gid://shopify/GiftCard/999999999999",
            "query": "id:999999999999"
        }),
    ));
    assert_eq!(empty.body["data"]["missingGiftCard"], Value::Null);
    assert_eq!(
        empty.body["data"]["filteredEmptyGiftCards"],
        json!({ "nodes": [], "pageInfo": { "hasNextPage": false, "hasPreviousPage": false } })
    );
    assert_eq!(
        empty.body["data"]["filteredEmptyGiftCardsCount"],
        json!({ "count": 0, "precision": "EXACT" })
    );
    assert_eq!(
        empty.body["data"]["giftCardConfiguration"],
        json!({
            "issueLimit": { "amount": "3000.0", "currencyCode": "CAD" },
            "purchaseLimit": { "amount": "14000.0", "currencyCode": "CAD" }
        })
    );

    let lifecycle = proxy.process_request(json_graphql_request(
        r#"mutation GiftCardLifecycle($id: ID!, $updateInput: GiftCardUpdateInput!, $creditInput: GiftCardCreditInput!, $debitInput: GiftCardDebitInput!) {
          update: giftCardUpdate(id: $id, input: $updateInput) { giftCard { note templateSuffix expiresOn balance { amount currencyCode } } userErrors { field message } }
          credit: giftCardCredit(id: $id, creditInput: $creditInput) { giftCardCreditTransaction { note amount { amount currencyCode } giftCard { balance { amount currencyCode } } } userErrors { field message } }
          debit: giftCardDebit(id: $id, debitInput: $debitInput) { giftCardDebitTransaction { note amount { amount currencyCode } giftCard { balance { amount currencyCode } } } userErrors { field message } }
          deactivate: giftCardDeactivate(id: $id) { giftCard { enabled balance { amount currencyCode } } userErrors { field message } }
        }"#,
        json!({
            "id": "gid://shopify/GiftCard/654773256498",
            "updateInput": { "note": "HAR-310 conformance gift card updated", "templateSuffix": "birthday", "expiresOn": "2028-04-26" },
            "creditInput": { "creditAmount": { "amount": "2.00", "currencyCode": "CAD" }, "note": "HAR-310 credit" },
            "debitInput": { "debitAmount": { "amount": "3.00", "currencyCode": "CAD" }, "note": "HAR-310 debit" }
        }),
    ));
    assert_eq!(
        lifecycle.body["data"],
        json!({
            "update": {
                "giftCard": { "note": "HAR-310 conformance gift card updated", "templateSuffix": "birthday", "expiresOn": "2028-04-26", "balance": { "amount": "5.0", "currencyCode": "CAD" } },
                "userErrors": []
            },
            "credit": {
                "giftCardCreditTransaction": { "note": "HAR-310 credit", "amount": { "amount": "2.0", "currencyCode": "CAD" }, "giftCard": { "balance": { "amount": "7.0", "currencyCode": "CAD" } } },
                "userErrors": []
            },
            "debit": {
                "giftCardDebitTransaction": { "note": "HAR-310 debit", "amount": { "amount": "-3.0", "currencyCode": "CAD" }, "giftCard": { "balance": { "amount": "4.0", "currencyCode": "CAD" } } },
                "userErrors": []
            },
            "deactivate": {
                "giftCard": { "enabled": false, "balance": { "amount": "4.0", "currencyCode": "CAD" } },
                "userErrors": []
            }
        })
    );

    let downstream = proxy.process_request(json_graphql_request(
        r#"query GiftCardReadAfterLifecycle($id: ID!, $query: String!) {
          giftCard(id: $id) { note templateSuffix expiresOn enabled balance { amount currencyCode } transactions(first: 5) { nodes { note amount { amount currencyCode } } pageInfo { hasNextPage hasPreviousPage } } }
          giftCards(first: 2, query: $query, sortKey: ID) { nodes { id lastCharacters enabled } pageInfo { hasNextPage hasPreviousPage } }
          giftCardsCount(query: $query) { count precision }
        }"#,
        json!({
            "id": "gid://shopify/GiftCard/654773256498",
            "query": "id:654773256498"
        }),
    ));
    let expected_card = json!({
        "note": "HAR-310 conformance gift card updated",
        "templateSuffix": "birthday",
        "expiresOn": "2028-04-26",
        "enabled": false,
        "balance": { "amount": "4.0", "currencyCode": "CAD" },
        "transactions": {
            "nodes": [
                { "note": "HAR-310 credit", "amount": { "amount": "2.0", "currencyCode": "CAD" } },
                { "note": "HAR-310 debit", "amount": { "amount": "-3.0", "currencyCode": "CAD" } }
            ],
            "pageInfo": { "hasNextPage": false, "hasPreviousPage": false }
        }
    });
    assert_eq!(downstream.body["data"]["giftCard"], expected_card);
    assert_eq!(
        downstream.body["data"]["giftCards"],
        json!({ "nodes": [{ "id": "gid://shopify/GiftCard/654773256498", "lastCharacters": "2053", "enabled": false }], "pageInfo": { "hasNextPage": false, "hasPreviousPage": false } })
    );
    assert_eq!(
        downstream.body["data"]["giftCardsCount"],
        json!({ "count": 1, "precision": "EXACT" })
    );

    let node = proxy.process_request(json_graphql_request(
        r#"query GiftCardNodeReadAfterLifecycle($id: ID!) {
          node(id: $id) { ... on GiftCard { note templateSuffix expiresOn enabled balance { amount currencyCode } transactions(first: 5) { nodes { note amount { amount currencyCode } } pageInfo { hasNextPage hasPreviousPage } } } }
        }"#,
        json!({ "id": "gid://shopify/GiftCard/654773256498" }),
    ));
    assert_eq!(node.body["data"]["node"], expected_card);
}

#[test]
fn gift_card_expiry_uses_shop_timezone_boundary_before_expired_validation() {
    let mut proxy = cad_snapshot_proxy_with_gift_card_fixed_validation_clock();

    let dump = proxy.process_request(request_with_body(
        "POST",
        "/__meta/dump",
        r#"{"createdAt":"2026-04-29T09:31:02Z"}"#,
    ));
    let mut restored = dump.body.clone();
    restored["state"]["baseState"]["shop"]["ianaTimezone"] = json!("Pacific/Honolulu");
    restored["state"]["baseState"]["shop"]["timezoneOffsetMinutes"] = json!(-600);
    let restore = proxy.process_request(request_with_body(
        "POST",
        "/__meta/restore",
        &restored.to_string(),
    ));
    assert_eq!(restore.status, 200);

    let setup_recipient = proxy.process_request(json_graphql_request(
        r#"mutation GiftCardExpiryShopTimezoneRecipient {
          customerCreate(input: { firstName: "Timezone", lastName: "Recipient", email: "timezone-recipient@example.com" }) {
            customer { id }
            userErrors { field message  }
          }
        }"#,
        json!({}),
    ));
    assert_eq!(
        setup_recipient.body["data"]["customerCreate"]["userErrors"],
        json!([])
    );
    let recipient_id = setup_recipient.body["data"]["customerCreate"]["customer"]["id"]
        .as_str()
        .expect("setup recipient id")
        .to_string();

    let setup = proxy.process_request(json_graphql_request(
        r#"mutation GiftCardExpiryShopTimezoneSetup($customerId: ID!, $recipientId: ID!) {
          creditCard: giftCardCreate(input: { initialValue: "20", expiresOn: "2026-04-28" }) { giftCard { id } giftCardCode userErrors { field code message } }
          debitCard: giftCardCreate(input: { initialValue: "20", expiresOn: "2026-04-28" }) { giftCard { id } giftCardCode userErrors { field code message } }
          customerNotificationCard: giftCardCreate(input: { initialValue: "20", expiresOn: "2026-04-28", customerId: $customerId }) { giftCard { id } giftCardCode userErrors { field code message } }
          recipientNotificationCard: giftCardCreate(input: { initialValue: "20", expiresOn: "2026-04-28", recipientAttributes: { id: $recipientId } }) { giftCard { id } giftCardCode userErrors { field code message } }
        }"#,
        json!({ "customerId": recipient_id.clone(), "recipientId": recipient_id }),
    ));
    assert_eq!(setup.body["data"]["creditCard"]["userErrors"], json!([]));
    assert_eq!(setup.body["data"]["debitCard"]["userErrors"], json!([]));
    assert_eq!(
        setup.body["data"]["customerNotificationCard"]["userErrors"],
        json!([])
    );
    assert_eq!(
        setup.body["data"]["recipientNotificationCard"]["userErrors"],
        json!([])
    );
    let credit_id = json_string(
        &setup.body["data"]["creditCard"]["giftCard"]["id"],
        "credit card id",
    );
    let debit_id = json_string(
        &setup.body["data"]["debitCard"]["giftCard"]["id"],
        "debit card id",
    );
    let customer_notification_id = json_string(
        &setup.body["data"]["customerNotificationCard"]["giftCard"]["id"],
        "customer notification card id",
    );
    let recipient_notification_id = json_string(
        &setup.body["data"]["recipientNotificationCard"]["giftCard"]["id"],
        "recipient notification card id",
    );

    let response = proxy.process_request(json_graphql_request(
        r#"mutation GiftCardExpiryShopTimezone($creditId: ID!, $debitId: ID!, $customerNotificationId: ID!, $recipientNotificationId: ID!, $creditInput: GiftCardCreditInput!, $debitInput: GiftCardDebitInput!) {
          credit: giftCardCredit(id: $creditId, creditInput: $creditInput) { giftCardCreditTransaction { __typename } userErrors { field code message } }
          debit: giftCardDebit(id: $debitId, debitInput: $debitInput) { giftCardDebitTransaction { __typename } userErrors { field code message } }
          customerNotification: giftCardSendNotificationToCustomer(id: $customerNotificationId) { giftCard { id } userErrors { field code message } }
          recipientNotification: giftCardSendNotificationToRecipient(id: $recipientNotificationId) { giftCard { id } userErrors { field code message } }
        }"#,
        json!({
            "creditId": credit_id,
            "debitId": debit_id,
            "customerNotificationId": customer_notification_id,
            "recipientNotificationId": recipient_notification_id,
            "creditInput": { "creditAmount": { "amount": "5.00", "currencyCode": "CAD" } },
            "debitInput": { "debitAmount": { "amount": "2.00", "currencyCode": "CAD" } }
        }),
    ));

    assert_eq!(
        response.body["data"],
        json!({
            "credit": { "giftCardCreditTransaction": { "__typename": "GiftCardCreditTransaction" }, "userErrors": [] },
            "debit": { "giftCardDebitTransaction": { "__typename": "GiftCardDebitTransaction" }, "userErrors": [] },
            "customerNotification": { "giftCard": { "id": customer_notification_id }, "userErrors": [] },
            "recipientNotification": { "giftCard": { "id": recipient_notification_id }, "userErrors": [] }
        })
    );
}

#[test]
fn gift_card_expiry_uses_utc_fallback_when_shop_timezone_is_missing() {
    let mut proxy = cad_snapshot_proxy_with_gift_card_fixed_validation_clock();

    let setup = proxy.process_request(json_graphql_request(
        r#"mutation GiftCardExpiryUtcFallbackSetup {
          expired: giftCardCreate(input: { initialValue: "10", expiresOn: "2026-04-28" }) { giftCard { id } giftCardCode userErrors { field code message } }
          active: giftCardCreate(input: { initialValue: "10", expiresOn: "2026-04-30" }) { giftCard { id } giftCardCode userErrors { field code message } }
        }"#,
        json!({}),
    ));
    assert_eq!(setup.body["data"]["expired"]["userErrors"], json!([]));
    assert_eq!(setup.body["data"]["active"]["userErrors"], json!([]));
    let expired_id = json_string(
        &setup.body["data"]["expired"]["giftCard"]["id"],
        "expired card id",
    );
    let active_id = json_string(
        &setup.body["data"]["active"]["giftCard"]["id"],
        "active card id",
    );

    let response = proxy.process_request(json_graphql_request(
        r#"mutation GiftCardExpiryUtcFallback($expiredId: ID!, $activeId: ID!) {
          expiredCredit: giftCardCredit(id: $expiredId, creditInput: { creditAmount: { amount: "1.00", currencyCode: CAD } }) {
            giftCardCreditTransaction { id giftCard { balance { amount currencyCode } } }
            userErrors { field code message }
          }
          activeCredit: giftCardCredit(id: $activeId, creditInput: { creditAmount: { amount: "1.00", currencyCode: CAD } }) {
            giftCardCreditTransaction { id giftCard { balance { amount currencyCode } } }
            userErrors { field code message }
          }
        }"#,
        json!({ "expiredId": expired_id, "activeId": active_id }),
    ));

    assert_eq!(
        response.body["data"],
        json!({
            "expiredCredit": { "giftCardCreditTransaction": null, "userErrors": [{ "field": ["id"], "code": "INVALID", "message": "The gift card has expired." }] },
            "activeCredit": { "giftCardCreditTransaction": { "id": "gid://shopify/GiftCardCreditTransaction/3", "giftCard": { "balance": { "amount": "11.0", "currencyCode": "CAD" } } }, "userErrors": [] }
        })
    );

    let read = proxy.process_request(json_graphql_request(
        r#"query GiftCardExpiryUtcFallbackRead($expiredId: ID!, $activeId: ID!) {
          expired: giftCard(id: $expiredId) { balance { amount currencyCode } transactions(first: 5) { nodes { id } } }
          active: giftCard(id: $activeId) { balance { amount currencyCode } transactions(first: 5) { nodes { id } } }
        }"#,
        json!({ "expiredId": expired_id, "activeId": active_id }),
    ));
    assert_eq!(
        read.body["data"],
        json!({
            "expired": { "balance": { "amount": "10.0", "currencyCode": "CAD" }, "transactions": { "nodes": [] } },
            "active": { "balance": { "amount": "11.0", "currencyCode": "CAD" }, "transactions": { "nodes": [{ "id": "gid://shopify/GiftCardCreditTransaction/3" }] } }
        })
    );
}

#[test]
fn gift_card_credit_limit_rejects_credit_but_allows_followup_debit_transaction() {
    let mut proxy = cad_snapshot_proxy();

    let setup = proxy.process_request(json_graphql_request(
        r#"mutation GiftCardCreditLimitSetup {
          giftCardCreate(input: { initialValue: "3000", code: "txnboundary1" }) {
            giftCard { id balance { amount currencyCode } }
            userErrors { field code message }
          }
        }"#,
        json!({}),
    ));
    assert_eq!(
        setup.body["data"]["giftCardCreate"]["userErrors"],
        json!([])
    );
    let boundary_id = json_string(
        &setup.body["data"]["giftCardCreate"]["giftCard"]["id"],
        "boundary gift card id",
    );

    let response = proxy.process_request(json_graphql_request(
        r#"mutation GiftCardCreditLimitExceeded($boundaryId: ID!, $creditInput: GiftCardCreditInput!, $debitInput: GiftCardDebitInput!) {
          overLimitCredit: giftCardCredit(id: $boundaryId, creditInput: $creditInput) {
            giftCardCreditTransaction { __typename amount { amount currencyCode } }
            userErrors { field code message }
          }
          debitAfterRejectedCredit: giftCardDebit(id: $boundaryId, debitInput: $debitInput) {
            giftCardDebitTransaction { __typename amount { amount currencyCode } }
            userErrors { field code message }
          }
        }"#,
        json!({
            "boundaryId": boundary_id,
            "creditInput": { "creditAmount": { "amount": "0.01", "currencyCode": "CAD" } },
            "debitInput": { "debitAmount": { "amount": "0.01", "currencyCode": "CAD" } }
        }),
    ));

    assert_eq!(
        response.body["data"],
        json!({
            "overLimitCredit": {
                "giftCardCreditTransaction": null,
                "userErrors": [{
                    "field": ["creditInput", "creditAmount", "amount"],
                    "code": "GIFT_CARD_LIMIT_EXCEEDED",
                    "message": "The gift card's value exceeds the allowed limits."
                }]
            },
            "debitAfterRejectedCredit": {
                "giftCardDebitTransaction": {
                    "__typename": "GiftCardDebitTransaction",
                    "amount": { "amount": "-0.01", "currencyCode": "CAD" }
                },
                "userErrors": []
            }
        })
    );
}

#[test]
fn gift_card_entitlement_disabled_wins_for_all_supported_mutation_roots() {
    let mut proxy = snapshot_proxy();
    seed_legacy_gift_card_base_state(&mut proxy);
    set_gift_cards_unavailable(&mut proxy);

    let response = proxy.process_request(json_graphql_request(
        r#"mutation GiftCardEntitlementDisabled {
          createError: giftCardCreate(input: { initialValue: "0", customerId: "gid://shopify/Customer/disabled-entitlement-customer" }) { giftCard { id } giftCardCode userErrors { field code message } }
          updateError: giftCardUpdate(id: "gid://shopify/GiftCard/disabled-entitlement-card", input: { note: "x" }) { giftCard { id } userErrors { field  message } }
          creditError: giftCardCredit(id: "gid://shopify/GiftCard/disabled-entitlement-card", creditInput: { creditAmount: { amount: "-1", currencyCode: CAD } }) { giftCardCreditTransaction { id } userErrors { field code message } }
          debitError: giftCardDebit(id: "gid://shopify/GiftCard/disabled-entitlement-card", debitInput: { debitAmount: { amount: "9999", currencyCode: CAD } }) { giftCardDebitTransaction { id } userErrors { field code message } }
          deactivateError: giftCardDeactivate(id: "gid://shopify/GiftCard/disabled-entitlement-card") { giftCard { id } userErrors { field code message } }
          notificationCustomerError: giftCardSendNotificationToCustomer(id: "gid://shopify/GiftCard/disabled-entitlement-card") { giftCard { id } userErrors { field code message } }
          notificationRecipientError: giftCardSendNotificationToRecipient(id: "gid://shopify/GiftCard/disabled-entitlement-card") { giftCard { id } userErrors { field code message } }
        }"#,
        json!({}),
    ));

    let base_error = json!([{ "field": null, "code": null, "message": "Gift cards are unavailable on your plan." }]);
    let plain_base_error =
        json!([{ "field": null, "message": "Gift cards are unavailable on your plan." }]);
    assert_eq!(
        response.body["data"],
        json!({
            "createError": { "giftCard": null, "giftCardCode": null, "userErrors": base_error },
            "updateError": { "giftCard": null, "userErrors": plain_base_error },
            "creditError": { "giftCardCreditTransaction": null, "userErrors": base_error },
            "debitError": { "giftCardDebitTransaction": null, "userErrors": base_error },
            "deactivateError": { "giftCard": null, "userErrors": base_error },
            "notificationCustomerError": { "giftCard": null, "userErrors": base_error },
            "notificationRecipientError": { "giftCard": null, "userErrors": base_error }
        })
    );
    assert_eq!(log_snapshot(&proxy)["entries"], json!([]));
}

#[test]
fn gift_card_create_notify_is_rejected_by_the_versioned_schema() {
    let mut proxy = snapshot_proxy();

    let response = proxy.process_request(json_graphql_request(
        r#"mutation GiftCardCreateNotify {
          giftCardCreate(input: { initialValue: "10", notify: false }) {
            giftCard { id }
            userErrors { field code message }
          }
        }"#,
        json!({}),
    ));

    assert_eq!(
        response.body["errors"][0]["message"],
        json!("InputObject 'GiftCardCreateInput' doesn't accept argument 'notify'")
    );
    assert_eq!(
        response.body["errors"][0]["extensions"],
        json!({
            "code": "argumentNotAccepted",
            "name": "GiftCardCreateInput",
            "typeName": "InputObject",
            "argumentName": "notify"
        })
    );
    assert!(response.body.get("data").is_none());
    assert_eq!(log_snapshot(&proxy)["entries"], json!([]));
}

#[test]
fn gift_card_local_only_and_schema_hidden_branches_have_explicit_runtime_coverage() {
    // gift-card-entitlement-disabled
    {
        let mut proxy = snapshot_proxy();
        seed_legacy_gift_card_base_state(&mut proxy);
        set_gift_cards_unavailable(&mut proxy);

        let response = proxy.process_request(json_graphql_request(
            r#"mutation GiftCardLocalEntitlementDisabled {
              createError: giftCardCreate(input: { initialValue: "0", customerId: "gid://shopify/Customer/retired-entitlement" }) { giftCard { id } giftCardCode userErrors { field code message } }
              updateError: giftCardUpdate(id: "gid://shopify/GiftCard/disabled-entitlement-card", input: { note: "x" }) { giftCard { id } userErrors { field  message } }
              creditError: giftCardCredit(id: "gid://shopify/GiftCard/disabled-entitlement-card", creditInput: { creditAmount: { amount: "-1", currencyCode: CAD } }) { giftCardCreditTransaction { id } userErrors { field code message } }
              debitError: giftCardDebit(id: "gid://shopify/GiftCard/disabled-entitlement-card", debitInput: { debitAmount: { amount: "9999", currencyCode: CAD } }) { giftCardDebitTransaction { id } userErrors { field code message } }
              deactivateError: giftCardDeactivate(id: "gid://shopify/GiftCard/disabled-entitlement-card") { giftCard { id } userErrors { field code message } }
              customerNotificationError: giftCardSendNotificationToCustomer(id: "gid://shopify/GiftCard/disabled-entitlement-card") { giftCard { id } userErrors { field code message } }
              recipientNotificationError: giftCardSendNotificationToRecipient(id: "gid://shopify/GiftCard/disabled-entitlement-card") { giftCard { id } userErrors { field code message } }
            }"#,
            json!({}),
        ));
        let entitlement_error = json!([{ "field": null, "code": null, "message": "Gift cards are unavailable on your plan." }]);
        let plain_entitlement_error =
            json!([{ "field": null, "message": "Gift cards are unavailable on your plan." }]);
        assert_eq!(
            response.body["data"],
            json!({
                "createError": { "giftCard": null, "giftCardCode": null, "userErrors": entitlement_error },
                "updateError": { "giftCard": null, "userErrors": plain_entitlement_error },
                "creditError": { "giftCardCreditTransaction": null, "userErrors": entitlement_error },
                "debitError": { "giftCardDebitTransaction": null, "userErrors": entitlement_error },
                "deactivateError": { "giftCard": null, "userErrors": entitlement_error },
                "customerNotificationError": { "giftCard": null, "userErrors": entitlement_error },
                "recipientNotificationError": { "giftCard": null, "userErrors": entitlement_error }
            })
        );
        assert_eq!(log_snapshot(&proxy)["entries"], json!([]));
    }

    // gift-card-trial-shop-assignment
    {
        let mut proxy = snapshot_proxy();
        seed_legacy_gift_card_base_state(&mut proxy);
        set_gift_card_trial_shop(&mut proxy);

        let response = proxy.process_request(json_graphql_request(
            r#"mutation GiftCardLocalTrialShopAssignment($customerId: ID!, $recipientId: ID!, $updateGiftCardId: ID!) {
              createCustomerAssignment: giftCardCreate(input: { initialValue: "10", customerId: $customerId }) { giftCard { id } giftCardCode userErrors { field code message } }
              createRecipientAssignment: giftCardCreate(input: { initialValue: "10", recipientAttributes: { id: $recipientId } }) { giftCard { id } giftCardCode userErrors { field code message } }
              updateCustomerAssignment: giftCardUpdate(id: $updateGiftCardId, input: { customerId: $customerId }) { giftCard { id } userErrors { field  message } }
              updateRecipientAssignment: giftCardUpdate(id: $updateGiftCardId, input: { recipientAttributes: { id: $recipientId } }) { giftCard { id } userErrors { field  message } }
            }"#,
            json!({
                "customerId": "gid://shopify/Customer/retired-trial-customer",
                "recipientId": "gid://shopify/Customer/retired-trial-recipient",
                "updateGiftCardId": "gid://shopify/GiftCard/trial-update-card"
            }),
        ));
        assert_eq!(
            response.body["data"],
            json!({
                "createCustomerAssignment": { "giftCard": null, "giftCardCode": null, "userErrors": [{ "field": ["input", "customerId"], "code": "INVALID", "message": "A trial shop cannot assign a customer to a gift card." }] },
                "createRecipientAssignment": { "giftCard": null, "giftCardCode": null, "userErrors": [{ "field": ["input", "recipientAttributes"], "code": "INVALID", "message": "A trial shop cannot assign a recipient to a gift card." }] },
                "updateCustomerAssignment": { "giftCard": null, "userErrors": [{ "field": ["input", "customerId"], "message": "A trial shop cannot assign a customer to a gift card." }] },
                "updateRecipientAssignment": { "giftCard": null, "userErrors": [{ "field": ["input", "recipientAttributes"], "message": "A trial shop cannot assign a recipient to a gift card." }] }
            })
        );
        assert_eq!(log_snapshot(&proxy)["entries"], json!([]));
    }

    // gift-card-expiry-shop-timezone
    {
        let mut proxy = cad_snapshot_proxy_with_gift_card_fixed_validation_clock();
        let dump = proxy.process_request(request_with_body(
            "POST",
            "/__meta/dump",
            r#"{"createdAt":"2026-04-29T09:31:02Z"}"#,
        ));
        let mut restored = dump.body.clone();
        restored["state"]["baseState"]["shop"]["ianaTimezone"] = json!("Pacific/Honolulu");
        restored["state"]["baseState"]["shop"]["timezoneOffsetMinutes"] = json!(-600);
        let restore = proxy.process_request(request_with_body(
            "POST",
            "/__meta/restore",
            &restored.to_string(),
        ));
        assert_eq!(restore.status, 200);

        let customer = proxy.process_request(json_graphql_request(
            r#"mutation GiftCardLocalExpiryCustomer {
              customerCreate(input: { firstName: "Retired", lastName: "Timezone", email: "retired-timezone@example.com" }) {
                customer { id }
                userErrors { field message  }
              }
            }"#,
            json!({}),
        ));
        assert_eq!(
            customer.body["data"]["customerCreate"]["userErrors"],
            json!([])
        );
        let customer_id = json_string(
            &customer.body["data"]["customerCreate"]["customer"]["id"],
            "retired timezone customer id",
        );

        let setup = proxy.process_request(json_graphql_request(
            r#"mutation GiftCardLocalExpirySetup($customerId: ID!) {
              creditCard: giftCardCreate(input: { initialValue: "20", expiresOn: "2026-04-28" }) { giftCard { id } userErrors { field code message } }
              debitCard: giftCardCreate(input: { initialValue: "20", expiresOn: "2026-04-28" }) { giftCard { id } userErrors { field code message } }
              customerNotificationCard: giftCardCreate(input: { initialValue: "20", expiresOn: "2026-04-28", customerId: $customerId }) { giftCard { id } userErrors { field code message } }
              recipientNotificationCard: giftCardCreate(input: { initialValue: "20", expiresOn: "2026-04-28", recipientAttributes: { id: $customerId } }) { giftCard { id } userErrors { field code message } }
            }"#,
            json!({ "customerId": customer_id }),
        ));
        assert_eq!(setup.body["data"]["creditCard"]["userErrors"], json!([]));
        assert_eq!(setup.body["data"]["debitCard"]["userErrors"], json!([]));
        assert_eq!(
            setup.body["data"]["customerNotificationCard"]["userErrors"],
            json!([])
        );
        assert_eq!(
            setup.body["data"]["recipientNotificationCard"]["userErrors"],
            json!([])
        );
        let credit_id = json_string(
            &setup.body["data"]["creditCard"]["giftCard"]["id"],
            "credit id",
        );
        let debit_id = json_string(
            &setup.body["data"]["debitCard"]["giftCard"]["id"],
            "debit id",
        );
        let customer_notification_id = json_string(
            &setup.body["data"]["customerNotificationCard"]["giftCard"]["id"],
            "customer notification id",
        );
        let recipient_notification_id = json_string(
            &setup.body["data"]["recipientNotificationCard"]["giftCard"]["id"],
            "recipient notification id",
        );

        let response = proxy.process_request(json_graphql_request(
            r#"mutation GiftCardLocalExpiryTimezone($creditId: ID!, $debitId: ID!, $customerNotificationId: ID!, $recipientNotificationId: ID!) {
              credit: giftCardCredit(id: $creditId, creditInput: { creditAmount: { amount: "5.00", currencyCode: CAD } }) { giftCardCreditTransaction { __typename } userErrors { field code message } }
              debit: giftCardDebit(id: $debitId, debitInput: { debitAmount: { amount: "2.00", currencyCode: CAD } }) { giftCardDebitTransaction { __typename } userErrors { field code message } }
              customerNotification: giftCardSendNotificationToCustomer(id: $customerNotificationId) { giftCard { id } userErrors { field code message } }
              recipientNotification: giftCardSendNotificationToRecipient(id: $recipientNotificationId) { giftCard { id } userErrors { field code message } }
            }"#,
            json!({
                "creditId": credit_id,
                "debitId": debit_id,
                "customerNotificationId": customer_notification_id,
                "recipientNotificationId": recipient_notification_id
            }),
        ));
        assert_eq!(
            response.body["data"],
            json!({
                "credit": { "giftCardCreditTransaction": { "__typename": "GiftCardCreditTransaction" }, "userErrors": [] },
                "debit": { "giftCardDebitTransaction": { "__typename": "GiftCardDebitTransaction" }, "userErrors": [] },
                "customerNotification": { "giftCard": { "id": customer_notification_id }, "userErrors": [] },
                "recipientNotification": { "giftCard": { "id": recipient_notification_id }, "userErrors": [] }
            })
        );
    }

    // gift-card-mutation-user-error-codes schema-hidden update code supplement
    {
        let mut proxy = cad_snapshot_proxy();

        let response = proxy.process_request(json_graphql_request(
            r#"mutation GiftCardLocalMutationUserErrorCodes {
              setupSmallBalance: giftCardCreate(input: { initialValue: "5", code: "retirederrors" }) { giftCard { id } userErrors { field code message } }
              zeroInitialValue: giftCardCreate(input: { initialValue: "0" }) { giftCard { id } userErrors { field code message } }
              missingUpdate: giftCardUpdate(id: "gid://shopify/GiftCard/retired-missing", input: { note: "x" }) { giftCard { id } userErrors { field  message } }
              negativeCredit: giftCardCredit(id: "gid://shopify/GiftCard/1?shopify-draft-proxy=synthetic", creditInput: { creditAmount: { amount: "-1", currencyCode: CAD } }) { giftCardCreditTransaction { id } userErrors { field code message } }
              insufficientDebit: giftCardDebit(id: "gid://shopify/GiftCard/1?shopify-draft-proxy=synthetic", debitInput: { debitAmount: { amount: "9999", currencyCode: CAD } }) { giftCardDebitTransaction { id } userErrors { field code message } }
            }"#,
            json!({}),
        ));

        assert_eq!(
            response.body["data"],
            json!({
                "setupSmallBalance": { "giftCard": { "id": "gid://shopify/GiftCard/1?shopify-draft-proxy=synthetic" }, "userErrors": [] },
                "zeroInitialValue": {
                    "giftCard": null,
                    "userErrors": [{ "field": ["input", "initialValue"], "code": "GREATER_THAN", "message": "must be greater than 0" }]
                },
                "missingUpdate": {
                    "giftCard": null,
                    "userErrors": [{ "field": ["id"], "message": "The gift card could not be found." }]
                },
                "negativeCredit": {
                    "giftCardCreditTransaction": null,
                    "userErrors": [{ "field": ["creditInput", "creditAmount", "amount"], "code": "NEGATIVE_OR_ZERO_AMOUNT", "message": "A positive amount must be used." }]
                },
                "insufficientDebit": {
                    "giftCardDebitTransaction": null,
                    "userErrors": [{ "field": ["debitInput", "debitAmount", "amount"], "code": "INSUFFICIENT_FUNDS", "message": "The gift card does not have sufficient funds to satisfy the request." }]
                }
            })
        );
    }
}

#[test]
fn discount_timestamps_create_update_and_code_reads_preserve_staged_values() {
    let mut proxy = snapshot_proxy();
    let create = r#"mutation DiscountTimestampsMonotonicCreate($input: DiscountCodeBasicInput!) { discountCodeBasicCreate(basicCodeDiscount: $input) { codeDiscountNode { id codeDiscount { __typename ... on DiscountCodeBasic { title createdAt updatedAt codes(first: 1) { nodes { code } } } } } userErrors { field message code } } }"#;
    let first_create = proxy.process_request(json_graphql_request(
        create,
        json!({ "input": {
            "title": "HAR-603 first 1777990267935",
            "code": "HAR603A1777990267935",
            "startsAt": "2026-05-05T14:10:07.935Z",
            "context": { "all": "ALL" },
            "customerGets": { "value": { "percentage": 0.1 }, "items": { "all": true } }
        }}),
    ));
    let first_id = first_create.body["data"]["discountCodeBasicCreate"]["codeDiscountNode"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let first_created_at = first_create.body["data"]["discountCodeBasicCreate"]["codeDiscountNode"]
        ["codeDiscount"]["createdAt"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(
        first_create.body["data"]["discountCodeBasicCreate"]["codeDiscountNode"]["codeDiscount"]
            ["title"],
        json!("HAR-603 first 1777990267935")
    );
    assert_eq!(
        first_create.body["data"]["discountCodeBasicCreate"]["codeDiscountNode"]["codeDiscount"]
            ["updatedAt"],
        json!(first_created_at)
    );
    assert_eq!(
        first_create.body["data"]["discountCodeBasicCreate"]["codeDiscountNode"]["codeDiscount"]
            ["codes"],
        json!({ "nodes": [{ "code": "HAR603A1777990267935" }] })
    );
    assert_eq!(
        first_create.body["data"]["discountCodeBasicCreate"]["userErrors"],
        json!([])
    );

    let second_create = proxy.process_request(json_graphql_request(
        create,
        json!({ "input": {
            "title": "HAR-603 second 1777990267935",
            "code": "HAR603B1777990267935",
            "startsAt": "2026-05-05T14:10:07.935Z",
            "context": { "all": "ALL" },
            "customerGets": { "value": { "percentage": 0.1 }, "items": { "all": true } }
        }}),
    ));
    let second_id = second_create.body["data"]["discountCodeBasicCreate"]["codeDiscountNode"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let second_created_at = second_create.body["data"]["discountCodeBasicCreate"]
        ["codeDiscountNode"]["codeDiscount"]["createdAt"]
        .as_str()
        .unwrap()
        .to_string();
    assert_ne!(first_id, second_id);
    assert_synthetic_gid(&first_id, "DiscountCodeNode");
    assert_synthetic_gid(&second_id, "DiscountCodeNode");
    assert_eq!(
        second_create.body["data"]["discountCodeBasicCreate"]["codeDiscountNode"]["codeDiscount"]
            ["updatedAt"],
        json!(second_created_at)
    );

    let update = r#"mutation DiscountTimestampsMonotonicUpdate($id: ID!, $input: DiscountCodeBasicInput!) { discountCodeBasicUpdate(id: $id, basicCodeDiscount: $input) { codeDiscountNode { id codeDiscount { __typename ... on DiscountCodeBasic { title createdAt updatedAt codes(first: 1) { nodes { code } } } } } userErrors { field message code } } }"#;
    let update_response = proxy.process_request(json_graphql_request(
        update,
        json!({ "id": first_id, "input": {
            "title": "HAR-603 first updated 1777990267935",
            "code": "HAR603A1777990267935",
            "startsAt": "2026-05-05T14:10:07.935Z",
            "context": { "all": "ALL" },
            "customerGets": { "value": { "percentage": 0.2 }, "items": { "all": true } }
        }}),
    ));
    let updated_at = update_response.body["data"]["discountCodeBasicUpdate"]["codeDiscountNode"]
        ["codeDiscount"]["updatedAt"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(
        update_response.body["data"]["discountCodeBasicUpdate"]["codeDiscountNode"]["id"],
        json!(first_id)
    );
    assert_eq!(
        update_response.body["data"]["discountCodeBasicUpdate"]["codeDiscountNode"]["codeDiscount"]
            ["createdAt"],
        json!(first_created_at)
    );
    assert!(!updated_at.is_empty());
    assert_eq!(
        update_response.body["data"]["discountCodeBasicUpdate"]["codeDiscountNode"]["codeDiscount"]
            ["title"],
        json!("HAR-603 first updated 1777990267935")
    );
    assert_eq!(
        update_response.body["data"]["discountCodeBasicUpdate"]["userErrors"],
        json!([])
    );

    let read = r#"query DiscountTimestampsMonotonicRead($firstId: ID!, $secondId: ID!, $firstCode: String!, $secondCode: String!) { first: codeDiscountNode(id: $firstId) { id codeDiscount { __typename ... on DiscountCodeBasic { title createdAt updatedAt } } } second: codeDiscountNode(id: $secondId) { id codeDiscount { __typename ... on DiscountCodeBasic { title createdAt updatedAt } } } firstByCode: codeDiscountNodeByCode(code: $firstCode) { id codeDiscount { __typename ... on DiscountCodeBasic { title createdAt updatedAt } } } secondByCode: codeDiscountNodeByCode(code: $secondCode) { id codeDiscount { __typename ... on DiscountCodeBasic { title createdAt updatedAt } } } }"#;
    let read_response = proxy.process_request(json_graphql_request(
        read,
        json!({
            "firstId": first_id,
            "secondId": second_id,
            "firstCode": "HAR603A1777990267935",
            "secondCode": "HAR603B1777990267935"
        }),
    ));
    assert_eq!(
        read_response.body["data"]["first"],
        read_response.body["data"]["firstByCode"]
    );
    assert_eq!(
        read_response.body["data"]["second"],
        read_response.body["data"]["secondByCode"]
    );
    assert_eq!(
        read_response.body["data"]["first"]["codeDiscount"]["updatedAt"],
        json!(updated_at)
    );
    assert_eq!(
        read_response.body["data"]["second"]["codeDiscount"]["updatedAt"],
        json!(second_created_at)
    );
}

#[test]
fn discount_redeem_code_bulk_live_add_delete_stages_case_insensitive_code_lookups() {
    let mut proxy = snapshot_proxy();
    let create = r#"mutation SeedRedeemCodeBulk($input: DiscountCodeBasicInput!) { discountCodeBasicCreate(basicCodeDiscount: $input) { codeDiscountNode { id codeDiscount { ... on DiscountCodeBasic { codes { nodes { id code } } } } } userErrors { field message code extraInfo } } }"#;
    let created = proxy.process_request(json_graphql_request(
        create,
        json!({ "input": {
            "title": "Redeem code bulk generic lifecycle",
            "code": "HAR438BASE1777416023154",
            "startsAt": "2026-04-28T22:39:23Z",
            "combinesWith": { "productDiscounts": false, "orderDiscounts": true, "shippingDiscounts": false },
            "context": { "all": "ALL" },
            "customerGets": { "value": { "percentage": 0.1 }, "items": { "all": true } }
        }}),
    ));
    let discount_id = created.body["data"]["discountCodeBasicCreate"]["codeDiscountNode"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let seed_redeem_code_id = created.body["data"]["discountCodeBasicCreate"]["codeDiscountNode"]
        ["codeDiscount"]["codes"]["nodes"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let add = r#"mutation AnyBulkAdd($discountId: ID!, $codes: [DiscountRedeemCodeInput!]!) { discountRedeemCodeBulkAdd(discountId: $discountId, codes: $codes) { bulkCreation { done codesCount importedCount failedCount } userErrors { field message code extraInfo } } }"#;
    let add_response = proxy.process_request(json_graphql_request(
        add,
        json!({
            "discountId": discount_id,
            "codes": [{ "code": "HAR438ADD1777416023154" }, { "code": "HAR438PLUS1777416023154" }]
        }),
    ));
    assert_eq!(
        add_response.body["data"]["discountRedeemCodeBulkAdd"]["bulkCreation"]["codesCount"],
        json!(2)
    );
    assert_eq!(
        add_response.body["data"]["discountRedeemCodeBulkAdd"]["bulkCreation"]["failedCount"],
        json!(0)
    );
    assert_eq!(
        add_response.body["data"]["discountRedeemCodeBulkAdd"]["userErrors"],
        json!([])
    );

    let read = r#"query AnyBulkRead($id: ID!, $exactAddedCode: String!, $lowerAddedCode: String!, $removedCode: String!) { codeDiscountNode(id: $id) { id codeDiscount { ... on DiscountCodeBasic { codesCount { count precision } } } } exactAdded: codeDiscountNodeByCode(code: $exactAddedCode) { id } lowerAdded: codeDiscountNodeByCode(code: $lowerAddedCode) { id } removed: codeDiscountNodeByCode(code: $removedCode) { id } }"#;
    let read_vars = json!({
        "id": discount_id,
        "exactAddedCode": "HAR438ADD1777416023154",
        "lowerAddedCode": "har438add1777416023154",
        "removedCode": "HAR438BASE1777416023154"
    });
    let after_add = proxy.process_request(json_graphql_request(read, read_vars.clone()));
    assert_eq!(
        after_add.body["data"]["codeDiscountNode"]["codeDiscount"]["codesCount"],
        json!({ "count": 3, "precision": "EXACT" })
    );
    assert_eq!(
        after_add.body["data"]["exactAdded"]["id"],
        after_add.body["data"]["codeDiscountNode"]["id"]
    );
    assert_eq!(
        after_add.body["data"]["lowerAdded"]["id"],
        after_add.body["data"]["codeDiscountNode"]["id"]
    );
    assert_eq!(
        after_add.body["data"]["removed"]["id"],
        after_add.body["data"]["codeDiscountNode"]["id"]
    );

    let delete = r#"mutation AnyBulkDelete($discountId: ID!, $ids: [ID!]!) { discountCodeRedeemCodeBulkDelete(discountId: $discountId, ids: $ids) { job { done } userErrors { field message code extraInfo } } }"#;
    let delete_response = proxy.process_request(json_graphql_request(
        delete,
        json!({
            "discountId": read_vars["id"].clone(),
            "ids": [seed_redeem_code_id]
        }),
    ));
    assert_eq!(
        delete_response.body["data"]["discountCodeRedeemCodeBulkDelete"]["job"]["done"],
        json!(true)
    );
    assert_eq!(
        delete_response.body["data"]["discountCodeRedeemCodeBulkDelete"]["userErrors"],
        json!([])
    );

    let after_delete = proxy.process_request(json_graphql_request(read, read_vars));
    assert_eq!(
        after_delete.body["data"]["codeDiscountNode"]["codeDiscount"]["codesCount"],
        json!({ "count": 2, "precision": "EXACT" })
    );
    assert_eq!(
        after_delete.body["data"]["exactAdded"]["id"],
        after_delete.body["data"]["codeDiscountNode"]["id"]
    );
    assert_eq!(
        after_delete.body["data"]["lowerAdded"]["id"],
        after_delete.body["data"]["codeDiscountNode"]["id"]
    );
    assert_eq!(after_delete.body["data"]["removed"], Value::Null);
}

#[test]
fn discount_redeem_code_bulk_delete_validation_matches_selector_errors_and_happy_job() {
    let mut proxy = snapshot_proxy();
    let create = r#"mutation SeedBulkDeleteValidation($input: DiscountCodeBasicInput!) { discountCodeBasicCreate(basicCodeDiscount: $input) { codeDiscountNode { id codeDiscount { ... on DiscountCodeBasic { codes { nodes { id code } } } } } userErrors { field message code extraInfo } } }"#;
    let created = proxy.process_request(json_graphql_request(
        create,
        json!({ "input": {
            "title": "Redeem code bulk delete validation",
            "code": "HAR1442BASE",
            "startsAt": "2026-04-27T19:31:14Z",
            "combinesWith": { "productDiscounts": false, "orderDiscounts": true, "shippingDiscounts": false },
            "context": { "all": "ALL" },
            "customerGets": { "value": { "percentage": 0.1 }, "items": { "all": true } }
        }}),
    ));
    let discount_id = created.body["data"]["discountCodeBasicCreate"]["codeDiscountNode"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let redeem_code_id = created.body["data"]["discountCodeBasicCreate"]["codeDiscountNode"]
        ["codeDiscount"]["codes"]["nodes"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let validation = r#"mutation BulkDelete($discountId: ID!, $unknownDiscountId: ID!, $ids: [ID!], $emptyIds: [ID!], $search: String, $blankSearch: String, $savedSearchId: ID!) { missing: discountCodeRedeemCodeBulkDelete(discountId: $discountId) { job { id done } userErrors { field message code extraInfo } } tooMany: discountCodeRedeemCodeBulkDelete(discountId: $discountId, ids: $ids, search: $search) { job { id done } userErrors { field message code extraInfo } } unknownDiscount: discountCodeRedeemCodeBulkDelete(discountId: $unknownDiscountId, ids: $ids) { job { id done } userErrors { field message code extraInfo } } emptyIds: discountCodeRedeemCodeBulkDelete(discountId: $discountId, ids: $emptyIds) { job { id done } userErrors { field message code extraInfo } } blankSearch: discountCodeRedeemCodeBulkDelete(discountId: $discountId, search: $blankSearch) { job { id done } userErrors { field message code extraInfo } } invalidSavedSearch: discountCodeRedeemCodeBulkDelete(discountId: $discountId, savedSearchId: $savedSearchId) { job { id done } userErrors { field message code extraInfo } } }"#;
    let variables = json!({
        "discountId": discount_id,
        "unknownDiscountId": "gid://shopify/DiscountCodeNode/0",
        "ids": [redeem_code_id],
        "emptyIds": [],
        "search": "code:ANY",
        "blankSearch": "   ",
        "savedSearchId": "gid://shopify/SavedSearch/0"
    });
    let response = proxy.process_request(json_graphql_request(validation, variables.clone()));
    assert_eq!(
        response.body["data"]["missing"],
        json!({ "job": null, "userErrors": [{ "field": null, "message": "Missing expected argument key: 'ids', 'search' or 'saved_search_id'.", "code": "MISSING_ARGUMENT", "extraInfo": null }] })
    );
    assert_eq!(
        response.body["data"]["tooMany"],
        json!({ "job": null, "userErrors": [{ "field": null, "message": "Only one of 'ids', 'search' or 'saved_search_id' is allowed.", "code": "TOO_MANY_ARGUMENTS", "extraInfo": null }] })
    );
    assert_eq!(
        response.body["data"]["unknownDiscount"],
        json!({ "job": null, "userErrors": [{ "field": ["discountId"], "message": "Code discount does not exist.", "code": "INVALID", "extraInfo": null }] })
    );
    assert_eq!(
        response.body["data"]["emptyIds"],
        json!({ "job": null, "userErrors": [{ "field": null, "message": "Something went wrong, please try again.", "code": null, "extraInfo": null }] })
    );
    assert_eq!(
        response.body["data"]["blankSearch"],
        json!({ "job": null, "userErrors": [{ "field": ["search"], "message": "'Search' can't be blank.", "code": "BLANK", "extraInfo": null }] })
    );
    assert_eq!(
        response.body["data"]["invalidSavedSearch"],
        json!({ "job": null, "userErrors": [{ "field": ["savedSearchId"], "message": "Invalid 'saved_search_id'.", "code": "INVALID", "extraInfo": null }] })
    );

    let happy = r#"mutation BulkDeleteHappy($discountId: ID!, $ids: [ID!]!) { happy: discountCodeRedeemCodeBulkDelete(discountId: $discountId, ids: $ids) { job { id done } userErrors { field message code extraInfo } } }"#;
    let happy_response = proxy.process_request(json_graphql_request(
        happy,
        json!({ "discountId": variables["discountId"].clone(), "ids": variables["ids"].clone() }),
    ));
    assert_eq!(
        happy_response.body["data"]["happy"]["job"]["done"],
        json!(true)
    );
    assert!(happy_response.body["data"]["happy"]["job"]["id"]
        .as_str()
        .unwrap()
        .starts_with("gid://shopify/Job/"));
    assert_eq!(
        happy_response.body["data"]["happy"]["userErrors"],
        json!([])
    );
}

#[test]
fn discount_redeem_code_bulk_add_validation_tracks_async_results_and_downstream_reads() {
    let mut proxy = snapshot_proxy();
    let create = r#"mutation DiscountRedeemCodeBulkValidationCreate($input: DiscountCodeBasicInput!) { discountCodeBasicCreate(basicCodeDiscount: $input) { codeDiscountNode { id } userErrors { field message code extraInfo } } }"#;
    let created = proxy.process_request(json_graphql_request(create, json!({ "input": { "title": "HAR-784 redeem code validation 1778166762181", "code": "HAR784BASE1778166762181", "startsAt": "2026-05-07T15:11:42.181Z", "combinesWith": { "productDiscounts": false, "orderDiscounts": true, "shippingDiscounts": false }, "context": { "all": "ALL" }, "customerGets": { "value": { "percentage": 0.1 }, "items": { "all": true } } } })));
    let discount_id = json_string(
        &created.body["data"]["discountCodeBasicCreate"]["codeDiscountNode"]["id"],
        "bulk validation discount id",
    );
    assert_synthetic_gid(&discount_id, "DiscountCodeNode");
    assert_eq!(
        created.body["data"]["discountCodeBasicCreate"]["userErrors"],
        json!([])
    );
    let cross_discount = proxy.process_request(json_graphql_request(create, json!({ "input": { "title": "HAR-784 cross discount validation 1778166762181", "code": "HAR784CROSS1778166762181", "startsAt": "2026-05-07T15:11:42.181Z", "combinesWith": { "productDiscounts": false, "orderDiscounts": true, "shippingDiscounts": false }, "context": { "all": "ALL" }, "customerGets": { "value": { "percentage": 0.1 }, "items": { "all": true } } } })));
    let cross_discount_id = json_string(
        &cross_discount.body["data"]["discountCodeBasicCreate"]["codeDiscountNode"]["id"],
        "cross discount id",
    );
    assert_synthetic_gid(&cross_discount_id, "DiscountCodeNode");
    assert_eq!(
        cross_discount.body["data"]["discountCodeBasicCreate"]["userErrors"],
        json!([])
    );

    let add = r#"mutation DiscountRedeemCodeBulkValidationAdd($discountId: ID!, $codes: [DiscountRedeemCodeInput!]!) { discountRedeemCodeBulkAdd(discountId: $discountId, codes: $codes) { bulkCreation { id done codesCount importedCount failedCount codes(first: 10) { nodes { code errors { field message code extraInfo } discountRedeemCode { id code } } } } userErrors { field message code extraInfo } } }"#;
    let unknown = proxy.process_request(json_graphql_request(
        add,
        json!({ "discountId": "gid://shopify/DiscountCodeNode/0", "codes": [{"code":"ABC"}] }),
    ));
    assert_eq!(
        unknown.body["data"]["discountRedeemCodeBulkAdd"]["bulkCreation"],
        json!(null)
    );
    assert_eq!(
        unknown.body["data"]["discountRedeemCodeBulkAdd"]["userErrors"],
        json!([{ "field": ["discountId"], "message": "Code discount does not exist.", "code": "INVALID", "extraInfo": null }])
    );

    let too_many_codes: Vec<_> = (0..251)
        .map(|i| json!({ "code": format!("HAR784MAX1778166762181-{i}") }))
        .collect();
    let too_many = proxy.process_request(json_graphql_request(
        add,
        json!({ "discountId": discount_id.clone(), "codes": too_many_codes }),
    ));
    assert_eq!(
        too_many.body["errors"][0]["message"],
        json!("The input array size of 251 is greater than the maximum allowed of 250.")
    );
    assert_eq!(
        too_many.body["errors"][0]["path"],
        json!(["discountRedeemCodeBulkAdd", "codes"])
    );
    assert_eq!(
        too_many.body["errors"][0]["extensions"]["code"],
        json!("MAX_INPUT_SIZE_EXCEEDED")
    );

    let empty = proxy.process_request(json_graphql_request(
        add,
        json!({ "discountId": discount_id.clone(), "codes": [] }),
    ));
    assert_eq!(
        empty.body["data"]["discountRedeemCodeBulkAdd"]["userErrors"],
        json!([{ "field": ["codes"], "message": "Codes can't be blank", "code": "BLANK", "extraInfo": null }])
    );

    let invalid_codes = json!([{"code":""},{"code":"HAR784NL1778166762181\nBAD"},{"code":"HAR784CR1778166762181\rBAD"},{"code":"XXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX"},{"code":"HAR784DUP1778166762181"},{"code":"HAR784DUP1778166762181"},{"code":"HAR784OK1778166762181"}]);
    let invalid_add = proxy.process_request(json_graphql_request(
        add,
        json!({ "discountId": discount_id.clone(), "codes": invalid_codes }),
    ));
    let invalid_bulk_id = invalid_add.body["data"]["discountRedeemCodeBulkAdd"]["bulkCreation"]
        ["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(
        invalid_add.body["data"]["discountRedeemCodeBulkAdd"]["bulkCreation"]["done"],
        json!(false)
    );
    assert_eq!(
        invalid_add.body["data"]["discountRedeemCodeBulkAdd"]["bulkCreation"]["codesCount"],
        json!(7)
    );
    assert_eq!(
        invalid_add.body["data"]["discountRedeemCodeBulkAdd"]["bulkCreation"]["importedCount"],
        json!(0)
    );
    assert_eq!(
        invalid_add.body["data"]["discountRedeemCodeBulkAdd"]["bulkCreation"]["failedCount"],
        json!(0)
    );
    assert_eq!(
        invalid_add.body["data"]["discountRedeemCodeBulkAdd"]["bulkCreation"]["codes"]["nodes"][0]
            ["errors"],
        json!([])
    );

    let creation_read = r#"query DiscountRedeemCodeBulkValidationCreationRead($id: ID!) { discountRedeemCodeBulkCreation(id: $id) { done codesCount importedCount failedCount codes(first: 10) { nodes { code errors { field message code extraInfo } discountRedeemCode { code } } } } }"#;
    let invalid_final = proxy.process_request(json_graphql_request(
        creation_read,
        json!({ "id": invalid_bulk_id }),
    ));
    assert_eq!(
        invalid_final.body["data"]["discountRedeemCodeBulkCreation"]["done"],
        json!(true)
    );
    assert_eq!(
        invalid_final.body["data"]["discountRedeemCodeBulkCreation"]["importedCount"],
        json!(2)
    );
    assert_eq!(
        invalid_final.body["data"]["discountRedeemCodeBulkCreation"]["failedCount"],
        json!(5)
    );
    assert_eq!(
        invalid_final.body["data"]["discountRedeemCodeBulkCreation"]["codes"]["nodes"][0]["errors"]
            [0]["message"],
        json!("is too short (minimum is 1 character)")
    );
    assert_eq!(
        invalid_final.body["data"]["discountRedeemCodeBulkCreation"]["codes"]["nodes"][5]["errors"]
            [0]["message"],
        json!("Codes must be unique within BulkDiscountCodeCreation")
    );

    let read = r#"query DiscountRedeemCodeBulkValidationRead($discountId: ID!, $duplicateCode: String!, $validCode: String!) { codeDiscountNode(id: $discountId) { codeDiscount { ... on DiscountCodeBasic { codes(first: 10) { nodes { code } } codesCount { count precision } } } } duplicate: codeDiscountNodeByCode(code: $duplicateCode) { id } valid: codeDiscountNodeByCode(code: $validCode) { id } }"#;
    let read_after_invalid = proxy.process_request(json_graphql_request(read, json!({ "discountId": discount_id.clone(), "duplicateCode": "HAR784DUP1778166762181", "validCode": "HAR784OK1778166762181" })));
    assert_eq!(
        read_after_invalid.body["data"]["codeDiscountNode"]["codeDiscount"]["codesCount"],
        json!({ "count": 3, "precision": "EXACT" })
    );
    assert_eq!(
        read_after_invalid.body["data"]["duplicate"]["id"],
        json!(discount_id)
    );
    assert_eq!(
        read_after_invalid.body["data"]["valid"]["id"],
        json!(discount_id)
    );

    let conflicts = json!([{"code":"HAR784BASE1778166762181"},{"code":"HAR784CROSS1778166762181"},{"code":"HAR784FRESH1778166762181"}]);
    let conflicts_add = proxy.process_request(json_graphql_request(
        add,
        json!({ "discountId": discount_id.clone(), "codes": conflicts }),
    ));
    let conflicts_bulk_id = conflicts_add.body["data"]["discountRedeemCodeBulkAdd"]["bulkCreation"]
        ["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(
        conflicts_add.body["data"]["discountRedeemCodeBulkAdd"]["bulkCreation"]["done"],
        json!(false)
    );
    assert_eq!(
        conflicts_add.body["data"]["discountRedeemCodeBulkAdd"]["bulkCreation"]["codesCount"],
        json!(3)
    );

    let conflicts_final = proxy.process_request(json_graphql_request(
        creation_read,
        json!({ "id": conflicts_bulk_id }),
    ));
    assert_eq!(
        conflicts_final.body["data"]["discountRedeemCodeBulkCreation"]["importedCount"],
        json!(1)
    );
    assert_eq!(
        conflicts_final.body["data"]["discountRedeemCodeBulkCreation"]["failedCount"],
        json!(2)
    );
    assert_eq!(
        conflicts_final.body["data"]["discountRedeemCodeBulkCreation"]["codes"]["nodes"][0]
            ["errors"][0]["message"],
        json!("must be unique. Please try a different code.")
    );
    assert_eq!(
        conflicts_final.body["data"]["discountRedeemCodeBulkCreation"]["codes"]["nodes"][2]
            ["discountRedeemCode"]["code"],
        json!("HAR784FRESH1778166762181")
    );

    let existing_read = r#"query DiscountRedeemCodeBulkValidationExistingRead($discountId: ID!, $sameDiscountCode: String!, $crossDiscountCode: String!, $freshCode: String!) { codeDiscountNode(id: $discountId) { codeDiscount { ... on DiscountCodeBasic { codes(first: 10) { nodes { code } } codesCount { count precision } } } } sameDiscount: codeDiscountNodeByCode(code: $sameDiscountCode) { id } crossDiscount: codeDiscountNodeByCode(code: $crossDiscountCode) { id } fresh: codeDiscountNodeByCode(code: $freshCode) { id } }"#;
    let read_after_conflicts = proxy.process_request(json_graphql_request(existing_read, json!({ "discountId": discount_id.clone(), "sameDiscountCode": "HAR784BASE1778166762181", "crossDiscountCode": "HAR784CROSS1778166762181", "freshCode": "HAR784FRESH1778166762181" })));
    assert_eq!(
        read_after_conflicts.body["data"]["codeDiscountNode"]["codeDiscount"]["codesCount"],
        json!({ "count": 4, "precision": "EXACT" })
    );
    assert_eq!(
        read_after_conflicts.body["data"]["sameDiscount"]["id"],
        json!(discount_id)
    );
    assert_eq!(
        read_after_conflicts.body["data"]["crossDiscount"]["id"],
        json!(cross_discount_id)
    );
    assert_eq!(
        read_after_conflicts.body["data"]["fresh"]["id"],
        json!(discount_id)
    );
}

#[test]
fn discount_update_edge_cases_reject_bulk_code_change_and_coerce_bxgy() {
    let mut proxy = snapshot_proxy();
    let create_basic = r#"mutation DiscountUpdateEdgeBasicCreate($input: DiscountCodeBasicInput!) { discountCodeBasicCreate(basicCodeDiscount: $input) { codeDiscountNode { id } userErrors { field message code extraInfo } } }"#;
    let created = proxy.process_request(json_graphql_request(create_basic, json!({ "input": { "title": "HAR-605 bulk rule 1778002393771", "code": "HAR605BULK1778002393771", "startsAt": "2026-04-25T00:00:00Z", "context": { "all": "ALL" }, "customerGets": { "value": { "percentage": 0.1 }, "items": { "all": true } } } })));
    assert_eq!(
        created.body["data"]["discountCodeBasicCreate"]["userErrors"],
        json!([])
    );
    let bulk_discount_id = json_string(
        &created.body["data"]["discountCodeBasicCreate"]["codeDiscountNode"]["id"],
        "bulk discount id",
    );
    assert_synthetic_gid(&bulk_discount_id, "DiscountCodeNode");

    let bulk_add = r#"mutation DiscountUpdateEdgeBulkAdd($discountId: ID!, $codes: [DiscountRedeemCodeInput!]!) { discountRedeemCodeBulkAdd(discountId: $discountId, codes: $codes) { bulkCreation { codesCount } userErrors { field message code extraInfo } } }"#;
    let bulk_added = proxy.process_request(json_graphql_request(bulk_add, json!({ "discountId": bulk_discount_id.clone(), "codes": [{"code":"HAR605BULK1778002393771_1"},{"code":"HAR605BULK1778002393771_2"},{"code":"HAR605BULK1778002393771_3"},{"code":"HAR605BULK1778002393771_4"},{"code":"HAR605BULK1778002393771_5"}] })));
    assert_eq!(
        bulk_added.body["data"]["discountRedeemCodeBulkAdd"]["bulkCreation"]["codesCount"],
        json!(5)
    );
    assert_eq!(
        bulk_added.body["data"]["discountRedeemCodeBulkAdd"]["userErrors"],
        json!([])
    );

    let basic_update = r#"mutation DiscountUpdateEdgeBasicUpdate($id: ID!, $input: DiscountCodeBasicInput!) { discountCodeBasicUpdate(id: $id, basicCodeDiscount: $input) { codeDiscountNode { id codeDiscount { __typename } } userErrors { field message code extraInfo } } }"#;
    let code_change = proxy.process_request(json_graphql_request(basic_update, json!({ "id": bulk_discount_id.clone(), "input": { "title": "HAR-605 bulk renamed 1778002393771", "code": "HAR605BULKNEW1778002393771", "startsAt": "2026-04-25T00:00:00Z", "context": { "all": "ALL" }, "customerGets": { "value": { "percentage": 0.2 }, "items": { "all": true } } } })));
    assert_eq!(
        code_change.body["data"]["discountCodeBasicUpdate"]["codeDiscountNode"],
        json!(null)
    );
    assert_eq!(
        code_change.body["data"]["discountCodeBasicUpdate"]["userErrors"],
        json!([{ "field": ["id"], "message": "Cannot update the code of a bulk discount.", "code": null, "extraInfo": null }])
    );

    let create_bxgy = r#"mutation DiscountUpdateEdgeBxgyCreate($input: DiscountCodeBxgyInput!) { discountCodeBxgyCreate(bxgyCodeDiscount: $input) { codeDiscountNode { id codeDiscount { __typename } } userErrors { field message code extraInfo } } }"#;
    let bxgy = proxy.process_request(json_graphql_request(create_bxgy, json!({ "input": { "title": "HAR-605 BXGY 1778002393771", "code": "HAR605BXGY1778002393771", "startsAt": "2026-04-25T00:00:00Z", "context": { "all": "ALL" }, "customerBuys": { "value": { "quantity": "1" }, "items": { "products": { "productsToAdd": ["gid://shopify/Product/10177504608562"] } } }, "customerGets": { "value": { "discountOnQuantity": { "quantity": "1", "effect": { "percentage": 0.5 } } }, "items": { "products": { "productsToAdd": ["gid://shopify/Product/10177504641330"] } } } } })));
    assert_eq!(
        bxgy.body["data"]["discountCodeBxgyCreate"]["codeDiscountNode"]["codeDiscount"]
            ["__typename"],
        json!("DiscountCodeBxgy")
    );
    let bxgy_id = json_string(
        &bxgy.body["data"]["discountCodeBxgyCreate"]["codeDiscountNode"]["id"],
        "bxgy discount id",
    );
    assert_synthetic_gid(&bxgy_id, "DiscountCodeNode");

    let bxgy_to_basic = proxy.process_request(json_graphql_request(basic_update, json!({ "id": bxgy_id, "input": { "title": "HAR-605 coerced basic 1778002393771", "code": "HAR605BXGY1778002393771", "startsAt": "2026-04-25T00:00:00Z", "context": { "all": "ALL" }, "customerGets": { "value": { "percentage": 0.25 }, "items": { "all": true } } } })));
    assert_eq!(
        bxgy_to_basic.body["data"]["discountCodeBasicUpdate"]["codeDiscountNode"]["codeDiscount"]
            ["__typename"],
        json!("DiscountCodeBasic")
    );
    assert_eq!(
        bxgy_to_basic.body["data"]["discountCodeBasicUpdate"]["userErrors"],
        json!([])
    );

    let unknown = r#"mutation DiscountUpdateEdgeUnknownUpdate($id: ID!, $input: DiscountCodeBasicInput!) { discountCodeBasicUpdate(id: $id, basicCodeDiscount: $input) { codeDiscountNode { id } userErrors { field message code extraInfo } } }"#;
    let unknown_response = proxy.process_request(json_graphql_request(unknown, json!({ "id": "gid://shopify/DiscountCodeNode/0", "input": { "title": "HAR-605 unknown 1778002393771", "code": "HAR605UNKNOWN1778002393771", "startsAt": "2026-04-25T00:00:00Z", "context": { "all": "ALL" }, "customerGets": { "value": { "percentage": 0.1 }, "items": { "all": true } } } })));
    assert_eq!(
        unknown_response.body["data"]["discountCodeBasicUpdate"]["codeDiscountNode"],
        json!(null)
    );
    assert_eq!(
        unknown_response.body["data"]["discountCodeBasicUpdate"]["userErrors"],
        json!([{ "field": ["id"], "message": "Discount does not exist", "code": null, "extraInfo": null }])
    );
}

#[test]
fn discount_subscription_fields_not_permitted_matches_local_runtime_gating() {
    let mut proxy = snapshot_proxy();
    let primary = r#"
        mutation DiscountSubscriptionFieldsNotPermitted {
          basicSub: discountCodeBasicCreate(basicCodeDiscount: { title: "Sub gated", code: "SUB-GATED", startsAt: "2026-04-25T00:00:00Z", customerGets: { value: { percentage: 0.1 }, items: { all: true }, appliesOnSubscription: true } }) { codeDiscountNode { id } userErrors { field message code extraInfo } }
          basicBlank: discountCodeBasicCreate(basicCodeDiscount: { title: "Sub blank", code: "SUB-BLANK", startsAt: "2026-04-25T00:00:00Z", customerGets: { value: { percentage: 0.1 }, items: { all: true }, appliesOnSubscription: null } }) { codeDiscountNode { id } userErrors { field message code extraInfo } }
          freeShippingSub: discountCodeFreeShippingCreate(freeShippingCodeDiscount: { title: "Free shipping sub gated", code: "SHIP-SUB-GATED", startsAt: "2026-04-25T00:00:00Z", destination: { all: true }, appliesOnSubscription: true }) { codeDiscountNode { id } userErrors { field message code extraInfo } }
          freeShippingRecurring: discountCodeFreeShippingCreate(freeShippingCodeDiscount: { title: "Free shipping recurring gated", code: "SHIP-REC-GATED", startsAt: "2026-04-25T00:00:00Z", destination: { all: true }, recurringCycleLimit: 2 }) { codeDiscountNode { id } userErrors { field message code extraInfo } }
          automaticBasicSub: discountAutomaticBasicCreate(automaticBasicDiscount: { title: "Automatic basic sub gated", startsAt: "2026-04-25T00:00:00Z", customerGets: { value: { percentage: 0.1 }, items: { all: true }, appliesOnSubscription: true } }) { automaticDiscountNode { id } userErrors { field message code extraInfo } }
          automaticBasicRecurring: discountAutomaticBasicCreate(automaticBasicDiscount: { title: "Automatic basic recurring gated", startsAt: "2026-04-25T00:00:00Z", recurringCycleLimit: 2, customerGets: { value: { percentage: 0.1 }, items: { all: true } } }) { automaticDiscountNode { id } userErrors { field message code extraInfo } }
          automaticFreeShippingSkip: discountAutomaticFreeShippingCreate(freeShippingAutomaticDiscount: { title: "Automatic shipping skip", startsAt: "2026-04-25T00:00:00Z", destination: { all: true }, appliesOnSubscription: true, appliesOnOneTimePurchase: false, recurringCycleLimit: 2 }) { automaticDiscountNode { id } userErrors { field message code extraInfo } }
          setupBasic: discountCodeBasicCreate(basicCodeDiscount: { title: "Setup basic", code: "SETUP-BASIC-SUB", startsAt: "2026-04-25T00:00:00Z", customerGets: { value: { percentage: 0.1 }, items: { all: true } } }) { codeDiscountNode { id } userErrors { field message code extraInfo } }
          setupFreeShipping: discountCodeFreeShippingCreate(freeShippingCodeDiscount: { title: "Setup shipping", code: "SETUP-SHIP-SUB", startsAt: "2026-04-25T00:00:00Z", destination: { all: true } }) { codeDiscountNode { id } userErrors { field message code extraInfo } }
          setupAutomaticBasic: discountAutomaticBasicCreate(automaticBasicDiscount: { title: "Setup automatic basic", startsAt: "2026-04-25T00:00:00Z", customerGets: { value: { percentage: 0.1 }, items: { all: true } } }) { automaticDiscountNode { id } userErrors { field message code extraInfo } }
        }
    "#;
    let response = proxy.process_request(json_graphql_request(primary, json!({})));
    assert_eq!(
        response.body["data"]["basicSub"]["codeDiscountNode"],
        json!(null)
    );
    assert_eq!(
        response.body["data"]["basicSub"]["userErrors"][0]["field"],
        json!(["basicCodeDiscount", "customerGets", "appliesOnSubscription"])
    );
    assert_eq!(
        response.body["data"]["freeShippingRecurring"]["userErrors"][0]["message"],
        json!("Recurring cycle limit is not permitted for this shop.")
    );
    let automatic_free_shipping_id = json_string(
        &response.body["data"]["automaticFreeShippingSkip"]["automaticDiscountNode"]["id"],
        "automatic free shipping id",
    );
    let setup_basic_id = json_string(
        &response.body["data"]["setupBasic"]["codeDiscountNode"]["id"],
        "setup basic discount id",
    );
    let setup_free_shipping_id = json_string(
        &response.body["data"]["setupFreeShipping"]["codeDiscountNode"]["id"],
        "setup free shipping id",
    );
    let setup_automatic_basic_id = json_string(
        &response.body["data"]["setupAutomaticBasic"]["automaticDiscountNode"]["id"],
        "setup automatic basic id",
    );
    assert_synthetic_gid(&automatic_free_shipping_id, "DiscountAutomaticNode");
    assert_synthetic_gid(&setup_basic_id, "DiscountCodeNode");
    assert_synthetic_gid(&setup_free_shipping_id, "DiscountCodeNode");
    assert_synthetic_gid(&setup_automatic_basic_id, "DiscountAutomaticNode");

    let basic_update = r#"mutation DiscountSubscriptionFieldsBasicUpdate($id: ID!) { basicUpdate: discountCodeBasicUpdate(id: $id, basicCodeDiscount: { title: "Setup basic", code: "SETUP-BASIC-SUB", startsAt: "2026-04-25T00:00:00Z", customerGets: { value: { percentage: 0.1 }, items: { all: true }, appliesOnSubscription: true } }) { codeDiscountNode { id } userErrors { field message code extraInfo } } }"#;
    let updated = proxy.process_request(json_graphql_request(
        basic_update,
        json!({ "id": setup_basic_id }),
    ));
    assert_eq!(
        updated.body["data"]["basicUpdate"]["codeDiscountNode"],
        json!(null)
    );
    assert_eq!(
        updated.body["data"]["basicUpdate"]["userErrors"][0]["message"],
        json!("Customer gets applies on subscription is not permitted for this shop.")
    );

    let free_shipping_update = r#"mutation DiscountSubscriptionFieldsFreeShippingUpdate($id: ID!) { freeShippingUpdate: discountCodeFreeShippingUpdate(id: $id, freeShippingCodeDiscount: { title: "Setup shipping", code: "SETUP-SHIP-SUB", startsAt: "2026-04-25T00:00:00Z", destination: { all: true }, appliesOnSubscription: true }) { codeDiscountNode { id } userErrors { field message code extraInfo } } }"#;
    let free_shipping_updated = proxy.process_request(json_graphql_request(
        free_shipping_update,
        json!({ "id": setup_free_shipping_id }),
    ));
    assert_eq!(
        free_shipping_updated.body["data"]["freeShippingUpdate"]["codeDiscountNode"],
        json!(null)
    );
    assert_eq!(
        free_shipping_updated.body["data"]["freeShippingUpdate"]["userErrors"][0]["message"],
        json!("Applies on subscription is not permitted for this shop.")
    );

    let automatic_basic_update = r#"mutation DiscountSubscriptionFieldsAutomaticBasicUpdate($id: ID!) { automaticBasicUpdate: discountAutomaticBasicUpdate(id: $id, automaticBasicDiscount: { title: "Setup automatic basic", startsAt: "2026-04-25T00:00:00Z", customerGets: { value: { percentage: 0.1 }, items: { all: true }, appliesOnSubscription: true } }) { automaticDiscountNode { id } userErrors { field message code extraInfo } } }"#;
    let automatic_basic_updated = proxy.process_request(json_graphql_request(
        automatic_basic_update,
        json!({ "id": setup_automatic_basic_id }),
    ));
    assert_eq!(
        automatic_basic_updated.body["data"]["automaticBasicUpdate"]["automaticDiscountNode"],
        json!(null)
    );
    assert_eq!(
        automatic_basic_updated.body["data"]["automaticBasicUpdate"]["userErrors"][0]["message"],
        json!("Customer gets applies on subscription is not permitted for this shop.")
    );

    let automatic_free_shipping_update = r#"mutation DiscountSubscriptionFieldsAutomaticFreeShippingUpdate($id: ID!) { automaticFreeShippingUpdate: discountAutomaticFreeShippingUpdate(id: $id, freeShippingAutomaticDiscount: { title: "Automatic shipping skip", startsAt: "2026-04-25T00:00:00Z", destination: { all: true }, appliesOnSubscription: true, appliesOnOneTimePurchase: false, recurringCycleLimit: 3 }) { automaticDiscountNode { id } userErrors { field message code extraInfo } } }"#;
    let automatic_free_shipping_updated = proxy.process_request(json_graphql_request(
        automatic_free_shipping_update,
        json!({ "id": automatic_free_shipping_id.clone() }),
    ));
    assert_eq!(
        automatic_free_shipping_updated.body["data"]["automaticFreeShippingUpdate"]
            ["automaticDiscountNode"]["id"],
        json!(automatic_free_shipping_id)
    );
    assert_eq!(
        automatic_free_shipping_updated.body["data"]["automaticFreeShippingUpdate"]["userErrors"],
        json!([])
    );
}

#[test]
fn discount_status_time_window_derives_create_and_read_filters() {
    let mut proxy = snapshot_proxy();
    let create_query = r#"
        mutation DiscountStatusTimeWindowDerivationCreate(
          $scheduled: DiscountCodeBasicInput!
          $expired: DiscountCodeBasicInput!
          $active: DiscountCodeBasicInput!
        ) {
          scheduled: discountCodeBasicCreate(basicCodeDiscount: $scheduled) { codeDiscountNode { id codeDiscount { __typename ... on DiscountCodeBasic { title status startsAt endsAt } } } userErrors { field message code extraInfo } }
          expired: discountCodeBasicCreate(basicCodeDiscount: $expired) { codeDiscountNode { id codeDiscount { __typename ... on DiscountCodeBasic { title status startsAt endsAt } } } userErrors { field message code extraInfo } }
          active: discountCodeBasicCreate(basicCodeDiscount: $active) { codeDiscountNode { id codeDiscount { __typename ... on DiscountCodeBasic { title status startsAt endsAt } } } userErrors { field message code extraInfo } }
        }
    "#;
    let created = proxy.process_request(json_graphql_request(create_query, json!({
        "scheduled": { "title": "HAR-593 scheduled 1777950794226", "code": "HAR593S1777950794226", "startsAt": "2099-01-01T00:00:00Z", "context": { "all": "ALL" }, "customerGets": { "value": { "percentage": 0.1 }, "items": { "all": true } } },
        "expired": { "title": "HAR-593 expired 1777950794226", "code": "HAR593E1777950794226", "startsAt": "2019-01-01T00:00:00Z", "endsAt": "2020-01-01T00:00:00Z", "context": { "all": "ALL" }, "customerGets": { "value": { "percentage": 0.1 }, "items": { "all": true } } },
        "active": { "title": "HAR-593 active 1777950794226", "code": "HAR593A1777950794226", "startsAt": "2020-01-01T00:00:00Z", "endsAt": "2099-01-01T00:00:00Z", "context": { "all": "ALL" }, "customerGets": { "value": { "percentage": 0.1 }, "items": { "all": true } } }
    })));
    assert_eq!(
        created.body["data"]["scheduled"]["codeDiscountNode"]["codeDiscount"]["status"],
        json!("SCHEDULED")
    );
    assert_eq!(
        created.body["data"]["expired"]["codeDiscountNode"]["codeDiscount"]["status"],
        json!("EXPIRED")
    );
    assert_eq!(
        created.body["data"]["active"]["codeDiscountNode"]["codeDiscount"]["status"],
        json!("ACTIVE")
    );
    assert_eq!(created.body["data"]["scheduled"]["userErrors"], json!([]));
    let scheduled_id = json_string(
        &created.body["data"]["scheduled"]["codeDiscountNode"]["id"],
        "scheduled discount id",
    );
    let expired_id = json_string(
        &created.body["data"]["expired"]["codeDiscountNode"]["id"],
        "expired discount id",
    );
    let active_id = json_string(
        &created.body["data"]["active"]["codeDiscountNode"]["id"],
        "active discount id",
    );
    for id in [&scheduled_id, &expired_id, &active_id] {
        assert_synthetic_gid(id, "DiscountCodeNode");
    }

    let read_query = r#"
        query DiscountStatusTimeWindowDerivationRead($scheduledId: ID!, $expiredId: ID!, $activeId: ID!, $scheduledQuery: String!, $expiredQuery: String!) {
          scheduledNode: codeDiscountNode(id: $scheduledId) { codeDiscount { __typename ... on DiscountCodeBasic { title status startsAt endsAt } } }
          expiredNode: codeDiscountNode(id: $expiredId) { codeDiscount { __typename ... on DiscountCodeBasic { title status startsAt endsAt } } }
          activeNode: discountNode(id: $activeId) { discount { __typename ... on DiscountCodeBasic { title status startsAt endsAt } } }
          scheduledDiscountNodes: discountNodes(first: 5, query: $scheduledQuery) { nodes { discount { __typename ... on DiscountCodeBasic { title status } } } }
          expiredDiscountNodesCount: discountNodesCount(query: $expiredQuery) { count precision }
        }
    "#;
    let read = proxy.process_request(json_graphql_request(
        read_query,
        json!({
            "scheduledId": scheduled_id,
            "expiredId": expired_id,
            "activeId": active_id,
            "scheduledQuery": "status:scheduled title:'HAR-593 scheduled 1777950794226'",
            "expiredQuery": "status:expired title:'HAR-593 expired 1777950794226'"
        }),
    ));
    assert_eq!(
        read.body["data"]["scheduledNode"]["codeDiscount"]["status"],
        json!("SCHEDULED")
    );
    assert_eq!(
        read.body["data"]["expiredNode"]["codeDiscount"]["endsAt"],
        json!("2020-01-01T00:00:00Z")
    );
    assert_eq!(
        read.body["data"]["activeNode"]["discount"]["title"],
        json!("HAR-593 active 1777950794226")
    );
    assert_eq!(
        read.body["data"]["scheduledDiscountNodes"]["nodes"],
        json!([{ "discount": { "__typename": "DiscountCodeBasic", "title": "HAR-593 scheduled 1777950794226", "status": "SCHEDULED" } }])
    );
    assert_eq!(
        read.body["data"]["expiredDiscountNodesCount"],
        json!({ "count": 1, "precision": "EXACT" })
    );
}

#[test]
fn discount_free_shipping_lifecycle_stages_code_and_automatic_statuses() {
    let mut proxy = snapshot_proxy();
    restore_shop_currency(&mut proxy, "USD");
    let create_query = r#"
        mutation DiscountFreeShippingLifecycleCreate($codeInput: DiscountCodeFreeShippingInput!, $automaticInput: DiscountAutomaticFreeShippingInput!) {
          discountCodeFreeShippingCreate(freeShippingCodeDiscount: $codeInput) { codeDiscountNode { id codeDiscount { __typename ... on DiscountCodeFreeShipping { title summary asyncUsageCount discountClasses combinesWith { productDiscounts orderDiscounts shippingDiscounts } codes(first: 2) { nodes { code asyncUsageCount } } destinationSelection { __typename ... on DiscountCountryAll { allCountries } ... on DiscountCountries { countries includeRestOfWorld } } maximumShippingPrice { amount currencyCode } appliesOncePerCustomer appliesOnOneTimePurchase appliesOnSubscription recurringCycleLimit usageLimit } } } userErrors { field message code extraInfo } }
          discountAutomaticFreeShippingCreate(freeShippingAutomaticDiscount: $automaticInput) { automaticDiscountNode { id automaticDiscount { __typename ... on DiscountAutomaticFreeShipping { title summary asyncUsageCount discountClasses combinesWith { productDiscounts orderDiscounts shippingDiscounts } destinationSelection { __typename ... on DiscountCountryAll { allCountries } ... on DiscountCountries { countries includeRestOfWorld } } maximumShippingPrice { amount currencyCode } appliesOnOneTimePurchase appliesOnSubscription recurringCycleLimit } } } userErrors { field message code extraInfo } }
        }
    "#;
    let created = proxy.process_request(json_graphql_request(create_query, json!({
        "codeInput": { "title": "HAR-196 code free shipping 1777150170404", "code": "HAR196FREE1777150170404", "startsAt": "2026-04-25T20:48:30.404Z", "combinesWith": { "productDiscounts": true, "orderDiscounts": false, "shippingDiscounts": false }, "context": { "all": "ALL" }, "minimumRequirement": { "subtotal": { "greaterThanOrEqualToSubtotal": "10.00" } }, "destination": { "all": true }, "maximumShippingPrice": "25.00", "appliesOncePerCustomer": true, "usageLimit": 5 },
        "automaticInput": { "title": "HAR-196 automatic free shipping 1777150170404", "startsAt": "2026-04-25T20:48:30.404Z", "endsAt": null, "combinesWith": { "productDiscounts": false, "orderDiscounts": true, "shippingDiscounts": false }, "context": { "all": "ALL" }, "minimumRequirement": { "subtotal": { "greaterThanOrEqualToSubtotal": "15.00" } }, "destination": { "all": true }, "maximumShippingPrice": "20.00" }
    })));
    assert_eq!(
        created.body["data"]["discountCodeFreeShippingCreate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        created.body["data"]["discountCodeFreeShippingCreate"]["codeDiscountNode"]["codeDiscount"]
            ["codes"]["nodes"][0]["code"],
        json!("HAR196FREE1777150170404")
    );
    assert_eq!(
        created.body["data"]["discountCodeFreeShippingCreate"]["codeDiscountNode"]["codeDiscount"]
            ["summary"],
        json!("Free shipping on all products • Minimum purchase of $10.00 • For all countries • Applies to shipping rates under $25.00 • One use per customer")
    );
    assert_eq!(
        created.body["data"]["discountAutomaticFreeShippingCreate"]["automaticDiscountNode"]
            ["automaticDiscount"]["maximumShippingPrice"],
        json!({ "amount": "20.0", "currencyCode": "USD" })
    );
    assert_eq!(
        created.body["data"]["discountAutomaticFreeShippingCreate"]["automaticDiscountNode"]
            ["automaticDiscount"]["summary"],
        json!("Free shipping on all products • Minimum purchase of $15.00 • For all countries • Applies to shipping rates under $20.00")
    );
    let code_id = json_string(
        &created.body["data"]["discountCodeFreeShippingCreate"]["codeDiscountNode"]["id"],
        "free shipping code discount id",
    );
    let automatic_id = json_string(
        &created.body["data"]["discountAutomaticFreeShippingCreate"]["automaticDiscountNode"]["id"],
        "free shipping automatic discount id",
    );
    assert_synthetic_gid(&code_id, "DiscountCodeNode");
    assert_synthetic_gid(&automatic_id, "DiscountAutomaticNode");

    let code_update = r#"mutation DiscountCodeFreeShippingLifecycleUpdate($id: ID!, $input: DiscountCodeFreeShippingInput!) { discountCodeFreeShippingUpdate(id: $id, freeShippingCodeDiscount: $input) { codeDiscountNode { id codeDiscount { __typename ... on DiscountCodeFreeShipping { title summary destinationSelection { __typename ... on DiscountCountries { countries includeRestOfWorld } } maximumShippingPrice { amount currencyCode } appliesOncePerCustomer appliesOnOneTimePurchase appliesOnSubscription recurringCycleLimit usageLimit } } } userErrors { field message code extraInfo } } }"#;
    let updated = proxy.process_request(json_graphql_request(
        code_update,
        json!({ "id": code_id.clone(), "input": { "title": "HAR-196 code free shipping updated 1777150170404", "code": "HAR196SHIP1777150170404", "startsAt": "2026-04-25T20:48:30.404Z", "combinesWith": { "productDiscounts": true, "orderDiscounts": false, "shippingDiscounts": false }, "context": { "all": "ALL" }, "minimumRequirement": { "subtotal": { "greaterThanOrEqualToSubtotal": "12.00" } }, "destination": { "countries": { "add": ["CA", "US"] } }, "maximumShippingPrice": "30.00", "appliesOncePerCustomer": false, "appliesOnOneTimePurchase": false, "appliesOnSubscription": true, "usageLimit": 10 } }),
    ));
    assert_eq!(
        updated.body["data"]["discountCodeFreeShippingUpdate"]["codeDiscountNode"]["codeDiscount"]
            ["destinationSelection"],
        json!({ "__typename": "DiscountCountries", "countries": [], "includeRestOfWorld": false })
    );
    assert_eq!(
        updated.body["data"]["discountCodeFreeShippingUpdate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        updated.body["data"]["discountCodeFreeShippingUpdate"]["codeDiscountNode"]["codeDiscount"]
            ["summary"],
        json!("Free shipping on subscription products • Minimum purchase of $12.00 • For 2 countries • Applies to shipping rates under $30.00")
    );

    let automatic_update = r#"mutation DiscountAutomaticFreeShippingLifecycleUpdate($id: ID!, $input: DiscountAutomaticFreeShippingInput!) { discountAutomaticFreeShippingUpdate(id: $id, freeShippingAutomaticDiscount: $input) { automaticDiscountNode { id automaticDiscount { __typename ... on DiscountAutomaticFreeShipping { title summary destinationSelection { __typename ... on DiscountCountries { countries includeRestOfWorld } } maximumShippingPrice { amount currencyCode } appliesOnOneTimePurchase appliesOnSubscription recurringCycleLimit } } } userErrors { field message code extraInfo } } }"#;
    let automatic_updated = proxy.process_request(json_graphql_request(
        automatic_update,
        json!({ "id": automatic_id.clone(), "input": { "title": "HAR-196 automatic free shipping updated 1777150170404", "startsAt": "2026-04-25T20:48:30.404Z", "combinesWith": { "productDiscounts": false, "orderDiscounts": true, "shippingDiscounts": false }, "context": { "all": "ALL" }, "minimumRequirement": { "subtotal": { "greaterThanOrEqualToSubtotal": "18.00" } }, "destination": { "countries": { "add": ["US"] } }, "maximumShippingPrice": "18.00" } }),
    ));
    assert_eq!(
        automatic_updated.body["data"]["discountAutomaticFreeShippingUpdate"]
            ["automaticDiscountNode"]["automaticDiscount"]["destinationSelection"],
        json!({ "__typename": "DiscountCountries", "countries": [], "includeRestOfWorld": false })
    );
    assert_eq!(
        automatic_updated.body["data"]["discountAutomaticFreeShippingUpdate"]
            ["automaticDiscountNode"]["automaticDiscount"]["summary"],
        json!("Free shipping on all products • Minimum purchase of $18.00 • For United States • Applies to shipping rates under $18.00")
    );

    let read_query = r#"query DiscountFreeShippingLifecycleRead($codeId: ID!, $automaticId: ID!, $code: String!) { discountNode(id: $codeId) { id discount { __typename ... on DiscountCodeFreeShipping { title status } } } codeDiscountNodeByCode(code: $code) { id } automaticDiscountNode(id: $automaticId) { id automaticDiscount { __typename ... on DiscountAutomaticFreeShipping { title status } } } }"#;
    let read_after_update = proxy.process_request(json_graphql_request(read_query, json!({ "codeId": code_id.clone(), "automaticId": automatic_id.clone(), "code": "HAR196SHIP1777150170404" })));
    assert_eq!(
        read_after_update.body["data"]["discountNode"]["discount"]["title"],
        json!("HAR-196 code free shipping updated 1777150170404")
    );
    assert_eq!(
        read_after_update.body["data"]["automaticDiscountNode"]["automaticDiscount"]["status"],
        json!("ACTIVE")
    );

    let code_deactivate = r#"mutation DiscountFreeShippingLifecycleDeactivate($id: ID!) { discountCodeDeactivate(id: $id) { codeDiscountNode { id codeDiscount { __typename ... on DiscountCodeFreeShipping { title status } } } userErrors { field message code extraInfo } } }"#;
    let code_deactivated = proxy.process_request(json_graphql_request(
        code_deactivate,
        json!({ "id": code_id.clone() }),
    ));
    assert_eq!(
        code_deactivated.body["data"]["discountCodeDeactivate"]["codeDiscountNode"]["codeDiscount"]
            ["status"],
        json!("EXPIRED")
    );

    let automatic_delete = r#"mutation DiscountFreeShippingLifecycleDelete($id: ID!) { discountAutomaticDelete(id: $id) { deletedAutomaticDiscountId userErrors { field message code extraInfo } } }"#;
    let automatic_deleted = proxy.process_request(json_graphql_request(
        automatic_delete,
        json!({ "id": automatic_id.clone() }),
    ));
    assert_eq!(
        automatic_deleted.body["data"]["discountAutomaticDelete"]["userErrors"],
        json!([])
    );

    let code_delete = r#"mutation DiscountFreeShippingLifecycleDelete($id: ID!) { discountCodeDelete(id: $id) { deletedCodeDiscountId userErrors { field message code extraInfo } } }"#;
    let _ = proxy.process_request(json_graphql_request(
        code_delete,
        json!({ "id": code_id.clone() }),
    ));
    let read_after_delete = proxy.process_request(json_graphql_request(read_query, json!({ "codeId": code_id, "automaticId": automatic_id, "code": "HAR196SHIP1777150170404" })));
    assert_eq!(read_after_delete.body["data"]["discountNode"], json!(null));
    assert_eq!(
        read_after_delete.body["data"]["codeDiscountNodeByCode"],
        json!(null)
    );
    assert_eq!(
        read_after_delete.body["data"]["automaticDiscountNode"],
        json!(null)
    );
}

#[test]
fn discount_class_inference_stages_all_discount_classes_and_product_count() {
    let mut proxy = snapshot_proxy();
    let create_query = r#"
        mutation DiscountClassInferenceCreate(
          $basicAll: DiscountCodeBasicInput!
          $basicProduct: DiscountCodeBasicInput!
          $basicCollection: DiscountCodeBasicInput!
          $bxgy: DiscountCodeBxgyInput!
          $freeShipping: DiscountCodeFreeShippingInput!
        ) {
          basicAll: discountCodeBasicCreate(basicCodeDiscount: $basicAll) { codeDiscountNode { codeDiscount { __typename ... on DiscountCodeBasic { title discountClasses } } } userErrors { field message code extraInfo } }
          basicProduct: discountCodeBasicCreate(basicCodeDiscount: $basicProduct) { codeDiscountNode { codeDiscount { __typename ... on DiscountCodeBasic { title discountClasses } } } userErrors { field message code extraInfo } }
          basicCollection: discountCodeBasicCreate(basicCodeDiscount: $basicCollection) { codeDiscountNode { codeDiscount { __typename ... on DiscountCodeBasic { title discountClasses } } } userErrors { field message code extraInfo } }
          bxgy: discountCodeBxgyCreate(bxgyCodeDiscount: $bxgy) { codeDiscountNode { codeDiscount { __typename ... on DiscountCodeBxgy { title discountClasses } } } userErrors { field message code extraInfo } }
          freeShipping: discountCodeFreeShippingCreate(freeShippingCodeDiscount: $freeShipping) { codeDiscountNode { codeDiscount { __typename ... on DiscountCodeFreeShipping { title discountClasses } } } userErrors { field message code extraInfo } }
        }
    "#;
    let created = proxy.process_request(json_graphql_request(
        create_query,
        json!({
            "basicAll": { "title": "HAR597CLASS1777950382203 basic order", "code": "HAR597ORDER1777950382203", "startsAt": "2026-05-05T03:05:22.203Z", "context": { "all": "ALL" }, "customerGets": { "value": { "percentage": 0.1 }, "items": { "all": true } } },
            "basicProduct": { "title": "HAR597CLASS1777950382203 basic product", "code": "HAR597PRODUCT1777950382203", "startsAt": "2026-05-05T03:05:22.203Z", "context": { "all": "ALL" }, "customerGets": { "value": { "percentage": 0.1 }, "items": { "products": { "productsToAdd": ["gid://shopify/Product/10177002799410"] } } } },
            "basicCollection": { "title": "HAR597CLASS1777950382203 basic collection", "code": "HAR597COLL1777950382203", "startsAt": "2026-05-05T03:05:22.203Z", "context": { "all": "ALL" }, "customerGets": { "value": { "percentage": 0.1 }, "items": { "collections": { "add": ["gid://shopify/Collection/512409665842"] } } } },
            "bxgy": { "title": "HAR597CLASS1777950382203 bxgy product", "code": "HAR597BXGY1777950382203", "startsAt": "2026-05-05T03:05:22.203Z", "context": { "all": "ALL" }, "customerBuys": { "value": { "quantity": "1" }, "items": { "products": { "productsToAdd": ["gid://shopify/Product/10177002832178"] } } }, "customerGets": { "value": { "discountOnQuantity": { "quantity": "1", "effect": { "percentage": 0.5 } } }, "items": { "products": { "productsToAdd": ["gid://shopify/Product/10177002799410"] } } } },
            "freeShipping": { "title": "HAR597CLASS1777950382203 free shipping", "code": "HAR597SHIP1777950382203", "startsAt": "2026-05-05T03:05:22.203Z", "context": { "all": "ALL" }, "destination": { "all": true } }
        }),
    ));

    assert_eq!(
        created.body["data"]["basicAll"]["codeDiscountNode"]["codeDiscount"],
        json!({ "__typename": "DiscountCodeBasic", "title": "HAR597CLASS1777950382203 basic order", "discountClasses": ["ORDER"] })
    );
    assert_eq!(
        created.body["data"]["basicProduct"]["codeDiscountNode"]["codeDiscount"]["discountClasses"],
        json!(["PRODUCT"])
    );
    assert_eq!(
        created.body["data"]["basicCollection"]["codeDiscountNode"]["codeDiscount"]
            ["discountClasses"],
        json!(["PRODUCT"])
    );
    assert_eq!(
        created.body["data"]["bxgy"]["codeDiscountNode"]["codeDiscount"],
        json!({ "__typename": "DiscountCodeBxgy", "title": "HAR597CLASS1777950382203 bxgy product", "discountClasses": ["PRODUCT"] })
    );
    assert_eq!(
        created.body["data"]["freeShipping"]["codeDiscountNode"]["codeDiscount"],
        json!({ "__typename": "DiscountCodeFreeShipping", "title": "HAR597CLASS1777950382203 free shipping", "discountClasses": ["SHIPPING"] })
    );
    assert_eq!(
        created.body["data"]["freeShipping"]["userErrors"],
        json!([])
    );

    let read = proxy.process_request(json_graphql_request(
        r#"query DiscountClassInferenceRead($productQuery: String!) { discountNodesCount(query: $productQuery) { count precision } }"#,
        json!({ "productQuery": "discount_class:product HAR597CLASS1777950382203" }),
    ));
    assert_eq!(
        read.body["data"]["discountNodesCount"],
        json!({ "count": 3, "precision": "EXACT" })
    );
}

#[test]
fn discount_code_basic_lifecycle_tracks_status_counts_and_delete_readback() {
    let mut proxy = snapshot_proxy();
    let create_query = r#"
        mutation DiscountCodeBasicLifecycleCreate($input: DiscountCodeBasicInput!) {
          discountCodeBasicCreate(basicCodeDiscount: $input) {
            codeDiscountNode { id codeDiscount { __typename ... on DiscountCodeBasic { title status asyncUsageCount discountClasses combinesWith { productDiscounts orderDiscounts shippingDiscounts } codes(first: 2) { nodes { code asyncUsageCount } } context { __typename ... on DiscountBuyerSelectionAll { all } } } } }
            userErrors { field message code extraInfo }
          }
        }
    "#;
    let create_input = json!({
        "title": "HAR-193 lifecycle 1777318334676",
        "code": "HAR193LIFE1777318334676",
        "startsAt": "2026-04-27T19:31:14.676Z",
        "combinesWith": { "productDiscounts": false, "orderDiscounts": true, "shippingDiscounts": false },
        "context": { "all": "ALL" },
        "minimumRequirement": { "subtotal": { "greaterThanOrEqualToSubtotal": "1.00" } },
        "customerGets": { "value": { "percentage": 0.1 }, "items": { "all": true } }
    });
    let created = proxy.process_request(json_graphql_request(
        create_query,
        json!({ "input": create_input }),
    ));
    assert_eq!(
        created.body["data"]["discountCodeBasicCreate"]["userErrors"],
        json!([])
    );
    let discount_id = json_string(
        &created.body["data"]["discountCodeBasicCreate"]["codeDiscountNode"]["id"],
        "code basic lifecycle discount id",
    );
    assert_synthetic_gid(&discount_id, "DiscountCodeNode");
    assert_eq!(
        created.body["data"]["discountCodeBasicCreate"]["codeDiscountNode"]["codeDiscount"]
            ["codes"]["nodes"][0]["code"],
        json!("HAR193LIFE1777318334676")
    );

    let update_query = r#"
        mutation DiscountCodeBasicLifecycleUpdate($id: ID!, $input: DiscountCodeBasicInput!) {
          discountCodeBasicUpdate(id: $id, basicCodeDiscount: $input) {
            codeDiscountNode { id codeDiscount { __typename ... on DiscountCodeBasic { title status asyncUsageCount discountClasses combinesWith { productDiscounts orderDiscounts shippingDiscounts } codes(first: 2) { nodes { code asyncUsageCount } } customerGets { items { __typename ... on AllDiscountItems { allItems } } } } } }
            userErrors { field message code extraInfo }
          }
        }
    "#;
    let update_input = json!({
        "title": "HAR-193 lifecycle updated 1777318334676",
        "code": "HAR193LIVE1777318334676",
        "startsAt": "2026-04-27T19:31:14.676Z",
        "combinesWith": { "productDiscounts": false, "orderDiscounts": true, "shippingDiscounts": false },
        "context": { "all": "ALL" },
        "minimumRequirement": { "subtotal": { "greaterThanOrEqualToSubtotal": "2.00" } },
        "customerGets": { "value": { "discountAmount": { "amount": "5.00", "appliesOnEachItem": false } }, "items": { "all": true } }
    });
    let updated = proxy.process_request(json_graphql_request(
        update_query,
        json!({ "id": discount_id.clone(), "input": update_input }),
    ));
    assert_eq!(
        updated.body["data"]["discountCodeBasicUpdate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        updated.body["data"]["discountCodeBasicUpdate"]["codeDiscountNode"]["codeDiscount"]
            ["title"],
        json!("HAR-193 lifecycle updated 1777318334676")
    );
    assert_eq!(
        updated.body["data"]["discountCodeBasicUpdate"]["codeDiscountNode"]["codeDiscount"]
            ["codes"]["nodes"][0]["code"],
        json!("HAR193LIVE1777318334676")
    );

    let read_query = r#"
        query DiscountCodeBasicLifecycleRead($id: ID!, $code: String!) {
          discountNode(id: $id) { id discount { __typename ... on DiscountCodeBasic { title status } } }
          codeDiscountNodeByCode(code: $code) { id }
          discountNodes(first: 5, query: "status:active") { nodes { id } }
          discountNodesCount(query: "status:active") { count precision }
        }
    "#;
    let read_active = proxy.process_request(json_graphql_request(
        read_query,
        json!({ "id": discount_id.clone(), "code": "HAR193LIVE1777318334676" }),
    ));
    assert_eq!(
        read_active.body["data"]["discountNode"]["discount"]["status"],
        json!("ACTIVE")
    );
    assert_eq!(
        read_active.body["data"]["discountNodesCount"],
        json!({ "count": 1, "precision": "EXACT" })
    );

    let deactivate_query = r#"
        mutation DiscountCodeBasicLifecycleDeactivate($id: ID!) {
          discountCodeDeactivate(id: $id) { codeDiscountNode { id codeDiscount { __typename ... on DiscountCodeBasic { title status } } } userErrors { field message code extraInfo } }
        }
    "#;
    let deactivated = proxy.process_request(json_graphql_request(
        deactivate_query,
        json!({ "id": discount_id.clone() }),
    ));
    assert_eq!(
        deactivated.body["data"]["discountCodeDeactivate"]["codeDiscountNode"]["codeDiscount"]
            ["status"],
        json!("EXPIRED")
    );
    let read_expired = proxy.process_request(json_graphql_request(
        read_query,
        json!({ "id": discount_id.clone(), "code": "HAR193LIVE1777318334676" }),
    ));
    assert_eq!(
        read_expired.body["data"]["discountNode"]["discount"]["status"],
        json!("EXPIRED")
    );
    assert_eq!(
        read_expired.body["data"]["discountNodes"]["nodes"],
        json!([])
    );
    assert_eq!(
        read_expired.body["data"]["discountNodesCount"],
        json!({ "count": 0, "precision": "EXACT" })
    );

    let activate_query = r#"
        mutation DiscountCodeBasicLifecycleActivate($id: ID!) {
          discountCodeActivate(id: $id) { codeDiscountNode { id codeDiscount { __typename ... on DiscountCodeBasic { title status } } } userErrors { field message code extraInfo } }
        }
    "#;
    let activated = proxy.process_request(json_graphql_request(
        activate_query,
        json!({ "id": discount_id.clone() }),
    ));
    assert_eq!(
        activated.body["data"]["discountCodeActivate"]["codeDiscountNode"]["codeDiscount"]
            ["status"],
        json!("ACTIVE")
    );

    let delete_query = r#"
        mutation DiscountCodeBasicLifecycleDelete($id: ID!) {
          discountCodeDelete(id: $id) { deletedCodeDiscountId userErrors { field message code extraInfo } }
        }
    "#;
    let deleted = proxy.process_request(json_graphql_request(
        delete_query,
        json!({ "id": discount_id.clone() }),
    ));
    assert_eq!(
        deleted.body["data"]["discountCodeDelete"]["userErrors"],
        json!([])
    );
    let read_deleted = proxy.process_request(json_graphql_request(
        read_query,
        json!({ "id": discount_id, "code": "HAR193LIVE1777318334676" }),
    ));
    assert_eq!(read_deleted.body["data"]["discountNode"], json!(null));
    assert_eq!(
        read_deleted.body["data"]["codeDiscountNodeByCode"],
        json!(null)
    );
    assert_eq!(
        read_deleted.body["data"]["discountNodesCount"],
        json!({ "count": 0, "precision": "EXACT" })
    );
}

#[test]
fn discount_hydrate_carries_async_usage_counts() {
    let discount_id = "gid://shopify/DiscountCodeNode/4242001".to_string();
    let redeem_code_id = "gid://shopify/DiscountRedeemCode/4242002".to_string();
    let hits = Arc::new(Mutex::new(0usize));
    let hit_counter = Arc::clone(&hits);
    let expected_discount_id = discount_id.clone();
    let expected_redeem_code_id = redeem_code_id.clone();
    let mut proxy =
        configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport(move |request| {
            assert!(
                request.body.contains("asyncUsageCount"),
                "discount hydrate query should select asyncUsageCount, got: {}",
                request.body
            );
            *hit_counter.lock().unwrap() += 1;
            let body: Value =
                serde_json::from_str(&request.body).expect("discount hydrate body should parse");
            assert_eq!(body["variables"]["id"], json!(expected_discount_id));
            Response {
                status: 200,
                headers: Default::default(),
                body: json!({
                    "data": {
                        "codeNode": {
                            "id": expected_discount_id.clone(),
                            "codeDiscount": {
                                "__typename": "DiscountCodeBasic",
                                "title": "Redeemed upstream discount",
                                "status": "ACTIVE",
                                "startsAt": "2026-04-27T19:31:14Z",
                                "endsAt": null,
                                "updatedAt": "2026-05-01T00:00:00Z",
                                "asyncUsageCount": 7,
                                "codes": {
                                    "nodes": [{
                                        "id": expected_redeem_code_id.clone(),
                                        "code": "REDEEMED-UPSTREAM",
                                        "asyncUsageCount": 3
                                    }]
                                }
                            }
                        },
                        "automaticNode": null
                    }
                }),
            }
        });

    let deactivated = proxy.process_request(json_graphql_request(
        r#"
        mutation HydrateRedeemedDiscount($id: ID!) {
          discountCodeDeactivate(id: $id) {
            codeDiscountNode {
              id
              codeDiscount {
                __typename
                ... on DiscountCodeBasic {
                  title
                  status
                  asyncUsageCount
                  codes(first: 1) { nodes { id code asyncUsageCount } }
                }
              }
            }
            userErrors { field message code extraInfo }
          }
        }
        "#,
        json!({ "id": discount_id }),
    ));

    assert_eq!(*hits.lock().unwrap(), 1);
    let hydrated =
        &deactivated.body["data"]["discountCodeDeactivate"]["codeDiscountNode"]["codeDiscount"];
    assert_eq!(hydrated["asyncUsageCount"], json!(7));
    assert_eq!(hydrated["codes"]["nodes"][0]["id"], json!(redeem_code_id));
    assert_eq!(hydrated["codes"]["nodes"][0]["asyncUsageCount"], json!(3));
    assert_eq!(
        deactivated.body["data"]["discountCodeDeactivate"]["userErrors"],
        json!([])
    );
}

#[test]
fn discount_activate_hydrates_full_upstream_code_basic_config() {
    let discount_id = "gid://shopify/DiscountCodeNode/4242101".to_string();
    let redeem_code_id = "gid://shopify/DiscountRedeemCode/4242102".to_string();
    let hits = Arc::new(Mutex::new(0usize));
    let hit_counter = Arc::clone(&hits);
    let expected_discount_id = discount_id.clone();
    let expected_redeem_code_id = redeem_code_id.clone();
    let mut proxy =
        configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport(move |request| {
            let body: Value =
                serde_json::from_str(&request.body).expect("upstream request body should parse");
            if body["query"]
                .as_str()
                .is_some_and(|query| query.contains("DraftProxyShopPricingHydrate"))
            {
                return Response {
                    status: 200,
                    headers: Default::default(),
                    body: json!({
                        "data": {
                            "shop": {
                                "currencyCode": "USD",
                                "taxesIncluded": false,
                                "taxShipping": false
                            }
                        }
                    }),
                };
            }
            *hit_counter.lock().unwrap() += 1;
            assert_full_discount_config_hydrate_request(&request.body);
            assert_eq!(body["variables"]["id"], json!(expected_discount_id));
            Response {
                status: 200,
                headers: Default::default(),
                body: json!({
                    "data": {
                        "codeNode": {
                            "id": expected_discount_id.clone(),
                            "metafields": upstream_discount_metafields(&expected_discount_id),
                            "codeDiscount": upstream_code_basic_fixed_amount_discount(
                                &expected_redeem_code_id,
                                "Upstream fixed amount",
                                "ACTIVE",
                            )
                        },
                        "automaticNode": null
                    }
                }),
            }
        });

    let activated = proxy.process_request(json_graphql_request(
        r#"
        mutation ActivateUpstreamFixedAmount($id: ID!) {
          discountCodeActivate(id: $id) {
            codeDiscountNode {
              id
              metafields(first: 2) {
                nodes { id namespace key type value }
                pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
              }
              codeDiscount {
                __typename
                ... on DiscountCodeBasic {
                  title
                  status
                  summary
                  usageLimit
                  recurringCycleLimit
                  appliesOncePerCustomer
                  discountClasses
                  combinesWith { productDiscounts orderDiscounts shippingDiscounts }
                  context { __typename ... on DiscountBuyerSelectionAll { all } }
                  customerGets {
                    value {
                      __typename
                      ... on DiscountPercentage { percentage }
                      ... on DiscountAmount { amount { amount currencyCode } appliesOnEachItem }
                    }
                    items { __typename ... on AllDiscountItems { allItems } }
                    appliesOnOneTimePurchase
                    appliesOnSubscription
                  }
                  minimumRequirement {
                    __typename
                    ... on DiscountMinimumSubtotal {
                      greaterThanOrEqualToSubtotal { amount currencyCode }
                    }
                  }
                }
              }
            }
            userErrors { field message code extraInfo }
          }
        }
        "#,
        json!({ "id": discount_id.clone() }),
    ));

    assert_eq!(*hits.lock().unwrap(), 1);
    assert_eq!(
        activated.body["data"]["discountCodeActivate"]["userErrors"],
        json!([])
    );
    let activated_discount =
        &activated.body["data"]["discountCodeActivate"]["codeDiscountNode"]["codeDiscount"];
    let activated_metafields =
        &activated.body["data"]["discountCodeActivate"]["codeDiscountNode"]["metafields"];
    assert_eq!(
        activated_discount["customerGets"]["value"]["__typename"],
        json!("DiscountAmount")
    );
    assert_eq!(
        activated_discount["customerGets"]["value"]["amount"],
        json!({ "amount": "5.0", "currencyCode": "USD" })
    );
    assert_eq!(
        activated_discount["minimumRequirement"]["greaterThanOrEqualToSubtotal"],
        json!({ "amount": "50.0", "currencyCode": "USD" })
    );
    assert_eq!(activated_discount["usageLimit"], json!(100));
    assert_eq!(activated_discount["appliesOncePerCustomer"], json!(true));
    assert_eq!(
        activated_discount["summary"],
        json!("$5.00 off entire order • Minimum purchase of $50.00")
    );
    assert_eq!(activated_metafields["nodes"][0]["value"], json!("summer"));

    let read = proxy.process_request(json_graphql_request(
        r#"
        query ReadActivatedUpstreamFixedAmount($id: ID!) {
          codeDiscountNode(id: $id) {
            id
            metafields(first: 2) {
              nodes { id namespace key type value }
              pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
            }
            codeDiscount {
              __typename
              ... on DiscountCodeBasic {
                title
                status
                summary
                usageLimit
                recurringCycleLimit
                appliesOncePerCustomer
                discountClasses
                combinesWith { productDiscounts orderDiscounts shippingDiscounts }
                context { __typename ... on DiscountBuyerSelectionAll { all } }
                customerGets {
                  value {
                    __typename
                    ... on DiscountPercentage { percentage }
                    ... on DiscountAmount { amount { amount currencyCode } appliesOnEachItem }
                  }
                  items { __typename ... on AllDiscountItems { allItems } }
                  appliesOnOneTimePurchase
                  appliesOnSubscription
                }
                minimumRequirement {
                  __typename
                  ... on DiscountMinimumSubtotal {
                    greaterThanOrEqualToSubtotal { amount currencyCode }
                  }
                }
              }
            }
          }
        }
        "#,
        json!({ "id": discount_id }),
    ));

    let read_discount = &read.body["data"]["codeDiscountNode"]["codeDiscount"];
    let read_metafields = &read.body["data"]["codeDiscountNode"]["metafields"];
    assert_eq!(
        read_discount["customerGets"]["value"],
        activated_discount["customerGets"]["value"]
    );
    assert_eq!(
        read_discount["minimumRequirement"],
        activated_discount["minimumRequirement"]
    );
    assert_eq!(read_discount["usageLimit"], json!(100));
    assert_eq!(read_discount["appliesOncePerCustomer"], json!(true));
    assert_eq!(read_metafields["nodes"][0]["value"], json!("summer"));
}

#[test]
fn discount_partial_update_hydrates_full_config_without_defaulting_customer_gets() {
    let discount_id = "gid://shopify/DiscountCodeNode/4242201".to_string();
    let redeem_code_id = "gid://shopify/DiscountRedeemCode/4242202".to_string();
    let hits = Arc::new(Mutex::new(0usize));
    let hit_counter = Arc::clone(&hits);
    let expected_discount_id = discount_id.clone();
    let expected_redeem_code_id = redeem_code_id.clone();
    let mut proxy =
        configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport(move |request| {
            *hit_counter.lock().unwrap() += 1;
            assert_full_discount_config_hydrate_request(&request.body);
            let body: Value =
                serde_json::from_str(&request.body).expect("discount hydrate body should parse");
            assert_eq!(body["variables"]["id"], json!(expected_discount_id));
            Response {
                status: 200,
                headers: Default::default(),
                body: json!({
                    "data": {
                        "codeNode": {
                            "id": expected_discount_id.clone(),
                            "metafields": upstream_discount_metafields(&expected_discount_id),
                            "codeDiscount": upstream_code_basic_fixed_amount_discount(
                                &expected_redeem_code_id,
                                "Upstream fixed amount",
                                "ACTIVE",
                            )
                        },
                        "automaticNode": null
                    }
                }),
            }
        });

    let updated = proxy.process_request(json_graphql_request(
        r#"
        mutation UpdateUpstreamFixedAmount($id: ID!, $input: DiscountCodeBasicInput!) {
          discountCodeBasicUpdate(id: $id, basicCodeDiscount: $input) {
            codeDiscountNode {
              id
              codeDiscount {
                __typename
                ... on DiscountCodeBasic {
                  title
                  customerGets {
                    value {
                      __typename
                      ... on DiscountPercentage { percentage }
                      ... on DiscountAmount { amount { amount currencyCode } appliesOnEachItem }
                    }
                    items { __typename ... on AllDiscountItems { allItems } }
                    appliesOnOneTimePurchase
                    appliesOnSubscription
                  }
                  minimumRequirement {
                    __typename
                    ... on DiscountMinimumSubtotal {
                      greaterThanOrEqualToSubtotal { amount currencyCode }
                    }
                  }
                  usageLimit
                  appliesOncePerCustomer
                  summary
                }
              }
            }
            userErrors { field message code extraInfo }
          }
        }
        "#,
        json!({
            "id": discount_id.clone(),
            "input": {
                "title": "Updated upstream title"
            }
        }),
    ));

    assert_eq!(*hits.lock().unwrap(), 1);
    assert_eq!(
        updated.body["data"]["discountCodeBasicUpdate"]["userErrors"],
        json!([])
    );
    let updated_discount =
        &updated.body["data"]["discountCodeBasicUpdate"]["codeDiscountNode"]["codeDiscount"];
    assert_eq!(updated_discount["title"], json!("Updated upstream title"));
    assert_eq!(
        updated_discount["customerGets"]["value"],
        json!({
            "__typename": "DiscountAmount",
            "amount": { "amount": "5.0", "currencyCode": "USD" },
            "appliesOnEachItem": false
        })
    );
    assert_eq!(
        updated_discount["minimumRequirement"]["greaterThanOrEqualToSubtotal"],
        json!({ "amount": "50.0", "currencyCode": "USD" })
    );
    assert_eq!(updated_discount["usageLimit"], json!(100));
    assert_eq!(updated_discount["appliesOncePerCustomer"], json!(true));
    assert_eq!(
        updated_discount["summary"],
        json!("$5.00 off entire order • Minimum purchase of $50.00")
    );
}

#[test]
fn discount_update_preserves_redeemed_usage_and_scope_when_omitted() {
    let mut proxy = snapshot_proxy();
    let created = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateScopedRedeemedDiscount($input: DiscountCodeFreeShippingInput!) {
          discountCodeFreeShippingCreate(freeShippingCodeDiscount: $input) {
            codeDiscountNode {
              id
              codeDiscount {
                __typename
                ... on DiscountCodeFreeShipping {
                  title
                  asyncUsageCount
                  codes(first: 1) { nodes { id code asyncUsageCount } }
                  appliesOnOneTimePurchase
                  appliesOnSubscription
                }
              }
            }
            userErrors { field message code extraInfo }
          }
        }
        "#,
        json!({ "input": {
            "title": "Redeemed free shipping",
            "code": "REDEEMED-SHIP",
            "startsAt": "2026-04-25T00:00:00Z",
            "destination": { "all": true },
            "appliesOnOneTimePurchase": false,
            "appliesOnSubscription": true
        }}),
    ));
    assert_eq!(
        created.body["data"]["discountCodeFreeShippingCreate"]["userErrors"],
        json!([])
    );
    let discount_id = json_string(
        &created.body["data"]["discountCodeFreeShippingCreate"]["codeDiscountNode"]["id"],
        "redeemed discount id",
    );
    restore_proxy_state(&mut proxy, |state| {
        let discount = state["state"]["stagedState"]["discounts"]
            .as_object_mut()
            .and_then(|discounts| discounts.get_mut(&discount_id))
            .expect("created discount should be present in staged state");
        discount["asyncUsageCount"] = json!(6);
        discount["codes"][0]["asyncUsageCount"] = json!(4);
    });

    let updated = proxy.process_request(json_graphql_request(
        r#"
        mutation UpdateRedeemedDiscount($id: ID!, $input: DiscountCodeFreeShippingInput!) {
          discountCodeFreeShippingUpdate(id: $id, freeShippingCodeDiscount: $input) {
            codeDiscountNode {
              id
              codeDiscount {
                __typename
                ... on DiscountCodeFreeShipping {
                  title
                  asyncUsageCount
                  codes(first: 1) { nodes { code asyncUsageCount } }
                  appliesOnOneTimePurchase
                  appliesOnSubscription
                }
              }
            }
            userErrors { field message code extraInfo }
          }
        }
        "#,
        json!({
            "id": discount_id,
            "input": {
                "title": "Redeemed free shipping renamed",
                "startsAt": "2026-04-25T00:00:00Z",
                "destination": { "all": true }
            }
        }),
    ));

    let discount =
        &updated.body["data"]["discountCodeFreeShippingUpdate"]["codeDiscountNode"]["codeDiscount"];
    assert_eq!(discount["title"], json!("Redeemed free shipping renamed"));
    assert_eq!(discount["asyncUsageCount"], json!(6));
    assert_eq!(
        discount["codes"]["nodes"][0]["code"],
        json!("REDEEMED-SHIP")
    );
    assert_eq!(discount["codes"]["nodes"][0]["asyncUsageCount"], json!(4));
    assert_eq!(discount["appliesOnOneTimePurchase"], json!(false));
    assert_eq!(discount["appliesOnSubscription"], json!(true));
    assert_eq!(
        updated.body["data"]["discountCodeFreeShippingUpdate"]["userErrors"],
        json!([])
    );
}

#[test]
fn discount_update_hydrates_free_shipping_scope_before_omitted_update() {
    let discount_id = "gid://shopify/DiscountCodeNode/4242101".to_string();
    let redeem_code_id = "gid://shopify/DiscountRedeemCode/4242102".to_string();
    let hits = Arc::new(Mutex::new(0usize));
    let hit_counter = Arc::clone(&hits);
    let expected_discount_id = discount_id.clone();
    let expected_redeem_code_id = redeem_code_id.clone();
    let mut proxy =
        configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport(move |request| {
            assert!(
                request.body.contains("appliesOnOneTimePurchase")
                    && request.body.contains("appliesOnSubscription")
                    && request.body.contains("asyncUsageCount"),
                "discount hydrate query should select usage and scope fields, got: {}",
                request.body
            );
            *hit_counter.lock().unwrap() += 1;
            let body: Value =
                serde_json::from_str(&request.body).expect("discount hydrate body should parse");
            assert_eq!(body["variables"]["id"], json!(expected_discount_id));
            Response {
                status: 200,
                headers: Default::default(),
                body: json!({
                    "data": {
                        "codeNode": {
                            "id": expected_discount_id.clone(),
                            "codeDiscount": {
                                "__typename": "DiscountCodeFreeShipping",
                                "title": "Redeemed upstream free shipping",
                                "status": "ACTIVE",
                                "startsAt": "2026-04-27T19:31:14Z",
                                "endsAt": null,
                                "updatedAt": "2026-05-01T00:00:00Z",
                                "asyncUsageCount": 9,
                                "codes": {
                                    "nodes": [{
                                        "id": expected_redeem_code_id.clone(),
                                        "code": "REDEEMED-SHIP-UPSTREAM",
                                        "asyncUsageCount": 4
                                    }]
                                },
                                "appliesOnOneTimePurchase": false,
                                "appliesOnSubscription": true
                            }
                        },
                        "automaticNode": null
                    }
                }),
            }
        });

    let updated = proxy.process_request(json_graphql_request(
        r#"
        mutation UpdateHydratedRedeemedShipping($id: ID!, $input: DiscountCodeFreeShippingInput!) {
          discountCodeFreeShippingUpdate(id: $id, freeShippingCodeDiscount: $input) {
            codeDiscountNode {
              id
              codeDiscount {
                __typename
                ... on DiscountCodeFreeShipping {
                  title
                  asyncUsageCount
                  codes(first: 1) { nodes { id code asyncUsageCount } }
                  appliesOnOneTimePurchase
                  appliesOnSubscription
                }
              }
            }
            userErrors { field message code extraInfo }
          }
        }
        "#,
        json!({
            "id": discount_id,
            "input": {
                "title": "Redeemed upstream free shipping renamed",
                "startsAt": "2026-04-27T19:31:14Z",
                "destination": { "all": true }
            }
        }),
    ));

    assert_eq!(*hits.lock().unwrap(), 1);
    let discount =
        &updated.body["data"]["discountCodeFreeShippingUpdate"]["codeDiscountNode"]["codeDiscount"];
    assert_eq!(
        discount["title"],
        json!("Redeemed upstream free shipping renamed")
    );
    assert_eq!(discount["asyncUsageCount"], json!(9));
    assert_eq!(discount["codes"]["nodes"][0]["id"], json!(redeem_code_id));
    assert_eq!(
        discount["codes"]["nodes"][0]["code"],
        json!("REDEEMED-SHIP-UPSTREAM")
    );
    assert_eq!(discount["codes"]["nodes"][0]["asyncUsageCount"], json!(4));
    assert_eq!(discount["appliesOnOneTimePurchase"], json!(false));
    assert_eq!(discount["appliesOnSubscription"], json!(true));
    assert_eq!(
        updated.body["data"]["discountCodeFreeShippingUpdate"]["userErrors"],
        json!([])
    );
}

#[test]
fn discount_context_refs_hydrate_in_one_shared_batch() {
    let captured_bodies = Arc::new(Mutex::new(Vec::<Value>::new()));
    let captured_requests = Arc::clone(&captured_bodies);
    let mut proxy =
        configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport(move |request| {
            let body: Value = serde_json::from_str(&request.body)
                .expect("discount context hydrate request body should parse");
            captured_requests.lock().unwrap().push(body.clone());
            let query = body["query"].as_str().unwrap_or_default();
            if query.contains("nodes(ids: $ids)") {
                let ids = body["variables"]["ids"]
                    .as_array()
                    .expect("batched context hydrate should carry ids")
                    .iter()
                    .map(|value| value.as_str().expect("context hydrate id").to_string())
                    .collect::<Vec<_>>();
                return Response {
                    status: 200,
                    headers: Default::default(),
                    body: json!({
                        "data": {
                            "nodes": ids
                                .iter()
                                .map(|id| discount_context_hydrate_node(id))
                                .collect::<Vec<_>>()
                        }
                    }),
                };
            }
            if query.contains("customer(id: $id)") {
                let id = body["variables"]["id"]
                    .as_str()
                    .expect("single customer hydrate should carry id");
                return Response {
                    status: 200,
                    headers: Default::default(),
                    body: json!({ "data": { "customer": discount_context_hydrate_node(id) } }),
                };
            }
            if query.contains("segment(id: $id)") {
                let id = body["variables"]["id"]
                    .as_str()
                    .expect("single segment hydrate should carry id");
                return Response {
                    status: 200,
                    headers: Default::default(),
                    body: json!({ "data": { "segment": discount_context_hydrate_node(id) } }),
                };
            }
            panic!("unexpected discount context upstream request: {body}");
        });

    let response = proxy.process_request(json_graphql_request(
        r#"
        mutation BatchDiscountContextRefs(
          $customers: DiscountAutomaticBasicInput!
          $segments: DiscountAutomaticBasicInput!
        ) {
          customerScoped: discountAutomaticBasicCreate(automaticBasicDiscount: $customers) {
            automaticDiscountNode {
              automaticDiscount {
                __typename
                ... on DiscountAutomaticBasic {
                  context {
                    __typename
                    ... on DiscountCustomers {
                      customers { id displayName }
                    }
                  }
                }
              }
            }
            userErrors { field message code extraInfo }
          }
          segmentScoped: discountAutomaticBasicCreate(automaticBasicDiscount: $segments) {
            automaticDiscountNode {
              automaticDiscount {
                __typename
                ... on DiscountAutomaticBasic {
                  context {
                    __typename
                    ... on DiscountCustomerSegments {
                      segments { id name }
                    }
                  }
                }
              }
            }
            userErrors { field message code extraInfo }
          }
        }
        "#,
        json!({
            "customers": {
                "title": "Customer scoped automatic",
                "startsAt": "2026-04-25T00:00:00Z",
                "context": {
                    "customers": {
                        "add": [
                            "gid://shopify/Customer/2",
                            "gid://shopify/Customer/1",
                            "gid://shopify/Customer/2"
                        ]
                    }
                },
                "customerGets": {
                    "value": { "percentage": 0.1 },
                    "items": { "all": true }
                }
            },
            "segments": {
                "title": "Segment scoped automatic",
                "startsAt": "2026-04-25T00:00:00Z",
                "context": {
                    "customerSegments": {
                        "add": [
                            "gid://shopify/Segment/20",
                            "gid://shopify/Segment/10",
                            "gid://shopify/Segment/10"
                        ]
                    }
                },
                "customerGets": {
                    "value": { "percentage": 0.2 },
                    "items": { "all": true }
                }
            }
        }),
    ));

    assert_eq!(response.status, 200);
    assert_eq!(
        response.body["data"]["customerScoped"]["userErrors"],
        json!([])
    );
    assert_eq!(
        response.body["data"]["segmentScoped"]["userErrors"],
        json!([])
    );
    assert_eq!(
        response.body["data"]["customerScoped"]["automaticDiscountNode"]["automaticDiscount"]
            ["context"]["customers"],
        json!([
            { "id": "gid://shopify/Customer/2", "displayName": "Customer 2" },
            { "id": "gid://shopify/Customer/1", "displayName": "Customer 1" },
            { "id": "gid://shopify/Customer/2", "displayName": "Customer 2" }
        ])
    );
    assert_eq!(
        response.body["data"]["segmentScoped"]["automaticDiscountNode"]["automaticDiscount"]
            ["context"]["segments"],
        json!([
            { "id": "gid://shopify/Segment/20", "name": "Segment 20" },
            { "id": "gid://shopify/Segment/10", "name": "Segment 10" },
            { "id": "gid://shopify/Segment/10", "name": "Segment 10" }
        ])
    );

    let requests = captured_bodies.lock().unwrap();
    assert_eq!(
        requests.len(),
        1,
        "the mutation should issue one shared nodes preflight, got: {:?}",
        requests
            .iter()
            .map(|body| body["operationName"].clone())
            .collect::<Vec<_>>()
    );
    for request in requests.iter() {
        let query = request["query"]
            .as_str()
            .expect("batched context hydrate should carry query");
        assert!(
            query.contains("nodes(ids: $ids)"),
            "context refs should use a batched nodes hydrate, got: {query}"
        );
        assert!(
            !query.contains("addressesV2"),
            "discount buyer-context hydrate should not fetch customer address windows, got: {query}"
        );
    }
    assert_eq!(
        requests[0]["variables"]["ids"],
        json!([
            "gid://shopify/Customer/1",
            "gid://shopify/Customer/2",
            "gid://shopify/Segment/10",
            "gid://shopify/Segment/20"
        ])
    );
}

#[test]
fn discount_update_and_status_hydrates_are_code_only_and_bounded() {
    let discount_id = "gid://shopify/DiscountCodeNode/4242301".to_string();
    let redeem_code_id = "gid://shopify/DiscountRedeemCode/4242302".to_string();
    let update_bodies = Arc::new(Mutex::new(Vec::<Value>::new()));
    let captured_update_bodies = Arc::clone(&update_bodies);
    let update_discount_id = discount_id.clone();
    let update_redeem_code_id = redeem_code_id.clone();
    let mut update_proxy =
        configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport(move |request| {
            let body: Value = serde_json::from_str(&request.body)
                .expect("discount update hydrate request body should parse");
            captured_update_bodies.lock().unwrap().push(body.clone());
            Response {
                status: 200,
                headers: Default::default(),
                body: json!({
                    "data": {
                        "codeNode": {
                            "id": update_discount_id.clone(),
                            "codeDiscount": upstream_code_basic_fixed_amount_discount(
                                &update_redeem_code_id,
                                "Hydrated update discount",
                                "ACTIVE",
                            )
                        }
                    }
                }),
            }
        });

    let update = update_proxy.process_request(json_graphql_request(
        r#"
        mutation BoundedUpdateHydrate($id: ID!, $input: DiscountCodeBasicInput!) {
          discountCodeBasicUpdate(id: $id, basicCodeDiscount: $input) {
            codeDiscountNode {
              id
              codeDiscount {
                __typename
                ... on DiscountCodeBasic { title status }
              }
            }
            userErrors { field message code extraInfo }
          }
        }
        "#,
        json!({
            "id": discount_id.clone(),
            "input": {
                "title": "Updated from bounded hydrate"
            }
        }),
    ));
    assert_eq!(update.status, 200);
    assert_eq!(
        update.body["data"]["discountCodeBasicUpdate"]["userErrors"],
        json!([])
    );
    let update_requests = update_bodies.lock().unwrap();
    assert_eq!(update_requests.len(), 1);
    assert_code_only_bounded_discount_hydrate_request(&update_requests[0]);

    let status_bodies = Arc::new(Mutex::new(Vec::<Value>::new()));
    let captured_status_bodies = Arc::clone(&status_bodies);
    let status_discount_id = discount_id.clone();
    let status_redeem_code_id = redeem_code_id.clone();
    let mut status_proxy =
        configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport(move |request| {
            let body: Value = serde_json::from_str(&request.body)
                .expect("discount status hydrate request body should parse");
            captured_status_bodies.lock().unwrap().push(body.clone());
            Response {
                status: 200,
                headers: Default::default(),
                body: json!({
                    "data": {
                        "codeNode": {
                            "id": status_discount_id.clone(),
                            "codeDiscount": upstream_code_basic_fixed_amount_discount(
                                &status_redeem_code_id,
                                "Hydrated status discount",
                                "ACTIVE",
                            )
                        }
                    }
                }),
            }
        });

    let deactivated = status_proxy.process_request(json_graphql_request(
        r#"
        mutation BoundedStatusHydrate($id: ID!) {
          discountCodeDeactivate(id: $id) {
            codeDiscountNode {
              id
              codeDiscount {
                __typename
                ... on DiscountCodeBasic { title status }
              }
            }
            userErrors { field message code extraInfo }
          }
        }
        "#,
        json!({ "id": discount_id }),
    ));
    assert_eq!(deactivated.status, 200);
    assert_eq!(
        deactivated.body["data"]["discountCodeDeactivate"]["userErrors"],
        json!([])
    );
    let status_requests = status_bodies.lock().unwrap();
    assert_eq!(status_requests.len(), 1);
    assert_code_only_bounded_discount_hydrate_request(&status_requests[0]);
}

#[test]
fn discount_basic_summary_derives_value_scope_and_minimum_requirement() {
    let mut proxy = snapshot_proxy();
    let product = proxy.process_request(json_graphql_request(
        r#"
        mutation ProductForDiscountSummary($product: ProductCreateInput!) {
          productCreate(product: $product) {
            product { id title }
            userErrors { field message  }
          }
        }
        "#,
        json!({ "product": { "title": "The Complete Snowboard (Ice)" } }),
    ));
    assert_eq!(
        product.body["data"]["productCreate"]["userErrors"],
        json!([])
    );
    let product_id = json_string(
        &product.body["data"]["productCreate"]["product"]["id"],
        "discount summary product id",
    );

    let created = proxy.process_request(json_graphql_request(
        r#"
        mutation DiscountBasicSummary(
          $codeInput: DiscountCodeBasicInput!
          $subscriptionInput: DiscountAutomaticBasicInput!
          $productInput: DiscountAutomaticBasicInput!
        ) {
          code: discountCodeBasicCreate(basicCodeDiscount: $codeInput) {
            codeDiscountNode {
              codeDiscount {
                __typename
                ... on DiscountCodeBasic {
                  summary
                  customerGets {
                    appliesOnOneTimePurchase
                    appliesOnSubscription
                  }
                }
              }
            }
            userErrors { field message code extraInfo }
          }
          subscription: discountAutomaticBasicCreate(automaticBasicDiscount: $subscriptionInput) {
            automaticDiscountNode {
              automaticDiscount {
                __typename
                ... on DiscountAutomaticBasic {
                  summary
                  customerGets {
                    appliesOnOneTimePurchase
                    appliesOnSubscription
                  }
                }
              }
            }
            userErrors { field message code extraInfo }
          }
          product: discountAutomaticBasicCreate(automaticBasicDiscount: $productInput) {
            automaticDiscountNode {
              automaticDiscount {
                __typename
                ... on DiscountAutomaticBasic {
                  summary
                }
              }
            }
            userErrors { field message code extraInfo }
          }
        }
        "#,
        json!({
            "codeInput": {
                "title": "Summary code basic",
                "code": "SUMMARYBASIC",
                "startsAt": "2026-04-25T00:00:00Z",
                "context": { "all": "ALL" },
                "minimumRequirement": { "subtotal": { "greaterThanOrEqualToSubtotal": "1.00" } },
                "customerGets": { "value": { "percentage": 0.1 }, "items": { "all": true } }
            },
            "subscriptionInput": {
                "title": "Summary subscription basic",
                "startsAt": "2026-04-25T00:00:00Z",
                "minimumRequirement": { "subtotal": { "greaterThanOrEqualToSubtotal": "2.00" } },
                "customerGets": {
                    "value": { "discountAmount": { "amount": "5.00", "appliesOnEachItem": false } },
                    "items": { "all": true },
                    "appliesOnOneTimePurchase": false,
                    "appliesOnSubscription": true
                }
            },
            "productInput": {
                "title": "Summary product basic",
                "startsAt": "2026-04-25T00:00:00Z",
                "minimumRequirement": { "quantity": { "greaterThanOrEqualToQuantity": "3" } },
                "customerGets": {
                    "value": { "percentage": 0.3 },
                    "items": { "products": { "productsToAdd": [product_id] } }
                }
            }
        }),
    ));

    assert_eq!(created.body["data"]["code"]["userErrors"], json!([]));
    assert_eq!(
        created.body["data"]["code"]["codeDiscountNode"]["codeDiscount"]["summary"],
        json!("10% off one-time purchase products • Minimum purchase of $1.00")
    );
    assert_eq!(
        created.body["data"]["subscription"]["userErrors"],
        json!([])
    );
    assert_eq!(
        created.body["data"]["subscription"]["automaticDiscountNode"]["automaticDiscount"]
            ["summary"],
        json!("$5.00 off subscription products • Minimum purchase of $2.00")
    );
    assert_eq!(
        created.body["data"]["subscription"]["automaticDiscountNode"]["automaticDiscount"]
            ["customerGets"]["appliesOnOneTimePurchase"],
        json!(false)
    );
    assert_eq!(
        created.body["data"]["subscription"]["automaticDiscountNode"]["automaticDiscount"]
            ["customerGets"]["appliesOnSubscription"],
        json!(true)
    );
    assert_eq!(created.body["data"]["product"]["userErrors"], json!([]));
    assert_eq!(
        created.body["data"]["product"]["automaticDiscountNode"]["automaticDiscount"]["summary"],
        json!("30% off The Complete Snowboard (Ice) • Minimum quantity of 3")
    );
}

#[test]
fn discount_basic_summary_supports_one_time_scope_when_shop_sells_subscriptions() {
    let hits = Arc::new(Mutex::new(0usize));
    let hit_counter = Arc::clone(&hits);
    let mut proxy =
        configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport(move |request| {
            assert!(
                request
                    .body
                    .contains("DraftProxyShopSubscriptionCapability"),
                "only the subscription capability probe should be forwarded, got: {}",
                request.body
            );
            *hit_counter.lock().unwrap() += 1;
            Response {
                status: 200,
                headers: Default::default(),
                body: json!({
                    "data": {
                        "shop": {
                            "features": {
                                "sellsSubscriptions": true
                            }
                        }
                    }
                }),
            }
        });

    let created = proxy.process_request(json_graphql_request(
        r#"
        mutation DiscountOneTimeSummary($input: DiscountAutomaticBasicInput!) {
          discountAutomaticBasicCreate(automaticBasicDiscount: $input) {
            automaticDiscountNode {
              automaticDiscount {
                __typename
                ... on DiscountAutomaticBasic {
                  summary
                }
              }
            }
            userErrors { field message code extraInfo }
          }
        }
        "#,
        json!({ "input": {
            "title": "One time summary",
            "startsAt": "2026-04-25T00:00:00Z",
            "customerGets": {
                "value": { "percentage": 0.1 },
                "items": { "all": true },
                "appliesOnOneTimePurchase": true,
                "appliesOnSubscription": false
            }
        }}),
    ));

    assert_eq!(
        *hits.lock().unwrap(),
        1,
        "one subscription capability probe should be forwarded"
    );
    assert_eq!(
        created.body["data"]["discountAutomaticBasicCreate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        created.body["data"]["discountAutomaticBasicCreate"]["automaticDiscountNode"]
            ["automaticDiscount"]["summary"],
        json!("10% off one-time purchase products")
    );
}

#[test]
fn discount_fixed_amount_applies_on_each_item_readback_matches_input() {
    let mut proxy = snapshot_proxy();
    restore_shop_currency(&mut proxy, "USD");

    let code_create_query = r#"
        mutation DiscountAmountEachCodeCreate($input: DiscountCodeBasicInput!) {
          discountCodeBasicCreate(basicCodeDiscount: $input) {
            codeDiscountNode {
              id
              codeDiscount {
                __typename
                ... on DiscountCodeBasic {
                  customerGets {
                    value {
                      __typename
                      ... on DiscountAmount {
                        amount { amount currencyCode }
                        appliesOnEachItem
                      }
                    }
                  }
                }
              }
            }
            userErrors { field message code extraInfo }
          }
        }
    "#;
    let code_created = proxy.process_request(json_graphql_request(
        code_create_query,
        json!({ "input": {
            "title": "Fixed amount each code",
            "code": "FIXEDEACHCODE",
            "startsAt": "2026-04-25T00:00:00Z",
            "customerGets": {
                "value": { "discountAmount": { "amount": "10.00", "appliesOnEachItem": true } },
                "items": { "all": true }
            }
        }}),
    ));
    assert_eq!(
        code_created.body["data"]["discountCodeBasicCreate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        code_created.body["data"]["discountCodeBasicCreate"]["codeDiscountNode"]["codeDiscount"]
            ["customerGets"]["value"],
        json!({
            "__typename": "DiscountAmount",
            "amount": { "amount": "10.0", "currencyCode": "USD" },
            "appliesOnEachItem": true
        })
    );
    let code_id = json_string(
        &code_created.body["data"]["discountCodeBasicCreate"]["codeDiscountNode"]["id"],
        "code discount id",
    );

    let code_read = proxy.process_request(json_graphql_request(
        r#"
        query DiscountAmountEachCodeRead($id: ID!) {
          discountNode(id: $id) {
            discount {
              __typename
              ... on DiscountCodeBasic {
                customerGets {
                  value {
                    __typename
                    ... on DiscountAmount {
                      amount { amount currencyCode }
                      appliesOnEachItem
                    }
                  }
                }
              }
            }
          }
        }
        "#,
        json!({ "id": code_id.clone() }),
    ));
    assert_eq!(
        code_read.body["data"]["discountNode"]["discount"]["customerGets"]["value"]
            ["appliesOnEachItem"],
        json!(true)
    );

    let code_update = proxy.process_request(json_graphql_request(
        r#"
        mutation DiscountAmountEachCodeUpdate($id: ID!, $input: DiscountCodeBasicInput!) {
          discountCodeBasicUpdate(id: $id, basicCodeDiscount: $input) {
            codeDiscountNode {
              codeDiscount {
                __typename
                ... on DiscountCodeBasic {
                  customerGets {
                    value {
                      __typename
                      ... on DiscountAmount {
                        amount { amount currencyCode }
                        appliesOnEachItem
                      }
                    }
                  }
                }
              }
            }
            userErrors { field message code extraInfo }
          }
        }
        "#,
        json!({
            "id": code_id,
            "input": {
                "title": "Fixed amount across code",
                "code": "FIXEDACROSSCODE",
                "startsAt": "2026-04-25T00:00:00Z",
                "customerGets": {
                    "value": { "discountAmount": { "amount": "7.00", "appliesOnEachItem": false } },
                    "items": { "all": true }
                }
            }
        }),
    ));
    assert_eq!(
        code_update.body["data"]["discountCodeBasicUpdate"]["codeDiscountNode"]["codeDiscount"]
            ["customerGets"]["value"],
        json!({
            "__typename": "DiscountAmount",
            "amount": { "amount": "7.0", "currencyCode": "USD" },
            "appliesOnEachItem": false
        })
    );

    let automatic_create = proxy.process_request(json_graphql_request(
        r#"
        mutation DiscountAmountEachAutomaticCreate($input: DiscountAutomaticBasicInput!) {
          discountAutomaticBasicCreate(automaticBasicDiscount: $input) {
            automaticDiscountNode {
              id
              automaticDiscount {
                __typename
                ... on DiscountAutomaticBasic {
                  customerGets {
                    value {
                      __typename
                      ... on DiscountAmount {
                        amount { amount currencyCode }
                        appliesOnEachItem
                      }
                    }
                  }
                }
              }
            }
            userErrors { field message code extraInfo }
          }
        }
        "#,
        json!({ "input": {
            "title": "Fixed amount each automatic",
            "startsAt": "2026-04-25T00:00:00Z",
            "customerGets": {
                "value": { "discountAmount": { "amount": "5.00", "appliesOnEachItem": true } },
                "items": { "all": true }
            }
        }}),
    ));
    assert_eq!(
        automatic_create.body["data"]["discountAutomaticBasicCreate"]["automaticDiscountNode"]
            ["automaticDiscount"]["customerGets"]["value"]["appliesOnEachItem"],
        json!(true)
    );
    let automatic_id = json_string(
        &automatic_create.body["data"]["discountAutomaticBasicCreate"]["automaticDiscountNode"]
            ["id"],
        "automatic discount id",
    );

    let automatic_update = proxy.process_request(json_graphql_request(
        r#"
        mutation DiscountAmountEachAutomaticUpdate($id: ID!, $input: DiscountAutomaticBasicInput!) {
          discountAutomaticBasicUpdate(id: $id, automaticBasicDiscount: $input) {
            automaticDiscountNode {
              automaticDiscount {
                __typename
                ... on DiscountAutomaticBasic {
                  customerGets {
                    value {
                      __typename
                      ... on DiscountAmount {
                        amount { amount currencyCode }
                        appliesOnEachItem
                      }
                    }
                  }
                }
              }
            }
            userErrors { field message code extraInfo }
          }
        }
        "#,
        json!({
            "id": automatic_id,
            "input": {
                "title": "Fixed amount across automatic",
                "startsAt": "2026-04-25T00:00:00Z",
                "customerGets": {
                    "value": { "discountAmount": { "amount": "4.00", "appliesOnEachItem": false } },
                    "items": { "all": true }
                }
            }
        }),
    ));
    assert_eq!(
        automatic_update.body["data"]["discountAutomaticBasicUpdate"]["automaticDiscountNode"]
            ["automaticDiscount"]["customerGets"]["value"],
        json!({
            "__typename": "DiscountAmount",
            "amount": { "amount": "4.0", "currencyCode": "USD" },
            "appliesOnEachItem": false
        })
    );

    let bxgy_create = proxy.process_request(json_graphql_request(
        r#"
        mutation DiscountAmountBxgyCreate($input: DiscountCodeBxgyInput!) {
          discountCodeBxgyCreate(bxgyCodeDiscount: $input) {
            codeDiscountNode {
              codeDiscount {
                __typename
                ... on DiscountCodeBxgy {
                  customerGets {
                    value {
                      __typename
                      ... on DiscountOnQuantity {
                        quantity { quantity }
                        effect {
                          __typename
                          ... on DiscountAmount {
                            amount { amount currencyCode }
                            appliesOnEachItem
                          }
                        }
                      }
                    }
                  }
                }
              }
            }
            userErrors { field message code extraInfo }
          }
        }
        "#,
        json!({ "input": {
            "title": "Fixed amount bxgy",
            "code": "FIXEDBXGY",
            "startsAt": "2026-04-25T00:00:00Z",
            "context": { "all": "ALL" },
            "customerBuys": {
                "value": { "quantity": "1" },
                "items": { "products": { "productsToAdd": ["gid://shopify/Product/1001"] } }
            },
            "customerGets": {
                "value": {
                    "discountOnQuantity": {
                        "quantity": "1",
                        "effect": { "amount": "3.00" }
                    }
                },
                "items": { "products": { "productsToAdd": ["gid://shopify/Product/1002"] } }
            }
        }}),
    ));
    assert_eq!(
        bxgy_create.body["data"]["discountCodeBxgyCreate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        bxgy_create.body["data"]["discountCodeBxgyCreate"]["codeDiscountNode"]["codeDiscount"]
            ["customerGets"]["value"]["effect"],
        json!({
            "__typename": "DiscountAmount",
            "amount": { "amount": "3.0", "currencyCode": "USD" },
            "appliesOnEachItem": false
        })
    );
}

#[test]
fn discount_amount_deprecated_each_fields_are_public_schema_errors() {
    let mut proxy = snapshot_proxy();
    let query = r#"
        mutation DiscountAmountDeprecatedEach($input: DiscountCodeBasicInput!) {
          discountCodeBasicCreate(basicCodeDiscount: $input) {
            codeDiscountNode { id }
            userErrors { field message code extraInfo }
          }
        }
    "#;

    for deprecated_field in ["each", "useEach"] {
        let mut discount_amount = serde_json::Map::new();
        discount_amount.insert("amount".to_string(), json!("10.00"));
        discount_amount.insert("appliesOnEachItem".to_string(), json!(true));
        discount_amount.insert(deprecated_field.to_string(), json!(true));
        let response = proxy.process_request(json_graphql_request(
            query,
            json!({ "input": {
                "title": format!("Deprecated field {deprecated_field}"),
                "code": format!("DEPRECATED{}", deprecated_field.to_ascii_uppercase()),
                "startsAt": "2026-04-25T00:00:00Z",
                "customerGets": {
                    "value": { "discountAmount": Value::Object(discount_amount) },
                    "items": { "all": true }
                }
            }}),
        ));

        assert!(response.body.get("data").is_none());
        assert_eq!(
            response.body["errors"][0]["extensions"]["code"],
            json!("INVALID_VARIABLE")
        );
        assert!(
            response.body["errors"][0]["message"]
                .as_str()
                .unwrap_or_default()
                .contains(&format!(
                    "customerGets.value.discountAmount.{deprecated_field}"
                )),
            "unexpected error payload: {}",
            response.body
        );
        assert_eq!(
            response.body["errors"][0]["extensions"]["problems"][0]["path"],
            json!(["customerGets", "value", "discountAmount", deprecated_field])
        );
    }
}

#[test]
fn discount_code_basic_buyer_context_lifecycle_stages_segment_readback() {
    let mut proxy = snapshot_proxy();

    let create_query = r#"
        mutation DiscountCodeBasicBuyerContextCreate($input: DiscountCodeBasicInput!) {
          discountCodeBasicCreate(basicCodeDiscount: $input) {
            codeDiscountNode {
              id
              codeDiscount {
                __typename
                ... on DiscountCodeBasic {
                  title
                  status
                  codes(first: 1) { nodes { code asyncUsageCount } }
                  context {
                    __typename
                    ... on DiscountCustomers { customers { __typename id } }
                    ... on DiscountCustomerSegments { segments { __typename id } }
                  }
                }
              }
            }
            userErrors { field message code extraInfo }
          }
        }
    "#;
    let create_input = json!({
        "title": "HAR-390 code customer context 1777346878525",
        "code": "HAR390CTX1777346878525",
        "startsAt": "2023-01-01T00:00:00Z",
        "combinesWith": { "productDiscounts": false, "orderDiscounts": true, "shippingDiscounts": false },
        "context": { "customers": { "add": ["gid://shopify/Customer/10548596015410"] } },
        "customerGets": { "value": { "percentage": 0.1 }, "items": { "all": true } }
    });
    let created = proxy.process_request(json_graphql_request(
        create_query,
        json!({ "input": create_input }),
    ));
    let discount_id = json_string(
        &created.body["data"]["discountCodeBasicCreate"]["codeDiscountNode"]["id"],
        "code buyer context discount id",
    );
    assert_synthetic_gid(&discount_id, "DiscountCodeNode");
    assert_eq!(
        created.body["data"]["discountCodeBasicCreate"]["codeDiscountNode"]["codeDiscount"]
            ["context"],
        json!({
            "__typename": "DiscountCustomers",
            "customers": [{
                "__typename": "Customer",
                "id": "gid://shopify/Customer/10548596015410"
            }]
        })
    );

    let update_query = r#"
        mutation DiscountCodeBasicBuyerContextUpdate($id: ID!, $input: DiscountCodeBasicInput!) {
          discountCodeBasicUpdate(id: $id, basicCodeDiscount: $input) {
            codeDiscountNode { id codeDiscount { __typename ... on DiscountCodeBasic { title status codes(first: 1) { nodes { code asyncUsageCount } } context { __typename ... on DiscountCustomerSegments { segments { __typename id } } } } } }
            userErrors { field message code extraInfo }
          }
        }
    "#;
    let update_input = json!({
        "title": "HAR-390 code segment context 1777346878525",
        "code": "HAR390SEG1777346878525",
        "startsAt": "2023-01-01T00:00:00Z",
        "combinesWith": { "productDiscounts": false, "orderDiscounts": true, "shippingDiscounts": false },
        "context": { "customerSegments": { "add": ["gid://shopify/Segment/647746715954"] } },
        "customerGets": { "value": { "discountAmount": { "amount": "5.00", "appliesOnEachItem": false } }, "items": { "all": true } }
    });
    let updated = proxy.process_request(json_graphql_request(
        update_query,
        json!({ "id": discount_id.clone(), "input": update_input }),
    ));
    assert_eq!(
        updated.body["data"]["discountCodeBasicUpdate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        updated.body["data"]["discountCodeBasicUpdate"]["codeDiscountNode"]["codeDiscount"]
            ["context"],
        json!({
            "__typename": "DiscountCustomerSegments",
            "segments": [{
                "__typename": "Segment",
                "id": "gid://shopify/Segment/647746715954"
            }]
        })
    );

    let read = proxy.process_request(json_graphql_request(r#"
        query DiscountCodeBasicBuyerContextRead($id: ID!, $code: String!) {
          discountNode(id: $id) { id discount { __typename ... on DiscountCodeBasic { title context { __typename ... on DiscountCustomerSegments { segments { __typename id } } } } } }
          codeDiscountNodeByCode(code: $code) { codeDiscount { __typename ... on DiscountCodeBasic { title context { __typename ... on DiscountCustomerSegments { segments { __typename id } } } } } }
        }
    "#, json!({ "id": discount_id.clone(), "code": "HAR390SEG1777346878525" })));
    assert_eq!(
        read.body["data"]["discountNode"]["discount"]["title"],
        json!("HAR-390 code segment context 1777346878525")
    );
    assert_eq!(
        read.body["data"]["codeDiscountNodeByCode"]["codeDiscount"]["context"]["segments"][0]["id"],
        json!("gid://shopify/Segment/647746715954")
    );

    let deleted = proxy.process_request(json_graphql_request(r#"
        mutation DiscountCodeBasicBuyerContextDelete($id: ID!) {
          discountCodeDelete(id: $id) { deletedCodeDiscountId userErrors { field message code extraInfo } }
        }
    "#, json!({ "id": discount_id })));
    assert_eq!(
        deleted.body["data"]["discountCodeDelete"]["userErrors"],
        json!([])
    );
}

#[test]
fn discount_basic_rejects_discount_on_quantity_for_non_bxgy_inputs() {
    let mut proxy = snapshot_proxy();

    let code_setup = proxy.process_request(json_graphql_request(
        r#"
        mutation DiscountBasicDisallowedQuantityCodeCreate($input: DiscountCodeBasicInput!) {
          discountCodeBasicCreate(basicCodeDiscount: $input) { codeDiscountNode { id } userErrors { field message code extraInfo } }
        }
        "#,
        json!({ "input": {
            "title": "Basic disallowed quantity code SETUP 1778038410003",
            "code": "BASICQTYSETUP1778038410003",
            "startsAt": "2026-04-25T00:00:00Z",
            "customerSelection": { "all": true },
            "customerGets": { "value": { "percentage": 0.1 }, "items": { "all": true } }
        }}),
    ));
    let code_discount_id = json_string(
        &code_setup.body["data"]["discountCodeBasicCreate"]["codeDiscountNode"]["id"],
        "basic quantity validation code discount id",
    );
    assert_synthetic_gid(&code_discount_id, "DiscountCodeNode");
    assert_eq!(
        code_setup.body["data"]["discountCodeBasicCreate"]["userErrors"],
        json!([])
    );

    let automatic_setup = proxy.process_request(json_graphql_request(
        r#"
        mutation DiscountBasicDisallowedQuantityAutomaticCreate($input: DiscountAutomaticBasicInput!) {
          discountAutomaticBasicCreate(automaticBasicDiscount: $input) { automaticDiscountNode { id } userErrors { field message code extraInfo } }
        }
        "#,
        json!({ "input": {
            "title": "Basic disallowed quantity automatic SETUP 1778038410003",
            "startsAt": "2026-04-25T00:00:00Z",
            "customerGets": { "value": { "percentage": 0.1 }, "items": { "all": true } }
        }}),
    ));
    let automatic_discount_id = json_string(
        &automatic_setup.body["data"]["discountAutomaticBasicCreate"]["automaticDiscountNode"]
            ["id"],
        "basic quantity validation automatic discount id",
    );
    assert_synthetic_gid(&automatic_discount_id, "DiscountAutomaticNode");
    assert_eq!(
        automatic_setup.body["data"]["discountAutomaticBasicCreate"]["userErrors"],
        json!([])
    );

    let invalid_value = json!({
        "title": "Basic disallowed quantity CREATE 1778038410003",
        "startsAt": "2026-04-25T00:00:00Z",
        "customerGets": {
            "value": { "discountOnQuantity": { "quantity": "2", "effect": { "percentage": 0.5 } } },
            "items": { "all": true }
        }
    });
    let mut invalid_code_value = invalid_value.clone();
    invalid_code_value["code"] = json!("BASICQTYCREATE1778038410003");

    let code_create = proxy.process_request(json_graphql_request(
        r#"
        mutation DiscountBasicDisallowedQuantityCodeCreate($input: DiscountCodeBasicInput!) {
          discountCodeBasicCreate(basicCodeDiscount: $input) { codeDiscountNode { id } userErrors { field message code extraInfo } }
        }
        "#,
        json!({ "input": invalid_code_value }),
    ));
    assert_eq!(
        code_create.body["data"]["discountCodeBasicCreate"]["codeDiscountNode"],
        json!(null)
    );
    assert_eq!(
        code_create.body["data"]["discountCodeBasicCreate"]["userErrors"][0]["field"],
        json!([
            "basicCodeDiscount",
            "customerGets",
            "value",
            "discountOnQuantity"
        ])
    );
    assert_eq!(
        code_create.body["data"]["discountCodeBasicCreate"]["userErrors"][0]["code"],
        json!("INVALID")
    );

    let automatic_update = proxy.process_request(json_graphql_request(
        r#"
        mutation DiscountBasicDisallowedQuantityAutomaticUpdate($id: ID!, $input: DiscountAutomaticBasicInput!) {
          discountAutomaticBasicUpdate(id: $id, automaticBasicDiscount: $input) { automaticDiscountNode { id } userErrors { field message code extraInfo } }
        }
        "#,
        json!({ "id": automatic_discount_id, "input": invalid_value }),
    ));
    assert_eq!(
        automatic_update.body["data"]["discountAutomaticBasicUpdate"]["automaticDiscountNode"],
        json!(null)
    );
    assert_eq!(
        automatic_update.body["data"]["discountAutomaticBasicUpdate"]["userErrors"][0]["field"],
        json!([
            "automaticBasicDiscount",
            "customerGets",
            "value",
            "discountOnQuantity"
        ])
    );
    assert_eq!(
        automatic_update.body["data"]["discountAutomaticBasicUpdate"]["userErrors"][0]["message"],
        json!("discountOnQuantity field is only permitted with bxgy discounts.")
    );
}

#[test]
fn discount_bxgy_numeric_validation_handles_bounds_and_variable_coercion() {
    let mut proxy = snapshot_proxy();

    let code_query = r#"
        mutation DiscountBxgyNumericValidationCodeCreate($input: DiscountCodeBxgyInput!) {
          discountCodeBxgyCreate(bxgyCodeDiscount: $input) { codeDiscountNode { id } userErrors { field message code extraInfo } }
        }
    "#;
    let automatic_query = r#"
        mutation DiscountBxgyNumericValidationAutomaticUpdate($id: ID!, $input: DiscountAutomaticBxgyInput!) {
          discountAutomaticBxgyUpdate(id: $id, automaticBxgyDiscount: $input) { automaticDiscountNode { id } userErrors { field message code extraInfo } }
        }
    "#;
    let automatic_create_query = r#"
        mutation DiscountBxgyNumericValidationAutomaticCreate($input: DiscountAutomaticBxgyInput!) {
          discountAutomaticBxgyCreate(automaticBxgyDiscount: $input) { automaticDiscountNode { id } userErrors { field message code extraInfo } }
        }
    "#;

    let mut base = json!({
        "title": "Conformance BXGY code SETUP 1778195290726",
        "code": "BXGYNSETUP1778195290726",
        "startsAt": "2026-04-25T00:00:00Z",
        "combinesWith": { "productDiscounts": true, "orderDiscounts": false, "shippingDiscounts": false },
        "context": { "all": "ALL" },
        "customerBuys": { "value": { "quantity": "1" }, "items": { "products": { "productsToAdd": ["gid://shopify/Product/10180236017970"] } } },
        "customerGets": { "value": { "discountOnQuantity": { "quantity": "1", "effect": { "percentage": 0.5 } } }, "items": { "products": { "productsToAdd": ["gid://shopify/Product/10180236017970"] } } }
    });

    let setup = proxy.process_request(json_graphql_request(
        code_query,
        json!({ "input": base.clone() }),
    ));
    assert_eq!(
        setup.body["data"]["discountCodeBxgyCreate"]["userErrors"],
        json!([])
    );
    let code_discount_id = json_string(
        &setup.body["data"]["discountCodeBxgyCreate"]["codeDiscountNode"]["id"],
        "bxgy numeric code discount id",
    );
    assert_synthetic_gid(&code_discount_id, "DiscountCodeNode");
    let mut automatic_base = base.clone();
    automatic_base.as_object_mut().unwrap().remove("code");
    let automatic_setup = proxy.process_request(json_graphql_request(
        automatic_create_query,
        json!({ "input": automatic_base.clone() }),
    ));
    let automatic_discount_id = json_string(
        &automatic_setup.body["data"]["discountAutomaticBxgyCreate"]["automaticDiscountNode"]["id"],
        "bxgy numeric automatic discount id",
    );
    assert_synthetic_gid(&automatic_discount_id, "DiscountAutomaticNode");
    assert_eq!(
        automatic_setup.body["data"]["discountAutomaticBxgyCreate"]["userErrors"],
        json!([])
    );

    base["usesPerOrderLimit"] = json!(0);
    let uses_zero = proxy.process_request(json_graphql_request(
        code_query,
        json!({ "input": base.clone() }),
    ));
    assert_eq!(
        uses_zero.body["data"]["discountCodeBxgyCreate"]["codeDiscountNode"],
        json!(null)
    );
    assert_eq!(
        uses_zero.body["data"]["discountCodeBxgyCreate"]["userErrors"][0],
        json!({
            "field": ["bxgyCodeDiscount", "usesPerOrderLimit"],
            "message": "Allocation limit cannot be zero",
            "code": "VALUE_OUTSIDE_RANGE",
            "extraInfo": null
        })
    );

    base["usesPerOrderLimit"] = json!("1.5");
    let uses_float = proxy.process_request(json_graphql_request(
        code_query,
        json!({ "input": base.clone() }),
    ));
    assert_eq!(
        uses_float.body["errors"][0]["extensions"]["code"],
        json!("INVALID_VARIABLE")
    );
    assert_eq!(
        uses_float.body["errors"][0]["extensions"]["problems"][0]["path"],
        json!(["usesPerOrderLimit"])
    );

    base.as_object_mut().unwrap().remove("usesPerOrderLimit");
    base["customerBuys"]["value"]["quantity"] = json!("100000");
    let buy_too_large = proxy.process_request(json_graphql_request(
        code_query,
        json!({ "input": base.clone() }),
    ));
    assert_eq!(
        buy_too_large.body["data"]["discountCodeBxgyCreate"]["userErrors"][0]["message"],
        json!("Prerequisite to entitlement quantity ratio antecedent must be less than 100000")
    );

    base["customerBuys"]["value"]["quantity"] = json!("1");
    automatic_base["customerGets"]["value"]["discountOnQuantity"]["quantity"] = json!("0");
    let get_zero = proxy.process_request(json_graphql_request(
        automatic_query,
        json!({ "id": automatic_discount_id.clone(), "input": automatic_base.clone() }),
    ));
    assert_eq!(
        get_zero.body["data"]["discountAutomaticBxgyUpdate"]["userErrors"][0]["field"],
        json!([
            "automaticBxgyDiscount",
            "customerGets",
            "value",
            "discountOnQuantity",
            "quantity"
        ])
    );

    automatic_base["customerGets"]["value"]["discountOnQuantity"]["quantity"] = json!("2");
    let ratio_ok = proxy.process_request(json_graphql_request(
        automatic_query,
        json!({ "id": automatic_discount_id.clone(), "input": automatic_base }),
    ));
    assert_eq!(
        ratio_ok.body["data"]["discountAutomaticBxgyUpdate"]["automaticDiscountNode"]["id"],
        json!(automatic_discount_id)
    );
    assert_eq!(
        ratio_ok.body["data"]["discountAutomaticBxgyUpdate"]["userErrors"],
        json!([])
    );
}

#[test]
fn discount_bxgy_lifecycle_stages_code_and_automatic_readback() {
    let mut proxy = snapshot_proxy();

    let create_query = r#"
        mutation DiscountBxgyLifecycleCreate($codeInput: DiscountCodeBxgyInput!, $automaticInput: DiscountAutomaticBxgyInput!) {
          discountCodeBxgyCreate(bxgyCodeDiscount: $codeInput) {
            codeDiscountNode {
              id
              codeDiscount {
                __typename
                ... on DiscountCodeBxgy {
                  title status summary usesPerOrderLimit
                  codes(first: 2) { nodes { code asyncUsageCount } }
                  customerBuys { value { __typename ... on DiscountQuantity { quantity } } items { __typename ... on DiscountProducts { products(first: 5) { nodes { id } } productVariants(first: 5) { nodes { id } } } } }
                  customerGets { value { __typename ... on DiscountOnQuantity { quantity { quantity } effect { __typename ... on DiscountPercentage { percentage } } } } items { __typename ... on DiscountCollections { collections(first: 5) { nodes { id } } } } appliesOnOneTimePurchase appliesOnSubscription }
                }
              }
            }
            userErrors { field message code extraInfo }
          }
          discountAutomaticBxgyCreate(automaticBxgyDiscount: $automaticInput) {
            automaticDiscountNode {
              id
              automaticDiscount {
                __typename
                ... on DiscountAutomaticBxgy {
                  title status summary usesPerOrderLimit
                  customerBuys { value { __typename ... on DiscountQuantity { quantity } } items { __typename ... on DiscountCollections { collections(first: 5) { nodes { id } } } } }
                  customerGets { value { __typename ... on DiscountOnQuantity { quantity { quantity } effect { __typename ... on DiscountPercentage { percentage } } } } items { __typename ... on DiscountProducts { products(first: 5) { nodes { id } } productVariants(first: 5) { nodes { id } } } } appliesOnOneTimePurchase appliesOnSubscription }
                }
              }
            }
            userErrors { field message code extraInfo }
          }
        }
    "#;
    let code_input = json!({
        "title": "HAR-195 code BXGY 1777150259502",
        "code": "HAR195BXGY1777150259502",
        "startsAt": "2026-04-25T00:00:00Z",
        "combinesWith": { "productDiscounts": true, "orderDiscounts": false, "shippingDiscounts": false },
        "context": { "all": "ALL" },
        "customerBuys": { "value": { "quantity": "2" }, "items": { "products": { "productsToAdd": ["gid://shopify/Product/10170555597106"], "productVariantsToAdd": ["gid://shopify/ProductVariant/51098643235122"] } } },
        "customerGets": { "value": { "discountOnQuantity": { "quantity": "1", "effect": { "percentage": 1 } } }, "items": { "collections": { "add": ["gid://shopify/Collection/512147128626"] } } },
        "usesPerOrderLimit": 1
    });
    let automatic_input = json!({
        "title": "HAR-195 automatic BXGY 1777150259502",
        "startsAt": "2026-04-25T00:00:00Z",
        "combinesWith": { "productDiscounts": true, "orderDiscounts": false, "shippingDiscounts": false },
        "context": { "all": "ALL" },
        "customerBuys": { "value": { "quantity": "1" }, "items": { "collections": { "add": ["gid://shopify/Collection/512147128626"] } } },
        "customerGets": { "value": { "discountOnQuantity": { "quantity": "1", "effect": { "percentage": 0.5 } } }, "items": { "products": { "productsToAdd": ["gid://shopify/Product/10170555629874"] } } },
        "usesPerOrderLimit": "1"
    });

    let created = proxy.process_request(json_graphql_request(
        create_query,
        json!({ "codeInput": code_input, "automaticInput": automatic_input }),
    ));
    assert_eq!(
        created.body["data"]["discountCodeBxgyCreate"]["userErrors"],
        json!([])
    );
    let code_id = json_string(
        &created.body["data"]["discountCodeBxgyCreate"]["codeDiscountNode"]["id"],
        "bxgy lifecycle code discount id",
    );
    assert_synthetic_gid(&code_id, "DiscountCodeNode");
    assert_eq!(
        created.body["data"]["discountCodeBxgyCreate"]["codeDiscountNode"]["codeDiscount"]
            ["summary"],
        json!("Buy 2 items, get 1 item free")
    );
    assert_eq!(
        created.body["data"]["discountCodeBxgyCreate"]["codeDiscountNode"]["codeDiscount"]
            ["customerBuys"]["items"]["products"]["nodes"][0]["id"],
        json!("gid://shopify/Product/10170555597106")
    );
    let automatic_id = json_string(
        &created.body["data"]["discountAutomaticBxgyCreate"]["automaticDiscountNode"]["id"],
        "bxgy lifecycle automatic discount id",
    );
    assert_synthetic_gid(&automatic_id, "DiscountAutomaticNode");
    assert_eq!(
        created.body["data"]["discountAutomaticBxgyCreate"]["automaticDiscountNode"]
            ["automaticDiscount"]["summary"],
        json!("Buy 1 item, get 1 item at 50% off")
    );
    assert_eq!(
        created.body["data"]["discountAutomaticBxgyCreate"]["automaticDiscountNode"]
            ["automaticDiscount"]["customerGets"]["items"]["products"]["nodes"][0]["id"],
        json!("gid://shopify/Product/10170555629874")
    );

    let code_update_query = r#"
        mutation DiscountCodeBxgyLifecycleUpdate($id: ID!, $input: DiscountCodeBxgyInput!) {
          discountCodeBxgyUpdate(id: $id, bxgyCodeDiscount: $input) { codeDiscountNode { id codeDiscount { __typename ... on DiscountCodeBxgy { title status summary customerGets { value { __typename ... on DiscountOnQuantity { quantity { quantity } effect { __typename ... on DiscountPercentage { percentage } } } } } } } } userErrors { field message code extraInfo } }
        }
    "#;
    let code_update_input = json!({
        "title": "HAR-195 code BXGY updated 1777150259502",
        "code": "HAR195BXGYUP1777150259502",
        "startsAt": "2026-04-25T00:00:00Z",
        "combinesWith": { "productDiscounts": true, "orderDiscounts": false, "shippingDiscounts": false },
        "context": { "all": "ALL" },
        "customerBuys": { "value": { "quantity": "2" }, "items": { "products": { "productsToAdd": ["gid://shopify/Product/10170555597106"], "productVariantsToAdd": ["gid://shopify/ProductVariant/51098643235122"] } } },
        "customerGets": { "value": { "discountOnQuantity": { "quantity": "2", "effect": { "percentage": 0.5 } } }, "items": { "collections": { "add": ["gid://shopify/Collection/512147128626"] } } },
        "usesPerOrderLimit": 1
    });
    let updated_code = proxy.process_request(json_graphql_request(
        code_update_query,
        json!({ "id": code_id.clone(), "input": code_update_input.clone() }),
    ));
    assert_eq!(
        updated_code.body["data"]["discountCodeBxgyUpdate"]["codeDiscountNode"]["codeDiscount"]
            ["title"],
        json!("HAR-195 code BXGY updated 1777150259502")
    );
    assert_eq!(
        updated_code.body["data"]["discountCodeBxgyUpdate"]["codeDiscountNode"]["codeDiscount"]
            ["summary"],
        json!("Buy 2 items, get 2 items at 50% off")
    );

    let status_query = r#"
        mutation DiscountCodeBxgyLifecycleStatus($id: ID!) {
          discountCodeDeactivate(id: $id) { codeDiscountNode { id codeDiscount { __typename ... on DiscountCodeBxgy { status endsAt } } } userErrors { field message code extraInfo } }
        }
    "#;
    let deactivated = proxy.process_request(json_graphql_request(
        status_query,
        json!({ "id": code_id.clone() }),
    ));
    assert_eq!(
        deactivated.body["data"]["discountCodeDeactivate"]["codeDiscountNode"]["codeDiscount"]
            ["status"],
        json!("EXPIRED")
    );

    let read = proxy.process_request(json_graphql_request(r#"
        query DiscountBxgyLifecycleRead($codeId: ID!, $automaticId: ID!, $code: String!) {
          discountNode(id: $codeId) { id discount { __typename ... on DiscountCodeBxgy { title status } } }
          codeDiscountNodeByCode(code: $code) { id }
          automaticDiscountNode(id: $automaticId) { id automaticDiscount { __typename ... on DiscountAutomaticBxgy { title status } } }
        }
    "#, json!({ "codeId": code_id.clone(), "automaticId": automatic_id.clone(), "code": "HAR195BXGYUP1777150259502" })));
    assert_eq!(
        read.body["data"]["discountNode"]["discount"]["title"],
        json!("HAR-195 code BXGY updated 1777150259502")
    );
    assert_eq!(
        read.body["data"]["codeDiscountNodeByCode"]["id"],
        json!(code_id)
    );
    assert_eq!(
        read.body["data"]["automaticDiscountNode"]["automaticDiscount"]["title"],
        json!("HAR-195 automatic BXGY 1777150259502")
    );

    let delete_query = r#"
        mutation DiscountBxgyLifecycleDelete($codeId: ID!, $automaticId: ID!) {
          discountCodeDelete(id: $codeId) { deletedCodeDiscountId userErrors { field message code extraInfo } }
          discountAutomaticDelete(id: $automaticId) { deletedAutomaticDiscountId userErrors { field message code extraInfo } }
        }
    "#;
    let deleted = proxy.process_request(json_graphql_request(
        delete_query,
        json!({ "codeId": code_id, "automaticId": automatic_id }),
    ));
    assert_eq!(
        deleted.body["data"]["discountCodeDelete"]["userErrors"],
        json!([])
    );
    assert_eq!(
        deleted.body["data"]["discountAutomaticDelete"]["userErrors"],
        json!([])
    );
}

fn fallback_product_title_value() -> &'static str {
    "The Inventory Not Tracked Snowboard"
}

fn fallback_product_body_value() -> &'static str {
    "<p>Fallback snowboard body</p>"
}

fn create_fallback_localization_product(proxy: &mut DraftProxy) -> String {
    let created = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateFallbackLocalizationProduct($product: ProductCreateInput!) {
          productCreate(product: $product) {
            product { id }
            userErrors { field message  }
          }
        }
        "#,
        json!({
            "product": {
                "title": fallback_product_title_value(),
                "handle": "the-inventory-not-tracked-snowboard",
                "descriptionHtml": fallback_product_body_value(),
                "productType": "snowboard"
            }
        }),
    ));
    assert_eq!(
        created.body["data"]["productCreate"]["userErrors"],
        json!([])
    );
    created.body["data"]["productCreate"]["product"]["id"]
        .as_str()
        .expect("fallback localization product id should be present")
        .to_string()
}

fn fallback_product_title_digest() -> String {
    localization_content_digest(fallback_product_title_value())
}

fn fallback_product_body_digest() -> String {
    localization_content_digest(fallback_product_body_value())
}

fn content_digest(content: &Value, key: &str) -> String {
    content
        .as_array()
        .unwrap_or_else(|| panic!("translatableContent should be an array, got {content}"))
        .iter()
        .find(|entry| entry["key"] == json!(key))
        .unwrap_or_else(|| panic!("translatableContent should include {key}, got {content}"))
        ["digest"]
        .as_str()
        .unwrap_or_else(|| panic!("{key} digest should be a string, got {content}"))
        .to_string()
}

fn localization_content_digest(value: &str) -> String {
    use sha2::{Digest, Sha256};

    format!("{:x}", Sha256::digest(value.as_bytes()))
}

fn fallback_product_handle_digest() -> String {
    localization_content_digest("the-inventory-not-tracked-snowboard")
}

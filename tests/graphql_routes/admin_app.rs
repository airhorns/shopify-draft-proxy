use super::common::*;
use pretty_assertions::assert_eq;

const DERIVED_RETURN_CONFIRMATION_URL: &str =
    "https://app.example.test/return?shopify_draft_proxy_confirmation=1";
const DERIVED_DEFAULT_CONFIRMATION_URL: &str =
    "https://shopify.com/local-confirmation?shopify_draft_proxy_confirmation=1";

fn synthetic_gid(value: &Value, resource: &str) -> String {
    let id = value.as_str().expect("synthetic gid should be a string");
    assert!(
        id.starts_with(&format!("gid://shopify/{resource}/")),
        "{id} should be a {resource} gid"
    );
    assert!(
        id.contains("shopify-draft-proxy=synthetic"),
        "{id} should be marked as synthetic"
    );
    id.to_string()
}

fn observed_app_subscription(id: &str, status: &str, trial_days: i64, line_items: Value) -> Value {
    json!({
        "__typename": "AppSubscription",
        "id": id,
        "name": "Observed Shopify plan",
        "status": status,
        "test": true,
        "trialDays": trial_days,
        "currentPeriodEnd": "2026-05-10T00:00:00.000Z",
        "createdAt": "2026-04-10T00:00:00.000Z",
        "returnUrl": "https://app.example.test/return",
        "lineItems": line_items,
    })
}

#[test]
fn delegate_access_token_create_validates_and_stages_synthetic_secret() {
    let mut proxy = snapshot_proxy();

    let empty = proxy.process_request(json_graphql_request(
        r#"
        mutation DelegateAccessTokenCreateEmptyScopeValidation {
          delegateAccessTokenCreate(input: { delegateAccessScope: [] }) {
            delegateAccessToken { accessToken accessScopes createdAt expiresIn }
            userErrors { field message code }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        empty.body["data"]["delegateAccessTokenCreate"],
        json!({
            "delegateAccessToken": null,
            "userErrors": [{ "field": null, "message": "The access scope can't be empty.", "code": "EMPTY_ACCESS_SCOPE" }]
        })
    );

    let negative_expires = proxy.process_request(json_graphql_request(
        r#"
        mutation DelegateAccessTokenCreateNegativeExpiresValidation {
          delegateAccessTokenCreate(input: { delegateAccessScope: ["read_products"], expiresIn: -1 }) {
            delegateAccessToken { accessToken accessScopes createdAt expiresIn }
            userErrors { field message code }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        negative_expires.body["data"]["delegateAccessTokenCreate"],
        json!({
            "delegateAccessToken": null,
            "userErrors": [{ "field": null, "message": "The expires_in value must be greater than 0.", "code": "NEGATIVE_EXPIRES_IN" }]
        })
    );

    let unknown_scope = proxy.process_request(json_graphql_request(
        r#"
        mutation DelegateAccessTokenCreateUnknownScopeValidation {
          delegateAccessTokenCreate(input: { delegateAccessScope: ["fake_scope"] }) {
            delegateAccessToken { accessToken accessScopes createdAt expiresIn }
            userErrors { field message code }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        unknown_scope.body["data"]["delegateAccessTokenCreate"],
        json!({
            "delegateAccessToken": null,
            "userErrors": [{ "field": null, "message": "The access scope is invalid: fake_scope", "code": "UNKNOWN_SCOPES" }]
        })
    );

    let happy = proxy.process_request(json_graphql_request(
        r#"
        mutation DelegateAccessTokenCreateHappyValidation {
          aliasCreate: delegateAccessTokenCreate(input: { delegateAccessScope: ["read_products"], expiresIn: 300 }) {
            delegateAccessToken { accessToken accessScopes createdAt expiresIn }
            userErrors { field message code }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        happy.body["data"]["aliasCreate"]["delegateAccessToken"]["accessScopes"],
        json!(["read_products"])
    );
    assert!(
        happy.body["data"]["aliasCreate"]["delegateAccessToken"]["accessToken"]
            .as_str()
            .is_some_and(|token| token.starts_with("shpat_delegate_proxy_"))
    );
    assert_eq!(
        happy.body["data"]["aliasCreate"]["delegateAccessToken"]["createdAt"],
        json!("2024-01-01T00:00:01.000Z")
    );
    assert_eq!(
        happy.body["data"]["aliasCreate"]["delegateAccessToken"]["expiresIn"],
        json!(300)
    );
    assert_eq!(happy.body["data"]["aliasCreate"]["userErrors"], json!([]));
}

#[test]
fn delegate_access_token_payload_hydrates_selected_shop_identity_in_live_hybrid() {
    let upstream_calls = Arc::new(Mutex::new(Vec::<Value>::new()));
    let captured_calls = Arc::clone(&upstream_calls);
    let mut proxy =
        configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport(move |request| {
            let body: Value =
                serde_json::from_str(&request.body).expect("shop identity hydrate parses");
            captured_calls.lock().unwrap().push(body.clone());
            assert!(body["query"]
                .as_str()
                .is_some_and(|query| query.contains("query ProductPayloadShopHydrate")));
            Response {
                status: 200,
                headers: Default::default(),
                body: json!({
                    "data": {
                        "shop": {
                            "id": "gid://shopify/Shop/live",
                            "name": "Live shop",
                            "myshopifyDomain": "live-shop.myshopify.com",
                            "url": "https://live-shop.example",
                            "currencyCode": "CAD",
                            "primaryDomain": {
                                "id": "gid://shopify/Domain/live",
                                "host": "live-shop.example",
                                "url": "https://live-shop.example",
                                "sslEnabled": true
                            }
                        }
                    }
                }),
            }
        });

    let response = proxy.process_request(json_graphql_request(
        r#"
        mutation DelegateAccessTokenSelectedShop {
          delegateAccessTokenCreate(
            input: { delegateAccessScope: ["read_products"], expiresIn: 300 }
          ) {
            delegateAccessToken { accessToken }
            shop { id myshopifyDomain currencyCode }
            userErrors { field message code }
          }
        }
        "#,
        json!({}),
    ));

    assert_eq!(response.status, 200);
    assert_eq!(response.body.get("errors"), None);
    assert_eq!(
        response.body["data"]["delegateAccessTokenCreate"]["shop"],
        json!({
            "id": "gid://shopify/Shop/live",
            "myshopifyDomain": "live-shop.myshopify.com",
            "currencyCode": "CAD"
        })
    );
    assert_eq!(
        response.body["data"]["delegateAccessTokenCreate"]["userErrors"],
        json!([])
    );
    assert_eq!(upstream_calls.lock().unwrap().len(), 1);
}

#[test]
fn apps_mutations_dispatch_by_root_field_for_ordinary_operation_names() {
    let mut proxy = configured_proxy(
        ReadMode::Snapshot,
        Some(shopify_draft_proxy::proxy::UnsupportedMutationMode::Reject),
    );

    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateSub($lineItems: [AppSubscriptionLineItemInput!]!) {
          appSubscriptionCreate(
            name: "Ordinary"
            returnUrl: "https://app.example.test/return"
            test: true
            lineItems: $lineItems
          ) {
            confirmationUrl
            appSubscription { id status lineItems { id } }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "lineItems": [{
                "plan": {
                    "appUsagePricingDetails": {
                        "cappedAmount": { "amount": 100, "currencyCode": "USD" },
                        "terms": "usage"
                    }
                }
            }]
        }),
    ));
    assert_eq!(create.status, 200);
    let subscription_id = synthetic_gid(
        &create.body["data"]["appSubscriptionCreate"]["appSubscription"]["id"],
        "AppSubscription",
    );
    let line_item_id = synthetic_gid(
        &create.body["data"]["appSubscriptionCreate"]["appSubscription"]["lineItems"][0]["id"],
        "AppSubscriptionLineItem",
    );
    assert_eq!(
        create.body["data"]["appSubscriptionCreate"],
        json!({
            "confirmationUrl": DERIVED_RETURN_CONFIRMATION_URL,
            "appSubscription": {
                "id": subscription_id,
                "status": "ACTIVE",
                "lineItems": [{ "id": line_item_id }]
            },
            "userErrors": []
        })
    );

    let usage = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateUsage($id: ID!) {
          appUsageRecordCreate(
            subscriptionLineItemId: $id
            price: { amount: "1.00", currencyCode: USD }
            description: "ordinary usage"
            idempotencyKey: "ordinary-usage-1"
          ) {
            appUsageRecord { id }
            userErrors { field message }
          }
        }
        "#,
        json!({ "id": line_item_id }),
    ));
    assert_eq!(usage.status, 200);
    let usage_record_id = synthetic_gid(
        &usage.body["data"]["appUsageRecordCreate"]["appUsageRecord"]["id"],
        "AppUsageRecord",
    );
    assert_eq!(
        usage.body["data"]["appUsageRecordCreate"],
        json!({
            "appUsageRecord": { "id": usage_record_id },
            "userErrors": []
        })
    );

    let roots = vec![
        (
            "CancelSub",
            r#"mutation CancelSub($id: ID!) { appSubscriptionCancel(id: $id) { appSubscription { id status } userErrors { field message } } }"#,
            json!({ "id": subscription_id }),
            "appSubscriptionCancel",
        ),
        (
            "ExtendTrial",
            r#"mutation ExtendTrial($id: ID!) { appSubscriptionTrialExtend(id: $id, days: 3) { appSubscription { id trialDays } userErrors { field message code } } }"#,
            json!({ "id": subscription_id }),
            "appSubscriptionTrialExtend",
        ),
        (
            "UpdateLineItem",
            r#"mutation UpdateLineItem($id: ID!) { appSubscriptionLineItemUpdate(id: $id, cappedAmount: { amount: 101, currencyCode: USD }) { appSubscription { id } userErrors { field message } } }"#,
            json!({ "id": line_item_id }),
            "appSubscriptionLineItemUpdate",
        ),
        (
            "OneTime",
            r#"mutation OneTime { appPurchaseOneTimeCreate(name: "Import", returnUrl: "https://app.example.test/return", price: { amount: 5, currencyCode: USD }, test: false) { appPurchaseOneTime { id test } confirmationUrl userErrors { field message  } } }"#,
            json!({}),
            "appPurchaseOneTimeCreate",
        ),
        (
            "RevokeScopes",
            r#"mutation RevokeScopes { appRevokeAccessScopes(scopes: ["fake_scope"]) { revoked { handle } userErrors { field message code } } }"#,
            json!({}),
            "appRevokeAccessScopes",
        ),
        (
            "CreateDelegate",
            r#"mutation CreateDelegate { delegateAccessTokenCreate(input: { delegateAccessScope: ["read_products"], expiresIn: 300 }) { delegateAccessToken { accessToken } userErrors { field message code } } }"#,
            json!({}),
            "delegateAccessTokenCreate",
        ),
    ];

    let mut delegate_token = String::new();
    for (_name, query, variables, root) in roots {
        let response = proxy.process_request(json_graphql_request(query, variables));
        assert_eq!(response.status, 200, "{root} should dispatch locally");
        assert!(
            response.body["data"][root].is_object(),
            "{root} should return a local payload, got {}",
            response.body
        );
        if root == "delegateAccessTokenCreate" {
            delegate_token = response.body["data"][root]["delegateAccessToken"]["accessToken"]
                .as_str()
                .unwrap()
                .to_string();
        }
    }

    let destroy = proxy.process_request(json_graphql_request(
        r#"
        mutation DestroyDelegate($token: String!) {
          delegateAccessTokenDestroy(accessToken: $token) {
            status
            userErrors { field message code }
          }
        }
        "#,
        json!({ "token": delegate_token }),
    ));
    assert_eq!(destroy.status, 200);
    assert_eq!(
        destroy.body["data"]["delegateAccessTokenDestroy"],
        json!({ "status": true, "userErrors": [] })
    );
}

#[test]
fn app_revoke_access_scopes_validates_atomically_and_updates_current_installation() {
    let mut proxy = snapshot_proxy();

    let unknown = proxy.process_request(json_graphql_request(
        r#"
        mutation AppRevokeAccessScopesFakeScope {
          appRevokeAccessScopes(scopes: ["fake_scope"]) {
            revoked { handle description }
            userErrors { field message code }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        unknown.body["data"]["appRevokeAccessScopes"],
        json!({
            "revoked": null,
            "userErrors": [{
                "field": ["scopes"],
                "message": "The requested list of scopes to revoke includes invalid handles.",
                "code": "UNKNOWN_SCOPES"
            }]
        })
    );

    let mixed = proxy.process_request(json_graphql_request(
        r#"
        mutation AppRevokeAccessScopesMixedFakeScope {
          appRevokeAccessScopes(scopes: ["read_products", "fake_scope"]) {
            revoked { handle description }
            userErrors { field message code }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        mixed.body["data"]["appRevokeAccessScopes"],
        json!({
            "revoked": null,
            "userErrors": [{
                "field": ["scopes"],
                "message": "The requested list of scopes to revoke includes invalid handles.",
                "code": "UNKNOWN_SCOPES"
            }]
        })
    );

    let required = proxy.process_request(json_graphql_request(
        r#"
        mutation AppRevokeAccessScopesRequiredReadProducts {
          appRevokeAccessScopes(scopes: ["read_products"]) {
            revoked { handle description }
            userErrors { field message code }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        required.body["data"]["appRevokeAccessScopes"],
        json!({
            "revoked": null,
            "userErrors": [{
                "field": ["scopes"],
                "message": "Scopes that are declared as required cannot be revoked.",
                "code": "CANNOT_REVOKE_REQUIRED_SCOPES"
            }]
        })
    );

    let mut missing_source_app_request = json_graphql_request(
        r#"
        mutation AppRevokeAccessScopesErrorCodes {
          appRevokeAccessScopes(scopes: ["write_products"]) {
            revoked { handle description }
            userErrors { field message code }
          }
        }
        "#,
        json!({}),
    );
    missing_source_app_request.headers.insert(
        "x-shopify-draft-proxy-source-app-missing".to_string(),
        "true".to_string(),
    );
    let missing_source_app = proxy.process_request(missing_source_app_request);
    assert_eq!(
        missing_source_app.body["data"]["appRevokeAccessScopes"],
        json!({
            "revoked": [],
            "userErrors": [{ "field": ["id"], "message": "No app found on the access token.", "code": "MISSING_SOURCE_APP" }]
        })
    );

    for query in [
        r#"
        mutation ConsumerNamedRevoke {
          appRevokeAccessScopes(scopes: ["write_products"]) {
            revoked { handle description }
            userErrors { field message code }
          }
        }
        "#,
        r#"
        mutation {
          appRevokeAccessScopes(scopes: ["write_products"]) {
            revoked { handle description }
            userErrors { field message code }
          }
        }
        "#,
    ] {
        let mut request = json_graphql_request(query, json!({}));
        request.headers.insert(
            "x-shopify-draft-proxy-source-app-missing".to_string(),
            "true".to_string(),
        );
        let response = proxy.process_request(request);
        assert_eq!(
            response.body["data"]["appRevokeAccessScopes"],
            json!({
                "revoked": [],
                "userErrors": [{ "field": ["id"], "message": "No app found on the access token.", "code": "MISSING_SOURCE_APP" }]
            })
        );
    }

    let optional = proxy.process_request(json_graphql_request(
        r#"
        mutation AppRevokeAccessScopesOptionalWriteProducts {
          appRevokeAccessScopes(scopes: ["write_products"]) {
            revoked { handle description }
            userErrors { field message code }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        optional.body["data"]["appRevokeAccessScopes"],
        json!({
            "revoked": [{ "handle": "write_products", "description": "Modify products, variants, and collections" }],
            "userErrors": []
        })
    );

    let read = proxy.process_request(json_graphql_request(
        r#"
        query AppAccessScopesLocalRead {
          currentAppInstallation { accessScopes { handle } }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        read.body,
        json!({ "data": { "currentAppInstallation": { "accessScopes": [{ "handle": "read_products" }] } } })
    );
}

#[test]
fn app_access_scopes_reflect_granted_non_products_scope_for_delegate_and_revoke() {
    let mut proxy = snapshot_proxy();

    let mut read = json_graphql_request(
        r#"
        query AppAccessScopesReadOrders {
          currentAppInstallation { accessScopes { handle } }
        }
        "#,
        json!({}),
    );
    read.headers.insert(
        "x-shopify-draft-proxy-access-scopes".to_string(),
        "read_products,write_products,read_orders".to_string(),
    );
    let read = proxy.process_request(read);
    assert_eq!(
        read.body["data"]["currentAppInstallation"]["accessScopes"],
        json!([
            { "handle": "read_products" },
            { "handle": "write_products" },
            { "handle": "read_orders" }
        ])
    );

    let mut delegate = json_graphql_request(
        r#"
        mutation DelegateReadOrders {
          delegateAccessTokenCreate(input: { delegateAccessScope: ["read_orders"], expiresIn: 300 }) {
            delegateAccessToken { accessScopes }
            userErrors { field message code }
          }
        }
        "#,
        json!({}),
    );
    delegate.headers.insert(
        "x-shopify-draft-proxy-access-scopes".to_string(),
        "read_products,write_products,read_orders".to_string(),
    );
    let delegate = proxy.process_request(delegate);
    assert_eq!(
        delegate.body["data"]["delegateAccessTokenCreate"],
        json!({
            "delegateAccessToken": { "accessScopes": ["read_orders"] },
            "userErrors": []
        })
    );

    let mut revoke = json_graphql_request(
        r#"
        mutation RevokeReadOrders {
          appRevokeAccessScopes(scopes: ["read_orders"]) {
            revoked { handle description }
            userErrors { field message code }
          }
        }
        "#,
        json!({}),
    );
    revoke.headers.insert(
        "x-shopify-draft-proxy-access-scopes".to_string(),
        "read_products,write_products,read_orders".to_string(),
    );
    let revoke = proxy.process_request(revoke);
    assert_eq!(
        revoke.body["data"]["appRevokeAccessScopes"],
        json!({
            "revoked": [{ "handle": "read_orders", "description": "Read orders, transactions, and fulfillments" }],
            "userErrors": []
        })
    );

    let mut read_after_revoke = json_graphql_request(
        r#"
        query AppAccessScopesAfterReadOrdersRevoke {
          currentAppInstallation { accessScopes { handle } }
        }
        "#,
        json!({}),
    );
    read_after_revoke.headers.insert(
        "x-shopify-draft-proxy-access-scopes".to_string(),
        "read_products,write_products,read_orders".to_string(),
    );
    let read_after_revoke = proxy.process_request(read_after_revoke);
    assert_eq!(
        read_after_revoke.body["data"]["currentAppInstallation"]["accessScopes"],
        json!([{ "handle": "read_products" }, { "handle": "write_products" }])
    );
}

#[test]
fn app_subscription_create_allocates_unique_ids_and_reads_both_subscriptions() {
    let mut proxy = snapshot_proxy();
    let create_query = r#"
        mutation CreateSubscription($name: String!) {
          appSubscriptionCreate(
            name: $name
            returnUrl: "https://app.example.test/return"
            test: true
            lineItems: [
              { plan: { appUsagePricingDetails: { cappedAmount: { amount: 100, currencyCode: USD }, terms: "usage terms" } } }
            ]
          ) {
            appSubscription { id lineItems { id } }
            userErrors { field message }
          }
        }
    "#;

    let first = proxy.process_request(json_graphql_request(
        create_query,
        json!({ "name": "First plan" }),
    ));
    let second = proxy.process_request(json_graphql_request(
        create_query,
        json!({ "name": "Second plan" }),
    ));

    let first_subscription_id = synthetic_gid(
        &first.body["data"]["appSubscriptionCreate"]["appSubscription"]["id"],
        "AppSubscription",
    );
    let second_subscription_id = synthetic_gid(
        &second.body["data"]["appSubscriptionCreate"]["appSubscription"]["id"],
        "AppSubscription",
    );
    let first_line_item_id = synthetic_gid(
        &first.body["data"]["appSubscriptionCreate"]["appSubscription"]["lineItems"][0]["id"],
        "AppSubscriptionLineItem",
    );
    let second_line_item_id = synthetic_gid(
        &second.body["data"]["appSubscriptionCreate"]["appSubscription"]["lineItems"][0]["id"],
        "AppSubscriptionLineItem",
    );
    assert_ne!(first_subscription_id, second_subscription_id);
    assert_ne!(first_line_item_id, second_line_item_id);
    assert_eq!(
        first.body["data"]["appSubscriptionCreate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        second.body["data"]["appSubscriptionCreate"]["userErrors"],
        json!([])
    );

    let read = proxy.process_request(json_graphql_request(
        r#"
        query ReadAllocatedSubscriptions {
          currentAppInstallation {
            allSubscriptions(first: 5) { nodes { id lineItems { id } } }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        read.body["data"]["currentAppInstallation"]["allSubscriptions"]["nodes"],
        json!([
            { "id": first_subscription_id, "lineItems": [{ "id": first_line_item_id }] },
            { "id": second_subscription_id, "lineItems": [{ "id": second_line_item_id }] }
        ])
    );
}

#[test]
fn app_lookup_and_uninstall_use_request_owned_app_identity() {
    let custom_app_id = "gid://shopify/App/347082227713";
    let mut proxy = snapshot_proxy();

    let mut node_request = json_graphql_request(
        r#"
        query CurrentAppNode($id: ID!) {
          node(id: $id) { ... on App { id handle } }
        }
        "#,
        json!({ "id": custom_app_id }),
    );
    node_request.headers.insert(
        "x-shopify-draft-proxy-api-client-id".to_string(),
        "347082227713".to_string(),
    );
    let node = proxy.process_request(node_request);
    assert_eq!(
        node.body["data"]["node"],
        json!({ "id": custom_app_id, "handle": "shopify-draft-proxy" })
    );

    let mut uninstall_request = json_graphql_request(
        r#"
        mutation CustomAppUninstall {
          appUninstall {
            app { id handle }
            userErrors { field message code }
          }
        }
        "#,
        json!({}),
    );
    uninstall_request.headers.insert(
        "x-shopify-draft-proxy-api-client-id".to_string(),
        "347082227713".to_string(),
    );
    let uninstall = proxy.process_request(uninstall_request);
    assert_eq!(
        uninstall.body["data"]["appUninstall"],
        json!({
            "app": { "id": custom_app_id, "handle": "shopify-draft-proxy" },
            "userErrors": []
        })
    );

    let mut missing_request = json_graphql_request(
        r#"
        mutation RepeatAppUninstall {
          appUninstall {
            app { id }
            userErrors { field message code }
          }
        }
        "#,
        json!({}),
    );
    missing_request.headers.insert(
        "x-shopify-draft-proxy-api-client-id".to_string(),
        "347082227713".to_string(),
    );
    let missing = proxy.process_request(missing_request);
    assert_eq!(
        missing.body["data"]["appUninstall"],
        json!({
            "app": null,
            "userErrors": [{
                "field": ["id"],
                "message": "App is not installed on shop",
                "code": "APP_NOT_INSTALLED"
            }]
        })
    );
}

#[test]
fn app_purchase_one_time_create_validates_and_stages_selected_fields() {
    let mut proxy = snapshot_proxy();

    let blank = proxy.process_request(json_graphql_request(
        r#"
        mutation AppPurchaseOneTimeCreateValidationBlankName {
          create: appPurchaseOneTimeCreate(name: "   ", returnUrl: "https://app.example.test/return", price: { amount: "5.00", currencyCode: USD }, test: true) {
            appPurchaseOneTime { id }
            confirmationUrl
            userErrors { field message  }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        blank.body["data"]["create"],
        json!({
            "appPurchaseOneTime": null,
            "confirmationUrl": null,
            "userErrors": [{ "field": ["name"], "message": "Name can't be blank" }]
        })
    );

    let zero_price = proxy.process_request(json_graphql_request(
        r#"
        mutation AppPurchaseOneTimeCreateValidationZeroPrice {
          appPurchaseOneTimeCreate(name: "Pro", returnUrl: "https://app.example.test/return", price: { amount: "0", currencyCode: USD }, test: true) {
            appPurchaseOneTime { id }
            confirmationUrl
            userErrors { field message  }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        zero_price.body["data"]["appPurchaseOneTimeCreate"],
        json!({
            "appPurchaseOneTime": null,
            "confirmationUrl": null,
            "userErrors": [{ "field": null, "message": "Validation failed: Price must be greater than or equal to 0.5" }]
        })
    );

    let currency_mismatch = proxy.process_request(json_graphql_request(
        r#"
        mutation AppPurchaseOneTimeCreateValidationCurrencyMismatch {
          appPurchaseOneTimeCreate(name: "Pro", returnUrl: "https://app.example.test/return", price: { amount: "5.00", currencyCode: EUR }, test: true) {
            appPurchaseOneTime { id price { amount currencyCode } }
            confirmationUrl
            userErrors { field message  }
          }
        }
        "#,
        json!({}),
    ));
    let eur_purchase_id = synthetic_gid(
        &currency_mismatch.body["data"]["appPurchaseOneTimeCreate"]["appPurchaseOneTime"]["id"],
        "AppPurchaseOneTime",
    );
    assert_eq!(
        currency_mismatch.body["data"]["appPurchaseOneTimeCreate"],
        json!({
            "appPurchaseOneTime": {
                "id": eur_purchase_id,
                "price": { "amount": "5.0", "currencyCode": "EUR" }
            },
            "confirmationUrl": DERIVED_RETURN_CONFIRMATION_URL,
            "userErrors": []
        })
    );

    let missing_return_url = proxy.process_request(json_graphql_request(
        r#"
        mutation AppPurchaseOneTimeCreateValidationMissingReturnUrl {
          appPurchaseOneTimeCreate(name: "Pro", price: { amount: "5.00", currencyCode: USD }, test: true) {
            appPurchaseOneTime { id }
            confirmationUrl
            userErrors { field message  }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        missing_return_url.body["errors"][0]["extensions"],
        json!({
            "code": "missingRequiredArguments",
            "className": "Field",
            "name": "appPurchaseOneTimeCreate",
            "arguments": "returnUrl"
        })
    );
    assert_eq!(
        missing_return_url.body["errors"][0]["path"],
        json!([
            "mutation AppPurchaseOneTimeCreateValidationMissingReturnUrl",
            "appPurchaseOneTimeCreate"
        ])
    );
    assert_ne!(
        missing_return_url.body["errors"][0]["locations"],
        json!([{ "line": 2, "column": 3 }])
    );

    let success = proxy.process_request(json_graphql_request(
        r#"
        mutation AppPurchaseOneTimeCreateValidationSuccess {
          appPurchaseOneTimeCreate(name: "HAR-646 valid test", returnUrl: "https://app.example.test/return", price: { amount: "5.00", currencyCode: USD }, test: true) {
            appPurchaseOneTime { id name status test createdAt price { amount currencyCode } }
            confirmationUrl
            userErrors { field message  }
          }
        }
        "#,
        json!({}),
    ));
    let purchase_id = synthetic_gid(
        &success.body["data"]["appPurchaseOneTimeCreate"]["appPurchaseOneTime"]["id"],
        "AppPurchaseOneTime",
    );
    assert_eq!(
        success.body["data"]["appPurchaseOneTimeCreate"],
        json!({
            "appPurchaseOneTime": {
                "id": purchase_id,
                "name": "HAR-646 valid test",
                "status": "ACTIVE",
                "test": true,
                "createdAt": "2024-01-01T00:00:02.000Z",
                "price": { "amount": "5.0", "currencyCode": "USD" }
            },
            "confirmationUrl": DERIVED_RETURN_CONFIRMATION_URL,
            "userErrors": []
        })
    );
}

#[test]
fn apps_user_errors_are_typed_and_selection_projected() {
    let mut proxy = snapshot_proxy();

    let first_uninstall = proxy.process_request(json_graphql_request(
        "mutation { appUninstall { app { id } userErrors { message } } }",
        json!({}),
    ));
    assert_eq!(
        first_uninstall.body["data"]["appUninstall"]["userErrors"],
        json!([])
    );

    let uninstall = proxy.process_request(json_graphql_request(
        r#"
        mutation {
          appUninstall {
            app { id }
            userErrors {
              __typename
              message
              ... on AppUninstallAppUninstallError { code }
            }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        uninstall.body["data"]["appUninstall"],
        json!({
            "app": null,
            "userErrors": [{
                "__typename": "AppUninstallAppUninstallError",
                "message": "App is not installed on shop",
                "code": "APP_NOT_INSTALLED"
            }]
        })
    );

    let revoke = proxy.process_request(json_graphql_request(
        r#"
        mutation {
          appRevokeAccessScopes(scopes: ["fake_scope"]) {
            userErrors {
              __typename
              field
              ... on AppRevokeAccessScopesAppRevokeScopeError { code }
            }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        revoke.body["data"]["appRevokeAccessScopes"],
        json!({
            "userErrors": [{
                "__typename": "AppRevokeAccessScopesAppRevokeScopeError",
                "field": ["scopes"],
                "code": "UNKNOWN_SCOPES"
            }]
        })
    );

    let delegate = proxy.process_request(json_graphql_request(
        r#"
        mutation {
          delegateAccessTokenCreate(input: { delegateAccessScope: [] }) {
            userErrors { __typename message }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        delegate.body["data"]["delegateAccessTokenCreate"],
        json!({
            "userErrors": [{
                "__typename": "DelegateAccessTokenCreateUserError",
                "message": "The access scope can't be empty."
            }]
        })
    );
}

#[test]
fn app_subscription_create_cancel_and_repeat_cancel_stages_status_transitions() {
    let mut proxy = snapshot_proxy();

    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation AppSubscriptionCreateLocalLifecycle($lineItems: [AppSubscriptionLineItemInput!]!) {
          appSubscriptionCreate(
            name: "Local plan"
            returnUrl: "https://app.example.test/return"
            trialDays: 7
            test: true
            lineItems: $lineItems
          ) {
            confirmationUrl
            appSubscription {
              id
              name
              status
              test
              trialDays
              lineItems {
                id
                plan {
                  pricingDetails {
                    __typename
                    ... on AppUsagePricing {
                      cappedAmount { amount currencyCode }
                      balanceUsed { amount currencyCode }
                      interval
                      terms
                    }
                  }
                }
              }
            }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "lineItems": [{
                "plan": {
                    "appUsagePricingDetails": {
                        "cappedAmount": { "amount": 100, "currencyCode": "USD" },
                        "terms": "usage terms"
                    }
                }
            }]
        }),
    ));
    assert_eq!(
        create.body["data"]["appSubscriptionCreate"]["userErrors"],
        json!([])
    );
    let subscription_id = synthetic_gid(
        &create.body["data"]["appSubscriptionCreate"]["appSubscription"]["id"],
        "AppSubscription",
    );
    let line_item_id = synthetic_gid(
        &create.body["data"]["appSubscriptionCreate"]["appSubscription"]["lineItems"][0]["id"],
        "AppSubscriptionLineItem",
    );
    assert_eq!(
        create.body["data"]["appSubscriptionCreate"]["appSubscription"],
        json!({
            "id": subscription_id,
            "name": "Local plan",
            "status": "ACTIVE",
            "test": true,
            "trialDays": 7,
            "lineItems": [{
                "id": line_item_id,
                "plan": { "pricingDetails": {
                    "__typename": "AppUsagePricing",
                    "cappedAmount": { "amount": "100.0", "currencyCode": "USD" },
                    "balanceUsed": { "amount": "0.0", "currencyCode": "USD" },
                    "interval": "EVERY_30_DAYS",
                    "terms": "usage terms"
                }}
            }]
        })
    );

    let cancel = proxy.process_request(json_graphql_request(
        r#"
        mutation AppSubscriptionCancelLocalLifecycle($id: ID!) {
          appSubscriptionCancel(id: $id, prorate: true) {
            appSubscription { id status trialDays }
            userErrors { field message }
          }
        }
        "#,
        json!({ "id": subscription_id }),
    ));
    assert_eq!(
        cancel.body["data"]["appSubscriptionCancel"],
        json!({
            "appSubscription": { "id": subscription_id, "status": "CANCELLED", "trialDays": 7 },
            "userErrors": []
        })
    );

    let repeat = proxy.process_request(json_graphql_request(
        r#"
        mutation AppSubscriptionCancelLocalLifecycle($id: ID!) {
          appSubscriptionCancel(id: $id, prorate: true) {
            appSubscription { id status trialDays }
            userErrors { field message }
          }
        }
        "#,
        json!({ "id": subscription_id }),
    ));
    assert_eq!(
        repeat.body["data"]["appSubscriptionCancel"],
        json!({
            "appSubscription": null,
            "userErrors": [{ "field": ["id"], "message": "Cannot transition status via :cancel from :cancelled" }]
        })
    );
}

#[test]
fn app_usage_record_create_caps_idempotency_and_readback_balance() {
    let mut proxy = snapshot_proxy();

    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation AppSubscriptionCreateLocalLifecycle($lineItems: [AppSubscriptionLineItemInput!]!) {
          appSubscriptionCreate(
            name: "Local plan"
            returnUrl: "https://app.example.test/return"
            trialDays: 7
            test: true
            lineItems: $lineItems
          ) {
            appSubscription {
              id
              lineItems {
                id
                plan { pricingDetails { __typename ... on AppUsagePricing { cappedAmount { amount currencyCode } } } }
              }
            }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "lineItems": [{
                "plan": { "appUsagePricingDetails": { "cappedAmount": { "amount": 5, "currencyCode": "USD" }, "terms": "usage terms" } }
            }]
        }),
    ));
    let line_item_id = synthetic_gid(
        &create.body["data"]["appSubscriptionCreate"]["appSubscription"]["lineItems"][0]["id"],
        "AppSubscriptionLineItem",
    );

    let success_query = r#"
        mutation AppUsageRecordCreateCapSuccess($id: ID!) {
          appUsageRecordCreate(
            subscriptionLineItemId: $id
            price: { amount: "3.00", currencyCode: USD }
            description: "first"
            idempotencyKey: "usage-key-cap-1"
          ) {
            appUsageRecord {
              id
              description
              price { amount currencyCode }
              subscriptionLineItem { id plan { pricingDetails { __typename ... on AppUsagePricing { balanceUsed { amount currencyCode } } } } }
            }
            userErrors { field message }
          }
        }
    "#;
    let success = proxy.process_request(json_graphql_request(
        success_query,
        json!({ "id": line_item_id }),
    ));
    let first_usage_record_id = synthetic_gid(
        &success.body["data"]["appUsageRecordCreate"]["appUsageRecord"]["id"],
        "AppUsageRecord",
    );
    assert_eq!(
        success.body["data"]["appUsageRecordCreate"],
        json!({
            "appUsageRecord": {
                "id": first_usage_record_id,
                "description": "first",
                "price": { "amount": "3.0", "currencyCode": "USD" },
                "subscriptionLineItem": {
                    "id": line_item_id,
                    "plan": { "pricingDetails": { "__typename": "AppUsagePricing", "balanceUsed": { "amount": "3.0", "currencyCode": "USD" } } }
                }
            },
            "userErrors": []
        })
    );

    let duplicate = proxy.process_request(json_graphql_request(
        success_query,
        json!({ "id": line_item_id }),
    ));
    assert_eq!(duplicate.body, success.body);

    let second = proxy.process_request(json_graphql_request(
        r#"
        mutation AppUsageRecordCreateSecondSuccess($id: ID!) {
          appUsageRecordCreate(
            subscriptionLineItemId: $id
            price: { amount: "1.00", currencyCode: USD }
            description: "second"
            idempotencyKey: "usage-key-cap-2"
          ) {
            appUsageRecord { id description price { amount currencyCode } }
            userErrors { field message }
          }
        }
        "#,
        json!({ "id": line_item_id }),
    ));
    let second_usage_record_id = synthetic_gid(
        &second.body["data"]["appUsageRecordCreate"]["appUsageRecord"]["id"],
        "AppUsageRecord",
    );
    assert_ne!(first_usage_record_id, second_usage_record_id);
    assert_eq!(
        second.body["data"]["appUsageRecordCreate"],
        json!({
            "appUsageRecord": {
                "id": second_usage_record_id,
                "description": "second",
                "price": { "amount": "1.0", "currencyCode": "USD" }
            },
            "userErrors": []
        })
    );

    let over_cap = proxy.process_request(json_graphql_request(
        r#"
        mutation AppUsageRecordCreateCapOverLimit($id: ID!) {
          appUsageRecordCreate(
            subscriptionLineItemId: $id
            price: { amount: "3.00", currencyCode: USD }
            description: "over-cap"
            idempotencyKey: "usage-key-cap-3"
          ) {
            appUsageRecord { id }
            userErrors { field message }
          }
        }
        "#,
        json!({ "id": line_item_id }),
    ));
    assert_eq!(
        over_cap.body["data"]["appUsageRecordCreate"],
        json!({
            "appUsageRecord": null,
            "userErrors": [{ "field": null, "message": "Total price exceeds balance remaining" }]
        })
    );

    let long_key = proxy.process_request(json_graphql_request(
        r#"
        mutation AppUsageRecordCreateLongIdempotencyKey($id: ID!, $key: String) {
          appUsageRecordCreate(
            subscriptionLineItemId: $id
            price: { amount: "1.00", currencyCode: USD }
            description: "too long"
            idempotencyKey: $key
          ) {
            appUsageRecord { id }
            userErrors { field message  }
          }
        }
        "#,
        json!({
            "id": line_item_id,
            "key": "x".repeat(256)
        }),
    ));
    assert_eq!(
        long_key.body["data"]["appUsageRecordCreate"],
        json!({
            "appUsageRecord": null,
            "userErrors": [{ "field": ["idempotencyKey"], "message": "Idempotency key exceeds the maximum length." }]
        })
    );

    let missing_description = proxy.process_request(json_graphql_request(
        r#"
        mutation UsageMissingDescription($id: ID!) {
          appUsageRecordCreate(
            subscriptionLineItemId: $id
            price: { amount: "1.00", currencyCode: USD }
            idempotencyKey: "usage-key-missing-description"
          ) {
            appUsageRecord { id }
            userErrors { field message  }
          }
        }
        "#,
        json!({ "id": line_item_id }),
    ));
    assert_eq!(
        missing_description.body["errors"][0]["extensions"]["code"],
        json!("missingRequiredArguments")
    );
    assert_eq!(
        missing_description.body["errors"][0]["extensions"]["arguments"],
        json!("description")
    );

    let invalid_line_item_id = proxy.process_request(json_graphql_request(
        r#"
        mutation UsageInvalidLineItem {
          appUsageRecordCreate(
            subscriptionLineItemId: "not-a-gid"
            price: { amount: "1.00", currencyCode: USD }
            description: "invalid"
            idempotencyKey: "usage-key-invalid-line-item"
          ) {
            appUsageRecord { id }
            userErrors { field message  }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        invalid_line_item_id.body["errors"][0]["extensions"]["code"],
        json!("argumentLiteralsIncompatible")
    );

    let readback = proxy.process_request(json_graphql_request(
        r#"
        query AppUsageRecordCreateCapRead {
          currentAppInstallation {
            allSubscriptions(first: 5) {
              nodes {
                lineItems {
                  plan { pricingDetails { __typename ... on AppUsagePricing { balanceUsed { amount currencyCode } } } }
                  usageRecords { nodes { id description price { amount currencyCode } } }
                }
              }
            }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        readback.body["data"]["currentAppInstallation"],
        json!({
            "allSubscriptions": { "nodes": [{
                "lineItems": [{
                    "plan": { "pricingDetails": { "__typename": "AppUsagePricing", "balanceUsed": { "amount": "4.0", "currencyCode": "USD" } } },
                    "usageRecords": { "nodes": [
                        {
                            "id": first_usage_record_id,
                            "description": "first",
                            "price": { "amount": "3.0", "currencyCode": "USD" }
                        },
                        {
                            "id": second_usage_record_id,
                            "description": "second",
                            "price": { "amount": "1.0", "currencyCode": "USD" }
                        }
                    ] }
                }]
            }] }
        })
    );

    let nodes = proxy.process_request(json_graphql_request(
        r#"
        query AppUsageRecordNodes($first: ID!, $second: ID!) {
          first: node(id: $first) { ... on AppUsageRecord { id description price { amount currencyCode } } }
          second: node(id: $second) { ... on AppUsageRecord { id description price { amount currencyCode } } }
        }
        "#,
        json!({ "first": first_usage_record_id, "second": second_usage_record_id }),
    ));
    assert_eq!(
        nodes.body["data"],
        json!({
            "first": {
                "id": first_usage_record_id,
                "description": "first",
                "price": { "amount": "3.0", "currencyCode": "USD" }
            },
            "second": {
                "id": second_usage_record_id,
                "description": "second",
                "price": { "amount": "1.0", "currencyCode": "USD" }
            }
        })
    );
}

#[test]
fn app_identity_hydrates_real_installation_for_nodes_scopes_and_uninstall() {
    let app_id = "gid://shopify/App/347082227713";
    let installation_id = "gid://shopify/AppInstallation/913990517042";
    let upstream_calls = Arc::new(Mutex::new(0usize));
    let captured_calls = Arc::clone(&upstream_calls);
    let mut proxy =
        configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport(move |_request| {
            *captured_calls.lock().unwrap() += 1;
            Response {
                status: 200,
                headers: Default::default(),
                body: json!({
                    "data": {
                        "currentAppInstallation": {
                            "id": installation_id,
                            "accessScopes": [
                                { "handle": "read_orders", "description": "Read orders" },
                                { "handle": "write_orders", "description": "Modify orders" }
                            ],
                            "app": {
                                "id": app_id,
                                "handle": "hermes-conformance-products",
                                "title": "hermes-conformance-products",
                                "requestedAccessScopes": [
                                    { "handle": "read_orders", "description": "Read orders" }
                                ]
                            }
                        }
                    }
                }),
            }
        });

    fn real_app_request(query: &str, variables: Value) -> Request {
        let mut request = json_graphql_request(query, variables);
        request.headers.insert(
            "x-shopify-draft-proxy-api-client-id".to_string(),
            "gid://shopify/App/347082227713".to_string(),
        );
        request
    }

    let hydrate = proxy.process_request(real_app_request(
        r#"
        query HydrateCurrentAppInstallation {
          currentAppInstallation {
            id
            accessScopes { handle description }
            app { id handle title requestedAccessScopes { handle } }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        hydrate.body["data"]["currentAppInstallation"]["id"],
        json!(installation_id)
    );
    assert_eq!(*upstream_calls.lock().unwrap(), 1);

    let installation_node = proxy.process_request(real_app_request(
        r#"
        query($id: ID!) {
          node(id: $id) {
            ... on AppInstallation {
              id
              app { id handle }
              accessScopes { handle description }
            }
          }
        }
        "#,
        json!({ "id": installation_id }),
    ));
    assert_eq!(
        installation_node.body["data"]["node"],
        json!({
            "id": installation_id,
            "app": { "id": app_id, "handle": "hermes-conformance-products" },
            "accessScopes": [
                { "handle": "read_orders", "description": "Read orders" },
                { "handle": "write_orders", "description": "Modify orders" }
            ]
        })
    );
    assert_eq!(*upstream_calls.lock().unwrap(), 1);

    let delegate = proxy.process_request(real_app_request(
        r#"
        mutation {
          delegateAccessTokenCreate(input: { delegateAccessScope: ["read_orders"], expiresIn: 300 }) {
            delegateAccessToken { accessScopes }
            userErrors { field message code }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        delegate.body["data"]["delegateAccessTokenCreate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        delegate.body["data"]["delegateAccessTokenCreate"]["delegateAccessToken"]["accessScopes"],
        json!(["read_orders"])
    );

    let revoke = proxy.process_request(real_app_request(
        r#"
        mutation {
          appRevokeAccessScopes(scopes: ["write_orders"]) {
            revoked { handle description }
            userErrors { field message code }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        revoke.body["data"]["appRevokeAccessScopes"],
        json!({
            "revoked": [{ "handle": "write_orders", "description": "Modify orders" }],
            "userErrors": []
        })
    );

    let readback = proxy.process_request(real_app_request(
        r#"
        query {
          currentAppInstallation { id accessScopes { handle } }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        readback.body["data"]["currentAppInstallation"],
        json!({
            "id": installation_id,
            "accessScopes": [{ "handle": "read_orders" }]
        })
    );

    let uninstall = proxy.process_request(real_app_request(
        r#"
        mutation {
          appUninstall {
            app { id handle }
            userErrors { field message code }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        uninstall.body["data"]["appUninstall"],
        json!({
            "app": { "id": app_id, "handle": "hermes-conformance-products" },
            "userErrors": []
        })
    );

    let app_node = proxy.process_request(real_app_request(
        r#"
        query($id: ID!) {
          node(id: $id) {
            ... on App { id handle }
          }
        }
        "#,
        json!({ "id": app_id }),
    ));
    assert_eq!(
        app_node.body["data"]["node"],
        json!({ "id": app_id, "handle": "hermes-conformance-products" })
    );

    let after_uninstall = proxy.process_request(real_app_request(
        r#"query { currentAppInstallation { id } }"#,
        json!({}),
    ));
    assert_eq!(
        after_uninstall.body["data"]["currentAppInstallation"],
        Value::Null
    );
}

#[test]
fn observed_current_app_installation_identity_survives_local_app_mutation_without_headers() {
    let installation_id = "gid://shopify/AppInstallation/913990517042";
    let app_id = "gid://shopify/App/347082227713";
    let upstream_calls = Arc::new(Mutex::new(0usize));
    let captured_calls = Arc::clone(&upstream_calls);
    let mut proxy =
        configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport(move |_request| {
            *captured_calls.lock().unwrap() += 1;
            Response {
                status: 200,
                headers: Default::default(),
                body: json!({
                    "data": {
                        "currentAppInstallation": {
                            "id": installation_id,
                            "app": {
                                "id": app_id,
                                "handle": "hermes-conformance-products",
                                "title": "Hermes Conformance Products"
                            },
                            "accessScopes": [
                                { "handle": "read_products", "description": "Read products" },
                                { "handle": "write_products", "description": "Write products" }
                            ]
                        }
                    }
                }),
            }
        });

    let observed = proxy.process_request(json_graphql_request(
        r#"
        query ObserveRealCurrentAppInstallation {
          currentAppInstallation {
            id
            app { id handle title }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        observed.body["data"]["currentAppInstallation"]["id"],
        json!(installation_id)
    );
    assert_eq!(
        observed.body["data"]["currentAppInstallation"]["app"],
        json!({
            "id": app_id,
            "handle": "hermes-conformance-products",
            "title": "Hermes Conformance Products"
        })
    );
    assert_eq!(*upstream_calls.lock().unwrap(), 1);

    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateLocalAppSubscription($lineItems: [AppSubscriptionLineItemInput!]!) {
          appSubscriptionCreate(
            name: "Observed install plan"
            returnUrl: "https://app.example.test/return"
            test: true
            lineItems: $lineItems
          ) {
            appSubscription { id }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "lineItems": [{
                "plan": {
                    "appUsagePricingDetails": {
                        "cappedAmount": { "amount": 100, "currencyCode": "USD" },
                        "terms": "usage terms"
                    }
                }
            }]
        }),
    ));
    assert_eq!(
        create.body["data"]["appSubscriptionCreate"]["userErrors"],
        json!([])
    );

    let readback = proxy.process_request(json_graphql_request(
        r#"
        query ReadCurrentAppInstallationAfterLocalAppMutation {
          currentAppInstallation {
            id
            app { id handle title }
            allSubscriptions(first: 5) { nodes { id } }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        readback.body["data"]["currentAppInstallation"]["id"],
        json!(installation_id)
    );
    assert_eq!(
        readback.body["data"]["currentAppInstallation"]["app"],
        json!({
            "id": app_id,
            "handle": "hermes-conformance-products",
            "title": "Hermes Conformance Products"
        })
    );
    assert_eq!(
        readback.body["data"]["currentAppInstallation"]["allSubscriptions"]["nodes"]
            .as_array()
            .map(Vec::len),
        Some(1)
    );
    assert_eq!(
        *upstream_calls.lock().unwrap(),
        2,
        "billing overlay reads must refresh the selected Shopify baseline once"
    );
}

#[test]
fn app_billing_overlays_local_create_on_observed_subscription_baseline() {
    let installation_id = "gid://shopify/AppInstallation/913990517042";
    let app_id = "gid://shopify/App/347082227713";
    let observed_subscription_id = "gid://shopify/AppSubscription/7001";
    let upstream_calls = Arc::new(Mutex::new(Vec::<Value>::new()));
    let captured_calls = Arc::clone(&upstream_calls);
    let observed_subscription =
        observed_app_subscription(observed_subscription_id, "ACTIVE", 0, json!([]));
    let upstream_subscription = observed_subscription.clone();
    let mut proxy =
        configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport(move |request| {
            let body: Value = serde_json::from_str(&request.body).unwrap();
            captured_calls.lock().unwrap().push(body);
            Response {
                status: 200,
                headers: Default::default(),
                body: json!({
                    "data": {
                        "currentAppInstallation": {
                            "id": installation_id,
                            "app": { "id": app_id, "handle": "billing-capable-app" },
                            "activeSubscriptions": [upstream_subscription.clone()],
                            "allSubscriptions": {
                                "nodes": [upstream_subscription.clone()],
                                "edges": [{
                                    "cursor": "observed-subscription-cursor",
                                    "node": upstream_subscription.clone()
                                }],
                                "pageInfo": {
                                    "hasNextPage": false,
                                    "hasPreviousPage": false,
                                    "startCursor": "observed-subscription-cursor",
                                    "endCursor": "observed-subscription-cursor"
                                }
                            }
                        }
                    }
                }),
            }
        });

    let baseline = proxy.process_request(json_graphql_request(
        r#"
        query ObserveShopifyBillingBaseline {
          currentAppInstallation {
            id
            app { id handle }
            activeSubscriptions { id name status }
            allSubscriptions(first: 10) { nodes { id name status } }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        baseline.body["data"]["currentAppInstallation"]["allSubscriptions"]["nodes"],
        json!([{ "id": observed_subscription_id, "name": "Observed Shopify plan", "status": "ACTIVE" }])
    );

    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateLocalSubscriptionBesideObservedBaseline(
          $lineItems: [AppSubscriptionLineItemInput!]!
        ) {
          appSubscriptionCreate(
            name: "Local staged plan"
            returnUrl: "https://app.example.test/return"
            test: true
            lineItems: $lineItems
          ) {
            appSubscription { id name status }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "lineItems": [{
                "plan": {
                    "appUsagePricingDetails": {
                        "cappedAmount": { "amount": 100, "currencyCode": "USD" },
                        "terms": "usage terms"
                    }
                }
            }]
        }),
    ));
    assert_eq!(
        create.body["data"]["appSubscriptionCreate"]["userErrors"],
        json!([])
    );
    let local_subscription_id = synthetic_gid(
        &create.body["data"]["appSubscriptionCreate"]["appSubscription"]["id"],
        "AppSubscription",
    );

    let readback = proxy.process_request(json_graphql_request(
        r#"
        query ReadObservedAndLocalBillingOverlay {
          currentAppInstallation {
            activeSubscriptions { id name status }
            allSubscriptions(first: 10) { nodes { id name status } }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        readback.body["data"]["currentAppInstallation"]["allSubscriptions"]["nodes"],
        json!([
            { "id": observed_subscription_id, "name": "Observed Shopify plan", "status": "ACTIVE" },
            { "id": local_subscription_id, "name": "Local staged plan", "status": "ACTIVE" }
        ])
    );
    assert_eq!(
        readback.body["data"]["currentAppInstallation"]["activeSubscriptions"],
        json!([
            { "id": observed_subscription_id, "name": "Observed Shopify plan", "status": "ACTIVE" },
            { "id": local_subscription_id, "name": "Local staged plan", "status": "ACTIVE" }
        ])
    );

    let observed_node = proxy.process_request(json_graphql_request(
        r#"
        query ReadObservedSubscriptionFromEffectiveNode($id: ID!) {
          node(id: $id) {
            ... on AppSubscription { id name status }
          }
        }
        "#,
        json!({ "id": observed_subscription_id }),
    ));
    assert_eq!(
        observed_node.body["data"]["node"],
        json!({
            "id": observed_subscription_id,
            "name": "Observed Shopify plan",
            "status": "ACTIVE"
        })
    );
    assert_eq!(upstream_calls.lock().unwrap().len(), 2);
}

#[test]
fn app_billing_hydrates_cold_subscription_targets_for_trial_and_cancel() {
    let subscription_id = "gid://shopify/AppSubscription/7101";
    let app_id = "gid://shopify/App/347082227713";
    let installation_id = "gid://shopify/AppInstallation/913990517042";

    let trial_calls = Arc::new(Mutex::new(Vec::<Value>::new()));
    let captured_trial_calls = Arc::clone(&trial_calls);
    let trial_subscription = observed_app_subscription(subscription_id, "ACTIVE", 5, json!([]));
    let mut trial_proxy =
        configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport(move |request| {
            let body: Value = serde_json::from_str(&request.body).unwrap();
            let query = body["query"].as_str().unwrap_or_default();
            assert!(query.trim_start().starts_with("query"));
            assert!(!query.contains("mutation"));
            captured_trial_calls.lock().unwrap().push(body);
            Response {
                status: 200,
                headers: Default::default(),
                body: json!({
                    "data": {
                        "currentAppInstallation": {
                            "id": installation_id,
                            "app": { "id": app_id }
                        },
                        "node": trial_subscription.clone()
                    }
                }),
            }
        });

    let trial = trial_proxy.process_request(json_graphql_request(
        r#"
        mutation ExtendColdObservedSubscription($id: ID!) {
          appSubscriptionTrialExtend(id: $id, days: 2) {
            appSubscription { id status trialDays currentPeriodEnd }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "id": subscription_id }),
    ));
    assert_eq!(
        trial.body["data"]["appSubscriptionTrialExtend"]["userErrors"],
        json!([])
    );
    assert_eq!(
        trial.body["data"]["appSubscriptionTrialExtend"]["appSubscription"]["trialDays"],
        json!(7)
    );
    assert_eq!(trial_calls.lock().unwrap().len(), 1);

    let trial_node = trial_proxy.process_request(json_graphql_request(
        r#"
        query ReadExtendedColdSubscription($id: ID!) {
          node(id: $id) {
            ... on AppSubscription { id status trialDays }
          }
        }
        "#,
        json!({ "id": subscription_id }),
    ));
    assert_eq!(
        trial_node.body["data"]["node"],
        json!({ "id": subscription_id, "status": "ACTIVE", "trialDays": 7 })
    );
    assert_eq!(trial_calls.lock().unwrap().len(), 1);

    let cancel_calls = Arc::new(Mutex::new(Vec::<Value>::new()));
    let captured_cancel_calls = Arc::clone(&cancel_calls);
    let cancel_subscription = observed_app_subscription(subscription_id, "ACTIVE", 5, json!([]));
    let mut cancel_proxy =
        configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport(move |request| {
            let body: Value = serde_json::from_str(&request.body).unwrap();
            let query = body["query"].as_str().unwrap_or_default();
            assert!(query.trim_start().starts_with("query"));
            assert!(!query.contains("mutation"));
            captured_cancel_calls.lock().unwrap().push(body);
            Response {
                status: 200,
                headers: Default::default(),
                body: json!({
                    "data": {
                        "currentAppInstallation": {
                            "id": installation_id,
                            "app": { "id": app_id }
                        },
                        "node": cancel_subscription.clone()
                    }
                }),
            }
        });
    let cancel = cancel_proxy.process_request(json_graphql_request(
        r#"
        mutation CancelColdObservedSubscription($id: ID!) {
          appSubscriptionCancel(id: $id, prorate: true) {
            appSubscription { id status }
            userErrors { field message }
          }
        }
        "#,
        json!({ "id": subscription_id }),
    ));
    assert_eq!(
        cancel.body["data"]["appSubscriptionCancel"],
        json!({
            "appSubscription": { "id": subscription_id, "status": "CANCELLED" },
            "userErrors": []
        })
    );
    assert_eq!(cancel_calls.lock().unwrap().len(), 1);
    assert_eq!(
        log_snapshot(&cancel_proxy)["entries"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn app_billing_hydrates_cold_line_item_for_effective_usage_validation() {
    let subscription_id = "gid://shopify/AppSubscription/7201";
    let line_item_id = "gid://shopify/AppSubscriptionLineItem/7202";
    let existing_usage_id = "gid://shopify/AppUsageRecord/7203";
    let app_id = "gid://shopify/App/347082227713";
    let installation_id = "gid://shopify/AppInstallation/913990517042";
    let usage_line_item = json!({
        "id": line_item_id,
        "plan": {
            "pricingDetails": {
                "__typename": "AppUsagePricing",
                "cappedAmount": { "amount": "10.0", "currencyCode": "USD" },
                "balanceUsed": { "amount": "6.0", "currencyCode": "USD" },
                "interval": "EVERY_30_DAYS",
                "terms": "Observed usage terms"
            }
        },
        "usageRecords": {
            "nodes": [{
                "__typename": "AppUsageRecord",
                "id": existing_usage_id,
                "createdAt": "2026-04-20T00:00:00.000Z",
                "description": "existing usage",
                "idempotencyKey": "existing-key",
                "price": { "amount": "6.0", "currencyCode": "USD" },
                "subscriptionLineItem": { "id": line_item_id }
            }],
            "pageInfo": {
                "hasNextPage": false,
                "hasPreviousPage": false,
                "startCursor": "usage-cursor",
                "endCursor": "usage-cursor"
            }
        }
    });
    let subscription =
        observed_app_subscription(subscription_id, "ACTIVE", 0, json!([usage_line_item]));
    let upstream_calls = Arc::new(Mutex::new(Vec::<Value>::new()));
    let captured_calls = Arc::clone(&upstream_calls);
    let mut proxy =
        configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport(move |request| {
            let body: Value = serde_json::from_str(&request.body).unwrap();
            let query = body["query"].as_str().unwrap_or_default();
            assert!(query.trim_start().starts_with("query"));
            assert!(!query.contains("mutation"));
            captured_calls.lock().unwrap().push(body);
            Response {
                status: 200,
                headers: Default::default(),
                body: json!({
                    "data": {
                        "currentAppInstallation": {
                            "id": installation_id,
                            "app": { "id": app_id },
                            "activeSubscriptions": [subscription.clone()]
                        }
                    }
                }),
            }
        });

    let usage_mutation = r#"
      mutation CreateUsageAgainstColdLineItem(
        $lineItemId: ID!
        $amount: Decimal!
        $description: String!
        $idempotencyKey: String!
      ) {
        appUsageRecordCreate(
          subscriptionLineItemId: $lineItemId
          price: { amount: $amount, currencyCode: USD }
          description: $description
          idempotencyKey: $idempotencyKey
        ) {
          appUsageRecord { id description idempotencyKey price { amount currencyCode } }
          userErrors { field message }
        }
      }
    "#;

    let duplicate = proxy.process_request(json_graphql_request(
        usage_mutation,
        json!({
            "lineItemId": line_item_id,
            "amount": "6.0",
            "description": "duplicate should reuse observed record",
            "idempotencyKey": "existing-key"
        }),
    ));
    assert_eq!(
        duplicate.body["data"]["appUsageRecordCreate"],
        json!({
            "appUsageRecord": {
                "id": existing_usage_id,
                "description": "existing usage",
                "idempotencyKey": "existing-key",
                "price": { "amount": "6.0", "currencyCode": "USD" }
            },
            "userErrors": []
        })
    );
    assert_eq!(upstream_calls.lock().unwrap().len(), 1);
    assert_eq!(log_snapshot(&proxy)["entries"], json!([]));

    let success = proxy.process_request(json_graphql_request(
        usage_mutation,
        json!({
            "lineItemId": line_item_id,
            "amount": "4.0",
            "description": "fills remaining balance",
            "idempotencyKey": "new-key"
        }),
    ));
    assert_eq!(
        success.body["data"]["appUsageRecordCreate"]["userErrors"],
        json!([])
    );
    let new_usage_id = synthetic_gid(
        &success.body["data"]["appUsageRecordCreate"]["appUsageRecord"]["id"],
        "AppUsageRecord",
    );

    let over_cap = proxy.process_request(json_graphql_request(
        usage_mutation,
        json!({
            "lineItemId": line_item_id,
            "amount": "1.0",
            "description": "over cap",
            "idempotencyKey": "over-cap-key"
        }),
    ));
    assert_eq!(
        over_cap.body["data"]["appUsageRecordCreate"],
        json!({
            "appUsageRecord": null,
            "userErrors": [{ "field": null, "message": "Total price exceeds balance remaining" }]
        })
    );

    let effective_node = proxy.process_request(json_graphql_request(
        r#"
        query ReadEffectiveSubscriptionAfterColdUsage($id: ID!) {
          node(id: $id) {
            ... on AppSubscription {
              id
              lineItems {
                id
                plan {
                  pricingDetails {
                    __typename
                    ... on AppUsagePricing { cappedAmount { amount currencyCode } balanceUsed { amount currencyCode } }
                  }
                }
                usageRecords(first: 10) { nodes { id idempotencyKey } }
              }
            }
          }
        }
        "#,
        json!({ "id": subscription_id }),
    ));
    assert_eq!(
        effective_node.body["data"]["node"]["lineItems"][0],
        json!({
            "id": line_item_id,
            "plan": {
                "pricingDetails": {
                    "__typename": "AppUsagePricing",
                    "cappedAmount": { "amount": "10.0", "currencyCode": "USD" },
                    "balanceUsed": { "amount": "10.0", "currencyCode": "USD" }
                }
            },
            "usageRecords": {
                "nodes": [
                    { "id": existing_usage_id, "idempotencyKey": "existing-key" },
                    { "id": new_usage_id, "idempotencyKey": "new-key" }
                ]
            }
        })
    );
    assert_eq!(upstream_calls.lock().unwrap().len(), 1);
    assert_eq!(log_snapshot(&proxy)["entries"].as_array().unwrap().len(), 1);
}

#[test]
fn app_billing_hydrates_cold_line_item_for_capped_amount_validation() {
    let subscription_id = "gid://shopify/AppSubscription/7301";
    let line_item_id = "gid://shopify/AppSubscriptionLineItem/7302";
    let app_id = "gid://shopify/App/347082227713";
    let installation_id = "gid://shopify/AppInstallation/913990517042";
    let line_item = json!({
        "id": line_item_id,
        "plan": {
            "pricingDetails": {
                "__typename": "AppUsagePricing",
                "cappedAmount": { "amount": "10.0", "currencyCode": "USD" },
                "balanceUsed": { "amount": "2.0", "currencyCode": "USD" },
                "interval": "EVERY_30_DAYS",
                "terms": "Observed usage terms"
            }
        },
        "usageRecords": {
            "nodes": [],
            "pageInfo": {
                "hasNextPage": false,
                "hasPreviousPage": false,
                "startCursor": null,
                "endCursor": null
            }
        }
    });
    let subscription = observed_app_subscription(subscription_id, "ACTIVE", 0, json!([line_item]));
    let upstream_calls = Arc::new(Mutex::new(Vec::<Value>::new()));
    let captured_calls = Arc::clone(&upstream_calls);
    let mut proxy =
        configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport(move |request| {
            let body: Value = serde_json::from_str(&request.body).unwrap();
            let query = body["query"].as_str().unwrap_or_default();
            assert!(query.trim_start().starts_with("query"));
            assert!(!query.contains("mutation"));
            captured_calls.lock().unwrap().push(body);
            Response {
                status: 200,
                headers: Default::default(),
                body: json!({
                    "data": {
                        "currentAppInstallation": {
                            "id": installation_id,
                            "app": { "id": app_id },
                            "activeSubscriptions": [subscription.clone()]
                        }
                    }
                }),
            }
        });

    let update = proxy.process_request(json_graphql_request(
        r#"
        mutation UpdateColdObservedUsageCap($id: ID!) {
          appSubscriptionLineItemUpdate(
            id: $id
            cappedAmount: { amount: 20, currencyCode: USD }
          ) {
            confirmationUrl
            appSubscription {
              id
              lineItems {
                id
                plan {
                  pricingDetails {
                    __typename
                    ... on AppUsagePricing { cappedAmount { amount currencyCode } }
                  }
                }
              }
            }
            userErrors { field message }
          }
        }
        "#,
        json!({ "id": line_item_id }),
    ));
    assert_eq!(
        update.body["data"]["appSubscriptionLineItemUpdate"],
        json!({
            "confirmationUrl": DERIVED_DEFAULT_CONFIRMATION_URL,
            "appSubscription": {
                "id": subscription_id,
                "lineItems": [{
                    "id": line_item_id,
                    "plan": {
                        "pricingDetails": {
                            "__typename": "AppUsagePricing",
                            "cappedAmount": { "amount": "10.0", "currencyCode": "USD" }
                        }
                    }
                }]
            },
            "userErrors": []
        })
    );
    assert_eq!(upstream_calls.lock().unwrap().len(), 1);
    assert_eq!(log_snapshot(&proxy)["entries"].as_array().unwrap().len(), 1);

    let invalid_currency = proxy.process_request(json_graphql_request(
        r#"
        mutation RejectColdObservedUsageCapCurrency($id: ID!) {
          appSubscriptionLineItemUpdate(
            id: $id
            cappedAmount: { amount: 20, currencyCode: EUR }
          ) {
            appSubscription { id }
            userErrors { field message }
          }
        }
        "#,
        json!({ "id": line_item_id }),
    ));
    assert_eq!(
        invalid_currency.body["data"]["appSubscriptionLineItemUpdate"],
        json!({
            "appSubscription": null,
            "userErrors": [{ "field": null, "message": "Currency code must be USD" }]
        })
    );
    assert_eq!(upstream_calls.lock().unwrap().len(), 1);
}

#[test]
fn app_billing_effective_state_enforces_app_ownership() {
    fn app_request(app_id: &str, query: &str, variables: Value) -> Request {
        let mut request = json_graphql_request(query, variables);
        request.headers.insert(
            "x-shopify-draft-proxy-api-client-id".to_string(),
            app_id.to_string(),
        );
        request
    }

    let owner_app_id = "gid://shopify/App/8101";
    let other_app_id = "gid://shopify/App/8102";
    let mut proxy = snapshot_proxy();
    let create = proxy.process_request(app_request(
        owner_app_id,
        r#"
        mutation CreateOwnedSubscription($lineItems: [AppSubscriptionLineItemInput!]!) {
          appSubscriptionCreate(
            name: "Owned plan"
            returnUrl: "https://app.example.test/return"
            test: true
            lineItems: $lineItems
          ) {
            appSubscription { id lineItems { id } }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "lineItems": [{
                "plan": {
                    "appUsagePricingDetails": {
                        "cappedAmount": { "amount": 10, "currencyCode": "USD" },
                        "terms": "usage terms"
                    }
                }
            }]
        }),
    ));
    let subscription_id = synthetic_gid(
        &create.body["data"]["appSubscriptionCreate"]["appSubscription"]["id"],
        "AppSubscription",
    );
    let line_item_id = synthetic_gid(
        &create.body["data"]["appSubscriptionCreate"]["appSubscription"]["lineItems"][0]["id"],
        "AppSubscriptionLineItem",
    );

    let cross_app_cancel = proxy.process_request(app_request(
        other_app_id,
        r#"
        mutation CrossAppCancel($id: ID!) {
          appSubscriptionCancel(id: $id) {
            appSubscription { id }
            userErrors { field message }
          }
        }
        "#,
        json!({ "id": subscription_id }),
    ));
    assert_eq!(
        cross_app_cancel.body["data"]["appSubscriptionCancel"],
        json!({
            "appSubscription": null,
            "userErrors": [{
                "field": ["id"],
                "message": "Couldn't find RecurringApplicationCharge"
            }]
        })
    );

    let cross_app_usage = proxy.process_request(app_request(
        other_app_id,
        r#"
        mutation CrossAppUsage($id: ID!) {
          appUsageRecordCreate(
            subscriptionLineItemId: $id
            price: { amount: 1, currencyCode: USD }
            description: "not owned"
            idempotencyKey: "cross-app"
          ) {
            appUsageRecord { id }
            userErrors { field message }
          }
        }
        "#,
        json!({ "id": line_item_id }),
    ));
    assert_eq!(
        cross_app_usage.body["data"]["appUsageRecordCreate"],
        json!({
            "appUsageRecord": null,
            "userErrors": [{ "field": ["subscriptionLineItemId"], "message": "Invalid id" }]
        })
    );

    let cross_app_node = proxy.process_request(app_request(
        other_app_id,
        r#"
        query CrossAppSubscriptionNode($id: ID!) {
          node(id: $id) { ... on AppSubscription { id status } }
        }
        "#,
        json!({ "id": subscription_id }),
    ));
    assert_eq!(cross_app_node.body["data"]["node"], Value::Null);

    let owner_node = proxy.process_request(app_request(
        owner_app_id,
        r#"
        query OwnerSubscriptionNode($id: ID!) {
          node(id: $id) { ... on AppSubscription { id status } }
        }
        "#,
        json!({ "id": subscription_id }),
    ));
    assert_eq!(
        owner_node.body["data"]["node"],
        json!({ "id": subscription_id, "status": "ACTIVE" })
    );
    assert_eq!(log_snapshot(&proxy)["entries"].as_array().unwrap().len(), 1);
}

#[test]
fn app_billing_dump_restore_preserves_observed_baseline_and_local_overlay() {
    let installation_id = "gid://shopify/AppInstallation/913990517042";
    let app_id = "gid://shopify/App/347082227713";
    let observed_subscription_id = "gid://shopify/AppSubscription/7401";
    let observed_subscription =
        observed_app_subscription(observed_subscription_id, "ACTIVE", 0, json!([]));
    let mut proxy =
        configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport(move |_request| {
            Response {
                status: 200,
                headers: Default::default(),
                body: json!({
                    "data": {
                        "currentAppInstallation": {
                            "id": installation_id,
                            "app": { "id": app_id },
                            "activeSubscriptions": [observed_subscription.clone()],
                            "allSubscriptions": { "nodes": [observed_subscription.clone()] }
                        }
                    }
                }),
            }
        });
    let baseline = proxy.process_request(json_graphql_request(
        r#"
        query ObserveBillingBeforeDump {
          currentAppInstallation {
            allSubscriptions(first: 10) { nodes { id } }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        baseline.body["data"]["currentAppInstallation"]["allSubscriptions"]["nodes"],
        json!([{ "id": observed_subscription_id }])
    );

    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateBillingOverlayBeforeDump($lineItems: [AppSubscriptionLineItemInput!]!) {
          appSubscriptionCreate(
            name: "Persisted local plan"
            returnUrl: "https://app.example.test/return"
            test: true
            lineItems: $lineItems
          ) {
            appSubscription { id }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "lineItems": [{
                "plan": {
                    "appUsagePricingDetails": {
                        "cappedAmount": { "amount": 10, "currencyCode": "USD" },
                        "terms": "usage terms"
                    }
                }
            }]
        }),
    ));
    let local_subscription_id = synthetic_gid(
        &create.body["data"]["appSubscriptionCreate"]["appSubscription"]["id"],
        "AppSubscription",
    );

    let dump = proxy.process_request(request_with_body("POST", "/__meta/dump", "{}"));
    assert_eq!(dump.status, 200);
    assert_eq!(
        dump.body["state"]["baseState"]["appSubscriptionOrder"],
        json!([observed_subscription_id])
    );
    assert_eq!(
        dump.body["state"]["stagedState"]["appSubscriptionOrder"],
        json!([local_subscription_id])
    );

    let mut restored = snapshot_proxy();
    let restore = restored.process_request(request_with_body(
        "POST",
        "/__meta/restore",
        &dump.body.to_string(),
    ));
    assert_eq!(restore.status, 200);
    let readback = restored.process_request(json_graphql_request(
        r#"
        query ReadRestoredBillingOverlay {
          currentAppInstallation {
            allSubscriptions(first: 10) { nodes { id name status } }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        readback.body["data"]["currentAppInstallation"]["allSubscriptions"]["nodes"],
        json!([
            { "id": observed_subscription_id, "name": "Observed Shopify plan", "status": "ACTIVE" },
            { "id": local_subscription_id, "name": "Persisted local plan", "status": "ACTIVE" }
        ])
    );

    let mut tombstone_dump = dump.body;
    tombstone_dump["state"]["stagedState"]["deletedAppSubscriptionIds"] =
        json!([observed_subscription_id]);
    let mut tombstoned = snapshot_proxy();
    let restore = tombstoned.process_request(request_with_body(
        "POST",
        "/__meta/restore",
        &tombstone_dump.to_string(),
    ));
    assert_eq!(restore.status, 200);
    let tombstone_readback = tombstoned.process_request(json_graphql_request(
        r#"
        query ReadRestoredBillingTombstone($id: ID!) {
          currentAppInstallation {
            activeSubscriptions { id }
            allSubscriptions(first: 10) { nodes { id } }
          }
          node(id: $id) { ... on AppSubscription { id } }
        }
        "#,
        json!({ "id": observed_subscription_id }),
    ));
    assert_eq!(
        tombstone_readback.body["data"]["currentAppInstallation"],
        json!({
            "activeSubscriptions": [{ "id": local_subscription_id }],
            "allSubscriptions": { "nodes": [{ "id": local_subscription_id }] }
        })
    );
    assert_eq!(tombstone_readback.body["data"]["node"], Value::Null);
}

#[test]
fn app_billing_commit_replays_original_mutations_in_order() {
    let replayed = Arc::new(Mutex::new(Vec::<Request>::new()));
    let captured_replays = Arc::clone(&replayed);
    let mut proxy = snapshot_proxy().with_commit_transport(move |request| {
        captured_replays.lock().unwrap().push(request);
        Response {
            status: 200,
            headers: Default::default(),
            body: json!({ "data": { "appSubscriptionCreate": { "appSubscription": null } } }),
        }
    });
    let first_query = r#"
      mutation CreateFirstBillingCommit($lineItems: [AppSubscriptionLineItemInput!]!) {
        appSubscriptionCreate(
          name: "First billing commit"
          returnUrl: "https://app.example.test/return"
          test: true
          lineItems: $lineItems
        ) {
          appSubscription { id }
          userErrors { field message }
        }
      }
    "#;
    let second_query = r#"
      mutation CreateSecondBillingCommit($lineItems: [AppSubscriptionLineItemInput!]!) {
        appSubscriptionCreate(
          name: "Second billing commit"
          returnUrl: "https://app.example.test/return"
          test: true
          lineItems: $lineItems
        ) {
          appSubscription { id }
          userErrors { field message }
        }
      }
    "#;
    let variables = json!({
        "lineItems": [{
            "plan": {
                "appUsagePricingDetails": {
                    "cappedAmount": { "amount": 10, "currencyCode": "USD" },
                    "terms": "usage terms"
                }
            }
        }]
    });
    assert_eq!(
        proxy
            .process_request(json_graphql_request(first_query, variables.clone()))
            .body["data"]["appSubscriptionCreate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        proxy
            .process_request(json_graphql_request(second_query, variables.clone()))
            .body["data"]["appSubscriptionCreate"]["userErrors"],
        json!([])
    );
    assert!(replayed.lock().unwrap().is_empty());

    let commit = proxy.process_request(request_with_body("POST", "/__meta/commit", ""));
    assert_eq!(commit.status, 200);
    assert_eq!(commit.body["committed"], json!(2));
    let replayed = replayed.lock().unwrap();
    assert_eq!(replayed.len(), 2);
    assert_eq!(
        serde_json::from_str::<Value>(&replayed[0].body).unwrap(),
        json!({ "query": first_query, "variables": variables.clone() })
    );
    assert_eq!(
        serde_json::from_str::<Value>(&replayed[1].body).unwrap(),
        json!({ "query": second_query, "variables": variables })
    );
}

#[test]
fn app_billing_access_local_lifecycle_reads_nodes_and_uninstall_cascade() {
    let mut proxy = snapshot_proxy();

    let create_subscription = proxy.process_request(json_graphql_request(
        r#"
        mutation AppSubscriptionCreateLocalLifecycle($lineItems: [AppSubscriptionLineItemInput!]!) {
          appSubscriptionCreate(name: "Local plan", returnUrl: "https://app.example.test/return", trialDays: 7, test: true, lineItems: $lineItems) {
            appSubscription { id status trialDays lineItems { id } }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "lineItems": [{
                "plan": { "appUsagePricingDetails": { "cappedAmount": { "amount": 100, "currencyCode": "USD" }, "terms": "usage terms" } }
            }]
        }),
    ));
    let subscription_id = synthetic_gid(
        &create_subscription.body["data"]["appSubscriptionCreate"]["appSubscription"]["id"],
        "AppSubscription",
    );
    let line_item_id = synthetic_gid(
        &create_subscription.body["data"]["appSubscriptionCreate"]["appSubscription"]["lineItems"]
            [0]["id"],
        "AppSubscriptionLineItem",
    );

    let one_time = proxy.process_request(json_graphql_request(
        r#"
        mutation AppPurchaseOneTimeCreateLocalLifecycle {
          appPurchaseOneTimeCreate(name: "Import package", returnUrl: "https://app.example.test/return", price: { amount: 10, currencyCode: USD }, test: true) {
            confirmationUrl
            appPurchaseOneTime { id name status test price { amount currencyCode } }
            userErrors { field message }
          }
        }
        "#,
        json!({}),
    ));
    let one_time_id = synthetic_gid(
        &one_time.body["data"]["appPurchaseOneTimeCreate"]["appPurchaseOneTime"]["id"],
        "AppPurchaseOneTime",
    );
    assert_eq!(
        one_time.body["data"]["appPurchaseOneTimeCreate"],
        json!({
            "confirmationUrl": DERIVED_RETURN_CONFIRMATION_URL,
            "appPurchaseOneTime": {
                "id": one_time_id,
                "name": "Import package",
                "status": "ACTIVE",
                "test": true,
                "price": { "amount": "10.0", "currencyCode": "USD" }
            },
            "userErrors": []
        })
    );

    let one_time_test_false = proxy.process_request(json_graphql_request(
        r#"
        mutation OneTimeTestFalse {
          appPurchaseOneTimeCreate(name: "Import package 2", returnUrl: "https://app.example.test/return", price: { amount: 10, currencyCode: USD }, test: false) {
            appPurchaseOneTime { id test }
            userErrors { field message }
          }
        }
        "#,
        json!({}),
    ));
    let second_one_time_id = synthetic_gid(
        &one_time_test_false.body["data"]["appPurchaseOneTimeCreate"]["appPurchaseOneTime"]["id"],
        "AppPurchaseOneTime",
    );
    assert_ne!(one_time_id, second_one_time_id);
    assert_eq!(
        one_time_test_false.body["data"]["appPurchaseOneTimeCreate"],
        json!({
            "appPurchaseOneTime": {
                "id": second_one_time_id,
                "test": false
            },
            "userErrors": []
        })
    );

    let usage = proxy.process_request(json_graphql_request(
        r#"
        mutation AppUsageRecordCreateLocalLifecycle($id: ID!) {
          appUsageRecordCreate(subscriptionLineItemId: $id, price: { amount: "12.5", currencyCode: USD }, description: "metered import", idempotencyKey: "usage-local-1") {
            appUsageRecord { id description price { amount currencyCode } subscriptionLineItem { id } }
            userErrors { field message }
          }
        }
        "#,
        json!({ "id": line_item_id }),
    ));
    let usage_record_id = synthetic_gid(
        &usage.body["data"]["appUsageRecordCreate"]["appUsageRecord"]["id"],
        "AppUsageRecord",
    );
    assert_eq!(
        usage.body["data"]["appUsageRecordCreate"]["appUsageRecord"],
        json!({
            "id": usage_record_id,
            "description": "metered import",
            "price": { "amount": "12.5", "currencyCode": "USD" },
            "subscriptionLineItem": { "id": line_item_id }
        })
    );

    let extended_trial = proxy.process_request(json_graphql_request(
        r#"
        mutation AppSubscriptionTrialExtendLocalLifecycle($id: ID!) {
          appSubscriptionTrialExtend(id: $id, days: 3) {
            appSubscription { id trialDays }
            userErrors { field message }
          }
        }
        "#,
        json!({ "id": subscription_id }),
    ));
    assert_eq!(
        extended_trial.body["data"]["appSubscriptionTrialExtend"],
        json!({
            "appSubscription": { "id": subscription_id, "trialDays": 10 },
            "userErrors": []
        })
    );

    proxy.process_request(json_graphql_request(
        r#"
        mutation AppSubscriptionCancelLocalLifecycle($id: ID!) {
          appSubscriptionCancel(id: $id, prorate: true) { appSubscription { id status trialDays } userErrors { field message } }
        }
        "#,
        json!({ "id": subscription_id }),
    ));

    let readback = proxy.process_request(json_graphql_request(
        r#"
        query AppBillingLocalRead {
          currentAppInstallation {
            id
            activeSubscriptions { id }
            allSubscriptions(first: 5) { nodes { id status trialDays lineItems { id usageRecords(first: 5) { nodes { description price { amount currencyCode } } } } } }
            oneTimePurchases(first: 5) { nodes { name status price { amount currencyCode } } }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        readback.body["data"]["currentAppInstallation"],
        json!({
            "id": "gid://shopify/AppInstallation/local",
            "activeSubscriptions": [],
            "allSubscriptions": { "nodes": [{
                "id": subscription_id,
                "status": "CANCELLED",
                "trialDays": 10,
                "lineItems": [{
                    "id": line_item_id,
                    "usageRecords": { "nodes": [{
                        "description": "metered import",
                        "price": { "amount": "12.5", "currencyCode": "USD" }
                    }] }
                }]
            }] },
            "oneTimePurchases": { "nodes": [
                {
                    "name": "Import package",
                    "status": "ACTIVE",
                    "price": { "amount": "10.0", "currencyCode": "USD" }
                },
                {
                    "name": "Import package 2",
                    "status": "ACTIVE",
                    "price": { "amount": "10.0", "currencyCode": "USD" }
                }
            ] }
        })
    );

    let node_read = proxy.process_request(json_graphql_request(
        r#"
        query AppBillingNodeRead($id: ID!) {
          node(id: $id) {
            ... on AppPurchaseOneTime { id name status test price { amount currencyCode } }
          }
        }
        "#,
        json!({ "id": one_time_id }),
    ));
    assert_eq!(
        node_read.body["data"]["node"],
        json!({
            "id": one_time_id,
            "name": "Import package",
            "status": "ACTIVE",
            "test": true,
            "price": { "amount": "10.0", "currencyCode": "USD" }
        })
    );

    let uninstall = proxy.process_request(json_graphql_request(
        r#"
        mutation AppUninstallLocalLifecycle { appUninstall { app { id handle } userErrors { field message } } }
        "#,
        json!({}),
    ));
    assert_eq!(
        uninstall.body["data"]["appUninstall"],
        json!({
            "app": { "id": "gid://shopify/App/local", "handle": "shopify-draft-proxy" },
            "userErrors": []
        })
    );

    let after_uninstall = proxy.process_request(json_graphql_request(
        r#"query AppInstallationIdLocalRead { currentAppInstallation { id } }"#,
        json!({}),
    ));
    assert_eq!(
        after_uninstall.body["data"]["currentAppInstallation"],
        Value::Null
    );
}

#[test]
fn app_uninstall_user_error_messages_match_core_i18n_strings() {
    let mut proxy = snapshot_proxy();

    let uninstall = proxy.process_request(json_graphql_request(
        r#"
        mutation AppUninstallCurrent {
          appUninstall {
            app { id }
            userErrors { field message code }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        uninstall.body["data"]["appUninstall"]["userErrors"],
        json!([])
    );
    let already_uninstalled = proxy.process_request(json_graphql_request(
        r#"
        mutation AppUninstallAlreadyUninstalled {
          appUninstall {
            app { id }
            userErrors { field message code }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        already_uninstalled.body["data"]["appUninstall"],
        json!({
            "app": null,
            "userErrors": [{
                "field": ["id"],
                "message": "App is not installed on shop",
                "code": "APP_NOT_INSTALLED"
            }]
        })
    );
}

#[test]
fn app_subscription_line_item_update_validates_recurring_currency_and_amount() {
    let mut proxy = snapshot_proxy();

    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation AppSubscriptionCreateLocalLifecycle($lineItems: [AppSubscriptionLineItemInput!]!) {
          appSubscriptionCreate(
            name: "Local plan"
            returnUrl: "https://app.example.test/return"
            trialDays: 7
            test: true
            lineItems: $lineItems
          ) {
            confirmationUrl
            appSubscription {
              id
              lineItems {
                id
                plan {
                  pricingDetails {
                    __typename
                    ... on AppUsagePricing { cappedAmount { amount currencyCode } }
                    ... on AppRecurringPricing { price { amount currencyCode } }
                  }
                }
              }
            }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "lineItems": [
                { "plan": { "appUsagePricingDetails": { "cappedAmount": { "amount": 5, "currencyCode": "USD" }, "terms": "usage terms" } } },
                { "plan": { "appRecurringPricingDetails": { "price": { "amount": 1, "currencyCode": "USD" }, "interval": "EVERY_30_DAYS" } } }
            ]
        }),
    ));
    let subscription_id = synthetic_gid(
        &create.body["data"]["appSubscriptionCreate"]["appSubscription"]["id"],
        "AppSubscription",
    );
    let usage_line_item_id = synthetic_gid(
        &create.body["data"]["appSubscriptionCreate"]["appSubscription"]["lineItems"][0]["id"],
        "AppSubscriptionLineItem",
    );
    let recurring_line_item_id = synthetic_gid(
        &create.body["data"]["appSubscriptionCreate"]["appSubscription"]["lineItems"][1]["id"],
        "AppSubscriptionLineItem",
    );
    assert_ne!(usage_line_item_id, recurring_line_item_id);
    assert_eq!(
        create.body["data"]["appSubscriptionCreate"],
        json!({
            "confirmationUrl": DERIVED_RETURN_CONFIRMATION_URL,
            "appSubscription": {
                "id": subscription_id,
                "lineItems": [
                    {
                        "id": usage_line_item_id,
                        "plan": { "pricingDetails": {
                            "__typename": "AppUsagePricing",
                            "cappedAmount": { "amount": "5.0", "currencyCode": "USD" }
                        }}
                    },
                    {
                        "id": recurring_line_item_id,
                        "plan": { "pricingDetails": {
                            "__typename": "AppRecurringPricing",
                            "price": { "amount": "1.0", "currencyCode": "USD" }
                        }}
                    }
                ]
            },
            "userErrors": []
        })
    );

    let update = proxy.process_request(json_graphql_request(
        r#"
        mutation AppSubscriptionLineItemUpdateValidation($usageLineItemId: ID!, $recurringLineItemId: ID!) {
          recurring: appSubscriptionLineItemUpdate(id: $recurringLineItemId, cappedAmount: { amount: 10, currencyCode: USD }) {
            appSubscription { id }
            userErrors { field message }
          }
          currencyMismatch: appSubscriptionLineItemUpdate(id: $usageLineItemId, cappedAmount: { amount: 10, currencyCode: EUR }) {
            appSubscription { id }
            userErrors { field message }
          }
          nonIncreasing: appSubscriptionLineItemUpdate(id: $usageLineItemId, cappedAmount: { amount: 3, currencyCode: USD }) {
            appSubscription { id }
            userErrors { field message }
          }
          success: appSubscriptionLineItemUpdate(id: $usageLineItemId, cappedAmount: { amount: 10, currencyCode: USD }) {
            confirmationUrl
            appSubscription {
              id
              lineItems {
                id
                plan {
                  pricingDetails {
                    __typename
                    ... on AppUsagePricing { cappedAmount { amount currencyCode } }
                    ... on AppRecurringPricing { price { amount currencyCode } }
                  }
                }
              }
            }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "usageLineItemId": usage_line_item_id,
            "recurringLineItemId": recurring_line_item_id
        }),
    ));

    assert_eq!(
        update.body["data"],
        json!({
            "recurring": {
                "appSubscription": null,
                "userErrors": [{ "field": null, "message": "Only variable subscriptions can be updated." }]
            },
            "currencyMismatch": {
                "appSubscription": null,
                "userErrors": [{ "field": null, "message": "Currency code must be USD" }]
            },
            "nonIncreasing": {
                "appSubscription": null,
                "userErrors": [{ "field": ["cappedAmount"], "message": "Spending limit can only be increased. Please contact the app developer to decrease spending limit." }]
            },
            "success": {
                "confirmationUrl": DERIVED_DEFAULT_CONFIRMATION_URL,
                "appSubscription": {
                    "id": subscription_id,
                    "lineItems": [
                        {
                            "id": usage_line_item_id,
                            "plan": { "pricingDetails": {
                                "__typename": "AppUsagePricing",
                                "cappedAmount": { "amount": "5.0", "currencyCode": "USD" }
                            }}
                        },
                        {
                            "id": recurring_line_item_id,
                            "plan": { "pricingDetails": {
                                "__typename": "AppRecurringPricing",
                                "price": { "amount": "1.0", "currencyCode": "USD" }
                            }}
                        }
                    ]
                },
                "userErrors": []
            }
        })
    );

    let synchronous_update = proxy.process_request(json_graphql_request(
        r#"
        mutation AppSubscriptionLineItemUpdateNoApproval($usageLineItemId: ID!) {
          appSubscriptionLineItemUpdate(
            id: $usageLineItemId
            cappedAmount: { amount: 12, currencyCode: USD }
            requireApproval: false
          ) {
            confirmationUrl
            appSubscription {
              lineItems {
                id
                plan {
                  pricingDetails {
                    __typename
                    ... on AppUsagePricing { cappedAmount { amount currencyCode } }
                  }
                }
              }
            }
            userErrors { field message }
          }
        }
        "#,
        json!({ "usageLineItemId": usage_line_item_id }),
    ));
    assert_eq!(
        synchronous_update.body["errors"][0]["extensions"],
        json!({
            "code": "argumentNotAccepted",
            "name": "appSubscriptionLineItemUpdate",
            "typeName": "Field",
            "argumentName": "requireApproval"
        })
    );
}

#[test]
fn app_subscription_trial_extend_validates_days_unknown_and_inactive_status() {
    let mut proxy = snapshot_proxy();

    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation AppSubscriptionCreatePendingLocalLifecycle($lineItems: [AppSubscriptionLineItemInput!]!) {
          appSubscriptionCreate(
            name: "Local plan"
            returnUrl: "https://app.example.test/return"
            trialDays: 7
            test: false
            lineItems: $lineItems
          ) {
            appSubscription { id status trialDays }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "lineItems": [{
                "plan": {
                    "appUsagePricingDetails": {
                        "cappedAmount": { "amount": 100, "currencyCode": "USD" },
                        "terms": "usage terms"
                    }
                }
            }]
        }),
    ));
    let subscription_id = synthetic_gid(
        &create.body["data"]["appSubscriptionCreate"]["appSubscription"]["id"],
        "AppSubscription",
    );
    assert_eq!(
        create.body["data"]["appSubscriptionCreate"],
        json!({
            "appSubscription": {
                "id": subscription_id,
                "status": "PENDING",
                "trialDays": 7
            },
            "userErrors": []
        })
    );

    let trial_extend_query = r#"
        mutation AppSubscriptionTrialExtendValidation($id: ID!, $days: Int!) {
          appSubscriptionTrialExtend(id: $id, days: $days) {
            appSubscription { id trialDays }
            userErrors { field message code }
          }
        }
    "#;

    let days_zero = proxy.process_request(json_graphql_request(
        trial_extend_query,
        json!({ "id": subscription_id, "days": 0 }),
    ));
    assert_eq!(
        days_zero.body["data"]["appSubscriptionTrialExtend"],
        json!({
            "appSubscription": null,
            "userErrors": [{ "field": ["days"], "message": "Days must be greater than 0", "code": null }]
        })
    );

    let days_too_large = proxy.process_request(json_graphql_request(
        trial_extend_query,
        json!({ "id": subscription_id, "days": 1001 }),
    ));
    assert_eq!(
        days_too_large.body["data"]["appSubscriptionTrialExtend"],
        json!({
            "appSubscription": null,
            "userErrors": [{ "field": ["days"], "message": "Days must be less than or equal to 1000", "code": null }]
        })
    );

    let unknown = proxy.process_request(json_graphql_request(
        trial_extend_query,
        json!({ "id": "gid://shopify/AppSubscription/unknown", "days": 5 }),
    ));
    assert_eq!(
        unknown.body["data"]["appSubscriptionTrialExtend"],
        json!({
            "appSubscription": null,
            "userErrors": [{ "field": ["id"], "message": "The app subscription wasn't found.", "code": "SUBSCRIPTION_NOT_FOUND" }]
        })
    );

    let pending = proxy.process_request(json_graphql_request(
        trial_extend_query,
        json!({ "id": subscription_id, "days": 5 }),
    ));
    assert_eq!(
        pending.body["data"]["appSubscriptionTrialExtend"],
        json!({
            "appSubscription": null,
            "userErrors": [{ "field": ["id"], "message": "The trial can't be extended on inactive app subscriptions.", "code": "SUBSCRIPTION_NOT_ACTIVE" }]
        })
    );
}

#[test]
fn app_subscription_create_activates_test_charge_and_reads_back_current_installation() {
    let mut proxy = snapshot_proxy();

    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation AppSubscriptionCreateActivationReadback {
          subscription: appSubscriptionCreate(
            name: "Activation readback plan"
            returnUrl: "https://app.example.test/return"
            trialDays: 7
            test: true
            lineItems: [
              { plan: { appRecurringPricingDetails: { price: { amount: "10.00", currencyCode: USD }, interval: EVERY_30_DAYS } } }
            ]
          ) {
            confirmationUrl
            appSubscription { id status test trialDays currentPeriodEnd }
            userErrors { field message }
          }
        }
        "#,
        json!({}),
    ));
    let subscription_id = create.body["data"]["subscription"]["appSubscription"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(
        create.body["data"]["subscription"],
        json!({
            "confirmationUrl": DERIVED_RETURN_CONFIRMATION_URL,
            "appSubscription": {
                "id": subscription_id,
                "status": "ACTIVE",
                "test": true,
                "trialDays": 7,
                "currentPeriodEnd": "2026-05-05T02:10:00.000Z"
            },
            "userErrors": []
        })
    );

    let read = proxy.process_request(json_graphql_request(
        r#"
        query AppSubscriptionActivationRead {
          installation: currentAppInstallation {
            activeSubscriptions { id status currentPeriodEnd }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        read.body,
        json!({
            "data": {
                "installation": {
                    "activeSubscriptions": [{
                        "id": subscription_id,
                        "status": "ACTIVE",
                        "currentPeriodEnd": "2026-05-05T02:10:00.000Z"
                    }]
                }
            }
        })
    );
}

#[test]
fn removed_app_revoke_access_scopes_codes_scenario_has_rust_coverage() {
    let mut proxy = snapshot_proxy();

    let unknown = proxy.process_request(json_graphql_request(
        r#"
        mutation {
          appRevokeAccessScopes(scopes: ["fake_scope"]) {
            revoked { handle description }
            userErrors { field message code }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        unknown.body["data"]["appRevokeAccessScopes"],
        json!({
            "revoked": null,
            "userErrors": [{
                "field": ["scopes"],
                "message": "The requested list of scopes to revoke includes invalid handles.",
                "code": "UNKNOWN_SCOPES"
            }]
        })
    );

    let mixed = proxy.process_request(json_graphql_request(
        r#"
        mutation {
          appRevokeAccessScopes(scopes: ["read_products", "fake_scope"]) {
            revoked { handle description }
            userErrors { field message code }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        mixed.body["data"]["appRevokeAccessScopes"],
        json!({
            "revoked": null,
            "userErrors": [{
                "field": ["scopes"],
                "message": "The requested list of scopes to revoke includes invalid handles.",
                "code": "UNKNOWN_SCOPES"
            }]
        })
    );

    let required = proxy.process_request(json_graphql_request(
        r#"
        mutation {
          appRevokeAccessScopes(scopes: ["read_products"]) {
            revoked { handle description }
            userErrors { field message code }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        required.body["data"]["appRevokeAccessScopes"],
        json!({
            "revoked": null,
            "userErrors": [{
                "field": ["scopes"],
                "message": "Scopes that are declared as required cannot be revoked.",
                "code": "CANNOT_REVOKE_REQUIRED_SCOPES"
            }]
        })
    );

    let optional = proxy.process_request(json_graphql_request(
        r#"
        mutation {
          appRevokeAccessScopes(scopes: ["write_products"]) {
            revoked { handle description }
            userErrors { field message code }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        optional.body["data"]["appRevokeAccessScopes"],
        json!({
            "revoked": [{ "handle": "write_products", "description": "Modify products, variants, and collections" }],
            "userErrors": []
        })
    );

    let read = proxy.process_request(json_graphql_request(
        r#"
        query {
          currentAppInstallation { accessScopes { handle } }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        read.body["data"]["currentAppInstallation"]["accessScopes"],
        json!([{ "handle": "read_products" }])
    );
}

#[test]
fn removed_app_revoke_access_scopes_error_codes_scenario_has_rust_coverage() {
    let mut proxy = snapshot_proxy();

    for query in [
        r#"
        mutation AppRevokeAccessScopesErrorCodes {
          appRevokeAccessScopes(scopes: ["write_products"]) {
            revoked { handle }
            userErrors { field message code }
          }
        }
        "#,
        r#"
        mutation OrdinaryName {
          appRevokeAccessScopes(scopes: ["write_products"]) {
            revoked { handle }
            userErrors { field message code }
          }
        }
        "#,
        r#"
        mutation {
          appRevokeAccessScopes(scopes: ["write_products"]) {
            revoked { handle }
            userErrors { field message code }
          }
        }
        "#,
    ] {
        let mut request = json_graphql_request(query, json!({}));
        request.headers.insert(
            "x-shopify-draft-proxy-source-app-missing".to_string(),
            "true".to_string(),
        );
        let response = proxy.process_request(request);
        assert_eq!(
            response.body["data"]["appRevokeAccessScopes"],
            json!({
                "revoked": [],
                "userErrors": [{
                    "field": ["id"],
                    "message": "No app found on the access token.",
                    "code": "MISSING_SOURCE_APP"
                }]
            })
        );
    }
}

#[test]
fn removed_delegate_access_token_current_input_local_staging_scenario_has_rust_coverage() {
    let mut proxy = snapshot_proxy();

    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation {
          delegateAccessTokenCreate(input: { delegateAccessScope: ["read_products"], expiresIn: 300 }) {
            delegateAccessToken { accessToken accessScopes createdAt expiresIn }
            userErrors { field message code }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        create.body["data"]["delegateAccessTokenCreate"]["delegateAccessToken"]["accessScopes"],
        json!(["read_products"])
    );
    assert!(
        create.body["data"]["delegateAccessTokenCreate"]["delegateAccessToken"]["accessToken"]
            .as_str()
            .is_some_and(|token| token.starts_with("shpat_delegate_proxy_"))
    );
    assert_eq!(
        create.body["data"]["delegateAccessTokenCreate"]["delegateAccessToken"]["createdAt"],
        json!("2024-01-01T00:00:01.000Z")
    );
    assert_eq!(
        create.body["data"]["delegateAccessTokenCreate"]["delegateAccessToken"]["expiresIn"],
        json!(300)
    );
    assert_eq!(
        create.body["data"]["delegateAccessTokenCreate"]["userErrors"],
        json!([])
    );
}

#[test]
fn removed_app_subscription_line_item_update_validation_scenario_has_rust_coverage() {
    let mut proxy = snapshot_proxy();
    let (_subscription_id, usage_line_item_id, recurring_line_item_id) =
        create_usage_and_recurring_subscription_for_removed_app_tests(&mut proxy);

    let update = proxy.process_request(json_graphql_request(
        r#"
        mutation($usageLineItemId: ID!, $recurringLineItemId: ID!) {
          recurring: appSubscriptionLineItemUpdate(id: $recurringLineItemId, cappedAmount: { amount: 10, currencyCode: USD }) {
            appSubscription { id }
            userErrors { field message }
          }
          currencyMismatch: appSubscriptionLineItemUpdate(id: $usageLineItemId, cappedAmount: { amount: 10, currencyCode: EUR }) {
            appSubscription { id }
            userErrors { field message }
          }
          nonIncreasing: appSubscriptionLineItemUpdate(id: $usageLineItemId, cappedAmount: { amount: 3, currencyCode: USD }) {
            appSubscription { id }
            userErrors { field message }
          }
          success: appSubscriptionLineItemUpdate(id: $usageLineItemId, cappedAmount: { amount: 10, currencyCode: USD }) {
            confirmationUrl
            appSubscription { id lineItems { id plan { pricingDetails { __typename ... on AppUsagePricing { cappedAmount { amount currencyCode } } } } } }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "usageLineItemId": usage_line_item_id,
            "recurringLineItemId": recurring_line_item_id
        }),
    ));
    assert_eq!(
        update.body["data"]["recurring"],
        json!({
            "appSubscription": null,
            "userErrors": [{ "field": null, "message": "Only variable subscriptions can be updated." }]
        })
    );
    assert_eq!(
        update.body["data"]["currencyMismatch"],
        json!({
            "appSubscription": null,
            "userErrors": [{ "field": null, "message": "Currency code must be USD" }]
        })
    );
    assert_eq!(
        update.body["data"]["nonIncreasing"],
        json!({
            "appSubscription": null,
            "userErrors": [{ "field": ["cappedAmount"], "message": "Spending limit can only be increased. Please contact the app developer to decrease spending limit." }]
        })
    );
    assert_eq!(
        update.body["data"]["success"]["confirmationUrl"],
        json!(DERIVED_DEFAULT_CONFIRMATION_URL)
    );
    assert_eq!(update.body["data"]["success"]["userErrors"], json!([]));
}

#[test]
fn removed_app_subscription_cancel_status_transitions_scenario_has_rust_coverage() {
    let mut proxy = snapshot_proxy();
    let (subscription_id, _line_item_id) =
        create_usage_subscription_for_removed_app_tests(&mut proxy, true, 100);

    let cancel = proxy.process_request(json_graphql_request(
        r#"
        mutation($id: ID!) {
          appSubscriptionCancel(id: $id, prorate: true) {
            appSubscription { id status }
            userErrors { field message }
          }
        }
        "#,
        json!({ "id": subscription_id }),
    ));
    assert_eq!(
        cancel.body["data"]["appSubscriptionCancel"]["appSubscription"]["status"],
        json!("CANCELLED")
    );
    assert_eq!(
        cancel.body["data"]["appSubscriptionCancel"]["userErrors"],
        json!([])
    );

    let repeat = proxy.process_request(json_graphql_request(
        r#"
        mutation($id: ID!) {
          appSubscriptionCancel(id: $id, prorate: true) {
            appSubscription { id status }
            userErrors { field message }
          }
        }
        "#,
        json!({ "id": cancel.body["data"]["appSubscriptionCancel"]["appSubscription"]["id"].clone() }),
    ));
    assert_eq!(
        repeat.body["data"]["appSubscriptionCancel"],
        json!({
            "appSubscription": null,
            "userErrors": [{ "field": ["id"], "message": "Cannot transition status via :cancel from :cancelled" }]
        })
    );
}

#[test]
fn removed_app_subscription_trial_extend_validation_scenario_has_rust_coverage() {
    let mut proxy = snapshot_proxy();
    let (subscription_id, _line_item_id) =
        create_usage_subscription_for_removed_app_tests(&mut proxy, false, 100);

    let trial_extend_query = r#"
        mutation($id: ID!, $days: Int!) {
          appSubscriptionTrialExtend(id: $id, days: $days) {
            appSubscription { id trialDays }
            userErrors { field message code }
          }
        }
    "#;

    let days_zero = proxy.process_request(json_graphql_request(
        trial_extend_query,
        json!({ "id": subscription_id, "days": 0 }),
    ));
    assert_eq!(
        days_zero.body["data"]["appSubscriptionTrialExtend"],
        json!({
            "appSubscription": null,
            "userErrors": [{ "field": ["days"], "message": "Days must be greater than 0", "code": null }]
        })
    );

    let days_too_large = proxy.process_request(json_graphql_request(
        trial_extend_query,
        json!({ "id": subscription_id, "days": 1001 }),
    ));
    assert_eq!(
        days_too_large.body["data"]["appSubscriptionTrialExtend"]["userErrors"][0]["message"],
        json!("Days must be less than or equal to 1000")
    );

    let unknown = proxy.process_request(json_graphql_request(
        trial_extend_query,
        json!({ "id": "gid://shopify/AppSubscription/unknown", "days": 5 }),
    ));
    assert_eq!(
        unknown.body["data"]["appSubscriptionTrialExtend"]["userErrors"][0]["code"],
        json!("SUBSCRIPTION_NOT_FOUND")
    );

    let pending = proxy.process_request(json_graphql_request(
        trial_extend_query,
        json!({ "id": subscription_id, "days": 5 }),
    ));
    assert_eq!(
        pending.body["data"]["appSubscriptionTrialExtend"],
        json!({
            "appSubscription": null,
            "userErrors": [{ "field": ["id"], "message": "The trial can't be extended on inactive app subscriptions.", "code": "SUBSCRIPTION_NOT_ACTIVE" }]
        })
    );
}

#[test]
fn removed_app_usage_record_create_cap_and_idempotency_scenario_has_rust_coverage() {
    let mut proxy = snapshot_proxy();
    let (_subscription_id, line_item_id) =
        create_usage_subscription_for_removed_app_tests(&mut proxy, true, 5);

    let success_query = r#"
        mutation($id: ID!) {
          appUsageRecordCreate(
            subscriptionLineItemId: $id
            price: { amount: "3.00", currencyCode: USD }
            description: "first"
            idempotencyKey: "removed-app-usage-key"
          ) {
            appUsageRecord {
              id
              description
              price { amount currencyCode }
              subscriptionLineItem { id plan { pricingDetails { __typename ... on AppUsagePricing { balanceUsed { amount currencyCode } } } } }
            }
            userErrors { field message  }
          }
        }
    "#;
    let success = proxy.process_request(json_graphql_request(
        success_query,
        json!({ "id": line_item_id }),
    ));
    assert_eq!(
        success.body["data"]["appUsageRecordCreate"]["appUsageRecord"]["subscriptionLineItem"]
            ["plan"]["pricingDetails"]["balanceUsed"],
        json!({ "amount": "3.0", "currencyCode": "USD" })
    );

    let duplicate = proxy.process_request(json_graphql_request(
        success_query,
        json!({ "id": line_item_id }),
    ));
    assert_eq!(
        duplicate.body["data"]["appUsageRecordCreate"],
        success.body["data"]["appUsageRecordCreate"]
    );

    let over_cap = proxy.process_request(json_graphql_request(
        r#"
        mutation($id: ID!) {
          appUsageRecordCreate(
            subscriptionLineItemId: $id
            price: { amount: "3.00", currencyCode: USD }
            description: "second"
            idempotencyKey: "removed-app-usage-key-2"
          ) {
            appUsageRecord { id }
            userErrors { field message }
          }
        }
        "#,
        json!({ "id": line_item_id }),
    ));
    assert_eq!(
        over_cap.body["data"]["appUsageRecordCreate"],
        json!({
            "appUsageRecord": null,
            "userErrors": [{ "field": null, "message": "Total price exceeds balance remaining" }]
        })
    );

    let long_key = proxy.process_request(json_graphql_request(
        r#"
        mutation($id: ID!, $key: String) {
          appUsageRecordCreate(
            subscriptionLineItemId: $id
            price: { amount: "1.00", currencyCode: USD }
            description: "too long"
            idempotencyKey: $key
          ) {
            appUsageRecord { id }
            userErrors { field message  }
          }
        }
        "#,
        json!({ "id": line_item_id, "key": "x".repeat(256) }),
    ));
    assert_eq!(
        long_key.body["data"]["appUsageRecordCreate"]["userErrors"][0]["message"],
        json!("Idempotency key exceeds the maximum length.")
    );

    let readback = proxy.process_request(json_graphql_request(
        r#"
        query {
          currentAppInstallation {
            allSubscriptions(first: 5) {
              nodes {
                lineItems {
                  plan { pricingDetails { __typename ... on AppUsagePricing { balanceUsed { amount currencyCode } } } }
                  usageRecords(first: 5) { nodes { id description } }
                }
              }
            }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        readback.body["data"]["currentAppInstallation"]["allSubscriptions"]["nodes"][0]
            ["lineItems"][0]["usageRecords"]["nodes"][0]["description"],
        json!("first")
    );
}

#[test]
fn removed_app_subscription_activation_readback_scenario_has_rust_coverage() {
    let mut proxy = snapshot_proxy();

    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation {
          appSubscriptionCreate(
            name: "Activation readback plan"
            returnUrl: "https://app.example.test/return"
            trialDays: 7
            test: true
            lineItems: [
              { plan: { appRecurringPricingDetails: { price: { amount: "10.00", currencyCode: USD }, interval: EVERY_30_DAYS } } }
            ]
          ) {
            confirmationUrl
            appSubscription { id status test currentPeriodEnd }
            userErrors { field message }
          }
        }
        "#,
        json!({}),
    ));
    let subscription_id =
        create.body["data"]["appSubscriptionCreate"]["appSubscription"]["id"].clone();
    assert_eq!(
        create.body["data"]["appSubscriptionCreate"]["appSubscription"]["status"],
        json!("ACTIVE")
    );

    let read = proxy.process_request(json_graphql_request(
        r#"
        query {
          currentAppInstallation {
            activeSubscriptions { id status currentPeriodEnd }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        read.body["data"]["currentAppInstallation"]["activeSubscriptions"],
        json!([{
            "id": subscription_id,
            "status": "ACTIVE",
            "currentPeriodEnd": "2026-05-05T02:10:00.000Z"
        }])
    );
}

#[test]
fn removed_app_purchase_one_time_create_validation_scenario_has_rust_coverage() {
    let mut proxy = snapshot_proxy();

    let blank = proxy.process_request(json_graphql_request(
        r#"
        mutation {
          appPurchaseOneTimeCreate(name: "   ", returnUrl: "https://app.example.test/return", price: { amount: "5.00", currencyCode: USD }, test: true) {
            appPurchaseOneTime { id }
            confirmationUrl
            userErrors { field message  }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        blank.body["data"]["appPurchaseOneTimeCreate"]["userErrors"][0]["message"],
        json!("Name can't be blank")
    );

    let zero_price = proxy.process_request(json_graphql_request(
        r#"
        mutation {
          appPurchaseOneTimeCreate(name: "Pro", returnUrl: "https://app.example.test/return", price: { amount: "0", currencyCode: USD }, test: true) {
            appPurchaseOneTime { id }
            confirmationUrl
            userErrors { field message  }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        zero_price.body["data"]["appPurchaseOneTimeCreate"]["userErrors"][0]["message"],
        json!("Validation failed: Price must be greater than or equal to 0.5")
    );

    let currency_mismatch = proxy.process_request(json_graphql_request(
        r#"
        mutation {
          appPurchaseOneTimeCreate(name: "Pro", returnUrl: "https://app.example.test/return", price: { amount: "5.00", currencyCode: EUR }, test: true) {
            appPurchaseOneTime { id }
            confirmationUrl
            userErrors { field message  }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        currency_mismatch.body["data"]["appPurchaseOneTimeCreate"]["userErrors"],
        json!([])
    );
    synthetic_gid(
        &currency_mismatch.body["data"]["appPurchaseOneTimeCreate"]["appPurchaseOneTime"]["id"],
        "AppPurchaseOneTime",
    );

    let missing_return_url = proxy.process_request(json_graphql_request(
        r#"
        mutation {
          appPurchaseOneTimeCreate(name: "Pro", price: { amount: "5.00", currencyCode: USD }, test: true) {
            appPurchaseOneTime { id }
            confirmationUrl
            userErrors { field message  }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        missing_return_url.body["errors"][0]["extensions"]["name"],
        json!("appPurchaseOneTimeCreate")
    );

    let success = proxy.process_request(json_graphql_request(
        r#"
        mutation {
          appPurchaseOneTimeCreate(name: "Valid test", returnUrl: "https://app.example.test/return", price: { amount: "5.00", currencyCode: USD }, test: true) {
            appPurchaseOneTime { id name status test createdAt price { amount currencyCode } }
            confirmationUrl
            userErrors { field message  }
          }
        }
        "#,
        json!({}),
    ));
    let success_purchase = &success.body["data"]["appPurchaseOneTimeCreate"]["appPurchaseOneTime"];
    synthetic_gid(&success_purchase["id"], "AppPurchaseOneTime");
    assert_eq!(
        success.body["data"]["appPurchaseOneTimeCreate"],
        json!({
            "appPurchaseOneTime": {
                "id": success_purchase["id"],
                "name": "Valid test",
                "status": "ACTIVE",
                "test": true,
                "createdAt": success_purchase["createdAt"],
                "price": { "amount": "5.0", "currencyCode": "USD" }
            },
            "confirmationUrl": DERIVED_RETURN_CONFIRMATION_URL,
            "userErrors": []
        })
    );
}

#[test]
fn removed_app_billing_access_local_staging_scenario_has_rust_coverage() {
    let mut proxy = snapshot_proxy();
    let (subscription_id, line_item_id) =
        create_usage_subscription_for_removed_app_tests(&mut proxy, true, 100);

    let one_time_id = create_one_time_purchase_for_removed_app_tests(&mut proxy);

    let line_item_update = proxy.process_request(json_graphql_request(
        r#"
        mutation($id: ID!) {
          appSubscriptionLineItemUpdate(id: $id, cappedAmount: { amount: 125, currencyCode: USD }) {
            confirmationUrl
            appSubscription { id lineItems { id plan { pricingDetails { __typename ... on AppUsagePricing { cappedAmount { amount currencyCode } } } } } }
            userErrors { field message }
          }
        }
        "#,
        json!({ "id": line_item_id }),
    ));
    assert_eq!(
        line_item_update.body["data"]["appSubscriptionLineItemUpdate"]["confirmationUrl"],
        json!(DERIVED_DEFAULT_CONFIRMATION_URL)
    );

    let usage = proxy.process_request(json_graphql_request(
        r#"
        mutation($id: ID!) {
          appUsageRecordCreate(subscriptionLineItemId: $id, price: { amount: "12.5", currencyCode: USD }, description: "metered import", idempotencyKey: "billing-access-usage") {
            appUsageRecord { id description subscriptionLineItem { id } }
            userErrors { field message }
          }
        }
        "#,
        json!({ "id": line_item_id }),
    ));
    let usage_record_id = usage.body["data"]["appUsageRecordCreate"]["appUsageRecord"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(
        usage.body["data"]["appUsageRecordCreate"]["userErrors"],
        json!([])
    );

    let trial_extend = proxy.process_request(json_graphql_request(
        r#"
        mutation($id: ID!) {
          appSubscriptionTrialExtend(id: $id, days: 3) {
            appSubscription { id trialDays }
            userErrors { field message }
          }
        }
        "#,
        json!({ "id": subscription_id }),
    ));
    assert_eq!(
        trial_extend.body["data"]["appSubscriptionTrialExtend"],
        json!({
            "appSubscription": { "id": subscription_id, "trialDays": 10 },
            "userErrors": []
        })
    );

    let cancel = proxy.process_request(json_graphql_request(
        r#"
        mutation($id: ID!) {
          appSubscriptionCancel(id: $id, prorate: true) {
            appSubscription { id status }
            userErrors { field message }
          }
        }
        "#,
        json!({ "id": subscription_id }),
    ));
    assert_eq!(
        cancel.body["data"]["appSubscriptionCancel"]["appSubscription"]["status"],
        json!("CANCELLED")
    );

    let readback = proxy.process_request(json_graphql_request(
        r#"
        query {
          currentAppInstallation {
            id
            activeSubscriptions { id }
            allSubscriptions(first: 5) { nodes { id status lineItems { id usageRecords(first: 5) { nodes { id description } } } } }
            oneTimePurchases(first: 5) { nodes { id name status } }
          }
        }
        "#,
        json!({}),
    ));
    let installation_id = readback.body["data"]["currentAppInstallation"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(
        readback.body["data"]["currentAppInstallation"]["allSubscriptions"]["nodes"][0]["status"],
        json!("CANCELLED")
    );
    assert_eq!(
        readback.body["data"]["currentAppInstallation"]["oneTimePurchases"]["nodes"][0]["id"],
        json!(one_time_id)
    );

    let installation_node = proxy.process_request(json_graphql_request(
        r#"
        query($id: ID!) {
          node(id: $id) {
            ... on AppInstallation { id allSubscriptions(first: 5) { nodes { id } } }
          }
        }
        "#,
        json!({ "id": installation_id }),
    ));
    assert_eq!(
        installation_node.body["data"]["node"]["id"],
        json!("gid://shopify/AppInstallation/local")
    );

    for id in [
        subscription_id.as_str(),
        one_time_id.as_str(),
        usage_record_id.as_str(),
    ] {
        let node = proxy.process_request(json_graphql_request(
            r#"
            query($id: ID!) {
              node(id: $id) {
                ... on AppSubscription { id }
                ... on AppPurchaseOneTime { id }
                ... on AppUsageRecord { id }
              }
            }
            "#,
            json!({ "id": id }),
        ));
        assert_eq!(node.body["data"]["node"]["id"], json!(id));
    }

    let revoke = proxy.process_request(json_graphql_request(
        r#"
        mutation {
          appRevokeAccessScopes(scopes: ["write_products"]) {
            revoked { handle }
            userErrors { field message }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        revoke.body["data"]["appRevokeAccessScopes"],
        json!({ "revoked": [{ "handle": "write_products" }], "userErrors": [] })
    );

    let delegate_token = create_delegate_access_token_for_removed_app_tests(&mut proxy);
    let destroy = proxy.process_request(json_graphql_request(
        r#"
        mutation($token: String!) {
          delegateAccessTokenDestroy(accessToken: $token) {
            status
            userErrors { field message code }
          }
        }
        "#,
        json!({ "token": delegate_token }),
    ));
    assert_eq!(
        destroy.body["data"]["delegateAccessTokenDestroy"],
        json!({ "status": true, "userErrors": [] })
    );

    let app_uninstall = proxy.process_request(json_graphql_request(
        r#"
        mutation {
          appUninstall {
            app { id handle }
            userErrors { field message }
          }
        }
        "#,
        json!({}),
    ));
    let app_id = app_uninstall.body["data"]["appUninstall"]["app"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(
        app_uninstall.body["data"]["appUninstall"]["userErrors"],
        json!([])
    );

    let app_node = proxy.process_request(json_graphql_request(
        r#"
        query($id: ID!) {
          node(id: $id) {
            ... on App { id handle }
          }
        }
        "#,
        json!({ "id": app_id.clone() }),
    ));
    assert_eq!(
        app_node.body["data"]["node"],
        json!({ "id": app_id, "handle": "shopify-draft-proxy" })
    );

    let after_uninstall = proxy.process_request(json_graphql_request(
        r#"query { currentAppInstallation { id } }"#,
        json!({}),
    ));
    assert_eq!(
        after_uninstall.body["data"]["currentAppInstallation"],
        Value::Null
    );
}

#[test]
fn removed_app_uninstall_error_codes_and_cascade_scenario_has_rust_coverage() {
    let mut proxy = snapshot_proxy();

    let (subscription_id, _line_item_id) =
        create_usage_subscription_for_removed_app_tests(&mut proxy, true, 100);
    let delegate_token = create_delegate_access_token_for_removed_app_tests(&mut proxy);

    let uninstall = proxy.process_request(json_graphql_request(
        r#"
        mutation {
          appUninstall {
            app { id handle }
            userErrors { field message code }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        uninstall.body["data"]["appUninstall"]["userErrors"],
        json!([])
    );

    let subscription_node = proxy.process_request(json_graphql_request(
        r#"
        query($id: ID!) {
          node(id: $id) {
            __typename
            ... on AppSubscription { status }
          }
        }
        "#,
        json!({ "id": subscription_id }),
    ));
    assert_eq!(
        subscription_node.body["data"]["node"],
        json!({ "__typename": "AppSubscription", "status": "CANCELLED" })
    );

    let destroy_after_uninstall = proxy.process_request(json_graphql_request(
        r#"
        mutation($token: String!) {
          delegateAccessTokenDestroy(accessToken: $token) {
            status
            userErrors { field message code }
          }
        }
        "#,
        json!({ "token": delegate_token }),
    ));
    assert_eq!(
        destroy_after_uninstall.body["data"]["delegateAccessTokenDestroy"],
        json!({
            "status": false,
            "userErrors": [{ "field": null, "message": "Access token does not exist.", "code": "ACCESS_TOKEN_NOT_FOUND" }]
        })
    );

    let known_uninstalled = proxy.process_request(json_graphql_request(
        r#"
        mutation {
          appUninstall {
            app { id }
            userErrors { field message code }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        known_uninstalled.body["data"]["appUninstall"],
        json!({
            "app": null,
            "userErrors": [{ "field": ["id"], "message": "App is not installed on shop", "code": "APP_NOT_INSTALLED" }]
        })
    );
}

fn create_usage_subscription_for_removed_app_tests(
    proxy: &mut DraftProxy,
    test_charge: bool,
    capped_amount: i64,
) -> (String, String) {
    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation($test: Boolean!, $lineItems: [AppSubscriptionLineItemInput!]!) {
          appSubscriptionCreate(
            name: "Removed scenario usage plan"
            returnUrl: "https://app.example.test/return"
            trialDays: 7
            test: $test
            lineItems: $lineItems
          ) {
            confirmationUrl
            appSubscription {
              id
              status
              trialDays
              lineItems { id }
            }
            userErrors { field message  }
          }
        }
        "#,
        json!({
            "test": test_charge,
            "lineItems": [{
                "plan": {
                    "appUsagePricingDetails": {
                        "cappedAmount": { "amount": capped_amount, "currencyCode": "USD" },
                        "terms": "usage terms"
                    }
                }
            }]
        }),
    ));
    assert_eq!(
        create.body["data"]["appSubscriptionCreate"]["userErrors"],
        json!([])
    );
    let subscription_id = create.body["data"]["appSubscriptionCreate"]["appSubscription"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let line_item_id = create.body["data"]["appSubscriptionCreate"]["appSubscription"]["lineItems"]
        [0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    (subscription_id, line_item_id)
}

fn create_usage_and_recurring_subscription_for_removed_app_tests(
    proxy: &mut DraftProxy,
) -> (String, String, String) {
    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation {
          appSubscriptionCreate(
            name: "Removed scenario mixed plan"
            returnUrl: "https://app.example.test/return"
            trialDays: 7
            test: true
            lineItems: [
              { plan: { appUsagePricingDetails: { cappedAmount: { amount: 5, currencyCode: USD }, terms: "usage terms" } } }
              { plan: { appRecurringPricingDetails: { price: { amount: 1, currencyCode: USD }, interval: EVERY_30_DAYS } } }
            ]
          ) {
            appSubscription { id lineItems { id } }
            userErrors { field message  }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        create.body["data"]["appSubscriptionCreate"]["userErrors"],
        json!([])
    );
    let subscription_id = create.body["data"]["appSubscriptionCreate"]["appSubscription"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let usage_line_item_id = create.body["data"]["appSubscriptionCreate"]["appSubscription"]
        ["lineItems"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let recurring_line_item_id = create.body["data"]["appSubscriptionCreate"]["appSubscription"]
        ["lineItems"][1]["id"]
        .as_str()
        .unwrap()
        .to_string();
    (subscription_id, usage_line_item_id, recurring_line_item_id)
}

fn create_one_time_purchase_for_removed_app_tests(proxy: &mut DraftProxy) -> String {
    let one_time = proxy.process_request(json_graphql_request(
        r#"
        mutation {
          appPurchaseOneTimeCreate(name: "Import package", returnUrl: "https://app.example.test/return", price: { amount: 10, currencyCode: USD }, test: true) {
            appPurchaseOneTime { id name status test }
            confirmationUrl
            userErrors { field message  }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        one_time.body["data"]["appPurchaseOneTimeCreate"]["userErrors"],
        json!([])
    );
    one_time.body["data"]["appPurchaseOneTimeCreate"]["appPurchaseOneTime"]["id"]
        .as_str()
        .unwrap()
        .to_string()
}

fn create_delegate_access_token_for_removed_app_tests(proxy: &mut DraftProxy) -> String {
    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation {
          delegateAccessTokenCreate(input: { delegateAccessScope: ["read_products"], expiresIn: 300 }) {
            delegateAccessToken { accessToken accessScopes expiresIn }
            userErrors { field message code }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        create.body["data"]["delegateAccessTokenCreate"]["userErrors"],
        json!([])
    );
    create.body["data"]["delegateAccessTokenCreate"]["delegateAccessToken"]["accessToken"]
        .as_str()
        .unwrap()
        .to_string()
}

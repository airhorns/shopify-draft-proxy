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

fn current_epoch_seconds() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("test clock should be after Unix epoch")
        .as_secs() as i64
}

fn rfc3339_millis_utc_epoch_seconds(value: &Value) -> i64 {
    let timestamp = value
        .as_str()
        .expect("timestamp should be a string in UTC RFC3339 format");
    assert!(
        timestamp.ends_with(".000Z"),
        "{timestamp} should use Shopify-style millisecond UTC formatting"
    );
    let parse = |start: usize, end: usize| -> i64 {
        timestamp[start..end]
            .parse::<i64>()
            .expect("timestamp component should be numeric")
    };
    let date = time::Date::from_calendar_date(
        parse(0, 4) as i32,
        time::Month::try_from(parse(5, 7) as u8).expect("timestamp month should be valid"),
        parse(8, 10) as u8,
    )
    .expect("timestamp date should be a valid calendar date");
    let clock_time = time::Time::from_hms(
        parse(11, 13) as u8,
        parse(14, 16) as u8,
        parse(17, 19) as u8,
    )
    .expect("timestamp time should be valid");
    time::PrimitiveDateTime::new(date, clock_time)
        .assume_utc()
        .unix_timestamp()
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
            r#"mutation OneTime { appPurchaseOneTimeCreate(name: "Import", returnUrl: "https://app.example.test/return", price: { amount: 5, currencyCode: USD }, test: false) { appPurchaseOneTime { id test } confirmationUrl userErrors { field message code } } }"#,
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
            "revoked": [{ "handle": "write_products", "description": null }],
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
            "revoked": [{ "handle": "read_orders", "description": null }],
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
        mutation CustomAppUninstall($id: ID!) {
          appUninstall(input: { id: $id }) {
            app { id handle }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "id": custom_app_id }),
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
        mutation WrongAppUninstall {
          appUninstall(input: { id: "gid://shopify/App/missing" }) {
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
    let missing = snapshot_proxy().process_request(missing_request);
    assert_eq!(
        missing.body["data"]["appUninstall"],
        json!({
            "app": null,
            "userErrors": [{
                "field": ["id"],
                "message": "App not found",
                "code": "APP_NOT_FOUND"
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
            userErrors { field message code }
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
            "userErrors": [{ "field": ["name"], "message": "Name can't be blank", "code": null }]
        })
    );

    let zero_price = proxy.process_request(json_graphql_request(
        r#"
        mutation AppPurchaseOneTimeCreateValidationZeroPrice {
          appPurchaseOneTimeCreate(name: "Pro", returnUrl: "https://app.example.test/return", price: { amount: "0", currencyCode: USD }, test: true) {
            appPurchaseOneTime { id }
            confirmationUrl
            userErrors { field message code }
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
            "userErrors": [{ "field": null, "message": "Validation failed: Price must be greater than or equal to 0.5", "code": null }]
        })
    );

    let currency_mismatch = proxy.process_request(json_graphql_request(
        r#"
        mutation AppPurchaseOneTimeCreateValidationCurrencyMismatch {
          appPurchaseOneTimeCreate(name: "Pro", returnUrl: "https://app.example.test/return", price: { amount: "5.00", currencyCode: EUR }, test: true) {
            appPurchaseOneTime { id price { amount currencyCode } }
            confirmationUrl
            userErrors { field message code }
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
            userErrors { field message code }
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
            userErrors { field message code }
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

    let uninstall = proxy.process_request(json_graphql_request(
        r#"
        mutation {
          appUninstall(input: { id: "gid://shopify/App/missing" }) {
            app { id }
            userErrors {
              __typename
              message
              ... on AppUninstallError { code }
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
                "__typename": "AppUninstallError",
                "message": "App not found",
                "code": "APP_NOT_FOUND"
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
              ... on AppRevokeScopeError { code }
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
                "__typename": "AppRevokeScopeError",
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
                "__typename": "UserError",
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
            userErrors { field message code }
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
            "userErrors": [{ "field": ["idempotencyKey"], "message": "Idempotency key exceeds the maximum length.", "code": null }]
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
            userErrors { field message code }
          }
        }
        "#,
        json!({ "id": line_item_id }),
    ));
    assert_eq!(
        missing_description.body["data"]["appUsageRecordCreate"],
        json!({
            "appUsageRecord": null,
            "userErrors": [{ "field": ["description"], "message": "Description can't be blank", "code": null }]
        })
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
            userErrors { field message code }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        invalid_line_item_id.body["data"]["appUsageRecordCreate"],
        json!({
            "appUsageRecord": null,
            "userErrors": [{ "field": ["subscriptionLineItemId"], "message": "Invalid id", "code": null }]
        })
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
            "id": "gid://shopify/AppInstallation/expected",
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
            "app": { "id": "gid://shopify/App/expected", "handle": "shopify-draft-proxy" },
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

    let unknown = proxy.process_request(json_graphql_request(
        r#"
        mutation AppUninstallUnknownInput($input: AppUninstallInput) {
          appUninstall(input: $input) {
            app { id }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "input": { "id": "gid://shopify/App/9999999999" } }),
    ));
    assert_eq!(
        unknown.body["data"]["appUninstall"],
        json!({
            "app": null,
            "userErrors": [{
                "field": ["id"],
                "message": "App not found",
                "code": "APP_NOT_FOUND"
            }]
        })
    );

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
    let current_app_id = uninstall.body["data"]["appUninstall"]["app"]["id"]
        .as_str()
        .expect("successful appUninstall should return app id")
        .to_string();

    let already_uninstalled = proxy.process_request(json_graphql_request(
        r#"
        mutation AppUninstallKnownInput($input: AppUninstallInput) {
          appUninstall(input: $input) {
            app { id }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "input": { "id": current_app_id } }),
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
        synchronous_update.body["data"]["appSubscriptionLineItemUpdate"],
        json!({
            "confirmationUrl": null,
            "appSubscription": {
                "lineItems": [
                    {
                        "id": usage_line_item_id,
                        "plan": { "pricingDetails": {
                            "__typename": "AppUsagePricing",
                            "cappedAmount": { "amount": "12.0", "currencyCode": "USD" }
                        }}
                    },
                    {
                        "id": recurring_line_item_id,
                        "plan": { "pricingDetails": {
                            "__typename": "AppRecurringPricing"
                        }}
                    }
                ]
            },
            "userErrors": []
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

    let before_create = current_epoch_seconds();
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
    let after_create = current_epoch_seconds();
    let subscription_id = create.body["data"]["subscription"]["appSubscription"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let current_period_end =
        create.body["data"]["subscription"]["appSubscription"]["currentPeriodEnd"].clone();
    let current_period_end_epoch = rfc3339_millis_utc_epoch_seconds(&current_period_end);
    assert!(
        current_period_end_epoch >= before_create + 7 * 86_400,
        "{current_period_end_epoch} should be at least 7 days after the pre-create clock"
    );
    assert!(
        current_period_end_epoch <= after_create + 7 * 86_400,
        "{current_period_end_epoch} should be no later than 7 days after the post-create clock"
    );
    assert_eq!(
        create.body["data"]["subscription"],
        json!({
            "confirmationUrl": DERIVED_RETURN_CONFIRMATION_URL,
            "appSubscription": {
                "id": subscription_id,
                "status": "ACTIVE",
                "test": true,
                "trialDays": 7,
                "currentPeriodEnd": current_period_end.clone()
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
                        "currentPeriodEnd": current_period_end
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
            "revoked": [{ "handle": "write_products", "description": null }],
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
            shop { id currencyCode }
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
        create.body["data"]["delegateAccessTokenCreate"]["shop"],
        json!({})
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
            userErrors { field message code }
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
            userErrors { field message code }
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

    let before_create = current_epoch_seconds();
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
    let after_create = current_epoch_seconds();
    let subscription_id =
        create.body["data"]["appSubscriptionCreate"]["appSubscription"]["id"].clone();
    let current_period_end =
        create.body["data"]["appSubscriptionCreate"]["appSubscription"]["currentPeriodEnd"].clone();
    let current_period_end_epoch = rfc3339_millis_utc_epoch_seconds(&current_period_end);
    assert!(
        current_period_end_epoch >= before_create + 7 * 86_400,
        "{current_period_end_epoch} should be at least 7 days after the pre-create clock"
    );
    assert!(
        current_period_end_epoch <= after_create + 7 * 86_400,
        "{current_period_end_epoch} should be no later than 7 days after the post-create clock"
    );
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
            "currentPeriodEnd": current_period_end
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
            userErrors { field message code }
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
            userErrors { field message code }
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
            userErrors { field message code }
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
            userErrors { field message code }
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
            userErrors { field message code }
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
        json!("gid://shopify/AppInstallation/expected")
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

    let unknown = proxy.process_request(json_graphql_request(
        r#"
        mutation($input: AppUninstallInput) {
          appUninstall(input: $input) {
            app { id }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "input": { "id": "gid://shopify/App/9999999999" } }),
    ));
    assert_eq!(
        unknown.body["data"]["appUninstall"],
        json!({
            "app": null,
            "userErrors": [{ "field": ["id"], "message": "App not found", "code": "APP_NOT_FOUND" }]
        })
    );

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
    let app_id = uninstall.body["data"]["appUninstall"]["app"]["id"]
        .as_str()
        .unwrap()
        .to_string();
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
        mutation($input: AppUninstallInput) {
          appUninstall(input: $input) {
            app { id }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "input": { "id": app_id } }),
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
            userErrors { field message code }
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
            userErrors { field message code }
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
            userErrors { field message code }
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

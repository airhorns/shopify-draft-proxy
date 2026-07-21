use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use pretty_assertions::assert_eq;
use serde_json::{json, Value};
use shopify_draft_proxy::proxy::{
    Config, DraftProxy, ProductRecord, ReadMode, Request, Response, UnsupportedMutationMode,
};

fn product(id: &str, title: &str, handle: &str) -> ProductRecord {
    ProductRecord {
        id: id.to_string(),
        created_at: "2024-01-01T00:00:00.000Z".to_string(),
        updated_at: "2024-01-01T00:00:00.000Z".to_string(),
        title: title.to_string(),
        handle: handle.to_string(),
        status: "ACTIVE".to_string(),
        ..ProductRecord::default()
    }
}

fn product_state(id: &str, title: &str, handle: &str) -> Value {
    json!({
        "id": id,
        "createdAt": "2024-01-01T00:00:00.000Z",
        "updatedAt": "2024-01-01T00:00:00.000Z",
        "title": title,
        "handle": handle,
        "status": "ACTIVE",
        "descriptionHtml": "",
        "vendor": "",
        "productType": "",
        "tags": [],
        "templateSuffix": "",
        "seo": {
            "title": "",
            "description": ""
        },
        "totalInventory": 0,
        "tracksInventory": false,
        "media": { "nodes": [] },
        "variants": { "nodes": [] },
        "collections": { "nodes": [] },
        "extraFields": {}
    })
}

fn snapshot_proxy() -> DraftProxy {
    DraftProxy::new(Config {
        read_mode: ReadMode::Snapshot,
        unsupported_mutation_mode: None,
        bulk_operation_run_mutation_max_input_file_size_bytes: None,
        port: 0,
        shopify_admin_origin: "https://shopify.com".to_string(),
        snapshot_path: None,
    })
}

fn request(method: &str, path: &str, body: &str) -> Request {
    Request {
        method: method.to_string(),
        path: path.to_string(),
        headers: Default::default(),
        body: body.to_string(),
    }
}

fn graphql_request(query: &str, variables: Value) -> Request {
    request(
        "POST",
        "/admin/api/2025-01/graphql.json",
        &json!({
            "query": query,
            "variables": variables
        })
        .to_string(),
    )
}

fn current_graphql_request(query: &str, variables: Value) -> Request {
    request(
        "POST",
        "/admin/api/2026-04/graphql.json",
        &json!({
            "query": query,
            "variables": variables
        })
        .to_string(),
    )
}

fn ok_json(body: Value) -> Response {
    Response {
        status: 200,
        headers: Default::default(),
        body,
    }
}

fn dump(proxy: &mut DraftProxy) -> Value {
    let response = proxy.process_request(request(
        "POST",
        "/__meta/dump",
        &json!({ "createdAt": "2026-05-23T00:00:00.000Z" }).to_string(),
    ));
    assert_eq!(response.status, 200);
    response.body
}

fn restore(proxy: &mut DraftProxy, body: &Value) {
    let response = proxy.process_request(request("POST", "/__meta/restore", &body.to_string()));
    assert_eq!(response.status, 200);
}

#[derive(Clone)]
struct RepresentativeRoundTripIds {
    webhook: Value,
    selling_plan_group: Value,
    draft_order: Value,
}

fn stage_representative_round_trip_state(proxy: &mut DraftProxy) -> RepresentativeRoundTripIds {
    let app_subscription = proxy.process_request(current_graphql_request(
        r#"
        mutation CreateRoundTripSubscription {
          appSubscriptionCreate(
            name: "Round-trip subscription"
            returnUrl: "https://app.example.test/return"
            test: true
            lineItems: [{
              plan: {
                appRecurringPricingDetails: {
                  price: { amount: "10.00", currencyCode: USD }
                  interval: EVERY_30_DAYS
                }
              }
            }]
          ) {
            appSubscription { id }
            userErrors { field message }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        app_subscription.body["data"]["appSubscriptionCreate"]["userErrors"],
        json!([])
    );

    let webhook = proxy.process_request(current_graphql_request(
        r#"
        mutation CreateRepresentativeWebhook {
          webhookSubscriptionCreate(
            topic: ORDERS_CREATE
            webhookSubscription: { uri: "https://hooks.example.com/representative", format: JSON }
          ) {
            webhookSubscription { id }
            userErrors { field message }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        webhook.body["data"]["webhookSubscriptionCreate"]["userErrors"],
        json!([])
    );
    let webhook_id =
        webhook.body["data"]["webhookSubscriptionCreate"]["webhookSubscription"]["id"].clone();

    let media = proxy.process_request(current_graphql_request(
        r#"
        mutation CreateRepresentativeMedia($files: [FileCreateInput!]!) {
          fileCreate(files: $files) {
            files { id }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "files": [{
                "alt": "Round-trip media",
                "contentType": "IMAGE",
                "filename": "round-trip.jpg",
                "originalSource": "https://cdn.example.com/round-trip.jpg"
            }]
        }),
    ));
    assert_eq!(media.body["data"]["fileCreate"]["userErrors"], json!([]));

    let selling_plan = proxy.process_request(current_graphql_request(
        r#"
        mutation CreateRepresentativeSellingPlan($input: SellingPlanGroupInput!) {
          sellingPlanGroupCreate(input: $input) {
            sellingPlanGroup { id }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "input": {
                "name": "Round-trip selling plan",
                "options": ["Delivery frequency"],
                "sellingPlansToCreate": [{
                    "name": "Monthly",
                    "options": ["Monthly"],
                    "category": "SUBSCRIPTION",
                    "billingPolicy": { "recurring": { "interval": "MONTH", "intervalCount": 1 } },
                    "deliveryPolicy": { "recurring": { "interval": "MONTH", "intervalCount": 1 } }
                }]
            }
        }),
    ));
    assert_eq!(
        selling_plan.body["data"]["sellingPlanGroupCreate"]["userErrors"],
        json!([])
    );
    let selling_plan_group =
        selling_plan.body["data"]["sellingPlanGroupCreate"]["sellingPlanGroup"]["id"].clone();

    let market = proxy.process_request(current_graphql_request(
        r#"
        mutation CreateRepresentativeMarket($input: MarketCreateInput!) {
          marketCreate(input: $input) {
            market { id }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "input": { "name": "Round-trip Canada", "regions": [{ "countryCode": "CA" }] } }),
    ));
    assert_eq!(market.body["data"]["marketCreate"]["userErrors"], json!([]));

    let draft = proxy.process_request(current_graphql_request(
        r#"
        mutation CreateRepresentativePaymentTermsDraft($input: DraftOrderInput!) {
          draftOrderCreate(input: $input) {
            draftOrder { id }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "input": {
                "email": "round-trip-terms@example.test",
                "lineItems": [{
                    "title": "Payment terms item",
                    "quantity": 1,
                    "originalUnitPrice": "18.50"
                }]
            }
        }),
    ));
    assert_eq!(
        draft.body["data"]["draftOrderCreate"]["userErrors"],
        json!([])
    );
    let draft_order = draft.body["data"]["draftOrderCreate"]["draftOrder"]["id"].clone();

    let payment_terms = proxy.process_request(current_graphql_request(
        r#"
        mutation CreateRepresentativePaymentTerms(
          $referenceId: ID!
          $attributes: PaymentTermsCreateInput!
        ) {
          paymentTermsCreate(
            referenceId: $referenceId
            paymentTermsAttributes: $attributes
          ) {
            paymentTerms { id }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "referenceId": draft_order.clone(),
            "attributes": {
                "paymentTermsTemplateId": "gid://shopify/PaymentTermsTemplate/4",
                "paymentSchedules": [{ "issuedAt": "2026-07-20T00:00:00Z" }]
            }
        }),
    ));
    assert_eq!(
        payment_terms.body["data"]["paymentTermsCreate"]["userErrors"],
        json!([])
    );

    RepresentativeRoundTripIds {
        webhook: webhook_id,
        selling_plan_group,
        draft_order,
    }
}

fn representative_round_trip_reads(
    proxy: &mut DraftProxy,
    ids: &RepresentativeRoundTripIds,
) -> Vec<Response> {
    [
        (
            r#"
            query ReadRoundTripBilling {
              currentAppInstallation {
                activeSubscriptions { id name status test }
              }
            }
            "#,
            json!({}),
        ),
        (
            r#"
            query ReadRoundTripWebhook($id: ID!) {
              webhookSubscription(id: $id) { id topic uri format }
              webhookSubscriptions(first: 10) { nodes { id topic uri format } }
              webhookSubscriptionsCount { count }
            }
            "#,
            json!({ "id": ids.webhook.clone() }),
        ),
        (
            r#"
            query ReadRoundTripMedia {
              files(first: 10) {
                nodes {
                  id alt fileStatus
                  ... on MediaImage { image { url } }
                }
              }
            }
            "#,
            json!({}),
        ),
        (
            r#"
            query ReadRoundTripSellingPlan($id: ID!) {
              sellingPlanGroup(id: $id) {
                id name options
                sellingPlans(first: 10) { nodes { id name category } }
              }
            }
            "#,
            json!({ "id": ids.selling_plan_group.clone() }),
        ),
        (
            r#"
            query ReadRoundTripMarkets {
              markets(first: 10) { nodes { id name handle status } }
            }
            "#,
            json!({}),
        ),
        (
            r#"
            query ReadRoundTripPaymentTerms($id: ID!) {
              draftOrder(id: $id) {
                id
                paymentTerms {
                  id paymentTermsName paymentTermsType
                  paymentSchedules(first: 10) { nodes { id issuedAt dueAt } }
                }
              }
            }
            "#,
            json!({ "id": ids.draft_order.clone() }),
        ),
    ]
    .into_iter()
    .map(|(query, variables)| proxy.process_request(current_graphql_request(query, variables)))
    .collect()
}

#[test]
fn structural_dump_restore_round_trips_representative_public_behavior() {
    let mut source = snapshot_proxy();
    let ids = stage_representative_round_trip_state(&mut source);
    let expected_reads = representative_round_trip_reads(&mut source, &ids);
    assert!(expected_reads
        .iter()
        .all(|response| response.status == 200 && response.body.get("errors").is_none()));
    let expected_log = source.process_request(request("GET", "/__meta/log", ""));
    let expected_roots = expected_log.body["entries"]
        .as_array()
        .expect("representative mutations should be logged")
        .iter()
        .map(|entry| entry["interpreted"]["primaryRootField"].clone())
        .collect::<Vec<_>>();
    assert_eq!(
        expected_roots,
        vec![
            json!("appSubscriptionCreate"),
            json!("webhookSubscriptionCreate"),
            json!("fileCreate"),
            json!("sellingPlanGroupCreate"),
            json!("marketCreate"),
            json!("draftOrderCreate"),
            json!("paymentTermsCreate")
        ]
    );
    let source_dump = dump(&mut source);
    assert_eq!(
        source_dump["schema"],
        json!("shopify-draft-proxy-rust-state/v2")
    );
    assert!(source_dump["runtimeState"]["store"].is_object());

    let mut fresh = snapshot_proxy();
    restore(&mut fresh, &source_dump);
    assert_eq!(
        representative_round_trip_reads(&mut fresh, &ids),
        expected_reads
    );
    assert_eq!(
        fresh.process_request(request("GET", "/__meta/log", "")),
        expected_log
    );

    let mut reused = snapshot_proxy();
    let _ = reused.process_request(current_graphql_request(
        "mutation { productCreate(product: { title: \"Stale product\" }) { product { id } } }",
        json!({}),
    ));
    restore(&mut reused, &source_dump);
    assert_eq!(
        representative_round_trip_reads(&mut reused, &ids),
        expected_reads
    );
    assert_eq!(
        reused.process_request(request("GET", "/__meta/log", "")),
        expected_log
    );

    let next_fresh = fresh.process_request(current_graphql_request(
        r#"
        mutation CreateNextRoundTripWebhook {
          webhookSubscriptionCreate(
            topic: SHOP_UPDATE
            webhookSubscription: { uri: "https://hooks.example.com/next-fresh", format: JSON }
          ) { webhookSubscription { id } userErrors { field message } }
        }
        "#,
        json!({}),
    ));
    let next_reused = reused.process_request(current_graphql_request(
        r#"
        mutation CreateNextRoundTripWebhook {
          webhookSubscriptionCreate(
            topic: SHOP_UPDATE
            webhookSubscription: { uri: "https://hooks.example.com/next-reused", format: JSON }
          ) { webhookSubscription { id } userErrors { field message } }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        next_fresh.body["data"]["webhookSubscriptionCreate"]["webhookSubscription"]["id"],
        next_reused.body["data"]["webhookSubscriptionCreate"]["webhookSubscription"]["id"],
        "synthetic identity must resume identically after fresh and reused restore"
    );

    let reset = fresh.process_request(request("POST", "/__meta/reset", ""));
    assert_eq!(reset.status, 200);
    let mut clean = snapshot_proxy();
    assert_eq!(
        representative_round_trip_reads(&mut fresh, &ids),
        representative_round_trip_reads(&mut clean, &ids),
        "reset restored state must expose the same public behavior as a fresh proxy"
    );
    assert_eq!(
        fresh.process_request(request("GET", "/__meta/log", "")),
        clean.process_request(request("GET", "/__meta/log", ""))
    );
}

#[test]
fn dump_restore_preserves_webhook_reads_and_replaces_reused_state() {
    const CREATE: &str = r#"
        mutation CreateRoundTripWebhook($uri: URL!) {
          webhookSubscriptionCreate(
            topic: ORDERS_CREATE
            webhookSubscription: { uri: $uri, format: JSON }
          ) {
            webhookSubscription { id topic uri }
            userErrors { field message }
          }
        }
    "#;
    const READ: &str = r#"
        query ReadRoundTripWebhooks($id: ID!) {
          webhookSubscription(id: $id) { id topic uri }
          webhookSubscriptions(first: 10) { nodes { id topic uri } }
          webhookSubscriptionsCount { count }
        }
    "#;

    let mut source = snapshot_proxy();
    let create = source.process_request(current_graphql_request(
        CREATE,
        json!({ "uri": "https://hooks.example.com/round-trip-source" }),
    ));
    assert_eq!(
        create.body["data"]["webhookSubscriptionCreate"]["userErrors"],
        json!([])
    );
    let source_id =
        create.body["data"]["webhookSubscriptionCreate"]["webhookSubscription"]["id"].clone();
    let expected = source.process_request(current_graphql_request(
        READ,
        json!({ "id": source_id.clone() }),
    ));
    let source_dump = dump(&mut source);

    let mut fresh = snapshot_proxy();
    restore(&mut fresh, &source_dump);
    assert_eq!(
        fresh.process_request(current_graphql_request(
            READ,
            json!({ "id": source_id.clone() }),
        )),
        expected,
        "fresh restore must preserve response-affecting webhook state"
    );

    let mut reused = snapshot_proxy();
    let _first_stale = reused.process_request(current_graphql_request(
        CREATE,
        json!({ "uri": "https://hooks.example.com/first-stale-reused-state" }),
    ));
    let stale = reused.process_request(current_graphql_request(
        CREATE,
        json!({ "uri": "https://hooks.example.com/stale-reused-state" }),
    ));
    let stale_id =
        stale.body["data"]["webhookSubscriptionCreate"]["webhookSubscription"]["id"].clone();
    restore(&mut reused, &source_dump);
    assert_eq!(
        reused.process_request(current_graphql_request(READ, json!({ "id": source_id }),)),
        expected,
        "restore into a reused proxy must replace prior state"
    );
    let stale_read =
        reused.process_request(current_graphql_request(READ, json!({ "id": stale_id })));
    assert_eq!(stale_read.body["data"]["webhookSubscription"], Value::Null);
    assert_eq!(
        stale_read.body["data"]["webhookSubscriptionsCount"],
        json!({ "count": 1 })
    );
}

#[test]
fn store_effective_products_stage_overrides_base_and_tombstones() {
    let mut proxy = snapshot_proxy().with_base_products(vec![
        product("gid://shopify/Product/base-1", "Base one", "base-one"),
        product("gid://shopify/Product/base-2", "Base two", "base-two"),
    ]);

    let update = proxy.process_request(graphql_request(
        r#"
        mutation ProductStoreUpdate($product: ProductUpdateInput!) {
          productUpdate(product: $product) {
            product { id title handle }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "product": {
                "id": "gid://shopify/Product/base-1",
                "title": "Updated one",
                "handle": "updated-one"
            }
        }),
    ));
    assert_eq!(update.status, 200);
    assert_eq!(
        update.body["data"]["productUpdate"]["product"]["title"],
        json!("Updated one")
    );

    let create = proxy.process_request(graphql_request(
        r#"
        mutation ProductStoreCreate($product: ProductCreateInput!) {
          productCreate(product: $product) {
            product { id title handle }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "product": {
                "title": "New product",
                "handle": "new-product"
            }
        }),
    ));
    assert_eq!(create.status, 200);
    let new_id = create.body["data"]["productCreate"]["product"]["id"]
        .as_str()
        .expect("productCreate should return an id")
        .to_string();

    let delete = proxy.process_request(graphql_request(
        r#"
        mutation ProductStoreDelete($input: ProductDeleteInput!) {
          productDelete(input: $input) {
            deletedProductId
            userErrors { field message }
          }
        }
        "#,
        json!({ "input": { "id": "gid://shopify/Product/base-2" } }),
    ));
    assert_eq!(delete.status, 200);
    assert_eq!(
        delete.body["data"]["productDelete"]["deletedProductId"],
        json!("gid://shopify/Product/base-2")
    );

    let read = proxy.process_request(graphql_request(
        r#"
        query ProductStoreRead($updatedId: ID!, $deletedId: ID!, $newHandle: String!) {
          updated: product(id: $updatedId) { id title handle }
          deleted: product(id: $deletedId) { id }
          byHandle: productByIdentifier(identifier: { handle: $newHandle }) { id title handle }
          products(first: 10) { nodes { id title handle } }
          productsCount { count precision }
        }
        "#,
        json!({
            "updatedId": "gid://shopify/Product/base-1",
            "deletedId": "gid://shopify/Product/base-2",
            "newHandle": "new-product"
        }),
    ));
    assert_eq!(read.status, 200);
    assert_eq!(
        read.body["data"]["updated"],
        json!({
            "id": "gid://shopify/Product/base-1",
            "title": "Updated one",
            "handle": "updated-one"
        })
    );
    assert_eq!(read.body["data"]["deleted"], Value::Null);
    assert_eq!(
        read.body["data"]["byHandle"],
        json!({
            "id": new_id,
            "title": "New product",
            "handle": "new-product"
        })
    );
    assert_eq!(
        read.body["data"]["products"]["nodes"],
        json!([
            {
                "id": "gid://shopify/Product/base-1",
                "title": "Updated one",
                "handle": "updated-one"
            },
            {
                "id": new_id,
                "title": "New product",
                "handle": "new-product"
            }
        ])
    );
    assert_eq!(
        read.body["data"]["productsCount"],
        json!({ "count": 2, "precision": "EXACT" })
    );
}

#[test]
fn store_saved_searches_overlay_defaults_base_and_tombstones_in_order() {
    let mut proxy = snapshot_proxy();
    let mut seed = dump(&mut proxy);
    seed["state"]["baseState"]["savedSearches"] = json!({
        "gid://shopify/SavedSearch/base": {
            "id": "gid://shopify/SavedSearch/base",
            "name": "Base products",
            "query": "tag:promo",
            "resourceType": "PRODUCT"
        }
    });
    seed["state"]["baseState"]["savedSearchOrder"] = json!(["gid://shopify/SavedSearch/base"]);
    restore(&mut proxy, &seed);

    let update = proxy.process_request(graphql_request(
        r#"
        mutation SavedSearchStoreUpdate($input: SavedSearchUpdateInput!) {
          savedSearchUpdate(input: $input) {
            savedSearch { id name query resourceType }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "input": {
                "id": "gid://shopify/SavedSearch/base",
                "name": "Updated base products",
                "query": "tag:promo"
            }
        }),
    ));
    assert_eq!(update.status, 200);
    assert_eq!(
        update.body["data"]["savedSearchUpdate"]["savedSearch"]["name"],
        json!("Updated base products")
    );

    let create = proxy.process_request(graphql_request(
        r#"
        mutation SavedSearchStoreCreate($input: SavedSearchCreateInput!) {
          savedSearchCreate(input: $input) {
            savedSearch { id name query resourceType }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "input": {
                "name": "New products",
                "query": "tag:promo",
                "resourceType": "PRODUCT"
            }
        }),
    ));
    assert_eq!(create.status, 200);
    let new_id = create.body["data"]["savedSearchCreate"]["savedSearch"]["id"]
        .as_str()
        .expect("savedSearchCreate should return an id")
        .to_string();

    let delete = proxy.process_request(graphql_request(
        r#"
        mutation SavedSearchStoreDelete($input: SavedSearchDeleteInput!) {
          savedSearchDelete(input: $input) {
            deletedSavedSearchId
            userErrors { field message }
          }
        }
        "#,
        json!({ "input": { "id": "gid://shopify/SavedSearch/base" } }),
    ));
    assert_eq!(delete.status, 200);
    assert_eq!(
        delete.body["data"]["savedSearchDelete"]["deletedSavedSearchId"],
        json!("gid://shopify/SavedSearch/base")
    );

    let read = proxy.process_request(graphql_request(
        r#"
        query SavedSearchStoreRead {
          productSavedSearches(first: 10) {
            nodes { id name query resourceType }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(read.status, 200);
    assert_eq!(
        read.body["data"]["productSavedSearches"]["nodes"],
        json!([{
            "id": new_id,
            "name": "New products",
            "query": "tag:promo",
            "resourceType": "PRODUCT"
        }])
    );
}

#[test]
fn store_clear_staged_resets_overlays_and_tombstones_without_dropping_base() {
    let mut proxy = snapshot_proxy().with_base_products(vec![product(
        "gid://shopify/Product/base",
        "Base product",
        "base-product",
    )]);
    let update = proxy.process_request(graphql_request(
        r#"
        mutation ResetProductUpdate($product: ProductUpdateInput!) {
          productUpdate(product: $product) {
            product { id title handle }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "product": {
                "id": "gid://shopify/Product/base",
                "title": "Updated product",
                "handle": "updated-product"
            }
        }),
    ));
    assert_eq!(update.status, 200);

    let delete = proxy.process_request(graphql_request(
        r#"
        mutation ResetProductDelete($input: ProductDeleteInput!) {
          productDelete(input: $input) { deletedProductId userErrors { field message } }
        }
        "#,
        json!({ "input": { "id": "gid://shopify/Product/base" } }),
    ));
    assert_eq!(delete.status, 200);

    let saved_search = proxy.process_request(graphql_request(
        r#"
        mutation ResetSavedSearch($input: SavedSearchCreateInput!) {
          savedSearchCreate(input: $input) {
            savedSearch { id name }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "input": {
                "name": "Reset me",
                "query": "tag:temporary",
                "resourceType": "PRODUCT"
            }
        }),
    ));
    assert_eq!(saved_search.status, 200, "{}", saved_search.body);
    assert!(
        saved_search.body["data"]["savedSearchCreate"]["savedSearch"]
            .get("id")
            .is_some()
    );

    let reset = proxy.process_request(request("POST", "/__meta/reset", ""));
    assert_eq!(reset.status, 200);

    let read = proxy.process_request(graphql_request(
        r#"
        query ResetProductRead {
          product(id: "gid://shopify/Product/base") { id title handle }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        read.body["data"]["product"],
        json!({
            "id": "gid://shopify/Product/base",
            "title": "Base product",
            "handle": "base-product"
        })
    );
    let state = proxy.process_request(request("GET", "/__meta/state", ""));
    assert_eq!(state.status, 200);
    assert_eq!(state.body["stagedState"]["products"], json!({}));
    assert_eq!(state.body["stagedState"]["deletedProductIds"], json!([]));
    assert_eq!(state.body["stagedState"]["savedSearches"], json!({}));
    assert_eq!(
        state.body["stagedState"]["deletedSavedSearchIds"],
        json!([])
    );

    let saved_searches = proxy.process_request(graphql_request(
        r#"
        query ResetSavedSearchRead {
          productSavedSearches(first: 10) { nodes { id name } }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        saved_searches.body["data"]["productSavedSearches"]["nodes"],
        json!([])
    );
}

#[test]
fn store_dump_restore_round_trips_order_and_tombstones() {
    let mut proxy = snapshot_proxy();
    let mut body = dump(&mut proxy);
    body["state"]["baseState"]["products"] = json!({
        "gid://shopify/Product/base-1": product_state(
            "gid://shopify/Product/base-1",
            "Base one",
            "base-one"
        ),
        "gid://shopify/Product/base-2": product_state(
            "gid://shopify/Product/base-2",
            "Base two",
            "base-two"
        )
    });
    body["state"]["baseState"]["productOrder"] = json!([
        "gid://shopify/Product/base-1",
        "gid://shopify/Product/base-2"
    ]);
    body["state"]["stagedState"]["products"] = json!({
        "gid://shopify/Product/base-1": product_state(
            "gid://shopify/Product/base-1",
            "Updated one",
            "updated-one"
        ),
        "gid://shopify/Product/new": product_state(
            "gid://shopify/Product/new",
            "New product",
            "new-product"
        )
    });
    body["state"]["stagedState"]["productOrder"] =
        json!(["gid://shopify/Product/base-1", "gid://shopify/Product/new"]);
    body["state"]["stagedState"]["deletedProductIds"] = json!(["gid://shopify/Product/base-2"]);
    body["state"]["stagedState"]["savedSearches"] = json!({
        "gid://shopify/SavedSearch/new": {
            "id": "gid://shopify/SavedSearch/new",
            "name": "New products",
            "query": "tag:promo",
            "resourceType": "PRODUCT"
        }
    });
    body["state"]["stagedState"]["savedSearchOrder"] = json!(["gid://shopify/SavedSearch/new"]);
    body["state"]["stagedState"]["locations"] = json!({
        "gid://shopify/Location/live": {
            "id": "gid://shopify/Location/live",
            "name": "Live location"
        }
    });
    body["state"]["stagedState"]["locationOrder"] = json!(["gid://shopify/Location/live"]);
    body["state"]["stagedState"]["deletedLocationIds"] = json!(["gid://shopify/Location/deleted"]);
    body["state"]["stagedState"]["deliveryProfiles"] = json!({
        "gid://shopify/DeliveryProfile/live": {
            "id": "gid://shopify/DeliveryProfile/live",
            "name": "Live profile"
        }
    });
    body["state"]["stagedState"]["deliveryProfileOrder"] =
        json!(["gid://shopify/DeliveryProfile/live"]);
    body["state"]["stagedState"]["deletedDeliveryProfileIds"] =
        json!(["gid://shopify/DeliveryProfile/deleted"]);
    body["state"]["stagedState"]["storeCreditAccounts"] = json!({
        "gid://shopify/StoreCreditAccount/1": {
            "id": "gid://shopify/StoreCreditAccount/1"
        }
    });
    body["state"]["stagedState"]["storeCreditAccountOrder"] =
        json!(["gid://shopify/StoreCreditAccount/1"]);
    body["state"]["stagedState"]["b2bLocations"] = json!({
        "gid://shopify/CompanyLocation/1": {
            "id": "gid://shopify/CompanyLocation/1"
        }
    });
    body["state"]["stagedState"]["b2bLocationOrder"] = json!(["gid://shopify/CompanyLocation/1"]);
    body["state"]["stagedState"]["deletedCustomerIds"] = json!(["gid://shopify/Customer/deleted"]);
    body["state"]["stagedState"]["deletedCollectionIds"] =
        json!(["gid://shopify/Collection/deleted"]);
    body["state"]["baseState"]["segments"] = json!({
        "gid://shopify/Segment/base-1": {
            "id": "gid://shopify/Segment/base-1",
            "name": "Base segment one",
            "query": "number_of_orders >= 1"
        },
        "gid://shopify/Segment/base-2": {
            "id": "gid://shopify/Segment/base-2",
            "name": "Base segment two",
            "query": "number_of_orders >= 2"
        }
    });
    body["state"]["baseState"]["segmentOrder"] = json!([
        "gid://shopify/Segment/base-1",
        "gid://shopify/Segment/base-2"
    ]);
    body["state"]["stagedState"]["segments"] = json!({
        "gid://shopify/Segment/base-1": {
            "id": "gid://shopify/Segment/base-1",
            "name": "Updated segment one",
            "query": "number_of_orders >= 3"
        },
        "gid://shopify/Segment/new": {
            "id": "gid://shopify/Segment/new",
            "name": "New segment",
            "query": "customer_tags CONTAINS 'new'"
        }
    });
    body["state"]["stagedState"]["segmentOrder"] =
        json!(["gid://shopify/Segment/base-1", "gid://shopify/Segment/new"]);
    body["state"]["stagedState"]["deletedSegmentIds"] = json!(["gid://shopify/Segment/base-2"]);

    let mut restored = snapshot_proxy();
    restore(&mut restored, &body);
    let round_trip = dump(&mut restored);

    assert_eq!(
        round_trip["state"]["baseState"]["productOrder"],
        json!([
            "gid://shopify/Product/base-1",
            "gid://shopify/Product/base-2"
        ])
    );
    assert_eq!(
        round_trip["state"]["stagedState"]["productOrder"],
        json!(["gid://shopify/Product/base-1", "gid://shopify/Product/new"])
    );
    assert_eq!(
        round_trip["state"]["stagedState"]["deletedProductIds"],
        json!(["gid://shopify/Product/base-2"])
    );
    assert_eq!(
        round_trip["state"]["stagedState"]["savedSearchOrder"],
        json!(["gid://shopify/SavedSearch/new"])
    );
    assert_eq!(
        round_trip["state"]["stagedState"]["locationOrder"],
        json!(["gid://shopify/Location/live"])
    );
    assert_eq!(
        round_trip["state"]["stagedState"]["deletedLocationIds"],
        json!(["gid://shopify/Location/deleted"])
    );
    assert_eq!(
        round_trip["state"]["stagedState"]["deliveryProfileOrder"],
        json!(["gid://shopify/DeliveryProfile/live"])
    );
    assert_eq!(
        round_trip["state"]["stagedState"]["deletedDeliveryProfileIds"],
        json!(["gid://shopify/DeliveryProfile/deleted"])
    );
    assert_eq!(
        round_trip["state"]["stagedState"]["storeCreditAccountOrder"],
        json!(["gid://shopify/StoreCreditAccount/1"])
    );
    assert_eq!(
        round_trip["state"]["stagedState"]["b2bLocationOrder"],
        json!(["gid://shopify/CompanyLocation/1"])
    );
    assert_eq!(
        round_trip["state"]["stagedState"]["deletedCustomerIds"],
        json!(["gid://shopify/Customer/deleted"])
    );
    assert_eq!(
        round_trip["state"]["stagedState"]["deletedCollectionIds"],
        json!(["gid://shopify/Collection/deleted"])
    );
    assert_eq!(
        round_trip["state"]["baseState"]["segmentOrder"],
        json!([
            "gid://shopify/Segment/base-1",
            "gid://shopify/Segment/base-2"
        ])
    );
    assert_eq!(
        round_trip["state"]["stagedState"]["segmentOrder"],
        json!(["gid://shopify/Segment/base-1", "gid://shopify/Segment/new"])
    );
    assert_eq!(
        round_trip["state"]["stagedState"]["deletedSegmentIds"],
        json!(["gid://shopify/Segment/base-2"])
    );

    let products = restored.process_request(graphql_request(
        r#"
        query RestoredProductRead {
          products(first: 10) { nodes { id title handle } }
          productsCount { count precision }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        products.body["data"]["products"]["nodes"],
        json!([
            {
                "id": "gid://shopify/Product/base-1",
                "title": "Updated one",
                "handle": "updated-one"
            },
            {
                "id": "gid://shopify/Product/new",
                "title": "New product",
                "handle": "new-product"
            }
        ])
    );
    assert_eq!(
        products.body["data"]["productsCount"],
        json!({ "count": 2, "precision": "EXACT" })
    );

    let saved_searches = restored.process_request(graphql_request(
        r#"
        query RestoredSavedSearchRead {
          productSavedSearches(first: 10) { nodes { id name query resourceType } }
        }
        "#,
        json!({}),
    ));
    assert_eq!(saved_searches.status, 200);
    assert_eq!(
        saved_searches.body["data"]["productSavedSearches"]["nodes"],
        json!([{
            "id": "gid://shopify/SavedSearch/new",
            "name": "New products",
            "query": "tag:promo",
            "resourceType": "PRODUCT"
        }])
    );

    let segments = restored.process_request(graphql_request(
        r#"
        query RestoredSegmentRead {
          segments(first: 10, sortKey: ID) { nodes { id name query } }
          segmentsCount { count precision }
        }
        "#,
        json!({}),
    ));
    assert_eq!(segments.status, 200);
    assert_eq!(
        segments.body["data"]["segments"]["nodes"],
        json!([
            {
                "id": "gid://shopify/Segment/base-1",
                "name": "Updated segment one",
                "query": "number_of_orders >= 3"
            },
            {
                "id": "gid://shopify/Segment/new",
                "name": "New segment",
                "query": "customer_tags CONTAINS 'new'"
            }
        ])
    );
    assert_eq!(
        segments.body["data"]["segmentsCount"],
        json!({ "count": 2, "precision": "EXACT" })
    );
}

#[test]
fn state_version_header_advances_on_mutation_and_holds_on_reads() {
    let mut proxy = snapshot_proxy();

    let version_of = |response: &Response| {
        response
            .headers
            .get("x-sdp-state-version")
            .cloned()
            .expect("every response should carry x-sdp-state-version")
    };

    let baseline = proxy.process_request(Request {
        method: "GET".to_string(),
        path: "/__meta/health".to_string(),
        headers: BTreeMap::new(),
        body: String::new(),
    });
    let baseline_version = version_of(&baseline);

    let create = proxy.process_request(graphql_request(
        r#"
        mutation ProductCreate($product: ProductInput!) {
          productCreate(product: $product) {
            product { id }
            userErrors { field message }
          }
        }
        "#,
        json!({ "product": { "title": "Versioned", "handle": "versioned" } }),
    ));
    let after_create = version_of(&create);
    assert_ne!(
        after_create, baseline_version,
        "a staged mutation must advance the state version"
    );

    // A pure read must not advance the version, so embedders skip persisting.
    let read = proxy.process_request(Request {
        method: "GET".to_string(),
        path: "/__meta/state".to_string(),
        headers: BTreeMap::new(),
        body: String::new(),
    });
    assert_eq!(
        version_of(&read),
        after_create,
        "reads must leave the state version unchanged"
    );

    // Reset returns the version to its pristine baseline.
    let reset = proxy.process_request(Request {
        method: "POST".to_string(),
        path: "/__meta/reset".to_string(),
        headers: BTreeMap::new(),
        body: String::new(),
    });
    assert_eq!(
        version_of(&reset),
        baseline_version,
        "reset must return the state version to baseline"
    );
}

#[test]
fn product_create_with_product_options_defaults_inventory_item_to_untracked() {
    let mut proxy = snapshot_proxy();

    let create = proxy.process_request(graphql_request(
        r#"
        mutation ProductOptionsCreate($product: ProductCreateInput!) {
          productCreate(product: $product) {
            product {
              id
              totalInventory
              tracksInventory
              options {
                name
                values
              }
              variants(first: 1) {
                nodes {
                  title
                  selectedOptions {
                    name
                    value
                  }
                  inventoryItem {
                    tracked
                    requiresShipping
                  }
                }
              }
            }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "product": {
                "title": "Options Product",
                "handle": "options-product",
                "productOptions": [
                    { "name": "Color", "values": [{ "name": "Red" }] }
                ]
            }
        }),
    ));
    assert_eq!(create.status, 200);
    assert_eq!(
        create.body["data"]["productCreate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        create.body["data"]["productCreate"]["product"]["variants"]["nodes"][0],
        json!({
            "title": "Red",
            "selectedOptions": [{ "name": "Color", "value": "Red" }],
            "inventoryItem": {
                "tracked": false,
                "requiresShipping": true
            }
        })
    );
    assert_eq!(
        create.body["data"]["productCreate"]["product"]["tracksInventory"],
        json!(false)
    );
    assert_eq!(
        create.body["data"]["productCreate"]["product"]["totalInventory"],
        json!(0)
    );
}

#[test]
fn conditional_directives_skip_root_mutations_and_project_nested_reads() {
    let mut proxy = snapshot_proxy();

    let skipped_create = proxy.process_request(graphql_request(
        r#"
        mutation ConditionalCreate($product: ProductInput!, $skipCreate: Boolean!) {
          skippedCreate: productCreate(product: $product) @include(if: true) @skip(if: $skipCreate) {
            product { id title }
            userErrors { message }
          }
        }
        "#,
        json!({
            "product": {
                "title": "Skipped product",
                "handle": "skipped-product"
            },
            "skipCreate": true
        }),
    ));
    assert_eq!(skipped_create.status, 200);
    assert_eq!(skipped_create.body, json!({ "data": {} }));

    let empty_log = proxy.process_request(request("GET", "/__meta/log", ""));
    assert_eq!(empty_log.status, 200);
    assert_eq!(empty_log.body["entries"], json!([]));

    let create = proxy.process_request(graphql_request(
        r#"
        mutation CreateIncludedProduct($product: ProductInput!) {
          productCreate(product: $product) {
            product { id title handle status seo { title description } }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "product": {
                "title": "Included product",
                "handle": "included-product",
                "seo": {
                    "title": "Visible SEO",
                    "description": "Hidden SEO"
                }
            }
        }),
    ));
    assert_eq!(create.status, 200);
    let product_id = create.body["data"]["productCreate"]["product"]["id"]
        .as_str()
        .expect("included product should be staged")
        .to_string();
    assert_eq!(
        product_id, "gid://shopify/Product/1?shopify-draft-proxy=synthetic",
        "skipped mutation must not allocate synthetic ids"
    );

    let read = proxy.process_request(graphql_request(
        r#"
        fragment ProductBits on Product {
          aliasStatus: status
          seo @skip(if: true) {
            title
            description
          }
        }

        query ConditionalRead(
          $id: ID!,
          $includeTitle: Boolean!,
          $skipInline: Boolean!,
          $includeSpread: Boolean!
        ) {
          skippedRoot: product(id: $id) @include(if: false) {
            id
          }
          product(id: $id) {
            id
            title @include(if: $includeTitle)
            hiddenHandle: handle @skip(if: true)
            ... on Product @skip(if: $skipInline) {
              vendor
            }
            ...ProductBits @include(if: $includeSpread)
          }
        }
        "#,
        json!({
            "id": product_id,
            "includeTitle": false,
            "skipInline": true,
            "includeSpread": true
        }),
    ));
    assert_eq!(read.status, 200);
    assert_eq!(
        read.body,
        json!({
            "data": {
                "product": {
                    "id": "gid://shopify/Product/1?shopify-draft-proxy=synthetic",
                    "aliasStatus": "ACTIVE"
                }
            }
        })
    );

    let log = proxy.process_request(request("GET", "/__meta/log", ""));
    assert_eq!(log.body["entries"].as_array().unwrap().len(), 1);
}

#[test]
fn product_downstream_read_uses_staged_store_instead_of_operation_name_fixture() {
    let mut proxy = snapshot_proxy();
    let create = proxy.process_request(graphql_request(
        r#"
        mutation ProductCreateParityPlan($product: ProductInput!) {
            productCreate(product: $product) {
              product {
                id
                variants(first: 1) { nodes { id } }
              }
            userErrors {
              field
              message
            }
          }
        }
        "#,
        json!({
            "product": {
                "title": "Store backed product",
                "handle": "store-backed-product",
                "vendor": "Hermes",
                "productType": "Proof",
                "tags": ["local", "store"],
                "seo": {
                    "title": "Store SEO",
                    "description": "Projected from store"
                }
            }
        }),
    ));
    let id = create.body["data"]["productCreate"]["product"]["id"]
        .as_str()
        .expect("productCreate should return a staged product id")
        .to_string();

    let read = proxy.process_request(graphql_request(
        r#"
        query ProductDetailRead($id: ID!) {
          product(id: $id) {
            id
            title
            handle
            vendor
            productType
            tags
            totalInventory
            tracksInventory
            onlineStorePreviewUrl
            category {
              id
              fullName
            }
            seo {
              title
              description
            }
            variants(first: 2) {
              nodes {
                id
              }
              pageInfo {
                hasNextPage
                hasPreviousPage
                startCursor
                endCursor
              }
            }
            metafield(namespace: "custom", key: "material") {
              value
            }
          }
        }
        "#,
        json!({ "id": id }),
    ));

    assert_eq!(read.status, 200);
    assert_eq!(read.body["data"]["product"]["id"], json!(id));
    assert_eq!(
        read.body["data"]["product"]["title"],
        json!("Store backed product")
    );
    assert_eq!(
        read.body["data"]["product"]["handle"],
        json!("store-backed-product")
    );
    assert_eq!(read.body["data"]["product"]["vendor"], json!("Hermes"));
    assert_eq!(read.body["data"]["product"]["productType"], json!("Proof"));
    assert_eq!(
        read.body["data"]["product"]["tags"],
        json!(["local", "store"])
    );
    assert_eq!(read.body["data"]["product"]["totalInventory"], json!(0));
    assert_eq!(
        read.body["data"]["product"]["tracksInventory"],
        json!(false)
    );
    assert_eq!(
        read.body["data"]["product"]["onlineStorePreviewUrl"],
        Value::Null
    );
    assert_eq!(read.body["data"]["product"]["category"], Value::Null);
    assert_eq!(
        read.body["data"]["product"]["seo"],
        json!({ "title": "Store SEO", "description": "Projected from store" })
    );
    assert_eq!(
        read.body["data"]["product"]["variants"]["pageInfo"],
        json!({
            "hasNextPage": false,
            "hasPreviousPage": false,
            "startCursor": "gid://shopify/ProductVariant/2?shopify-draft-proxy=synthetic",
            "endCursor": "gid://shopify/ProductVariant/2?shopify-draft-proxy=synthetic"
        })
    );
    assert_eq!(
        read.body["data"]["product"]["variants"]["nodes"],
        json!([{ "id": "gid://shopify/ProductVariant/2?shopify-draft-proxy=synthetic" }])
    );
    assert_eq!(read.body["data"]["product"]["metafield"], Value::Null);
}

#[test]
fn product_read_passthroughs_in_live_hybrid_when_there_is_no_local_overlay_state() {
    let upstream_body = json!({
        "data": {
            "product": {
                "id": "gid://shopify/Product/upstream",
                "title": "Upstream product"
            }
        }
    });
    let mut proxy = DraftProxy::new(Config {
        read_mode: ReadMode::LiveHybrid,
        unsupported_mutation_mode: Some(UnsupportedMutationMode::Passthrough),
        bulk_operation_run_mutation_max_input_file_size_bytes: None,
        port: 0,
        shopify_admin_origin: "https://shopify.com".to_string(),
        snapshot_path: None,
    })
    .with_upstream_transport({
        let upstream_body = upstream_body.clone();
        move |_| ok_json(upstream_body.clone())
    });

    let response = proxy.process_request(graphql_request(
        r#"
        query ProductDetailRead($id: ID!) {
          product(id: $id) {
            id
            title
          }
        }
        "#,
        json!({ "id": "gid://shopify/Product/upstream" }),
    ));

    assert_eq!(response.status, 200);
    assert_eq!(response.body, upstream_body);
}

#[test]
fn top_level_collections_reflect_staged_collection_lifecycle() {
    let mut proxy = snapshot_proxy();

    let first = proxy.process_request(graphql_request(
        r#"
        mutation CollectionLifecycleCreateFirst($input: CollectionInput!) {
          collectionCreate(input: $input) {
            collection {
              id
              title
              handle
            }
            userErrors {
              field
              message
            }
          }
        }
        "#,
        json!({ "input": { "title": "Alpha Collection", "handle": "alpha-collection" } }),
    ));
    assert_eq!(first.status, 200);
    let first_id = first.body["data"]["collectionCreate"]["collection"]["id"]
        .as_str()
        .expect("first collection should have an id")
        .to_string();

    let second = proxy.process_request(graphql_request(
        r#"
        mutation CollectionLifecycleCreateSecond($input: CollectionInput!) {
          collectionCreate(input: $input) {
            collection {
              id
            }
            userErrors {
              field
              message
            }
          }
        }
        "#,
        json!({ "input": { "title": "Beta Collection", "handle": "beta-collection" } }),
    ));
    assert_eq!(second.status, 200);
    let second_id = second.body["data"]["collectionCreate"]["collection"]["id"]
        .as_str()
        .expect("second collection should have an id")
        .to_string();

    let initial_read = proxy.process_request(graphql_request(
        r#"
        query CollectionLifecycleInitialRead($titleQuery: String!, $handleQuery: String!) {
          titleMatches: collections(first: 10, query: $titleQuery, sortKey: TITLE) {
            nodes {
              id
              title
              handle
              updatedAt
            }
            pageInfo {
              hasNextPage
              hasPreviousPage
              startCursor
              endCursor
            }
          }
          handleMatches: collections(first: 10, query: $handleQuery) {
            nodes {
              id
              title
              handle
            }
          }
          titleCount: collectionsCount(query: $titleQuery) {
            count
            precision
          }
        }
        "#,
        json!({
            "titleQuery": "title:Alpha*",
            "handleQuery": "handle:alpha-collection"
        }),
    ));
    assert_eq!(initial_read.status, 200);
    assert_eq!(
        initial_read.body["data"]["titleMatches"]["nodes"],
        json!([{
            "id": first_id,
            "title": "Alpha Collection",
            "handle": "alpha-collection",
            "updatedAt": "2024-01-01T00:00:01.000Z"
        }])
    );
    assert_eq!(
        initial_read.body["data"]["titleMatches"]["pageInfo"],
        json!({
            "hasNextPage": false,
            "hasPreviousPage": false,
            "startCursor": first_id,
            "endCursor": first_id
        })
    );
    assert_eq!(
        initial_read.body["data"]["handleMatches"]["nodes"],
        json!([{
            "id": first_id,
            "title": "Alpha Collection",
            "handle": "alpha-collection"
        }])
    );
    assert_eq!(
        initial_read.body["data"]["titleCount"],
        json!({ "count": 1, "precision": "EXACT" })
    );

    let update = proxy.process_request(graphql_request(
        r#"
        mutation CollectionLifecycleUpdate($input: CollectionInput!) {
          collectionUpdate(input: $input) {
            collection {
              id
              title
              handle
              updatedAt
            }
            userErrors {
              field
              message
            }
          }
        }
        "#,
        json!({
            "input": {
                "id": first_id,
                "title": "Gamma Collection",
                "handle": "alpha-collection-updated"
            }
        }),
    ));
    assert_eq!(update.status, 200);
    assert_eq!(
        update.body["data"]["collectionUpdate"]["collection"]["updatedAt"],
        json!("2024-01-01T00:00:03.000Z")
    );

    let update_read = proxy.process_request(graphql_request(
        r#"
        query CollectionLifecycleUpdatedRead($oldTitleQuery: String!, $oldHandleQuery: String!, $newHandleQuery: String!) {
          oldTitleMatches: collections(first: 10, query: $oldTitleQuery) {
            nodes {
              id
            }
          }
          oldHandleMatches: collections(first: 10, query: $oldHandleQuery) {
            nodes {
              id
            }
          }
          newHandleMatches: collections(first: 10, query: $newHandleQuery) {
            nodes {
              id
              title
              handle
              updatedAt
            }
          }
        }
        "#,
        json!({
            "oldTitleQuery": "title:Alpha*",
            "oldHandleQuery": "handle:alpha-collection",
            "newHandleQuery": "handle:alpha-collection-updated"
        }),
    ));
    assert_eq!(update_read.status, 200);
    assert_eq!(
        update_read.body["data"]["oldTitleMatches"]["nodes"],
        json!([])
    );
    assert_eq!(
        update_read.body["data"]["oldHandleMatches"]["nodes"],
        json!([])
    );
    assert_eq!(
        update_read.body["data"]["newHandleMatches"]["nodes"],
        json!([{
            "id": first_id,
            "title": "Gamma Collection",
            "handle": "alpha-collection-updated",
            "updatedAt": "2024-01-01T00:00:03.000Z"
        }])
    );

    let delete = proxy.process_request(graphql_request(
        r#"
        mutation CollectionLifecycleDelete($input: CollectionDeleteInput!) {
          collectionDelete(input: $input) {
            deletedCollectionId
            userErrors {
              field
              message
            }
          }
        }
        "#,
        json!({ "input": { "id": second_id } }),
    ));
    assert_eq!(delete.status, 200);
    assert_eq!(
        delete.body["data"]["collectionDelete"]["deletedCollectionId"],
        json!(second_id)
    );

    let delete_read = proxy.process_request(graphql_request(
        r#"
        query CollectionLifecycleDeleteRead {
          collections(first: 10) {
            nodes {
              id
              title
            }
          }
          collectionsCount {
            count
            precision
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(delete_read.status, 200);
    assert_eq!(
        delete_read.body["data"]["collections"]["nodes"],
        json!([{ "id": first_id, "title": "Gamma Collection" }])
    );
    assert_eq!(
        delete_read.body["data"]["collectionsCount"],
        json!({ "count": 1, "precision": "EXACT" })
    );
}

#[test]
fn top_level_collections_honor_sort_reverse_cursors_and_limited_counts() {
    let mut proxy = snapshot_proxy();
    let mut ids = Vec::new();
    for (title, handle) in [
        ("Bravo Collection", "bravo-collection"),
        ("Alpha Collection", "alpha-collection"),
        ("Charlie Collection", "charlie-collection"),
    ] {
        let create = proxy.process_request(graphql_request(
            r#"
            mutation CollectionConnectionCreate($input: CollectionInput!) {
              collectionCreate(input: $input) {
                collection {
                  id
                }
                userErrors {
                  field
                  message
                }
              }
            }
            "#,
            json!({ "input": { "title": title, "handle": handle } }),
        ));
        assert_eq!(create.status, 200);
        ids.push(
            create.body["data"]["collectionCreate"]["collection"]["id"]
                .as_str()
                .expect("collection should have id")
                .to_string(),
        );
    }

    let first_page = proxy.process_request(graphql_request(
        r#"
        query CollectionConnectionFirstPage {
          collections(first: 2) {
            edges {
              cursor
              node {
                id
                title
              }
            }
            pageInfo {
              hasNextPage
              hasPreviousPage
              startCursor
              endCursor
            }
          }
          collectionsCount(limit: 2) {
            count
            precision
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(first_page.status, 200);
    assert_eq!(
        first_page.body["data"]["collections"]["edges"],
        json!([
            { "cursor": ids[0], "node": { "id": ids[0], "title": "Bravo Collection" } },
            { "cursor": ids[1], "node": { "id": ids[1], "title": "Alpha Collection" } }
        ])
    );
    assert_eq!(
        first_page.body["data"]["collections"]["pageInfo"],
        json!({
            "hasNextPage": true,
            "hasPreviousPage": false,
            "startCursor": ids[0],
            "endCursor": ids[1]
        })
    );
    assert_eq!(
        first_page.body["data"]["collectionsCount"],
        json!({ "count": 2, "precision": "AT_LEAST" })
    );

    let after_page = proxy.process_request(graphql_request(
        r#"
        query CollectionConnectionAfter($after: String!) {
          collections(first: 2, after: $after) {
            nodes {
              id
              title
            }
            pageInfo {
              hasNextPage
              hasPreviousPage
              startCursor
              endCursor
            }
          }
        }
        "#,
        json!({ "after": ids[1] }),
    ));
    assert_eq!(after_page.status, 200);
    assert_eq!(
        after_page.body["data"]["collections"]["nodes"],
        json!([{ "id": ids[2], "title": "Charlie Collection" }])
    );
    assert_eq!(
        after_page.body["data"]["collections"]["pageInfo"],
        json!({
            "hasNextPage": false,
            "hasPreviousPage": true,
            "startCursor": ids[2],
            "endCursor": ids[2]
        })
    );

    let title_reverse = proxy.process_request(graphql_request(
        r#"
        query CollectionConnectionTitleReverse {
          collections(first: 3, sortKey: TITLE, reverse: true) {
            nodes {
              title
            }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(title_reverse.status, 200);
    assert_eq!(
        title_reverse.body["data"]["collections"]["nodes"],
        json!([
            { "title": "Charlie Collection" },
            { "title": "Bravo Collection" },
            { "title": "Alpha Collection" }
        ])
    );

    let update = proxy.process_request(graphql_request(
        r#"
        mutation CollectionConnectionUpdate($input: CollectionInput!) {
          collectionUpdate(input: $input) {
            collection {
              id
              updatedAt
            }
            userErrors {
              field
              message
            }
          }
        }
        "#,
        json!({ "input": { "id": ids[1], "title": "Alpha Updated Collection" } }),
    ));
    assert_eq!(update.status, 200);

    let updated_filter = proxy.process_request(graphql_request(
        r#"
        query CollectionConnectionUpdatedFilter($query: String!) {
          collections(first: 10, query: $query, sortKey: UPDATED_AT, reverse: true) {
            nodes {
              id
              title
              updatedAt
            }
          }
        }
        "#,
        json!({ "query": "updated_at:>=2024-01-01T00:00:03.000Z" }),
    ));
    assert_eq!(updated_filter.status, 200);
    assert_eq!(
        updated_filter.body["data"]["collections"]["nodes"],
        json!([
            {
                "id": ids[1],
                "title": "Alpha Updated Collection",
                "updatedAt": "2024-01-01T00:00:04.000Z"
            },
            {
                "id": ids[2],
                "title": "Charlie Collection",
                "updatedAt": "2024-01-01T00:00:03.000Z"
            }
        ])
    );
}

#[test]
fn top_level_collections_live_hybrid_overlays_observed_upstream_state() {
    let upstream_body = json!({
        "data": {
            "collections": {
                "nodes": [
                    {
                        "id": "gid://shopify/Collection/901",
                        "title": "Local Staged Collection",
                        "handle": "local-staged-collection",
                        "updatedAt": "2024-01-01T00:00:00.000Z",
                        "products": { "nodes": [] }
                    },
                    {
                        "id": "gid://shopify/Collection/900",
                        "title": "Upstream Base Collection",
                        "handle": "upstream-base-collection",
                        "updatedAt": "2024-01-01T00:00:00.000Z",
                        "products": { "nodes": [] }
                    }
                ],
                "pageInfo": {
                    "hasNextPage": false,
                    "hasPreviousPage": false,
                    "startCursor": "gid://shopify/Collection/900",
                    "endCursor": "gid://shopify/Collection/900"
                }
            }
        }
    });
    let mut proxy = DraftProxy::new(Config {
        read_mode: ReadMode::LiveHybrid,
        unsupported_mutation_mode: Some(UnsupportedMutationMode::Passthrough),
        bulk_operation_run_mutation_max_input_file_size_bytes: None,
        port: 0,
        shopify_admin_origin: "https://shopify.com".to_string(),
        snapshot_path: None,
    })
    .with_upstream_transport({
        let upstream_body = upstream_body.clone();
        move |_| ok_json(upstream_body.clone())
    });

    let create = proxy.process_request(graphql_request(
        r#"
        mutation CollectionLiveHybridCreate($input: CollectionInput!) {
          collectionCreate(input: $input) {
            collection {
              id
            }
            userErrors {
              field
              message
            }
          }
        }
        "#,
        json!({ "input": { "title": "Local Staged Collection", "handle": "local-staged-collection" } }),
    ));
    assert_eq!(create.status, 200);
    let staged_id = create.body["data"]["collectionCreate"]["collection"]["id"]
        .as_str()
        .expect("staged collection should have id")
        .to_string();

    let read = proxy.process_request(graphql_request(
        r#"
        query CollectionLiveHybridRead {
          collections(first: 10, sortKey: TITLE) {
            nodes {
              id
              title
              handle
            }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(read.status, 200);
    assert_eq!(
        read.body["data"]["collections"]["nodes"],
        json!([
            {
                "id": staged_id,
                "title": "Local Staged Collection",
                "handle": "local-staged-collection"
            },
            {
                "id": "gid://shopify/Collection/900",
                "title": "Upstream Base Collection",
                "handle": "upstream-base-collection"
            }
        ])
    );
}

#[test]
fn collection_handle_lookups_forward_after_staged_collection_miss() {
    let upstream_requests = Arc::new(Mutex::new(Vec::<Value>::new()));
    let captured_requests = Arc::clone(&upstream_requests);
    let mut proxy = DraftProxy::new(Config {
        read_mode: ReadMode::LiveHybrid,
        unsupported_mutation_mode: Some(UnsupportedMutationMode::Passthrough),
        bulk_operation_run_mutation_max_input_file_size_bytes: None,
        port: 0,
        shopify_admin_origin: "https://shopify.com".to_string(),
        snapshot_path: None,
    })
    .with_upstream_transport(move |request| {
        let body: Value = serde_json::from_str(&request.body).expect("upstream body parses");
        captured_requests.lock().unwrap().push(body);
        ok_json(json!({
            "data": {
                "byHandle": {
                    "id": "gid://shopify/Collection/494967390514",
                    "title": "Automated Collection",
                    "handle": "automated-collection"
                },
                "byIdentifier": {
                    "id": "gid://shopify/Collection/494967390515",
                    "title": "Home page",
                    "handle": "frontpage"
                }
            }
        }))
    });

    let create = proxy.process_request(graphql_request(
        r#"
        mutation CreateCollection($input: CollectionInput!) {
          collectionCreate(input: $input) {
            collection { id }
            userErrors { field message }
          }
        }
        "#,
        json!({ "input": { "title": "Summer Sale", "handle": "summer-sale" } }),
    ));
    assert_eq!(create.status, 200);
    assert_eq!(upstream_requests.lock().unwrap().len(), 0);

    let read = proxy.process_request(graphql_request(
        r#"
        query CollectionHandleLookupMisses {
          byIdentifier: collectionByIdentifier(identifier: { handle: "frontpage" }) {
            id
            title
            handle
          }
          byHandle: collectionByHandle(handle: "automated-collection") {
            id
            title
            handle
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(read.status, 200);
    assert_eq!(
        read.body["data"]["byHandle"],
        json!({
            "id": "gid://shopify/Collection/494967390514",
            "title": "Automated Collection",
            "handle": "automated-collection"
        })
    );
    assert_eq!(
        read.body["data"]["byIdentifier"],
        json!({
            "id": "gid://shopify/Collection/494967390515",
            "title": "Home page",
            "handle": "frontpage"
        })
    );
    assert_eq!(upstream_requests.lock().unwrap().len(), 1);

    let cached = proxy.process_request(graphql_request(
        r#"
        query CollectionHandleLookupCached {
          byHandle: collectionByHandle(handle: "automated-collection") { id title handle }
          byIdentifier: collectionByIdentifier(identifier: { handle: "frontpage" }) { id title handle }
        }
        "#,
        json!({}),
    ));
    assert_eq!(cached.status, 200);
    assert_eq!(cached.body["data"], read.body["data"]);
    assert_eq!(
        upstream_requests.lock().unwrap().len(),
        1,
        "observed upstream collections should satisfy subsequent handle reads locally"
    );
}

#[test]
fn collections_count_uses_upstream_total_with_staged_delta() {
    let upstream_requests = Arc::new(Mutex::new(Vec::<Value>::new()));
    let captured_requests = Arc::clone(&upstream_requests);
    let mut proxy = DraftProxy::new(Config {
        read_mode: ReadMode::LiveHybrid,
        unsupported_mutation_mode: Some(UnsupportedMutationMode::Passthrough),
        bulk_operation_run_mutation_max_input_file_size_bytes: None,
        port: 0,
        shopify_admin_origin: "https://shopify.com".to_string(),
        snapshot_path: None,
    })
    .with_upstream_transport(move |request| {
        let body: Value = serde_json::from_str(&request.body).expect("upstream body parses");
        captured_requests.lock().unwrap().push(body);
        ok_json(json!({
            "data": {
                "catalogTotal": {
                    "total": 3,
                    "accuracy": "EXACT"
                }
            }
        }))
    });

    let create = proxy.process_request(graphql_request(
        r#"
        mutation CreateCollection($input: CollectionInput!) {
          collectionCreate(input: $input) {
            collection { id }
            userErrors { field message }
          }
        }
        "#,
        json!({ "input": { "title": "Summer Sale", "handle": "summer-sale" } }),
    ));
    assert_eq!(create.status, 200);

    let count = proxy.process_request(graphql_request(
        r#"
        query CollectionCountAfterCreate {
          catalogTotal: collectionsCount(limit: 10) {
            total: count
            accuracy: precision
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(count.status, 200);
    assert_eq!(
        count.body["data"]["catalogTotal"],
        json!({ "total": 4, "accuracy": "EXACT" })
    );
    assert_eq!(upstream_requests.lock().unwrap().len(), 1);
}

#[test]
fn collections_live_hybrid_hydrates_identity_when_selection_omits_id() {
    let upstream_requests = Arc::new(Mutex::new(Vec::<Value>::new()));
    let captured_requests = Arc::clone(&upstream_requests);
    let mut proxy = DraftProxy::new(Config {
        read_mode: ReadMode::LiveHybrid,
        unsupported_mutation_mode: Some(UnsupportedMutationMode::Passthrough),
        bulk_operation_run_mutation_max_input_file_size_bytes: None,
        port: 0,
        shopify_admin_origin: "https://shopify.com".to_string(),
        snapshot_path: None,
    })
    .with_upstream_transport(move |request| {
        let body: Value = serde_json::from_str(&request.body).expect("upstream body parses");
        let operation_name = body
            .get("operationName")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        captured_requests.lock().unwrap().push(body);
        if operation_name == "CollectionsIdentityHydrate" {
            ok_json(json!({
                "data": {
                    "collections": {
                        "nodes": [{
                            "id": "gid://shopify/Collection/900",
                            "title": "Upstream Base Collection",
                            "handle": "upstream-base-collection",
                            "createdAt": "2024-01-01T00:00:00.000Z",
                            "updatedAt": "2024-01-01T00:00:00.000Z",
                            "sortOrder": "BEST_SELLING",
                            "ruleSet": null,
                            "productsCount": { "count": 0, "precision": "EXACT" }
                        }]
                    }
                }
            }))
        } else {
            ok_json(json!({
                "data": {
                    "collections": {
                        "nodes": [{
                            "title": "Upstream Base Collection"
                        }],
                        "pageInfo": {
                            "hasNextPage": false,
                            "hasPreviousPage": false,
                            "startCursor": null,
                            "endCursor": null
                        }
                    }
                }
            }))
        }
    });

    let create = proxy.process_request(graphql_request(
        r#"
        mutation CreateCollection($input: CollectionInput!) {
          collectionCreate(input: $input) {
            collection { id }
            userErrors { field message }
          }
        }
        "#,
        json!({ "input": { "title": "Local Staged Collection", "handle": "local-staged-collection" } }),
    ));
    assert_eq!(create.status, 200);

    let read = proxy.process_request(graphql_request(
        r#"
        query CollectionReadWithoutIds {
          collections(first: 10) {
            nodes {
              title
            }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(read.status, 200);
    assert_eq!(
        read.body["data"]["collections"]["nodes"],
        json!([
            { "title": "Local Staged Collection" },
            { "title": "Upstream Base Collection" }
        ])
    );
    let requests = upstream_requests.lock().unwrap();
    assert_eq!(requests.len(), 2);
    assert_eq!(
        requests[1]["operationName"],
        json!("CollectionsIdentityHydrate"),
        "idless collection selections should trigger an identity hydrate"
    );
}

#[test]
fn product_variant_downstream_read_uses_staged_variant_state() {
    let mut proxy = snapshot_proxy();

    let create_product = proxy.process_request(graphql_request(
        r#"
        mutation ProductVariantUpdateSetupProduct($product: ProductCreateInput!) {
          productCreate(product: $product) {
            product {
              id
            }
            userErrors {
              field
              message
            }
          }
        }
        "#,
        json!({
            "product": {
                "title": "Store Variant Product",
                "status": "ACTIVE",
                "productOptions": [{
                    "name": "Color",
                    "values": [{ "name": "Store Red" }]
                }]
            }
        }),
    ));
    let product_id = create_product.body["data"]["productCreate"]["product"]["id"]
        .as_str()
        .expect("product create should return product id")
        .to_string();

    let create_variant = proxy.process_request(graphql_request(
        r#"
        mutation ProductVariantUpdateSetupVariant(
          $productId: ID!
          $variants: [ProductVariantsBulkInput!]!
        ) {
          productVariantsBulkCreate(productId: $productId, variants: $variants) {
            productVariants { id }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "productId": product_id,
            "variants": [{
                "optionValues": [{ "optionName": "Color", "name": "Store Red" }],
                "inventoryItem": {
                    "sku": "STORE-DRAFT",
                    "tracked": false,
                    "requiresShipping": true
                }
            }]
        }),
    ));
    let variant_id = create_variant.body["data"]["productVariantsBulkCreate"]["productVariants"][0]
        ["id"]
        .as_str()
        .expect("bulk create should return a variant id")
        .to_string();

    let update = proxy.process_request(graphql_request(
        r#"
        mutation ProductVariantUpdateParityPlan($productId: ID!, $variants: [ProductVariantsBulkInput!]!) {
          productVariantsBulkUpdate(productId: $productId, variants: $variants) {
            product {
              id
              totalInventory
              tracksInventory
              variants(first: 10) {
                nodes {
                  id
                  title
                  sku
                }
              }
            }
            productVariants {
              id
              title
              sku
              barcode
              selectedOptions {
                name
                value
              }
              inventoryItem {
                id
                tracked
                requiresShipping
              }
            }
            userErrors {
              field
              message
            }
          }
        }
        "#,
        json!({
            "productId": product_id,
            "variants": [{
                "id": variant_id,
                "barcode": "store-barcode",
                "optionValues": [{ "optionName": "Color", "name": "Store Red" }],
                "inventoryItem": {
                    "sku": "STORE-RED",
                    "tracked": true,
                    "requiresShipping": false
                }
            }]
        }),
    ));
    assert_eq!(
        update.body["data"]["productVariantsBulkUpdate"]["product"]["id"],
        json!(product_id)
    );

    let read = proxy.process_request(graphql_request(
        r#"
        query ProductVariantUpdateDownstreamRead($id: ID!, $query: String!) {
          product(id: $id) {
            id
            totalInventory
            tracksInventory
            variants(first: 10) {
              nodes {
                id
                title
                sku
                barcode
                selectedOptions {
                  name
                  value
                }
                inventoryItem {
                  id
                  tracked
                  requiresShipping
                }
              }
            }
          }
          products(first: 10, query: $query) {
            nodes {
              id
            }
          }
          skuCount: productsCount(query: $query) {
            count
            precision
          }
        }
        "#,
        json!({ "id": product_id, "query": "sku:STORE-RED" }),
    ));

    assert_eq!(read.status, 200);
    assert_eq!(read.body["data"]["product"]["id"], json!(product_id));
    assert_eq!(read.body["data"]["product"]["tracksInventory"], json!(true));
    let updated_variant = read.body["data"]["product"]["variants"]["nodes"]
        .as_array()
        .and_then(|variants| {
            variants
                .iter()
                .find(|variant| variant.get("id") == Some(&json!(variant_id)))
        })
        .expect("updated variant should be present in product variants");
    assert_eq!(updated_variant["title"], json!("Store Red"));
    assert_eq!(updated_variant["sku"], json!("STORE-RED"));
    assert_eq!(
        updated_variant["inventoryItem"]["requiresShipping"],
        json!(false)
    );
    assert_eq!(
        read.body["data"]["products"]["nodes"],
        json!([{ "id": product_id }])
    );
    assert_eq!(
        read.body["data"]["skuCount"],
        json!({ "count": 1, "precision": "EXACT" })
    );
}

#[test]
fn collection_downstream_read_uses_observed_passthrough_membership_state() {
    let mut proxy = snapshot_proxy().with_base_products(vec![
        ProductRecord {
            id: "gid://shopify/Product/first".to_string(),
            title: "First Product".to_string(),
            handle: "first-product".to_string(),
            status: "ACTIVE".to_string(),
            ..ProductRecord::default()
        },
        ProductRecord {
            id: "gid://shopify/Product/second".to_string(),
            title: "Second Product".to_string(),
            handle: "second-product".to_string(),
            status: "ACTIVE".to_string(),
            ..ProductRecord::default()
        },
    ]);

    let create = proxy.process_request(graphql_request(
        r#"
        mutation CollectionCreateForDownstreamRead($input: CollectionInput!) {
          collectionCreate(input: $input) {
            collection {
              id
            }
            userErrors {
              field
              message
            }
          }
        }
        "#,
        json!({
            "input": {
                "title": "Store Backed Collection",
                "handle": "store-backed-collection",
                "sortOrder": "MANUAL"
            }
        }),
    ));
    assert_eq!(create.status, 200);
    assert_eq!(
        create.body["data"]["collectionCreate"]["userErrors"],
        json!([])
    );
    let collection_id = create.body["data"]["collectionCreate"]["collection"]["id"]
        .as_str()
        .expect("collection create should return id")
        .to_string();

    let mutation = proxy.process_request(graphql_request(
        r#"
        mutation CollectionAddProductsParityPlan($id: ID!, $productIds: [ID!]!) {
          collectionAddProducts(id: $id, productIds: $productIds) {
            collection {
              id
              title
              handle
              products(first: 10) {
                nodes {
                  id
                  title
                  handle
                }
                pageInfo {
                  hasNextPage
                  hasPreviousPage
                }
              }
            }
            userErrors {
              field
              message
            }
          }
        }
        "#,
        json!({
            "id": collection_id,
            "productIds": ["gid://shopify/Product/first", "gid://shopify/Product/second"]
        }),
    ));
    assert_eq!(mutation.status, 200);

    let read = proxy.process_request(graphql_request(
        r#"
        query CollectionAddProductsDownstream($collectionId: ID!, $firstProductId: ID!, $secondProductId: ID!) {
          collection(id: $collectionId) {
            id
            title
            handle
            products(first: 10) {
              nodes {
                id
                title
                handle
              }
              pageInfo {
                hasNextPage
                hasPreviousPage
              }
            }
          }
          first: product(id: $firstProductId) {
            id
            collections(first: 10) {
              nodes {
                id
                title
                handle
              }
            }
          }
          second: product(id: $secondProductId) {
            id
            collections(first: 10) {
              nodes {
                id
                title
                handle
              }
            }
          }
        }
        "#,
        json!({
            "collectionId": collection_id,
            "firstProductId": "gid://shopify/Product/first",
            "secondProductId": "gid://shopify/Product/second"
        }),
    ));

    assert_eq!(read.status, 200);
    assert_eq!(
        read.body["data"]["collection"]["products"]["nodes"],
        json!([
            {
                "id": "gid://shopify/Product/first",
                "title": "First Product",
                "handle": "first-product"
            },
            {
                "id": "gid://shopify/Product/second",
                "title": "Second Product",
                "handle": "second-product"
            }
        ])
    );
    assert_eq!(
        read.body["data"]["first"]["collections"]["nodes"],
        json!([
            {
                "id": collection_id,
                "title": "Store Backed Collection",
                "handle": "store-backed-collection"
            }
        ])
    );
    assert_eq!(
        read.body["data"]["second"]["collections"]["nodes"],
        read.body["data"]["first"]["collections"]["nodes"]
    );
}

#[test]
fn smart_collection_downstream_read_evaluates_rule_set_membership() {
    let mut cheap_product = product("gid://shopify/Product/cheap", "Budget Tee", "budget-tee");
    cheap_product.vendor = "Hermes".to_string();
    cheap_product.product_type = "Shirt".to_string();
    cheap_product.tags = vec!["summer".to_string(), "sale".to_string()];
    cheap_product.variants = vec![json!({
        "id": "gid://shopify/ProductVariant/cheap-default",
        "price": "7.50"
    })];
    let mut expensive_product = product(
        "gid://shopify/Product/expensive",
        "Premium Jacket",
        "premium-jacket",
    );
    expensive_product.vendor = "Hermes".to_string();
    expensive_product.product_type = "Outerwear".to_string();
    expensive_product.tags = vec!["winter".to_string()];
    expensive_product.variants = vec![json!({
        "id": "gid://shopify/ProductVariant/expensive-default",
        "price": "19.00"
    })];
    let mut proxy = snapshot_proxy().with_base_products(vec![cheap_product, expensive_product]);

    let create = proxy.process_request(graphql_request(
        r#"
        mutation SmartCollectionCreate($input: CollectionInput!) {
          collectionCreate(input: $input) {
            collection {
              id
              ruleSet {
                appliedDisjunctively
                rules {
                  column
                  relation
                  condition
                }
              }
            }
            userErrors {
              field
              message
            }
          }
        }
        "#,
        json!({
            "input": {
                "title": "Cheap Hermes Shirts",
                "ruleSet": {
                    "appliedDisjunctively": false,
                    "rules": [
                        {
                            "column": "VENDOR",
                            "relation": "EQUALS",
                            "condition": "Hermes"
                        },
                        {
                            "column": "TYPE",
                            "relation": "EQUALS",
                            "condition": "Shirt"
                        },
                        {
                            "column": "TAG",
                            "relation": "EQUALS",
                            "condition": "sale"
                        },
                        {
                            "column": "VARIANT_PRICE",
                            "relation": "LESS_THAN",
                            "condition": "10"
                        }
                    ]
                }
            }
        }),
    ));
    assert_eq!(create.status, 200);
    assert_eq!(
        create.body["data"]["collectionCreate"]["userErrors"],
        json!([])
    );
    let collection_id = create.body["data"]["collectionCreate"]["collection"]["id"]
        .as_str()
        .expect("collection create should return id")
        .to_string();

    let read = proxy.process_request(graphql_request(
        r#"
        query SmartCollectionRead($id: ID!) {
          collection(id: $id) {
            productsCount {
              count
              precision
            }
            products(first: 10) {
              nodes {
                id
                title
              }
            }
          }
        }
        "#,
        json!({ "id": collection_id }),
    ));

    assert_eq!(read.status, 200);
    assert_eq!(
        read.body["data"]["collection"]["productsCount"],
        json!({ "count": 1, "precision": "EXACT" })
    );
    assert_eq!(
        read.body["data"]["collection"]["products"]["nodes"],
        json!([
            {
                "id": "gid://shopify/Product/cheap",
                "title": "Budget Tee"
            }
        ])
    );
}

#[test]
fn smart_collection_rules_include_staged_products_without_affecting_manual_collections() {
    let mut base_match = product("gid://shopify/Product/silk", "Silk Scarf", "silk-scarf");
    base_match.vendor = "Baseline Vendor".to_string();
    let mut base_miss = product("gid://shopify/Product/canvas", "Canvas Bag", "canvas-bag");
    base_miss.vendor = "Baseline Vendor".to_string();
    let mut proxy = snapshot_proxy().with_base_products(vec![base_match, base_miss]);

    let smart_create = proxy.process_request(graphql_request(
        r#"
        mutation SmartCollectionOrCreate($input: CollectionInput!) {
          collectionCreate(input: $input) {
            collection {
              id
            }
            userErrors {
              field
              message
            }
          }
        }
        "#,
        json!({
            "input": {
                "title": "Silk Or Staged Vendor",
                "ruleSet": {
                    "appliedDisjunctively": true,
                    "rules": [
                        {
                            "column": "TITLE",
                            "relation": "CONTAINS",
                            "condition": "silk"
                        },
                        {
                            "column": "VENDOR",
                            "relation": "EQUALS",
                            "condition": "Staged Vendor"
                        }
                    ]
                }
            }
        }),
    ));
    assert_eq!(smart_create.status, 200);
    assert_eq!(
        smart_create.body["data"]["collectionCreate"]["userErrors"],
        json!([])
    );
    let smart_collection_id = smart_create.body["data"]["collectionCreate"]["collection"]["id"]
        .as_str()
        .expect("smart collection create should return id")
        .to_string();

    let manual_create = proxy.process_request(graphql_request(
        r#"
        mutation ManualCollectionCreate($input: CollectionInput!) {
          collectionCreate(input: $input) {
            collection {
              id
            }
            userErrors {
              field
              message
            }
          }
        }
        "#,
        json!({
            "input": {
                "title": "Manual Collection",
                "sortOrder": "MANUAL"
            }
        }),
    ));
    assert_eq!(manual_create.status, 200);
    assert_eq!(
        manual_create.body["data"]["collectionCreate"]["userErrors"],
        json!([])
    );
    let manual_collection_id = manual_create.body["data"]["collectionCreate"]["collection"]["id"]
        .as_str()
        .expect("manual collection create should return id")
        .to_string();

    let product_create = proxy.process_request(graphql_request(
        r#"
        mutation StagedSmartCollectionProduct($product: ProductCreateInput!) {
          productCreate(product: $product) {
            product {
              id
              title
            }
            userErrors {
              field
              message
            }
          }
        }
        "#,
        json!({
            "product": {
                "title": "Staged Vendor Cap",
                "vendor": "Staged Vendor"
            }
        }),
    ));
    assert_eq!(product_create.status, 200);
    assert_eq!(
        product_create.body["data"]["productCreate"]["userErrors"],
        json!([])
    );
    let staged_product_id = product_create.body["data"]["productCreate"]["product"]["id"]
        .as_str()
        .expect("product create should return id")
        .to_string();

    let unsupported_create = proxy.process_request(graphql_request(
        r#"
        mutation UnsupportedSmartCollectionRuleCreate($input: CollectionInput!) {
          collectionCreate(input: $input) {
            collection {
              id
            }
            userErrors {
              field
              message
            }
          }
        }
        "#,
        json!({
            "input": {
                "title": "Unsupported Variant Title",
                "ruleSet": {
                    "appliedDisjunctively": true,
                    "rules": [
                        {
                            "column": "VARIANT_TITLE",
                            "relation": "EQUALS",
                            "condition": "Default Title"
                        }
                    ]
                }
            }
        }),
    ));
    assert_eq!(unsupported_create.status, 200);
    assert_eq!(
        unsupported_create.body["data"]["collectionCreate"]["userErrors"],
        json!([])
    );
    let unsupported_collection_id = unsupported_create.body["data"]["collectionCreate"]
        ["collection"]["id"]
        .as_str()
        .expect("unsupported smart collection create should return id")
        .to_string();

    let read = proxy.process_request(graphql_request(
        r#"
        query SmartManualAndUnsupportedRead($smartId: ID!, $manualId: ID!, $unsupportedId: ID!) {
          smart: collection(id: $smartId) {
            productsCount {
              count
              precision
            }
            products(first: 10, sortKey: TITLE) {
              nodes {
                id
                title
              }
            }
          }
          manual: collection(id: $manualId) {
            productsCount {
              count
              precision
            }
            products(first: 10, sortKey: MANUAL) {
              nodes {
                id
                title
              }
            }
          }
          unsupported: collection(id: $unsupportedId) {
            productsCount {
              count
              precision
            }
            products(first: 10) {
              nodes {
                id
              }
            }
          }
        }
        "#,
        json!({
            "smartId": smart_collection_id,
            "manualId": manual_collection_id,
            "unsupportedId": unsupported_collection_id
        }),
    ));

    assert_eq!(read.status, 200);
    assert_eq!(
        read.body["data"]["smart"]["productsCount"],
        json!({ "count": 2, "precision": "EXACT" })
    );
    assert_eq!(
        read.body["data"]["smart"]["products"]["nodes"],
        json!([
            {
                "id": "gid://shopify/Product/silk",
                "title": "Silk Scarf"
            },
            {
                "id": staged_product_id,
                "title": "Staged Vendor Cap"
            }
        ])
    );
    assert_eq!(
        read.body["data"]["manual"]["productsCount"],
        json!({ "count": 0, "precision": "EXACT" })
    );
    assert_eq!(read.body["data"]["manual"]["products"]["nodes"], json!([]));
    assert_eq!(
        read.body["data"]["unsupported"]["productsCount"],
        json!({ "count": 0, "precision": "EXACT" })
    );
    assert_eq!(
        read.body["data"]["unsupported"]["products"]["nodes"],
        json!([])
    );
}

#[test]
fn smart_collection_rules_include_product_set_products() {
    let mut proxy = snapshot_proxy();

    let product_set = proxy.process_request(graphql_request(
        r#"
        mutation SmartCollectionProductSet($input: ProductSetInput!, $synchronous: Boolean!) {
          productSet(input: $input, synchronous: $synchronous) {
            product {
              id
              title
              vendor
              productType
              tags
              variants(first: 10) {
                nodes {
                  price
                }
              }
            }
            userErrors {
              field
              message
            }
          }
        }
        "#,
        json!({
            "synchronous": true,
            "input": {
                "title": "Smart RuleSet Product",
                "status": "ACTIVE",
                "vendor": "Hermes Smart Rule Vendor",
                "productType": "Smart Rule Shirt",
                "tags": ["smart-ruleset"],
                "productOptions": [{ "name": "Color", "values": [{ "name": "Blue" }] }],
                "variants": [{
                    "optionValues": [{ "optionName": "Color", "name": "Blue" }],
                    "price": "7.50",
                    "inventoryItem": { "tracked": false, "requiresShipping": true }
                }]
            }
        }),
    ));
    assert_eq!(product_set.status, 200);
    assert_eq!(
        product_set.body["data"]["productSet"]["userErrors"],
        json!([])
    );
    let product_id = product_set.body["data"]["productSet"]["product"]["id"]
        .as_str()
        .expect("productSet should return id")
        .to_string();

    let dumped = proxy.process_request(request(
        "POST",
        "/__meta/dump",
        &json!({ "createdAt": "1970-01-01T00:00:00.000Z" }).to_string(),
    ));
    assert_eq!(dumped.status, 200);
    let restored =
        proxy.process_request(request("POST", "/__meta/restore", &dumped.body.to_string()));
    assert_eq!(restored.status, 200);

    let collection_create = proxy.process_request(graphql_request(
        r#"
        mutation SmartCollectionCreate($input: CollectionInput!) {
          collectionCreate(input: $input) {
            collection {
              id
            }
            userErrors {
              field
              message
            }
          }
        }
        "#,
        json!({
            "input": {
                "title": "Smart RuleSet Collection",
                "ruleSet": {
                    "appliedDisjunctively": false,
                    "rules": [
                        { "column": "TITLE", "relation": "CONTAINS", "condition": "RuleSet Product" },
                        { "column": "TYPE", "relation": "EQUALS", "condition": "Smart Rule Shirt" },
                        { "column": "VENDOR", "relation": "EQUALS", "condition": "Hermes Smart Rule Vendor" },
                        { "column": "TAG", "relation": "EQUALS", "condition": "smart-ruleset" },
                        { "column": "VARIANT_PRICE", "relation": "LESS_THAN", "condition": "10" }
                    ]
                }
            }
        }),
    ));
    assert_eq!(collection_create.status, 200);
    assert_eq!(
        collection_create.body["data"]["collectionCreate"]["userErrors"],
        json!([])
    );
    let collection_id = collection_create.body["data"]["collectionCreate"]["collection"]["id"]
        .as_str()
        .expect("collectionCreate should return id")
        .to_string();

    let dumped = proxy.process_request(request(
        "POST",
        "/__meta/dump",
        &json!({ "createdAt": "1970-01-01T00:00:00.000Z" }).to_string(),
    ));
    assert_eq!(dumped.status, 200);
    let restored =
        proxy.process_request(request("POST", "/__meta/restore", &dumped.body.to_string()));
    assert_eq!(restored.status, 200);

    let read = proxy.process_request(graphql_request(
        r#"
        query SmartCollectionRead($id: ID!) {
          collection(id: $id) {
            productsCount {
              count
              precision
            }
            products(first: 10) {
              nodes {
                id
                title
              }
            }
          }
        }
        "#,
        json!({ "id": collection_id }),
    ));

    assert_eq!(read.status, 200);
    assert_eq!(
        read.body["data"]["collection"]["productsCount"],
        json!({ "count": 1, "precision": "EXACT" })
    );
    assert_eq!(
        read.body["data"]["collection"]["products"]["nodes"],
        json!([{
            "id": product_id,
            "title": "Smart RuleSet Product"
        }])
    );
}

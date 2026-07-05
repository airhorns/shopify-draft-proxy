use super::*;

fn product(id: &str, title: &str, handle: &str) -> ProductRecord {
    ProductRecord {
        id: id.to_string(),
        created_at: default_product_timestamp(),
        updated_at: default_product_timestamp(),
        title: title.to_string(),
        handle: handle.to_string(),
        status: "ACTIVE".to_string(),
        description_html: String::new(),
        vendor: String::new(),
        product_type: String::new(),
        tags: Vec::new(),
        template_suffix: String::new(),
        seo_title: String::new(),
        seo_description: String::new(),
        total_inventory: 0,
        tracks_inventory: false,
        variants: Vec::new(),
        media: Vec::new(),
        collections: Vec::new(),
        extra_fields: BTreeMap::new(),
    }
}

fn saved_search(id: &str, name: &str, resource_type: &str) -> SavedSearchRecord {
    SavedSearchRecord {
        id: id.to_string(),
        name: name.to_string(),
        query: "tag:promo".to_string(),
        resource_type: resource_type.to_string(),
    }
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
        headers: BTreeMap::new(),
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

#[test]
fn store_effective_products_stage_overrides_base_and_tombstones() {
    let mut store = Store::default();
    store.replace_base_products(vec![
        product("gid://shopify/Product/base-1", "Base one", "base-one"),
        product("gid://shopify/Product/base-2", "Base two", "base-two"),
    ]);

    store.stage_product(product(
        "gid://shopify/Product/base-1",
        "Updated one",
        "updated-one",
    ));
    store.stage_product(product(
        "gid://shopify/Product/new",
        "New product",
        "new-product",
    ));
    store.delete_product("gid://shopify/Product/base-2");

    assert_eq!(
        store
            .product_by_id("gid://shopify/Product/base-1")
            .unwrap()
            .title,
        "Updated one"
    );
    assert!(store
        .product_by_id("gid://shopify/Product/base-2")
        .is_none());
    assert_eq!(
        store
            .product_by_handle("new-product")
            .map(|record| record.id.as_str()),
        Some("gid://shopify/Product/new")
    );
    assert_eq!(
        store
            .products()
            .iter()
            .map(|record| record.id.as_str())
            .collect::<Vec<_>>(),
        vec!["gid://shopify/Product/base-1", "gid://shopify/Product/new"]
    );
    assert_eq!(store.product_count(), 2);
}

#[test]
fn store_saved_searches_overlay_defaults_base_and_tombstones_in_order() {
    let mut store = Store::default();
    store.base.saved_searches.replace_with_order(
        BTreeMap::from([(
            "gid://shopify/SavedSearch/base".to_string(),
            saved_search("gid://shopify/SavedSearch/base", "Base products", "PRODUCT"),
        )]),
        vec!["gid://shopify/SavedSearch/base".to_string()],
    );

    store.stage_saved_search(saved_search(
        "gid://shopify/SavedSearch/base",
        "Updated base products",
        "PRODUCT",
    ));
    store.stage_saved_search(saved_search(
        "gid://shopify/SavedSearch/new",
        "New products",
        "PRODUCT",
    ));
    assert!(store.delete_saved_search("gid://shopify/SavedSearch/base"));

    assert!(store
        .saved_search_by_id("gid://shopify/SavedSearch/base")
        .is_none());
    assert_eq!(
        store
            .saved_searches_for_resource("PRODUCT")
            .iter()
            .map(|record| record.id.as_str())
            .collect::<Vec<_>>(),
        vec!["gid://shopify/SavedSearch/new"]
    );
}

#[test]
fn store_clear_staged_resets_overlays_and_tombstones_without_dropping_base() {
    let mut store = Store::default();
    store.replace_base_products(vec![product(
        "gid://shopify/Product/base",
        "Base product",
        "base-product",
    )]);
    store.stage_product(product(
        "gid://shopify/Product/base",
        "Updated product",
        "updated-product",
    ));
    store.delete_product("gid://shopify/Product/base");

    store.clear_staged();

    assert_eq!(
        store
            .product_by_id("gid://shopify/Product/base")
            .unwrap()
            .title,
        "Base product"
    );
    assert!(store.staged.products.records.is_empty());
    assert!(store.staged.products.tombstones.is_empty());
}

#[test]
fn store_dump_restore_round_trips_order_and_tombstones() {
    let mut proxy = snapshot_proxy().with_base_products(vec![
        product("gid://shopify/Product/base-1", "Base one", "base-one"),
        product("gid://shopify/Product/base-2", "Base two", "base-two"),
    ]);
    proxy.store.stage_product(product(
        "gid://shopify/Product/base-1",
        "Updated one",
        "updated-one",
    ));
    proxy.store.stage_product(product(
        "gid://shopify/Product/new",
        "New product",
        "new-product",
    ));
    proxy.store.delete_product("gid://shopify/Product/base-2");
    proxy.store.stage_saved_search(saved_search(
        "gid://shopify/SavedSearch/new",
        "New products",
        "PRODUCT",
    ));
    proxy.store.staged.locations.insert(
        "gid://shopify/Location/live".to_string(),
        json!({"id": "gid://shopify/Location/live", "name": "Live location"}),
    );
    proxy.store.staged.locations.insert(
        "gid://shopify/Location/deleted".to_string(),
        json!({"id": "gid://shopify/Location/deleted", "name": "Deleted location"}),
    );
    proxy
        .store
        .staged
        .locations
        .tombstone_staged("gid://shopify/Location/deleted");
    proxy.store.staged.delivery_profiles.insert(
        "gid://shopify/DeliveryProfile/live".to_string(),
        json!({"id": "gid://shopify/DeliveryProfile/live", "name": "Live profile"}),
    );
    proxy.store.staged.delivery_profiles.insert(
        "gid://shopify/DeliveryProfile/deleted".to_string(),
        json!({"id": "gid://shopify/DeliveryProfile/deleted", "name": "Deleted profile"}),
    );
    proxy
        .store
        .staged
        .delivery_profiles
        .tombstone_staged("gid://shopify/DeliveryProfile/deleted");
    proxy.store.staged.store_credit_accounts.insert(
        "gid://shopify/StoreCreditAccount/1".to_string(),
        json!({"id": "gid://shopify/StoreCreditAccount/1"}),
    );
    proxy.store.staged.b2b_locations.insert(
        "gid://shopify/CompanyLocation/1".to_string(),
        json!({"id": "gid://shopify/CompanyLocation/1"}),
    );
    proxy.store.staged.customers.insert(
        "gid://shopify/Customer/deleted".to_string(),
        json!({"id": "gid://shopify/Customer/deleted"}),
    );
    proxy
        .store
        .staged
        .customers
        .tombstone_staged("gid://shopify/Customer/deleted");
    proxy.store.staged.collections.insert(
        "gid://shopify/Collection/deleted".to_string(),
        json!({"id": "gid://shopify/Collection/deleted"}),
    );
    proxy
        .store
        .staged
        .collections
        .tombstone_staged("gid://shopify/Collection/deleted");

    let dump = proxy.process_request(request(
        "POST",
        "/__meta/dump",
        &json!({ "createdAt": "2026-05-23T00:00:00.000Z" }).to_string(),
    ));
    assert_eq!(
        dump.body["state"]["baseState"]["productOrder"],
        json!([
            "gid://shopify/Product/base-1",
            "gid://shopify/Product/base-2"
        ])
    );
    assert_eq!(
        dump.body["state"]["stagedState"]["productOrder"],
        json!(["gid://shopify/Product/base-1", "gid://shopify/Product/new"])
    );
    assert_eq!(
        dump.body["state"]["stagedState"]["deletedProductIds"],
        json!(["gid://shopify/Product/base-2"])
    );
    assert_eq!(
        dump.body["state"]["stagedState"]["locationOrder"],
        json!(["gid://shopify/Location/live"])
    );
    assert_eq!(
        dump.body["state"]["stagedState"]["deletedLocationIds"],
        json!(["gid://shopify/Location/deleted"])
    );
    assert_eq!(
        dump.body["state"]["stagedState"]["deliveryProfileOrder"],
        json!(["gid://shopify/DeliveryProfile/live"])
    );
    assert_eq!(
        dump.body["state"]["stagedState"]["deletedDeliveryProfileIds"],
        json!(["gid://shopify/DeliveryProfile/deleted"])
    );
    assert_eq!(
        dump.body["state"]["stagedState"]["storeCreditAccountOrder"],
        json!(["gid://shopify/StoreCreditAccount/1"])
    );
    assert_eq!(
        dump.body["state"]["stagedState"]["b2bLocationOrder"],
        json!(["gid://shopify/CompanyLocation/1"])
    );
    assert_eq!(
        dump.body["state"]["stagedState"]["deletedCustomerIds"],
        json!(["gid://shopify/Customer/deleted"])
    );
    assert_eq!(
        dump.body["state"]["stagedState"]["deletedCollectionIds"],
        json!(["gid://shopify/Collection/deleted"])
    );

    let mut restored = snapshot_proxy();
    let restore =
        restored.process_request(request("POST", "/__meta/restore", &dump.body.to_string()));
    assert_eq!(restore.status, 200);
    assert_eq!(
        restored
            .store
            .products()
            .iter()
            .map(|record| record.id.as_str())
            .collect::<Vec<_>>(),
        vec!["gid://shopify/Product/base-1", "gid://shopify/Product/new"]
    );
    assert_eq!(
        restored.store.saved_searches_for_resource("PRODUCT")[0].id,
        "gid://shopify/SavedSearch/new"
    );
    assert_eq!(
        restored
            .store
            .staged
            .locations
            .get("gid://shopify/Location/live"),
        Some(&json!({"id": "gid://shopify/Location/live", "name": "Live location"}))
    );
    assert!(restored
        .store
        .staged
        .locations
        .is_tombstoned("gid://shopify/Location/deleted"));
    assert!(restored
        .store
        .staged
        .delivery_profiles
        .is_tombstoned("gid://shopify/DeliveryProfile/deleted"));
    assert!(restored
        .store
        .staged
        .customers
        .is_tombstoned("gid://shopify/Customer/deleted"));
    assert!(restored
        .store
        .staged
        .collections
        .is_tombstoned("gid://shopify/Collection/deleted"));
    assert_eq!(
        restored.store.staged.store_credit_accounts.order,
        vec!["gid://shopify/StoreCreditAccount/1"]
    );
    assert_eq!(
        restored.store.staged.b2b_locations.order,
        vec!["gid://shopify/CompanyLocation/1"]
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
fn product_downstream_read_uses_staged_store_instead_of_operation_name_fixture() {
    let mut proxy = snapshot_proxy();
    let create = proxy.process_request(graphql_request(
        r#"
        mutation ProductCreateParityPlan($product: ProductInput!) {
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
                "status": "ACTIVE"
            }
        }),
    ));
    let product_id = create_product.body["data"]["productCreate"]["product"]["id"]
        .as_str()
        .expect("product create should return product id")
        .to_string();

    let create_variant = proxy.process_request(graphql_request(
        r#"
        mutation ProductVariantUpdateSetupVariant($input: ProductVariantInput!) {
          productVariantCreate(input: $input) {
            productVariant {
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
                "productId": product_id,
                "title": "Store Red",
                "sku": "STORE-DRAFT",
                "inventoryItem": {
                    "tracked": false,
                    "requiresShipping": true
                }
            }
        }),
    ));
    let variant_id = create_variant.body["data"]["productVariantCreate"]["productVariant"]["id"]
        .as_str()
        .expect("variant create should return variant id")
        .to_string();

    let update = proxy.process_request(graphql_request(
        r#"
        mutation ProductVariantUpdateParityPlan($input: ProductVariantInput!) {
          productVariantUpdate(input: $input) {
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
            productVariant {
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
            "input": {
                "id": variant_id,
                "title": "Store Red",
                "sku": "STORE-RED",
                "barcode": "store-barcode",
                "selectedOptions": [{ "name": "Color", "value": "Store Red" }],
                "inventoryItem": {
                    "tracked": true,
                    "requiresShipping": false
                }
            }
        }),
    ));
    assert_eq!(
        update.body["data"]["productVariantUpdate"]["product"]["id"],
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

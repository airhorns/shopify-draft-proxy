use super::common::*;
use pretty_assertions::assert_eq;

#[test]
fn standard_proxy_construction_attaches_default_registry_for_core_roots() {
    let mut proxy = snapshot_proxy();

    let product = proxy.process_request(graphql_request(
        "POST",
        r#"{"query":"query { product(id: \"gid://shopify/Product/1\") { id } }"}"#,
    ));

    assert_eq!(product.status, 200);
    assert_eq!(product.body, json!({ "data": { "product": null } }));
}

#[test]
fn product_read_serializes_seeded_base_product_by_id() {
    let mut proxy = snapshot_proxy().with_base_products(vec![ProductRecord {
        id: "gid://shopify/Product/1".to_string(),
        created_at: "2024-01-01T00:00:00.000Z".to_string(),
        updated_at: "2024-01-01T00:00:00.000Z".to_string(),
        title: "Seeded product".to_string(),
        handle: "seeded-product".to_string(),
        status: "ACTIVE".to_string(),
        description_html: String::new(),
        vendor: String::new(),
        product_type: String::new(),
        tags: Vec::new(),
        template_suffix: String::new(),
        seo_title: String::new(),
        seo_description: String::new(),
        ..ProductRecord::default()
    }]);

    let product = proxy.process_request(graphql_request(
        "POST",
        r#"{"query":"query { product(id: \"gid://shopify/Product/1\") { id title handle status } }"}"#,
    ));

    assert_eq!(product.status, 200);
    assert_eq!(
        product.body,
        json!({
            "data": {
                "product": {
                    "id": "gid://shopify/Product/1",
                    "title": "Seeded product",
                    "handle": "seeded-product",
                    "status": "ACTIVE"
                }
            }
        })
    );
}

#[test]
fn product_read_serializes_only_requested_scalar_fields() {
    let mut proxy = snapshot_proxy().with_base_products(vec![ProductRecord {
        id: "gid://shopify/Product/1".to_string(),
        created_at: "2024-01-01T00:00:00.000Z".to_string(),
        updated_at: "2024-01-01T00:00:00.000Z".to_string(),
        title: "Seeded product".to_string(),
        handle: "seeded-product".to_string(),
        status: "ACTIVE".to_string(),
        description_html: String::new(),
        vendor: String::new(),
        product_type: String::new(),
        tags: Vec::new(),
        template_suffix: String::new(),
        seo_title: String::new(),
        seo_description: String::new(),
        ..ProductRecord::default()
    }]);

    let product = proxy.process_request(graphql_request(
        "POST",
        r#"{"query":"query { product(id: \"gid://shopify/Product/1\") { handle } }"}"#,
    ));

    assert_eq!(product.status, 200);
    assert_eq!(
        product.body,
        json!({
            "data": {
                "product": {
                    "handle": "seeded-product"
                }
            }
        })
    );
}

#[test]
fn product_read_preserves_root_alias() {
    let mut proxy = snapshot_proxy().with_base_products(vec![ProductRecord {
        id: "gid://shopify/Product/1".to_string(),
        created_at: "2024-01-01T00:00:00.000Z".to_string(),
        updated_at: "2024-01-01T00:00:00.000Z".to_string(),
        title: "Seeded product".to_string(),
        handle: "seeded-product".to_string(),
        status: "ACTIVE".to_string(),
        description_html: String::new(),
        vendor: String::new(),
        product_type: String::new(),
        tags: Vec::new(),
        template_suffix: String::new(),
        seo_title: String::new(),
        seo_description: String::new(),
        ..ProductRecord::default()
    }]);

    let product = proxy.process_request(graphql_request(
        "POST",
        r#"{"query":"query { selectedProduct: product(id: \"gid://shopify/Product/1\") { id title } }"}"#,
    ));

    assert_eq!(product.status, 200);
    assert_eq!(
        product.body,
        json!({
            "data": {
                "selectedProduct": {
                    "id": "gid://shopify/Product/1",
                    "title": "Seeded product"
                }
            }
        })
    );
}

#[test]
fn collection_publishable_mutations_stage_publication_state_for_downstream_reads() {
    let mut proxy = snapshot_proxy();
    let variables = json!({
        "id": "gid://shopify/Collection/468787757289",
        "input": [{ "publicationId": "gid://shopify/Publication/82090459369" }],
        "publicationId": "gid://shopify/Publication/82090459369"
    });
    let publish_response = proxy.process_request(json_graphql_request(
        r#"
        mutation CollectionPublishablePublish($id: ID!, $input: [PublicationInput!]!, $publicationId: ID!) {
          publishablePublish(id: $id, input: $input) {
            publishable { ... on Collection { id title handle publishedOnCurrentPublication publishedOnPublication(publicationId: $publicationId) availablePublicationsCount { count precision } resourcePublicationsCount { count precision } } }
            userErrors { field message }
          }
        }
        "#,
        variables.clone(),
    ));
    assert_eq!(
        publish_response.body["data"]["publishablePublish"]["publishable"],
        json!({
            "id": "gid://shopify/Collection/468787757289",
            "title": "Hermes Collection Conformance 1777078204269",
            "handle": "hermes-collection-conformance-1777078204269",
            "publishedOnCurrentPublication": false,
            "publishedOnPublication": true,
            "availablePublicationsCount": { "count": 1, "precision": "EXACT" },
            "resourcePublicationsCount": { "count": 1, "precision": "EXACT" }
        })
    );
    assert_eq!(
        publish_response.body["data"]["publishablePublish"]["userErrors"],
        json!([])
    );

    let read_query = r#"
        query CollectionPublicationRead($id: ID!, $publicationId: ID!) {
          collection(id: $id) {
            id title handle publishedOnCurrentPublication publishedOnPublication(publicationId: $publicationId)
            availablePublicationsCount { count precision }
            resourcePublicationsCount { count precision }
          }
        }
    "#;
    let read_after_publish =
        proxy.process_request(json_graphql_request(read_query, variables.clone()));
    assert_eq!(
        read_after_publish.body["data"]["collection"],
        publish_response.body["data"]["publishablePublish"]["publishable"]
    );

    let unpublish_response = proxy.process_request(json_graphql_request(
        r#"
        mutation CollectionPublishableUnpublish($id: ID!, $input: [PublicationInput!]!, $publicationId: ID!) {
          publishableUnpublish(id: $id, input: $input) {
            publishable { ... on Collection { id title handle publishedOnCurrentPublication publishedOnPublication(publicationId: $publicationId) availablePublicationsCount { count precision } resourcePublicationsCount { count precision } } }
            userErrors { field message }
          }
        }
        "#,
        variables.clone(),
    ));
    assert_eq!(
        unpublish_response.body["data"]["publishableUnpublish"]["publishable"],
        json!({
            "id": "gid://shopify/Collection/468787757289",
            "title": "Hermes Collection Conformance 1777078204269",
            "handle": "hermes-collection-conformance-1777078204269",
            "publishedOnCurrentPublication": false,
            "publishedOnPublication": false,
            "availablePublicationsCount": { "count": 0, "precision": "EXACT" },
            "resourcePublicationsCount": { "count": 0, "precision": "EXACT" }
        })
    );
    let read_after_unpublish = proxy.process_request(json_graphql_request(read_query, variables));
    assert_eq!(
        read_after_unpublish.body["data"]["collection"],
        unpublish_response.body["data"]["publishableUnpublish"]["publishable"]
    );
}

#[test]
fn top_level_inventory_level_read_observes_staged_inventory_level_state() {
    let mut proxy = snapshot_proxy();
    let seed = proxy.process_request(json_graphql_request(
        r#"
        mutation SeedInventoryLevel($input: InventorySetQuantitiesInput!) {
          inventorySetQuantities(input: $input) { userErrors { field message } }
        }
        "#,
        json!({"input": {"name": "available", "reason": "correction", "ignoreCompareQuantity": true, "quantities": [
            {"inventoryItemId": "gid://shopify/InventoryItem/50643009569001", "locationId": "gid://shopify/Location/68509171945", "quantity": 4}
        ]}}),
    ));
    assert_eq!(
        seed.body["data"]["inventorySetQuantities"]["userErrors"],
        json!([])
    );

    let item_response = proxy.process_request(json_graphql_request(
        r#"
        query InventoryItemRead($inventoryItemId: ID!) {
          inventoryItem(id: $inventoryItemId) {
            inventoryLevels(first: 10) {
              nodes { id }
            }
          }
        }
        "#,
        json!({
            "inventoryItemId": "gid://shopify/InventoryItem/50643009569001"
        }),
    ));
    assert_eq!(item_response.status, 200);
    let inventory_level_id = item_response.body["data"]["inventoryItem"]["inventoryLevels"]
        ["nodes"][0]["id"]
        .as_str()
        .expect("staged inventory level should have an id")
        .to_string();

    let response = proxy.process_request(json_graphql_request(
        r#"
        query InventoryLevelRead($inventoryLevelId: ID!) {
          inventoryLevel(id: $inventoryLevelId) {
            id
            location { id name }
            quantities(names: ["available", "on_hand", "incoming"]) { name quantity updatedAt }
          }
        }
        "#,
        json!({
            "inventoryLevelId": inventory_level_id
        }),
    ));

    assert_eq!(response.status, 200);
    assert_eq!(
        response.body["data"]["inventoryLevel"],
        json!({
            "id": "gid://shopify/InventoryLevel/50643009569001-68509171945?inventory_item_id=gid://shopify/InventoryItem/50643009569001",
            "location": { "id": "gid://shopify/Location/68509171945", "name": "Shop location" },
            "quantities": [
                { "name": "available", "quantity": 4, "updatedAt": "2024-01-01T00:00:00.000Z" },
                { "name": "on_hand", "quantity": 4, "updatedAt": "2024-01-01T00:00:00.000Z" },
                { "name": "incoming", "quantity": 0, "updatedAt": null }
            ]
        })
    );
}

#[test]
fn product_variant_bulk_fixture_downstream_reads_return_captured_shapes() {
    let mut proxy = snapshot_proxy();

    let bulk_create = proxy.process_request(json_graphql_request(
        r#"
        query ProductVariantsBulkCreateDownstreamRead($id: ID!, $query: String!) {
          product(id: $id) { id totalInventory tracksInventory variants(first: 10) { nodes { id title sku inventoryItem { id tracked requiresShipping } } } }
          products(first: 10, query: $query) { nodes { id totalInventory tracksInventory } }
          skuCount: productsCount(query: $query) { count precision }
        }
        "#,
        json!({
            "id": "gid://shopify/Product/10180320788786",
            "query": "sku:HERMES-BULK-962361-BLUE"
        }),
    ));
    assert_eq!(bulk_create.status, 200);
    assert_eq!(
        bulk_create.body["data"]["product"],
        Value::Null,
        "unobserved product reads should not replay a baked downstream fixture"
    );
    assert_eq!(bulk_create.body["data"]["skuCount"]["count"], json!(0));

    let inventory_read = proxy.process_request(json_graphql_request(
        r#"
        query ProductVariantsBulkCreateInventoryReadDownstream($productId: ID!, $variantId: ID!, $inventoryItemId: ID!) {
          product(id: $productId) { id title handle status totalInventory tracksInventory variants(first: 10) { nodes { id title sku inventoryItem { id tracked requiresShipping } } } }
          variant: productVariant(id: $variantId) { id title sku inventoryItem { id tracked requiresShipping } product { id title handle status totalInventory tracksInventory } }
          stock: inventoryItem(id: $inventoryItemId) { id tracked requiresShipping variant { id title sku inventoryQuantity product { id title handle status totalInventory tracksInventory } } }
        }
        "#,
        json!({
            "productId": "gid://shopify/Product/9263919988969",
            "variantId": "gid://shopify/ProductVariant/50933258911977",
            "inventoryItemId": "gid://shopify/InventoryItem/53081336283369"
        }),
    ));
    assert_eq!(inventory_read.status, 200);
    assert_eq!(
        inventory_read.body["data"]["product"]["id"],
        json!("gid://shopify/Product/9263919988969")
    );
    assert_eq!(
        inventory_read.body["data"]["variant"],
        Value::Null,
        "unobserved variant reads should not replay a baked downstream fixture"
    );
    assert_eq!(
        inventory_read.body["data"]["stock"]["variant"]["id"],
        json!("gid://shopify/ProductVariant/50933258911977")
    );

    let bulk_update = proxy.process_request(json_graphql_request(
        r#"
        query ProductVariantsBulkUpdateDownstreamRead($id: ID!, $query: String!) {
          product(id: $id) { id totalInventory tracksInventory variants(first: 10) { nodes { id title sku metafield(namespace: "specs", key: "bulkUpdateTier") { value ownerType } inventoryItem { id tracked requiresShipping } } } }
          products(first: 10, query: $query) { nodes { id totalInventory tracksInventory } }
          skuCount: productsCount(query: $query) { count precision }
        }
        "#,
        json!({
            "id": "gid://shopify/Product/10180320788786",
            "query": "sku:HERMES-BULK-962361-RED"
        }),
    ));
    assert_eq!(bulk_update.status, 200);
    assert_eq!(
        bulk_update.body["data"]["product"],
        Value::Null,
        "unobserved product reads should not replay a baked bulk-update downstream fixture"
    );

    let reorder = proxy.process_request(json_graphql_request(
        r#"
        query ProductVariantsBulkReorderDownstreamRead($productId: ID!) {
          product(id: $productId) { id variants(first: 10) { nodes { id title selectedOptions { name value } } } }
        }
        "#,
        json!({ "productId": "gid://shopify/Product/10170568114482" }),
    ));
    assert_eq!(reorder.status, 200);
    assert_eq!(
        reorder.body["data"]["product"],
        Value::Null,
        "unobserved product reads should not replay a baked bulk-reorder downstream fixture"
    );

    let node = proxy.process_request(json_graphql_request(
        r#"
        query ProductVariantNodeRead($id: ID!) {
          node(id: $id) { ... on ProductVariant { id title selectedOptions { name value } } }
        }
        "#,
        json!({ "id": "gid://shopify/ProductVariant/51098748059954" }),
    ));
    assert_eq!(node.status, 200);
    assert_eq!(
        node.body["data"]["node"],
        json!({
            "id": "gid://shopify/ProductVariant/51098748059954",
            "title": "Blue",
            "selectedOptions": [{ "name": "Color", "value": "Blue" }]
        })
    );
}

#[test]
fn product_media_deprecated_user_errors_and_variant_media_guards_port_old_gleam_helpers() {
    let mut proxy = snapshot_proxy();

    let deprecated = proxy.process_request(json_graphql_request(
        r#"
        mutation RustProductMediaDeprecatedUserErrors {
          create: productCreateMedia(productId: "gid://shopify/Product/optioned", media: [{ originalSource: "not-a-url", mediaContentType: IMAGE }]) {
            userErrors { field message }
            mediaUserErrors { field message }
          }
          update: productUpdateMedia(productId: "gid://shopify/Product/optioned", media: [{ id: "gid://shopify/MediaImage/missing", alt: "Missing" }]) {
            userErrors { field message }
            mediaUserErrors { field message }
          }
          delete: productDeleteMedia(productId: "gid://shopify/Product/optioned", mediaIds: ["gid://shopify/MediaImage/missing"]) {
            userErrors { field message }
            mediaUserErrors { field message }
          }
          reorder: productReorderMedia(id: "gid://shopify/Product/optioned", moves: [{ id: "gid://shopify/MediaImage/missing", newPosition: "0" }]) {
            userErrors { field message }
            mediaUserErrors { field message }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(deprecated.status, 200);
    assert_eq!(
        deprecated.body["data"]["create"],
        json!({
            "userErrors": [{ "field": ["media", "0", "originalSource"], "message": "Image URL is invalid" }],
            "mediaUserErrors": [{ "field": ["media", "0", "originalSource"], "message": "Image URL is invalid" }]
        })
    );
    assert_eq!(
        deprecated.body["data"]["update"],
        json!({
            "userErrors": [{ "field": ["media"], "message": "Media id gid://shopify/MediaImage/missing does not exist" }],
            "mediaUserErrors": [{ "field": ["media"], "message": "Media id gid://shopify/MediaImage/missing does not exist" }]
        })
    );
    assert_eq!(
        deprecated.body["data"]["delete"],
        json!({
            "userErrors": [{ "field": ["mediaIds"], "message": "Media id gid://shopify/MediaImage/missing does not exist" }],
            "mediaUserErrors": [{ "field": ["mediaIds"], "message": "Media id gid://shopify/MediaImage/missing does not exist" }]
        })
    );
    assert_eq!(
        deprecated.body["data"]["reorder"],
        json!({
            "userErrors": [{ "field": ["moves", "0", "id"], "message": "Media does not exist" }],
            "mediaUserErrors": [{ "field": ["moves", "0", "id"], "message": "Media does not exist" }]
        })
    );

    let variant_guards = proxy.process_request(json_graphql_request(
        r#"
        mutation RustProductVariantMediaValidationTailHelpers {
          variantFromOtherProduct: productVariantAppendMedia(
            productId: "gid://shopify/Product/optioned"
            variantMedia: [{ variantId: "gid://shopify/ProductVariant/child", mediaIds: ["gid://shopify/MediaImage/ready"] }]
          ) { productVariants { id } userErrors { field message code } }
          mediaFromOtherProduct: productVariantAppendMedia(
            productId: "gid://shopify/Product/optioned"
            variantMedia: [{ variantId: "gid://shopify/ProductVariant/default", mediaIds: ["gid://shopify/MediaImage/child"] }]
          ) { productVariants { id } userErrors { field message code } }
          processingMedia: productVariantAppendMedia(
            productId: "gid://shopify/Product/optioned"
            variantMedia: [{ variantId: "gid://shopify/ProductVariant/default", mediaIds: ["gid://shopify/MediaImage/processing"] }]
          ) { productVariants { id } userErrors { field message code } }
          detachUnattached: productVariantDetachMedia(
            productId: "gid://shopify/Product/optioned"
            variantMedia: [{ variantId: "gid://shopify/ProductVariant/default", mediaIds: ["gid://shopify/MediaImage/ready"] }]
          ) { productVariants { id } userErrors { field message code } }
        }
        "#,
        json!({}),
    ));
    assert_eq!(variant_guards.status, 200);
    assert_eq!(
        variant_guards.body["data"]["variantFromOtherProduct"],
        json!({
            "productVariants": Value::Null,
            "userErrors": [{
                "field": ["variantMedia", "0", "variantId"],
                "message": "Variant does not exist on the specified product.",
                "code": "PRODUCT_VARIANT_DOES_NOT_EXIST_ON_PRODUCT"
            }]
        })
    );
    assert_eq!(
        variant_guards.body["data"]["mediaFromOtherProduct"],
        json!({
            "productVariants": Value::Null,
            "userErrors": [{
                "field": ["variantMedia", "0", "mediaIds"],
                "message": "Media does not exist on the specified product.",
                "code": "MEDIA_DOES_NOT_EXIST_ON_PRODUCT"
            }]
        })
    );
    assert_eq!(
        variant_guards.body["data"]["processingMedia"],
        json!({
            "productVariants": Value::Null,
            "userErrors": [{
                "field": ["variantMedia", "0", "mediaIds"],
                "message": "Non-ready media cannot be attached to variants.",
                "code": "NON_READY_MEDIA"
            }]
        })
    );
    assert_eq!(
        variant_guards.body["data"]["detachUnattached"],
        json!({
            "productVariants": Value::Null,
            "userErrors": [{
                "field": ["variantMedia", "0", "variantId"],
                "message": "The specified media is not attached to the specified variant.",
                "code": "MEDIA_IS_NOT_ATTACHED_TO_VARIANT"
            }]
        })
    );
}

#[test]
fn product_publication_full_sync_and_feedback_tail_helpers_port_old_gleam_tests() {
    let mut proxy = snapshot_proxy();

    let publication_validation = proxy.process_request(json_graphql_request(
        r#"
        mutation RustProductPublicationTargetValidation {
          both: publicationCreate(input: { catalogId: "gid://shopify/MarketCatalog/999", channelId: "gid://shopify/Channel/999" }) {
            publication { id }
            userErrors { field message code }
          }
          blankCatalog: publicationCreate(input: {}) {
            publication { id }
            userErrors { field message code }
          }
          missingCatalog: publicationCreate(input: { catalogId: "gid://shopify/MarketCatalog/999" }) {
            publication { id }
            userErrors { field message code }
          }
          missingChannel: publicationCreate(input: { channelId: "gid://shopify/Channel/999" }) {
            publication { id }
            userErrors { field message code }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(publication_validation.status, 200);
    assert_eq!(
        publication_validation.body["data"]["both"],
        json!({
            "publication": Value::Null,
            "userErrors": [{
                "field": ["input"],
                "message": "Only one of catalog or channel can be provided",
                "code": "INVALID"
            }]
        })
    );
    assert_eq!(
        publication_validation.body["data"]["blankCatalog"],
        json!({
            "publication": Value::Null,
            "userErrors": [{
                "field": ["input", "catalogId"],
                "message": "Catalog can't be blank",
                "code": "BLANK"
            }]
        })
    );
    assert_eq!(
        publication_validation.body["data"]["missingCatalog"],
        json!({
            "publication": Value::Null,
            "userErrors": [{
                "field": ["input", "catalogId"],
                "message": "Catalog not found",
                "code": "NOT_FOUND"
            }]
        })
    );
    assert_eq!(
        publication_validation.body["data"]["missingChannel"],
        json!({
            "publication": Value::Null,
            "userErrors": [{
                "field": ["input", "channelId"],
                "message": "Channel not found",
                "code": "NOT_FOUND"
            }]
        })
    );

    let create_publication = proxy.process_request(json_graphql_request(
        r#"
        mutation RustProductPublicationCreateSeed {
          publicationCreate(input: { name: "Seed" }) {
            publication { id }
            userErrors { field message code }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(create_publication.status, 200);
    assert_eq!(
        create_publication.body["data"]["publicationCreate"],
        json!({
            "publication": { "id": "gid://shopify/Publication/2" },
            "userErrors": []
        })
    );

    let publication_update_delete = proxy.process_request(json_graphql_request(
        r#"
        mutation RustProductPublicationUpdateDeleteValidation {
          updateBoth: publicationUpdate(id: "gid://shopify/Publication/2", input: { catalogId: "gid://shopify/MarketCatalog/999", channelId: "gid://shopify/Channel/999" }) {
            publication { id }
            userErrors { field message code }
          }
          updateMissingCatalog: publicationUpdate(id: "gid://shopify/Publication/2", input: { catalogId: "gid://shopify/MarketCatalog/999" }) {
            publication { id }
            userErrors { field message code }
          }
          deleteDefault: publicationDelete(id: "gid://shopify/Publication/1") {
            deletedId
            userErrors { field message code }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(publication_update_delete.status, 200);
    assert_eq!(
        publication_update_delete.body["data"]["updateBoth"],
        json!({
            "publication": Value::Null,
            "userErrors": [{
                "field": ["input"],
                "message": "Only one of catalog or channel can be provided",
                "code": "INVALID"
            }]
        })
    );
    assert_eq!(
        publication_update_delete.body["data"]["updateMissingCatalog"],
        json!({
            "publication": Value::Null,
            "userErrors": [{
                "field": ["input", "catalogId"],
                "message": "Catalog not found",
                "code": "NOT_FOUND"
            }]
        })
    );
    assert_eq!(
        publication_update_delete.body["data"]["deleteDefault"],
        json!({
            "deletedId": Value::Null,
            "userErrors": [{
                "field": ["id"],
                "message": "Cannot delete the default publication",
                "code": "CANNOT_DELETE_DEFAULT_PUBLICATION"
            }]
        })
    );

    let unknown_feed = proxy.process_request(json_graphql_request(
        r#"
        mutation RustProductFullSyncUnknown($id: ID!) {
          productFullSync(id: $id) { id userErrors { field message code } }
        }
        "#,
        json!({ "id": "gid://shopify/ProductFeed/999999999" }),
    ));
    assert_eq!(unknown_feed.status, 200);
    assert_eq!(
        unknown_feed.body,
        json!({
            "data": {
                "productFullSync": {
                    "id": Value::Null,
                    "userErrors": [{
                        "field": ["id"],
                        "message": "ProductFeed does not exist",
                        "code": "NOT_FOUND"
                    }]
                }
            }
        })
    );

    let sync_before_create = proxy.process_request(json_graphql_request(
        r#"
        mutation RustProductFullSyncJob($id: ID!) {
          productFullSync(id: $id) { id job { id } userErrors { field message code } }
        }
        "#,
        json!({ "id": "gid://shopify/ProductFeed/US-EN" }),
    ));
    assert_eq!(sync_before_create.status, 200);
    assert_eq!(
        sync_before_create.body["data"]["productFullSync"],
        json!({
            "id": Value::Null,
            "job": Value::Null,
            "userErrors": [{
                "field": ["id"],
                "message": "ProductFeed does not exist",
                "code": "NOT_FOUND"
            }]
        })
    );

    let feed_create = proxy.process_request(json_graphql_request(
        r#"
        mutation RustProductFeedCreateForFullSync($input: ProductFeedInput) {
          productFeedCreate(input: $input) { productFeed { id } userErrors { field message code } }
        }
        "#,
        json!({ "input": { "country": "US", "language": "EN" } }),
    ));
    assert_eq!(feed_create.status, 200);
    assert_eq!(
        feed_create.body["data"]["productFeedCreate"],
        json!({
            "productFeed": { "id": "gid://shopify/ProductFeed/US-EN" },
            "userErrors": []
        })
    );

    let sync = proxy.process_request(json_graphql_request(
        r#"
        mutation RustProductFullSyncJob($id: ID!) {
          productFullSync(id: $id) {
            __typename
            id
            job { __typename id done query { __typename } }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "id": "gid://shopify/ProductFeed/US-EN" }),
    ));
    assert_eq!(sync.status, 200);
    assert_eq!(
        sync.body["data"]["productFullSync"],
        json!({
            "__typename": "ProductFullSyncPayload",
            "id": "gid://shopify/ProductFeed/US-EN",
            "job": {
                "__typename": "Job",
                "id": "gid://shopify/Job/2",
                "done": false,
                "query": { "__typename": "QueryRoot" }
            },
            "userErrors": []
        })
    );

    let job = proxy.process_request(json_graphql_request(
        r#"
        query RustProductFullSyncJobPoll($id: ID!) {
          job(id: $id) { __typename id done query { __typename } }
        }
        "#,
        json!({ "id": "gid://shopify/Job/2" }),
    ));
    assert_eq!(job.status, 200);
    assert_eq!(
        job.body,
        json!({
            "data": {
                "job": {
                    "__typename": "Job",
                    "id": "gid://shopify/Job/2",
                    "done": false,
                    "query": { "__typename": "QueryRoot" }
                }
            }
        })
    );

    let too_long = "x".repeat(101);
    let batch_entries = std::iter::repeat_n(
        "{ productId: \"gid://shopify/Product/optioned\", state: ACCEPTED, feedbackGeneratedAt: \"2024-01-01T00:00:00Z\", productUpdatedAt: \"2024-01-01T00:00:00Z\", messages: [] }",
        51,
    )
    .collect::<Vec<_>>()
    .join(",");
    let product_feedback_query = format!(
        r#"
        mutation RustProductFeedbackValidationTailHelpers {{
          blankMessages: bulkProductResourceFeedbackCreate(feedbackInput: [{{ productId: "gid://shopify/Product/optioned", state: REQUIRES_ACTION, feedbackGeneratedAt: "2024-01-01T00:00:00Z", productUpdatedAt: "2024-01-01T00:00:00Z", messages: [] }}]) {{ feedback {{ productId }} userErrors {{ field message code }} }}
          futureGeneratedAt: bulkProductResourceFeedbackCreate(feedbackInput: [{{ productId: "gid://shopify/Product/optioned", state: ACCEPTED, feedbackGeneratedAt: "2099-01-01T00:00:00Z", productUpdatedAt: "2024-01-01T00:00:00Z", messages: ["needs review"] }}]) {{ feedback {{ productId }} userErrors {{ field message code }} }}
          tooLongMessage: bulkProductResourceFeedbackCreate(feedbackInput: [{{ productId: "gid://shopify/Product/optioned", state: REQUIRES_ACTION, feedbackGeneratedAt: "2024-01-01T00:00:00Z", productUpdatedAt: "2024-01-01T00:00:00Z", messages: ["{too_long}"] }}]) {{ feedback {{ productId }} userErrors {{ field message code }} }}
          batchTooLong: bulkProductResourceFeedbackCreate(feedbackInput: [{batch_entries}]) {{ feedback {{ productId }} userErrors {{ field message code }} }}
        }}
        "#
    );
    let product_feedback =
        proxy.process_request(json_graphql_request(&product_feedback_query, json!({})));
    assert_eq!(product_feedback.status, 200);
    assert_eq!(
        product_feedback.body["data"]["blankMessages"],
        json!({
            "feedback": [],
            "userErrors": [{
                "field": ["feedback", "0", "messages"],
                "message": "Messages can't be blank",
                "code": "BLANK"
            }]
        })
    );
    assert_eq!(
        product_feedback.body["data"]["futureGeneratedAt"],
        json!({
            "feedback": [],
            "userErrors": [{
                "field": ["feedback", "0", "feedbackGeneratedAt"],
                "message": "Feedback generated at must not be in the future",
                "code": "INVALID"
            }]
        })
    );
    assert_eq!(
        product_feedback.body["data"]["tooLongMessage"],
        json!({
            "feedback": [],
            "userErrors": [{
                "field": ["feedback", "0", "messages", "0"],
                "message": "Message is too long (maximum is 100 characters)",
                "code": "TOO_LONG"
            }]
        })
    );
    assert_eq!(
        product_feedback.body["data"]["batchTooLong"],
        json!({
            "feedback": [],
            "userErrors": [{
                "field": ["feedback"],
                "message": "Feedback cannot contain more than 50 entries",
                "code": "TOO_LONG"
            }]
        })
    );

    let shop_feedback = proxy.process_request(json_graphql_request(
        &format!(
            r#"
            mutation RustShopFeedbackValidationTailHelpers {{
              blankMessages: shopResourceFeedbackCreate(input: {{ state: REQUIRES_ACTION, feedbackGeneratedAt: "2024-01-01T00:00:00Z", messages: [] }}) {{ feedback {{ state }} userErrors {{ field message code }} }}
              futureGeneratedAt: shopResourceFeedbackCreate(input: {{ state: ACCEPTED, feedbackGeneratedAt: "2099-01-01T00:00:00Z", messages: ["needs review"] }}) {{ feedback {{ state }} userErrors {{ field message code }} }}
              tooLongMessage: shopResourceFeedbackCreate(input: {{ state: REQUIRES_ACTION, feedbackGeneratedAt: "2024-01-01T00:00:00Z", messages: ["{too_long}"] }}) {{ feedback {{ state }} userErrors {{ field message code }} }}
            }}
            "#
        ),
        json!({}),
    ));
    assert_eq!(shop_feedback.status, 200);
    assert_eq!(
        shop_feedback.body["data"]["blankMessages"],
        json!({
            "feedback": Value::Null,
            "userErrors": [{
                "field": ["feedback", "messages"],
                "message": "Messages can't be blank",
                "code": "BLANK"
            }]
        })
    );
    assert_eq!(
        shop_feedback.body["data"]["futureGeneratedAt"],
        json!({
            "feedback": Value::Null,
            "userErrors": [{
                "field": ["feedback", "feedbackGeneratedAt"],
                "message": "Feedback generated at must not be in the future",
                "code": "INVALID"
            }]
        })
    );
    assert_eq!(
        shop_feedback.body["data"]["tooLongMessage"],
        json!({
            "feedback": Value::Null,
            "userErrors": [{
                "field": ["feedback", "messages", "0"],
                "message": "Message is too long (maximum is 100 characters)",
                "code": "TOO_LONG"
            }]
        })
    );

    let log = proxy.get_log_snapshot();
    let entries = log["entries"].as_array().expect("log entries");
    assert!(
        entries
            .iter()
            .any(|entry| entry["status"] == json!("failed")
                && entry["interpreted"]["primaryRootField"] == json!("publicationCreate")),
        "publication validation branches should still record failed log entries: {log}"
    );
    for root in [
        "bulkProductResourceFeedbackCreate",
        "shopResourceFeedbackCreate",
    ] {
        assert!(
            entries
                .iter()
                .any(|entry| entry["status"] == json!("failed")
                    && entry["interpreted"]["primaryRootField"] == json!(root)),
            "feedback validation branches should record failed log entries for {root}: {log}"
        );
    }
    assert!(
        entries
            .iter()
            .any(|entry| entry["status"] == json!("staged")
                && entry["stagedResourceIds"]
                    .as_array()
                    .is_some_and(|ids| ids.iter().any(|id| id == "gid://shopify/Job/2"))),
        "successful full sync should stage the ProductFeed and pollable Job IDs: {log}"
    );
}

#[test]
fn product_resource_feedback_validates_mixed_batches_with_per_entry_errors() {
    let mut proxy = snapshot_proxy();
    let response = proxy.process_request(json_graphql_request(
        r#"
        mutation RustProductFeedbackMixedBatch {
          productFeedback: bulkProductResourceFeedbackCreate(feedbackInput: [
            {
              productId: "gid://shopify/Product/optioned",
              state: ACCEPTED,
              feedbackGeneratedAt: "2024-01-01T00:00:00Z",
              productUpdatedAt: "2024-01-01T00:00:00Z",
              messages: ["ready"]
            },
            {
              productId: "gid://shopify/Product/optioned",
              state: ACCEPTED,
              feedbackGeneratedAt: "2100-01-01T00:00:00Z",
              productUpdatedAt: "2024-01-01T00:00:00Z",
              messages: ["future"]
            },
            {
              productId: "gid://shopify/Product/optioned",
              state: REQUIRES_ACTION,
              feedbackGeneratedAt: "2024-01-01T00:00:00Z",
              productUpdatedAt: "2024-01-01T00:00:00Z",
              messages: []
            }
          ]) {
            feedback {
              productId
              state
              messages
              feedbackGeneratedAt
              productUpdatedAt
            }
            userErrors { field message code }
          }
          shopFuture: shopResourceFeedbackCreate(input: {
            state: ACCEPTED,
            feedbackGeneratedAt: "2100-01-01T00:00:00Z",
            messages: ["future"]
          }) {
            feedback { state }
            userErrors { field message code }
          }
          shopSecondMessageTooLong: shopResourceFeedbackCreate(input: {
            state: REQUIRES_ACTION,
            feedbackGeneratedAt: "2024-01-01T00:00:00Z",
            messages: ["ok", "xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"]
          }) {
            feedback { state messages { message } feedbackGeneratedAt }
            userErrors { field message code }
          }
        }
        "#,
        json!({}),
    ));

    assert_eq!(response.status, 200, "response body: {}", response.body);
    assert_eq!(
        response.body["data"]["productFeedback"],
        json!({
            "feedback": [{
                "productId": "gid://shopify/Product/optioned",
                "state": "ACCEPTED",
                "messages": ["ready"],
                "feedbackGeneratedAt": "2024-01-01T00:00:00Z",
                "productUpdatedAt": "2024-01-01T00:00:00Z"
            }],
            "userErrors": [
                {
                    "field": ["feedback", "1", "feedbackGeneratedAt"],
                    "message": "Feedback generated at must not be in the future",
                    "code": "INVALID"
                },
                {
                    "field": ["feedback", "2", "messages"],
                    "message": "Messages can't be blank",
                    "code": "BLANK"
                }
            ]
        })
    );
    assert_eq!(
        response.body["data"]["shopFuture"],
        json!({
            "feedback": Value::Null,
            "userErrors": [{
                "field": ["feedback", "feedbackGeneratedAt"],
                "message": "Feedback generated at must not be in the future",
                "code": "INVALID"
            }]
        })
    );
    assert_eq!(
        response.body["data"]["shopSecondMessageTooLong"],
        json!({
            "feedback": Value::Null,
            "userErrors": [{
                "field": ["feedback", "messages", "1"],
                "message": "Message is too long (maximum is 100 characters)",
                "code": "TOO_LONG"
            }]
        })
    );
}

#[test]
fn product_publication_and_feedback_enum_coercion_errors_do_not_stage_or_log() {
    let mut proxy = snapshot_proxy();
    let default_state = proxy.process_request(json_graphql_request(
        r#"
        mutation RustProductPublicationInvalidDefaultState($input: PublicationCreateInput!) {
          publicationCreate(input: $input) { publication { id } userErrors { field message code } }
        }
        "#,
        json!({ "input": { "defaultState": "BANANAS" } }),
    ));
    assert_eq!(default_state.status, 200);
    assert_eq!(
        default_state.body["errors"][0]["extensions"]["code"],
        json!("INVALID_VARIABLE")
    );
    assert!(default_state.body["errors"][0]["message"]
        .as_str()
        .is_some_and(
            |message| message.contains("Expected \"BANANAS\" to be one of: EMPTY, ALL_PRODUCTS")
        ));

    let product_feedback_enum = proxy.process_request(json_graphql_request(
        r#"
        mutation RustProductFeedbackInvalidEnum {
          bulkProductResourceFeedbackCreate(feedbackInput: [{ productId: "gid://shopify/Product/optioned", state: BANANAS, feedbackGeneratedAt: "2024-01-01T00:00:00Z", productUpdatedAt: "2024-01-01T00:00:00Z", messages: [] }]) {
            feedback { productId }
            userErrors { field message code }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(product_feedback_enum.status, 200);
    assert!(product_feedback_enum.body["errors"][0]["message"]
        .as_str()
        .is_some_and(|message| message.contains("Argument 'state' on InputObject 'ProductResourceFeedbackInput' has an invalid value (BANANAS). Expected type 'ResourceFeedbackState'.")));
    assert_eq!(
        product_feedback_enum.body["errors"][0]["extensions"]["code"],
        json!("argumentLiteralsIncompatible")
    );
    assert_eq!(
        product_feedback_enum.body["errors"][0]["extensions"]["argumentName"],
        json!("state")
    );

    let shop_feedback_enum = proxy.process_request(json_graphql_request(
        r#"
        mutation RustShopFeedbackInvalidEnum {
          shopResourceFeedbackCreate(input: { state: BANANAS, feedbackGeneratedAt: "2024-01-01T00:00:00Z", messages: [] }) {
            feedback { state }
            userErrors { field message code }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(shop_feedback_enum.status, 200);
    assert!(shop_feedback_enum.body["errors"][0]["message"]
        .as_str()
        .is_some_and(|message| message.contains("Argument 'state' on InputObject 'ResourceFeedbackCreateInput' has an invalid value (BANANAS). Expected type 'ResourceFeedbackState'.")));
    assert_eq!(
        shop_feedback_enum.body["errors"][0]["extensions"]["code"],
        json!("argumentLiteralsIncompatible")
    );
    assert_eq!(proxy.get_log_snapshot(), json!({ "entries": [] }));
}

#[test]
fn product_reorder_media_replays_captured_job_and_downstream_order() {
    let mut proxy = snapshot_proxy();

    let mutation = proxy.process_request(json_graphql_request(
        r#"
        mutation ProductReorderMediaParity($id: ID!, $moves: [MoveInput!]!) {
          productReorderMedia(id: $id, moves: $moves) {
            job { id done }
            mediaUserErrors { field message }
          }
        }
        "#,
        json!({
            "id": "gid://shopify/Product/10170568147250",
            "moves": [
                { "id": "gid://shopify/MediaImage/43607668621618", "newPosition": "0" },
                { "id": "gid://shopify/MediaImage/43607668588850", "newPosition": "99" }
            ]
        }),
    ));
    assert_eq!(mutation.status, 200);
    assert_eq!(
        mutation.body["data"]["productReorderMedia"]["job"]["done"],
        json!(false)
    );
    assert_eq!(
        mutation.body["data"]["productReorderMedia"]["mediaUserErrors"],
        json!([])
    );

    let read = proxy.process_request(json_graphql_request(
        r#"
        query ProductReorderMediaDownstreamRead($id: ID!) {
          product(id: $id) {
            id
            media(first: 10) { nodes { id alt mediaContentType status } }
            images(first: 10) { nodes { id altText } }
          }
        }
        "#,
        json!({ "id": "gid://shopify/Product/10170568147250" }),
    ));
    assert_eq!(read.status, 200);
    assert_eq!(
        read.body["data"]["product"]["media"]["nodes"],
        json!([
            {
                "id": "gid://shopify/MediaImage/43607668621618",
                "alt": "Back",
                "mediaContentType": "IMAGE",
                "status": "PROCESSING"
            },
            {
                "id": "gid://shopify/MediaImage/43607668588850",
                "alt": "Front",
                "mediaContentType": "IMAGE",
                "status": "PROCESSING"
            }
        ])
    );
    assert_eq!(read.body["data"]["product"]["images"]["nodes"], json!([]));
}

#[test]
fn product_create_and_delete_media_replay_captured_mutations_and_downstream_reads() {
    let mut proxy = snapshot_proxy();

    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation ProductCreateMediaParityPlan($productId: ID!, $media: [CreateMediaInput!]!) {
          productCreateMedia(productId: $productId, media: $media) {
            media { id alt mediaContentType status preview { image { url } } ... on MediaImage { image { url } } }
            mediaUserErrors { field message }
            product { id media(first: 10) { nodes { id alt mediaContentType status preview { image { url } } ... on MediaImage { image { url } } } } }
          }
        }
        "#,
        json!({
            "productId": "gid://shopify/Product/9257219162345",
            "media": [{
                "mediaContentType": "IMAGE",
                "originalSource": "https://placehold.co/600x400/png",
                "alt": "Front view"
            }]
        }),
    ));
    assert_eq!(create.status, 200);
    let created_media_id = create.body["data"]["productCreateMedia"]["media"][0]["id"]
        .as_str()
        .expect("productCreateMedia media id should be a string")
        .to_string();
    assert!(created_media_id.starts_with("gid://shopify/MediaImage/"));
    assert!(created_media_id.contains("shopify-draft-proxy=synthetic"));
    assert_eq!(
        create.body["data"]["productCreateMedia"]["media"][0],
        json!({
            "id": created_media_id,
            "alt": "Front view",
            "mediaContentType": "IMAGE",
            "status": "UPLOADED",
            "preview": { "image": null },
            "image": null
        })
    );
    assert_eq!(
        create.body["data"]["productCreateMedia"]["product"]["media"]["nodes"][0]["status"],
        json!("UPLOADED")
    );
    assert_eq!(
        create.body["data"]["productCreateMedia"]["product"]["media"]["nodes"][0]["id"],
        json!(created_media_id)
    );

    let create_read = proxy.process_request(json_graphql_request(
        r#"
        query ProductCreateMediaDownstreamRead($id: ID!) {
          product(id: $id) { id media(first: 10) { nodes { id alt mediaContentType status preview { image { url } } ... on MediaImage { image { url } } } } }
        }
        "#,
        json!({ "id": "gid://shopify/Product/9257219162345" }),
    ));
    assert_eq!(create_read.status, 200);
    assert_eq!(
        create_read.body["data"]["product"]["media"]["nodes"][0],
        json!({
            "id": created_media_id,
            "alt": "Front view",
            "mediaContentType": "IMAGE",
            "status": "PROCESSING",
            "preview": { "image": null },
            "image": null
        })
    );

    let delete = proxy.process_request(json_graphql_request(
        r#"
        mutation ProductDeleteMediaParityPlan($productId: ID!, $mediaIds: [ID!]!) {
          productDeleteMedia(productId: $productId, mediaIds: $mediaIds) {
            deletedMediaIds
            deletedProductImageIds
            mediaUserErrors { field message }
            product { id media(first: 10) { nodes { id alt mediaContentType status preview { image { url } } ... on MediaImage { image { url } } } } }
          }
        }
        "#,
        json!({
            "productId": "gid://shopify/Product/9257219162345",
            "mediaIds": ["gid://shopify/MediaImage/39467722375401"]
        }),
    ));
    assert_eq!(delete.status, 200);
    assert_eq!(
        delete.body["data"]["productDeleteMedia"],
        json!({
            "deletedMediaIds": ["gid://shopify/MediaImage/39467722375401"],
            "deletedProductImageIds": ["gid://shopify/ProductImage/48929036730601"],
            "mediaUserErrors": [],
            "product": {
                "id": "gid://shopify/Product/9257219162345",
                "media": { "nodes": [] }
            }
        })
    );

    let delete_read = proxy.process_request(json_graphql_request(
        r#"
        query ProductDeleteMediaDownstreamRead($productId: ID!) {
          product(id: $productId) { id media(first: 10) { nodes { id alt mediaContentType status preview { image { url } } ... on MediaImage { image { url } } } } }
        }
        "#,
        json!({ "productId": "gid://shopify/Product/9257219162345" }),
    ));
    assert_eq!(delete_read.status, 200);
    assert_eq!(
        delete_read.body["data"]["product"],
        json!({
            "id": "gid://shopify/Product/9257219162345",
            "media": { "nodes": [] }
        })
    );
}

#[test]
fn product_update_media_replays_captured_mutation_and_downstream_product_media() {
    let mut proxy = snapshot_proxy();

    let mutation = proxy.process_request(json_graphql_request(
        r#"
        mutation ProductUpdateMediaParityPlan($productId: ID!, $media: [UpdateMediaInput!]!) {
          productUpdateMedia(productId: $productId, media: $media) {
            media {
              id
              alt
              mediaContentType
              status
              preview { image { url } }
              ... on MediaImage { image { url } }
            }
            mediaUserErrors { field message }
          }
        }
        "#,
        json!({
            "productId": "gid://shopify/Product/9257219162345",
            "media": [{
                "id": "gid://shopify/MediaImage/39467722375401",
                "alt": "Updated front view"
            }]
        }),
    ));
    assert_eq!(mutation.status, 200);
    assert_eq!(
        mutation.body["data"]["productUpdateMedia"],
        json!({
            "media": [{
                "id": "gid://shopify/MediaImage/39467722375401",
                "alt": "Updated front view",
                "mediaContentType": "IMAGE",
                "status": "READY",
                "preview": { "image": { "url": "https://cdn.shopify.com/s/files/1/0637/5541/9881/files/png.png?v=1776550664" } },
                "image": { "url": "https://cdn.shopify.com/s/files/1/0637/5541/9881/files/png.png?v=1776550664" }
            }],
            "mediaUserErrors": []
        })
    );

    let read = proxy.process_request(json_graphql_request(
        r#"
        query ProductUpdateMediaDownstreamRead($productId: ID!) {
          product(id: $productId) {
            id
            media(first: 10) {
              nodes {
                id
                alt
                mediaContentType
                status
                preview { image { url } }
                ... on MediaImage { image { url } }
              }
            }
          }
        }
        "#,
        json!({ "productId": "gid://shopify/Product/9257219162345" }),
    ));
    assert_eq!(read.status, 200);
    assert_eq!(
        read.body["data"]["product"],
        json!({
            "id": "gid://shopify/Product/9257219162345",
            "media": { "nodes": [{
                "id": "gid://shopify/MediaImage/39467722375401",
                "alt": "Updated front view",
                "mediaContentType": "IMAGE",
                "status": "READY",
                "preview": { "image": { "url": "https://cdn.shopify.com/s/files/1/0637/5541/9881/files/png.png?v=1776550664" } },
                "image": { "url": "https://cdn.shopify.com/s/files/1/0637/5541/9881/files/png.png?v=1776550664" }
            }] }
        })
    );
}

#[test]
fn product_publication_aggregate_downstream_read_returns_captured_product_shape() {
    let mut proxy = snapshot_proxy();
    let response = proxy.process_request(json_graphql_request(
        r#"
        query ProductPublicationAggregateDownstream($id: ID!) {
          product(id: $id) {
            id
            publishedOnCurrentPublication
            availablePublicationsCount { count precision }
            resourcePublicationsCount { count precision }
          }
        }
        "#,
        json!({ "id": "gid://shopify/Product/9264105488617" }),
    ));

    assert_eq!(response.status, 200);
    assert_eq!(
        response.body["data"]["product"],
        json!({
            "id": "gid://shopify/Product/9264105488617",
            "publishedOnCurrentPublication": false,
            "availablePublicationsCount": { "count": 0, "precision": "EXACT" },
            "resourcePublicationsCount": { "count": 0, "precision": "EXACT" }
        })
    );
}

#[test]
fn product_publishable_mutations_return_captured_aggregate_shape() {
    let mut proxy = snapshot_proxy();
    for (root, query) in [
        (
            "publishablePublish",
            r#"
            mutation PublishablePublishProductParity($id: ID!, $input: [PublicationInput!]!) {
              publishablePublish(id: $id, input: $input) {
                publishable { ... on Product { id publishedOnCurrentPublication availablePublicationsCount { count precision } resourcePublicationsCount { count precision } } }
                userErrors { field message }
              }
            }
            "#,
        ),
        (
            "publishableUnpublish",
            r#"
            mutation PublishableUnpublishProductParity($id: ID!, $input: [PublicationInput!]!) {
              publishableUnpublish(id: $id, input: $input) {
                publishable { ... on Product { id publishedOnCurrentPublication availablePublicationsCount { count precision } resourcePublicationsCount { count precision } } }
                userErrors { field message }
              }
            }
            "#,
        ),
        (
            "publishablePublishToCurrentChannel",
            r#"
            mutation PublishablePublishToCurrentChannelProductParity($id: ID!) {
              publishablePublishToCurrentChannel(id: $id) {
                publishable { ... on Product { id publishedOnCurrentPublication availablePublicationsCount { count precision } resourcePublicationsCount { count precision } } }
                userErrors { field message }
              }
            }
            "#,
        ),
        (
            "publishableUnpublishToCurrentChannel",
            r#"
            mutation PublishableUnpublishToCurrentChannelProductParity($id: ID!) {
              publishableUnpublishToCurrentChannel(id: $id) {
                publishable { ... on Product { id publishedOnCurrentPublication availablePublicationsCount { count precision } resourcePublicationsCount { count precision } } }
                userErrors { field message }
              }
            }
            "#,
        ),
    ] {
        let response = proxy.process_request(json_graphql_request(
            query,
            json!({
                "id": "gid://shopify/Product/9264105488617",
                "input": [{ "publicationId": "gid://shopify/Publication/82090459369" }]
            }),
        ));
        assert_eq!(
            response.body["data"][root]["publishable"],
            json!({
                "id": "gid://shopify/Product/9264105488617",
                "publishedOnCurrentPublication": false,
                "availablePublicationsCount": { "count": 0, "precision": "EXACT" },
                "resourcePublicationsCount": { "count": 0, "precision": "EXACT" }
            })
        );
        assert_eq!(response.body["data"][root]["userErrors"], json!([]));
    }
}

#[test]
fn publishable_mutations_validate_publication_input_locally() {
    let mut proxy = snapshot_proxy();
    let product_id = "gid://shopify/Product/10179659858226";
    let publication_id = "gid://shopify/Publication/268039389490";
    let publish = r#"
        mutation PublishableInputValidation($id: ID!, $input: [PublicationInput!]!) {
          publishablePublish(id: $id, input: $input) {
            publishable { ... on Product { id publishedOnCurrentPublication resourcePublicationsCount { count precision } } }
            userErrors { field message }
          }
        }
    "#;
    let unpublish = r#"
        mutation PublishableInputValidationUnpublish($id: ID!, $input: [PublicationInput!]!) {
          publishableUnpublish(id: $id, input: $input) {
            publishable { ... on Product { id publishedOnCurrentPublication resourcePublicationsCount { count precision } } }
            userErrors { field message }
          }
        }
    "#;

    for (query, root) in [
        (publish, "publishablePublish"),
        (unpublish, "publishableUnpublish"),
    ] {
        let duplicate = proxy.process_request(json_graphql_request(
            query,
            json!({ "id": product_id, "input": [{ "publicationId": publication_id }, { "publicationId": publication_id }] }),
        ));
        assert_eq!(
            duplicate.body["data"][root]["userErrors"],
            json!([{
                "field": ["input", "1", "publicationId"],
                "message": "The same publication was specified more than once"
            }])
        );

        let past_date = proxy.process_request(json_graphql_request(
            query,
            json!({ "id": product_id, "input": [{ "publicationId": publication_id, "publishDate": "1900-01-01T00:00:00Z" }] }),
        ));
        assert_eq!(
            past_date.body["data"][root]["userErrors"],
            json!([{
                "field": ["input", "0", "publishDate"],
                "message": "Publish date must be a date after the year 1969"
            }])
        );

        let blank = proxy.process_request(json_graphql_request(
            query,
            json!({ "id": product_id, "input": [{}] }),
        ));
        assert_eq!(
            blank.body["data"][root]["userErrors"],
            json!([{
                "field": ["input", "0", "publicationId"],
                "message": "PublicationId cannot be empty"
            }])
        );

        let unknown = proxy.process_request(json_graphql_request(
            query,
            json!({ "id": product_id, "input": [{ "publicationId": "gid://shopify/Publication/999999999999" }] }),
        ));
        assert_eq!(
            unknown.body["data"][root]["userErrors"],
            json!([{
                "field": ["input", "0", "publicationId"],
                "message": "Publication does not exist or is not publishable"
            }])
        );

        let empty_string = proxy.process_request(json_graphql_request(
            query,
            json!({ "id": product_id, "input": [{ "publicationId": "" }] }),
        ));
        assert_eq!(empty_string.body.get("data"), None);
        assert_eq!(
            empty_string.body["errors"][0]["extensions"]["code"],
            json!("INVALID_VARIABLE")
        );
        assert_eq!(
            empty_string.body["errors"][0]["extensions"]["problems"][0]["path"],
            json!([0, "publicationId"])
        );
        assert_eq!(
            empty_string.body["errors"][0]["extensions"]["problems"][0]["message"],
            json!("Invalid global id ''")
        );
    }

    let log = proxy.process_request(Request {
        method: "GET".to_string(),
        path: "/__meta/log".to_string(),
        headers: Default::default(),
        body: String::new(),
    });
    assert_eq!(log.body["entries"], json!([]));
}

#[test]
fn product_create_blank_title_user_errors_match_public_shape_and_selected_fields() {
    let mut proxy = snapshot_proxy();

    let public_shape = proxy.process_request(json_graphql_request(
        r#"
        mutation ProductUserErrorShapeProductCreate($product: ProductCreateInput!) {
          productCreate(product: $product) {
            product { id }
            userErrors { field message }
          }
        }
        "#,
        json!({ "product": { "title": "" } }),
    ));
    assert_eq!(
        public_shape.body["data"]["productCreate"],
        json!({
            "product": null,
            "userErrors": [{ "field": ["title"], "message": "Title can't be blank" }]
        })
    );

    let local_code_projection = proxy.process_request(json_graphql_request(
        r#"
        mutation ProductCreateBlankTitleWithCode($product: ProductCreateInput!) {
          productCreate(product: $product) {
            product { id }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "product": { "title": "" } }),
    ));
    assert_eq!(
        local_code_projection.body["data"]["productCreate"],
        json!({
            "product": null,
            "userErrors": [{ "field": ["title"], "message": "Title can't be blank", "code": "BLANK" }]
        })
    );
}

#[test]
fn product_create_legacy_input_id_and_variants_validation_matches_2026_04_shapes() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/products/product-create-no-key-on-create.json"
    ))
    .unwrap();
    let query = include_str!(
        "../../config/parity-requests/products/product-create-no-key-on-create.graphql"
    );
    let mut proxy = snapshot_proxy();

    for scenario in ["inputId", "inputIdBeforeBlankTitle"] {
        let response = proxy.process_request(json_graphql_request(
            query,
            fixture["scenarios"][scenario]["variables"].clone(),
        ));
        assert_eq!(
            response.body["data"]["productCreate"],
            fixture["scenarios"][scenario]["response"]["data"]["productCreate"]
        );
    }

    let variants_response = proxy.process_request(json_graphql_request(
        query,
        fixture["scenarios"]["variantProductId"]["variables"].clone(),
    ));
    assert_eq!(
        variants_response.body["errors"][0]["message"],
        fixture["scenarios"]["variantProductId"]["response"]["errors"][0]["message"]
    );
    assert_eq!(
        variants_response.body["errors"][0]["extensions"],
        fixture["scenarios"]["variantProductId"]["response"]["errors"][0]["extensions"]
    );
}

#[test]
fn product_create_serializes_only_requested_payload_fields() {
    let mut proxy = snapshot_proxy();

    let create = proxy.process_request(graphql_request(
        "POST",
        r#"{"query":"mutation { productCreate(product: { title: \"Selection product\" }) { product { title } } }"}"#,
    ));

    assert_eq!(create.status, 200);
    assert_eq!(
        create.body,
        json!({
            "data": {
                "productCreate": {
                    "product": {
                        "title": "Selection product"
                    }
                }
            }
        })
    );
}

#[test]
fn product_create_stages_extended_product_scalars_visible_to_product_read() {
    let mut proxy = snapshot_proxy();

    let create = proxy.process_request(graphql_request(
        "POST",
        r#"{"query":"mutation { productCreate(product: { title: \"Extended product\", descriptionHtml: \"<p>Rich</p>\", vendor: \"Hermes\", productType: \"Accessory\", tags: [\"alpha\", \"beta\"] }) { product { title descriptionHtml vendor productType tags } userErrors { field message code } } }"}"#,
    ));

    assert_eq!(create.status, 200);
    assert_eq!(
        create.body,
        json!({
            "data": {
                "productCreate": {
                    "product": {
                        "title": "Extended product",
                        "descriptionHtml": "<p>Rich</p>",
                        "vendor": "Hermes",
                        "productType": "Accessory",
                        "tags": ["alpha", "beta"]
                    },
                    "userErrors": []
                }
            }
        })
    );

    let read_back = proxy.process_request(graphql_request(
        "POST",
        r#"{"query":"query { product(id: \"gid://shopify/Product/1?shopify-draft-proxy=synthetic\") { descriptionHtml vendor productType tags } }"}"#,
    ));

    assert_eq!(read_back.status, 200);
    assert_eq!(
        read_back.body,
        json!({
            "data": {
                "product": {
                    "descriptionHtml": "<p>Rich</p>",
                    "vendor": "Hermes",
                    "productType": "Accessory",
                    "tags": ["alpha", "beta"]
                }
            }
        })
    );
}

#[test]
fn product_update_stages_scalar_changes_visible_to_product_read() {
    let mut proxy = snapshot_proxy().with_base_products(vec![ProductRecord {
        id: "gid://shopify/Product/1".to_string(),
        created_at: "2024-01-01T00:00:00.000Z".to_string(),
        updated_at: "2024-01-01T00:00:00.000Z".to_string(),
        title: "Original product".to_string(),
        handle: "original-product".to_string(),
        status: "ACTIVE".to_string(),
        description_html: "<p>Original</p>".to_string(),
        vendor: "Original vendor".to_string(),
        product_type: "Original type".to_string(),
        tags: vec!["old".to_string()],
        template_suffix: String::new(),
        seo_title: String::new(),
        seo_description: String::new(),
        ..ProductRecord::default()
    }]);

    let update = proxy.process_request(graphql_request(
        "POST",
        r#"{"query":"mutation { productUpdate(product: { id: \"gid://shopify/Product/1\", title: \"Updated product\", handle: \"updated-product\", status: DRAFT, descriptionHtml: \"<p>Updated</p>\", vendor: \"Hermes\", productType: \"Accessory\", tags: [\"alpha\", \"beta\"] }) { product { id title handle status descriptionHtml vendor productType tags } userErrors { field message code } } }"}"#,
    ));

    assert_eq!(update.status, 200);
    assert_eq!(
        update.body,
        json!({
            "data": {
                "productUpdate": {
                    "product": {
                        "id": "gid://shopify/Product/1",
                        "title": "Updated product",
                        "handle": "updated-product",
                        "status": "DRAFT",
                        "descriptionHtml": "<p>Updated</p>",
                        "vendor": "Hermes",
                        "productType": "Accessory",
                        "tags": ["alpha", "beta"]
                    },
                    "userErrors": []
                }
            }
        })
    );

    let read_back = proxy.process_request(graphql_request(
        "POST",
        r#"{"query":"query { product(id: \"gid://shopify/Product/1\") { title handle status descriptionHtml vendor productType tags } }"}"#,
    ));

    assert_eq!(read_back.status, 200);
    assert_eq!(
        read_back.body,
        json!({
            "data": {
                "product": {
                    "title": "Updated product",
                    "handle": "updated-product",
                    "status": "DRAFT",
                    "descriptionHtml": "<p>Updated</p>",
                    "vendor": "Hermes",
                    "productType": "Accessory",
                    "tags": ["alpha", "beta"]
                }
            }
        })
    );
}

#[test]
fn products_connection_reflects_staged_creates_and_deletes() {
    let mut proxy = snapshot_proxy().with_base_products(vec![ProductRecord {
        id: "gid://shopify/Product/base".to_string(),
        created_at: "2024-01-01T00:00:00.000Z".to_string(),
        updated_at: "2024-01-01T00:00:00.000Z".to_string(),
        title: "Base product".to_string(),
        handle: "base-product".to_string(),
        status: "ACTIVE".to_string(),
        description_html: String::new(),
        vendor: String::new(),
        product_type: String::new(),
        tags: Vec::new(),
        template_suffix: String::new(),
        seo_title: String::new(),
        seo_description: String::new(),
        ..ProductRecord::default()
    }]);

    let create = proxy.process_request(graphql_request(
        "POST",
        r#"{"query":"mutation { productCreate(product: { title: \"Created product\", handle: \"created-product\" }) { product { id } userErrors { field message code } } }"}"#,
    ));
    assert_eq!(create.status, 200);

    let list_after_create = proxy.process_request(graphql_request(
        "POST",
        r#"{"query":"query { products(first: 10) { nodes { id title handle } } }"}"#,
    ));
    assert_eq!(list_after_create.status, 200);
    assert_eq!(
        list_after_create.body,
        json!({
            "data": {
                "products": {
                    "nodes": [
                        {
                            "id": "gid://shopify/Product/base",
                            "title": "Base product",
                            "handle": "base-product"
                        },
                        {
                            "id": "gid://shopify/Product/1?shopify-draft-proxy=synthetic",
                            "title": "Created product",
                            "handle": "created-product"
                        }
                    ]
                }
            }
        })
    );

    let delete = proxy.process_request(graphql_request(
        "POST",
        r#"{"query":"mutation { productDelete(input: { id: \"gid://shopify/Product/base\" }) { deletedProductId userErrors { field message code } } }"}"#,
    ));
    assert_eq!(delete.status, 200);

    let list_after_delete = proxy.process_request(graphql_request(
        "POST",
        r#"{"query":"query { products(first: 10) { nodes { id title handle } } }"}"#,
    ));
    assert_eq!(list_after_delete.status, 200);
    assert_eq!(
        list_after_delete.body,
        json!({
            "data": {
                "products": {
                    "nodes": [
                        {
                            "id": "gid://shopify/Product/1?shopify-draft-proxy=synthetic",
                            "title": "Created product",
                            "handle": "created-product"
                        }
                    ]
                }
            }
        })
    );
}

#[test]
fn products_connection_applies_first_limit_after_overlaying_state() {
    let mut proxy = snapshot_proxy().with_base_products(vec![
        ProductRecord {
            id: "gid://shopify/Product/1".to_string(),
            created_at: "2024-01-01T00:00:00.000Z".to_string(),
            updated_at: "2024-01-01T00:00:00.000Z".to_string(),
            title: "First product".to_string(),
            handle: "first-product".to_string(),
            status: "ACTIVE".to_string(),
            description_html: String::new(),
            vendor: String::new(),
            product_type: String::new(),
            tags: Vec::new(),
            template_suffix: String::new(),
            seo_title: String::new(),
            seo_description: String::new(),
            ..ProductRecord::default()
        },
        ProductRecord {
            id: "gid://shopify/Product/2".to_string(),
            created_at: "2024-01-01T00:00:00.000Z".to_string(),
            updated_at: "2024-01-01T00:00:00.000Z".to_string(),
            title: "Second product".to_string(),
            handle: "second-product".to_string(),
            status: "ACTIVE".to_string(),
            description_html: String::new(),
            vendor: String::new(),
            product_type: String::new(),
            tags: Vec::new(),
            template_suffix: String::new(),
            seo_title: String::new(),
            seo_description: String::new(),
            ..ProductRecord::default()
        },
    ]);

    let first_only = proxy.process_request(graphql_request(
        "POST",
        r#"{"query":"query { products(first: 1) { nodes { id title } } }"}"#,
    ));

    assert_eq!(first_only.status, 200);
    assert_eq!(
        first_only.body,
        json!({
            "data": {
                "products": {
                    "nodes": [
                        {
                            "id": "gid://shopify/Product/1",
                            "title": "First product"
                        }
                    ]
                }
            }
        })
    );
}

#[test]
fn products_connection_serializes_edges_and_page_info_for_selected_window() {
    let mut proxy = snapshot_proxy().with_base_products(vec![
        ProductRecord {
            id: "gid://shopify/Product/1".to_string(),
            created_at: "2024-01-01T00:00:00.000Z".to_string(),
            updated_at: "2024-01-01T00:00:00.000Z".to_string(),
            title: "First product".to_string(),
            handle: "first-product".to_string(),
            status: "ACTIVE".to_string(),
            description_html: String::new(),
            vendor: String::new(),
            product_type: String::new(),
            tags: Vec::new(),
            template_suffix: String::new(),
            seo_title: String::new(),
            seo_description: String::new(),
            ..ProductRecord::default()
        },
        ProductRecord {
            id: "gid://shopify/Product/2".to_string(),
            created_at: "2024-01-01T00:00:00.000Z".to_string(),
            updated_at: "2024-01-01T00:00:00.000Z".to_string(),
            title: "Second product".to_string(),
            handle: "second-product".to_string(),
            status: "ACTIVE".to_string(),
            description_html: String::new(),
            vendor: String::new(),
            product_type: String::new(),
            tags: Vec::new(),
            template_suffix: String::new(),
            seo_title: String::new(),
            seo_description: String::new(),
            ..ProductRecord::default()
        },
    ]);

    let connection = proxy.process_request(graphql_request(
        "POST",
        r#"{"query":"query { products(first: 2) { edges { cursor node { id title } } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } }"}"#,
    ));

    assert_eq!(connection.status, 200);
    assert_eq!(
        connection.body,
        json!({
            "data": {
                "products": {
                    "edges": [
                        {
                            "cursor": "gid://shopify/Product/1",
                            "node": {
                                "id": "gid://shopify/Product/1",
                                "title": "First product"
                            }
                        },
                        {
                            "cursor": "gid://shopify/Product/2",
                            "node": {
                                "id": "gid://shopify/Product/2",
                                "title": "Second product"
                            }
                        }
                    ],
                    "pageInfo": {
                        "hasNextPage": false,
                        "hasPreviousPage": false,
                        "startCursor": "gid://shopify/Product/1",
                        "endCursor": "gid://shopify/Product/2"
                    }
                }
            }
        })
    );
}

#[test]
fn products_connection_paginates_edges_nodes_and_page_info_consistently() {
    let mut proxy = snapshot_proxy().with_base_products(vec![
        ProductRecord {
            id: "gid://shopify/Product/1".to_string(),
            created_at: "2024-01-01T00:00:01Z".to_string(),
            updated_at: "2024-01-01T00:00:01Z".to_string(),
            title: "First product".to_string(),
            handle: "first-product".to_string(),
            status: "ACTIVE".to_string(),
            description_html: String::new(),
            vendor: String::new(),
            product_type: String::new(),
            tags: Vec::new(),
            template_suffix: String::new(),
            seo_title: String::new(),
            seo_description: String::new(),
            ..ProductRecord::default()
        },
        ProductRecord {
            id: "gid://shopify/Product/2".to_string(),
            created_at: "2024-01-01T00:00:02Z".to_string(),
            updated_at: "2024-01-01T00:00:02Z".to_string(),
            title: "Second product".to_string(),
            handle: "second-product".to_string(),
            status: "ACTIVE".to_string(),
            description_html: String::new(),
            vendor: String::new(),
            product_type: String::new(),
            tags: Vec::new(),
            template_suffix: String::new(),
            seo_title: String::new(),
            seo_description: String::new(),
            ..ProductRecord::default()
        },
        ProductRecord {
            id: "gid://shopify/Product/3".to_string(),
            created_at: "2024-01-01T00:00:03Z".to_string(),
            updated_at: "2024-01-01T00:00:03Z".to_string(),
            title: "Third product".to_string(),
            handle: "third-product".to_string(),
            status: "ACTIVE".to_string(),
            description_html: String::new(),
            vendor: String::new(),
            product_type: String::new(),
            tags: Vec::new(),
            template_suffix: String::new(),
            seo_title: String::new(),
            seo_description: String::new(),
            ..ProductRecord::default()
        },
    ]);

    let first_page = proxy.process_request(json_graphql_request(
        r#"
        query ProductConnectionRelayPage($first: Int!) {
          products(first: $first) {
            nodes { id title }
            edges { cursor node { id title } }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
        }
        "#,
        json!({"first": 2}),
    ));
    assert_eq!(
        first_page.body["data"]["products"],
        json!({
            "nodes": [
                {"id": "gid://shopify/Product/1", "title": "First product"},
                {"id": "gid://shopify/Product/2", "title": "Second product"}
            ],
            "edges": [
                {"cursor": "gid://shopify/Product/1", "node": {"id": "gid://shopify/Product/1", "title": "First product"}},
                {"cursor": "gid://shopify/Product/2", "node": {"id": "gid://shopify/Product/2", "title": "Second product"}}
            ],
            "pageInfo": {
                "hasNextPage": true,
                "hasPreviousPage": false,
                "startCursor": "gid://shopify/Product/1",
                "endCursor": "gid://shopify/Product/2"
            }
        })
    );

    let second_page = proxy.process_request(json_graphql_request(
        r#"
        query ProductConnectionRelayAfter($first: Int!, $after: String!) {
          products(first: $first, after: $after) {
            nodes { id title }
            edges { cursor node { id title } }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
        }
        "#,
        json!({"first": 2, "after": first_page.body["data"]["products"]["pageInfo"]["endCursor"]}),
    ));
    assert_eq!(
        second_page.body["data"]["products"],
        json!({
            "nodes": [{"id": "gid://shopify/Product/3", "title": "Third product"}],
            "edges": [{"cursor": "gid://shopify/Product/3", "node": {"id": "gid://shopify/Product/3", "title": "Third product"}}],
            "pageInfo": {
                "hasNextPage": false,
                "hasPreviousPage": true,
                "startCursor": "gid://shopify/Product/3",
                "endCursor": "gid://shopify/Product/3"
            }
        })
    );

    let tail = proxy.process_request(json_graphql_request(
        r#"
        query ProductConnectionRelayLast($last: Int!, $before: String!) {
          products(last: $last, before: $before) {
            nodes { id }
            edges { cursor node { id } }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
        }
        "#,
        json!({"last": 1, "before": "gid://shopify/Product/3"}),
    ));
    assert_eq!(
        tail.body["data"]["products"],
        json!({
            "nodes": [{"id": "gid://shopify/Product/2"}],
            "edges": [{"cursor": "gid://shopify/Product/2", "node": {"id": "gid://shopify/Product/2"}}],
            "pageInfo": {
                "hasNextPage": true,
                "hasPreviousPage": true,
                "startCursor": "gid://shopify/Product/2",
                "endCursor": "gid://shopify/Product/2"
            }
        })
    );
}

#[test]
fn products_count_reflects_staged_creates_and_deletes() {
    let mut proxy = snapshot_proxy().with_base_products(vec![ProductRecord {
        id: "gid://shopify/Product/base".to_string(),
        created_at: "2024-01-01T00:00:00.000Z".to_string(),
        updated_at: "2024-01-01T00:00:00.000Z".to_string(),
        title: "Base product".to_string(),
        handle: "base-product".to_string(),
        status: "ACTIVE".to_string(),
        description_html: String::new(),
        vendor: String::new(),
        product_type: String::new(),
        tags: Vec::new(),
        template_suffix: String::new(),
        seo_title: String::new(),
        seo_description: String::new(),
        ..ProductRecord::default()
    }]);

    let create = proxy.process_request(graphql_request(
        "POST",
        r#"{"query":"mutation { productCreate(product: { title: \"Created product\" }) { product { id } } }"}"#,
    ));
    assert_eq!(create.status, 200);

    let count_after_create = proxy.process_request(graphql_request(
        "POST",
        r#"{"query":"query { productsCount { count precision } }"}"#,
    ));
    assert_eq!(count_after_create.status, 200);
    assert_eq!(
        count_after_create.body,
        json!({
            "data": {
                "productsCount": {
                    "count": 2,
                    "precision": "EXACT"
                }
            }
        })
    );

    let delete = proxy.process_request(graphql_request(
        "POST",
        r#"{"query":"mutation { productDelete(input: { id: \"gid://shopify/Product/base\" }) { deletedProductId } }"}"#,
    ));
    assert_eq!(delete.status, 200);

    let count_after_delete = proxy.process_request(graphql_request(
        "POST",
        r#"{"query":"query { productsCount { count precision } }"}"#,
    ));
    assert_eq!(count_after_delete.status, 200);
    assert_eq!(
        count_after_delete.body,
        json!({
            "data": {
                "productsCount": {
                    "count": 1,
                    "precision": "EXACT"
                }
            }
        })
    );
}

#[test]
fn product_by_identifier_finds_staged_product_by_handle() {
    let mut proxy = snapshot_proxy();

    let create = proxy.process_request(graphql_request(
        "POST",
        r#"{"query":"mutation { productCreate(product: { title: \"Identifier product\", handle: \"identifier-product\" }) { product { id } } }"}"#,
    ));
    assert_eq!(create.status, 200);

    let by_handle = proxy.process_request(graphql_request(
        "POST",
        r#"{"query":"query { productByIdentifier(identifier: { handle: \"identifier-product\" }) { id title handle } }"}"#,
    ));

    assert_eq!(by_handle.status, 200);
    assert_eq!(
        by_handle.body,
        json!({
            "data": {
                "productByIdentifier": {
                    "id": "gid://shopify/Product/1?shopify-draft-proxy=synthetic",
                    "title": "Identifier product",
                    "handle": "identifier-product"
                }
            }
        })
    );
}

#[test]
fn product_by_identifier_preserves_root_alias() {
    let mut proxy = snapshot_proxy().with_base_products(vec![ProductRecord {
        id: "gid://shopify/Product/base".to_string(),
        created_at: "2024-01-01T00:00:00.000Z".to_string(),
        updated_at: "2024-01-01T00:00:00.000Z".to_string(),
        title: "Base product".to_string(),
        handle: "base-product".to_string(),
        status: "ACTIVE".to_string(),
        description_html: String::new(),
        vendor: String::new(),
        product_type: String::new(),
        tags: Vec::new(),
        template_suffix: String::new(),
        seo_title: String::new(),
        seo_description: String::new(),
        ..ProductRecord::default()
    }]);

    let by_handle = proxy.process_request(graphql_request(
        "POST",
        r#"{"query":"query { byHandle: productByIdentifier(identifier: { handle: \"base-product\" }) { id title handle } }"}"#,
    ));

    assert_eq!(by_handle.status, 200);
    assert_eq!(
        by_handle.body,
        json!({
            "data": {
                "byHandle": {
                    "id": "gid://shopify/Product/base",
                    "title": "Base product",
                    "handle": "base-product"
                }
            }
        })
    );
}

#[test]
fn product_by_identifier_supports_multiple_aliases_in_one_query() {
    let mut proxy = snapshot_proxy().with_base_products(vec![ProductRecord {
        id: "gid://shopify/Product/base".to_string(),
        created_at: "2024-01-01T00:00:00.000Z".to_string(),
        updated_at: "2024-01-01T00:00:00.000Z".to_string(),
        title: "Base product".to_string(),
        handle: "base-product".to_string(),
        status: "ACTIVE".to_string(),
        description_html: String::new(),
        vendor: String::new(),
        product_type: String::new(),
        tags: Vec::new(),
        template_suffix: String::new(),
        seo_title: String::new(),
        seo_description: String::new(),
        ..ProductRecord::default()
    }]);

    let create = proxy.process_request(graphql_request(
        "POST",
        r#"{"query":"mutation { productCreate(product: { title: \"Created product\", handle: \"created-product\" }) { product { id } } }"}"#,
    ));
    assert_eq!(create.status, 200);

    let by_identifiers = proxy.process_request(graphql_request(
        "POST",
        r#"{"query":"query { byId: productByIdentifier(identifier: { id: \"gid://shopify/Product/base\" }) { id title } byHandle: productByIdentifier(identifier: { handle: \"created-product\" }) { id handle } missing: productByIdentifier(identifier: { handle: \"missing-product\" }) { id } }"}"#,
    ));

    assert_eq!(by_identifiers.status, 200);
    assert_eq!(
        by_identifiers.body,
        json!({
            "data": {
                "byId": {
                    "id": "gid://shopify/Product/base",
                    "title": "Base product"
                },
                "byHandle": {
                    "id": "gid://shopify/Product/1?shopify-draft-proxy=synthetic",
                    "handle": "created-product"
                },
                "missing": null
            }
        })
    );
}

#[test]
fn products_and_products_count_preserve_root_aliases() {
    let mut proxy = snapshot_proxy().with_base_products(vec![
        ProductRecord {
            id: "gid://shopify/Product/1".to_string(),
            created_at: "2024-01-01T00:00:00.000Z".to_string(),
            updated_at: "2024-01-01T00:00:00.000Z".to_string(),
            title: "First product".to_string(),
            handle: "first-product".to_string(),
            status: "ACTIVE".to_string(),
            description_html: String::new(),
            vendor: String::new(),
            product_type: String::new(),
            tags: Vec::new(),
            template_suffix: String::new(),
            seo_title: String::new(),
            seo_description: String::new(),
            ..ProductRecord::default()
        },
        ProductRecord {
            id: "gid://shopify/Product/2".to_string(),
            created_at: "2024-01-01T00:00:00.000Z".to_string(),
            updated_at: "2024-01-01T00:00:00.000Z".to_string(),
            title: "Second product".to_string(),
            handle: "second-product".to_string(),
            status: "ACTIVE".to_string(),
            description_html: String::new(),
            vendor: String::new(),
            product_type: String::new(),
            tags: Vec::new(),
            template_suffix: String::new(),
            seo_title: String::new(),
            seo_description: String::new(),
            ..ProductRecord::default()
        },
    ]);

    let response = proxy.process_request(graphql_request(
        "POST",
        r#"{"query":"query { listedProducts: products(first: 1) { nodes { id title } } localProductCount: productsCount { count precision } }"}"#,
    ));

    assert_eq!(response.status, 200);
    assert_eq!(
        response.body,
        json!({
            "data": {
                "listedProducts": {
                    "nodes": [
                        {
                            "id": "gid://shopify/Product/1",
                            "title": "First product"
                        }
                    ]
                },
                "localProductCount": {
                    "count": 2,
                    "precision": "EXACT"
                }
            }
        })
    );
}

#[test]
fn product_roots_support_multiple_aliases_in_one_query() {
    let mut proxy = snapshot_proxy().with_base_products(vec![
        ProductRecord {
            id: "gid://shopify/Product/1".to_string(),
            created_at: "2024-01-01T00:00:00.000Z".to_string(),
            updated_at: "2024-01-01T00:00:00.000Z".to_string(),
            title: "First product".to_string(),
            handle: "first-product".to_string(),
            status: "ACTIVE".to_string(),
            description_html: String::new(),
            vendor: String::new(),
            product_type: String::new(),
            tags: Vec::new(),
            template_suffix: String::new(),
            seo_title: String::new(),
            seo_description: String::new(),
            ..ProductRecord::default()
        },
        ProductRecord {
            id: "gid://shopify/Product/2".to_string(),
            created_at: "2024-01-01T00:00:00.000Z".to_string(),
            updated_at: "2024-01-01T00:00:00.000Z".to_string(),
            title: "Second product".to_string(),
            handle: "second-product".to_string(),
            status: "DRAFT".to_string(),
            description_html: String::new(),
            vendor: String::new(),
            product_type: String::new(),
            tags: Vec::new(),
            template_suffix: String::new(),
            seo_title: String::new(),
            seo_description: String::new(),
            ..ProductRecord::default()
        },
    ]);

    let response = proxy.process_request(graphql_request(
        "POST",
        r#"{"query":"query { first: product(id: \"gid://shopify/Product/1\") { title } second: product(id: \"gid://shopify/Product/2\") { status } missing: product(id: \"gid://shopify/Product/missing\") { id } }"}"#,
    ));

    assert_eq!(response.status, 200);
    assert_eq!(
        response.body,
        json!({
            "data": {
                "first": { "title": "First product" },
                "second": { "status": "DRAFT" },
                "missing": null
            }
        })
    );
}

#[test]
fn product_mutations_preserve_root_alias_response_keys() {
    let mut proxy = snapshot_proxy().with_base_products(vec![ProductRecord {
        id: "gid://shopify/Product/1".to_string(),
        created_at: "2024-01-01T00:00:00.000Z".to_string(),
        updated_at: "2024-01-01T00:00:00.000Z".to_string(),
        title: "Seeded product".to_string(),
        handle: "seeded-product".to_string(),
        status: "ACTIVE".to_string(),
        description_html: String::new(),
        vendor: String::new(),
        product_type: String::new(),
        tags: Vec::new(),
        template_suffix: String::new(),
        seo_title: String::new(),
        seo_description: String::new(),
        ..ProductRecord::default()
    }]);

    let create = proxy.process_request(graphql_request(
        "POST",
        r#"{"query":"mutation { createResult: productCreate(product: { title: \"Alias product\" }) { product { id title } userErrors { field message code } } }"}"#,
    ));
    assert_eq!(create.status, 200);
    assert_eq!(
        create.body,
        json!({
            "data": {
                "createResult": {
                    "product": {
                        "id": "gid://shopify/Product/1?shopify-draft-proxy=synthetic",
                        "title": "Alias product"
                    },
                    "userErrors": []
                }
            }
        })
    );

    let update = proxy.process_request(graphql_request(
        "POST",
        r#"{"query":"mutation { updateResult: productUpdate(product: { id: \"gid://shopify/Product/1\", title: \"Updated alias\" }) { product { id title } userErrors { field message code } } }"}"#,
    ));
    assert_eq!(update.status, 200);
    assert_eq!(
        update.body,
        json!({
            "data": {
                "updateResult": {
                    "product": {
                        "id": "gid://shopify/Product/1",
                        "title": "Updated alias"
                    },
                    "userErrors": []
                }
            }
        })
    );

    let delete = proxy.process_request(graphql_request(
        "POST",
        r#"{"query":"mutation { deleteResult: productDelete(product: { id: \"gid://shopify/Product/1\" }) { deletedProductId userErrors { field message code } } }"}"#,
    ));
    assert_eq!(delete.status, 200);
    assert_eq!(
        delete.body,
        json!({
            "data": {
                "deleteResult": {
                    "deletedProductId": "gid://shopify/Product/1",
                    "userErrors": []
                }
            }
        })
    );
}

#[test]
fn saved_search_roots_support_defaults_filtering_pagination_edges_and_aliases() {
    let mut proxy = snapshot_proxy();

    let response = proxy.process_request(graphql_request(
        "POST",
        r#"{"query":"query { ord: orderSavedSearches(first: 2) { nodes { id name } edges { cursor node { id } } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } draftOrderSavedSearches(first: 1) { nodes { name resourceType } } productSavedSearches(first: 2) { nodes { id } edges { cursor node { id } } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } filtered: orderSavedSearches(query: \"financial_status\") { nodes { name query } } }"}"#,
    ));

    assert_eq!(response.status, 200);
    assert_eq!(
        response.body,
        json!({
            "data": {
                "ord": {
                    "nodes": [
                        { "id": "gid://shopify/SavedSearch/3634391515442", "name": "Unfulfilled" },
                        { "id": "gid://shopify/SavedSearch/3634391548210", "name": "Unpaid" }
                    ],
                    "edges": [
                        { "cursor": "cursor:gid://shopify/SavedSearch/3634391515442", "node": { "id": "gid://shopify/SavedSearch/3634391515442" } },
                        { "cursor": "cursor:gid://shopify/SavedSearch/3634391548210", "node": { "id": "gid://shopify/SavedSearch/3634391548210" } }
                    ],
                    "pageInfo": {
                        "hasNextPage": true,
                        "hasPreviousPage": false,
                        "startCursor": "cursor:gid://shopify/SavedSearch/3634391515442",
                        "endCursor": "cursor:gid://shopify/SavedSearch/3634391548210"
                    }
                },
                "draftOrderSavedSearches": {
                    "nodes": [
                        { "name": "Open and invoice sent", "resourceType": "DRAFT_ORDER" }
                    ]
                },
                "productSavedSearches": {
                    "nodes": [],
                    "edges": [],
                    "pageInfo": {
                        "hasNextPage": false,
                        "hasPreviousPage": false,
                        "startCursor": null,
                        "endCursor": null
                    }
                },
                "filtered": {
                    "nodes": [
                        { "name": "Unpaid", "query": "status:open financial_status:unpaid" }
                    ]
                }
            }
        })
    );
}

#[test]
fn segment_create_update_query_grammar_stages_and_reads_generic_node() {
    let mut proxy = snapshot_proxy();
    let create_query = r#"
        mutation SegmentCreateQueryGrammar($name: String!, $query: String!) {
          segmentCreate(name: $name, query: $query) {
            segment { id name query creationDate lastEditDate }
            userErrors { field message }
          }
        }
    "#;

    let created = proxy.process_request(json_graphql_request(
        create_query,
        json!({
            "name": "Query grammar update setup segment-query-grammar-local",
            "query": "number_of_orders >= 1"
        }),
    ));
    let created_segment = &created.body["data"]["segmentCreate"]["segment"];
    let segment_id = created_segment["id"].as_str().unwrap().to_string();
    assert!(segment_id.starts_with("gid://shopify/Segment/"));
    assert_eq!(
        created.body["data"]["segmentCreate"],
        json!({
            "segment": {
                "id": segment_id,
                "name": "Query grammar update setup segment-query-grammar-local",
                "query": "number_of_orders >= 1",
                "creationDate": created_segment["creationDate"],
                "lastEditDate": created_segment["lastEditDate"],
            },
            "userErrors": []
        })
    );

    let updated = proxy.process_request(json_graphql_request(
        r#"
        mutation SegmentUpdateQueryGrammar($id: ID!, $query: String) {
          segmentUpdate(id: $id, query: $query) {
            segment { id name query creationDate lastEditDate }
            userErrors { field message }
          }
        }
        "#,
        json!({ "id": segment_id, "query": "customer_countries CONTAINS 'CA'" }),
    ));
    assert_eq!(
        updated.body["data"]["segmentUpdate"]["segment"]["query"],
        json!("customer_countries CONTAINS 'CA'")
    );

    let node = proxy.process_request(json_graphql_request(
        r#"
        query SegmentNodeRead($id: ID!) {
          node(id: $id) { ... on Segment { id name query creationDate lastEditDate } }
        }
        "#,
        json!({ "id": segment_id }),
    ));
    assert_eq!(
        node.body["data"]["node"]["query"],
        json!("customer_countries CONTAINS 'CA'")
    );

    let malformed = proxy.process_request(json_graphql_request(
        create_query,
        json!({
            "name": "Query grammar malformed segment-query-grammar-local",
            "query": "not a valid segment query ???"
        }),
    ));
    assert_eq!(
        malformed.body["data"]["segmentCreate"]["segment"],
        Value::Null
    );
    assert_eq!(
        malformed.body["data"]["segmentCreate"]["userErrors"],
        json!([
            { "field": ["query"], "message": "Query Line 1 Column 6: 'valid' is unexpected." },
            { "field": ["query"], "message": "Query Line 1 Column 4: 'a' filter cannot be found." }
        ])
    );
}

#[test]
fn segment_mutations_validate_inputs_without_operation_name_markers() {
    let mut proxy = snapshot_proxy();
    let create_query = r#"
        mutation LocalSegmentCreate($name: String!, $query: String!) {
          segmentCreate(name: $name, query: $query) {
            segment { id name query creationDate lastEditDate }
            userErrors { __typename field message }
          }
        }
    "#;

    let blank = proxy.process_request(json_graphql_request(
        create_query,
        json!({ "name": "", "query": "" }),
    ));
    assert_eq!(blank.status, 200);
    assert_eq!(
        blank.body["data"]["segmentCreate"],
        json!({
            "segment": null,
            "userErrors": [
                { "__typename": "UserError", "field": ["name"], "message": "Name can't be blank" },
                { "__typename": "UserError", "field": ["query"], "message": "Query can't be blank" }
            ]
        })
    );
    assert_eq!(proxy.get_log_snapshot()["entries"], json!([]));

    let long_name = proxy.process_request(json_graphql_request(
        create_query,
        json!({ "name": "N".repeat(256), "query": "number_of_orders >= 1" }),
    ));
    assert_eq!(long_name.status, 200);
    assert_eq!(
        long_name.body["data"]["segmentCreate"],
        json!({
            "segment": null,
            "userErrors": [{
                "__typename": "UserError",
                "field": ["name"],
                "message": "Name is too long (maximum is 255 characters)"
            }]
        })
    );

    let padded = proxy.process_request(json_graphql_request(
        create_query,
        json!({ "name": format!("{}Trimmed segment", " ".repeat(260)), "query": "number_of_orders >= 1" }),
    ));
    let segment_id = padded.body["data"]["segmentCreate"]["segment"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(
        padded.body["data"]["segmentCreate"]["segment"]["name"],
        json!("Trimmed segment")
    );
    assert_eq!(
        padded.body["data"]["segmentCreate"]["segment"]["query"],
        json!("number_of_orders >= 1")
    );

    let unknown = proxy.process_request(json_graphql_request(
        r#"
        mutation LocalSegmentUpdate($id: ID!, $name: String) {
          segmentUpdate(id: $id, name: $name) {
            segment { id }
            userErrors { __typename field message }
          }
        }
        "#,
        json!({ "id": "gid://shopify/Segment/999999999999", "name": "Nope" }),
    ));
    assert_eq!(
        unknown.body["data"]["segmentUpdate"],
        json!({
            "segment": null,
            "userErrors": [{
                "__typename": "UserError",
                "field": ["id"],
                "message": "Segment does not exist"
            }]
        })
    );

    let noop = proxy.process_request(json_graphql_request(
        r#"
        mutation LocalSegmentNoop($id: ID!) {
          segmentUpdate(id: $id) {
            segment { id }
            userErrors { __typename field message }
          }
        }
        "#,
        json!({ "id": segment_id }),
    ));
    assert_eq!(
        noop.body["data"]["segmentUpdate"],
        json!({
            "segment": null,
            "userErrors": [{
                "__typename": "UserError",
                "field": null,
                "message": "At least one attribute to change must be present"
            }]
        })
    );
}

#[test]
fn segment_mutations_suffix_duplicate_names() {
    let mut proxy = snapshot_proxy();
    let create_query = r#"
        mutation AnySegmentCreateName($name: String!, $query: String!) {
          segmentCreate(name: $name, query: $query) {
            segment { id name }
            userErrors { field message }
          }
        }
    "#;

    let first = proxy.process_request(json_graphql_request(
        create_query,
        json!({ "name": "Duplicate Segment", "query": "number_of_orders >= 1" }),
    ));
    let first_id = first.body["data"]["segmentCreate"]["segment"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(
        first.body["data"]["segmentCreate"]["segment"]["name"],
        json!("Duplicate Segment")
    );

    let second = proxy.process_request(json_graphql_request(
        create_query,
        json!({ "name": "Duplicate Segment", "query": "number_of_orders >= 1" }),
    ));
    assert_eq!(
        second.body["data"]["segmentCreate"]["segment"]["name"],
        json!("Duplicate Segment (2)")
    );

    let update = proxy.process_request(json_graphql_request(
        r#"
        mutation AnySegmentUpdateName($id: ID!, $name: String) {
          segmentUpdate(id: $id, name: $name) {
            segment { id name }
            userErrors { field message }
          }
        }
        "#,
        json!({ "id": first_id, "name": "Duplicate Segment (2)" }),
    ));
    assert_eq!(
        update.body["data"]["segmentUpdate"]["segment"]["name"],
        json!("Duplicate Segment (3)")
    );
}

#[test]
fn customer_segment_members_query_create_validates_stages_and_reads_node() {
    let mut proxy = snapshot_proxy();
    let create_query = r#"
        mutation CustomerSegmentMembersQueryCreateValidationAndShape($input: CustomerSegmentMembersQueryInput!) {
          customerSegmentMembersQueryCreate(input: $input) {
            customerSegmentMembersQuery { id currentCount done }
            userErrors { field code message }
          }
        }
    "#;

    let both = proxy.process_request(json_graphql_request(
        create_query,
        json!({ "input": { "segmentId": "gid://shopify/Segment/1", "query": "number_of_orders > 0" } }),
    ));
    assert_eq!(
        both.body["data"]["customerSegmentMembersQueryCreate"],
        json!({
            "customerSegmentMembersQuery": null,
            "userErrors": [{
                "field": ["input"],
                "code": "INVALID",
                "message": "Providing both segment_id and query is not supported."
            }]
        })
    );

    let neither = proxy.process_request(json_graphql_request(create_query, json!({ "input": {} })));
    assert_eq!(
        neither.body["data"]["customerSegmentMembersQueryCreate"],
        json!({
            "customerSegmentMembersQuery": null,
            "userErrors": [{
                "field": ["input"],
                "code": "INVALID",
                "message": "You must provide one of segment_id or query."
            }]
        })
    );

    let created = proxy.process_request(json_graphql_request(
        create_query,
        json!({ "input": { "query": "number_of_orders > 0" } }),
    ));
    let created_query =
        &created.body["data"]["customerSegmentMembersQueryCreate"]["customerSegmentMembersQuery"];
    let query_id = created_query["id"].as_str().unwrap().to_string();
    assert!(query_id.starts_with("gid://shopify/CustomerSegmentMembersQuery/"));
    assert_eq!(
        created.body["data"]["customerSegmentMembersQueryCreate"],
        json!({
            "customerSegmentMembersQuery": {
                "id": query_id,
                "currentCount": 0,
                "done": false
            },
            "userErrors": []
        })
    );

    let lookup = proxy.process_request(json_graphql_request(
        r#"
        query CustomerSegmentMembersQueryLookupValidationAndShape($id: ID!) {
          customerSegmentMembersQuery(id: $id) { id currentCount done }
        }
        "#,
        json!({ "id": query_id }),
    ));
    assert_eq!(
        lookup.body["data"]["customerSegmentMembersQuery"],
        json!({ "id": query_id, "currentCount": 0, "done": false })
    );

    let node = proxy.process_request(json_graphql_request(
        r#"
        query CustomerSegmentMembersQueryNodeRead($id: ID!) {
          node(id: $id) { ... on CustomerSegmentMembersQuery { id currentCount done } }
        }
        "#,
        json!({ "id": query_id }),
    ));
    assert_eq!(
        node.body["data"]["node"],
        json!({ "id": query_id, "currentCount": 0, "done": false })
    );
}

#[test]
fn saved_search_create_stages_and_reads_back_selection_aware_results() {
    let mut proxy = snapshot_proxy();

    let create = proxy.process_request(graphql_request(
        "POST",
        r#"{"query":"mutation CreateSearch($input: SavedSearchCreateInput!) { made: savedSearchCreate(input: $input) { savedSearch { id legacyResourceId name query resourceType filters { key value } } userErrors { field message code } } }","variables":{"input":{"name":"Promo products","query":"tag:promo vendor:acme","resourceType":"PRODUCT"}}}"#,
    ));

    assert_eq!(create.status, 200);
    assert_eq!(
        create.body,
        json!({
            "data": {
                "made": {
                    "savedSearch": {
                        "id": "gid://shopify/SavedSearch/1?shopify-draft-proxy=synthetic",
                        "legacyResourceId": "1",
                        "name": "Promo products",
                        "query": "tag:promo vendor:acme",
                        "resourceType": "PRODUCT",
                        "filters": [
                            { "key": "tag", "value": "promo" },
                            { "key": "vendor", "value": "acme" }
                        ]
                    },
                    "userErrors": []
                }
            }
        })
    );

    let read = proxy.process_request(graphql_request(
        "POST",
        r#"{"query":"query { productSavedSearches(first: 5) { nodes { id name query resourceType filters { key value } } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } }"}"#,
    ));

    assert_eq!(read.status, 200);
    assert_eq!(
        read.body,
        json!({
            "data": {
                "productSavedSearches": {
                    "nodes": [
                        {
                            "id": "gid://shopify/SavedSearch/1?shopify-draft-proxy=synthetic",
                            "name": "Promo products",
                            "query": "tag:promo vendor:acme",
                            "resourceType": "PRODUCT",
                            "filters": [
                                { "key": "tag", "value": "promo" },
                                { "key": "vendor", "value": "acme" }
                            ]
                        }
                    ],
                    "pageInfo": {
                        "hasNextPage": false,
                        "hasPreviousPage": false,
                        "startCursor": "cursor:gid://shopify/SavedSearch/1?shopify-draft-proxy=synthetic",
                        "endCursor": "cursor:gid://shopify/SavedSearch/1?shopify-draft-proxy=synthetic"
                    }
                }
            }
        })
    );
}

#[test]
fn saved_search_reserved_names_are_rejected_and_failed_update_preserves_existing_name() {
    let mut proxy = snapshot_proxy();

    for (resource_type, name, query) in [
        ("PRODUCT", "All products", "vendor:Acme"),
        ("PRODUCT", "ALL PRODUCTS", "vendor:Acme"),
        ("ORDER", "All", "status:open"),
        ("DRAFT_ORDER", "All Drafts", "status:open"),
        ("FILE", "All Files", "status:READY"),
        ("COLLECTION", "All collections", "title:Sale"),
        ("PRICE_RULE", "All price rules", "title:summer"),
        ("DISCOUNT_REDEEM_CODE", "All codes", "code:SUMMER"),
    ] {
        let create = proxy.process_request(json_graphql_request(
            r#"
            mutation SavedSearchLocalStagingCreate($input: SavedSearchCreateInput!) {
              savedSearchCreate(input: $input) {
                savedSearch { id name query resourceType }
                userErrors { field message }
              }
            }
            "#,
            json!({ "input": { "resourceType": resource_type, "name": name, "query": query } }),
        ));
        assert_eq!(
            create.body["data"]["savedSearchCreate"],
            json!({
                "savedSearch": null,
                "userErrors": [{ "field": ["input", "name"], "message": "Name has already been taken" }]
            })
        );
    }

    let positive = proxy.process_request(json_graphql_request(
        r#"
        mutation SavedSearchLocalStagingCreate($input: SavedSearchCreateInput!) {
          savedSearchCreate(input: $input) {
            savedSearch { id name query resourceType searchTerms filters { key value } }
            userErrors { field message }
          }
        }
        "#,
        json!({ "input": { "resourceType": "PRODUCT", "name": "All products v2", "query": "vendor:Acme" } }),
    ));
    let id = positive.body["data"]["savedSearchCreate"]["savedSearch"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(
        positive.body["data"]["savedSearchCreate"]["userErrors"],
        json!([])
    );

    let update = proxy.process_request(json_graphql_request(
        r#"
        mutation SavedSearchNameUniquenessUpdateConflict($input: SavedSearchUpdateInput!) {
          savedSearchUpdate(input: $input) {
            savedSearch { id name query resourceType searchTerms filters { key value } }
            userErrors { field message }
          }
        }
        "#,
        json!({ "input": { "id": id, "name": "All products", "query": "vendor:Changed" } }),
    ));
    assert_eq!(
        update.body["data"]["savedSearchUpdate"],
        json!({
            "savedSearch": {
                "id": positive.body["data"]["savedSearchCreate"]["savedSearch"]["id"].clone(),
                "name": "All products v2",
                "query": "vendor:Changed",
                "resourceType": "PRODUCT",
                "searchTerms": "",
                "filters": [{ "key": "vendor", "value": "Changed" }]
            },
            "userErrors": [{ "field": ["input", "name"], "message": "Name has already been taken" }]
        })
    );

    let create_a = proxy.process_request(json_graphql_request(
        r#"
        mutation SavedSearchLocalStagingCreate($input: SavedSearchCreateInput!) {
          savedSearchCreate(input: $input) { savedSearch { id name query resourceType } userErrors { field message } }
        }
        "#,
        json!({ "input": { "resourceType": "PRODUCT", "name": "Conflict A", "query": "title:a" } }),
    ));
    let create_b = proxy.process_request(json_graphql_request(
        r#"
        mutation SavedSearchLocalStagingCreate($input: SavedSearchCreateInput!) {
          savedSearchCreate(input: $input) { savedSearch { id name query resourceType } userErrors { field message } }
        }
        "#,
        json!({ "input": { "resourceType": "PRODUCT", "name": "Conflict B", "query": "title:b" } }),
    ));
    let duplicate = proxy.process_request(json_graphql_request(
        r#"
        mutation SavedSearchLocalStagingCreate($input: SavedSearchCreateInput!) {
          savedSearchCreate(input: $input) { savedSearch { id name query resourceType } userErrors { field message } }
        }
        "#,
        json!({ "input": { "resourceType": "PRODUCT", "name": "Conflict A", "query": "title:duplicate" } }),
    ));
    assert_eq!(
        duplicate.body["data"]["savedSearchCreate"],
        json!({
            "savedSearch": null,
            "userErrors": [{ "field": ["input", "name"], "message": "Name has already been taken" }]
        })
    );
    let b_id = create_b.body["data"]["savedSearchCreate"]["savedSearch"]["id"]
        .as_str()
        .unwrap();
    let rename_conflict = proxy.process_request(json_graphql_request(
        r#"
        mutation SavedSearchNameUniquenessUpdateConflict($input: SavedSearchUpdateInput!) {
          savedSearchUpdate(input: $input) { savedSearch { id name query resourceType filters { key value } } userErrors { field message } }
        }
        "#,
        json!({ "input": { "id": b_id, "name": "Conflict A", "query": "title:rename-conflict" } }),
    ));
    assert_eq!(
        rename_conflict.body["data"]["savedSearchUpdate"],
        json!({
            "savedSearch": {
                "id": create_b.body["data"]["savedSearchCreate"]["savedSearch"]["id"].clone(),
                "name": "Conflict B",
                "query": "title:rename-conflict",
                "resourceType": "PRODUCT",
                "filters": [{ "key": "title", "value": "rename-conflict" }]
            },
            "userErrors": [{ "field": ["input", "name"], "message": "Name has already been taken" }]
        })
    );
    assert_eq!(
        create_a.body["data"]["savedSearchCreate"]["userErrors"],
        json!([])
    );

    let case_primary = proxy.process_request(json_graphql_request(
        r#"
        mutation SavedSearchCaseSensitivePrimary($input: SavedSearchCreateInput!) {
          savedSearchCreate(input: $input) { savedSearch { id name query resourceType } userErrors { field message } }
        }
        "#,
        json!({ "input": { "resourceType": "PRODUCT", "name": "Case Sensitive", "query": "title:primary" } }),
    ));
    let case_variant = proxy.process_request(json_graphql_request(
        r#"
        mutation SavedSearchCaseSensitiveVariant($input: SavedSearchCreateInput!) {
          savedSearchCreate(input: $input) { savedSearch { id name query resourceType } userErrors { field message } }
        }
        "#,
        json!({ "input": { "resourceType": "PRODUCT", "name": "case sensitive", "query": "title:variant" } }),
    ));
    assert_eq!(
        case_primary.body["data"]["savedSearchCreate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        case_variant.body["data"]["savedSearchCreate"]["savedSearch"]["name"],
        json!("case sensitive")
    );
    assert_eq!(
        case_variant.body["data"]["savedSearchCreate"]["savedSearch"]["query"],
        json!("title:variant")
    );
    assert_eq!(
        case_variant.body["data"]["savedSearchCreate"]["savedSearch"]["resourceType"],
        json!("PRODUCT")
    );
    assert_eq!(
        case_variant.body["data"]["savedSearchCreate"]["userErrors"],
        json!([])
    );
    assert!(
        case_variant.body["data"]["savedSearchCreate"]["savedSearch"]["id"]
            .as_str()
            .is_some_and(|id| id.starts_with("gid://shopify/SavedSearch/"))
    );
}

#[test]
fn saved_search_multi_root_create_delete_and_filter_projection() {
    let mut proxy = snapshot_proxy();

    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation SavedSearchFilterProjection($product: SavedSearchCreateInput!, $collection: SavedSearchCreateInput!, $exists: SavedSearchCreateInput!, $bounded: SavedSearchCreateInput!, $grammar: SavedSearchCreateInput!) {
          product: savedSearchCreate(input: $product) { savedSearch { id name query resourceType searchTerms filters { __typename key value } } userErrors { field message } }
          collection: savedSearchCreate(input: $collection) { savedSearch { id name query resourceType searchTerms filters { key value } } userErrors { field message } }
          exists: savedSearchCreate(input: $exists) { savedSearch { id name query resourceType searchTerms filters { key value } } userErrors { field message } }
          bounded: savedSearchCreate(input: $bounded) { savedSearch { id name query resourceType searchTerms filters { key value } } userErrors { field message } }
          grammar: savedSearchCreate(input: $grammar) { savedSearch { id name query resourceType searchTerms filters { key value } } userErrors { field message } }
        }
        "#,
        json!({
            "product": { "resourceType": "PRODUCT", "name": "Recorder Product", "query": "1778109773860 title:Recorder" },
            "collection": { "resourceType": "COLLECTION", "name": "Collection Search", "query": "Collection Search" },
            "exists": { "resourceType": "PRODUCT", "name": "Exists Search", "query": "sku:*" },
            "bounded": { "resourceType": "PRODUCT", "name": "Bounded Search", "query": "inventory_total:>2 inventory_total:<10" },
            "grammar": { "resourceType": "PRODUCT", "name": "Grammar Search", "query": "title:\"Alpha\" OR (status:ACTIVE tag:\"tagged\") -vendor:Archived" }
        }),
    ));
    assert_eq!(create.status, 200);
    assert_eq!(
        create.body["data"]["product"],
        json!({
            "savedSearch": {
                "id": "gid://shopify/SavedSearch/1?shopify-draft-proxy=synthetic",
                "name": "Recorder Product",
                "query": "1778109773860 title:Recorder",
                "resourceType": "PRODUCT",
                "searchTerms": "1778109773860",
                "filters": [{ "__typename": "SearchFilter", "key": "title", "value": "Recorder" }]
            },
            "userErrors": []
        })
    );
    assert_eq!(
        create.body["data"]["collection"]["savedSearch"],
        json!({
            "id": "gid://shopify/SavedSearch/2?shopify-draft-proxy=synthetic",
            "name": "Collection Search",
            "query": "Collection Search",
            "resourceType": "COLLECTION",
            "searchTerms": "Collection Search",
            "filters": []
        })
    );
    assert_eq!(
        create.body["data"]["exists"]["savedSearch"]["filters"],
        json!([{ "key": "sku", "value": "true" }])
    );
    assert_eq!(
        create.body["data"]["bounded"]["savedSearch"]["filters"],
        json!([
            { "key": "inventory_total_min", "value": "2" },
            { "key": "inventory_total_max", "value": "10" }
        ])
    );
    assert_eq!(
        create.body["data"]["grammar"]["savedSearch"],
        json!({
            "id": "gid://shopify/SavedSearch/5?shopify-draft-proxy=synthetic",
            "name": "Grammar Search",
            "query": "title:\"Alpha\" OR (status:ACTIVE tag:\"tagged\") -vendor:Archived",
            "resourceType": "PRODUCT",
            "searchTerms": "title:\"Alpha\" OR (status:ACTIVE tag:\"tagged\")",
            "filters": [{ "key": "vendor_not", "value": "Archived" }]
        })
    );

    let read = proxy.process_request(json_graphql_request(
        r#"
        query SavedSearchResourceReads {
          products: productSavedSearches(first: 10) { nodes { id name searchTerms filters { key value } } pageInfo { hasNextPage hasPreviousPage } }
          collections: collectionSavedSearches(first: 10) { nodes { id name searchTerms filters { key value } } pageInfo { hasNextPage hasPreviousPage } }
        }
        "#,
        json!({}),
    ));
    assert_eq!(read.status, 200);
    assert_eq!(
        read.body["data"]["products"]["nodes"]
            .as_array()
            .unwrap()
            .len(),
        4
    );
    assert_eq!(
        read.body["data"]["collections"]["nodes"]
            .as_array()
            .unwrap()
            .len(),
        1
    );

    let delete = proxy.process_request(json_graphql_request(
        r#"
        mutation SavedSearchCleanup($first: SavedSearchDeleteInput!, $second: SavedSearchDeleteInput!, $missing: SavedSearchDeleteInput!) {
          first: savedSearchDelete(input: $first) { deletedSavedSearchId userErrors { field message } }
          second: savedSearchDelete(input: $second) { deletedSavedSearchId userErrors { field message } }
          missing: savedSearchDelete(input: $missing) { deletedSavedSearchId userErrors { field message } }
        }
        "#,
        json!({
            "first": { "id": "gid://shopify/SavedSearch/1?shopify-draft-proxy=synthetic" },
            "second": { "id": "gid://shopify/SavedSearch/2?shopify-draft-proxy=synthetic" },
            "missing": { "id": "gid://shopify/SavedSearch/missing" }
        }),
    ));
    assert_eq!(delete.status, 200);
    assert_eq!(
        delete.body["data"],
        json!({
            "first": { "deletedSavedSearchId": "gid://shopify/SavedSearch/1?shopify-draft-proxy=synthetic", "userErrors": [] },
            "second": { "deletedSavedSearchId": "gid://shopify/SavedSearch/2?shopify-draft-proxy=synthetic", "userErrors": [] },
            "missing": { "deletedSavedSearchId": null, "userErrors": [{ "field": ["input", "id"], "message": "Saved Search does not exist" }] }
        })
    );
}

#[test]
fn saved_search_query_validation_paths_sorting_deduping_and_allowlists_match_core() {
    let mut proxy = snapshot_proxy();

    let product_allowlist = proxy.process_request(json_graphql_request(
        r#"
        mutation SavedSearchProductAllowlist($handle: SavedSearchCreateInput!, $created: SavedSearchCreateInput!, $productType: SavedSearchCreateInput!) {
          handle: savedSearchCreate(input: $handle) { savedSearch { id name query resourceType filters { key value } } userErrors { field message } }
          created: savedSearchCreate(input: $created) { savedSearch { id name query resourceType filters { key value } } userErrors { field message } }
          productType: savedSearchCreate(input: $productType) { savedSearch { id name query resourceType filters { key value } } userErrors { field message } }
        }
        "#,
        json!({
            "handle": { "resourceType": "PRODUCT", "name": "Handle Filter", "query": "handle:alpha" },
            "created": { "resourceType": "PRODUCT", "name": "Created Filter", "query": "created_at:>=2025-01-01" },
            "productType": { "resourceType": "PRODUCT", "name": "Product Type Filter", "query": "product_type:Widget" }
        }),
    ));
    assert_eq!(
        product_allowlist.body["data"]["handle"]["userErrors"],
        json!([])
    );
    assert_eq!(
        product_allowlist.body["data"]["created"]["userErrors"],
        json!([])
    );
    assert_eq!(
        product_allowlist.body["data"]["productType"]["userErrors"],
        json!([])
    );

    let resource_allowlist = proxy.process_request(json_graphql_request(
        r#"
        mutation SavedSearchResourceAllowlist($collection: SavedSearchCreateInput!, $draftOrder: SavedSearchCreateInput!, $file: SavedSearchCreateInput!, $discountCode: SavedSearchCreateInput!) {
          collection: savedSearchCreate(input: $collection) { savedSearch { id name query resourceType filters { key value } } userErrors { field message } }
          draftOrder: savedSearchCreate(input: $draftOrder) { savedSearch { id name query resourceType filters { key value } } userErrors { field message } }
          file: savedSearchCreate(input: $file) { savedSearch { id name query resourceType filters { key value } } userErrors { field message } }
          discountCode: savedSearchCreate(input: $discountCode) { savedSearch { id name query resourceType filters { key value } } userErrors { field message } }
        }
        "#,
        json!({
            "collection": { "resourceType": "COLLECTION", "name": "Collection Handle", "query": "handle:summer" },
            "draftOrder": { "resourceType": "DRAFT_ORDER", "name": "Draft Order Tag", "query": "tag:vip" },
            "file": { "resourceType": "FILE", "name": "File Media Type", "query": "media_type:IMAGE" },
            "discountCode": { "resourceType": "DISCOUNT_REDEEM_CODE", "name": "Discount Code", "query": "code:SUMMER" }
        }),
    ));
    assert_eq!(
        resource_allowlist.body["data"]["collection"]["userErrors"],
        json!([])
    );
    assert_eq!(
        resource_allowlist.body["data"]["draftOrder"]["userErrors"],
        json!([])
    );
    assert_eq!(
        resource_allowlist.body["data"]["file"]["userErrors"],
        json!([])
    );
    assert_eq!(
        resource_allowlist.body["data"]["discountCode"]["userErrors"],
        json!([])
    );

    let unknown_create = proxy.process_request(json_graphql_request(
        r#"
        mutation SavedSearchUnknownFilterCreate($input: SavedSearchCreateInput!) {
          savedSearchCreate(input: $input) { savedSearch { id } userErrors { field message } }
        }
        "#,
        json!({ "input": { "resourceType": "PRODUCT", "name": "Unknown Create", "query": "zzz_filter:1 aaa_filter:2 aaa_filter:3 -aaa_filter:4" } }),
    ));
    assert_eq!(
        unknown_create.body["data"]["savedSearchCreate"],
        json!({
            "savedSearch": null,
            "userErrors": [
                { "field": ["input", "query"], "message": "Query is invalid, 'aaa_filter' is not a valid filter" },
                { "field": ["input", "query"], "message": "Query is invalid, 'zzz_filter' is not a valid filter" }
            ]
        })
    );

    let update_seed = proxy.process_request(json_graphql_request(
        r#"
        mutation SavedSearchUpdateSeed($input: SavedSearchCreateInput!) {
          savedSearchCreate(input: $input) { savedSearch { id name query resourceType } userErrors { field message } }
        }
        "#,
        json!({ "input": { "resourceType": "PRODUCT", "name": "Update Unknown Seed", "query": "vendor:Acme" } }),
    ));
    let update_id = update_seed.body["data"]["savedSearchCreate"]["savedSearch"]["id"]
        .as_str()
        .unwrap();
    let unknown_update = proxy.process_request(json_graphql_request(
        r#"
        mutation SavedSearchUnknownFilterUpdate($input: SavedSearchUpdateInput!) {
          savedSearchUpdate(input: $input) { savedSearch { id name query resourceType } userErrors { field message } }
        }
        "#,
        json!({ "input": { "id": update_id, "query": "zzz_filter:1 aaa_filter:2 aaa_filter:3" } }),
    ));
    assert_eq!(
        unknown_update.body["data"]["savedSearchUpdate"],
        json!({
            "savedSearch": {
                "id": update_id,
                "name": "Update Unknown Seed",
                "query": "zzz_filter:1 aaa_filter:2 aaa_filter:3",
            "resourceType": "PRODUCT"
            },
            "userErrors": [
                { "field": ["input", "query"], "message": "Query is invalid, 'aaa_filter' is not a valid filter" },
                { "field": ["input", "query"], "message": "Query is invalid, 'zzz_filter' is not a valid filter" }
            ]
        })
    );
}

#[test]
fn saved_search_required_input_omissions_return_top_level_graphql_errors() {
    let mut proxy = snapshot_proxy();

    let missing_name = proxy.process_request(json_graphql_request(
        r#"
        mutation SavedSearchCreateMissingName {
          savedSearchCreate(input: { resourceType: PRODUCT }) {
            savedSearch { id name query resourceType }
            userErrors { field message }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(missing_name.status, 200);
    assert_eq!(missing_name.body.get("data"), None);
    assert_eq!(
        missing_name.body["errors"],
        json!([
            {
                "message": "Argument 'name' on InputObject 'SavedSearchCreateInput' is required. Expected type String!",
                "locations": [{ "line": 2, "column": 28 }],
                "path": ["mutation SavedSearchCreateMissingName", "savedSearchCreate", "input", "name"],
                "extensions": {
                    "code": "missingRequiredInputObjectAttribute",
                    "argumentName": "name",
                    "argumentType": "String!",
                    "inputObjectType": "SavedSearchCreateInput"
                }
            },
            {
                "message": "Argument 'query' on InputObject 'SavedSearchCreateInput' is required. Expected type String!",
                "locations": [{ "line": 2, "column": 28 }],
                "path": ["mutation SavedSearchCreateMissingName", "savedSearchCreate", "input", "query"],
                "extensions": {
                    "code": "missingRequiredInputObjectAttribute",
                    "argumentName": "query",
                    "argumentType": "String!",
                    "inputObjectType": "SavedSearchCreateInput"
                }
            }
        ])
    );

    let missing_resource_type = proxy.process_request(json_graphql_request(
        r#"
        mutation SavedSearchCreateMissingResourceType {
          savedSearchCreate(input: { name: "Missing resource type", query: "tag:missing-resource-type" }) {
            savedSearch { id name query resourceType }
            userErrors { field message }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(missing_resource_type.body.get("data"), None);
    assert_eq!(
        missing_resource_type.body["errors"][0],
        json!({
            "message": "Argument 'resourceType' on InputObject 'SavedSearchCreateInput' is required. Expected type SearchResultType!",
            "locations": [{ "line": 2, "column": 28 }],
            "path": ["mutation SavedSearchCreateMissingResourceType", "savedSearchCreate", "input", "resourceType"],
            "extensions": {
                "code": "missingRequiredInputObjectAttribute",
                "argumentName": "resourceType",
                "argumentType": "SearchResultType!",
                "inputObjectType": "SavedSearchCreateInput"
            }
        })
    );

    let missing_id = proxy.process_request(json_graphql_request(
        r#"
        mutation SavedSearchUpdateMissingId {
          savedSearchUpdate(input: { name: "Missing id" }) {
            savedSearch { id name }
            userErrors { field message }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(missing_id.body.get("data"), None);
    assert_eq!(
        missing_id.body["errors"][0],
        json!({
            "message": "Argument 'id' on InputObject 'SavedSearchUpdateInput' is required. Expected type ID!",
            "locations": [{ "line": 2, "column": 28 }],
            "path": ["mutation SavedSearchUpdateMissingId", "savedSearchUpdate", "input", "id"],
            "extensions": {
                "code": "missingRequiredInputObjectAttribute",
                "argumentName": "id",
                "argumentType": "ID!",
                "inputObjectType": "SavedSearchUpdateInput"
            }
        })
    );
}

#[test]
fn saved_search_required_variable_omissions_return_invalid_variable_errors() {
    let mut proxy = snapshot_proxy();

    let missing_resource_type = proxy.process_request(json_graphql_request(
        r#"
        mutation SavedSearchCreateVariableMissingResourceType($input: SavedSearchCreateInput!) {
          savedSearchCreate(input: $input) { savedSearch { id } userErrors { field message } }
        }
        "#,
        json!({ "input": { "name": "Variable missing resource type ssri-mowc", "query": "tag:variable-required" } }),
    ));
    assert_eq!(missing_resource_type.body.get("data"), None);
    assert_eq!(
        missing_resource_type.body["errors"][0],
        json!({
            "message": "Variable $input of type SavedSearchCreateInput! was provided invalid value for resourceType (Expected value to not be null)",
            "locations": [{ "line": 1, "column": 55 }],
            "extensions": {
                "code": "INVALID_VARIABLE",
                "value": { "name": "Variable missing resource type ssri-mowc", "query": "tag:variable-required" },
                "problems": [{ "path": ["resourceType"], "explanation": "Expected value to not be null" }]
            }
        })
    );

    let missing_name = proxy.process_request(json_graphql_request(
        r#"
        mutation SavedSearchCreateVariableMissingName($input: SavedSearchCreateInput!) {
          savedSearchCreate(input: $input) { savedSearch { id } userErrors { field message } }
        }
        "#,
        json!({ "input": { "resourceType": "PRODUCT", "query": "tag:variable-required" } }),
    ));
    assert_eq!(missing_name.body.get("data"), None);
    assert_eq!(
        missing_name.body["errors"][0],
        json!({
            "message": "Variable $input of type SavedSearchCreateInput! was provided invalid value for name (Expected value to not be null)",
            "locations": [{ "line": 1, "column": 47 }],
            "extensions": {
                "code": "INVALID_VARIABLE",
                "value": { "resourceType": "PRODUCT", "query": "tag:variable-required" },
                "problems": [{ "path": ["name"], "explanation": "Expected value to not be null" }]
            }
        })
    );
}

#[test]
fn product_mutation_error_payloads_preserve_root_alias_response_keys() {
    let mut proxy = snapshot_proxy();

    let create = proxy.process_request(graphql_request(
        "POST",
        r#"{"query":"mutation { failedCreate: productCreate(product: { title: \" \" }) { product { id } userErrors { field message code } } }"}"#,
    ));
    assert_eq!(create.status, 200);
    assert_eq!(
        create.body,
        json!({
            "data": {
                "failedCreate": {
                    "product": null,
                    "userErrors": [{
                        "field": ["title"],
                        "message": "Title can't be blank",
                        "code": "BLANK"
                    }]
                }
            }
        })
    );

    let update = proxy.process_request(graphql_request(
        "POST",
        r#"{"query":"mutation { failedUpdate: productUpdate(product: { id: \"gid://shopify/Product/missing\", title: \"Missing\" }) { product { id } userErrors { field message code } } }"}"#,
    ));
    assert_eq!(update.status, 200);
    assert_eq!(
        update.body,
        json!({
            "data": {
                "failedUpdate": {
                    "product": null,
                    "userErrors": [{
                        "field": ["id"],
                        "message": "Product does not exist",
                        "code": "NOT_FOUND"
                    }]
                }
            }
        })
    );

    let delete = proxy.process_request(graphql_request(
        "POST",
        r#"{"query":"mutation { failedDelete: productDelete(input: { id: \"gid://shopify/Product/missing\" }) { deletedProductId userErrors { field message code } } }"}"#,
    ));
    assert_eq!(delete.status, 200);
    assert_eq!(
        delete.body,
        json!({
            "data": {
                "failedDelete": {
                    "deletedProductId": null,
                    "userErrors": [{
                        "field": ["id"],
                        "message": "Product does not exist",
                        "code": "NOT_FOUND"
                    }]
                }
            }
        })
    );
}

#[test]
fn product_delete_stages_downstream_no_data_for_product_read() {
    let mut proxy = snapshot_proxy().with_base_products(vec![ProductRecord {
        id: "gid://shopify/Product/1".to_string(),
        created_at: "2024-01-01T00:00:00.000Z".to_string(),
        updated_at: "2024-01-01T00:00:00.000Z".to_string(),
        title: "Deletable product".to_string(),
        handle: "deletable-product".to_string(),
        status: "ACTIVE".to_string(),
        description_html: String::new(),
        vendor: String::new(),
        product_type: String::new(),
        tags: Vec::new(),
        template_suffix: String::new(),
        seo_title: String::new(),
        seo_description: String::new(),
        ..ProductRecord::default()
    }]);

    let delete = proxy.process_request(graphql_request(
        "POST",
        r#"{"query":"mutation { productDelete(input: { id: \"gid://shopify/Product/1\" }) { deletedProductId userErrors { field message code } } }"}"#,
    ));

    assert_eq!(delete.status, 200);
    assert_eq!(
        delete.body,
        json!({
            "data": {
                "productDelete": {
                    "deletedProductId": "gid://shopify/Product/1",
                    "userErrors": []
                }
            }
        })
    );

    let read_back = proxy.process_request(graphql_request(
        "POST",
        r#"{"query":"query { product(id: \"gid://shopify/Product/1\") { id title } }"}"#,
    ));

    assert_eq!(read_back.status, 200);
    assert_eq!(read_back.body, json!({ "data": { "product": null } }));
}

#[test]
fn product_create_stages_product_visible_to_product_read() {
    let mut proxy = snapshot_proxy();

    let create = proxy.process_request(graphql_request(
        "POST",
        r#"{"query":"mutation { productCreate(product: { title: \"Rust staged product\", handle: \"rust-staged-product\", status: ACTIVE }) { product { id title handle status } userErrors { field message code } } }"}"#,
    ));

    assert_eq!(create.status, 200);
    assert_eq!(
        create.body,
        json!({
            "data": {
                "productCreate": {
                    "product": {
                        "id": "gid://shopify/Product/1?shopify-draft-proxy=synthetic",
                        "title": "Rust staged product",
                        "handle": "rust-staged-product",
                        "status": "ACTIVE"
                    },
                    "userErrors": []
                }
            }
        })
    );

    let read_back = proxy.process_request(graphql_request(
        "POST",
        r#"{"query":"query { product(id: \"gid://shopify/Product/1?shopify-draft-proxy=synthetic\") { id title handle status } }"}"#,
    ));

    assert_eq!(read_back.status, 200);
    assert_eq!(
        read_back.body,
        json!({
            "data": {
                "product": {
                    "id": "gid://shopify/Product/1?shopify-draft-proxy=synthetic",
                    "title": "Rust staged product",
                    "handle": "rust-staged-product",
                    "status": "ACTIVE"
                }
            }
        })
    );
}

#[test]
fn product_create_update_and_connection_reads_emit_product_timestamps() {
    let mut proxy = snapshot_proxy();

    let create = proxy.process_request(graphql_request(
        "POST",
        r#"{"query":"mutation { productCreate(product: { title: \"Timestamped product\", handle: \"timestamped-product\", status: ACTIVE }) { product { id createdAt updatedAt } userErrors { field message code } } }"}"#,
    ));

    assert_eq!(create.status, 200);
    let created_product = &create.body["data"]["productCreate"]["product"];
    assert_eq!(
        created_product["id"],
        json!("gid://shopify/Product/1?shopify-draft-proxy=synthetic")
    );
    let created_at = created_product["createdAt"]
        .as_str()
        .expect("productCreate should return createdAt")
        .to_string();
    let first_updated_at = created_product["updatedAt"]
        .as_str()
        .expect("productCreate should return updatedAt")
        .to_string();
    assert_eq!(created_at, first_updated_at);
    assert_eq!(
        create.body["data"]["productCreate"]["userErrors"],
        json!([])
    );

    let read_back = proxy.process_request(graphql_request(
        "POST",
        r#"{"query":"query { product(id: \"gid://shopify/Product/1?shopify-draft-proxy=synthetic\") { id createdAt updatedAt } }"}"#,
    ));
    assert_eq!(read_back.status, 200);
    assert_eq!(
        read_back.body["data"]["product"],
        json!({
            "id": "gid://shopify/Product/1?shopify-draft-proxy=synthetic",
            "createdAt": created_at,
            "updatedAt": first_updated_at
        })
    );

    let update = proxy.process_request(graphql_request(
        "POST",
        r#"{"query":"mutation { productUpdate(product: { id: \"gid://shopify/Product/1?shopify-draft-proxy=synthetic\", title: \"Updated timestamped product\" }) { product { id createdAt updatedAt } userErrors { field message code } } }"}"#,
    ));
    assert_eq!(update.status, 200);
    let updated_product = &update.body["data"]["productUpdate"]["product"];
    let second_updated_at = updated_product["updatedAt"]
        .as_str()
        .expect("productUpdate should return updatedAt")
        .to_string();
    assert_eq!(updated_product["createdAt"], json!(created_at));
    assert!(second_updated_at > first_updated_at);
    assert_eq!(
        update.body["data"]["productUpdate"]["userErrors"],
        json!([])
    );

    let connection = proxy.process_request(graphql_request(
        "POST",
        r#"{"query":"query { products(first: 10) { nodes { id createdAt updatedAt } } }"}"#,
    ));
    assert_eq!(connection.status, 200);
    assert_eq!(
        connection.body["data"]["products"]["nodes"],
        json!([{
            "id": "gid://shopify/Product/1?shopify-draft-proxy=synthetic",
            "createdAt": created_at,
            "updatedAt": second_updated_at
        }])
    );
}

#[test]
fn product_read_resolves_id_from_request_variables() {
    let mut proxy = snapshot_proxy().with_base_products(vec![ProductRecord {
        id: "gid://shopify/Product/variable-id".to_string(),
        created_at: "2024-01-01T00:00:00.000Z".to_string(),
        updated_at: "2024-01-01T00:00:00.000Z".to_string(),
        title: "Variable product".to_string(),
        handle: "variable-product".to_string(),
        status: "DRAFT".to_string(),
        description_html: String::new(),
        vendor: String::new(),
        product_type: String::new(),
        tags: Vec::new(),
        template_suffix: String::new(),
        seo_title: String::new(),
        seo_description: String::new(),
        ..ProductRecord::default()
    }]);

    let product = proxy.process_request(graphql_request(
        "POST",
        r#"{"query":"query ProductById($id: ID!) { product(id: $id) { id title handle status } }","variables":{"id":"gid://shopify/Product/variable-id"}}"#,
    ));

    assert_eq!(product.status, 200);
    assert_eq!(
        product.body,
        json!({
            "data": {
                "product": {
                    "id": "gid://shopify/Product/variable-id",
                    "title": "Variable product",
                    "handle": "variable-product",
                    "status": "DRAFT"
                }
            }
        })
    );
}

#[test]
fn product_create_resolves_input_from_request_variables() {
    let mut proxy = snapshot_proxy();

    let create = proxy.process_request(graphql_request(
        "POST",
        r#"{"query":"mutation ProductCreate($product: ProductCreateInput!) { productCreate(product: $product) { product { id title handle status } userErrors { field message code } } }","variables":{"product":{"title":"Variable staged product","handle":"variable-staged-product","status":"ARCHIVED"}}}"#,
    ));

    assert_eq!(create.status, 200);
    assert_eq!(
        create.body,
        json!({
            "data": {
                "productCreate": {
                    "product": {
                        "id": "gid://shopify/Product/1?shopify-draft-proxy=synthetic",
                        "title": "Variable staged product",
                        "handle": "variable-staged-product",
                        "status": "ARCHIVED"
                    },
                    "userErrors": []
                }
            }
        })
    );
}

#[test]
fn product_create_rejects_invalid_status_literals_and_variables_without_staging() {
    let mut proxy = snapshot_proxy();

    let mut literal_request = graphql_request(
        "POST",
        r#"{"query":"mutation InvalidCreateStatusLiteral { productCreate(product: { title: \"Invalid status\", status: PUBLISHED }) { product { id status } userErrors { field message } } }"}"#,
    );
    literal_request.path = "/admin/api/2025-01/graphql.json".to_string();
    let literal = proxy.process_request(literal_request);
    assert_eq!(literal.status, 200);
    assert_eq!(
        literal.body["errors"][0]["message"],
        json!(
            "Argument 'status' on InputObject 'ProductCreateInput' has an invalid value (PUBLISHED). Expected type 'ProductStatus'."
        )
    );
    assert_eq!(
        literal.body["errors"][0]["path"],
        json!([
            "mutation InvalidCreateStatusLiteral",
            "productCreate",
            "product",
            "status"
        ])
    );
    assert_eq!(
        literal.body["errors"][0]["extensions"],
        json!({
            "code": "argumentLiteralsIncompatible",
            "typeName": "InputObject",
            "argumentName": "status"
        })
    );

    let mut variable_request = json_graphql_request(
        r#"
        mutation InvalidCreateStatusVariable($product: ProductCreateInput!) {
          productCreate(product: $product) {
            product { id status }
            userErrors { field message }
          }
        }
        "#,
        json!({ "product": { "title": "Invalid status", "status": "ENABLED" } }),
    );
    variable_request.path = "/admin/api/2025-01/graphql.json".to_string();
    let variable = proxy.process_request(variable_request);
    assert_eq!(variable.status, 200);
    assert_eq!(
        variable.body["errors"][0]["message"],
        json!(
            "Variable $product of type ProductCreateInput! was provided invalid value for status (Expected \"ENABLED\" to be one of: ACTIVE, ARCHIVED, DRAFT)"
        )
    );
    assert_eq!(
        variable.body["errors"][0]["extensions"],
        json!({
            "code": "INVALID_VARIABLE",
            "value": {
                "title": "Invalid status",
                "status": "ENABLED"
            },
            "problems": [{
                "path": ["status"],
                "explanation": "Expected \"ENABLED\" to be one of: ACTIVE, ARCHIVED, DRAFT"
            }]
        })
    );
    assert_eq!(proxy.get_log_snapshot(), json!({ "entries": [] }));

    let read_back = proxy.process_request(graphql_request(
        "POST",
        r#"{"query":"query { products(first: 10) { nodes { title status } } }"}"#,
    ));
    assert_eq!(read_back.body["data"]["products"]["nodes"], json!([]));
}

#[test]
fn product_change_status_rejects_invalid_status_without_staging() {
    let mut proxy = snapshot_proxy().with_base_products(vec![ProductRecord {
        id: "gid://shopify/Product/1".to_string(),
        created_at: "2024-01-01T00:00:00.000Z".to_string(),
        updated_at: "2024-01-01T00:00:00.000Z".to_string(),
        title: "Seeded product".to_string(),
        handle: "seeded-product".to_string(),
        status: "ACTIVE".to_string(),
        description_html: String::new(),
        vendor: String::new(),
        product_type: String::new(),
        tags: Vec::new(),
        template_suffix: String::new(),
        seo_title: String::new(),
        seo_description: String::new(),
        ..ProductRecord::default()
    }]);

    let mut literal_request = graphql_request(
        "POST",
        r#"{"query":"mutation InvalidStatusLiteral { productChangeStatus(productId: \"gid://shopify/Product/1\", status: PUBLISHED) { product { id status } userErrors { field message } } }"}"#,
    );
    literal_request.path = "/admin/api/2025-01/graphql.json".to_string();
    let literal = proxy.process_request(literal_request);
    assert_eq!(literal.status, 200);
    assert_eq!(
        literal.body["errors"][0]["message"],
        json!(
            "Argument 'status' on Field 'productChangeStatus' has an invalid value (PUBLISHED). Expected type 'ProductStatus!'."
        )
    );
    assert_eq!(
        literal.body["errors"][0]["path"],
        json!([
            "mutation InvalidStatusLiteral",
            "productChangeStatus",
            "status"
        ])
    );
    assert_eq!(
        literal.body["errors"][0]["extensions"],
        json!({
            "code": "argumentLiteralsIncompatible",
            "typeName": "Field",
            "argumentName": "status"
        })
    );

    let mut variable_request = json_graphql_request(
        r#"
        mutation InvalidStatusVariable($productId: ID!, $status: ProductStatus!) {
          productChangeStatus(productId: $productId, status: $status) {
            product { id status }
            userErrors { field message }
          }
        }
        "#,
        json!({ "productId": "gid://shopify/Product/1", "status": "ENABLED" }),
    );
    variable_request.path = "/admin/api/2025-01/graphql.json".to_string();
    let variable = proxy.process_request(variable_request);
    assert_eq!(variable.status, 200);
    assert_eq!(
        variable.body["errors"][0]["message"],
        json!("Variable $status of type ProductStatus! was provided invalid value")
    );
    assert_eq!(
        variable.body["errors"][0]["extensions"],
        json!({
            "code": "INVALID_VARIABLE",
            "value": "ENABLED",
            "problems": [{
                "path": [],
                "explanation": "Expected \"ENABLED\" to be one of: ACTIVE, ARCHIVED, DRAFT"
            }]
        })
    );
    assert_eq!(proxy.get_log_snapshot(), json!({ "entries": [] }));

    let read_back = proxy.process_request(graphql_request(
        "POST",
        r#"{"query":"query { product(id: \"gid://shopify/Product/1\") { id status } }"}"#,
    ));
    assert_eq!(read_back.body["data"]["product"]["status"], json!("ACTIVE"));
}

#[test]
fn admin_graphql_capability_classification_requires_local_dispatch_root() {
    // Table-dispatch classification keys on LOCAL_DISPATCH_ROOTS, not on the registry's
    // `implemented` flag. None of these synthetic roots is a dispatch root, so all three fall
    // through to passthrough regardless of `implemented` — and crucially never 501. (In snapshot
    // mode there is no upstream, so passthrough surfaces as a 400 "no dispatcher" error.)
    let mut proxy = snapshot_proxy().with_registry(vec![
        registry_entry(
            "knownProducts",
            OperationType::Query,
            CapabilityExecution::OverlayRead,
            true,
        ),
        registry_entry(
            "knownProductCreate",
            OperationType::Mutation,
            CapabilityExecution::StageLocally,
            true,
        ),
        registry_entry(
            "knownButUnimplemented",
            OperationType::Query,
            CapabilityExecution::OverlayRead,
            false,
        ),
    ]);

    let known_query = proxy.process_request(graphql_request(
        "POST",
        r#"{"query":"query { knownProducts(first: 1) { nodes { id } } }"}"#,
    ));
    assert_eq!(known_query.status, 400);
    assert_eq!(
        known_query.body,
        json!({ "errors": [{ "message": "No domain dispatcher implemented for root field: knownProducts" }] })
    );

    let known_mutation = proxy.process_request(graphql_request(
        "POST",
        r#"{"query":"mutation { knownProductCreate(input: {}) { product { id } } }"}"#,
    ));
    assert_eq!(known_mutation.status, 400);
    assert_eq!(
        known_mutation.body,
        json!({ "errors": [{ "message": "No mutation dispatcher implemented for root field: knownProductCreate" }] })
    );

    let unimplemented = proxy.process_request(graphql_request(
        "POST",
        r#"{"query":"query { knownButUnimplemented { id } }"}"#,
    ));
    assert_eq!(unimplemented.status, 400);
    assert_eq!(
        unimplemented.body,
        json!({ "errors": [{ "message": "No domain dispatcher implemented for root field: knownButUnimplemented" }] })
    );
}

#[test]
fn local_dispatch_root_without_registry_classification_fails_closed() {
    let mut proxy = snapshot_proxy().with_registry(vec![registry_entry(
        "productCreate",
        OperationType::Mutation,
        CapabilityExecution::StageLocally,
        true,
    )]);

    let response = proxy.process_request(graphql_request(
        "POST",
        r#"{"query":"mutation { productVariantCreate(input: {}) { productVariant { id } userErrors { message } } }"}"#,
    ));

    assert_eq!(response.status, 400);
    assert_eq!(
        response.body,
        json!({ "errors": [{ "message": "No mutation dispatcher implemented for root field: productVariantCreate" }] })
    );
    assert_eq!(proxy.get_log_snapshot(), json!({ "entries": [] }));
}

#[test]
fn implemented_registry_entry_without_local_dispatch_passes_through_never_501() {
    // An implemented registry entry that is not a LOCAL_DISPATCH_ROOT must NOT be hard-failed by
    // the table dispatch: an implemented operation never 501s. It resolves to Unknown/Passthrough
    // and passes through upstream (surfaced as a 400 "no dispatcher" in snapshot mode, where no
    // upstream exists). In production these entries are handled earlier in the special-case chain.
    let mut proxy = snapshot_proxy().with_registry(vec![OperationRegistryEntry {
        name: "unknownSavedSearches".to_string(),
        operation_type: OperationType::Query,
        domain: CapabilityDomain::SavedSearches,
        execution: CapabilityExecution::OverlayRead,
        implemented: true,
        match_names: vec!["unknownSavedSearches".to_string()],
        runtime_tests: vec!["tests/graphql_routes.rs".to_string()],
        support_notes: None,
    }]);

    let response = proxy.process_request(graphql_request(
        "POST",
        r#"{"query":"query { unknownSavedSearches(first: 1) { nodes { id } } }"}"#,
    ));

    assert_ne!(response.status, 501);
    assert_eq!(response.status, 400);
    assert_eq!(
        response.body,
        json!({ "errors": [{ "message": "No domain dispatcher implemented for root field: unknownSavedSearches" }] })
    );
}

#[test]
fn supported_product_variant_mutation_keeps_capability_metadata_in_log() {
    let mut proxy = snapshot_proxy();

    let response = proxy.process_request(graphql_request(
        "POST",
        r#"{"query":"mutation { productVariantCreate(input: {}) { productVariant { id } userErrors { message } } }"}"#,
    ));

    assert_eq!(response.status, 200);
    assert_eq!(
        proxy.get_log_snapshot()["entries"][0]["interpreted"]["capability"],
        json!({
            "operationName": "productVariantCreate",
            "domain": "products",
            "execution": "stage-locally"
        })
    );
}

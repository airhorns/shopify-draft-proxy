use super::common::*;
use pretty_assertions::assert_eq;

const DEFAULT_ORDER_UNFULFILLED_ID: &str =
    "gid://shopify/SavedSearch/default-order-unfulfilled?shopify-draft-proxy=synthetic";
const DEFAULT_ORDER_UNPAID_ID: &str =
    "gid://shopify/SavedSearch/default-order-unpaid?shopify-draft-proxy=synthetic";

fn seed_product(id: &str) -> ProductRecord {
    ProductRecord {
        id: id.to_string(),
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
    }
}

fn restore_product_payload_shop(proxy: &mut DraftProxy) -> Value {
    let shop = json!({
        "id": "gid://shopify/Shop/restored-product-payload",
        "name": "Restored product payload shop",
        "myshopifyDomain": "restored-product-payload.myshopify.com",
        "currencyCode": "CAD",
        "primaryDomain": {
            "id": "gid://shopify/Domain/909090",
            "host": "restored-product-payload.example",
            "url": "https://restored-product-payload.example",
            "sslEnabled": true
        }
    });
    let dump = proxy.process_request(request_with_body("POST", "/__meta/dump", "{}"));
    assert_eq!(dump.status, 200);
    let mut restored = dump.body;
    restored["state"]["baseState"]["shop"] = shop.clone();
    let restore = proxy.process_request(request_with_body(
        "POST",
        "/__meta/restore",
        &restored.to_string(),
    ));
    assert_eq!(restore.status, 200);
    selected_product_payload_shop()
}

fn selected_product_payload_shop() -> Value {
    json!({
        "id": "gid://shopify/Shop/restored-product-payload",
        "name": "Restored product payload shop",
        "myshopifyDomain": "restored-product-payload.myshopify.com"
    })
}

fn assert_user_error_with_field_and_code(user_errors: &Value, field: Value, code: &str) {
    let errors = user_errors
        .as_array()
        .expect("userErrors should serialize as an array");
    assert!(
        errors
            .iter()
            .any(|error| error.get("field") == Some(&field)
                && error.get("code") == Some(&json!(code))),
        "expected userErrors to contain field {field:?} and code {code}, got {errors:?}"
    );
}

fn read_variant_sku_positions(proxy: &mut DraftProxy, product_id: &str) -> Value {
    let read = proxy.process_request(json_graphql_request(
        r#"
        query VariantPositions($productId: ID!) {
          product(id: $productId) {
            variants(first: 10) {
              nodes { sku position }
            }
          }
        }
        "#,
        json!({ "productId": product_id }),
    ));
    assert_eq!(read.status, 200);
    read.body["data"]["product"]["variants"]["nodes"].clone()
}

fn create_product_media_for_test(
    proxy: &mut DraftProxy,
    product_id: &str,
    media_content_type: &str,
    alt: &str,
) -> String {
    let response = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateProductMediaForTest($productId: ID!, $media: [CreateMediaInput!]!) {
          productCreateMedia(productId: $productId, media: $media) {
            media { id mediaContentType status }
            mediaUserErrors { field message }
          }
        }
        "#,
        json!({
            "productId": product_id,
            "media": [{
                "mediaContentType": media_content_type,
                "originalSource": if media_content_type == "EXTERNAL_VIDEO" {
                    "https://www.youtube.com/watch?v=dQw4w9WgXcQ"
                } else {
                    "https://placehold.co/640x480/png"
                },
                "alt": alt
            }]
        }),
    ));
    assert_eq!(response.status, 200);
    assert_eq!(
        response.body["data"]["productCreateMedia"]["mediaUserErrors"],
        json!([])
    );
    response.body["data"]["productCreateMedia"]["media"][0]["id"]
        .as_str()
        .expect("created media id should be present")
        .to_string()
}

fn settle_product_media_for_test(proxy: &mut DraftProxy, product_id: &str, media_id: &str) {
    let response = proxy.process_request(json_graphql_request(
        r#"
        mutation SettleProductMediaForTest($productId: ID!, $media: [UpdateMediaInput!]!) {
          productUpdateMedia(productId: $productId, media: $media) {
            media { id status }
            mediaUserErrors { field message }
          }
        }
        "#,
        json!({
            "productId": product_id,
            "media": [{ "id": media_id }]
        }),
    ));
    assert_eq!(response.status, 200);
    assert_eq!(
        response.body["data"]["productUpdateMedia"]["mediaUserErrors"],
        json!([])
    );
}

fn assert_product_media_type(
    node: &Value,
    media_content_type: &str,
    typename: &str,
    gid_type: &str,
) {
    let id = node["id"]
        .as_str()
        .expect("media node should include a string id");
    assert!(
        id.starts_with(&format!("gid://shopify/{gid_type}/")),
        "expected {gid_type} gid, got {id}"
    );
    assert_eq!(node["__typename"], json!(typename));
    assert_eq!(node["mediaContentType"], json!(media_content_type));
}

#[test]
fn product_create_media_payload_product_connection_uses_uploaded_before_processing_readback() {
    let product_id = "gid://shopify/Product/media-status";
    let mut proxy = configured_proxy(ReadMode::LiveHybrid, None)
        .with_base_products(vec![seed_product(product_id)])
        .with_upstream_transport(|_| {
            panic!("product media status staging should not call upstream")
        });

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
            "productId": product_id,
            "media": [{
                "mediaContentType": "IMAGE",
                "originalSource": "https://placehold.co/600x400/png",
                "alt": "Front view"
            }]
        }),
    ));
    assert_eq!(create.status, 200);
    let payload = &create.body["data"]["productCreateMedia"];
    assert_eq!(payload["mediaUserErrors"], json!([]));
    assert_eq!(payload["media"][0]["status"], json!("UPLOADED"));
    assert_eq!(
        payload["product"]["media"]["nodes"][0]["status"],
        json!("UPLOADED"),
        "the mutation payload product connection should mirror Shopify's immediate UPLOADED media node"
    );

    let read = proxy.process_request(json_graphql_request(
        r#"
        query ProductCreateMediaDownstreamRead($id: ID!) {
          product(id: $id) {
            id
            media(first: 10) {
              nodes { id alt mediaContentType status preview { image { url } } ... on MediaImage { image { url } } }
            }
          }
        }
        "#,
        json!({ "id": product_id }),
    ));
    assert_eq!(read.status, 200);
    assert_eq!(
        read.body["data"]["product"]["media"]["nodes"][0]["status"],
        json!("PROCESSING"),
        "the stored downstream read remains the async processing state"
    );
}

#[test]
fn product_create_update_and_reorder_media_preserve_non_image_media_types() {
    let product_id = "gid://shopify/Product/media-types";
    let mut proxy = configured_proxy(ReadMode::LiveHybrid, None)
        .with_base_products(vec![seed_product(product_id)])
        .with_upstream_transport(|_| panic!("product media type staging should not call upstream"));

    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation ProductCreateMediaTypes($productId: ID!, $media: [CreateMediaInput!]!) {
          productCreateMedia(productId: $productId, media: $media) {
            media {
              id
              __typename
              alt
              mediaContentType
              status
              ... on Video {
                originalSource { url }
                sources { url }
              }
              ... on Model3d {
                originalSource { url }
                sources { url }
              }
              ... on MediaImage {
                image { url }
              }
            }
            mediaUserErrors { field message }
            product {
              id
              media(first: 10) {
                nodes {
                  id
                  __typename
                  mediaContentType
                  status
                  ... on Video { sources { url } }
                  ... on Model3d { sources { url } }
                  ... on MediaImage { image { url } }
                }
              }
            }
          }
        }
        "#,
        json!({
            "productId": product_id,
            "media": [
                {
                    "mediaContentType": "VIDEO",
                    "originalSource": "https://cdn.example.com/declared-video.mp4",
                    "alt": "Declared video"
                },
                {
                    "mediaContentType": "EXTERNAL_VIDEO",
                    "originalSource": "https://www.youtube.com/watch?v=dQw4w9WgXcQ",
                    "alt": "Declared external video"
                },
                {
                    "mediaContentType": "MODEL_3D",
                    "originalSource": "https://cdn.example.com/model.glb",
                    "alt": "Declared model"
                },
                {
                    "originalSource": "https://cdn.example.com/inferred-video.MP4?download=1",
                    "alt": "Inferred video"
                }
            ]
        }),
    ));
    assert_eq!(create.status, 200);
    assert_eq!(
        create.body["data"]["productCreateMedia"]["mediaUserErrors"],
        json!([])
    );

    let created = create.body["data"]["productCreateMedia"]["media"]
        .as_array()
        .expect("created media should be an array");
    assert_product_media_type(&created[0], "VIDEO", "Video", "Video");
    assert_eq!(
        created[0]["sources"],
        json!([{ "url": "https://cdn.example.com/declared-video.mp4" }])
    );
    assert_product_media_type(
        &created[1],
        "EXTERNAL_VIDEO",
        "ExternalVideo",
        "ExternalVideo",
    );
    assert_product_media_type(&created[2], "MODEL_3D", "Model3d", "Model3d");
    assert_eq!(
        created[2]["sources"],
        json!([{ "url": "https://cdn.example.com/model.glb" }])
    );
    assert_product_media_type(&created[3], "VIDEO", "Video", "Video");
    assert_eq!(
        created[3]["sources"],
        json!([{ "url": "https://cdn.example.com/inferred-video.MP4?download=1" }])
    );

    let downstream = create.body["data"]["productCreateMedia"]["product"]["media"]["nodes"]
        .as_array()
        .expect("downstream product media should be an array");
    assert_product_media_type(&downstream[0], "VIDEO", "Video", "Video");
    assert_product_media_type(
        &downstream[1],
        "EXTERNAL_VIDEO",
        "ExternalVideo",
        "ExternalVideo",
    );
    assert_product_media_type(&downstream[2], "MODEL_3D", "Model3d", "Model3d");
    assert_product_media_type(&downstream[3], "VIDEO", "Video", "Video");

    let external_video_id = created[1]["id"].as_str().unwrap();
    let update = proxy.process_request(json_graphql_request(
        r#"
        mutation ProductUpdateExternalVideo($productId: ID!, $media: [UpdateMediaInput!]!) {
          productUpdateMedia(productId: $productId, media: $media) {
            media {
              id
              __typename
              alt
              mediaContentType
              status
              ... on MediaImage { image { url } }
            }
            mediaUserErrors { field message }
          }
        }
        "#,
        json!({
            "productId": product_id,
            "media": [{ "id": external_video_id, "alt": "Updated external video" }]
        }),
    ));
    assert_eq!(update.status, 200);
    let updated = &update.body["data"]["productUpdateMedia"]["media"][0];
    assert_product_media_type(updated, "EXTERNAL_VIDEO", "ExternalVideo", "ExternalVideo");
    assert_eq!(updated["alt"], json!("Updated external video"));
    assert_eq!(updated["status"], json!("READY"));
    assert!(
        updated.get("image").is_none(),
        "external video update should not project a MediaImage fragment"
    );

    let model_id = created[2]["id"].as_str().unwrap();
    let reorder = proxy.process_request(json_graphql_request(
        r#"
        mutation ProductReorderNonImageMedia($productId: ID!, $moves: [MoveInput!]!) {
          productReorderMedia(id: $productId, moves: $moves) {
            job { id done }
            mediaUserErrors { field message }
          }
        }
        "#,
        json!({
            "productId": product_id,
            "moves": [
                { "id": model_id, "newPosition": "0" },
                { "id": external_video_id, "newPosition": "1" }
            ]
        }),
    ));
    assert_eq!(reorder.status, 200);
    assert_eq!(
        reorder.body["data"]["productReorderMedia"]["mediaUserErrors"],
        json!([])
    );

    let read = proxy.process_request(json_graphql_request(
        r#"
        query ProductMediaAfterReorder($productId: ID!) {
          product(id: $productId) {
            media(first: 10) {
              nodes {
                id
                __typename
                mediaContentType
                status
              }
            }
          }
        }
        "#,
        json!({ "productId": product_id }),
    ));
    assert_eq!(read.status, 200);
    let reordered = read.body["data"]["product"]["media"]["nodes"]
        .as_array()
        .expect("reordered media should be an array");
    assert_product_media_type(&reordered[0], "MODEL_3D", "Model3d", "Model3d");
    assert_eq!(reordered[0]["status"], json!("PROCESSING"));
    assert_product_media_type(
        &reordered[1],
        "EXTERNAL_VIDEO",
        "ExternalVideo",
        "ExternalVideo",
    );
    assert_eq!(reordered[1]["status"], json!("PROCESSING"));
}

fn missing_product_hydrate_response() -> Response {
    Response {
        status: 200,
        headers: Default::default(),
        body: json!({ "data": { "nodes": [Value::Null] } }),
    }
}

#[test]
fn product_media_missing_product_errors_use_media_user_error_code() {
    let mut proxy = configured_proxy(ReadMode::LiveHybrid, None)
        .with_upstream_transport(|_| missing_product_hydrate_response());

    let cases = [
        (
            "productCreateMedia",
            r#"
            mutation MissingProductCreateMedia($productId: ID!, $media: [CreateMediaInput!]!) {
              productCreateMedia(productId: $productId, media: $media) {
                userErrors { field message code }
                mediaUserErrors { field message code }
              }
            }
            "#,
            json!({
                "productId": "gid://shopify/Product/999999999999",
                "media": [{
                    "mediaContentType": "IMAGE",
                    "originalSource": "https://placehold.co/640x480/png",
                    "alt": "Unknown product"
                }]
            }),
        ),
        (
            "productUpdateMedia",
            r#"
            mutation MissingProductUpdateMedia($productId: ID!, $media: [UpdateMediaInput!]!) {
              productUpdateMedia(productId: $productId, media: $media) {
                userErrors { field message code }
                mediaUserErrors { field message code }
              }
            }
            "#,
            json!({
                "productId": "gid://shopify/Product/999999999999",
                "media": [{ "id": "gid://shopify/MediaImage/999999999999", "alt": "Unknown product" }]
            }),
        ),
        (
            "productDeleteMedia",
            r#"
            mutation MissingProductDeleteMedia($productId: ID!, $mediaIds: [ID!]!) {
              productDeleteMedia(productId: $productId, mediaIds: $mediaIds) {
                userErrors { field message code }
                mediaUserErrors { field message code }
              }
            }
            "#,
            json!({
                "productId": "gid://shopify/Product/999999999999",
                "mediaIds": ["gid://shopify/MediaImage/999999999999"]
            }),
        ),
        (
            "productReorderMedia",
            r#"
            mutation MissingProductReorderMedia($id: ID!, $moves: [MoveInput!]!) {
              productReorderMedia(id: $id, moves: $moves) {
                userErrors { field message code }
                mediaUserErrors { field message code }
              }
            }
            "#,
            json!({
                "id": "gid://shopify/Product/999999999999",
                "moves": [{ "id": "gid://shopify/MediaImage/999999999999", "newPosition": "0" }]
            }),
        ),
    ];

    for (root, query, variables) in cases {
        let response = proxy.process_request(json_graphql_request(query, variables));
        assert_eq!(response.status, 200, "{root} should return a payload");
        let payload = &response.body["data"][root];
        let expected = json!([{
            "field": if root == "productReorderMedia" { json!(["id"]) } else { json!(["productId"]) },
            "message": "Product does not exist",
            "code": "PRODUCT_DOES_NOT_EXIST"
        }]);
        assert_eq!(
            payload["mediaUserErrors"], expected,
            "{root} mediaUserErrors"
        );
        assert_eq!(payload["userErrors"], expected, "{root} userErrors");
    }
}

#[test]
fn product_media_missing_media_errors_use_media_user_error_code_and_captured_message() {
    let product_id = "gid://shopify/Product/1";
    let missing_media_id = "gid://shopify/MediaImage/999999999999";
    let mut proxy = configured_proxy(ReadMode::LiveHybrid, None)
        .with_base_products(vec![seed_product(product_id)])
        .with_upstream_transport(|_| panic!("media validation should use local product state"));

    let cases = [
        (
            "productUpdateMedia",
            r#"
            mutation MissingMediaUpdate($productId: ID!, $media: [UpdateMediaInput!]!) {
              productUpdateMedia(productId: $productId, media: $media) {
                userErrors { field message code }
                mediaUserErrors { field message code }
              }
            }
            "#,
            json!({ "productId": product_id, "media": [{ "id": missing_media_id, "alt": "Missing" }] }),
            json!(["media"]),
        ),
        (
            "productDeleteMedia",
            r#"
            mutation MissingMediaDelete($productId: ID!, $mediaIds: [ID!]!) {
              productDeleteMedia(productId: $productId, mediaIds: $mediaIds) {
                userErrors { field message code }
                mediaUserErrors { field message code }
              }
            }
            "#,
            json!({ "productId": product_id, "mediaIds": [missing_media_id] }),
            json!(["mediaIds"]),
        ),
    ];

    for (root, query, variables, field) in cases {
        let response = proxy.process_request(json_graphql_request(query, variables));
        assert_eq!(response.status, 200, "{root} should return a payload");
        let payload = &response.body["data"][root];
        let expected = json!([{
            "field": field,
            "message": format!("Media id {missing_media_id} does not exist"),
            "code": "MEDIA_DOES_NOT_EXIST"
        }]);
        assert_eq!(
            payload["mediaUserErrors"], expected,
            "{root} mediaUserErrors"
        );
        assert_eq!(payload["userErrors"], expected, "{root} userErrors");
    }
}

#[test]
fn product_media_missing_media_errors_aggregate_all_missing_ids() {
    let product_id = "gid://shopify/Product/1";
    let mut proxy = configured_proxy(ReadMode::LiveHybrid, None)
        .with_base_products(vec![seed_product(product_id)])
        .with_upstream_transport(|_| panic!("media validation should use local product state"));
    let ready_media_id =
        create_product_media_for_test(&mut proxy, product_id, "IMAGE", "Ready media");

    let before_invalid = proxy.process_request(json_graphql_request(
        r#"
        query ProductMediaBeforeInvalidMutation($productId: ID!) {
          product(id: $productId) {
            media(first: 10) {
              nodes { id alt mediaContentType status }
            }
          }
        }
        "#,
        json!({ "productId": product_id }),
    ));
    assert_eq!(before_invalid.status, 200);
    assert_eq!(
        before_invalid.body["data"]["product"]["media"]["nodes"][0]["id"],
        json!(ready_media_id)
    );
    let media_before_invalid = before_invalid.body["data"]["product"]["media"]["nodes"].clone();

    let missing_media_ids = [
        "gid://shopify/MediaImage/999999999998",
        "gid://shopify/MediaImage/999999999999",
    ];
    let expected_plural_message = format!(
        "Media ids {},{} do not exist",
        missing_media_ids[0], missing_media_ids[1]
    );

    let update = proxy.process_request(json_graphql_request(
        r#"
        mutation MissingMediaUpdate($productId: ID!, $media: [UpdateMediaInput!]!) {
          productUpdateMedia(productId: $productId, media: $media) {
            media { id }
            userErrors { field message code }
            mediaUserErrors { field message code }
          }
        }
        "#,
        json!({
            "productId": product_id,
            "media": [
                { "id": missing_media_ids[0], "alt": "Missing one" },
                { "id": missing_media_ids[1], "alt": "Missing two" }
            ]
        }),
    ));
    assert_eq!(update.status, 200);
    let update_payload = &update.body["data"]["productUpdateMedia"];
    let expected_update_errors = json!([{
        "field": ["media"],
        "message": expected_plural_message,
        "code": "MEDIA_DOES_NOT_EXIST"
    }]);
    assert_eq!(update_payload["media"], Value::Null);
    assert_eq!(update_payload["mediaUserErrors"], expected_update_errors);
    assert_eq!(update_payload["userErrors"], expected_update_errors);

    let delete = proxy.process_request(json_graphql_request(
        r#"
        mutation MissingMediaDelete($productId: ID!, $mediaIds: [ID!]!) {
          productDeleteMedia(productId: $productId, mediaIds: $mediaIds) {
            deletedMediaIds
            deletedProductImageIds
            userErrors { field message code }
            mediaUserErrors { field message code }
          }
        }
        "#,
        json!({ "productId": product_id, "mediaIds": missing_media_ids }),
    ));
    assert_eq!(delete.status, 200);
    let delete_payload = &delete.body["data"]["productDeleteMedia"];
    let expected_delete_errors = json!([{
        "field": ["mediaIds"],
        "message": expected_plural_message,
        "code": "MEDIA_DOES_NOT_EXIST"
    }]);
    assert_eq!(delete_payload["deletedMediaIds"], Value::Null);
    assert_eq!(delete_payload["deletedProductImageIds"], Value::Null);
    assert_eq!(delete_payload["mediaUserErrors"], expected_delete_errors);
    assert_eq!(delete_payload["userErrors"], expected_delete_errors);

    let after_invalid = proxy.process_request(json_graphql_request(
        r#"
        query ProductMediaAfterInvalidMutation($productId: ID!) {
          product(id: $productId) {
            media(first: 10) {
              nodes { id alt mediaContentType status }
            }
          }
        }
        "#,
        json!({ "productId": product_id }),
    ));
    assert_eq!(after_invalid.status, 200);
    assert_eq!(
        after_invalid.body["data"]["product"]["media"]["nodes"], media_before_invalid,
        "rejected update/delete should not stage product media changes"
    );
}

#[test]
fn product_reorder_media_unknown_media_id_returns_async_job_without_immediate_error() {
    let product_id = "gid://shopify/Product/1";
    let missing_media_id = "gid://shopify/MediaImage/999999999999";
    let mut proxy = configured_proxy(ReadMode::LiveHybrid, None)
        .with_base_products(vec![seed_product(product_id)])
        .with_upstream_transport(|_| panic!("reorder validation should use local product state"));

    let response = proxy.process_request(json_graphql_request(
        r#"
        mutation MissingMediaReorder($id: ID!, $moves: [MoveInput!]!) {
          productReorderMedia(id: $id, moves: $moves) {
            job { id done }
            userErrors { field message code }
            mediaUserErrors { field message code }
          }
        }
        "#,
        json!({ "id": product_id, "moves": [{ "id": missing_media_id, "newPosition": "0" }] }),
    ));

    assert_eq!(response.status, 200);
    let payload = &response.body["data"]["productReorderMedia"];
    assert!(payload["job"]["id"]
        .as_str()
        .is_some_and(|id| id.starts_with("gid://shopify/Job/")));
    assert_eq!(payload["job"]["done"], json!(false));
    assert_eq!(payload["mediaUserErrors"], json!([]));
    assert_eq!(payload["userErrors"], json!([]));
}

fn append_variant_media_for_test(
    proxy: &mut DraftProxy,
    product_id: &str,
    variant_media: Value,
) -> Value {
    let response = proxy.process_request(json_graphql_request(
        r#"
        mutation AppendVariantMediaForTest($productId: ID!, $variantMedia: [ProductVariantAppendMediaInput!]!) {
          productVariantAppendMedia(productId: $productId, variantMedia: $variantMedia) {
            productVariants { id }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "productId": product_id, "variantMedia": variant_media }),
    ));
    assert_eq!(response.status, 200);
    response.body["data"]["productVariantAppendMedia"]["userErrors"].clone()
}

fn detach_variant_media_for_test(
    proxy: &mut DraftProxy,
    product_id: &str,
    variant_media: Value,
) -> Value {
    let response = proxy.process_request(json_graphql_request(
        r#"
        mutation DetachVariantMediaForTest($productId: ID!, $variantMedia: [ProductVariantDetachMediaInput!]!) {
          productVariantDetachMedia(productId: $productId, variantMedia: $variantMedia) {
            productVariants { id }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "productId": product_id, "variantMedia": variant_media }),
    ));
    assert_eq!(response.status, 200);
    response.body["data"]["productVariantDetachMedia"]["userErrors"].clone()
}

fn seed_product_with_options(id: &str) -> ProductRecord {
    let mut product = seed_product(id);
    product.extra_fields.insert(
        "options".to_string(),
        json!([
            {
                "id": "gid://shopify/ProductOption/1",
                "name": "Color",
                "position": 1,
                "values": ["Red"],
                "optionValues": [{
                    "id": "gid://shopify/ProductOptionValue/1",
                    "name": "Red",
                    "hasVariants": true
                }]
            },
            {
                "id": "gid://shopify/ProductOption/2",
                "name": "Size",
                "position": 2,
                "values": ["Small"],
                "optionValues": [{
                    "id": "gid://shopify/ProductOptionValue/2",
                    "name": "Small",
                    "hasVariants": true
                }]
            }
        ]),
    );
    product
}

fn create_bulk_positioned_variants(
    proxy: &mut DraftProxy,
    product_id: &str,
    skus: &[&str],
) -> Vec<Value> {
    let variants = skus
        .iter()
        .enumerate()
        .map(|(index, sku)| {
            json!({
                "price": format!("{}.00", index + 10),
                "optionValues": [{ "optionName": "Color", "name": *sku }],
                "inventoryItem": { "sku": *sku }
            })
        })
        .collect::<Vec<_>>();
    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation BulkVariantCreateForPositionTest($productId: ID!, $variants: [ProductVariantsBulkInput!]!) {
          productVariantsBulkCreate(productId: $productId, variants: $variants) {
            productVariants { id sku position }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "productId": product_id, "variants": variants }),
    ));
    assert_eq!(create.status, 200);
    assert_eq!(
        create.body["data"]["productVariantsBulkCreate"]["userErrors"],
        json!([])
    );
    create.body["data"]["productVariantsBulkCreate"]["productVariants"]
        .as_array()
        .expect("bulk variant create should return variants")
        .clone()
}

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
fn product_variants_read_respects_connection_arguments() {
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
    let first_variant =
        create_legacy_variant(&mut proxy, "gid://shopify/Product/1", "FIRST", "1.00");
    create_legacy_variant(&mut proxy, "gid://shopify/Product/1", "SECOND", "1.00");

    let response = proxy.process_request(graphql_request(
        "POST",
        r#"{"query":"query { product(id: \"gid://shopify/Product/1\") { variants(first: 1) { nodes { id sku } pageInfo { hasNextPage endCursor } } } }"}"#,
    ));

    assert_eq!(response.status, 200);
    assert_eq!(
        response.body["data"]["product"]["variants"]["nodes"],
        json!([{ "id": first_variant["id"], "sku": "FIRST" }])
    );
    assert_eq!(
        response.body["data"]["product"]["variants"]["pageInfo"]["hasNextPage"],
        json!(true)
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
fn legacy_product_variant_roots_stage_variants_into_product_graph() {
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

    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation LegacyVariantCreate($input: ProductVariantInput!) {
          productVariantCreate(input: $input) {
            product { id totalInventory tracksInventory variants(first: 10) { nodes { id sku price selectedOptions { name value } } } }
            productVariant { id title sku barcode price compareAtPrice taxable inventoryPolicy inventoryQuantity taxCode showUnitPrice unitPriceMeasurement { quantityValue quantityUnit referenceValue referenceUnit } selectedOptions { name value } inventoryItem { id tracked requiresShipping countryCodeOfOrigin provinceCodeOfOrigin harmonizedSystemCode measurement { weight { value unit } } } }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "input": {
                "productId": "gid://shopify/Product/1",
                "title": "Blue",
                "sku": "LEGACY-BLUE",
                "barcode": "2222222222222",
                "price": "9.99",
                "compareAtPrice": "12.50",
                "taxable": false,
                "inventoryPolicy": "CONTINUE",
                "inventoryQuantity": 7,
                "taxCode": "PC040100",
                "showUnitPrice": true,
                "unitPriceMeasurement": {
                    "quantityValue": 1,
                    "quantityUnit": "L",
                    "referenceValue": 100,
                    "referenceUnit": "ML"
                },
                "selectedOptions": [{ "name": "Color", "value": "Blue" }],
                "inventoryItem": {
                    "tracked": true,
                    "requiresShipping": false,
                    "countryCodeOfOrigin": "CA",
                    "provinceCodeOfOrigin": "ON",
                    "harmonizedSystemCode": "1234.56",
                    "measurement": { "weight": { "value": 2.5, "unit": "KILOGRAMS" } }
                }
            }
        }),
    ));

    assert_eq!(create.status, 200);
    assert_eq!(
        create.body["data"]["productVariantCreate"]["productVariant"]["sku"],
        json!("LEGACY-BLUE")
    );
    assert_eq!(
        create.body["data"]["productVariantCreate"]["productVariant"]["price"],
        json!("9.99")
    );
    assert_eq!(
        create.body["data"]["productVariantCreate"]["productVariant"]["taxCode"],
        json!("PC040100")
    );
    assert_eq!(
        create.body["data"]["productVariantCreate"]["productVariant"]["inventoryItem"]
            ["harmonizedSystemCode"],
        json!("123456")
    );
    assert_eq!(
        create.body["data"]["productVariantCreate"]["productVariant"]["inventoryItem"]
            ["measurement"]["weight"]["value"],
        json!(2.5)
    );
    assert_eq!(
        create.body["data"]["productVariantCreate"]["product"]["variants"]["nodes"][0]["sku"],
        json!("LEGACY-BLUE")
    );
    let variant_id = create.body["data"]["productVariantCreate"]["productVariant"]["id"]
        .as_str()
        .expect("variant id should be synthesized")
        .to_string();
    let inventory_item_id = create.body["data"]["productVariantCreate"]["productVariant"]
        ["inventoryItem"]["id"]
        .as_str()
        .expect("inventory item id should be synthesized")
        .to_string();

    let read_after_create = proxy.process_request(json_graphql_request(
        r#"
        query LegacyVariantReadAfterWrite($productId: ID!, $variantId: ID!, $inventoryItemId: ID!) {
          product(id: $productId) { id variants(first: 10) { nodes { id sku price taxCode inventoryItem { id tracked requiresShipping countryCodeOfOrigin harmonizedSystemCode } } } }
          variant: productVariant(id: $variantId) { id sku price showUnitPrice product { id title } inventoryItem { id tracked requiresShipping measurement { weight { value unit } } } }
          stock: inventoryItem(id: $inventoryItemId) { id tracked requiresShipping countryCodeOfOrigin harmonizedSystemCode variant { id sku inventoryQuantity product { id title } } }
        }
        "#,
        json!({
            "productId": "gid://shopify/Product/1",
            "variantId": variant_id,
            "inventoryItemId": inventory_item_id
        }),
    ));
    assert_eq!(read_after_create.status, 200);
    assert_eq!(
        read_after_create.body["data"]["product"]["variants"]["nodes"][0]["sku"],
        json!("LEGACY-BLUE")
    );
    assert_eq!(
        read_after_create.body["data"]["variant"]["price"],
        json!("9.99")
    );
    assert_eq!(
        read_after_create.body["data"]["product"]["variants"]["nodes"][0]["taxCode"],
        json!("PC040100")
    );
    assert_eq!(
        read_after_create.body["data"]["stock"]["harmonizedSystemCode"],
        json!("123456")
    );
    assert_eq!(
        read_after_create.body["data"]["stock"]["variant"]["inventoryQuantity"],
        json!(7)
    );

    let update = proxy.process_request(json_graphql_request(
        r#"
        mutation LegacyVariantUpdate($input: ProductVariantInput!) {
          productVariantUpdate(input: $input) {
            productVariant { id sku price taxCode inventoryItem { id tracked requiresShipping harmonizedSystemCode } }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "input": {
                "id": variant_id,
                "sku": "LEGACY-BLUE-UPDATED",
                "price": "1.00",
                "taxCode": "PC040200",
                "inventoryItem": { "tracked": false, "requiresShipping": true, "harmonizedSystemCode": "6543 21" }
            }
        }),
    ));
    assert_eq!(update.status, 200);
    assert_eq!(
        update.body["data"]["productVariantUpdate"]["productVariant"]["sku"],
        json!("LEGACY-BLUE-UPDATED")
    );
    assert_eq!(
        update.body["data"]["productVariantUpdate"]["productVariant"]["price"],
        json!("1.00")
    );
    assert_eq!(
        update.body["data"]["productVariantUpdate"]["productVariant"]["taxCode"],
        json!("PC040200")
    );
    assert_eq!(
        update.body["data"]["productVariantUpdate"]["productVariant"]["inventoryItem"]
            ["harmonizedSystemCode"],
        json!("654321")
    );

    let read_after_update = proxy.process_request(json_graphql_request(
        r#"
        query LegacyVariantReadAfterUpdate($productId: ID!, $variantId: ID!) {
          product(id: $productId) { id variants(first: 10) { nodes { id sku price taxCode inventoryItem { tracked requiresShipping harmonizedSystemCode } } } }
          variant: productVariant(id: $variantId) { id sku price taxCode inventoryItem { tracked requiresShipping harmonizedSystemCode } }
        }
        "#,
        json!({
            "productId": "gid://shopify/Product/1",
            "variantId": variant_id
        }),
    ));
    assert_eq!(
        read_after_update.body["data"]["product"]["variants"]["nodes"][0]["sku"],
        json!("LEGACY-BLUE-UPDATED")
    );
    assert_eq!(
        read_after_update.body["data"]["variant"]["inventoryItem"]["tracked"],
        json!(false)
    );
    assert_eq!(
        read_after_update.body["data"]["variant"]["inventoryItem"]["harmonizedSystemCode"],
        json!("654321")
    );

    let delete = proxy.process_request(json_graphql_request(
        r#"
        mutation LegacyVariantDelete($id: ID!) {
          productVariantDelete(id: $id) { deletedProductVariantId userErrors { field message } }
        }
        "#,
        json!({ "id": variant_id }),
    ));
    assert_eq!(delete.status, 200);
    assert_eq!(
        delete.body["data"]["productVariantDelete"]["deletedProductVariantId"],
        json!(variant_id)
    );

    let read_after_delete = proxy.process_request(json_graphql_request(
        r#"
        query LegacyVariantReadAfterDelete($productId: ID!, $variantId: ID!, $inventoryItemId: ID!) {
          product(id: $productId) { id variants(first: 10) { nodes { id } } }
          variant: productVariant(id: $variantId) { id }
          stock: inventoryItem(id: $inventoryItemId) { id }
        }
        "#,
        json!({
            "productId": "gid://shopify/Product/1",
            "variantId": variant_id,
            "inventoryItemId": inventory_item_id
        }),
    ));
    assert_eq!(
        read_after_delete.body["data"]["product"]["variants"]["nodes"],
        json!([])
    );
    assert_eq!(read_after_delete.body["data"]["variant"], Value::Null);
    assert_eq!(read_after_delete.body["data"]["stock"], Value::Null);
}

#[test]
fn legacy_product_variant_create_validates_forbidden_keys_and_missing_product() {
    let mut proxy = snapshot_proxy();

    let forbidden_id = proxy.process_request(json_graphql_request(
        r#"
        mutation LegacyVariantCreateForbiddenId($input: ProductVariantInput!) {
          productVariantCreate(input: $input) { productVariant { id } userErrors { field message } }
        }
        "#,
        json!({
            "input": {
                "id": "gid://shopify/ProductVariant/1",
                "productId": "gid://shopify/Product/1"
            }
        }),
    ));
    assert_eq!(forbidden_id.status, 200);
    assert_eq!(
        forbidden_id.body["errors"][0]["extensions"]["code"],
        json!("no_key_on_create")
    );

    let forbidden_adjustment = proxy.process_request(json_graphql_request(
        r#"
        mutation LegacyVariantCreateForbiddenAdjustment($input: ProductVariantInput!) {
          productVariantCreate(input: $input) { productVariant { id } userErrors { field message } }
        }
        "#,
        json!({
            "input": {
                "productId": "gid://shopify/Product/1",
                "inventoryQuantityAdjustment": 1
            }
        }),
    ));
    assert_eq!(forbidden_adjustment.status, 200);
    assert_eq!(
        forbidden_adjustment.body["errors"][0]["extensions"]["key"],
        json!("inventoryQuantityAdjustment")
    );

    let missing_product = proxy.process_request(json_graphql_request(
        r#"
        mutation LegacyVariantCreateMissingProduct($input: ProductVariantInput!) {
          productVariantCreate(input: $input) { product { id } productVariant { id } userErrors { field message } }
        }
        "#,
        json!({
            "input": {
                "productId": "gid://shopify/Product/missing",
                "sku": "MISSING"
            }
        }),
    ));
    assert_eq!(missing_product.status, 200);
    assert_eq!(
        missing_product.body["data"]["productVariantCreate"]["product"],
        Value::Null
    );
    assert_eq!(
        missing_product.body["data"]["productVariantCreate"]["productVariant"],
        Value::Null
    );
    assert_eq!(
        missing_product.body["data"]["productVariantCreate"]["userErrors"],
        json!([{ "field": ["productId"], "message": "Product does not exist" }])
    );
}

#[test]
fn legacy_product_variant_update_and_delete_validate_unknown_ids() {
    let mut proxy = snapshot_proxy();

    let update = proxy.process_request(json_graphql_request(
        r#"
        mutation LegacyVariantUpdateMissing($input: ProductVariantInput!) {
          productVariantUpdate(input: $input) { productVariant { id } userErrors { field message } }
        }
        "#,
        json!({
            "input": {
                "id": "gid://shopify/ProductVariant/missing",
                "price": "1.00"
            }
        }),
    ));
    assert_eq!(update.status, 200);
    assert_eq!(
        update.body["data"]["productVariantUpdate"]["productVariant"],
        Value::Null
    );
    assert_eq!(
        update.body["data"]["productVariantUpdate"]["userErrors"],
        json!([{ "field": ["id"], "message": "Product variant does not exist" }])
    );

    let delete = proxy.process_request(json_graphql_request(
        r#"
        mutation LegacyVariantDeleteMissing($id: ID!) {
          productVariantDelete(id: $id) { deletedProductVariantId userErrors { field message } }
        }
        "#,
        json!({ "id": "gid://shopify/ProductVariant/missing" }),
    ));
    assert_eq!(delete.status, 200);
    assert_eq!(
        delete.body["data"]["productVariantDelete"]["deletedProductVariantId"],
        Value::Null
    );
    assert_eq!(
        delete.body["data"]["productVariantDelete"]["userErrors"],
        json!([{ "field": ["id"], "message": "Product variant does not exist" }])
    );
}

#[test]
fn legacy_product_variant_scalar_validation_rejects_before_staging() {
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
    let seeded_variant =
        create_legacy_variant(&mut proxy, "gid://shopify/Product/1", "VALID-SKU", "10.00");
    let seeded_variant_id = seeded_variant["id"]
        .as_str()
        .expect("setup variant should have an id");

    let update = proxy.process_request(json_graphql_request(
        r#"
        mutation LegacyVariantInvalidScalars($input: ProductVariantInput!) {
          productVariantUpdate(input: $input) {
            productVariant { id }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "input": {
                "id": seeded_variant_id,
                "price": "-1.00",
                "sku": "S".repeat(256),
                "inventoryItem": { "measurement": { "weight": { "value": -1, "unit": "KILOGRAMS" } } }
            }
        }),
    ));
    assert_eq!(update.status, 200);
    let errors = update.body["data"]["productVariantUpdate"]["userErrors"]
        .as_array()
        .expect("validation user errors should be an array");
    assert!(errors.contains(&json!({
        "field": ["price"],
        "message": "Price must be greater than or equal to 0",
        "code": "GREATER_THAN_OR_EQUAL_TO"
    })));
    assert!(errors.contains(&json!({
        "field": ["sku"],
        "message": "SKU is too long (maximum is 255 characters)",
        "code": "INVALID_INPUT"
    })));
    assert!(errors.contains(&json!({
        "field": ["inventoryItem", "measurement", "weight"],
        "message": "Weight must be greater than or equal to 0",
        "code": "GREATER_THAN_OR_EQUAL_TO"
    })));

    let read = proxy.process_request(json_graphql_request(
        r#"
        query LegacyVariantUnchangedAfterInvalidUpdate($id: ID!) {
          productVariant(id: $id) { id sku price }
        }
        "#,
        json!({ "id": seeded_variant_id }),
    ));
    assert_eq!(
        read.body["data"]["productVariant"],
        json!({
            "id": seeded_variant_id,
            "sku": "VALID-SKU",
            "price": "10.00"
        })
    );
}

#[test]
fn product_variants_bulk_create_stages_locally_and_hydrates_downstream_reads() {
    let forwarded = Arc::new(Mutex::new(0usize));
    let captured = Arc::clone(&forwarded);
    let mut proxy = configured_proxy(ReadMode::LiveHybrid, None)
        .with_base_products(vec![seed_product("gid://shopify/Product/1")])
        .with_upstream_transport(move |_| {
            *captured.lock().unwrap() += 1;
            panic!("bulk variant create should not call upstream")
        });

    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation BulkVariantCreate($productId: ID!, $variants: [ProductVariantsBulkInput!]!) {
          productVariantsBulkCreate(productId: $productId, variants: $variants) {
            product { id totalInventory tracksInventory variants(first: 10) { nodes { id title sku price inventoryQuantity selectedOptions { name value } inventoryItem { id tracked requiresShipping harmonizedSystemCode measurement { weight { value unit } } } } } }
            productVariants { id title sku price inventoryQuantity selectedOptions { name value } inventoryItem { id tracked requiresShipping harmonizedSystemCode measurement { weight { value unit } } } }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "productId": "gid://shopify/Product/1",
            "variants": [{
                "price": "9.99",
                "inventoryQuantities": [{ "availableQuantity": 7, "locationId": "gid://shopify/Location/1" }],
                "optionValues": [{ "optionName": "Color", "name": "Blue" }],
                "inventoryItem": {
                    "sku": "BULK-BLUE",
                    "tracked": true,
                    "requiresShipping": false,
                    "harmonizedSystemCode": "1234.56",
                    "measurement": { "weight": { "value": 2.5, "unit": "KILOGRAMS" } }
                }
            }]
        }),
    ));

    assert_eq!(create.status, 200);
    assert_eq!(
        create.body["data"]["productVariantsBulkCreate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        create.body["data"]["productVariantsBulkCreate"]["productVariants"][0]["sku"],
        json!("BULK-BLUE")
    );
    assert_eq!(
        create.body["data"]["productVariantsBulkCreate"]["productVariants"][0]["title"],
        json!("Blue")
    );
    assert_eq!(
        create.body["data"]["productVariantsBulkCreate"]["product"]["totalInventory"],
        json!(7)
    );
    assert_eq!(
        create.body["data"]["productVariantsBulkCreate"]["product"]["tracksInventory"],
        json!(true)
    );
    assert_eq!(
        create.body["data"]["productVariantsBulkCreate"]["productVariants"][0]["inventoryItem"]
            ["harmonizedSystemCode"],
        json!("123456")
    );
    let variant_id = create.body["data"]["productVariantsBulkCreate"]["productVariants"][0]["id"]
        .as_str()
        .expect("created variant id should be present")
        .to_string();
    let inventory_item_id = create.body["data"]["productVariantsBulkCreate"]["productVariants"][0]
        ["inventoryItem"]["id"]
        .as_str()
        .expect("created inventory item id should be present")
        .to_string();

    let read = proxy.process_request(json_graphql_request(
        r#"
        query BulkVariantCreateRead($productId: ID!, $variantId: ID!, $inventoryItemId: ID!) {
          product(id: $productId) { id variants(first: 10) { nodes { id sku selectedOptions { name value } inventoryItem { id tracked requiresShipping } } } }
          variant: productVariant(id: $variantId) { id sku price product { id title } }
          stock: inventoryItem(id: $inventoryItemId) { id tracked requiresShipping variant { id sku inventoryQuantity } }
        }
        "#,
        json!({
            "productId": "gid://shopify/Product/1",
            "variantId": variant_id,
            "inventoryItemId": inventory_item_id
        }),
    ));

    assert_eq!(read.status, 200);
    assert_eq!(
        read.body["data"]["product"]["variants"]["nodes"][0]["sku"],
        json!("BULK-BLUE")
    );
    assert_eq!(read.body["data"]["variant"]["price"], json!("9.99"));
    assert_eq!(
        read.body["data"]["stock"]["variant"]["inventoryQuantity"],
        json!(7)
    );
    assert_eq!(*forwarded.lock().unwrap(), 0);
    assert_eq!(
        log_snapshot(&proxy)["entries"][0]["interpreted"]["capability"],
        json!({
            "operationName": "productVariantsBulkCreate",
            "domain": "products",
            "execution": "stage-locally"
        })
    );
}

#[test]
fn product_variants_bulk_create_omitted_strategy_removes_default_standalone_variant() {
    let forwarded = Arc::new(Mutex::new(0usize));
    let captured = Arc::clone(&forwarded);
    let mut proxy =
        configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport(move |_| {
            *captured.lock().unwrap() += 1;
            panic!("product create and bulk variant create should stage locally")
        });

    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateStandaloneDefaultProduct($product: ProductCreateInput!) {
          productCreate(product: $product) {
            product {
              id
              variants(first: 10) {
                nodes { title selectedOptions { name value } }
              }
            }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "product": { "title": "Omitted strategy source" } }),
    ));
    assert_eq!(create.status, 200);
    assert_eq!(
        create.body["data"]["productCreate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        create.body["data"]["productCreate"]["product"]["variants"]["nodes"],
        json!([{
            "title": "Default Title",
            "selectedOptions": [{ "name": "Title", "value": "Default Title" }]
        }])
    );
    let product_id = create.body["data"]["productCreate"]["product"]["id"]
        .as_str()
        .expect("productCreate should return a product id")
        .to_string();

    let bulk_create = proxy.process_request(json_graphql_request(
        r#"
        mutation BulkCreateWithOmittedStrategy(
          $productId: ID!
          $variants: [ProductVariantsBulkInput!]!
        ) {
          productVariantsBulkCreate(productId: $productId, variants: $variants) {
            product {
              options { name values optionValues { name hasVariants } }
              variants(first: 10) {
                nodes { title selectedOptions { name value } }
              }
            }
            productVariants { title selectedOptions { name value } }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "productId": product_id,
            "variants": [{
                "price": "25.00",
                "optionValues": [{ "optionName": "Title", "name": "Default Blue" }],
                "inventoryItem": {
                    "sku": "OMITTED-DEFAULT",
                    "tracked": true,
                    "requiresShipping": true
                }
            }]
        }),
    ));

    assert_eq!(bulk_create.status, 200);
    assert_eq!(
        bulk_create.body["data"]["productVariantsBulkCreate"]["userErrors"],
        json!([])
    );
    let expected_variants = json!([{
        "title": "Default Blue",
        "selectedOptions": [{ "name": "Title", "value": "Default Blue" }]
    }]);
    assert_eq!(
        bulk_create.body["data"]["productVariantsBulkCreate"]["productVariants"],
        expected_variants
    );
    assert_eq!(
        bulk_create.body["data"]["productVariantsBulkCreate"]["product"]["variants"]["nodes"],
        expected_variants
    );
    assert_eq!(
        bulk_create.body["data"]["productVariantsBulkCreate"]["product"]["options"],
        json!([{
            "name": "Title",
            "values": ["Default Blue"],
            "optionValues": [{ "name": "Default Blue", "hasVariants": true }]
        }])
    );

    let read = proxy.process_request(json_graphql_request(
        r#"
        query ReadBulkCreateWithOmittedStrategy($productId: ID!) {
          product(id: $productId) {
            options { name values optionValues { name hasVariants } }
            variants(first: 10) {
              nodes { title selectedOptions { name value } }
            }
          }
        }
        "#,
        json!({ "productId": product_id }),
    ));
    assert_eq!(read.status, 200);
    assert_eq!(
        read.body["data"]["product"]["variants"]["nodes"],
        expected_variants
    );
    assert_eq!(
        read.body["data"]["product"]["options"],
        json!([{
            "name": "Title",
            "values": ["Default Blue"],
            "optionValues": [{ "name": "Default Blue", "hasVariants": true }]
        }])
    );
    assert_eq!(*forwarded.lock().unwrap(), 0);
    assert_eq!(
        log_snapshot(&proxy)["entries"][1]["interpreted"]["capability"],
        json!({
            "operationName": "productVariantsBulkCreate",
            "domain": "products",
            "execution": "stage-locally"
        })
    );
}

#[test]
fn product_variants_bulk_create_rejects_inventory_quantity_caps_atomically() {
    let product_id = "gid://shopify/Product/1";
    let mutation = r#"
        mutation BulkVariantCreateInventoryCaps($productId: ID!, $variants: [ProductVariantsBulkInput!]!) {
          productVariantsBulkCreate(productId: $productId, variants: $variants) {
            product { id }
            productVariants { id }
            userErrors { field message code }
          }
        }
        "#;
    let read_query = r#"
        query BulkVariantCreateInventoryCapsRead($productId: ID!) {
          product(id: $productId) {
            variants(first: 10) { nodes { id } }
          }
        }
        "#;
    let inventory_quantities = (0..25)
        .map(|_| json!({ "availableQuantity": 1, "locationId": "gid://shopify/Location/1" }))
        .collect::<Vec<_>>();
    let variants_over_total_limit = (0..2048)
        .map(|_| json!({ "inventoryQuantities": inventory_quantities.clone() }))
        .collect::<Vec<_>>();

    let mut proxy = snapshot_proxy().with_base_products(vec![seed_product(product_id)]);
    let response = proxy.process_request(json_graphql_request(
        mutation,
        json!({
            "productId": product_id,
            "variants": variants_over_total_limit
        }),
    ));

    assert_eq!(response.status, 200);
    assert_eq!(
        response.body["data"]["productVariantsBulkCreate"],
        json!({
            "product": null,
            "productVariants": [],
            "userErrors": [{
                "field": ["variants"],
                "message": "Inventory quantity input exceeds the limit of 50000. Consider using separate `inventorySetQuantities` mutations.",
                "code": "INVALID_INPUT"
            }]
        })
    );
    assert_eq!(log_snapshot(&proxy)["entries"], json!([]));
    let read = proxy.process_request(json_graphql_request(
        read_query,
        json!({ "productId": product_id }),
    ));
    assert_eq!(read.body["data"]["product"]["variants"]["nodes"], json!([]));

    let too_many_locations = (0..201)
        .map(|_| json!({ "availableQuantity": 1, "locationId": "gid://shopify/Location/1" }))
        .collect::<Vec<_>>();
    let mut proxy = snapshot_proxy().with_base_products(vec![seed_product(product_id)]);
    let response = proxy.process_request(json_graphql_request(
        mutation,
        json!({
            "productId": product_id,
            "variants": [{
                "price": "10",
                "inventoryQuantities": too_many_locations
            }]
        }),
    ));

    assert_eq!(response.status, 200);
    assert_eq!(
        response.body["data"]["productVariantsBulkCreate"],
        json!({
            "product": null,
            "productVariants": [],
            "userErrors": [{
                "field": ["variants", "0"],
                "message": "Inventory locations cannot exceed the allowed resource limit",
                "code": "TOO_MANY_INVENTORY_LOCATIONS"
            }]
        })
    );
    assert_eq!(log_snapshot(&proxy)["entries"], json!([]));
    let read = proxy.process_request(json_graphql_request(
        read_query,
        json!({ "productId": product_id }),
    ));
    assert_eq!(read.body["data"]["product"]["variants"]["nodes"], json!([]));
}

#[test]
fn product_variants_bulk_reorder_rejects_invalid_inputs_atomically() {
    let mut proxy = configured_proxy(ReadMode::LiveHybrid, None)
        .with_base_products(vec![seed_product("gid://shopify/Product/1")])
        .with_upstream_transport(|_| panic!("bulk variant mutation should not call upstream"));
    let red = create_legacy_variant(&mut proxy, "gid://shopify/Product/1", "RED", "10.00");
    let blue = create_legacy_variant(&mut proxy, "gid://shopify/Product/1", "BLUE", "11.00");
    let green = create_legacy_variant(&mut proxy, "gid://shopify/Product/1", "GREEN", "12.00");
    let red_id = red["id"].as_str().unwrap().to_string();
    let blue_id = blue["id"].as_str().unwrap().to_string();
    let green_id = green["id"].as_str().unwrap().to_string();
    let original_order = json!([
        { "sku": "RED" },
        { "sku": "BLUE" },
        { "sku": "GREEN" }
    ]);

    let invalid_position = proxy.process_request(json_graphql_request(
        r#"
        mutation InvalidVariantPosition($productId: ID!, $positions: [ProductVariantPositionInput!]!) {
          productVariantsBulkReorder(productId: $productId, positions: $positions) {
            product { variants(first: 10) { nodes { sku position } } }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "productId": "gid://shopify/Product/1",
            "positions": [
                { "id": green_id, "position": 0 },
                { "id": red_id, "position": 2 }
            ]
        }),
    ));
    assert_eq!(invalid_position.status, 200);
    assert_eq!(
        invalid_position.body["data"]["productVariantsBulkReorder"]["product"],
        Value::Null
    );
    assert_user_error_with_field_and_code(
        &invalid_position.body["data"]["productVariantsBulkReorder"]["userErrors"],
        json!(["positions", "0", "position"]),
        "INVALID_POSITION",
    );
    assert_eq!(
        read_variant_sku_positions(&mut proxy, "gid://shopify/Product/1"),
        original_order
    );

    let duplicate_id = proxy.process_request(json_graphql_request(
        r#"
        mutation DuplicatedVariantPosition($productId: ID!, $positions: [ProductVariantPositionInput!]!) {
          productVariantsBulkReorder(productId: $productId, positions: $positions) {
            product { id }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "productId": "gid://shopify/Product/1",
            "positions": [
                { "id": blue_id, "position": 1 },
                { "id": blue_id, "position": 2 }
            ]
        }),
    ));
    assert_eq!(duplicate_id.status, 200);
    assert_eq!(
        duplicate_id.body["data"]["productVariantsBulkReorder"]["product"],
        Value::Null
    );
    assert_user_error_with_field_and_code(
        &duplicate_id.body["data"]["productVariantsBulkReorder"]["userErrors"],
        json!(["positions"]),
        "DUPLICATED_VARIANT_ID",
    );
    assert_eq!(
        read_variant_sku_positions(&mut proxy, "gid://shopify/Product/1"),
        original_order
    );

    let missing_variant = proxy.process_request(json_graphql_request(
        r#"
        mutation MissingVariantPosition($productId: ID!, $positions: [ProductVariantPositionInput!]!) {
          productVariantsBulkReorder(productId: $productId, positions: $positions) {
            product { id }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "productId": "gid://shopify/Product/1",
            "positions": [
                { "position": 1 },
                { "id": "gid://shopify/ProductVariant/missing", "position": 2 }
            ]
        }),
    ));
    assert_eq!(missing_variant.status, 200);
    assert_eq!(
        missing_variant.body["data"]["productVariantsBulkReorder"]["product"],
        Value::Null
    );
    assert_user_error_with_field_and_code(
        &missing_variant.body["data"]["productVariantsBulkReorder"]["userErrors"],
        json!(["positions", "0", "id"]),
        "MISSING_VARIANT",
    );
    assert_user_error_with_field_and_code(
        &missing_variant.body["data"]["productVariantsBulkReorder"]["userErrors"],
        json!(["positions", "1", "id"]),
        "MISSING_VARIANT",
    );
    assert_eq!(
        read_variant_sku_positions(&mut proxy, "gid://shopify/Product/1"),
        original_order
    );
}

#[test]
fn product_variants_bulk_reorder_and_update_resequence_positions() {
    let mut proxy = configured_proxy(ReadMode::LiveHybrid, None)
        .with_base_products(vec![seed_product("gid://shopify/Product/1")])
        .with_upstream_transport(|_| panic!("bulk variant mutation should not call upstream"));
    let red = create_legacy_variant(&mut proxy, "gid://shopify/Product/1", "RED", "10.00");
    let blue = create_legacy_variant(&mut proxy, "gid://shopify/Product/1", "BLUE", "11.00");
    let green = create_legacy_variant(&mut proxy, "gid://shopify/Product/1", "GREEN", "12.00");
    let red_id = red["id"].as_str().unwrap().to_string();
    let blue_id = blue["id"].as_str().unwrap().to_string();
    let green_id = green["id"].as_str().unwrap().to_string();

    let reorder = proxy.process_request(json_graphql_request(
        r#"
        mutation ReorderVariants($productId: ID!, $positions: [ProductVariantPositionInput!]!) {
          productVariantsBulkReorder(productId: $productId, positions: $positions) {
            product {
              variants(first: 10) {
                nodes { id sku position }
              }
            }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "productId": "gid://shopify/Product/1",
            "positions": [
                { "id": green_id, "position": 1 },
                { "id": red_id, "position": 2 }
            ]
        }),
    ));
    assert_eq!(reorder.status, 200);
    assert_eq!(
        reorder.body["data"]["productVariantsBulkReorder"]["userErrors"],
        json!([])
    );
    assert_eq!(
        reorder.body["data"]["productVariantsBulkReorder"]["product"]["variants"]["nodes"],
        json!([
            { "id": green_id, "sku": "GREEN", "position": 1 },
            { "id": red_id, "sku": "RED", "position": 2 },
            { "id": blue_id, "sku": "BLUE", "position": 3 }
        ])
    );
    assert_eq!(
        read_variant_sku_positions(&mut proxy, "gid://shopify/Product/1"),
        json!([
            { "sku": "GREEN", "position": 1 },
            { "sku": "RED", "position": 2 },
            { "sku": "BLUE", "position": 3 }
        ])
    );

    let public_position_input = proxy.process_request(json_graphql_request(
        r#"
        mutation PublicBulkUpdatePosition($productId: ID!, $variants: [ProductVariantsBulkInput!]!) {
          productVariantsBulkUpdate(productId: $productId, variants: $variants) {
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "productId": "gid://shopify/Product/1",
            "variants": [
                { "id": blue_id, "position": 1 }
            ]
        }),
    ));
    assert_eq!(public_position_input.status, 200);
    assert_eq!(
        public_position_input.body,
        json!({
            "errors": [{
                "message": "Variable $variants of type [ProductVariantsBulkInput!]! was provided invalid value for 0.position (Field is not defined on ProductVariantsBulkInput)",
                "locations": [{ "line": 2, "column": 60 }],
                "extensions": {
                    "code": "INVALID_VARIABLE",
                    "value": [{
                        "id": blue_id,
                        "position": 1
                    }],
                    "problems": [{
                        "path": [0, "position"],
                        "explanation": "Field is not defined on ProductVariantsBulkInput"
                    }]
                }
            }]
        })
    );
}

#[test]
fn product_variants_bulk_create_rejects_option_conflicts_and_duplicate_tuples_atomically() {
    let product_id = "gid://shopify/Product/1";
    let mutation = r#"
        mutation BulkVariantCreateValidation($productId: ID!, $variants: [ProductVariantsBulkInput!]!) {
          productVariantsBulkCreate(productId: $productId, variants: $variants) {
            product { id variants(first: 10) { nodes { id selectedOptions { name value } } } }
            productVariants { id selectedOptions { name value } }
            userErrors { field message code }
          }
        }
        "#;

    let mut proxy =
        snapshot_proxy().with_base_products(vec![seed_product_with_options(product_id)]);
    let options_response = proxy.process_request(json_graphql_request(
        mutation,
        json!({
            "productId": product_id,
            "variants": [{
                "options": ["Blue", "Large"],
                "optionValues": [
                    { "optionName": "Color", "name": "Blue" },
                    { "optionName": "Size", "name": "Large" }
                ]
            }]
        }),
    ));
    assert_eq!(options_response.status, 200);
    assert_eq!(
        options_response.body,
        json!({
            "errors": [{
                "message": "Variable $variants of type [ProductVariantsBulkInput!]! was provided invalid value for 0.options (Field is not defined on ProductVariantsBulkInput)",
                "locations": [{ "line": 2, "column": 63 }],
                "extensions": {
                    "code": "INVALID_VARIABLE",
                    "value": [{
                        "optionValues": [
                            { "name": "Blue", "optionName": "Color" },
                            { "name": "Large", "optionName": "Size" }
                        ],
                        "options": ["Blue", "Large"]
                    }],
                    "problems": [{
                        "path": [0, "options"],
                        "explanation": "Field is not defined on ProductVariantsBulkInput"
                    }]
                }
            }]
        })
    );
    assert_eq!(
        log_snapshot(&proxy)["entries"],
        json!([]),
        "schema-level options rejection should not stage a mutation log entry"
    );
    let read = proxy.process_request(json_graphql_request(
        r#"
        query BulkVariantCreateInvalidOptionsAtomicRead($productId: ID!) {
          product(id: $productId) {
            variants(first: 10) { nodes { id selectedOptions { name value } } }
          }
        }
        "#,
        json!({ "productId": product_id }),
    ));
    assert_eq!(
        read.body["data"]["product"]["variants"]["nodes"],
        json!([]),
        "schema-level options rejection should not stage variants"
    );

    let cases = [
        (
            "structured option id and name conflict",
            json!([{
                "optionValues": [
                    {
                        "optionId": "gid://shopify/ProductOption/1",
                        "optionName": "Color",
                        "name": "Red"
                    },
                    { "optionName": "Size", "name": "Small" }
                ]
            }]),
            json!({
                "field": ["variants", "0", "optionValues", "0"],
                "message": "cannot specify both `optionId` and `optionName`",
                "code": "INVALID_INPUT"
            }),
        ),
        (
            "structured option value id and name conflict",
            json!([{
                "optionValues": [
                    {
                        "optionName": "Color",
                        "id": "gid://shopify/ProductOptionValue/1",
                        "name": "Red"
                    },
                    { "optionName": "Size", "name": "Small" }
                ]
            }]),
            json!({
                "field": ["variants", "0", "optionValues", "0"],
                "message": "cannot specify both `id` and `name`",
                "code": "INVALID_INPUT"
            }),
        ),
        (
            "duplicate option tuple in one bulk create",
            json!([
                {
                    "optionValues": [
                        { "optionName": "Color", "name": "Red" },
                        { "optionName": "Size", "name": "Small" }
                    ]
                },
                {
                    "optionValues": [
                        { "optionName": "Color", "name": "Red" },
                        { "optionName": "Size", "name": "Small" }
                    ]
                }
            ]),
            json!({
                "field": ["variants", "1"],
                "message": "The variant 'Red / Small' already exists. Please change at least one option value.",
                "code": "VARIANT_ALREADY_EXISTS_CHANGE_OPTION_VALUE"
            }),
        ),
    ];

    for (label, variants, expected_error) in cases {
        let mut proxy =
            snapshot_proxy().with_base_products(vec![seed_product_with_options(product_id)]);
        let response = proxy.process_request(json_graphql_request(
            mutation,
            json!({ "productId": product_id, "variants": variants }),
        ));

        assert_eq!(response.status, 200, "{label}");
        assert_eq!(
            response.body["data"]["productVariantsBulkCreate"]["product"],
            Value::Null,
            "{label}"
        );
        assert_eq!(
            response.body["data"]["productVariantsBulkCreate"]["productVariants"],
            json!([]),
            "{label}"
        );
        assert_eq!(
            response.body["data"]["productVariantsBulkCreate"]["userErrors"],
            json!([expected_error]),
            "{label}"
        );
        assert_eq!(
            log_snapshot(&proxy)["entries"],
            json!([]),
            "{label}: rejected create should not stage a mutation log entry"
        );

        let read = proxy.process_request(json_graphql_request(
            r#"
            query BulkVariantCreateValidationAtomicRead($productId: ID!) {
              product(id: $productId) {
                variants(first: 10) { nodes { id selectedOptions { name value } } }
              }
            }
            "#,
            json!({ "productId": product_id }),
        ));
        assert_eq!(
            read.body["data"]["product"]["variants"]["nodes"],
            json!([]),
            "{label}: rejected create should not stage variants"
        );
    }
}

#[test]
fn product_variant_delete_compacts_surviving_variant_positions() {
    let product_id = "gid://shopify/Product/1";
    let mut proxy = snapshot_proxy().with_base_products(vec![seed_product(product_id)]);
    let variants =
        create_bulk_positioned_variants(&mut proxy, product_id, &["RED", "BLUE", "GREEN"]);
    let red_id = variants[0]["id"].as_str().unwrap().to_string();
    let blue_id = variants[1]["id"].as_str().unwrap().to_string();
    let green_id = variants[2]["id"].as_str().unwrap().to_string();

    let delete = proxy.process_request(json_graphql_request(
        r#"
        mutation DeleteMiddleVariant($id: ID!) {
          productVariantDelete(id: $id) {
            deletedProductVariantId
            userErrors { field message }
          }
        }
        "#,
        json!({ "id": blue_id }),
    ));
    assert_eq!(delete.status, 200);
    assert_eq!(
        delete.body["data"]["productVariantDelete"]["deletedProductVariantId"],
        json!(blue_id)
    );
    assert_eq!(
        delete.body["data"]["productVariantDelete"]["userErrors"],
        json!([])
    );

    let read = proxy.process_request(json_graphql_request(
        r#"
        query VariantPositionsAfterSingleDelete($productId: ID!) {
          product(id: $productId) {
            variants(first: 10) { nodes { id sku position } }
          }
        }
        "#,
        json!({ "productId": product_id }),
    ));
    assert_eq!(
        read.body["data"]["product"]["variants"]["nodes"],
        json!([
            { "id": red_id, "sku": "RED", "position": 1 },
            { "id": green_id, "sku": "GREEN", "position": 2 }
        ])
    );
}

#[test]
fn product_variants_bulk_delete_compacts_surviving_variant_positions() {
    let product_id = "gid://shopify/Product/1";
    let mut proxy = snapshot_proxy().with_base_products(vec![seed_product(product_id)]);
    let variants = create_bulk_positioned_variants(
        &mut proxy,
        product_id,
        &["RED", "BLUE", "GREEN", "YELLOW"],
    );
    let red_id = variants[0]["id"].as_str().unwrap().to_string();
    let blue_id = variants[1]["id"].as_str().unwrap().to_string();
    let green_id = variants[2]["id"].as_str().unwrap().to_string();
    let yellow_id = variants[3]["id"].as_str().unwrap().to_string();

    let delete = proxy.process_request(json_graphql_request(
        r#"
        mutation BulkDeleteMiddleVariants($productId: ID!, $variantsIds: [ID!]!) {
          productVariantsBulkDelete(productId: $productId, variantsIds: $variantsIds) {
            product { variants(first: 10) { nodes { id sku position } } }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "productId": product_id, "variantsIds": [blue_id, green_id] }),
    ));
    assert_eq!(delete.status, 200);
    assert_eq!(
        delete.body["data"]["productVariantsBulkDelete"]["userErrors"],
        json!([])
    );
    assert_eq!(
        delete.body["data"]["productVariantsBulkDelete"]["product"]["variants"]["nodes"],
        json!([
            { "id": red_id, "sku": "RED", "position": 1 },
            { "id": yellow_id, "sku": "YELLOW", "position": 2 }
        ])
    );

    let read = proxy.process_request(json_graphql_request(
        r#"
        query VariantPositionsAfterBulkDelete($productId: ID!) {
          product(id: $productId) {
            variants(first: 10) { nodes { id sku position } }
          }
        }
        "#,
        json!({ "productId": product_id }),
    ));
    assert_eq!(
        read.body["data"]["product"]["variants"]["nodes"],
        json!([
            { "id": red_id, "sku": "RED", "position": 1 },
            { "id": yellow_id, "sku": "YELLOW", "position": 2 }
        ])
    );
}

#[test]
fn product_variants_bulk_update_delete_and_reorder_stage_atomically() {
    let forwarded = Arc::new(Mutex::new(0usize));
    let captured = Arc::clone(&forwarded);
    let mut proxy = configured_proxy(ReadMode::LiveHybrid, None)
        .with_base_products(vec![seed_product("gid://shopify/Product/1")])
        .with_upstream_transport(move |_| {
            *captured.lock().unwrap() += 1;
            panic!("bulk variant mutation should not call upstream")
        });
    let red = create_legacy_variant(&mut proxy, "gid://shopify/Product/1", "RED", "10.00");
    let blue = create_legacy_variant(&mut proxy, "gid://shopify/Product/1", "BLUE", "11.00");
    let red_id = red["id"].as_str().unwrap().to_string();
    let blue_id = blue["id"].as_str().unwrap().to_string();

    let log_entries_before_empty_update = log_snapshot(&proxy)["entries"].as_array().unwrap().len();
    let empty_update = proxy.process_request(json_graphql_request(
        r#"
        mutation BulkVariantEmptyUpdate($productId: ID!, $variants: [ProductVariantsBulkInput!]!) {
          productVariantsBulkUpdate(productId: $productId, variants: $variants) {
            product { id totalInventory tracksInventory }
            productVariants { id }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "productId": "gid://shopify/Product/1",
            "variants": []
        }),
    ));
    assert_eq!(empty_update.status, 200);
    assert_eq!(
        empty_update.body["data"]["productVariantsBulkUpdate"],
        json!({
            "product": {
                "id": "gid://shopify/Product/1",
                "totalInventory": 0,
                "tracksInventory": true
            },
            "productVariants": [],
            "userErrors": []
        })
    );
    assert_eq!(
        log_snapshot(&proxy)["entries"].as_array().unwrap().len(),
        log_entries_before_empty_update,
        "empty update should be a no-op response without a staged mutation log entry"
    );

    let invalid_update = proxy.process_request(json_graphql_request(
        r#"
        mutation BulkVariantInvalidUpdate($productId: ID!, $variants: [ProductVariantsBulkInput!]!) {
          productVariantsBulkUpdate(productId: $productId, variants: $variants) {
            productVariants { id sku price }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "productId": "gid://shopify/Product/1",
            "variants": [
                { "id": red_id, "inventoryItem": { "sku": "RED-UPDATED" } },
                { "id": blue_id, "price": "-1.00" }
            ]
        }),
    ));
    assert_eq!(invalid_update.status, 200);
    assert_eq!(
        invalid_update.body["data"]["productVariantsBulkUpdate"]["productVariants"],
        json!([
            { "id": red_id, "sku": "RED", "price": "10.00" },
            { "id": blue_id, "sku": "BLUE", "price": "11.00" }
        ])
    );
    assert!(
        invalid_update.body["data"]["productVariantsBulkUpdate"]["userErrors"]
            .as_array()
            .unwrap()
            .contains(&json!({
                "field": ["variants", "1", "price"],
                "message": "Price must be greater than or equal to 0",
                "code": "GREATER_THAN_OR_EQUAL_TO"
            }))
    );
    let unchanged = proxy.process_request(json_graphql_request(
        r#"query BulkVariantUpdateAtomicRead($red: ID!, $blue: ID!) {
          red: productVariant(id: $red) { sku price }
          blue: productVariant(id: $blue) { sku price }
        }"#,
        json!({ "red": red_id, "blue": blue_id }),
    ));
    assert_eq!(
        unchanged.body["data"]["red"],
        json!({"sku": "RED", "price": "10.00"})
    );
    assert_eq!(
        unchanged.body["data"]["blue"],
        json!({"sku": "BLUE", "price": "11.00"})
    );

    let update = proxy.process_request(json_graphql_request(
        r#"
        mutation BulkVariantUpdate($productId: ID!, $variants: [ProductVariantsBulkInput!]!) {
          productVariantsBulkUpdate(productId: $productId, variants: $variants) {
            productVariants { id sku price inventoryItem { tracked } }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "productId": "gid://shopify/Product/1",
            "variants": [
                { "id": red_id, "inventoryItem": { "sku": "RED-UPDATED", "tracked": false }, "price": "12.00" },
                { "id": blue_id, "inventoryItem": { "sku": "BLUE-UPDATED" }, "price": "13.00" }
            ]
        }),
    ));
    assert_eq!(update.status, 200);
    assert_eq!(
        update.body["data"]["productVariantsBulkUpdate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        update.body["data"]["productVariantsBulkUpdate"]["productVariants"][0]["sku"],
        json!("RED-UPDATED")
    );

    let reorder = proxy.process_request(json_graphql_request(
        r#"
        mutation BulkVariantReorder($productId: ID!, $positions: [ProductVariantPositionInput!]!) {
          productVariantsBulkReorder(productId: $productId, positions: $positions) {
            product { variants(first: 10) { nodes { id sku } } }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "productId": "gid://shopify/Product/1",
            "positions": [
                { "id": blue_id, "position": 1 },
                { "id": red_id, "position": 2 }
            ]
        }),
    ));
    assert_eq!(reorder.status, 200);
    assert_eq!(
        reorder.body["data"]["productVariantsBulkReorder"]["product"]["variants"]["nodes"][0]
            ["sku"],
        json!("BLUE-UPDATED")
    );
    assert_eq!(
        reorder.body["data"]["productVariantsBulkReorder"]["product"]["variants"]["nodes"][1]
            ["sku"],
        json!("RED-UPDATED")
    );

    let invalid_delete = proxy.process_request(json_graphql_request(
        r#"
        mutation BulkVariantInvalidDelete($productId: ID!, $variantsIds: [ID!]!) {
          productVariantsBulkDelete(productId: $productId, variantsIds: $variantsIds) {
            product { variants(first: 10) { nodes { id } } }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "productId": "gid://shopify/Product/1",
            "variantsIds": [blue_id, "gid://shopify/ProductVariant/missing"]
        }),
    ));
    assert_eq!(invalid_delete.status, 200);
    assert_eq!(
        invalid_delete.body["data"]["productVariantsBulkDelete"]["product"],
        Value::Null
    );
    assert!(
        invalid_delete.body["data"]["productVariantsBulkDelete"]["userErrors"]
            .as_array()
            .unwrap()
            .contains(&json!({
                "field": ["variantsIds", "1"],
                "message": "At least one variant does not belong to the product",
                "code": "AT_LEAST_ONE_VARIANT_DOES_NOT_BELONG_TO_THE_PRODUCT"
            }))
    );
    let after_invalid_delete = proxy.process_request(json_graphql_request(
        r#"query BulkVariantInvalidDeleteRead($productId: ID!) {
          product(id: $productId) { variants(first: 10) { nodes { sku } } }
        }"#,
        json!({ "productId": "gid://shopify/Product/1" }),
    ));
    assert_eq!(
        after_invalid_delete.body["data"]["product"]["variants"]["nodes"],
        json!([{ "sku": "BLUE-UPDATED" }, { "sku": "RED-UPDATED" }])
    );

    let delete = proxy.process_request(json_graphql_request(
        r#"
        mutation BulkVariantDelete($productId: ID!, $variantsIds: [ID!]!) {
          productVariantsBulkDelete(productId: $productId, variantsIds: $variantsIds) {
            product { variants(first: 10) { nodes { id sku } } }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "productId": "gid://shopify/Product/1",
            "variantsIds": [blue_id]
        }),
    ));
    assert_eq!(delete.status, 200);
    assert_eq!(
        delete.body["data"]["productVariantsBulkDelete"]["product"]["variants"]["nodes"],
        json!([{ "id": red_id, "sku": "RED-UPDATED" }])
    );

    assert_eq!(*forwarded.lock().unwrap(), 0);
}

#[test]
fn product_variants_bulk_update_stages_valid_inputs_when_partial_updates_allowed() {
    let forwarded = Arc::new(Mutex::new(0usize));
    let captured = Arc::clone(&forwarded);
    let product_id = "gid://shopify/Product/partial-update";
    let mut proxy = configured_proxy(ReadMode::LiveHybrid, None)
        .with_base_products(vec![seed_product(product_id)])
        .with_upstream_transport(move |_| {
            *captured.lock().unwrap() += 1;
            panic!("supported bulk variant update should not call upstream")
        });
    let red = create_legacy_variant(&mut proxy, product_id, "RED", "10.00");
    let blue = create_legacy_variant(&mut proxy, product_id, "BLUE", "11.00");
    let red_id = red["id"].as_str().unwrap().to_string();
    let blue_id = blue["id"].as_str().unwrap().to_string();

    let update = proxy.process_request(json_graphql_request(
        r#"
        mutation BulkVariantPartialUpdate($productId: ID!, $variants: [ProductVariantsBulkInput!]!) {
          productVariantsBulkUpdate(productId: $productId, allowPartialUpdates: true, variants: $variants) {
            productVariants { id sku price }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "productId": product_id,
            "variants": [
                { "id": red_id, "inventoryItem": { "sku": "RED-PARTIAL" }, "price": "9.99" },
                { "id": blue_id, "price": "-1.00" }
            ]
        }),
    ));
    assert_eq!(update.status, 200);
    assert_eq!(
        update.body["data"]["productVariantsBulkUpdate"]["productVariants"],
        json!([
            { "id": red_id, "sku": "RED-PARTIAL", "price": "9.99" },
            { "id": blue_id, "sku": "BLUE", "price": "11.00" }
        ])
    );
    assert_eq!(
        update.body["data"]["productVariantsBulkUpdate"]["userErrors"],
        json!([{
            "field": ["variants", "1", "price"],
            "message": "Price must be greater than or equal to 0",
            "code": "GREATER_THAN_OR_EQUAL_TO"
        }])
    );

    let read = proxy.process_request(json_graphql_request(
        r#"
        query BulkVariantPartialUpdateRead($red: ID!, $blue: ID!) {
          red: productVariant(id: $red) { sku price }
          blue: productVariant(id: $blue) { sku price }
        }
        "#,
        json!({ "red": red_id, "blue": blue_id }),
    ));
    assert_eq!(read.status, 200);
    assert_eq!(
        read.body["data"]["red"],
        json!({"sku": "RED-PARTIAL", "price": "9.99"})
    );
    assert_eq!(
        read.body["data"]["blue"],
        json!({"sku": "BLUE", "price": "11.00"})
    );

    let log = log_snapshot(&proxy);
    assert!(
        log["entries"].as_array().unwrap().iter().any(|entry| {
            entry["interpreted"]["operationName"] == json!("productVariantsBulkUpdate")
                && entry["query"]
                    .as_str()
                    .is_some_and(|query| query.contains("allowPartialUpdates: true"))
        }),
        "staged mutation log should preserve the original raw mutation for commit replay: {log:?}"
    );
    assert_eq!(*forwarded.lock().unwrap(), 0);
}

#[test]
fn product_variants_bulk_update_explicit_partial_updates_false_stays_atomic() {
    let product_id = "gid://shopify/Product/partial-update-false";
    let mut proxy = configured_proxy(ReadMode::LiveHybrid, None)
        .with_base_products(vec![seed_product(product_id)])
        .with_upstream_transport(|_| {
            panic!("supported bulk variant update should not call upstream")
        });
    let red = create_legacy_variant(&mut proxy, product_id, "RED", "10.00");
    let blue = create_legacy_variant(&mut proxy, product_id, "BLUE", "11.00");
    let red_id = red["id"].as_str().unwrap().to_string();
    let blue_id = blue["id"].as_str().unwrap().to_string();

    let update = proxy.process_request(json_graphql_request(
        r#"
        mutation BulkVariantAtomicUpdate($productId: ID!, $variants: [ProductVariantsBulkInput!]!) {
          productVariantsBulkUpdate(productId: $productId, allowPartialUpdates: false, variants: $variants) {
            productVariants { id sku price }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "productId": product_id,
            "variants": [
                { "id": red_id, "inventoryItem": { "sku": "RED-SHOULD-NOT-STAGE" }, "price": "9.99" },
                { "id": blue_id, "price": "-1.00" }
            ]
        }),
    ));
    assert_eq!(update.status, 200);
    assert_eq!(
        update.body["data"]["productVariantsBulkUpdate"]["productVariants"],
        json!([
            { "id": red_id, "sku": "RED", "price": "10.00" },
            { "id": blue_id, "sku": "BLUE", "price": "11.00" }
        ])
    );
    assert_eq!(
        update.body["data"]["productVariantsBulkUpdate"]["userErrors"],
        json!([{
            "field": ["variants", "1", "price"],
            "message": "Price must be greater than or equal to 0",
            "code": "GREATER_THAN_OR_EQUAL_TO"
        }])
    );

    let read = proxy.process_request(json_graphql_request(
        r#"
        query BulkVariantAtomicUpdateRead($red: ID!, $blue: ID!) {
          red: productVariant(id: $red) { sku price }
          blue: productVariant(id: $blue) { sku price }
        }
        "#,
        json!({ "red": red_id, "blue": blue_id }),
    ));
    assert_eq!(read.status, 200);
    assert_eq!(
        read.body["data"]["red"],
        json!({"sku": "RED", "price": "10.00"})
    );
    assert_eq!(
        read.body["data"]["blue"],
        json!({"sku": "BLUE", "price": "11.00"})
    );

    let log = log_snapshot(&proxy);
    assert!(
        !log["entries"].as_array().unwrap().iter().any(|entry| {
            entry["interpreted"]["operationName"] == json!("productVariantsBulkUpdate")
        }),
        "atomic validation failure should not stage a bulk update log entry: {log:?}"
    );
}

#[test]
fn product_variants_bulk_update_sorts_user_errors_by_field_and_code() {
    let product_id = "gid://shopify/Product/error-order";
    let mut proxy = configured_proxy(ReadMode::LiveHybrid, None)
        .with_base_products(vec![seed_product(product_id)])
        .with_upstream_transport(|_| {
            panic!("supported bulk variant update should not call upstream")
        });
    let variant = create_legacy_variant(&mut proxy, product_id, "RED", "10.00");
    let variant_id = variant["id"].as_str().unwrap().to_string();

    let update = proxy.process_request(json_graphql_request(
        r#"
        mutation BulkVariantSortedErrors($productId: ID!, $variants: [ProductVariantsBulkInput!]!) {
          productVariantsBulkUpdate(productId: $productId, variants: $variants) {
            productVariants { id }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "productId": product_id,
            "variants": [{
                "id": variant_id,
                "price": "-1.00",
                "compareAtPrice": "1000000000000000000"
            }]
        }),
    ));
    assert_eq!(update.status, 200);
    assert_eq!(
        update.body["data"]["productVariantsBulkUpdate"]["productVariants"],
        json!([{ "id": variant_id }])
    );
    assert_eq!(
        update.body["data"]["productVariantsBulkUpdate"]["userErrors"],
        json!([
            {
                "field": ["variants", "0", "compareAtPrice"],
                "message": "must be less than 1000000000000000000",
                "code": "INVALID_INPUT"
            },
            {
                "field": ["variants", "0", "price"],
                "message": "Price must be greater than or equal to 0",
                "code": "GREATER_THAN_OR_EQUAL_TO"
            }
        ])
    );
}

#[test]
fn product_media_roots_without_store_backed_handlers_fail_closed() {
    let mut proxy = snapshot_proxy();

    for (root, query) in [
        (
            "productCreateMedia",
            r#"mutation { productCreateMedia(productId: "gid://shopify/Product/optioned", media: [{ originalSource: "not-a-url", mediaContentType: IMAGE }]) { media { id } mediaUserErrors { message } } }"#,
        ),
        (
            "productUpdateMedia",
            r#"mutation { productUpdateMedia(productId: "gid://shopify/Product/optioned", media: [{ id: "gid://shopify/MediaImage/missing", alt: "Missing" }]) { media { id } mediaUserErrors { message } } }"#,
        ),
        (
            "productDeleteMedia",
            r#"mutation { productDeleteMedia(productId: "gid://shopify/Product/optioned", mediaIds: ["gid://shopify/MediaImage/missing"]) { deletedMediaIds mediaUserErrors { message } } }"#,
        ),
        (
            "productReorderMedia",
            r#"mutation { productReorderMedia(id: "gid://shopify/Product/optioned", moves: [{ id: "gid://shopify/MediaImage/missing", newPosition: "0" }]) { job { id } mediaUserErrors { message } } }"#,
        ),
        (
            "productVariantAppendMedia",
            r#"mutation { productVariantAppendMedia(productId: "gid://shopify/Product/optioned", variantMedia: [{ variantId: "gid://shopify/ProductVariant/child", mediaIds: ["gid://shopify/MediaImage/ready"] }]) { productVariants { id } userErrors { message } } }"#,
        ),
        (
            "productVariantDetachMedia",
            r#"mutation { productVariantDetachMedia(productId: "gid://shopify/Product/optioned", variantMedia: [{ variantId: "gid://shopify/ProductVariant/default", mediaIds: ["gid://shopify/MediaImage/ready"] }]) { productVariants { id } userErrors { message } } }"#,
        ),
    ] {
        let response = proxy.process_request(json_graphql_request(query, json!({})));
        assert_eq!(response.status, 400, "{root} should fail closed");
        assert_eq!(
            response.body,
            json!({ "errors": [{ "message": format!("No mutation dispatcher implemented for root field: {root}") }] })
        );
    }
    assert_eq!(log_snapshot(&proxy), json!({ "entries": [] }));
}

#[test]
fn product_variant_media_validation_guards_match_captured_shopify_errors() {
    let forwarded = Arc::new(Mutex::new(0usize));
    let captured = Arc::clone(&forwarded);
    let product_id = "gid://shopify/Product/1";
    let mut proxy = configured_proxy(ReadMode::LiveHybrid, None)
        .with_base_products(vec![seed_product(product_id)])
        .with_upstream_transport(move |_| {
            *captured.lock().unwrap() += 1;
            panic!("product variant media validation should not call upstream")
        });
    let variant = create_legacy_variant(&mut proxy, product_id, "MEDIA-VALIDATION", "10.00");
    let variant_id = variant["id"].as_str().unwrap().to_string();
    let ready_media_id =
        create_product_media_for_test(&mut proxy, product_id, "IMAGE", "Ready media");
    settle_product_media_for_test(&mut proxy, product_id, &ready_media_id);
    let second_ready_media_id =
        create_product_media_for_test(&mut proxy, product_id, "IMAGE", "Second ready media");
    settle_product_media_for_test(&mut proxy, product_id, &second_ready_media_id);
    let external_video_id = create_product_media_for_test(
        &mut proxy,
        product_id,
        "EXTERNAL_VIDEO",
        "External video media",
    );

    let too_many_pairs = Value::Array(
        (0..101)
            .map(|_| json!({ "variantId": variant_id, "mediaIds": [ready_media_id] }))
            .collect(),
    );
    assert_eq!(
        append_variant_media_for_test(&mut proxy, product_id, too_many_pairs.clone()),
        json!([{
            "field": ["variantMedia"],
            "message": "Exceeded 100 variant-media pairs per mutation.",
            "code": "MAXIMUM_VARIANT_MEDIA_PAIRS_EXCEEDED"
        }])
    );
    assert_eq!(
        detach_variant_media_for_test(&mut proxy, product_id, too_many_pairs),
        json!([{
            "field": ["variantMedia"],
            "message": "Exceeded 100 variant-media pairs per mutation.",
            "code": "MAXIMUM_VARIANT_MEDIA_PAIRS_EXCEEDED"
        }])
    );

    let too_many_media_ids =
        json!([{ "variantId": variant_id, "mediaIds": [ready_media_id, second_ready_media_id] }]);
    assert_eq!(
        append_variant_media_for_test(&mut proxy, product_id, too_many_media_ids.clone()),
        json!([{
            "field": ["variantMedia", "0", "mediaIds"],
            "message": "Only one mediaId is allowed per media input.",
            "code": "TOO_MANY_MEDIA_PER_INPUT_PAIR"
        }])
    );
    assert_eq!(
        detach_variant_media_for_test(&mut proxy, product_id, too_many_media_ids),
        json!([{
            "field": ["variantMedia", "0", "mediaIds"],
            "message": "Only one mediaId is allowed per media input.",
            "code": "TOO_MANY_MEDIA_PER_INPUT_PAIR"
        }])
    );

    let duplicate_variant = json!([
        { "variantId": variant_id, "mediaIds": [ready_media_id] },
        { "variantId": variant_id, "mediaIds": [second_ready_media_id] }
    ]);
    assert_eq!(
        append_variant_media_for_test(&mut proxy, product_id, duplicate_variant.clone()),
        json!([{
            "field": ["variantMedia", "0", "variantId"],
            "message": "Variant was specified in more than one media input.",
            "code": "PRODUCT_VARIANT_SPECIFIED_MULTIPLE_TIMES"
        }])
    );
    assert_eq!(
        detach_variant_media_for_test(&mut proxy, product_id, duplicate_variant),
        json!([{
            "field": ["variantMedia", "0", "variantId"],
            "message": "Variant was specified in more than one media input.",
            "code": "PRODUCT_VARIANT_SPECIFIED_MULTIPLE_TIMES"
        }])
    );

    assert_eq!(
        append_variant_media_for_test(
            &mut proxy,
            product_id,
            json!([{ "variantId": variant_id, "mediaIds": [external_video_id] }]),
        ),
        json!([{
            "field": ["variantMedia", "0", "mediaIds"],
            "message": "Non-image media cannot be attached to variants.",
            "code": "INVALID_MEDIA_TYPE"
        }])
    );

    assert_eq!(
        detach_variant_media_for_test(
            &mut proxy,
            product_id,
            json!([{ "variantId": variant_id, "mediaIds": [ready_media_id] }]),
        ),
        json!([{
            "field": ["variantMedia", "0", "variantId"],
            "message": "The specified media is not attached to the specified variant.",
            "code": "MEDIA_IS_NOT_ATTACHED_TO_VARIANT"
        }])
    );

    assert_eq!(
        append_variant_media_for_test(
            &mut proxy,
            product_id,
            json!([{ "variantId": variant_id, "mediaIds": [ready_media_id] }]),
        ),
        json!([])
    );
    assert_eq!(
        append_variant_media_for_test(
            &mut proxy,
            product_id,
            json!([{ "variantId": variant_id, "mediaIds": [second_ready_media_id] }]),
        ),
        json!([{
            "field": ["variantMedia", "0", "variantId"],
            "message": "The given variant already has attached media.",
            "code": "PRODUCT_VARIANT_ALREADY_HAS_MEDIA"
        }])
    );
    assert_eq!(*forwarded.lock().unwrap(), 0);
}

#[test]
fn product_variant_media_empty_media_ids_return_blank_without_staging() {
    let product_id = "gid://shopify/Product/1";
    let mut proxy = configured_proxy(ReadMode::LiveHybrid, None)
        .with_base_products(vec![seed_product(product_id)])
        .with_upstream_transport(|_| panic!("empty mediaIds validation should not call upstream"));
    let variant = create_legacy_variant(&mut proxy, product_id, "EMPTY-MEDIA-IDS", "10.00");
    let variant_id = variant["id"].as_str().unwrap().to_string();
    let log_before_validation = log_snapshot(&proxy);

    let append = proxy.process_request(json_graphql_request(
        r#"
        mutation AppendEmptyMediaIds($productId: ID!, $variantMedia: [ProductVariantAppendMediaInput!]!) {
          productVariantAppendMedia(productId: $productId, variantMedia: $variantMedia) {
            productVariants { id }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "productId": product_id,
            "variantMedia": [{ "variantId": variant_id, "mediaIds": [] }]
        }),
    ));
    assert_eq!(append.status, 200);
    assert_eq!(
        append.body["data"]["productVariantAppendMedia"],
        json!({
            "productVariants": Value::Null,
            "userErrors": [{
                "field": ["variantMedia", "0", "mediaIds"],
                "message": "The mediaIds list cannot be empty.",
                "code": "BLANK"
            }]
        })
    );
    assert_eq!(
        log_snapshot(&proxy),
        log_before_validation,
        "append empty mediaIds should not stage a mutation log entry"
    );

    let detach = proxy.process_request(json_graphql_request(
        r#"
        mutation DetachEmptyMediaIds($productId: ID!, $variantMedia: [ProductVariantDetachMediaInput!]!) {
          productVariantDetachMedia(productId: $productId, variantMedia: $variantMedia) {
            productVariants { id }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "productId": product_id,
            "variantMedia": [{ "variantId": variant_id, "mediaIds": [] }]
        }),
    ));
    assert_eq!(detach.status, 200);
    assert_eq!(
        detach.body["data"]["productVariantDetachMedia"],
        json!({
            "productVariants": Value::Null,
            "userErrors": [{
                "field": ["variantMedia", "0", "mediaIds"],
                "message": "The mediaIds list cannot be empty.",
                "code": "BLANK"
            }]
        })
    );
    assert_eq!(
        log_snapshot(&proxy),
        log_before_validation,
        "detach empty mediaIds should not stage a mutation log entry"
    );

    let append_empty_variant_media = proxy.process_request(json_graphql_request(
        r#"
        mutation AppendEmptyVariantMedia($productId: ID!, $variantMedia: [ProductVariantAppendMediaInput!]!) {
          productVariantAppendMedia(productId: $productId, variantMedia: $variantMedia) {
            productVariants { id }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "productId": product_id,
            "variantMedia": []
        }),
    ));
    assert_eq!(append_empty_variant_media.status, 200);
    assert_eq!(
        append_empty_variant_media.body["data"]["productVariantAppendMedia"],
        json!({
            "productVariants": [],
            "userErrors": []
        })
    );

    let detach_empty_variant_media = proxy.process_request(json_graphql_request(
        r#"
        mutation DetachEmptyVariantMedia($productId: ID!, $variantMedia: [ProductVariantDetachMediaInput!]!) {
          productVariantDetachMedia(productId: $productId, variantMedia: $variantMedia) {
            productVariants { id }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "productId": product_id,
            "variantMedia": []
        }),
    ));
    assert_eq!(detach_empty_variant_media.status, 200);
    assert_eq!(
        detach_empty_variant_media.body["data"]["productVariantDetachMedia"],
        json!({
            "productVariants": [],
            "userErrors": []
        })
    );
}

#[test]
fn product_publication_full_sync_and_feedback_tail_helpers_port_old_gleam_tests() {
    let mut proxy = snapshot_proxy();

    let publication_validation = proxy.process_request(json_graphql_request(
        r#"
        mutation RustProductPublicationTargetValidation {
          omittedCatalog: publicationCreate(input: {}) {
            publication { id name autoPublish }
            userErrors { field message code }
          }
          missingCatalog: publicationCreate(input: { catalogId: "gid://shopify/MarketCatalog/999" }) {
            publication { id }
            userErrors { field message code }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(publication_validation.status, 200);
    assert_eq!(
        publication_validation.body["data"]["omittedCatalog"],
        json!({
            "publication": {
                "id": "gid://shopify/Publication/2",
                "name": "Publication 2",
                "autoPublish": false
            },
            "userErrors": []
        })
    );
    assert_eq!(
        publication_validation.body["data"]["missingCatalog"],
        json!({
            "publication": Value::Null,
            "userErrors": [{
                "field": ["input", "catalogId"],
                "message": "A catalog was not found for id= gid://shopify/MarketCatalog/999.",
                "code": "CATALOG_NOT_FOUND"
            }]
        })
    );

    let publication_update_delete = proxy.process_request(json_graphql_request(
        r#"
        mutation RustProductPublicationUpdateDeleteValidation {
          updateMissing: publicationUpdate(id: "gid://shopify/Publication/999", input: { autoPublish: true }) {
            publication { id }
            userErrors { field message code }
          }
          deleteMissing: publicationDelete(id: "gid://shopify/Publication/999") {
            deletedId
            userErrors { field message code }
          }
          deleteDefault: publicationDelete(id: "gid://shopify/Publication/1") {
            deletedId
            userErrors { field message code }
          }
          deleteCreated: publicationDelete(id: "gid://shopify/Publication/2") {
            deletedId
            publication { id name }
            userErrors { field message code }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(publication_update_delete.status, 200);
    assert_eq!(
        publication_update_delete.body["data"]["updateMissing"],
        json!({
            "publication": Value::Null,
            "userErrors": [{
                "field": ["id"],
                "message": "Publication was not found",
                "code": "PUBLICATION_NOT_FOUND"
            }]
        })
    );
    assert_eq!(
        publication_update_delete.body["data"]["deleteMissing"],
        json!({
            "deletedId": Value::Null,
            "userErrors": [{
                "field": ["id"],
                "message": "Publication was not found",
                "code": "PUBLICATION_NOT_FOUND"
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
    assert_eq!(
        publication_update_delete.body["data"]["deleteCreated"],
        json!({
            "deletedId": "gid://shopify/Publication/2",
            "userErrors": []
        })
    );

    let staged_non_feed = proxy.process_request(json_graphql_request(
        r#"
        mutation RustProductFullSyncStagedNonFeed($id: ID!) {
          productFullSync(id: $id) { id job { id } userErrors { field message code } }
        }
        "#,
        json!({ "id": "gid://shopify/Publication/2" }),
    ));
    assert_eq!(staged_non_feed.status, 200);
    assert_eq!(
        staged_non_feed.body["data"]["productFullSync"],
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

    let unknown_feed = proxy.process_request(json_graphql_request(
        r#"
        mutation RustProductFullSyncUnknown($id: ID!) {
          productFullSync(id: $id) { id job { id } userErrors { field message code } }
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
                    "job": Value::Null,
                    "userErrors": [{
                        "field": ["id"],
                        "message": "ProductFeed does not exist",
                        "code": "NOT_FOUND"
                    }]
                }
            }
        })
    );

    let job_selection = proxy.process_request(json_graphql_request(
        r#"
        mutation RustProductFullSyncJob($id: ID!) {
          productFullSync(id: $id) { id job { id } userErrors { field message code } }
        }
        "#,
        json!({ "id": "gid://shopify/ProductFeed/US-EN" }),
    ));
    assert_eq!(job_selection.status, 200);
    assert_eq!(
        job_selection.body["data"]["productFullSync"],
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

    let sync_before_create = proxy.process_request(json_graphql_request(
        r#"
        mutation RustProductFullSyncBeforeCreate($id: ID!) {
          productFullSync(id: $id) { id job { id } userErrors { field message code } }
        }
        "#,
        json!({ "id": "gid://shopify/ProductFeed/CA-FR" }),
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

    let non_us_feed_create = proxy.process_request(json_graphql_request(
        r#"
        mutation RustProductFeedCreateNonUsForFullSync($input: ProductFeedInput) {
          productFeedCreate(input: $input) { productFeed { id } userErrors { field message code } }
        }
        "#,
        json!({ "input": { "country": "CA", "language": "FR" } }),
    ));
    assert_eq!(non_us_feed_create.status, 200);
    assert_eq!(
        non_us_feed_create.body["data"]["productFeedCreate"],
        json!({
            "productFeed": { "id": "gid://shopify/ProductFeed/CA-FR" },
            "userErrors": []
        })
    );

    let sync = proxy.process_request(json_graphql_request(
        r#"
        mutation RustProductFullSync($id: ID!) {
          productFullSync(id: $id) {
            __typename
            id
            job { __typename id done query { __typename } }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "id": "gid://shopify/ProductFeed/CA-FR" }),
    ));
    assert_eq!(sync.status, 200);
    let sync_payload = &sync.body["data"]["productFullSync"];
    let job_id = sync_payload["job"]["id"].as_str().expect("sync job id");
    assert_eq!(job_id, "gid://shopify/Job/2");
    assert_eq!(
        sync_payload,
        &json!({
            "__typename": "ProductFullSyncPayload",
            "id": "gid://shopify/ProductFeed/CA-FR",
            "job": {
                "__typename": "Job",
                "id": job_id,
                "done": false,
                "query": { "__typename": "QueryRoot" }
            },
            "userErrors": []
        })
    );

    let invalid_range = proxy.process_request(json_graphql_request(
        r#"
        mutation RustProductFullSyncInvalidRange(
          $id: ID!,
          $updatedAtSince: DateTime,
          $beforeUpdatedAt: DateTime
        ) {
          productFullSync(
            id: $id,
            updatedAtSince: $updatedAtSince,
            beforeUpdatedAt: $beforeUpdatedAt
          ) { id job { id } userErrors { field message code } }
        }
        "#,
        json!({
            "id": "gid://shopify/ProductFeed/CA-FR",
            "updatedAtSince": "2024-02-01T00:00:00Z",
            "beforeUpdatedAt": "2024-01-01T00:00:00Z"
        }),
    ));
    assert_eq!(invalid_range.status, 200);
    assert_eq!(
        invalid_range.body["data"]["productFullSync"]["id"],
        Value::Null
    );
    assert_eq!(
        invalid_range.body["data"]["productFullSync"]["job"],
        Value::Null
    );
    assert_eq!(
        invalid_range.body["data"]["productFullSync"]["userErrors"][0]["field"],
        json!(["updatedAtSince"])
    );
    assert_eq!(
        invalid_range.body["data"]["productFullSync"]["userErrors"][0]["code"],
        Value::Null
    );
    assert!(
        invalid_range.body["data"]["productFullSync"]["userErrors"][0]["message"]
            .as_str()
            .is_some_and(|message| message.contains("updatedAtSince"))
    );

    let job = proxy.process_request(json_graphql_request(
        r#"
        query RustProductFullSyncJobPoll($id: ID!) {
          job(id: $id) { __typename id done query { __typename } }
        }
        "#,
        json!({ "id": job_id }),
    ));
    assert_eq!(job.status, 200);
    assert_eq!(
        job.body,
        json!({
            "data": {
                "job": {
                    "__typename": "Job",
                    "id": job_id,
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

    let log = log_snapshot(&proxy);
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
                && entry["stagedResourceIds"].as_array().is_some_and(|ids| {
                    ids.iter().any(|id| id == "gid://shopify/ProductFeed/CA-FR")
                        && ids.iter().any(|id| {
                            id.as_str().is_some_and(|id| {
                                id.starts_with("gid://shopify/ProductFullSyncOperation/")
                            })
                        })
                        && ids.iter().any(|id| id == "gid://shopify/Job/2")
                })),
        "successful full sync should stage the ProductFeed, operation, and Job IDs: {log}"
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
fn product_resource_feedback_reports_unavailable_products_as_product_not_found() {
    let deleted_product_id = "gid://shopify/Product/deleted-feedback-product";
    let archived_product_id = "gid://shopify/Product/archived-feedback-product";
    let mut archived_product = seed_product(archived_product_id);
    archived_product.status = "ARCHIVED".to_string();
    let mut proxy = snapshot_proxy()
        .with_base_products(vec![seed_product(deleted_product_id), archived_product]);

    let delete = proxy.process_request(json_graphql_request(
        r#"
        mutation DeleteProductBeforeFeedback($input: ProductDeleteInput!) {
          productDelete(input: $input) {
            deletedProductId
            userErrors { field message code }
          }
        }
        "#,
        json!({ "input": { "id": deleted_product_id } }),
    ));
    assert_eq!(delete.status, 200, "delete response: {}", delete.body);

    let feedback = proxy.process_request(json_graphql_request(
        r#"
        mutation ProductFeedbackUnavailableProducts($feedbackInput: [ProductResourceFeedbackInput!]!) {
          productFeedback: bulkProductResourceFeedbackCreate(feedbackInput: $feedbackInput) {
            feedback { productId }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "feedbackInput": [
                {
                    "productId": deleted_product_id,
                    "state": "REQUIRES_ACTION",
                    "feedbackGeneratedAt": "2024-01-01T00:00:00Z",
                    "productUpdatedAt": "2024-01-01T00:00:00Z",
                    "messages": ["needs review"]
                },
                {
                    "productId": archived_product_id,
                    "state": "REQUIRES_ACTION",
                    "feedbackGeneratedAt": "2024-01-01T00:00:00Z",
                    "productUpdatedAt": "2024-01-01T00:00:00Z",
                    "messages": ["needs review"]
                }
            ]
        }),
    ));

    assert_eq!(feedback.status, 200, "feedback response: {}", feedback.body);
    assert_eq!(
        feedback.body["data"]["productFeedback"],
        json!({
            "feedback": [],
            "userErrors": [
                {
                    "field": ["feedback", "0", "productId"],
                    "message": "Product does not exist",
                    "code": Value::Null
                },
                {
                    "field": ["feedback", "1", "productId"],
                    "message": "Product does not exist",
                    "code": Value::Null
                }
            ]
        })
    );
}

#[test]
fn publication_update_stages_publishables_and_validates_real_input_contract() {
    let product_id = "gid://shopify/Product/publication-update-product";
    let product = ProductRecord {
        id: product_id.to_string(),
        title: "Publication update product".to_string(),
        handle: "publication-update-product".to_string(),
        status: "ACTIVE".to_string(),
        ..ProductRecord::default()
    };
    let mut proxy = snapshot_proxy().with_base_products(vec![product]);
    let variant = create_legacy_variant(&mut proxy, product_id, "PUB-UPD", "1.00");
    let variant_id = variant["id"].as_str().unwrap().to_string();

    let create_publication = proxy.process_request(json_graphql_request(
        r#"
        mutation CreatePublicationUpdateTarget($input: PublicationCreateInput!) {
          publicationCreate(input: $input) {
            publication { id autoPublish }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "input": {} }),
    ));
    let publication_id = create_publication.body["data"]["publicationCreate"]["publication"]["id"]
        .as_str()
        .expect("publicationCreate should return an id")
        .to_string();

    let update_query = r#"
        mutation UpdatePublicationPublishables($id: ID!, $input: PublicationUpdateInput!) {
          publicationUpdate(id: $id, input: $input) {
            publication { id autoPublish }
            userErrors { field message code }
          }
        }
    "#;
    let add_product = proxy.process_request(json_graphql_request(
        update_query,
        json!({
            "id": publication_id,
            "input": { "publishablesToAdd": [product_id], "autoPublish": true }
        }),
    ));
    assert_eq!(
        add_product.body["data"]["publicationUpdate"],
        json!({
            "publication": {
                "id": "gid://shopify/Publication/2",
                "autoPublish": true
            },
            "userErrors": []
        })
    );

    let product_read = proxy.process_request(json_graphql_request(
        r#"
        query ReadPublicationUpdateProduct($id: ID!, $publicationId: ID!) {
          product(id: $id) {
            id
            publishedOnPublication(publicationId: $publicationId)
            resourcePublicationsCount { count precision }
          }
        }
        "#,
        json!({ "id": product_id, "publicationId": "gid://shopify/Publication/2" }),
    ));
    assert_eq!(
        product_read.body["data"]["product"],
        json!({
            "id": product_id,
            "publishedOnPublication": true,
            "resourcePublicationsCount": { "count": 1, "precision": "EXACT" }
        })
    );

    let remove_product = proxy.process_request(json_graphql_request(
        update_query,
        json!({
            "id": "gid://shopify/Publication/2",
            "input": { "publishablesToRemove": [product_id] }
        }),
    ));
    assert_eq!(
        remove_product.body["data"]["publicationUpdate"],
        json!({
            "publication": {
                "id": "gid://shopify/Publication/2",
                "autoPublish": true
            },
            "userErrors": []
        })
    );
    let product_after_remove = proxy.process_request(json_graphql_request(
        r#"
        query ReadPublicationUpdateProductAfterRemove($id: ID!, $publicationId: ID!) {
          product(id: $id) {
            publishedOnPublication(publicationId: $publicationId)
            resourcePublicationsCount { count precision }
          }
        }
        "#,
        json!({ "id": product_id, "publicationId": "gid://shopify/Publication/2" }),
    ));
    assert_eq!(
        product_after_remove.body["data"]["product"],
        json!({
            "publishedOnPublication": false,
            "resourcePublicationsCount": { "count": 0, "precision": "EXACT" }
        })
    );

    let variant_only = proxy.process_request(json_graphql_request(
        update_query,
        json!({
            "id": "gid://shopify/Publication/2",
            "input": { "publishablesToAdd": [variant_id] }
        }),
    ));
    assert_eq!(variant_only.body["data"]["publicationUpdate"], Value::Null);
    assert_eq!(
        variant_only.body["errors"][0]["message"],
        json!(format!("Invalid id: {variant_id}"))
    );
    assert_eq!(
        variant_only.body["errors"][0]["extensions"]["code"],
        json!("RESOURCE_NOT_FOUND")
    );
    assert_eq!(
        variant_only.body["errors"][0]["path"],
        json!(["publicationUpdate"])
    );

    let mixed = proxy.process_request(json_graphql_request(
        update_query,
        json!({
            "id": "gid://shopify/Publication/2",
            "input": { "publishablesToAdd": [product_id, variant_id] }
        }),
    ));
    assert_eq!(mixed.body["data"]["publicationUpdate"], Value::Null);
    assert_eq!(
        mixed.body["errors"][0]["message"],
        json!(format!("Invalid id: {variant_id}"))
    );
    assert_eq!(
        mixed.body["errors"][0]["extensions"]["code"],
        json!("RESOURCE_NOT_FOUND")
    );
    assert_eq!(
        mixed.body["errors"][0]["path"],
        json!(["publicationUpdate"])
    );

    let invalid = proxy.process_request(json_graphql_request(
        update_query,
        json!({
            "id": "gid://shopify/Publication/2",
            "input": { "publishablesToAdd": ["gid://shopify/Product/missing"] }
        }),
    ));
    assert_eq!(
        invalid.body["data"]["publicationUpdate"]["userErrors"],
        json!([{
            "field": ["input", "publishablesToAdd", "0"],
            "message": "Publishable ID not found.",
            "code": "INVALID_PUBLISHABLE_ID"
        }])
    );

    let too_many_publishables = (0..51)
        .map(|index| format!("gid://shopify/Product/limit-{index}"))
        .collect::<Vec<_>>();
    let too_many = proxy.process_request(json_graphql_request(
        update_query,
        json!({
            "id": "gid://shopify/Publication/2",
            "input": { "publishablesToAdd": too_many_publishables }
        }),
    ));
    assert_eq!(
        too_many.body["data"]["publicationUpdate"]["userErrors"],
        json!([{
            "field": ["input", "publishablesToAdd", "51"],
            "message": "The limit for simultaneous publication updates has been exceeded.",
            "code": "PUBLICATION_UPDATE_LIMIT_EXCEEDED"
        }])
    );
}

#[test]
fn product_publication_and_feedback_enum_coercion_errors_do_not_stage_or_log() {
    let mut proxy = snapshot_proxy();
    let undefined_publication_fields = proxy.process_request(json_graphql_request(
        r#"
        mutation RustProductPublicationUnknownFields($input: PublicationCreateInput!) {
          publicationCreate(input: $input) { publication { id } userErrors { field message code } }
        }
        "#,
        json!({ "input": { "name": "Nope", "channelId": "gid://shopify/Channel/999" } }),
    ));
    assert_eq!(undefined_publication_fields.status, 200);
    assert_eq!(undefined_publication_fields.body.get("data"), None);
    assert_eq!(
        undefined_publication_fields.body["errors"][0]["extensions"]["code"],
        json!("INVALID_VARIABLE")
    );
    assert!(undefined_publication_fields.body["errors"][0]["message"]
        .as_str()
        .is_some_and(|message| message.contains("Field is not defined on PublicationCreateInput")));

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
    assert_eq!(log_snapshot(&proxy), json!({ "entries": [] }));
}

#[test]
fn product_reorder_media_without_store_backed_handler_fails_closed() {
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
    assert_eq!(mutation.status, 400);
    assert_eq!(
        mutation.body,
        json!({ "errors": [{ "message": "No mutation dispatcher implemented for root field: productReorderMedia" }] })
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
        read.body["data"]["product"],
        Value::Null,
        "unobserved product reads should not replay a baked reorder-media fixture"
    );
    assert_eq!(log_snapshot(&proxy), json!({ "entries": [] }));
}

#[test]
fn product_create_and_delete_media_without_store_backed_handlers_fail_closed() {
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
    assert_eq!(create.status, 400);
    assert_eq!(
        create.body,
        json!({ "errors": [{ "message": "No mutation dispatcher implemented for root field: productCreateMedia" }] })
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
        create_read.body["data"]["product"],
        Value::Null,
        "unobserved product reads should not replay a baked create-media fixture"
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
    assert_eq!(delete.status, 400);
    assert_eq!(
        delete.body,
        json!({ "errors": [{ "message": "No mutation dispatcher implemented for root field: productDeleteMedia" }] })
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
        Value::Null,
        "unobserved product reads should not replay a baked delete-media fixture"
    );
    assert_eq!(log_snapshot(&proxy), json!({ "entries": [] }));
}

#[test]
fn product_update_media_without_store_backed_handler_fails_closed() {
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
    assert_eq!(mutation.status, 400);
    assert_eq!(
        mutation.body,
        json!({ "errors": [{ "message": "No mutation dispatcher implemented for root field: productUpdateMedia" }] })
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
        Value::Null,
        "unobserved product reads should not replay a baked update-media fixture"
    );
    assert_eq!(log_snapshot(&proxy), json!({ "entries": [] }));
}

#[test]
fn product_publication_aggregate_unobserved_read_returns_no_data() {
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
        Value::Null,
        "unobserved product reads should not replay a baked publication aggregate fixture"
    );
}

#[test]
fn product_publish_unpublish_stage_publication_state_for_downstream_reads() {
    let product_id = "gid://shopify/Product/publication-active";
    let mut product = ProductRecord {
        id: product_id.to_string(),
        title: "Publication active product".to_string(),
        handle: "publication-active-product".to_string(),
        status: "ACTIVE".to_string(),
        ..ProductRecord::default()
    };
    product
        .extra_fields
        .insert("productPublications".to_string(), json!([]));
    let mut proxy = snapshot_proxy().with_base_products(vec![product]);

    let create_publication = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateProductPublicationTarget($input: PublicationCreateInput!) {
          publicationCreate(input: $input) { publication { id } userErrors { field message } }
        }
        "#,
        json!({ "input": {} }),
    ));
    let publication_id = create_publication.body["data"]["publicationCreate"]["publication"]["id"]
        .as_str()
        .expect("publicationCreate should return an id")
        .to_string();

    let publish_query = r#"
        mutation ProductPublishReadAfterWrite($input: ProductPublishInput!, $publicationId: ID!) {
          productPublish(input: $input) {
            product {
              id
              publishedAt
              publishedOnCurrentPublication
              publishedOnPublication(publicationId: $publicationId)
              availablePublicationsCount { count precision }
              resourcePublicationsCount { count precision }
              resourcePublicationsV2(first: 10) {
                nodes { publication { id } isPublished publishDate publishable { ... on Product { id } } }
              }
              publications(first: 10) {
                nodes { isPublished publishDate product { id } }
              }
            }
            userErrors { field message }
          }
        }
    "#;
    let publish_variables = json!({
        "input": {
            "id": product_id,
            "productPublications": [{
                "publicationId": publication_id,
                "publishDate": "2026-01-02T03:04:05Z"
            }]
        },
        "publicationId": publication_id
    });
    let publish = proxy.process_request(json_graphql_request(publish_query, publish_variables));
    let published_product = &publish.body["data"]["productPublish"]["product"];
    assert_eq!(
        published_product,
        &json!({
            "id": product_id,
            "publishedAt": "2026-01-02T03:04:05Z",
            "publishedOnCurrentPublication": false,
            "publishedOnPublication": true,
            "availablePublicationsCount": { "count": 1, "precision": "EXACT" },
            "resourcePublicationsCount": { "count": 1, "precision": "EXACT" },
            "resourcePublicationsV2": {
                "nodes": [{
                    "publication": { "id": publication_id },
                    "isPublished": true,
                    "publishDate": "2026-01-02T03:04:05Z",
                    "publishable": { "id": product_id }
                }]
            },
            "publications": {
                "nodes": [{
                    "isPublished": true,
                    "publishDate": "2026-01-02T03:04:05Z",
                    "product": { "id": product_id }
                }]
            }
        })
    );
    assert_eq!(
        publish.body["data"]["productPublish"]["userErrors"],
        json!([])
    );

    let read_query = r#"
        query ProductPublicationReadAfterWrite($id: ID!, $publicationId: ID!) {
          product(id: $id) {
            id
            publishedAt
            publishedOnPublication(publicationId: $publicationId)
            availablePublicationsCount { count precision }
            resourcePublicationsCount { count precision }
            resourcePublicationsV2(first: 10) { nodes { publication { id } isPublished publishDate } }
          }
        }
    "#;
    let read_after_publish = proxy.process_request(json_graphql_request(
        read_query,
        json!({ "id": product_id, "publicationId": publication_id }),
    ));
    assert_eq!(
        read_after_publish.body["data"]["product"],
        json!({
            "id": product_id,
            "publishedAt": "2026-01-02T03:04:05Z",
            "publishedOnPublication": true,
            "availablePublicationsCount": { "count": 1, "precision": "EXACT" },
            "resourcePublicationsCount": { "count": 1, "precision": "EXACT" },
            "resourcePublicationsV2": {
                "nodes": [{
                    "publication": { "id": publication_id },
                    "isPublished": true,
                    "publishDate": "2026-01-02T03:04:05Z"
                }]
            }
        })
    );

    let unpublish = proxy.process_request(json_graphql_request(
        r#"
        mutation ProductUnpublishReadAfterWrite($input: ProductUnpublishInput!, $publicationId: ID!) {
          productUnpublish(input: $input) {
            product {
              id
              publishedAt
              publishedOnPublication(publicationId: $publicationId)
              availablePublicationsCount { count precision }
              resourcePublicationsCount { count precision }
              resourcePublicationsV2(first: 10) { nodes { publication { id } isPublished publishDate } }
            }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "input": {
                "id": product_id,
                "productPublications": [{ "publicationId": publication_id }]
            },
            "publicationId": publication_id
        }),
    ));
    assert_eq!(
        unpublish.body["data"]["productUnpublish"],
        json!({
            "product": {
                "id": product_id,
                "publishedAt": null,
                "publishedOnPublication": false,
                "availablePublicationsCount": { "count": 0, "precision": "EXACT" },
                "resourcePublicationsCount": { "count": 0, "precision": "EXACT" },
                "resourcePublicationsV2": { "nodes": [] }
            },
            "userErrors": []
        })
    );

    let read_after_unpublish = proxy.process_request(json_graphql_request(
        read_query,
        json!({ "id": product_id, "publicationId": publication_id }),
    ));
    assert_eq!(
        read_after_unpublish.body["data"]["product"],
        json!({
            "id": product_id,
            "publishedAt": null,
            "publishedOnPublication": false,
            "availablePublicationsCount": { "count": 0, "precision": "EXACT" },
            "resourcePublicationsCount": { "count": 0, "precision": "EXACT" },
            "resourcePublicationsV2": { "nodes": [] }
        })
    );

    let log = proxy.process_request(Request {
        method: "GET".to_string(),
        path: "/__meta/log".to_string(),
        headers: Default::default(),
        body: String::new(),
    });
    assert!(
        log.body["entries"].as_array().is_some_and(|entries| {
            entries.iter().any(|entry| {
                entry["interpreted"]["primaryRootField"] == json!("productPublish")
                    && entry["rawBody"]
                        .as_str()
                        .is_some_and(|body| body.contains("ProductPublishReadAfterWrite"))
            }) && entries.iter().any(|entry| {
                entry["interpreted"]["primaryRootField"] == json!("productUnpublish")
                    && entry["rawBody"]
                        .as_str()
                        .is_some_and(|body| body.contains("ProductUnpublishReadAfterWrite"))
            })
        }),
        "productPublish/productUnpublish should be staged with replay-ready raw bodies: {}",
        log.body
    );
}

#[test]
fn product_publish_unpublish_validate_publication_state_locally() {
    let product_id = "gid://shopify/Product/publication-validation";
    let mut product = ProductRecord {
        id: product_id.to_string(),
        title: "Publication validation product".to_string(),
        handle: "publication-validation-product".to_string(),
        status: "ACTIVE".to_string(),
        ..ProductRecord::default()
    };
    product
        .extra_fields
        .insert("productPublications".to_string(), json!([]));
    let mut proxy = snapshot_proxy().with_base_products(vec![product]);
    let create_publication = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateProductPublicationValidationTarget($input: PublicationCreateInput!) {
          publicationCreate(input: $input) { publication { id } userErrors { field message } }
        }
        "#,
        json!({ "input": {} }),
    ));
    let publication_id = create_publication.body["data"]["publicationCreate"]["publication"]["id"]
        .as_str()
        .expect("publicationCreate should return an id")
        .to_string();
    let publish = r#"
        mutation ProductPublishValidation($input: ProductPublishInput!) {
          productPublish(input: $input) { product { id } userErrors { field message } }
        }
    "#;
    let unpublish = r#"
        mutation ProductUnpublishValidation($input: ProductUnpublishInput!) {
          productUnpublish(input: $input) { product { id } userErrors { field message } }
        }
    "#;

    let unknown = proxy.process_request(json_graphql_request(
        publish,
        json!({ "input": { "id": product_id, "productPublications": [{ "publicationId": "gid://shopify/Publication/999999999999" }] } }),
    ));
    assert_eq!(
        unknown.body["data"]["productPublish"]["userErrors"],
        json!([{
            "field": ["productPublications", "0", "publicationId"],
            "message": "Publication does not exist or is not publishable"
        }])
    );

    let first_publish = proxy.process_request(json_graphql_request(
        publish,
        json!({ "input": { "id": product_id, "productPublications": [{ "publicationId": publication_id }] } }),
    ));
    assert_eq!(
        first_publish.body["data"]["productPublish"]["userErrors"],
        json!([])
    );

    let duplicate_publish = proxy.process_request(json_graphql_request(
        publish,
        json!({ "input": { "id": product_id, "productPublications": [{ "publicationId": publication_id }] } }),
    ));
    assert_eq!(
        duplicate_publish.body["data"]["productPublish"]["userErrors"],
        json!([{
            "field": ["productPublications", "0", "publicationId"],
            "message": "Product is already published on this publication"
        }])
    );

    let first_unpublish = proxy.process_request(json_graphql_request(
        unpublish,
        json!({ "input": { "id": product_id, "productPublications": [{ "publicationId": publication_id }] } }),
    ));
    assert_eq!(
        first_unpublish.body["data"]["productUnpublish"]["userErrors"],
        json!([])
    );

    let duplicate_unpublish = proxy.process_request(json_graphql_request(
        unpublish,
        json!({ "input": { "id": product_id, "productPublications": [{ "publicationId": publication_id }] } }),
    ));
    assert_eq!(
        duplicate_unpublish.body["data"]["productUnpublish"]["userErrors"],
        json!([{
            "field": ["productPublications", "0", "publicationId"],
            "message": "Product is not published on this publication"
        }])
    );
}

#[test]
fn product_publish_live_hybrid_stages_seeded_product_without_upstream_write() {
    let product_id = "gid://shopify/Product/publication-local-only";
    let mut product = ProductRecord {
        id: product_id.to_string(),
        title: "Publication local only".to_string(),
        handle: "publication-local-only".to_string(),
        status: "ACTIVE".to_string(),
        ..ProductRecord::default()
    };
    product
        .extra_fields
        .insert("productPublications".to_string(), json!([]));
    let forwarded = Arc::new(Mutex::new(Vec::<Request>::new()));
    let captured = Arc::clone(&forwarded);
    let mut proxy = configured_proxy(ReadMode::LiveHybrid, None)
        .with_base_products(vec![product])
        .with_upstream_transport(move |request| {
            captured.lock().unwrap().push(request);
            Response {
                status: 500,
                headers: Default::default(),
                body: json!({ "errors": [{ "message": "unexpected upstream write" }] }),
            }
        });
    let create_publication = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateProductPublicationNoWriteTarget($input: PublicationCreateInput!) {
          publicationCreate(input: $input) { publication { id } userErrors { field message } }
        }
        "#,
        json!({ "input": {} }),
    ));
    let publication_id = create_publication.body["data"]["publicationCreate"]["publication"]["id"]
        .as_str()
        .expect("publicationCreate should return an id");

    let publish = proxy.process_request(json_graphql_request(
        r#"
        mutation ProductPublishNoRuntimeWrite($input: ProductPublishInput!) {
          productPublish(input: $input) { product { id publishedOnPublication(publicationId: "gid://shopify/Publication/2") } userErrors { field message } }
        }
        "#,
        json!({ "input": { "id": product_id, "productPublications": [{ "publicationId": publication_id }] } }),
    ));
    assert_eq!(publish.status, 200);
    assert_eq!(
        publish.body["data"]["productPublish"]["userErrors"],
        json!([])
    );
    assert_eq!(forwarded.lock().unwrap().len(), 0);
}

#[test]
fn product_publishable_mutations_return_captured_aggregate_shape() {
    let product_id = "gid://shopify/Product/publishable-state";
    let mut proxy = snapshot_proxy().with_base_products(vec![seed_product(product_id)]);
    let dump = proxy.process_request(request_with_body("POST", "/__meta/dump", "{}"));
    assert_eq!(dump.status, 200);
    let mut restored = dump.body;
    restored["state"]["baseState"]["shop"] = json!({
        "id": "gid://shopify/Shop/test-store",
        "name": "Seeded Test Store",
        "myshopifyDomain": "seeded-test-store.myshopify.com",
        "currencyCode": "USD"
    });
    restored["state"]["baseState"]["publicationIds"] = json!([
        "gid://shopify/Publication/base-a",
        "gid://shopify/Publication/base-b",
        "gid://shopify/Publication/base-c"
    ]);
    restored["state"]["baseState"]["publicationCount"] = json!(3);
    let restore = proxy.process_request(request_with_body(
        "POST",
        "/__meta/restore",
        &restored.to_string(),
    ));
    assert_eq!(restore.status, 200);

    for (root, query, expected_current, expected_count) in [
        (
            "publishablePublish",
            r#"
            mutation PublishablePublishProductParity($id: ID!, $input: [PublicationInput!]!) {
              publishablePublish(id: $id, input: $input) {
                publishable { ... on Product { id publishedOnCurrentPublication availablePublicationsCount { count precision } resourcePublicationsCount { count precision } } }
                shop { id name publicationCount }
                userErrors { field message }
              }
            }
            "#,
            false,
            1,
        ),
        (
            "publishableUnpublish",
            r#"
            mutation PublishableUnpublishProductParity($id: ID!, $input: [PublicationInput!]!) {
              publishableUnpublish(id: $id, input: $input) {
                publishable { ... on Product { id publishedOnCurrentPublication availablePublicationsCount { count precision } resourcePublicationsCount { count precision } } }
                shop { id name publicationCount }
                userErrors { field message }
              }
            }
            "#,
            false,
            0,
        ),
        (
            "publishablePublishToCurrentChannel",
            r#"
            mutation PublishablePublishToCurrentChannelProductParity($id: ID!) {
              publishablePublishToCurrentChannel(id: $id) {
                publishable { ... on Product { id publishedOnCurrentPublication availablePublicationsCount { count precision } resourcePublicationsCount { count precision } } }
                shop { id name publicationCount }
                userErrors { field message }
              }
            }
            "#,
            true,
            1,
        ),
        (
            "publishableUnpublishToCurrentChannel",
            r#"
            mutation PublishableUnpublishToCurrentChannelProductParity($id: ID!) {
              publishableUnpublishToCurrentChannel(id: $id) {
                publishable { ... on Product { id publishedOnCurrentPublication availablePublicationsCount { count precision } resourcePublicationsCount { count precision } } }
                shop { id name publicationCount }
                userErrors { field message }
              }
            }
            "#,
            false,
            0,
        ),
    ] {
        let response = proxy.process_request(json_graphql_request(
            query,
            json!({
                "id": product_id,
                "input": [{ "publicationId": "gid://shopify/Publication/base-a" }]
            }),
        ));
        assert_eq!(
            response.body["data"][root]["publishable"],
            json!({
                "id": product_id,
                "publishedOnCurrentPublication": expected_current,
                "availablePublicationsCount": { "count": expected_count, "precision": "EXACT" },
                "resourcePublicationsCount": { "count": expected_count, "precision": "EXACT" }
            })
        );
        assert_eq!(
            response.body["data"][root]["shop"],
            json!({
                "id": "gid://shopify/Shop/test-store",
                "name": "Seeded Test Store",
                "publicationCount": 3
            })
        );
        assert_eq!(response.body["data"][root]["userErrors"], json!([]));
    }

    let create_publication = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateLocalPublication($input: PublicationCreateInput!) {
          publicationCreate(input: $input) {
            publication { id }
            userErrors { field message }
          }
        }
        "#,
        json!({ "input": {} }),
    ));
    assert_eq!(
        create_publication.body["data"]["publicationCreate"],
        json!({
            "publication": { "id": "gid://shopify/Publication/2" },
            "userErrors": []
        })
    );

    let state = proxy.process_request(Request {
        method: "GET".to_string(),
        path: "/__meta/state".to_string(),
        headers: Default::default(),
        body: String::new(),
    });
    assert_eq!(
        state.body["stagedState"]["publicationIds"],
        json!(["gid://shopify/Publication/2"])
    );
    assert_eq!(
        state.body["stagedState"]["createdPublicationIds"],
        json!(["gid://shopify/Publication/2"])
    );

    let staged_count = proxy.process_request(json_graphql_request(
        r#"
        mutation PublishablePublishProductStagedShopCount($id: ID!, $input: [PublicationInput!]!) {
          publishablePublish(id: $id, input: $input) {
            publishable { ... on Product { id } }
            shop { id publicationCount }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "id": product_id,
            "input": [{ "publicationId": "gid://shopify/Publication/2" }]
        }),
    ));
    assert_eq!(
        staged_count.body["data"]["publishablePublish"]["shop"],
        json!({
            "id": "gid://shopify/Shop/test-store",
            "publicationCount": 4
        })
    );
    assert_eq!(
        staged_count.body["data"]["publishablePublish"]["userErrors"],
        json!([])
    );
}

#[test]
fn publishable_collection_payload_uses_staged_collection_title_handle_and_counts() {
    let mut proxy = snapshot_proxy();
    let dump = proxy.process_request(request_with_body("POST", "/__meta/dump", "{}"));
    assert_eq!(dump.status, 200);
    let mut restored = dump.body;
    restored["state"]["baseState"]["publicationIds"] = json!(["gid://shopify/Publication/base-a"]);
    restored["state"]["baseState"]["publicationCount"] = json!(1);
    let restore = proxy.process_request(request_with_body(
        "POST",
        "/__meta/restore",
        &restored.to_string(),
    ));
    assert_eq!(restore.status, 200);

    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation CreatePublishableCollection($input: CollectionInput!) {
          collectionCreate(input: $input) {
            collection { id title handle }
            userErrors { field message }
          }
        }
        "#,
        json!({ "input": { "title": "State Backed Collection", "handle": "state-backed-collection" } }),
    ));
    assert_eq!(
        create.body["data"]["collectionCreate"]["userErrors"],
        json!([])
    );
    let collection = &create.body["data"]["collectionCreate"]["collection"];
    let collection_id = collection["id"].as_str().unwrap().to_string();

    let publish = proxy.process_request(json_graphql_request(
        r#"
        mutation PublishCollection($id: ID!, $input: [PublicationInput!]!, $publicationId: ID!) {
          publishablePublish(id: $id, input: $input) {
            publishable {
              ... on Collection {
                id
                title
                handle
                publishedOnPublication(publicationId: $publicationId)
                availablePublicationsCount { count precision }
                resourcePublicationsCount { count precision }
              }
            }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "id": collection_id,
            "publicationId": "gid://shopify/Publication/base-a",
            "input": [{ "publicationId": "gid://shopify/Publication/base-a" }]
        }),
    ));
    assert_eq!(
        publish.body["data"]["publishablePublish"],
        json!({
            "publishable": {
                "id": collection_id,
                "title": "State Backed Collection",
                "handle": "state-backed-collection",
                "publishedOnPublication": true,
                "availablePublicationsCount": { "count": 1, "precision": "EXACT" },
                "resourcePublicationsCount": { "count": 1, "precision": "EXACT" }
            },
            "userErrors": []
        })
    );
}

#[test]
fn publishable_payload_shop_hydrates_from_upstream_when_selected() {
    let forwarded = Arc::new(Mutex::new(Vec::<Request>::new()));
    let captured = Arc::clone(&forwarded);
    let mut proxy =
        configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport(move |request| {
            captured.lock().unwrap().push(request.clone());
            let body: Value =
                serde_json::from_str(&request.body).expect("upstream GraphQL body parses");
            if body["query"]
                .as_str()
                .is_some_and(|query| query.contains("PublicationResourceHydrate"))
            {
                assert_eq!(
                    body["variables"],
                    json!({ "ids": ["gid://shopify/Product/10172067414322"] })
                );
                return shopify_draft_proxy::proxy::Response {
                    status: 200,
                    headers: Default::default(),
                    body: json!({
                        "data": {
                            "nodes": [{
                                "id": "gid://shopify/Product/10172067414322",
                                "title": "Hydrated publishable product",
                                "handle": "hydrated-publishable-product",
                                "status": "ACTIVE",
                                "resourcePublications": { "nodes": [] }
                            }]
                        }
                    }),
                };
            }
            assert!(
                body["query"].as_str().is_some_and(
                    |query| query.contains("StorePropertiesPublishableInputValidationHydrate")
                ),
                "unexpected upstream query: {}",
                body["query"]
            );
            assert_eq!(
                body["variables"],
                json!({ "id": "gid://shopify/Product/10172067414322" })
            );
            shopify_draft_proxy::proxy::Response {
                status: 200,
                headers: Default::default(),
                body: json!({
                    "data": {
                        "publishable": {
                            "id": "gid://shopify/Product/10172067414322",
                            "publishedOnCurrentPublication": false,
                            "availablePublicationsCount": { "count": 0, "precision": "EXACT" },
                            "resourcePublicationsCount": { "count": 0, "precision": "EXACT" }
                        },
                        "shop": {
                            "id": "gid://shopify/Shop/upstream",
                            "name": "Upstream Shop",
                            "myshopifyDomain": "upstream-shop.myshopify.com",
                            "primaryDomain": { "host": "policies.upstream.example" },
                            "currencyCode": "CAD",
                            "publicationCount": 5,
                            "shopPolicies": [
                                {
                                    "id": "gid://shopify/ShopPolicy/2002",
                                    "title": "Privacy Policy",
                                    "body": "<p>Old upstream privacy</p>",
                                    "type": "PRIVACY_POLICY",
                                    "url": "https://upstream-shop.myshopify.com/policies/2002.html?locale=en",
                                    "createdAt": "2026-02-03T04:05:06Z",
                                    "updatedAt": "2026-02-03T04:05:06Z"
                                }
                            ]
                        },
                        "publications": {
                            "nodes": [
                                { "id": "gid://shopify/Publication/one" },
                                { "id": "gid://shopify/Publication/two" },
                                { "id": "gid://shopify/Publication/three" },
                                { "id": "gid://shopify/Publication/four" },
                                { "id": "gid://shopify/Publication/five" }
                            ]
                        }
                    }
                }),
            }
        });

    let response = proxy.process_request(json_graphql_request(
        r#"
        mutation PublishablePayloadShopHydrate($id: ID!, $input: [PublicationInput!]!) {
          publishablePublish(id: $id, input: $input) {
            publishable { ... on Product { id } }
            shop { id name myshopifyDomain currencyCode publicationCount }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "id": "gid://shopify/Product/10172067414322",
            "input": [{ "publicationId": "gid://shopify/Publication/one" }]
        }),
    ));

    assert_eq!(response.status, 200);
    assert_eq!(
        response.body["data"]["publishablePublish"]["shop"],
        json!({
            "id": "gid://shopify/Shop/upstream",
            "name": "Upstream Shop",
            "myshopifyDomain": "upstream-shop.myshopify.com",
            "currencyCode": "CAD",
            "publicationCount": 5
        })
    );
    assert_eq!(
        response.body["data"]["publishablePublish"]["userErrors"],
        json!([])
    );

    let state = proxy.process_request(Request {
        method: "GET".to_string(),
        path: "/__meta/state".to_string(),
        headers: Default::default(),
        body: String::new(),
    });
    assert_eq!(
        state.body["baseState"]["shop"]["id"],
        json!("gid://shopify/Shop/upstream")
    );
    assert_eq!(state.body["baseState"]["publicationCount"], json!(5));
    assert_eq!(
        state.body["baseState"]["publicationIds"],
        json!([
            "gid://shopify/Publication/five",
            "gid://shopify/Publication/four",
            "gid://shopify/Publication/one",
            "gid://shopify/Publication/three",
            "gid://shopify/Publication/two"
        ])
    );
    assert_eq!(
        state.body["baseState"]["shopPolicyOrder"],
        json!(["gid://shopify/ShopPolicy/2002"])
    );

    let policy_update = proxy.process_request(json_graphql_request(
        r#"
        mutation PublishableHydratedPolicyUpdate($shopPolicy: ShopPolicyInput!) {
          shopPolicyUpdate(shopPolicy: $shopPolicy) {
            shopPolicy { id title body url createdAt }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "shopPolicy": { "type": "PRIVACY_POLICY", "body": "<p>Updated privacy</p>" } }),
    ));
    assert_eq!(policy_update.status, 200);
    assert_eq!(
        policy_update.body["data"]["shopPolicyUpdate"]["shopPolicy"],
        json!({
            "id": "gid://shopify/ShopPolicy/2002",
            "title": "Privacy Policy",
            "body": "<p>Updated privacy</p>",
            "url": "https://policies.upstream.example/policies/2002.html?locale=en",
            "createdAt": "2026-02-03T04:05:06Z"
        })
    );
    assert_eq!(
        policy_update.body["data"]["shopPolicyUpdate"]["userErrors"],
        json!([])
    );

    let policy_read = proxy.process_request(json_graphql_request(
        r#"
        query PublishableHydratedPolicyRead {
          shop { shopPolicies { id type title body url } }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        policy_read.body["data"]["shop"]["shopPolicies"],
        json!([
            {
                "id": "gid://shopify/ShopPolicy/2002",
                "type": "PRIVACY_POLICY",
                "title": "Privacy Policy",
                "body": "<p>Updated privacy</p>",
                "url": "https://policies.upstream.example/policies/2002.html?locale=en"
            }
        ])
    );

    assert_eq!(forwarded.lock().unwrap().len(), 2);
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
fn product_create_payload_shop_uses_restored_shop_state_for_success_and_user_errors() {
    let mut proxy = snapshot_proxy();
    let expected_shop = restore_product_payload_shop(&mut proxy);

    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation ProductCreatePayloadShop($product: ProductCreateInput!) {
          productCreate(product: $product) {
            product { id title }
            shop { id name myshopifyDomain }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "product": { "title": "Payload shop product" } }),
    ));
    assert_eq!(create.status, 200);
    assert_eq!(
        create.body["data"]["productCreate"]["product"]["title"],
        json!("Payload shop product")
    );
    assert_eq!(create.body["data"]["productCreate"]["shop"], expected_shop);
    assert_eq!(
        create.body["data"]["productCreate"]["userErrors"],
        json!([])
    );

    let user_error = proxy.process_request(json_graphql_request(
        r#"
        mutation ProductCreatePayloadShopUserError($product: ProductCreateInput!) {
          productCreate(product: $product) {
            product { id }
            shop { id name myshopifyDomain }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "product": { "title": " " } }),
    ));
    assert_eq!(user_error.status, 200);
    assert_eq!(
        user_error.body["data"]["productCreate"],
        json!({
            "product": null,
            "shop": selected_product_payload_shop(),
            "userErrors": [{
                "field": ["title"],
                "message": "Title can't be blank",
                "code": "BLANK"
            }]
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
fn products_connection_and_count_filter_common_search_fields_from_store_state() {
    let mut alpha = seed_product("gid://shopify/Product/alpha");
    alpha.title = "Alpha status: ACTIVE Jacket".to_string();
    alpha.handle = "alpha-jacket".to_string();
    alpha.vendor = "Northwind".to_string();
    alpha.product_type = "Jackets".to_string();
    alpha.tags = vec!["featured".to_string(), "outerwear".to_string()];
    alpha
        .extra_fields
        .insert("publishedAt".to_string(), json!("2024-01-02T00:00:00.000Z"));

    let mut beta = seed_product("gid://shopify/Product/beta");
    beta.title = "Beta Jacket".to_string();
    beta.handle = "beta-jacket".to_string();
    beta.status = "DRAFT".to_string();
    beta.vendor = "Southwind".to_string();
    beta.product_type = "Jackets".to_string();
    beta.tags = vec!["clearance".to_string()];

    let mut gamma = seed_product("gid://shopify/Product/gamma");
    gamma.title = "Gamma Shirt".to_string();
    gamma.handle = "gamma-shirt".to_string();
    gamma.vendor = "Northwind".to_string();
    gamma.product_type = "Shirts".to_string();
    gamma.tags = vec!["featured".to_string()];

    let mut proxy = snapshot_proxy().with_base_products(vec![alpha, beta, gamma]);

    let variant = create_legacy_variant(
        &mut proxy,
        "gid://shopify/Product/alpha",
        "ALPHA-FILTER-SKU",
        "10.00",
    );
    assert!(variant["id"].as_str().is_some());

    let read = proxy.process_request(json_graphql_request(
        r#"
        query ProductCommonSearchFilters($status: String!, $vendorType: String!, $title: String!, $tag: String!, $sku: String!, $literalStatusText: String!, $published: String!, $boolean: String!, $negated: String!, $unknown: String!) {
          active: products(first: 10, query: $status) { nodes { id title status vendor productType tags } }
          activeCount: productsCount(query: $status) { count precision }
          vendorType: products(first: 10, query: $vendorType) { nodes { id } }
          vendorTypeCount: productsCount(query: $vendorType) { count precision }
          title: products(first: 10, query: $title) { nodes { id } }
          tag: products(first: 10, query: $tag) { nodes { id } }
          sku: products(first: 10, query: $sku) { nodes { id } }
          literalStatusText: products(first: 10, query: $literalStatusText) { nodes { id } }
          published: products(first: 10, query: $published) { nodes { id } }
          boolean: products(first: 10, query: $boolean) { nodes { id } }
          negated: products(first: 10, query: $negated) { nodes { id } }
          unknown: products(first: 10, query: $unknown) { nodes { id } }
          unknownCount: productsCount(query: $unknown) { count precision }
        }
        "#,
        json!({
            "status": "status:ACTIVE",
            "vendorType": "vendor:Northwind product_type:Jackets",
            "title": "title:Alpha",
            "tag": "tag:featured",
            "sku": "sku:ALPHA-FILTER-SKU",
            "literalStatusText": "\"status: ACTIVE\"",
            "published": "published_status:published",
            "boolean": "(vendor:Northwind OR vendor:Southwind) status:ACTIVE",
            "negated": "tag:featured -product_type:Shirts",
            "unknown": "warehouse:Northwind"
        }),
    ));

    assert_eq!(read.status, 200);
    assert_eq!(
        read.body["data"]["active"]["nodes"],
        json!([
            {
                "id": "gid://shopify/Product/alpha",
                "title": "Alpha status: ACTIVE Jacket",
                "status": "ACTIVE",
                "vendor": "Northwind",
                "productType": "Jackets",
                "tags": ["featured", "outerwear"]
            },
            {
                "id": "gid://shopify/Product/gamma",
                "title": "Gamma Shirt",
                "status": "ACTIVE",
                "vendor": "Northwind",
                "productType": "Shirts",
                "tags": ["featured"]
            }
        ])
    );
    assert_eq!(
        read.body["data"]["activeCount"],
        json!({ "count": 2, "precision": "EXACT" })
    );
    assert_eq!(
        read.body["data"]["vendorType"]["nodes"],
        json!([{ "id": "gid://shopify/Product/alpha" }])
    );
    assert_eq!(
        read.body["data"]["vendorTypeCount"],
        json!({ "count": 1, "precision": "EXACT" })
    );
    assert_eq!(
        read.body["data"]["title"]["nodes"],
        json!([{ "id": "gid://shopify/Product/alpha" }])
    );
    assert_eq!(
        read.body["data"]["tag"]["nodes"],
        json!([
            { "id": "gid://shopify/Product/alpha" },
            { "id": "gid://shopify/Product/gamma" }
        ])
    );
    assert_eq!(
        read.body["data"]["sku"]["nodes"],
        json!([{ "id": "gid://shopify/Product/alpha" }])
    );
    assert_eq!(
        read.body["data"]["literalStatusText"]["nodes"],
        json!([{ "id": "gid://shopify/Product/alpha" }])
    );
    assert_eq!(
        read.body["data"]["published"]["nodes"],
        json!([{ "id": "gid://shopify/Product/alpha" }])
    );
    assert_eq!(
        read.body["data"]["boolean"]["nodes"],
        json!([
            { "id": "gid://shopify/Product/alpha" },
            { "id": "gid://shopify/Product/gamma" }
        ])
    );
    assert_eq!(
        read.body["data"]["negated"]["nodes"],
        json!([{ "id": "gid://shopify/Product/alpha" }])
    );
    assert_eq!(read.body["data"]["unknown"]["nodes"], json!([]));
    assert_eq!(
        read.body["data"]["unknownCount"],
        json!({ "count": 0, "precision": "EXACT" })
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
                        { "id": DEFAULT_ORDER_UNFULFILLED_ID, "name": "Unfulfilled" },
                        { "id": DEFAULT_ORDER_UNPAID_ID, "name": "Unpaid" }
                    ],
                    "edges": [
                        { "cursor": format!("cursor:{DEFAULT_ORDER_UNFULFILLED_ID}"), "node": { "id": DEFAULT_ORDER_UNFULFILLED_ID } },
                        { "cursor": format!("cursor:{DEFAULT_ORDER_UNPAID_ID}"), "node": { "id": DEFAULT_ORDER_UNPAID_ID } }
                    ],
                    "pageInfo": {
                        "hasNextPage": true,
                        "hasPreviousPage": false,
                        "startCursor": format!("cursor:{DEFAULT_ORDER_UNFULFILLED_ID}"),
                        "endCursor": format!("cursor:{DEFAULT_ORDER_UNPAID_ID}")
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
        json!([{ "field": ["query"], "message": "Query Line 1 Column 6: 'valid' is unexpected." }])
    );
}

#[test]
fn segment_delete_stages_local_removal_and_keeps_raw_mutation_for_commit() {
    let mut proxy = snapshot_proxy();
    let create_query = r#"
        mutation SegmentCreateQueryGrammar($name: String!, $query: String!) {
          segmentCreate(name: $name, query: $query) {
            segment { id name query }
            userErrors { field message }
          }
        }
    "#;
    let created = proxy.process_request(json_graphql_request(
        create_query,
        json!({
            "name": "Delete setup segment-query-grammar-local",
            "query": "number_of_orders >= 1"
        }),
    ));
    let segment_id = created.body["data"]["segmentCreate"]["segment"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let remaining = proxy.process_request(json_graphql_request(
        create_query,
        json!({
            "name": "Remaining setup segment-query-grammar-local",
            "query": "customer_countries CONTAINS 'US'"
        }),
    ));
    let remaining_segment_id = remaining.body["data"]["segmentCreate"]["segment"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let read_query = r#"
        query SegmentDeleteReadAfterWrite($id: ID!) {
          segment(id: $id) { id name query }
          segments(first: 10) {
            nodes { id name query }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
          segmentsCount { count precision }
        }
    "#;
    let before_delete = proxy.process_request(json_graphql_request(
        read_query,
        json!({ "id": segment_id }),
    ));
    assert_eq!(
        before_delete.body["data"]["segment"],
        json!({
            "id": segment_id,
            "name": "Delete setup segment-query-grammar-local",
            "query": "number_of_orders >= 1"
        })
    );
    assert_eq!(
        before_delete.body["data"]["segments"]["nodes"],
        json!([
            {
                "id": segment_id,
                "name": "Delete setup segment-query-grammar-local",
                "query": "number_of_orders >= 1"
            },
            {
                "id": remaining_segment_id,
                "name": "Remaining setup segment-query-grammar-local",
                "query": "customer_countries CONTAINS 'US'"
            }
        ])
    );
    assert_eq!(
        before_delete.body["data"]["segmentsCount"],
        json!({ "count": 2, "precision": "EXACT" })
    );

    let delete_query = r#"
        mutation LocalSegmentDelete($id: ID!) {
          segmentDelete(id: $id) {
            deletedSegmentId
            userErrors { field message }
          }
        }
    "#;
    let deleted = proxy.process_request(json_graphql_request(
        delete_query,
        json!({ "id": segment_id }),
    ));
    assert_eq!(deleted.status, 200);
    assert_eq!(
        deleted.body["data"]["segmentDelete"],
        json!({
            "deletedSegmentId": segment_id,
            "userErrors": []
        })
    );

    let node = proxy.process_request(json_graphql_request(
        r#"
        query SegmentNodeRead($id: ID!) {
          node(id: $id) { ... on Segment { id name query } }
        }
        "#,
        json!({ "id": segment_id }),
    ));
    assert_eq!(node.status, 200);
    assert_eq!(node.body["data"]["node"], Value::Null);

    let after_delete = proxy.process_request(json_graphql_request(
        read_query,
        json!({ "id": segment_id }),
    ));
    assert_eq!(after_delete.body["data"]["segment"], Value::Null);
    assert_eq!(
        after_delete.body["data"]["segments"]["nodes"],
        json!([{
            "id": remaining_segment_id,
            "name": "Remaining setup segment-query-grammar-local",
            "query": "customer_countries CONTAINS 'US'"
        }])
    );
    assert_eq!(
        after_delete.body["data"]["segmentsCount"],
        json!({ "count": 1, "precision": "EXACT" })
    );

    let log = log_snapshot(&proxy);
    assert_eq!(log["entries"].as_array().unwrap().len(), 3);
    assert_eq!(
        log["entries"][2]["interpreted"]["primaryRootField"],
        json!("segmentDelete")
    );
    assert_eq!(
        log["entries"][2]["rawBody"],
        json_graphql_request(delete_query, json!({ "id": segment_id })).body
    );
    assert_eq!(log["entries"][2]["stagedResourceIds"], json!([segment_id]));
}

#[test]
fn segment_delete_matches_shopify_validation_shapes() {
    let mut proxy = snapshot_proxy();
    let delete_query = r#"
        mutation LocalSegmentDelete($id: ID!) {
          segmentDelete(id: $id) {
            deletedSegmentId
            userErrors { field message }
          }
        }
    "#;

    let unknown = proxy.process_request(json_graphql_request(
        delete_query,
        json!({ "id": "gid://shopify/Segment/999999999999" }),
    ));
    assert_eq!(unknown.status, 200);
    assert_eq!(
        unknown.body["data"]["segmentDelete"],
        json!({
            "deletedSegmentId": null,
            "userErrors": [{
                "field": ["id"],
                "message": "Segment does not exist"
            }]
        })
    );

    let malformed = proxy.process_request(json_graphql_request(
        delete_query,
        json!({ "id": "not-a-gid" }),
    ));
    assert_eq!(malformed.status, 200);
    assert_eq!(
        malformed.body["errors"][0]["extensions"]["code"],
        json!("INVALID_VARIABLE")
    );
    assert_eq!(malformed.body.get("data"), None);

    let wrong_type = proxy.process_request(json_graphql_request(
        delete_query,
        json!({ "id": "gid://shopify/Order/1" }),
    ));
    assert_eq!(wrong_type.status, 200);
    assert_eq!(
        wrong_type.body["errors"][0]["extensions"]["code"],
        json!("RESOURCE_NOT_FOUND")
    );
    assert_eq!(wrong_type.body["data"]["segmentDelete"], Value::Null);
    assert_eq!(log_snapshot(&proxy)["entries"], json!([]));
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
    assert_eq!(log_snapshot(&proxy)["entries"], json!([]));

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
fn segment_update_literal_null_only_attributes_are_absent_changes() {
    let mut proxy = snapshot_proxy();
    let created = proxy.process_request(json_graphql_request(
        r#"
        mutation SegmentUpdateNullSetup($name: String!, $query: String!) {
          segmentCreate(name: $name, query: $query) {
            segment { id name query creationDate lastEditDate }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "name": "Literal null update setup",
            "query": "number_of_orders >= 1"
        }),
    ));
    let original_segment = created.body["data"]["segmentCreate"]["segment"].clone();
    let segment_id = original_segment["id"].as_str().unwrap().to_string();
    assert_eq!(log_snapshot(&proxy)["entries"].as_array().unwrap().len(), 1);

    for update_query in [
        r#"
        mutation SegmentUpdateNameLiteralNull($id: ID!) {
          segmentUpdate(id: $id, name: null) {
            segment { id name query creationDate lastEditDate }
            userErrors { field message }
          }
        }
        "#,
        r#"
        mutation SegmentUpdateQueryLiteralNull($id: ID!) {
          segmentUpdate(id: $id, query: null) {
            segment { id name query creationDate lastEditDate }
            userErrors { field message }
          }
        }
        "#,
    ] {
        let rejected = proxy.process_request(json_graphql_request(
            update_query,
            json!({ "id": segment_id }),
        ));
        assert_eq!(
            rejected.body["data"]["segmentUpdate"],
            json!({
                "segment": null,
                "userErrors": [{
                    "field": null,
                    "message": "At least one attribute to change must be present"
                }]
            })
        );

        let read_back = proxy.process_request(json_graphql_request(
            r#"
            query SegmentAfterRejectedLiteralNullUpdate($id: ID!) {
              segment(id: $id) { id name query creationDate lastEditDate }
            }
            "#,
            json!({ "id": segment_id }),
        ));
        assert_eq!(
            read_back.body["data"]["segment"], original_segment,
            "null-only update must not mutate the staged segment"
        );
        assert_eq!(
            log_snapshot(&proxy)["entries"].as_array().unwrap().len(),
            1,
            "null-only update must not append a staged mutation log entry"
        );
    }

    let renamed = proxy.process_request(json_graphql_request(
        r#"
        mutation SegmentUpdateNameWithQueryLiteralNull($id: ID!) {
          segmentUpdate(id: $id, name: "Literal null renamed", query: null) {
            segment { id name query creationDate lastEditDate }
            userErrors { field message }
          }
        }
        "#,
        json!({ "id": segment_id }),
    ));
    assert_eq!(
        renamed.body["data"]["segmentUpdate"],
        json!({
            "segment": {
                "id": segment_id,
                "name": "Literal null renamed",
                "query": "number_of_orders >= 1",
                "creationDate": original_segment["creationDate"],
                "lastEditDate": "2024-01-01T00:00:02.000Z"
            },
            "userErrors": []
        })
    );
    assert_eq!(log_snapshot(&proxy)["entries"].as_array().unwrap().len(), 2);
}

#[test]
fn segment_create_rejects_at_limit_with_shopify_message() {
    let mut proxy = snapshot_proxy();
    let create_query = r#"
        mutation AnySegmentCreateName($name: String!, $query: String!) {
          segmentCreate(name: $name, query: $query) {
            segment { id }
            userErrors { field message }
          }
        }
    "#;

    for index in 0..6000 {
        let created = proxy.process_request(json_graphql_request(
            create_query,
            json!({ "name": format!("Limit Segment {index}"), "query": "number_of_orders >= 1" }),
        ));
        assert_eq!(
            created.body["data"]["segmentCreate"]["userErrors"],
            json!([]),
            "segment {index} should stage without userErrors"
        );
    }

    let rejected = proxy.process_request(json_graphql_request(
        create_query,
        json!({ "name": "Limit Segment Overflow", "query": "number_of_orders >= 1" }),
    ));
    assert_eq!(rejected.status, 200);
    assert_eq!(
        rejected.body["data"]["segmentCreate"],
        json!({
            "segment": null,
            "userErrors": [{
                "field": null,
                "message": "Segment limit reached. Delete an existing segment to create more."
            }]
        })
    );
    assert_eq!(
        log_snapshot(&proxy)["entries"].as_array().unwrap().len(),
        6000
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

    let one = proxy.process_request(json_graphql_request(
        create_query,
        json!({ "name": "Counter Segment (1)", "query": "number_of_orders >= 1" }),
    ));
    assert_eq!(
        one.body["data"]["segmentCreate"]["segment"]["name"],
        json!("Counter Segment (1)")
    );

    let one_duplicate = proxy.process_request(json_graphql_request(
        create_query,
        json!({ "name": "Counter Segment (1)", "query": "number_of_orders >= 1" }),
    ));
    assert_eq!(
        one_duplicate.body["data"]["segmentCreate"]["segment"]["name"],
        json!("Counter Segment (2)")
    );

    let zero = proxy.process_request(json_graphql_request(
        create_query,
        json!({ "name": "Zero Counter Segment (0)", "query": "number_of_orders >= 1" }),
    ));
    assert_eq!(
        zero.body["data"]["segmentCreate"]["segment"]["name"],
        json!("Zero Counter Segment (0)")
    );

    let zero_duplicate = proxy.process_request(json_graphql_request(
        create_query,
        json!({ "name": "Zero Counter Segment (0)", "query": "number_of_orders >= 1" }),
    ));
    assert_eq!(
        zero_duplicate.body["data"]["segmentCreate"]["segment"]["name"],
        json!("Zero Counter Segment (1)")
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
fn customer_segment_members_query_create_coerces_segment_id_before_resolver_errors() {
    let mut proxy = snapshot_proxy();
    let create_query = r#"
        mutation CustomerSegmentMembersQueryCreateSegmentIdPaths($input: CustomerSegmentMembersQueryInput!) {
          customerSegmentMembersQueryCreate(input: $input) {
            customerSegmentMembersQuery { id }
            userErrors { field code message }
          }
        }
    "#;

    for segment_id in ["not-a-gid", ""] {
        let response = proxy.process_request(json_graphql_request(
            create_query,
            json!({ "input": { "segmentId": segment_id } }),
        ));

        assert_eq!(response.status, 200);
        assert_eq!(response.body.get("data"), None);
        assert_eq!(
            response.body["errors"][0]["message"],
            json!(format!(
                "Variable $input of type CustomerSegmentMembersQueryInput! was provided invalid value for segmentId (Invalid global id '{segment_id}')"
            ))
        );
        assert_eq!(
            response.body["errors"][0]["extensions"]["code"],
            json!("INVALID_VARIABLE")
        );
        assert_eq!(
            response.body["errors"][0]["extensions"]["problems"][0]["path"],
            json!(["segmentId"])
        );
        assert_eq!(
            response.body["errors"][0]["extensions"]["problems"][0]["message"],
            json!(format!("Invalid global id '{segment_id}'"))
        );
    }

    let wrong_type = proxy.process_request(json_graphql_request(
        create_query,
        json!({ "input": { "segmentId": "gid://shopify/Customer/1" } }),
    ));
    assert_eq!(wrong_type.status, 200);
    assert_eq!(
        wrong_type.body["errors"],
        json!([{
            "message": "invalid id",
            "locations": [{"line": 3, "column": 11}],
            "extensions": {"code": "RESOURCE_NOT_FOUND"},
            "path": ["customerSegmentMembersQueryCreate"]
        }])
    );
    assert_eq!(
        wrong_type.body["data"]["customerSegmentMembersQueryCreate"],
        Value::Null
    );

    let unknown_segment = proxy.process_request(json_graphql_request(
        create_query,
        json!({ "input": { "segmentId": "gid://shopify/Segment/999999999999" } }),
    ));
    assert_eq!(
        unknown_segment.body["data"]["customerSegmentMembersQueryCreate"],
        json!({
            "customerSegmentMembersQuery": null,
            "userErrors": [{
                "field": null,
                "code": "INVALID",
                "message": "Invalid segment ID."
            }]
        })
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
fn saved_search_app_namespace_uses_request_api_client_id() {
    let mut proxy = snapshot_proxy();
    let mut create_request = json_graphql_request(
        r#"
        mutation SavedSearchAppNamespace($input: SavedSearchCreateInput!) {
          savedSearchCreate(input: $input) {
            savedSearch { id name query resourceType searchTerms filters { key value } }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "input": {
                "resourceType": "PRODUCT",
                "name": "App namespace products",
                "query": "metafields.$app.tier:gold"
            }
        }),
    );
    create_request.headers.insert(
        "x-shopify-draft-proxy-api-client-id".to_string(),
        "999999999999".to_string(),
    );
    let create = proxy.process_request(create_request);

    assert_eq!(create.status, 200);
    assert_eq!(
        create.body["data"]["savedSearchCreate"]["savedSearch"],
        json!({
            "id": "gid://shopify/SavedSearch/1?shopify-draft-proxy=synthetic",
            "name": "App namespace products",
            "query": "metafields.app--999999999999.tier:gold",
            "resourceType": "PRODUCT",
            "searchTerms": "",
            "filters": [{ "key": "metafields.app--999999999999.tier", "value": "gold" }]
        })
    );
    assert_eq!(
        create.body["data"]["savedSearchCreate"]["userErrors"],
        json!([])
    );

    let created_id = create.body["data"]["savedSearchCreate"]["savedSearch"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let mut update_request = json_graphql_request(
        r#"
        mutation SavedSearchAppNamespaceUpdate($input: SavedSearchUpdateInput!) {
          savedSearchUpdate(input: $input) {
            savedSearch { id name query resourceType searchTerms filters { key value } }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "input": {
                "id": created_id,
                "query": "metafields.$app.vip:true"
            }
        }),
    );
    update_request.headers.insert(
        "x-shopify-draft-proxy-api-client-id".to_string(),
        "999999999999".to_string(),
    );
    let update = proxy.process_request(update_request);
    assert_eq!(
        update.body["data"]["savedSearchUpdate"]["savedSearch"]["query"],
        json!("metafields.app--999999999999.vip:true")
    );
    assert_eq!(
        update.body["data"]["savedSearchUpdate"]["savedSearch"]["filters"],
        json!([{ "key": "metafields.app--999999999999.vip", "value": "true" }])
    );

    let mut read_request = json_graphql_request(
        r#"
        query SavedSearchAppNamespaceRead {
          productSavedSearches(first: 1, reverse: true) {
            nodes { query searchTerms filters { key value } }
          }
        }
        "#,
        json!({}),
    );
    read_request.headers.insert(
        "x-shopify-draft-proxy-api-client-id".to_string(),
        "999999999999".to_string(),
    );
    let read = proxy.process_request(read_request);
    assert_eq!(
        read.body["data"]["productSavedSearches"]["nodes"],
        json!([{
            "query": "metafields.app--999999999999.vip:true",
            "searchTerms": "",
            "filters": [{ "key": "metafields.app--999999999999.vip", "value": "true" }]
        }])
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
fn saved_search_delete_payload_shop_uses_restored_shop_state() {
    let mut proxy = snapshot_proxy();
    let dump = proxy.process_request(request_with_body("POST", "/__meta/dump", "{}"));
    let mut restored = dump.body.clone();
    restored["state"]["baseState"]["shop"] = json!({
        "id": "gid://shopify/Shop/restored-saved-search",
        "name": "Restored saved search shop",
        "myshopifyDomain": "restored-saved-search.myshopify.com",
        "currencyCode": "EUR",
        "primaryDomain": {
            "id": "gid://shopify/Domain/555666777",
            "host": "restored-saved-search.example",
            "url": "https://restored-saved-search.example",
            "sslEnabled": true
        }
    });
    let restore = proxy.process_request(request_with_body(
        "POST",
        "/__meta/restore",
        &restored.to_string(),
    ));
    assert_eq!(restore.status, 200);

    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateSavedSearchForShopPayload($input: SavedSearchCreateInput!) {
          savedSearchCreate(input: $input) {
            savedSearch { id }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "input": {
                "resourceType": "PRODUCT",
                "name": "Delete payload shop",
                "query": "status:ACTIVE"
            }
        }),
    ));
    assert_eq!(create.status, 200);
    assert_eq!(
        create.body["data"]["savedSearchCreate"]["userErrors"],
        json!([])
    );
    let id = create.body["data"]["savedSearchCreate"]["savedSearch"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let delete = proxy.process_request(json_graphql_request(
        r#"
        mutation DeleteSavedSearchShopPayload($input: SavedSearchDeleteInput!) {
          savedSearchDelete(input: $input) {
            deletedSavedSearchId
            shop { id name myshopifyDomain currencyCode primaryDomain { id host } }
            userErrors { field message }
          }
        }
        "#,
        json!({ "input": { "id": id } }),
    ));

    assert_eq!(delete.status, 200);
    assert_eq!(
        delete.body["data"]["savedSearchDelete"],
        json!({
            "deletedSavedSearchId": id,
            "shop": {
                "id": "gid://shopify/Shop/restored-saved-search",
                "name": "Restored saved search shop",
                "myshopifyDomain": "restored-saved-search.myshopify.com",
                "currencyCode": "EUR",
                "primaryDomain": {
                    "id": "gid://shopify/Domain/555666777",
                    "host": "restored-saved-search.example"
                }
            },
            "userErrors": []
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

    let order_reserved_and_unknown = proxy.process_request(json_graphql_request(
        r#"
        mutation SavedSearchOrderReservedUnknown($input: SavedSearchCreateInput!) {
          savedSearchCreate(input: $input) { savedSearch { id } userErrors { field message } }
        }
        "#,
        json!({ "input": { "resourceType": "ORDER", "name": "Order Reserved Unknown", "query": "reference_location_id:1 made_up_filter:foo" } }),
    ));
    assert_eq!(
        order_reserved_and_unknown.body["data"]["savedSearchCreate"],
        json!({
            "savedSearch": null,
            "userErrors": [
                { "field": ["input", "query"], "message": "Search terms is invalid, 'reference_location_id' is a reserved filter name" },
                { "field": ["input", "query"], "message": "Query is invalid, 'made_up_filter' is not a valid filter" }
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

    let incompatible_update = proxy.process_request(json_graphql_request(
        r#"
        mutation SavedSearchIncompatibleFilterUpdate($input: SavedSearchUpdateInput!) {
          savedSearchUpdate(input: $input) { savedSearch { id name query resourceType } userErrors { field message } }
        }
        "#,
        json!({ "input": { "id": update_id, "query": "collection_id:\"123\" tag:\"AAA\"" } }),
    ));
    assert_eq!(
        incompatible_update.body["data"]["savedSearchUpdate"],
        json!({
            "savedSearch": {
                "id": update_id,
                "name": "Update Unknown Seed",
                "query": "collection_id:\"123\" tag:\"AAA\"",
                "resourceType": "PRODUCT"
            },
            "userErrors": [
                { "field": ["input", "query"], "message": "Query has incompatible filters: collection_id, tag" }
            ]
        })
    );

    let order_reserved_update = proxy.process_request(json_graphql_request(
        r#"
        mutation SavedSearchOrderReservedUpdate($input: SavedSearchUpdateInput!) {
          savedSearchUpdate(input: $input) { savedSearch { id name query resourceType } userErrors { field message } }
        }
        "#,
        json!({ "input": { "id": DEFAULT_ORDER_UNFULFILLED_ID, "query": "reference_location_id:42 made_up_filter:foo" } }),
    ));
    assert_eq!(
        order_reserved_update.body["data"]["savedSearchUpdate"],
        json!({
            "savedSearch": {
                "id": DEFAULT_ORDER_UNFULFILLED_ID,
                "name": "Unfulfilled",
                "query": "reference_location_id:42 made_up_filter:foo",
                "resourceType": "ORDER"
            },
            "userErrors": [
                { "field": ["input", "searchTerms"], "message": "Search terms is invalid, 'reference_location_id' is a reserved filter name" },
                { "field": ["input", "query"], "message": "Query is invalid, 'made_up_filter' is not a valid filter" }
            ]
        })
    );
}

#[test]
fn saved_search_blank_name_and_input_required_user_errors_are_schema_shaped_and_aggregated() {
    let mut proxy = snapshot_proxy();

    let blank_invalid_query = proxy.process_request(json_graphql_request(
        r#"
        mutation SavedSearchBlankNameInvalidQuery($input: SavedSearchCreateInput!) {
          savedSearchCreate(input: $input) {
            savedSearch { id }
            userErrors { field message }
          }
        }
        "#,
        json!({ "input": { "resourceType": "PRODUCT", "name": "", "query": "made_up_filter:foo" } }),
    ));
    assert_eq!(blank_invalid_query.status, 200);
    assert_eq!(
        blank_invalid_query.body["data"]["savedSearchCreate"],
        json!({
            "savedSearch": null,
            "userErrors": [
                { "field": ["input", "name"], "message": "Name can't be blank" },
                { "field": ["input", "query"], "message": "Query is invalid, 'made_up_filter' is not a valid filter" }
            ]
        })
    );

    let blank_empty_query = proxy.process_request(json_graphql_request(
        r#"
        mutation SavedSearchBlankName($input: SavedSearchCreateInput!) {
          savedSearchCreate(input: $input) {
            savedSearch { id }
            userErrors { field message }
          }
        }
        "#,
        json!({ "input": { "resourceType": "PRODUCT", "name": "", "query": "" } }),
    ));
    assert_eq!(blank_empty_query.status, 200);
    assert_eq!(
        blank_empty_query.body["data"]["savedSearchCreate"],
        json!({
            "savedSearch": null,
            "userErrors": [
                { "field": ["input", "name"], "message": "Name can't be blank" }
            ]
        })
    );

    let update_seed = proxy.process_request(json_graphql_request(
        r#"
        mutation SavedSearchBlankUpdateSeed($input: SavedSearchCreateInput!) {
          savedSearchCreate(input: $input) {
            savedSearch { id name query resourceType }
            userErrors { field message }
          }
        }
        "#,
        json!({ "input": { "resourceType": "PRODUCT", "name": "Blank Update Seed", "query": "vendor:Acme" } }),
    ));
    assert_eq!(
        update_seed.body["data"]["savedSearchCreate"]["userErrors"],
        json!([])
    );
    let update_id = update_seed.body["data"]["savedSearchCreate"]["savedSearch"]["id"]
        .as_str()
        .unwrap();

    let blank_update = proxy.process_request(json_graphql_request(
        r#"
        mutation SavedSearchBlankNameUpdate($input: SavedSearchUpdateInput!) {
          savedSearchUpdate(input: $input) {
            savedSearch { id name query resourceType }
            userErrors { field message }
          }
        }
        "#,
        json!({ "input": { "id": update_id, "name": "" } }),
    ));
    assert_eq!(blank_update.status, 200);
    assert_eq!(
        blank_update.body["data"]["savedSearchUpdate"],
        json!({
            "savedSearch": {
                "id": update_id,
                "name": "Blank Update Seed",
                "query": "vendor:Acme",
                "resourceType": "PRODUCT"
            },
            "userErrors": [
                { "field": ["input", "name"], "message": "Name can't be blank" }
            ]
        })
    );

    let blank_invalid_query_update = proxy.process_request(json_graphql_request(
        r#"
        mutation SavedSearchBlankNameInvalidQueryUpdate($input: SavedSearchUpdateInput!) {
          savedSearchUpdate(input: $input) {
            savedSearch { id name query resourceType }
            userErrors { field message }
          }
        }
        "#,
        json!({ "input": { "id": update_id, "name": "   ", "query": "made_up_filter:foo" } }),
    ));
    assert_eq!(blank_invalid_query_update.status, 200);
    assert_eq!(
        blank_invalid_query_update.body["data"]["savedSearchUpdate"],
        json!({
            "savedSearch": {
                "id": update_id,
                "name": "Blank Update Seed",
                "query": "made_up_filter:foo",
                "resourceType": "PRODUCT"
            },
            "userErrors": [
                { "field": ["input", "name"], "message": "Name can't be blank" },
                { "field": ["input", "query"], "message": "Query is invalid, 'made_up_filter' is not a valid filter" }
            ]
        })
    );

    let omitted_name_update = proxy.process_request(json_graphql_request(
        r#"
        mutation SavedSearchOmittedNameUpdate($input: SavedSearchUpdateInput!) {
          savedSearchUpdate(input: $input) {
            savedSearch { id name query resourceType }
            userErrors { field message }
          }
        }
        "#,
        json!({ "input": { "id": update_id, "query": "vendor:Changed" } }),
    ));
    assert_eq!(omitted_name_update.status, 200);
    assert_eq!(
        omitted_name_update.body["data"]["savedSearchUpdate"],
        json!({
            "savedSearch": {
                "id": update_id,
                "name": "Blank Update Seed",
                "query": "vendor:Changed",
                "resourceType": "PRODUCT"
            },
            "userErrors": []
        })
    );

    let read_after_updates = proxy.process_request(json_graphql_request(
        r#"
        query SavedSearchReadAfterBlankUpdate {
          productSavedSearches(first: 10) {
            nodes { id name query resourceType }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        read_after_updates.body["data"]["productSavedSearches"]["nodes"],
        json!([
            {
                "id": update_id,
                "name": "Blank Update Seed",
                "query": "vendor:Changed",
                "resourceType": "PRODUCT"
            }
        ])
    );

    let null_inputs = proxy.process_request(json_graphql_request(
        r#"
        mutation SavedSearchNullInputs($createInput: SavedSearchCreateInput, $updateInput: SavedSearchUpdateInput) {
          create: savedSearchCreate(input: $createInput) {
            savedSearch { id }
            userErrors { field message }
          }
          update: savedSearchUpdate(input: $updateInput) {
            savedSearch { id }
            userErrors { field message }
          }
        }
        "#,
        json!({ "createInput": null, "updateInput": null }),
    ));
    assert_eq!(null_inputs.status, 200);
    assert_eq!(
        null_inputs.body["data"],
        json!({
            "create": {
                "savedSearch": null,
                "userErrors": [
                    { "field": ["input"], "message": "Saved search input is required" }
                ]
            },
            "update": {
                "savedSearch": null,
                "userErrors": [
                    { "field": ["input"], "message": "Saved search input is required" }
                ]
            }
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

    let missing_create_input = proxy.process_request(graphql_request(
        "POST",
        r#"{"query":"mutation { missingCreateInput: productCreate { userErrors { msg: message } } }"}"#,
    ));
    assert_eq!(missing_create_input.status, 200);
    assert_eq!(
        missing_create_input.body,
        json!({
            "data": {
                "missingCreateInput": {
                    "userErrors": [{
                        "msg": "Product input is required"
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

    let missing_update_input = proxy.process_request(graphql_request(
        "POST",
        r#"{"query":"mutation { missingUpdateInput: productUpdate { userErrors { msg: message } } }"}"#,
    ));
    assert_eq!(missing_update_input.status, 200);
    assert_eq!(
        missing_update_input.body,
        json!({
            "data": {
                "missingUpdateInput": {
                    "userErrors": [{
                        "msg": "Product input is required"
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
fn product_delete_payload_shop_uses_restored_shop_state_for_all_payload_branches() {
    let mut proxy = snapshot_proxy().with_base_products(vec![
        seed_product("gid://shopify/Product/sync-delete"),
        seed_product("gid://shopify/Product/async-delete"),
    ]);
    let expected_shop = restore_product_payload_shop(&mut proxy);

    let sync_delete = proxy.process_request(json_graphql_request(
        r#"
        mutation ProductDeletePayloadShop($input: ProductDeleteInput!) {
          productDelete(input: $input) {
            deletedProductId
            shop { id name myshopifyDomain }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "input": { "id": "gid://shopify/Product/sync-delete" } }),
    ));
    assert_eq!(sync_delete.status, 200);
    assert_eq!(
        sync_delete.body["data"]["productDelete"],
        json!({
            "deletedProductId": "gid://shopify/Product/sync-delete",
            "shop": expected_shop,
            "userErrors": []
        })
    );

    let not_found = proxy.process_request(json_graphql_request(
        r#"
        mutation ProductDeletePayloadShopNotFound($input: ProductDeleteInput!) {
          productDelete(input: $input) {
            deletedProductId
            shop { id name myshopifyDomain }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "input": { "id": "gid://shopify/Product/missing" } }),
    ));
    assert_eq!(not_found.status, 200);
    assert_eq!(
        not_found.body["data"]["productDelete"],
        json!({
            "deletedProductId": null,
            "shop": selected_product_payload_shop(),
            "userErrors": [{
                "field": ["id"],
                "message": "Product does not exist",
                "code": "NOT_FOUND"
            }]
        })
    );

    let async_delete = proxy.process_request(json_graphql_request(
        r#"
        mutation ProductDeletePayloadShopAsync($input: ProductDeleteInput!, $synchronous: Boolean!) {
          productDelete(input: $input, synchronous: $synchronous) {
            deletedProductId
            shop { id name myshopifyDomain }
            productDeleteOperation {
              id
              status
              deletedProductId
              userErrors { field message }
            }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "input": { "id": "gid://shopify/Product/async-delete" },
            "synchronous": false
        }),
    ));
    assert_eq!(async_delete.status, 200);
    assert_eq!(
        async_delete.body["data"]["productDelete"]["shop"],
        selected_product_payload_shop()
    );
    assert_eq!(
        async_delete.body["data"]["productDelete"]["deletedProductId"],
        Value::Null
    );
    assert_eq!(
        async_delete.body["data"]["productDelete"]["productDeleteOperation"]["status"],
        json!("CREATED")
    );
    assert_eq!(
        async_delete.body["data"]["productDelete"]["productDeleteOperation"]["deletedProductId"],
        Value::Null
    );
    assert_eq!(
        async_delete.body["data"]["productDelete"]["productDeleteOperation"]["userErrors"],
        json!([])
    );
    assert_eq!(
        async_delete.body["data"]["productDelete"]["userErrors"],
        json!([])
    );

    let duplicate_async = proxy.process_request(json_graphql_request(
        r#"
        mutation ProductDeletePayloadShopAsyncDuplicate($input: ProductDeleteInput!, $synchronous: Boolean!) {
          productDelete(input: $input, synchronous: $synchronous) {
            deletedProductId
            shop { id name myshopifyDomain }
            productDeleteOperation { id status }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "input": { "id": "gid://shopify/Product/async-delete" },
            "synchronous": false
        }),
    ));
    assert_eq!(duplicate_async.status, 200);
    assert_eq!(
        duplicate_async.body["data"]["productDelete"],
        json!({
            "deletedProductId": null,
            "shop": selected_product_payload_shop(),
            "productDeleteOperation": null,
            "userErrors": [{
                "field": null,
                "message": "Another operation already in progress. Please wait until current one is finished."
            }]
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
    assert_eq!(log_snapshot(&proxy), json!({ "entries": [] }));

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
    assert_eq!(log_snapshot(&proxy), json!({ "entries": [] }));

    let read_back = proxy.process_request(graphql_request(
        "POST",
        r#"{"query":"query { product(id: \"gid://shopify/Product/1\") { id status } }"}"#,
    ));
    assert_eq!(read_back.body["data"]["product"]["status"], json!("ACTIVE"));
}

#[test]
fn admin_graphql_capability_classification_uses_implemented_registry_entries() {
    // Implemented synthetic roots are now classified from the registry, but they still fail
    // closed when no domain dispatcher match arm handles the concrete root. Unimplemented roots
    // keep the passthrough fallback; in snapshot mode that surfaces as a 400 no-dispatcher error
    // because there is no upstream transport.
    let mut proxy = snapshot_proxy().with_registry(vec![
        registry_entry("knownProducts", OperationType::Query, true),
        registry_entry("knownProductCreate", OperationType::Mutation, true),
        registry_entry("knownButUnimplemented", OperationType::Query, false),
    ]);

    let known_query = proxy.process_request(graphql_request(
        "POST",
        r#"{"query":"query { knownProducts(first: 1) { nodes { id } } }"}"#,
    ));
    assert_eq!(known_query.status, 501);
    assert_eq!(
        known_query.body,
        json!({ "errors": [{ "message": "No Rust overlay-read dispatcher implemented for root field: knownProducts" }] })
    );

    let known_mutation = proxy.process_request(graphql_request(
        "POST",
        r#"{"query":"mutation { knownProductCreate(input: {}) { product { id } } }"}"#,
    ));
    assert_eq!(known_mutation.status, 501);
    assert_eq!(
        known_mutation.body,
        json!({ "errors": [{ "message": "No Rust stage-locally dispatcher implemented for root field: knownProductCreate" }] })
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
fn registry_classification_without_matching_root_field_fails_closed() {
    let mut proxy = snapshot_proxy().with_registry(vec![registry_entry(
        "productCreate",
        OperationType::Mutation,
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
    assert_eq!(log_snapshot(&proxy), json!({ "entries": [] }));
}

#[test]
fn implemented_registry_entry_without_dispatch_match_arm_fails_closed() {
    let mut proxy = snapshot_proxy().with_registry(vec![OperationRegistryEntry {
        name: "unknownSavedSearches".to_string(),
        operation_type: OperationType::Query,
        domain: CapabilityDomain::SavedSearches,
        implemented: true,
        match_names: vec!["unknownSavedSearches".to_string()],
        runtime_tests: vec!["tests/graphql_routes.rs".to_string()],
    }]);

    let response = proxy.process_request(graphql_request(
        "POST",
        r#"{"query":"query { unknownSavedSearches(first: 1) { nodes { id } } }"}"#,
    ));

    assert_eq!(response.status, 200);
    assert_eq!(response.body, json!({ "data": {} }));
}

#[test]
fn supported_product_variant_mutation_keeps_capability_metadata_in_log() {
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

    let response = proxy.process_request(graphql_request(
        "POST",
        r#"{"query":"mutation { productVariantCreate(input: { productId: \"gid://shopify/Product/1\" }) { productVariant { id } userErrors { message } } }"}"#,
    ));

    assert_eq!(response.status, 200);
    assert_eq!(
        log_snapshot(&proxy)["entries"][0]["interpreted"]["capability"],
        json!({
            "operationName": "productVariantCreate",
            "domain": "products",
            "execution": "stage-locally"
        })
    );
}

#[test]
fn collection_lifecycle_mutations_stage_locally_without_upstream_writes() {
    let upstream_calls = Arc::new(Mutex::new(Vec::<String>::new()));
    let mut proxy = configured_proxy(
        ReadMode::LiveHybrid,
        Some(shopify_draft_proxy::proxy::UnsupportedMutationMode::Passthrough),
    )
    .with_base_products(vec![
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
    ])
    .with_upstream_transport({
        let upstream_calls = Arc::clone(&upstream_calls);
        move |request| {
            upstream_calls.lock().unwrap().push(request.body);
            shopify_draft_proxy::proxy::Response {
                status: 599,
                headers: Default::default(),
                body: json!({"errors": [{"message": "upstream should not be called"}]}),
            }
        }
    });

    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateCollection($input: CollectionInput!) {
          collectionCreate(input: $input) {
            collection {
              id
              title
              handle
              sortOrder
              products(first: 10) { nodes { id } }
              hasFirst: hasProduct(id: "gid://shopify/Product/first")
              productsCount { count precision }
            }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "input": {
                "title": "Local Collection",
                "sortOrder": "MANUAL",
                "products": ["gid://shopify/Product/first"]
            }
        }),
    ));
    assert_eq!(create.status, 200);
    assert_eq!(
        create.body["data"]["collectionCreate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        create.body["data"]["collectionCreate"]["collection"]["products"]["nodes"],
        json!([{ "id": "gid://shopify/Product/first" }])
    );
    assert_eq!(
        create.body["data"]["collectionCreate"]["collection"]["hasFirst"],
        json!(true)
    );
    assert_eq!(
        create.body["data"]["collectionCreate"]["collection"]["productsCount"],
        json!({ "count": 0, "precision": "EXACT" })
    );
    let collection_id = create.body["data"]["collectionCreate"]["collection"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let update = proxy.process_request(json_graphql_request(
        r#"
        mutation UpdateCollection($input: CollectionInput!) {
          collectionUpdate(input: $input) {
            collection { id title handle sortOrder }
            userErrors { field message }
          }
        }
        "#,
        json!({ "input": { "id": collection_id, "title": "Updated Collection" } }),
    ));
    assert_eq!(
        update.body["data"]["collectionUpdate"]["collection"]["title"],
        json!("Updated Collection")
    );

    let add = proxy.process_request(json_graphql_request(
        r#"
        mutation AddProducts($id: ID!, $productIds: [ID!]!) {
          collectionAddProducts(id: $id, productIds: $productIds) {
            collection {
              id
              products(first: 10) { nodes { id title handle } }
              hasFirst: hasProduct(id: "gid://shopify/Product/first")
              productsCount { count precision }
            }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "id": collection_id,
            "productIds": ["gid://shopify/Product/first", "gid://shopify/Product/second"]
        }),
    ));
    assert_eq!(
        add.body["data"]["collectionAddProducts"]["userErrors"],
        json!([])
    );
    assert_eq!(
        add.body["data"]["collectionAddProducts"]["collection"]["products"]["nodes"],
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
        add.body["data"]["collectionAddProducts"]["collection"]["hasFirst"],
        json!(true)
    );

    let add = proxy.process_request(json_graphql_request(
        r#"
        mutation AddProducts($id: ID!, $productIds: [ID!]!) {
          collectionAddProducts(id: $id, productIds: $productIds) {
            collection {
              id
              products(first: 10) { nodes { id title handle } }
              hasFirst: hasProduct(id: "gid://shopify/Product/first")
              productsCount { count precision }
            }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "id": collection_id,
            "productIds": ["gid://shopify/Product/second"]
        }),
    ));
    assert_eq!(
        add.body["data"]["collectionAddProducts"]["userErrors"],
        json!([])
    );
    assert_eq!(
        add.body["data"]["collectionAddProducts"]["collection"]["products"]["nodes"],
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
        add.body["data"]["collectionAddProducts"]["collection"]["hasFirst"],
        json!(true)
    );

    let remove = proxy.process_request(json_graphql_request(
        r#"
        mutation RemoveProducts($id: ID!, $productIds: [ID!]!) {
          collectionRemoveProducts(id: $id, productIds: $productIds) {
            job { __typename id done query { __typename } }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "id": collection_id,
            "productIds": ["gid://shopify/Product/second"]
        }),
    ));
    assert_eq!(
        remove.body["data"]["collectionRemoveProducts"]["userErrors"],
        json!([])
    );
    assert_eq!(
        remove.body["data"]["collectionRemoveProducts"]["job"]["done"],
        json!(false)
    );
    assert_eq!(
        remove.body["data"]["collectionRemoveProducts"]["job"]["query"],
        Value::Null
    );
    let remove_job_id = remove.body["data"]["collectionRemoveProducts"]["job"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let job = proxy.process_request(json_graphql_request(
        r#"
        query CollectionJob($id: ID!) {
          job(id: $id) { __typename id done query { __typename } }
        }
        "#,
        json!({ "id": remove_job_id }),
    ));
    assert_eq!(job.body["data"]["job"]["__typename"], json!("Job"));
    assert_eq!(job.body["data"]["job"]["done"], json!(true));
    assert_eq!(
        job.body["data"]["job"]["query"],
        json!({ "__typename": "QueryRoot" })
    );

    let read = proxy.process_request(json_graphql_request(
        r#"
        query CollectionRead($collectionId: ID!, $productId: ID!) {
          collection(id: $collectionId) {
            id
            title
            products(first: 10) { nodes { id } }
            productsCount { count precision }
          }
          product(id: $productId) {
            id
            collections(first: 10) { nodes { id title handle } }
          }
        }
        "#,
        json!({
            "collectionId": collection_id,
            "productId": "gid://shopify/Product/first"
        }),
    ));
    assert_eq!(
        read.body["data"]["collection"]["products"]["nodes"],
        json!([{ "id": "gid://shopify/Product/first" }])
    );
    assert_eq!(
        read.body["data"]["collection"]["productsCount"],
        json!({ "count": 1, "precision": "EXACT" })
    );
    assert_eq!(
        read.body["data"]["product"]["collections"]["nodes"][0]["id"],
        json!(collection_id)
    );

    let delete = proxy.process_request(json_graphql_request(
        r#"
        mutation DeleteCollection($input: CollectionDeleteInput!) {
          collectionDelete(input: $input) {
            deletedCollectionId
            shop { id }
            userErrors { field message }
          }
        }
        "#,
        json!({ "input": { "id": collection_id } }),
    ));
    assert_eq!(
        delete.body["data"]["collectionDelete"]["deletedCollectionId"],
        json!(collection_id)
    );
    assert_eq!(
        delete.body["data"]["collectionDelete"]["shop"],
        json!({ "id": "gid://shopify/Shop/0" })
    );
    assert_eq!(
        delete.body["data"]["collectionDelete"]["userErrors"],
        json!([])
    );
    let after_delete = proxy.process_request(json_graphql_request(
        r#"
        query DeletedCollectionRead($id: ID!) {
          collection(id: $id) { id }
          product(id: "gid://shopify/Product/first") {
            collections(first: 10) { nodes { id } }
          }
        }
        "#,
        json!({ "id": collection_id }),
    ));
    assert_eq!(after_delete.body["data"]["collection"], Value::Null);
    assert_eq!(
        after_delete.body["data"]["product"]["collections"]["nodes"],
        json!([])
    );
    assert!(upstream_calls.lock().unwrap().is_empty());

    let log = log_snapshot(&proxy);
    let entries = log["entries"].as_array().unwrap();
    for root in [
        "collectionCreate",
        "collectionUpdate",
        "collectionAddProducts",
        "collectionRemoveProducts",
        "collectionDelete",
    ] {
        assert!(
            entries.iter().any(
                |entry| entry["interpreted"]["primaryRootField"] == json!(root)
                    && entry["status"] == json!("staged")
                    && entry["rawBody"].as_str().unwrap_or_default().contains(root)
            ),
            "missing staged log entry for {root}: {log}"
        );
    }
}

#[test]
fn collection_delete_payload_includes_shop_on_user_error() {
    let mut proxy = snapshot_proxy();

    let missing = proxy.process_request(json_graphql_request(
        r#"
        mutation DeleteMissingCollection($input: CollectionDeleteInput!) {
          collectionDelete(input: $input) {
            deletedCollectionId
            shop { id }
            userErrors { field message }
          }
        }
        "#,
        json!({ "input": { "id": "gid://shopify/Collection/missing" } }),
    ));

    assert_eq!(missing.status, 200);
    assert_eq!(
        missing.body["data"]["collectionDelete"],
        json!({
            "deletedCollectionId": null,
            "shop": { "id": "gid://shopify/Shop/0" },
            "userErrors": [{
                "field": ["id"],
                "message": "Collection does not exist"
            }]
        })
    );
    assert_eq!(log_snapshot(&proxy)["entries"], json!([]));
}

#[test]
fn collection_create_rejects_client_supplied_id_without_staging() {
    let mut proxy = snapshot_proxy();

    let rejected = proxy.process_request(json_graphql_request(
        r#"
        mutation CollectionCreateRejectId($input: CollectionInput!) {
          collectionCreate(input: $input) {
            collection { id title }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "input": {
                "id": "gid://shopify/Collection/123",
                "title": "Rejected Collection"
            }
        }),
    ));

    assert_eq!(rejected.status, 200);
    assert_eq!(
        rejected.body,
        json!({
            "data": {
                "collectionCreate": {
                    "collection": Value::Null,
                    "userErrors": [{
                        "field": ["id"],
                        "message": "id cannot be specified on collection creation"
                    }]
                }
            }
        })
    );
    assert_eq!(log_snapshot(&proxy), json!({ "entries": [] }));
    assert_eq!(
        state_snapshot(&proxy)["stagedState"]["collections"],
        json!({})
    );

    let created = proxy.process_request(json_graphql_request(
        r#"
        mutation CollectionCreateWithoutId($input: CollectionInput!) {
          collectionCreate(input: $input) {
            collection { id title }
            userErrors { field message }
          }
        }
        "#,
        json!({ "input": { "title": "Accepted Collection" } }),
    ));

    assert_eq!(created.status, 200);
    assert_eq!(
        created.body["data"]["collectionCreate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        created.body["data"]["collectionCreate"]["collection"]["title"],
        json!("Accepted Collection")
    );
    let collection_id = created.body["data"]["collectionCreate"]["collection"]["id"]
        .as_str()
        .expect("accepted collection has id")
        .to_string();
    assert_eq!(
        log_snapshot(&proxy)["entries"][0]["stagedResourceIds"],
        json!([collection_id])
    );
}

#[test]
fn collection_create_rejects_unknown_initial_products_without_staging() {
    let mut proxy = snapshot_proxy().with_base_products(vec![ProductRecord {
        id: "gid://shopify/Product/known".to_string(),
        title: "Known Product".to_string(),
        handle: "known-product".to_string(),
        status: "ACTIVE".to_string(),
        ..ProductRecord::default()
    }]);
    let state_before = state_snapshot(&proxy);
    let log_before = log_snapshot(&proxy);

    let rejected = proxy.process_request(json_graphql_request(
        r#"
        mutation CollectionCreateUnknownProduct($input: CollectionInput!) {
          collectionCreate(input: $input) {
            collection { id products(first: 10) { nodes { id } } }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "input": {
                "title": "Rejected Product Collection",
                "products": [
                    "gid://shopify/Product/known",
                    "gid://shopify/Product/missing"
                ]
            }
        }),
    ));

    assert_eq!(rejected.status, 200);
    assert_eq!(
        rejected.body["data"]["collectionCreate"],
        json!({
            "collection": null,
            "userErrors": [{
                "field": ["products", "1"],
                "message": "Product does not exist"
            }]
        })
    );
    assert_eq!(state_snapshot(&proxy), state_before);
    assert_eq!(log_snapshot(&proxy), log_before);

    let accepted = proxy.process_request(json_graphql_request(
        r#"
        mutation CollectionCreateKnownProduct($input: CollectionInput!) {
          collectionCreate(input: $input) {
            collection { id products(first: 10) { nodes { id } } }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "input": {
                "title": "Accepted Product Collection",
                "products": ["gid://shopify/Product/known"]
            }
        }),
    ));

    assert_eq!(
        accepted.body["data"]["collectionCreate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        accepted.body["data"]["collectionCreate"]["collection"]["products"]["nodes"],
        json!([{ "id": "gid://shopify/Product/known" }])
    );
    assert_eq!(
        log_snapshot(&proxy)["entries"]
            .as_array()
            .expect("log entries should be an array")
            .len(),
        1
    );
}

#[test]
fn collection_create_accepts_empty_rules_as_custom_and_rejects_missing_rules_without_staging() {
    let mut proxy = snapshot_proxy();
    let empty_rules = proxy.process_request(json_graphql_request(
        r#"
        mutation CollectionCreateEmptyRuleSet($input: CollectionInput!) {
          collectionCreate(input: $input) {
            collection { id title ruleSet { appliedDisjunctively rules { column relation condition } } }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "input": {
                "title": "Empty Rules Custom Collection",
                "ruleSet": {
                    "appliedDisjunctively": false,
                    "rules": []
                }
            }
        }),
    ));

    assert_eq!(empty_rules.status, 200);
    assert_eq!(
        empty_rules.body["data"]["collectionCreate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        empty_rules.body["data"]["collectionCreate"]["collection"]["title"],
        json!("Empty Rules Custom Collection")
    );
    assert_eq!(
        empty_rules.body["data"]["collectionCreate"]["collection"]["ruleSet"],
        Value::Null
    );

    let mut proxy = snapshot_proxy();
    let state_before = state_snapshot(&proxy);
    let log_before = log_snapshot(&proxy);
    let missing_rules = proxy.process_request(json_graphql_request(
        r#"
        mutation CollectionCreateMissingRuleSetRules($input: CollectionInput!) {
          collectionCreate(input: $input) {
            collection { id ruleSet { rules { column relation condition } } }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "input": {
                "title": "Rejected Smart Collection",
                "ruleSet": { "appliedDisjunctively": false }
            }
        }),
    ));

    assert_eq!(missing_rules.status, 200);
    assert_eq!(
        missing_rules.body["data"]["collectionCreate"],
        json!({
            "collection": null,
            "userErrors": [{
                "field": ["ruleSet", "rules"],
                "message": "Rules cannot be an empty set"
            }]
        })
    );
    assert_eq!(state_snapshot(&proxy), state_before);
    assert_eq!(log_snapshot(&proxy), log_before);
}

#[test]
fn collection_update_missing_id_returns_top_level_bad_request_without_user_errors() {
    let mut proxy = snapshot_proxy();

    let missing_id = proxy.process_request(json_graphql_request(
        r#"
        mutation CollectionUpdateMissingId($input: CollectionInput!) {
          collectionUpdate(input: $input) {
            collection { id title handle }
            userErrors { field message }
          }
        }
        "#,
        json!({ "input": { "title": "Missing Id" } }),
    ));

    assert_eq!(missing_id.status, 200);
    assert_eq!(
        missing_id.body["errors"][0]["message"],
        json!("id must be specified on collectionUpdate")
    );
    assert_eq!(
        missing_id.body["errors"][0]["extensions"]["code"],
        json!("BAD_REQUEST")
    );
    assert_eq!(
        missing_id.body["errors"][0]["path"],
        json!(["collectionUpdate"])
    );
    assert_eq!(missing_id.body["data"]["collectionUpdate"], Value::Null);
    assert!(missing_id
        .body
        .pointer("/data/collectionUpdate/userErrors")
        .is_none());
    assert_eq!(log_snapshot(&proxy), json!({ "entries": [] }));

    let unknown_id = proxy.process_request(json_graphql_request(
        r#"
        mutation CollectionUpdateUnknownId($input: CollectionInput!) {
          collectionUpdate(input: $input) {
            collection { id title handle }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "input": {
                "id": "gid://shopify/Collection/999999999999999",
                "title": "Unknown Id"
            }
        }),
    ));

    assert_eq!(unknown_id.status, 200);
    assert_eq!(
        unknown_id.body["data"]["collectionUpdate"],
        json!({
            "collection": Value::Null,
            "userErrors": [{
                "field": Value::Null,
                "message": "Collection does not exist"
            }]
        })
    );
    assert_eq!(log_snapshot(&proxy), json!({ "entries": [] }));
}

#[test]
fn collection_validations_and_reorder_are_store_backed() {
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

    let long_title = proxy.process_request(json_graphql_request(
        r#"
        mutation LongCollectionTitle($input: CollectionInput!) {
          collectionCreate(input: $input) {
            collection { id }
            userErrors { field message }
          }
        }
        "#,
        json!({ "input": { "title": "T".repeat(256) } }),
    ));
    assert_eq!(
        long_title.body["data"]["collectionCreate"]["userErrors"],
        json!([{
            "field": ["title"],
            "message": "Title is too long (maximum is 255 characters)"
        }])
    );

    let invalid_sort = proxy.process_request(json_graphql_request(
        r#"
        mutation InvalidCollectionSort($input: CollectionInput!) {
          collectionCreate(input: $input) {
            collection { id }
            userErrors { field message }
          }
        }
        "#,
        json!({ "input": { "title": "Invalid Sort", "sortOrder": "NOT_REAL" } }),
    ));
    assert_eq!(
        invalid_sort.body["errors"][0]["extensions"]["code"],
        json!("INVALID_VARIABLE")
    );
    assert_eq!(
        invalid_sort.body["errors"][0]["extensions"]["problems"][0]["path"],
        json!(["sortOrder"])
    );

    let smart_create = proxy.process_request(json_graphql_request(
        r#"
        mutation SmartCollection($input: CollectionInput!) {
          collectionCreate(input: $input) {
            collection { id ruleSet { rules { column relation condition } } }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "input": {
                "title": "Smart Collection",
                "ruleSet": {
                    "appliedDisjunctively": false,
                    "rules": [{ "column": "TITLE", "relation": "CONTAINS", "condition": "First" }]
                }
            }
        }),
    ));
    let smart_id = smart_create.body["data"]["collectionCreate"]["collection"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let smart_add = proxy.process_request(json_graphql_request(
        r#"
        mutation SmartAdd($id: ID!, $productIds: [ID!]!) {
          collectionAddProductsV2(id: $id, productIds: $productIds) {
            job { id }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "id": smart_id,
            "productIds": ["gid://shopify/Product/first"]
        }),
    ));
    assert_eq!(
        smart_add.body["data"]["collectionAddProductsV2"]["job"],
        Value::Null
    );
    assert_eq!(
        smart_add.body["data"]["collectionAddProductsV2"]["userErrors"],
        json!([{
            "field": ["id"],
            "message": "Can't manually add products to a smart collection"
        }])
    );
    let state_before_smart_reorder = state_snapshot(&proxy);
    let log_len_before_smart_reorder = log_snapshot(&proxy)["entries"].as_array().unwrap().len();
    let smart_reorder = proxy.process_request(json_graphql_request(
        r#"
        mutation SmartReorder($id: ID!, $moves: [MoveInput!]!) {
          collectionReorderProducts(id: $id, moves: $moves) {
            job { id }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "id": smart_id,
            "moves": [{ "id": "gid://shopify/Product/first", "newPosition": "0" }]
        }),
    ));
    assert_eq!(
        smart_reorder.body["data"]["collectionReorderProducts"]["job"],
        Value::Null
    );
    assert_eq!(
        smart_reorder.body["data"]["collectionReorderProducts"]["userErrors"],
        json!([{
            "field": ["id"],
            "message": "Can't reorder products unless collection is manually sorted"
        }])
    );
    assert_eq!(state_snapshot(&proxy), state_before_smart_reorder);
    assert_eq!(
        log_snapshot(&proxy)["entries"].as_array().unwrap().len(),
        log_len_before_smart_reorder
    );

    let missing_reorder = proxy.process_request(json_graphql_request(
        r#"
        mutation MissingReorder($id: ID!, $moves: [MoveInput!]!) {
          collectionReorderProducts(id: $id, moves: $moves) {
            job { id }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "id": "gid://shopify/Collection/missing",
            "moves": [{ "id": "gid://shopify/Product/first", "newPosition": "0" }]
        }),
    ));
    assert_eq!(
        missing_reorder.body["data"]["collectionReorderProducts"]["job"],
        Value::Null
    );
    assert_eq!(
        missing_reorder.body["data"]["collectionReorderProducts"]["userErrors"],
        json!([{
            "field": ["id"],
            "message": "Collection does not exist"
        }])
    );

    let non_manual_create = proxy.process_request(json_graphql_request(
        r#"
        mutation NonManualCollection($input: CollectionInput!) {
          collectionCreate(input: $input) {
            collection { id sortOrder }
            userErrors { field message }
          }
        }
        "#,
        json!({ "input": { "title": "Best Selling Collection", "sortOrder": "BEST_SELLING" } }),
    ));
    let non_manual_id = non_manual_create.body["data"]["collectionCreate"]["collection"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let non_manual_add = proxy.process_request(json_graphql_request(
        r#"
        mutation AddNonManual($id: ID!, $productIds: [ID!]!) {
          collectionAddProductsV2(id: $id, productIds: $productIds) {
            job { id }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "id": non_manual_id,
            "productIds": ["gid://shopify/Product/first", "gid://shopify/Product/second"]
        }),
    ));
    assert_eq!(
        non_manual_add.body["data"]["collectionAddProductsV2"]["userErrors"],
        json!([])
    );
    let state_before_rejected_reorder = state_snapshot(&proxy);
    let log_len_before_rejected_reorder = log_snapshot(&proxy)["entries"].as_array().unwrap().len();
    let non_manual_reorder = proxy.process_request(json_graphql_request(
        r#"
        mutation NonManualReorder($id: ID!, $moves: [MoveInput!]!) {
          collectionReorderProducts(id: $id, moves: $moves) {
            job { id done }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "id": non_manual_id,
            "moves": [{ "id": "gid://shopify/Product/second", "newPosition": "0" }]
        }),
    ));
    assert_eq!(
        non_manual_reorder.body["data"]["collectionReorderProducts"],
        json!({
            "job": null,
            "userErrors": [{
                "field": ["id"],
                "message": "Can't reorder products unless collection is manually sorted"
            }]
        })
    );
    assert_eq!(state_snapshot(&proxy), state_before_rejected_reorder);
    assert_eq!(
        log_snapshot(&proxy)["entries"].as_array().unwrap().len(),
        log_len_before_rejected_reorder
    );

    let custom_create = proxy.process_request(json_graphql_request(
        r#"
        mutation CustomCollection($input: CollectionInput!) {
          collectionCreate(input: $input) {
            collection { id }
            userErrors { field message }
          }
        }
        "#,
        json!({ "input": { "title": "Manual Collection", "sortOrder": "MANUAL" } }),
    ));
    let custom_id = custom_create.body["data"]["collectionCreate"]["collection"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let add_v2 = proxy.process_request(json_graphql_request(
        r#"
        mutation AddV2($id: ID!, $productIds: [ID!]!) {
          collectionAddProductsV2(id: $id, productIds: $productIds) {
            job { id done query { __typename } }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "id": custom_id,
            "productIds": ["gid://shopify/Product/first", "gid://shopify/Product/second"]
        }),
    ));
    assert_eq!(
        add_v2.body["data"]["collectionAddProductsV2"]["userErrors"],
        json!([])
    );
    assert_eq!(
        add_v2.body["data"]["collectionAddProductsV2"]["job"]["done"],
        json!(false)
    );
    assert_eq!(
        add_v2.body["data"]["collectionAddProductsV2"]["job"]["query"],
        Value::Null
    );

    let reorder = proxy.process_request(json_graphql_request(
        r#"
        mutation Reorder($id: ID!, $moves: [MoveInput!]!) {
          collectionReorderProducts(id: $id, moves: $moves) {
            job { id done }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "id": custom_id,
            "moves": [{ "id": "gid://shopify/Product/second", "newPosition": "0" }]
        }),
    ));
    assert_eq!(
        reorder.body["data"]["collectionReorderProducts"]["userErrors"],
        json!([])
    );
    let read = proxy.process_request(json_graphql_request(
        r#"
        query ReorderedCollection($id: ID!) {
          collection(id: $id) {
            products(first: 10, sortKey: MANUAL) { nodes { id } }
          }
        }
        "#,
        json!({ "id": custom_id }),
    ));
    assert_eq!(
        read.body["data"]["collection"]["products"]["nodes"],
        json!([
            { "id": "gid://shopify/Product/second" },
            { "id": "gid://shopify/Product/first" }
        ])
    );

    let too_many = proxy.process_request(json_graphql_request(
        r#"
        mutation TooMany($id: ID!, $productIds: [ID!]!) {
          collectionRemoveProducts(id: $id, productIds: $productIds) {
            job { id }
            userErrors { field message }
          }
        }
        "#,
        json!({ "id": custom_id, "productIds": vec!["gid://shopify/Product/first"; 251] }),
    ));
    assert_eq!(
        too_many.body["errors"][0]["extensions"]["code"],
        json!("MAX_INPUT_SIZE_EXCEEDED")
    );
}

#[test]
fn collection_update_rejects_custom_rule_set_without_staging() {
    let mut proxy = snapshot_proxy();

    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateCustomCollection($input: CollectionInput!) {
          collectionCreate(input: $input) {
            collection { id ruleSet { rules { column relation condition } } }
            userErrors { field message }
          }
        }
        "#,
        json!({ "input": { "title": "Manual Collection" } }),
    ));
    assert_eq!(
        create.body["data"]["collectionCreate"]["userErrors"],
        json!([])
    );
    let collection_id = create.body["data"]["collectionCreate"]["collection"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let state_after_create = state_snapshot(&proxy);
    let log_len_after_create = log_snapshot(&proxy)["entries"].as_array().unwrap().len();

    let update = proxy.process_request(json_graphql_request(
        r#"
        mutation UpdateCustomCollectionRules($input: CollectionInput!) {
          collectionUpdate(input: $input) {
            collection { id ruleSet { appliedDisjunctively rules { column relation condition } } }
            job { id done }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "input": {
                "id": collection_id,
                "ruleSet": {
                    "appliedDisjunctively": false,
                    "rules": [{ "column": "TITLE", "relation": "CONTAINS", "condition": "Shoes" }]
                }
            }
        }),
    ));

    assert_eq!(
        update.body["data"]["collectionUpdate"],
        json!({
            "collection": null,
            "job": null,
            "userErrors": [{
                "field": ["id"],
                "message": "Cannot update rule set of a custom collection"
            }]
        })
    );
    assert_eq!(state_snapshot(&proxy), state_after_create);
    assert_eq!(
        log_snapshot(&proxy)["entries"].as_array().unwrap().len(),
        log_len_after_create
    );
}

#[test]
fn collection_update_rejects_empty_rule_set_rules() {
    let mut proxy = snapshot_proxy();

    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateSmartCollection($input: CollectionInput!) {
          collectionCreate(input: $input) {
            collection { id ruleSet { rules { column relation condition } } }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "input": {
                "title": "Smart Collection",
                "ruleSet": {
                    "appliedDisjunctively": false,
                    "rules": [{ "column": "TITLE", "relation": "CONTAINS", "condition": "Shoes" }]
                }
            }
        }),
    ));
    assert_eq!(
        create.body["data"]["collectionCreate"]["userErrors"],
        json!([])
    );
    let collection_id = create.body["data"]["collectionCreate"]["collection"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let state_after_create = state_snapshot(&proxy);

    let update = proxy.process_request(json_graphql_request(
        r#"
        mutation UpdateEmptyRules($input: CollectionInput!) {
          collectionUpdate(input: $input) {
            collection { id ruleSet { rules { column relation condition } } }
            job { id done }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "input": {
                "id": collection_id,
                "ruleSet": {
                    "appliedDisjunctively": false,
                    "rules": []
                }
            }
        }),
    ));

    assert_eq!(
        update.body["data"]["collectionUpdate"],
        json!({
            "collection": null,
            "job": null,
            "userErrors": [{
                "field": ["ruleSet", "rules"],
                "message": "Rules cannot be an empty set"
            }]
        })
    );
    assert_eq!(state_snapshot(&proxy), state_after_create);
}

#[test]
fn collection_update_returns_selected_job_on_success() {
    let mut proxy = snapshot_proxy();

    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateSmartCollection($input: CollectionInput!) {
          collectionCreate(input: $input) {
            collection { id title ruleSet { rules { column relation condition } } }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "input": {
                "title": "Smart Collection",
                "ruleSet": {
                    "appliedDisjunctively": false,
                    "rules": [{ "column": "TITLE", "relation": "CONTAINS", "condition": "Shoes" }]
                }
            }
        }),
    ));
    assert_eq!(
        create.body["data"]["collectionCreate"]["userErrors"],
        json!([])
    );
    let collection_id = create.body["data"]["collectionCreate"]["collection"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let update = proxy.process_request(json_graphql_request(
        r#"
        mutation UpdateCollectionWithJob($input: CollectionInput!) {
          collectionUpdate(input: $input) {
            collection { id title ruleSet { rules { column relation condition } } }
            job { id done }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "input": {
                "id": collection_id,
                "ruleSet": {
                    "appliedDisjunctively": false,
                    "rules": [{ "column": "TITLE", "relation": "CONTAINS", "condition": "Boots" }]
                }
            }
        }),
    ));

    assert_eq!(
        update.body["data"]["collectionUpdate"]["collection"]["ruleSet"]["rules"][0]["condition"],
        json!("Boots")
    );
    assert_eq!(
        update.body["data"]["collectionUpdate"]["userErrors"],
        json!([])
    );
    let job = &update.body["data"]["collectionUpdate"]["job"];
    assert!(job["id"]
        .as_str()
        .is_some_and(|id| id.starts_with("gid://shopify/Job/")));
    assert_eq!(job["done"], json!(false));
}

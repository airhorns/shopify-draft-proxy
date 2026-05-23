use super::common::*;
use pretty_assertions::assert_eq;

#[test]
fn product_create_preserves_parity_fields_and_downstream_read() {
    let mut proxy = snapshot_proxy();
    let create_query = r#"
        mutation ProductCreateParityPlan($product: ProductCreateInput!) {
          productCreate(product: $product) {
            product {
              id
              title
              handle
              status
              vendor
              productType
              tags
              descriptionHtml
              templateSuffix
              seo { title description }
            }
            userErrors { field message }
          }
        }
    "#;
    let variables = json!({
        "product": {
            "title": "Hermes Product Conformance 1776299742511",
            "status": "DRAFT",
            "vendor": "HERMES",
            "productType": "ACCESSORIES",
            "tags": ["conformance", "product-mutation", "1776299742511"],
            "descriptionHtml": "<p>Hermes product mutation conformance 1776299742511</p>",
            "templateSuffix": "product-mutation-parity",
            "seo": {
                "title": "Hermes Product 1776299742511",
                "description": "Hermes product mutation conformance 1776299742511"
            }
        }
    });

    let create = proxy.process_request(json_graphql_request(create_query, variables));
    let id = create.body["data"]["productCreate"]["product"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    assert_eq!(
        create.body["data"]["productCreate"]["product"],
        json!({
            "id": id,
            "title": "Hermes Product Conformance 1776299742511",
            "handle": "hermes-product-conformance-1776299742511",
            "status": "DRAFT",
            "vendor": "HERMES",
            "productType": "ACCESSORIES",
            "tags": ["1776299742511", "conformance", "product-mutation"],
            "descriptionHtml": "<p>Hermes product mutation conformance 1776299742511</p>",
            "templateSuffix": "product-mutation-parity",
            "seo": {
                "title": "Hermes Product 1776299742511",
                "description": "Hermes product mutation conformance 1776299742511"
            }
        })
    );

    let read_query = r#"
        query ProductCreateDownstreamRead($id: ID!) {
          product(id: $id) {
            id
            title
            handle
            status
            vendor
            productType
            tags
            descriptionHtml
            templateSuffix
            seo { title description }
          }
        }
    "#;
    let read = proxy.process_request(json_graphql_request(read_query, json!({ "id": id })));
    assert_eq!(
        read.body["data"]["product"],
        create.body["data"]["productCreate"]["product"]
    );
}

#[test]
fn supported_mutation_projection_includes_fragment_alias_selections() {
    let mut proxy = snapshot_proxy();

    let response = proxy.process_request(json_graphql_request(
        r#"
            mutation CreateWithFragments($product: ProductCreateInput!) {
              createAlias: productCreate(product: $product) {
                ...PayloadFields
              }
            }

            fragment PayloadFields on ProductCreatePayload {
              madeProduct: product {
                ...ProductFields
              }
              problems: userErrors {
                field
                message
              }
            }

            fragment ProductFields on Product {
              productId: id
              productTitle: title
              productHandle: handle
            }
        "#,
        json!({ "product": { "title": "Fragment alias product" } }),
    ));

    assert_eq!(response.status, 200);
    assert_eq!(
        response.body["data"]["createAlias"]["madeProduct"]["productTitle"],
        json!("Fragment alias product")
    );
    assert!(
        response.body["data"]["createAlias"]["madeProduct"]["productId"]
            .as_str()
            .is_some_and(|id| id.starts_with("gid://shopify/Product/"))
    );
    assert_eq!(
        response.body["data"]["createAlias"]["madeProduct"]["productHandle"],
        json!("fragment-alias-product")
    );
    assert_eq!(response.body["data"]["createAlias"]["problems"], json!([]));
}

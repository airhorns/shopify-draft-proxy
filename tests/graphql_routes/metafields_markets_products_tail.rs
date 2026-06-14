use super::common::*;
use pretty_assertions::assert_eq;

#[test]
fn generic_product_domain_metafields_set_delete_stage_for_natural_operation_names() {
    let mut proxy = configured_proxy(
        ReadMode::Snapshot,
        Some(shopify_draft_proxy::proxy::UnsupportedMutationMode::Reject),
    );

    let set = proxy.process_request(json_graphql_request(
        r#"
        mutation NaturalOwnerMetafieldsSet($metafields: [MetafieldsSetInput!]!) {
          metafieldsSet(metafields: $metafields) {
            metafields {
              id
              namespace
              key
              type
              value
              jsonValue
              compareDigest
              ownerType
              owner { __typename ... on Product { id } ... on ProductVariant { id } ... on Collection { id } }
            }
            userErrors { field message code elementIndex }
          }
        }
        "#,
        json!({"metafields": [
            {"ownerId": "gid://shopify/Product/987654321", "namespace": "custom", "key": "material", "type": "single_line_text_field", "value": "Wool"},
            {"ownerId": "gid://shopify/ProductVariant/987654322", "namespace": "custom", "key": "variant_care", "type": "single_line_text_field", "value": "Spot clean"},
            {"ownerId": "gid://shopify/Collection/987654323", "namespace": "custom", "key": "collection_season", "type": "single_line_text_field", "value": "Winter"}
        ]}),
    ));
    assert_eq!(set.status, 200);
    assert_eq!(set.body["data"]["metafieldsSet"]["userErrors"], json!([]));
    assert_eq!(
        set.body["data"]["metafieldsSet"]["metafields"][0]["owner"]["__typename"],
        json!("Product")
    );
    assert_eq!(
        set.body["data"]["metafieldsSet"]["metafields"][1]["ownerType"],
        json!("PRODUCTVARIANT")
    );
    assert_eq!(
        set.body["data"]["metafieldsSet"]["metafields"][2]["ownerType"],
        json!("COLLECTION")
    );

    let read = proxy.process_request(json_graphql_request(
        r#"
        query NaturalOwnerMetafieldsRead($productId: ID!, $variantId: ID!, $collectionId: ID!) {
          product(id: $productId) {
            id
            material: metafield(namespace: "custom", key: "material") { key value ownerType }
            metafields(first: 10, namespace: "custom") { nodes { key value ownerType } }
          }
          productVariant(id: $variantId) {
            id
            care: metafield(namespace: "custom", key: "variant_care") { key value ownerType }
            metafields(first: 10, namespace: "custom") { nodes { key value ownerType } }
          }
          collection(id: $collectionId) {
            id
            season: metafield(namespace: "custom", key: "collection_season") { key value ownerType }
            metafields(first: 10, namespace: "custom") { nodes { key value ownerType } }
          }
        }
        "#,
        json!({
            "productId": "gid://shopify/Product/987654321",
            "variantId": "gid://shopify/ProductVariant/987654322",
            "collectionId": "gid://shopify/Collection/987654323"
        }),
    ));
    assert_eq!(read.status, 200);
    assert_eq!(
        read.body["data"]["product"]["material"]["value"],
        json!("Wool")
    );
    assert_eq!(
        read.body["data"]["productVariant"]["care"]["value"],
        json!("Spot clean")
    );
    assert_eq!(
        read.body["data"]["collection"]["season"]["value"],
        json!("Winter")
    );

    let delete = proxy.process_request(json_graphql_request(
        r#"
        mutation NaturalOwnerMetafieldsDelete($metafields: [MetafieldIdentifierInput!]!) {
          metafieldsDelete(metafields: $metafields) {
            deletedMetafields { ownerId namespace key }
            userErrors { field message }
          }
        }
        "#,
        json!({"metafields": [
            {"ownerId": "gid://shopify/Product/987654321", "namespace": "custom", "key": "material"},
            {"ownerId": "gid://shopify/ProductVariant/987654322", "namespace": "custom", "key": "variant_care"},
            {"ownerId": "gid://shopify/Collection/987654323", "namespace": "custom", "key": "collection_season"},
            {"ownerId": "gid://shopify/Product/987654321", "namespace": "custom", "key": "missing"}
        ]}),
    ));
    assert_eq!(delete.status, 200);
    assert_eq!(
        delete.body["data"]["metafieldsDelete"]["userErrors"],
        json!([])
    );
    assert_eq!(
        delete.body["data"]["metafieldsDelete"]["deletedMetafields"][0],
        json!({"ownerId": "gid://shopify/Product/987654321", "namespace": "custom", "key": "material"})
    );
    assert_eq!(
        delete.body["data"]["metafieldsDelete"]["deletedMetafields"][3],
        Value::Null
    );

    let post_delete = proxy.process_request(json_graphql_request(
        r#"
        query NaturalOwnerMetafieldsPostDelete($productId: ID!, $variantId: ID!, $collectionId: ID!) {
          product(id: $productId) { material: metafield(namespace: "custom", key: "material") { value } metafields(first: 10, namespace: "custom") { nodes { key } } }
          productVariant(id: $variantId) { care: metafield(namespace: "custom", key: "variant_care") { value } metafields(first: 10, namespace: "custom") { nodes { key } } }
          collection(id: $collectionId) { season: metafield(namespace: "custom", key: "collection_season") { value } metafields(first: 10, namespace: "custom") { nodes { key } } }
        }
        "#,
        json!({
            "productId": "gid://shopify/Product/987654321",
            "variantId": "gid://shopify/ProductVariant/987654322",
            "collectionId": "gid://shopify/Collection/987654323"
        }),
    ));
    assert_eq!(post_delete.body["data"]["product"]["material"], Value::Null);
    assert_eq!(
        post_delete.body["data"]["productVariant"]["care"],
        Value::Null
    );
    assert_eq!(
        post_delete.body["data"]["collection"]["season"],
        Value::Null
    );

    let log = proxy.process_request(Request {
        method: "GET".to_string(),
        path: "/__meta/log".to_string(),
        headers: Default::default(),
        body: String::new(),
    });
    assert_eq!(log.body["entries"].as_array().unwrap().len(), 2);
    assert!(log.body["entries"][0]["rawBody"]
        .as_str()
        .unwrap()
        .contains("NaturalOwnerMetafieldsSet"));
    assert!(log.body["entries"][1]["rawBody"]
        .as_str()
        .unwrap()
        .contains("NaturalOwnerMetafieldsDelete"));
}

#[test]
fn generic_product_domain_metafields_set_validates_cas_and_atomicity() {
    let mut proxy = configured_proxy(
        ReadMode::Snapshot,
        Some(shopify_draft_proxy::proxy::UnsupportedMutationMode::Reject),
    );
    let owner_id = "gid://shopify/Product/987654399";

    let initial = proxy.process_request(json_graphql_request(
        r#"
        mutation NaturalCasMetafieldsSet($metafields: [MetafieldsSetInput!]!) {
          metafieldsSet(metafields: $metafields) {
            metafields { namespace key value compareDigest }
            userErrors { field message code elementIndex }
          }
        }
        "#,
        json!({"metafields": [{"ownerId": owner_id, "namespace": "custom", "key": "material", "type": "single_line_text_field", "value": "Wool"}]}),
    ));
    assert_eq!(
        initial.body["data"]["metafieldsSet"]["userErrors"],
        json!([])
    );
    let digest = initial.body["data"]["metafieldsSet"]["metafields"][0]["compareDigest"]
        .as_str()
        .unwrap()
        .to_string();

    let rejected = proxy.process_request(json_graphql_request(
        r#"
        mutation NaturalCasMetafieldsSet($metafields: [MetafieldsSetInput!]!) {
          metafieldsSet(metafields: $metafields) {
            metafields { namespace key value compareDigest }
            userErrors { field message code elementIndex }
          }
        }
        "#,
        json!({"metafields": [
            {"ownerId": owner_id, "namespace": "custom", "key": "material", "type": "single_line_text_field", "value": "Linen", "compareDigest": "stale"},
            {"ownerId": owner_id, "namespace": "custom", "key": "flag", "type": "boolean", "value": "yes"}
        ]}),
    ));
    assert_eq!(rejected.status, 200);
    assert_eq!(
        rejected.body["data"]["metafieldsSet"]["metafields"],
        json!([])
    );
    assert_eq!(
        rejected.body["data"]["metafieldsSet"]["userErrors"][0]["code"],
        json!("STALE_OBJECT")
    );
    assert_eq!(
        rejected.body["data"]["metafieldsSet"]["userErrors"][1]["message"],
        json!("Value must be true or false.")
    );

    let after_reject = proxy.process_request(json_graphql_request(
        r#"
        query NaturalCasMetafieldsRead($id: ID!) {
          product(id: $id) { material: metafield(namespace: "custom", key: "material") { value compareDigest } flag: metafield(namespace: "custom", key: "flag") { value } }
        }
        "#,
        json!({"id": owner_id}),
    ));
    assert_eq!(
        after_reject.body["data"]["product"]["material"]["value"],
        json!("Wool")
    );
    assert_eq!(after_reject.body["data"]["product"]["flag"], Value::Null);

    let accepted = proxy.process_request(json_graphql_request(
        r#"
        mutation NaturalCasMetafieldsSet($metafields: [MetafieldsSetInput!]!) {
          metafieldsSet(metafields: $metafields) {
            metafields { namespace key value compareDigest }
            userErrors { field message code elementIndex }
          }
        }
        "#,
        json!({"metafields": [{"ownerId": owner_id, "namespace": "custom", "key": "material", "type": "single_line_text_field", "value": "Cotton", "compareDigest": digest}]}),
    ));
    assert_eq!(
        accepted.body["data"]["metafieldsSet"]["userErrors"],
        json!([])
    );
    assert_eq!(
        accepted.body["data"]["metafieldsSet"]["metafields"][0]["value"],
        json!("Cotton")
    );
}

#[test]
fn metafields_set_stages_owner_metafield_connections_for_product_and_customer_reads() {
    let mut proxy = snapshot_proxy();

    let product_set = proxy.process_request(json_graphql_request(
        r#"
        mutation MetafieldDefinitionLifecycleMetafieldsSet($metafields: [MetafieldsSetInput!]!) {
          metafieldsSet(metafields: $metafields) {
            metafields { id namespace key type value jsonValue compareDigest createdAt updatedAt ownerType }
            userErrors { field message code }
          }
        }
        "#,
        json!({"metafields": [
            {"ownerId": "gid://shopify/Product/10173071262002", "namespace": "har294_test", "key": "boolean", "type": "boolean", "value": "true"},
            {"ownerId": "gid://shopify/Product/10173071262002", "namespace": "har294_test", "key": "json", "type": "json", "value": "{\"ingredient\":\"flour\",\"amount\":0.3}"},
            {"ownerId": "gid://shopify/Product/10173071262002", "namespace": "har294_test", "key": "number_decimal", "type": "number_decimal", "value": "10.4"}
        ]}),
    ));
    assert_eq!(
        product_set.body["data"]["metafieldsSet"]["userErrors"],
        json!([])
    );
    assert_eq!(
        product_set.body["data"]["metafieldsSet"]["metafields"][0]["jsonValue"],
        json!(true)
    );
    assert_eq!(
        product_set.body["data"]["metafieldsSet"]["metafields"][1]["jsonValue"],
        json!({"ingredient": "flour", "amount": 0.3})
    );
    assert_eq!(
        product_set.body["data"]["metafieldsSet"]["metafields"][2]["jsonValue"],
        json!("10.4")
    );

    let product_read = proxy.process_request(json_graphql_request(
        r#"
        query CustomDataMetafieldTypeMatrixRead($id: ID!, $namespace: String!) {
          product(id: $id) { id metafields(first: 100, namespace: $namespace) { nodes { id namespace key type value jsonValue compareDigest createdAt updatedAt ownerType } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } }
        }
        "#,
        json!({"id": "gid://shopify/Product/10173071262002", "namespace": "har294_test"}),
    ));
    assert_eq!(
        product_read.body["data"]["product"]["id"],
        json!("gid://shopify/Product/10173071262002")
    );
    assert_eq!(
        product_read.body["data"]["product"]["metafields"]["nodes"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
    assert_eq!(
        product_read.body["data"]["product"]["metafields"]["nodes"][1]["key"],
        json!("json")
    );

    let customer_set = proxy.process_request(json_graphql_request(
        r#"
        mutation MetafieldDefinitionNonProductMetafieldsSet($metafields: [MetafieldsSetInput!]!) {
          metafieldsSet(metafields: $metafields) { metafields { id namespace key type value } userErrors { field message code } }
        }
        "#,
        json!({"metafields": [{"ownerId": "gid://shopify/Customer/1", "namespace": "har691_value_customer_mosma2dg", "key": "value", "type": "single_line_text_field", "value": "CUSTOMER metafieldsSet value"}]}),
    ));
    assert_eq!(
        customer_set.body["data"]["metafieldsSet"]["userErrors"],
        json!([])
    );

    let customer_read = proxy.process_request(json_graphql_request(
        r#"
        query MetafieldDefinitionNonProductCustomerMetafieldsRead($id: ID!, $namespace: String!, $key: String!) {
          customer(id: $id) { id metafield(namespace: $namespace, key: $key) { id namespace key type value } metafields(first: 10, namespace: $namespace) { nodes { id namespace key type value } } }
        }
        "#,
        json!({"id": "gid://shopify/Customer/1", "namespace": "har691_value_customer_mosma2dg", "key": "value"}),
    ));
    assert_eq!(
        customer_read.body["data"]["customer"]["metafield"]["value"],
        json!("CUSTOMER metafieldsSet value")
    );
    assert_eq!(
        customer_read.body["data"]["customer"]["metafields"]["nodes"][0]["key"],
        json!("value")
    );
}

#[test]
fn owner_scoped_metafields_do_not_leak_between_products() {
    let mut proxy = snapshot_proxy();

    let owner_with_metafields = "gid://shopify/Product/10173071262002";
    let owner_without_metafields = "gid://shopify/Product/10173071262003";
    let namespace = "owner_scope_isolation";

    let set = proxy.process_request(json_graphql_request(
        r#"
        mutation MetafieldDefinitionLifecycleMetafieldsSet($metafields: [MetafieldsSetInput!]!) {
          metafieldsSet(metafields: $metafields) {
            metafields { id namespace key type value owner { id } }
            userErrors { field message code }
          }
        }
        "#,
        json!({"metafields": [{
            "ownerId": owner_with_metafields,
            "namespace": namespace,
            "key": "tier",
            "type": "single_line_text_field",
            "value": "gold"
        }]}),
    ));
    assert_eq!(set.status, 200);
    assert_eq!(set.body["data"]["metafieldsSet"]["userErrors"], json!([]));

    let read_empty_owner = proxy.process_request(json_graphql_request(
        r#"
        query MetafieldDefinitionLifecycleReadProductMetafield($id: ID!, $namespace: String!, $key: String!) {
          product(id: $id) {
            id
            metafield(namespace: $namespace, key: $key) { id namespace key type value owner { id } }
            metafields(first: 10, namespace: $namespace) {
              nodes { id namespace key type value owner { id } }
              edges { cursor node { id namespace key type value owner { id } } }
              pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
            }
          }
        }
        "#,
        json!({"id": owner_without_metafields, "namespace": namespace, "key": "tier"}),
    ));
    assert_eq!(read_empty_owner.status, 200);
    assert_eq!(
        read_empty_owner.body["data"]["product"],
        json!({
            "id": owner_without_metafields,
            "metafield": null,
            "metafields": {
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
    );

    let read_populated_owner = proxy.process_request(json_graphql_request(
        r#"
        query MetafieldDefinitionLifecycleReadProductMetafield($id: ID!, $namespace: String!, $key: String!) {
          product(id: $id) {
            id
            metafield(namespace: $namespace, key: $key) { value owner { id } }
            metafields(first: 10, namespace: $namespace) { nodes { key value owner { id } } }
          }
        }
        "#,
        json!({"id": owner_with_metafields, "namespace": namespace, "key": "tier"}),
    ));
    assert_eq!(
        read_populated_owner.body["data"]["product"]["metafield"],
        json!({"value": "gold", "owner": {"id": owner_with_metafields}})
    );
    assert_eq!(
        read_populated_owner.body["data"]["product"]["metafields"]["nodes"],
        json!([{"key": "tier", "value": "gold", "owner": {"id": owner_with_metafields}}])
    );
}

#[test]
fn metafields_app_namespace_set_delete_stages_product_readback() {
    let mut proxy = snapshot_proxy();

    let set_canonical = proxy.process_request(json_graphql_request(
        r#"
        mutation MetafieldsSetAppNamespaceResolution($metafields: [MetafieldsSetInput!]!) {
          metafieldsSet(metafields: $metafields) { metafields { id namespace key type value } userErrors { field message code elementIndex } }
        }
        "#,
        json!({"metafields": [{"ownerId": "gid://shopify/Product/10180596236594", "namespace": "$app:value_namespace_mowuw5ai", "key": "tier", "type": "single_line_text_field", "value": "gold"}]}),
    ));
    assert_eq!(
        set_canonical.body["data"]["metafieldsSet"]["metafields"][0]["namespace"],
        json!("app--347082227713--value_namespace_mowuw5ai")
    );

    let read_after_canonical = proxy.process_request(json_graphql_request(
        r#"
        query MetafieldsAppNamespaceProductRead($productId: ID!, $canonicalNamespace: String!, $defaultNamespace: String!, $key: String!, $defaultKey: String!) {
          product(id: $productId) {
            id
            canonical: metafield(namespace: $canonicalNamespace, key: $key) { id namespace key type value }
            defaulted: metafield(namespace: $defaultNamespace, key: $defaultKey) { id namespace key type value }
          }
        }
        "#,
        json!({"productId": "gid://shopify/Product/10180596236594", "canonicalNamespace": "app--347082227713--value_namespace_mowuw5ai", "defaultNamespace": "app--347082227713", "key": "tier", "defaultKey": "default_mowuw5ai"}),
    ));
    assert_eq!(
        read_after_canonical.body["data"]["product"],
        json!({
            "id": "gid://shopify/Product/10180596236594",
            "canonical": {"id": "gid://shopify/Metafield/1", "namespace": "app--347082227713--value_namespace_mowuw5ai", "key": "tier", "type": "single_line_text_field", "value": "gold"},
            "defaulted": null
        })
    );

    let set_default = proxy.process_request(json_graphql_request(
        r#"
        mutation MetafieldsSetAppNamespaceResolution($metafields: [MetafieldsSetInput!]!) {
          metafieldsSet(metafields: $metafields) { metafields { id namespace key type value } userErrors { field message code elementIndex } }
        }
        "#,
        json!({"metafields": [{"ownerId": "gid://shopify/Product/10180596236594", "key": "default_mowuw5ai", "type": "single_line_text_field", "value": "silver"}]}),
    ));
    assert_eq!(
        set_default.body["data"]["metafieldsSet"]["metafields"][0]["namespace"],
        json!("app--347082227713")
    );

    let delete_canonical = proxy.process_request(json_graphql_request(
        r#"
        mutation MetafieldsDeleteAppNamespaceResolution($metafields: [MetafieldIdentifierInput!]!) {
          metafieldsDelete(metafields: $metafields) { deletedMetafields { ownerId namespace key } userErrors { field message } }
        }
        "#,
        json!({"metafields": [{"ownerId": "gid://shopify/Product/10180596236594", "namespace": "$app:value_namespace_mowuw5ai", "key": "tier"}]}),
    ));
    assert_eq!(
        delete_canonical.body["data"]["metafieldsDelete"],
        json!({"deletedMetafields": [{"ownerId": "gid://shopify/Product/10180596236594", "namespace": "app--347082227713--value_namespace_mowuw5ai", "key": "tier"}], "userErrors": []})
    );

    let post_delete = proxy.process_request(json_graphql_request(
        r#"
        query MetafieldsAppNamespaceProductRead($productId: ID!, $canonicalNamespace: String!, $defaultNamespace: String!, $key: String!, $defaultKey: String!) {
          product(id: $productId) {
            id
            canonical: metafield(namespace: $canonicalNamespace, key: $key) { id namespace key type value }
            defaulted: metafield(namespace: $defaultNamespace, key: $defaultKey) { id namespace key type value }
          }
        }
        "#,
        json!({"productId": "gid://shopify/Product/10180596236594", "canonicalNamespace": "app--347082227713--value_namespace_mowuw5ai", "defaultNamespace": "app--347082227713", "key": "tier", "defaultKey": "default_mowuw5ai"}),
    ));
    assert_eq!(
        post_delete.body["data"]["product"],
        json!({
            "id": "gid://shopify/Product/10180596236594",
            "canonical": null,
            "defaulted": {"id": "gid://shopify/Metafield/2", "namespace": "app--347082227713", "key": "default_mowuw5ai", "type": "single_line_text_field", "value": "silver"}
        })
    );
}

#[test]
fn markets_quantity_pricing_and_web_presence_local_staging_match_captured_shapes() {
    let mut proxy = snapshot_proxy();

    let unknown_price_list = proxy.process_request(json_graphql_request(
        r#"
        mutation QuantityPricingByVariantUpdate($priceListId: ID!, $input: QuantityPricingByVariantUpdateInput!) {
          quantityPricingByVariantUpdate(priceListId: $priceListId, input: $input) {
            productVariants { id }
            userErrors { __typename field code message }
          }
        }
        "#,
        json!({
            "priceListId": "gid://shopify/PriceList/0",
            "input": {
                "pricesToAdd": [{"variantId": "gid://shopify/ProductVariant/49875425296690", "price": {"amount": "12.00", "currencyCode": "CAD"}}],
                "pricesToDeleteByVariantId": [],
                "quantityRulesToAdd": [],
                "quantityRulesToDeleteByVariantId": [],
                "quantityPriceBreaksToAdd": [],
                "quantityPriceBreaksToDelete": [],
                "quantityPriceBreaksToDeleteByVariantId": []
            }
        }),
    ));
    assert_eq!(
        unknown_price_list.body["data"]["quantityPricingByVariantUpdate"]["productVariants"],
        Value::Null
    );
    assert_eq!(
        unknown_price_list.body["data"]["quantityPricingByVariantUpdate"]["userErrors"][0],
        json!({"__typename": "QuantityPricingByVariantUserError", "field": ["priceListId"], "code": "PRICE_LIST_NOT_FOUND", "message": "Price list not found."})
    );

    let duplicate_delete = proxy.process_request(json_graphql_request(
        r#"
        mutation QuantityPricingByVariantUpdate($priceListId: ID!, $input: QuantityPricingByVariantUpdateInput!) {
          quantityPricingByVariantUpdate(priceListId: $priceListId, input: $input) { productVariants { id } userErrors { __typename field code message } }
        }
        "#,
        json!({
            "priceListId": "gid://shopify/PriceList/31575376178",
            "input": {
                "pricesToAdd": [],
                "pricesToDeleteByVariantId": ["gid://shopify/ProductVariant/49875425296690", "gid://shopify/ProductVariant/49875425296690"],
                "quantityRulesToAdd": [],
                "quantityRulesToDeleteByVariantId": [],
                "quantityPriceBreaksToAdd": [],
                "quantityPriceBreaksToDelete": [],
                "quantityPriceBreaksToDeleteByVariantId": []
            }
        }),
    ));
    assert_eq!(
        duplicate_delete.body["data"]["quantityPricingByVariantUpdate"],
        json!({"productVariants": [{"id": "gid://shopify/ProductVariant/49875425296690"}], "userErrors": []})
    );

    let cleanup = proxy.process_request(json_graphql_request(
        r#"
        mutation QuantityRulesDelete($priceListId: ID!, $variantIds: [ID!]!) {
          quantityRulesDelete(priceListId: $priceListId, variantIds: $variantIds) { deletedQuantityRulesVariantIds userErrors { field code message } }
        }
        "#,
        json!({"priceListId": "gid://shopify/PriceList/32128106802", "variantIds": ["gid://shopify/ProductVariant/49875425296690"]}),
    ));
    assert_eq!(
        cleanup.body["data"]["quantityRulesDelete"],
        json!({"deletedQuantityRulesVariantIds": ["gid://shopify/ProductVariant/49875425296690"], "userErrors": []})
    );

    let unknown_variant = proxy.process_request(json_graphql_request(
        r#"
        mutation QuantityRulesAdd($priceListId: ID!, $quantityRules: [QuantityRuleInput!]!) {
          quantityRulesAdd(priceListId: $priceListId, quantityRules: $quantityRules) { quantityRules { minimum maximum increment productVariant { id } } userErrors { field code message } }
        }
        "#,
        json!({"priceListId": "gid://shopify/PriceList/32128106802", "quantityRules": [{"variantId": "gid://shopify/ProductVariant/0", "minimum": 2, "maximum": 10, "increment": 2}]}),
    ));
    assert_eq!(
        unknown_variant.body["data"]["quantityRulesAdd"]["quantityRules"],
        json!([])
    );
    assert_eq!(
        unknown_variant.body["data"]["quantityRulesAdd"]["userErrors"][0],
        json!({"field": ["quantityRules", "0", "variantId"], "code": "PRODUCT_VARIANT_DOES_NOT_EXIST", "message": "Product variant ID does not exist."})
    );

    let unknown_quantity_rules_price_list = proxy.process_request(json_graphql_request(
        r#"
        mutation QuantityRulesAdd($priceListId: ID!, $quantityRules: [QuantityRuleInput!]!) {
          quantityRulesAdd(priceListId: $priceListId, quantityRules: $quantityRules) { quantityRules { minimum maximum increment productVariant { id } } userErrors { __typename field code message } }
        }
        "#,
        json!({"priceListId": "gid://shopify/PriceList/999", "quantityRules": [{"variantId": "gid://shopify/ProductVariant/49875425296690", "minimum": 2, "maximum": 10, "increment": 2}]}),
    ));
    assert_eq!(
        unknown_quantity_rules_price_list.body["data"]["quantityRulesAdd"],
        json!({"quantityRules": [], "userErrors": [{"__typename": "QuantityRuleUserError", "field": ["priceListId"], "message": "Price list does not exist.", "code": "PRICE_LIST_DOES_NOT_EXIST"}]})
    );

    let valid_quantity_rules_add = proxy.process_request(json_graphql_request(
        r#"
        mutation QuantityRulesAdd($priceListId: ID!, $quantityRules: [QuantityRuleInput!]!) {
          quantityRulesAdd(priceListId: $priceListId, quantityRules: $quantityRules) { quantityRules { minimum maximum increment productVariant { id } } userErrors { __typename field code message } }
        }
        "#,
        json!({"priceListId": "gid://shopify/PriceList/32128106802", "quantityRules": [{"variantId": "gid://shopify/ProductVariant/49875425296690", "minimum": 2, "maximum": 10, "increment": 2}]}),
    ));
    assert_eq!(
        valid_quantity_rules_add.body["data"]["quantityRulesAdd"],
        json!({"quantityRules": [{"minimum": 2, "maximum": 10, "increment": 2, "productVariant": {"id": "gid://shopify/ProductVariant/49875425296690"}}], "userErrors": []})
    );

    let invalid_quantity_rule_cases = [
        (
            json!([{"variantId": "gid://shopify/ProductVariant/49875425296690", "minimum": 0, "maximum": 10, "increment": 1}]),
            json!([
                {"__typename": "QuantityRuleUserError", "field": ["quantityRules", "0", "minimum"], "message": "Minimum must be greater than or equal to one.", "code": "GREATER_THAN_OR_EQUAL_TO"},
                {"__typename": "QuantityRuleUserError", "field": ["quantityRules", "0", "increment"], "message": "Increment must be lower than or equal to the minimum.", "code": "INCREMENT_IS_GREATER_THAN_MINIMUM"}
            ]),
        ),
        (
            json!([{"variantId": "gid://shopify/ProductVariant/49875425296690", "minimum": 1, "maximum": 10, "increment": 0}]),
            json!([{ "__typename": "QuantityRuleUserError", "field": ["quantityRules", "0", "increment"], "message": "Increment must be greater than or equal to one.", "code": "GREATER_THAN_OR_EQUAL_TO" }]),
        ),
        (
            json!([{"variantId": "gid://shopify/ProductVariant/49875425296690", "minimum": 10, "maximum": 5, "increment": 1}]),
            json!([{ "__typename": "QuantityRuleUserError", "field": ["quantityRules", "0", "minimum"], "message": "Minimum must be lower than or equal to the maximum.", "code": "MINIMUM_IS_GREATER_THAN_MAXIMUM" }]),
        ),
        (
            json!([{"variantId": "gid://shopify/ProductVariant/49875425296690", "minimum": 5, "maximum": 12, "increment": 3}]),
            json!([{ "__typename": "QuantityRuleUserError", "field": ["quantityRules", "0", "minimum"], "message": "Minimum must be a multiple of the increment.", "code": "MINIMUM_NOT_MULTIPLE_OF_INCREMENT" }]),
        ),
        (
            json!([{"variantId": "gid://shopify/ProductVariant/49875425296690", "minimum": 6, "maximum": 10, "increment": 3}]),
            json!([{ "__typename": "QuantityRuleUserError", "field": ["quantityRules", "0", "maximum"], "message": "Maximum must be a multiple of the increment.", "code": "MAXIMUM_NOT_MULTIPLE_OF_INCREMENT" }]),
        ),
        (
            json!([
                {"variantId": "gid://shopify/ProductVariant/49875425296690", "minimum": 2, "maximum": 10, "increment": 2},
                {"variantId": "gid://shopify/ProductVariant/49875425296690", "minimum": 4, "maximum": 12, "increment": 2}
            ]),
            json!([
                {"__typename": "QuantityRuleUserError", "field": ["quantityRules", "0", "variantId"], "message": "Quantity rule inputs must be unique by variant id.", "code": "DUPLICATE_INPUT_FOR_VARIANT"},
                {"__typename": "QuantityRuleUserError", "field": ["quantityRules", "1", "variantId"], "message": "Quantity rule inputs must be unique by variant id.", "code": "DUPLICATE_INPUT_FOR_VARIANT"}
            ]),
        ),
    ];
    for (quantity_rules, user_errors) in invalid_quantity_rule_cases {
        let invalid = proxy.process_request(json_graphql_request(
            r#"
            mutation QuantityRulesAddValidation($priceListId: ID!, $quantityRules: [QuantityRuleInput!]!) {
              quantityRulesAdd(priceListId: $priceListId, quantityRules: $quantityRules) {
                quantityRules { minimum maximum increment productVariant { id } }
                userErrors { __typename field message code }
              }
            }
            "#,
            json!({"priceListId": "gid://shopify/PriceList/31575376178", "quantityRules": quantity_rules}),
        ));
        assert_eq!(
            invalid.body["data"]["quantityRulesAdd"],
            json!({"quantityRules": [], "userErrors": user_errors})
        );
    }

    let fr_ca = proxy.process_request(json_graphql_request(
        r#"
        mutation MarketWebPresenceLifecycleCreate($input: WebPresenceCreateInput!) {
          webPresenceCreate(input: $input) { webPresence { id subfolderSuffix rootUrls { locale url } defaultLocale { locale name primary published } alternateLocales { locale name primary published } markets(first: 5) { nodes { id name handle status type } } } userErrors { field message code } }
        }
        "#,
        json!({"input": {"defaultLocale": "fr-CA", "alternateLocales": [], "subfolderSuffix": "fr"}}),
    ));
    assert_eq!(
        fr_ca.body["data"]["webPresenceCreate"]["webPresence"]["defaultLocale"]["locale"],
        json!("fr-CA")
    );
    assert_eq!(
        fr_ca.body["data"]["webPresenceCreate"]["userErrors"],
        json!([])
    );

    let multi = proxy.process_request(json_graphql_request(
        r#"
        mutation MarketWebPresenceLifecycleCreate($input: WebPresenceCreateInput!) {
          webPresenceCreate(input: $input) { webPresence { id subfolderSuffix domain { id host url sslEnabled } rootUrls { locale url } defaultLocale { locale name primary published } alternateLocales { locale name primary published } markets(first: 5) { nodes { id name handle status type } } } userErrors { field message code } }
        }
        "#,
        json!({"input": {"defaultLocale": "en", "alternateLocales": ["fr", "de"], "subfolderSuffix": "intl"}}),
    ));
    assert_eq!(
        multi.body["data"]["webPresenceCreate"]["webPresence"]["rootUrls"],
        json!([
            {"locale": "en", "url": "https://harry-test-heelo.myshopify.com/intl/"},
            {"locale": "fr", "url": "https://harry-test-heelo.myshopify.com/intl/fr/"},
            {"locale": "de", "url": "https://harry-test-heelo.myshopify.com/intl/de/"}
        ])
    );
}

#[test]
fn market_web_presence_ported_gleam_helpers_stage_and_validate() {
    // Ports old Gleam web-presence helper behavior from markets_mutation_test.gleam:
    // root URL construction for subfolder/domain routing, Shopify locale normalization,
    // aggregate locale errors, subfolder validation ordering, create/update readback,
    // unknown-domain create guards, and taken-suffix/no-op update behavior.
    let create_query = r#"
        mutation RustMarketWebPresenceHelperLocalRuntimeCreate($input: WebPresenceCreateInput!) {
          webPresenceCreate(input: $input) {
            webPresence {
              id subfolderSuffix
              domain { id host url sslEnabled }
              rootUrls { locale url }
              defaultLocale { locale primary }
              alternateLocales { locale primary }
            }
            userErrors { __typename field message code }
          }
        }
    "#;
    let update_query = r#"
        mutation RustMarketWebPresenceHelperLocalRuntimeUpdate($id: ID!, $input: WebPresenceUpdateInput!) {
          webPresenceUpdate(id: $id, input: $input) {
            webPresence {
              id subfolderSuffix
              domain { id host url sslEnabled }
              rootUrls { locale url }
              defaultLocale { locale primary }
              alternateLocales { locale primary }
            }
            userErrors { __typename field message code }
          }
        }
    "#;
    let read_query = r#"
        query RustMarketWebPresenceHelperLocalRuntimeRead {
          webPresences(first: 10) {
            nodes {
              id subfolderSuffix
              domain { id host url sslEnabled }
              rootUrls { locale url }
              defaultLocale { locale primary }
              alternateLocales { locale primary }
            }
          }
        }
    "#;

    let mut proxy = snapshot_proxy();
    let subfolder = proxy.process_request(json_graphql_request(
        create_query,
        json!({"input": {"defaultLocale": "en", "alternateLocales": ["fr", "de"], "subfolderSuffix": "intl"}}),
    ));
    assert_eq!(
        subfolder.body["data"]["webPresenceCreate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        subfolder.body["data"]["webPresenceCreate"]["webPresence"]["domain"],
        Value::Null
    );
    assert_eq!(
        subfolder.body["data"]["webPresenceCreate"]["webPresence"]["rootUrls"],
        json!([
            {"locale": "en", "url": "https://acme.myshopify.com/intl/"},
            {"locale": "fr", "url": "https://acme.myshopify.com/intl/fr/"},
            {"locale": "de", "url": "https://acme.myshopify.com/intl/de/"}
        ])
    );

    let domain = proxy.process_request(json_graphql_request(
        create_query,
        json!({"input": {"defaultLocale": "en", "alternateLocales": ["fr"], "domainId": "gid://shopify/Domain/1000"}}),
    ));
    assert_eq!(
        domain.body["data"]["webPresenceCreate"]["webPresence"]["domain"],
        json!({"id": "gid://shopify/Domain/1000", "host": "acme.myshopify.com", "url": "https://acme.myshopify.com", "sslEnabled": true})
    );
    assert_eq!(
        domain.body["data"]["webPresenceCreate"]["webPresence"]["rootUrls"],
        json!([
            {"locale": "en", "url": "https://acme.myshopify.com/"},
            {"locale": "fr", "url": "https://acme.myshopify.com/fr/"}
        ])
    );
    assert_eq!(
        domain.body["data"]["webPresenceCreate"]["webPresence"]["subfolderSuffix"],
        Value::Null
    );

    let mut locale_proxy = snapshot_proxy();
    let normalized = locale_proxy.process_request(json_graphql_request(
        create_query,
        json!({"input": {"defaultLocale": "EN-us", "alternateLocales": ["ZH-hant-tw", "pt-br"], "subfolderSuffix": "us"}}),
    ));
    assert_eq!(
        normalized.body["data"]["webPresenceCreate"]["webPresence"]["defaultLocale"],
        json!({"locale": "en-US", "primary": true})
    );
    assert_eq!(
        normalized.body["data"]["webPresenceCreate"]["webPresence"]["alternateLocales"],
        json!([
            {"locale": "zh-Hant-TW", "primary": false},
            {"locale": "pt-BR", "primary": false}
        ])
    );
    assert_eq!(
        normalized.body["data"]["webPresenceCreate"]["webPresence"]["rootUrls"],
        json!([
            {"locale": "en-US", "url": "https://acme.myshopify.com/us/"},
            {"locale": "zh-Hant-TW", "url": "https://acme.myshopify.com/us/zh-Hant-TW/"},
            {"locale": "pt-BR", "url": "https://acme.myshopify.com/us/pt-BR/"}
        ])
    );

    let invalid_locales = snapshot_proxy().process_request(json_graphql_request(
        create_query,
        json!({"input": {"defaultLocale": "fr-CA", "alternateLocales": ["fr", "zz", "pt-BR", "yy"], "subfolderSuffix": "fr"}}),
    ));
    assert_eq!(
        invalid_locales.body["data"]["webPresenceCreate"]["webPresence"],
        Value::Null
    );
    assert_eq!(
        invalid_locales.body["data"]["webPresenceCreate"]["userErrors"],
        json!([{"__typename": "MarketUserError", "field": ["input", "alternateLocales"], "message": "Invalid locale codes: zz, and yy", "code": "INVALID"}])
    );

    let validation_cases = [
        (
            json!({"defaultLocale": "en", "domainId": "gid://shopify/Domain/1000", "subfolderSuffix": "fr"}),
            json!([{ "__typename": "MarketUserError", "field": ["input"], "message": "Cannot have both a subfolder suffix and a domain.", "code": "CANNOT_HAVE_SUBFOLDER_AND_DOMAIN" }]),
        ),
        (
            json!({"defaultLocale": "en"}),
            json!([{ "__typename": "MarketUserError", "field": ["input"], "message": "Requires a domain or subfolder suffix.", "code": "REQUIRES_DOMAIN_OR_SUBFOLDER" }]),
        ),
        (
            json!({"defaultLocale": "en", "subfolderSuffix": "x"}),
            json!([{ "__typename": "MarketUserError", "field": ["input", "subfolderSuffix"], "message": "Subfolder suffix must be at least 2 letters", "code": "SUBFOLDER_SUFFIX_MUST_BE_AT_LEAST_2_LETTERS" }]),
        ),
        (
            json!({"defaultLocale": "en", "subfolderSuffix": "Latn"}),
            json!([{ "__typename": "MarketUserError", "field": ["input", "subfolderSuffix"], "message": "Subfolder suffix cannot be a script code", "code": "SUBFOLDER_SUFFIX_CANNOT_BE_SCRIPT_CODE" }]),
        ),
        (
            json!({"defaultLocale": "en", "subfolderSuffix": "us2"}),
            json!([{ "__typename": "MarketUserError", "field": ["input", "subfolderSuffix"], "message": "Subfolder suffix must contain only letters", "code": "SUBFOLDER_SUFFIX_MUST_CONTAIN_ONLY_LETTERS" }]),
        ),
        (
            json!({"defaultLocale": "en", "subfolderSuffix": "1"}),
            json!([
                { "__typename": "MarketUserError", "field": ["input", "subfolderSuffix"], "message": "Subfolder suffix must be at least 2 letters", "code": "SUBFOLDER_SUFFIX_MUST_BE_AT_LEAST_2_LETTERS" },
                { "__typename": "MarketUserError", "field": ["input", "subfolderSuffix"], "message": "Subfolder suffix must contain only letters", "code": "SUBFOLDER_SUFFIX_MUST_CONTAIN_ONLY_LETTERS" }
            ]),
        ),
        (
            json!({"defaultLocale": "en", "subfolderSuffix": "Latn1"}),
            json!([{ "__typename": "MarketUserError", "field": ["input", "subfolderSuffix"], "message": "Subfolder suffix must contain only letters", "code": "SUBFOLDER_SUFFIX_MUST_CONTAIN_ONLY_LETTERS" }]),
        ),
        (
            json!({"defaultLocale": "en", "domainId": "gid://shopify/Domain/9999"}),
            json!([{ "__typename": "MarketUserError", "field": ["input", "domainId"], "message": "Domain does not exist", "code": "DOMAIN_NOT_FOUND" }]),
        ),
    ];
    for (input, expected_errors) in validation_cases {
        let response = snapshot_proxy()
            .process_request(json_graphql_request(create_query, json!({"input": input})));
        assert_eq!(
            response.body["data"]["webPresenceCreate"]["webPresence"],
            Value::Null
        );
        assert_eq!(
            response.body["data"]["webPresenceCreate"]["userErrors"],
            expected_errors
        );
    }

    let mut duplicate_proxy = snapshot_proxy();
    let deduped = duplicate_proxy.process_request(json_graphql_request(
        create_query,
        json!({"input": {"defaultLocale": "en", "alternateLocales": ["fr", "fr"], "subfolderSuffix": "dup"}}),
    ));
    assert_eq!(
        deduped.body["data"]["webPresenceCreate"]["webPresence"]["alternateLocales"],
        json!([{ "locale": "fr", "primary": false }])
    );
    let taken = duplicate_proxy.process_request(json_graphql_request(
        create_query,
        json!({"input": {"defaultLocale": "en", "subfolderSuffix": "dup"}}),
    ));
    assert_eq!(
        taken.body["data"]["webPresenceCreate"]["webPresence"],
        Value::Null
    );
    assert_eq!(
        taken.body["data"]["webPresenceCreate"]["userErrors"][0]["code"],
        json!("TAKEN")
    );
    let deduped_read = duplicate_proxy.process_request(json_graphql_request(read_query, json!({})));
    assert_eq!(
        deduped_read.body["data"]["webPresences"]["nodes"]
            .as_array()
            .unwrap()
            .len(),
        1
    );

    let mut update_proxy = snapshot_proxy();
    let create = update_proxy.process_request(json_graphql_request(
        create_query,
        json!({"input": {"defaultLocale": "en", "alternateLocales": ["es"], "subfolderSuffix": "intl"}}),
    ));
    let id = create.body["data"]["webPresenceCreate"]["webPresence"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let update_default = update_proxy.process_request(json_graphql_request(
        update_query,
        json!({"id": id, "input": {"defaultLocale": "fr"}}),
    ));
    assert_eq!(
        update_default.body["data"]["webPresenceUpdate"]["webPresence"]["defaultLocale"],
        json!({"locale": "fr", "primary": true})
    );
    assert_eq!(
        update_default.body["data"]["webPresenceUpdate"]["webPresence"]["alternateLocales"],
        json!([{ "locale": "es", "primary": false }])
    );
    let update_alternates = update_proxy.process_request(json_graphql_request(
        update_query,
        json!({"id": id, "input": {"alternateLocales": ["de"]}}),
    ));
    assert_eq!(
        update_alternates.body["data"]["webPresenceUpdate"]["webPresence"]["defaultLocale"],
        json!({"locale": "fr", "primary": true})
    );
    assert_eq!(
        update_alternates.body["data"]["webPresenceUpdate"]["webPresence"]["alternateLocales"],
        json!([{ "locale": "de", "primary": false }])
    );
    let empty_noop = update_proxy.process_request(json_graphql_request(
        update_query,
        json!({"id": id, "input": {}}),
    ));
    assert_eq!(
        empty_noop.body["data"]["webPresenceUpdate"]["webPresence"]["subfolderSuffix"],
        json!("intl")
    );
    let ignored_domain = update_proxy.process_request(json_graphql_request(
        update_query,
        json!({"id": id, "input": {"domainId": "gid://shopify/Domain/9999"}}),
    ));
    assert_eq!(
        ignored_domain.body["data"]["webPresenceUpdate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        ignored_domain.body["data"]["webPresenceUpdate"]["webPresence"]["id"],
        json!(id)
    );

    for (input, expected) in [
        (
            json!({"defaultLocale": ""}),
            json!({"field": ["input", "defaultLocale"], "message": "Default locale can't be blank", "code": "CANNOT_SET_DEFAULT_LOCALE_TO_NULL"}),
        ),
        (
            json!({"defaultLocale": "bogus"}),
            json!({"field": ["input", "defaultLocale"], "message": "Invalid locale codes: bogus", "code": "INVALID"}),
        ),
        (
            json!({"subfolderSuffix": "en1"}),
            json!({"field": ["input", "subfolderSuffix"], "message": "Subfolder suffix must contain only letters", "code": "SUBFOLDER_SUFFIX_MUST_CONTAIN_ONLY_LETTERS"}),
        ),
    ] {
        let response = update_proxy.process_request(json_graphql_request(
            update_query,
            json!({"id": id, "input": input}),
        ));
        assert_eq!(
            response.body["data"]["webPresenceUpdate"]["webPresence"],
            Value::Null
        );
        assert_eq!(
            response.body["data"]["webPresenceUpdate"]["userErrors"][0]["field"],
            expected["field"]
        );
        assert_eq!(
            response.body["data"]["webPresenceUpdate"]["userErrors"][0]["message"],
            expected["message"]
        );
        assert_eq!(
            response.body["data"]["webPresenceUpdate"]["userErrors"][0]["code"],
            expected["code"]
        );
    }

    let mut taken_update_proxy = snapshot_proxy();
    let first = taken_update_proxy.process_request(json_graphql_request(
        create_query,
        json!({"input": {"defaultLocale": "en", "subfolderSuffix": "fr"}}),
    ));
    let first_id = first.body["data"]["webPresenceCreate"]["webPresence"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let _second = taken_update_proxy.process_request(json_graphql_request(
        create_query,
        json!({"input": {"defaultLocale": "en", "subfolderSuffix": "de"}}),
    ));
    let conflict = taken_update_proxy.process_request(json_graphql_request(
        update_query,
        json!({"id": first_id, "input": {"subfolderSuffix": "de"}}),
    ));
    assert_eq!(
        conflict.body["data"]["webPresenceUpdate"]["webPresence"],
        Value::Null
    );
    assert_eq!(
        conflict.body["data"]["webPresenceUpdate"]["userErrors"][0]["code"],
        json!("TAKEN")
    );
    let noop = taken_update_proxy.process_request(json_graphql_request(
        update_query,
        json!({"id": first_id, "input": {"subfolderSuffix": "fr"}}),
    ));
    assert_eq!(
        noop.body["data"]["webPresenceUpdate"]["webPresence"]["subfolderSuffix"],
        json!("fr")
    );
}

#[test]
fn market_create_ported_gleam_validation_and_staging_helpers_match_old_proxy_tests() {
    // Ports old Gleam proxy tests around marketCreate validation/staging:
    // - status/enabled mismatch and partial-input defaults
    // - price-inclusion projection and location-condition rejection
    // - currency settings flags/read-after-write, invalid base currency, manual FX rate
    // - duplicate/unsupported country-region guards without staging rejected records
    // - generated handle slugification/deduplication and explicit duplicate handle/name errors
    let create_query = r#"
        mutation RustMarketCreateLocalRuntimeCreate($input: MarketCreateInput!) {
          marketCreate(input: $input) {
            market {
              id name handle status enabled
              priceInclusions { inclusiveDutiesPricingStrategy inclusiveTaxPricingStrategy }
              currencySettings { baseCurrency { currencyCode currencyName } localCurrencies roundingEnabled }
            }
            userErrors { __typename field message code }
          }
        }
    "#;
    let read_query = r#"
        query RustMarketCreateLocalRuntimeRead($id: ID!) {
          market(id: $id) {
            id name handle status enabled
            priceInclusions { inclusiveDutiesPricingStrategy inclusiveTaxPricingStrategy }
            currencySettings { baseCurrency { currencyCode currencyName } localCurrencies roundingEnabled }
          }
        }
    "#;

    let mut mismatch_proxy = snapshot_proxy();
    for input in [
        json!({"name": "Mismatch", "status": "DRAFT", "enabled": true, "regions": [{"countryCode": "US"}]}),
        json!({"name": "Mismatch", "status": "ACTIVE", "enabled": false, "regions": [{"countryCode": "US"}]}),
    ] {
        let response = mismatch_proxy
            .process_request(json_graphql_request(create_query, json!({"input": input})));
        assert_eq!(response.status, 200);
        assert_eq!(
            response.body["data"]["marketCreate"],
            json!({
                "market": null,
                "userErrors": [{"__typename": "MarketUserError", "field": ["input"], "message": "Invalid status and enabled combination.", "code": "INVALID_STATUS_AND_ENABLED_COMBINATION"}]
            })
        );
    }

    let mut partial_proxy = snapshot_proxy();
    let blank_name = partial_proxy.process_request(json_graphql_request(
        create_query,
        json!({"input": {"name": ""}}),
    ));
    assert_eq!(
        blank_name.body["data"]["marketCreate"],
        json!({
            "market": null,
            "userErrors": [
                {"__typename": "MarketUserError", "field": ["input", "name"], "message": "Name can't be blank", "code": "BLANK"},
                {"__typename": "MarketUserError", "field": ["input", "name"], "message": "Name is too short (minimum is 2 characters)", "code": "TOO_SHORT"}
            ]
        })
    );
    let one_char_name = partial_proxy.process_request(json_graphql_request(
        create_query,
        json!({"input": {"name": "A"}}),
    ));
    assert_eq!(
        one_char_name.body["data"]["marketCreate"],
        json!({
            "market": null,
            "userErrors": [
                {"__typename": "MarketUserError", "field": ["input", "name"], "message": "Name is too short (minimum is 2 characters)", "code": "TOO_SHORT"}
            ]
        })
    );
    let enabled_only = partial_proxy.process_request(json_graphql_request(
        create_query,
        json!({"input": {"name": "Enabled Only", "enabled": true, "regions": [{"countryCode": "US"}]}}),
    ));
    assert_eq!(
        enabled_only.body["data"]["marketCreate"]["market"],
        json!({
            "id": "gid://shopify/Market/1",
            "name": "Enabled Only",
            "handle": "enabled-only",
            "status": "ACTIVE",
            "enabled": true,
            "priceInclusions": null,
            "currencySettings": null
        })
    );
    let draft = partial_proxy.process_request(json_graphql_request(
        create_query,
        json!({"input": {"name": "Draft", "status": "DRAFT", "enabled": false}}),
    ));
    assert_eq!(
        draft.body["data"]["marketCreate"]["market"]["status"],
        json!("DRAFT")
    );
    assert_eq!(
        draft.body["data"]["marketCreate"]["market"]["enabled"],
        json!(false)
    );

    let mut price_proxy = snapshot_proxy();
    let price_create = price_proxy.process_request(json_graphql_request(
        create_query,
        json!({"input": {
            "name": "Pricing",
            "conditions": {"regionsCondition": {"regions": [{"countryCode": "DK"}]}},
            "priceInclusions": {"taxPricingStrategy": "ADD_TAXES_AT_CHECKOUT", "dutiesPricingStrategy": "INCLUDE_DUTIES_IN_PRICE"}
        }}),
    ));
    assert_eq!(
        price_create.body["data"]["marketCreate"]["market"]["priceInclusions"],
        json!({"inclusiveDutiesPricingStrategy": "INCLUDE_DUTIES_IN_PRICE", "inclusiveTaxPricingStrategy": "ADD_TAXES_AT_CHECKOUT"})
    );
    let price_read = price_proxy.process_request(json_graphql_request(
        read_query,
        json!({"id": "gid://shopify/Market/1"}),
    ));
    assert_eq!(
        price_read.body["data"]["market"]["priceInclusions"],
        json!({"inclusiveDutiesPricingStrategy": "INCLUDE_DUTIES_IN_PRICE", "inclusiveTaxPricingStrategy": "ADD_TAXES_AT_CHECKOUT"})
    );

    let location_price_error = snapshot_proxy().process_request(json_graphql_request(
        create_query,
        json!({"input": {
            "name": "Location Pricing",
            "conditions": {"locationsCondition": {"locationIds": ["gid://shopify/Location/1"]}},
            "priceInclusions": {"taxPricingStrategy": "INCLUDES_TAXES_IN_PRICE", "dutiesPricingStrategy": "INCLUDE_DUTIES_IN_PRICE"}
        }}),
    ));
    assert_eq!(
        location_price_error.body["data"]["marketCreate"],
        json!({
            "market": null,
            "userErrors": [{"__typename": "MarketUserError", "field": ["input", "priceInclusions"], "message": "Inclusive pricing cannot be added to a market with the specified condition types.", "code": "INCLUSIVE_PRICING_NOT_COMPATIBLE_WITH_CONDITION_TYPES"}]
        })
    );

    let mut currency_proxy = snapshot_proxy();
    let currency_create = currency_proxy.process_request(json_graphql_request(
        create_query,
        json!({"input": {"name": "Currency Flags", "status": "ACTIVE", "enabled": true, "currencySettings": {"baseCurrency": "USD", "localCurrencies": true, "roundingEnabled": true}}}),
    ));
    assert_eq!(
        currency_create.body["data"]["marketCreate"]["market"]["currencySettings"],
        json!({"baseCurrency": {"currencyCode": "USD", "currencyName": "US Dollar"}, "localCurrencies": true, "roundingEnabled": true})
    );
    let currency_read = currency_proxy.process_request(json_graphql_request(
        read_query,
        json!({"id": "gid://shopify/Market/1"}),
    ));
    assert_eq!(
        currency_read.body["data"]["market"]["currencySettings"],
        currency_create.body["data"]["marketCreate"]["market"]["currencySettings"]
    );
    let eur_create = currency_proxy.process_request(json_graphql_request(
        create_query,
        json!({"input": {"name": "Euro Currency", "currencySettings": {"baseCurrency": "EUR"}}}),
    ));
    assert_eq!(
        eur_create.body["data"]["marketCreate"]["market"]["currencySettings"],
        json!({"baseCurrency": {"currencyCode": "EUR", "currencyName": "Euro"}, "localCurrencies": false, "roundingEnabled": false})
    );
    let eur_read = currency_proxy.process_request(json_graphql_request(
        read_query,
        json!({"id": "gid://shopify/Market/2"}),
    ));
    assert_eq!(
        eur_read.body["data"]["market"]["currencySettings"],
        eur_create.body["data"]["marketCreate"]["market"]["currencySettings"]
    );
    for (code, name) in [
        ("GBP", "British Pound"),
        ("CAD", "Canadian Dollar"),
        ("DKK", "Danish Krone"),
        ("MXN", "Mexican Peso"),
    ] {
        let response = currency_proxy.process_request(json_graphql_request(
            create_query,
            json!({"input": {"name": format!("{code} Currency"), "currencySettings": {"baseCurrency": code}}}),
        ));
        assert_eq!(
            response.body["data"]["marketCreate"]["market"]["currencySettings"],
            json!({"baseCurrency": {"currencyCode": code, "currencyName": name}, "localCurrencies": false, "roundingEnabled": false})
        );
    }
    let unknown_currency = currency_proxy.process_request(json_graphql_request(
        create_query,
        json!({"input": {"name": "Unknown Currency", "currencySettings": {"baseCurrency": "ZZZ"}}}),
    ));
    assert_eq!(
        unknown_currency.body["data"]["marketCreate"]["market"]["currencySettings"],
        json!({"baseCurrency": {"currencyCode": "ZZZ", "currencyName": "Unknown Currency"}, "localCurrencies": false, "roundingEnabled": false})
    );

    for input in [
        json!({"name": "Currency", "currencySettings": {"baseCurrency": "XXX"}}),
        json!({"name": "Currency", "currencySettings": {"baseCurrency": "XAF"}}),
    ] {
        let response = snapshot_proxy()
            .process_request(json_graphql_request(create_query, json!({"input": input})));
        assert_eq!(
            response.body["data"]["marketCreate"]["userErrors"][0],
            json!({"__typename": "MarketUserError", "field": ["input", "currencySettings", "baseCurrency"], "message": "Base currency is invalid", "code": "INVALID"})
        );
    }
    for input in [
        json!({"name": "Manual Rate", "currencySettings": {"baseCurrency": "USD", "baseCurrencyManualRate": 0}}),
        json!({"name": "Manual Rate", "currencySettings": {"baseCurrency": "USD", "baseCurrencyManualRate": -1.5}}),
    ] {
        let response = snapshot_proxy()
            .process_request(json_graphql_request(create_query, json!({"input": input})));
        assert_eq!(
            response.body["data"]["marketCreate"]["userErrors"][0],
            json!({"__typename": "MarketUserError", "field": ["input", "currencySettings", "baseCurrencyManualRate"], "message": "Enter a rate above 0.", "code": null})
        );
    }

    let mut region_proxy = snapshot_proxy();
    let first_ca = region_proxy.process_request(json_graphql_request(
        create_query,
        json!({"input": {"name": "Canada Local", "regions": [{"countryCode": "CA"}]}}),
    ));
    assert_eq!(
        first_ca.body["data"]["marketCreate"]["market"]["id"],
        json!("gid://shopify/Market/1")
    );
    let duplicate_ca = region_proxy.process_request(json_graphql_request(
        create_query,
        json!({"input": {"name": "Canada Duplicate", "regions": [{"countryCode": "CA"}]}}),
    ));
    assert_eq!(
        duplicate_ca.body["data"]["marketCreate"]["userErrors"][0],
        json!({"__typename": "MarketUserError", "field": ["input", "regions", "0", "countryCode"], "message": "Code has already been taken", "code": "TAKEN"})
    );
    let unsupported = region_proxy.process_request(json_graphql_request(
        create_query,
        json!({"input": {"name": "Unsupported", "regions": [{"countryCode": "US"}, {"countryCode": "CU"}]}}),
    ));
    assert_eq!(
        unsupported.body["data"]["marketCreate"]["userErrors"][0],
        json!({"__typename": "MarketUserError", "field": ["input", "regions", "1", "countryCode"], "message": "CU is not a supported country or region code.", "code": "UNSUPPORTED_COUNTRY_REGION"})
    );
    assert!(!region_proxy
        .get_state_snapshot()
        .to_string()
        .contains("Unsupported"));

    let mut handle_proxy = snapshot_proxy();
    for (name, expected_handle) in [
        ("Europe", "europe"),
        ("Europe!", "europe-1"),
        ("Europe?", "europe-2"),
    ] {
        let response = handle_proxy.process_request(json_graphql_request(
            create_query,
            json!({"input": {"name": name}}),
        ));
        assert_eq!(
            response.body["data"]["marketCreate"]["market"]["handle"],
            json!(expected_handle)
        );
    }
    let slug = snapshot_proxy().process_request(json_graphql_request(
        create_query,
        json!({"input": {"name": "  North & South / EU!  "}}),
    ));
    assert_eq!(
        slug.body["data"]["marketCreate"]["market"]["handle"],
        json!("north-south-eu")
    );

    let mut duplicate_name_proxy = snapshot_proxy();
    let _ = duplicate_name_proxy.process_request(json_graphql_request(
        create_query,
        json!({"input": {"name": "Europe"}}),
    ));
    let duplicate_name = duplicate_name_proxy.process_request(json_graphql_request(
        create_query,
        json!({"input": {"name": "europe"}}),
    ));
    assert_eq!(
        duplicate_name.body["data"]["marketCreate"]["userErrors"][0],
        json!({"__typename": "MarketUserError", "field": ["input", "name"], "message": "Name has already been taken", "code": "TAKEN"})
    );
    assert_eq!(
        duplicate_name_proxy.get_log_snapshot()["entries"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let mut duplicate_handle_proxy = snapshot_proxy();
    let _ = duplicate_handle_proxy.process_request(json_graphql_request(
        create_query,
        json!({"input": {"name": "Europe"}}),
    ));
    let duplicate_handle = duplicate_handle_proxy.process_request(json_graphql_request(
        create_query,
        json!({"input": {"name": "Other", "handle": "Europe"}}),
    ));
    assert_eq!(
        duplicate_handle.body["data"]["marketCreate"]["userErrors"][0],
        json!({"__typename": "MarketUserError", "field": ["input", "handle"], "message": "Generated handle has already been taken", "code": "GENERATED_DUPLICATED_HANDLE"})
    );
}

#[test]
fn catalog_create_and_context_update_ported_gleam_helpers_stage_and_validate() {
    // Ports old Gleam catalog/context helper behavior from markets_mutation_test.gleam:
    // required/invalid status, required context/market IDs, unsupported country contexts,
    // typed CatalogUserError shapes, market-context staging/readback, unknown catalog delete,
    // and catalogContextUpdate add/remove validation/readback.
    let create_query = r#"
        mutation RustCatalogLocalRuntimeCreate($input: CatalogCreateInput!) {
          catalogCreate(input: $input) {
            catalog { id title status markets(first: 5) { nodes { id } } }
            userErrors { __typename field message code }
          }
        }
    "#;

    let validation_cases = [
        (
            json!({"title": "EU Catalog", "context": {"driverType": "MARKET", "marketIds": ["gid://shopify/Market/1"]}}),
            json!({"__typename": "CatalogUserError", "field": ["input", "status"], "message": "Status is required", "code": "REQUIRED"}),
        ),
        (
            json!({"title": "EU Catalog", "status": "DISABLED", "context": {"driverType": "MARKET", "marketIds": ["gid://shopify/Market/1"]}}),
            json!({"__typename": "CatalogUserError", "field": ["input", "status"], "message": "Status is invalid", "code": "INVALID"}),
        ),
        (
            json!({"title": "EU Catalog", "status": "ACTIVE"}),
            json!({"__typename": "CatalogUserError", "field": ["input", "context"], "message": "Context is required", "code": "INVALID"}),
        ),
        (
            json!({"title": "EU Catalog", "status": "ACTIVE", "context": {}}),
            json!({"__typename": "CatalogUserError", "field": ["input", "context", "marketIds"], "message": "Market ids can't be blank", "code": "INVALID"}),
        ),
        (
            json!({"title": "EU Catalog", "status": "ACTIVE", "context": {"driverType": "MARKET", "marketIds": ["gid://shopify/Market/404"]}}),
            json!({"__typename": "CatalogUserError", "field": ["input", "context", "marketIds", "0"], "message": "Market does not exist", "code": "INVALID"}),
        ),
        (
            json!({"title": "EU Catalog", "status": "ACTIVE", "context": {"driverType": "MARKET", "marketIds": []}}),
            json!({"__typename": "CatalogUserError", "field": ["input", "context", "marketIds"], "message": "Market ids can't be blank", "code": "INVALID"}),
        ),
        (
            json!({"title": "Country Catalog", "status": "ACTIVE", "context": {"driverType": "COUNTRY", "countryCodes": []}}),
            json!({"__typename": "CatalogUserError", "field": ["input", "context", "countryCodes"], "message": "Country codes can't be blank", "code": "INVALID"}),
        ),
        (
            json!({"title": "Country Catalog", "status": "ACTIVE", "context": {"driverType": "COUNTRY", "countryCodes": ["US"]}}),
            json!({"__typename": "CatalogUserError", "field": ["input", "context", "driverType"], "message": "Catalog context driverType COUNTRY is not supported by the local MarketCatalog model", "code": "INVALID"}),
        ),
        (
            json!({"title": "", "status": "ACTIVE", "context": {"marketIds": ["gid://shopify/Market/missing"]}}),
            json!({"__typename": "CatalogUserError", "field": ["input", "title"], "message": "Title can't be blank", "code": "BLANK"}),
        ),
    ];
    for (input, error) in validation_cases {
        let mut proxy = snapshot_proxy();
        let response =
            proxy.process_request(json_graphql_request(create_query, json!({"input": input})));
        assert_eq!(response.status, 200);
        assert_eq!(
            response.body["data"]["catalogCreate"],
            json!({"catalog": null, "userErrors": [error]})
        );
    }

    let mut proxy = snapshot_proxy();
    let market_create = proxy.process_request(json_graphql_request(
        r#"
        mutation RustMarketCreateLocalRuntimeCreate($input: MarketCreateInput!) {
          marketCreate(input: $input) { market { id } userErrors { field message code } }
        }
        "#,
        json!({"input": {"name": "Europe", "regions": [{"countryCode": "DK"}]}}),
    ));
    assert_eq!(
        market_create.body["data"]["marketCreate"]["market"]["id"],
        json!("gid://shopify/Market/1")
    );

    let unknown_price_list = proxy.process_request(json_graphql_request(
        create_query,
        json!({"input": {"title": "EU Catalog", "status": "ACTIVE", "context": {"driverType": "MARKET", "marketIds": ["gid://shopify/Market/1"]}, "priceListId": "gid://shopify/PriceList/9999999999"}}),
    ));
    assert_eq!(
        unknown_price_list.body["data"]["catalogCreate"],
        json!({"catalog": null, "userErrors": [{"__typename": "CatalogUserError", "field": ["input", "priceListId"], "message": "Price list not found.", "code": "PRICE_LIST_NOT_FOUND"}]})
    );
    let unknown_publication = proxy.process_request(json_graphql_request(
        create_query,
        json!({"input": {"title": "EU Catalog", "status": "ACTIVE", "context": {"driverType": "MARKET", "marketIds": ["gid://shopify/Market/1"]}, "publicationId": "gid://shopify/Publication/9999999999"}}),
    ));
    assert_eq!(
        unknown_publication.body["data"]["catalogCreate"],
        json!({"catalog": null, "userErrors": [{"__typename": "CatalogUserError", "field": ["input", "publicationId"], "message": "Publication not found.", "code": "PUBLICATION_NOT_FOUND"}]})
    );

    let catalog_create = proxy.process_request(json_graphql_request(
        create_query,
        json!({"input": {"title": "EU Catalog", "status": "ACTIVE", "context": {"driverType": "MARKET", "marketIds": ["gid://shopify/Market/1"]}}}),
    ));
    assert_eq!(
        catalog_create.body["data"]["catalogCreate"],
        json!({
            "catalog": {"id": "gid://shopify/MarketCatalog/3", "title": "EU Catalog", "status": "ACTIVE", "markets": {"nodes": [{"id": "gid://shopify/Market/1"}]}},
            "userErrors": []
        })
    );

    let read = proxy.process_request(json_graphql_request(
        r#"
        query RustCatalogLocalRuntimeRead($id: ID!) {
          catalog(id: $id) { id title status markets(first: 5) { nodes { id } } }
          catalogs(first: 5, type: MARKET) { nodes { id title status markets(first: 5) { nodes { id } } } }
        }
        "#,
        json!({"id": "gid://shopify/MarketCatalog/3"}),
    ));
    assert_eq!(
        read.body["data"]["catalog"],
        json!({"id": "gid://shopify/MarketCatalog/3", "title": "EU Catalog", "status": "ACTIVE", "markets": {"nodes": [{"id": "gid://shopify/Market/1"}]}})
    );
    assert_eq!(
        read.body["data"]["catalogs"]["nodes"],
        json!([{"id": "gid://shopify/MarketCatalog/3", "title": "EU Catalog", "status": "ACTIVE", "markets": {"nodes": [{"id": "gid://shopify/Market/1"}]}}])
    );

    let unknown_delete = proxy.process_request(json_graphql_request(
        r#"
        mutation RustCatalogLocalRuntimeDelete($id: ID!) {
          catalogDelete(id: $id) { deletedId userErrors { __typename field message code } }
        }
        "#,
        json!({"id": "gid://shopify/MarketCatalog/missing"}),
    ));
    assert_eq!(
        unknown_delete.body["data"]["catalogDelete"],
        json!({"deletedId": null, "userErrors": [{"__typename": "CatalogUserError", "field": ["id"], "message": "Catalog does not exist", "code": "CATALOG_NOT_FOUND"}]})
    );

    let unknown_context_catalog = proxy.process_request(json_graphql_request(
        r#"
        mutation RustCatalogLocalRuntimeContextUpdate($catalogId: ID!, $add: CatalogContextInput!) {
          catalogContextUpdate(catalogId: $catalogId, contextsToAdd: $add) { catalog { id } userErrors { __typename field message code } }
        }
        "#,
        json!({"catalogId": "gid://shopify/MarketCatalog/404", "add": {"marketIds": ["gid://shopify/Market/404"]}}),
    ));
    assert_eq!(
        unknown_context_catalog.body["data"]["catalogContextUpdate"],
        json!({"catalog": null, "userErrors": [{"__typename": "CatalogUserError", "field": ["catalogId"], "message": "Catalog does not exist", "code": "CATALOG_NOT_FOUND"}]})
    );

    let missing_contexts = proxy.process_request(json_graphql_request(
        r#"
        mutation RustCatalogLocalRuntimeContextUpdate($catalogId: ID!) {
          catalogContextUpdate(catalogId: $catalogId) { catalog { id } userErrors { __typename field message code } }
        }
        "#,
        json!({"catalogId": "gid://shopify/MarketCatalog/3"}),
    ));
    assert_eq!(
        missing_contexts.body["data"]["catalogContextUpdate"],
        json!({"catalog": null, "userErrors": [{"__typename": "CatalogUserError", "field": ["contextsToAdd"], "message": "Must have `contexts_to_add` or `contexts_to_remove` argument.", "code": "REQUIRES_CONTEXTS_TO_ADD_OR_REMOVE"}]})
    );

    let missing_market_contexts = proxy.process_request(json_graphql_request(
        r#"
        mutation RustCatalogLocalRuntimeContextUpdate($catalogId: ID!, $add: CatalogContextInput!, $remove: CatalogContextInput!) {
          catalogContextUpdate(catalogId: $catalogId, contextsToAdd: $add, contextsToRemove: $remove) { catalog { id } userErrors { __typename field message code } }
        }
        "#,
        json!({"catalogId": "gid://shopify/MarketCatalog/3", "add": {"marketIds": ["gid://shopify/Market/404"]}, "remove": {"marketIds": ["gid://shopify/Market/405"]}}),
    ));
    assert_eq!(
        missing_market_contexts.body["data"]["catalogContextUpdate"],
        json!({"catalog": null, "userErrors": [
            {"__typename": "CatalogUserError", "field": ["contextsToAdd", "marketIds", "0"], "message": "Market does not exist", "code": "MARKET_NOT_FOUND"},
            {"__typename": "CatalogUserError", "field": ["contextsToRemove", "marketIds", "0"], "message": "Market does not exist", "code": "MARKET_NOT_FOUND"}
        ]})
    );

    let second_market = proxy.process_request(json_graphql_request(
        r#"
        mutation RustMarketCreateLocalRuntimeCreate($input: MarketCreateInput!) {
          marketCreate(input: $input) { market { id } userErrors { field message code } }
        }
        "#,
        json!({"input": {"name": "North America", "regions": [{"countryCode": "US"}]}}),
    ));
    let second_market_id = second_market.body["data"]["marketCreate"]["market"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let context_update = proxy.process_request(json_graphql_request(
        r#"
        mutation RustCatalogLocalRuntimeContextUpdate($catalogId: ID!, $add: CatalogContextInput!, $remove: CatalogContextInput!) {
          catalogContextUpdate(catalogId: $catalogId, contextsToAdd: $add, contextsToRemove: $remove) {
            catalog { id markets(first: 5) { nodes { id } } }
            userErrors { __typename field message code }
          }
        }
        "#,
        json!({"catalogId": "gid://shopify/MarketCatalog/3", "add": {"marketIds": [second_market_id]}, "remove": {"marketIds": ["gid://shopify/Market/1"]}}),
    ));
    assert_eq!(
        context_update.body["data"]["catalogContextUpdate"],
        json!({"catalog": {"id": "gid://shopify/MarketCatalog/3", "markets": {"nodes": [{"id": second_market_id}]}}, "userErrors": []})
    );
}

#[test]
fn market_catalog_relation_tail_helpers_ported_from_gleam() {
    // Ports the remaining old Gleam markets_mutation tail helpers around:
    // - marketCreate plan-limit skip cases in the Rust local-runtime shape
    // - marketUpdate unknown catalog/web-presence link additions
    // - catalogDelete detaching a surviving price list
    // - catalogCreate/catalogUpdate taken price-list/publication relation guards
    let market_create_query = r#"
        mutation RustMarketCreateLocalRuntimeRelationCreate($input: MarketCreateInput!) {
          marketCreate(input: $input) { market { id name status enabled catalogs(first: 5) { nodes { id } } webPresences(first: 5) { nodes { id } } } userErrors { __typename field message code } }
        }
    "#;
    let catalog_create_query = r#"
        mutation RustCatalogLocalRuntimeRelationCreate($input: CatalogCreateInput!) {
          catalogCreate(input: $input) { catalog { id title status priceList { id } publication { id } markets(first: 5) { nodes { id } } } userErrors { __typename field message code } }
        }
    "#;
    let catalog_update_query = r#"
        mutation RustCatalogLocalRuntimeRelationUpdate($id: ID!, $input: CatalogUpdateInput!) {
          catalogUpdate(id: $id, input: $input) { catalog { id priceList { id } publication { id } } userErrors { __typename field message code } }
        }
    "#;
    let catalog_delete_query = r#"
        mutation RustCatalogLocalRuntimeRelationDelete($id: ID!) {
          catalogDelete(id: $id) { deletedId userErrors { __typename field message code } }
        }
    "#;
    let price_list_create_query = r#"
        mutation RustPriceListLocalRuntimeRelationCreate($input: PriceListCreateInput!) {
          priceListCreate(input: $input) { priceList { id catalog { id } } userErrors { __typename field message code } }
        }
    "#;
    let price_list_read_query = r#"
        query RustPriceListLocalRuntimeRelationRead($catalogId: ID!, $priceListId: ID!) {
          catalog(id: $catalogId) { id }
          priceList(id: $priceListId) { id catalog { id } }
        }
    "#;
    let market_update_query = r#"
        mutation RustMarketRelationsLocalRuntimeUpdate($id: ID!, $input: MarketUpdateInput!) {
          marketUpdate(id: $id, input: $input) {
            market {
              id
              catalogs(first: 5) { nodes { id markets(first: 5) { nodes { id } } } }
              webPresences(first: 5) { nodes { id markets(first: 5) { nodes { id } } } }
            }
            userErrors { __typename field message code }
          }
        }
    "#;
    let web_presence_create_query = r#"
        mutation RustMarketWebPresenceHelperLocalRuntimeRelationCreate($input: WebPresenceCreateInput!) {
          webPresenceCreate(input: $input) { webPresence { id markets(first: 5) { nodes { id } } } userErrors { __typename field message code } }
        }
    "#;
    let web_presence_read_query = r#"
        query RustMarketWebPresenceHelperLocalRuntimeRelationRead {
          webPresences(first: 5) { nodes { id markets(first: 5) { nodes { id } } } }
        }
    "#;
    let catalog_read_query = r#"
        query RustCatalogLocalRuntimeRelationRead($id: ID!) {
          catalog(id: $id) { id markets(first: 5) { nodes { id } } priceList { id } publication { id } }
        }
    "#;

    let mut plan_skip_proxy = snapshot_proxy();
    let home_style = plan_skip_proxy.process_request(json_graphql_request(
        market_create_query,
        json!({"input": {"name": "Markets Home", "status": "ACTIVE", "enabled": true}}),
    ));
    assert_eq!(home_style.status, 200);
    assert_eq!(
        home_style.body["data"]["marketCreate"]["market"]["status"],
        json!("ACTIVE")
    );
    assert_eq!(
        home_style.body["data"]["marketCreate"]["market"]["enabled"],
        json!(true)
    );
    let draft_style = plan_skip_proxy.process_request(json_graphql_request(
        market_create_query,
        json!({"input": {"name": "Draft", "status": "DRAFT", "enabled": false}}),
    ));
    assert_eq!(
        draft_style.body["data"]["marketCreate"]["market"]["status"],
        json!("DRAFT")
    );
    assert_eq!(
        draft_style.body["data"]["marketCreate"]["market"]["enabled"],
        json!(false)
    );

    let mut detach_proxy = snapshot_proxy();
    let _market = detach_proxy.process_request(json_graphql_request(
        market_create_query,
        json!({"input": {"name": "Attached Market"}}),
    ));
    let catalog = detach_proxy.process_request(json_graphql_request(
        catalog_create_query,
        json!({"input": {"title": "Attached Catalog", "status": "ACTIVE", "context": {"driverType": "MARKET", "marketIds": ["gid://shopify/Market/1"]}}}),
    ));
    let catalog_id = catalog.body["data"]["catalogCreate"]["catalog"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let price_list = detach_proxy.process_request(json_graphql_request(
        price_list_create_query,
        json!({"input": {"name": "Attached Price List", "currency": "USD", "parent": {"adjustment": {"type": "PERCENTAGE_DECREASE", "value": 10}}, "catalogId": catalog_id}}),
    ));
    let price_list_id = price_list.body["data"]["priceListCreate"]["priceList"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let delete = detach_proxy.process_request(json_graphql_request(
        catalog_delete_query,
        json!({"id": catalog_id}),
    ));
    assert_eq!(
        delete.body["data"]["catalogDelete"]["userErrors"],
        json!([])
    );
    let detach_read = detach_proxy.process_request(json_graphql_request(
        price_list_read_query,
        json!({"catalogId": catalog.body["data"]["catalogCreate"]["catalog"]["id"], "priceListId": price_list_id}),
    ));
    assert_eq!(detach_read.body["data"]["catalog"], Value::Null);
    assert_eq!(
        detach_read.body["data"]["priceList"]["catalog"],
        Value::Null
    );

    let mut relation_proxy = snapshot_proxy();
    let market = relation_proxy.process_request(json_graphql_request(
        market_create_query,
        json!({"input": {"name": "Europe", "regions": [{"countryCode": "DK"}]}}),
    ));
    assert_eq!(
        market.body["data"]["marketCreate"]["market"]["id"],
        json!("gid://shopify/Market/1")
    );
    let first_catalog = relation_proxy.process_request(json_graphql_request(
        catalog_create_query,
        json!({"input": {"title": "First Catalog", "status": "ACTIVE", "context": {"driverType": "MARKET", "marketIds": ["gid://shopify/Market/1"]}, "priceListId": "gid://shopify/PriceList/1", "publicationId": "gid://shopify/Publication/1"}}),
    ));
    assert_eq!(
        first_catalog.body["data"]["catalogCreate"]["userErrors"],
        json!([])
    );
    let second_catalog = relation_proxy.process_request(json_graphql_request(
        catalog_create_query,
        json!({"input": {"title": "Second Catalog", "status": "ACTIVE", "context": {"driverType": "MARKET", "marketIds": ["gid://shopify/Market/1"]}}}),
    ));
    let second_catalog_id = second_catalog.body["data"]["catalogCreate"]["catalog"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    for (input, expected_error) in [
        (
            json!({"title": "Price List Taken", "status": "ACTIVE", "context": {"driverType": "MARKET", "marketIds": ["gid://shopify/Market/1"]}, "priceListId": "gid://shopify/PriceList/1"}),
            json!({"__typename": "CatalogUserError", "field": ["input", "priceListId"], "message": "Price list has already been taken", "code": "TAKEN"}),
        ),
        (
            json!({"title": "Publication Taken", "status": "ACTIVE", "context": {"driverType": "MARKET", "marketIds": ["gid://shopify/Market/1"]}, "publicationId": "gid://shopify/Publication/1"}),
            json!({"__typename": "CatalogUserError", "field": ["input", "publicationId"], "message": "Publication is already attached to another catalog", "code": "PUBLICATION_TAKEN"}),
        ),
    ] {
        let response = relation_proxy.process_request(json_graphql_request(
            catalog_create_query,
            json!({"input": input}),
        ));
        assert_eq!(
            response.body["data"]["catalogCreate"],
            json!({"catalog": null, "userErrors": [expected_error]})
        );
    }

    for (input, expected_error) in [
        (
            json!({"priceListId": "gid://shopify/PriceList/9999999999"}),
            json!({"__typename": "CatalogUserError", "field": ["input", "priceListId"], "message": "Price list not found.", "code": "PRICE_LIST_NOT_FOUND"}),
        ),
        (
            json!({"publicationId": "gid://shopify/Publication/9999999999"}),
            json!({"__typename": "CatalogUserError", "field": ["input", "publicationId"], "message": "Publication not found.", "code": "PUBLICATION_NOT_FOUND"}),
        ),
        (
            json!({"priceListId": "gid://shopify/PriceList/1"}),
            json!({"__typename": "CatalogUserError", "field": ["input", "priceListId"], "message": "Price list has already been taken", "code": "TAKEN"}),
        ),
        (
            json!({"publicationId": "gid://shopify/Publication/1"}),
            json!({"__typename": "CatalogUserError", "field": ["input", "publicationId"], "message": "Publication is already attached to another catalog", "code": "PUBLICATION_TAKEN"}),
        ),
    ] {
        let response = relation_proxy.process_request(json_graphql_request(
            catalog_update_query,
            json!({"id": second_catalog_id, "input": input}),
        ));
        assert_eq!(
            response.body["data"]["catalogUpdate"],
            json!({"catalog": null, "userErrors": [expected_error]})
        );
    }

    let mut update_proxy = snapshot_proxy();
    let _primary = update_proxy.process_request(json_graphql_request(
        market_create_query,
        json!({"input": {"name": "Primary"}}),
    ));
    let _secondary = update_proxy.process_request(json_graphql_request(
        market_create_query,
        json!({"input": {"name": "Secondary"}}),
    ));
    let linked_catalog = update_proxy.process_request(json_graphql_request(
        catalog_create_query,
        json!({"input": {"title": "Linked Catalog", "status": "ACTIVE", "context": {"driverType": "MARKET", "marketIds": ["gid://shopify/Market/2"]}}}),
    ));
    let linked_catalog_id = linked_catalog.body["data"]["catalogCreate"]["catalog"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let unknown_catalog_link = update_proxy.process_request(json_graphql_request(
        market_update_query,
        json!({"id": "gid://shopify/Market/1", "input": {"catalogsToAdd": ["gid://shopify/MarketCatalog/9999999999"]}}),
    ));
    assert_eq!(
        unknown_catalog_link.body["data"]["marketUpdate"],
        json!({"market": null, "userErrors": [{"__typename": "MarketUserError", "field": ["input", "catalogsToAdd"], "message": "The following customization IDs were not found: 9999999999", "code": "CUSTOMIZATIONS_NOT_FOUND"}]})
    );
    let add_catalog = update_proxy.process_request(json_graphql_request(
        market_update_query,
        json!({"id": "gid://shopify/Market/1", "input": {"catalogsToAdd": [linked_catalog_id]}}),
    ));
    assert_eq!(
        add_catalog.body["data"]["marketUpdate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        add_catalog.body["data"]["marketUpdate"]["market"]["catalogs"]["nodes"][0]["markets"]
            ["nodes"],
        json!([{"id": "gid://shopify/Market/2"}, {"id": "gid://shopify/Market/1"}])
    );
    let catalog_read = update_proxy.process_request(json_graphql_request(
        catalog_read_query,
        json!({"id": linked_catalog_id}),
    ));
    assert_eq!(
        catalog_read.body["data"]["catalog"]["markets"]["nodes"],
        json!([{"id": "gid://shopify/Market/2"}, {"id": "gid://shopify/Market/1"}])
    );
    let remove_catalog = update_proxy.process_request(json_graphql_request(
        market_update_query,
        json!({"id": "gid://shopify/Market/1", "input": {"catalogsToDelete": [linked_catalog_id]}}),
    ));
    assert_eq!(
        remove_catalog.body["data"]["marketUpdate"]["market"]["catalogs"]["nodes"],
        json!([])
    );

    let web_presence = update_proxy.process_request(json_graphql_request(
        web_presence_create_query,
        json!({"input": {"defaultLocale": "en", "subfolderSuffix": "intl"}}),
    ));
    let web_presence_id = web_presence.body["data"]["webPresenceCreate"]["webPresence"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let unknown_web_presence_link = update_proxy.process_request(json_graphql_request(
        market_update_query,
        json!({"id": "gid://shopify/Market/1", "input": {"webPresencesToAdd": ["gid://shopify/MarketWebPresence/9999999999"]}}),
    ));
    assert_eq!(
        unknown_web_presence_link.body["data"]["marketUpdate"],
        json!({"market": null, "userErrors": [{"__typename": "MarketUserError", "field": ["input", "webPresencesToAdd"], "message": "The following customization IDs were not found: 9999999999", "code": "CUSTOMIZATIONS_NOT_FOUND"}]})
    );
    let add_web_presence = update_proxy.process_request(json_graphql_request(
        market_update_query,
        json!({"id": "gid://shopify/Market/1", "input": {"webPresencesToAdd": [web_presence_id]}}),
    ));
    assert_eq!(
        add_web_presence.body["data"]["marketUpdate"]["market"]["webPresences"]["nodes"][0]
            ["markets"]["nodes"],
        json!([{"id": "gid://shopify/Market/1"}])
    );
    let web_presence_read =
        update_proxy.process_request(json_graphql_request(web_presence_read_query, json!({})));
    assert_eq!(
        web_presence_read.body["data"]["webPresences"]["nodes"][0]["markets"]["nodes"],
        json!([{"id": "gid://shopify/Market/1"}])
    );
    let remove_web_presence = update_proxy.process_request(json_graphql_request(
        market_update_query,
        json!({"id": "gid://shopify/Market/1", "input": {"webPresencesToDelete": [web_presence_id]}}),
    ));
    assert_eq!(
        remove_web_presence.body["data"]["marketUpdate"]["market"]["webPresences"]["nodes"],
        json!([])
    );
}

#[test]
fn price_list_fixed_prices_ported_gleam_helpers_stage_and_validate() {
    // Ports old Gleam fixed-price helper behavior from markets_mutation_test.gleam:
    // by-product bulk validation/staging, fixed price add/update/delete lifecycle,
    // duplicate variant last-wins semantics, price-list/variant/currency guards,
    // missing fixed-price deletion errors, and downstream selected price-list readback.
    let read_query = r#"
        query RustPriceListFixedPricesLocalRuntimeRead($id: ID!) {
          priceList(id: $id) {
            id
            fixedPricesCount
            prices(first: 10, originType: FIXED) {
              edges {
                node {
                  originType
                  price { amount currencyCode }
                  compareAtPrice { amount currencyCode }
                  variant { id product { id title } }
                }
              }
            }
          }
        }
    "#;
    let by_product_update_query = r#"
        mutation RustPriceListFixedPricesLocalRuntimeByProductUpdate($priceListId: ID!, $pricesToAdd: [PriceListFixedPriceByProductInput!]!, $pricesToDeleteByProductIds: [ID!]!) {
          priceListFixedPricesByProductUpdate(priceListId: $priceListId, pricesToAdd: $pricesToAdd, pricesToDeleteByProductIds: $pricesToDeleteByProductIds) {
            priceList { id fixedPricesCount }
            pricesToAddProducts { id title }
            pricesToDeleteProducts { id title }
            userErrors { __typename field message code }
          }
        }
    "#;

    let mut noop_proxy = snapshot_proxy();
    let noop = noop_proxy.process_request(json_graphql_request(
        by_product_update_query,
        json!({"priceListId": "gid://shopify/PriceList/test", "pricesToAdd": [], "pricesToDeleteByProductIds": []}),
    ));
    assert_eq!(noop.status, 200);
    assert_eq!(
        noop.body["data"]["priceListFixedPricesByProductUpdate"],
        json!({
            "priceList": null,
            "pricesToAddProducts": [],
            "pricesToDeleteProducts": [],
            "userErrors": [{"__typename": "PriceListFixedPricesByProductBulkUpdateUserError", "field": null, "message": "No update operations specified.", "code": "NO_UPDATE_OPERATIONS_SPECIFIED"}]
        })
    );
    let noop_read = noop_proxy.process_request(json_graphql_request(
        read_query,
        json!({"id": "gid://shopify/PriceList/test"}),
    ));
    assert_eq!(
        noop_read.body["data"]["priceList"]["fixedPricesCount"],
        json!(0)
    );

    let mut invalid_bulk_proxy = snapshot_proxy();
    let invalid_bulk = invalid_bulk_proxy.process_request(json_graphql_request(
        by_product_update_query,
        json!({
            "priceListId": "gid://shopify/PriceList/test",
            "pricesToAdd": [
                {"productId": "gid://shopify/Product/test", "price": {"amount": "12.00", "currencyCode": "USD"}, "compareAtPrice": {"amount": "15.00", "currencyCode": "GBP"}},
                {"productId": "gid://shopify/Product/test", "price": {"amount": "13.00", "currencyCode": "EUR"}}
            ],
            "pricesToDeleteByProductIds": ["gid://shopify/Product/test", "gid://shopify/Product/test"]
        }),
    ));
    let invalid_bulk_errors = invalid_bulk.body["data"]["priceListFixedPricesByProductUpdate"]
        ["userErrors"]
        .as_array()
        .unwrap();
    for expected_error in [
        json!({"__typename": "PriceListFixedPricesByProductBulkUpdateUserError", "field": ["pricesToAdd", "0", "price", "currencyCode"], "message": "The specified currency does not match the price list's currency.", "code": "PRICES_TO_ADD_CURRENCY_MISMATCH"}),
        json!({"__typename": "PriceListFixedPricesByProductBulkUpdateUserError", "field": ["pricesToAdd", "0", "compareAtPrice", "currencyCode"], "message": "The specified currency does not match the price list's currency.", "code": "PRICES_TO_ADD_CURRENCY_MISMATCH"}),
        json!({"__typename": "PriceListFixedPricesByProductBulkUpdateUserError", "field": ["pricesToAdd"], "message": "Duplicate product IDs are not allowed.", "code": "DUPLICATE_ID_IN_INPUT"}),
        json!({"__typename": "PriceListFixedPricesByProductBulkUpdateUserError", "field": ["pricesToDeleteByProductIds"], "message": "Duplicate product IDs are not allowed.", "code": "DUPLICATE_ID_IN_INPUT"}),
        json!({"__typename": "PriceListFixedPricesByProductBulkUpdateUserError", "field": null, "message": "Product IDs cannot be both added and deleted.", "code": "ID_MUST_BE_MUTUALLY_EXCLUSIVE"}),
    ] {
        assert!(
            invalid_bulk_errors.contains(&expected_error),
            "missing fixed-price by-product validation error: {expected_error:?}\nbody={:?}",
            invalid_bulk.body
        );
    }
    let invalid_bulk_read = invalid_bulk_proxy.process_request(json_graphql_request(
        read_query,
        json!({"id": "gid://shopify/PriceList/test"}),
    ));
    assert_eq!(
        invalid_bulk_read.body["data"]["priceList"]["fixedPricesCount"],
        json!(0)
    );

    let missing_products = snapshot_proxy().process_request(json_graphql_request(
        by_product_update_query,
        json!({
            "priceListId": "gid://shopify/PriceList/test",
            "pricesToAdd": [{"productId": "gid://shopify/Product/missing", "price": {"amount": "12.00", "currencyCode": "EUR"}}],
            "pricesToDeleteByProductIds": ["gid://shopify/Product/missing-delete"]
        }),
    ));
    let missing_product_errors = missing_products.body["data"]
        ["priceListFixedPricesByProductUpdate"]["userErrors"]
        .as_array()
        .unwrap();
    assert!(missing_product_errors.contains(&json!({"__typename": "PriceListFixedPricesByProductBulkUpdateUserError", "field": ["pricesToAdd", "0", "productId"], "message": "Product does not exist.", "code": "PRODUCT_DOES_NOT_EXIST"})));
    assert!(missing_product_errors.contains(&json!({"__typename": "PriceListFixedPricesByProductBulkUpdateUserError", "field": ["pricesToDeleteByProductIds", "0"], "message": "Product does not exist.", "code": "PRODUCT_DOES_NOT_EXIST"})));

    let mut limit_proxy = snapshot_proxy();
    let limit = limit_proxy.process_request(json_graphql_request(
        by_product_update_query,
        json!({"priceListId": "gid://shopify/PriceList/test-9999", "pricesToAdd": [{"productId": "gid://shopify/Product/test", "price": {"amount": "12.00", "currencyCode": "EUR"}}], "pricesToDeleteByProductIds": []}),
    ));
    assert_eq!(
        limit.body["data"]["priceListFixedPricesByProductUpdate"]["userErrors"][0]["code"],
        json!("PRICE_LIMIT_EXCEEDED")
    );
    let limit_read = limit_proxy.process_request(json_graphql_request(
        read_query,
        json!({"id": "gid://shopify/PriceList/test-9999"}),
    ));
    assert_eq!(
        limit_read.body["data"]["priceList"]["fixedPricesCount"],
        json!(9999)
    );

    let mut by_product_proxy = snapshot_proxy();
    let valid_bulk = by_product_proxy.process_request(json_graphql_request(
        by_product_update_query,
        json!({"priceListId": "gid://shopify/PriceList/test", "pricesToAdd": [{"productId": "gid://shopify/Product/test", "price": {"amount": "12.00", "currencyCode": "EUR"}, "compareAtPrice": {"amount": "15.00", "currencyCode": "EUR"}}], "pricesToDeleteByProductIds": []}),
    ));
    assert_eq!(
        valid_bulk.body["data"]["priceListFixedPricesByProductUpdate"],
        json!({
            "priceList": {"id": "gid://shopify/PriceList/test", "fixedPricesCount": 1},
            "pricesToAddProducts": [{"id": "gid://shopify/Product/test", "title": "Test product"}],
            "pricesToDeleteProducts": [],
            "userErrors": []
        })
    );
    let valid_bulk_read = by_product_proxy.process_request(json_graphql_request(
        read_query,
        json!({"id": "gid://shopify/PriceList/test"}),
    ));
    let fixed_price_node = &valid_bulk_read.body["data"]["priceList"]["prices"]["edges"][0]["node"];
    assert_eq!(
        fixed_price_node["price"],
        json!({"amount": "12.0", "currencyCode": "EUR"})
    );
    assert_eq!(
        fixed_price_node["compareAtPrice"],
        json!({"amount": "15.0", "currencyCode": "EUR"})
    );
    assert_eq!(
        fixed_price_node["variant"]["product"],
        json!({"id": "gid://shopify/Product/test", "title": "Test product"})
    );

    let add_query = r#"
        mutation RustPriceListFixedPricesLocalRuntimeAdd($priceListId: ID!, $prices: [PriceListPriceInput!]!) {
          priceListFixedPricesAdd(priceListId: $priceListId, prices: $prices) {
            prices { originType price { amount currencyCode } variant { id } }
            userErrors { __typename field message code }
          }
        }
    "#;
    let update_query = r#"
        mutation RustPriceListFixedPricesLocalRuntimeUpdate($priceListId: ID!, $pricesToAdd: [PriceListPriceInput!]!, $variantIdsToDelete: [ID!]!) {
          priceListFixedPricesUpdate(priceListId: $priceListId, pricesToAdd: $pricesToAdd, variantIdsToDelete: $variantIdsToDelete) {
            priceList { id fixedPricesCount prices(first: 10, originType: FIXED) { edges { node { price { amount currencyCode } variant { id } } } } }
            pricesAdded { price { amount currencyCode } variant { id } }
            deletedFixedPriceVariantIds
            userErrors { __typename field message code }
          }
        }
    "#;
    let delete_query = r#"
        mutation RustPriceListFixedPricesLocalRuntimeDelete($priceListId: ID!, $variantIds: [ID!]!) {
          priceListFixedPricesDelete(priceListId: $priceListId, variantIds: $variantIds) {
            deletedFixedPriceVariantIds
            userErrors { __typename field message code }
          }
        }
    "#;

    let missing_price_list = snapshot_proxy().process_request(json_graphql_request(
        add_query,
        json!({"priceListId": "gid://shopify/PriceList/missing", "prices": [{"variantId": "gid://shopify/ProductVariant/alpha", "price": {"amount": "12.50", "currencyCode": "EUR"}}]}),
    ));
    assert_eq!(
        missing_price_list.body["data"]["priceListFixedPricesAdd"],
        json!({"prices": [], "userErrors": [{"__typename": "PriceListPriceUserError", "field": ["priceListId"], "message": "Price list does not exist.", "code": "PRICE_LIST_NOT_FOUND"}]})
    );

    let validation = snapshot_proxy().process_request(json_graphql_request(
        add_query,
        json!({"priceListId": "gid://shopify/PriceList/fixed", "prices": [
            {"variantId": "gid://shopify/ProductVariant/missing", "price": {"amount": "12.50", "currencyCode": "EUR"}},
            {"variantId": "gid://shopify/ProductVariant/alpha", "price": {"amount": "10.00", "currencyCode": "USD"}},
            {"variantId": "gid://shopify/ProductVariant/alpha", "price": {"amount": "11.00", "currencyCode": "EUR"}}
        ]}),
    ));
    let validation_errors = validation.body["data"]["priceListFixedPricesAdd"]["userErrors"]
        .as_array()
        .unwrap();
    assert!(validation_errors.contains(&json!({"__typename": "PriceListPriceUserError", "field": ["prices", "0", "variantId"], "message": "Product variant ID does not exist.", "code": "VARIANT_NOT_FOUND"})));
    assert!(validation_errors.contains(&json!({"__typename": "PriceListPriceUserError", "field": ["prices", "1", "price", "currencyCode"], "message": "The specified currency does not match the price list's currency.", "code": "PRICE_LIST_CURRENCY_MISMATCH"})));
    assert_eq!(
        validation.body["data"]["priceListFixedPricesAdd"]["prices"],
        json!([])
    );

    let mut duplicate_proxy = snapshot_proxy();
    let duplicate_add = duplicate_proxy.process_request(json_graphql_request(
        add_query,
        json!({"priceListId": "gid://shopify/PriceList/fixed", "prices": [
            {"variantId": "gid://shopify/ProductVariant/alpha", "price": {"amount": "12.50", "currencyCode": "EUR"}},
            {"variantId": "gid://shopify/ProductVariant/alpha", "price": {"amount": "13.75", "currencyCode": "EUR"}}
        ]}),
    ));
    assert_eq!(
        duplicate_add.body["data"]["priceListFixedPricesAdd"]["prices"],
        json!([{"originType": "FIXED", "price": {"amount": "13.75", "currencyCode": "EUR"}, "variant": {"id": "gid://shopify/ProductVariant/alpha"}}])
    );
    let duplicate_update = duplicate_proxy.process_request(json_graphql_request(
        update_query,
        json!({"priceListId": "gid://shopify/PriceList/fixed", "pricesToAdd": [
            {"variantId": "gid://shopify/ProductVariant/alpha", "price": {"amount": "14.00", "currencyCode": "EUR"}},
            {"variantId": "gid://shopify/ProductVariant/alpha", "price": {"amount": "15.00", "currencyCode": "EUR"}}
        ], "variantIdsToDelete": []}),
    ));
    assert_eq!(
        duplicate_update.body["data"]["priceListFixedPricesUpdate"]["pricesAdded"],
        json!([{"price": {"amount": "15.0", "currencyCode": "EUR"}, "variant": {"id": "gid://shopify/ProductVariant/alpha"}}])
    );
    assert_eq!(
        duplicate_update.body["data"]["priceListFixedPricesUpdate"]["userErrors"],
        json!([])
    );

    let mut lifecycle_proxy = snapshot_proxy();
    let add = lifecycle_proxy.process_request(json_graphql_request(
        add_query,
        json!({"priceListId": "gid://shopify/PriceList/fixed", "prices": [
            {"variantId": "gid://shopify/ProductVariant/alpha", "price": {"amount": "12.50", "currencyCode": "EUR"}},
            {"variantId": "gid://shopify/ProductVariant/beta", "price": {"amount": "20.00", "currencyCode": "EUR"}}
        ]}),
    ));
    assert_eq!(add.status, 200);
    let update = lifecycle_proxy.process_request(json_graphql_request(
        update_query,
        json!({"priceListId": "gid://shopify/PriceList/fixed", "pricesToAdd": [{"variantId": "gid://shopify/ProductVariant/alpha", "price": {"amount": "15.00", "currencyCode": "EUR"}}], "variantIdsToDelete": ["gid://shopify/ProductVariant/beta"]}),
    ));
    assert_eq!(
        update.body["data"]["priceListFixedPricesUpdate"]["deletedFixedPriceVariantIds"],
        json!(["gid://shopify/ProductVariant/beta"])
    );
    assert_eq!(
        update.body["data"]["priceListFixedPricesUpdate"]["priceList"]["fixedPricesCount"],
        json!(1)
    );
    assert_eq!(
        update.body["data"]["priceListFixedPricesUpdate"]["priceList"]["prices"]["edges"][0]
            ["node"]["price"],
        json!({"amount": "15.0", "currencyCode": "EUR"})
    );
    let delete = lifecycle_proxy.process_request(json_graphql_request(
        delete_query,
        json!({"priceListId": "gid://shopify/PriceList/fixed", "variantIds": ["gid://shopify/ProductVariant/alpha"]}),
    ));
    assert_eq!(
        delete.body["data"]["priceListFixedPricesDelete"],
        json!({"deletedFixedPriceVariantIds": ["gid://shopify/ProductVariant/alpha"], "userErrors": []})
    );
    let lifecycle_read = lifecycle_proxy.process_request(json_graphql_request(
        read_query,
        json!({"id": "gid://shopify/PriceList/fixed"}),
    ));
    assert_eq!(
        lifecycle_read.body["data"]["priceList"]["fixedPricesCount"],
        json!(0)
    );
    assert_eq!(
        lifecycle_read.body["data"]["priceList"]["prices"]["edges"],
        json!([])
    );

    let update_adds_missing = snapshot_proxy().process_request(json_graphql_request(
        update_query,
        json!({"priceListId": "gid://shopify/PriceList/fixed", "pricesToAdd": [{"variantId": "gid://shopify/ProductVariant/alpha", "price": {"amount": "15.00", "currencyCode": "EUR"}}], "variantIdsToDelete": []}),
    ));
    assert_eq!(
        update_adds_missing.body["data"]["priceListFixedPricesUpdate"]["priceList"]
            ["fixedPricesCount"],
        json!(1)
    );
    assert_eq!(
        update_adds_missing.body["data"]["priceListFixedPricesUpdate"]["pricesAdded"][0]["price"],
        json!({"amount": "15.0", "currencyCode": "EUR"})
    );

    let missing_fixed_delete = snapshot_proxy().process_request(json_graphql_request(
        delete_query,
        json!({"priceListId": "gid://shopify/PriceList/fixed", "variantIds": ["gid://shopify/ProductVariant/alpha"]}),
    ));
    assert_eq!(
        missing_fixed_delete.body["data"]["priceListFixedPricesDelete"]["userErrors"][0],
        json!({"__typename": "PriceListPriceUserError", "field": ["variantIds", "0"], "message": "Only fixed prices can be deleted.", "code": "PRICE_NOT_FIXED"})
    );
}

#[test]
fn price_list_create_update_delete_ported_gleam_helpers_stage_and_validate() {
    // Ports old Gleam price-list helper behavior from markets_mutation_test.gleam:
    // create validation, adjustment bounds, typed mutation user errors, name uniqueness,
    // staged reads, catalog attachment, and null-catalog detachment.
    let create_query = r#"
        mutation RustPriceListLocalRuntimeCreate($input: PriceListCreateInput!) {
          priceListCreate(input: $input) {
            priceList { id name currency parent { adjustment { type value } } catalog { id } }
            userErrors { __typename field message code }
          }
        }
    "#;
    let update_query = r#"
        mutation RustPriceListLocalRuntimeUpdate($id: ID!, $input: PriceListUpdateInput!) {
          priceListUpdate(id: $id, input: $input) {
            priceList { id name currency parent { adjustment { type value } } catalog { id } }
            userErrors { __typename field message code }
          }
        }
    "#;

    let validation_cases = [
        (
            json!({"name": "EUR", "parent": {"adjustment": {"type": "PERCENTAGE_DECREASE", "value": 10}}}),
            json!({"__typename": "PriceListUserError", "field": ["input", "currency"], "message": "Currency can't be blank", "code": "BLANK"}),
        ),
        (
            json!({"name": "EUR", "currency": "EUR"}),
            json!({"__typename": "PriceListUserError", "field": ["input", "parent"], "message": "Parent must exist", "code": "REQUIRED"}),
        ),
        (
            json!({"name": "EUR", "currency": "EUR", "parent": {"adjustment": {"type": "FIXED", "value": 10}}}),
            json!({"__typename": "PriceListUserError", "field": ["input", "parent", "adjustment", "type"], "message": "Type is invalid", "code": "INVALID"}),
        ),
        (
            json!({"name": "", "currency": "USD", "parent": {"adjustment": {"type": "PERCENTAGE_DECREASE", "value": 10}}}),
            json!({"__typename": "PriceListUserError", "field": ["input", "name"], "message": "Name can't be blank", "code": "BLANK"}),
        ),
        (
            json!({"name": "Negative", "currency": "USD", "parent": {"adjustment": {"type": "PERCENTAGE_DECREASE", "value": -10}}}),
            json!({"__typename": "PriceListUserError", "field": ["input", "parent", "adjustment", "value"], "message": "The adjustment value must be a positive value and not be greater than 100% for PERCENTAGE_DECREASE and not be greater than 1000% for PERCENTAGE_INCREASE.", "code": "INVALID_ADJUSTMENT_VALUE"}),
        ),
        (
            json!({"name": "Too Low", "currency": "USD", "parent": {"adjustment": {"type": "PERCENTAGE_DECREASE", "value": 250}}}),
            json!({"__typename": "PriceListUserError", "field": ["input", "parent", "adjustment", "value"], "message": "The adjustment value must be a positive value and not be greater than 100% for PERCENTAGE_DECREASE and not be greater than 1000% for PERCENTAGE_INCREASE.", "code": "INVALID_ADJUSTMENT_VALUE"}),
        ),
        (
            json!({"name": "Too High", "currency": "USD", "parent": {"adjustment": {"type": "PERCENTAGE_INCREASE", "value": 5000}}}),
            json!({"__typename": "PriceListUserError", "field": ["input", "parent", "adjustment", "value"], "message": "The adjustment value must be a positive value and not be greater than 100% for PERCENTAGE_DECREASE and not be greater than 1000% for PERCENTAGE_INCREASE.", "code": "INVALID_ADJUSTMENT_VALUE"}),
        ),
    ];
    for (input, error) in validation_cases {
        let mut proxy = snapshot_proxy();
        let response =
            proxy.process_request(json_graphql_request(create_query, json!({"input": input})));
        assert_eq!(response.status, 200);
        assert_eq!(
            response.body["data"]["priceListCreate"],
            json!({"priceList": null, "userErrors": [error]})
        );
    }

    let mut proxy = snapshot_proxy();
    let dkk = proxy.process_request(json_graphql_request(
        create_query,
        json!({"input": {"name": "Denmark", "currency": "DKK", "parent": {"adjustment": {"type": "PERCENTAGE_DECREASE", "value": 10}}}}),
    ));
    assert_eq!(dkk.status, 200);
    assert_eq!(
        dkk.body["data"]["priceListCreate"],
        json!({"priceList": {"id": "gid://shopify/PriceList/1", "name": "Denmark", "currency": "DKK", "parent": {"adjustment": {"type": "PERCENTAGE_DECREASE", "value": 10}}, "catalog": null}, "userErrors": []})
    );

    let zero_adjustment = proxy.process_request(json_graphql_request(
        create_query,
        json!({"input": {"name": "Zero", "currency": "USD", "parent": {"adjustment": {"type": "PERCENTAGE_DECREASE", "value": 0}}}}),
    ));
    assert_eq!(
        zero_adjustment.body["data"]["priceListCreate"]["priceList"]["id"],
        json!("gid://shopify/PriceList/2")
    );

    let duplicate = proxy.process_request(json_graphql_request(
        create_query,
        json!({"input": {"name": "Denmark", "currency": "CAD", "parent": {"adjustment": {"type": "PERCENTAGE_DECREASE", "value": 10}}}}),
    ));
    assert_eq!(
        duplicate.body["data"]["priceListCreate"],
        json!({"priceList": null, "userErrors": [{"__typename": "PriceListUserError", "field": ["input", "name"], "message": "Name has already been taken", "code": "TAKEN"}]})
    );

    let unique_update = proxy.process_request(json_graphql_request(
        update_query,
        json!({"id": "gid://shopify/PriceList/2", "input": {"name": "Unique A"}}),
    ));
    assert_eq!(
        unique_update.body["data"]["priceListUpdate"],
        json!({"priceList": {"id": "gid://shopify/PriceList/2", "name": "Unique A", "currency": "USD", "parent": {"adjustment": {"type": "PERCENTAGE_DECREASE", "value": 0}}, "catalog": null}, "userErrors": []})
    );
    let duplicate_update = proxy.process_request(json_graphql_request(
        update_query,
        json!({"id": "gid://shopify/PriceList/2", "input": {"name": "Denmark"}}),
    ));
    assert_eq!(
        duplicate_update.body["data"]["priceListUpdate"],
        json!({"priceList": null, "userErrors": [{"__typename": "PriceListUserError", "field": ["input", "name"], "message": "Name has already been taken", "code": "TAKEN"}]})
    );
    let invalid_parent_update = proxy.process_request(json_graphql_request(
        update_query,
        json!({"id": "gid://shopify/PriceList/2", "input": {"parent": {"adjustment": {"type": "PERCENTAGE_DECREASE", "value": 250}}}}),
    ));
    assert_eq!(
        invalid_parent_update.body["data"]["priceListUpdate"],
        json!({"priceList": {"id": "gid://shopify/PriceList/2", "name": "Unique A", "currency": "USD", "parent": {"adjustment": {"type": "PERCENTAGE_DECREASE", "value": 0}}, "catalog": null}, "userErrors": [{"__typename": "PriceListUserError", "field": ["input", "parent", "adjustment", "value"], "message": "The adjustment value must be a positive value and not be greater than 100% for PERCENTAGE_DECREASE and not be greater than 1000% for PERCENTAGE_INCREASE.", "code": "INVALID_ADJUSTMENT_VALUE"}]})
    );
    let read_after_failed_update = proxy.process_request(json_graphql_request(
        r#"
        query RustPriceListLocalRuntimeRead($id: ID!) {
          priceList(id: $id) { id name currency parent { adjustment { type value } } catalog { id } }
        }
        "#,
        json!({"id": "gid://shopify/PriceList/2"}),
    ));
    assert_eq!(
        read_after_failed_update.body["data"]["priceList"],
        json!({"id": "gid://shopify/PriceList/2", "name": "Unique A", "currency": "USD", "parent": {"adjustment": {"type": "PERCENTAGE_DECREASE", "value": 0}}, "catalog": null})
    );

    let typed_errors = proxy.process_request(json_graphql_request(
        r#"
        mutation RustPriceListLocalRuntimeTypedErrors {
          priceListCreate(input: { name: "", currency: USD, parent: { adjustment: { type: PERCENTAGE_DECREASE, value: 10 } } }) { priceList { id } userErrors { __typename field message code } }
          priceListUpdate(id: "gid://shopify/PriceList/0", input: { name: "Missing" }) { priceList { id } userErrors { __typename field message code } }
          priceListDelete(id: "gid://shopify/PriceList/0") { deletedId userErrors { __typename field message code } }
          quantityRulesDelete(priceListId: "gid://shopify/PriceList/0", variantIds: ["gid://shopify/ProductVariant/0"]) { deletedQuantityRulesVariantIds userErrors { __typename field message code } }
          webPresenceCreate(input: { defaultLocale: "en", subfolderSuffix: "x" }) { webPresence { id } userErrors { __typename field message code } }
          webPresenceUpdate(id: "gid://shopify/MarketWebPresence/0", input: { defaultLocale: "en" }) { webPresence { id } userErrors { __typename field message code } }
          webPresenceDelete(id: "gid://shopify/MarketWebPresence/0") { deletedId userErrors { __typename field message code } }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        typed_errors.body["data"]["priceListCreate"]["userErrors"][0],
        json!({"__typename": "PriceListUserError", "field": ["input", "name"], "message": "Name can't be blank", "code": "BLANK"})
    );
    assert_eq!(
        typed_errors.body["data"]["priceListUpdate"]["userErrors"][0],
        json!({"__typename": "PriceListUserError", "field": ["id"], "message": "Price list does not exist.", "code": "PRICE_LIST_NOT_FOUND"})
    );
    assert_eq!(
        typed_errors.body["data"]["priceListDelete"]["userErrors"][0],
        json!({"__typename": "PriceListUserError", "field": ["id"], "message": "Price list does not exist.", "code": "PRICE_LIST_NOT_FOUND"})
    );
    assert_eq!(
        typed_errors.body["data"]["quantityRulesDelete"]["userErrors"][0],
        json!({"__typename": "QuantityRuleUserError", "field": ["priceListId"], "message": "Price list does not exist.", "code": "PRICE_LIST_DOES_NOT_EXIST"})
    );
    assert_eq!(
        typed_errors.body["data"]["webPresenceCreate"]["userErrors"][0],
        json!({"__typename": "MarketUserError", "field": ["input", "subfolderSuffix"], "message": "Subfolder suffix must be at least 2 letters", "code": "SUBFOLDER_SUFFIX_MUST_BE_AT_LEAST_2_LETTERS"})
    );
    assert_eq!(
        typed_errors.body["data"]["webPresenceUpdate"]["userErrors"][0],
        json!({"__typename": "MarketUserError", "field": ["id"], "message": "The market web presence wasn't found.", "code": "WEB_PRESENCE_NOT_FOUND"})
    );
    assert_eq!(
        typed_errors.body["data"]["webPresenceDelete"]["userErrors"][0],
        json!({"__typename": "MarketUserError", "field": ["id"], "message": "The market web presence wasn't found.", "code": "WEB_PRESENCE_NOT_FOUND"})
    );

    let mut attached_proxy = snapshot_proxy();
    attached_proxy.process_request(json_graphql_request(
        r#"
        mutation RustMarketCreateLocalRuntimeCreate($input: MarketCreateInput!) {
          marketCreate(input: $input) { market { id } userErrors { field message code } }
        }
        "#,
        json!({"input": {"name": "Europe", "regions": [{"countryCode": "DK"}]}}),
    ));
    attached_proxy.process_request(json_graphql_request(
        r#"
        mutation RustCatalogLocalRuntimeCreate($input: CatalogCreateInput!) {
          catalogCreate(input: $input) { catalog { id } userErrors { field message code } }
        }
        "#,
        json!({"input": {"title": "EU Catalog", "status": "ACTIVE", "context": {"driverType": "MARKET", "marketIds": ["gid://shopify/Market/1"]}}}),
    ));
    let attached = attached_proxy.process_request(json_graphql_request(
        create_query,
        json!({"input": {"name": "EU Prices", "currency": "DKK", "catalogId": "gid://shopify/MarketCatalog/3", "parent": {"adjustment": {"type": "PERCENTAGE_DECREASE", "value": 10}}}}),
    ));
    assert_eq!(
        attached.body["data"]["priceListCreate"]["priceList"]["catalog"],
        json!({"id": "gid://shopify/MarketCatalog/3"})
    );
    let currency_mismatch_update = attached_proxy.process_request(json_graphql_request(
        update_query,
        json!({"id": "gid://shopify/PriceList/5", "input": {"currency": "USD"}}),
    ));
    assert_eq!(
        currency_mismatch_update.body["data"]["priceListUpdate"],
        json!({"priceList": {"id": "gid://shopify/PriceList/5", "name": "EU Prices", "currency": "USD", "parent": {"adjustment": {"type": "PERCENTAGE_DECREASE", "value": 10}}, "catalog": {"id": "gid://shopify/MarketCatalog/3"}}, "userErrors": []})
    );
    let detached = attached_proxy.process_request(json_graphql_request(
        r#"
        mutation RustPriceListLocalRuntimeUpdate($id: ID!, $input: PriceListUpdateInput!) {
          priceListUpdate(id: $id, input: $input) { priceList { id catalog { id } } userErrors { __typename field message code } }
        }
        "#,
        json!({"id": "gid://shopify/PriceList/5", "input": {"catalogId": null}}),
    ));
    assert_eq!(
        detached.body["data"]["priceListUpdate"],
        json!({"priceList": {"id": "gid://shopify/PriceList/5", "catalog": null}, "userErrors": []})
    );
    let readback = attached_proxy.process_request(json_graphql_request(
        r#"
        query RustPriceListLocalRuntimeRead($catalogId: ID!, $priceListId: ID!) {
          catalog(id: $catalogId) { id priceList { id } }
          priceList(id: $priceListId) { id catalog { id } }
          priceLists(first: 10) { nodes { id name currency } }
        }
        "#,
        json!({"catalogId": "gid://shopify/MarketCatalog/3", "priceListId": "gid://shopify/PriceList/5"}),
    ));
    assert_eq!(
        readback.body["data"]["catalog"],
        json!({"id": "gid://shopify/MarketCatalog/3", "priceList": null})
    );
    assert_eq!(
        readback.body["data"]["priceList"],
        json!({"id": "gid://shopify/PriceList/5", "catalog": null})
    );
    assert_eq!(
        readback.body["data"]["priceLists"]["nodes"][0],
        json!({"id": "gid://shopify/PriceList/5", "name": "EU Prices", "currency": "USD"})
    );
}

#[test]
fn market_localizations_register_remove_ported_gleam_helpers_stage_and_validate() {
    // Ports old Gleam proxy tests:
    // - market_localizations_register_rejects_more_than_100_keys_test
    // - market_localizations_register_returns_translation_error_for_missing_resource_test
    // - market_localizations_remove_returns_translation_error_for_missing_resource_test
    // - market_localizations_register_validates_market_key_digest_and_value_test
    // - market_localizations_register_stages_seeded_content_test
    // - market_localizations_remove_deletes_matching_staged_records_test
    // - market_localizations_remove_returns_null_when_no_staged_records_match_test
    // - market_localizations_remove_unmatched_filters_noop_test
    // - market_localizations_remove_returns_removed_staged_rows_test
    let mut proxy = snapshot_proxy();
    let resource_id = "gid://shopify/Metafield/localizable";
    let register_query = r#"
        mutation RustMarketLocalizationsLocalRuntimeRegister($resourceId: ID!, $marketLocalizations: [MarketLocalizationInput!]!) {
          marketLocalizationsRegister(resourceId: $resourceId, marketLocalizations: $marketLocalizations) {
            marketLocalizations { key value outdated market { id name } }
            userErrors { __typename field code }
          }
        }
    "#;
    let remove_query = r#"
        mutation RustMarketLocalizationsLocalRuntimeRemove($resourceId: ID!, $keys: [String!]!, $marketIds: [ID!]!) {
          marketLocalizationsRemove(resourceId: $resourceId, marketLocalizationKeys: $keys, marketIds: $marketIds) {
            marketLocalizations { key value outdated market { id name } }
            userErrors { __typename field code }
          }
        }
    "#;
    let read_query = r#"
        query RustMarketLocalizationsLocalRuntimeRead($resourceId: ID!) {
          marketLocalizableResource(resourceId: $resourceId) {
            marketLocalizableContent { key value digest }
            marketLocalizations { key value outdated market { id name } }
          }
        }
    "#;
    let valid_title = json!({
        "marketId": "gid://shopify/Market/ca",
        "key": "title",
        "value": "Titre",
        "marketLocalizableContentDigest": "digest-title"
    });
    let valid_subtitle = json!({
        "marketId": "gid://shopify/Market/ca",
        "key": "subtitle",
        "value": "Sous-titre",
        "marketLocalizableContentDigest": "digest-subtitle"
    });

    let too_many = (1..=101)
        .map(|index| {
            json!({
                "marketId": format!("gid://shopify/Market/{index}"),
                "key": "title",
                "value": "Titre",
                "marketLocalizableContentDigest": "digest-title"
            })
        })
        .collect::<Vec<_>>();
    let too_many_response = proxy.process_request(json_graphql_request(
        register_query,
        json!({"resourceId": "gid://shopify/Metafield/missing", "marketLocalizations": too_many}),
    ));
    assert_eq!(too_many_response.status, 200);
    assert_eq!(
        too_many_response.body["data"]["marketLocalizationsRegister"],
        json!({
            "marketLocalizations": null,
            "userErrors": [{"__typename": "TranslationUserError", "field": ["resourceId"], "code": "TOO_MANY_KEYS_FOR_RESOURCE"}]
        })
    );

    let missing_register = proxy.process_request(json_graphql_request(
        register_query,
        json!({"resourceId": "gid://shopify/Metafield/missing", "marketLocalizations": [valid_title.clone()]}),
    ));
    assert_eq!(
        missing_register.body["data"]["marketLocalizationsRegister"]["userErrors"][0],
        json!({"__typename": "TranslationUserError", "field": ["resourceId"], "code": "RESOURCE_NOT_FOUND"})
    );

    let missing_remove = proxy.process_request(json_graphql_request(
        remove_query,
        json!({"resourceId": "gid://shopify/Metafield/missing", "keys": [], "marketIds": []}),
    ));
    assert_eq!(
        missing_remove.body["data"]["marketLocalizationsRemove"],
        json!({
            "marketLocalizations": null,
            "userErrors": [{"__typename": "TranslationUserError", "field": ["resourceId"], "code": "RESOURCE_NOT_FOUND"}]
        })
    );

    let validation_cases = [
        (
            json!({"marketId": "gid://shopify/Market/missing", "key": "title", "value": "Titre", "marketLocalizableContentDigest": "digest-title"}),
            json!({"__typename": "TranslationUserError", "field": ["marketLocalizations", "0", "marketId"], "code": "MARKET_DOES_NOT_EXIST"}),
        ),
        (
            json!({"marketId": "gid://shopify/Market/ca", "key": "value", "value": "Titre", "marketLocalizableContentDigest": "digest-title"}),
            json!({"__typename": "TranslationUserError", "field": ["marketLocalizations", "0", "key"], "code": "INVALID_KEY_FOR_MODEL"}),
        ),
        (
            json!({"marketId": "gid://shopify/Market/ca", "key": "title", "value": "Titre", "marketLocalizableContentDigest": "stale"}),
            json!({"__typename": "TranslationUserError", "field": ["marketLocalizations", "0", "marketLocalizableContentDigest"], "code": "INVALID_MARKET_LOCALIZABLE_CONTENT"}),
        ),
        (
            json!({"marketId": "gid://shopify/Market/ca", "key": "title", "value": "", "marketLocalizableContentDigest": "digest-title"}),
            json!({"__typename": "TranslationUserError", "field": ["marketLocalizations", "0", "value"], "code": "FAILS_RESOURCE_VALIDATION"}),
        ),
    ];
    for (input, expected_error) in validation_cases {
        let response = proxy.process_request(json_graphql_request(
            register_query,
            json!({"resourceId": resource_id, "marketLocalizations": [input]}),
        ));
        assert_eq!(
            response.body["data"]["marketLocalizationsRegister"],
            json!({"marketLocalizations": null, "userErrors": [expected_error]})
        );
    }

    let register = proxy.process_request(json_graphql_request(
        register_query,
        json!({"resourceId": resource_id, "marketLocalizations": [valid_title.clone(), valid_subtitle.clone()]}),
    ));
    assert_eq!(
        register.body["data"]["marketLocalizationsRegister"],
        json!({
            "marketLocalizations": [
                {"key": "title", "value": "Titre", "outdated": false, "market": {"id": "gid://shopify/Market/ca", "name": "Canada"}},
                {"key": "subtitle", "value": "Sous-titre", "outdated": false, "market": {"id": "gid://shopify/Market/ca", "name": "Canada"}}
            ],
            "userErrors": []
        })
    );

    let read_after_register = proxy.process_request(json_graphql_request(
        read_query,
        json!({"resourceId": resource_id}),
    ));
    assert_eq!(
        read_after_register.body["data"]["marketLocalizableResource"]["marketLocalizableContent"],
        json!([
            {"key": "title", "value": "Title", "digest": "digest-title"},
            {"key": "subtitle", "value": "Subtitle", "digest": "digest-subtitle"}
        ])
    );
    assert_eq!(
        read_after_register.body["data"]["marketLocalizableResource"]["marketLocalizations"]
            .as_array()
            .map(Vec::len),
        Some(2)
    );

    for (keys, market_ids) in [
        (json!([]), json!(["gid://shopify/Market/ca"])),
        (json!(["unknown"]), json!(["gid://shopify/Market/ca"])),
        (json!(["title"]), json!(["gid://shopify/Market/missing"])),
    ] {
        let noop = proxy.process_request(json_graphql_request(
            remove_query,
            json!({"resourceId": resource_id, "keys": keys, "marketIds": market_ids}),
        ));
        assert_eq!(
            noop.body["data"]["marketLocalizationsRemove"],
            json!({"marketLocalizations": null, "userErrors": []})
        );
    }

    let remove_title = proxy.process_request(json_graphql_request(
        remove_query,
        json!({"resourceId": resource_id, "keys": ["title"], "marketIds": ["gid://shopify/Market/ca"]}),
    ));
    assert_eq!(
        remove_title.body["data"]["marketLocalizationsRemove"],
        json!({
            "marketLocalizations": [{"key": "title", "value": "Titre", "outdated": false, "market": {"id": "gid://shopify/Market/ca", "name": "Canada"}}],
            "userErrors": []
        })
    );
    let read_after_title_remove = proxy.process_request(json_graphql_request(
        read_query,
        json!({"resourceId": resource_id}),
    ));
    assert_eq!(
        read_after_title_remove.body["data"]["marketLocalizableResource"]["marketLocalizations"],
        json!([{"key": "subtitle", "value": "Sous-titre", "outdated": false, "market": {"id": "gid://shopify/Market/ca", "name": "Canada"}}])
    );

    let remove_subtitle = proxy.process_request(json_graphql_request(
        remove_query,
        json!({"resourceId": resource_id, "keys": ["subtitle"], "marketIds": ["gid://shopify/Market/ca"]}),
    ));
    assert_eq!(
        remove_subtitle.body["data"]["marketLocalizationsRemove"]["marketLocalizations"][0]["key"],
        json!("subtitle")
    );
    let second_remove = proxy.process_request(json_graphql_request(
        remove_query,
        json!({"resourceId": resource_id, "keys": ["subtitle"], "marketIds": ["gid://shopify/Market/ca"]}),
    ));
    assert_eq!(
        second_remove.body["data"]["marketLocalizationsRemove"],
        json!({"marketLocalizations": null, "userErrors": []})
    );
    let read_after_remove = proxy.process_request(json_graphql_request(
        read_query,
        json!({"resourceId": resource_id}),
    ));
    assert_eq!(
        read_after_remove.body["data"]["marketLocalizableResource"]["marketLocalizations"],
        json!([])
    );
}

#[test]
fn product_fixture_backed_helper_and_variant_reads_preserve_captured_shapes() {
    let mut proxy = snapshot_proxy();
    let helper_query =
        include_str!("../../config/parity-requests/products/product-helper-roots-read.graphql");
    let helper = proxy.process_request(json_graphql_request(
        helper_query,
        json!({
            "helperProductId": "gid://shopify/Product/9801098789170",
            "productHandle": "the-inventory-not-tracked-snowboard",
            "variantId": "gid://shopify/ProductVariant/49875425296690",
            "missingProductId": "gid://shopify/Product/999999999999",
            "missingVariantId": "gid://shopify/ProductVariant/999999999999",
            "missingJobId": "gid://shopify/ProductDuplicateJob/999999999999",
            "missingOperationId": "gid://shopify/ProductSetOperation/999999999999"
        }),
    ));
    assert_eq!(helper.status, 200);
    assert_eq!(
        helper.body["data"]["byId"],
        json!({
            "id": "gid://shopify/Product/9801098789170",
            "handle": "the-inventory-not-tracked-snowboard",
            "title": "The Inventory Not Tracked Snowboard"
        })
    );
    assert_eq!(helper.body["data"]["missingProduct"], Value::Null);
    assert_eq!(
        helper.body["data"]["variantById"],
        json!({
            "id": "gid://shopify/ProductVariant/49875425296690",
            "title": "Default Title",
            "sku": "sku-untracked-1",
            "product": { "id": "gid://shopify/Product/9801098789170" }
        })
    );
    assert_eq!(
        helper.body["data"]["productVariantsCount"],
        json!({ "count": 2279, "precision": "EXACT" })
    );
    assert_eq!(
        helper.body["data"]["productDuplicateJob"],
        json!({ "id": "gid://shopify/ProductDuplicateJob/999999999999", "done": true })
    );

    let variant_query =
        include_str!("../../config/parity-requests/products/product-variants-read.graphql");
    let variant = proxy.process_request(json_graphql_request(
        variant_query,
        json!({
            "productId": "gid://shopify/Product/8971842846953",
            "variantId": "gid://shopify/ProductVariant/48540157378793",
            "inventoryItemId": "gid://shopify/InventoryItem/50643009569001"
        }),
    ));
    assert_eq!(variant.status, 200);
    assert_eq!(
        variant.body["data"]["product"]["variants"]["edges"][0]["node"]["inventoryItem"],
        variant.body["data"]["variant"]["inventoryItem"]
    );
    assert_eq!(
        variant.body["data"]["stock"],
        variant.body["data"]["variant"]["inventoryItem"]
    );
    assert_eq!(
        variant.body["data"]["stockBackreference"]["variant"],
        json!({
            "id": "gid://shopify/ProductVariant/48540157378793",
            "title": "Default Title",
            "sku": null,
            "inventoryQuantity": 0,
            "product": {
                "id": "gid://shopify/Product/8971842846953",
                "title": "Test Product - 6635"
            }
        })
    );
}

#[test]
fn collections_catalog_read_replays_captured_catalog_branches() {
    let mut proxy = snapshot_proxy();
    let query =
        include_str!("../../config/parity-requests/products/collections-catalog-read.graphql");
    let response = proxy.process_request(json_graphql_request(
        query,
        json!({
            "catalogFirst": 20,
            "first": 3,
            "titleWildcardQuery": "title:VAN*",
            "customTypeQuery": "collection_type:custom",
            "smartTypeQuery": "collection_type:smart",
            "updatedSortQuery": "collection_type:smart",
            "emptyQuery": "title:No collection should match this 157*",
            "productMembershipQuery": "product_id:8397255672041"
        }),
    ));
    assert_eq!(response.status, 200);
    let first_collection = &response.body["data"]["collections"]["edges"][0]["node"];
    assert_eq!(
        first_collection["id"],
        json!("gid://shopify/Collection/402476531945")
    );
    assert_eq!(first_collection["legacyResourceId"], json!("402476531945"));
    assert_eq!(first_collection["title"], json!("Home page"));
    assert_eq!(first_collection["handle"], json!("frontpage"));
    assert_eq!(
        first_collection["products"]["edges"][0]["node"],
        json!({
            "id": "gid://shopify/Product/8397254426857",
            "title": "VANS |AUTHENTIC | LO PRO | BURGANDY/WHITE",
            "handle": "vans-authentic-lo-pro-burgandy-white",
            "vendor": "VANS"
        })
    );
    assert_eq!(
        response.body["data"]["emptyUnmatched"],
        json!({
            "edges": [],
            "pageInfo": {
                "hasNextPage": false,
                "hasPreviousPage": false,
                "startCursor": null,
                "endCursor": null
            }
        })
    );
    assert_eq!(
        response.body["data"]["titleWildcard"]["edges"][0]["node"]["handle"],
        json!("vans")
    );
}

fn assert_product_fixture_backed_read_matches_capture(
    query: &str,
    variables: Value,
    fixture: &str,
) {
    let mut proxy = snapshot_proxy();
    let expected: Value = serde_json::from_str(fixture).expect("product read fixture must parse");
    let expected_data = expected
        .get("data")
        .or_else(|| {
            expected
                .get("response")
                .and_then(|response| response.get("data"))
        })
        .cloned()
        .unwrap_or(Value::Null);
    let response = proxy.process_request(json_graphql_request(query, variables));
    assert_eq!(response.status, 200);
    assert_eq!(response.body["data"], expected_data);
}

#[test]
fn product_catalog_and_search_reads_replay_captured_fixture_data() {
    assert_product_fixture_backed_read_matches_capture(
        include_str!("../../config/parity-requests/products/products-catalog-read.graphql"),
        serde_json::from_str(include_str!("../../config/parity-requests/products/products-catalog-read.variables.json")).unwrap(),
        include_str!("../../fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/products-catalog-page.json"),
    );
    assert_product_fixture_backed_read_matches_capture(
        include_str!("../../config/parity-requests/products/products-sort-keys-read.graphql"),
        serde_json::from_str(include_str!("../../config/parity-requests/products/products-sort-keys-read.variables.json")).unwrap(),
        include_str!("../../fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/products-sort-keys.json"),
    );
    assert_product_fixture_backed_read_matches_capture(
        include_str!("../../config/parity-requests/products/products-search-read.graphql"),
        serde_json::from_str(include_str!("../../config/parity-requests/products/products-search-read.variables.json")).unwrap(),
        include_str!("../../fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/products-search.json"),
    );
    assert_product_fixture_backed_read_matches_capture(
        include_str!("../../config/parity-requests/products/products-search-pagination-read.graphql"),
        serde_json::from_str(include_str!("../../config/parity-requests/products/products-search-pagination-read.variables.json")).unwrap(),
        include_str!("../../fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/products-search-pagination.json"),
    );
    assert_product_fixture_backed_read_matches_capture(
        include_str!("../../config/parity-requests/products/products-advanced-search-read.graphql"),
        serde_json::from_str(include_str!("../../config/parity-requests/products/products-advanced-search-read.variables.json")).unwrap(),
        include_str!("../../fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/products-advanced-search.json"),
    );
    assert_product_fixture_backed_read_matches_capture(
        include_str!("../../config/parity-requests/products/products-or-precedence-read.graphql"),
        serde_json::from_str(include_str!("../../config/parity-requests/products/products-or-precedence-read.variables.json")).unwrap(),
        include_str!("../../fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/products-or-precedence.json"),
    );
    assert_product_fixture_backed_read_matches_capture(
        include_str!("../../config/parity-requests/products/products-relevance-search-read.graphql"),
        serde_json::from_str(include_str!("../../config/parity-requests/products/products-relevance-search-read.variables.json")).unwrap(),
        include_str!("../../fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/products-relevance-search.json"),
    );
    assert_product_fixture_backed_read_matches_capture(
        include_str!("../../config/parity-requests/products/products-search-grammar-read.graphql"),
        serde_json::from_str(include_str!("../../config/parity-requests/products/products-search-grammar-read.variables.json")).unwrap(),
        include_str!("../../fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/products-search-grammar.json"),
    );
    assert_product_fixture_backed_read_matches_capture(
        include_str!("../../config/parity-requests/products/products-variant-search-read.graphql"),
        serde_json::from_str(include_str!("../../config/parity-requests/products/products-variant-search-read.variables.json")).unwrap(),
        include_str!("../../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/products-variant-search.json"),
    );
    assert_product_fixture_backed_read_matches_capture(
        include_str!("../../config/parity-requests/products/product-detail-read.graphql"),
        serde_json::from_str(include_str!("../../config/parity-requests/products/product-detail-read.variables.json")).unwrap(),
        include_str!("../../fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/product-detail.json"),
    );
    assert_product_fixture_backed_read_matches_capture(
        include_str!("../../config/parity-requests/products/product-metafields-read.graphql"),
        serde_json::from_str(include_str!("../../config/parity-requests/products/product-metafields-read.variables.json")).unwrap(),
        include_str!("../../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/product-metafields.json"),
    );
    assert_product_fixture_backed_read_matches_capture(
        include_str!("../../config/parity-requests/products/collection-detail-read.graphql"),
        serde_json::from_str(include_str!("../../config/parity-requests/products/collection-detail-read.variables.json")).unwrap(),
        include_str!("../../fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/collection-detail.json"),
    );
}

fn captured_payload_data(fixture: &Value, path: &[&str]) -> Value {
    let mut value = fixture;
    for key in path {
        value = &value[*key];
    }
    value
        .get("response")
        .and_then(|response| response.get("payload"))
        .or_else(|| value.get("response"))
        .and_then(|response| response.get("data"))
        .or_else(|| value.get("data"))
        .cloned()
        .unwrap_or(Value::Null)
}

#[test]
fn product_create_rich_fixture_readbacks_preserve_captured_product_shapes() {
    let create_cases = [
        (
            include_str!("../../config/parity-requests/products/productCreate-with-options-parity.graphql"),
            include_str!("../../config/parity-requests/products/productCreate-with-options-downstream-read.graphql"),
            include_str!("../../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/product-create-with-options-parity.json"),
            &[][..],
            "options",
        ),
        (
            include_str!("../../config/parity-requests/products/productCreate-with-options-parity.graphql"),
            include_str!("../../config/parity-requests/products/productCreate-with-options-downstream-read.graphql"),
            include_str!("../../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/product-create-with-options-multi-value-parity.json"),
            &[][..],
            "options",
        ),
        (
            include_str!("../../config/parity-requests/products/productCreate-inventory-read-parity.graphql"),
            include_str!("../../config/parity-requests/products/productCreate-inventory-read-downstream.graphql"),
            include_str!("../../fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/product-create-inventory-read-parity.json"),
            &[][..],
            "inventory",
        ),
        (
            include_str!("../../config/parity-requests/products/productCreate-category-parity.graphql"),
            include_str!("../../config/parity-requests/products/productCreate-category-downstream-read.graphql"),
            include_str!("../../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/productCreate-category-parity.json"),
            &[][..],
            "category",
        ),
        (
            include_str!("../../config/parity-requests/products/productCreate-collections-to-join-parity.graphql"),
            include_str!("../../config/parity-requests/products/productCreate-collections-to-join-downstream-read.graphql"),
            include_str!("../../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/productCreate-collections-to-join-parity.json"),
            &[][..],
            "collections",
        ),
        (
            include_str!("../../config/parity-requests/products/productCreate-requires-selling-plan-parity.graphql"),
            include_str!("../../config/parity-requests/products/productCreate-requires-selling-plan-downstream-read.graphql"),
            include_str!("../../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/productCreate-requires-selling-plan-parity.json"),
            &[][..],
            "requiresSellingPlan",
        ),
        (
            include_str!("../../config/parity-requests/products/productCreate-dropped-inputs-parity.graphql"),
            include_str!("../../config/parity-requests/products/productCreate-dropped-inputs-downstream-read.graphql"),
            include_str!("../../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/productCreate-dropped-inputs-parity.json"),
            &["giftCardAndMetafields"][..],
            "giftCard",
        ),
    ];

    for (mutation_query, downstream_query, fixture_source, section_path, kind) in create_cases {
        let fixture: Value = serde_json::from_str(fixture_source).unwrap();
        let mut proxy = snapshot_proxy();
        let mutation_section = if section_path.is_empty() {
            &fixture["mutation"]
        } else {
            &fixture[section_path[0]]["mutation"]
        };
        let mutation = proxy.process_request(json_graphql_request(
            mutation_query,
            mutation_section["variables"].clone(),
        ));
        assert_eq!(mutation.status, 200, "{kind} mutation status");
        assert_eq!(
            mutation.body["data"],
            captured_payload_data(&fixture, &[section_path, &["mutation"]].concat()),
            "{kind} mutation data"
        );

        let product_id = mutation.body["data"]["productCreate"]["product"]["id"]
            .as_str()
            .unwrap();
        let downstream_variables = match kind {
            "inventory" => json!({
                "productId": product_id,
                "variantId": mutation.body["data"]["productCreate"]["product"]["variants"]["nodes"][0]["id"],
                "inventoryItemId": mutation.body["data"]["productCreate"]["product"]["variants"]["nodes"][0]["inventoryItem"]["id"]
            }),
            _ => json!({ "id": product_id }),
        };
        let downstream =
            proxy.process_request(json_graphql_request(downstream_query, downstream_variables));
        assert_eq!(downstream.status, 200, "{kind} downstream status");
        assert_eq!(
            downstream.body["data"],
            captured_payload_data(&fixture, &[section_path, &["downstreamRead"]].concat()),
            "{kind} downstream data"
        );
    }
}

#[test]
fn product_variants_bulk_create_strategy_downstreams_replay_captured_variant_shapes() {
    let query = include_str!(
        "../../config/parity-requests/products/product-option-variant-strategy-edge-downstream-read.graphql"
    );
    for (product_id, fixture_source, expected_sku) in [
        (
            "gid://shopify/Product/10172135506226",
            include_str!("../../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/productVariantsBulkCreate-strategy-default-custom-standalone.json"),
            "HERMES-1777346728237-BULK-DEFAULT-CUSTOM",
        ),
        (
            "gid://shopify/Product/10172135440690",
            include_str!("../../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/productVariantsBulkCreate-strategy-default-default-standalone.json"),
            "HERMES-1777346728237-BULK-DEFAULT-DEFAULT",
        ),
        (
            "gid://shopify/Product/10172135538994",
            include_str!("../../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/productVariantsBulkCreate-strategy-remove-custom-standalone.json"),
            "HERMES-1777346728237-BULK-REMOVE-CUSTOM",
        ),
        (
            "gid://shopify/Product/10172135473458",
            include_str!("../../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/productVariantsBulkCreate-strategy-remove-default-standalone.json"),
            "HERMES-1777346728237-BULK-REMOVE-DEFAULT",
        ),
    ] {
        let fixture: Value = serde_json::from_str(fixture_source).unwrap();
        let mut proxy = snapshot_proxy();
        let response = proxy.process_request(json_graphql_request(
            query,
            json!({ "id": product_id }),
        ));
        assert_eq!(response.status, 200);
        assert_eq!(response.body["data"], fixture["downstreamRead"]["data"]);
        assert_eq!(
            response.body["data"]["product"]["variants"]["nodes"]
                .as_array()
                .unwrap()
                .iter()
                .any(|variant| variant["sku"] == json!(expected_sku)),
            true
        );
    }
}

#[test]
fn product_set_fixture_replay_preserves_mutation_and_downstream_product_graphs() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/product-set-parity.json"
    ))
    .unwrap();
    let mut proxy = snapshot_proxy();
    let mutation_query =
        include_str!("../../config/parity-requests/products/productSet-parity-plan.graphql");
    let read_query =
        include_str!("../../config/parity-requests/products/productSet-downstream-read.graphql");

    let create = proxy.process_request(json_graphql_request(
        mutation_query,
        fixture["mutation"]["variables"].clone(),
    ));
    assert_eq!(create.status, 200);
    assert_eq!(create.body["data"], fixture["mutation"]["response"]["data"]);
    assert_eq!(
        create.body["data"]["productSet"]["product"]["variants"]["nodes"][0]["inventoryItem"]
            ["inventoryLevels"]["nodes"][0]["quantities"],
        json!([
            {"name": "available", "quantity": 2, "updatedAt": "2026-04-25T23:03:30Z"},
            {"name": "on_hand", "quantity": 2, "updatedAt": null},
            {"name": "incoming", "quantity": 0, "updatedAt": null}
        ])
    );

    let create_read = proxy.process_request(json_graphql_request(
        read_query,
        fixture["downstreamReadVariables"].clone(),
    ));
    assert_eq!(create_read.status, 200);
    assert_eq!(create_read.body["data"], fixture["downstreamRead"]["data"]);
    assert_eq!(
        create_read.body["data"]["variantOne"]["id"],
        create_read.body["data"]["product"]["variants"]["nodes"][0]["id"]
    );
    assert_eq!(
        create_read.body["data"]["variantOne"]["inventoryItem"],
        create_read.body["data"]["stockOne"]
            .as_object()
            .map(|stock| {
                let mut item = stock.clone();
                item.remove("variant");
                Value::Object(item)
            })
            .unwrap()
    );

    let update = proxy.process_request(json_graphql_request(
        mutation_query,
        fixture["update"]["mutation"]["variables"].clone(),
    ));
    assert_eq!(update.status, 200);
    assert_eq!(
        update.body["data"],
        fixture["update"]["mutation"]["response"]["data"]
    );

    let update_read = proxy.process_request(json_graphql_request(
        read_query,
        fixture["update"]["downstreamReadVariables"].clone(),
    ));
    assert_eq!(update_read.status, 200);
    assert_eq!(
        update_read.body["data"],
        fixture["update"]["downstreamRead"]["data"]
    );
    assert_eq!(
        update_read.body["data"]["product"]["variants"]["nodes"][0]["sku"],
        json!("GRAPH-BLUE-UPDATED-1777158209644")
    );
}

#[test]
fn custom_data_metafield_type_matrix_sets_and_reads_product_owned_values() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/metafields/custom-data-field-type-matrix.json"
    ))
    .unwrap();
    let mut proxy = snapshot_proxy();
    let set_query = include_str!(
        "../../config/parity-requests/metafields/custom-data-metafield-type-matrix-set.graphql"
    );
    let read_query = include_str!(
        "../../config/parity-requests/metafields/custom-data-metafield-type-matrix-read.graphql"
    );

    for batch in fixture["metafieldBatches"].as_array().unwrap() {
        let set_variables = batch["mutation"]["request"]["variables"].clone();
        let set_response = proxy.process_request(json_graphql_request(set_query, set_variables));
        assert_eq!(set_response.status, 200);
        assert_eq!(
            set_response.body["data"]["metafieldsSet"]["userErrors"],
            json!([])
        );
        assert_eq!(
            set_response.body["data"]["metafieldsSet"]["metafields"]
                .as_array()
                .unwrap()
                .len(),
            batch["mutation"]["request"]["variables"]["metafields"]
                .as_array()
                .unwrap()
                .len()
        );

        let read_variables = batch["downstreamRead"]["request"]["variables"].clone();
        let read_response = proxy.process_request(json_graphql_request(read_query, read_variables));
        assert_eq!(read_response.status, 200);
        let expected_nodes = batch["downstreamRead"]["response"]["data"]["product"]["metafields"]
            ["nodes"]
            .as_array()
            .unwrap();
        let actual_nodes = read_response.body["data"]["product"]["metafields"]["nodes"]
            .as_array()
            .unwrap();
        assert_eq!(actual_nodes.len(), expected_nodes.len());
        assert_eq!(actual_nodes[0]["namespace"], expected_nodes[0]["namespace"]);
        assert_eq!(actual_nodes[0]["key"], expected_nodes[0]["key"]);
        assert_eq!(actual_nodes[0]["type"], expected_nodes[0]["type"]);
        assert_eq!(actual_nodes[0]["value"], expected_nodes[0]["value"]);
        assert_eq!(actual_nodes[0]["jsonValue"], expected_nodes[0]["jsonValue"]);
        assert_eq!(actual_nodes[0]["ownerType"], expected_nodes[0]["ownerType"]);
    }
}

#[test]
fn product_metafields_set_replays_captured_product_owned_readbacks() {
    let cases = [
        "metafields-set-parity.json",
        "metafields-set-cas-success-parity.json",
        "metafields-set-stale-digest-parity.json",
        "metafields-set-duplicate-input-parity.json",
        "metafields-set-missing-type-parity.json",
        "metafields-set-null-create-parity.json",
        "metafields-set-missing-namespace-parity.json",
        "metafields-set-over-limit-parity.json",
    ];
    let mutation_query =
        include_str!("../../config/parity-requests/products/metafieldsSet-parity-plan.graphql");
    let read_query =
        include_str!("../../config/parity-requests/products/metafieldsSet-downstream-read.graphql");

    for case in cases {
        let fixture: Value = serde_json::from_str(match case {
            "metafields-set-parity.json" => include_str!("../../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/metafields-set-parity.json"),
            "metafields-set-cas-success-parity.json" => include_str!("../../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/metafields-set-cas-success-parity.json"),
            "metafields-set-stale-digest-parity.json" => include_str!("../../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/metafields-set-stale-digest-parity.json"),
            "metafields-set-duplicate-input-parity.json" => include_str!("../../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/metafields-set-duplicate-input-parity.json"),
            "metafields-set-missing-type-parity.json" => include_str!("../../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/metafields-set-missing-type-parity.json"),
            "metafields-set-null-create-parity.json" => include_str!("../../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/metafields-set-null-create-parity.json"),
            "metafields-set-missing-namespace-parity.json" => include_str!("../../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/metafields-set-missing-namespace-parity.json"),
            "metafields-set-over-limit-parity.json" => include_str!("../../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/metafields-set-over-limit-parity.json"),
            _ => unreachable!(),
        })
        .unwrap();
        let mut proxy = snapshot_proxy();

        let mutation = proxy.process_request(json_graphql_request(
            mutation_query,
            fixture["mutation"]["variables"].clone(),
        ));
        assert_eq!(mutation.status, 200, "{case}");
        assert_eq!(
            mutation.body["data"], fixture["mutation"]["response"]["data"],
            "{case} mutation payload"
        );

        let downstream = proxy.process_request(json_graphql_request(
            read_query,
            fixture["downstreamReadVariables"].clone(),
        ));
        assert_eq!(downstream.status, 200, "{case}");
        assert_eq!(
            downstream.body["data"], fixture["downstreamRead"]["data"],
            "{case} downstream payload"
        );
    }
}

#[test]
fn product_metafields_set_owner_expansion_replays_variant_and_collection_readbacks() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/metafields-set-owner-expansion-parity.json"
    ))
    .unwrap();
    let mutation_query =
        include_str!("../../config/parity-requests/products/metafieldsSet-owner-expansion.graphql");
    let read_query = include_str!(
        "../../config/parity-requests/products/metafieldsSet-owner-expansion-downstream-read.graphql"
    );
    let mut proxy = snapshot_proxy();

    let mutation = proxy.process_request(json_graphql_request(
        mutation_query,
        fixture["mutation"]["variables"].clone(),
    ));
    assert_eq!(mutation.status, 200);
    assert_eq!(
        mutation.body["data"],
        fixture["mutation"]["response"]["data"]
    );

    let downstream = proxy.process_request(json_graphql_request(
        read_query,
        fixture["downstreamReadVariables"].clone(),
    ));
    assert_eq!(downstream.status, 200);
    assert_eq!(downstream.body["data"], fixture["downstreamRead"]["data"]);
    assert_eq!(
        downstream.body["data"]["product"]["variants"]["nodes"][0]["care"]["value"],
        json!("Spot clean")
    );
    assert_eq!(
        downstream.body["data"]["collection"]["season"]["value"],
        json!("Winter")
    );
}

#[test]
fn product_metafields_delete_replays_captured_product_owned_readback() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/metafields-delete-parity.json"
    ))
    .unwrap();
    let mutation_query =
        include_str!("../../config/parity-requests/products/metafieldsDelete-parity-plan.graphql");
    let read_query =
        include_str!("../../config/parity-requests/products/metafieldsSet-downstream-read.graphql");
    let mut proxy = snapshot_proxy();

    let mutation = proxy.process_request(json_graphql_request(
        mutation_query,
        fixture["mutation"]["variables"].clone(),
    ));
    assert_eq!(mutation.status, 200);
    assert_eq!(
        mutation.body["data"],
        fixture["mutation"]["response"]["data"]
    );

    let downstream = proxy.process_request(json_graphql_request(
        read_query,
        fixture["downstreamReadVariables"].clone(),
    ));
    assert_eq!(downstream.status, 200);
    assert_eq!(downstream.body["data"], fixture["downstreamRead"]["data"]);
    assert_eq!(
        downstream.body["data"]["product"]["primarySpec"],
        Value::Null
    );
}

#[test]
fn product_tags_add_remove_and_multi_resource_reads_match_captured_state() {
    let mut proxy = snapshot_proxy();

    let add = proxy.process_request(json_graphql_request(
        r#"
        mutation TagsAddParityPlan($id: ID!, $tags: [String!]!) {
          tagsAdd(id: $id, tags: $tags) {
            node { id tags }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "id": "gid://shopify/Product/10173064872242",
            "tags": ["existing", "hermes-summer-1777416213315", "hermes-sale-1777416213315"]
        }),
    ));
    assert_eq!(
        add.body["data"]["tagsAdd"],
        json!({
            "node": {
                "id": "gid://shopify/Product/10173064872242",
                "tags": ["existing", "hermes-sale-1777416213315", "hermes-state-1777416213315", "hermes-summer-1777416213315"]
            },
            "userErrors": []
        })
    );

    let add_read = proxy.process_request(json_graphql_request(
        r#"
        query TagsAddDownstreamRead($id: ID!, $query: String!) {
          product(id: $id) { id tags }
          products(first: 10, query: $query) { nodes { id tags } }
          productsCount(query: $query) { count precision }
        }
        "#,
        json!({
            "id": "gid://shopify/Product/10173064872242",
            "query": "tag:hermes-sale-1777416213315"
        }),
    ));
    assert_eq!(
        add_read.body["data"],
        json!({
            "product": {
                "id": "gid://shopify/Product/10173064872242",
                "tags": ["existing", "hermes-sale-1777416213315", "hermes-state-1777416213315", "hermes-summer-1777416213315"]
            },
            "products": { "nodes": [] },
            "productsCount": { "count": 0, "precision": "EXACT" }
        })
    );

    let mut proxy = snapshot_proxy();
    let remove = proxy.process_request(json_graphql_request(
        r#"
        mutation TagsRemoveParityPlan($id: ID!, $tags: [String!]!) {
          tagsRemove(id: $id, tags: $tags) {
            node { id tags }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "id": "gid://shopify/Product/10173064872242",
            "tags": ["hermes-sale-1777416213315", "missing"]
        }),
    ));
    assert_eq!(
        remove.body["data"]["tagsRemove"],
        json!({
            "node": {
                "id": "gid://shopify/Product/10173064872242",
                "tags": ["existing", "hermes-state-1777416213315", "hermes-summer-1777416213315"]
            },
            "userErrors": []
        })
    );

    let remove_read = proxy.process_request(json_graphql_request(
        r#"
        query TagsRemoveDownstreamRead($id: ID!, $remainingQuery: String!, $removedQuery: String!) {
          product(id: $id) { id tags }
          remaining: products(first: 10, query: $remainingQuery) { nodes { id tags } }
          removed: products(first: 10, query: $removedQuery) { nodes { id tags } }
          remainingCount: productsCount(query: $remainingQuery) { count precision }
          removedCount: productsCount(query: $removedQuery) { count precision }
        }
        "#,
        json!({
            "id": "gid://shopify/Product/10173064872242",
            "remainingQuery": "tag:hermes-summer-1777416213315",
            "removedQuery": "tag:hermes-sale-1777416213315"
        }),
    ));
    assert_eq!(
        remove_read.body["data"],
        json!({
            "product": {
                "id": "gid://shopify/Product/10173064872242",
                "tags": ["existing", "hermes-state-1777416213315", "hermes-summer-1777416213315"]
            },
            "remaining": { "nodes": [{ "id": "gid://shopify/Product/10173064872242", "tags": ["existing", "hermes-state-1777416213315", "hermes-summer-1777416213315"] }] },
            "removed": { "nodes": [{ "id": "gid://shopify/Product/10173064872242", "tags": ["existing", "hermes-state-1777416213315", "hermes-summer-1777416213315"] }] },
            "remainingCount": { "count": 1, "precision": "EXACT" },
            "removedCount": { "count": 1, "precision": "EXACT" }
        })
    );

    let mut proxy = snapshot_proxy();
    let multi = proxy.process_request(json_graphql_request(
        r#"
        mutation TagsAddMultiResource($id: ID!, $tags: [String!]!) {
          tagsAdd(id: $id, tags: $tags) {
            node { __typename ... on Product { id title tags } }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "id": "gid://shopify/Product/10178790424882",
            "tags": ["hermes-tags-added-1778091014318"]
        }),
    ));
    assert_eq!(
        multi.body["data"]["tagsAdd"],
        json!({
            "node": {
                "__typename": "Product",
                "id": "gid://shopify/Product/10178790424882",
                "title": "Hermes Tags Product 1778091014318",
                "tags": ["hermes-tags-added-1778091014318", "hermes-tags-base-1778091014318"]
            },
            "userErrors": []
        })
    );

    let multi_read = proxy.process_request(json_graphql_request(
        r#"
        query TagsMultiResourceProductRead($productId: ID!) {
          product(id: $productId) { id title tags }
        }
        "#,
        json!({ "productId": "gid://shopify/Product/10178790424882" }),
    ));
    assert_eq!(
        multi_read.body["data"]["product"],
        json!({
            "id": "gid://shopify/Product/10178790424882",
            "title": "Hermes Tags Product 1778091014318",
            "tags": ["hermes-tags-added-1778091014318", "hermes-tags-base-1778091014318"]
        })
    );
}

#[test]
fn product_tags_add_remove_split_and_match_case_insensitively() {
    fn seeded_proxy() -> DraftProxy {
        snapshot_proxy().with_base_products(vec![ProductRecord {
            id: "gid://shopify/Product/tag-normalization".to_string(),
            created_at: "2024-01-01T00:00:00.000Z".to_string(),
            updated_at: "2024-01-01T00:00:00.000Z".to_string(),
            title: "Tag normalization product".to_string(),
            handle: "tag-normalization-product".to_string(),
            status: "ACTIVE".to_string(),
            description_html: String::new(),
            vendor: String::new(),
            product_type: String::new(),
            tags: vec!["Red".to_string()],
            template_suffix: String::new(),
            seo_title: String::new(),
            seo_description: String::new(),
        }])
    }

    let add_string = seeded_proxy().process_request(json_graphql_request(
        r#"
        mutation ProductTagsAddCommaString($id: ID!, $tags: [String!]!) {
          tagsAdd(id: $id, tags: $tags) {
            node { ... on Product { id tags } }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "id": "gid://shopify/Product/tag-normalization",
            "tags": "blue, green"
        }),
    ));
    assert_eq!(add_string.status, 200);
    assert_eq!(
        add_string.body["data"]["tagsAdd"]["node"]["tags"],
        json!(["blue", "green", "Red"])
    );

    let add_list_element = seeded_proxy().process_request(json_graphql_request(
        r#"
        mutation ProductTagsAddCommaListElement($id: ID!, $tags: [String!]!) {
          tagsAdd(id: $id, tags: $tags) {
            node { ... on Product { id tags } }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "id": "gid://shopify/Product/tag-normalization",
            "tags": ["blue,green"]
        }),
    ));
    assert_eq!(add_list_element.status, 200);
    assert_eq!(
        add_list_element.body["data"]["tagsAdd"]["node"]["tags"],
        json!(["blue", "green", "Red"])
    );

    let add_case_variant = seeded_proxy().process_request(json_graphql_request(
        r#"
        mutation ProductTagsAddCaseVariant($id: ID!, $tags: [String!]!) {
          tagsAdd(id: $id, tags: $tags) {
            node { ... on Product { id tags } }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "id": "gid://shopify/Product/tag-normalization",
            "tags": ["red"]
        }),
    ));
    assert_eq!(add_case_variant.status, 200);
    assert_eq!(
        add_case_variant.body["data"]["tagsAdd"]["node"]["tags"],
        json!(["Red"])
    );

    let add_case_sort = seeded_proxy().process_request(json_graphql_request(
        r#"
        mutation ProductTagsAddCaseSort($id: ID!, $tags: [String!]!) {
          tagsAdd(id: $id, tags: $tags) {
            node { ... on Product { id tags } }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "id": "gid://shopify/Product/tag-normalization",
            "tags": ["b", "A"]
        }),
    ));
    assert_eq!(add_case_sort.status, 200);
    assert_eq!(
        add_case_sort.body["data"]["tagsAdd"]["node"]["tags"],
        json!(["A", "b", "Red"])
    );

    let remove_case_variant = seeded_proxy().process_request(json_graphql_request(
        r#"
        mutation ProductTagsRemoveCaseVariant($id: ID!, $tags: [String!]!) {
          tagsRemove(id: $id, tags: $tags) {
            node { ... on Product { id tags } }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "id": "gid://shopify/Product/tag-normalization",
            "tags": ["red"]
        }),
    ));
    assert_eq!(remove_case_variant.status, 200);
    assert_eq!(
        remove_case_variant.body["data"]["tagsRemove"]["node"]["tags"],
        json!([])
    );

    let remove_string = seeded_proxy().process_request(json_graphql_request(
        r#"
        mutation ProductTagsRemoveString($id: ID!, $tags: [String!]!) {
          tagsRemove(id: $id, tags: $tags) {
            node { ... on Product { id tags } }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "id": "gid://shopify/Product/tag-normalization",
            "tags": "Red"
        }),
    ));
    assert_eq!(remove_string.status, 200);
    assert_eq!(
        remove_string.body["data"]["tagsRemove"]["node"]["tags"],
        json!([])
    );
}

#[test]
fn polymorphic_tags_add_remove_split_and_match_case_insensitively() {
    fn proxy_with_taggable_hydration(id: &'static str) -> (DraftProxy, Arc<Mutex<Vec<String>>>) {
        let upstream_queries = Arc::new(Mutex::new(Vec::new()));
        let captured_queries = Arc::clone(&upstream_queries);
        let proxy = configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport(
            move |request| {
                let body: Value =
                    serde_json::from_str(&request.body).expect("upstream GraphQL body parses");
                let query = body["query"]
                    .as_str()
                    .expect("upstream GraphQL query is a string")
                    .to_string();
                captured_queries.lock().unwrap().push(query.clone());
                let response = if query.contains("OrdersOrderHydrate") {
                    json!({"data": {"order": {"id": id, "__typename": "Order", "name": "#1001", "tags": ["Red"]}}})
                } else if query.contains("CustomerHydrate") {
                    json!({"data": {"customer": {"id": id, "__typename": "Customer", "email": "tags@example.com", "displayName": "Tags Customer", "tags": ["Red"]}}})
                } else if query.contains("TagsArticleHydrate") {
                    json!({"data": {"article": {"id": id, "__typename": "Article", "title": "Tags Article", "tags": ["Red"], "blog": {"id": "gid://shopify/Blog/1"}}}})
                } else if query.contains("OrdersDraftOrderHydrate") {
                    json!({"data": {"draftOrder": {"id": id, "__typename": "DraftOrder", "name": "#D1", "tags": ["Red"]}}})
                } else {
                    json!({"errors": [{"message": format!("unexpected upstream query: {query}")}]})
                };
                shopify_draft_proxy::proxy::Response {
                    status: 200,
                    headers: Default::default(),
                    body: response,
                }
            },
        );
        (proxy, upstream_queries)
    }

    fn assert_tags_mutation(id: &'static str, root: &str, tags: Value, expected: Value) {
        let (mut proxy, upstream_queries) = proxy_with_taggable_hydration(id);
        let response = proxy.process_request(json_graphql_request(
            &format!(
                r#"
                mutation PolymorphicTags($id: ID!, $tags: [String!]!) {{
                  {root}(id: $id, tags: $tags) {{
                    node {{
                      __typename
                      ... on Order {{ id name tags }}
                      ... on Customer {{ id email displayName tags }}
                      ... on Article {{ id title tags }}
                      ... on DraftOrder {{ id name tags }}
                    }}
                    userErrors {{ field message }}
                  }}
                }}
                "#
            ),
            json!({ "id": id, "tags": tags }),
        ));
        assert_eq!(response.status, 200);
        assert_eq!(response.body["data"][root]["node"]["tags"], expected);
        assert_eq!(response.body["data"][root]["userErrors"], json!([]));

        let read_response = proxy.process_request(json_graphql_request(
            r#"
            query PolymorphicTagsRead($id: ID!) {
              order(id: $id) { id name tags }
              customer(id: $id) { id email displayName tags }
              article(id: $id) { id title tags }
              draftOrder(id: $id) { id name tags }
            }
            "#,
            json!({ "id": id }),
        ));
        let read_key = if id.contains("/Order/") {
            "order"
        } else if id.contains("/Customer/") {
            "customer"
        } else if id.contains("/Article/") {
            "article"
        } else {
            "draftOrder"
        };
        assert_eq!(read_response.status, 200);
        assert_eq!(read_response.body["data"][read_key]["tags"], expected);
        assert!(
            upstream_queries
                .lock()
                .unwrap()
                .iter()
                .all(|query| !query.contains("mutation PolymorphicTags")),
            "generic tags mutation must not be sent upstream"
        );
    }

    let resource_ids = [
        "gid://shopify/Order/tag-normalization",
        "gid://shopify/Customer/tag-normalization",
        "gid://shopify/Article/tag-normalization",
        "gid://shopify/DraftOrder/tag-normalization",
    ];
    for id in resource_ids {
        assert_tags_mutation(
            id,
            "tagsAdd",
            json!("blue, green"),
            json!(["blue", "green", "Red"]),
        );
        assert_tags_mutation(
            id,
            "tagsAdd",
            json!(["blue,green"]),
            json!(["blue", "green", "Red"]),
        );
        assert_tags_mutation(id, "tagsAdd", json!(["red"]), json!(["Red"]));
        let remove_case_expected = if id.contains("/Customer/") {
            json!([])
        } else {
            json!(["Red"])
        };
        assert_tags_mutation(id, "tagsRemove", json!(["red"]), remove_case_expected);
        assert_tags_mutation(id, "tagsRemove", json!("Red"), json!([]));
    }
}

#[test]
fn product_change_status_stages_archived_status_and_downstream_read_lag() {
    let mut proxy = snapshot_proxy();

    let changed = proxy.process_request(json_graphql_request(
        r#"
        mutation ProductChangeStatusParityPlan($productId: ID!, $status: ProductStatus!) {
          productChangeStatus(productId: $productId, status: $status) {
            product { id status updatedAt }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "productId": "gid://shopify/Product/10173064872242",
            "status": "ARCHIVED"
        }),
    ));
    assert_eq!(
        changed.body["data"]["productChangeStatus"],
        json!({
            "product": {
                "id": "gid://shopify/Product/10173064872242",
                "status": "ARCHIVED",
                "updatedAt": "2026-04-28T22:43:34Z"
            },
            "userErrors": []
        })
    );

    let null_id = proxy.process_request(json_graphql_request(
        r#"
        mutation ProductChangeStatusNullLiteralConformance {
          productChangeStatus(productId: null, status: ARCHIVED) {
            product { id status updatedAt }
            userErrors { field message }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        null_id.body["errors"][0]["message"],
        json!("Argument 'productId' on Field 'productChangeStatus' has an invalid value (null). Expected type 'ID!'.")
    );
    assert_eq!(
        null_id.body["errors"][0]["path"],
        json!([
            "mutation ProductChangeStatusNullLiteralConformance",
            "productChangeStatus",
            "productId"
        ])
    );
    assert_eq!(
        null_id.body["errors"][0]["extensions"],
        json!({"code": "argumentLiteralsIncompatible", "typeName": "Field", "argumentName": "productId"})
    );

    let downstream = proxy.process_request(json_graphql_request(
        r#"
        query ProductChangeStatusDownstreamRead($id: ID!, $query: String!) {
          product(id: $id) { id status updatedAt }
          products(first: 10, query: $query) { nodes { id status } }
          productsCount(query: $query) { count precision }
        }
        "#,
        json!({
            "id": "gid://shopify/Product/10173064872242",
            "query": "status:archived tag:hermes-state-1777416213315"
        }),
    ));
    assert_eq!(
        downstream.body["data"],
        json!({
            "product": {
                "id": "gid://shopify/Product/10173064872242",
                "status": "ARCHIVED",
                "updatedAt": "2026-04-28T22:43:34Z"
            },
            "products": { "nodes": [] },
            "productsCount": { "count": 0, "precision": "EXACT" }
        })
    );
}

#[test]
fn product_variant_compatibility_mutations_replay_captured_bulk_shapes() {
    let mut proxy = snapshot_proxy();

    let create = proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/products/productVariantCreate-parity-plan.graphql"
        ),
        json!({
            "input": {
                "productId": "gid://shopify/Product/9259552407785",
                "title": "Blue",
                "sku": "HERMES-BULK-810153-BLUE",
                "barcode": "2222222222222",
                "price": "26.00",
                "inventoryQuantity": 0,
                "selectedOptions": [{ "name": "Color", "value": "Blue" }],
                "inventoryItem": { "tracked": true, "requiresShipping": false }
            }
        }),
    ));
    assert_eq!(create.status, 200);
    assert_eq!(
        create.body["data"]["productVariantCreate"]["product"],
        json!({
            "id": "gid://shopify/Product/9259552407785",
            "totalInventory": 0,
            "tracksInventory": true
        })
    );
    assert_eq!(
        create.body["data"]["productVariantCreate"]["productVariant"]["sku"],
        json!("HERMES-BULK-810153-BLUE")
    );
    assert_eq!(
        create.body["data"]["productVariantCreate"]["productVariant"]["inventoryItem"],
        json!({
            "id": "gid://shopify/InventoryItem/53053417259241",
            "tracked": true,
            "requiresShipping": false
        })
    );

    let create_read = proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/products/productVariantCreate-downstream-read.graphql"
        ),
        json!({ "id": "gid://shopify/Product/9259552407785" }),
    ));
    assert_eq!(
        create_read.body["data"]["product"],
        json!({
            "id": "gid://shopify/Product/9259552407785",
            "totalInventory": 0,
            "tracksInventory": true
        })
    );

    let update = proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/products/productVariantUpdate-parity-plan.graphql"
        ),
        json!({
            "input": {
                "id": "gid://shopify/ProductVariant/50905436913897",
                "title": "Red",
                "sku": "HERMES-BULK-810153-RED",
                "barcode": "1111111111111",
                "price": "24.00",
                "compareAtPrice": "30.00",
                "taxable": true,
                "inventoryPolicy": "DENY",
                "inventoryQuantity": 0,
                "selectedOptions": [{ "name": "Color", "value": "Red" }],
                "inventoryItem": { "tracked": true, "requiresShipping": true }
            }
        }),
    ));
    assert_eq!(update.status, 200);
    assert_eq!(
        update.body["data"]["productVariantUpdate"]["productVariant"],
        json!({
            "id": "gid://shopify/ProductVariant/50905436913897",
            "title": "Red",
            "sku": "HERMES-BULK-810153-RED",
            "barcode": "1111111111111",
            "price": "24.00",
            "compareAtPrice": "30.00",
            "taxable": true,
            "inventoryPolicy": "DENY",
            "inventoryQuantity": 0,
            "selectedOptions": [{ "name": "Color", "value": "Red" }],
            "inventoryItem": {
                "id": "gid://shopify/InventoryItem/53053417160937",
                "tracked": true,
                "requiresShipping": true
            }
        })
    );

    let update_read = proxy.process_request(json_graphql_request(
        include_str!("../../config/parity-requests/products/productVariantUpdate-downstream-read.graphql"),
        json!({ "id": "gid://shopify/Product/9259552407785", "query": "sku:HERMES-BULK-810153-RED" }),
    ));
    assert_eq!(
        update_read.body["data"]["product"]["variants"]["nodes"][0]["id"],
        json!("gid://shopify/ProductVariant/50905436913897")
    );
    assert_eq!(update_read.body["data"]["products"], json!({ "nodes": [] }));
    assert_eq!(
        update_read.body["data"]["skuCount"],
        json!({ "count": 0, "precision": "EXACT" })
    );

    let delete = proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/products/productVariantDelete-parity-plan.graphql"
        ),
        json!({ "id": "gid://shopify/ProductVariant/50905436913897" }),
    ));
    assert_eq!(delete.status, 200);
    assert_eq!(
        delete.body["data"]["productVariantDelete"],
        json!({
            "deletedProductVariantId": "gid://shopify/ProductVariant/50905436913897",
            "userErrors": []
        })
    );

    let delete_read = proxy.process_request(json_graphql_request(
        include_str!("../../config/parity-requests/products/productVariantsBulkDelete-downstream-read.graphql"),
        json!({ "id": "gid://shopify/Product/9259552407785", "query": "sku:HERMES-BULK-810153-RED" }),
    ));
    assert_eq!(
        delete_read.body["data"]["product"]["variants"]["nodes"],
        json!([{
            "id": "gid://shopify/ProductVariant/50905437012201",
            "title": "Blue",
            "sku": "HERMES-BULK-810153-BLUE",
            "barcode": "2222222222222",
            "price": "26.00",
            "compareAtPrice": "30.00",
            "taxable": true,
            "inventoryPolicy": "DENY",
            "inventoryQuantity": 0,
            "selectedOptions": [{ "name": "Color", "value": "Blue" }],
            "inventoryItem": {
                "id": "gid://shopify/InventoryItem/53053417259241",
                "tracked": true,
                "requiresShipping": false
            }
        }])
    );
}

#[test]
fn product_fixture_backed_update_and_delete_mutations_return_captured_shapes() {
    let mut proxy = snapshot_proxy();

    let update = proxy.process_request(json_graphql_request(
        include_str!("../../config/parity-requests/products/productUpdate-parity-plan.graphql"),
        json!({
            "product": {
                "id": "gid://shopify/Product/9257218801897",
                "title": "Hermes Product Conformance 1776550632328 Updated",
                "vendor": "HERMES-LABS",
                "productType": "TEST-GOODS",
                "tags": ["1776550632328-updated", "conformance", "product-mutation"],
                "descriptionHtml": "<p>Updated Hermes product mutation conformance 1776550632328</p>",
                "templateSuffix": "product-mutation-updated",
                "seo": {
                    "title": "Hermes Product 1776550632328 Updated",
                    "description": "Updated Hermes product mutation conformance 1776550632328"
                }
            }
        }),
    ));
    assert_eq!(update.status, 200);
    let update_fixture: Value = serde_json::from_str(include_str!(
        "../../fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/product-update-parity.json"
    ))
    .unwrap();
    assert_eq!(
        update.body["data"]["productUpdate"]["product"],
        update_fixture["mutation"]["response"]["data"]["productUpdate"]["product"]
    );
    assert_eq!(
        update.body["data"]["productUpdate"]["userErrors"],
        json!([])
    );

    let delete = proxy.process_request(json_graphql_request(
        include_str!("../../config/parity-requests/products/productDelete-parity-plan.graphql"),
        json!({ "input": { "id": "gid://shopify/Product/9257218801897" } }),
    ));
    assert_eq!(delete.status, 200);
    assert_eq!(
        delete.body["data"]["productDelete"],
        json!({
            "deletedProductId": "gid://shopify/Product/9257218801897",
            "userErrors": []
        })
    );
}

#[test]
fn product_update_fixture_backed_validation_branches_preserve_captured_shapes() {
    let mut proxy = snapshot_proxy();

    let blank = proxy.process_request(json_graphql_request(
        include_str!("../../config/parity-requests/products/productUpdate-parity-plan.graphql"),
        json!({
            "product": {
                "id": "gid://shopify/Product/9257218801897",
                "title": ""
            }
        }),
    ));
    assert_eq!(blank.status, 200);
    let blank_fixture: Value = serde_json::from_str(include_str!(
        "../../fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/product-update-blank-title-parity.json"
    ))
    .unwrap();
    assert_eq!(
        blank.body["data"],
        blank_fixture["mutation"]["response"]["data"]
    );

    let too_long = proxy.process_request(json_graphql_request(
        include_str!("../../config/parity-requests/products/productUpdate-too-long-handle-parity.graphql"),
        json!({
            "product": {
                "id": "gid://shopify/Product/10170567196978",
                "handle": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
            }
        }),
    ));
    assert_eq!(too_long.status, 200);
    assert_eq!(
        too_long.body["data"]["productUpdate"],
        json!({
            "product": {
                "id": "gid://shopify/Product/10170567196978",
                "title": "HAR-22 update seed 1777153541365",
                "handle": "har-22-update-seed-1777153541365"
            },
            "userErrors": [{
                "field": ["handle"],
                "message": "Handle is too long (maximum is 255 characters)"
            }]
        })
    );
}

#[test]
fn product_update_tag_normalization_limits_match_shopify_shapes() {
    let mut proxy = snapshot_proxy();
    let create = proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/products/productCreate-tag-normalization-setup.graphql"
        ),
        json!({
            "product": {
                "title": "HAR tag normalization seed",
                "tags": ["base"]
            }
        }),
    ));
    assert_eq!(create.status, 200);
    let product_id = create.body["data"]["productCreate"]["product"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let mixed = proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/products/productUpdate-tag-normalization.graphql"
        ),
        json!({
            "product": {
                "id": product_id,
                "tags": [" Red ", "red", "RED", " big   sale "]
            }
        }),
    ));
    assert_eq!(mixed.status, 200);
    assert_eq!(
        mixed.body["data"]["productUpdate"]["product"]["tags"],
        json!(["big   sale", "Red"])
    );
    assert_eq!(mixed.body["data"]["productUpdate"]["userErrors"], json!([]));

    let too_many_tags: Vec<String> = (0..251).map(|index| format!("tag-{index:03}")).collect();
    let too_many = proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/products/productUpdate-tag-normalization.graphql"
        ),
        json!({
            "product": {
                "id": product_id,
                "tags": too_many_tags
            }
        }),
    ));
    assert_eq!(too_many.status, 200);
    assert!(too_many.body.get("data").is_none());
    assert_eq!(
        too_many.body["errors"][0]["message"],
        json!("The input array size of 251 is greater than the maximum allowed of 250.")
    );
    assert_eq!(
        too_many.body["errors"][0]["path"],
        json!(["productUpdate", "product", "tags"])
    );
    assert_eq!(
        too_many.body["errors"][0]["extensions"],
        json!({ "code": "MAX_INPUT_SIZE_EXCEEDED" })
    );

    let too_long = proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/products/productUpdate-tag-normalization.graphql"
        ),
        json!({
            "product": {
                "id": product_id,
                "tags": ["x".repeat(256)]
            }
        }),
    ));
    assert_eq!(too_long.status, 200);
    assert_eq!(
        too_long.body["data"]["productUpdate"]["product"]["tags"],
        json!(["big   sale", "Red"])
    );
    assert_eq!(
        too_long.body["data"]["productUpdate"]["userErrors"],
        json!([{ "field": ["tags"], "message": "Product tags is invalid" }])
    );
}

#[test]
fn product_delete_required_id_graphql_errors_match_shopify_shapes() {
    let mut proxy = snapshot_proxy();

    let inline_missing = proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/products/productDelete-inline-missing-id-parity.graphql"
        ),
        json!({}),
    ));
    assert_eq!(inline_missing.status, 200);
    assert!(inline_missing.body.get("data").is_none());
    assert_eq!(
        inline_missing.body["errors"][0],
        json!({
            "message": "Argument 'id' on InputObject 'ProductDeleteInput' is required. Expected type ID!",
            "locations": [{ "line": 3, "column": 26 }],
            "path": ["mutation", "productDelete", "input", "id"],
            "extensions": {
                "code": "missingRequiredInputObjectAttribute",
                "argumentName": "id",
                "argumentType": "ID!",
                "inputObjectType": "ProductDeleteInput"
            }
        })
    );

    let inline_null = proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/products/productDelete-inline-null-id-parity.graphql"
        ),
        json!({}),
    ));
    assert_eq!(inline_null.status, 200);
    assert!(inline_null.body.get("data").is_none());
    assert_eq!(
        inline_null.body["errors"][0],
        json!({
            "message": "Argument 'id' on InputObject 'ProductDeleteInput' has an invalid value (null). Expected type 'ID!'.",
            "locations": [{ "line": 3, "column": 26 }],
            "path": ["mutation", "productDelete", "input", "id"],
            "extensions": {
                "code": "argumentLiteralsIncompatible",
                "typeName": "InputObject",
                "argumentName": "id"
            }
        })
    );

    let variable_missing = proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/products/productDelete-variable-missing-id-parity.graphql"
        ),
        json!({ "input": {} }),
    ));
    assert_eq!(variable_missing.status, 200);
    assert!(variable_missing.body.get("data").is_none());
    assert_eq!(
        variable_missing.body["errors"][0],
        json!({
            "message": "Variable $input of type ProductDeleteInput! was provided invalid value for id (Expected value to not be null)",
            "locations": [{ "line": 2, "column": 37 }],
            "extensions": {
                "code": "INVALID_VARIABLE",
                "value": {},
                "problems": [{
                    "path": ["id"],
                    "explanation": "Expected value to not be null"
                }]
            }
        })
    );
}

#[test]
fn product_delete_validation_distinguishes_inline_missing_null_and_unbound_variables_by_ast() {
    let mut proxy = snapshot_proxy();

    let missing_inline = proxy.process_request(graphql_request(
        "POST",
        &json!({
            "query": r#"
                mutation AnyDeleteName {
                  deletionAlias: productDelete(input: {
                  }) {
                    deletedProductId
                    userErrors { field message code }
                  }
                }
            "#
        })
        .to_string(),
    ));
    assert_eq!(missing_inline.status, 200);
    assert_eq!(
        missing_inline.body["errors"][0]["extensions"]["code"],
        json!("missingRequiredInputObjectAttribute")
    );

    let null_inline = proxy.process_request(graphql_request(
        "POST",
        &json!({
            "query": r#"
                mutation AnyDeleteName {
                  deletionAlias: productDelete(input: {
                    id: null
                  }) {
                    deletedProductId
                    userErrors { field message code }
                  }
                }
            "#
        })
        .to_string(),
    ));
    assert_eq!(null_inline.status, 200);
    assert_eq!(
        null_inline.body["errors"][0]["extensions"]["code"],
        json!("argumentLiteralsIncompatible")
    );

    let unbound_variable = proxy.process_request(json_graphql_request(
        r#"
            mutation AnyDeleteName($input: ProductDeleteInput!) {
              deletionAlias: productDelete(input: $input) {
                deletedProductId
                userErrors { field message code }
              }
            }
        "#,
        json!({}),
    ));
    assert_eq!(unbound_variable.status, 200);
    assert_eq!(
        unbound_variable.body["errors"][0]["extensions"]["code"],
        json!("INVALID_VARIABLE")
    );
    assert_eq!(
        unbound_variable.body["errors"][0]["extensions"]["value"],
        Value::Null
    );
}

#[test]
fn product_create_length_validation_errors_match_shopify_shapes() {
    let mut proxy = snapshot_proxy();
    let too_long = "a".repeat(260);

    let handle = proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/products/productCreate-too-long-handle-parity.graphql"
        ),
        json!({
            "product": {
                "title": "HAR too-long handle",
                "handle": too_long
            }
        }),
    ));
    assert_eq!(handle.status, 200);
    assert_eq!(handle.body["data"]["productCreate"]["product"], json!(null));
    assert_eq!(
        handle.body["data"]["productCreate"]["userErrors"],
        json!([{ "field": ["handle"], "message": "Handle is too long (maximum is 255 characters)" }])
    );

    let vendor = proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/products/productCreate-input-validation.graphql"
        ),
        json!({
            "product": {
                "title": "HAR too-long vendor",
                "vendor": "v".repeat(256)
            }
        }),
    ));
    assert_eq!(vendor.status, 200);
    assert_eq!(vendor.body["data"]["productCreate"]["product"], json!(null));
    assert_eq!(
        vendor.body["data"]["productCreate"]["userErrors"],
        json!([{ "field": ["vendor"], "message": "Vendor is too long (maximum is 255 characters)" }])
    );

    let product_type = proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/products/productCreate-input-validation.graphql"
        ),
        json!({
            "product": {
                "title": "HAR too-long product type",
                "productType": "t".repeat(256)
            }
        }),
    ));
    assert_eq!(product_type.status, 200);
    assert_eq!(
        product_type.body["data"]["productCreate"]["product"],
        json!(null)
    );
    assert_eq!(
        product_type.body["data"]["productCreate"]["userErrors"],
        json!([
            { "field": ["productType"], "message": "Product type is too long (maximum is 255 characters)" },
            { "field": ["customProductType"], "message": "Custom product type is too long (maximum is 255 characters)" }
        ])
    );
}

#[test]
fn product_option_lifecycle_replays_captured_mutations_and_downstream_reads() {
    let mut proxy = snapshot_proxy();
    let downstream_query = include_str!(
        "../../config/parity-requests/products/product-option-lifecycle-downstream-read.graphql"
    );

    let create_fixture = product_fixture(include_str!(
        "../../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/product-options-create-parity.json"
    ));
    let create = proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/products/productOptionsCreate-parity-plan.graphql"
        ),
        create_fixture["mutation"]["variables"].clone(),
    ));
    assert_eq!(create.status, 200);
    assert_eq!(
        create.body["data"],
        create_fixture["mutation"]["response"]["data"]
    );
    let create_read = proxy.process_request(json_graphql_request(
        downstream_query,
        json!({ "id": create.body["data"]["productOptionsCreate"]["product"]["id"].clone() }),
    ));
    assert_eq!(
        create_read.body["data"],
        create_fixture["downstreamRead"]["data"]
    );

    let update_fixture = product_fixture(include_str!(
        "../../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/product-option-update-parity.json"
    ));
    let update = proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/products/productOptionUpdate-parity-plan.graphql"
        ),
        update_fixture["mutation"]["variables"].clone(),
    ));
    assert_eq!(update.status, 200);
    assert_eq!(
        update.body["data"],
        update_fixture["mutation"]["response"]["data"]
    );
    let update_read = proxy.process_request(json_graphql_request(
        downstream_query,
        json!({ "id": update.body["data"]["productOptionUpdate"]["product"]["id"].clone() }),
    ));
    assert_eq!(
        update_read.body["data"],
        update_fixture["downstreamRead"]["data"]
    );

    let delete_fixture = product_fixture(include_str!(
        "../../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/product-options-delete-parity.json"
    ));
    let delete = proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/products/productOptionsDelete-parity-plan.graphql"
        ),
        delete_fixture["mutation"]["variables"].clone(),
    ));
    assert_eq!(delete.status, 200);
    assert_eq!(
        delete.body["data"],
        delete_fixture["mutation"]["response"]["data"]
    );
    let delete_read = proxy.process_request(json_graphql_request(
        downstream_query,
        json!({ "id": delete.body["data"]["productOptionsDelete"]["product"]["id"].clone() }),
    ));
    assert_eq!(
        delete_read.body["data"],
        delete_fixture["downstreamRead"]["data"]
    );
}

#[test]
fn product_options_create_variant_strategy_edges_replay_captured_shapes() {
    let cases = [
        (
            include_str!("../../config/parity-requests/products/productOptionsCreate-variant-strategy-create.graphql"),
            include_str!("../../config/parity-requests/products/product-option-lifecycle-downstream-read.graphql"),
            include_str!("../../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/product-options-create-variant-strategy-create-parity.json"),
        ),
        (
            include_str!("../../config/parity-requests/products/productOptionsCreate-variant-strategy-edge.graphql"),
            include_str!("../../config/parity-requests/products/product-option-variant-strategy-edge-downstream-read.graphql"),
            include_str!("../../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/product-options-create-variant-strategy-leave-as-is-parity.json"),
        ),
        (
            include_str!("../../config/parity-requests/products/productOptionsCreate-variant-strategy-edge.graphql"),
            include_str!("../../config/parity-requests/products/product-option-variant-strategy-edge-downstream-read.graphql"),
            include_str!("../../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/product-options-create-variant-strategy-null-parity.json"),
        ),
        (
            include_str!("../../config/parity-requests/products/productOptionsCreate-variant-strategy-edge.graphql"),
            include_str!("../../config/parity-requests/products/product-option-variant-strategy-edge-downstream-read.graphql"),
            include_str!("../../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/product-options-create-variant-strategy-create-over-default-limit.json"),
        ),
    ];

    for (mutation_query, downstream_query, fixture_source) in cases {
        let mut proxy = snapshot_proxy();
        let fixture = product_fixture(fixture_source);
        let mutation = proxy.process_request(json_graphql_request(
            mutation_query,
            fixture["mutation"]["variables"].clone(),
        ));
        assert_eq!(mutation.status, 200);
        assert_eq!(
            mutation.body["data"],
            fixture["mutation"]["response"]["data"]
        );

        let product_id = mutation.body["data"]["productOptionsCreate"]["product"]["id"].clone();
        let downstream = proxy.process_request(json_graphql_request(
            downstream_query,
            json!({ "id": product_id }),
        ));
        assert_eq!(downstream.status, 200);
        assert_eq!(downstream.body["data"], fixture["downstreamRead"]["data"]);
    }
}

#[test]
fn product_duplicate_replays_captured_sync_and_async_readbacks() {
    let mut proxy = snapshot_proxy();

    let sync_fixture = product_fixture(include_str!(
        "../../fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/product-duplicate-parity.json"
    ));
    let duplicate = proxy.process_request(json_graphql_request(
        include_str!("../../config/parity-requests/products/productDuplicate-parity-plan.graphql"),
        sync_fixture["mutation"]["variables"].clone(),
    ));
    assert_eq!(duplicate.status, 200);
    assert_eq!(
        duplicate.body["data"],
        sync_fixture["mutation"]["response"]["data"]
    );
    let duplicate_read = proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/products/productDuplicate-downstream-read.graphql"
        ),
        json!({ "id": duplicate.body["data"]["productDuplicate"]["newProduct"]["id"].clone() }),
    ));
    assert_eq!(duplicate_read.status, 200);
    assert_eq!(
        duplicate_read.body["data"],
        sync_fixture["downstreamRead"]["data"]
    );

    let mut async_proxy = snapshot_proxy();
    let async_fixture = product_fixture(include_str!(
        "../../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/product-duplicate-async-success.json"
    ));
    let async_duplicate = async_proxy.process_request(json_graphql_request(
        include_str!("../../config/parity-requests/products/productDuplicate-async.graphql"),
        async_fixture["mutation"]["variables"].clone(),
    ));
    assert_eq!(async_duplicate.status, 200);
    assert_eq!(
        async_duplicate.body["data"],
        async_fixture["mutation"]["response"]["data"]
    );
    let operation = async_proxy.process_request(json_graphql_request(
        include_str!("../../config/parity-requests/products/productDuplicate-operation-read.graphql"),
        json!({
            "id": async_duplicate.body["data"]["productDuplicate"]["productDuplicateOperation"]["id"].clone()
        }),
    ));
    assert_eq!(operation.status, 200);
    assert_eq!(
        operation.body["data"],
        async_fixture["operationRead"]["response"]["data"]
    );
    let async_read = async_proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/products/productDuplicate-async-product-read.graphql"
        ),
        json!({ "id": operation.body["data"]["productOperation"]["newProduct"]["id"].clone() }),
    ));
    assert_eq!(async_read.status, 200);
    assert_eq!(
        async_read.body["data"],
        async_fixture["downstreamRead"]["response"]["data"]
    );

    let mut missing_proxy = snapshot_proxy();
    let missing_fixture = product_fixture(include_str!(
        "../../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/product-duplicate-async-missing.json"
    ));
    let missing_duplicate = missing_proxy.process_request(json_graphql_request(
        include_str!("../../config/parity-requests/products/productDuplicate-async.graphql"),
        missing_fixture["mutation"]["variables"].clone(),
    ));
    assert_eq!(missing_duplicate.status, 200);
    assert_eq!(
        missing_duplicate.body["data"],
        missing_fixture["mutation"]["response"]["data"]
    );
    let missing_operation = missing_proxy.process_request(json_graphql_request(
        include_str!("../../config/parity-requests/products/productDuplicate-operation-read.graphql"),
        json!({
            "id": missing_duplicate.body["data"]["productDuplicate"]["productDuplicateOperation"]["id"].clone()
        }),
    ));
    assert_eq!(missing_operation.status, 200);
    assert_eq!(
        missing_operation.body["data"],
        missing_fixture["operationRead"]["response"]["data"]
    );
}

#[test]
fn product_delete_async_operation_preserves_pending_delete_readbacks() {
    let mut proxy = snapshot_proxy();
    let fixture = product_fixture(include_str!(
        "../../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/product-delete-async-operation.json"
    ));

    let source_create = proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/products/productDelete-async-source-create.graphql"
        ),
        fixture["setup"]["sourceCreate"]["variables"].clone(),
    ));
    assert_eq!(source_create.status, 200);
    let product_id = source_create.body["data"]["productSet"]["product"]["id"].clone();
    assert!(product_id.as_str().unwrap().contains("/Product/"));
    assert_eq!(
        source_create.body["data"]["productSet"]["product"]["title"],
        fixture["setup"]["sourceCreate"]["response"]["data"]["productSet"]["product"]["title"]
    );
    assert_eq!(
        source_create.body["data"]["productSet"]["product"]["status"],
        fixture["setup"]["sourceCreate"]["response"]["data"]["productSet"]["product"]["status"]
    );

    let delete = proxy.process_request(json_graphql_request(
        include_str!("../../config/parity-requests/products/productDelete-async-operation.graphql"),
        json!({ "input": { "id": product_id.clone() }, "synchronous": false }),
    ));
    assert_eq!(delete.status, 200);
    assert_eq!(
        delete.body["data"]["productDelete"]["deletedProductId"],
        Value::Null
    );
    assert_eq!(
        delete.body["data"]["productDelete"]["userErrors"],
        json!([])
    );
    assert_eq!(
        delete.body["data"]["productDelete"]["productDeleteOperation"]["status"],
        "CREATED"
    );
    assert_eq!(
        delete.body["data"]["productDelete"]["productDeleteOperation"]["deletedProductId"],
        Value::Null
    );
    assert_eq!(
        delete.body["data"]["productDelete"]["productDeleteOperation"]["userErrors"],
        json!([])
    );
    let operation_id = delete.body["data"]["productDelete"]["productDeleteOperation"]["id"].clone();
    assert!(operation_id
        .as_str()
        .unwrap()
        .contains("/ProductDeleteOperation/"));

    let duplicate = proxy.process_request(json_graphql_request(
        include_str!("../../config/parity-requests/products/productDelete-async-operation.graphql"),
        json!({ "input": { "id": product_id.clone() }, "synchronous": false }),
    ));
    assert_eq!(duplicate.status, 200);
    assert_eq!(
        duplicate.body["data"],
        fixture["duplicateMutation"]["response"]["data"]
    );

    let immediate_read = proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/products/productDelete-async-product-read.graphql"
        ),
        json!({ "id": product_id.clone() }),
    ));
    assert_eq!(immediate_read.status, 200);
    assert_eq!(immediate_read.body["data"]["product"]["id"], product_id);
    assert_eq!(
        immediate_read.body["data"]["product"]["title"],
        fixture["downstreamRead"]["response"]["data"]["product"]["title"]
    );
    assert_eq!(
        immediate_read.body["data"]["product"]["status"],
        fixture["downstreamRead"]["response"]["data"]["product"]["status"]
    );

    let operation_read = proxy.process_request(json_graphql_request(
        include_str!("../../config/parity-requests/products/productDelete-operation-read.graphql"),
        json!({ "id": operation_id.clone() }),
    ));
    assert_eq!(operation_read.status, 200);
    assert_eq!(
        operation_read.body["data"]["productOperation"]["__typename"],
        "ProductDeleteOperation"
    );
    assert_eq!(
        operation_read.body["data"]["productOperation"]["id"],
        operation_id
    );
    assert_eq!(
        operation_read.body["data"]["productOperation"]["deletedProductId"],
        product_id
    );
    assert_eq!(
        operation_read.body["data"]["productOperation"]["userErrors"],
        json!([])
    );

    let node_read = proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/products/productDelete-operation-node-read.graphql"
        ),
        json!({ "id": operation_id.clone() }),
    ));
    assert_eq!(node_read.status, 200);
    assert_eq!(node_read.body["data"]["node"]["id"], operation_id);
    assert_eq!(
        node_read.body["data"]["node"]["deletedProductId"],
        product_id
    );
    assert_eq!(node_read.body["data"]["node"]["status"], "COMPLETE");
}

#[test]
fn product_relationship_options_reads_replay_captured_reorder_downstreams() {
    let validation_fixture = product_fixture(include_str!(
        "../../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/product-options-reorder-validation.json"
    ));
    let relationship_fixture = product_fixture(include_str!(
        "../../fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/products/product-relationship-roots.json"
    ));

    let mut validation_proxy = snapshot_proxy();
    let created = validation_proxy.process_request(json_graphql_request(
        include_str!("../../config/parity-requests/products/productCreate-parity-plan.graphql"),
        validation_fixture["captures"]["productCreate"]["variables"].clone(),
    ));
    assert_eq!(created.status, 200);
    let staged_product_id = created.body["data"]["productCreate"]["product"]["id"].clone();
    let validation_read = validation_proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/products/product-relationship-product-options-read.graphql"
        ),
        json!({ "productId": staged_product_id }),
    ));
    assert_eq!(validation_read.status, 200);
    assert_eq!(
        validation_read.body["data"]["product"]["options"],
        validation_fixture["captures"]["downstreamRead"]["result"]["data"]["product"]["options"]
    );
    assert_eq!(
        validation_read.body["data"]["product"]["variants"],
        validation_fixture["captures"]["downstreamRead"]["result"]["data"]["product"]["variants"]
    );

    let mut relationship_proxy = snapshot_proxy();
    let relationship_read = relationship_proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/products/product-relationship-product-options-read.graphql"
        ),
        relationship_fixture["optionDownstreamRead"]["variables"].clone(),
    ));
    assert_eq!(relationship_read.status, 200);
    assert_eq!(
        relationship_read.body["data"],
        relationship_fixture["optionDownstreamRead"]["response"]["data"]
    );
}

#[test]
fn collection_membership_downstream_reads_replay_captured_shapes() {
    let mut proxy = snapshot_proxy();

    let add_fixture: Value = serde_json::from_str(include_str!(
        "../../fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/collection-add-products-parity.json"
    ))
    .unwrap();
    let add_response = proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/products/collectionAddProducts-downstream-read.graphql"
        ),
        add_fixture["downstreamReadVariables"].clone(),
    ));
    assert_eq!(
        add_response.body,
        json!({ "data": add_fixture["downstreamRead"]["data"] })
    );

    let create_fixture: Value = serde_json::from_str(include_str!(
        "../../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/collection-create-initial-products-parity.json"
    ))
    .unwrap();
    let create_response = proxy.process_request(json_graphql_request(
        include_str!("../../config/parity-requests/products/collectionCreate-initial-products-downstream-read.graphql"),
        create_fixture["downstreamReadVariables"].clone(),
    ));
    assert_eq!(
        create_response.body,
        json!({ "data": create_fixture["downstreamRead"]["data"] })
    );

    let reorder_fixture: Value = serde_json::from_str(include_str!(
        "../../fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/collection-reorder-products-parity.json"
    ))
    .unwrap();
    let reorder_response = proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/products/collectionReorderProducts-downstream-read.graphql"
        ),
        reorder_fixture["downstreamReadVariables"].clone(),
    ));
    assert_eq!(
        reorder_response.body,
        json!({ "data": reorder_fixture["downstreamRead"]["data"] })
    );
}

#[test]
fn product_contextual_pricing_price_list_read_replays_captured_shape() {
    let mut proxy = snapshot_proxy();
    let fixture: Value = serde_json::from_str(include_str!(
        "../../fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/products/product-contextual-pricing-price-list-parity.json"
    ))
    .unwrap();
    let variables: Value = serde_json::from_str(include_str!(
        "../../config/parity-requests/products/product-contextual-pricing-price-list-read.variables.json"
    ))
    .unwrap();

    let response = proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/products/product-contextual-pricing-price-list-read.graphql"
        ),
        variables,
    ));

    assert_eq!(response.body, json!({ "data": fixture["data"] }));
}

#[test]
fn product_create_then_bulk_create_downstream_includes_total_inventory_zero() {
    let mut proxy = snapshot_proxy();
    let fixture: Value = serde_json::from_str(include_str!(
        "../../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/product-create-then-bulk-create-price-range-parity.json"
    ))
    .unwrap();

    let create = proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/products/productCreate-then-bulkCreate-derived-create.graphql"
        ),
        fixture["create"]["variables"].clone(),
    ));
    let product_id = create.body["data"]["productCreate"]["product"]["id"].clone();
    assert!(product_id.is_string());

    let downstream = proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/products/productCreate-then-bulkCreate-derived-downstream.graphql"
        ),
        json!({ "id": product_id }),
    ));

    assert_eq!(
        downstream.body["data"]["product"]["totalInventory"],
        json!(0)
    );
}

#[test]
fn product_invalid_search_query_syntax_replays_staged_search_semantics() {
    let mut proxy = snapshot_proxy();
    let fixture: Value = serde_json::from_str(include_str!(
        "../../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/product-invalid-search-query-syntax.json"
    ))
    .unwrap();

    let create = proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/products/product-invalid-search-query-create.graphql"
        ),
        fixture["captures"]["productCreate"]["variables"].clone(),
    ));
    assert_eq!(
        create.body["data"]["productCreate"]["product"]["id"],
        fixture["captures"]["productCreate"]["result"]["payload"]["data"]["productCreate"]
            ["product"]["id"]
    );

    for capture in [
        "validTagSearchAfterCreate",
        "fieldOpenParenSearchAfterCreate",
        "fieldQuotedOpenParenSearchAfterCreate",
        "bareLeadingParenSearchAfterCreate",
        "danglingOrSearchAfterCreate",
    ] {
        let response = proxy.process_request(json_graphql_request(
            include_str!(
                "../../config/parity-requests/products/product-invalid-search-query-search.graphql"
            ),
            fixture["captures"][capture]["variables"].clone(),
        ));
        assert_eq!(
            response.body["data"], fixture["captures"][capture]["result"]["payload"]["data"],
            "{capture}"
        );
    }
}

#[test]
fn product_media_validation_downstream_read_preserves_seed_and_mixed_create_media() {
    let mut proxy = snapshot_proxy();
    let fixture: Value = serde_json::from_str(include_str!(
        "../../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/product-media-validation-branches.json"
    ))
    .unwrap();

    let response = proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/products/product-media-validation-downstream-read.graphql"
        ),
        json!({ "productId": "gid://shopify/Product/10170577518898" }),
    ));

    assert_eq!(
        response.body["data"],
        fixture["scenarios"][9]["downstreamReadAfterScenario"]["data"]
    );
}

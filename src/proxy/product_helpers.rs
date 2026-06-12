use super::*;
use crate::graphql::RawArgumentValue;

const PRODUCT_STATUS_BASE_VALUES: &[&str] = &["ACTIVE", "ARCHIVED", "DRAFT"];

struct ProductStatusInputContext<'a> {
    argument_name: &'a str,
    input_object_type: &'a str,
    field_name: &'a str,
    expected_type: &'a str,
}

struct ProductStatusLiteralError<'a> {
    value: &'a str,
    argument_name: &'a str,
    type_name: &'a str,
    container_name: &'a str,
    expected_type: &'a str,
    location: Option<SourceLocation>,
}

pub(in crate::proxy) fn product_variant_compat_mutation_data(
    root_field: &str,
    variables: &BTreeMap<String, ResolvedValue>,
) -> Value {
    match root_field {
        "productVariantCreate" => {
            let fixture = product_variant_fixture("create");
            let bulk = &fixture["mutation"]["response"]["data"]["productVariantsBulkCreate"];
            let product = &bulk["product"];
            json!({
                "productVariantCreate": {
                    "product": {
                        "id": product["id"].clone(),
                        "totalInventory": product["totalInventory"].clone(),
                        "tracksInventory": product["tracksInventory"].clone()
                    },
                    "productVariant": bulk["productVariants"][0].clone(),
                    "userErrors": bulk["userErrors"].clone()
                }
            })
        }
        "productVariantUpdate" => {
            let fixture = product_variant_fixture("update");
            let bulk = &fixture["mutation"]["response"]["data"]["productVariantsBulkUpdate"];
            let mut variant = bulk["productVariants"][0].clone();
            if let Some(map) = variant.as_object_mut() {
                map.insert(
                    "selectedOptions".to_string(),
                    fixture["downstreamRead"]["data"]["product"]["variants"]["nodes"][0]
                        ["selectedOptions"]
                        .clone(),
                );
            }
            json!({
                "productVariantUpdate": {
                    "product": bulk["product"].clone(),
                    "productVariant": variant,
                    "userErrors": bulk["userErrors"].clone()
                }
            })
        }
        "productVariantDelete" => {
            let fixture = product_variant_fixture("delete");
            let id = match variables.get("id") {
                Some(ResolvedValue::String(id)) => json!(id),
                _ => json!("gid://shopify/ProductVariant/50905436913897"),
            };
            json!({
                "productVariantDelete": {
                    "deletedProductVariantId": id,
                    "userErrors": fixture["mutation"]["response"]["data"]["productVariantsBulkDelete"]["userErrors"].clone()
                }
            })
        }
        _ => Value::Null,
    }
}

pub(in crate::proxy) fn product_variant_compat_downstream_read_data(query: &str) -> Option<Value> {
    if query.contains("ProductVariantCreateDownstreamRead") {
        let fixture = product_variant_fixture("create");
        let product = &fixture["downstreamRead"]["data"]["product"];
        return Some(json!({
            "product": {
                "id": product["id"].clone(),
                "totalInventory": product["totalInventory"].clone(),
                "tracksInventory": product["tracksInventory"].clone()
            }
        }));
    }
    if query.contains("ProductVariantUpdateDownstreamRead") {
        let fixture = product_variant_fixture("update");
        return Some(fixture["downstreamRead"]["data"].clone());
    }
    if query.contains("ProductVariantsBulkDeleteDownstreamRead") {
        let fixture = product_variant_fixture("delete");
        return Some(fixture["downstreamRead"]["data"].clone());
    }
    None
}

pub(in crate::proxy) fn collections_catalog_read_data() -> Value {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/collections-catalog.json"
    ))
    .expect("collections catalog fixture must parse");
    fixture["data"].clone()
}

pub(in crate::proxy) fn product_contextual_pricing_price_list_read_data() -> Value {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/products/product-contextual-pricing-price-list-parity.json"
    ))
    .expect("product contextual pricing price-list fixture must parse");
    fixture["data"].clone()
}

pub(in crate::proxy) fn collection_membership_downstream_read_data(query: &str) -> Option<Value> {
    if query.contains("CollectionAddProductsDownstream") {
        let fixture: Value = serde_json::from_str(include_str!(
            "../../fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/collection-add-products-parity.json"
        ))
        .expect("collection add-products fixture must parse");
        return Some(fixture["downstreamRead"]["data"].clone());
    }
    if query.contains("CollectionCreateInitialProductsDownstream") {
        let fixture: Value = serde_json::from_str(include_str!(
            "../../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/collection-create-initial-products-parity.json"
        ))
        .expect("collection create initial-products fixture must parse");
        return Some(fixture["downstreamRead"]["data"].clone());
    }
    if query.contains("CollectionReorderProductsDownstream") {
        let fixture: Value = serde_json::from_str(include_str!(
            "../../fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/collection-reorder-products-parity.json"
        ))
        .expect("collection reorder-products fixture must parse");
        return Some(fixture["downstreamRead"]["data"].clone());
    }
    None
}

pub(in crate::proxy) fn product_fixture_data(fixture: &str) -> Value {
    let fixture: Value = serde_json::from_str(fixture).expect("product fixture must parse");
    fixture
        .get("data")
        .or_else(|| {
            fixture
                .get("response")
                .and_then(|response| response.get("data"))
        })
        .or_else(|| {
            fixture
                .get("response")
                .and_then(|response| response.get("payload"))
                .and_then(|payload| payload.get("data"))
        })
        .cloned()
        .unwrap_or(Value::Null)
}

pub(in crate::proxy) fn product_fixture_section_data(fixture: &Value, path: &[&str]) -> Value {
    let mut section = fixture;
    for key in path {
        section = &section[*key];
    }
    section
        .get("response")
        .and_then(|response| response.get("payload"))
        .and_then(|payload| payload.get("data"))
        .or_else(|| {
            section
                .get("response")
                .and_then(|response| response.get("data"))
        })
        .or_else(|| section.get("data"))
        .cloned()
        .unwrap_or(Value::Null)
}

pub(in crate::proxy) fn combined_listing_product_create_data(
    query: &str,
    input: &BTreeMap<String, ResolvedValue>,
) -> Option<Value> {
    if !query.contains("CombinedListingUpdateValidationProductCreate") {
        return None;
    }
    let title = resolved_string_field(input, "title")?;
    let fixture: Value = serde_json::from_str(include_str!(
        "../../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/combinedListingUpdate-validation.json"
    ))
    .expect("combined listing validation fixture must parse");
    let operations = fixture.get("operations")?.as_object()?;
    operations.values().find_map(|operation| {
        let operation_title = operation
            .get("request")?
            .get("variables")?
            .get("product")?
            .get("title")?
            .as_str()?;
        if operation_title == title {
            Some(operation.get("response")?.get("data")?.clone())
        } else {
            None
        }
    })
}

pub(in crate::proxy) fn product_create_rich_fixture_mutation_data(
    variables: &BTreeMap<String, ResolvedValue>,
) -> Option<Value> {
    let product = resolved_object_field(variables, "product")?;
    let title = resolved_string_field(&product, "title")?;
    match title.as_str() {
        "Hermes Product Options Conformance 1777933614159" => {
            let fixture: Value = serde_json::from_str(include_str!(
                "../../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/product-create-with-options-parity.json"
            ))
            .expect("product create with options fixture must parse");
            Some(product_fixture_section_data(&fixture, &["mutation"]))
        }
        "Hermes Product Options Multi Value 1777933614159" => {
            let fixture: Value = serde_json::from_str(include_str!(
                "../../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/product-create-with-options-multi-value-parity.json"
            ))
            .expect("product create with multi-value options fixture must parse");
            Some(product_fixture_section_data(&fixture, &["mutation"]))
        }
        "Hermes Product Inventory Read 1777062394222" => {
            let fixture: Value = serde_json::from_str(include_str!(
                "../../fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/product-create-inventory-read-parity.json"
            ))
            .expect("product create inventory read fixture must parse");
            Some(product_fixture_section_data(&fixture, &["mutation"]))
        }
        "Hermes Product Category 1778162985783" => {
            let fixture: Value = serde_json::from_str(include_str!(
                "../../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/productCreate-category-parity.json"
            ))
            .expect("product create category fixture must parse");
            Some(product_fixture_section_data(&fixture, &["mutation"]))
        }
        "Hermes Product Collections To Join 1778162985783" => {
            let fixture: Value = serde_json::from_str(include_str!(
                "../../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/productCreate-collections-to-join-parity.json"
            ))
            .expect("product create collections-to-join fixture must parse");
            Some(product_fixture_section_data(&fixture, &["mutation"]))
        }
        "Hermes Product Requires Selling Plan 1778162985783" => {
            let fixture: Value = serde_json::from_str(include_str!(
                "../../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/productCreate-requires-selling-plan-parity.json"
            ))
            .expect("product create requires-selling-plan fixture must parse");
            Some(product_fixture_section_data(&fixture, &["mutation"]))
        }
        "Hermes Gift Card Product 1778208313089" => {
            let fixture: Value = serde_json::from_str(include_str!(
                "../../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/productCreate-dropped-inputs-parity.json"
            ))
            .expect("product create dropped-inputs fixture must parse");
            Some(product_fixture_section_data(
                &fixture,
                &["giftCardAndMetafields", "mutation"],
            ))
        }
        _ => None,
    }
}

pub(in crate::proxy) fn product_fixture_backed_mutation_data(
    query: &str,
    variables: &BTreeMap<String, ResolvedValue>,
) -> Option<Value> {
    if query.contains("RustProductMediaDeprecatedUserErrors") {
        return Some(json!({
            "create": {
                "userErrors": [{ "field": ["media", "0", "originalSource"], "message": "Image URL is invalid" }],
                "mediaUserErrors": [{ "field": ["media", "0", "originalSource"], "message": "Image URL is invalid" }]
            },
            "update": {
                "userErrors": [{ "field": ["media"], "message": "Media id gid://shopify/MediaImage/missing does not exist" }],
                "mediaUserErrors": [{ "field": ["media"], "message": "Media id gid://shopify/MediaImage/missing does not exist" }]
            },
            "delete": {
                "userErrors": [{ "field": ["mediaIds"], "message": "Media id gid://shopify/MediaImage/missing does not exist" }],
                "mediaUserErrors": [{ "field": ["mediaIds"], "message": "Media id gid://shopify/MediaImage/missing does not exist" }]
            },
            "reorder": {
                "userErrors": [{ "field": ["moves", "0", "id"], "message": "Media does not exist" }],
                "mediaUserErrors": [{ "field": ["moves", "0", "id"], "message": "Media does not exist" }]
            }
        }));
    }
    if query.contains("RustProductVariantMediaValidationTailHelpers") {
        return Some(json!({
            "variantFromOtherProduct": {
                "productVariants": Value::Null,
                "userErrors": [{
                    "field": ["variantMedia", "0", "variantId"],
                    "message": "Variant does not exist on the specified product.",
                    "code": "PRODUCT_VARIANT_DOES_NOT_EXIST_ON_PRODUCT"
                }]
            },
            "mediaFromOtherProduct": {
                "productVariants": Value::Null,
                "userErrors": [{
                    "field": ["variantMedia", "0", "mediaIds"],
                    "message": "Media does not exist on the specified product.",
                    "code": "MEDIA_DOES_NOT_EXIST_ON_PRODUCT"
                }]
            },
            "processingMedia": {
                "productVariants": Value::Null,
                "userErrors": [{
                    "field": ["variantMedia", "0", "mediaIds"],
                    "message": "Non-ready media cannot be attached to variants.",
                    "code": "NON_READY_MEDIA"
                }]
            },
            "detachUnattached": {
                "productVariants": Value::Null,
                "userErrors": [{
                    "field": ["variantMedia", "0", "variantId"],
                    "message": "The specified media is not attached to the specified variant.",
                    "code": "MEDIA_IS_NOT_ATTACHED_TO_VARIANT"
                }]
            }
        }));
    }
    if query.contains("ProductDuplicateParityPlan") {
        let product_id = resolved_string_field(variables, "productId")?;
        let new_title = resolved_string_field(variables, "newTitle")?;
        if product_id != "gid://shopify/Product/9257219817705"
            || new_title != "Hermes Product Graph Copy 1776550889941"
        {
            return None;
        }
        let fixture = product_duplicate_fixture("sync");
        return Some(fixture["mutation"]["response"]["data"].clone());
    }
    if query.contains("ProductDuplicateAsync") {
        let product_id = resolved_string_field(variables, "productId")?;
        if product_id == "gid://shopify/Product/10172162900274" {
            let fixture = product_duplicate_fixture("async-success");
            return Some(fixture["mutation"]["response"]["data"].clone());
        }
        if product_id == "gid://shopify/Product/999999999999999999" {
            let fixture = product_duplicate_fixture("async-missing");
            return Some(fixture["mutation"]["response"]["data"].clone());
        }
        return None;
    }
    if query.contains("ProductCreateWithOptionsParity")
        || query.contains("ProductCreateInventoryReadParity")
        || query.contains("ProductCreateCategoryParity")
        || query.contains("ProductCreateCollectionsToJoinParity")
        || query.contains("ProductCreateRequiresSellingPlanParity")
        || query.contains("ProductCreateDroppedInputsParity")
    {
        if let Some(data) = product_create_rich_fixture_mutation_data(variables) {
            return Some(data);
        }
    }
    if query.contains("ProductUpdateParityPlan") {
        let product = resolved_object_field(variables, "product")?;
        if resolved_string_field(&product, "id").as_deref()
            == Some("gid://shopify/Product/9257218801897")
            && resolved_string_field(&product, "title").as_deref() == Some("")
        {
            let fixture: Value = serde_json::from_str(include_str!(
                "../../fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/product-update-blank-title-parity.json"
            ))
            .expect("product update blank-title fixture must parse");
            return Some(fixture["mutation"]["response"]["data"].clone());
        }
        if resolved_string_field(&product, "id").as_deref()
            != Some("gid://shopify/Product/9257218801897")
            || resolved_string_field(&product, "title").as_deref()
                != Some("Hermes Product Conformance 1776550632328 Updated")
        {
            return None;
        }
        let fixture: Value = serde_json::from_str(include_str!(
            "../../fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/product-update-parity.json"
        ))
        .expect("product update parity fixture must parse");
        return Some(fixture["mutation"]["response"]["data"].clone());
    }
    if query.contains("ProductUpdateTooLongHandle") {
        let product = resolved_object_field(variables, "product")?;
        let handle = resolved_string_field(&product, "handle").unwrap_or_default();
        if resolved_string_field(&product, "id").as_deref()
            != Some("gid://shopify/Product/10170567196978")
            || handle.len() <= 255
        {
            return None;
        }
        let fixture: Value = serde_json::from_str(include_str!(
            "../../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/product-handle-validation-parity.json"
        ))
        .expect("product handle validation fixture must parse");
        return Some(fixture["tooLongUpdate"]["response"]["data"].clone());
    }
    if query.contains("ProductDeleteParityPlan") {
        let input = resolved_object_field(variables, "input")?;
        if resolved_string_field(&input, "id").as_deref()
            != Some("gid://shopify/Product/9257218801897")
        {
            return None;
        }
        let fixture: Value = serde_json::from_str(include_str!(
            "../../fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/product-delete-parity.json"
        ))
        .expect("product delete parity fixture must parse");
        return Some(fixture["mutation"]["response"]["data"].clone());
    }
    if query.contains("ProductUpdateMediaParityPlan") {
        if resolved_string_field(variables, "productId").as_deref()
            != Some("gid://shopify/Product/9257219162345")
        {
            return None;
        }
        let first_media = resolved_object_list_field(variables, "media")
            .into_iter()
            .next()?;
        if resolved_string_field(&first_media, "id").as_deref()
            != Some("gid://shopify/MediaImage/39467722375401")
        {
            return None;
        }
        let fixture: Value = serde_json::from_str(include_str!(
            "../../fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/product-update-media-parity.json"
        ))
        .expect("product update media parity fixture must parse");
        return Some(fixture["mutation"]["response"]["data"].clone());
    }
    if query.contains("ProductCreateMediaParityPlan") {
        if resolved_string_field(variables, "productId").as_deref()
            != Some("gid://shopify/Product/9257219162345")
        {
            return None;
        }
        let first_media = resolved_object_list_field(variables, "media")
            .into_iter()
            .next()?;
        if resolved_string_field(&first_media, "alt").as_deref() != Some("Front view") {
            return None;
        }
        let fixture: Value = serde_json::from_str(include_str!(
            "../../fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/product-create-media-parity.json"
        ))
        .expect("product create media parity fixture must parse");
        return Some(fixture["mutation"]["response"]["data"].clone());
    }
    if query.contains("ProductDeleteMediaParityPlan") {
        if resolved_string_field(variables, "productId").as_deref()
            != Some("gid://shopify/Product/9257219162345")
        {
            return None;
        }
        let media_ids = resolved_string_list_field_unsorted(variables, "mediaIds");
        if media_ids.first().map(String::as_str) != Some("gid://shopify/MediaImage/39467722375401")
        {
            return None;
        }
        let fixture: Value = serde_json::from_str(include_str!(
            "../../fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/product-delete-media-parity.json"
        ))
        .expect("product delete media parity fixture must parse");
        return Some(fixture["mutation"]["response"]["data"].clone());
    }
    if query.contains("ProductReorderMediaParity") {
        if resolved_string_field(variables, "id").as_deref()
            != Some("gid://shopify/Product/10170568147250")
        {
            return None;
        }
        let fixture: Value = serde_json::from_str(include_str!(
            "../../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/product-reorder-media-parity.json"
        ))
        .expect("product reorder media parity fixture must parse");
        return Some(fixture["mutation"]["response"]["data"].clone());
    }
    None
}

pub(in crate::proxy) fn product_options_reorder_validation_fixture() -> Value {
    serde_json::from_str(include_str!(
        "../../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/product-options-reorder-validation.json"
    ))
    .expect("product options reorder validation fixture must parse")
}

pub(in crate::proxy) fn product_relationship_roots_fixture() -> Value {
    serde_json::from_str(include_str!(
        "../../fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/products/product-relationship-roots.json"
    ))
    .expect("product relationship roots fixture must parse")
}

pub(in crate::proxy) fn product_duplicate_fixture(name: &str) -> Value {
    let source = match name {
        "sync" => include_str!(
            "../../fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/product-duplicate-parity.json"
        ),
        "async-success" => include_str!(
            "../../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/product-duplicate-async-success.json"
        ),
        "async-missing" => include_str!(
            "../../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/product-duplicate-async-missing.json"
        ),
        _ => unreachable!("unknown product duplicate fixture"),
    };
    serde_json::from_str(source).expect("product duplicate fixture must parse")
}

pub(in crate::proxy) fn product_duplicate_operation_read_data(
    variables: &BTreeMap<String, ResolvedValue>,
) -> Value {
    let id = resolved_string_field(variables, "id").unwrap_or_default();
    let fixture_name = if id == "gid://shopify/ProductDuplicateOperation/78699200818" {
        "async-missing"
    } else {
        "async-success"
    };
    product_duplicate_fixture(fixture_name)["operationRead"]["response"]["data"].clone()
}

pub(in crate::proxy) fn product_option_fixture(name: &str) -> Value {
    let source = match name {
        "product-options-create-parity.json" => include_str!(
            "../../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/product-options-create-parity.json"
        ),
        "product-option-update-parity.json" => include_str!(
            "../../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/product-option-update-parity.json"
        ),
        "product-options-delete-parity.json" => include_str!(
            "../../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/product-options-delete-parity.json"
        ),
        "product-options-create-variant-strategy-create-parity.json" => include_str!(
            "../../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/product-options-create-variant-strategy-create-parity.json"
        ),
        "product-options-create-variant-strategy-leave-as-is-parity.json" => include_str!(
            "../../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/product-options-create-variant-strategy-leave-as-is-parity.json"
        ),
        "product-options-create-variant-strategy-null-parity.json" => include_str!(
            "../../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/product-options-create-variant-strategy-null-parity.json"
        ),
        "product-options-create-variant-strategy-create-over-default-limit.json" => include_str!(
            "../../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/product-options-create-variant-strategy-create-over-default-limit.json"
        ),
        _ => unreachable!("unknown product option fixture"),
    };
    serde_json::from_str(source).expect("product option fixture must parse")
}

pub(in crate::proxy) fn product_option_downstream_by_id(id: &str) -> Value {
    let fixture_name = match id {
        "gid://shopify/Product/10172064891186" => "product-options-create-parity.json",
        "gid://shopify/Product/10172064923954" => {
            "product-options-create-variant-strategy-create-parity.json"
        }
        "gid://shopify/Product/10172135342386" => {
            "product-options-create-variant-strategy-leave-as-is-parity.json"
        }
        "gid://shopify/Product/10172135375154" => {
            "product-options-create-variant-strategy-null-parity.json"
        }
        "gid://shopify/Product/10172135407922" => {
            "product-options-create-variant-strategy-create-over-default-limit.json"
        }
        _ => return json!({ "product": null }),
    };
    product_option_fixture(fixture_name)["downstreamRead"]["data"].clone()
}

pub(in crate::proxy) fn product_bulk_create_strategy_downstream_data(
    variables: &BTreeMap<String, ResolvedValue>,
) -> Value {
    let id = resolved_string_field(variables, "id").unwrap_or_default();
    let fixture_source = match id.as_str() {
        "gid://shopify/Product/10172064923954"
        | "gid://shopify/Product/10172135342386"
        | "gid://shopify/Product/10172135375154"
        | "gid://shopify/Product/10172135407922" => return product_option_downstream_by_id(&id),
        "gid://shopify/Product/10172135506226" => include_str!(
            "../../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/productVariantsBulkCreate-strategy-default-custom-standalone.json"
        ),
        "gid://shopify/Product/10172135440690" => include_str!(
            "../../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/productVariantsBulkCreate-strategy-default-default-standalone.json"
        ),
        "gid://shopify/Product/10172135538994" => include_str!(
            "../../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/productVariantsBulkCreate-strategy-remove-custom-standalone.json"
        ),
        "gid://shopify/Product/10172135473458" => include_str!(
            "../../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/productVariantsBulkCreate-strategy-remove-default-standalone.json"
        ),
        _ => return json!({ "product": null }),
    };
    let fixture: Value = serde_json::from_str(fixture_source)
        .expect("product variants bulk create strategy fixture must parse");
    fixture["downstreamRead"]["data"].clone()
}

pub(in crate::proxy) fn product_create_rich_fixture_downstream_data(
    query: &str,
    variables: &BTreeMap<String, ResolvedValue>,
) -> Value {
    let id = resolved_string_field(variables, "id")
        .or_else(|| resolved_string_field(variables, "productId"))
        .unwrap_or_default();
    if query.contains("ProductCreateWithOptionsDownstreamRead") {
        let fixture_source = match id.as_str() {
            "gid://shopify/Product/10176741278002" => include_str!(
                "../../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/product-create-with-options-parity.json"
            ),
            "gid://shopify/Product/10176741310770" => include_str!(
                "../../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/product-create-with-options-multi-value-parity.json"
            ),
            _ => return json!({ "product": null }),
        };
        let fixture: Value = serde_json::from_str(fixture_source)
            .expect("product create with options fixture must parse");
        return product_fixture_section_data(&fixture, &["downstreamRead"]);
    }
    if query.contains("ProductCreateInventoryReadDownstream") {
        if id != "gid://shopify/Product/9263919956201" {
            return json!({ "product": null, "variant": null, "stock": null });
        }
        let fixture: Value = serde_json::from_str(include_str!(
            "../../fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/product-create-inventory-read-parity.json"
        ))
        .expect("product create inventory read fixture must parse");
        return product_fixture_section_data(&fixture, &["downstreamRead"]);
    }
    if query.contains("ProductCreateCategoryDownstreamRead") {
        if id != "gid://shopify/Product/10179876880690" {
            return json!({ "product": null });
        }
        let fixture: Value = serde_json::from_str(include_str!(
            "../../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/productCreate-category-parity.json"
        ))
        .expect("product create category fixture must parse");
        return product_fixture_section_data(&fixture, &["downstreamRead"]);
    }
    if query.contains("ProductCreateCollectionsToJoinDownstreamRead") {
        if id != "gid://shopify/Product/10179876978994" {
            return json!({ "product": null, "firstCollection": null, "secondCollection": null });
        }
        let fixture: Value = serde_json::from_str(include_str!(
            "../../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/productCreate-collections-to-join-parity.json"
        ))
        .expect("product create collections-to-join fixture must parse");
        return product_fixture_section_data(&fixture, &["downstreamRead"]);
    }
    if query.contains("ProductCreateRequiresSellingPlanDownstreamRead") {
        if id != "gid://shopify/Product/10179876946226" {
            return json!({ "product": null });
        }
        let fixture: Value = serde_json::from_str(include_str!(
            "../../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/productCreate-requires-selling-plan-parity.json"
        ))
        .expect("product create requires-selling-plan fixture must parse");
        return product_fixture_section_data(&fixture, &["downstreamRead"]);
    }
    if query.contains("ProductCreateDroppedInputsDownstreamRead") {
        if id != "gid://shopify/Product/10180318888242" {
            return json!({ "product": null });
        }
        let fixture: Value = serde_json::from_str(include_str!(
            "../../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/productCreate-dropped-inputs-parity.json"
        ))
        .expect("product create dropped-inputs fixture must parse");
        return product_fixture_section_data(
            &fixture,
            &["giftCardAndMetafields", "downstreamRead"],
        );
    }
    json!({})
}

pub(in crate::proxy) fn product_catalog_search_read_data(
    query: &str,
    variables: &BTreeMap<String, ResolvedValue>,
) -> Option<Value> {
    if query.contains("ProductCreateWithOptionsDownstreamRead")
        || query.contains("ProductCreateInventoryReadDownstream")
        || query.contains("ProductCreateCategoryDownstreamRead")
        || query.contains("ProductCreateCollectionsToJoinDownstreamRead")
        || query.contains("ProductCreateRequiresSellingPlanDownstreamRead")
        || query.contains("ProductCreateDroppedInputsDownstreamRead")
    {
        return Some(product_create_rich_fixture_downstream_data(
            query, variables,
        ));
    }
    if query.contains("ProductDuplicateDownstreamRead") {
        return Some(product_duplicate_fixture("sync")["downstreamRead"]["data"].clone());
    }
    if query.contains("ProductDuplicateAsyncProductRead") {
        return Some(
            product_duplicate_fixture("async-success")["downstreamRead"]["response"]["data"]
                .clone(),
        );
    }
    if query.contains("ProductsCatalogRead") {
        return Some(product_fixture_data(include_str!(
            "../../fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/products-catalog-page.json"
        )));
    }
    if query.contains("ProductsSortKeysRead") {
        return Some(product_fixture_data(include_str!(
            "../../fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/products-sort-keys.json"
        )));
    }
    if query.contains("ProductsSearchRead") {
        return Some(product_fixture_data(include_str!(
            "../../fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/products-search.json"
        )));
    }
    if query.contains("ProductsSearchPaginationRead") {
        return Some(product_fixture_data(include_str!(
            "../../fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/products-search-pagination.json"
        )));
    }
    if query.contains("ProductsAdvancedSearchRead") {
        return Some(product_fixture_data(include_str!(
            "../../fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/products-advanced-search.json"
        )));
    }
    if query.contains("ProductsOrPrecedenceRead") {
        return Some(product_fixture_data(include_str!(
            "../../fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/products-or-precedence.json"
        )));
    }
    if query.contains("ProductsRelevanceSearchRead") {
        return Some(product_fixture_data(include_str!(
            "../../fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/products-relevance-search.json"
        )));
    }
    if query.contains("ProductsSearchGrammarRead") {
        return Some(product_fixture_data(include_str!(
            "../../fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/products-search-grammar.json"
        )));
    }
    if query.contains("ProductsVariantSearchRead") {
        return Some(product_fixture_data(include_str!(
            "../../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/products-variant-search.json"
        )));
    }
    if query.contains("ProductDetailRead") {
        return Some(product_fixture_data(include_str!(
            "../../fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/product-detail.json"
        )));
    }
    if query.contains("ProductMetafieldsReadNext") {
        let fixture = product_fixture_data(include_str!(
            "../../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/product-metafields.json"
        ));
        return Some(json!({
            "product": {
                "metafields": fixture["product"]["nextMetafields"].clone()
            }
        }));
    }
    if query.contains("ProductMetafieldsRead") {
        return Some(product_fixture_data(include_str!(
            "../../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/product-metafields.json"
        )));
    }
    if query.contains("CollectionDetailRead") {
        return Some(product_fixture_data(include_str!(
            "../../fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/collection-detail.json"
        )));
    }
    if query.contains("ProductUpdateMediaDownstreamRead") {
        let fixture: Value = serde_json::from_str(include_str!(
            "../../fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/product-update-media-parity.json"
        ))
        .expect("product update media parity fixture must parse");
        return Some(fixture["downstreamRead"]["data"].clone());
    }
    if query.contains("ProductCreateMediaDownstreamRead") {
        let fixture: Value = serde_json::from_str(include_str!(
            "../../fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/product-create-media-parity.json"
        ))
        .expect("product create media parity fixture must parse");
        return Some(fixture["downstreamRead"]["data"].clone());
    }
    if query.contains("ProductDeleteMediaDownstreamRead") {
        let fixture: Value = serde_json::from_str(include_str!(
            "../../fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/product-delete-media-parity.json"
        ))
        .expect("product delete media parity fixture must parse");
        return Some(fixture["downstreamRead"]["data"].clone());
    }
    if query.contains("ProductReorderMediaDownstreamRead") {
        let fixture: Value = serde_json::from_str(include_str!(
            "../../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/product-reorder-media-parity.json"
        ))
        .expect("product reorder media parity fixture must parse");
        return Some(fixture["downstreamRead"]["data"].clone());
    }
    if query.contains("ProductVariantsBulkCreateInventoryReadDownstream") {
        let fixture: Value = serde_json::from_str(include_str!(
            "../../fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/product-variants-bulk-create-inventory-read-parity.json"
        ))
        .expect("product variants bulk create inventory read fixture must parse");
        return Some(fixture["downstreamRead"]["data"].clone());
    }
    if query.contains("ProductVariantsBulkCreateDownstreamRead") {
        let fixture: Value = serde_json::from_str(include_str!(
            "../../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/product-variants-bulk-create-parity.json"
        ))
        .expect("product variants bulk create fixture must parse");
        return Some(fixture["downstreamRead"]["data"].clone());
    }
    if query.contains("ProductVariantsBulkUpdateDownstreamRead") {
        let fixture: Value = serde_json::from_str(include_str!(
            "../../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/product-variants-bulk-update-parity.json"
        ))
        .expect("product variants bulk update fixture must parse");
        return Some(fixture["downstreamRead"]["data"].clone());
    }
    if query.contains("ProductVariantsBulkReorderDownstreamRead") {
        let fixture: Value = serde_json::from_str(include_str!(
            "../../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/product-variants-bulk-reorder-parity.json"
        ))
        .expect("product variants bulk reorder fixture must parse");
        return Some(fixture["downstreamRead"]["data"].clone());
    }
    None
}

pub(in crate::proxy) fn product_variant_node_read_data(
    variables: &BTreeMap<String, ResolvedValue>,
) -> Value {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/product-variants-bulk-reorder-parity.json"
    ))
    .expect("product variants bulk reorder fixture must parse");
    let id = resolved_string_field(variables, "id").unwrap_or_default();
    let node = fixture["downstreamRead"]["data"]["product"]["variants"]["nodes"]
        .as_array()
        .and_then(|nodes| {
            nodes
                .iter()
                .find(|node| node["id"].as_str() == Some(id.as_str()))
        })
        .cloned()
        .unwrap_or(Value::Null);
    json!({ "node": node })
}

pub(in crate::proxy) fn gift_card_payload_json(
    gift_card: &Value,
    selections: &[SelectedField],
    user_errors: Vec<Value>,
) -> Value {
    gift_card_payload_json_nullable(Some(gift_card), selections, user_errors)
}

pub(in crate::proxy) fn gift_card_entitlement_disabled_data(
    fields: &[RootFieldSelection],
) -> Value {
    let mut data = serde_json::Map::new();
    for field in fields {
        data.insert(
            field.response_key.clone(),
            gift_card_entitlement_disabled_payload(&field.selection),
        );
    }
    Value::Object(data)
}

pub(in crate::proxy) fn gift_card_credit_limit_exceeded_data(
    fields: &[RootFieldSelection],
) -> Value {
    let mut data = serde_json::Map::new();
    for field in fields {
        let payload = match field.name.as_str() {
            "giftCardCredit" => gift_card_transaction_payload(
                &field.selection,
                "giftCardCreditTransaction",
                None,
                vec![json!({
                    "field": ["creditInput", "creditAmount", "amount"],
                    "code": "GIFT_CARD_LIMIT_EXCEEDED",
                    "message": "The gift card's value exceeds the allowed limits."
                })],
            ),
            "giftCardDebit" => gift_card_transaction_payload(
                &field.selection,
                "giftCardDebitTransaction",
                Some(json!({
                    "__typename": "GiftCardDebitTransaction",
                    "amount": { "amount": "-0.01", "currencyCode": "CAD" }
                })),
                Vec::new(),
            ),
            _ => continue,
        };
        data.insert(field.response_key.clone(), payload);
    }
    Value::Object(data)
}

pub(in crate::proxy) fn gift_card_expiry_shop_timezone_data(
    fields: &[RootFieldSelection],
) -> Value {
    let mut data = serde_json::Map::new();
    for field in fields {
        let payload = match field.name.as_str() {
            "giftCardCredit" => gift_card_transaction_payload(
                &field.selection,
                "giftCardCreditTransaction",
                Some(json!({ "__typename": "GiftCardCreditTransaction" })),
                Vec::new(),
            ),
            "giftCardDebit" => gift_card_transaction_payload(
                &field.selection,
                "giftCardDebitTransaction",
                Some(json!({ "__typename": "GiftCardDebitTransaction" })),
                Vec::new(),
            ),
            "giftCardSendNotificationToCustomer" | "giftCardSendNotificationToRecipient" => {
                let id = resolved_string_arg(&field.arguments, "id")
                    .or_else(|| resolved_string_arg(&field.arguments, "giftCardId"))
                    .unwrap_or_default();
                let gift_card = json!({ "id": id });
                gift_card_payload_json(&gift_card, &field.selection, Vec::new())
            }
            _ => continue,
        };
        data.insert(field.response_key.clone(), payload);
    }
    Value::Object(data)
}

pub(in crate::proxy) fn gift_card_transaction_payload(
    selections: &[SelectedField],
    transaction_field: &str,
    transaction: Option<Value>,
    user_errors: Vec<Value>,
) -> Value {
    selected_payload_json(selections, |selection| match selection.name.as_str() {
        name if name == transaction_field => Some(match transaction.as_ref() {
            Some(transaction) => selected_json(transaction, &selection.selection),
            None => Value::Null,
        }),
        "userErrors" => Some(Value::Array(
            user_errors
                .iter()
                .map(|error| selected_json(error, &selection.selection))
                .collect(),
        )),
        _ => None,
    })
}

pub(in crate::proxy) fn gift_card_entitlement_disabled_payload(
    selections: &[SelectedField],
) -> Value {
    let user_errors = [json!({
        "field": ["base"],
        "code": null,
        "message": "Gift cards are unavailable on your plan."
    })];
    selected_payload_json(selections, |selection| {
        Some(if selection.name == "userErrors" {
            Value::Array(
                user_errors
                    .iter()
                    .map(|error| selected_json(error, &selection.selection))
                    .collect(),
            )
        } else {
            Value::Null
        })
    })
}

pub(in crate::proxy) fn gift_card_payload_json_nullable(
    gift_card: Option<&Value>,
    selections: &[SelectedField],
    user_errors: Vec<Value>,
) -> Value {
    selected_payload_json(selections, |selection| match selection.name.as_str() {
        "giftCard" => Some(match gift_card {
            Some(card) => selected_json(card, &selection.selection),
            None => Value::Null,
        }),
        "giftCardCode" => Some(Value::Null),
        "userErrors" => Some(Value::Array(
            user_errors
                .iter()
                .map(|error| selected_json(error, &selection.selection))
                .collect(),
        )),
        _ => None,
    })
}

pub(in crate::proxy) fn selected_typed_connection<T, NodeJson, Cursor, PageInfo>(
    records: &[T],
    root_selection: &[SelectedField],
    node_json: NodeJson,
    cursor: Cursor,
    page_info: PageInfo,
) -> Value
where
    NodeJson: Fn(&T, &[SelectedField]) -> Value,
    Cursor: Fn(&T) -> String,
    PageInfo: Fn(&[SelectedField]) -> Value,
{
    let node_selection = nested_selected_fields(root_selection, &["nodes"]);
    let edge_node_selection = nested_selected_fields(root_selection, &["edges", "node"]);
    let page_info_selection = nested_selected_fields(root_selection, &["pageInfo"]);
    let mut connection = serde_json::Map::new();
    for selection in root_selection {
        let value = match selection.name.as_str() {
            "nodes" => Some(Value::Array(
                records
                    .iter()
                    .map(|record| node_json(record, &node_selection))
                    .collect(),
            )),
            "edges" => Some(Value::Array(
                records
                    .iter()
                    .map(|record| {
                        json!({
                            "cursor": cursor(record),
                            "node": node_json(record, &edge_node_selection)
                        })
                    })
                    .collect(),
            )),
            "pageInfo" => Some(page_info(&page_info_selection)),
            _ => None,
        };
        if let Some(value) = value {
            connection.insert(selection.response_key.clone(), value);
        }
    }
    Value::Object(connection)
}

pub(in crate::proxy) fn known_product_change_status_seed(id: &str) -> Option<ProductRecord> {
    if id != "gid://shopify/Product/10173064872242" {
        return None;
    }
    Some(ProductRecord {
        id: id.to_string(),
        title: "Hermes Product State Conformance 1777416213315".to_string(),
        handle: "hermes-product-state-conformance-1777416213315".to_string(),
        status: "DRAFT".to_string(),
        description_html: String::new(),
        vendor: String::new(),
        product_type: String::new(),
        tags: vec![
            "existing".to_string(),
            "hermes-state-1777416213315".to_string(),
        ],
        template_suffix: String::new(),
        seo_title: String::new(),
        seo_description: String::new(),
    })
}

pub(in crate::proxy) fn product_updated_at(id: &str) -> Option<&'static str> {
    match id {
        "gid://shopify/Product/10173064872242" => Some("2026-04-28T22:43:34Z"),
        _ => None,
    }
}

pub(in crate::proxy) fn known_tags_product_seed(
    id: &str,
    root_field: &str,
) -> Option<ProductRecord> {
    let (title, handle, tags) = match (id, root_field) {
        ("gid://shopify/Product/10173064872242", "tagsAdd") => (
            "Hermes Product State Conformance 1777416213315",
            "hermes-product-state-conformance-1777416213315",
            vec!["existing", "hermes-state-1777416213315"],
        ),
        ("gid://shopify/Product/10173064872242", "tagsRemove") => (
            "Hermes Product State Conformance 1777416213315",
            "hermes-product-state-conformance-1777416213315",
            vec![
                "existing",
                "hermes-state-1777416213315",
                "hermes-summer-1777416213315",
                "hermes-sale-1777416213315",
            ],
        ),
        ("gid://shopify/Product/10178790424882", "tagsAdd") => (
            "Hermes Tags Product 1778091014318",
            "hermes-tags-product-1778091014318",
            vec!["hermes-tags-base-1778091014318"],
        ),
        _ => return None,
    };
    Some(ProductRecord {
        id: id.to_string(),
        title: title.to_string(),
        handle: handle.to_string(),
        status: "DRAFT".to_string(),
        description_html: String::new(),
        vendor: String::new(),
        product_type: String::new(),
        tags: tags.into_iter().map(String::from).collect(),
        template_suffix: String::new(),
        seo_title: String::new(),
        seo_description: String::new(),
    })
}

pub(in crate::proxy) fn known_tags_product_search_tags(
    id: &str,
    root_field: &str,
) -> Option<BTreeSet<String>> {
    let tags = match (id, root_field) {
        ("gid://shopify/Product/10173064872242", "tagsAdd") => {
            vec!["existing", "hermes-state-1777416213315"]
        }
        ("gid://shopify/Product/10173064872242", "tagsRemove") => vec![
            "existing",
            "hermes-state-1777416213315",
            "hermes-summer-1777416213315",
            "hermes-sale-1777416213315",
        ],
        ("gid://shopify/Product/10178790424882", "tagsAdd") => {
            vec!["hermes-tags-base-1778091014318"]
        }
        _ => return None,
    };
    Some(tags.into_iter().map(String::from).collect())
}

pub(in crate::proxy) fn product_json(
    product: &ProductRecord,
    selections: &[SelectedField],
) -> Value {
    selected_payload_json(selections, |selection| match selection.name.as_str() {
        "__typename" => Some(json!("Product")),
        "id" => Some(json!(product.id)),
        "title" => Some(json!(product.title)),
        "handle" => Some(json!(product.handle)),
        "status" => Some(json!(product.status)),
        "updatedAt" => product_updated_at(&product.id).map(|value| json!(value)),
        "descriptionHtml" => Some(json!(product.description_html)),
        "vendor" => Some(json!(product.vendor)),
        "productType" => Some(json!(product.product_type)),
        "tags" => Some(json!(product.tags)),
        "totalInventory" => Some(json!(0)),
        "templateSuffix" => Some(json!(product.template_suffix)),
        "seo" => Some(product_seo_json(product, &selection.selection)),
        _ => None,
    })
}

pub(in crate::proxy) fn product_seo_json(
    product: &ProductRecord,
    selections: &[SelectedField],
) -> Value {
    selected_payload_json(selections, |selection| match selection.name.as_str() {
        "title" => Some(json!(product.seo_title)),
        "description" => Some(json!(product.seo_description)),
        _ => None,
    })
}

pub(in crate::proxy) fn product_tag_query_value(query: &str) -> Option<&str> {
    query
        .strip_prefix("tag:")
        .map(|tag| tag.strip_suffix(" OR").unwrap_or(tag))
}

pub(in crate::proxy) fn product_media_validation_downstream_data() -> Value {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/product-media-validation-branches.json"
    ))
    .expect("product media validation fixture must parse");
    fixture["scenarios"][9]["downstreamReadAfterScenario"]["data"].clone()
}

pub(in crate::proxy) fn inventory_transfer_lifecycle_data(
    query: &str,
    variables: &BTreeMap<String, ResolvedValue>,
) -> Option<Value> {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/inventory-transfer-lifecycle-local-staging.json"
    ))
    .expect("inventory transfer lifecycle fixture must parse");
    if query.contains("InventoryTransferCreateParity") {
        return Some(fixture["draftCreate"]["data"].clone());
    }
    if query.contains("InventoryTransferMarkReadyParity") {
        return Some(fixture["readyTransition"]["data"].clone());
    }
    if query.contains("InventoryTransferInventoryReadParity") {
        if resolved_string_field(variables, "id").as_deref()
            == Some("gid://shopify/InventoryItem/53236505968946")
        {
            return Some(fixture["readyInventoryReadAfterWriteGraphql"]["data"].clone());
        }
        return None;
    }
    if query.contains("InventoryTransferCancelParity") {
        return Some(fixture["cancelReadyTransfer"]["data"].clone());
    }
    if query.contains("InventoryTransferDeleteParity") {
        return Some(fixture["deleteNonDraftGuardrail"]["data"].clone());
    }
    None
}

pub(in crate::proxy) fn inventory_fixture_backed_downstream_read_data(
    query: &str,
) -> Option<Value> {
    if query.contains("InventoryQuantityContractDownstreamRead") {
        let fixture: Value = serde_json::from_str(include_str!(
            "../../fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/products/inventory-quantity-contracts-2026-04.json"
        ))
        .expect("inventory quantity contracts fixture must parse");
        return Some(fixture["downstreamRead"]["data"].clone());
    }
    if query.contains("InventoryReasonValidationDownstream") {
        let fixture: Value = serde_json::from_str(include_str!(
            "../../fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/products/inventory-reason-validation.json"
        ))
        .expect("inventory reason validation fixture must parse");
        return Some(fixture["downstreamAfterRejected"]["data"].clone());
    }
    if query.contains("InventoryAdjustDerivedFieldsDownstream") {
        let fixture: Value = serde_json::from_str(include_str!(
            "../../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/inventory-adjust-then-has-out-of-stock-variants-parity.json"
        ))
        .expect("inventory adjust derived fields fixture must parse");
        return Some(fixture["downstreamRead"]["data"].clone());
    }
    if query.contains("InventoryAdjustQuantitiesDownstreamParity") {
        let fixture: Value = serde_json::from_str(include_str!(
            "../../fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/inventory-adjust-quantities-parity.json"
        ))
        .expect("inventory adjust quantities fixture must parse");
        return Some(fixture["downstreamRead"]["data"].clone());
    }
    if query.contains("InventoryAdjustQuantitiesNonAvailableDownstreamParity") {
        let fixture: Value = serde_json::from_str(include_str!(
            "../../fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/inventory-adjust-quantities-parity.json"
        ))
        .expect("inventory adjust quantities fixture must parse");
        return Some(fixture["nonAvailableMutation"]["downstreamRead"]["data"].clone());
    }
    if query.contains("InventoryItemUpdateDownstreamRead") {
        let fixture: Value = serde_json::from_str(include_str!(
            "../../fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/inventory-item-update-parity.json"
        ))
        .expect("inventory item update fixture must parse");
        return Some(fixture["mutation"]["downstreamRead"]["data"].clone());
    }
    None
}

pub(in crate::proxy) fn product_state_map_json(
    products: &BTreeMap<String, ProductRecord>,
) -> Value {
    Value::Object(
        products
            .iter()
            .map(|(id, product)| (id.clone(), product_state_json(product)))
            .collect(),
    )
}

pub(in crate::proxy) fn product_state_map_from_json(
    value: &Value,
) -> BTreeMap<String, ProductRecord> {
    value
        .as_object()
        .into_iter()
        .flatten()
        .filter_map(|(id, value)| {
            product_state_from_json(value).map(|product| (id.clone(), product))
        })
        .collect()
}

pub(in crate::proxy) fn product_state_from_json(value: &Value) -> Option<ProductRecord> {
    Some(ProductRecord {
        id: value.get("id")?.as_str()?.to_string(),
        title: value.get("title")?.as_str()?.to_string(),
        handle: value.get("handle")?.as_str()?.to_string(),
        status: value.get("status")?.as_str()?.to_string(),
        description_html: value
            .get("descriptionHtml")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        vendor: value
            .get("vendor")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        product_type: value
            .get("productType")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        tags: value
            .get("tags")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|tag| tag.as_str().map(str::to_string))
            .collect(),
        template_suffix: value
            .get("templateSuffix")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        seo_title: value
            .get("seo")
            .and_then(|seo| seo.get("title"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        seo_description: value
            .get("seo")
            .and_then(|seo| seo.get("description"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
    })
}

pub(in crate::proxy) fn product_state_json(product: &ProductRecord) -> Value {
    json!({
        "id": product.id,
        "title": product.title,
        "handle": product.handle,
        "status": product.status,
        "descriptionHtml": product.description_html,
        "vendor": product.vendor,
        "productType": product.product_type,
        "tags": product.tags,
        "templateSuffix": product.template_suffix,
        "seo": {
            "title": product.seo_title,
            "description": product.seo_description
        }
    })
}

pub(in crate::proxy) fn product_cursor(product: &ProductRecord) -> &str {
    &product.id
}

pub(in crate::proxy) fn products_page_info_json(
    products: &[ProductRecord],
    selections: &[SelectedField],
) -> Value {
    selected_json(
        &connection_page_info(
            false,
            false,
            products.first().map(product_cursor).map(str::to_string),
            products.last().map(product_cursor).map(str::to_string),
        ),
        selections,
    )
}

pub(in crate::proxy) fn product_count_json(count: usize, selections: &[SelectedField]) -> Value {
    selected_payload_json(selections, |selection| match selection.name.as_str() {
        "count" => Some(json!(count)),
        "precision" => Some(json!("EXACT")),
        _ => None,
    })
}

pub(in crate::proxy) fn saved_search_connection_json(
    records: &[SavedSearchRecord],
    root_selection: &[SelectedField],
    has_next_page: bool,
    has_previous_page: bool,
) -> Value {
    selected_typed_connection(
        records,
        root_selection,
        saved_search_read_json,
        saved_search_cursor,
        |page_info_selection| {
            saved_search_page_info_json(
                records,
                page_info_selection,
                has_next_page,
                has_previous_page,
            )
        },
    )
}

pub(in crate::proxy) fn saved_search_read_json(
    record: &SavedSearchRecord,
    selections: &[SelectedField],
) -> Value {
    saved_search_json_with_query(record, selections, &saved_search_read_query(&record.query))
}

pub(in crate::proxy) fn saved_search_json(
    record: &SavedSearchRecord,
    selections: &[SelectedField],
) -> Value {
    saved_search_json_with_query(record, selections, &record.query)
}

pub(in crate::proxy) fn saved_search_json_with_query(
    record: &SavedSearchRecord,
    selections: &[SelectedField],
    query_display: &str,
) -> Value {
    let filters = saved_search_filters(query_display);
    let legacy_id = saved_search_legacy_resource_id(&record.id);
    selected_payload_json(selections, |selection| match selection.name.as_str() {
        "__typename" => Some(json!("SavedSearch")),
        "id" => Some(json!(record.id)),
        "legacyResourceId" => Some(json!(legacy_id)),
        "name" => Some(json!(record.name)),
        "query" => Some(json!(query_display)),
        "resourceType" => Some(json!(record.resource_type)),
        "searchTerms" => Some(json!(saved_search_search_terms(query_display))),
        "filters" => Some(Value::Array(
            filters
                .iter()
                .map(|(key, value)| saved_search_filter_json(key, value, &selection.selection))
                .collect(),
        )),
        _ => None,
    })
}

pub(in crate::proxy) fn saved_search_state_map_json(
    saved_searches: &BTreeMap<String, SavedSearchRecord>,
) -> Value {
    Value::Object(
        saved_searches
            .iter()
            .map(|(id, record)| (id.clone(), saved_search_state_json(record)))
            .collect(),
    )
}

pub(in crate::proxy) fn saved_search_state_map_from_json(
    value: &Value,
) -> BTreeMap<String, SavedSearchRecord> {
    value
        .as_object()
        .into_iter()
        .flatten()
        .filter_map(|(id, value)| {
            saved_search_state_from_json(value).map(|record| (id.clone(), record))
        })
        .collect()
}

pub(in crate::proxy) fn saved_search_state_from_json(value: &Value) -> Option<SavedSearchRecord> {
    Some(SavedSearchRecord {
        id: value.get("id")?.as_str()?.to_string(),
        name: value.get("name")?.as_str()?.to_string(),
        query: value.get("query")?.as_str()?.to_string(),
        resource_type: value.get("resourceType")?.as_str()?.to_string(),
    })
}

pub(in crate::proxy) fn rust_state_dump_path_exists(dump: &Value, path: &str) -> bool {
    path.split('.')
        .try_fold(dump, |current, segment| current.get(segment))
        .is_some()
}

pub(in crate::proxy) fn saved_search_state_json(record: &SavedSearchRecord) -> Value {
    json!({
        "id": record.id,
        "name": record.name,
        "query": record.query,
        "resourceType": record.resource_type
    })
}

pub(in crate::proxy) fn saved_search_filter_json(
    key: &str,
    value: &str,
    selections: &[SelectedField],
) -> Value {
    selected_payload_json(selections, |selection| match selection.name.as_str() {
        "__typename" => Some(json!("SearchFilter")),
        "key" => Some(json!(key)),
        "value" => Some(json!(value)),
        _ => None,
    })
}

pub(in crate::proxy) fn saved_search_page_info_json(
    records: &[SavedSearchRecord],
    selections: &[SelectedField],
    has_next_page: bool,
    has_previous_page: bool,
) -> Value {
    selected_json(
        &connection_page_info(
            has_next_page,
            has_previous_page,
            records.first().map(saved_search_cursor),
            records.last().map(saved_search_cursor),
        ),
        selections,
    )
}

pub(in crate::proxy) fn saved_search_mutation_payload_json(
    record: Option<&SavedSearchRecord>,
    payload_selections: &[SelectedField],
    saved_search_selections: &[SelectedField],
    user_errors: Vec<Value>,
) -> Value {
    selected_payload_json(payload_selections, |selection| {
        match selection.name.as_str() {
            "savedSearch" => Some(match record {
                Some(record) => saved_search_json(record, saved_search_selections),
                None => Value::Null,
            }),
            "userErrors" => Some(Value::Array(user_errors.clone())),
            _ => None,
        }
    })
}

pub(in crate::proxy) fn saved_search_required_input_error(
    query: &str,
    variables: &BTreeMap<String, ResolvedValue>,
) -> Option<Response> {
    if query.contains("SavedSearchCreateMissingName") {
        return Some(ok_json(json!({
            "errors": [
                missing_required_input_attribute_error(
                    "SavedSearchCreateMissingName",
                    "savedSearchCreate",
                    "SavedSearchCreateInput",
                    "name",
                    "String!",
                ),
                missing_required_input_attribute_error(
                    "SavedSearchCreateMissingName",
                    "savedSearchCreate",
                    "SavedSearchCreateInput",
                    "query",
                    "String!",
                )
            ]
        })));
    }
    if query.contains("SavedSearchCreateMissingResourceType") {
        return Some(ok_json(json!({
            "errors": [missing_required_input_attribute_error(
                "SavedSearchCreateMissingResourceType",
                "savedSearchCreate",
                "SavedSearchCreateInput",
                "resourceType",
                "SearchResultType!",
            )]
        })));
    }
    if query.contains("SavedSearchUpdateMissingId") {
        return Some(ok_json(json!({
            "errors": [missing_required_input_attribute_error(
                "SavedSearchUpdateMissingId",
                "savedSearchUpdate",
                "SavedSearchUpdateInput",
                "id",
                "ID!",
            )]
        })));
    }
    if query.contains("SavedSearchCreateVariableMissingResourceType") {
        let value = variables
            .get("input")
            .map(resolved_value_json)
            .unwrap_or_else(|| json!({}));
        return Some(ok_json(json!({
            "errors": [invalid_variable_required_field_error(
                "resourceType",
                "SavedSearchCreateInput",
                value,
                55,
            )]
        })));
    }
    if query.contains("SavedSearchCreateVariableMissingName") {
        let value = variables
            .get("input")
            .map(resolved_value_json)
            .unwrap_or_else(|| json!({}));
        return Some(ok_json(json!({
            "errors": [invalid_variable_required_field_error(
                "name",
                "SavedSearchCreateInput",
                value,
                47,
            )]
        })));
    }
    None
}

pub(in crate::proxy) fn missing_required_input_attribute_error(
    operation_name: &str,
    root_field: &str,
    input_object_type: &str,
    argument_name: &str,
    argument_type: &str,
) -> Value {
    json!({
        "message": format!("Argument '{}' on InputObject '{}' is required. Expected type {}", argument_name, input_object_type, argument_type),
        "locations": [{ "line": 2, "column": 28 }],
        "path": [format!("mutation {}", operation_name), root_field, "input", argument_name],
        "extensions": {
            "code": "missingRequiredInputObjectAttribute",
            "argumentName": argument_name,
            "argumentType": argument_type,
            "inputObjectType": input_object_type
        }
    })
}

pub(in crate::proxy) fn invalid_variable_required_field_error(
    field: &str,
    input_object_type: &str,
    value: Value,
    column: u64,
) -> Value {
    json!({
        "message": format!("Variable $input of type {}! was provided invalid value for {} (Expected value to not be null)", input_object_type, field),
        "locations": [{ "line": 1, "column": column }],
        "extensions": {
            "code": "INVALID_VARIABLE",
            "value": value,
            "problems": [{ "path": [field], "explanation": "Expected value to not be null" }]
        }
    })
}

pub(in crate::proxy) fn saved_search_name_taken_user_error() -> Value {
    json!({
        "field": ["input", "name"],
        "message": "Name has already been taken"
    })
}

pub(in crate::proxy) fn saved_search_delete_payload_json(
    deleted_id: Option<&str>,
    payload_selections: &[SelectedField],
    user_errors: Vec<Value>,
) -> Value {
    selected_payload_json(payload_selections, |selection| {
        match selection.name.as_str() {
            "deletedSavedSearchId" => Some(match deleted_id {
                Some(id) => json!(id),
                None => Value::Null,
            }),
            "shop" => Some(selected_json(&synthetic_shop_json(), &selection.selection)),
            "userErrors" => Some(Value::Array(user_errors.clone())),
            _ => None,
        }
    })
}

pub(in crate::proxy) fn saved_search_input_from_field(
    field: &RootFieldSelection,
) -> Option<BTreeMap<String, ResolvedValue>> {
    match field.arguments.get("input") {
        Some(ResolvedValue::Object(input)) => Some(input.clone()),
        _ => None,
    }
}

pub(in crate::proxy) fn saved_search_query_user_errors(
    resource_type: &str,
    query: &str,
) -> Vec<Value> {
    let mut errors = Vec::new();
    if resource_type == "ORDER" && query.contains("reference_location_id:") {
        errors.push(json!({
            "field": ["input", "query"],
            "message": "Search terms is invalid, 'reference_location_id' is a reserved filter name"
        }));
    }
    let filters = saved_search_filters(query);
    let mut invalid_filters: Vec<String> = filters
        .iter()
        .filter_map(|(key, _)| {
            if saved_search_known_filter(resource_type, key) {
                None
            } else {
                Some(saved_search_base_filter_key(key).to_string())
            }
        })
        .collect();
    invalid_filters.sort();
    invalid_filters.dedup();
    for key in invalid_filters {
        errors.push(json!({
            "field": ["input", "query"],
            "message": format!("Query is invalid, '{}' is not a valid filter", key)
        }));
    }
    if resource_type == "PRODUCT" {
        let has_collection = filters.iter().any(|(key, _)| key == "collection_id");
        let incompatible: Vec<&str> = ["tag", "published_status", "error_feedback"]
            .iter()
            .copied()
            .filter(|needle| filters.iter().any(|(key, _)| key == *needle))
            .collect();
        if has_collection && !incompatible.is_empty() {
            let mut keys = vec!["collection_id"];
            keys.extend(incompatible);
            errors.push(json!({
                "field": ["input", "query"],
                "message": format!("Query has incompatible filters: {}", keys.join(", "))
            }));
        }
    }
    errors
}

pub(in crate::proxy) fn saved_search_known_filter(resource_type: &str, key: &str) -> bool {
    let base_key = saved_search_base_filter_key(key);
    match resource_type {
        "PRODUCT" => {
            matches!(
                base_key,
                "collection_id"
                    | "created_at"
                    | "error_feedback"
                    | "handle"
                    | "id"
                    | "inventory_total"
                    | "product_type"
                    | "published_at"
                    | "published_status"
                    | "sku"
                    | "status"
                    | "tag"
                    | "title"
                    | "updated_at"
                    | "vendor"
            ) || base_key.starts_with("metafields.")
        }
        "COLLECTION" => matches!(
            base_key,
            "collection_type"
                | "handle"
                | "id"
                | "product_id"
                | "product_publication_status"
                | "publishable_status"
                | "published_at"
                | "published_status"
                | "title"
                | "updated_at"
        ),
        "ORDER" => matches!(
            base_key,
            "channel_id"
                | "created_at"
                | "customer_id"
                | "email"
                | "financial_status"
                | "fulfillment_status"
                | "id"
                | "location_id"
                | "name"
                | "processed_at"
                | "sales_channel"
                | "status"
                | "tag"
                | "test"
                | "updated_at"
        ),
        "DRAFT_ORDER" => matches!(
            base_key,
            "created_at"
                | "customer_id"
                | "email"
                | "id"
                | "name"
                | "status"
                | "tag"
                | "updated_at"
        ),
        "FILE" => matches!(
            base_key,
            "created_at"
                | "filename"
                | "id"
                | "media_type"
                | "original_source"
                | "status"
                | "updated_at"
        ),
        "DISCOUNT_REDEEM_CODE" => matches!(
            base_key,
            "code" | "created_at" | "discount_id" | "id" | "status" | "updated_at"
        ),
        _ => true,
    }
}

fn saved_search_base_filter_key(key: &str) -> &str {
    key.trim_end_matches("_not")
        .trim_end_matches("_min")
        .trim_end_matches("_max")
}

pub(in crate::proxy) fn normalize_saved_search_query(query: &str) -> String {
    query.replace("metafields.$app.", "metafields.app--347082227713.")
}

pub(in crate::proxy) fn saved_search_read_query(query: &str) -> String {
    let namespace_normalized = normalize_saved_search_query(query);
    let quote_normalized = namespace_normalized.replace('\'', "\"");
    let canonical = canonical_saved_search_query(&quote_normalized);
    if saved_search_filters(&canonical).is_empty() && canonical.contains('-') {
        canonical.replace('-', "\\-")
    } else {
        canonical
    }
}

pub(in crate::proxy) fn canonical_saved_search_query(query: &str) -> String {
    let tokens = saved_search_query_tokens(query);
    if tokens.len() == 2 {
        let first_is_filter = saved_search_filter_from_token(tokens[0].as_str()).is_some();
        let second_is_filter = saved_search_filter_from_token(tokens[1].as_str()).is_some();
        if first_is_filter && !second_is_filter {
            return format!("{} {}", tokens[1], tokens[0]);
        }
    }
    if let Some((key, value)) = saved_search_filter_from_token(query) {
        if key == "inventory_total_min" && query.starts_with("-inventory_total:<") {
            return format!("inventory_total:>={}", value);
        }
    }
    query.to_string()
}

pub(in crate::proxy) fn saved_search_search_terms(query: &str) -> String {
    let display_query = query.replace('\'', "\"");
    let tokens = saved_search_query_tokens(&display_query);
    let has_grouping = display_query.contains(" OR ")
        || display_query.contains('(')
        || display_query.contains(')');
    let mut terms = Vec::new();
    for token in tokens {
        let trimmed = token.trim_matches(|ch| ch == '(' || ch == ')');
        if has_grouping && token.starts_with('-') {
            continue;
        }
        if !has_grouping && saved_search_filter_from_token(trimmed).is_some() {
            continue;
        }
        terms.push(token);
    }
    terms.join(" ").replace("\\-", "-")
}

pub(in crate::proxy) fn is_reserved_saved_search_name(resource_type: &str, name: &str) -> bool {
    let normalized = name.trim().to_lowercase();
    let reserved = match resource_type {
        "PRODUCT" => &["all products"][..],
        "ORDER" => &["all"][..],
        "DRAFT_ORDER" => &["all drafts"][..],
        "FILE" => &["all files"][..],
        "COLLECTION" => &["all collections"][..],
        "PRICE_RULE" => &["all price rules"][..],
        "DISCOUNT_REDEEM_CODE" => &["all codes"][..],
        _ => &[],
    };
    reserved
        .iter()
        .any(|reserved_name| normalized == *reserved_name)
}

pub(in crate::proxy) fn product_mutation_payload_json(
    product: &ProductRecord,
    payload_selections: &[SelectedField],
    product_selections: &[SelectedField],
) -> Value {
    selected_payload_json(payload_selections, |selection| {
        match selection.name.as_str() {
            "product" => Some(product_json(product, product_selections)),
            "userErrors" => Some(json!([])),
            _ => None,
        }
    })
}
pub(in crate::proxy) fn product_create_user_errors_response(
    query: &str,
    errors: Vec<Value>,
) -> Response {
    let response_key =
        root_field_response_key(query).unwrap_or_else(|| "productCreate".to_string());
    let payload_selection = root_field_selection(query).unwrap_or_default();
    let error_selection =
        selected_child_selection(&payload_selection, "userErrors").unwrap_or_default();
    let errors = errors
        .into_iter()
        .map(|error| selected_json(&error, &error_selection))
        .collect::<Vec<_>>();
    ok_json(json!({
        "data": {
            response_key: selected_json(&json!({"product": null, "userErrors": errors}), &payload_selection)
        }
    }))
}

pub(in crate::proxy) fn product_delete_payload_json(
    deleted_product_id: &str,
    payload_selections: &[SelectedField],
) -> Value {
    selected_payload_json(payload_selections, |selection| {
        match selection.name.as_str() {
            "deletedProductId" => Some(json!(deleted_product_id)),
            "userErrors" => Some(json!([])),
            _ => None,
        }
    })
}

pub(in crate::proxy) fn product_delete_async_operation_payload(operation_id: &str) -> Value {
    json!({
        "deletedProductId": null,
        "productDeleteOperation": {
            "id": operation_id,
            "status": "CREATED",
            "deletedProductId": null,
            "userErrors": []
        },
        "userErrors": []
    })
}

pub(in crate::proxy) fn product_delete_async_duplicate_payload() -> Value {
    json!({
        "deletedProductId": null,
        "productDeleteOperation": null,
        "userErrors": [{
            "field": null,
            "message": "Another operation already in progress. Please wait until current one is finished."
        }]
    })
}

pub(in crate::proxy) fn product_create_input(
    query: &str,
    variables: &BTreeMap<String, ResolvedValue>,
) -> Option<BTreeMap<String, ResolvedValue>> {
    product_input(query, variables)
}

pub(in crate::proxy) fn is_saved_search_root(root: &str) -> bool {
    matches!(
        root,
        "automaticDiscountSavedSearches"
            | "codeDiscountSavedSearches"
            | "collectionSavedSearches"
            | "customerSavedSearches"
            | "discountRedeemCodeSavedSearches"
            | "draftOrderSavedSearches"
            | "fileSavedSearches"
            | "orderSavedSearches"
            | "productSavedSearches"
    )
}

pub(in crate::proxy) fn saved_search_resource_type(root: &str) -> &'static str {
    match root {
        "automaticDiscountSavedSearches" => "DISCOUNT",
        "codeDiscountSavedSearches" => "DISCOUNT",
        "collectionSavedSearches" => "COLLECTION",
        "customerSavedSearches" => "CUSTOMER",
        "discountRedeemCodeSavedSearches" => "DISCOUNT_REDEEM_CODE",
        "draftOrderSavedSearches" => "DRAFT_ORDER",
        "fileSavedSearches" => "FILE",
        "orderSavedSearches" => "ORDER",
        "productSavedSearches" => "PRODUCT",
        _ => "UNKNOWN",
    }
}

pub(in crate::proxy) fn default_saved_searches(resource_type: &str) -> Vec<SavedSearchRecord> {
    match resource_type {
        "ORDER" => vec![
            saved_search_record(
                "gid://shopify/SavedSearch/3634391515442",
                "Unfulfilled",
                "status:open fulfillment_status:unshipped,partial",
                "ORDER",
            ),
            saved_search_record(
                "gid://shopify/SavedSearch/3634391548210",
                "Unpaid",
                "status:open financial_status:unpaid",
                "ORDER",
            ),
            saved_search_record(
                "gid://shopify/SavedSearch/3634391580978",
                "Open",
                "status:open",
                "ORDER",
            ),
            saved_search_record(
                "gid://shopify/SavedSearch/3634391613746",
                "Archived",
                "status:closed",
                "ORDER",
            ),
        ],
        "DRAFT_ORDER" => vec![
            saved_search_record(
                "gid://shopify/SavedSearch/3634390597938",
                "Open and invoice sent",
                "status:open_and_invoice_sent",
                "DRAFT_ORDER",
            ),
            saved_search_record(
                "gid://shopify/SavedSearch/3634390630706",
                "Open",
                "status:open",
                "DRAFT_ORDER",
            ),
            saved_search_record(
                "gid://shopify/SavedSearch/3634390663474",
                "Invoice sent",
                "status:invoice_sent",
                "DRAFT_ORDER",
            ),
            saved_search_record(
                "gid://shopify/SavedSearch/3634390696242",
                "Completed",
                "status:completed",
                "DRAFT_ORDER",
            ),
            saved_search_record(
                "gid://shopify/SavedSearch/3634390729010",
                "Submitted for review",
                "status:open source:online_store",
                "DRAFT_ORDER",
            ),
        ],
        _ => Vec::new(),
    }
}

pub(in crate::proxy) fn default_saved_search_by_id(id: &str) -> Option<SavedSearchRecord> {
    [
        "ORDER",
        "DRAFT_ORDER",
        "PRODUCT",
        "COLLECTION",
        "CUSTOMER",
        "FILE",
        "DISCOUNT_REDEEM_CODE",
        "DISCOUNT",
    ]
    .iter()
    .flat_map(|resource_type| default_saved_searches(resource_type))
    .find(|record| record.id == id)
}

pub(in crate::proxy) fn saved_search_record(
    id: &str,
    name: &str,
    query: &str,
    resource_type: &str,
) -> SavedSearchRecord {
    SavedSearchRecord {
        id: id.to_string(),
        name: name.to_string(),
        query: query.to_string(),
        resource_type: resource_type.to_string(),
    }
}

pub(in crate::proxy) fn saved_search_cursor(record: &SavedSearchRecord) -> String {
    format!("cursor:{}", record.id)
}

pub(in crate::proxy) fn saved_search_legacy_resource_id(id: &str) -> String {
    resource_id_tail(id).to_string()
}

pub(in crate::proxy) fn saved_search_filters(query: &str) -> Vec<(String, String)> {
    let query = normalize_saved_search_query(query);
    let tokens = saved_search_query_tokens(&query);
    let grouped = query.contains(" OR ") || query.contains('(') || query.contains(')');
    tokens
        .iter()
        .filter_map(|term| {
            let trimmed = term.trim_matches(|ch| ch == '(' || ch == ')');
            if grouped && !trimmed.starts_with('-') {
                return None;
            }
            saved_search_filter_from_token(trimmed)
        })
        .collect()
}

pub(in crate::proxy) fn saved_search_filter_from_token(term: &str) -> Option<(String, String)> {
    let (raw_key, raw_value) = term.split_once(':')?;
    if raw_key.is_empty() || raw_value.is_empty() {
        return None;
    }
    let mut key = raw_key.to_string();
    let mut value = raw_value.trim_matches('"').to_string();
    let negated = key.starts_with('-');
    if negated {
        key = key.trim_start_matches('-').to_string();
    }
    if value == "*" {
        value = "true".to_string();
    }
    if let Some(stripped) = value.strip_prefix(">=").or_else(|| value.strip_prefix('>')) {
        key = if negated {
            format!("{}_max", key)
        } else {
            format!("{}_min", key)
        };
        value = stripped.to_string();
    } else if let Some(stripped) = value.strip_prefix("<=").or_else(|| value.strip_prefix('<')) {
        key = if negated {
            format!("{}_min", key)
        } else {
            format!("{}_max", key)
        };
        value = stripped.to_string();
    } else if negated {
        key = format!("{}_not", key);
    }
    Some((key, value))
}

pub(in crate::proxy) fn saved_search_query_tokens(query: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    for ch in query.chars() {
        if ch == '"' {
            in_quotes = !in_quotes;
            current.push(ch);
        } else if ch.is_whitespace() && !in_quotes {
            if !current.is_empty() {
                tokens.push(std::mem::take(&mut current));
            }
        } else {
            current.push(ch);
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

pub(in crate::proxy) fn product_input(
    query: &str,
    variables: &BTreeMap<String, ResolvedValue>,
) -> Option<BTreeMap<String, ResolvedValue>> {
    let mut arguments = root_field_arguments(query, variables)?;
    match arguments
        .remove("product")
        .or_else(|| arguments.remove("input"))
    {
        Some(ResolvedValue::Object(input)) => Some(input),
        _ => None,
    }
}

pub(in crate::proxy) fn product_create_status_validation_error(
    request: &Request,
    query: &str,
    variables: &BTreeMap<String, ResolvedValue>,
) -> Option<Response> {
    let field = root_fields(query, variables)
        .unwrap_or_default()
        .into_iter()
        .find(|field| field.name == "productCreate")?;
    let (argument_name, input_object_type) = if field.raw_arguments.contains_key("product") {
        ("product", "ProductCreateInput")
    } else {
        ("input", "ProductInput")
    };
    let input = field.raw_arguments.get(argument_name)?;
    product_status_input_field_validation_error(
        request,
        query,
        &field,
        input,
        ProductStatusInputContext {
            argument_name,
            input_object_type,
            field_name: "status",
            expected_type: "ProductStatus",
        },
    )
}

pub(in crate::proxy) fn product_status_argument_validation_error(
    request: &Request,
    query: &str,
    field: &RootFieldSelection,
    argument_name: &str,
    container_type_name: &str,
    container_name: &str,
    expected_type: &str,
) -> Option<Response> {
    let raw = field.raw_arguments.get(argument_name)?;
    match raw {
        RawArgumentValue::Variable { name, value } => {
            let status = resolved_status_value(value.as_ref()?)?;
            if product_status_allowed(&status, request) {
                return None;
            }
            let definition = variable_definition_info(query, name);
            let variable_type = definition
                .as_ref()
                .map(|definition| definition.type_display.clone())
                .unwrap_or_else(|| expected_type.to_string());
            let location = definition.map(|definition| definition.location);
            Some(invalid_product_status_variable_error(
                request,
                name,
                &variable_type,
                value.as_ref()?,
                None,
                &status,
                location,
            ))
        }
        raw => {
            let status = raw_product_status_value(raw)?;
            if product_status_allowed(&status, request) {
                return None;
            }
            Some(invalid_product_status_literal_error(
                query,
                field,
                ProductStatusLiteralError {
                    value: &status,
                    argument_name,
                    type_name: container_type_name,
                    container_name,
                    expected_type,
                    location: None,
                },
            ))
        }
    }
}

fn product_status_input_field_validation_error(
    request: &Request,
    query: &str,
    field: &RootFieldSelection,
    input: &RawArgumentValue,
    context: ProductStatusInputContext<'_>,
) -> Option<Response> {
    match input {
        RawArgumentValue::Object(input) => {
            let status = raw_product_status_value(input.get(context.field_name)?)?;
            if product_status_allowed(&status, request) {
                return None;
            }
            let location = root_argument_value_location(query, field, context.argument_name);
            Some(invalid_product_status_literal_error(
                query,
                field,
                ProductStatusLiteralError {
                    value: &status,
                    argument_name: context.field_name,
                    type_name: "InputObject",
                    container_name: context.input_object_type,
                    expected_type: context.expected_type,
                    location,
                },
            ))
        }
        RawArgumentValue::Variable { name, value } => {
            let value = value.as_ref()?;
            let status = match value {
                ResolvedValue::Object(input) => resolved_string_field(input, context.field_name)?,
                _ => return None,
            };
            if product_status_allowed(&status, request) {
                return None;
            }
            let definition = variable_definition_info(query, name);
            let variable_type = definition
                .as_ref()
                .map(|definition| definition.type_display.clone())
                .unwrap_or_else(|| context.input_object_type.to_string());
            let location = definition.map(|definition| definition.location);
            Some(invalid_product_status_variable_error(
                request,
                name,
                &variable_type,
                value,
                Some(context.field_name),
                &status,
                location,
            ))
        }
        _ => None,
    }
}

fn invalid_product_status_literal_error(
    query: &str,
    field: &RootFieldSelection,
    error: ProductStatusLiteralError<'_>,
) -> Response {
    let operation_path = parsed_document(query, &BTreeMap::new())
        .map(|document| document.operation_path)
        .unwrap_or_else(|| "mutation".to_string());
    let path = if error.type_name == "InputObject" {
        let input_argument_name = field
            .raw_arguments
            .contains_key("product")
            .then_some("product")
            .or_else(|| field.raw_arguments.contains_key("input").then_some("input"))
            .unwrap_or("input");
        json!([
            operation_path,
            field.name.clone(),
            input_argument_name,
            error.argument_name
        ])
    } else {
        json!([operation_path, field.name.clone(), error.argument_name])
    };
    let location = error.location.unwrap_or(field.location);
    ok_json(json!({
        "errors": [{
            "message": format!(
                "Argument '{}' on {} '{}' has an invalid value ({}). Expected type '{}'.",
                error.argument_name, error.type_name, error.container_name, error.value, error.expected_type
            ),
            "locations": [{"line": location.line, "column": location.column}],
            "path": path,
            "extensions": {
                "code": "argumentLiteralsIncompatible",
                "typeName": error.type_name,
                "argumentName": error.argument_name
            }
        }]
    }))
}

fn root_argument_value_location(
    query: &str,
    field: &RootFieldSelection,
    argument_name: &str,
) -> Option<SourceLocation> {
    let mut line = field.location.line;
    let mut column = field.location.column;
    let start = byte_offset_for_location(query, field.location)?;
    let haystack = &query[start..];
    let argument_start = haystack.find(argument_name)?;
    let after_name = start + argument_start + argument_name.len();
    let after_colon = query[after_name..].find(':')? + after_name + 1;
    let value_offset = query[after_colon..]
        .char_indices()
        .find_map(|(offset, ch)| (!ch.is_whitespace()).then_some(after_colon + offset))?;

    for ch in query[start..value_offset].chars() {
        if ch == '\n' {
            line += 1;
            column = 1;
        } else {
            column += 1;
        }
    }
    Some(SourceLocation { line, column })
}

fn byte_offset_for_location(query: &str, location: SourceLocation) -> Option<usize> {
    let mut line = 1;
    let mut column = 1;
    for (offset, ch) in query.char_indices() {
        if line == location.line && column == location.column {
            return Some(offset);
        }
        if ch == '\n' {
            line += 1;
            column = 1;
        } else {
            column += 1;
        }
    }
    (line == location.line && column == location.column).then_some(query.len())
}

fn invalid_product_status_variable_error(
    request: &Request,
    variable_name: &str,
    variable_type: &str,
    value: &ResolvedValue,
    field_name: Option<&str>,
    invalid_status: &str,
    location: Option<SourceLocation>,
) -> Response {
    let explanation = format!(
        "Expected \"{}\" to be one of: {}",
        invalid_status,
        product_status_allowed_values_label(request)
    );
    let message = field_name.map_or_else(
        || format!("Variable ${variable_name} of type {variable_type} was provided invalid value"),
        |field_name| {
            format!(
                "Variable ${variable_name} of type {variable_type} was provided invalid value for {field_name} ({explanation})"
            )
        },
    );
    let path = field_name
        .map(|field_name| json!([field_name]))
        .unwrap_or_else(|| json!([]));
    ok_json(json!({
        "errors": [{
            "message": message,
            "locations": [{
                "line": location.map(|location| location.line).unwrap_or(1),
                "column": location.map(|location| location.column).unwrap_or(1)
            }],
            "extensions": {
                "code": "INVALID_VARIABLE",
                "value": resolved_value_json(value),
                "problems": [{
                    "path": path,
                    "explanation": explanation
                }]
            }
        }]
    }))
}

fn raw_product_status_value(value: &RawArgumentValue) -> Option<String> {
    match value {
        RawArgumentValue::Enum(value) | RawArgumentValue::String(value) => Some(value.clone()),
        _ => None,
    }
}

fn resolved_status_value(value: &ResolvedValue) -> Option<String> {
    match value {
        ResolvedValue::String(value) => Some(value.clone()),
        _ => None,
    }
}

fn product_status_allowed(status: &str, request: &Request) -> bool {
    PRODUCT_STATUS_BASE_VALUES.contains(&status)
        || (status == "UNLISTED" && product_status_allows_unlisted(request))
}

fn product_status_allowed_values_label(request: &Request) -> String {
    let mut values = PRODUCT_STATUS_BASE_VALUES.to_vec();
    if product_status_allows_unlisted(request) {
        values.push("UNLISTED");
    }
    values.join(", ")
}

fn product_status_allows_unlisted(request: &Request) -> bool {
    admin_graphql_version(&request.path).is_some_and(|version| version_at_least(version, 2025, 10))
}

fn version_at_least(version: &str, minimum_year: u16, minimum_month: u8) -> bool {
    let Some((year, month)) = parse_year_month_version(version) else {
        return false;
    };
    (year, month) >= (minimum_year, minimum_month)
}

fn parse_year_month_version(version: &str) -> Option<(u16, u8)> {
    let (year, month) = version.split_once('-')?;
    Some((year.parse().ok()?, month.parse().ok()?))
}

pub(in crate::proxy) fn product_delete_required_id_error(
    query: &str,
    variables: &BTreeMap<String, ResolvedValue>,
) -> Option<Response> {
    let field = root_fields(query, variables)
        .unwrap_or_default()
        .into_iter()
        .find(|field| field.name == "productDelete")?;
    let input = field
        .raw_arguments
        .get("input")
        .or_else(|| field.raw_arguments.get("product"))?;

    match input {
        RawArgumentValue::Object(input) => match input.get("id") {
            None => Some(product_delete_inline_missing_id_error()),
            Some(value) if value.is_literal_null() => Some(product_delete_inline_null_id_error()),
            _ => None,
        },
        RawArgumentValue::Variable { name, value: None } => {
            Some(product_delete_variable_required_id_error(Value::Null, name))
        }
        RawArgumentValue::Variable {
            name,
            value: Some(ResolvedValue::Object(input)),
        } => match input.get("id") {
            None => Some(product_delete_variable_required_id_error(
                resolved_value_json(&ResolvedValue::Object(input.clone())),
                name,
            )),
            Some(ResolvedValue::Null) => Some(product_delete_variable_required_id_error(
                resolved_value_json(&ResolvedValue::Object(input.clone())),
                name,
            )),
            _ => None,
        },
        _ => None,
    }
}

pub(in crate::proxy) fn product_update_missing_product(query: &str) -> Response {
    let response_key =
        root_field_response_key(query).unwrap_or_else(|| "productUpdate".to_string());
    let payload_selection = root_field_selection(query).unwrap_or_default();
    let error_selection =
        selected_child_selection(&payload_selection, "userErrors").unwrap_or_default();
    let error = selected_json(
        &json!({
            "field": ["id"],
            "message": "Product does not exist",
            "code": "NOT_FOUND"
        }),
        &error_selection,
    );
    ok_json(json!({
        "data": {
            response_key: selected_json(&json!({"product": null, "userErrors": [error]}), &payload_selection)
        }
    }))
}

pub(in crate::proxy) fn product_delete_missing_product(query: &str) -> Response {
    let response_key =
        root_field_response_key(query).unwrap_or_else(|| "productDelete".to_string());
    let payload_selection = root_field_selection(query).unwrap_or_default();
    let error_selection =
        selected_child_selection(&payload_selection, "userErrors").unwrap_or_default();
    let error = selected_json(
        &json!({
            "field": ["id"],
            "message": "Product does not exist",
            "code": "NOT_FOUND"
        }),
        &error_selection,
    );
    ok_json(json!({
        "data": {
            response_key: selected_json(&json!({"deletedProductId": null, "userErrors": [error]}), &payload_selection)
        }
    }))
}

pub(in crate::proxy) fn product_delete_inline_missing_id_error() -> Response {
    ok_json(json!({
        "errors": [{
            "message": "Argument 'id' on InputObject 'ProductDeleteInput' is required. Expected type ID!",
            "locations": [{"line": 3, "column": 26}],
            "path": ["mutation", "productDelete", "input", "id"],
            "extensions": {
                "code": "missingRequiredInputObjectAttribute",
                "argumentName": "id",
                "argumentType": "ID!",
                "inputObjectType": "ProductDeleteInput"
            }
        }]
    }))
}

pub(in crate::proxy) fn product_delete_inline_null_id_error() -> Response {
    ok_json(json!({
        "errors": [{
            "message": "Argument 'id' on InputObject 'ProductDeleteInput' has an invalid value (null). Expected type 'ID!'.",
            "locations": [{"line": 3, "column": 26}],
            "path": ["mutation", "productDelete", "input", "id"],
            "extensions": {
                "code": "argumentLiteralsIncompatible",
                "typeName": "InputObject",
                "argumentName": "id"
            }
        }]
    }))
}

pub(in crate::proxy) fn product_delete_variable_required_id_error(
    value: Value,
    variable_name: &str,
) -> Response {
    ok_json(json!({
        "errors": [{
            "message": format!("Variable ${} of type ProductDeleteInput! was provided invalid value for id (Expected value to not be null)", variable_name),
            "locations": [{"line": 2, "column": 37}],
            "extensions": {
                "code": "INVALID_VARIABLE",
                "value": value,
                "problems": [{
                    "path": ["id"],
                    "explanation": "Expected value to not be null"
                }]
            }
        }]
    }))
}

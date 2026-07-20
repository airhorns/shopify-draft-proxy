use super::*;
use crate::graphql::ParsedDocument;
use crate::graphql::RawArgumentValue;

mod collections;
mod product_tail;
mod saved_search;
mod search;

pub(in crate::proxy) use self::collections::*;
pub(in crate::proxy) use self::saved_search::*;
pub(in crate::proxy) use self::search::*;

impl DraftProxy {
    /// Resolve the effective handle for every product lifecycle mutation.
    ///
    /// Shopify treats nonblank merchant handles differently from generated handles:
    /// normalized merchant collisions are rejected, while generated handles and the
    /// punctuation-only `product` fallback reserve the next numeric suffix. An omitted
    /// handle remains sticky when mutating an existing product, while a supplied blank
    /// handle regenerates from the effective title.
    pub(in crate::proxy) fn resolve_product_handle(
        &self,
        title: &str,
        explicit_handle: Option<&str>,
        existing: Option<&ProductRecord>,
    ) -> Result<String, Value> {
        let (candidate, reject_collision) = match explicit_handle {
            Some(handle) => {
                let normalized = normalize_product_handle(handle);
                if normalized.is_empty() {
                    if handle.trim().is_empty() {
                        let generated = normalize_product_handle(title);
                        (
                            if generated.is_empty() {
                                "untitled-product".to_string()
                            } else {
                                generated
                            },
                            false,
                        )
                    } else {
                        ("product".to_string(), false)
                    }
                } else {
                    (normalized, true)
                }
            }
            None => {
                if let Some(existing) = existing {
                    return Ok(existing.handle.clone());
                }
                let generated = normalize_product_handle(title);
                (
                    if generated.is_empty() {
                        "untitled-product".to_string()
                    } else {
                        generated
                    },
                    false,
                )
            }
        };

        let excluded_id = existing.map(|product| product.id.as_str());
        let occupied = self
            .store
            .products()
            .into_iter()
            .filter(|product| Some(product.id.as_str()) != excluded_id)
            .map(|product| product.handle)
            .collect::<BTreeSet<_>>();
        if !occupied.contains(&candidate) {
            return Ok(candidate);
        }
        if reject_collision {
            return Err(product_handle_collision_user_error(
                explicit_handle.expect("collision-rejecting handles are explicit"),
            ));
        }

        Ok(next_available_product_handle(&candidate, &occupied))
    }
}

fn normalize_product_handle(value: &str) -> String {
    let mut handle = String::new();
    let mut pending_separator = false;
    for character in value.chars().flat_map(char::to_lowercase) {
        if character.is_alphanumeric() {
            if pending_separator && !handle.is_empty() {
                handle.push('-');
            }
            handle.push(character);
            pending_separator = false;
        } else if !handle.is_empty() {
            pending_separator = true;
        }
    }
    handle
}

fn next_available_product_handle(candidate: &str, occupied: &BTreeSet<String>) -> String {
    let trailing_digit_count = candidate
        .chars()
        .rev()
        .take_while(|character| character.is_ascii_digit())
        .count();
    let trailing_suffix = if trailing_digit_count > 0 {
        let suffix_start = candidate.len() - trailing_digit_count;
        candidate[suffix_start..]
            .parse::<u128>()
            .ok()
            .and_then(|suffix| {
                suffix
                    .checked_add(1)
                    .map(|next_suffix| (&candidate[..suffix_start], next_suffix))
            })
    } else {
        None
    };
    let (prefix, separator, mut suffix) = trailing_suffix
        .map(|(prefix, suffix)| (prefix, "", suffix))
        .unwrap_or((candidate, "-", 1));
    loop {
        let handle = format!("{prefix}{separator}{suffix}");
        if !occupied.contains(&handle) {
            return handle;
        }
        suffix = suffix
            .checked_add(1)
            .expect("product handle suffix space should not be exhausted");
    }
}

fn product_handle_collision_user_error(handle: &str) -> Value {
    user_error_omit_code(
        ["input", "handle"],
        &format!("Handle '{handle}' already in use. Please provide a new handle."),
        None,
    )
}

pub(in crate::proxy) fn product_field_resolver_type_policies() -> Vec<FieldResolverTypePolicy> {
    let mut policies = vec![
        FieldResolverTypePolicy::unsupported_remaining(
            ApiSurface::Admin,
            "Product",
            "field is not yet modeled by the canonical Product resolver",
        ),
        FieldResolverTypePolicy::unsupported_remaining(
            ApiSurface::Admin,
            "ProductVariant",
            "field is not yet modeled by the canonical ProductVariant resolver",
        ),
        FieldResolverTypePolicy::unsupported_remaining(
            ApiSurface::Admin,
            "Collection",
            "field is not yet modeled by the canonical collection resolver",
        ),
        FieldResolverTypePolicy::unsupported_remaining(
            ApiSurface::Admin,
            "Publication",
            "field is not yet modeled by the canonical publication resolver",
        ),
        FieldResolverTypePolicy::unsupported_remaining(
            ApiSurface::Admin,
            "Channel",
            "field is not yet modeled by the canonical publication resolver",
        ),
        FieldResolverTypePolicy::unsupported_remaining(
            ApiSurface::Admin,
            "Metafield",
            "field is not yet modeled by the canonical metafield resolver",
        ),
        FieldResolverTypePolicy::unsupported_remaining(
            ApiSurface::Admin,
            "MetafieldIdentifier",
            "field is not yet modeled by the canonical metafield-delete resolver",
        ),
        FieldResolverTypePolicy::unsupported_remaining(
            ApiSurface::Admin,
            "MetafieldsDeletePayload",
            "field is not yet modeled by the canonical metafield-delete resolver",
        ),
        FieldResolverTypePolicy::unsupported_remaining(
            ApiSurface::Admin,
            "MetafieldsSetPayload",
            "field is not yet modeled by the canonical metafield-set resolver",
        ),
        FieldResolverTypePolicy::unsupported_remaining(
            ApiSurface::Admin,
            "MetafieldsSetUserError",
            "field is not yet modeled by the canonical metafield-set resolver",
        ),
        FieldResolverTypePolicy::unsupported_remaining(
            ApiSurface::Admin,
            "ProductCreatePayload",
            "field is not yet modeled by the canonical product-create resolver",
        ),
        FieldResolverTypePolicy::unsupported_remaining(
            ApiSurface::Admin,
            "ProductUpdatePayload",
            "field is not yet modeled by the canonical product-update resolver",
        ),
        FieldResolverTypePolicy::unsupported_remaining(
            ApiSurface::Admin,
            "ProductDeletePayload",
            "field is not yet modeled by the canonical product-delete resolver",
        ),
        FieldResolverTypePolicy::unsupported_remaining(
            ApiSurface::Admin,
            "ProductDeleteOperation",
            "field is not yet modeled by the canonical product-delete operation resolver",
        ),
        FieldResolverTypePolicy::unsupported_remaining(
            ApiSurface::Admin,
            "ProductChangeStatusPayload",
            "field is not yet modeled by the canonical product-status resolver",
        ),
        FieldResolverTypePolicy::unsupported_remaining(
            ApiSurface::Admin,
            "ProductChangeStatusUserError",
            "field is not yet modeled by the canonical product-status resolver",
        ),
        FieldResolverTypePolicy::unsupported_remaining(
            ApiSurface::Admin,
            "TagsAddPayload",
            "field is not yet modeled by the canonical tag mutation resolver",
        ),
        FieldResolverTypePolicy::unsupported_remaining(
            ApiSurface::Admin,
            "TagsRemovePayload",
            "field is not yet modeled by the canonical tag mutation resolver",
        ),
        FieldResolverTypePolicy::unsupported_remaining(
            ApiSurface::Admin,
            "ProductPublishPayload",
            "field is not yet modeled by the canonical product-publication resolver",
        ),
        FieldResolverTypePolicy::unsupported_remaining(
            ApiSurface::Admin,
            "ProductUnpublishPayload",
            "field is not yet modeled by the canonical product-publication resolver",
        ),
        FieldResolverTypePolicy::unsupported_remaining(
            ApiSurface::Admin,
            "ProductSetPayload",
            "field is not yet modeled by the canonical product-set resolver",
        ),
        FieldResolverTypePolicy::unsupported_remaining(
            ApiSurface::Admin,
            "ProductSetOperation",
            "field is not yet modeled by the canonical product-operation resolver",
        ),
        FieldResolverTypePolicy::unsupported_remaining(
            ApiSurface::Admin,
            "ProductSetUserError",
            "field is not yet modeled by the canonical product-set resolver",
        ),
        FieldResolverTypePolicy::unsupported_remaining(
            ApiSurface::Admin,
            "ProductDuplicatePayload",
            "field is not yet modeled by the canonical product-duplicate resolver",
        ),
        FieldResolverTypePolicy::unsupported_remaining(
            ApiSurface::Admin,
            "ProductDuplicateOperation",
            "field is not yet modeled by the canonical product-operation resolver",
        ),
        FieldResolverTypePolicy::unsupported_remaining(
            ApiSurface::Admin,
            "ProductBundleCreatePayload",
            "field is not yet modeled by the canonical product-bundle resolver",
        ),
        FieldResolverTypePolicy::unsupported_remaining(
            ApiSurface::Admin,
            "ProductBundleUpdatePayload",
            "field is not yet modeled by the canonical product-bundle resolver",
        ),
        FieldResolverTypePolicy::unsupported_remaining(
            ApiSurface::Admin,
            "ProductBundleOperation",
            "field is not yet modeled by the canonical product-operation resolver",
        ),
        FieldResolverTypePolicy::unsupported_remaining(
            ApiSurface::Admin,
            "ProductOptionsCreatePayload",
            "field is not yet modeled by the canonical product-option resolver",
        ),
        FieldResolverTypePolicy::unsupported_remaining(
            ApiSurface::Admin,
            "ProductOptionUpdatePayload",
            "field is not yet modeled by the canonical product-option resolver",
        ),
        FieldResolverTypePolicy::unsupported_remaining(
            ApiSurface::Admin,
            "ProductOptionsDeletePayload",
            "field is not yet modeled by the canonical product-option resolver",
        ),
        FieldResolverTypePolicy::unsupported_remaining(
            ApiSurface::Admin,
            "ProductOptionsReorderPayload",
            "field is not yet modeled by the canonical product-option resolver",
        ),
        FieldResolverTypePolicy::unsupported_remaining(
            ApiSurface::Admin,
            "ProductVariantAppendMediaPayload",
            "field is not yet modeled by the canonical product-variant media resolver",
        ),
        FieldResolverTypePolicy::unsupported_remaining(
            ApiSurface::Admin,
            "ProductVariantDetachMediaPayload",
            "field is not yet modeled by the canonical product-variant media resolver",
        ),
        FieldResolverTypePolicy::unsupported_remaining(
            ApiSurface::Admin,
            "ProductVariantsBulkCreatePayload",
            "field is not yet modeled by the canonical product-variant resolver",
        ),
        FieldResolverTypePolicy::unsupported_remaining(
            ApiSurface::Admin,
            "ProductVariantsBulkUpdatePayload",
            "field is not yet modeled by the canonical product-variant resolver",
        ),
        FieldResolverTypePolicy::unsupported_remaining(
            ApiSurface::Admin,
            "ProductVariantsBulkDeletePayload",
            "field is not yet modeled by the canonical product-variant resolver",
        ),
        FieldResolverTypePolicy::unsupported_remaining(
            ApiSurface::Admin,
            "ProductVariantsBulkReorderPayload",
            "field is not yet modeled by the canonical product-variant resolver",
        ),
        FieldResolverTypePolicy::unsupported_remaining(
            ApiSurface::Admin,
            "ProductFeed",
            "field is not yet modeled by the canonical product-feed resolver",
        ),
        FieldResolverTypePolicy::unsupported_remaining(
            ApiSurface::Admin,
            "ProductFeedCreatePayload",
            "field is not yet modeled by the canonical product-feed resolver",
        ),
        FieldResolverTypePolicy::unsupported_remaining(
            ApiSurface::Admin,
            "ProductFeedDeletePayload",
            "field is not yet modeled by the canonical product-feed resolver",
        ),
        FieldResolverTypePolicy::unsupported_remaining(
            ApiSurface::Admin,
            "ProductFullSyncPayload",
            "field is not yet modeled by the canonical product-feed resolver",
        ),
        FieldResolverTypePolicy::unsupported_remaining(
            ApiSurface::Admin,
            "CollectionCreatePayload",
            "field is not yet modeled by the canonical collection resolver",
        ),
        FieldResolverTypePolicy::unsupported_remaining(
            ApiSurface::Admin,
            "CollectionUpdatePayload",
            "field is not yet modeled by the canonical collection resolver",
        ),
        FieldResolverTypePolicy::unsupported_remaining(
            ApiSurface::Admin,
            "CollectionDeletePayload",
            "field is not yet modeled by the canonical collection resolver",
        ),
        FieldResolverTypePolicy::unsupported_remaining(
            ApiSurface::Admin,
            "CollectionAddProductsPayload",
            "field is not yet modeled by the canonical collection resolver",
        ),
        FieldResolverTypePolicy::unsupported_remaining(
            ApiSurface::Admin,
            "CollectionAddProductsV2Payload",
            "field is not yet modeled by the canonical collection resolver",
        ),
        FieldResolverTypePolicy::unsupported_remaining(
            ApiSurface::Admin,
            "CollectionRemoveProductsPayload",
            "field is not yet modeled by the canonical collection resolver",
        ),
        FieldResolverTypePolicy::unsupported_remaining(
            ApiSurface::Admin,
            "CollectionReorderProductsPayload",
            "field is not yet modeled by the canonical collection resolver",
        ),
    ];
    policies.extend(
        [
            "CombinedListing",
            "ProductOption",
            "ProductOptionValue",
            "ProductTaxonomyNode",
            "Publishable",
            "TaxonomyCategory",
            "TaxonomyValue",
        ]
            .into_iter()
            .map(|parent_type| {
                FieldResolverTypePolicy::property_backed_ordinary_fields(
                    ApiSurface::Admin,
                    parent_type,
                    "argument-bearing product taxonomy or publication field has no explicit canonical resolver",
                )
            }),
    );
    policies
}

pub(in crate::proxy) fn product_field_resolver_registrations() -> Vec<FieldResolverRegistration> {
    let mut registrations = Vec::new();
    for (parent_type, fields) in [
        ("ProductConnection", &["edges", "nodes", "pageInfo"][..]),
        ("ProductEdge", &["cursor", "node"][..]),
        ("MetafieldIdentifier", &["key", "namespace", "ownerId"][..]),
        (
            "MetafieldsDeletePayload",
            &["deletedMetafields", "userErrors"][..],
        ),
        ("MetafieldsSetPayload", &["metafields", "userErrors"][..]),
        (
            "MetafieldsSetUserError",
            &["code", "elementIndex", "field", "message"][..],
        ),
        ("ProductCreatePayload", &["product", "userErrors"][..]),
        ("ProductUpdatePayload", &["product", "userErrors"][..]),
        (
            "ProductDeletePayload",
            &["deletedProductId", "productDeleteOperation", "userErrors"][..],
        ),
        (
            "ProductDeleteOperation",
            &["deletedProductId", "id", "status", "userErrors"][..],
        ),
        ("ProductChangeStatusPayload", &["product", "userErrors"][..]),
        (
            "ProductChangeStatusUserError",
            &["code", "field", "message"][..],
        ),
        ("TagsAddPayload", &["node", "userErrors"][..]),
        ("TagsRemovePayload", &["node", "userErrors"][..]),
        (
            "ProductPublishPayload",
            &["product", "productPublications", "userErrors"][..],
        ),
        ("ProductUnpublishPayload", &["product", "userErrors"][..]),
        (
            "ProductSetPayload",
            &["product", "productSetOperation", "userErrors"][..],
        ),
        (
            "ProductSetOperation",
            &["id", "product", "status", "userErrors"][..],
        ),
        ("ProductSetUserError", &["code", "field", "message"][..]),
        (
            "ProductDuplicatePayload",
            &[
                "imageJob",
                "newProduct",
                "productDuplicateOperation",
                "userErrors",
            ][..],
        ),
        (
            "ProductDuplicateOperation",
            &["id", "newProduct", "product", "status", "userErrors"][..],
        ),
        (
            "ProductBundleCreatePayload",
            &["productBundleOperation", "userErrors"][..],
        ),
        (
            "ProductBundleUpdatePayload",
            &["productBundleOperation", "userErrors"][..],
        ),
        (
            "ProductBundleOperation",
            &["id", "product", "status", "userErrors"][..],
        ),
        (
            "ProductOptionsCreatePayload",
            &["product", "userErrors"][..],
        ),
        ("ProductOptionUpdatePayload", &["product", "userErrors"][..]),
        (
            "ProductOptionsDeletePayload",
            &["deletedOptionsIds", "product", "userErrors"][..],
        ),
        (
            "ProductOptionsReorderPayload",
            &["product", "userErrors"][..],
        ),
        (
            "ProductVariantAppendMediaPayload",
            &["product", "productVariants", "userErrors"][..],
        ),
        (
            "ProductVariantDetachMediaPayload",
            &["product", "productVariants", "userErrors"][..],
        ),
        (
            "ProductVariantsBulkCreatePayload",
            &["product", "productVariants", "userErrors"][..],
        ),
        (
            "ProductVariantsBulkUpdatePayload",
            &["product", "productVariants", "userErrors"][..],
        ),
        (
            "ProductVariantsBulkDeletePayload",
            &["product", "userErrors"][..],
        ),
        (
            "ProductVariantsBulkReorderPayload",
            &["product", "userErrors"][..],
        ),
        ("ProductFeedConnection", &["edges", "nodes", "pageInfo"][..]),
        ("ProductFeedEdge", &["cursor", "node"][..]),
        (
            "ProductFeedCreatePayload",
            &["productFeed", "userErrors"][..],
        ),
        ("ProductFeedDeletePayload", &["deletedId", "userErrors"][..]),
        ("ProductFullSyncPayload", &["id", "userErrors"][..]),
        (
            "ProductFeedCreateUserError",
            &["code", "field", "message"][..],
        ),
        (
            "ProductFeedDeleteUserError",
            &["code", "field", "message"][..],
        ),
        (
            "ProductFullSyncUserError",
            &["code", "field", "message"][..],
        ),
        ("CollectionCreatePayload", &["collection", "userErrors"][..]),
        (
            "CollectionUpdatePayload",
            &["collection", "job", "userErrors"][..],
        ),
        (
            "CollectionDeletePayload",
            &["deletedCollectionId", "userErrors"][..],
        ),
        (
            "CollectionAddProductsPayload",
            &["collection", "userErrors"][..],
        ),
        ("CollectionAddProductsV2Payload", &["job", "userErrors"][..]),
        (
            "CollectionRemoveProductsPayload",
            &["job", "userErrors"][..],
        ),
        (
            "CollectionReorderProductsPayload",
            &["job", "userErrors"][..],
        ),
    ] {
        registrations.extend(fields.iter().map(|field| {
            FieldResolverRegistration::property(ApiSurface::Admin, parent_type, field)
        }));
    }
    registrations.push(FieldResolverRegistration::explicit(
        ApiSurface::Admin,
        "ProductCreatePayload",
        "shop",
        mutation_payload_shop_field,
    ));
    registrations.push(FieldResolverRegistration::explicit(
        ApiSurface::Admin,
        "ProductDeletePayload",
        "shop",
        mutation_payload_shop_field,
    ));
    for parent_type in ["ProductPublishPayload", "ProductUnpublishPayload"] {
        registrations.push(FieldResolverRegistration::explicit(
            ApiSurface::Admin,
            parent_type,
            "shop",
            mutation_payload_shop_field,
        ));
    }
    registrations.push(FieldResolverRegistration::explicit(
        ApiSurface::Admin,
        "ProductDuplicatePayload",
        "shop",
        mutation_payload_shop_field,
    ));
    registrations.push(FieldResolverRegistration::explicit(
        ApiSurface::Admin,
        "CollectionDeletePayload",
        "shop",
        mutation_payload_shop_field,
    ));
    registrations.push(FieldResolverRegistration::explicit(
        ApiSurface::Admin,
        "CombinedListing",
        "combinedListingChildren",
        combined_listing_children_field,
    ));
    for (field, handler) in [
        (
            "products",
            collection_products_field as crate::resolver_registry::FieldResolverHandler,
        ),
        ("hasProduct", collection_has_product_field),
        ("productsCount", collection_products_count_field),
    ] {
        registrations.push(FieldResolverRegistration::explicit(
            ApiSurface::Admin,
            "Collection",
            field,
            handler,
        ));
    }
    registrations.extend([
        FieldResolverRegistration::explicit(
            ApiSurface::Admin,
            "Collection",
            "metafield",
            product_metafield_field,
        ),
        FieldResolverRegistration::explicit(
            ApiSurface::Admin,
            "Collection",
            "metafields",
            product_metafields_field,
        ),
    ]);
    for field in [
        "description",
        "descriptionHtml",
        "handle",
        "id",
        "image",
        "legacyResourceId",
        "ruleSet",
        "seo",
        "sortOrder",
        "storefrontId",
        "templateSuffix",
        "title",
        "updatedAt",
    ] {
        registrations.push(FieldResolverRegistration::property(
            ApiSurface::Admin,
            "Collection",
            field,
        ));
    }
    for field in [
        "autoPublish",
        "catalog",
        "id",
        "name",
        "supportsFuturePublishing",
    ] {
        registrations.push(FieldResolverRegistration::property(
            ApiSurface::Admin,
            "Publication",
            field,
        ));
    }
    for (field, handler) in [
        (
            "channel",
            publication_channel_field as crate::resolver_registry::FieldResolverHandler,
        ),
        ("channels", publication_channels_field),
        ("products", publication_products_field),
        ("includedProducts", publication_products_field),
        ("includedProductsCount", publication_product_count_field),
        ("publishedProductsCount", publication_product_count_field),
        ("collectionsCount", publication_collections_count_field),
    ] {
        registrations.push(FieldResolverRegistration::explicit(
            ApiSurface::Admin,
            "Publication",
            field,
            handler,
        ));
    }
    for field in ["id", "name", "publication"] {
        registrations.push(FieldResolverRegistration::property(
            ApiSurface::Admin,
            "Channel",
            field,
        ));
    }
    registrations.push(FieldResolverRegistration::explicit(
        ApiSurface::Admin,
        "Channel",
        "productsCount",
        channel_products_count_field,
    ));
    for (field, handler) in [
        (
            "publishedOnPublication",
            collection_published_on_publication_field
                as crate::resolver_registry::FieldResolverHandler,
        ),
        (
            "publishedOnCurrentPublication",
            collection_published_on_current_publication_field,
        ),
        (
            "availablePublicationsCount",
            collection_publications_count_field,
        ),
        (
            "resourcePublicationsCount",
            collection_publications_count_field,
        ),
        ("publicationCount", collection_publication_count_field),
    ] {
        registrations.push(FieldResolverRegistration::explicit(
            ApiSurface::Admin,
            "Collection",
            field,
            handler,
        ));
    }
    for field in [
        "category",
        "combinedListing",
        "combinedListingRole",
        "createdAt",
        "descriptionHtml",
        "giftCardTemplateSuffix",
        "handle",
        "hasOnlyDefaultVariant",
        "hasOutOfStockVariants",
        "id",
        "isGiftCard",
        "legacyResourceId",
        "onlineStorePreviewUrl",
        "options",
        "productType",
        "requiresSellingPlan",
        "seo",
        "status",
        "tags",
        "templateSuffix",
        "title",
        "totalInventory",
        "totalVariants",
        "tracksInventory",
        "updatedAt",
        "vendor",
    ] {
        registrations.push(FieldResolverRegistration::property(
            ApiSurface::Admin,
            "Product",
            field,
        ));
    }
    for field in ["channelId", "country", "id", "language", "status"] {
        registrations.push(FieldResolverRegistration::property(
            ApiSurface::Admin,
            "ProductFeed",
            field,
        ));
    }
    for (field, handler) in [
        (
            "compareAtPriceRange",
            product_compare_at_price_range_field as crate::resolver_registry::FieldResolverHandler,
        ),
        ("collections", product_collections_field),
        ("featuredImage", product_featured_image_field),
        ("featuredMedia", product_featured_media_field),
        ("images", product_images_field),
        ("media", product_media_field),
        ("metafield", product_metafield_field),
        ("metafields", product_metafields_field),
        (
            "availablePublicationsCount",
            product_publication_count_field,
        ),
        ("publications", product_publication_connection_field),
        ("productPublications", product_publication_connection_field),
        ("publishedAt", product_published_at_field),
        (
            "publishedOnCurrentPublication",
            product_published_on_current_publication_field,
        ),
        (
            "resourcePublicationOnCurrentPublication",
            product_resource_publication_on_current_publication_field,
        ),
        ("resourcePublications", product_publication_connection_field),
        ("resourcePublicationsCount", product_publication_count_field),
        (
            "resourcePublicationsV2",
            product_publication_connection_field,
        ),
        ("sellingPlanGroups", product_selling_plan_groups_field),
        (
            "sellingPlanGroupsCount",
            product_selling_plan_groups_count_field,
        ),
        ("priceRange", product_price_range_field),
        ("priceRangeV2", product_price_range_v2_field),
        ("variants", product_variants_field),
        ("variantsCount", product_variants_count_field),
    ] {
        let registration =
            FieldResolverRegistration::explicit(ApiSurface::Admin, "Product", field, handler);
        registrations.push(registration);
    }
    registrations.push(FieldResolverRegistration::explicit_outcome_terminal(
        ApiSurface::Admin,
        "Product",
        "publishedOnPublication",
        product_published_on_publication_field_outcome,
    ));
    registrations.extend([
        FieldResolverRegistration::explicit(
            ApiSurface::Admin,
            "Metafield",
            "reference",
            metafield_reference_field,
        ),
        FieldResolverRegistration::explicit(
            ApiSurface::Admin,
            "Metafield",
            "references",
            metafield_references_field,
        ),
    ]);
    for field in [
        "compareDigest",
        "createdAt",
        "definition",
        "id",
        "jsonValue",
        "key",
        "namespace",
        "owner",
        "ownerType",
        "type",
        "updatedAt",
        "value",
    ] {
        registrations.push(FieldResolverRegistration::property(
            ApiSurface::Admin,
            "Metafield",
            field,
        ));
    }
    for field in [
        "availableForSale",
        "barcode",
        "compareAtPrice",
        "defaultCursor",
        "id",
        "inventoryPolicy",
        "inventoryQuantity",
        "legacyResourceId",
        "position",
        "price",
        "productVariantComponents",
        "requiresComponents",
        "selectedOptions",
        "sellableOnlineQuantity",
        "sku",
        "taxable",
        "title",
    ] {
        registrations.push(FieldResolverRegistration::property(
            ApiSurface::Admin,
            "ProductVariant",
            field,
        ));
    }
    for (field, handler) in [
        (
            "inventoryItem",
            product_variant_inventory_item_field as crate::resolver_registry::FieldResolverHandler,
        ),
        ("media", product_variant_media_field),
        ("metafield", product_variant_metafield_field),
        ("metafields", product_variant_metafields_field),
        ("product", product_variant_product_field),
        (
            "publishedOnCurrentPublication",
            product_variant_published_on_current_publication_field,
        ),
        (
            "publishedOnPublication",
            product_variant_published_on_publication_field,
        ),
        (
            "sellingPlanGroups",
            product_variant_selling_plan_groups_field,
        ),
        (
            "sellingPlanGroupsCount",
            product_variant_selling_plan_groups_count_field,
        ),
    ] {
        registrations.push(FieldResolverRegistration::explicit(
            ApiSurface::Admin,
            "ProductVariant",
            field,
            handler,
        ));
    }
    registrations
}

fn catalog_search_predicate_requires_full_catalog(predicate: &str) -> bool {
    let predicate = predicate.to_ascii_lowercase();
    predicate.contains("inventory_total:")
        || predicate.contains("variants.price:")
        || predicate.contains("metafields.")
}

impl DraftProxy {
    /// A catalog search over aggregate predicates needs Shopify's complete
    /// index; a partial observed/staged graph cannot answer it faithfully.
    fn product_query_needs_upstream_catalog_search(fields: &[RootFieldSelection]) -> bool {
        fields.iter().any(|field| {
            matches!(
                field.name.as_str(),
                "products" | "productsCount" | "productVariants" | "productVariantsCount"
            ) && matches!(
                field.arguments.get("query"),
                Some(ResolvedValue::String(predicate))
                    if catalog_search_predicate_requires_full_catalog(predicate)
            )
        })
    }

    pub(in crate::proxy) fn product_read_needs_upstream(
        &self,
        fields: &[RootFieldSelection],
    ) -> bool {
        // Publication creates/deletes need the executable local root and field
        // resolvers so they can overlay the caller's single shared upstream
        // response. A direct full-document passthrough would bypass those
        // tombstones and surface deleted publications or memberships again.
        if self.store.has_publication_overlay() {
            return false;
        }
        if Self::product_query_needs_upstream_catalog_search(fields) {
            return true;
        }
        if self.config.read_mode == ReadMode::Snapshot {
            return false;
        }
        fields
            .iter()
            .any(|field| self.live_hybrid_product_field_needs_upstream(field))
    }

    fn live_hybrid_product_field_needs_upstream(&self, field: &RootFieldSelection) -> bool {
        match field.name.as_str() {
            "products" | "productsCount" => true,
            "product" => {
                let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
                id.is_empty()
                    || (!self.store.has_product(&id) && !self.store.product_is_tombstoned(&id))
            }
            "productByIdentifier" => !self.product_identifier_has_local_answer(field),
            _ => false,
        }
    }

    fn product_identifier_has_local_answer(&self, field: &RootFieldSelection) -> bool {
        let Some(identifier) = resolved_object_field(&field.arguments, "identifier") else {
            return false;
        };
        if let Some(id) = resolved_string_field(&identifier, "id") {
            return self.store.has_product(&id) || self.store.product_is_tombstoned(&id);
        }
        if let Some(handle) = resolved_string_field(&identifier, "handle") {
            return self.store.product_by_handle(&handle).is_some();
        }
        false
    }
}

impl DraftProxy {
    pub(crate) fn products_count_outcome(
        &mut self,
        invocation: RootInvocation<'_>,
    ) -> ResolverOutcome<Value> {
        if self.config.read_mode != ReadMode::Snapshot {
            return self.cached_or_forward_upstream_root_outcome(
                invocation.request,
                invocation.response_key,
            );
        }
        let arguments = invocation
            .arguments
            .iter()
            .map(|(name, value)| (name.clone(), resolved_value_from_json(value)))
            .collect::<BTreeMap<_, _>>();
        let count = if arguments.contains_key("query") {
            staged_connection_query(
                self.store.products(),
                &arguments,
                |product, query| self.product_search_decision(product, query),
                product_staged_sort_key,
                |product| product_cursor(product).to_string(),
            )
            .total_count
        } else {
            self.store.product_count()
        };
        ResolverOutcome::value(count_object(count))
    }

    pub(crate) fn products_root_outcome(
        &mut self,
        invocation: RootInvocation<'_>,
    ) -> ResolverOutcome<Value> {
        if self.config.read_mode != ReadMode::Snapshot {
            return self.cached_or_forward_upstream_root_outcome(
                invocation.request,
                invocation.response_key,
            );
        }
        let arguments = invocation
            .arguments
            .iter()
            .map(|(name, value)| (name.clone(), resolved_value_from_json(value)))
            .collect::<BTreeMap<_, _>>();
        ResolverOutcome::value(staged_connection_value_with_args(
            self.store.products(),
            &arguments,
            |product, query| self.product_search_decision(product, query),
            product_staged_sort_key,
            |product| self.product_canonical_value(product),
            |product| product_cursor(product).to_string(),
        ))
    }

    pub(crate) fn product_by_identifier_root_outcome(
        &mut self,
        invocation: RootInvocation<'_>,
    ) -> ResolverOutcome<Value> {
        let identifier = invocation
            .arguments
            .get("identifier")
            .and_then(Value::as_object);
        let id = identifier
            .and_then(|identifier| identifier.get("id"))
            .and_then(Value::as_str);
        let handle = identifier
            .and_then(|identifier| identifier.get("handle"))
            .and_then(Value::as_str);
        let product = id
            .and_then(|id| self.store.product_by_id(id))
            .or_else(|| handle.and_then(|handle| self.store.product_by_handle(handle)));
        let has_local_answer =
            product.is_some() || id.is_some_and(|id| self.store.product_is_tombstoned(id));
        if self.config.read_mode == ReadMode::Live
            || (self.config.read_mode == ReadMode::LiveHybrid && !has_local_answer)
        {
            return self.cached_or_forward_upstream_root_outcome(
                invocation.request,
                invocation.response_key,
            );
        }
        ResolverOutcome::value(
            product
                .map(|product| self.product_canonical_value(product))
                .unwrap_or(Value::Null),
        )
    }

    pub(crate) fn product_variant_root_outcome(
        &mut self,
        invocation: RootInvocation<'_>,
    ) -> ResolverOutcome<Value> {
        let id = invocation
            .arguments
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let owner_metafield_catalog_active = self
            .store
            .staged
            .owner_metafields
            .keys()
            .any(|owner_id| shopify_gid_resource_type(owner_id) == Some("ProductVariant"));
        let owner_read_fallback = owner_metafield_catalog_active
            && self.execution_session.owner_metafield_read_ids.contains(id);
        let owner_known_missing = self
            .execution_session
            .owner_metafield_missing_ids
            .contains(id);
        let has_local_answer = self.store.product_variant_by_id(id).is_some()
            || self.store.product_variants.staged.is_tombstoned(id)
            || self.owner_has_metafield_local_effects(id)
            || owner_read_fallback
            || owner_known_missing
            || self
                .execution_session
                .owner_metafield_hydrated_ids
                .contains(id);
        if self.config.read_mode == ReadMode::Live
            || (self.config.read_mode == ReadMode::LiveHybrid && !has_local_answer)
        {
            return self.cached_or_forward_upstream_root_outcome(
                invocation.request,
                invocation.response_key,
            );
        }
        let value = if self.store.product_variants.staged.is_tombstoned(id) || owner_known_missing {
            Value::Null
        } else {
            self.store
                .product_variant_by_id(id)
                .map(|variant| self.product_variant_canonical_value(variant))
                .or_else(|| {
                    (self.owner_has_metafield_local_effects(id)
                        || (self.config.read_mode == ReadMode::Snapshot
                            && owner_metafield_catalog_active)
                        || owner_read_fallback)
                        .then(|| json!({ "id": id }))
                })
                .unwrap_or(Value::Null)
        };
        ResolverOutcome::value(value)
    }

    pub(crate) fn product_root_outcome(
        &mut self,
        invocation: RootInvocation<'_>,
    ) -> ResolverOutcome<Value> {
        let id = invocation
            .arguments
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let owner_metafield_catalog_active = self
            .store
            .staged
            .owner_metafields
            .keys()
            .any(|owner_id| shopify_gid_resource_type(owner_id) == Some("Product"));
        let owner_read_fallback = owner_metafield_catalog_active
            && self.execution_session.owner_metafield_read_ids.contains(id);
        let owner_known_missing = self
            .execution_session
            .owner_metafield_missing_ids
            .contains(id);
        let has_local_answer = self.store.has_product(id)
            || self.store.product_is_tombstoned(id)
            || self.owner_has_metafield_local_effects(id)
            || owner_read_fallback
            || owner_known_missing
            || self
                .execution_session
                .owner_metafield_hydrated_ids
                .contains(id);
        if self.config.read_mode == ReadMode::LiveHybrid
            && !has_local_answer
            && !self.store.staged.deleted_publication_ids.is_empty()
        {
            let upstream = self.cached_or_forward_upstream_root_outcome(
                invocation.request,
                invocation.response_key,
            );
            if !upstream.errors.is_empty() {
                return upstream;
            }
            self.store.observe_base_product_json(&upstream.value);
            return ResolverOutcome::value(
                self.store
                    .product_by_id(id)
                    .map(|product| self.product_canonical_value(product))
                    .unwrap_or(Value::Null),
            );
        }
        if self.config.read_mode == ReadMode::Live
            || (self.config.read_mode == ReadMode::LiveHybrid && !has_local_answer)
        {
            return self.cached_or_forward_upstream_root_outcome(
                invocation.request,
                invocation.response_key,
            );
        }
        let value = if self.store.product_is_tombstoned(id) || owner_known_missing {
            Value::Null
        } else {
            self.store
                .product_by_id(id)
                .map(|product| self.product_canonical_value(product))
                .or_else(|| {
                    (self.owner_has_metafield_local_effects(id)
                        || (self.config.read_mode == ReadMode::Snapshot
                            && owner_metafield_catalog_active)
                        || owner_read_fallback)
                        .then(|| json!({ "id": id }))
                })
                .unwrap_or(Value::Null)
        };
        ResolverOutcome::value(value)
    }

    pub(in crate::proxy) fn product_canonical_value(&self, product: &ProductRecord) -> Value {
        let variants = self.store.product_variants_for_product(&product.id);
        let mut object = product
            .extra_fields
            .clone()
            .into_iter()
            .collect::<serde_json::Map<_, _>>();
        object.extend([
            ("__typename".to_string(), json!("Product")),
            ("id".to_string(), json!(product.id)),
            ("title".to_string(), json!(product.title)),
            ("handle".to_string(), json!(product.handle)),
            ("status".to_string(), json!(product.status)),
            ("createdAt".to_string(), json!(product.created_at)),
            ("updatedAt".to_string(), json!(product.updated_at)),
            (
                "descriptionHtml".to_string(),
                json!(product.description_html),
            ),
            ("vendor".to_string(), json!(product.vendor)),
            ("productType".to_string(), json!(product.product_type)),
            ("tags".to_string(), json!(product.tags)),
            (
                "legacyResourceId".to_string(),
                json!(resource_id_tail(&product.id)),
            ),
            ("totalInventory".to_string(), json!(product.total_inventory)),
            (
                "tracksInventory".to_string(),
                if variants.is_empty() {
                    json!(product.tracks_inventory)
                } else {
                    json!(variants
                        .iter()
                        .any(|variant| variant.inventory_item.tracked))
                },
            ),
            (
                "hasOnlyDefaultVariant".to_string(),
                if variants.is_empty() {
                    product
                        .extra_fields
                        .get("hasOnlyDefaultVariant")
                        .cloned()
                        .unwrap_or(Value::Bool(true))
                } else {
                    json!(product_has_only_default_variant(&variants))
                },
            ),
            (
                "hasOutOfStockVariants".to_string(),
                if variants.is_empty() {
                    product
                        .extra_fields
                        .get("hasOutOfStockVariants")
                        .cloned()
                        .unwrap_or(Value::Bool(false))
                } else {
                    json!(product_has_out_of_stock_variants(&variants))
                },
            ),
            (
                "totalVariants".to_string(),
                if variants.is_empty() {
                    product
                        .extra_fields
                        .get("totalVariants")
                        .cloned()
                        .unwrap_or_else(|| json!(product.variants.len()))
                } else {
                    json!(variants.len())
                },
            ),
            (
                "templateSuffix".to_string(),
                if product.template_suffix.is_empty() {
                    Value::Null
                } else {
                    json!(product.template_suffix)
                },
            ),
            (
                "seo".to_string(),
                product.extra_fields.get("seo").cloned().unwrap_or_else(|| {
                    json!({
                        "title": product.seo_title,
                        "description": product.seo_description,
                    })
                }),
            ),
            (
                "onlineStorePreviewUrl".to_string(),
                product
                    .extra_fields
                    .get("onlineStorePreviewUrl")
                    .cloned()
                    .unwrap_or(Value::Null),
            ),
            (
                "category".to_string(),
                product
                    .extra_fields
                    .get("category")
                    .cloned()
                    .unwrap_or(Value::Null),
            ),
            (
                "requiresSellingPlan".to_string(),
                product
                    .extra_fields
                    .get("requiresSellingPlan")
                    .cloned()
                    .unwrap_or(Value::Bool(false)),
            ),
            (
                "isGiftCard".to_string(),
                product
                    .extra_fields
                    .get("isGiftCard")
                    .cloned()
                    .unwrap_or(Value::Bool(false)),
            ),
            (
                "giftCardTemplateSuffix".to_string(),
                product
                    .extra_fields
                    .get("giftCardTemplateSuffix")
                    .cloned()
                    .unwrap_or(Value::Null),
            ),
            (
                "options".to_string(),
                product
                    .extra_fields
                    .get("options")
                    .cloned()
                    .unwrap_or_else(|| Value::Array(Vec::new())),
            ),
        ]);
        Value::Object(object)
    }

    pub(in crate::proxy) fn product_variant_canonical_value(
        &self,
        variant: &ProductVariantRecord,
    ) -> Value {
        let variant = self.variant_with_inventory_levels(variant);
        let mut value = product_variant_state_json(&variant);
        value["__typename"] = json!("ProductVariant");
        value["legacyResourceId"] = json!(resource_id_tail(&variant.id));
        value["defaultCursor"] = json!(variant.id);
        value["availableForSale"] =
            json!(!variant.inventory_item.tracked || variant.inventory_quantity > 0);
        value["sellableOnlineQuantity"] = json!(variant.inventory_quantity.max(0));
        value
    }
}

fn product_field_arguments(
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> BTreeMap<String, ResolvedValue> {
    invocation
        .arguments
        .iter()
        .map(|(name, value)| (name.clone(), resolved_value_from_json(value)))
        .collect()
}

fn combined_listing_children_field(
    _proxy: &mut DraftProxy,
    _request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> Result<Value, String> {
    Ok(connection_value_with_args(
        invocation
            .parent
            .get("combinedListingChildren")
            .map(connection_nodes)
            .unwrap_or_default(),
        &product_field_arguments(invocation),
        |child| {
            child
                .pointer("/product/id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        },
    ))
}

fn product_field_record(
    proxy: &DraftProxy,
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> Option<ProductRecord> {
    let id = invocation.parent.get("id").and_then(Value::as_str)?;
    proxy.store.product_by_id(id).cloned()
}

fn product_price_range_field(
    proxy: &mut DraftProxy,
    request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> Result<Value, String> {
    product_price_range_resolver_value(proxy, request, invocation, ProductPriceRangeKind::Legacy)
}

fn product_price_range_v2_field(
    proxy: &mut DraftProxy,
    request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> Result<Value, String> {
    product_price_range_resolver_value(proxy, request, invocation, ProductPriceRangeKind::Current)
}

fn product_price_range_resolver_value(
    proxy: &mut DraftProxy,
    request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
    kind: ProductPriceRangeKind,
) -> Result<Value, String> {
    proxy.hydrate_shop_pricing_state_if_missing(request, true, false);
    let Some(product) = product_field_record(proxy, invocation) else {
        return Ok(Value::Null);
    };
    let variants = proxy.store.product_variants_for_product(&product.id);
    Ok(product_price_range_value(
        &product,
        &variants,
        &proxy.store.shop_currency_code(),
        kind,
    ))
}

fn product_compare_at_price_range_field(
    proxy: &mut DraftProxy,
    request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> Result<Value, String> {
    proxy.hydrate_shop_pricing_state_if_missing(request, true, false);
    let Some(product) = product_field_record(proxy, invocation) else {
        return Ok(Value::Null);
    };
    let variants = proxy.store.product_variants_for_product(&product.id);
    Ok(product_compare_at_price_range_value(
        &product,
        &variants,
        &proxy.store.shop_currency_code(),
    ))
}

fn product_variants_count_field(
    proxy: &mut DraftProxy,
    _request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> Result<Value, String> {
    let Some(product) = product_field_record(proxy, invocation) else {
        return Ok(Value::Null);
    };
    let variants = proxy.store.product_variants_for_product(&product.id);
    Ok(product_variants_count_value(&product, &variants))
}

fn product_variants_field(
    proxy: &mut DraftProxy,
    _request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> Result<Value, String> {
    let Some(product) = product_field_record(proxy, invocation) else {
        return Ok(connection_json(Vec::new()));
    };
    let variants = proxy.store.product_variants_for_product(&product.id);
    Ok(product_variant_connection_with_fallback_value(
        &variants,
        &product.variants,
        &product_field_arguments(invocation),
    ))
}

fn product_collections_field(
    proxy: &mut DraftProxy,
    _request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> Result<Value, String> {
    let Some(product) = product_field_record(proxy, invocation) else {
        return Ok(connection_json(Vec::new()));
    };
    Ok(product_collections_connection_value(
        &product,
        &product_field_arguments(invocation),
    ))
}

fn product_media_field(
    proxy: &mut DraftProxy,
    _request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> Result<Value, String> {
    // Mutation payloads may intentionally expose a point-in-time media view
    // (for example productCreateMedia returns UPLOADED before the next read
    // promotes the stored asset to READY). The explicit resolver owns that
    // distinction; the generic field runtime never falls back to a materialized
    // property implicitly.
    if let Some(media) = invocation.parent.get("media") {
        return Ok(connection_value_with_args(
            connection_nodes(media),
            &product_field_arguments(invocation),
            value_id_cursor,
        ));
    }
    proxy.promote_all_product_media_ready_on_read();
    let Some(product) = product_field_record(proxy, invocation) else {
        return Ok(connection_json(Vec::new()));
    };
    Ok(product_media_connection_value(
        product.media,
        &product_field_arguments(invocation),
    ))
}

fn product_images_field(
    proxy: &mut DraftProxy,
    _request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> Result<Value, String> {
    proxy.promote_all_product_media_ready_on_read();
    let Some(product) = product_field_record(proxy, invocation) else {
        return Ok(connection_json(Vec::new()));
    };
    Ok(product_image_connection_value(
        product.media,
        &product_field_arguments(invocation),
    ))
}

fn product_featured_image_field(
    proxy: &mut DraftProxy,
    _request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> Result<Value, String> {
    proxy.promote_all_product_media_ready_on_read();
    Ok(product_field_record(proxy, invocation)
        .and_then(|product| product.media.iter().find_map(product_image_json_from_media))
        .unwrap_or(Value::Null))
}

fn product_featured_media_field(
    proxy: &mut DraftProxy,
    _request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> Result<Value, String> {
    proxy.promote_all_product_media_ready_on_read();
    Ok(product_field_record(proxy, invocation)
        .and_then(|product| product.media.first().cloned())
        .unwrap_or(Value::Null))
}

fn product_metafield_field(
    proxy: &mut DraftProxy,
    request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> Result<Value, String> {
    if invocation
        .parent
        .get("id")
        .and_then(Value::as_str)
        .is_none()
    {
        return Ok(Value::Null);
    }
    let arguments = product_field_arguments(invocation);
    let api_client_id = request_app_namespace_api_client_id(request);
    Ok(proxy.canonical_embedded_or_owner_metafield_value(
        invocation.parent,
        &arguments,
        api_client_id.as_deref(),
    ))
}

fn product_metafields_field(
    proxy: &mut DraftProxy,
    request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> Result<Value, String> {
    if invocation
        .parent
        .get("id")
        .and_then(Value::as_str)
        .is_none()
    {
        return Ok(connection_json(Vec::new()));
    }
    let arguments = product_field_arguments(invocation);
    let api_client_id = request_app_namespace_api_client_id(request);
    Ok(
        proxy.canonical_embedded_or_owner_metafields_connection_value(
            invocation.parent,
            &arguments,
            api_client_id.as_deref(),
        ),
    )
}

fn product_variant_record(
    proxy: &DraftProxy,
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> Option<ProductVariantRecord> {
    let id = invocation.parent.get("id").and_then(Value::as_str)?;
    proxy.store.product_variant_by_id(id).cloned()
}

fn product_variant_inventory_item_field(
    proxy: &mut DraftProxy,
    _request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> Result<Value, String> {
    Ok(product_variant_record(proxy, invocation)
        .filter(|variant| is_shopify_gid_of_type(&variant.inventory_item.id, "InventoryItem"))
        .map(|variant| {
            let variant = proxy.variant_with_inventory_levels(&variant);
            product_variant_state_json(&variant)["inventoryItem"].clone()
        })
        .unwrap_or(Value::Null))
}

fn product_variant_product_field(
    proxy: &mut DraftProxy,
    _request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> Result<Value, String> {
    Ok(product_variant_record(proxy, invocation)
        .and_then(|variant| {
            proxy
                .store
                .product_by_id(&variant.product_id)
                .map(|product| proxy.product_canonical_value(product))
        })
        // Relationship parents learned through another domain (for example a
        // PriceListPrice variant) can carry a canonical embedded product before
        // the product aggregate itself has been hydrated. Preserve that observed
        // relationship instead of replacing it with null merely because the
        // typed product store is cold.
        .or_else(|| invocation.parent.get("product").cloned())
        .unwrap_or(Value::Null))
}

fn product_variant_media_field(
    proxy: &mut DraftProxy,
    _request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> Result<Value, String> {
    let Some(variant) = product_variant_record(proxy, invocation) else {
        return Ok(connection_json(Vec::new()));
    };
    let media =
        variant_attached_media_nodes(&variant, proxy.store.product_by_id(&variant.product_id));
    Ok(product_media_connection_value(
        media,
        &product_field_arguments(invocation),
    ))
}

fn product_variant_metafield_field(
    proxy: &mut DraftProxy,
    request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> Result<Value, String> {
    if invocation
        .parent
        .get("id")
        .and_then(Value::as_str)
        .is_none()
    {
        return Ok(Value::Null);
    }
    let arguments = product_field_arguments(invocation);
    let api_client_id = request_app_namespace_api_client_id(request);
    Ok(proxy.canonical_embedded_or_owner_metafield_value(
        invocation.parent,
        &arguments,
        api_client_id.as_deref(),
    ))
}

fn product_variant_metafields_field(
    proxy: &mut DraftProxy,
    request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> Result<Value, String> {
    if invocation
        .parent
        .get("id")
        .and_then(Value::as_str)
        .is_none()
    {
        return Ok(connection_json(Vec::new()));
    }
    let arguments = product_field_arguments(invocation);
    let api_client_id = request_app_namespace_api_client_id(request);
    Ok(
        proxy.canonical_embedded_or_owner_metafields_connection_value(
            invocation.parent,
            &arguments,
            api_client_id.as_deref(),
        ),
    )
}

fn product_variant_published_on_current_publication_field(
    proxy: &mut DraftProxy,
    _request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> Result<Value, String> {
    Ok(product_variant_record(proxy, invocation)
        .and_then(|variant| proxy.store.product_by_id(&variant.product_id))
        .map(|product| {
            Value::Bool(
                proxy
                    .store
                    .product_is_published_on_current_publication(product),
            )
        })
        .unwrap_or(Value::Bool(false)))
}

fn product_variant_published_on_publication_field(
    proxy: &mut DraftProxy,
    _request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> Result<Value, String> {
    let publication_id = invocation
        .arguments
        .get("publicationId")
        .and_then(Value::as_str)
        .unwrap_or_default();
    Ok(product_variant_record(proxy, invocation)
        .and_then(|variant| proxy.store.product_by_id(&variant.product_id))
        .map(|product| Value::Bool(product_is_published_on_publication(product, publication_id)))
        .unwrap_or(Value::Bool(false)))
}

fn product_variant_selling_plan_groups_field(
    proxy: &mut DraftProxy,
    _request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> Result<Value, String> {
    let Some(variant) = product_variant_record(proxy, invocation) else {
        return Ok(connection_json(Vec::new()));
    };
    Ok(proxy.canonical_product_variant_selling_plan_groups_value(
        &variant.id,
        &variant.product_id,
        &product_field_arguments(invocation),
    ))
}

fn product_variant_selling_plan_groups_count_field(
    proxy: &mut DraftProxy,
    _request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> Result<Value, String> {
    Ok(product_variant_record(proxy, invocation)
        .map(|variant| proxy.canonical_product_variant_selling_plan_groups_count_value(&variant.id))
        .unwrap_or_else(|| count_object(0)))
}

fn product_selling_plan_groups_field(
    proxy: &mut DraftProxy,
    _request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> Result<Value, String> {
    let Some(product) = product_field_record(proxy, invocation) else {
        return Ok(connection_json(Vec::new()));
    };
    Ok(proxy.canonical_product_selling_plan_groups_value(
        &product.id,
        &product_field_arguments(invocation),
    ))
}

fn product_selling_plan_groups_count_field(
    proxy: &mut DraftProxy,
    _request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> Result<Value, String> {
    Ok(product_field_record(proxy, invocation)
        .map(|product| proxy.canonical_product_selling_plan_groups_count_value(&product.id))
        .unwrap_or_else(|| count_object(0)))
}

fn metafield_reference_field(
    proxy: &mut DraftProxy,
    request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> Result<Value, String> {
    Ok(proxy.canonical_metafield_reference_value(invocation.parent, Some(request)))
}

fn metafield_references_field(
    proxy: &mut DraftProxy,
    request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> Result<Value, String> {
    Ok(proxy.canonical_metafield_references_connection_value(
        invocation.parent,
        &product_field_arguments(invocation),
        Some(request),
    ))
}

fn product_published_at_field(
    proxy: &mut DraftProxy,
    _request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> Result<Value, String> {
    Ok(product_field_record(proxy, invocation)
        .and_then(|product| product.extra_fields.get("publishedAt").cloned())
        .unwrap_or(Value::Null))
}

fn product_published_on_current_publication_field(
    proxy: &mut DraftProxy,
    _request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> Result<Value, String> {
    Ok(product_field_record(proxy, invocation)
        .map(|product| {
            Value::Bool(
                proxy
                    .store
                    .product_is_published_on_current_publication(&product),
            )
        })
        .unwrap_or(Value::Bool(false)))
}

fn product_published_on_publication_field(
    proxy: &mut DraftProxy,
    _request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> Result<Value, String> {
    let publication_id = invocation
        .arguments
        .get("publicationId")
        .and_then(Value::as_str)
        .unwrap_or_default();
    Ok(product_field_record(proxy, invocation)
        .map(|product| {
            Value::Bool(product_is_published_on_publication(
                &product,
                publication_id,
            ))
        })
        .unwrap_or(Value::Bool(false)))
}

fn product_published_on_publication_field_outcome(
    proxy: &mut DraftProxy,
    request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> Result<crate::admin_graphql::FieldResolverResult, String> {
    let publication_id = invocation
        .arguments
        .get("publicationId")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if proxy.store.publication_is_deleted(publication_id) {
        return Ok(crate::admin_graphql::FieldResolverResult::Error(
            crate::admin_graphql::RootFieldError {
                message: "Invalid publication id.".to_string(),
                extensions: BTreeMap::from([("code".to_string(), json!("NOT_FOUND"))]),
                path: None,
                locations: Vec::new(),
            },
        ));
    }
    product_published_on_publication_field(proxy, request, invocation)
        .map(crate::admin_graphql::FieldResolverResult::Resolved)
}

fn product_resource_publication_on_current_publication_field(
    _proxy: &mut DraftProxy,
    _request: &Request,
    _invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> Result<Value, String> {
    Ok(Value::Null)
}

fn product_publication_count_field(
    proxy: &mut DraftProxy,
    _request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> Result<Value, String> {
    let Some(product) = product_field_record(proxy, invocation) else {
        return Ok(count_object(0));
    };
    let deleted_membership_count = proxy
        .store
        .staged
        .deleted_publication_resource_ids
        .values()
        .filter(|resource_ids| resource_ids.contains(&product.id))
        .count();
    let deleted_memberships_are_known = proxy
        .store
        .staged
        .deleted_publication_resource_ids
        .keys()
        .all(|publication_id| {
            proxy
                .store
                .staged
                .deleted_publication_resource_ids_complete
                .contains(publication_id)
                || proxy
                    .store
                    .staged
                    .deleted_publication_resource_ids
                    .get(publication_id)
                    .is_some_and(|resource_ids| resource_ids.contains(&product.id))
        });
    if !proxy
        .store
        .staged
        .deleted_publication_resource_ids
        .is_empty()
        && deleted_memberships_are_known
    {
        if let Some(mut value) = product.extra_fields.get(&invocation.field_name).cloned() {
            if let Some(count) = value.get("count").and_then(Value::as_u64) {
                value["count"] = json!(count.saturating_sub(deleted_membership_count as u64));
                return Ok(value);
            }
        }
    }
    if proxy.store.staged.deleted_publication_ids.is_empty() {
        if let Some(value) = product.extra_fields.get(&invocation.field_name) {
            return Ok(value.clone());
        }
    }
    Ok(count_object(
        product_visible_publication_entries(&product)
            .into_iter()
            .filter(|entry| !proxy.store.publication_is_deleted(&entry.publication_id))
            .count(),
    ))
}

pub(in crate::proxy) fn canonical_product_publication_node(
    product: &ProductRecord,
    entry: &ProductPublicationEntry,
) -> Value {
    json!({
        "__typename": "ProductPublication",
        "channel": Value::Null,
        "isPublished": true,
        "publishDate": product_publication_publish_date_json(entry),
        "product": { "__typename": "Product", "id": product.id },
    })
}

fn canonical_resource_publication_node(
    product: &ProductRecord,
    entry: &ProductPublicationEntry,
    typename: &str,
) -> Value {
    json!({
        "__typename": typename,
        "channel": Value::Null,
        "isPublished": true,
        "publication": { "__typename": "Publication", "id": entry.publication_id },
        "publishDate": product_publication_publish_date_json(entry),
        "publishable": { "__typename": "Product", "id": product.id },
    })
}

fn product_publication_connection_field(
    proxy: &mut DraftProxy,
    _request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> Result<Value, String> {
    let Some(product) = product_field_record(proxy, invocation) else {
        return Ok(connection_json(Vec::new()));
    };
    if proxy.store.staged.deleted_publication_ids.is_empty() {
        if let Some(value) = product.extra_fields.get(&invocation.field_name) {
            if matches!(
                invocation.field_name.as_str(),
                "publications" | "resourcePublicationsV2"
            ) {
                return Ok(value.clone());
            }
        }
    }
    let entries = product_visible_publication_entries(&product)
        .into_iter()
        .filter(|entry| !proxy.store.publication_is_deleted(&entry.publication_id))
        .collect::<Vec<_>>();
    let nodes = entries
        .iter()
        .map(|entry| match invocation.field_name.as_str() {
            "publications" | "productPublications" => {
                canonical_product_publication_node(&product, entry)
            }
            "resourcePublications" => {
                canonical_resource_publication_node(&product, entry, "ResourcePublication")
            }
            _ => canonical_resource_publication_node(&product, entry, "ResourcePublicationV2"),
        })
        .collect::<Vec<_>>();
    Ok(connection_value_with_args(
        nodes,
        &product_field_arguments(invocation),
        |node| {
            node.pointer("/publication/id")
                .or_else(|| node.pointer("/product/id"))
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        },
    ))
}

const PRODUCT_STATUS_BASE_VALUES: &[&str] = &["ACTIVE", "ARCHIVED", "DRAFT"];
const PRODUCT_SCALAR_MAX_LENGTH: usize = 255;
const VARIANT_MONEY_UPPER_BOUND: f64 = 1_000_000_000_000_000_000.0;
pub(in crate::proxy) const PRODUCT_OPTION_NAME_TITLE_DELIMITER: &str = " / ";
pub(in crate::proxy) const PRODUCT_OPTION_NAME_DELIMITER_MESSAGE: &str =
    "The name contains unsupported sequence ' / '";
pub(in crate::proxy) const PRODUCT_CREATE_OPTION_NAME_DELIMITER_MESSAGE: &str =
    "Name cannot contain the character sequence \" / \"";

pub(in crate::proxy) fn product_option_name_has_title_delimiter(name: &str) -> bool {
    name.trim().contains(PRODUCT_OPTION_NAME_TITLE_DELIMITER)
}

// The batched node-hydrate query the proxy forwards to observe pre-existing
// products / variants / collections in LiveHybrid. Shared verbatim with the
// conformance capture scripts so re-recorded cassettes match byte-for-byte.
pub(in crate::proxy) const PRODUCTS_HYDRATE_NODES_OBSERVATION_QUERY: &str = include_str!(
    "../../config/parity-requests/products/products-hydrate-nodes-observation.graphql"
);

// `productSet` must decide update-vs-create from a real existing product when
// LiveHybrid has not observed it yet. These productSet-owned hydrate documents
// select product options in addition to variants so omitted-field and replacement
// semantics can build from the upstream product graph.
pub(in crate::proxy) const PRODUCT_SET_TARGET_HYDRATE_BY_ID_QUERY: &str =
    include_str!("../../config/parity-requests/products/productSet-target-hydrate-by-id.graphql");

pub(in crate::proxy) const PRODUCT_SET_TARGET_HYDRATE_BY_HANDLE_QUERY: &str = include_str!(
    "../../config/parity-requests/products/productSet-target-hydrate-by-handle.graphql"
);

pub(in crate::proxy) const TAXONOMY_CATEGORY_HYDRATE_QUERY: &str = "query ProductTaxonomyCategoryHydrate($id: ID!) { node(id: $id) { __typename id ... on TaxonomyCategory { name fullName isLeaf level parentId } } }";

pub(in crate::proxy) const COLLECTION_REORDER_PRODUCTS_COLLECTION_HYDRATE_QUERY: &str = include_str!(
    "../../config/parity-requests/products/collectionReorderProducts-collection-hydrate.graphql"
);

// The generic observation query above does not select product `options`, which the
// productOptionsReorder graph needs. This options-aware node hydrate selects the
// option/optionValue graph (and variants) and is forwarded only by the reorder
// owner-hydrate path. Kept as a shared `.graphql` doc so re-recorded cassettes match
// the emitted forward byte-for-byte.
pub(in crate::proxy) const PRODUCT_OPTIONS_HYDRATE_NODES_QUERY: &str =
    include_str!("../../config/parity-requests/products/product-options-hydrate-nodes.graphql");

// Publication-membership hydrate forwarded the first time the local publication
// engine publishes a publishable resource (product / collection) it has never
// seen. It reads the resource's title/status and the set of publications it is
// already published on (e.g. the default Online Store), so a pre-existing
// resource's membership is discovered by reading upstream rather than injected
// via `/__meta/seed`. Shared verbatim with the cassette so the forward matches
// byte-for-byte.
pub(in crate::proxy) const PUBLICATION_RESOURCE_HYDRATE_QUERY: &str = include_str!(
    "../../config/parity-requests/products/publication-resource-hydrate-nodes.graphql"
);

pub(in crate::proxy) const CURRENT_APP_PUBLICATION_HYDRATE_QUERY: &str = include_str!(
    "../../config/parity-requests/store-properties/current-app-publication-hydrate.graphql"
);

struct ProductStatusInputContext<'a> {
    argument_name: &'a str,
    input_object_type: &'a str,
    field_name: &'a str,
    expected_type: &'a str,
}

pub(in crate::proxy) struct ProductStatusArgumentContext<'a> {
    pub argument_name: &'a str,
    pub container_type_name: &'a str,
    pub container_name: &'a str,
    pub expected_type: &'a str,
}

struct ProductStatusLiteralError<'a> {
    value: &'a str,
    argument_name: &'a str,
    type_name: &'a str,
    container_name: &'a str,
    expected_type: &'a str,
    location: Option<SourceLocation>,
}

pub(in crate::proxy) fn merge_observed_product(
    mut existing: ProductRecord,
    observed: ProductRecord,
) -> ProductRecord {
    existing.title = observed.title;
    existing.handle = observed.handle;
    existing.status = observed.status;
    existing.created_at = observed.created_at;
    existing.updated_at = observed.updated_at;
    existing.description_html = observed.description_html;
    existing.vendor = observed.vendor;
    existing.product_type = observed.product_type;
    existing.tags = observed.tags;
    existing.template_suffix = observed.template_suffix;
    existing.seo_title = observed.seo_title;
    existing.seo_description = observed.seo_description;
    existing.total_inventory = observed.total_inventory;
    existing.tracks_inventory = observed.tracks_inventory;
    if !observed.media.is_empty() {
        existing.media = observed.media;
    }
    if !observed.variants.is_empty() {
        existing.variants = observed
            .variants
            .into_iter()
            .filter_map(|variant| {
                let observed_id = variant.get("id").and_then(Value::as_str);
                let Some(id) = observed_id else {
                    return Some(variant);
                };
                existing
                    .variants
                    .iter()
                    .find(|existing| existing.get("id").and_then(Value::as_str) == Some(id))
                    .map(|existing| shallow_merged_object(existing.clone(), variant))
            })
            .collect();
    }
    for collection in observed.collections {
        upsert_minimal_collection(&mut existing.collections, &collection);
    }
    existing.extra_fields.extend(observed.extra_fields);
    existing.collections.sort_by(|left, right| {
        let left_title = left
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let right_title = right
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or_default();
        left_title.cmp(right_title)
    });
    existing
}

pub(in crate::proxy) fn product_summary_json(product: &ProductRecord) -> Value {
    json!({
        "id": product.id.clone(),
        "title": product.title.clone(),
        "handle": product.handle.clone()
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::proxy) struct ProductPublicationEntry {
    pub publication_id: String,
    pub publish_date: Option<String>,
    pub published_at: Option<String>,
}

pub(in crate::proxy) fn product_publication_state_known(product: &ProductRecord) -> bool {
    if product.extra_fields.contains_key("productPublications") {
        return true;
    }
    let resource_nodes = product
        .extra_fields
        .get("resourcePublicationsV2")
        .or_else(|| product.extra_fields.get("resourcePublications"))
        .and_then(|connection| connection.get("nodes"))
        .and_then(Value::as_array);
    if resource_nodes.is_some_and(|nodes| !nodes.is_empty()) {
        return true;
    }
    product
        .extra_fields
        .get("resourcePublicationsCount")
        .and_then(|count| count.get("count"))
        .and_then(Value::as_u64)
        == Some(0)
}

pub(in crate::proxy) fn product_publication_entries(
    product: &ProductRecord,
) -> Vec<ProductPublicationEntry> {
    let direct_entries = product
        .extra_fields
        .get("productPublications")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|entry| {
            let publication_id = entry.get("publicationId").and_then(Value::as_str)?;
            Some(ProductPublicationEntry {
                publication_id: publication_id.to_string(),
                publish_date: entry
                    .get("publishDate")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                published_at: entry
                    .get("publishedAt")
                    .and_then(Value::as_str)
                    .map(str::to_string),
            })
        })
        .collect::<Vec<_>>();
    if product.extra_fields.contains_key("productPublications") {
        return direct_entries;
    }

    product
        .extra_fields
        .get("resourcePublicationsV2")
        .or_else(|| product.extra_fields.get("resourcePublications"))
        .and_then(|connection| connection.get("nodes"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|entry| {
            entry
                .get("isPublished")
                .and_then(Value::as_bool)
                .unwrap_or(true)
        })
        .filter_map(|entry| {
            let publication_id = entry
                .get("publication")
                .and_then(|publication| publication.get("id"))
                .and_then(Value::as_str)?;
            Some(ProductPublicationEntry {
                publication_id: publication_id.to_string(),
                publish_date: entry
                    .get("publishDate")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                published_at: entry
                    .get("publishedAt")
                    .and_then(Value::as_str)
                    .map(str::to_string),
            })
        })
        .collect()
}

pub(in crate::proxy) fn set_product_publication_entries(
    product: &mut ProductRecord,
    mut entries: Vec<ProductPublicationEntry>,
) {
    entries.sort_by(|left, right| left.publication_id.cmp(&right.publication_id));
    let published_at = entries
        .iter()
        .filter_map(|entry| entry.published_at.as_ref().or(entry.publish_date.as_ref()))
        .min()
        .cloned();
    let values = entries
        .iter()
        .map(|entry| {
            let mut object = serde_json::Map::new();
            object.insert(
                "publicationId".to_string(),
                json!(entry.publication_id.clone()),
            );
            if let Some(publish_date) = &entry.publish_date {
                object.insert("publishDate".to_string(), json!(publish_date));
            }
            if let Some(published_at) = &entry.published_at {
                object.insert("publishedAt".to_string(), json!(published_at));
            }
            Value::Object(object)
        })
        .collect::<Vec<_>>();
    product
        .extra_fields
        .insert("productPublications".to_string(), Value::Array(values));
    product.extra_fields.insert(
        "publishedAt".to_string(),
        published_at.map(Value::String).unwrap_or(Value::Null),
    );
}

pub(in crate::proxy) fn product_is_published_on_publication(
    product: &ProductRecord,
    publication_id: &str,
) -> bool {
    product_publication_entries(product)
        .iter()
        .any(|entry| entry.publication_id == publication_id)
}

pub(in crate::proxy) fn product_visible_publication_entries(
    product: &ProductRecord,
) -> Vec<ProductPublicationEntry> {
    if product.status == "ACTIVE" {
        product_publication_entries(product)
    } else {
        Vec::new()
    }
}

fn product_publication_publish_date_json(entry: &ProductPublicationEntry) -> Value {
    entry
        .publish_date
        .as_ref()
        .or(entry.published_at.as_ref())
        .map(|value| json!(value))
        .unwrap_or(Value::Null)
}

/// The canonical `Publication` record the local publication engine stages and
/// serves. A publication's backing `Channel` shares the publication's numeric
/// id suffix and name, so both are derived rather than recorded per scenario.
pub(in crate::proxy) fn publication_record_json(id: &str, name: &str, auto_publish: bool) -> Value {
    let suffix = resource_id_path_tail(id);
    let channel_id = shopify_gid("Channel", suffix);
    json!({
        "id": id,
        "name": name,
        "autoPublish": auto_publish,
        "supportsFuturePublishing": false,
        "channel": {
            "id": channel_id,
            "name": name,
            "publication": { "id": id, "name": name }
        }
    })
}

impl DraftProxy {
    pub(in crate::proxy) fn product_top_level_media_append(
        &mut self,
        media_inputs: &[BTreeMap<String, ResolvedValue>],
    ) -> ProductTopLevelMediaAppend {
        let mut append = ProductTopLevelMediaAppend::default();
        for item in media_inputs {
            let original_source = resolved_string_field(item, "originalSource").unwrap_or_default();
            let media_content_type = resolved_string_field(item, "mediaContentType")
                .unwrap_or_else(|| infer_product_media_content_type(&original_source).to_string());
            let id = self.next_proxy_synthetic_gid(product_media_typename(&media_content_type));
            let alt = resolved_string_field(item, "alt").unwrap_or_default();
            append.mutation_nodes.push(product_media_node_with_type(
                &id,
                &alt,
                &media_content_type,
                "UPLOADED",
                None,
                Some(&original_source),
            ));
            append.staged_nodes.push(product_media_node_with_type(
                &id,
                &alt,
                &media_content_type,
                if media_content_type == "IMAGE" {
                    "PROCESSING"
                } else {
                    "UPLOADED"
                },
                None,
                Some(&original_source),
            ));
            append.staged_ids.push(id);
        }
        append
    }

    pub(crate) fn product_media_outcome(
        &mut self,
        invocation: RootInvocation<'_>,
    ) -> ResolverOutcome<Value> {
        if self.config.read_mode == ReadMode::Snapshot {
            return resolver_http_error_outcome(
                400,
                format!(
                    "No mutation dispatcher implemented for root field: {}",
                    invocation.root_name
                ),
            );
        }
        let arguments = resolved_arguments_from_json(&invocation.arguments);
        let payload = match invocation.root_name {
            "productCreateMedia" => {
                self.product_create_media_payload(invocation.request, &arguments)
            }
            "productUpdateMedia" => {
                self.product_update_media_payload(invocation.request, &arguments)
            }
            "productDeleteMedia" => {
                self.product_delete_media_payload(invocation.request, &arguments)
            }
            "productReorderMedia" => {
                self.product_reorder_media_payload(invocation.request, &arguments)
            }
            root => {
                return ResolverOutcome::error(format!(
                    "No product-media resolver implemented for root `{root}`"
                ))
            }
        };
        let Some(payload) = payload else {
            return self.cached_or_forward_upstream_root_outcome(
                invocation.request,
                invocation.response_key,
            );
        };
        let reorder_failed_validation = invocation.root_name == "productReorderMedia"
            && payload
                .get("mediaUserErrors")
                .and_then(Value::as_array)
                .is_some_and(|errors| !errors.is_empty());
        let outcome = ResolverOutcome::value(payload);
        if reorder_failed_validation {
            outcome
        } else {
            outcome.with_log_draft(LogDraft::staged(
                invocation.root_name,
                "products",
                Vec::new(),
            ))
        }
    }

    /// productCreateMedia stages newly uploaded media on a product. Each media
    /// entry is validated independently: an unreachable `originalSource` is
    /// rejected with `Image URL is invalid` while the remaining valid entries
    /// are still created (Shopify reports a partial success). Product existence
    /// is only enforced when no source-level error already rejected the batch,
    /// matching live Admin behaviour where the bad source wins over a missing
    /// product lookup.
    fn product_create_media_payload(
        &mut self,
        request: &Request,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) -> Option<Value> {
        let product_id = resolved_string_field(arguments, "productId")?;
        let media_inputs = resolved_object_list_field(arguments, "media");

        let mut source_errors = Vec::new();
        let mut created = Vec::new();
        let mut staged = Vec::new();
        let mut ready_on_read_ids = Vec::new();
        for (index, item) in media_inputs.iter().enumerate() {
            let original_source = resolved_string_field(item, "originalSource").unwrap_or_default();
            if !media_source_is_valid(&original_source) {
                source_errors.push(user_error_omit_code(
                    vec![
                        "media".to_string(),
                        index.to_string(),
                        "originalSource".to_string(),
                    ],
                    "Image URL is invalid",
                    Some("INVALID"),
                ));
                continue;
            }
            let media_content_type = resolved_string_field(item, "mediaContentType")
                .unwrap_or_else(|| infer_product_media_content_type(&original_source).to_string());
            let id = self.next_proxy_synthetic_gid(product_media_typename(&media_content_type));
            let alt = resolved_string_field(item, "alt").unwrap_or_default();
            created.push(product_media_node_with_type(
                &id,
                &alt,
                &media_content_type,
                "UPLOADED",
                None,
                Some(&original_source),
            ));
            let staged_node = product_media_node_with_type(
                &id,
                &alt,
                &media_content_type,
                if media_content_type == "IMAGE" {
                    "PROCESSING"
                } else {
                    "UPLOADED"
                },
                None,
                Some(&original_source),
            );
            staged.push(staged_node);
            ready_on_read_ids.push(id);
        }

        if source_errors.is_empty() && !self.ensure_product_for_media(request, &product_id) {
            return Some(json!({
                "media": Value::Null,
                "userErrors": [product_does_not_exist_error("productId")],
                "mediaUserErrors": [product_does_not_exist_error("productId")],
                "product": Value::Null,
            }));
        }

        let mut product_media_nodes = self.product_known_media(&product_id);
        product_media_nodes.extend(created.clone());
        if !staged.is_empty() {
            self.append_product_media_nodes(&product_id, staged);
            self.store
                .staged
                .media_ready_on_read
                .extend(ready_on_read_ids);
        }

        let product = self
            .store
            .product_staged_or_base(&product_id)
            .map(|product| {
                let mut value = self.product_canonical_value(&product);
                value["media"] = connection_json(product_media_nodes);
                value
            })
            .unwrap_or(Value::Null);

        Some(json!({
            "media": created.clone(),
            "userErrors": source_errors.clone(),
            "mediaUserErrors": source_errors,
            "product": product,
        }))
    }

    /// productUpdateMedia edits existing media in place. A missing product or any
    /// unknown media id rejects the whole batch without a write; otherwise each
    /// referenced media's caption is updated and its asset is marked `READY`.
    fn product_update_media_payload(
        &mut self,
        request: &Request,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) -> Option<Value> {
        let product_id = resolved_string_field(arguments, "productId")?;
        let media_inputs = resolved_object_list_field(arguments, "media");

        if !self.ensure_product_for_media(request, &product_id) {
            return Some(json!({
                "media": Value::Null,
                "userErrors": [product_does_not_exist_error("productId")],
                "mediaUserErrors": [product_does_not_exist_error("productId")],
            }));
        }

        let mut overlay = self.product_known_media(&product_id);
        let missing_media_ids: Vec<String> = media_inputs
            .iter()
            .filter_map(|item| resolved_string_field(item, "id"))
            .filter(|id| !media_nodes_contain(&overlay, id))
            .collect();
        if !missing_media_ids.is_empty() {
            let error = media_missing_ids_error("media", &missing_media_ids);
            return Some(json!({
                "media": Value::Null,
                "userErrors": [error.clone()],
                "mediaUserErrors": [error],
            }));
        }

        let mut updated = Vec::new();
        for item in &media_inputs {
            let Some(id) = resolved_string_field(item, "id") else {
                continue;
            };
            let alt = resolved_string_field(item, "alt");
            for node in overlay.iter_mut() {
                if node.get("id").and_then(Value::as_str) != Some(id.as_str()) {
                    continue;
                }
                if let Some(alt) = &alt {
                    node["alt"] = json!(alt);
                }
                promote_product_media_node_to_ready(node);
                updated.push(node.clone());
                self.store.staged.media_ready_on_read.remove(&id);
            }
        }

        self.stage_product_media_nodes(&product_id, overlay);
        Some(json!({
            "media": updated,
            "userErrors": [],
            "mediaUserErrors": []
        }))
    }

    /// productDeleteMedia removes media from a product. A missing product or any
    /// unknown media id rejects the whole batch without a write; otherwise the
    /// referenced media are removed and their backing ProductImage ids are
    /// derived from the observed assets.
    fn product_delete_media_payload(
        &mut self,
        request: &Request,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) -> Option<Value> {
        let product_id = resolved_string_field(arguments, "productId")?;
        let media_ids = list_string_field(arguments, "mediaIds");

        if !self.ensure_product_for_media(request, &product_id) {
            return Some(json!({
                "deletedMediaIds": Value::Null,
                "deletedProductImageIds": Value::Null,
                "userErrors": [product_does_not_exist_error("productId")],
                "mediaUserErrors": [product_does_not_exist_error("productId")],
                "product": Value::Null,
            }));
        }

        let known = self.product_known_media(&product_id);
        let missing_media_ids: Vec<String> = media_ids
            .iter()
            .filter(|id| !media_nodes_contain(&known, id))
            .cloned()
            .collect();
        if !missing_media_ids.is_empty() {
            let error = media_missing_ids_error("mediaIds", &missing_media_ids);
            return Some(json!({
                "deletedMediaIds": Value::Null,
                "deletedProductImageIds": Value::Null,
                "userErrors": [error.clone()],
                "mediaUserErrors": [error],
                "product": Value::Null,
            }));
        }

        let deleted_product_image_ids: Vec<Value> = media_ids
            .iter()
            .filter_map(|id| {
                known
                    .iter()
                    .find(|node| node.get("id").and_then(Value::as_str) == Some(id.as_str()))
                    .and_then(product_image_id_from_media)
                    .map(|product_image_id| json!(product_image_id))
            })
            .collect();

        let remaining: Vec<Value> = known
            .into_iter()
            .filter(|node| {
                let id = node.get("id").and_then(Value::as_str).unwrap_or_default();
                !media_ids.iter().any(|deleted| deleted == id)
            })
            .collect();
        for id in &media_ids {
            self.store.staged.media_ready_on_read.remove(id);
        }
        self.stage_product_media_nodes(&product_id, remaining.clone());
        let product = self
            .store
            .product_staged_or_base(&product_id)
            .map(|product| self.product_canonical_value(&product))
            .unwrap_or(Value::Null);

        Some(json!({
            "deletedMediaIds": media_ids,
            "deletedProductImageIds": deleted_product_image_ids,
            "userErrors": [],
            "mediaUserErrors": [],
            "product": product,
        }))
    }

    fn product_reorder_media_payload(
        &mut self,
        request: &Request,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) -> Option<Value> {
        let product_id = resolved_string_field(arguments, "id")?;
        let moves = resolved_object_list_field(arguments, "moves");

        // Reorder operates on media that already exists on the product. If the
        // product has not been staged locally yet, hydrate it from upstream so
        // existing media (and their alt text) are observed rather than guessed.
        if !self.ensure_product_for_media(request, &product_id) {
            return Some(product_media_user_errors_payload(
                ["id"],
                "Product does not exist",
                "PRODUCT_DOES_NOT_EXIST",
            ));
        }

        let mut media = self.product_known_media(&product_id);
        for media_move in moves {
            let Some(id) = resolved_string_field(&media_move, "id") else {
                continue;
            };
            let new_position = resolved_string_field(&media_move, "newPosition")
                .and_then(|position| position.parse::<usize>().ok())
                .or_else(|| {
                    resolved_int_field(&media_move, "newPosition")
                        .map(|position| position.max(0) as usize)
                })
                .unwrap_or(0);
            let Some(current_position) = media
                .iter()
                .position(|node| node.get("id").and_then(Value::as_str) == Some(id.as_str()))
            else {
                continue;
            };
            let node = media.remove(current_position);
            media.insert(new_position.min(media.len()), node);
            self.store.staged.media_ready_on_read.remove(&id);
        }
        self.stage_product_media_nodes(&product_id, media);
        Some(json!({
            "job": {
                "id": self.next_proxy_synthetic_gid("Job"),
                "done": false
            },
            "userErrors": [],
            "mediaUserErrors": []
        }))
    }

    fn stage_product_media_nodes(&mut self, product_id: &str, media: Vec<Value>) {
        let timestamp = default_product_timestamp();
        let mut product = self
            .store
            .product_staged_or_base(product_id)
            .unwrap_or_else(|| ProductRecord {
                id: product_id.to_string(),
                created_at: timestamp.clone(),
                updated_at: timestamp,
                ..ProductRecord::default()
            });
        product.media = media;
        self.store.stage_product(product);
    }

    /// Append newly created media nodes to a product's observed media, keeping
    /// any media already staged/hydrated for the product.
    fn append_product_media_nodes(&mut self, product_id: &str, mut nodes: Vec<Value>) {
        let mut media = self.product_known_media(product_id);
        media.append(&mut nodes);
        self.stage_product_media_nodes(product_id, media);
    }

    /// Observed media nodes for a product, drawn from the staged/base overlay.
    fn product_known_media(&self, product_id: &str) -> Vec<Value> {
        self.store
            .product_staged_or_base(product_id)
            .map(|product| product.media)
            .unwrap_or_default()
    }

    pub(in crate::proxy) fn promote_all_product_media_ready_on_read(&mut self) {
        let ready_on_read_ids = self.store.staged.media_ready_on_read.clone();
        let product_ids = self
            .store
            .products
            .staged
            .iter()
            .map(|(id, _)| id.clone())
            .collect::<Vec<_>>();
        for product_id in product_ids {
            let Some(product) = self.store.products.staged.get_mut(&product_id) else {
                continue;
            };
            let mut promoted = Vec::new();
            for node in &mut product.media {
                let Some(id) = node.get("id").and_then(Value::as_str).map(str::to_string) else {
                    continue;
                };
                if !ready_on_read_ids.contains(&id) {
                    continue;
                }
                promote_product_media_node_to_ready(node);
                promoted.push(id);
            }
            for id in promoted {
                self.store.staged.media_ready_on_read.remove(&id);
            }
        }
    }

    /// Confirm a product exists, hydrating it from upstream when it has no
    /// overlay yet. Returns true when an overlay is present afterwards — a
    /// hydration that observes no node leaves the product absent, which the
    /// media mutations surface as `Product does not exist`.
    fn ensure_product_for_media(&mut self, request: &Request, product_id: &str) -> bool {
        if self.store.product_staged_or_base(product_id).is_some() {
            return true;
        }
        self.hydrate_product_nodes_for_observation_with_request(
            request,
            vec![product_id.to_string()],
        );
        self.store.product_staged_or_base(product_id).is_some()
    }
}

#[derive(Default)]
pub(in crate::proxy) struct ProductTopLevelMediaAppend {
    pub(in crate::proxy) mutation_nodes: Vec<Value>,
    pub(in crate::proxy) staged_nodes: Vec<Value>,
    pub(in crate::proxy) staged_ids: Vec<String>,
}

pub(in crate::proxy) fn product_top_level_media_inputs(
    arguments: &BTreeMap<String, ResolvedValue>,
) -> Option<Vec<BTreeMap<String, ResolvedValue>>> {
    match arguments.get("media") {
        Some(ResolvedValue::List(items)) => Some(
            items
                .iter()
                .filter_map(|item| match item {
                    ResolvedValue::Object(fields) => Some(fields.clone()),
                    _ => None,
                })
                .collect(),
        ),
        Some(ResolvedValue::Null) | None => None,
        _ => Some(Vec::new()),
    }
}

pub(in crate::proxy) fn product_top_level_media_user_errors(
    media_inputs: &[BTreeMap<String, ResolvedValue>],
) -> Vec<Value> {
    media_inputs
        .iter()
        .enumerate()
        .filter_map(|(index, item)| {
            let original_source = resolved_string_field(item, "originalSource").unwrap_or_default();
            if media_source_is_valid(&original_source) {
                return None;
            }
            Some(user_error_omit_code(
                vec![
                    "media".to_string(),
                    index.to_string(),
                    "originalSource".to_string(),
                ],
                "Image URL is invalid",
                None,
            ))
        })
        .collect()
}

fn product_media_node_with_type(
    id: &str,
    alt: &str,
    media_content_type: &str,
    status: &str,
    image_url: Option<&str>,
    original_source: Option<&str>,
) -> Value {
    let image = image_url
        .map(|url| json!({ "url": url }))
        .unwrap_or(Value::Null);
    let typename = product_media_typename(media_content_type);
    let mut node = json!({
        "__typename": typename,
        "id": id,
        "alt": alt,
        "mediaContentType": media_content_type,
        "status": status,
        "preview": {
            "image": image.clone()
        }
    });
    if media_content_type == "IMAGE" {
        node["image"] = image;
        if let Some(source) = original_source {
            node["originalSource"] = json!({ "url": source });
        }
    } else if media_content_type == "EXTERNAL_VIDEO" {
        if let Some(source) = original_source {
            let (origin_url, embed_url) = external_video_urls(source);
            node["originUrl"] = json!(origin_url);
            if let Some(embed_url) = embed_url {
                node["embedUrl"] = json!(embed_url);
            }
        }
    } else if matches!(media_content_type, "VIDEO" | "MODEL_3D") {
        if let Some(source) = original_source {
            node["originalSource"] = json!({ "url": source });
            node["sources"] = json!([{ "url": source }]);
        }
    }
    node
}

fn external_video_urls(source: &str) -> (String, Option<String>) {
    if let Some(video_id) = youtube_video_id(source) {
        return (
            format!("https://youtu.be/{video_id}"),
            Some(format!("https://www.youtube.com/embed/{video_id}")),
        );
    }
    (source.to_string(), None)
}

fn youtube_video_id(source: &str) -> Option<String> {
    if let Some(tail) = source.split("youtu.be/").nth(1) {
        return video_id_token(tail);
    }
    let query = source.split_once('?')?.1;
    query.split('&').find_map(|part| {
        let (key, value) = part.split_once('=')?;
        (key == "v").then(|| video_id_token(value)).flatten()
    })
}

fn video_id_token(raw: &str) -> Option<String> {
    let token = raw
        .split(['?', '&', '#', '/'])
        .next()
        .unwrap_or_default()
        .trim();
    (!token.is_empty()).then(|| token.to_string())
}

fn product_media_typename(media_content_type: &str) -> &'static str {
    match media_content_type {
        "EXTERNAL_VIDEO" => "ExternalVideo",
        "MODEL_3D" => "Model3d",
        "VIDEO" => "Video",
        _ => "MediaImage",
    }
}

fn product_media_ready_url(node: &Value) -> String {
    product_media_image_url(node)
        .map(str::to_string)
        .unwrap_or_else(|| product_media_local_ready_url(node))
}

fn promote_product_media_node_to_ready(node: &mut Value) {
    let ready_url = product_media_ready_url(node);
    node["status"] = json!("READY");
    node["preview"] = json!({ "image": product_media_preview_image_json(node, &ready_url) });
    if node.get("mediaContentType").and_then(Value::as_str) == Some("IMAGE") {
        node["image"] = product_media_image_json(node, &ready_url);
    }
}

fn product_media_preview_image_json(node: &Value, ready_url: &str) -> Value {
    let mut image = serde_json::Map::new();
    image.insert("url".to_string(), json!(ready_url));
    if let Some(width) = product_media_image_dimension(node, "width", 0) {
        image.insert("width".to_string(), json!(width));
    }
    if let Some(height) = product_media_image_dimension(node, "height", 1) {
        image.insert("height".to_string(), json!(height));
    }
    Value::Object(image)
}

fn product_media_image_json(node: &Value, ready_url: &str) -> Value {
    let mut image = serde_json::Map::new();
    if let Some(id) = product_media_image_source_id_from_media(node) {
        image.insert("id".to_string(), json!(id));
    }
    image.insert("url".to_string(), json!(ready_url));
    if let Some(alt_text) = node.get("alt").cloned().or_else(|| {
        node.get("image")
            .and_then(|image| image.get("altText"))
            .cloned()
    }) {
        image.insert("altText".to_string(), alt_text);
    }
    if let Some(width) = product_media_image_dimension(node, "width", 0) {
        image.insert("width".to_string(), json!(width));
    }
    if let Some(height) = product_media_image_dimension(node, "height", 1) {
        image.insert("height".to_string(), json!(height));
    }
    Value::Object(image)
}

fn product_media_image_source_id_from_media(media: &Value) -> Option<String> {
    media
        .get("image")
        .and_then(|image| image.get("id"))
        .and_then(Value::as_str)
        .filter(|id| shopify_gid_resource_type(id) != Some("ProductImage"))
        .map(str::to_string)
        .or_else(|| {
            media
                .get("id")
                .and_then(Value::as_str)
                .map(|id| shopify_gid("ImageSource", resource_id_tail(id)))
        })
}

fn product_media_image_dimension(media: &Value, field: &str, index: usize) -> Option<i64> {
    media
        .get("image")
        .and_then(|image| image.get(field))
        .and_then(Value::as_i64)
        .or_else(|| {
            media
                .get("preview")
                .and_then(|preview| preview.get("image"))
                .and_then(|image| image.get(field))
                .and_then(Value::as_i64)
        })
        .or_else(|| {
            product_media_original_source_url(media)
                .and_then(product_media_dimensions_from_source)
                .map(|dimensions| {
                    if index == 0 {
                        dimensions.0
                    } else {
                        dimensions.1
                    }
                })
        })
}

fn product_media_dimensions_from_source(source: &str) -> Option<(i64, i64)> {
    source.split(['/', '?', '&']).find_map(|part| {
        let (width, height) = part.split_once('x')?;
        let width = width.parse::<i64>().ok()?;
        let height = height.parse::<i64>().ok()?;
        (width > 0 && height > 0).then_some((width, height))
    })
}

fn product_media_image_url(node: &Value) -> Option<&str> {
    node.get("image")
        .and_then(|image| image.get("url"))
        .and_then(Value::as_str)
        .filter(|url| !url.is_empty())
        .or_else(|| {
            node.get("preview")
                .and_then(|preview| preview.get("image"))
                .and_then(|image| image.get("url"))
                .and_then(Value::as_str)
                .filter(|url| !url.is_empty())
        })
}

fn product_media_original_source_url(node: &Value) -> Option<&str> {
    node.get("originalSource")
        .and_then(|source| {
            source
                .get("url")
                .and_then(Value::as_str)
                .or_else(|| source.as_str())
        })
        .filter(|url| !url.is_empty())
}

fn product_media_local_ready_url(node: &Value) -> String {
    let id = node.get("id").and_then(Value::as_str).unwrap_or("media");
    let resource_type = shopify_gid_resource_type(id).unwrap_or("Media");
    let tail = resource_id_tail(id);
    let token = product_media_url_token(&format!("{resource_type}-{tail}"));
    let extension = product_media_original_source_url(node)
        .map(file_extension)
        .map(|extension| extension.to_ascii_lowercase())
        .filter(|extension| !extension.is_empty() && extension.chars().all(token_char))
        .unwrap_or_else(|| "png".to_string());
    format!("https://shopify-draft-proxy.local/media/{token}.{extension}")
}

fn product_media_url_token(value: &str) -> String {
    let token: String = value
        .chars()
        .map(|ch| if token_char(ch) { ch } else { '-' })
        .collect();
    let token = token.trim_matches('-');
    if token.is_empty() {
        "media".to_string()
    } else {
        token.to_ascii_lowercase()
    }
}

fn infer_product_media_content_type(original_source: &str) -> &'static str {
    if product_media_source_is_external_video(original_source) {
        return "EXTERNAL_VIDEO";
    }
    match file_extension(original_source)
        .to_ascii_lowercase()
        .as_str()
    {
        "mp4" | "mov" | "m4v" | "webm" => "VIDEO",
        "glb" | "gltf" | "usdz" => "MODEL_3D",
        _ => "IMAGE",
    }
}

fn product_media_source_is_external_video(original_source: &str) -> bool {
    let source = original_source.to_ascii_lowercase();
    source.contains("youtube.com/") || source.contains("youtu.be/") || source.contains("vimeo.com/")
}

fn product_media_user_errors_payload(
    field: impl Into<UserErrorField>,
    message: &str,
    code: &str,
) -> Value {
    let errors = json!([user_error_omit_code(field, message, Some(code))]);
    json!({
        "userErrors": errors.clone(),
        "mediaUserErrors": errors
    })
}

/// Media originalSource is reachable only when it is an http(s) URL; anything
/// else (e.g. the literal `not-a-url`) is rejected as an invalid image URL.
fn media_source_is_valid(original_source: &str) -> bool {
    original_source.starts_with("http://") || original_source.starts_with("https://")
}

/// True when `media` contains a node whose id equals `id`.
fn media_nodes_contain(media: &[Value], id: &str) -> bool {
    media
        .iter()
        .any(|node| node.get("id").and_then(Value::as_str) == Some(id))
}

fn product_does_not_exist_error(field: &str) -> Value {
    user_error_omit_code(
        [field],
        "Product does not exist",
        Some("PRODUCT_DOES_NOT_EXIST"),
    )
}

fn media_missing_ids_error(field: &str, ids: &[String]) -> Value {
    let joined_ids = ids.join(",");
    let message = if ids.len() == 1 {
        format!("Media id {joined_ids} does not exist")
    } else {
        format!("Media ids {joined_ids} do not exist")
    };
    user_error_omit_code([field], &message, Some("MEDIA_DOES_NOT_EXIST"))
}

pub(in crate::proxy) fn default_product_timestamp() -> String {
    "2024-01-01T00:00:00.000Z".to_string()
}

pub(in crate::proxy) fn product_mutation_timestamp(ordinal: u64) -> String {
    format!("2024-01-01T00:00:{:02}.000Z", (ordinal + 1) % 60)
}

pub(in crate::proxy) fn product_next_updated_at(current: &str, ordinal: u64) -> String {
    let candidate = product_mutation_timestamp(ordinal);
    if candidate.as_str() > current {
        candidate
    } else {
        current.to_string()
    }
}

impl DraftProxy {
    pub(in crate::proxy) fn next_product_timestamp(&self) -> String {
        product_mutation_timestamp(self.mutation_log_ordinal() as u64)
    }

    pub(in crate::proxy) fn next_product_updated_at(&self, current: &str) -> String {
        product_next_updated_at(current, self.mutation_log_ordinal() as u64)
    }
}

pub(in crate::proxy) fn product_root_fields_select_shop_currency_money(
    fields: &[RootFieldSelection],
) -> bool {
    fields
        .iter()
        .any(product_root_field_selects_shop_currency_money)
}

fn product_root_field_selects_shop_currency_money(field: &RootFieldSelection) -> bool {
    if product_selections_include_names(&field.selection, &["priceRange", "priceRangeV2"]) {
        return true;
    }
    if !product_selections_include_names(&field.selection, &["adjustmentValue", "summary"]) {
        return false;
    }
    if matches!(
        field.name.as_str(),
        "sellingPlanGroupCreate" | "sellingPlanGroupUpdate"
    ) {
        return resolved_value_contains_field(
            &ResolvedValue::Object(field.arguments.clone()),
            "fixedValue",
        );
    }
    true
}

fn product_selections_include_names(selections: &[SelectedField], names: &[&str]) -> bool {
    selections.iter().any(|selection| {
        names.iter().any(|field_name| selection.name == *field_name)
            || product_selections_include_names(&selection.selection, names)
    })
}

#[derive(Clone, Copy)]
enum ProductPriceRangeKind {
    Current,
    Legacy,
}

fn product_price_range_value(
    product: &ProductRecord,
    variants: &[ProductVariantRecord],
    currency_code: &str,
    kind: ProductPriceRangeKind,
) -> Value {
    if !variants.is_empty() {
        if let Some((min_price, max_price)) = product_variant_price_bounds(variants) {
            return computed_product_price_range_value(min_price, max_price, currency_code, kind);
        }
    }

    let observed_field = match kind {
        ProductPriceRangeKind::Current => "priceRangeV2",
        ProductPriceRangeKind::Legacy => "priceRange",
    };
    if let Some(observed) = product.extra_fields.get(observed_field) {
        return observed.clone();
    }

    if let Some((min_price, max_price)) = product_raw_variant_price_bounds(&product.variants) {
        return computed_product_price_range_value(min_price, max_price, currency_code, kind);
    }

    computed_product_price_range_value(0.0, 0.0, currency_code, kind)
}

fn product_variant_price_bounds(variants: &[ProductVariantRecord]) -> Option<(f64, f64)> {
    price_bounds(
        variants
            .iter()
            .filter_map(|variant| parse_product_price(&variant.price)),
    )
}

fn product_raw_variant_price_bounds(variants: &[Value]) -> Option<(f64, f64)> {
    price_bounds(variants.iter().filter_map(|variant| {
        variant
            .get("price")
            .and_then(Value::as_str)
            .and_then(parse_product_price)
    }))
}

fn product_variants_count_value(
    product: &ProductRecord,
    variants: &[ProductVariantRecord],
) -> Value {
    if !variants.is_empty() {
        return count_object(variants.len());
    }

    product
        .extra_fields
        .get("variantsCount")
        .cloned()
        .unwrap_or_else(|| count_object(product.variants.len()))
}

fn product_compare_at_price_range_value(
    product: &ProductRecord,
    variants: &[ProductVariantRecord],
    currency_code: &str,
) -> Value {
    if !variants.is_empty() {
        return product_variant_compare_at_price_bounds(variants)
            .map(|(min_price, max_price)| {
                computed_product_compare_at_price_range_value(min_price, max_price, currency_code)
            })
            .unwrap_or(Value::Null);
    }

    if let Some(observed) = product.extra_fields.get("compareAtPriceRange") {
        return observed.clone();
    }

    product_raw_variant_compare_at_price_bounds(&product.variants)
        .map(|(min_price, max_price)| {
            computed_product_compare_at_price_range_value(min_price, max_price, currency_code)
        })
        .unwrap_or(Value::Null)
}

fn product_variant_compare_at_price_bounds(
    variants: &[ProductVariantRecord],
) -> Option<(f64, f64)> {
    price_bounds(variants.iter().filter_map(|variant| {
        variant
            .compare_at_price
            .as_deref()
            .and_then(parse_product_price)
    }))
}

fn product_raw_variant_compare_at_price_bounds(variants: &[Value]) -> Option<(f64, f64)> {
    price_bounds(variants.iter().filter_map(|variant| {
        variant
            .get("compareAtPrice")
            .and_then(Value::as_str)
            .and_then(parse_product_price)
    }))
}

fn price_bounds<I>(prices: I) -> Option<(f64, f64)>
where
    I: IntoIterator<Item = f64>,
{
    let mut iter = prices.into_iter();
    let first = iter.next()?;
    let mut min_price = first;
    let mut max_price = first;
    for price in iter {
        if price < min_price {
            min_price = price;
        }
        if price > max_price {
            max_price = price;
        }
    }
    Some((min_price, max_price))
}

fn parse_product_price(price: impl AsRef<str>) -> Option<f64> {
    price.as_ref().trim().parse::<f64>().ok()
}

fn computed_product_price_range_value(
    min_price: f64,
    max_price: f64,
    currency_code: &str,
    kind: ProductPriceRangeKind,
) -> Value {
    json!({
        "__typename": match kind {
            ProductPriceRangeKind::Current => "ProductPriceRangeV2",
            ProductPriceRangeKind::Legacy => "ProductPriceRange",
        },
        "minVariantPrice": product_price_range_money(min_price, currency_code, kind),
        "maxVariantPrice": product_price_range_money(max_price, currency_code, kind),
    })
}

fn computed_product_compare_at_price_range_value(
    min_price: f64,
    max_price: f64,
    currency_code: &str,
) -> Value {
    json!({
        "__typename": "ProductCompareAtPriceRange",
        "minVariantCompareAtPrice": product_price_range_money(
            min_price,
            currency_code,
            ProductPriceRangeKind::Current,
        ),
        "maxVariantCompareAtPrice": product_price_range_money(
            max_price,
            currency_code,
            ProductPriceRangeKind::Current,
        ),
    })
}

fn product_price_range_money(
    price: f64,
    currency_code: &str,
    kind: ProductPriceRangeKind,
) -> Value {
    let amount = match kind {
        ProductPriceRangeKind::Current => price,
        ProductPriceRangeKind::Legacy => price * 100.0,
    };
    json!({
        "__typename": "MoneyV2",
        "amount": normalize_money_amount(&format!("{amount:.2}")),
        "currencyCode": currency_code
    })
}

fn product_collections_connection_value(
    product: &ProductRecord,
    arguments: &BTreeMap<String, ResolvedValue>,
) -> Value {
    let collections =
        sorted_product_collection_nodes_for_connection(product.collections.clone(), arguments);
    connection_value_with_args(collections, arguments, value_id_cursor)
}

fn sorted_product_collection_nodes_for_connection(
    collections: Vec<Value>,
    arguments: &BTreeMap<String, ResolvedValue>,
) -> Vec<Value> {
    let sort_key_name = resolved_string_field(arguments, "sortKey");
    sorted_indexed_records(
        collections,
        false,
        |collection, index| {
            product_collection_sort_key(collection, sort_key_name.as_deref(), index)
        },
        value_id_cursor,
    )
}

fn product_collection_sort_key(
    collection: &Value,
    sort_key: Option<&str>,
    index: usize,
) -> StagedSortKey {
    match sort_key {
        Some("ID") | Some("RELEVANCE") => value_gid_sort_key(collection),
        Some("TITLE") => value_string_field_sort_key(collection, "title"),
        Some("UPDATED_AT") => value_string_field_sort_key(collection, "updatedAt"),
        _ => vec![StagedSortValue::I64(index as i64)],
    }
}

/// `Product.hasOnlyDefaultVariant` is true exactly when the product has a single variant
/// carrying Shopify's implicit default option (`Title: Default Title`).
pub(in crate::proxy) fn product_has_only_default_variant(
    variants: &[ProductVariantRecord],
) -> bool {
    match variants {
        [variant] => {
            variant.selected_options.len() == 1
                && variant.selected_options[0].name == "Title"
                && variant.selected_options[0].value == "Default Title"
        }
        _ => false,
    }
}

/// `Product.hasOutOfStockVariants` is true when any inventory-tracked variant has a
/// non-positive available quantity. `inventory_quantity` mirrors the variant's total
/// available stock (kept in sync by the inventory mutation handlers), so it is the
/// available figure to test; untracked variants never count as out of stock.
pub(in crate::proxy) fn product_has_out_of_stock_variants(
    variants: &[ProductVariantRecord],
) -> bool {
    variants
        .iter()
        .filter(|variant| variant.inventory_item.tracked)
        .any(|variant| variant.inventory_quantity <= 0)
}

fn product_variant_connection_with_fallback_value(
    variants: &[ProductVariantRecord],
    fallback_variants: &[Value],
    arguments: &BTreeMap<String, ResolvedValue>,
) -> Value {
    // The embedded product connection owns the observed order. A separately
    // hydrated/staged variant is an overlay for its existing slot, not a new
    // record to move to the end of the connection.
    let mut sort_arguments = arguments.clone();
    sort_arguments.remove("reverse");
    let variants =
        sorted_product_variant_records_for_connection(variants.to_vec(), &sort_arguments);
    let variants_by_id = variants
        .iter()
        .map(|variant| (variant.id.as_str(), variant))
        .collect::<BTreeMap<_, _>>();
    let fallback_ids = fallback_variants
        .iter()
        .filter_map(|variant| variant.get("id").and_then(Value::as_str))
        .collect::<BTreeSet<_>>();
    let typed_order_is_authoritative = !fallback_ids.is_empty()
        && variants
            .iter()
            .all(|variant| product_variant_position(variant).is_some())
        && fallback_ids
            .iter()
            .all(|id| variants_by_id.contains_key(*id));
    let mut nodes: Vec<Value> = if typed_order_is_authoritative {
        variants.iter().map(product_variant_state_json).collect()
    } else {
        fallback_variants
            .iter()
            .map(|fallback| {
                fallback
                    .get("id")
                    .and_then(Value::as_str)
                    .and_then(|id| variants_by_id.get(id))
                    .map(|variant| product_variant_state_json(variant))
                    .unwrap_or_else(|| fallback.clone())
            })
            .collect()
    };
    if !typed_order_is_authoritative {
        nodes.extend(
            variants
                .iter()
                .filter(|variant| !fallback_ids.contains(variant.id.as_str()))
                .map(product_variant_state_json),
        );
    }
    connection_value_with_args(nodes, arguments, value_id_cursor)
}

fn sorted_product_variant_records_for_connection(
    variants: Vec<ProductVariantRecord>,
    arguments: &BTreeMap<String, ResolvedValue>,
) -> Vec<ProductVariantRecord> {
    let sort_key_name = resolved_string_field(arguments, "sortKey");
    if sort_key_name.as_deref() == Some("INVENTORY_LEVELS_AVAILABLE") {
        return Vec::new();
    }
    sorted_indexed_records(
        variants,
        resolved_bool_field(arguments, "reverse").unwrap_or(false),
        |variant, index| {
            product_variant_connection_sort_key(variant, sort_key_name.as_deref(), index)
        },
        |variant| variant.id.clone(),
    )
}

fn product_variant_connection_sort_key(
    variant: &ProductVariantRecord,
    sort_key: Option<&str>,
    index: usize,
) -> StagedSortKey {
    match sort_key {
        Some("ID") => gid_string_sort_key(&variant.id),
        Some("INVENTORY_QUANTITY") => vec![StagedSortValue::I64(variant.inventory_quantity)],
        Some("INVENTORY_MANAGEMENT") => {
            let mut key = vec![StagedSortValue::I64(variant.inventory_item.tracked as i64)];
            key.extend(gid_string_sort_key(&variant.id));
            key
        }
        Some("INVENTORY_POLICY") => {
            let mut key = vec![sort_string_value(&variant.inventory_policy)];
            key.extend(gid_string_sort_key(&variant.id));
            key
        }
        Some("NAME") | Some("TITLE") => vec![sort_string_value(&variant.title)],
        Some("SKU") => vec![sort_string_value(&variant.sku)],
        Some("FULL_TITLE") | Some("POPULAR") | Some("POSITION") | Some("RELEVANCE") => {
            vec![StagedSortValue::I64(
                product_variant_position(variant).unwrap_or(index as i64),
            )]
        }
        _ => vec![StagedSortValue::I64(index as i64)],
    }
}

/// Resolve a variant's attached `media_ids` against its owning product's media
/// library, preserving attachment order. Falls back to any media nodes stashed
/// in `extra_fields` when the product (library) is not available in this render
/// context.
pub(in crate::proxy) fn variant_attached_media_nodes(
    variant: &ProductVariantRecord,
    product: Option<&ProductRecord>,
) -> Vec<Value> {
    match product {
        Some(product) => variant
            .media_ids
            .iter()
            .filter_map(|media_id| {
                product
                    .media
                    .iter()
                    .find(|node| node.get("id").and_then(Value::as_str) == Some(media_id.as_str()))
                    .cloned()
            })
            .collect(),
        None => Vec::new(),
    }
}

fn product_media_connection_value(
    media: Vec<Value>,
    arguments: &BTreeMap<String, ResolvedValue>,
) -> Value {
    let media = sorted_product_media_nodes_for_connection(media, arguments);
    connection_value_with_args(media, arguments, value_id_cursor)
}

fn sorted_product_media_nodes_for_connection(
    media: Vec<Value>,
    arguments: &BTreeMap<String, ResolvedValue>,
) -> Vec<Value> {
    let sort_key_name = resolved_string_field(arguments, "sortKey");
    sorted_indexed_records(
        media,
        false,
        |media, index| product_media_sort_key(media, sort_key_name.as_deref(), index),
        value_id_cursor,
    )
}

fn product_image_connection_value(
    media: Vec<Value>,
    arguments: &BTreeMap<String, ResolvedValue>,
) -> Value {
    let images = sorted_product_media_nodes_for_connection(media, arguments)
        .iter()
        .filter_map(product_image_json_from_media)
        .collect::<Vec<_>>();
    connection_value_with_args(images, arguments, value_id_cursor)
}

pub(in crate::proxy) fn product_image_json_from_media(media: &Value) -> Option<Value> {
    if !product_media_is_image(media) {
        return None;
    }
    let url = product_media_image_url(media)?;
    let id = product_image_id_from_media(media)?;
    let image = media.get("image");
    let preview_image = media
        .get("preview")
        .and_then(|preview| preview.get("image"));
    let alt_text = media
        .get("alt")
        .cloned()
        .or_else(|| image.and_then(|image| image.get("altText")).cloned())
        .unwrap_or(Value::Null);
    let width = image
        .and_then(|image| image.get("width"))
        .or_else(|| preview_image.and_then(|image| image.get("width")))
        .cloned()
        .unwrap_or(Value::Null);
    let height = image
        .and_then(|image| image.get("height"))
        .or_else(|| preview_image.and_then(|image| image.get("height")))
        .cloned()
        .unwrap_or(Value::Null);
    Some(json!({
        "__typename": "Image",
        "id": id,
        "url": url,
        "src": url,
        "originalSrc": url,
        "transformedSrc": url,
        "altText": alt_text,
        "width": width,
        "height": height
    }))
}

fn product_media_is_image(media: &Value) -> bool {
    media.get("mediaContentType").and_then(Value::as_str) == Some("IMAGE")
        || media.get("__typename").and_then(Value::as_str) == Some("MediaImage")
        || media
            .get("id")
            .and_then(Value::as_str)
            .and_then(shopify_gid_resource_type)
            == Some("MediaImage")
        || media.get("image").is_some()
}

fn product_image_id_from_media(media: &Value) -> Option<String> {
    media
        .get("image")
        .and_then(|image| image.get("id"))
        .and_then(Value::as_str)
        .filter(|id| shopify_gid_resource_type(id) == Some("ProductImage"))
        .map(str::to_string)
        .or_else(|| {
            media
                .get("id")
                .and_then(Value::as_str)
                .map(|id| shopify_gid("ProductImage", resource_id_tail(id)))
        })
}

fn product_media_sort_key(media: &Value, sort_key: Option<&str>, index: usize) -> StagedSortKey {
    match sort_key {
        Some("ID") => value_gid_sort_key(media),
        Some("POSITION") | Some("RELEVANCE") | None => vec![StagedSortValue::I64(index as i64)],
        _ => vec![StagedSortValue::I64(index as i64)],
    }
}

fn value_gid_sort_key(value: &Value) -> StagedSortKey {
    value
        .get("id")
        .and_then(Value::as_str)
        .map_or_else(|| vec![StagedSortValue::Null], gid_string_sort_key)
}

fn gid_string_sort_key(id: &str) -> StagedSortKey {
    match resource_id_tail(id).parse::<i64>() {
        Ok(tail) => vec![StagedSortValue::I64(tail)],
        Err(_) => vec![sort_string_value(id)],
    }
}

fn value_string_field_sort_key(value: &Value, field: &str) -> StagedSortKey {
    value.get(field).and_then(Value::as_str).map_or_else(
        || vec![StagedSortValue::Null],
        |field_value| vec![sort_string_value(field_value)],
    )
}

fn sort_string_value(value: impl AsRef<str>) -> StagedSortValue {
    StagedSortValue::String(value.as_ref().to_ascii_lowercase())
}

pub(in crate::proxy) fn product_variant_state_from_observed_json(
    value: &Value,
) -> Option<ProductVariantRecord> {
    let product_id = value
        .get("productId")
        .and_then(Value::as_str)
        .or_else(|| {
            value
                .get("product")
                .and_then(|product| product.get("id"))
                .and_then(Value::as_str)
        })?
        .to_string();
    product_variant_state_from_json_parts(
        value,
        product_id,
        ProductVariantInventoryItemMode::Optional,
        &[
            "id",
            "productId",
            "product",
            "title",
            "sku",
            "barcode",
            "price",
            "compareAtPrice",
            "taxable",
            "inventoryPolicy",
            "inventoryQuantity",
            "selectedOptions",
            "inventoryItem",
        ],
    )
}

#[derive(Clone, Copy)]
enum ProductVariantInventoryItemMode {
    Optional,
    Required,
}

fn product_variant_state_from_json_parts(
    value: &Value,
    product_id: String,
    inventory_item_mode: ProductVariantInventoryItemMode,
    extra_field_exclusions: &[&str],
) -> Option<ProductVariantRecord> {
    let id = value.get("id")?.as_str()?.to_string();
    let inventory_item = value.get("inventoryItem");
    let inventory_item_id = match inventory_item_mode {
        ProductVariantInventoryItemMode::Optional => inventory_item
            .and_then(|item| item.get("id"))
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_default(),
        ProductVariantInventoryItemMode::Required => {
            inventory_item?.get("id")?.as_str()?.to_string()
        }
    };
    Some(ProductVariantRecord {
        id,
        product_id,
        title: value
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        sku: value
            .get("sku")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        barcode: value
            .get("barcode")
            .and_then(Value::as_str)
            .map(str::to_string),
        price: value
            .get("price")
            .and_then(Value::as_str)
            .unwrap_or("0.00")
            .to_string(),
        compare_at_price: value
            .get("compareAtPrice")
            .and_then(Value::as_str)
            .map(str::to_string),
        taxable: value
            .get("taxable")
            .and_then(Value::as_bool)
            .unwrap_or(true),
        inventory_policy: value
            .get("inventoryPolicy")
            .and_then(Value::as_str)
            .unwrap_or("DENY")
            .to_string(),
        inventory_quantity: value
            .get("inventoryQuantity")
            .and_then(Value::as_i64)
            .unwrap_or_default(),
        selected_options: value
            .get("selectedOptions")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|option| {
                Some(ProductVariantSelectedOption {
                    name: option.get("name")?.as_str()?.to_string(),
                    value: option.get("value")?.as_str()?.to_string(),
                })
            })
            .collect(),
        inventory_item: ProductVariantInventoryItem {
            id: inventory_item_id,
            tracked: inventory_item
                .and_then(|item| item.get("tracked"))
                .and_then(Value::as_bool)
                .unwrap_or(false),
            requires_shipping: inventory_item
                .and_then(|item| item.get("requiresShipping"))
                .and_then(Value::as_bool)
                .unwrap_or(true),
            extra_fields: inventory_item
                .map(|inventory_item| {
                    product_variant_state_extra_fields(
                        inventory_item,
                        &["id", "tracked", "requiresShipping"],
                    )
                })
                .unwrap_or_default(),
        },
        media_ids: variant_media_ids_from_json(value),
        extra_fields: product_variant_state_extra_fields(value, extra_field_exclusions),
    })
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
    let id = value.get("id")?.as_str()?.to_string();
    let created_at = value
        .get("createdAt")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(default_product_timestamp);
    let updated_at = value
        .get("updatedAt")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| created_at.clone());
    let mut extra_fields = product_extra_fields_from_json(value);
    if let Some(state_extra_fields) = value.get("extraFields").and_then(Value::as_object) {
        for (key, observed) in state_extra_fields {
            extra_fields.insert(key.clone(), observed.clone());
        }
    }
    Some(ProductRecord {
        id,
        created_at,
        updated_at,
        title: value
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        handle: value
            .get("handle")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        status: value
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or("ACTIVE")
            .to_string(),
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
        total_inventory: value
            .get("totalInventory")
            .and_then(Value::as_i64)
            .unwrap_or(0),
        tracks_inventory: value
            .get("tracksInventory")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        variants: value
            .get("variants")
            .and_then(|connection| connection.get("nodes"))
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default(),
        media: value
            .get("media")
            .and_then(|connection| connection.get("nodes"))
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default(),
        collections: value
            .get("collections")
            .and_then(|connection| connection.get("nodes"))
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default(),
        extra_fields,
    })
}

pub(in crate::proxy) fn product_extra_fields_from_json(value: &Value) -> BTreeMap<String, Value> {
    let mut extra_fields = BTreeMap::new();
    if let Some(object) = value.as_object() {
        for (key, observed) in object {
            if !matches!(
                key.as_str(),
                "id" | "createdAt"
                    | "updatedAt"
                    | "title"
                    | "handle"
                    | "status"
                    | "descriptionHtml"
                    | "vendor"
                    | "productType"
                    | "tags"
                    | "totalInventory"
                    | "tracksInventory"
                    | "variants"
                    | "media"
                    | "collections"
            ) {
                extra_fields.insert(key.clone(), observed.clone());
            }
        }
    }
    extra_fields
}

pub(in crate::proxy) fn product_state_json(product: &ProductRecord) -> Value {
    json!({
        "id": product.id,
        "createdAt": product.created_at,
        "updatedAt": product.updated_at,
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
        },
        "totalInventory": product.total_inventory,
        "tracksInventory": product.tracks_inventory,
        "media": connection_json(product.media.clone()),
        "variants": connection_json(product.variants.clone()),
        "collections": connection_json(product.collections.clone()),
        "extraFields": product.extra_fields
    })
}

pub(in crate::proxy) fn product_variant_state_map_json(
    variants: &BTreeMap<String, ProductVariantRecord>,
) -> Value {
    Value::Object(
        variants
            .iter()
            .map(|(id, variant)| (id.clone(), product_variant_state_json(variant)))
            .collect(),
    )
}

pub(in crate::proxy) fn product_variant_state_map_from_json(
    value: &Value,
) -> BTreeMap<String, ProductVariantRecord> {
    value
        .as_object()
        .into_iter()
        .flatten()
        .filter_map(|(id, value)| {
            product_variant_state_from_json(value).map(|variant| (id.clone(), variant))
        })
        .collect()
}

pub(in crate::proxy) fn product_variant_state_from_json(
    value: &Value,
) -> Option<ProductVariantRecord> {
    let product_id = value.get("productId")?.as_str()?.to_string();
    product_variant_state_from_json_parts(
        value,
        product_id,
        ProductVariantInventoryItemMode::Required,
        &[
            "id",
            "productId",
            "title",
            "sku",
            "barcode",
            "price",
            "compareAtPrice",
            "taxable",
            "inventoryPolicy",
            "inventoryQuantity",
            "selectedOptions",
            "inventoryItem",
            "mediaIds",
            "media",
        ],
    )
}

pub(in crate::proxy) fn product_variant_state_json(variant: &ProductVariantRecord) -> Value {
    // Shopify returns `null` (not an empty string) for a variant with no SKU. The state
    // parser reads a null SKU back as an empty string, so this round-trips cleanly.
    let sku = if variant.sku.is_empty() {
        Value::Null
    } else {
        json!(variant.sku)
    };
    let mut value = json!({
        "id": variant.id,
        "productId": variant.product_id,
        "title": variant.title,
        "sku": sku,
        "barcode": variant.barcode,
        "price": variant.price,
        "compareAtPrice": variant.compare_at_price,
        "taxable": variant.taxable,
        "inventoryPolicy": variant.inventory_policy,
        "inventoryQuantity": variant.inventory_quantity,
        "selectedOptions": variant.selected_options.iter().map(|option| {
            json!({ "name": option.name, "value": option.value })
        }).collect::<Vec<_>>(),
        "inventoryItem": {
            "id": variant.inventory_item.id,
            "tracked": variant.inventory_item.tracked,
            "requiresShipping": variant.inventory_item.requires_shipping
        }
    });
    if let Some(map) = value.as_object_mut() {
        for (key, field_value) in &variant.extra_fields {
            map.insert(key.clone(), field_value.clone());
        }
        if let Some(inventory_item) = map.get_mut("inventoryItem").and_then(Value::as_object_mut) {
            for (key, field_value) in &variant.inventory_item.extra_fields {
                inventory_item.insert(key.clone(), field_value.clone());
            }
        }
        // Round-trip the variant→media attachment so chained mutation targets
        // (append-media → detach-media → downstream-read share an evolving
        // dump/restore state) preserve which library media a variant carries.
        if !variant.media_ids.is_empty() {
            map.insert("mediaIds".to_string(), json!(variant.media_ids));
        }
    }
    value
}

pub(in crate::proxy) fn product_variant_state_extra_fields(
    value: &Value,
    known_fields: &[&str],
) -> BTreeMap<String, Value> {
    value
        .as_object()
        .into_iter()
        .flat_map(|fields| fields.iter())
        .filter(|(key, _)| !known_fields.contains(&key.as_str()))
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect()
}

pub(in crate::proxy) fn product_cursor(product: &ProductRecord) -> &str {
    &product.id
}

pub(in crate::proxy) fn rust_state_dump_path_exists(dump: &Value, path: &str) -> bool {
    path.split('.')
        .try_fold(dump, |current, segment| current.get(segment))
        .is_some()
}

pub(in crate::proxy) fn product_variant_record_from_create_input(
    input: &BTreeMap<String, ResolvedValue>,
    id: String,
    product_id: String,
    inventory_item_id: String,
) -> ProductVariantRecord {
    let mut variant = empty_product_variant_record(product_id, id, inventory_item_id);
    variant.inventory_item.tracked = true;
    apply_product_variant_input(&mut variant, input);
    variant
}

pub(in crate::proxy) fn apply_product_variant_input(
    variant: &mut ProductVariantRecord,
    input: &BTreeMap<String, ResolvedValue>,
) {
    if let Some(title) = resolved_string_field(input, "title") {
        variant.title = title;
    }
    if let Some(sku) = resolved_string_field(input, "sku") {
        variant.sku = sku;
    }
    if input.contains_key("barcode") {
        variant.barcode = resolved_string_field(input, "barcode");
    }
    if let Some(price) = resolved_string_field(input, "price") {
        variant.price = price;
    }
    if input.contains_key("compareAtPrice") {
        variant.compare_at_price = resolved_string_field(input, "compareAtPrice");
    }
    if let Some(taxable) = resolved_bool_field(input, "taxable") {
        variant.taxable = taxable;
    }
    if let Some(inventory_policy) = resolved_string_field(input, "inventoryPolicy") {
        variant.inventory_policy = inventory_policy;
    }
    if let Some(inventory_quantity) = resolved_int_field(input, "inventoryQuantity") {
        variant.inventory_quantity = inventory_quantity;
    }
    if let Some(inventory_quantity) = resolved_object_list_field(input, "inventoryQuantities")
        .into_iter()
        .filter_map(|quantity| resolved_int_field(&quantity, "availableQuantity"))
        .next()
    {
        variant.inventory_quantity = inventory_quantity;
    }
    for field in [
        "taxCode",
        "position",
        "requiresComponents",
        "showUnitPrice",
        "unitPriceMeasurement",
    ] {
        if let Some(value) = input.get(field) {
            variant
                .extra_fields
                .insert(field.to_string(), resolved_value_json(value));
        }
    }
    let selected_options = resolved_product_variant_selected_options(input);
    if input.contains_key("selectedOptions")
        || input.contains_key("options")
        || input.contains_key("optionValues")
    {
        variant.selected_options = selected_options;
    }
    if let Some(inventory_item) = resolved_object_field(input, "inventoryItem") {
        if let Some(sku) = resolved_string_field(&inventory_item, "sku") {
            variant.sku = sku;
        }
        if let Some(tracked) = resolved_bool_field(&inventory_item, "tracked") {
            variant.inventory_item.tracked = tracked;
        }
        if let Some(requires_shipping) = resolved_bool_field(&inventory_item, "requiresShipping") {
            variant.inventory_item.requires_shipping = requires_shipping;
        }
        if let Some(id) = resolved_string_field(&inventory_item, "id") {
            variant.inventory_item.id = id;
        }
        for field in [
            "sku",
            "countryCodeOfOrigin",
            "provinceCodeOfOrigin",
            "measurement",
        ] {
            if let Some(value) = inventory_item.get(field) {
                variant
                    .inventory_item
                    .extra_fields
                    .insert(field.to_string(), resolved_value_json(value));
            }
        }
        if let Some(value) = inventory_item.get("harmonizedSystemCode") {
            let value = match value {
                ResolvedValue::String(value) => {
                    Value::String(product_variant_normalized_harmonized_system_code(value))
                }
                _ => resolved_value_json(value),
            };
            variant
                .inventory_item
                .extra_fields
                .insert("harmonizedSystemCode".to_string(), value);
        }
    }
}

fn product_variant_normalized_harmonized_system_code(value: &str) -> String {
    value.chars().filter(char::is_ascii_alphanumeric).collect()
}

pub(in crate::proxy) fn resolved_product_variant_selected_options(
    input: &BTreeMap<String, ResolvedValue>,
) -> Vec<ProductVariantSelectedOption> {
    let selected_options = resolved_object_list_field(input, "selectedOptions")
        .into_iter()
        .filter_map(|option| {
            Some(ProductVariantSelectedOption {
                name: resolved_string_field(&option, "name")?,
                value: resolved_string_field(&option, "value")?,
            })
        })
        .collect::<Vec<_>>();
    if !selected_options.is_empty() || input.contains_key("selectedOptions") {
        return selected_options;
    }
    let option_values = resolved_object_list_field(input, "optionValues")
        .into_iter()
        .filter_map(|option| {
            Some(ProductVariantSelectedOption {
                name: resolved_string_field(&option, "optionName")
                    .or_else(|| resolved_string_field(&option, "name"))
                    .unwrap_or_else(|| "Title".to_string()),
                value: resolved_string_field(&option, "name")
                    .or_else(|| resolved_string_field(&option, "linkedMetafieldValue"))?,
            })
        })
        .collect::<Vec<_>>();
    if !option_values.is_empty() || input.contains_key("optionValues") {
        return option_values;
    }
    match input.get("options") {
        Some(ResolvedValue::List(options)) => options
            .iter()
            .enumerate()
            .filter_map(|(index, option)| match option {
                ResolvedValue::String(value) => Some(ProductVariantSelectedOption {
                    name: format!("Option{}", index + 1),
                    value: value.clone(),
                }),
                ResolvedValue::Object(object) => Some(ProductVariantSelectedOption {
                    name: resolved_string_field(object, "name")
                        .unwrap_or_else(|| format!("Option{}", index + 1)),
                    value: resolved_string_field(object, "value")?,
                }),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

pub(in crate::proxy) fn product_variant_input_user_errors_with_prefix(
    input: &BTreeMap<String, ResolvedValue>,
    field_prefix: &[String],
) -> Vec<Value> {
    let mut errors = Vec::new();
    if input.get("price") == Some(&ResolvedValue::Null) {
        errors.push(user_error(
            prefixed_error_field(field_prefix, &["price"]),
            "Price can't be blank",
            Some("INVALID"),
        ));
    } else if let Some(price) = resolved_f64_path(input, &["price"]) {
        if price < 0.0 {
            errors.push(user_error(
                prefixed_error_field(field_prefix, &["price"]),
                "Price must be greater than or equal to 0",
                Some("GREATER_THAN_OR_EQUAL_TO"),
            ));
        } else if price >= VARIANT_MONEY_UPPER_BOUND {
            errors.push(user_error(
                prefixed_error_field(field_prefix, &["price"]),
                "Price must be less than 1000000000000000000",
                Some("INVALID_INPUT"),
            ));
        }
    }

    if let Some(compare_at_price) = resolved_f64_path(input, &["compareAtPrice"]) {
        if compare_at_price >= VARIANT_MONEY_UPPER_BOUND {
            errors.push(user_error(
                prefixed_error_field(field_prefix, &["compareAtPrice"]),
                "must be less than 1000000000000000000",
                Some("INVALID_INPUT"),
            ));
        }
    }

    if let Some(quantity) = resolved_int_field(input, "inventoryQuantity") {
        if quantity > 1_000_000_000 {
            errors.push(user_error(
                prefixed_error_field(field_prefix, &["inventoryQuantity"]),
                "Inventory quantity must be less than or equal to 1000000000",
                Some("INVALID_INPUT"),
            ));
        }
    }
    for quantity in resolved_object_list_field(input, "inventoryQuantities") {
        if let Some(available_quantity) = resolved_int_field(&quantity, "availableQuantity") {
            if available_quantity > 1_000_000_000 {
                errors.push(user_error(
                    prefixed_error_field(field_prefix, &["inventoryQuantities"]),
                    "Inventory quantity must be less than or equal to 1000000000",
                    Some("INVALID_INPUT"),
                ));
                break;
            }
        }
    }

    if resolved_string_field(input, "sku").is_some_and(|sku| sku.chars().count() > 255) {
        errors.push(user_error(
            prefixed_error_field(field_prefix, &["sku"]),
            "SKU is too long (maximum is 255 characters)",
            Some("INVALID_INPUT"),
        ));
    }
    if resolved_string_field(input, "barcode").is_some_and(|barcode| barcode.chars().count() > 255)
    {
        errors.push(user_error(
            prefixed_error_field(field_prefix, &["barcode"]),
            "Barcode is too long (maximum is 255 characters)",
            Some("INVALID_INPUT"),
        ));
    }

    if let Some(inventory_item) = resolved_object_field(input, "inventoryItem") {
        if let Some(cost) = resolved_f64_path(&inventory_item, &["cost"]) {
            if cost < 0.0 {
                if is_bulk_variant_error_prefix(field_prefix) {
                    errors.push(user_error(
                        prefixed_error_field(field_prefix, &[]),
                        "must be greater than or equal to 0",
                        None,
                    ));
                }
                errors.push(user_error(
                    prefixed_error_field(field_prefix, &["inventoryItem", "cost"]),
                    "Cost per item must be greater than or equal to 0",
                    Some(if is_product_set_variant_error_prefix(field_prefix) {
                        "INVALID_VARIANT"
                    } else {
                        "GREATER_THAN_OR_EQUAL_TO"
                    }),
                ));
            } else if cost >= VARIANT_MONEY_UPPER_BOUND {
                if is_bulk_variant_error_prefix(field_prefix) {
                    errors.push(user_error(
                        prefixed_error_field(field_prefix, &[]),
                        "must be less than 1000000000000000000",
                        None,
                    ));
                }
                errors.push(user_error(
                    prefixed_error_field(field_prefix, &["inventoryItem", "cost"]),
                    "Cost per item must be less than 1000000000000000000",
                    Some(if is_product_set_variant_error_prefix(field_prefix) {
                        "INVALID_VARIANT"
                    } else {
                        "INVALID_INPUT"
                    }),
                ));
            }
        }

        if resolved_string_field(&inventory_item, "sku")
            .is_some_and(|sku| sku.chars().count() > 255)
        {
            let bulk_field = is_bulk_variant_error_prefix(field_prefix);
            errors.push(user_error(
                if bulk_field {
                    prefixed_error_field(field_prefix, &[])
                } else {
                    prefixed_error_field(field_prefix, &["inventoryItem", "sku"])
                },
                "SKU is too long (maximum is 255 characters)",
                Some("INVALID_INPUT"),
            ));
            if bulk_field {
                errors.push(user_error(
                    prefixed_error_field(field_prefix, &[]),
                    "is too long (maximum is 255 characters)",
                    None,
                ));
            }
        }
    }

    for (option_index, option) in resolved_product_variant_selected_options(input)
        .into_iter()
        .enumerate()
    {
        if option.value.chars().count() > 255 {
            errors.push(user_error(
                if input.contains_key("optionValues") {
                    prefixed_error_field(
                        field_prefix,
                        &["optionValues", &option_index.to_string(), "name"],
                    )
                } else {
                    prefixed_error_field(field_prefix, &["options"])
                },
                "Option value name is too long",
                Some("INVALID_INPUT"),
            ));
            break;
        }
    }

    if let Some(inventory_item) = resolved_object_field(input, "inventoryItem") {
        if let Some(measurement) = resolved_object_field(&inventory_item, "measurement") {
            if let Some(weight) = resolved_object_field(&measurement, "weight") {
                if let Some(value) = resolved_f64_path(&weight, &["value"]) {
                    if value < 0.0 {
                        errors.push(user_error(
                            variant_weight_error_field(field_prefix),
                            "Weight must be greater than or equal to 0",
                            Some("GREATER_THAN_OR_EQUAL_TO"),
                        ));
                    } else if value >= 2_000_000_000.0 {
                        errors.push(user_error(
                            variant_weight_error_field(field_prefix),
                            "Weight must be less than 2000000000",
                            Some("INVALID_INPUT"),
                        ));
                    }
                }
                if let Some(unit) = resolved_string_field(&weight, "unit") {
                    if !matches!(unit.as_str(), "KILOGRAMS" | "GRAMS" | "POUNDS" | "OUNCES") {
                        errors.push(user_error(
                            variant_weight_error_field(field_prefix),
                            "Weight unit must be one of KILOGRAMS, GRAMS, POUNDS, OUNCES",
                            Some("INVALID_INPUT"),
                        ));
                    }
                }
            }
        }
    }

    errors
}

fn prefixed_error_field(prefix: &[String], suffix: &[&str]) -> Value {
    Value::Array(
        prefix
            .iter()
            .cloned()
            .chain(suffix.iter().map(|field| (*field).to_string()))
            .map(Value::String)
            .collect(),
    )
}

fn is_bulk_variant_error_prefix(prefix: &[String]) -> bool {
    prefix.first().is_some_and(|field| field == "variants")
}

fn is_product_set_variant_error_prefix(prefix: &[String]) -> bool {
    matches!(
        prefix,
        [input, variants, ..] if input == "input" && variants == "variants"
    )
}

fn variant_weight_error_field(prefix: &[String]) -> Value {
    if is_bulk_variant_error_prefix(prefix) {
        prefixed_error_field(prefix, &[])
    } else {
        prefixed_error_field(prefix, &["inventoryItem", "measurement", "weight"])
    }
}

pub(in crate::proxy) fn product_create_payload_value(
    product: Option<Value>,
    errors: Vec<Value>,
) -> Value {
    json!({
        "product": product,
        // `ProductCreatePayload.shop` is resolved lazily by its explicit field
        // resolver, which performs identity hydration only when selected.
        "shop": Value::Null,
        "userErrors": errors,
    })
}

pub(in crate::proxy) fn product_update_payload_value(
    product: Option<Value>,
    errors: Vec<Value>,
) -> Value {
    json!({
        "product": product,
        "userErrors": errors,
    })
}

pub(in crate::proxy) fn product_update_missing_payload_value() -> Value {
    product_update_payload_value(
        None,
        vec![user_error(
            ["id"],
            "Product does not exist",
            Some("NOT_FOUND"),
        )],
    )
}

pub(in crate::proxy) fn product_change_status_payload_value(
    product: Option<Value>,
    errors: Vec<Value>,
) -> Value {
    json!({
        "product": product,
        "userErrors": errors,
    })
}

pub(in crate::proxy) fn product_delete_payload_value(deleted_product_id: &str) -> Value {
    json!({
        "deletedProductId": deleted_product_id,
        "productDeleteOperation": Value::Null,
        "shop": Value::Null,
        "userErrors": [],
    })
}

pub(in crate::proxy) fn product_delete_async_operation_payload_value(operation_id: &str) -> Value {
    json!({
        "deletedProductId": Value::Null,
        "productDeleteOperation": {
            "id": operation_id,
            "status": "CREATED",
            "deletedProductId": Value::Null,
            "userErrors": [],
        },
        "shop": Value::Null,
        "userErrors": [],
    })
}

pub(in crate::proxy) fn product_delete_async_duplicate_payload_value() -> Value {
    json!({
        "deletedProductId": Value::Null,
        "productDeleteOperation": Value::Null,
        "shop": Value::Null,
        "userErrors": [user_error_omit_code(
            Value::Null,
            "Another operation already in progress. Please wait until current one is finished.",
            None,
        )],
    })
}

pub(in crate::proxy) fn product_delete_missing_payload_value() -> Value {
    json!({
        "deletedProductId": Value::Null,
        "productDeleteOperation": Value::Null,
        "shop": Value::Null,
        "userErrors": [user_error(
            ["id"],
            "Product does not exist",
            Some("NOT_FOUND"),
        )],
    })
}

/// Extract the taxonomy category GID from a product mutation input. Shopify accepts
/// the category as a scalar `category` GID, or nested under the legacy
/// `productCategory`/`standardProductType`/`standardizedProductType` objects keyed by
/// `productTaxonomyNodeId`.
pub(in crate::proxy) fn product_category_input_id(
    input: &BTreeMap<String, ResolvedValue>,
) -> Option<String> {
    resolved_string_field(input, "category")
        .or_else(|| resolved_object_string_field(input, "productCategory", "productTaxonomyNodeId"))
        .or_else(|| {
            resolved_object_string_field(input, "standardProductType", "productTaxonomyNodeId")
        })
        .or_else(|| {
            resolved_object_string_field(input, "standardizedProductType", "productTaxonomyNodeId")
        })
}

impl DraftProxy {
    pub(in crate::proxy) fn product_category_for_mutation_input(
        &self,
        request: &Request,
        input: &BTreeMap<String, ResolvedValue>,
        response_key: &str,
        root_location: SourceLocation,
    ) -> Result<Option<Value>, ResolverOutcome<Value>> {
        let Some(category_id) = product_category_input_id(input) else {
            return Ok(None);
        };
        match self.product_category_resolution_for_input(request, &category_id) {
            ProductCategoryResolution::Found(category) => Ok(Some(category)),
            ProductCategoryResolution::Malformed | ProductCategoryResolution::VerifiedAbsent => {
                Err(graphql_error_outcome(
                    vec![invalid_product_taxonomy_node_id_error(
                        response_key,
                        root_location,
                    )],
                    response_key,
                ))
            }
            ProductCategoryResolution::Indeterminate => {
                Err(indeterminate_product_taxonomy_category_outcome())
            }
        }
    }

    /// Resolve product category input from authoritative effective state or Shopify's
    /// taxonomy node read. A well-formed GID proves only the resource type, never that the
    /// category exists or that its path encodes hierarchy metadata.
    pub(in crate::proxy) fn product_category_resolution_for_input(
        &self,
        request: &Request,
        id: &str,
    ) -> ProductCategoryResolution {
        if !is_shopify_gid_of_type(id, "TaxonomyCategory") {
            return ProductCategoryResolution::Malformed;
        }

        if let Some(category) = self.product_category_value_from_effective_state(id) {
            return ProductCategoryResolution::Found(category);
        }

        if self.config.read_mode != ReadMode::LiveHybrid {
            return ProductCategoryResolution::Indeterminate;
        }

        self.hydrate_taxonomy_category_value(request, id)
    }

    fn product_category_value_from_effective_state(&self, id: &str) -> Option<Value> {
        self.store
            .products
            .staged
            .records
            .values()
            .chain(self.store.products.base.records.values())
            .filter_map(|product| product.extra_fields.get("category"))
            .find_map(|category| authoritative_product_category_value(category, id))
    }

    fn hydrate_taxonomy_category_value(
        &self,
        request: &Request,
        id: &str,
    ) -> ProductCategoryResolution {
        let response = self.upstream_post(
            request,
            json!({
                "query": TAXONOMY_CATEGORY_HYDRATE_QUERY,
                "operationName": "ProductTaxonomyCategoryHydrate",
                "variables": { "id": id }
            }),
        );
        if !(200..300).contains(&response.status)
            || response
                .body
                .get("errors")
                .and_then(Value::as_array)
                .is_some_and(|errors| !errors.is_empty())
        {
            return ProductCategoryResolution::Indeterminate;
        }
        let Some(node) = response.body.pointer("/data/node") else {
            return ProductCategoryResolution::Indeterminate;
        };
        if node.is_null() {
            return ProductCategoryResolution::VerifiedAbsent;
        }

        authoritative_product_category_value(node, id)
            .map(ProductCategoryResolution::Found)
            .unwrap_or(ProductCategoryResolution::Indeterminate)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(in crate::proxy) enum ProductCategoryResolution {
    Found(Value),
    VerifiedAbsent,
    Malformed,
    Indeterminate,
}

fn authoritative_product_category_value(category: &Value, requested_id: &str) -> Option<Value> {
    if category.get("__typename").is_some()
        && category.get("__typename").and_then(Value::as_str) != Some("TaxonomyCategory")
    {
        return None;
    }
    if category.get("id").and_then(Value::as_str) != Some(requested_id) {
        return None;
    }
    let name = category.get("name")?.as_str()?;
    let full_name = category.get("fullName")?.as_str()?;
    let is_leaf = category.get("isLeaf")?.as_bool()?;
    let level = category.get("level")?.as_u64()?;
    let parent_id = category.get("parentId")?;
    if !parent_id.is_null() && parent_id.as_str().is_none() {
        return None;
    }

    Some(json!({
        "id": requested_id,
        "fullName": full_name,
        "name": name,
        "isLeaf": is_leaf,
        "level": level,
        "parentId": parent_id
    }))
}

pub(in crate::proxy) fn indeterminate_product_taxonomy_category_outcome() -> ResolverOutcome<Value>
{
    ResolverOutcome::error("Unable to verify product taxonomy category")
}

pub(in crate::proxy) fn invalid_product_taxonomy_node_id_error(
    response_key: &str,
    location: SourceLocation,
) -> Value {
    json!({
        "message": "Invalid product_taxonomy_node_id",
        "locations": [{ "line": location.line, "column": location.column }],
        "extensions": { "code": "INVALID_PRODUCT_TAXONOMY_NODE_ID" },
        "path": [response_key]
    })
}

pub(in crate::proxy) fn product_input(
    arguments: &BTreeMap<String, ResolvedValue>,
) -> Option<BTreeMap<String, ResolvedValue>> {
    match arguments.get("product").or_else(|| arguments.get("input")) {
        Some(ResolvedValue::Object(input)) => Some(input.clone()),
        _ => None,
    }
}

#[derive(Debug, Clone, Copy)]
pub(in crate::proxy) enum ProductScalarLengthValidationShape {
    ProductInput,
    ProductSetInput,
}

pub(in crate::proxy) fn product_scalar_length_user_errors(
    input: &BTreeMap<String, ResolvedValue>,
    shape: ProductScalarLengthValidationShape,
) -> Vec<Value> {
    let mut errors = Vec::new();
    for field in [
        ProductScalarLengthField::Title,
        ProductScalarLengthField::Handle,
        ProductScalarLengthField::Vendor,
        ProductScalarLengthField::ProductType,
    ] {
        errors.extend(product_scalar_field_length_user_errors(input, field, shape));
    }

    if matches!(shape, ProductScalarLengthValidationShape::ProductSetInput)
        && !input.contains_key("handle")
    {
        if let Some(title) = resolved_string_field(input, "title") {
            let derived_handle = normalize_product_handle(&title);
            if derived_handle.chars().count() > PRODUCT_SCALAR_MAX_LENGTH {
                errors.push(product_set_scalar_length_user_error(
                    ProductScalarLengthField::Handle,
                ));
            }
        }
    }

    errors
}

#[derive(Debug, Clone, Copy)]
enum ProductScalarLengthField {
    Title,
    Handle,
    Vendor,
    ProductType,
    CustomProductType,
}

fn product_scalar_field_length_user_errors(
    input: &BTreeMap<String, ResolvedValue>,
    field: ProductScalarLengthField,
    shape: ProductScalarLengthValidationShape,
) -> Vec<Value> {
    let Some(value) = resolved_string_field(input, product_scalar_length_field_name(field)) else {
        return Vec::new();
    };
    if value.chars().count() <= PRODUCT_SCALAR_MAX_LENGTH {
        return Vec::new();
    }

    let mut errors = vec![product_scalar_length_user_error(field, shape)];
    if matches!(field, ProductScalarLengthField::ProductType) {
        errors.push(product_scalar_length_user_error(
            ProductScalarLengthField::CustomProductType,
            shape,
        ));
    }
    errors
}

fn product_scalar_length_field_name(field: ProductScalarLengthField) -> &'static str {
    match field {
        ProductScalarLengthField::Title => "title",
        ProductScalarLengthField::Handle => "handle",
        ProductScalarLengthField::Vendor => "vendor",
        ProductScalarLengthField::ProductType => "productType",
        ProductScalarLengthField::CustomProductType => "customProductType",
    }
}

fn product_scalar_length_user_error(
    field: ProductScalarLengthField,
    shape: ProductScalarLengthValidationShape,
) -> Value {
    match shape {
        ProductScalarLengthValidationShape::ProductInput => length_user_error(
            [product_scalar_length_field_name(field)],
            product_scalar_length_field_label(field),
            LengthUserErrorBound::TooLong {
                maximum: PRODUCT_SCALAR_MAX_LENGTH,
            },
        ),
        ProductScalarLengthValidationShape::ProductSetInput => {
            product_set_scalar_length_user_error(field)
        }
    }
}

fn product_set_scalar_length_user_error(field: ProductScalarLengthField) -> Value {
    match field {
        ProductScalarLengthField::Title => user_error_omit_code(
            ["input", "title"],
            &format!("is too long (maximum is {PRODUCT_SCALAR_MAX_LENGTH} characters)"),
            None,
        ),
        ProductScalarLengthField::Handle => user_error_omit_code(
            ["input"],
            &too_long_message("Handle", PRODUCT_SCALAR_MAX_LENGTH),
            None,
        ),
        ProductScalarLengthField::Vendor => user_error_omit_code(
            ["input"],
            &too_long_message("Vendor", PRODUCT_SCALAR_MAX_LENGTH),
            None,
        ),
        ProductScalarLengthField::ProductType => user_error_omit_code(
            ["input"],
            &too_long_message("Product type", PRODUCT_SCALAR_MAX_LENGTH),
            None,
        ),
        ProductScalarLengthField::CustomProductType => user_error_omit_code(
            ["input"],
            &too_long_message("Custom product type", PRODUCT_SCALAR_MAX_LENGTH),
            None,
        ),
    }
}

fn product_scalar_length_field_label(field: ProductScalarLengthField) -> &'static str {
    match field {
        ProductScalarLengthField::Title => "Title",
        ProductScalarLengthField::Handle => "Handle",
        ProductScalarLengthField::Vendor => "Vendor",
        ProductScalarLengthField::ProductType => "Product type",
        ProductScalarLengthField::CustomProductType => "Custom product type",
    }
}

pub(in crate::proxy) fn product_create_status_validation_errors(
    request: &Request,
    query: &str,
    root_name: &str,
    root_location: SourceLocation,
    raw_arguments: &BTreeMap<String, RawArgumentValue>,
) -> Option<Vec<Value>> {
    let (argument_name, input_object_type) = if raw_arguments.contains_key("product") {
        ("product", "ProductCreateInput")
    } else {
        ("input", "ProductInput")
    };
    let input = raw_arguments.get(argument_name)?;
    product_status_input_field_validation_errors(
        request,
        query,
        root_name,
        root_location,
        raw_arguments,
        input,
        ProductStatusInputContext {
            argument_name,
            input_object_type,
            field_name: "status",
            expected_type: "ProductStatus",
        },
    )
}

pub(in crate::proxy) fn product_status_argument_validation_errors(
    request: &Request,
    query: &str,
    root_name: &str,
    root_location: SourceLocation,
    raw_arguments: &BTreeMap<String, RawArgumentValue>,
    context: ProductStatusArgumentContext<'_>,
) -> Option<Vec<Value>> {
    let raw = raw_arguments.get(context.argument_name)?;
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
                .unwrap_or_else(|| context.expected_type.to_string());
            let location = definition.map(|definition| definition.location);
            Some(vec![invalid_product_status_variable_error(
                request,
                name,
                &variable_type,
                value.as_ref()?,
                None,
                &status,
                location,
            )])
        }
        raw => {
            let status = raw_product_status_value(raw)?;
            if product_status_allowed(&status, request) {
                return None;
            }
            Some(vec![invalid_product_status_literal_error(
                query,
                root_name,
                root_location,
                raw_arguments,
                ProductStatusLiteralError {
                    value: &status,
                    argument_name: context.argument_name,
                    type_name: context.container_type_name,
                    container_name: context.container_name,
                    expected_type: context.expected_type,
                    location: None,
                },
            )])
        }
    }
}

fn product_status_input_field_validation_errors(
    request: &Request,
    query: &str,
    root_name: &str,
    root_location: SourceLocation,
    raw_arguments: &BTreeMap<String, RawArgumentValue>,
    input: &RawArgumentValue,
    context: ProductStatusInputContext<'_>,
) -> Option<Vec<Value>> {
    match input {
        RawArgumentValue::Object(input) => {
            let status = raw_product_status_value(input.get(context.field_name)?)?;
            if product_status_allowed(&status, request) {
                return None;
            }
            let location =
                argument_value_location_after(query, root_location, context.argument_name);
            Some(vec![invalid_product_status_literal_error(
                query,
                root_name,
                root_location,
                raw_arguments,
                ProductStatusLiteralError {
                    value: &status,
                    argument_name: context.field_name,
                    type_name: "InputObject",
                    container_name: context.input_object_type,
                    expected_type: context.expected_type,
                    location,
                },
            )])
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
            Some(vec![invalid_product_status_variable_error(
                request,
                name,
                &variable_type,
                value,
                Some(context.field_name),
                &status,
                location,
            )])
        }
        _ => None,
    }
}

fn invalid_product_status_literal_error(
    query: &str,
    root_name: &str,
    root_location: SourceLocation,
    raw_arguments: &BTreeMap<String, RawArgumentValue>,
    error: ProductStatusLiteralError<'_>,
) -> Value {
    let operation_path = parsed_document(query, &BTreeMap::new())
        .map(|document| document.operation_path)
        .unwrap_or_else(|| "mutation".to_string());
    let path = if error.type_name == "InputObject" {
        let input_argument_name = raw_arguments
            .contains_key("product")
            .then_some("product")
            .or_else(|| raw_arguments.contains_key("input").then_some("input"))
            .unwrap_or("input");
        json!([
            operation_path,
            root_name,
            input_argument_name,
            error.argument_name
        ])
    } else {
        json!([operation_path, root_name, error.argument_name])
    };
    let location = error.location.unwrap_or(root_location);
    json!({
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
    })
}

fn invalid_product_status_variable_error(
    request: &Request,
    variable_name: &str,
    variable_type: &str,
    value: &ResolvedValue,
    field_name: Option<&str>,
    invalid_status: &str,
    location: Option<SourceLocation>,
) -> Value {
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
    invalid_variable_error_envelope(
        message,
        location.unwrap_or(SourceLocation { line: 1, column: 1 }),
        resolved_value_json(value),
        json!([{ "path": path, "explanation": explanation }]),
    )
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

pub(in crate::proxy) fn product_delete_required_id_errors(
    query: &str,
    variables: &BTreeMap<String, ResolvedValue>,
) -> Option<Vec<Value>> {
    let document = parsed_document(query, variables)?;
    let field = document
        .root_fields
        .iter()
        .find(|field| field.name == "productDelete")?;
    let input_argument_name = if field.raw_arguments.contains_key("input") {
        "input"
    } else {
        "product"
    };
    let input = field.raw_arguments.get(input_argument_name)?;
    let inline_location =
        inline_argument_value_location(query, field, input_argument_name).unwrap_or(field.location);

    match input {
        RawArgumentValue::Object(input) => match input.get("id") {
            None => Some(vec![product_delete_inline_missing_id_error(
                &document.operation_path,
                field,
                input_argument_name,
                inline_location,
            )]),
            Some(value) if value.is_literal_null() => {
                Some(vec![product_delete_inline_null_id_error(
                    &document.operation_path,
                    field,
                    input_argument_name,
                    inline_location,
                )])
            }
            _ => None,
        },
        RawArgumentValue::Variable { name, value: None } => {
            Some(vec![product_delete_variable_required_id_error(
                Value::Null,
                name,
                &document,
                field,
            )])
        }
        RawArgumentValue::Variable {
            name,
            value: Some(ResolvedValue::Object(input)),
        } => match input.get("id") {
            None => Some(vec![product_delete_variable_required_id_error(
                resolved_value_json(&ResolvedValue::Object(input.clone())),
                name,
                &document,
                field,
            )]),
            Some(ResolvedValue::Null) => Some(vec![product_delete_variable_required_id_error(
                resolved_value_json(&ResolvedValue::Object(input.clone())),
                name,
                &document,
                field,
            )]),
            _ => None,
        },
        _ => None,
    }
}

pub(in crate::proxy) fn product_delete_inline_missing_id_error(
    operation_path: &str,
    field: &RootFieldSelection,
    input_argument_name: &str,
    location: SourceLocation,
) -> Value {
    missing_required_input_object_attribute_error_envelope(
        "ProductDeleteInput",
        "id",
        "ID!",
        location,
        json!([
            operation_path,
            field.response_key.clone(),
            input_argument_name,
            "id"
        ]),
    )
}

pub(in crate::proxy) fn product_delete_inline_null_id_error(
    operation_path: &str,
    field: &RootFieldSelection,
    input_argument_name: &str,
    location: SourceLocation,
) -> Value {
    argument_literals_incompatible_error_envelope(
        "Argument 'id' on InputObject 'ProductDeleteInput' has an invalid value (null). Expected type 'ID!'.".to_string(),
        Some(location),
        Some(json!([operation_path, field.response_key.clone(), input_argument_name, "id"])),
        Some("InputObject"),
        Some("id"),
    )
}

pub(in crate::proxy) fn product_delete_variable_required_id_error(
    value: Value,
    variable_name: &str,
    document: &ParsedDocument,
    field: &RootFieldSelection,
) -> Value {
    let (variable_type, location) = document
        .variable_definitions
        .get(variable_name)
        .map(|definition| (definition.type_display.as_str(), definition.location))
        .unwrap_or(("ProductDeleteInput!", field.location));
    let message = format!("Variable ${variable_name} of type {variable_type} was provided invalid value for id (Expected value to not be null)");
    invalid_variable_error_envelope(
        message,
        location,
        value,
        json!([{ "path": ["id"], "explanation": "Expected value to not be null" }]),
    )
}

pub(in crate::proxy) fn product_variant_media_user_error(
    field: &[&str],
    message: &str,
    code: &str,
) -> Value {
    user_error(field, message, Some(code))
}

pub(in crate::proxy) fn variant_media_ids_from_json(value: &Value) -> Vec<String> {
    value
        .get("mediaIds")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|media_id| media_id.as_str().map(str::to_string))
        .chain(
            value
                .get("media")
                .and_then(|connection| connection.get("nodes"))
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|media| media.get("id").and_then(Value::as_str).map(str::to_string)),
        )
        .fold(Vec::new(), |mut ids, id| {
            if !ids.iter().any(|existing| existing == &id) {
                ids.push(id);
            }
            ids
        })
}

#[cfg(test)]
mod product_variant_connection_tests {
    use super::*;

    #[test]
    fn observed_variant_without_inventory_item_does_not_invent_cross_resource_identity() {
        let variant = product_variant_state_from_observed_json(&json!({
            "id": "gid://shopify/ProductVariant/424242",
            "product": { "id": "gid://shopify/Product/1" },
            "title": "Partially observed variant",
            "price": "10.00",
        }))
        .expect("partially observed variant should normalize");

        assert_eq!(variant.inventory_item.id, "");
        assert_ne!(
            variant.inventory_item.id,
            "gid://shopify/InventoryItem/424242"
        );
    }

    #[test]
    fn staged_variant_overlays_its_observed_connection_position() {
        let product_id = "gid://shopify/Product/1";
        let variant_ids = [
            "gid://shopify/ProductVariant/1",
            "gid://shopify/ProductVariant/2",
            "gid://shopify/ProductVariant/3",
        ];
        let fallback = variant_ids
            .iter()
            .enumerate()
            .map(|(index, id)| {
                json!({
                    "id": id,
                    "title": format!("Variant {}", index + 1),
                    "sku": format!("SKU-{}", index + 1),
                    "price": format!("{}.00", index + 1),
                })
            })
            .collect::<Vec<_>>();
        let staged = product_variant_state_from_observed_json(&json!({
            "id": variant_ids[1],
            "title": "Updated variant 2",
            "sku": "SKU-2",
            "price": "22.00",
            "product": { "id": product_id },
        }))
        .expect("observed variant should normalize");

        let connection =
            product_variant_connection_with_fallback_value(&[staged], &fallback, &BTreeMap::new());
        let nodes = connection["nodes"]
            .as_array()
            .expect("connection should have nodes");
        assert_eq!(
            nodes
                .iter()
                .filter_map(|node| node["id"].as_str())
                .collect::<Vec<_>>(),
            variant_ids
        );
        assert_eq!(nodes[1]["title"], json!("Updated variant 2"));
        assert_eq!(nodes[1]["price"], json!("22.00"));
    }

    #[test]
    fn complete_positioned_variant_state_replaces_stale_embedded_order() {
        let product_id = "gid://shopify/Product/1";
        let first_id = "gid://shopify/ProductVariant/1";
        let second_id = "gid://shopify/ProductVariant/2";
        let fallback = vec![
            json!({ "id": first_id, "title": "Red", "price": "1.00" }),
            json!({ "id": second_id, "title": "Green", "price": "2.00" }),
        ];
        let reordered = [
            json!({
                "id": second_id,
                "title": "Small / Green",
                "price": "2.00",
                "position": 1,
                "product": { "id": product_id },
            }),
            json!({
                "id": first_id,
                "title": "Small / Red",
                "price": "1.00",
                "position": 2,
                "product": { "id": product_id },
            }),
        ]
        .iter()
        .map(product_variant_state_from_observed_json)
        .collect::<Option<Vec<_>>>()
        .expect("positioned variants should normalize");

        let connection =
            product_variant_connection_with_fallback_value(&reordered, &fallback, &BTreeMap::new());
        assert_eq!(
            connection["nodes"]
                .as_array()
                .expect("connection should have nodes")
                .iter()
                .filter_map(|node| node["id"].as_str())
                .collect::<Vec<_>>(),
            vec![second_id, first_id]
        );
    }
}

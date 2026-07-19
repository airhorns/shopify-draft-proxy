use super::b2b_customers::{
    customer_address_cursor, customer_address_dedup_key, customer_address_input_node,
    customer_address_nodes, customer_rebuild_addresses,
};
use super::*;
use crate::graphql::operation_directive_invocations;
use base64::Engine as _;
use sha2::{Digest, Sha256};

const STOREFRONT_FIRST_SLICE_ROOTS: &[&str] = &[
    "shop",
    "localization",
    "locations",
    "paymentSettings",
    "publicApiVersions",
    "product",
    "productByHandle",
    "productRecommendations",
    "productTags",
    "productTypes",
    "products",
];
const STOREFRONT_CONTENT_ROOTS: &[&str] = &[
    "article",
    "articles",
    "blog",
    "blogByHandle",
    "blogs",
    "page",
    "pageByHandle",
    "pages",
];
const STOREFRONT_LOCAL_CONTENT_ROOTS: &[&str] = &[
    "article",
    "articles",
    "blog",
    "blogByHandle",
    "blogs",
    "menu",
    "page",
    "pageByHandle",
    "pages",
    "sitemap",
    "urlRedirects",
];
const STOREFRONT_CUSTOM_DATA_ROOTS: &[&str] = &["metaobject", "metaobjects"];
const STOREFRONT_COLLECTION_ROOTS: &[&str] = &["collection", "collectionByHandle", "collections"];
const STOREFRONT_DISCOVERY_ROOTS: &[&str] = &["node", "nodes", "search", "predictiveSearch"];
const STOREFRONT_CAPTURED_COLLECTION_DEFAULT_ORDER_FIELD: &str =
    "__storefrontCapturedDefaultProductOrder";
pub(in crate::proxy) const STOREFRONT_CUSTOMER_AUTH_MUTATION_ROOTS: &[&str] = &[
    "customerCreate",
    "customerAccessTokenCreate",
    "customerAccessTokenRenew",
    "customerAccessTokenDelete",
    "customerActivate",
    "customerActivateByUrl",
    "customerRecover",
    "customerReset",
    "customerResetByUrl",
    "customerAccessTokenCreateWithMultipass",
    "customerUpdate",
    "customerAddressCreate",
    "customerAddressUpdate",
    "customerAddressDelete",
    "customerDefaultAddressUpdate",
];
pub(in crate::proxy) const STOREFRONT_CART_MUTATION_ROOTS: &[&str] = &[
    "cartCreate",
    "cartLinesAdd",
    "cartLinesUpdate",
    "cartLinesRemove",
    "cartAttributesUpdate",
    "cartNoteUpdate",
    "cartBuyerIdentityUpdate",
    "cartDiscountCodesUpdate",
    "cartGiftCardCodesAdd",
    "cartGiftCardCodesRemove",
    "cartGiftCardCodesUpdate",
    "cartMetafieldsSet",
    "cartMetafieldDelete",
    "cartDeliveryAddressesAdd",
    "cartDeliveryAddressesUpdate",
    "cartDeliveryAddressesRemove",
    "cartDeliveryAddressesReplace",
    "cartSelectedDeliveryOptionsUpdate",
];
const STOREFRONT_DEFAULT_CONTEXT_KEY: &str = "country=*;language=*";
const STOREFRONT_CUSTOMER_PASSWORD_FINGERPRINT_FIELD: &str = "__storefrontPasswordFingerprint";
const STOREFRONT_CUSTOMER_RESET_TOKEN_HASH_FIELD: &str = "__storefrontResetTokenHash";
const STOREFRONT_CUSTOMER_RESET_REQUESTED_AT_FIELD: &str = "__storefrontResetRequestedAt";
const STOREFRONT_CUSTOMER_ACTIVATION_TOKEN_FIELD: &str = "__proxyAccountActivationToken";

const STOREFRONT_FIRST_SLICE_HYDRATE_QUERY: &str =
    include_str!("../../config/parity-requests/storefront/storefront-first-slice-hydrate.graphql");
const STOREFRONT_FIRST_SLICE_CONTEXT_HYDRATE_QUERY: &str = include_str!(
    "../../config/parity-requests/storefront/storefront-first-slice-hydrate-context.graphql"
);
const STOREFRONT_ENRICHMENT_TAXONOMY_HYDRATE_QUERY: &str = include_str!(
    "../../config/parity-requests/storefront/storefront-enrichment-taxonomy-hydrate.graphql"
);
const STOREFRONT_ENRICHMENT_CONTEXT_HYDRATE_QUERY: &str = include_str!(
    "../../config/parity-requests/storefront/storefront-enrichment-context-hydrate.graphql"
);
const STOREFRONT_MENU_HYDRATE_QUERY: &str =
    include_str!("../../config/parity-requests/storefront/storefront-content-menu-hydrate.graphql");

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StorefrontContentKind {
    Blog,
    Page,
    Article,
}

#[derive(Clone)]
enum StorefrontSearchItem {
    Product(Box<ProductRecord>),
    Article(Value),
    Page(Value),
}

#[derive(Clone, Copy)]
enum StorefrontProductTaxonomyKind {
    Tag,
    ProductType,
}

#[derive(Clone)]
pub(in crate::proxy) struct StorefrontVariantPricing {
    pub(in crate::proxy) price: String,
    pub(in crate::proxy) compare_at_price: Option<String>,
    pub(in crate::proxy) currency_code: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(in crate::proxy) struct StorefrontRequestContext {
    pub(in crate::proxy) country: Option<String>,
    pub(in crate::proxy) language: Option<String>,
    pub(in crate::proxy) preferred_location_id: Option<String>,
    pub(in crate::proxy) buyer_customer_access_token: Option<String>,
    pub(in crate::proxy) buyer_company_location_id: Option<String>,
    pub(in crate::proxy) uses_enrichment_context: bool,
}

impl StorefrontRequestContext {
    fn key(&self) -> String {
        match (
            self.country.as_deref(),
            self.language.as_deref(),
            self.buyer_company_location_id.as_deref(),
        ) {
            (None, None, None) => STOREFRONT_DEFAULT_CONTEXT_KEY.to_string(),
            (country, language, None) => format!(
                "country={};language={}",
                country.unwrap_or("*"),
                language.unwrap_or("*")
            ),
            (country, language, Some(company_location_id)) => format!(
                "country={};language={};companyLocation={}",
                country.unwrap_or("*"),
                language.unwrap_or("*"),
                company_location_id
            ),
        }
    }

    fn has_in_context_values(&self) -> bool {
        self.country.is_some() || self.language.is_some()
    }

    pub(in crate::proxy) fn invalid_buyer_token(&self, proxy: &DraftProxy) -> bool {
        self.buyer_customer_access_token
            .as_deref()
            .is_some_and(|token| {
                proxy
                    .storefront_customer_id_for_access_token(token)
                    .is_none()
            })
    }
}

struct StorefrontCustomerAuthOutcome {
    value: Value,
    errors: Vec<crate::admin_graphql::RootFieldError>,
}

fn storefront_argument_string(arguments: &BTreeMap<String, Value>, name: &str) -> String {
    arguments
        .get(name)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn storefront_argument_object<'a>(
    arguments: &'a BTreeMap<String, Value>,
    name: &str,
) -> Option<&'a serde_json::Map<String, Value>> {
    arguments.get(name).and_then(Value::as_object)
}

fn storefront_input_string(input: Option<&serde_json::Map<String, Value>>, name: &str) -> String {
    input
        .and_then(|input| input.get(name))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn storefront_input_bool(
    input: Option<&serde_json::Map<String, Value>>,
    name: &str,
) -> Option<bool> {
    input
        .and_then(|input| input.get(name))
        .and_then(Value::as_bool)
}

pub(in crate::proxy) struct StorefrontCustomerAuthLogDetails<'a> {
    pub status: &'a str,
    pub execution: &'a str,
    pub notes: &'a str,
}

pub(in crate::proxy) fn storefront_field_resolver_registrations() -> Vec<FieldResolverRegistration>
{
    let mut registrations = Vec::new();
    for (parent_type, fields) in [
        (
            "Article",
            &[
                "contentHtml",
                "excerptHtml",
                "handle",
                "id",
                "image",
                "publishedAt",
                "tags",
                "title",
            ][..],
        ),
        ("Blog", &["handle", "id", "title"]),
        (
            "Page",
            &[
                "body",
                "bodySummary",
                "createdAt",
                "handle",
                "id",
                "title",
                "updatedAt",
            ],
        ),
        (
            "ArticleAuthor",
            &["bio", "email", "firstName", "lastName", "name"],
        ),
        ("SEO", &["description", "title"]),
        (
            "Image",
            &[
                "altText",
                "height",
                "id",
                "originalSrc",
                "src",
                "thumbhash",
                "width",
            ],
        ),
        ("SitemapImage", &["alt", "filepath", "updatedAt"]),
        ("ApiVersion", &["displayName", "handle", "supported"]),
        (
            "PaymentSettings",
            &[
                "acceptedCardBrands",
                "cardVaultUrl",
                "countryCode",
                "currencyCode",
                "enabledPresentmentCurrencies",
                "shopifyPaymentsAccountId",
                "supportedDigitalWallets",
            ],
        ),
        (
            "Localization",
            &[
                "availableCountries",
                "availableLanguages",
                "country",
                "language",
                "market",
            ],
        ),
        (
            "Country",
            &[
                "availableLanguages",
                "currency",
                "defaultLanguage",
                "isoCode",
                "market",
                "name",
                "unitSystem",
            ],
        ),
        ("Currency", &["isoCode", "name", "symbol"]),
        ("Language", &["endonymName", "isoCode", "name"]),
        ("Market", &["handle", "id"]),
        ("LocationConnection", &["edges", "nodes", "pageInfo"]),
        ("LocationEdge", &["cursor", "node"]),
        ("Location", &["address", "id", "name"]),
        (
            "LocationAddress",
            &[
                "address1",
                "address2",
                "city",
                "country",
                "countryCode",
                "formatted",
                "latitude",
                "longitude",
                "phone",
                "province",
                "provinceCode",
                "zip",
            ],
        ),
        (
            "Shop",
            &[
                "brand",
                "contactInformation",
                "customerAccountTranslations",
                "customerAccountUrl",
                "description",
                "id",
                "legalNotice",
                "moneyFormat",
                "name",
                "paymentSettings",
                "primaryDomain",
                "privacyPolicy",
                "refundPolicy",
                "shippingPolicy",
                "shipsToCountries",
                "shopPayInstallmentsPricing",
                "socialLoginProviders",
                "subscriptionPolicy",
                "termsOfSale",
                "termsOfService",
            ],
        ),
        (
            "Brand",
            &[
                "colors",
                "coverImage",
                "logo",
                "shortDescription",
                "slogan",
                "squareLogo",
            ],
        ),
        ("BrandColors", &["primary", "secondary"]),
        ("BrandColorGroup", &["background", "foreground"]),
        ("Domain", &["host", "sslEnabled", "url"]),
        ("ShopPolicy", &["body", "handle", "id", "title", "url"]),
        (
            "ShopPolicyWithDefault",
            &["body", "handle", "id", "title", "url"],
        ),
        ("Translation", &["key", "value"]),
        ("Menu", &["handle", "id", "items", "itemsCount", "title"]),
        (
            "MenuItem",
            &[
                "id",
                "items",
                "resource",
                "resourceId",
                "tags",
                "title",
                "type",
                "url",
            ],
        ),
        ("UrlRedirectConnection", &["edges", "nodes", "pageInfo"]),
        ("UrlRedirectEdge", &["cursor", "node"]),
        ("UrlRedirect", &["id", "path", "target"]),
        ("Count", &["count", "precision"]),
        ("PaginatedSitemapResources", &["hasNextPage", "items"]),
        (
            "SitemapResource",
            &["handle", "image", "title", "updatedAt"],
        ),
        ("ArticleConnection", &["edges", "nodes", "pageInfo"]),
        ("ArticleEdge", &["cursor", "node"]),
        ("BlogConnection", &["edges", "nodes", "pageInfo"]),
        ("BlogEdge", &["cursor", "node"]),
        ("PageConnection", &["edges", "nodes", "pageInfo"]),
        ("PageEdge", &["cursor", "node"]),
        ("CommentConnection", &["edges", "nodes", "pageInfo"]),
        ("CommentEdge", &["cursor", "node"]),
        (
            "PageInfo",
            &["endCursor", "hasNextPage", "hasPreviousPage", "startCursor"],
        ),
        (
            "Metaobject",
            &[
                "fields",
                "handle",
                "id",
                "onlineStoreUrl",
                "seo",
                "type",
                "updatedAt",
            ],
        ),
        ("MetaobjectField", &["key", "type", "value"]),
        ("MetaobjectSEO", &["description", "title"]),
        (
            "Metafield",
            &[
                "createdAt",
                "description",
                "id",
                "key",
                "list",
                "namespace",
                "type",
                "updatedAt",
                "value",
            ],
        ),
        (
            "MetafieldReferenceConnection",
            &["edges", "nodes", "pageInfo"],
        ),
        ("MetafieldReferenceEdge", &["cursor", "node"]),
        (
            "Customer",
            &[
                "acceptsMarketing",
                "avatarUrl",
                "createdAt",
                "defaultAddress",
                "displayName",
                "email",
                "firstName",
                "id",
                "lastName",
                "numberOfOrders",
                "phone",
                "socialLoginProvider",
                "tags",
                "updatedAt",
            ],
        ),
        (
            "MailingAddress",
            &[
                "address1",
                "address2",
                "city",
                "company",
                "country",
                "countryCode",
                "countryCodeV2",
                "firstName",
                "formattedArea",
                "id",
                "lastName",
                "latitude",
                "longitude",
                "name",
                "phone",
                "province",
                "provinceCode",
                "zip",
            ],
        ),
        (
            "Order",
            &[
                "currencyCode",
                "customerUrl",
                "email",
                "financialStatus",
                "fulfillmentStatus",
                "id",
                "name",
                "orderNumber",
                "phone",
                "processedAt",
                "subtotalPriceV2",
                "totalPrice",
                "totalPriceV2",
            ],
        ),
        (
            "Product",
            &[
                "availableForSale",
                "compareAtPriceRange",
                "createdAt",
                "descriptionHtml",
                "handle",
                "id",
                "isGiftCard",
                "onlineStoreUrl",
                "priceRange",
                "productType",
                "publishedAt",
                "requiresSellingPlan",
                "seo",
                "tags",
                "title",
                "totalInventory",
                "updatedAt",
                "variantsCount",
                "vendor",
            ],
        ),
        (
            "Collection",
            &[
                "descriptionHtml",
                "handle",
                "id",
                "image",
                "onlineStoreUrl",
                "seo",
                "title",
                "trackingParameters",
                "updatedAt",
            ],
        ),
        (
            "ProductVariant",
            &[
                "availableForSale",
                "barcode",
                "compareAtPrice",
                "compareAtPriceV2",
                "components",
                "currentlyNotInStock",
                "groupedBy",
                "id",
                "price",
                "priceV2",
                "quantityAvailable",
                "quantityPriceBreaks",
                "quantityRule",
                "requiresComponents",
                "requiresShipping",
                "selectedOptions",
                "shopPayInstallmentsPricing",
                "sku",
                "storeAvailability",
                "taxable",
                "title",
                "unitPrice",
                "unitPriceMeasurement",
                "weight",
                "weightUnit",
            ],
        ),
        ("ProductOption", &["id", "name", "optionValues", "values"]),
        (
            "ProductOptionValue",
            &["firstSelectableVariant", "id", "name", "swatch"],
        ),
        ("ProductOptionValueSwatch", &["color", "image"]),
        ("ProductPriceRange", &["maxVariantPrice", "minVariantPrice"]),
        ("SellingPlanGroup", &["appName", "name", "options"]),
        (
            "SellingPlan",
            &[
                "billingPolicy",
                "checkoutCharge",
                "deliveryPolicy",
                "description",
                "id",
                "name",
                "options",
                "priceAdjustments",
                "recurringDeliveries",
            ],
        ),
        ("MoneyV2", &["amount", "currencyCode"]),
        ("SelectedOption", &["name", "value"]),
    ] {
        for field in fields {
            registrations.push(FieldResolverRegistration::property(
                ApiSurface::Storefront,
                parent_type,
                field,
            ));
        }
    }
    for (parent_type, field, handler) in [
        (
            "Blog",
            "articleByHandle",
            storefront_blog_article_by_handle_field
                as crate::resolver_registry::FieldResolverHandler,
        ),
        ("Blog", "articles", storefront_blog_articles_field),
        ("Blog", "authors", storefront_blog_authors_field),
        ("Blog", "metafield", storefront_null_field),
        ("Blog", "metafields", storefront_metafields_field),
        ("Blog", "onlineStoreUrl", storefront_null_field),
        ("Blog", "seo", storefront_seo_field),
        ("Article", "author", storefront_article_author_field),
        ("Article", "authorV2", storefront_article_author_field),
        ("Article", "blog", storefront_article_blog_field),
        ("Article", "comments", storefront_empty_connection_field),
        ("Article", "content", storefront_truncated_content_field),
        ("Article", "excerpt", storefront_truncated_excerpt_field),
        ("Article", "metafield", storefront_null_field),
        ("Article", "metafields", storefront_metafields_field),
        ("Article", "onlineStoreUrl", storefront_null_field),
        ("Article", "seo", storefront_seo_field),
        ("Article", "trackingParameters", storefront_null_field),
        ("Page", "metafield", storefront_null_field),
        ("Page", "metafields", storefront_metafields_field),
        ("Page", "onlineStoreUrl", storefront_null_field),
        ("Page", "seo", storefront_seo_field),
        ("Page", "trackingParameters", storefront_null_field),
        ("Location", "metafield", storefront_null_field),
        ("Location", "metafields", storefront_metafields_field),
        ("Market", "metafield", storefront_null_field),
        ("Market", "metafields", storefront_metafields_field),
        ("Shop", "metafield", storefront_shop_metafield_field),
        ("Shop", "metafields", storefront_shop_metafields_field),
        ("Metaobject", "field", storefront_metaobject_field_field),
        ("MetaobjectField", "reference", storefront_reference_field),
        ("MetaobjectField", "references", storefront_references_field),
        (
            "Metafield",
            "parentResource",
            storefront_parent_resource_field,
        ),
        ("Metafield", "reference", storefront_reference_field),
        ("Metafield", "references", storefront_references_field),
        ("Customer", "addresses", storefront_customer_addresses_field),
        ("Customer", "orders", storefront_customer_orders_field),
        ("Customer", "metafield", storefront_owner_metafield_field),
        ("Customer", "metafields", storefront_owner_metafields_field),
        (
            "MailingAddress",
            "formatted",
            storefront_mailing_address_formatted_field,
        ),
        (
            "Product",
            "description",
            storefront_product_description_field,
        ),
        (
            "Product",
            "featuredImage",
            storefront_product_featured_image_field,
        ),
        ("Product", "images", storefront_product_images_field),
        ("Product", "media", storefront_product_media_field),
        ("Product", "metafield", storefront_owner_metafield_field),
        ("Product", "metafields", storefront_owner_metafields_field),
        ("Product", "options", storefront_product_options_field),
        (
            "Product",
            "selectedOrFirstAvailableVariant",
            storefront_product_selected_variant_field,
        ),
        (
            "Product",
            "variantBySelectedOptions",
            storefront_product_variant_by_options_field,
        ),
        ("Product", "variants", storefront_product_variants_field),
        (
            "Product",
            "sellingPlanGroups",
            storefront_product_selling_plan_groups_field,
        ),
        ("ProductVariant", "image", storefront_variant_image_field),
        (
            "ProductVariant",
            "metafield",
            storefront_owner_metafield_field,
        ),
        (
            "ProductVariant",
            "metafields",
            storefront_owner_metafields_field,
        ),
        (
            "ProductVariant",
            "product",
            storefront_variant_product_field,
        ),
        (
            "ProductVariant",
            "sellingPlanAllocations",
            storefront_variant_selling_plan_allocations_field,
        ),
        (
            "SellingPlanGroup",
            "sellingPlans",
            storefront_selling_plan_group_selling_plans_field,
        ),
        ("SellingPlan", "metafield", storefront_null_field),
        ("SellingPlan", "metafields", storefront_metafields_field),
        (
            "Collection",
            "description",
            storefront_collection_description_field,
        ),
        ("Collection", "metafield", storefront_owner_metafield_field),
        (
            "Collection",
            "metafields",
            storefront_owner_metafields_field,
        ),
        (
            "Collection",
            "products",
            storefront_collection_products_field,
        ),
        ("Image", "transformedSrc", storefront_image_url_field),
        ("Image", "url", storefront_image_url_field),
        (
            "Sitemap",
            "pagesCount",
            storefront_sitemap_pages_count_field,
        ),
        ("Sitemap", "resources", storefront_sitemap_resources_field),
    ] {
        let registration = FieldResolverRegistration::explicit(
            ApiSurface::Storefront,
            parent_type,
            field,
            handler,
        );
        registrations.push(registration);
    }
    registrations.extend(super::storefront_cart::storefront_cart_field_resolver_registrations());
    registrations
}

pub(in crate::proxy) fn storefront_field_resolver_type_policies() -> Vec<FieldResolverTypePolicy> {
    let mut policies = [
        "Shop",
        "Metaobject",
        "MetaobjectField",
        "Metafield",
        "Customer",
        "MailingAddress",
        "Product",
        "ProductVariant",
        "Collection",
        "SellingPlanGroup",
        "SellingPlan",
        "ProductOption",
        "ProductOptionValue",
        "ProductOptionValueSwatch",
        "ProductPriceRange",
    ]
    .into_iter()
    .map(|parent_type| {
        FieldResolverTypePolicy::unsupported_remaining(
            ApiSurface::Storefront,
            parent_type,
            "the local Storefront model does not implement this field",
        )
    })
    .collect::<Vec<_>>();
    policies.extend(
        [
            "BaseCartLine",
            "Cart",
            "CartDeliveryAddress",
            "CartDeliveryGroup",
            "CartLine",
            "Comment",
            "Company",
            "CompanyLocation",
            "ComponentizableCartLine",
            "MediaPresentation",
        ]
        .into_iter()
        .map(|parent_type| {
            FieldResolverTypePolicy::property_backed_ordinary_fields(
                ApiSurface::Storefront,
                parent_type,
                "argument-bearing Storefront field has no explicit canonical resolver",
            )
        }),
    );
    policies
}

fn storefront_parent_string<'a>(
    invocation: &'a crate::admin_graphql::FieldResolverInvocation,
    field: &str,
) -> &'a str {
    invocation
        .parent
        .get(field)
        .and_then(Value::as_str)
        .unwrap_or_default()
}

fn storefront_null_field(
    _proxy: &mut DraftProxy,
    _request: &Request,
    _invocation: &crate::admin_graphql::FieldResolverInvocation,
) -> Result<Value, String> {
    Ok(Value::Null)
}

fn storefront_seo_field(
    _proxy: &mut DraftProxy,
    _request: &Request,
    _invocation: &crate::admin_graphql::FieldResolverInvocation,
) -> Result<Value, String> {
    Ok(storefront_default_seo())
}

fn storefront_image_url_field(
    _proxy: &mut DraftProxy,
    _request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> Result<Value, String> {
    Ok(invocation
        .parent
        .get("_url")
        .or_else(|| invocation.parent.get("url"))
        .or_else(|| invocation.parent.get("src"))
        .cloned()
        .unwrap_or(Value::Null))
}

fn storefront_collection_record_for_parent(proxy: &DraftProxy, parent: &Value) -> Option<Value> {
    parent
        .get("id")
        .and_then(Value::as_str)
        .and_then(|id| proxy.store.collection_by_id(id))
        .cloned()
        .or_else(|| parent.is_object().then(|| parent.clone()))
}

fn storefront_collection_description_field(
    proxy: &mut DraftProxy,
    _request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> Result<Value, String> {
    let Some(collection) = storefront_collection_record_for_parent(proxy, invocation.parent) else {
        return Ok(json!(""));
    };
    let description = collection
        .get("description")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| {
            storefront_strip_html(
                collection
                    .get("descriptionHtml")
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
            )
        });
    let Some(limit) = invocation
        .arguments
        .get("truncateAt")
        .and_then(Value::as_i64)
        .and_then(|limit| (limit >= 0).then_some(limit as usize))
    else {
        return Ok(json!(description));
    };
    if description.chars().count() <= limit {
        return Ok(json!(description));
    }
    let prefix_len = limit.saturating_sub(3);
    Ok(json!(format!(
        "{}...",
        description.chars().take(prefix_len).collect::<String>()
    )))
}

fn storefront_owner_metafield_field(
    proxy: &mut DraftProxy,
    _request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> Result<Value, String> {
    let owner_id = invocation
        .parent
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let namespace = invocation
        .arguments
        .get("namespace")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let key = invocation
        .arguments
        .get("key")
        .and_then(Value::as_str)
        .unwrap_or_default();
    Ok(proxy
        .storefront_owner_metafield(owner_id, namespace, key)
        .map(storefront_metafield_runtime_value)
        .unwrap_or(Value::Null))
}

fn storefront_owner_metafields_field(
    proxy: &mut DraftProxy,
    _request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> Result<Value, String> {
    let owner_id = invocation
        .parent
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or_default();
    Ok(Value::Array(
        invocation
            .arguments
            .get("identifiers")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .map(|identifier| {
                let namespace = identifier
                    .get("namespace")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let key = identifier
                    .get("key")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                proxy
                    .storefront_owner_metafield(owner_id, namespace, key)
                    .map(storefront_metafield_runtime_value)
                    .unwrap_or(Value::Null)
            })
            .collect(),
    ))
}

fn storefront_collection_products_field(
    proxy: &mut DraftProxy,
    request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> Result<Value, String> {
    let Some(collection) = storefront_collection_record_for_parent(proxy, invocation.parent) else {
        return Ok(connection_json(Vec::new()));
    };
    let arguments = resolved_arguments_from_json(&invocation.arguments);
    let filters = resolved_object_list_field(&arguments, "filters");
    let mut products = proxy
        .collection_product_entries(&collection)
        .into_iter()
        .filter(|entry| proxy.storefront_product_is_visible(&entry.product))
        .filter(|entry| storefront_collection_product_matches_filters(entry, &filters))
        .collect::<Vec<_>>();
    let requested_sort_key = resolved_string_field(&arguments, "sortKey");
    let sort_key = if matches!(
        requested_sort_key.as_deref(),
        None | Some("COLLECTION_DEFAULT")
    ) && collection
        .get(STOREFRONT_CAPTURED_COLLECTION_DEFAULT_ORDER_FIELD)
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        Some("MANUAL")
    } else {
        requested_sort_key.as_deref()
    };
    let reverse = resolved_bool_field(&arguments, "reverse").unwrap_or(false);
    sort_collection_product_entries(&collection, &mut products, sort_key, reverse);
    let (products, page_info) = connection_window(&products, &arguments, collection_product_cursor);
    let context = storefront_request_context_from_request(request);
    Ok(typed_connection_value(
        &products,
        |entry| storefront_product_value(proxy, &entry.product, &entry.variants, &context),
        collection_product_cursor,
        page_info,
    ))
}

fn storefront_metafields_field(
    _proxy: &mut DraftProxy,
    _request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation,
) -> Result<Value, String> {
    let count = invocation
        .arguments
        .get("identifiers")
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or_default();
    Ok(Value::Array(vec![Value::Null; count]))
}

fn storefront_shop_metafield_field(
    proxy: &mut DraftProxy,
    _request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation,
) -> Result<Value, String> {
    let Some(owner_id) = proxy.storefront_shop_owner_id() else {
        return Ok(Value::Null);
    };
    let namespace = invocation
        .arguments
        .get("namespace")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let key = invocation
        .arguments
        .get("key")
        .and_then(Value::as_str)
        .unwrap_or_default();
    Ok(proxy
        .storefront_owner_metafield(&owner_id, namespace, key)
        .map(storefront_metafield_runtime_value)
        .unwrap_or(Value::Null))
}

fn storefront_shop_metafields_field(
    proxy: &mut DraftProxy,
    _request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation,
) -> Result<Value, String> {
    let Some(owner_id) = proxy.storefront_shop_owner_id() else {
        return Ok(Value::Array(Vec::new()));
    };
    let values = invocation
        .arguments
        .get("identifiers")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|identifier| {
            let namespace = identifier
                .get("namespace")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let key = identifier
                .get("key")
                .and_then(Value::as_str)
                .unwrap_or_default();
            proxy
                .storefront_owner_metafield(&owner_id, namespace, key)
                .map(storefront_metafield_runtime_value)
                .unwrap_or(Value::Null)
        })
        .collect();
    Ok(Value::Array(values))
}

fn storefront_metafield_runtime_value(mut metafield: Value) -> Value {
    let list = metafield
        .get("type")
        .and_then(Value::as_str)
        .is_some_and(|field_type| field_type.starts_with("list."));
    if let Some(object) = metafield.as_object_mut() {
        object
            .entry("__typename".to_string())
            .or_insert_with(|| json!("Metafield"));
        object.insert("list".to_string(), json!(list));
        object
            .entry("description".to_string())
            .or_insert(Value::Null);
    }
    metafield
}

fn storefront_metaobject_field_field(
    _proxy: &mut DraftProxy,
    _request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation,
) -> Result<Value, String> {
    let key = invocation
        .arguments
        .get("key")
        .and_then(Value::as_str)
        .unwrap_or_default();
    Ok(invocation
        .parent
        .get("fields")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find(|field| field.get("key").and_then(Value::as_str) == Some(key))
        .cloned()
        .unwrap_or(Value::Null))
}

fn storefront_reference_field(
    proxy: &mut DraftProxy,
    _request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation,
) -> Result<Value, String> {
    let Some(id) = scalar_reference_id(invocation.parent) else {
        return Ok(Value::Null);
    };
    Ok(proxy
        .storefront_reference_node_value(&id)
        .unwrap_or(Value::Null))
}

fn storefront_references_field(
    proxy: &mut DraftProxy,
    _request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation,
) -> Result<Value, String> {
    Ok(proxy.storefront_reference_connection_value(invocation.parent, &invocation.arguments))
}

fn storefront_parent_resource_field(
    proxy: &mut DraftProxy,
    _request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation,
) -> Result<Value, String> {
    if let Some(parent_resource) = invocation.parent.get("parentResource") {
        return Ok(parent_resource.clone());
    }
    let Some(owner_id) = invocation
        .parent
        .pointer("/owner/id")
        .and_then(Value::as_str)
    else {
        return Ok(Value::Null);
    };
    Ok(proxy
        .storefront_reference_node_value(owner_id)
        .unwrap_or(Value::Null))
}

fn storefront_customer_record_for_parent(proxy: &DraftProxy, parent: &Value) -> Option<Value> {
    parent
        .get("id")
        .and_then(Value::as_str)
        .and_then(|id| proxy.storefront_customer_by_id(id))
        .or_else(|| parent.is_object().then(|| parent.clone()))
}

fn storefront_customer_addresses_field(
    proxy: &mut DraftProxy,
    _request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> Result<Value, String> {
    let Some(customer) = storefront_customer_record_for_parent(proxy, invocation.parent) else {
        return Ok(connection_json(Vec::new()));
    };
    let addresses = customer_address_nodes(&customer);
    let arguments = resolved_arguments_from_json(&invocation.arguments);
    let (addresses, page_info) = connection_window(&addresses, &arguments, |address| {
        customer_address_cursor(address).unwrap_or_default()
    });
    Ok(typed_connection_value(
        &addresses,
        storefront_mailing_address_json,
        |address| customer_address_cursor(address).unwrap_or_default(),
        page_info,
    ))
}

fn storefront_customer_orders_field(
    proxy: &mut DraftProxy,
    _request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> Result<Value, String> {
    let customer_id = invocation
        .parent
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let Some(customer) = storefront_customer_record_for_parent(proxy, invocation.parent) else {
        return Ok(connection_json(Vec::new()));
    };
    let orders = proxy
        .store
        .staged
        .customer_orders
        .get(customer_id)
        .cloned()
        .unwrap_or_else(|| connection_nodes(&customer["orders"]));
    let arguments = resolved_arguments_from_json(&invocation.arguments);
    let (orders, page_info) = connection_window(&orders, &arguments, storefront_order_cursor);
    Ok(typed_connection_value(
        &orders,
        storefront_order_json,
        storefront_order_cursor,
        page_info,
    ))
}

pub(in crate::proxy) fn storefront_mailing_address_formatted_field(
    _proxy: &mut DraftProxy,
    _request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> Result<Value, String> {
    let mut lines = invocation
        .parent
        .get("formatted")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_else(|| {
            storefront_formatted_address_lines(invocation.parent)
                .as_array()
                .cloned()
                .unwrap_or_default()
        });
    if invocation
        .arguments
        .get("withName")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        if let Some(name) = invocation.parent.get("name").and_then(Value::as_str) {
            if !name.is_empty() && lines.first().and_then(Value::as_str) != Some(name) {
                lines.insert(0, json!(name));
            }
        }
    }
    if invocation
        .arguments
        .get("withCompany")
        .and_then(Value::as_bool)
        .unwrap_or(true)
    {
        if let Some(company) = invocation.parent.get("company").and_then(Value::as_str) {
            if !company.is_empty() && !lines.iter().any(|line| line.as_str() == Some(company)) {
                let index = usize::from(
                    invocation
                        .arguments
                        .get("withName")
                        .and_then(Value::as_bool)
                        .unwrap_or(false),
                );
                lines.insert(index.min(lines.len()), json!(company));
            }
        }
    }
    Ok(Value::Array(lines))
}

fn storefront_product_description_field(
    _proxy: &mut DraftProxy,
    _request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation,
) -> Result<Value, String> {
    let description = invocation
        .parent
        .get("description")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let limit = invocation
        .arguments
        .get("truncateAt")
        .and_then(Value::as_i64)
        .filter(|limit| *limit >= 0)
        .map(|limit| limit as usize);
    Ok(Value::String(match limit {
        Some(limit) => description.chars().take(limit).collect(),
        None => description.to_string(),
    }))
}

fn storefront_product_options_field(
    proxy: &mut DraftProxy,
    _request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation,
) -> Result<Value, String> {
    let Some(product) = storefront_product_record_for_parent(proxy, invocation.parent) else {
        return Ok(Value::Array(Vec::new()));
    };
    let variants = proxy.store.product_variants_for_product(&product.id);
    Ok(storefront_product_options_value(
        &product,
        &variants,
        invocation.arguments.get("first").and_then(Value::as_i64),
    ))
}

fn storefront_product_variants_field(
    proxy: &mut DraftProxy,
    request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation,
) -> Result<Value, String> {
    let Some(product) = storefront_product_record_for_parent(proxy, invocation.parent) else {
        return Ok(connection_json(Vec::new()));
    };
    let variants = proxy.store.product_variants_for_product(&product.id);
    let context = storefront_request_context_from_request(request);
    Ok(storefront_product_variants_connection_value(
        proxy,
        &product,
        &variants,
        &context,
        &invocation.arguments,
    ))
}

fn storefront_product_images_field(
    proxy: &mut DraftProxy,
    _request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation,
) -> Result<Value, String> {
    proxy.promote_all_product_media_ready_on_read();
    let Some(product) = storefront_product_record_for_parent(proxy, invocation.parent) else {
        return Ok(connection_json(Vec::new()));
    };
    Ok(storefront_product_images_connection_value(
        &product,
        &invocation.arguments,
    ))
}

fn storefront_product_featured_image_field(
    proxy: &mut DraftProxy,
    _request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation,
) -> Result<Value, String> {
    proxy.promote_all_product_media_ready_on_read();
    Ok(
        storefront_product_record_for_parent(proxy, invocation.parent)
            .and_then(|product| {
                product
                    .media
                    .iter()
                    .find_map(storefront_product_image_value_from_media)
            })
            .unwrap_or(Value::Null),
    )
}

fn storefront_product_media_field(
    proxy: &mut DraftProxy,
    _request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation,
) -> Result<Value, String> {
    proxy.promote_all_product_media_ready_on_read();
    let Some(product) = storefront_product_record_for_parent(proxy, invocation.parent) else {
        return Ok(connection_json(Vec::new()));
    };
    Ok(storefront_product_media_connection_value(
        &product,
        &invocation.arguments,
    ))
}

fn storefront_variant_image_field(
    proxy: &mut DraftProxy,
    _request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation,
) -> Result<Value, String> {
    proxy.promote_all_product_media_ready_on_read();
    let variant_id = invocation
        .parent
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let Some(variant) = proxy.store.product_variant_by_id(variant_id) else {
        return Ok(Value::Null);
    };
    Ok(proxy
        .store
        .product_by_id(&variant.product_id)
        .map(|product| storefront_variant_image_value(variant, product))
        .unwrap_or(Value::Null))
}

fn storefront_product_selected_variant_field(
    proxy: &mut DraftProxy,
    request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation,
) -> Result<Value, String> {
    let Some(product) = storefront_product_record_for_parent(proxy, invocation.parent) else {
        return Ok(Value::Null);
    };
    let variants = proxy.store.product_variants_for_product(&product.id);
    let context = storefront_request_context_from_request(request);
    let selected = storefront_variant_matching_json_options(&variants, &invocation.arguments)
        .or_else(|| {
            variants
                .iter()
                .find(|variant| storefront_variant_available_for_sale(variant))
        })
        .or_else(|| variants.first());
    Ok(selected
        .map(|variant| {
            storefront_product_variant_value(proxy, variant, Some(&product), &context, None)
        })
        .unwrap_or(Value::Null))
}

fn storefront_product_variant_by_options_field(
    proxy: &mut DraftProxy,
    request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation,
) -> Result<Value, String> {
    let Some(product) = storefront_product_record_for_parent(proxy, invocation.parent) else {
        return Ok(Value::Null);
    };
    let variants = proxy.store.product_variants_for_product(&product.id);
    let context = storefront_request_context_from_request(request);
    Ok(
        storefront_variant_matching_json_options(&variants, &invocation.arguments)
            .map(|variant| {
                storefront_product_variant_value(proxy, variant, Some(&product), &context, None)
            })
            .unwrap_or(Value::Null),
    )
}

fn storefront_variant_product_field(
    proxy: &mut DraftProxy,
    request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation,
) -> Result<Value, String> {
    let product_id = invocation
        .parent
        .get("_productId")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let product = proxy.store.product_by_id(product_id).cloned();
    let context = storefront_request_context_from_request(request);
    Ok(proxy.storefront_visible_product_value(product.as_ref(), &context))
}

fn storefront_product_selling_plan_groups_field(
    proxy: &mut DraftProxy,
    request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> Result<Value, String> {
    let Some(product) = storefront_product_record_for_parent(proxy, invocation.parent) else {
        return Ok(connection_json(Vec::new()));
    };
    let variants = proxy.store.product_variants_for_product(&product.id);
    let variant_ids = variants
        .iter()
        .map(|variant| variant.id.as_str())
        .collect::<BTreeSet<_>>();
    let groups = proxy
        .store
        .selling_plan_groups()
        .into_iter()
        .filter(|group| {
            group.product_ids.iter().any(|id| id == &product.id)
                || group
                    .product_variant_ids
                    .iter()
                    .any(|id| variant_ids.contains(id.as_str()))
        })
        .collect::<Vec<_>>();
    let context = storefront_request_context_from_request(request);
    let currency_code = storefront_product_currency_code(proxy, &variants, &context);
    let arguments = resolved_arguments_from_json(&invocation.arguments);
    let (groups, page_info) = connection_window(&groups, &arguments, |group| group.id.clone());
    Ok(typed_connection_value(
        &groups,
        |group| storefront_selling_plan_group_value(group, &currency_code),
        |group| group.id.clone(),
        page_info,
    ))
}

fn storefront_selling_plan_group_selling_plans_field(
    proxy: &mut DraftProxy,
    _request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> Result<Value, String> {
    let group_id = invocation
        .parent
        .get("_sellingPlanGroupId")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let currency_code = invocation
        .parent
        .get("_currencyCode")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let Some(group) = proxy.store.selling_plan_group_by_id(group_id) else {
        return Ok(connection_json(Vec::new()));
    };
    let arguments = resolved_arguments_from_json(&invocation.arguments);
    let (plans, page_info) =
        connection_window(&group.selling_plans, &arguments, |plan| plan.id.clone());
    Ok(typed_connection_value(
        &plans,
        |plan| storefront_selling_plan_value(plan, &group.options, currency_code),
        |plan| plan.id.clone(),
        page_info,
    ))
}

fn storefront_variant_selling_plan_allocations_field(
    proxy: &mut DraftProxy,
    request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> Result<Value, String> {
    let variant_id = invocation
        .parent
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let product_id = invocation
        .parent
        .get("_productId")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let Some(variant) = proxy.store.product_variant_by_id(variant_id).cloned() else {
        return Ok(connection_json(Vec::new()));
    };
    let allocations = proxy
        .store
        .selling_plan_groups()
        .into_iter()
        .filter(|group| {
            group.product_ids.iter().any(|id| id == product_id)
                || group.product_variant_ids.iter().any(|id| id == variant_id)
        })
        .flat_map(|group| {
            group
                .selling_plans
                .into_iter()
                .map(move |plan| (plan, group.options.clone()))
        })
        .collect::<Vec<_>>();
    let context = storefront_request_context_from_request(request);
    let pricing = proxy.storefront_variant_pricing(&variant, &context);
    let arguments = resolved_arguments_from_json(&invocation.arguments);
    let (allocations, page_info) =
        connection_window(&allocations, &arguments, |(plan, _)| plan.id.clone());
    Ok(typed_connection_value(
        &allocations,
        |(plan, option_names)| {
            storefront_selling_plan_allocation_value(plan, option_names, &pricing)
        },
        |(plan, _)| plan.id.clone(),
        page_info,
    ))
}

fn storefront_product_record_for_parent(
    proxy: &DraftProxy,
    parent: &Value,
) -> Option<ProductRecord> {
    let id = parent.get("id").and_then(Value::as_str)?;
    proxy.store.product_by_id(id).cloned()
}

fn storefront_empty_connection_field(
    _proxy: &mut DraftProxy,
    _request: &Request,
    _invocation: &crate::admin_graphql::FieldResolverInvocation,
) -> Result<Value, String> {
    Ok(connection_json(Vec::new()))
}

fn storefront_truncated_field(
    invocation: &crate::admin_graphql::FieldResolverInvocation,
    field: &str,
) -> Value {
    let value = invocation.parent.get(field).and_then(Value::as_str);
    let Some(value) = value else {
        return Value::Null;
    };
    let limit = invocation
        .arguments
        .get("truncateAt")
        .and_then(Value::as_i64)
        .filter(|limit| *limit >= 0)
        .map(|limit| limit as usize);
    Value::String(match limit {
        Some(limit) => value.chars().take(limit).collect(),
        None => value.to_string(),
    })
}

fn storefront_truncated_content_field(
    _proxy: &mut DraftProxy,
    _request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation,
) -> Result<Value, String> {
    Ok(storefront_truncated_field(invocation, "content"))
}

fn storefront_truncated_excerpt_field(
    _proxy: &mut DraftProxy,
    _request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation,
) -> Result<Value, String> {
    Ok(storefront_truncated_field(invocation, "excerpt"))
}

fn storefront_article_author_field(
    _proxy: &mut DraftProxy,
    _request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation,
) -> Result<Value, String> {
    Ok(invocation
        .parent
        .get("author")
        .cloned()
        .unwrap_or(Value::Null))
}

fn storefront_article_blog_field(
    proxy: &mut DraftProxy,
    _request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation,
) -> Result<Value, String> {
    Ok(invocation
        .parent
        .get("blogId")
        .and_then(Value::as_str)
        .and_then(|blog_id| proxy.storefront_content_by_id(StorefrontContentKind::Blog, blog_id))
        .unwrap_or(Value::Null))
}

fn storefront_blog_article_by_handle_field(
    proxy: &mut DraftProxy,
    _request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation,
) -> Result<Value, String> {
    let blog_id = storefront_parent_string(invocation, "id");
    let handle = invocation
        .arguments
        .get("handle")
        .and_then(Value::as_str)
        .unwrap_or_default();
    Ok(proxy
        .storefront_articles_for_blog(blog_id)
        .into_iter()
        .find(|article| article.get("handle").and_then(Value::as_str) == Some(handle))
        .unwrap_or(Value::Null))
}

fn storefront_blog_articles_field(
    proxy: &mut DraftProxy,
    _request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation,
) -> Result<Value, String> {
    let blog_id = storefront_parent_string(invocation, "id");
    Ok(proxy.storefront_content_connection_value(
        StorefrontContentKind::Article,
        proxy.storefront_articles_for_blog(blog_id),
        &invocation.arguments,
    ))
}

fn storefront_blog_authors_field(
    proxy: &mut DraftProxy,
    _request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation,
) -> Result<Value, String> {
    let mut seen = BTreeSet::new();
    let authors = proxy
        .storefront_articles_for_blog(storefront_parent_string(invocation, "id"))
        .into_iter()
        .filter_map(|article| article.get("author").cloned())
        .filter(|author| {
            let name = author
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or_default();
            !name.is_empty() && seen.insert(name.to_string())
        })
        .collect();
    Ok(Value::Array(authors))
}

fn storefront_sitemap_pages_count_field(
    _proxy: &mut DraftProxy,
    _request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> Result<Value, String> {
    let count = invocation
        .parent
        .get("_resources")
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or_default();
    Ok(count_object(count))
}

fn storefront_sitemap_resources_field(
    _proxy: &mut DraftProxy,
    _request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> Result<Value, String> {
    let resources = invocation
        .parent
        .get("_resources")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default();
    let page = invocation
        .arguments
        .get("page")
        .and_then(Value::as_i64)
        .and_then(|page| (page > 0).then_some(page as usize))
        .unwrap_or(1);
    let start = (page - 1) * 250;
    let end = (start + 250).min(resources.len());
    let items = if start < resources.len() {
        resources[start..end].to_vec()
    } else {
        Vec::new()
    };
    Ok(json!({
        "hasNextPage": end < resources.len(),
        "items": items,
    }))
}

impl DraftProxy {
    fn storefront_customer_query_root(
        &self,
        arguments: &BTreeMap<String, Value>,
    ) -> StorefrontCustomerAuthOutcome {
        let token = storefront_argument_string(arguments, "customerAccessToken");
        let customer = self
            .storefront_customer_id_for_access_token(&token)
            .and_then(|customer_id| {
                self.storefront_customer_by_id(&customer_id)
                    .map(|customer| storefront_customer_json(&customer))
            });
        StorefrontCustomerAuthOutcome {
            value: customer.unwrap_or(Value::Null),
            errors: Vec::new(),
        }
    }

    fn storefront_customer_mutation_root(
        &mut self,
        root_name: &str,
        arguments: &BTreeMap<String, Value>,
    ) -> StorefrontCustomerAuthOutcome {
        let resolved_arguments = resolved_arguments_from_json(arguments);
        match root_name {
            "customerCreate" => self.storefront_customer_create(arguments),
            "customerAccessTokenCreate" => self.storefront_customer_access_token_create(arguments),
            "customerAccessTokenRenew" => self.storefront_customer_access_token_renew(arguments),
            "customerAccessTokenDelete" => self.storefront_customer_access_token_delete(arguments),
            "customerActivate" => self.storefront_customer_activate(arguments),
            "customerActivateByUrl" => self.storefront_customer_activate_by_url(arguments),
            "customerRecover" => self.storefront_customer_recover(arguments),
            "customerReset" => self.storefront_customer_reset(arguments),
            "customerResetByUrl" => self.storefront_customer_reset_by_url(arguments),
            "customerAccessTokenCreateWithMultipass" => {
                self.storefront_customer_access_token_create_with_multipass()
            }
            "customerUpdate" => self.storefront_customer_update(&resolved_arguments),
            "customerAddressCreate" => {
                self.storefront_customer_address_create(root_name, &resolved_arguments)
            }
            "customerAddressUpdate" => self.storefront_customer_address_update(&resolved_arguments),
            "customerAddressDelete" => self.storefront_customer_address_delete(&resolved_arguments),
            "customerDefaultAddressUpdate" => {
                self.storefront_customer_default_address_update(&resolved_arguments)
            }
            _ => StorefrontCustomerAuthOutcome {
                value: Value::Null,
                errors: Vec::new(),
            },
        }
    }

    fn storefront_customer_create(
        &mut self,
        arguments: &BTreeMap<String, Value>,
    ) -> StorefrontCustomerAuthOutcome {
        let input = storefront_argument_object(arguments, "input");
        let email = storefront_input_string(input, "email");
        let password = storefront_input_string(input, "password");
        let normalized_email = storefront_customer_email_key(&email);
        let mut errors = Vec::new();
        if password.is_empty() {
            errors.push(storefront_customer_user_error(
                ["input", "password"],
                "Password can't be blank",
                Some("BLANK"),
            ));
        }
        if !storefront_email_looks_valid(&email) {
            errors.push(storefront_customer_user_error(
                ["input", "email"],
                "Email is invalid",
                Some("INVALID"),
            ));
        }
        if self
            .storefront_customer_id_by_email(&normalized_email)
            .is_some()
        {
            errors.push(storefront_customer_user_error(
                ["input", "email"],
                "Email has already been taken",
                Some("TAKEN"),
            ));
        }
        for (field_name, message, code) in [
            (
                "firstName",
                "First name cannot contain HTML tags",
                "CONTAINS_HTML_TAGS",
            ),
            (
                "lastName",
                "Last name cannot contain HTML tags",
                "CONTAINS_HTML_TAGS",
            ),
        ] {
            let candidate = storefront_input_string(input, field_name);
            if storefront_customer_contains_html_tag(&candidate) {
                errors.push(storefront_customer_user_error(
                    ["input", field_name],
                    message,
                    Some(code),
                ));
            }
        }
        if !errors.is_empty() {
            return StorefrontCustomerAuthOutcome {
                value: storefront_customer_payload(Value::Null, Value::Null, errors),
                errors: Vec::new(),
            };
        }

        let id = self.next_synthetic_gid("Customer");
        let timestamp = self.next_product_timestamp();
        let accepts_marketing = storefront_input_bool(input, "acceptsMarketing").unwrap_or(false);
        let first_name = storefront_input_string(input, "firstName");
        let first_name = (!first_name.is_empty()).then_some(first_name);
        let last_name = storefront_input_string(input, "lastName");
        let last_name = (!last_name.is_empty()).then_some(last_name);
        let phone = storefront_input_string(input, "phone");
        let phone = (!phone.is_empty()).then_some(phone);
        let mut customer = storefront_customer_shared_record(
            &id,
            first_name.as_deref(),
            last_name.as_deref(),
            &email,
            phone.as_deref(),
            accepts_marketing,
            &timestamp,
        );
        customer[STOREFRONT_CUSTOMER_PASSWORD_FINGERPRINT_FIELD] =
            json!(storefront_password_fingerprint(&id, &password));
        self.store
            .staged
            .customers
            .insert(id.clone(), customer.clone());
        self.store
            .staged
            .locally_created_customer_ids
            .insert(id.clone());
        self.store
            .staged
            .storefront_customer_email_index
            .insert(normalized_email, id);

        StorefrontCustomerAuthOutcome {
            value: storefront_customer_payload(
                storefront_customer_json(&customer),
                Value::Null,
                Vec::new(),
            ),
            errors: Vec::new(),
        }
    }

    fn storefront_customer_access_token_create(
        &mut self,
        arguments: &BTreeMap<String, Value>,
    ) -> StorefrontCustomerAuthOutcome {
        let input = storefront_argument_object(arguments, "input");
        let email = storefront_input_string(input, "email");
        let password = storefront_input_string(input, "password");
        let payload = match self
            .storefront_customer_id_by_email(&storefront_customer_email_key(&email))
            .and_then(|customer_id| self.storefront_customer_by_id(&customer_id))
        {
            Some(customer) if storefront_customer_password_matches(&customer, &password) => {
                if storefront_customer_state(&customer) == "DISABLED" {
                    storefront_customer_token_payload(
                        Value::Null,
                        vec![storefront_customer_user_error(
                            Value::Null,
                            "Customer is disabled",
                            Some("CUSTOMER_DISABLED"),
                        )],
                    )
                } else {
                    let customer_id = customer["id"].as_str().unwrap_or_default().to_string();
                    let token = self.issue_storefront_customer_access_token(&customer_id);
                    storefront_customer_token_payload(token, Vec::new())
                }
            }
            _ => storefront_customer_token_payload(
                Value::Null,
                vec![storefront_customer_user_error(
                    Value::Null,
                    "Unidentified customer",
                    Some("UNIDENTIFIED_CUSTOMER"),
                )],
            ),
        };

        StorefrontCustomerAuthOutcome {
            value: payload,
            errors: Vec::new(),
        }
    }

    fn storefront_customer_access_token_renew(
        &mut self,
        arguments: &BTreeMap<String, Value>,
    ) -> StorefrontCustomerAuthOutcome {
        let token = storefront_argument_string(arguments, "customerAccessToken");
        let token_hash = storefront_token_hash(&token);
        let payload = if self.storefront_access_token_is_active(&token_hash) {
            let expires_at = self.store.staged.storefront_customer_access_tokens[&token_hash]
                ["expiresAt"]
                .clone();
            json!({
                "customerAccessToken": {
                    "accessToken": token,
                    "expiresAt": expires_at
                },
                "userErrors": []
            })
        } else {
            json!({
                "customerAccessToken": null,
                "userErrors": [{
                    "field": ["customerAccessToken"],
                    "message": "access token does not exist"
                }]
            })
        };
        StorefrontCustomerAuthOutcome {
            value: payload,
            errors: Vec::new(),
        }
    }

    fn storefront_customer_access_token_delete(
        &mut self,
        arguments: &BTreeMap<String, Value>,
    ) -> StorefrontCustomerAuthOutcome {
        let token = storefront_argument_string(arguments, "customerAccessToken");
        let token_hash = storefront_token_hash(&token);
        if !self.storefront_access_token_is_active(&token_hash) {
            return StorefrontCustomerAuthOutcome {
                value: Value::Null,
                errors: vec![storefront_access_denied_error()],
            };
        }
        let token_id =
            self.store.staged.storefront_customer_access_tokens[&token_hash]["id"].clone();
        if let Some(record) = self
            .store
            .staged
            .storefront_customer_access_tokens
            .get_mut(&token_hash)
        {
            record["revoked"] = json!(true);
        }
        let payload = json!({
            "deletedAccessToken": token,
            "deletedCustomerAccessTokenId": token_id,
            "userErrors": []
        });
        StorefrontCustomerAuthOutcome {
            value: payload,
            errors: Vec::new(),
        }
    }

    fn storefront_customer_activate(
        &mut self,
        arguments: &BTreeMap<String, Value>,
    ) -> StorefrontCustomerAuthOutcome {
        let customer_id = storefront_argument_string(arguments, "id");
        let input = storefront_argument_object(arguments, "input");
        let activation_token = storefront_input_string(input, "activationToken");
        let password = storefront_input_string(input, "password");
        self.storefront_activate_customer_with_token(
            &customer_id,
            &activation_token,
            &password,
            ["input"],
        )
    }

    fn storefront_customer_activate_by_url(
        &mut self,
        arguments: &BTreeMap<String, Value>,
    ) -> StorefrontCustomerAuthOutcome {
        let activation_url = storefront_argument_string(arguments, "activationUrl");
        let password = storefront_argument_string(arguments, "password");
        let Some((customer_id, token)) =
            self.storefront_customer_activation_url_parts(&activation_url)
        else {
            return StorefrontCustomerAuthOutcome {
                value: storefront_customer_activation_payload(
                    Value::Null,
                    Value::Null,
                    vec![storefront_customer_user_error(
                        ["activationUrl"],
                        "Invalid activation url",
                        Some("INVALID"),
                    )],
                    false,
                ),
                errors: Vec::new(),
            };
        };
        self.storefront_activate_customer_with_token(
            &customer_id,
            &token,
            &password,
            ["activationUrl"],
        )
    }

    fn storefront_activate_customer_with_token<const N: usize>(
        &mut self,
        customer_id: &str,
        activation_token: &str,
        password: &str,
        invalid_field: [&str; N],
    ) -> StorefrontCustomerAuthOutcome {
        let Some(mut customer) = self.storefront_customer_by_id(customer_id) else {
            return StorefrontCustomerAuthOutcome {
                value: storefront_customer_activation_payload(
                    Value::Null,
                    Value::Null,
                    vec![storefront_customer_user_error(
                        invalid_field.to_vec(),
                        "Invalid activation token",
                        Some("TOKEN_INVALID"),
                    )],
                    true,
                ),
                errors: Vec::new(),
            };
        };
        if storefront_customer_state(&customer) == "ENABLED" {
            return StorefrontCustomerAuthOutcome {
                value: storefront_customer_activation_payload(
                    Value::Null,
                    Value::Null,
                    vec![storefront_customer_user_error(
                        Value::Null,
                        "Customer already enabled",
                        Some("ALREADY_ENABLED"),
                    )],
                    true,
                ),
                errors: Vec::new(),
            };
        }
        if activation_token != storefront_customer_activation_token_for_id(customer_id)
            && customer
                .get(STOREFRONT_CUSTOMER_ACTIVATION_TOKEN_FIELD)
                .and_then(Value::as_str)
                != Some(activation_token)
        {
            return StorefrontCustomerAuthOutcome {
                value: storefront_customer_activation_payload(
                    Value::Null,
                    Value::Null,
                    vec![storefront_customer_user_error(
                        invalid_field.to_vec(),
                        "Invalid activation token",
                        Some("TOKEN_INVALID"),
                    )],
                    true,
                ),
                errors: Vec::new(),
            };
        }

        customer["state"] = json!("ENABLED");
        customer["updatedAt"] = json!(self.next_product_timestamp());
        customer[STOREFRONT_CUSTOMER_PASSWORD_FINGERPRINT_FIELD] =
            json!(storefront_password_fingerprint(customer_id, password));
        self.store
            .staged
            .customers
            .stage(customer_id.to_string(), customer.clone());
        if let Some(email) = customer.get("email").and_then(Value::as_str) {
            self.store.staged.storefront_customer_email_index.insert(
                storefront_customer_email_key(email),
                customer_id.to_string(),
            );
        }
        let token = self.issue_storefront_customer_access_token(customer_id);
        StorefrontCustomerAuthOutcome {
            value: storefront_customer_activation_payload(
                storefront_customer_json(&customer),
                token,
                Vec::new(),
                true,
            ),
            errors: Vec::new(),
        }
    }

    fn storefront_customer_recover(
        &mut self,
        arguments: &BTreeMap<String, Value>,
    ) -> StorefrontCustomerAuthOutcome {
        let email = storefront_argument_string(arguments, "email");
        let payload = if let Some(customer_id) =
            self.storefront_customer_id_by_email(&storefront_customer_email_key(&email))
        {
            if let Some(mut customer) = self.storefront_customer_by_id(&customer_id) {
                let reset_token = self.next_storefront_customer_reset_token(&customer_id);
                customer[STOREFRONT_CUSTOMER_RESET_TOKEN_HASH_FIELD] =
                    json!(storefront_token_hash(&reset_token));
                customer[STOREFRONT_CUSTOMER_RESET_REQUESTED_AT_FIELD] =
                    json!(self.next_product_timestamp());
                self.store.staged.customers.stage(customer_id, customer);
            }
            json!({ "customerUserErrors": [], "userErrors": [] })
        } else {
            let errors = vec![storefront_customer_user_error(
                ["email"],
                "Could not find customer",
                Some("UNIDENTIFIED_CUSTOMER"),
            )];
            json!({
                "customerUserErrors": errors,
                "userErrors": storefront_user_errors_without_code(&errors)
            })
        };
        StorefrontCustomerAuthOutcome {
            value: payload,
            errors: Vec::new(),
        }
    }

    fn storefront_customer_reset(
        &mut self,
        arguments: &BTreeMap<String, Value>,
    ) -> StorefrontCustomerAuthOutcome {
        let customer_id = storefront_argument_string(arguments, "id");
        let input = storefront_argument_object(arguments, "input");
        let reset_token = storefront_input_string(input, "resetToken");
        let password = storefront_input_string(input, "password");
        self.storefront_reset_customer_with_token(&customer_id, &reset_token, &password, true)
    }

    fn storefront_customer_reset_by_url(
        &mut self,
        arguments: &BTreeMap<String, Value>,
    ) -> StorefrontCustomerAuthOutcome {
        let reset_url = storefront_argument_string(arguments, "resetUrl");
        let password = storefront_argument_string(arguments, "password");
        let Some((customer_id, token)) = self.storefront_customer_reset_url_parts(&reset_url)
        else {
            return StorefrontCustomerAuthOutcome {
                value: Value::Null,
                errors: vec![storefront_not_found_error()],
            };
        };
        self.storefront_reset_customer_with_token(&customer_id, &token, &password, true)
    }

    fn storefront_reset_customer_with_token(
        &mut self,
        customer_id: &str,
        reset_token: &str,
        password: &str,
        include_user_errors: bool,
    ) -> StorefrontCustomerAuthOutcome {
        let Some(mut customer) = self.storefront_customer_by_id(customer_id) else {
            return StorefrontCustomerAuthOutcome {
                value: Value::Null,
                errors: vec![storefront_not_found_error()],
            };
        };
        let reset_hash = storefront_token_hash(reset_token);
        let expected_hash = customer
            .get(STOREFRONT_CUSTOMER_RESET_TOKEN_HASH_FIELD)
            .and_then(Value::as_str);
        if expected_hash != Some(reset_hash.as_str()) {
            return StorefrontCustomerAuthOutcome {
                value: storefront_customer_activation_payload(
                    Value::Null,
                    Value::Null,
                    vec![storefront_customer_user_error(
                        ["input"],
                        "Invalid reset token",
                        Some("TOKEN_INVALID"),
                    )],
                    include_user_errors,
                ),
                errors: Vec::new(),
            };
        }
        customer["state"] = json!("ENABLED");
        customer["updatedAt"] = json!(self.next_product_timestamp());
        customer[STOREFRONT_CUSTOMER_PASSWORD_FINGERPRINT_FIELD] =
            json!(storefront_password_fingerprint(customer_id, password));
        if let Some(object) = customer.as_object_mut() {
            object.remove(STOREFRONT_CUSTOMER_RESET_TOKEN_HASH_FIELD);
            object.remove(STOREFRONT_CUSTOMER_RESET_REQUESTED_AT_FIELD);
        }
        self.store
            .staged
            .customers
            .stage(customer_id.to_string(), customer.clone());
        let token = self.issue_storefront_customer_access_token(customer_id);
        StorefrontCustomerAuthOutcome {
            value: storefront_customer_activation_payload(
                storefront_customer_json(&customer),
                token,
                Vec::new(),
                include_user_errors,
            ),
            errors: Vec::new(),
        }
    }

    fn storefront_customer_access_token_create_with_multipass(
        &self,
    ) -> StorefrontCustomerAuthOutcome {
        let payload = storefront_customer_token_payload(
            Value::Null,
            vec![storefront_customer_user_error(
                ["multipassToken"],
                "Invalid Multipass request",
                Some("INVALID_MULTIPASS_REQUEST"),
            )],
        );
        StorefrontCustomerAuthOutcome {
            value: payload,
            errors: Vec::new(),
        }
    }

    fn storefront_customer_update(
        &mut self,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) -> StorefrontCustomerAuthOutcome {
        let token = resolved_string_field(arguments, "customerAccessToken").unwrap_or_default();
        let input = resolved_object_field(arguments, "customer").unwrap_or_default();
        let Some(customer_id) = self.storefront_customer_id_for_access_token(&token) else {
            return StorefrontCustomerAuthOutcome {
                value: self.storefront_customer_update_payload(
                    None,
                    Value::Null,
                    storefront_invalid_customer_access_token_errors(),
                ),
                errors: Vec::new(),
            };
        };
        let Some(mut customer) = self.storefront_customer_by_id(&customer_id) else {
            return StorefrontCustomerAuthOutcome {
                value: self.storefront_customer_update_payload(
                    None,
                    Value::Null,
                    storefront_invalid_customer_access_token_errors(),
                ),
                errors: Vec::new(),
            };
        };

        let mut errors = Vec::new();
        if input.contains_key("email") {
            return StorefrontCustomerAuthOutcome {
                value: self.storefront_customer_update_payload(
                    None,
                    Value::Null,
                    vec![storefront_customer_user_error(
                        ["customer", "email"],
                        "CustomerUpdate access denied",
                        Some("INVALID"),
                    )],
                ),
                errors: Vec::new(),
            };
        }
        if input.contains_key("password")
            && resolved_string_field(&input, "password")
                .unwrap_or_default()
                .is_empty()
        {
            errors.push(storefront_customer_user_error(
                ["customer", "password"],
                "Password can't be blank",
                Some("BLANK"),
            ));
        }
        for (field_name, message, code) in [
            (
                "firstName",
                "First name cannot contain HTML tags",
                "CONTAINS_HTML_TAGS",
            ),
            (
                "lastName",
                "Last name cannot contain HTML tags",
                "CONTAINS_HTML_TAGS",
            ),
        ] {
            if resolved_string_field(&input, field_name)
                .is_some_and(|value| storefront_customer_contains_html_tag(&value))
            {
                errors.push(storefront_customer_user_error(
                    ["customer", field_name],
                    message,
                    Some(code),
                ));
            }
        }
        if !errors.is_empty() {
            return StorefrontCustomerAuthOutcome {
                value: self.storefront_customer_update_payload(None, Value::Null, errors),
                errors: Vec::new(),
            };
        }

        let old_email = customer
            .get("email")
            .and_then(Value::as_str)
            .map(storefront_customer_email_key);
        for string_field in ["firstName", "lastName", "email"] {
            if input.contains_key(string_field) {
                let value = resolved_string_field(&input, string_field)
                    .filter(|value| !value.is_empty())
                    .map(Value::String)
                    .unwrap_or(Value::Null);
                customer[string_field] = value;
            }
        }
        if input.contains_key("phone") {
            let phone = resolved_string_field(&input, "phone")
                .filter(|value| !value.is_empty())
                .map(Value::String)
                .unwrap_or(Value::Null);
            customer["phone"] = phone.clone();
            customer["defaultPhoneNumber"] = if phone.is_null() {
                Value::Null
            } else {
                json!({ "phoneNumber": phone })
            };
        }
        if let Some(accepts_marketing) = resolved_bool_field(&input, "acceptsMarketing") {
            customer["acceptsMarketing"] = json!(accepts_marketing);
            customer["emailMarketingConsent"] = json!({
                "marketingState": if accepts_marketing { "SUBSCRIBED" } else { "NOT_SUBSCRIBED" },
                "marketingOptInLevel": Value::Null,
                "consentUpdatedAt": self.next_product_timestamp()
            });
        }
        let first_name = customer.get("firstName").and_then(Value::as_str);
        let last_name = customer.get("lastName").and_then(Value::as_str);
        let email = customer.get("email").and_then(Value::as_str);
        customer["displayName"] = json!(storefront_customer_display_name(
            first_name, last_name, email
        ));
        customer["updatedAt"] = json!(self.next_product_timestamp());

        let mut new_access_token = Value::Null;
        if let Some(password) = resolved_string_field(&input, "password") {
            customer[STOREFRONT_CUSTOMER_PASSWORD_FINGERPRINT_FIELD] =
                json!(storefront_password_fingerprint(&customer_id, &password));
            self.revoke_storefront_customer_access_tokens_for_customer(&customer_id);
            new_access_token = self.issue_storefront_customer_access_token(&customer_id);
        }

        if let Some(old_email) = old_email {
            self.store
                .staged
                .storefront_customer_email_index
                .remove(&old_email);
        }
        if let Some(email) = customer.get("email").and_then(Value::as_str) {
            self.store
                .staged
                .storefront_customer_email_index
                .insert(storefront_customer_email_key(email), customer_id.clone());
        }
        self.store
            .staged
            .customers
            .stage(customer_id.clone(), customer.clone());

        StorefrontCustomerAuthOutcome {
            value: self.storefront_customer_update_payload(
                Some((&customer_id, &customer)),
                new_access_token,
                Vec::new(),
            ),
            errors: Vec::new(),
        }
    }

    fn storefront_customer_address_create(
        &mut self,
        root_name: &str,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) -> StorefrontCustomerAuthOutcome {
        let token = resolved_string_field(arguments, "customerAccessToken").unwrap_or_default();
        let address_input = resolved_object_field(arguments, "address").unwrap_or_default();
        let Some(customer_id) = self.storefront_customer_id_for_access_token(&token) else {
            return StorefrontCustomerAuthOutcome {
                value: Value::Null,
                errors: vec![storefront_access_denied_error_for(root_name)],
            };
        };
        let Some(customer) = self.storefront_customer_by_id(&customer_id) else {
            return StorefrontCustomerAuthOutcome {
                value: storefront_customer_address_payload(
                    "customerAddress",
                    Value::Null,
                    storefront_invalid_customer_access_token_errors(),
                ),
                errors: Vec::new(),
            };
        };
        let new_id = self.next_proxy_synthetic_gid("MailingAddress");
        let existing_nodes = customer_address_nodes(&customer);
        let (node, errors) = customer_address_input_node(
            &address_input,
            None,
            customer.get("firstName").and_then(Value::as_str),
            customer.get("lastName").and_then(Value::as_str),
            &new_id,
        );
        if !errors.is_empty() {
            return StorefrontCustomerAuthOutcome {
                value: storefront_customer_address_payload(
                    "customerAddress",
                    Value::Null,
                    storefront_customer_user_errors_with_codes(errors),
                ),
                errors: Vec::new(),
            };
        }
        let mut node = node.unwrap_or(Value::Null);
        preserve_storefront_address_phone(&mut node, &address_input);
        let new_key = customer_address_dedup_key(&node);
        if existing_nodes
            .iter()
            .any(|existing| customer_address_dedup_key(existing) == new_key)
        {
            return StorefrontCustomerAuthOutcome {
                value: storefront_customer_address_payload(
                    "customerAddress",
                    Value::Null,
                    vec![storefront_customer_user_error(
                        ["address"],
                        "Address already exists",
                        None,
                    )],
                ),
                errors: Vec::new(),
            };
        }
        let mut nodes = existing_nodes;
        let was_empty = nodes.is_empty();
        nodes.push(node.clone());
        let default_id = if was_empty {
            Some(new_id.as_str())
        } else {
            storefront_customer_default_address_id(&customer)
        };
        let updated_at = self.next_product_timestamp();
        if let Some(customer) = self.store.staged.customers.get_mut(&customer_id) {
            customer_rebuild_addresses(customer, nodes, default_id);
            customer["updatedAt"] = json!(updated_at);
        }
        StorefrontCustomerAuthOutcome {
            value: storefront_customer_address_payload("customerAddress", node, Vec::new()),
            errors: Vec::new(),
        }
    }

    fn storefront_customer_address_update(
        &mut self,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) -> StorefrontCustomerAuthOutcome {
        let token = resolved_string_field(arguments, "customerAccessToken").unwrap_or_default();
        let address_id = resolved_string_field(arguments, "id").unwrap_or_default();
        let address_input = resolved_object_field(arguments, "address").unwrap_or_default();
        let Some(customer_id) = self.storefront_customer_id_for_access_token(&token) else {
            return StorefrontCustomerAuthOutcome {
                value: storefront_customer_address_payload(
                    "customerAddress",
                    Value::Null,
                    storefront_invalid_customer_access_token_errors(),
                ),
                errors: Vec::new(),
            };
        };
        let Some(customer) = self.storefront_customer_by_id(&customer_id) else {
            return StorefrontCustomerAuthOutcome {
                value: storefront_customer_address_payload(
                    "customerAddress",
                    Value::Null,
                    storefront_invalid_customer_access_token_errors(),
                ),
                errors: Vec::new(),
            };
        };
        let existing_nodes = customer_address_nodes(&customer);
        let Some(index) = storefront_customer_address_node_index(&existing_nodes, &address_id)
        else {
            return StorefrontCustomerAuthOutcome {
                value: storefront_customer_address_payload(
                    "customerAddress",
                    Value::Null,
                    vec![storefront_customer_user_error(
                        ["id"],
                        "Address does not exist",
                        Some("NOT_FOUND"),
                    )],
                ),
                errors: Vec::new(),
            };
        };
        let (node, errors) = customer_address_input_node(
            &address_input,
            Some(&existing_nodes[index]),
            customer.get("firstName").and_then(Value::as_str),
            customer.get("lastName").and_then(Value::as_str),
            &address_id,
        );
        if !errors.is_empty() {
            return StorefrontCustomerAuthOutcome {
                value: storefront_customer_address_payload(
                    "customerAddress",
                    Value::Null,
                    storefront_customer_user_errors_with_codes(errors),
                ),
                errors: Vec::new(),
            };
        }
        let mut node = node.unwrap_or(Value::Null);
        preserve_storefront_address_phone(&mut node, &address_input);
        let mut nodes = existing_nodes;
        nodes[index] = node.clone();
        let default_id = storefront_customer_default_address_id(&customer);
        let updated_at = self.next_product_timestamp();
        if let Some(customer) = self.store.staged.customers.get_mut(&customer_id) {
            customer_rebuild_addresses(customer, nodes, default_id);
            customer["updatedAt"] = json!(updated_at);
        }
        StorefrontCustomerAuthOutcome {
            value: storefront_customer_address_payload("customerAddress", node, Vec::new()),
            errors: Vec::new(),
        }
    }

    fn storefront_customer_address_delete(
        &mut self,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) -> StorefrontCustomerAuthOutcome {
        let token = resolved_string_field(arguments, "customerAccessToken").unwrap_or_default();
        let address_id = resolved_string_field(arguments, "id").unwrap_or_default();
        let Some(customer_id) = self.storefront_customer_id_for_access_token(&token) else {
            return StorefrontCustomerAuthOutcome {
                value: storefront_customer_address_delete_payload(
                    Value::Null,
                    storefront_invalid_customer_access_token_errors(),
                ),
                errors: Vec::new(),
            };
        };
        let Some(customer) = self.storefront_customer_by_id(&customer_id) else {
            return StorefrontCustomerAuthOutcome {
                value: storefront_customer_address_delete_payload(
                    Value::Null,
                    storefront_invalid_customer_access_token_errors(),
                ),
                errors: Vec::new(),
            };
        };
        let mut nodes = customer_address_nodes(&customer);
        let Some(index) = storefront_customer_address_node_index(&nodes, &address_id) else {
            return StorefrontCustomerAuthOutcome {
                value: storefront_customer_address_delete_payload(
                    Value::Null,
                    vec![storefront_customer_user_error(
                        ["id"],
                        "Address does not exist",
                        Some("NOT_FOUND"),
                    )],
                ),
                errors: Vec::new(),
            };
        };
        let was_default =
            storefront_customer_default_address_id(&customer) == Some(address_id.as_str());
        nodes.remove(index);
        let default_id = if was_default {
            nodes
                .first()
                .and_then(|node: &Value| node.get("id"))
                .and_then(Value::as_str)
                .map(str::to_string)
        } else {
            storefront_customer_default_address_id(&customer).map(str::to_string)
        };
        let updated_at = self.next_product_timestamp();
        if let Some(customer) = self.store.staged.customers.get_mut(&customer_id) {
            customer_rebuild_addresses(customer, nodes, default_id.as_deref());
            customer["updatedAt"] = json!(updated_at);
        }
        StorefrontCustomerAuthOutcome {
            value: storefront_customer_address_delete_payload(json!(address_id), Vec::new()),
            errors: Vec::new(),
        }
    }

    fn storefront_customer_default_address_update(
        &mut self,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) -> StorefrontCustomerAuthOutcome {
        let token = resolved_string_field(arguments, "customerAccessToken").unwrap_or_default();
        let address_id = resolved_string_field(arguments, "addressId").unwrap_or_default();
        let Some(customer_id) = self.storefront_customer_id_for_access_token(&token) else {
            return StorefrontCustomerAuthOutcome {
                value: self.storefront_customer_default_address_payload(
                    None,
                    storefront_invalid_customer_access_token_errors(),
                ),
                errors: Vec::new(),
            };
        };
        let Some(customer) = self.storefront_customer_by_id(&customer_id) else {
            return StorefrontCustomerAuthOutcome {
                value: self.storefront_customer_default_address_payload(
                    None,
                    storefront_invalid_customer_access_token_errors(),
                ),
                errors: Vec::new(),
            };
        };
        let nodes = customer_address_nodes(&customer);
        if storefront_customer_address_node_index(&nodes, &address_id).is_none() {
            return StorefrontCustomerAuthOutcome {
                value: self.storefront_customer_default_address_payload(
                    Some((&customer_id, &customer)),
                    vec![storefront_customer_user_error(
                        ["addressId"],
                        "Address does not exist",
                        Some("NOT_FOUND"),
                    )],
                ),
                errors: Vec::new(),
            };
        }
        let updated_at = self.next_product_timestamp();
        if let Some(customer) = self.store.staged.customers.get_mut(&customer_id) {
            customer_rebuild_addresses(customer, nodes, Some(address_id.as_str()));
            customer["updatedAt"] = json!(updated_at);
        }
        let customer = self
            .storefront_customer_by_id(&customer_id)
            .unwrap_or(Value::Null);
        StorefrontCustomerAuthOutcome {
            value: self.storefront_customer_default_address_payload(
                Some((&customer_id, &customer)),
                Vec::new(),
            ),
            errors: Vec::new(),
        }
    }

    fn issue_storefront_customer_access_token(&mut self, customer_id: &str) -> Value {
        let sequence = self.store.staged.next_storefront_customer_access_token_id;
        self.store.staged.next_storefront_customer_access_token_id += 1;
        let issued_at = self.current_time();
        let expires_at = issued_at + time::Duration::days(42);
        let expires_at = storefront_format_timestamp(expires_at);
        let token = storefront_access_token_value(customer_id, sequence, &expires_at);
        let token_hash = storefront_token_hash(&token);
        let token_id = format!("gid://shopify/CustomerAccessToken/{sequence}");
        self.store.staged.storefront_customer_access_tokens.insert(
            token_hash,
            json!({
                "id": token_id,
                "customerId": customer_id,
                "expiresAt": expires_at,
                "revoked": false
            }),
        );
        json!({
            "accessToken": token,
            "expiresAt": expires_at
        })
    }

    fn next_storefront_customer_reset_token(&mut self, customer_id: &str) -> String {
        let sequence = self.store.staged.next_storefront_customer_reset_token_id;
        self.store.staged.next_storefront_customer_reset_token_id += 1;
        format!("sdp-reset-{}-{sequence}", resource_id_tail(customer_id))
    }

    fn storefront_access_token_is_active(&self, token_hash: &str) -> bool {
        let Some(record) = self
            .store
            .staged
            .storefront_customer_access_tokens
            .get(token_hash)
        else {
            return false;
        };
        if record["revoked"].as_bool().unwrap_or(false) {
            return false;
        }
        let Some(expires_at) = record["expiresAt"].as_str() else {
            return false;
        };
        storefront_timestamp_is_future(expires_at, self.current_time())
    }

    pub(in crate::proxy) fn storefront_customer_id_for_access_token(
        &self,
        token: &str,
    ) -> Option<String> {
        let token_hash = storefront_token_hash(token);
        if !self.storefront_access_token_is_active(&token_hash) {
            return None;
        }
        self.store
            .staged
            .storefront_customer_access_tokens
            .get(&token_hash)?
            .get("customerId")?
            .as_str()
            .map(str::to_string)
    }

    pub(in crate::proxy) fn storefront_customer_by_id(&self, customer_id: &str) -> Option<Value> {
        if self.store.staged.customers.is_tombstoned(customer_id) {
            return None;
        }
        self.store.staged.customers.get(customer_id).cloned()
    }

    fn revoke_storefront_customer_access_tokens_for_customer(&mut self, customer_id: &str) {
        for record in self
            .store
            .staged
            .storefront_customer_access_tokens
            .values_mut()
        {
            if record.get("customerId").and_then(Value::as_str) == Some(customer_id) {
                record["revoked"] = json!(true);
            }
        }
    }

    fn storefront_customer_update_payload(
        &self,
        customer: Option<(&str, &Value)>,
        customer_access_token: Value,
        customer_user_errors: Vec<Value>,
    ) -> Value {
        let customer_user_errors = storefront_customer_user_errors_with_codes(customer_user_errors);
        let user_errors = storefront_user_errors_without_code(&customer_user_errors);
        json!({
            "customer": customer
                .map(|(_, customer)| storefront_customer_json(customer))
                .unwrap_or(Value::Null),
            "customerAccessToken": customer_access_token,
            "customerUserErrors": customer_user_errors,
            "userErrors": user_errors,
        })
    }

    fn storefront_customer_default_address_payload(
        &self,
        customer: Option<(&str, &Value)>,
        customer_user_errors: Vec<Value>,
    ) -> Value {
        let customer_user_errors = storefront_customer_user_errors_with_codes(customer_user_errors);
        let user_errors = storefront_user_errors_without_code(&customer_user_errors);
        json!({
            "customer": customer
                .map(|(_, customer)| storefront_customer_json(customer))
                .unwrap_or(Value::Null),
            "customerUserErrors": customer_user_errors,
            "userErrors": user_errors,
        })
    }

    fn storefront_customer_id_by_email(&self, normalized_email: &str) -> Option<String> {
        if let Some(customer_id) = self
            .store
            .staged
            .storefront_customer_email_index
            .get(normalized_email)
            .filter(|customer_id| self.storefront_customer_by_id(customer_id).is_some())
        {
            return Some(customer_id.clone());
        }
        self.store
            .staged
            .customers
            .iter()
            .find_map(|(customer_id, customer)| {
                let email = customer.get("email").and_then(Value::as_str)?;
                (storefront_customer_email_key(email) == normalized_email)
                    .then(|| customer_id.clone())
            })
    }

    fn storefront_customer_activation_url_parts(&self, url: &str) -> Option<(String, String)> {
        let token = url.rsplit('/').next()?.to_string();
        if token.is_empty() {
            return None;
        }
        let customer_id = self
            .store
            .staged
            .customers
            .iter()
            .find_map(|(id, customer)| {
                let deterministic = storefront_customer_activation_token_for_id(id);
                let stored = customer
                    .get(STOREFRONT_CUSTOMER_ACTIVATION_TOKEN_FIELD)
                    .and_then(Value::as_str);
                (token == deterministic || stored == Some(token.as_str())).then(|| id.clone())
            })?;
        Some((customer_id, token))
    }

    fn storefront_customer_reset_url_parts(&self, url: &str) -> Option<(String, String)> {
        let token = url.rsplit('/').next()?.to_string();
        if token.is_empty() {
            return None;
        }
        let token_hash = storefront_token_hash(&token);
        let customer_id = self
            .store
            .staged
            .customers
            .iter()
            .find_map(|(id, customer)| {
                (customer
                    .get(STOREFRONT_CUSTOMER_RESET_TOKEN_HASH_FIELD)
                    .and_then(Value::as_str)
                    == Some(token_hash.as_str()))
                .then(|| id.clone())
            })?;
        Some((customer_id, token))
    }

    pub(in crate::proxy) fn record_storefront_customer_auth_log_entry(
        &mut self,
        request: &Request,
        variables: &BTreeMap<String, ResolvedValue>,
        operation_type: OperationType,
        root_fields: &[String],
        details: StorefrontCustomerAuthLogDetails<'_>,
    ) {
        let id = format!("log-{}", self.log_entries.len() + 1);
        let primary_root_field = root_fields.first().cloned().unwrap_or_default();
        self.log_entries.push(json!({
            "id": id,
            "operationName": Value::Null,
            "apiSurface": "storefront",
            "status": details.status,
            "path": request.path,
            "query": "<redacted:storefront-customer-auth-query>",
            "variables": storefront_redacted_variables_json(variables),
            "rawBody": "<redacted:storefront-customer-auth-request>",
            "interpreted": {
                "operationType": operation_type.keyword(),
                "rootFields": root_fields,
                "primaryRootField": primary_root_field,
                "capability": {
                    "domain": "storefront",
                    "execution": details.execution
                }
            },
            "notes": details.notes
        }));
    }

    pub(crate) fn storefront_platform_query_resolver(
        &mut self,
        invocation: RootInvocation<'_>,
    ) -> ResolverOutcome<Value> {
        let RootInvocation {
            root_name,
            arguments,
            request,
            query,
            variables,
            ..
        } = invocation;
        let context = storefront_request_context(query, variables);
        if self.config.read_mode == ReadMode::LiveHybrid
            && self.storefront_first_slice_needs_hydration(root_name, &context)
        {
            self.hydrate_storefront_first_slice(request, &context);
        }
        let value = match root_name {
            "shop" => self.storefront_shop_value(),
            "localization" => self
                .store
                .base
                .storefront_localizations
                .get(&context.key())
                .cloned()
                .unwrap_or(Value::Null),
            "locations" => self.storefront_locations_connection_value(&arguments),
            "paymentSettings" => self.storefront_payment_settings_value(),
            "publicApiVersions" => {
                Value::Array(self.store.base.storefront_public_api_versions.clone())
            }
            _ => {
                return ResolverOutcome::error(format!(
                    "Storefront platform resolver does not own `{root_name}`"
                ));
            }
        };
        ResolverOutcome::value(value)
    }

    pub(crate) fn storefront_catalog_query_resolver(
        &mut self,
        invocation: RootInvocation<'_>,
    ) -> ResolverOutcome<Value> {
        let context = storefront_request_context(invocation.query, invocation.variables);
        if matches!(
            invocation.root_name,
            "product" | "productByHandle" | "products"
        ) && self.config.read_mode == ReadMode::LiveHybrid
            && !self.storefront_catalog_is_locally_ready()
        {
            return ResolverOutcome::error(format!(
                "Storefront root `{}` has no local catalog backing",
                invocation.root_name
            ));
        }
        let value = match invocation.root_name {
            "product" => self.storefront_product_root_value(&invocation.arguments, &context),
            "productByHandle" => {
                self.storefront_product_by_handle_root_value(&invocation.arguments, &context)
            }
            "products" => {
                self.storefront_products_connection_value(&invocation.arguments, &context)
            }
            "productRecommendations" => {
                self.storefront_product_recommendations_json(&invocation.arguments, &context)
            }
            "productTags" | "productTypes" => {
                let kind = if invocation.root_name == "productTags" {
                    StorefrontProductTaxonomyKind::Tag
                } else {
                    StorefrontProductTaxonomyKind::ProductType
                };
                if self.config.read_mode == ReadMode::LiveHybrid {
                    self.hydrate_storefront_taxonomy(invocation.request, kind);
                }
                self.storefront_product_taxonomy_connection_json(&invocation.arguments, kind)
            }
            _ => {
                return ResolverOutcome::error(format!(
                    "Storefront catalog resolver does not own `{}`",
                    invocation.root_name
                ));
            }
        };
        ResolverOutcome::value(value)
    }

    pub(crate) fn storefront_collection_query_resolver(
        &mut self,
        invocation: RootInvocation<'_>,
    ) -> ResolverOutcome<Value> {
        if self.config.read_mode == ReadMode::LiveHybrid
            && self.storefront_collections_need_hydration()
        {
            self.hydrate_storefront_collections(invocation.request);
        }
        let value = match invocation.root_name {
            "collection" => self.storefront_collection_root_value(&invocation.arguments),
            "collectionByHandle" => {
                self.storefront_collection_by_handle_root_value(&invocation.arguments)
            }
            "collections" => self.storefront_collections_connection_value(&invocation.arguments),
            _ => {
                return ResolverOutcome::error(format!(
                    "Storefront collection resolver does not own `{}`",
                    invocation.root_name
                ));
            }
        };
        ResolverOutcome::value(value)
    }

    pub(crate) fn storefront_content_query_resolver(
        &mut self,
        invocation: RootInvocation<'_>,
    ) -> ResolverOutcome<Value> {
        if self.config.read_mode == ReadMode::LiveHybrid && invocation.root_name == "menu" {
            let handle = invocation
                .arguments
                .get("handle")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if self.storefront_menu_by_handle(handle).is_none() {
                self.hydrate_storefront_menu(invocation.request, handle);
            }
        }
        let value = match invocation.root_name {
            root if STOREFRONT_CONTENT_ROOTS.contains(&root) => {
                self.storefront_content_root_value(root, &invocation.arguments)
            }
            "menu" => invocation
                .arguments
                .get("handle")
                .and_then(Value::as_str)
                .and_then(|handle| self.storefront_menu_by_handle(handle))
                .unwrap_or(Value::Null),
            "sitemap" => self.storefront_sitemap_value(&invocation.arguments),
            "urlRedirects" => self.url_redirect_connection_value(&invocation.arguments),
            _ => {
                return ResolverOutcome::error(format!(
                    "Storefront content resolver does not own `{}`",
                    invocation.root_name
                ));
            }
        };
        ResolverOutcome::value(value)
    }

    pub(crate) fn storefront_custom_data_query_resolver(
        &mut self,
        invocation: RootInvocation<'_>,
    ) -> ResolverOutcome<Value> {
        let value = match invocation.root_name {
            "metaobject" => self.storefront_metaobject_root_value(&invocation.arguments),
            "metaobjects" => self.storefront_metaobjects_connection_value(&invocation.arguments),
            _ => {
                return ResolverOutcome::error(format!(
                    "Storefront custom-data resolver does not own `{}`",
                    invocation.root_name
                ));
            }
        };
        ResolverOutcome::value(value)
    }

    pub(crate) fn storefront_discovery_query_resolver(
        &mut self,
        invocation: RootInvocation<'_>,
    ) -> ResolverOutcome<Value> {
        let context = storefront_request_context(invocation.query, invocation.variables);
        let value = match invocation.root_name {
            "node" => self.storefront_node_root_value(&invocation.arguments, &context),
            "nodes" => self.storefront_nodes_root_value(&invocation.arguments, &context),
            "search" => self.storefront_search_root_json(&invocation.arguments, &context),
            "predictiveSearch" => {
                self.storefront_predictive_search_root_json(&invocation.arguments, &context)
            }
            _ => {
                return ResolverOutcome::error(format!(
                    "Storefront discovery resolver does not own `{}`",
                    invocation.root_name
                ));
            }
        };
        ResolverOutcome::value(value)
    }

    pub(crate) fn storefront_customer_query_resolver(
        &mut self,
        invocation: RootInvocation<'_>,
    ) -> ResolverOutcome<Value> {
        let outcome = self.storefront_customer_query_root(&invocation.arguments);
        ResolverOutcome::value(outcome.value).with_errors(outcome.errors)
    }

    pub(crate) fn storefront_customer_mutation_resolver(
        &mut self,
        invocation: RootInvocation<'_>,
    ) -> ResolverOutcome<Value> {
        let outcome =
            self.storefront_customer_mutation_root(invocation.root_name, &invocation.arguments);
        ResolverOutcome::value(outcome.value).with_errors(outcome.errors)
    }

    pub(in crate::proxy) fn storefront_query_root_is_local(
        &self,
        root_name: &str,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) -> bool {
        if matches!(root_name, "product" | "productByHandle" | "products")
            && self.config.read_mode == ReadMode::LiveHybrid
            && !self.storefront_catalog_is_locally_ready()
        {
            return false;
        }
        let capability = self.registry.resolve_for_surface(
            ApiSurface::Storefront,
            OperationType::Query,
            root_name,
        );
        capability.domain == CapabilityDomain::Storefront
            && self.storefront_root_is_promoted(root_name)
            && self.storefront_root_has_local_backing(root_name, arguments)
    }

    fn storefront_custom_data_root_has_local_effect(
        &self,
        root_name: &str,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) -> bool {
        match root_name {
            "metaobject" => self.has_local_metaobject_state(),
            "metaobjects" => {
                let meta_type = resolved_string_field(arguments, "type").unwrap_or_default();
                meta_type.is_empty()
                    || self.metaobject_definition_by_type(&meta_type).is_some()
                    || self.store.staged.metaobjects.values().any(|record| {
                        record.get("type").and_then(Value::as_str) == Some(meta_type.as_str())
                    })
            }
            _ => false,
        }
    }

    pub(in crate::proxy) fn storefront_mutation_root_is_local(&self, root_name: &str) -> bool {
        (STOREFRONT_CUSTOMER_AUTH_MUTATION_ROOTS.contains(&root_name)
            || STOREFRONT_CART_MUTATION_ROOTS.contains(&root_name))
            && self
                .registry
                .resolve_for_surface(ApiSurface::Storefront, OperationType::Mutation, root_name)
                .execution
                == CapabilityExecution::StageLocally
    }

    fn storefront_root_is_promoted(&self, root: &str) -> bool {
        root == "cart"
            || root == "customer"
            || STOREFRONT_FIRST_SLICE_ROOTS.contains(&root)
            || STOREFRONT_COLLECTION_ROOTS.contains(&root)
            || STOREFRONT_LOCAL_CONTENT_ROOTS.contains(&root)
            || STOREFRONT_CUSTOM_DATA_ROOTS.contains(&root)
            || STOREFRONT_DISCOVERY_ROOTS.contains(&root)
    }

    fn storefront_root_has_local_backing(
        &self,
        root_name: &str,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) -> bool {
        if self.config.read_mode == ReadMode::Snapshot
            || STOREFRONT_FIRST_SLICE_ROOTS.contains(&root_name)
        {
            return true;
        }
        match root_name {
            "cart" => true,
            "customer" => true,
            root if STOREFRONT_COLLECTION_ROOTS.contains(&root) => true,
            root if STOREFRONT_CONTENT_ROOTS.contains(&root) => {
                self.has_online_store_content_state()
            }
            "sitemap" => self.has_online_store_content_state(),
            "urlRedirects" => self.has_staged_url_redirects(),
            "menu" => true,
            root if STOREFRONT_CUSTOM_DATA_ROOTS.contains(&root) => {
                self.storefront_custom_data_root_has_local_effect(root, arguments)
            }
            root if STOREFRONT_DISCOVERY_ROOTS.contains(&root) => {
                self.has_storefront_discovery_state()
            }
            _ => false,
        }
    }

    fn has_storefront_discovery_state(&self) -> bool {
        self.store.has_product_state()
            || !self.store.staged.collections.is_empty()
            || self.has_online_store_content_state()
            || !self.store.staged.metaobjects.is_empty()
            || !self
                .store
                .base
                .storefront_locations
                .ordered_values()
                .is_empty()
            || !self.store.base.storefront_menus.ordered_values().is_empty()
    }

    fn storefront_catalog_is_locally_ready(&self) -> bool {
        self.store.has_product_state()
            && (self.store.staged.current_channel_publication_resolved
                || self.store.has_known_publication_catalog())
    }

    fn storefront_first_slice_needs_hydration(
        &self,
        root_name: &str,
        context: &StorefrontRequestContext,
    ) -> bool {
        match root_name {
            "shop" => self.storefront_shop_needs_hydration(),
            "localization" => !self.storefront_localization_is_observed(context),
            "locations" => self.storefront_location_records().is_empty(),
            "paymentSettings" => self
                .storefront_payment_settings_source()
                .is_none_or(|settings| !settings.is_object()),
            "publicApiVersions" => self.store.base.storefront_public_api_versions.is_empty(),
            "product" | "productByHandle" | "products" => false,
            _ => false,
        }
    }

    fn storefront_shop_needs_hydration(&self) -> bool {
        if self.store.base.storefront_shop.is_object() || self.store.base.shop.is_object() {
            return false;
        }
        !self.storefront_has_local_shop_metafield_state()
    }

    fn storefront_has_local_shop_metafield_state(&self) -> bool {
        self.storefront_shop_owner_id().is_some_and(|owner_id| {
            self.store
                .staged
                .owner_metafields
                .get(&owner_id)
                .is_some_and(|records| !records.is_empty())
                || self
                    .store
                    .staged
                    .deleted_owner_metafields
                    .iter()
                    .any(|(deleted_owner_id, _, _)| deleted_owner_id == &owner_id)
        })
    }

    fn hydrate_storefront_first_slice(
        &mut self,
        request: &Request,
        context: &StorefrontRequestContext,
    ) {
        let (query, variables) = storefront_first_slice_hydrate_body(context);
        let response = self.storefront_upstream_post(
            request,
            json!({
                "query": query,
                "variables": variables
            }),
        );
        if (200..300).contains(&response.status) {
            self.hydrate_storefront_first_slice_from_data(&response.body["data"], context);
        }
    }

    fn hydrate_storefront_taxonomy(
        &mut self,
        request: &Request,
        kind: StorefrontProductTaxonomyKind,
    ) {
        let needs_hydration = match kind {
            StorefrontProductTaxonomyKind::Tag => {
                !self.store.base.storefront_product_tags.is_object()
            }
            StorefrontProductTaxonomyKind::ProductType => {
                !self.store.base.storefront_product_types.is_object()
            }
        };
        if !needs_hydration {
            return;
        }
        let response = self.storefront_upstream_post(
            request,
            json!({
                "query": STOREFRONT_ENRICHMENT_TAXONOMY_HYDRATE_QUERY,
                "variables": {}
            }),
        );
        if !(200..300).contains(&response.status) {
            return;
        }
        if let Some(connection) = response
            .body
            .pointer("/data/productTags")
            .filter(|value| value.is_object())
        {
            self.store.base.storefront_product_tags = connection.clone();
        }
        if let Some(connection) = response
            .body
            .pointer("/data/productTypes")
            .filter(|value| value.is_object())
        {
            self.store.base.storefront_product_types = connection.clone();
        }
    }

    fn storefront_upstream_post(&self, request: &Request, body: Value) -> Response {
        (self.storefront_upstream_transport)(Request {
            method: "POST".to_string(),
            path: request.path.clone(),
            headers: request.headers.clone(),
            body: body.to_string(),
        })
    }

    fn hydrate_storefront_first_slice_from_data(
        &mut self,
        data: &Value,
        context: &StorefrontRequestContext,
    ) {
        if let Some(shop) = data.get("shop").filter(|shop| shop.is_object()) {
            self.store.base.storefront_shop =
                shallow_merged_object(self.store.base.storefront_shop.clone(), shop.clone());
        }
        if let Some(localization) = data.get("localization").filter(|value| value.is_object()) {
            self.store
                .base
                .storefront_localizations
                .insert(context.key(), localization.clone());
        }
        if let Some(settings) = data
            .get("paymentSettings")
            .filter(|value| value.is_object())
        {
            self.store.base.storefront_payment_settings = shallow_merged_object(
                self.store.base.storefront_payment_settings.clone(),
                settings.clone(),
            );
        } else if let Some(settings) = data
            .pointer("/shop/paymentSettings")
            .filter(|value| value.is_object())
        {
            self.store.base.storefront_payment_settings = shallow_merged_object(
                self.store.base.storefront_payment_settings.clone(),
                settings.clone(),
            );
        }
        if let Some(versions) = data.get("publicApiVersions").and_then(Value::as_array) {
            self.store.base.storefront_public_api_versions = versions.clone();
        }
        self.hydrate_storefront_locations_from_connection(data.get("locations"));
    }

    fn hydrate_storefront_locations_from_connection(&mut self, connection: Option<&Value>) {
        let Some(connection) = connection.filter(|value| value.is_object()) else {
            return;
        };
        let mut cursor_by_id = BTreeMap::new();
        if let Some(edges) = connection.get("edges").and_then(Value::as_array) {
            for edge in edges {
                let Some(node) = edge.get("node").filter(|node| node.is_object()) else {
                    continue;
                };
                if let (Some(id), Some(cursor)) = (
                    node.get("id").and_then(Value::as_str),
                    edge.get("cursor").and_then(Value::as_str),
                ) {
                    cursor_by_id.insert(id.to_string(), cursor.to_string());
                }
            }
        }
        for node in connection_nodes(connection) {
            let Some(id) = node.get("id").and_then(Value::as_str).map(str::to_string) else {
                continue;
            };
            self.store
                .base
                .storefront_locations
                .insert(id.clone(), node);
            if let Some(cursor) = cursor_by_id.get(&id) {
                self.store
                    .base
                    .storefront_location_cursors
                    .insert(id, cursor.clone());
            }
        }
    }

    fn hydrate_storefront_menu(&mut self, request: &Request, handle: &str) {
        let response = self.storefront_upstream_post(
            request,
            json!({
                "query": STOREFRONT_MENU_HYDRATE_QUERY,
                "variables": { "handle": handle }
            }),
        );
        if !(200..300).contains(&response.status) {
            return;
        }
        let Some(menu) = response
            .body
            .pointer("/data/menu")
            .filter(|menu| menu.is_object())
            .cloned()
        else {
            return;
        };
        let Some(id) = menu.get("id").and_then(Value::as_str).map(str::to_string) else {
            return;
        };
        self.store.base.storefront_menus.insert(id, menu);
    }

    fn storefront_node_root_value(
        &self,
        arguments: &BTreeMap<String, Value>,
        context: &StorefrontRequestContext,
    ) -> Value {
        arguments
            .get("id")
            .and_then(Value::as_str)
            .map(|id| self.storefront_node_by_id_value(id, context))
            .unwrap_or(Value::Null)
    }

    fn storefront_nodes_root_value(
        &self,
        arguments: &BTreeMap<String, Value>,
        context: &StorefrontRequestContext,
    ) -> Value {
        Value::Array(
            arguments
                .get("ids")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(|id| self.storefront_node_by_id_value(id, context))
                .collect(),
        )
    }

    fn storefront_node_by_id_value(&self, id: &str, context: &StorefrontRequestContext) -> Value {
        match self.storefront_node_load_state(id, context) {
            NodeLoadState::Found(entity) => entity.value,
            NodeLoadState::KnownMissing
            | NodeLoadState::NeedsHydration
            | NodeLoadState::UnsupportedType => Value::Null,
        }
    }

    fn storefront_node_load_state(
        &self,
        id: &str,
        context: &StorefrontRequestContext,
    ) -> NodeLoadState<EntityRef> {
        let key = RequestEntityCacheKey::storefront(
            self.execution_session.api_version(ApiSurface::Storefront),
            id,
            context.key(),
        );
        self.cached_request_entity_load_state(key, || {
            let value = self.uncached_storefront_node_by_id_value(id, context);
            if value.is_null() {
                return NodeLoadState::KnownMissing;
            }
            let Some(type_name) = value
                .get("__typename")
                .and_then(Value::as_str)
                .or_else(|| shopify_gid_resource_type(id))
                .map(str::to_string)
            else {
                return NodeLoadState::UnsupportedType;
            };
            NodeLoadState::Found(EntityRef::new(type_name, id, value))
        })
    }

    fn uncached_storefront_node_by_id_value(
        &self,
        id: &str,
        context: &StorefrontRequestContext,
    ) -> Value {
        match shopify_gid_resource_type(id) {
            Some("Product") => {
                self.storefront_visible_product_value(self.store.product_by_id(id), context)
            }
            Some("ProductVariant") => self
                .store
                .product_variant_by_id(id)
                .filter(|variant| {
                    self.store
                        .product_by_id(&variant.product_id)
                        .is_some_and(|product| self.storefront_product_is_visible(product))
                })
                .map(|variant| {
                    storefront_product_variant_value(
                        self,
                        variant,
                        self.store.product_by_id(&variant.product_id),
                        context,
                        None,
                    )
                })
                .unwrap_or(Value::Null),
            Some("Collection") => {
                self.storefront_visible_collection_value(self.store.collection_by_id(id))
            }
            Some("Article") => self
                .storefront_content_by_id(StorefrontContentKind::Article, id)
                .unwrap_or(Value::Null),
            Some("Blog") => self
                .storefront_content_by_id(StorefrontContentKind::Blog, id)
                .unwrap_or(Value::Null),
            Some("Page") => self
                .storefront_content_by_id(StorefrontContentKind::Page, id)
                .unwrap_or(Value::Null),
            Some("Metaobject") => self
                .metaobject_by_id(id)
                .and_then(|record| self.storefront_visible_metaobject(&record))
                .map(|record| self.storefront_metaobject_value(&record))
                .unwrap_or(Value::Null),
            Some("Location") => self
                .storefront_location_records()
                .into_iter()
                .find(|record| record.get("id").and_then(Value::as_str) == Some(id))
                .unwrap_or(Value::Null),
            Some("Menu") => self
                .store
                .base
                .storefront_menus
                .ordered_values()
                .into_iter()
                .find(|record| record.get("id").and_then(Value::as_str) == Some(id))
                .cloned()
                .unwrap_or(Value::Null),
            _ => Value::Null,
        }
    }

    fn storefront_search_root_json(
        &self,
        arguments: &BTreeMap<String, Value>,
        context: &StorefrontRequestContext,
    ) -> Value {
        let arguments = resolved_arguments_from_json(arguments);
        let mut items = self.storefront_search_items(&arguments);
        storefront_sort_search_items(self, &mut items, &arguments);
        let total_count = items.len();
        let filter_items = items.clone();
        let (items, page_info) =
            connection_window(&items, &arguments, storefront_search_item_cursor);
        json!({
            "nodes": Value::Array(
                items
                    .iter()
                    .map(|item| self.storefront_search_item_json(item, context))
                    .collect(),
            ),
            "edges": Value::Array(
                items
                    .iter()
                    .map(|item| {
                        json!({
                            "cursor": storefront_search_item_cursor(item),
                            "node": self.storefront_search_item_json(item, context)
                        })
                    })
                    .collect(),
            ),
            "pageInfo": page_info,
            "totalCount": total_count,
            "productFilters": storefront_search_product_filters(self, &filter_items),
        })
    }

    fn storefront_search_items(
        &self,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) -> Vec<StorefrontSearchItem> {
        let requested_types = list_string_field(arguments, "types");
        let includes = |name: &str| {
            requested_types
                .first()
                .is_none_or(|requested_type| requested_type == name)
        };
        let query = resolved_string_field(arguments, "query").unwrap_or_default();
        let prefix =
            resolved_string_field(arguments, "prefix").unwrap_or_else(|| "NONE".to_string());
        let unavailable = resolved_string_field(arguments, "unavailableProducts")
            .unwrap_or_else(|| "LAST".to_string());
        let product_filters = resolved_object_list_field(arguments, "productFilters");
        let mut items = Vec::new();
        if includes("PRODUCT") {
            items.extend(
                self.storefront_visible_products()
                    .into_iter()
                    .filter(|product| {
                        storefront_product_matches_discovery_query(
                            self,
                            product,
                            &query,
                            &prefix,
                            &[],
                        )
                    })
                    .filter(|product| {
                        storefront_product_matches_search_filters(self, product, &product_filters)
                    })
                    .filter(|product| {
                        unavailable != "HIDE" || storefront_search_product_available(self, product)
                    })
                    .map(|product| StorefrontSearchItem::Product(Box::new(product))),
            );
        }
        if includes("ARTICLE") {
            items.extend(
                self.storefront_article_records()
                    .into_iter()
                    .filter(|record| {
                        storefront_value_matches_discovery_query(record, &query, &prefix, &[])
                    })
                    .map(StorefrontSearchItem::Article),
            );
        }
        if includes("PAGE") {
            items.extend(
                self.storefront_page_records()
                    .into_iter()
                    .filter(|record| {
                        storefront_value_matches_discovery_query(record, &query, &prefix, &[])
                    })
                    .map(StorefrontSearchItem::Page),
            );
        }
        if unavailable == "LAST" {
            items.sort_by_key(|item| match item {
                StorefrontSearchItem::Product(product) => {
                    !storefront_search_product_available(self, product)
                }
                _ => false,
            });
        }
        items
    }

    fn storefront_search_item_json(
        &self,
        item: &StorefrontSearchItem,
        context: &StorefrontRequestContext,
    ) -> Value {
        let (mut projected, type_name, id) = match item {
            StorefrontSearchItem::Product(product) => {
                let variants = self.store.product_variants_for_product(&product.id);
                (
                    storefront_product_value(self, product, &variants, context),
                    "Product",
                    product.id.as_str(),
                )
            }
            StorefrontSearchItem::Article(article) => (
                article.clone(),
                "Article",
                article
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
            ),
            StorefrontSearchItem::Page(page) => (
                page.clone(),
                "Page",
                page.get("id").and_then(Value::as_str).unwrap_or_default(),
            ),
        };
        if let Some(object) = projected.as_object_mut() {
            object
                .entry("__typename".to_string())
                .or_insert_with(|| json!(type_name));
            object.entry("id".to_string()).or_insert_with(|| json!(id));
        }
        projected
    }

    fn storefront_predictive_search_root_json(
        &self,
        arguments: &BTreeMap<String, Value>,
        context: &StorefrontRequestContext,
    ) -> Value {
        let arguments = resolved_arguments_from_json(arguments);
        let query = resolved_string_field(&arguments, "query").unwrap_or_default();
        let limit = resolved_int_field(&arguments, "limit")
            .unwrap_or(10)
            .clamp(1, 10) as usize;
        let limit_scope =
            resolved_string_field(&arguments, "limitScope").unwrap_or_else(|| "ALL".to_string());
        let requested_types = list_string_field(&arguments, "types");
        let includes = |name: &str| {
            requested_types.is_empty() || requested_types.iter().any(|value| value == name)
        };
        let searchable_fields = list_string_field(&arguments, "searchableFields");
        let unavailable = resolved_string_field(&arguments, "unavailableProducts")
            .unwrap_or_else(|| "LAST".to_string());
        let mut products = if includes("PRODUCT") {
            self.storefront_visible_products()
                .into_iter()
                .filter(|product| {
                    storefront_product_matches_discovery_query(
                        self,
                        product,
                        &query,
                        "LAST",
                        &searchable_fields,
                    )
                })
                .filter(|product| {
                    unavailable != "HIDE" || storefront_search_product_available(self, product)
                })
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        products.sort_by(|left, right| {
            left.title
                .to_ascii_lowercase()
                .cmp(&right.title.to_ascii_lowercase())
                .then_with(|| left.id.cmp(&right.id))
        });
        if unavailable == "LAST" {
            products.sort_by_key(|product| !storefront_search_product_available(self, product));
        }
        let mut collections = if includes("COLLECTION") {
            self.store
                .staged
                .collections
                .values()
                .filter(|record| self.storefront_collection_is_visible(record))
                .filter(|record| {
                    storefront_value_matches_discovery_query(
                        record,
                        &query,
                        "LAST",
                        &searchable_fields,
                    )
                })
                .cloned()
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        let mut articles = if includes("ARTICLE") {
            self.storefront_article_records()
                .into_iter()
                .filter(|record| {
                    storefront_value_matches_discovery_query(
                        record,
                        &query,
                        "LAST",
                        &searchable_fields,
                    )
                })
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        let mut pages = if includes("PAGE") {
            self.storefront_page_records()
                .into_iter()
                .filter(|record| {
                    storefront_value_matches_discovery_query(
                        record,
                        &query,
                        "LAST",
                        &searchable_fields,
                    )
                })
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        for records in [&mut collections, &mut articles, &mut pages] {
            records.sort_by(|left, right| {
                storefront_value_title(left)
                    .cmp(&storefront_value_title(right))
                    .then_with(|| value_id_cursor(left).cmp(&value_id_cursor(right)))
            });
        }
        if limit_scope == "ALL" {
            let mut remaining = limit;
            truncate_with_remaining(&mut products, &mut remaining);
            truncate_with_remaining(&mut collections, &mut remaining);
            truncate_with_remaining(&mut pages, &mut remaining);
            truncate_with_remaining(&mut articles, &mut remaining);
        } else {
            products.truncate(limit);
            collections.truncate(limit);
            articles.truncate(limit);
            pages.truncate(limit);
        }
        let suggestions = if includes("QUERY") {
            let suggestion_products = self.storefront_visible_products();
            let suggestion_collections = self
                .store
                .staged
                .collections
                .values()
                .filter(|record| self.storefront_collection_is_visible(record))
                .cloned()
                .collect::<Vec<_>>();
            let suggestion_articles = self.storefront_article_records();
            let suggestion_pages = self.storefront_page_records();
            storefront_query_suggestions(
                &query,
                limit,
                &suggestion_products,
                &suggestion_collections,
                &suggestion_articles,
                &suggestion_pages,
            )
        } else {
            Vec::new()
        };
        json!({
            "products": products
                .iter()
                .map(|product| {
                    let variants = self.store.product_variants_for_product(&product.id);
                    storefront_product_value(self, product, &variants, context)
                })
                .collect::<Vec<_>>(),
            "collections": collections
                .iter()
                .map(|record| self.storefront_collection_value(record))
                .collect::<Vec<_>>(),
            "articles": articles,
            "pages": pages,
            "queries": suggestions,
        })
    }

    fn storefront_collections_need_hydration(&self) -> bool {
        self.store.staged.collections.is_empty()
    }

    fn hydrate_storefront_collections(&mut self, request: &Request) {
        let response = (self.storefront_upstream_transport)(request.clone());
        if (200..300).contains(&response.status) {
            self.observe_storefront_collection_value(&response.body["data"]);
        }
    }

    fn observe_storefront_collection_value(&mut self, value: &Value) {
        if value
            .get("id")
            .and_then(Value::as_str)
            .is_some_and(|id| is_shopify_gid_of_type(id, "Collection"))
        {
            let mut observed = value.clone();
            observed["__storefrontVisible"] = json!(true);
            let observed_products = storefront_collection_observed_products(&observed);
            if !observed_products.is_empty() {
                observed["products"] = connection_json(observed_products);
                if value.get("products").is_some() {
                    observed[STOREFRONT_CAPTURED_COLLECTION_DEFAULT_ORDER_FIELD] = json!(true);
                }
            }
            let owner_id = observed
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            self.stage_collection_from_observed_json(&observed);
            let mut metafields = observed
                .get("metafields")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter(|value| value.is_object())
                .cloned()
                .collect::<Vec<_>>();
            if let Some(metafield) = observed.get("metafield").filter(|value| value.is_object()) {
                metafields.push(metafield.clone());
            }
            for metafield in &mut metafields {
                metafield["__storefrontPublic"] = json!(true);
            }
            if !metafields.is_empty() {
                self.stage_observed_owner_metafields(
                    &owner_id,
                    &json!({ "metafields": { "nodes": metafields } }),
                );
            }
        }
        match value {
            Value::Array(values) => {
                for value in values {
                    self.observe_storefront_collection_value(value);
                }
            }
            Value::Object(object) => {
                for value in object.values() {
                    self.observe_storefront_collection_value(value);
                }
            }
            _ => {}
        }
    }

    fn storefront_collection_root_value(&self, arguments: &BTreeMap<String, Value>) -> Value {
        let collection = arguments
            .get("id")
            .and_then(Value::as_str)
            .and_then(|id| self.store.collection_by_id(id))
            .or_else(|| {
                arguments
                    .get("handle")
                    .and_then(Value::as_str)
                    .and_then(|handle| self.store.collection_by_handle(handle))
            });
        self.storefront_visible_collection_value(collection)
    }

    fn storefront_collection_by_handle_root_value(
        &self,
        arguments: &BTreeMap<String, Value>,
    ) -> Value {
        let collection = arguments
            .get("handle")
            .and_then(Value::as_str)
            .and_then(|handle| self.store.collection_by_handle(handle));
        self.storefront_visible_collection_value(collection)
    }

    fn storefront_visible_collection_value(&self, collection: Option<&Value>) -> Value {
        collection
            .filter(|collection| self.storefront_collection_is_visible(collection))
            .map(|collection| self.storefront_collection_value(collection))
            .unwrap_or(Value::Null)
    }

    fn storefront_collections_connection_value(
        &self,
        arguments: &BTreeMap<String, Value>,
    ) -> Value {
        let arguments = resolved_arguments_from_json(arguments);
        staged_connection_value_with_args(
            self.store
                .staged
                .collections
                .values()
                .filter(|collection| self.storefront_collection_is_visible(collection))
                .cloned()
                .collect(),
            &arguments,
            |collection, query| self.collection_search_decision(collection, query),
            |collection, sort_key| self.storefront_collection_sort_key(collection, sort_key),
            |collection| self.storefront_collection_value(collection),
            value_id_cursor,
        )
    }

    fn storefront_collection_value(&self, collection: &Value) -> Value {
        let description = collection
            .get("description")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| {
                storefront_strip_html(
                    collection
                        .get("descriptionHtml")
                        .and_then(Value::as_str)
                        .unwrap_or_default(),
                )
            });
        json!({
            "__typename": "Collection",
            "description": description,
            "descriptionHtml": collection
                .get("descriptionHtml")
                .cloned()
                .unwrap_or_else(|| json!("")),
            "handle": collection.get("handle").cloned().unwrap_or(Value::Null),
            "id": collection.get("id").cloned().unwrap_or(Value::Null),
            "image": collection.get("image").cloned().unwrap_or(Value::Null),
            "onlineStoreUrl": collection
                .get("onlineStoreUrl")
                .cloned()
                .unwrap_or(Value::Null),
            "seo": storefront_collection_seo(collection),
            "title": collection.get("title").cloned().unwrap_or(Value::Null),
            "trackingParameters": collection
                .get("trackingParameters")
                .cloned()
                .unwrap_or(Value::Null),
            "updatedAt": collection.get("updatedAt").cloned().unwrap_or(Value::Null),
        })
    }

    fn storefront_collection_is_visible(&self, collection: &Value) -> bool {
        let Some(id) = collection.get("id").and_then(Value::as_str) else {
            return false;
        };
        if let Some(publications) = self.store.staged.resource_publications.get(id) {
            if self.store.staged.current_channel_publication_resolved {
                return self.store.resource_is_published_on_current_publication(id);
            }
            return publications
                .iter()
                .any(|publication_id| self.store.has_publication_id(publication_id));
        }
        collection
            .get("__storefrontVisible")
            .and_then(Value::as_bool)
            .unwrap_or(false)
    }

    fn storefront_collection_sort_key(
        &self,
        collection: &Value,
        sort_key: Option<&str>,
    ) -> StagedSortKey {
        if sort_key != Some("UPDATED_AT") {
            return collection_staged_sort_key(collection, sort_key);
        }

        let has_hidden_member = collection
            .pointer("/products/nodes")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|product| product.get("id").and_then(Value::as_str))
            .any(|id| {
                self.store.product_is_tombstoned(id)
                    || self
                        .store
                        .product_by_id(id)
                        .is_some_and(|product| !self.storefront_product_is_visible(product))
            });
        let projected_updated_at = if has_hidden_member {
            collection.get("updatedAt")
        } else {
            collection
                .get(STOREFRONT_COLLECTION_BASELINE_UPDATED_AT_FIELD)
                .or_else(|| collection.get("updatedAt"))
        }
        .and_then(Value::as_str)
        .unwrap_or_default();
        let id = collection
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default();
        vec![
            StagedSortValue::String(projected_updated_at.to_string()),
            resource_id_tail_sort_value(Some(id)),
        ]
    }

    fn storefront_product_root_value(
        &self,
        arguments: &BTreeMap<String, Value>,
        context: &StorefrontRequestContext,
    ) -> Value {
        let product = arguments
            .get("id")
            .and_then(Value::as_str)
            .and_then(|id| self.store.product_by_id(id))
            .or_else(|| {
                arguments
                    .get("handle")
                    .and_then(Value::as_str)
                    .and_then(|handle| self.store.product_by_handle(handle))
            });
        self.storefront_visible_product_value(product, context)
    }

    fn storefront_product_by_handle_root_value(
        &self,
        arguments: &BTreeMap<String, Value>,
        context: &StorefrontRequestContext,
    ) -> Value {
        let product = arguments
            .get("handle")
            .and_then(Value::as_str)
            .and_then(|handle| self.store.product_by_handle(handle));
        self.storefront_visible_product_value(product, context)
    }

    fn storefront_visible_product_value(
        &self,
        product: Option<&ProductRecord>,
        context: &StorefrontRequestContext,
    ) -> Value {
        let Some(product) = product.filter(|product| self.storefront_product_is_visible(product))
        else {
            return Value::Null;
        };
        let variants = self.store.product_variants_for_product(&product.id);
        storefront_product_value(self, product, &variants, context)
    }

    fn storefront_products_connection_value(
        &self,
        arguments: &BTreeMap<String, Value>,
        context: &StorefrontRequestContext,
    ) -> Value {
        let arguments = arguments
            .iter()
            .map(|(name, value)| (name.clone(), resolved_value_from_json(value)))
            .collect();
        let result = staged_connection_query(
            self.storefront_visible_products(),
            &arguments,
            |product, query| self.storefront_product_search_decision(product, query),
            |product, sort_key| self.storefront_product_sort_key(product, sort_key),
            |product| product_cursor(product).to_string(),
        );
        connection_json_with_cursor(
            result
                .records
                .iter()
                .map(|product| {
                    let variants = self.store.product_variants_for_product(&product.id);
                    storefront_product_value(self, product, &variants, context)
                })
                .collect(),
            |index, _| {
                result
                    .records
                    .get(index)
                    .map(product_cursor)
                    .unwrap_or_default()
                    .to_string()
            },
            result.page_info,
        )
    }

    fn storefront_product_recommendations_json(
        &self,
        arguments: &BTreeMap<String, Value>,
        context: &StorefrontRequestContext,
    ) -> Value {
        let source = arguments
            .get("productId")
            .and_then(Value::as_str)
            .and_then(|id| self.store.product_by_id(id))
            .or_else(|| {
                arguments
                    .get("productHandle")
                    .and_then(Value::as_str)
                    .and_then(|handle| self.store.product_by_handle(handle))
            })
            .filter(|product| self.storefront_product_is_visible(product));
        let Some(source) = source else {
            return Value::Null;
        };
        let mut candidates = self
            .storefront_visible_products()
            .into_iter()
            .filter(|candidate| candidate.id != source.id)
            .map(|candidate| {
                let shared_tags = candidate
                    .tags
                    .iter()
                    .filter(|tag| source.tags.iter().any(|source_tag| source_tag == *tag))
                    .count();
                let score = shared_tags * 4
                    + usize::from(
                        !source.product_type.is_empty()
                            && candidate.product_type == source.product_type,
                    ) * 3
                    + usize::from(!source.vendor.is_empty() && candidate.vendor == source.vendor)
                        * 2;
                (score, candidate)
            })
            .collect::<Vec<_>>();
        candidates.sort_by(|(left_score, left), (right_score, right)| {
            right_score
                .cmp(left_score)
                .then_with(|| {
                    left.title
                        .to_ascii_lowercase()
                        .cmp(&right.title.to_ascii_lowercase())
                })
                .then_with(|| left.id.cmp(&right.id))
        });
        Value::Array(
            candidates
                .into_iter()
                .take(10)
                .map(|(_, product)| {
                    let variants = self.store.product_variants_for_product(&product.id);
                    storefront_product_value(self, &product, &variants, context)
                })
                .collect(),
        )
    }

    fn storefront_product_taxonomy_connection_json(
        &self,
        arguments: &BTreeMap<String, Value>,
        kind: StorefrontProductTaxonomyKind,
    ) -> Value {
        let observed = match kind {
            StorefrontProductTaxonomyKind::Tag => &self.store.base.storefront_product_tags,
            StorefrontProductTaxonomyKind::ProductType => &self.store.base.storefront_product_types,
        };
        let mut values = if observed.is_object() {
            connection_nodes(observed)
                .into_iter()
                .filter_map(|value| value.as_str().map(str::to_string))
                .collect::<Vec<_>>()
        } else {
            self.storefront_visible_products()
                .into_iter()
                .flat_map(|product| match kind {
                    StorefrontProductTaxonomyKind::Tag => product.tags,
                    StorefrontProductTaxonomyKind::ProductType => vec![product.product_type],
                })
                .collect::<Vec<_>>()
        };
        values.sort_by(|left, right| {
            left.to_ascii_lowercase()
                .cmp(&right.to_ascii_lowercase())
                .then_with(|| left.cmp(right))
        });
        values.dedup();
        let arguments = resolved_arguments_from_json(arguments);
        connection_value_with_args(
            values.into_iter().map(Value::String).collect(),
            &arguments,
            |value| {
                base64::engine::general_purpose::STANDARD
                    .encode(value.as_str().unwrap_or_default().as_bytes())
            },
        )
    }

    fn storefront_context_localization(
        &self,
        context: &StorefrontRequestContext,
    ) -> Option<&Value> {
        self.store
            .base
            .storefront_localizations
            .get(&context.key())
            .or_else(|| {
                context.country.as_deref().and_then(|country_code| {
                    self.store
                        .base
                        .storefront_localizations
                        .values()
                        .find(|localization| {
                            localization
                                .pointer("/country/isoCode")
                                .and_then(Value::as_str)
                                == Some(country_code)
                        })
                })
            })
            .or_else(|| {
                self.store
                    .base
                    .storefront_localizations
                    .get(STOREFRONT_DEFAULT_CONTEXT_KEY)
            })
    }

    fn storefront_context_price_list(&self, context: &StorefrontRequestContext) -> Option<&Value> {
        let localization = self.storefront_context_localization(context)?;
        let observed_market_id = localization.pointer("/market/id").and_then(Value::as_str);
        let observed_market_handle = localization
            .pointer("/market/handle")
            .and_then(Value::as_str);
        let market_id = self
            .store
            .staged
            .markets
            .iter()
            .find_map(|(id, market)| {
                (market.get("handle").and_then(Value::as_str) == observed_market_handle)
                    .then_some(id.as_str())
            })
            .or(observed_market_id)?;
        let catalog = self.store.staged.catalogs.values().find(|catalog| {
            catalog.get("status").and_then(Value::as_str) == Some("ACTIVE")
                && catalog_market_ids(catalog).iter().any(|id| id == market_id)
        })?;
        let price_list_id = catalog_relation_id(catalog, "priceListId", "priceList")?;
        self.store.staged.price_lists.get(&price_list_id)
    }

    pub(in crate::proxy) fn storefront_variant_pricing(
        &self,
        variant: &ProductVariantRecord,
        context: &StorefrontRequestContext,
    ) -> StorefrontVariantPricing {
        let contextual_price_list = self.storefront_context_price_list(context);
        let fixed_price = contextual_price_list.and_then(|price_list| {
            price_edges(price_list).into_iter().find_map(|edge| {
                (fixed_price_edge_variant_id(&edge).as_deref() == Some(variant.id.as_str()))
                    .then(|| edge.get("node").cloned())
                    .flatten()
            })
        });
        let currency_code = self
            .storefront_context_localization(context)
            .and_then(|localization| localization.pointer("/country/currency/isoCode"))
            .and_then(Value::as_str)
            .map(str::to_string)
            .or_else(|| contextual_price_list.map(price_list_currency))
            .or_else(|| self.store.observed_shop_currency_code())
            .or_else(|| {
                variant
                    .extra_fields
                    .get("currencyCode")
                    .and_then(Value::as_str)
                    .map(str::to_string)
            })
            .unwrap_or_default();
        StorefrontVariantPricing {
            price: fixed_price
                .as_ref()
                .and_then(|price| price.pointer("/price/amount"))
                .and_then(Value::as_str)
                .unwrap_or(&variant.price)
                .to_string(),
            compare_at_price: fixed_price
                .as_ref()
                .and_then(|price| price.pointer("/compareAtPrice/amount"))
                .and_then(Value::as_str)
                .map(str::to_string)
                .or_else(|| variant.compare_at_price.clone()),
            currency_code,
        }
    }

    fn storefront_visible_products(&self) -> Vec<ProductRecord> {
        self.store
            .products()
            .into_iter()
            .filter(|product| self.storefront_product_is_visible(product))
            .collect()
    }

    pub(in crate::proxy) fn storefront_product_is_visible(&self, product: &ProductRecord) -> bool {
        if product.status != "ACTIVE" {
            return false;
        }
        if let Some(publications) = self.store.staged.resource_publications.get(&product.id) {
            if self.store.staged.current_channel_publication_resolved {
                return self
                    .store
                    .product_is_published_on_current_publication(product);
            }
            return publications
                .iter()
                .any(|publication_id| self.store.has_publication_id(publication_id));
        }
        if product
            .extra_fields
            .get("__storefrontVisible")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            return true;
        }
        if self.store.staged.current_channel_publication_resolved {
            return self
                .store
                .product_is_published_on_current_publication(product);
        }
        self.store
            .product_is_published_on_known_publication(product)
    }

    fn storefront_product_search_decision(
        &self,
        product: &ProductRecord,
        query: Option<&str>,
    ) -> StagedSearchDecision {
        let Some(query) = query else {
            return StagedSearchDecision::Match;
        };
        let variants = self.store.product_variants_for_product(&product.id);
        StagedSearchDecision::from_bool(product_matches_search_query(product, &variants, query))
    }

    fn storefront_product_sort_key(
        &self,
        product: &ProductRecord,
        sort_key: Option<&str>,
    ) -> StagedSortKey {
        let variants = self.store.product_variants_for_product(&product.id);
        storefront_product_sort_key(product, &variants, sort_key)
    }

    pub(in crate::proxy) fn storefront_currency_code(&self) -> String {
        self.store
            .observed_shop_currency_code()
            .unwrap_or_else(|| "USD".to_string())
    }

    fn storefront_shop_value(&self) -> Value {
        let storefront_shop = self.store.base.storefront_shop.clone();
        let admin_shop = if self.store.base.shop.is_object() {
            self.store.effective_shop()
        } else {
            Value::Null
        };
        let has_shop = storefront_shop.is_object()
            || admin_shop.is_object()
            || self.storefront_has_local_shop_metafield_state();
        if !has_shop {
            return Value::Null;
        }
        let mut value = serde_json::Map::new();
        value.insert("__typename".to_string(), json!("Shop"));
        for field in [
            "brand",
            "customerAccountTranslations",
            "customerAccountUrl",
            "description",
            "id",
            "name",
            "shipsToCountries",
            "shopPayInstallmentsPricing",
            "socialLoginProviders",
            "subscriptionPolicy",
        ] {
            value.insert(
                field.to_string(),
                self.storefront_shop_field(&storefront_shop, &admin_shop, field)
                    .unwrap_or(Value::Null),
            );
        }
        value.insert(
            "paymentSettings".to_string(),
            self.storefront_payment_settings_value(),
        );
        value.insert(
            "primaryDomain".to_string(),
            self.storefront_shop_field(&storefront_shop, &admin_shop, "primaryDomain")
                .unwrap_or(Value::Null),
        );
        for (field, policy_type) in [
            ("privacyPolicy", "PRIVACY_POLICY"),
            ("refundPolicy", "REFUND_POLICY"),
            ("shippingPolicy", "SHIPPING_POLICY"),
            ("termsOfService", "TERMS_OF_SERVICE"),
            ("termsOfSale", "TERMS_OF_SALE"),
            ("legalNotice", "LEGAL_NOTICE"),
            ("contactInformation", "CONTACT_INFORMATION"),
        ] {
            value.insert(
                field.to_string(),
                self.storefront_shop_policy_value(&storefront_shop, field, policy_type)
                    .unwrap_or(Value::Null),
            );
        }
        value.insert(
            "moneyFormat".to_string(),
            self.storefront_shop_field(&storefront_shop, &admin_shop, "moneyFormat")
                .or_else(|| self.store.shop_money_format().map(Value::String))
                .unwrap_or(Value::Null),
        );
        Value::Object(value)
    }

    fn storefront_shop_field(
        &self,
        storefront_shop: &Value,
        admin_shop: &Value,
        field: &str,
    ) -> Option<Value> {
        storefront_shop
            .get(field)
            .cloned()
            .or_else(|| admin_shop.get(field).cloned())
    }

    fn storefront_shop_policy_value(
        &self,
        storefront_shop: &Value,
        storefront_field: &str,
        policy_type: &str,
    ) -> Option<Value> {
        if let Some(policy) = storefront_shop.get(storefront_field) {
            return Some(policy.clone());
        }
        let policy = self.store.shop_policy_by_type(policy_type)?;
        Some(storefront_policy_from_admin(policy))
    }

    fn storefront_localization_is_observed(&self, context: &StorefrontRequestContext) -> bool {
        self.store
            .base
            .storefront_localizations
            .contains_key(&context.key())
    }

    fn storefront_payment_settings_source(&self) -> Option<Value> {
        if self.store.base.storefront_payment_settings.is_object() {
            return Some(self.store.base.storefront_payment_settings.clone());
        }
        self.admin_storefront_payment_settings_source()
    }

    fn storefront_payment_settings_value(&self) -> Value {
        let Some(source) = self.storefront_payment_settings_source() else {
            return Value::Null;
        };
        let mut value = serde_json::Map::new();
        for field in [
            "acceptedCardBrands",
            "cardVaultUrl",
            "countryCode",
            "currencyCode",
            "enabledPresentmentCurrencies",
            "shopifyPaymentsAccountId",
            "supportedDigitalWallets",
        ] {
            value.insert(
                field.to_string(),
                source.get(field).cloned().unwrap_or_else(|| match field {
                    "acceptedCardBrands"
                    | "enabledPresentmentCurrencies"
                    | "supportedDigitalWallets" => json!([]),
                    _ => Value::Null,
                }),
            );
        }
        Value::Object(value)
    }

    fn admin_storefront_payment_settings_source(&self) -> Option<Value> {
        let shop = self.store.effective_shop();
        let mut settings = serde_json::Map::new();
        if let Some(value) = shop.pointer("/paymentSettings/supportedDigitalWallets") {
            settings.insert("supportedDigitalWallets".to_string(), value.clone());
        }
        if let Some(value) = shop.get("currencyCode") {
            settings.insert("currencyCode".to_string(), value.clone());
        }
        if let Some(value) = shop.get("enabledPresentmentCurrencies") {
            settings.insert("enabledPresentmentCurrencies".to_string(), value.clone());
        }
        if let Some(value) = shop.pointer("/shopAddress/countryCodeV2") {
            settings.insert("countryCode".to_string(), value.clone());
        }
        (!settings.is_empty()).then_some(Value::Object(settings))
    }

    fn storefront_locations_connection_value(&self, arguments: &BTreeMap<String, Value>) -> Value {
        let arguments = arguments
            .iter()
            .map(|(name, value)| (name.clone(), resolved_value_from_json(value)))
            .collect();
        let mut records = self.storefront_location_records();
        sort_storefront_locations(&mut records, &arguments);
        let cursor_by_id = self.storefront_location_cursor_map(&records);
        let (records, page_info) = connection_window(&records, &arguments, |location| {
            storefront_location_cursor(location, &cursor_by_id)
        });
        connection_json_with_cursor(
            records,
            |_, location| storefront_location_cursor(location, &cursor_by_id),
            page_info,
        )
    }

    fn storefront_location_records(&self) -> Vec<Value> {
        let mut records = Vec::new();
        let mut seen = BTreeSet::new();
        for location in self.store.base.storefront_locations.ordered_values() {
            push_storefront_location(&mut records, &mut seen, location.clone());
        }
        for id in &self.store.staged.observed_shipping_location_order {
            if let Some(location) = self.store.staged.observed_shipping_locations.get(id) {
                push_admin_location_as_storefront(&mut records, &mut seen, location);
            }
        }
        for location in self.store.staged.observed_shipping_locations.values() {
            push_admin_location_as_storefront(&mut records, &mut seen, location);
        }
        for id in &self.store.staged.locations.order {
            if let Some(location) = self.store.staged.locations.get(id) {
                push_admin_location_as_storefront(&mut records, &mut seen, location);
            }
        }
        for (_, location) in self.store.staged.locations.iter() {
            push_admin_location_as_storefront(&mut records, &mut seen, location);
        }
        records
    }

    fn storefront_location_cursor_map(&self, records: &[Value]) -> BTreeMap<String, String> {
        records
            .iter()
            .filter_map(|location| {
                let id = location.get("id").and_then(Value::as_str)?;
                let cursor = self
                    .store
                    .base
                    .storefront_location_cursors
                    .get(id)
                    .cloned()
                    .unwrap_or_else(|| id.to_string());
                Some((id.to_string(), cursor))
            })
            .collect()
    }

    fn storefront_content_root_value(
        &self,
        root_name: &str,
        arguments: &BTreeMap<String, Value>,
    ) -> Value {
        let string_argument = |name: &str| {
            arguments
                .get(name)
                .and_then(Value::as_str)
                .unwrap_or_default()
        };
        match root_name {
            "article" => self
                .storefront_content_by_id(StorefrontContentKind::Article, string_argument("id"))
                .unwrap_or(Value::Null),
            "articles" => self.storefront_content_connection_value(
                StorefrontContentKind::Article,
                self.storefront_article_records(),
                arguments,
            ),
            "blog" => arguments
                .get("id")
                .and_then(Value::as_str)
                .and_then(|id| self.storefront_content_by_id(StorefrontContentKind::Blog, id))
                .or_else(|| {
                    arguments
                        .get("handle")
                        .and_then(Value::as_str)
                        .and_then(|handle| {
                            self.storefront_content_by_handle(StorefrontContentKind::Blog, handle)
                        })
                })
                .unwrap_or(Value::Null),
            "blogByHandle" => self
                .storefront_content_by_handle(
                    StorefrontContentKind::Blog,
                    string_argument("handle"),
                )
                .unwrap_or(Value::Null),
            "blogs" => self.storefront_content_connection_value(
                StorefrontContentKind::Blog,
                self.storefront_blog_records(),
                arguments,
            ),
            "page" => arguments
                .get("id")
                .and_then(Value::as_str)
                .and_then(|id| self.storefront_content_by_id(StorefrontContentKind::Page, id))
                .or_else(|| {
                    arguments
                        .get("handle")
                        .and_then(Value::as_str)
                        .and_then(|handle| {
                            self.storefront_content_by_handle(StorefrontContentKind::Page, handle)
                        })
                })
                .unwrap_or(Value::Null),
            "pageByHandle" => self
                .storefront_content_by_handle(
                    StorefrontContentKind::Page,
                    string_argument("handle"),
                )
                .unwrap_or(Value::Null),
            "pages" => self.storefront_content_connection_value(
                StorefrontContentKind::Page,
                self.storefront_page_records(),
                arguments,
            ),
            _ => Value::Null,
        }
    }

    fn storefront_content_connection_value(
        &self,
        kind: StorefrontContentKind,
        records: Vec<Value>,
        arguments: &BTreeMap<String, Value>,
    ) -> Value {
        let arguments = arguments
            .iter()
            .map(|(name, value)| (name.clone(), resolved_value_from_json(value)))
            .collect();
        let result = staged_connection_query(
            records,
            &arguments,
            |record, query| storefront_content_search_decision(kind, record, query),
            |record, sort_key| storefront_content_sort_key(kind, record, sort_key),
            value_id_cursor,
        );
        connection_json_with_cursor(
            result.records,
            |_, record| value_id_cursor(record),
            result.page_info,
        )
    }

    fn storefront_content_by_id(&self, kind: StorefrontContentKind, id: &str) -> Option<Value> {
        self.storefront_content_records(kind)
            .into_iter()
            .find(|record| record.get("id").and_then(Value::as_str) == Some(id))
    }

    fn storefront_content_by_handle(
        &self,
        kind: StorefrontContentKind,
        handle: &str,
    ) -> Option<Value> {
        self.storefront_content_records(kind)
            .into_iter()
            .find(|record| record.get("handle").and_then(Value::as_str) == Some(handle))
    }

    fn storefront_content_records(&self, kind: StorefrontContentKind) -> Vec<Value> {
        match kind {
            StorefrontContentKind::Blog => self.storefront_blog_records(),
            StorefrontContentKind::Page => self.storefront_page_records(),
            StorefrontContentKind::Article => self.storefront_article_records(),
        }
    }

    fn storefront_blog_records(&self) -> Vec<Value> {
        self.store
            .staged
            .online_store_blog_order
            .iter()
            .filter(|id| {
                !self
                    .store
                    .staged
                    .deleted_online_store_blog_ids
                    .contains(*id)
            })
            .filter_map(|id| self.store.staged.online_store_blogs.get(id))
            .map(storefront_blog_record_from_admin)
            .collect()
    }

    fn storefront_page_records(&self) -> Vec<Value> {
        self.store
            .staged
            .online_store_page_order
            .iter()
            .filter(|id| {
                !self
                    .store
                    .staged
                    .deleted_online_store_page_ids
                    .contains(*id)
            })
            .filter_map(|id| self.store.staged.online_store_pages.get(id))
            .filter(|page| storefront_content_is_visible(page))
            .map(storefront_page_record_from_admin)
            .collect()
    }

    fn storefront_article_records(&self) -> Vec<Value> {
        self.store
            .staged
            .online_store_article_order
            .iter()
            .filter(|id| {
                !self
                    .store
                    .staged
                    .deleted_online_store_article_ids
                    .contains(*id)
            })
            .filter_map(|id| self.store.staged.online_store_articles.get(id))
            .filter(|article| storefront_content_is_visible(article))
            .filter(|article| {
                article
                    .get("blogId")
                    .and_then(Value::as_str)
                    .and_then(|blog_id| {
                        self.storefront_content_by_id(StorefrontContentKind::Blog, blog_id)
                    })
                    .is_some()
            })
            .map(storefront_article_record_from_admin)
            .collect()
    }

    fn storefront_articles_for_blog(&self, blog_id: &str) -> Vec<Value> {
        self.storefront_article_records()
            .into_iter()
            .filter(|article| article.get("blogId").and_then(Value::as_str) == Some(blog_id))
            .collect()
    }

    fn storefront_menu_by_handle(&self, handle: &str) -> Option<Value> {
        self.store
            .base
            .storefront_menus
            .ordered_values()
            .into_iter()
            .find(|menu| menu.get("handle").and_then(Value::as_str) == Some(handle))
            .cloned()
    }

    fn storefront_sitemap_value(&self, arguments: &BTreeMap<String, Value>) -> Value {
        let sitemap_type = arguments
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default();
        json!({
            "_resources": self.storefront_sitemap_resources(sitemap_type),
        })
    }

    fn storefront_sitemap_resources(&self, sitemap_type: &str) -> Vec<Value> {
        let records = match sitemap_type {
            "ARTICLE" => self.storefront_article_records(),
            "BLOG" => self.storefront_blog_records(),
            "PAGE" => self.storefront_page_records(),
            _ => Vec::new(),
        };
        records
            .into_iter()
            .map(|record| {
                json!({
                    "__typename": "SitemapResource",
                    "handle": record.get("handle").cloned().unwrap_or(Value::Null),
                    "title": record.get("title").cloned().unwrap_or(Value::Null),
                    "updatedAt": record
                        .get("updatedAt")
                        .or_else(|| record.get("publishedAt"))
                        .cloned()
                        .unwrap_or(Value::Null),
                    "image": storefront_sitemap_image(record.get("image"))
                })
            })
            .collect()
    }

    fn storefront_metaobject_root_value(&self, arguments: &BTreeMap<String, Value>) -> Value {
        let record = if let Some(id) = arguments.get("id").and_then(Value::as_str) {
            self.metaobject_by_id(id)
        } else if let Some(handle) = arguments.get("handle").and_then(Value::as_object) {
            let meta_type = handle
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let meta_handle = handle
                .get("handle")
                .and_then(Value::as_str)
                .unwrap_or_default();
            self.metaobject_by_type_and_handle(meta_type, meta_handle)
        } else {
            None
        };
        record
            .and_then(|record| self.storefront_visible_metaobject(&record))
            .map(|record| self.storefront_metaobject_value(&record))
            .unwrap_or(Value::Null)
    }

    fn storefront_metaobjects_connection_value(
        &self,
        arguments: &BTreeMap<String, Value>,
    ) -> Value {
        let meta_type = arguments
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let records =
            self.store
                .staged
                .metaobjects
                .values()
                .filter(|record| {
                    record.get("type").and_then(Value::as_str) == Some(meta_type)
                        && !self.store.staged.metaobjects.is_tombstoned(
                            record.get("id").and_then(Value::as_str).unwrap_or_default(),
                        )
                })
                .filter_map(|record| self.storefront_visible_metaobject(record))
                .filter(|record| self.metaobject_visible_in_catalog(record))
                .collect::<Vec<_>>();
        let arguments = arguments
            .iter()
            .map(|(name, value)| (name.clone(), resolved_value_from_json(value)))
            .collect();
        let result = staged_connection_query(
            records,
            &arguments,
            |_record, _query| StagedSearchDecision::Match,
            storefront_metaobject_sort_key,
            metaobject_cursor,
        );
        connection_json_with_cursor(
            result
                .records
                .iter()
                .map(|record| self.storefront_metaobject_value(record))
                .collect(),
            |_, record| metaobject_cursor(record),
            result.page_info,
        )
    }

    fn storefront_visible_metaobject(&self, record: &Value) -> Option<Value> {
        let projected = self.project_metaobject_against_definition(record);
        let meta_type = projected.get("type").and_then(Value::as_str)?;
        let definition = self.metaobject_definition_by_type(meta_type)?;
        if definition
            .pointer("/access/storefront")
            .and_then(Value::as_str)
            != Some("PUBLIC_READ")
        {
            return None;
        }
        if definition
            .pointer("/capabilities/publishable/enabled")
            .and_then(Value::as_bool)
            .unwrap_or(false)
            && projected
                .pointer("/capabilities/publishable/status")
                .and_then(Value::as_str)
                != Some("ACTIVE")
        {
            return None;
        }
        Some(projected)
    }

    fn storefront_metaobject_value(&self, record: &Value) -> Value {
        let fields = storefront_metaobject_fields(record)
            .as_array()
            .into_iter()
            .flatten()
            .map(storefront_metaobject_field_value)
            .collect::<Vec<_>>();
        json!({
            "__typename": "Metaobject",
            "field": Value::Null,
            "fields": fields,
            "handle": record.get("handle").cloned().unwrap_or(Value::Null),
            "id": record.get("id").cloned().unwrap_or(Value::Null),
            "onlineStoreUrl": record.get("onlineStoreUrl").cloned().unwrap_or(Value::Null),
            "seo": record.get("seo").cloned().unwrap_or(Value::Null),
            "type": record.get("type").cloned().unwrap_or(Value::Null),
            "updatedAt": record.get("updatedAt").cloned().unwrap_or(Value::Null),
        })
    }

    fn storefront_owner_metafield(
        &self,
        owner_id: &str,
        namespace: &str,
        key: &str,
    ) -> Option<Value> {
        let keys = vec![(namespace.to_string(), key.to_string())];
        self.owner_metafields(owner_id, Some(namespace), Some(&keys))
            .into_iter()
            .find(|metafield| {
                metafield.get("namespace").and_then(Value::as_str) == Some(namespace)
                    && metafield.get("key").and_then(Value::as_str) == Some(key)
            })
            .filter(storefront_metafield_is_public)
    }

    fn storefront_reference_connection_value(
        &self,
        record: &Value,
        arguments: &BTreeMap<String, Value>,
    ) -> Value {
        let ids = list_reference_ids(record)
            .into_iter()
            .filter(|id| self.storefront_reference_node_value(id).is_some())
            .collect::<Vec<_>>();
        let arguments = arguments
            .iter()
            .map(|(name, value)| (name.clone(), resolved_value_from_json(value)))
            .collect();
        let (ids, page_info) = connection_window(&ids, &arguments, |id| id.clone());
        connection_json_with_cursor(
            ids.iter()
                .filter_map(|id| self.storefront_reference_node_value(id))
                .collect(),
            |index, _| ids.get(index).cloned().unwrap_or_default(),
            page_info,
        )
    }

    fn storefront_reference_node_value(&self, id: &str) -> Option<Value> {
        match shopify_gid_resource_type(id) {
            Some("Metaobject") => {
                let record = self.metaobject_by_id(id)?;
                let record = self.storefront_visible_metaobject(&record)?;
                Some(self.storefront_metaobject_value(&record))
            }
            Some("Shop") => {
                let shop = self.store.effective_shop();
                (shop.get("id").and_then(Value::as_str) == Some(id))
                    .then(|| self.storefront_shop_value())
            }
            _ => None,
        }
    }

    fn storefront_shop_owner_id(&self) -> Option<String> {
        self.store
            .effective_shop()
            .get("id")
            .and_then(Value::as_str)
            .map(str::to_string)
            .or_else(|| {
                self.store
                    .staged
                    .owner_metafields
                    .keys()
                    .find(|id| shopify_gid_resource_type(id.as_str()) == Some("Shop"))
                    .cloned()
            })
    }
}

pub(in crate::proxy) fn storefront_discovery_argument_error(
    field: &RootFieldSelection,
) -> Option<(String, Value)> {
    if matches!(field.name.as_str(), "node" | "nodes") {
        let ids = if field.name == "node" {
            resolved_string_field(&field.arguments, "id")
                .into_iter()
                .collect::<Vec<_>>()
        } else {
            list_string_field(&field.arguments, "ids")
        };
        if let Some(id) = ids
            .into_iter()
            .find(|id| shopify_gid_resource_type(id).is_none())
        {
            return Some((
                format!("Invalid global id '{id}'"),
                json!({ "code": "argumentLiteralsIncompatible", "typeName": "CoercionError" }),
            ));
        }
    }
    if field.name == "predictiveSearch"
        && resolved_int_field(&field.arguments, "limit")
            .is_some_and(|limit| !(1..=10).contains(&limit))
    {
        return Some((
            "limit must be within 1..10".to_string(),
            json!({ "code": "INVALID_FIELD_ARGUMENTS" }),
        ));
    }
    None
}

fn storefront_search_item_cursor(item: &StorefrontSearchItem) -> String {
    match item {
        StorefrontSearchItem::Product(product) => product.id.clone(),
        StorefrontSearchItem::Article(record) | StorefrontSearchItem::Page(record) => {
            value_id_cursor(record)
        }
    }
}

fn storefront_search_item_type_rank(item: &StorefrontSearchItem) -> u8 {
    match item {
        StorefrontSearchItem::Product(_) => 0,
        StorefrontSearchItem::Article(_) => 1,
        StorefrontSearchItem::Page(_) => 2,
    }
}

fn storefront_search_item_title(item: &StorefrontSearchItem) -> String {
    match item {
        StorefrontSearchItem::Product(product) => product.title.to_ascii_lowercase(),
        StorefrontSearchItem::Article(record) | StorefrontSearchItem::Page(record) => {
            storefront_value_title(record)
        }
    }
}

fn storefront_sort_search_items(
    proxy: &DraftProxy,
    items: &mut [StorefrontSearchItem],
    arguments: &BTreeMap<String, ResolvedValue>,
) {
    let sort_key =
        resolved_string_field(arguments, "sortKey").unwrap_or_else(|| "RELEVANCE".to_string());
    items.sort_by(|left, right| {
        let ordering = if sort_key == "PRICE" {
            storefront_search_item_price(left)
                .total_cmp(&storefront_search_item_price(right))
                .then_with(|| {
                    storefront_search_item_type_rank(left)
                        .cmp(&storefront_search_item_type_rank(right))
                })
        } else {
            storefront_search_item_type_rank(left)
                .cmp(&storefront_search_item_type_rank(right))
                .then_with(|| {
                    storefront_search_item_title(left).cmp(&storefront_search_item_title(right))
                })
        };
        ordering.then_with(|| {
            storefront_search_item_cursor(left).cmp(&storefront_search_item_cursor(right))
        })
    });
    if resolved_bool_field(arguments, "reverse").unwrap_or(false) {
        items.reverse();
    }
    if resolved_string_field(arguments, "unavailableProducts").as_deref() == Some("LAST") {
        items.sort_by_key(|item| match item {
            StorefrontSearchItem::Product(product) => {
                !storefront_search_product_available(proxy, product)
            }
            _ => false,
        });
    }
}

fn storefront_search_item_price(item: &StorefrontSearchItem) -> f64 {
    match item {
        StorefrontSearchItem::Product(product) => product
            .variants
            .iter()
            .filter_map(|variant| {
                variant
                    .get("price")
                    .and_then(Value::as_str)?
                    .parse::<f64>()
                    .ok()
            })
            .min_by(f64::total_cmp)
            .unwrap_or(0.0),
        _ => f64::INFINITY,
    }
}

fn storefront_value_title(record: &Value) -> String {
    record
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_ascii_lowercase()
}

fn storefront_discovery_query_terms(query: &str) -> Vec<String> {
    query
        .split_whitespace()
        .map(|term| {
            term.trim_matches(|character: char| !character.is_alphanumeric())
                .to_ascii_lowercase()
        })
        .filter(|term| !term.is_empty())
        .collect()
}

fn storefront_discovery_text_matches(
    texts: &[String],
    query: &str,
    prefix: &str,
    allow_infix: bool,
) -> bool {
    let terms = storefront_discovery_query_terms(query);
    if terms.is_empty() {
        return true;
    }
    let words = texts
        .iter()
        .flat_map(|text| {
            text.split(|character: char| !character.is_alphanumeric())
                .filter(|word| !word.is_empty())
                .map(str::to_ascii_lowercase)
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    terms.iter().enumerate().all(|(index, term)| {
        let is_last_prefix = prefix == "LAST" && index + 1 == terms.len();
        words.iter().any(|word| {
            if is_last_prefix {
                word.starts_with(term)
            } else {
                word == term || (allow_infix && word.contains(term))
            }
        })
    })
}

fn storefront_product_matches_discovery_query(
    proxy: &DraftProxy,
    product: &ProductRecord,
    query: &str,
    prefix: &str,
    searchable_fields: &[String],
) -> bool {
    let includes = |field: &str| {
        searchable_fields.is_empty() || searchable_fields.iter().any(|value| value == field)
    };
    let mut texts = Vec::new();
    if includes("TITLE") {
        texts.push(product.title.clone());
    }
    if includes("VENDOR") {
        texts.push(product.vendor.clone());
    }
    if includes("PRODUCT_TYPE") {
        texts.push(product.product_type.clone());
    }
    if includes("TAG") {
        texts.extend(product.tags.clone());
    }
    let variants = proxy.store.product_variants_for_product(&product.id);
    if includes("VARIANT_TITLE") {
        texts.extend(variants.iter().map(|variant| variant.title.clone()));
    }
    if includes("VARIANTS_SKU") {
        texts.extend(variants.iter().map(|variant| variant.sku.clone()));
    }
    if includes("VARIANTS_BARCODE") {
        texts.extend(
            variants
                .iter()
                .filter_map(|variant| variant.barcode.clone()),
        );
    }
    storefront_discovery_text_matches(&texts, query, prefix, true)
}

fn storefront_value_matches_discovery_query(
    record: &Value,
    query: &str,
    prefix: &str,
    searchable_fields: &[String],
) -> bool {
    let includes = |field: &str| {
        searchable_fields.is_empty() || searchable_fields.iter().any(|value| value == field)
    };
    let mut texts = Vec::new();
    if includes("TITLE") {
        if let Some(value) = record.get("title").and_then(Value::as_str) {
            texts.push(value.to_string());
        }
    }
    if includes("BODY") {
        for key in [
            "body",
            "bodySummary",
            "content",
            "contentHtml",
            "summary",
            "excerpt",
        ] {
            if let Some(value) = record.get(key).and_then(Value::as_str) {
                texts.push(value.to_string());
            }
        }
    }
    if includes("TAG") {
        texts.extend(
            record
                .get("tags")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(str::to_string),
        );
    }
    if includes("AUTHOR") {
        if let Some(value) = record.pointer("/author/name").and_then(Value::as_str) {
            texts.push(value.to_string());
        }
    }
    if searchable_fields.is_empty() {
        if let Some(value) = record.get("handle").and_then(Value::as_str) {
            texts.push(value.to_string());
        }
    }
    storefront_discovery_text_matches(&texts, query, prefix, false)
}

fn storefront_search_product_available(proxy: &DraftProxy, product: &ProductRecord) -> bool {
    let variants = proxy.store.product_variants_for_product(&product.id);
    storefront_product_available_for_sale(product, &variants)
}

fn storefront_product_matches_search_filters(
    proxy: &DraftProxy,
    product: &ProductRecord,
    filters: &[BTreeMap<String, ResolvedValue>],
) -> bool {
    filters.iter().all(|filter| {
        if let Some(available) = resolved_bool_field(filter, "available") {
            if storefront_search_product_available(proxy, product) != available {
                return false;
            }
        }
        if let Some(tag) = resolved_string_field(filter, "tag") {
            if !product
                .tags
                .iter()
                .any(|value| value.eq_ignore_ascii_case(&tag))
            {
                return false;
            }
        }
        if let Some(product_type) = resolved_string_field(filter, "productType") {
            if !product.product_type.eq_ignore_ascii_case(&product_type) {
                return false;
            }
        }
        if let Some(vendor) = resolved_string_field(filter, "productVendor") {
            if !product.vendor.eq_ignore_ascii_case(&vendor) {
                return false;
            }
        }
        let variants = proxy.store.product_variants_for_product(&product.id);
        if let Some(option) = resolved_object_field(filter, "variantOption") {
            let name = resolved_string_field(&option, "name").unwrap_or_default();
            let value = resolved_string_field(&option, "value").unwrap_or_default();
            if !variants.iter().any(|variant| {
                variant.selected_options.iter().any(|option| {
                    option.name.eq_ignore_ascii_case(&name)
                        && option.value.eq_ignore_ascii_case(&value)
                })
            }) {
                return false;
            }
        }
        if let Some(price) = resolved_object_field(filter, "price") {
            let min = price
                .get("min")
                .and_then(resolved_value_number)
                .unwrap_or(0.0);
            let max = price.get("max").and_then(resolved_value_number);
            if !variants.iter().any(|variant| {
                variant
                    .price
                    .parse::<f64>()
                    .ok()
                    .is_some_and(|amount| amount >= min && max.is_none_or(|max| amount <= max))
            }) {
                return false;
            }
        }
        true
    })
}

fn storefront_search_product_filters(
    proxy: &DraftProxy,
    items: &[StorefrontSearchItem],
) -> Vec<Value> {
    let products = items
        .iter()
        .filter_map(|item| match item {
            StorefrontSearchItem::Product(product) => Some(product.as_ref()),
            _ => None,
        })
        .collect::<Vec<_>>();
    if products.is_empty() {
        return vec![json!({
            "id": "filter.v.price", "label": "Price", "presentation": Value::Null, "type": "PRICE_RANGE",
            "values": [{ "id": "filter.v.price", "label": "Price", "count": 0, "input": "{\"price\":{\"min\":0,\"max\":0.0}}" }]
        })];
    }
    let available_count = products
        .iter()
        .filter(|product| storefront_search_product_available(proxy, product))
        .count();
    vec![json!({
        "id": "filter.v.availability", "label": "Availability", "presentation": "TEXT", "type": "LIST",
        "values": [
            { "id": "filter.v.availability.1", "label": "In stock", "count": available_count, "input": "{\"available\":true}" },
            { "id": "filter.v.availability.0", "label": "Out of stock", "count": products.len() - available_count, "input": "{\"available\":false}" }
        ]
    })]
}

fn truncate_with_remaining<T>(values: &mut Vec<T>, remaining: &mut usize) {
    values.truncate(*remaining);
    *remaining = remaining.saturating_sub(values.len());
}

fn storefront_query_suggestions(
    query: &str,
    limit: usize,
    products: &[ProductRecord],
    collections: &[Value],
    articles: &[Value],
    pages: &[Value],
) -> Vec<Value> {
    let normalized = query.trim().to_ascii_lowercase();
    if normalized.is_empty() {
        return Vec::new();
    }
    let mut candidates = BTreeSet::new();
    for title in products.iter().map(|record| record.title.as_str()).chain(
        collections
            .iter()
            .chain(articles)
            .chain(pages)
            .filter_map(|record| record.get("title").and_then(Value::as_str)),
    ) {
        for word in title.split(|character: char| !character.is_alphanumeric()) {
            let word = word.to_ascii_lowercase();
            if word.starts_with(&normalized) && word != normalized {
                candidates.insert(word);
            }
        }
    }
    for author in articles
        .iter()
        .filter_map(|record| record.pointer("/author/name").and_then(Value::as_str))
    {
        for word in author.split(|character: char| !character.is_alphanumeric()) {
            let word = word.to_ascii_lowercase();
            if word.starts_with(&normalized) && word != normalized {
                candidates.insert(word);
            }
        }
    }
    let session = format!("{:x}", Sha256::digest(normalized.as_bytes()));
    candidates.into_iter().take(limit).enumerate().map(|(index, text)| {
        let remainder = text.strip_prefix(&normalized).unwrap_or_default();
        json!({
            "text": text,
            "styledText": format!("<mark>{}</mark><span>{}</span>", normalized, remainder),
            "trackingParameters": format!("_pos={}&_psid={}&_psq={}&_ss=e&_v=1.0", index + 1, &session[..9], normalized)
        })
    }).collect()
}

fn storefront_blog_record_from_admin(record: &Value) -> Value {
    json!({
        "__typename": "Blog",
        "id": record.get("id").cloned().unwrap_or(Value::Null),
        "handle": record.get("handle").cloned().unwrap_or(Value::Null),
        "title": record.get("title").cloned().unwrap_or(Value::Null),
    })
}

fn storefront_page_record_from_admin(record: &Value) -> Value {
    let body = record
        .get("body")
        .and_then(Value::as_str)
        .unwrap_or_default();
    json!({
        "__typename": "Page",
        "id": record.get("id").cloned().unwrap_or(Value::Null),
        "handle": record.get("handle").cloned().unwrap_or(Value::Null),
        "title": record.get("title").cloned().unwrap_or(Value::Null),
        "body": body,
        "bodySummary": record
            .get("bodySummary")
            .cloned()
            .unwrap_or_else(|| json!(storefront_strip_html(body))),
        "createdAt": record.get("createdAt").cloned().unwrap_or(Value::Null),
        "updatedAt": record.get("updatedAt").cloned().unwrap_or(Value::Null),
    })
}

fn storefront_article_record_from_admin(record: &Value) -> Value {
    let body_html = record
        .get("body")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let excerpt_html = record.get("summary").cloned().unwrap_or(Value::Null);
    json!({
        "__typename": "Article",
        "id": record.get("id").cloned().unwrap_or(Value::Null),
        "blogId": record.get("blogId").cloned().unwrap_or(Value::Null),
        "handle": record.get("handle").cloned().unwrap_or(Value::Null),
        "title": record.get("title").cloned().unwrap_or(Value::Null),
        "content": storefront_strip_html(body_html),
        "contentHtml": body_html,
        "excerpt": excerpt_html
            .as_str()
            .map(storefront_strip_html)
            .map(Value::String)
            .unwrap_or(Value::Null),
        "excerptHtml": excerpt_html,
        "tags": record.get("tags").cloned().unwrap_or_else(|| json!([])),
        "publishedAt": record
            .get("publishedAt")
            .cloned()
            .or_else(|| record.get("createdAt").cloned())
            .unwrap_or(Value::Null),
        "author": storefront_article_author(record.get("author")),
        "image": storefront_article_image(record.get("image")),
    })
}

fn storefront_article_author(author: Option<&Value>) -> Value {
    let name = author
        .and_then(|author| author.get("name"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    json!({
        "bio": author.and_then(|value| value.get("bio")).cloned().unwrap_or(Value::Null),
        "email": author.and_then(|value| value.get("email")).cloned().unwrap_or(Value::Null),
        "firstName": author.and_then(|value| value.get("firstName")).cloned().unwrap_or(Value::Null),
        "lastName": author.and_then(|value| value.get("lastName")).cloned().unwrap_or(Value::Null),
        "name": name,
    })
}

fn storefront_article_image(image: Option<&Value>) -> Value {
    let Some(image) = image.filter(|image| image.is_object()) else {
        return Value::Null;
    };
    let url = image
        .get("url")
        .or_else(|| image.get("originalSrc"))
        .or_else(|| image.get("src"))
        .cloned()
        .unwrap_or(Value::Null);
    json!({
        "_url": url,
        "altText": image.get("altText").or_else(|| image.get("alt")).cloned().unwrap_or(Value::Null),
        "height": image.get("height").cloned().unwrap_or(Value::Null),
        "id": image.get("id").cloned().unwrap_or(Value::Null),
        "originalSrc": url,
        "src": url,
        "thumbhash": image.get("thumbhash").cloned().unwrap_or(Value::Null),
        "width": image.get("width").cloned().unwrap_or(Value::Null),
    })
}

fn storefront_sitemap_image(image: Option<&Value>) -> Value {
    let Some(image) = image.filter(|image| image.is_object()) else {
        return Value::Null;
    };
    json!({
        "alt": image.get("alt").or_else(|| image.get("altText")).cloned().unwrap_or(Value::Null),
        "filepath": image
            .get("filepath")
            .or_else(|| image.get("url"))
            .or_else(|| image.get("originalSrc"))
            .or_else(|| image.get("src"))
            .cloned()
            .unwrap_or(Value::Null),
        "updatedAt": image.get("updatedAt").cloned().unwrap_or(Value::Null),
    })
}

fn storefront_content_is_visible(record: &Value) -> bool {
    record
        .get("isPublished")
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

fn storefront_default_seo() -> Value {
    json!({
        "title": Value::Null,
        "description": Value::Null,
    })
}

fn storefront_collection_observed_products(collection: &Value) -> Vec<Value> {
    let mut product_order = Vec::new();
    let mut products = BTreeMap::<String, Value>::new();

    let mut observe_connection = |connection: &Value| {
        for mut product in connection_nodes(connection) {
            let Some(id) = product
                .get("id")
                .and_then(Value::as_str)
                .filter(|id| is_shopify_gid_of_type(id, "Product"))
                .map(str::to_string)
            else {
                continue;
            };
            product["__storefrontVisible"] = json!(true);
            if let Some(existing) = products.remove(&id) {
                products.insert(id, shallow_merged_object(existing, product));
            } else {
                product_order.push(id.clone());
                products.insert(id, product);
            }
        }
    };

    // Preserve the captured default connection prefix first. Other aliases
    // then fill fields and append members that fell outside that window.
    if let Some(default_products) = collection.get("products") {
        observe_connection(default_products);
    }
    if let Some(object) = collection.as_object() {
        for (response_key, value) in object {
            if response_key != "products" {
                observe_connection(value);
            }
        }
    }

    product_order
        .into_iter()
        .filter_map(|id| products.remove(&id))
        .collect()
}

fn storefront_collection_seo(collection: &Value) -> Value {
    collection.get("seo").cloned().unwrap_or_else(|| {
        json!({
            "title": collection.get("title").cloned().unwrap_or(Value::Null),
            "description": storefront_strip_html(
                collection
                    .get("descriptionHtml")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
            )
        })
    })
}

fn storefront_collection_product_matches_filters(
    entry: &CollectionProductEntry,
    filters: &[BTreeMap<String, ResolvedValue>],
) -> bool {
    if filters.is_empty() {
        return true;
    }
    filters.iter().any(|filter| {
        resolved_bool_field(filter, "available").is_none_or(|available| {
            storefront_product_available_for_sale(&entry.product, &entry.variants) == available
        }) && resolved_string_field(filter, "productType").is_none_or(|product_type| {
            entry
                .product
                .product_type
                .eq_ignore_ascii_case(&product_type)
        }) && resolved_string_field(filter, "productVendor")
            .is_none_or(|vendor| entry.product.vendor.eq_ignore_ascii_case(&vendor))
            && resolved_string_field(filter, "tag").is_none_or(|tag| {
                entry
                    .product
                    .tags
                    .iter()
                    .any(|candidate| candidate.eq_ignore_ascii_case(&tag))
            })
    })
}

fn storefront_strip_html(value: &str) -> String {
    let mut output = String::new();
    let mut in_tag = false;
    for character in value.chars() {
        match character {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => output.push(character),
            _ => {}
        }
    }
    output.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn storefront_content_search_decision(
    kind: StorefrontContentKind,
    record: &Value,
    query: Option<&str>,
) -> StagedSearchDecision {
    let Some(query) = query.map(str::trim).filter(|query| !query.is_empty()) else {
        return StagedSearchDecision::Match;
    };
    for token in storefront_query_tokens(query) {
        if token.eq_ignore_ascii_case("AND") {
            continue;
        }
        if !storefront_content_matches_token(kind, record, &token) {
            return StagedSearchDecision::NoMatch;
        }
    }
    StagedSearchDecision::Match
}

fn storefront_query_tokens(query: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut quote = None;
    for character in query.chars() {
        match quote {
            Some(active_quote) if character == active_quote => {
                quote = None;
                current.push(character);
            }
            Some(_) => current.push(character),
            None if matches!(character, '"' | '\'') => {
                quote = Some(character);
                current.push(character);
            }
            None if character.is_whitespace() => {
                storefront_push_query_token(&mut tokens, &mut current);
            }
            None => current.push(character),
        }
    }
    storefront_push_query_token(&mut tokens, &mut current);
    tokens
}

fn storefront_push_query_token(tokens: &mut Vec<String>, current: &mut String) {
    let token = current.trim();
    if !token.is_empty() {
        tokens.push(token.to_string());
    }
    current.clear();
}

fn storefront_content_matches_token(
    kind: StorefrontContentKind,
    record: &Value,
    token: &str,
) -> bool {
    let token = token
        .trim()
        .trim_matches(|character: char| matches!(character, '(' | ')' | ','))
        .trim_matches('"')
        .trim_matches('\'');
    let (field, value) = token
        .split_once(':')
        .map(|(field, value)| {
            (
                Some(field.trim().trim_start_matches('-').to_ascii_lowercase()),
                value
                    .trim()
                    .trim_matches('"')
                    .trim_matches('\'')
                    .to_string(),
            )
        })
        .unwrap_or_else(|| (None, token.to_string()));
    let value = value.trim();
    if value.is_empty() {
        return true;
    }
    match field.as_deref() {
        Some("id") => storefront_string_matches(record.get("id").and_then(Value::as_str), value),
        Some("handle") => {
            storefront_string_matches(record.get("handle").and_then(Value::as_str), value)
        }
        Some("title") => {
            storefront_string_matches(record.get("title").and_then(Value::as_str), value)
        }
        Some("author") if kind == StorefrontContentKind::Article => storefront_string_matches(
            record
                .get("author")
                .and_then(|author| author.get("name"))
                .and_then(Value::as_str),
            value,
        ),
        Some("tag") if kind == StorefrontContentKind::Article => record
            .get("tags")
            .and_then(Value::as_array)
            .map(|tags| {
                tags.iter()
                    .any(|tag| storefront_string_matches(tag.as_str(), value))
            })
            .unwrap_or(false),
        Some("tag_not") if kind == StorefrontContentKind::Article => record
            .get("tags")
            .and_then(Value::as_array)
            .map(|tags| {
                !tags
                    .iter()
                    .any(|tag| storefront_string_matches(tag.as_str(), value))
            })
            .unwrap_or(true),
        Some("blog_title") if kind == StorefrontContentKind::Article => false,
        Some("created_at" | "updated_at") => true,
        Some(_) => false,
        None => storefront_content_free_text_matches(kind, record, value),
    }
}

fn storefront_content_free_text_matches(
    kind: StorefrontContentKind,
    record: &Value,
    value: &str,
) -> bool {
    let fields = match kind {
        StorefrontContentKind::Blog => vec!["title", "handle"],
        StorefrontContentKind::Page => vec!["title", "handle", "body", "bodySummary"],
        StorefrontContentKind::Article => vec!["title", "handle", "content", "excerpt"],
    };
    fields
        .iter()
        .any(|field| storefront_string_matches(record.get(*field).and_then(Value::as_str), value))
}

fn storefront_string_matches(actual: Option<&str>, expected: &str) -> bool {
    let expected = expected.trim().to_ascii_lowercase();
    if expected.is_empty() {
        return true;
    }
    let actual = actual.unwrap_or_default().to_ascii_lowercase();
    if let Some(prefix) = expected.strip_suffix('*') {
        return actual
            .split(|character: char| !character.is_ascii_alphanumeric())
            .any(|part| part.starts_with(prefix));
    }
    actual.contains(&expected)
}

fn storefront_content_sort_key(
    kind: StorefrontContentKind,
    record: &Value,
    sort_key: Option<&str>,
) -> StagedSortKey {
    let normalized = sort_key.unwrap_or("ID").to_ascii_uppercase();
    let primary = match normalized.as_str() {
        "TITLE" => storefront_record_sort_string(record, "title"),
        "HANDLE" => storefront_record_sort_string(record, "handle"),
        "AUTHOR" if kind == StorefrontContentKind::Article => record
            .get("author")
            .and_then(|author| author.get("name"))
            .and_then(Value::as_str)
            .map(|value| StagedSortValue::String(value.to_ascii_lowercase()))
            .unwrap_or(StagedSortValue::Null),
        "PUBLISHED_AT" if kind == StorefrontContentKind::Article => {
            storefront_record_sort_string(record, "publishedAt")
        }
        "UPDATED_AT" => storefront_record_sort_string(record, "updatedAt"),
        _ => storefront_record_gid_tail_sort_value(record),
    };
    vec![primary, storefront_record_gid_tail_sort_value(record)]
}

fn storefront_record_sort_string(record: &Value, field: &str) -> StagedSortValue {
    record
        .get(field)
        .and_then(Value::as_str)
        .map(|value| StagedSortValue::String(value.to_ascii_lowercase()))
        .unwrap_or(StagedSortValue::Null)
}

fn storefront_record_gid_tail_sort_value(record: &Value) -> StagedSortValue {
    let id = record.get("id").and_then(Value::as_str).unwrap_or_default();
    let tail = resource_id_tail(id);
    tail.parse::<i64>()
        .map(StagedSortValue::I64)
        .unwrap_or_else(|_| StagedSortValue::String(tail.to_ascii_lowercase()))
}

fn storefront_product_value(
    proxy: &DraftProxy,
    product: &ProductRecord,
    variants: &[ProductVariantRecord],
    context: &StorefrontRequestContext,
) -> Value {
    let currency_code = storefront_product_currency_code(proxy, variants, context);
    let description = product
        .extra_fields
        .get("description")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| strip_html_tags(&product.description_html));
    let featured_image = product
        .media
        .iter()
        .find_map(storefront_product_image_value_from_media)
        .unwrap_or(Value::Null);
    json!({
        "__typename": "Product",
        "availableForSale": storefront_product_available_for_sale(product, variants),
        "compareAtPriceRange": storefront_product_price_range_value(
            proxy,
            product,
            variants,
            context,
            &currency_code,
            StorefrontPriceRangeKind::CompareAtPrice,
        ),
        "createdAt": product.created_at,
        "description": description,
        "descriptionHtml": product.description_html,
        "featuredImage": featured_image,
        "handle": product.handle,
        "id": product.id,
        "isGiftCard": product
            .extra_fields
            .get("isGiftCard")
            .cloned()
            .unwrap_or(Value::Bool(false)),
        "onlineStoreUrl": product
            .extra_fields
            .get("onlineStoreUrl")
            .cloned()
            .unwrap_or(Value::Null),
        "priceRange": storefront_product_price_range_value(
            proxy,
            product,
            variants,
            context,
            &currency_code,
            StorefrontPriceRangeKind::Price,
        ),
        "productType": product.product_type,
        "publishedAt": storefront_product_published_at(product),
        "requiresSellingPlan": product
            .extra_fields
            .get("requiresSellingPlan")
            .cloned()
            .unwrap_or(Value::Bool(false)),
        "seo": {
            "title": product.seo_title,
            "description": product.seo_description,
        },
        "tags": product.tags,
        "title": product.title,
        "totalInventory": storefront_product_total_inventory(product, variants),
        "updatedAt": product.updated_at,
        "variantsCount": count_object(storefront_product_variant_count(product, variants)),
        "vendor": product.vendor,
    })
}

fn storefront_product_options_value(
    product: &ProductRecord,
    variants: &[ProductVariantRecord],
    first: Option<i64>,
) -> Value {
    let mut options = product
        .extra_fields
        .get("options")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_else(|| storefront_options_from_variants(product, variants));
    if let Some(first) = first.filter(|first| *first >= 0) {
        options.truncate(first as usize);
    }
    Value::Array(
        options
            .iter()
            .map(storefront_product_option_value)
            .collect(),
    )
}

fn storefront_product_option_value(option: &Value) -> Value {
    let option_values = option
        .get("optionValues")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_else(|| {
            option
                .get("values")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .map(|value| {
                    json!({
                        "firstSelectableVariant": Value::Null,
                        "id": Value::Null,
                        "name": value,
                        "swatch": Value::Null,
                    })
                })
                .collect()
        });
    let values = option.get("values").cloned().unwrap_or_else(|| {
        Value::Array(
            option_values
                .iter()
                .filter_map(|value| value.get("name").cloned())
                .collect(),
        )
    });
    json!({
        "__typename": "ProductOption",
        "id": option.get("id").cloned().unwrap_or(Value::Null),
        "name": option.get("name").cloned().unwrap_or(Value::Null),
        "optionValues": option_values,
        "values": values,
    })
}

fn storefront_selling_plan_group_value(
    group: &SellingPlanGroupRecord,
    currency_code: &str,
) -> Value {
    let options = group
        .options
        .iter()
        .enumerate()
        .map(|(index, name)| {
            let mut values = group
                .selling_plans
                .iter()
                .filter_map(|plan| plan.options.get(index).cloned())
                .collect::<Vec<_>>();
            values.dedup();
            json!({ "name": name, "values": values })
        })
        .collect::<Vec<_>>();
    json!({
        "__typename": "SellingPlanGroup",
        "_sellingPlanGroupId": group.id,
        "_currencyCode": currency_code,
        "appName": Value::Null,
        "name": group.name,
        "options": options,
    })
}

fn storefront_selling_plan_value(
    plan: &SellingPlanRecord,
    option_names: &[String],
    currency_code: &str,
) -> Value {
    let recurring_deliveries = plan
        .delivery_policy
        .get("__typename")
        .and_then(Value::as_str)
        == Some("SellingPlanRecurringDeliveryPolicy")
        || plan.delivery_policy.get("recurring").is_some();
    let options = plan
        .options
        .iter()
        .enumerate()
        .map(|(index, value)| {
            json!({
                "name": option_names.get(index).cloned().unwrap_or_default(),
                "value": value,
            })
        })
        .collect::<Vec<_>>();
    let price_adjustments = plan
        .pricing_policies
        .iter()
        .map(|policy| {
            json!({
                "orderCount": policy
                    .get("afterCycle")
                    .and_then(Value::as_i64)
                    .map(|after_cycle| json!(after_cycle + 1))
                    .unwrap_or(Value::Null),
                "adjustmentValue": storefront_selling_plan_adjustment_value(
                    policy,
                    currency_code,
                ),
            })
        })
        .collect::<Vec<_>>();
    json!({
        "__typename": "SellingPlan",
        "billingPolicy": plan.billing_policy,
        "checkoutCharge": plan
            .billing_policy
            .get("checkoutCharge")
            .cloned()
            .unwrap_or(Value::Null),
        "deliveryPolicy": plan.delivery_policy,
        "description": plan.description,
        "id": plan.id,
        "name": plan.name,
        "options": options,
        "priceAdjustments": price_adjustments,
        "recurringDeliveries": recurring_deliveries,
    })
}

fn storefront_selling_plan_adjustment_value(policy: &Value, currency_code: &str) -> Value {
    match policy
        .get("adjustmentType")
        .and_then(Value::as_str)
        .unwrap_or("PERCENTAGE")
    {
        "FIXED_AMOUNT" => json!({
            "__typename": "SellingPlanFixedAmountPriceAdjustment",
            "adjustmentAmount": storefront_money_value(
                policy
                    .pointer("/adjustmentValue/amount")
                    .and_then(Value::as_str)
                    .unwrap_or("0"),
                currency_code,
            ),
        }),
        "PRICE" => json!({
            "__typename": "SellingPlanFixedPriceAdjustment",
            "price": storefront_money_value(
                policy
                    .pointer("/adjustmentValue/amount")
                    .and_then(Value::as_str)
                    .unwrap_or("0"),
                currency_code,
            ),
        }),
        _ => json!({
            "__typename": "SellingPlanPercentagePriceAdjustment",
            "adjustmentPercentage": policy
                .pointer("/adjustmentValue/percentage")
                .cloned()
                .unwrap_or_else(|| json!(0)),
        }),
    }
}

fn storefront_selling_plan_allocation_value(
    plan: &SellingPlanRecord,
    option_names: &[String],
    pricing: &StorefrontVariantPricing,
) -> Value {
    let original = storefront_parse_price(&pricing.price).unwrap_or_default();
    let adjusted = plan
        .pricing_policies
        .first()
        .map(|policy| storefront_adjusted_selling_plan_price(original, policy))
        .unwrap_or(original);
    let adjusted_amount = format_money_amount(adjusted);
    json!({
        "checkoutChargeAmount": storefront_money_value(
            &adjusted_amount,
            &pricing.currency_code,
        ),
        "remainingBalanceChargeAmount": storefront_money_value("0", &pricing.currency_code),
        "priceAdjustments": [{
            "price": storefront_money_value(&adjusted_amount, &pricing.currency_code),
            "perDeliveryPrice": storefront_money_value(&adjusted_amount, &pricing.currency_code),
            "compareAtPrice": storefront_money_value(&pricing.price, &pricing.currency_code),
            "unitPrice": Value::Null,
        }],
        "sellingPlan": storefront_selling_plan_value(
            plan,
            option_names,
            &pricing.currency_code,
        ),
    })
}

fn storefront_adjusted_selling_plan_price(price: f64, policy: &Value) -> f64 {
    let percentage = policy
        .pointer("/adjustmentValue/percentage")
        .and_then(Value::as_f64)
        .or_else(|| {
            policy
                .pointer("/adjustmentValue/percentage")
                .and_then(Value::as_i64)
                .map(|value| value as f64)
        })
        .unwrap_or_default();
    match policy
        .get("adjustmentType")
        .and_then(Value::as_str)
        .unwrap_or("PERCENTAGE")
    {
        "FIXED_AMOUNT" => {
            let amount = policy
                .pointer("/adjustmentValue/amount")
                .and_then(Value::as_str)
                .and_then(storefront_parse_price)
                .unwrap_or_default();
            (price - amount).max(0.0)
        }
        "PRICE" => policy
            .pointer("/adjustmentValue/amount")
            .and_then(Value::as_str)
            .and_then(storefront_parse_price)
            .unwrap_or(price),
        _ => price * (1.0 - percentage / 100.0),
    }
}

fn storefront_product_variants_connection_value(
    proxy: &DraftProxy,
    product: &ProductRecord,
    variants: &[ProductVariantRecord],
    context: &StorefrontRequestContext,
    arguments: &BTreeMap<String, Value>,
) -> Value {
    let arguments = arguments
        .iter()
        .map(|(name, value)| (name.clone(), resolved_value_from_json(value)))
        .collect::<BTreeMap<_, _>>();
    if variants.is_empty() {
        let raw_variants = sorted_storefront_raw_variants(product.variants.clone(), &arguments);
        let (raw_variants, page_info) =
            connection_window(&raw_variants, &arguments, value_id_cursor);
        let nodes = raw_variants
            .into_iter()
            .map(|mut variant| {
                if let Some(object) = variant.as_object_mut() {
                    object
                        .entry("__typename".to_string())
                        .or_insert_with(|| json!("ProductVariant"));
                    object.insert("_productId".to_string(), json!(product.id));
                }
                variant
            })
            .collect();
        return connection_json_with_cursor(nodes, |_, node| value_id_cursor(node), page_info);
    }
    let variants = sorted_storefront_variants(variants.to_vec(), &arguments);
    let (variants, page_info) =
        connection_window(&variants, &arguments, |variant| variant.id.clone());
    typed_connection_value(
        &variants,
        |variant| storefront_product_variant_value(proxy, variant, Some(product), context, None),
        |variant| variant.id.clone(),
        page_info,
    )
}

pub(in crate::proxy) fn storefront_product_variant_value(
    proxy: &DraftProxy,
    variant: &ProductVariantRecord,
    product: Option<&ProductRecord>,
    context: &StorefrontRequestContext,
    currency_code_override: Option<&str>,
) -> Value {
    let mut pricing = proxy.storefront_variant_pricing(variant, context);
    if let Some(currency_code) = currency_code_override {
        pricing.currency_code = currency_code.to_string();
    }
    let image = product
        .map(|product| storefront_variant_image_value(variant, product))
        .unwrap_or(Value::Null);
    let empty_connection = || connection_json(Vec::new());
    json!({
        "__typename": "ProductVariant",
        "_productId": variant.product_id,
        "availableForSale": storefront_variant_available_for_sale(variant),
        "barcode": variant.barcode,
        "compareAtPrice": pricing.compare_at_price.as_deref().map(|price| storefront_money_value(price, &pricing.currency_code)),
        "compareAtPriceV2": pricing.compare_at_price.as_deref().map(|price| storefront_money_value(price, &pricing.currency_code)),
        "components": empty_connection(),
        "currentlyNotInStock": variant.inventory_item.tracked
            && variant.inventory_quantity <= 0
            && variant.inventory_policy == "CONTINUE",
        "groupedBy": empty_connection(),
        "id": variant.id,
        "image": image,
        "price": storefront_money_value(&pricing.price, &pricing.currency_code),
        "priceV2": storefront_money_value(&pricing.price, &pricing.currency_code),
        "quantityAvailable": if variant.inventory_item.tracked {
            json!(variant.inventory_quantity.max(0))
        } else {
            Value::Null
        },
        "quantityPriceBreaks": empty_connection(),
        "quantityRule": { "increment": 1, "maximum": Value::Null, "minimum": 1 },
        "requiresComponents": false,
        "requiresShipping": variant.inventory_item.requires_shipping,
        "selectedOptions": variant.selected_options.iter().map(|option| json!({
            "name": option.name,
            "value": option.value,
        })).collect::<Vec<_>>(),
        "sellingPlanAllocations": empty_connection(),
        "shopPayInstallmentsPricing": Value::Null,
        "sku": if variant.sku.is_empty() { Value::Null } else { json!(variant.sku) },
        "storeAvailability": empty_connection(),
        "taxable": variant.taxable,
        "title": variant.title,
        "unitPrice": Value::Null,
        "unitPriceMeasurement": Value::Null,
        "weight": variant.extra_fields.get("weight").cloned().unwrap_or(Value::Null),
        "weightUnit": variant.extra_fields.get("weightUnit").cloned().unwrap_or_else(|| json!("KILOGRAMS")),
    })
}

fn storefront_variant_image_value(
    variant: &ProductVariantRecord,
    product: &ProductRecord,
) -> Value {
    variant
        .media_ids
        .iter()
        .find_map(|media_id| {
            product
                .media
                .iter()
                .find(|media| media.get("id").and_then(Value::as_str) == Some(media_id))
                .and_then(storefront_product_image_value_from_media)
        })
        // Shopify's Storefront ProductVariant.image falls back to the
        // product image when the variant has no explicitly attached image.
        .or_else(|| {
            product
                .media
                .iter()
                .find_map(storefront_product_image_value_from_media)
        })
        .unwrap_or(Value::Null)
}

fn storefront_variant_matching_json_options<'a>(
    variants: &'a [ProductVariantRecord],
    arguments: &BTreeMap<String, Value>,
) -> Option<&'a ProductVariantRecord> {
    let selected = arguments
        .get("selectedOptions")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|option| {
            Some((
                option.get("name")?.as_str()?.to_string(),
                option.get("value")?.as_str()?.to_string(),
            ))
        })
        .collect::<Vec<_>>();
    if selected.is_empty() {
        return None;
    }
    let case_insensitive = arguments
        .get("caseInsensitiveMatch")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    variants.iter().find(|variant| {
        selected.iter().all(|(name, value)| {
            variant.selected_options.iter().any(|option| {
                if case_insensitive {
                    option.name.eq_ignore_ascii_case(name)
                        && option.value.eq_ignore_ascii_case(value)
                } else {
                    option.name == *name && option.value == *value
                }
            })
        })
    })
}

fn storefront_product_price_range_value(
    proxy: &DraftProxy,
    product: &ProductRecord,
    variants: &[ProductVariantRecord],
    context: &StorefrontRequestContext,
    currency_code: &str,
    kind: StorefrontPriceRangeKind,
) -> Value {
    let observed_field = match kind {
        StorefrontPriceRangeKind::Price => "priceRange",
        StorefrontPriceRangeKind::CompareAtPrice => "compareAtPriceRange",
    };
    if variants.is_empty() && product.variants.is_empty() {
        if let Some(observed) = product.extra_fields.get(observed_field) {
            return observed.clone();
        }
    }
    let prices = match kind {
        StorefrontPriceRangeKind::Price => {
            storefront_product_variant_prices(proxy, product, variants, context)
        }
        StorefrontPriceRangeKind::CompareAtPrice => {
            storefront_product_variant_compare_at_prices(proxy, product, variants, context)
        }
    };
    let (min_price, max_price) = storefront_price_bounds(prices).unwrap_or((0.0, 0.0));
    json!({
        "__typename": "ProductPriceRange",
        "minVariantPrice": storefront_money_value(&format!("{min_price:.2}"), currency_code),
        "maxVariantPrice": storefront_money_value(&format!("{max_price:.2}"), currency_code),
    })
}

pub(in crate::proxy) fn storefront_money_value(price: &str, currency_code: &str) -> Value {
    json!({
        "__typename": "MoneyV2",
        "amount": normalize_money_amount(price),
        "currencyCode": currency_code,
    })
}

fn storefront_product_images_connection_value(
    product: &ProductRecord,
    arguments: &BTreeMap<String, Value>,
) -> Value {
    let arguments = arguments
        .iter()
        .map(|(name, value)| (name.clone(), resolved_value_from_json(value)))
        .collect();
    connection_value_with_args(
        product
            .media
            .iter()
            .filter_map(storefront_product_image_value_from_media)
            .collect(),
        &arguments,
        value_id_cursor,
    )
}

fn storefront_product_media_connection_value(
    product: &ProductRecord,
    arguments: &BTreeMap<String, Value>,
) -> Value {
    let arguments = arguments
        .iter()
        .map(|(name, value)| (name.clone(), resolved_value_from_json(value)))
        .collect();
    connection_value_with_args(
        product.media.iter().map(storefront_media_value).collect(),
        &arguments,
        value_id_cursor,
    )
}

fn storefront_product_image_value_from_media(media: &Value) -> Option<Value> {
    let mut image = product_image_json_from_media(media).or_else(|| {
        let source = media
            .pointer("/originalSource/url")
            .and_then(Value::as_str)
            .or_else(|| media.get("originalSource").and_then(Value::as_str))?;
        let media_id = media.get("id").and_then(Value::as_str)?;
        Some(json!({
            "id": shopify_gid("ProductImage", resource_id_tail(media_id)),
            "url": source,
            "altText": media.get("alt").cloned().unwrap_or(Value::Null),
            "width": Value::Null,
            "height": Value::Null,
        }))
    })?;
    if image.get("width").is_none_or(Value::is_null)
        || image.get("height").is_none_or(Value::is_null)
    {
        if let Some(source) = image.get("url").and_then(Value::as_str) {
            if let Some((width, height)) = storefront_image_dimensions_from_url(source) {
                image["width"] = json!(width);
                image["height"] = json!(height);
            }
        }
    }
    Some(image)
}

fn storefront_media_image_value(media: &Value) -> Value {
    let Some(mut image) = storefront_product_image_value_from_media(media) else {
        return Value::Null;
    };
    if let Some(media_id) = media.get("id").and_then(Value::as_str) {
        image["id"] = json!(shopify_gid("ImageSource", resource_id_tail(media_id)));
    }
    image
}

fn storefront_image_dimensions_from_url(url: &str) -> Option<(i64, i64)> {
    url.split(['/', '?', '&']).find_map(|part| {
        let (width, height) = part.split_once('x')?;
        let width = width.parse::<i64>().ok()?;
        let height = height.parse::<i64>().ok()?;
        (width > 0 && height > 0).then_some((width, height))
    })
}

fn storefront_media_value(media: &Value) -> Value {
    let media_content_type = media
        .get("mediaContentType")
        .and_then(Value::as_str)
        .unwrap_or("IMAGE");
    let type_name = match media_content_type {
        "EXTERNAL_VIDEO" => "ExternalVideo",
        "MODEL_3D" => "Model3d",
        "VIDEO" => "Video",
        _ => "MediaImage",
    };
    let preview_image = storefront_media_image_value(media);
    let mut value = json!({
        "__typename": type_name,
        "alt": media
            .get("alt")
            .or_else(|| media.get("altText"))
            .cloned()
            .unwrap_or(Value::Null),
        "id": media.get("id").cloned().unwrap_or(Value::Null),
        "mediaContentType": media_content_type,
        "presentation": media.get("presentation").cloned().unwrap_or(Value::Null),
        "previewImage": preview_image,
    });
    if let Some(object) = value.as_object_mut() {
        match type_name {
            "MediaImage" => {
                object.insert("image".to_string(), storefront_media_image_value(media));
            }
            "ExternalVideo" => {
                for field in ["embedUrl", "embeddedUrl", "host", "originUrl"] {
                    object.insert(
                        field.to_string(),
                        media.get(field).cloned().unwrap_or(Value::Null),
                    );
                }
            }
            "Video" | "Model3d" => {
                object.insert(
                    "sources".to_string(),
                    media.get("sources").cloned().unwrap_or_else(|| json!([])),
                );
            }
            _ => {}
        }
    }
    value
}

fn strip_html_tags(value: &str) -> String {
    let mut text = String::new();
    let mut in_tag = false;
    for ch in value.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => text.push(ch),
            _ => {}
        }
    }
    text
}

fn storefront_product_available_for_sale(
    product: &ProductRecord,
    variants: &[ProductVariantRecord],
) -> bool {
    if !variants.is_empty() {
        return variants.iter().any(storefront_variant_available_for_sale);
    }
    if !product.variants.is_empty() {
        return product
            .variants
            .iter()
            .any(storefront_raw_variant_available);
    }
    if let Some(observed) = product
        .extra_fields
        .get("availableForSale")
        .and_then(Value::as_bool)
    {
        return observed;
    }
    !product.tracks_inventory || product.total_inventory > 0
}

fn storefront_product_total_inventory(
    product: &ProductRecord,
    variants: &[ProductVariantRecord],
) -> i64 {
    if variants.is_empty() {
        return product.total_inventory;
    }
    variants
        .iter()
        .filter(|variant| variant.inventory_item.tracked)
        .map(|variant| variant.inventory_quantity)
        .sum()
}

fn storefront_variant_available_for_sale(variant: &ProductVariantRecord) -> bool {
    !variant.inventory_item.tracked
        || variant.inventory_quantity > 0
        || variant.inventory_policy == "CONTINUE"
}

fn storefront_raw_variant_available(variant: &Value) -> bool {
    variant
        .get("availableForSale")
        .and_then(Value::as_bool)
        .or_else(|| variant.get("available").and_then(Value::as_bool))
        .unwrap_or_else(|| {
            let tracked = variant
                .get("inventoryItem")
                .and_then(|item| item.get("tracked"))
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let quantity = variant
                .get("inventoryQuantity")
                .or_else(|| variant.get("quantityAvailable"))
                .and_then(Value::as_i64)
                .unwrap_or(0);
            let policy = variant
                .get("inventoryPolicy")
                .and_then(Value::as_str)
                .unwrap_or_default();
            !tracked || quantity > 0 || policy == "CONTINUE"
        })
}

fn storefront_product_published_at(product: &ProductRecord) -> Value {
    product
        .extra_fields
        .get("publishedAt")
        .cloned()
        .or_else(|| {
            product_publication_entries(product)
                .into_iter()
                .filter_map(|entry| entry.published_at.or(entry.publish_date))
                .min()
                .map(Value::String)
        })
        .unwrap_or(Value::Null)
}

fn storefront_product_variant_count(
    product: &ProductRecord,
    variants: &[ProductVariantRecord],
) -> usize {
    if !variants.is_empty() {
        variants.len()
    } else {
        product.variants.len()
    }
}

fn storefront_options_from_variants(
    product: &ProductRecord,
    variants: &[ProductVariantRecord],
) -> Vec<Value> {
    let mut values_by_name = BTreeMap::<String, Vec<String>>::new();
    for variant in variants {
        for option in &variant.selected_options {
            let values = values_by_name.entry(option.name.clone()).or_default();
            if !values.contains(&option.value) {
                values.push(option.value.clone());
            }
        }
    }
    for variant in &product.variants {
        if let Some(options) = variant.get("selectedOptions").and_then(Value::as_array) {
            for option in options {
                let Some(name) = option.get("name").and_then(Value::as_str) else {
                    continue;
                };
                let Some(value) = option.get("value").and_then(Value::as_str) else {
                    continue;
                };
                let values = values_by_name.entry(name.to_string()).or_default();
                if !values.iter().any(|existing| existing == value) {
                    values.push(value.to_string());
                }
            }
        }
    }
    values_by_name
        .into_iter()
        .enumerate()
        .map(|(index, (name, values))| {
            json!({
                "id": format!("{}/options/{}", product.id, index + 1),
                "name": name,
                "values": values,
                "optionValues": values
                    .iter()
                    .map(|value| json!({ "id": Value::Null, "name": value, "swatch": Value::Null }))
                    .collect::<Vec<_>>()
            })
        })
        .collect()
}

fn sorted_storefront_variants(
    variants: Vec<ProductVariantRecord>,
    arguments: &BTreeMap<String, ResolvedValue>,
) -> Vec<ProductVariantRecord> {
    let sort_key_name = resolved_string_field(arguments, "sortKey");
    sorted_indexed_records(
        variants,
        resolved_bool_field(arguments, "reverse").unwrap_or(false),
        |variant, index| storefront_variant_sort_key(variant, sort_key_name.as_deref(), index),
        |variant| variant.id.clone(),
    )
}

fn sorted_storefront_raw_variants(
    variants: Vec<Value>,
    arguments: &BTreeMap<String, ResolvedValue>,
) -> Vec<Value> {
    let sort_key_name = resolved_string_field(arguments, "sortKey");
    sorted_indexed_records(
        variants,
        resolved_bool_field(arguments, "reverse").unwrap_or(false),
        |variant, index| storefront_raw_variant_sort_key(variant, sort_key_name.as_deref(), index),
        value_id_cursor,
    )
}

fn storefront_variant_sort_key(
    variant: &ProductVariantRecord,
    sort_key: Option<&str>,
    index: usize,
) -> StagedSortKey {
    match sort_key {
        Some("ID") => storefront_gid_sort_key(&variant.id),
        Some("SKU") => vec![storefront_sort_string(&variant.sku)],
        Some("TITLE") => vec![storefront_sort_string(&variant.title)],
        Some("POSITION") | Some("RELEVANCE") | None => vec![StagedSortValue::I64(
            product_variant_position(variant).unwrap_or(index as i64),
        )],
        _ => vec![StagedSortValue::I64(index as i64)],
    }
}

fn storefront_raw_variant_sort_key(
    variant: &Value,
    sort_key: Option<&str>,
    index: usize,
) -> StagedSortKey {
    match sort_key {
        Some("ID") => variant
            .get("id")
            .and_then(Value::as_str)
            .map(storefront_gid_sort_key)
            .unwrap_or_else(|| vec![StagedSortValue::Null]),
        Some("SKU") => vec![storefront_sort_string(
            variant
                .get("sku")
                .and_then(Value::as_str)
                .unwrap_or_default(),
        )],
        Some("TITLE") => vec![storefront_sort_string(
            variant
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or_default(),
        )],
        Some("POSITION") | Some("RELEVANCE") | None => {
            let position = variant
                .get("position")
                .and_then(Value::as_i64)
                .unwrap_or(index as i64);
            vec![StagedSortValue::I64(position)]
        }
        _ => vec![StagedSortValue::I64(index as i64)],
    }
}

#[derive(Clone, Copy)]
enum StorefrontPriceRangeKind {
    Price,
    CompareAtPrice,
}

fn storefront_product_currency_code(
    proxy: &DraftProxy,
    variants: &[ProductVariantRecord],
    context: &StorefrontRequestContext,
) -> String {
    variants
        .first()
        .map(|variant| {
            proxy
                .storefront_variant_pricing(variant, context)
                .currency_code
        })
        .filter(|currency| !currency.is_empty())
        .or_else(|| {
            proxy
                .storefront_context_localization(context)
                .and_then(|localization| localization.pointer("/country/currency/isoCode"))
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .or_else(|| proxy.store.observed_shop_currency_code())
        .unwrap_or_else(|| "USD".to_string())
}

fn storefront_product_variant_prices(
    proxy: &DraftProxy,
    product: &ProductRecord,
    variants: &[ProductVariantRecord],
    context: &StorefrontRequestContext,
) -> Vec<f64> {
    if !variants.is_empty() {
        return variants
            .iter()
            .filter_map(|variant| {
                storefront_parse_price(&proxy.storefront_variant_pricing(variant, context).price)
            })
            .collect();
    }
    product
        .variants
        .iter()
        .filter_map(|variant| variant.get("price").and_then(Value::as_str))
        .filter_map(storefront_parse_price)
        .collect()
}

fn storefront_product_variant_compare_at_prices(
    proxy: &DraftProxy,
    product: &ProductRecord,
    variants: &[ProductVariantRecord],
    context: &StorefrontRequestContext,
) -> Vec<f64> {
    if !variants.is_empty() {
        return variants
            .iter()
            .filter_map(|variant| {
                proxy
                    .storefront_variant_pricing(variant, context)
                    .compare_at_price
            })
            .filter_map(|price| storefront_parse_price(&price))
            .collect();
    }
    product
        .variants
        .iter()
        .filter_map(|variant| variant.get("compareAtPrice").and_then(Value::as_str))
        .filter_map(storefront_parse_price)
        .collect()
}

fn storefront_price_bounds(prices: Vec<f64>) -> Option<(f64, f64)> {
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

fn storefront_parse_price(price: &str) -> Option<f64> {
    price.trim().parse::<f64>().ok()
}

fn storefront_product_sort_key(
    product: &ProductRecord,
    variants: &[ProductVariantRecord],
    sort_key: Option<&str>,
) -> StagedSortKey {
    let primary = match sort_key {
        Some("TITLE") => storefront_sort_string(&product.title),
        Some("PRODUCT_TYPE") => storefront_sort_string(&product.product_type),
        Some("VENDOR") => storefront_sort_string(&product.vendor),
        Some("UPDATED_AT") => StagedSortValue::String(product.updated_at.clone()),
        None | Some("CREATED_AT") | Some("BEST_SELLING") | Some("RELEVANCE") => {
            StagedSortValue::String(product.created_at.clone())
        }
        Some("PRICE") => {
            let prices = if variants.is_empty() {
                product
                    .variants
                    .iter()
                    .filter_map(|variant| variant.get("price").and_then(Value::as_str))
                    .filter_map(storefront_parse_price)
                    .collect::<Vec<_>>()
            } else {
                variants
                    .iter()
                    .filter_map(|variant| storefront_parse_price(&variant.price))
                    .collect::<Vec<_>>()
            };
            storefront_price_bounds(prices)
                .map(|(min_price, _)| StagedSortValue::String(format!("{min_price:020.4}")))
                .unwrap_or(StagedSortValue::Null)
        }
        Some("ID") => return storefront_gid_sort_key(&product.id),
        Some(_) => storefront_gid_sort_key(&product.id)
            .into_iter()
            .next()
            .unwrap_or(StagedSortValue::Null),
    };
    vec![primary, storefront_gid_tail_sort_value(&product.id)]
}

fn storefront_gid_sort_key(id: &str) -> StagedSortKey {
    vec![storefront_gid_tail_sort_value(id)]
}

fn storefront_gid_tail_sort_value(id: &str) -> StagedSortValue {
    let tail = resource_id_tail(id);
    tail.parse::<i64>()
        .map(StagedSortValue::I64)
        .unwrap_or_else(|_| storefront_sort_string(tail))
}

fn storefront_sort_string(value: impl AsRef<str>) -> StagedSortValue {
    StagedSortValue::String(value.as_ref().to_ascii_lowercase())
}

pub(in crate::proxy) fn storefront_request_context(
    query: &str,
    variables: &BTreeMap<String, ResolvedValue>,
) -> StorefrontRequestContext {
    let mut context = StorefrontRequestContext::default();
    let Ok(directives) = operation_directive_invocations(query, variables, None) else {
        return context;
    };
    for directive in directives
        .into_iter()
        .filter(|directive| directive.name == "inContext")
    {
        if let Some(country) = resolved_value_string(directive.arguments.get("country")) {
            context.country = Some(country);
        }
        if let Some(language) = resolved_value_string(directive.arguments.get("language")) {
            context.language = Some(language);
        }
        if let Some(preferred_location_id) =
            resolved_value_string(directive.arguments.get("preferredLocationId"))
        {
            context.preferred_location_id = Some(preferred_location_id);
        }
        if let Some(ResolvedValue::Object(buyer)) = directive.arguments.get("buyer") {
            context.buyer_customer_access_token =
                resolved_string_field(buyer, "customerAccessToken");
            context.buyer_company_location_id = resolved_string_field(buyer, "companyLocationId");
        }
        context.uses_enrichment_context = directive.arguments.contains_key("preferredLocationId")
            || directive.arguments.contains_key("buyer");
    }
    context
}

fn storefront_request_context_from_request(request: &Request) -> StorefrontRequestContext {
    parse_graphql_request_body(&request.body)
        .map(|body| storefront_request_context(&body.query, &body.variables))
        .unwrap_or_default()
}

fn storefront_first_slice_hydrate_body(
    context: &StorefrontRequestContext,
) -> (&'static str, Value) {
    if context.uses_enrichment_context {
        return (
            STOREFRONT_ENRICHMENT_CONTEXT_HYDRATE_QUERY,
            json!({
                "country": context.country,
                "language": context.language,
                // A locally allocated Admin Location id cannot be sent to Shopify's
                // Storefront API. Captured behavior shows it does not alter the
                // country/language localization or this scenario's empty availability.
                "preferredLocationId": Value::Null,
                "buyer": Value::Null
            }),
        );
    }
    if context.has_in_context_values() {
        (
            STOREFRONT_FIRST_SLICE_CONTEXT_HYDRATE_QUERY,
            json!({
                "country": context.country,
                "language": context.language
            }),
        )
    } else {
        (STOREFRONT_FIRST_SLICE_HYDRATE_QUERY, json!({}))
    }
}

fn resolved_value_string(value: Option<&ResolvedValue>) -> Option<String> {
    match value {
        Some(ResolvedValue::String(value)) if !value.is_empty() => Some(value.clone()),
        _ => None,
    }
}

fn storefront_metafield_is_public(metafield: &Value) -> bool {
    metafield
        .pointer("/definition/access/storefront")
        .and_then(Value::as_str)
        == Some("PUBLIC_READ")
        || metafield
            .get("__storefrontPublic")
            .and_then(Value::as_bool)
            .unwrap_or(false)
}

fn storefront_metaobject_fields(record: &Value) -> Value {
    let mut fields = record["fields"]
        .as_array()
        .into_iter()
        .flatten()
        .cloned()
        .collect::<Vec<_>>();
    fields.sort_by(|left, right| {
        left.get("key")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .cmp(right.get("key").and_then(Value::as_str).unwrap_or_default())
    });
    Value::Array(fields)
}

fn storefront_metaobject_field_value(record: &Value) -> Value {
    json!({
        "__typename": "MetaobjectField",
        "key": record.get("key").cloned().unwrap_or(Value::Null),
        "type": record.get("type").cloned().unwrap_or(Value::Null),
        "value": record.get("value").cloned().unwrap_or(Value::Null),
    })
}

fn storefront_metaobject_sort_key(record: &Value, sort_key: Option<&str>) -> StagedSortKey {
    let sort_key = sort_key
        .unwrap_or("id")
        .replace('-', "_")
        .to_ascii_lowercase();
    let primary = match sort_key.as_str() {
        "updated_at" | "updatedat" => StagedSortValue::String(
            record
                .get("updatedAt")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
        ),
        _ => resource_id_tail_sort_value(record.get("id").and_then(Value::as_str)),
    };
    vec![
        primary,
        resource_id_tail_sort_value(record.get("id").and_then(Value::as_str)),
    ]
}

fn storefront_policy_from_admin(policy: &ShopPolicyRecord) -> Value {
    let mut policy = shop_policy_record_json(policy);
    policy["handle"] = policy
        .get("handle")
        .cloned()
        .or_else(|| {
            policy
                .get("url")
                .and_then(Value::as_str)
                .and_then(policy_handle_from_url)
                .map(Value::String)
        })
        .unwrap_or(Value::Null);
    policy
}

fn policy_handle_from_url(url: &str) -> Option<String> {
    let without_query = url.split('?').next().unwrap_or(url);
    let segment = without_query
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .map(str::trim)
        .filter(|value| !value.is_empty())?;
    Some(segment.strip_suffix(".html").unwrap_or(segment).to_string())
}

fn push_storefront_location(
    records: &mut Vec<Value>,
    seen: &mut BTreeSet<String>,
    location: Value,
) {
    let Some(id) = location.get("id").and_then(Value::as_str) else {
        return;
    };
    if seen.insert(id.to_string()) {
        records.push(location);
    }
}

fn push_admin_location_as_storefront(
    records: &mut Vec<Value>,
    seen: &mut BTreeSet<String>,
    location: &Value,
) {
    if location.get("isActive").and_then(Value::as_bool) == Some(false)
        || location
            .get("isFulfillmentService")
            .and_then(Value::as_bool)
            == Some(true)
    {
        return;
    }
    push_storefront_location(records, seen, storefront_location_from_admin(location));
}

fn storefront_location_from_admin(location: &Value) -> Value {
    json!({
        "id": location.get("id").cloned().unwrap_or(Value::Null),
        "name": location.get("name").cloned().unwrap_or(Value::Null),
        "address": location.get("address").cloned().unwrap_or(Value::Null)
    })
}

fn sort_storefront_locations(records: &mut [Value], arguments: &BTreeMap<String, ResolvedValue>) {
    let sort_key = resolved_string_field(arguments, "sortKey").unwrap_or_else(|| "ID".to_string());
    records.sort_by(|left, right| {
        storefront_location_sort_value(left, &sort_key)
            .cmp(&storefront_location_sort_value(right, &sort_key))
            .then_with(|| {
                left.get("id")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .cmp(right.get("id").and_then(Value::as_str).unwrap_or_default())
            })
    });
    if resolved_bool_field(arguments, "reverse").unwrap_or(false) {
        records.reverse();
    }
}

fn storefront_location_sort_value(location: &Value, sort_key: &str) -> String {
    match sort_key {
        "NAME" => location
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_ascii_lowercase(),
        "CITY" => location
            .pointer("/address/city")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_ascii_lowercase(),
        _ => location
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
    }
}

fn storefront_location_cursor(location: &Value, cursor_by_id: &BTreeMap<String, String>) -> String {
    location
        .get("id")
        .and_then(Value::as_str)
        .and_then(|id| cursor_by_id.get(id).cloned())
        .unwrap_or_default()
}

fn storefront_customer_shared_record(
    id: &str,
    first_name: Option<&str>,
    last_name: Option<&str>,
    email: &str,
    phone: Option<&str>,
    accepts_marketing: bool,
    timestamp: &str,
) -> Value {
    let display_name = storefront_customer_display_name(first_name, last_name, Some(email));
    json!({
        "id": id,
        "firstName": first_name,
        "lastName": last_name,
        "displayName": display_name,
        "email": email,
        "phone": phone,
        "locale": Value::Null,
        "note": Value::Null,
        "verifiedEmail": true,
        "taxExempt": false,
        "taxExemptions": [],
        "tags": [],
        "state": "ENABLED",
        "dataSaleOptOut": false,
        "canDelete": true,
        "acceptsMarketing": accepts_marketing,
        "metafield": Value::Null,
        "metafields": [],
        "defaultEmailAddress": { "emailAddress": email },
        "defaultPhoneNumber": Value::Null,
        "emailMarketingConsent": {
            "marketingState": if accepts_marketing { "SUBSCRIBED" } else { "NOT_SUBSCRIBED" },
            "marketingOptInLevel": Value::Null,
            "consentUpdatedAt": timestamp
        },
        "smsMarketingConsent": Value::Null,
        "defaultAddress": Value::Null,
        "addressesV2": connection_json_with_empty_edges(Vec::new()),
        "orders": connection_json_with_empty_edges(Vec::new()),
        "numberOfOrders": "0",
        "createdAt": timestamp,
        "updatedAt": timestamp
    })
}

pub(in crate::proxy) fn storefront_customer_json(customer: &Value) -> Value {
    let email = customer.get("email").and_then(Value::as_str);
    let first_name = customer.get("firstName").and_then(Value::as_str);
    let last_name = customer.get("lastName").and_then(Value::as_str);
    let display_name = customer
        .get("displayName")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| storefront_customer_display_name(first_name, last_name, email));
    let accepts_marketing = customer
        .get("acceptsMarketing")
        .and_then(Value::as_bool)
        .unwrap_or_else(|| {
            customer
                .pointer("/emailMarketingConsent/marketingState")
                .and_then(Value::as_str)
                == Some("SUBSCRIBED")
        });
    let default_address = customer
        .get("defaultAddress")
        .map(storefront_mailing_address_json)
        .unwrap_or(Value::Null);
    json!({
        "__typename": "Customer",
        "id": customer.get("id").cloned().unwrap_or(Value::Null),
        "email": customer.get("email").cloned().unwrap_or(Value::Null),
        "firstName": customer.get("firstName").cloned().unwrap_or(Value::Null),
        "lastName": customer.get("lastName").cloned().unwrap_or(Value::Null),
        "displayName": display_name,
        "phone": customer.get("phone").cloned().unwrap_or(Value::Null),
        "acceptsMarketing": accepts_marketing,
        "createdAt": customer.get("createdAt").cloned().unwrap_or(Value::Null),
        "updatedAt": customer.get("updatedAt").cloned().unwrap_or(Value::Null),
        "numberOfOrders": customer.get("numberOfOrders").cloned().unwrap_or_else(|| json!("0")),
        "tags": customer.get("tags").cloned().unwrap_or_else(|| json!([])),
        "defaultAddress": default_address,
        "addresses": connection_json_with_empty_edges(Vec::new()),
        "orders": connection_json_with_empty_edges(Vec::new()),
        "avatarUrl": Value::Null,
        "socialLoginProvider": Value::Null,
        "metafield": Value::Null,
        "metafields": []
    })
}

fn storefront_mailing_address_json(address: &Value) -> Value {
    if address.is_null() {
        return Value::Null;
    }
    let mut projected = address.clone();
    if let Some(object) = projected.as_object_mut() {
        if !object.contains_key("countryCode") {
            object.insert(
                "countryCode".to_string(),
                address.get("countryCodeV2").cloned().unwrap_or(Value::Null),
            );
        }
        object
            .entry("formatted".to_string())
            .or_insert_with(|| storefront_formatted_address_lines(address));
        object.entry("latitude".to_string()).or_insert(Value::Null);
        object.entry("longitude".to_string()).or_insert(Value::Null);
    }
    projected
}

fn storefront_formatted_address_lines(address: &Value) -> Value {
    let mut lines = Vec::new();
    for field in ["address1", "address2"] {
        if let Some(value) = address.get(field).and_then(Value::as_str) {
            if !value.is_empty() {
                lines.push(json!(value));
            }
        }
    }
    let locality = ["city", "province", "zip"]
        .into_iter()
        .filter_map(|field| address.get(field).and_then(Value::as_str))
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>()
        .join(", ");
    if !locality.is_empty() {
        lines.push(json!(locality));
    }
    if let Some(country) = address.get("country").and_then(Value::as_str) {
        if !country.is_empty() {
            lines.push(json!(country));
        }
    }
    Value::Array(lines)
}

fn storefront_order_json(order: &Value) -> Value {
    let currency_code = order
        .get("currencyCode")
        .or_else(|| order.pointer("/currentTotalPriceSet/shopMoney/currencyCode"))
        .or_else(|| order.pointer("/totalPriceSet/shopMoney/currencyCode"))
        .cloned()
        .unwrap_or_else(|| json!("USD"));
    let total_price = order
        .get("totalPriceV2")
        .or_else(|| order.pointer("/currentTotalPriceSet/shopMoney"))
        .or_else(|| order.pointer("/totalPriceSet/shopMoney"))
        .cloned()
        .unwrap_or_else(|| json!({ "amount": "0.0", "currencyCode": currency_code.clone() }));
    json!({
        "__typename": "Order",
        "id": order.get("id").cloned().unwrap_or(Value::Null),
        "name": order.get("name").cloned().unwrap_or_else(|| json!("")),
        "email": order.get("email").cloned().unwrap_or(Value::Null),
        "phone": order.get("phone").cloned().unwrap_or(Value::Null),
        "currencyCode": currency_code,
        "customerUrl": order.get("customerUrl").cloned().unwrap_or(Value::Null),
        "financialStatus": order.get("displayFinancialStatus").or_else(|| order.get("financialStatus")).cloned().unwrap_or(Value::Null),
        "fulfillmentStatus": order.get("displayFulfillmentStatus").or_else(|| order.get("fulfillmentStatus")).cloned().unwrap_or_else(|| json!("UNFULFILLED")),
        "orderNumber": storefront_order_number(order),
        "processedAt": order.get("processedAt").or_else(|| order.get("createdAt")).cloned().unwrap_or_else(|| json!("1970-01-01T00:00:00Z")),
        "subtotalPriceV2": order.get("subtotalPriceV2").or_else(|| order.pointer("/subtotalPriceSet/shopMoney")).cloned().unwrap_or(Value::Null),
        "totalPrice": total_price.clone(),
        "totalPriceV2": total_price,
        "lineItems": order.get("lineItems").cloned().unwrap_or_else(|| connection_json_with_empty_edges(Vec::new()))
    })
}

fn storefront_order_number(order: &Value) -> Value {
    if let Some(number) = order.get("orderNumber").and_then(Value::as_i64) {
        return json!(number);
    }
    let digits = order
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .chars()
        .filter(|ch| ch.is_ascii_digit())
        .collect::<String>();
    digits
        .parse::<i64>()
        .map(Value::from)
        .unwrap_or_else(|_| json!(0))
}

fn storefront_order_cursor(order: &Value) -> String {
    order
        .get("id")
        .and_then(Value::as_str)
        .map(|id| format!("cursor:{id}"))
        .unwrap_or_default()
}

fn storefront_customer_address_node_index(nodes: &[Value], address_id: &str) -> Option<usize> {
    nodes
        .iter()
        .position(|node| node.get("id").and_then(Value::as_str) == Some(address_id))
}

fn storefront_customer_default_address_id(customer: &Value) -> Option<&str> {
    customer
        .get("defaultAddress")
        .and_then(|address| address.get("id"))
        .and_then(Value::as_str)
}

fn storefront_customer_payload(
    customer: Value,
    customer_access_token: Value,
    customer_user_errors: Vec<Value>,
) -> Value {
    let user_errors = storefront_user_errors_without_code(&customer_user_errors);
    json!({
        "customer": customer,
        "customerAccessToken": customer_access_token,
        "customerUserErrors": customer_user_errors,
        "userErrors": user_errors
    })
}

fn storefront_customer_address_payload(
    address_field: &str,
    address: Value,
    customer_user_errors: Vec<Value>,
) -> Value {
    let customer_user_errors = storefront_customer_user_errors_with_codes(customer_user_errors);
    let user_errors = storefront_user_errors_without_code(&customer_user_errors);
    json!({
        (address_field): storefront_mailing_address_json(&address),
        "customerUserErrors": customer_user_errors,
        "userErrors": user_errors,
    })
}

fn storefront_customer_address_delete_payload(
    deleted_customer_address_id: Value,
    customer_user_errors: Vec<Value>,
) -> Value {
    let customer_user_errors = storefront_customer_user_errors_with_codes(customer_user_errors);
    let user_errors = storefront_user_errors_without_code(&customer_user_errors);
    json!({
        "deletedCustomerAddressId": deleted_customer_address_id,
        "customerUserErrors": customer_user_errors,
        "userErrors": user_errors,
    })
}

fn storefront_customer_token_payload(
    customer_access_token: Value,
    customer_user_errors: Vec<Value>,
) -> Value {
    let user_errors = storefront_user_errors_without_code(&customer_user_errors);
    json!({
        "customerAccessToken": customer_access_token,
        "customerUserErrors": customer_user_errors,
        "userErrors": user_errors
    })
}

fn storefront_customer_activation_payload(
    customer: Value,
    customer_access_token: Value,
    customer_user_errors: Vec<Value>,
    include_user_errors: bool,
) -> Value {
    let mut payload = json!({
        "customer": customer,
        "customerAccessToken": customer_access_token,
        "customerUserErrors": customer_user_errors
    });
    if include_user_errors {
        payload["userErrors"] = storefront_plain_user_errors_with_null_field(
            payload["customerUserErrors"]
                .as_array()
                .map(Vec::as_slice)
                .unwrap_or(&[]),
        );
    }
    payload
}

fn storefront_plain_user_errors_with_null_field(errors: &[Value]) -> Value {
    Value::Array(
        errors
            .iter()
            .map(|error| {
                json!({
                    "field": Value::Null,
                    "message": error.get("message").cloned().unwrap_or(Value::Null)
                })
            })
            .collect(),
    )
}

fn storefront_user_errors_without_code(errors: &[Value]) -> Value {
    Value::Array(
        errors
            .iter()
            .map(|error| {
                json!({
                    "field": error.get("field").cloned().unwrap_or(Value::Null),
                    "message": error.get("message").cloned().unwrap_or(Value::Null)
                })
            })
            .collect(),
    )
}

fn storefront_customer_user_errors_with_codes(errors: Vec<Value>) -> Vec<Value> {
    errors
        .into_iter()
        .map(|mut error| {
            if let Some(object) = error.as_object_mut() {
                object.entry("code".to_string()).or_insert(Value::Null);
            }
            error
        })
        .collect()
}

fn storefront_customer_user_error(
    field: impl serde::Serialize,
    message: &str,
    code: Option<&str>,
) -> Value {
    json!({
        "field": field,
        "message": message,
        "code": code
    })
}

fn storefront_invalid_customer_access_token_errors() -> Vec<Value> {
    vec![storefront_customer_user_error(
        ["customerAccessToken"],
        "Invalid customer access token",
        Some("INVALID"),
    )]
}

fn storefront_access_denied_error() -> crate::admin_graphql::RootFieldError {
    storefront_access_denied_error_for("customerAccessTokenDelete")
}

fn storefront_access_denied_error_for(root_name: &str) -> crate::admin_graphql::RootFieldError {
    crate::admin_graphql::RootFieldError {
        message: format!(
            "Access denied for {root_name} field. Required access: `unauthenticated_write_customers` access scope. Also: Requires valid customer access token."
        ),
        extensions: [
            ("code".to_string(), json!("ACCESS_DENIED")),
            (
                "documentation".to_string(),
                json!("https://shopify.dev/api/usage/access-scopes"),
            ),
            (
                "requiredAccess".to_string(),
                json!("`unauthenticated_write_customers` access scope. Also: Requires valid customer access token."),
            ),
        ]
        .into_iter()
        .collect(),
        path: Some(Vec::new()),
        locations: Vec::new(),
    }
}

fn preserve_storefront_address_phone(
    node: &mut Value,
    address_input: &BTreeMap<String, ResolvedValue>,
) {
    if !address_input.contains_key("phone") {
        return;
    }
    let phone = resolved_string_field(address_input, "phone")
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .map(Value::String)
        .unwrap_or(Value::Null);
    if let Some(object) = node.as_object_mut() {
        object.insert("phone".to_string(), phone);
    }
}

fn storefront_not_found_error() -> crate::admin_graphql::RootFieldError {
    crate::admin_graphql::RootFieldError {
        message: "Unidentified customer".to_string(),
        extensions: [("code".to_string(), json!("NOT_FOUND"))]
            .into_iter()
            .collect(),
        path: Some(Vec::new()),
        locations: Vec::new(),
    }
}

fn storefront_customer_display_name(
    first_name: Option<&str>,
    last_name: Option<&str>,
    email: Option<&str>,
) -> String {
    let name = [first_name, last_name]
        .into_iter()
        .flatten()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    if !name.is_empty() {
        return name;
    }
    email.unwrap_or_default().to_string()
}

fn storefront_customer_state(customer: &Value) -> &str {
    customer
        .get("state")
        .and_then(Value::as_str)
        .unwrap_or("DISABLED")
}

fn storefront_customer_password_matches(customer: &Value, password: &str) -> bool {
    let Some(customer_id) = customer.get("id").and_then(Value::as_str) else {
        return false;
    };
    customer
        .get(STOREFRONT_CUSTOMER_PASSWORD_FINGERPRINT_FIELD)
        .and_then(Value::as_str)
        == Some(storefront_password_fingerprint(customer_id, password).as_str())
}

fn storefront_customer_activation_token_for_id(customer_id: &str) -> String {
    let stable_tail = resource_id_tail(customer_id)
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .collect::<String>();
    if stable_tail.is_empty() {
        "sdp-activation-token".to_string()
    } else {
        format!("sdp-activation-{stable_tail}")
    }
}

fn storefront_access_token_value(customer_id: &str, sequence: u64, expires_at: &str) -> String {
    let seed = format!("{customer_id}:{sequence}:{expires_at}");
    format!(
        "sdp_ca_{}_{}",
        sequence,
        &storefront_sha256_hex(&seed)[..24]
    )
}

fn storefront_password_fingerprint(customer_id: &str, password: &str) -> String {
    storefront_sha256_hex(&format!("storefront-password:{customer_id}:{password}"))
}

fn storefront_token_hash(token: &str) -> String {
    storefront_sha256_hex(&format!("storefront-token:{token}"))
}

pub(in crate::proxy) fn storefront_sha256_hex(value: &str) -> String {
    let digest = Sha256::digest(value.as_bytes());
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

pub(in crate::proxy) fn storefront_format_timestamp(timestamp: time::OffsetDateTime) -> String {
    timestamp
        .format(&time::format_description::well_known::Rfc3339)
        .expect("UTC timestamps should format as RFC3339")
}

fn storefront_timestamp_is_future(value: &str, now: time::OffsetDateTime) -> bool {
    time::OffsetDateTime::parse(value, &time::format_description::well_known::Rfc3339)
        .map(|expires_at| expires_at > now)
        .unwrap_or(false)
}

fn storefront_email_looks_valid(email: &str) -> bool {
    let Some((local, domain)) = email.split_once('@') else {
        return false;
    };
    !local.is_empty() && domain.contains('.') && !domain.starts_with('.') && !domain.ends_with('.')
}

pub(in crate::proxy) fn storefront_customer_email_key(email: &str) -> String {
    email.trim().to_ascii_lowercase()
}

fn storefront_customer_contains_html_tag(value: &str) -> bool {
    let Some(start) = value.find('<') else {
        return false;
    };
    value[start..].contains('>')
}

fn storefront_redacted_variables_json(variables: &BTreeMap<String, ResolvedValue>) -> Value {
    let value = resolved_variables_json(variables);
    storefront_redact_sensitive_json(value, None)
}

pub(in crate::proxy) fn storefront_redact_sensitive_json(value: Value, key: Option<&str>) -> Value {
    if key.is_some_and(storefront_sensitive_customer_auth_key) {
        return json!("<redacted:storefront-customer-auth>");
    }
    match value {
        Value::Array(values) => Value::Array(
            values
                .into_iter()
                .map(|value| storefront_redact_sensitive_json(value, None))
                .collect(),
        ),
        Value::Object(object) => Value::Object(
            object
                .into_iter()
                .map(|(key, value)| {
                    let redacted = storefront_redact_sensitive_json(value, Some(&key));
                    (key, redacted)
                })
                .collect(),
        ),
        other => other,
    }
}

fn storefront_sensitive_customer_auth_key(key: &str) -> bool {
    matches!(
        key,
        "password"
            | "customerAccessToken"
            | "accessToken"
            | "activationToken"
            | "activationUrl"
            | "resetToken"
            | "resetUrl"
            | "token"
            | "multipassToken"
    )
}

use crate::{
    admin_graphql::{self, AdminApiVersion},
    graphql::OperationType,
    proxy::DraftProxy,
    resolver_registry::ExecutableRootRegistration,
    storefront_graphql::{self, StorefrontApiVersion},
};
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ApiSurface {
    Admin,
    Storefront,
}

impl ApiSurface {
    pub fn registry_name(self) -> &'static str {
        match self {
            Self::Admin => "admin",
            Self::Storefront => "storefront",
        }
    }

    /// Return the globally unique internal resolver name for one public
    /// GraphQL root. Admin keeps the Shopify root name; Storefront uses an
    /// explicit prefix so same-named roots can never share a callback slot.
    pub fn resolver_name(self, graphql_root_name: &str) -> String {
        match self {
            Self::Admin => graphql_root_name.to_string(),
            Self::Storefront => {
                let mut chars = graphql_root_name.chars();
                let Some(first) = chars.next() else {
                    return "storefront".to_string();
                };
                format!("storefront{}{}", first.to_ascii_uppercase(), chars.as_str())
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapabilityDomain {
    Products,
    AdminPlatform,
    B2b,
    Apps,
    Media,
    BulkOperations,
    Customers,
    Orders,
    StoreProperties,
    Discounts,
    Events,
    Functions,
    Payments,
    Marketing,
    OnlineStore,
    SavedSearches,
    Privacy,
    Segments,
    ShippingFulfillments,
    GiftCards,
    Webhooks,
    Localization,
    Markets,
    Metafields,
    Metaobjects,
    Storefront,
    Unknown,
}

impl CapabilityDomain {
    pub fn registry_name(self) -> &'static str {
        match self {
            Self::Products => "products",
            Self::AdminPlatform => "admin-platform",
            Self::B2b => "b2b",
            Self::Apps => "apps",
            Self::Media => "media",
            Self::BulkOperations => "bulk-operations",
            Self::Customers => "customers",
            Self::Orders => "orders",
            Self::StoreProperties => "store-properties",
            Self::Discounts => "discounts",
            Self::Events => "events",
            Self::Functions => "functions",
            Self::Payments => "payments",
            Self::Marketing => "marketing",
            Self::OnlineStore => "online-store",
            Self::SavedSearches => "saved-searches",
            Self::Privacy => "privacy",
            Self::Segments => "segments",
            Self::ShippingFulfillments => "shipping-fulfillments",
            Self::GiftCards => "gift-cards",
            Self::Webhooks => "webhooks",
            Self::Localization => "localization",
            Self::Markets => "markets",
            Self::Metafields => "metafields",
            Self::Metaobjects => "metaobjects",
            Self::Storefront => "storefront",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapabilityExecution {
    OverlayRead,
    StageLocally,
    Passthrough,
}

impl CapabilityExecution {
    pub fn registry_name(self) -> &'static str {
        match self {
            Self::OverlayRead => "overlay-read",
            Self::StageLocally => "stage-locally",
            Self::Passthrough => "passthrough",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationRegistryEntry {
    pub api_surface: ApiSurface,
    pub name: String,
    pub operation_type: OperationType,
    pub domain: CapabilityDomain,
    pub implemented: bool,
    pub runtime_tests: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphqlRootCatalogEntry {
    pub api_surface: ApiSurface,
    pub api_versions: Vec<String>,
    pub name: String,
    pub operation_type: OperationType,
    pub registration: Option<OperationRegistryEntry>,
}

impl OperationRegistryEntry {
    pub fn execution(&self) -> CapabilityExecution {
        execution_for_operation_type(self.operation_type)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationCapability {
    pub api_surface: ApiSurface,
    pub domain: CapabilityDomain,
    pub execution: CapabilityExecution,
}

pub fn default_registry() -> Vec<OperationRegistryEntry> {
    let registry = default_executable_registry()
        .into_iter()
        .map(|registration| registration.entry)
        .collect::<Vec<_>>();
    debug_assert_default_registry_local_routing_contract(&registry);
    registry
}

pub(crate) fn default_executable_registry() -> Vec<ExecutableRootRegistration> {
    let mut registry = crate::operation_registry_data::default_registry_bindings();
    registry.extend(storefront_registry_bindings());
    registry
}

pub fn default_registry_json_value() -> Value {
    registry_json_value(&default_registry())
}

/// Complete captured root inventory across every executable surface/version.
/// Capability registration is attached when present, but absence remains an
/// explicit catalog state rather than silently omitting the schema root.
pub fn default_graphql_root_catalog() -> Vec<GraphqlRootCatalogEntry> {
    let mut roots = BTreeMap::<(ApiSurface, OperationType, String), BTreeSet<String>>::new();
    for version in AdminApiVersion::ALL {
        collect_surface_roots(
            &mut roots,
            ApiSurface::Admin,
            version.as_str(),
            |operation_type| admin_graphql::root_field_names(version, operation_type),
        );
    }
    for version in StorefrontApiVersion::ALL {
        collect_surface_roots(
            &mut roots,
            ApiSurface::Storefront,
            version.as_str(),
            |operation_type| storefront_graphql::root_field_names(version, operation_type),
        );
    }

    let registry = default_registry();
    roots
        .into_iter()
        .map(
            |((api_surface, operation_type, name), api_versions)| GraphqlRootCatalogEntry {
                registration: registry
                    .iter()
                    .find(|entry| {
                        entry.api_surface == api_surface
                            && entry.operation_type == operation_type
                            && entry.name == name
                    })
                    .cloned(),
                api_surface,
                api_versions: api_versions.into_iter().collect(),
                name,
                operation_type,
            },
        )
        .collect()
}

pub fn default_graphql_root_catalog_json_value() -> Value {
    Value::Array(
        default_graphql_root_catalog()
            .iter()
            .map(graphql_root_catalog_entry_json_value)
            .collect(),
    )
}

fn collect_surface_roots(
    roots: &mut BTreeMap<(ApiSurface, OperationType, String), BTreeSet<String>>,
    api_surface: ApiSurface,
    api_version: &str,
    names: impl Fn(OperationType) -> Vec<String>,
) {
    for operation_type in [
        OperationType::Query,
        OperationType::Mutation,
        OperationType::Subscription,
    ] {
        for name in names(operation_type) {
            roots
                .entry((api_surface, operation_type, name))
                .or_default()
                .insert(api_version.to_string());
        }
    }
}

fn graphql_root_catalog_entry_json_value(entry: &GraphqlRootCatalogEntry) -> Value {
    json!({
        "apiSurface": entry.api_surface.registry_name(),
        "apiVersions": entry.api_versions,
        "name": entry.name,
        "type": entry.operation_type.keyword(),
        "registration": entry.registration.as_ref().map(registry_entry_json_value),
    })
}

pub fn registry_json_value(registry: &[OperationRegistryEntry]) -> Value {
    Value::Array(registry.iter().map(registry_entry_json_value).collect())
}

pub fn implemented_entries(registry: &[OperationRegistryEntry]) -> Vec<&OperationRegistryEntry> {
    registry.iter().filter(|entry| entry.implemented).collect()
}

pub fn execution_for_operation_type(operation_type: OperationType) -> CapabilityExecution {
    match operation_type {
        OperationType::Query => CapabilityExecution::OverlayRead,
        OperationType::Mutation => CapabilityExecution::StageLocally,
        OperationType::Subscription => CapabilityExecution::Passthrough,
    }
}

pub fn operation_capability(
    registry: &[OperationRegistryEntry],
    operation_type: OperationType,
    root_field: Option<&str>,
) -> OperationCapability {
    operation_capability_for_surface(registry, ApiSurface::Admin, operation_type, root_field)
}

pub fn operation_capability_for_surface(
    registry: &[OperationRegistryEntry],
    api_surface: ApiSurface,
    operation_type: OperationType,
    root_field: Option<&str>,
) -> OperationCapability {
    let Some(field) = root_field.filter(|name| !name.is_empty()) else {
        return OperationCapability {
            api_surface,
            domain: CapabilityDomain::Unknown,
            execution: CapabilityExecution::Passthrough,
        };
    };
    let local_entry = registry.iter().find(|entry| {
        entry.implemented
            && entry.api_surface == api_surface
            && entry.operation_type == operation_type
            && entry.name == field
    });

    match local_entry {
        Some(entry) => OperationCapability {
            api_surface: entry.api_surface,
            domain: entry.domain,
            execution: entry.execution(),
        },
        None => OperationCapability {
            api_surface,
            domain: CapabilityDomain::Unknown,
            execution: CapabilityExecution::Passthrough,
        },
    }
}

fn registry_entry_json_value(entry: &OperationRegistryEntry) -> Value {
    let mut object = Map::new();
    object.insert(
        "apiSurface".to_string(),
        json!(entry.api_surface.registry_name()),
    );
    object.insert("name".to_string(), json!(entry.name));
    object.insert("type".to_string(), json!(entry.operation_type.keyword()));
    object.insert("domain".to_string(), json!(entry.domain.registry_name()));
    object.insert(
        "execution".to_string(),
        json!(entry.execution().registry_name()),
    );
    object.insert("implemented".to_string(), json!(entry.implemented));
    object.insert("runtimeTests".to_string(), json!(entry.runtime_tests));
    Value::Object(object)
}

fn debug_assert_default_registry_local_routing_contract(registry: &[OperationRegistryEntry]) {
    static CHECKED: OnceLock<()> = OnceLock::new();
    CHECKED.get_or_init(|| {
        for entry in implemented_entries(registry) {
            let capability = operation_capability_for_surface(
                registry,
                entry.api_surface,
                entry.operation_type,
                Some(entry.name.as_str()),
            );
            debug_assert_eq!(
                capability.api_surface, entry.api_surface,
                "{} must classify through its declared API surface",
                entry.name
            );
            debug_assert_eq!(
                capability.domain, entry.domain,
                "{} must classify through its canonical local registry root",
                entry.name
            );
            debug_assert_eq!(
                capability.execution,
                entry.execution(),
                "{} must derive local execution from operation type",
                entry.name
            );
            debug_assert_ne!(
                capability.execution,
                CapabilityExecution::Passthrough,
                "{} is implemented and must dispatch locally",
                entry.name
            );
        }
    });
}

fn storefront_registry_bindings() -> Vec<ExecutableRootRegistration> {
    let mut entries = Vec::new();
    let mut seen = BTreeSet::new();
    for version in StorefrontApiVersion::ALL {
        for operation_type in [OperationType::Query, OperationType::Mutation] {
            entries.extend(storefront_registry_bindings_for_roots(
                storefront_graphql::root_field_names(version, operation_type),
                operation_type,
                &mut seen,
            ));
        }
    }
    entries
}

fn storefront_registry_bindings_for_roots(
    roots: impl IntoIterator<Item = String>,
    operation_type: OperationType,
    seen: &mut BTreeSet<(&'static str, String)>,
) -> Vec<ExecutableRootRegistration> {
    roots
        .into_iter()
        .filter(|name| seen.insert((operation_type.keyword(), name.clone())))
        .map(|name| {
            let handler = storefront_root_handler(operation_type, &name);
            ExecutableRootRegistration {
                entry: OperationRegistryEntry {
                    api_surface: ApiSurface::Storefront,
                    name: name.clone(),
                    operation_type,
                    domain: CapabilityDomain::Storefront,
                    implemented: handler.is_some(),
                    runtime_tests: storefront_runtime_tests(operation_type, &name),
                },
                handler,
            }
        })
        .collect()
}

fn storefront_root_handler(
    operation_type: OperationType,
    name: &str,
) -> Option<crate::resolver_registry::NativeResolverHandler> {
    use crate::resolver_registry::NativeResolverHandler;

    let handler = match operation_type {
        OperationType::Query => match name {
            "shop" | "localization" | "locations" | "paymentSettings" | "publicApiVersions" => {
                DraftProxy::storefront_platform_query_resolver
            }
            "product"
            | "productByHandle"
            | "productRecommendations"
            | "productTags"
            | "productTypes"
            | "products" => DraftProxy::storefront_catalog_query_resolver,
            "collection" | "collectionByHandle" | "collections" => {
                DraftProxy::storefront_collection_query_resolver
            }
            "article" | "articles" | "blog" | "blogByHandle" | "blogs" | "menu" | "page"
            | "pageByHandle" | "pages" | "sitemap" | "urlRedirects" => {
                DraftProxy::storefront_content_query_resolver
            }
            "metaobject" | "metaobjects" => DraftProxy::storefront_custom_data_query_resolver,
            "node" | "nodes" | "search" | "predictiveSearch" => {
                DraftProxy::storefront_discovery_query_resolver
            }
            "cart" => DraftProxy::storefront_cart_query_resolver,
            "customer" => DraftProxy::storefront_customer_query_resolver,
            _ => return None,
        },
        OperationType::Mutation => match name {
            "cartCreate"
            | "cartLinesAdd"
            | "cartLinesUpdate"
            | "cartLinesRemove"
            | "cartAttributesUpdate"
            | "cartNoteUpdate"
            | "cartBuyerIdentityUpdate"
            | "cartDiscountCodesUpdate"
            | "cartGiftCardCodesAdd"
            | "cartGiftCardCodesRemove"
            | "cartGiftCardCodesUpdate"
            | "cartMetafieldsSet"
            | "cartMetafieldDelete"
            | "cartDeliveryAddressesAdd"
            | "cartDeliveryAddressesUpdate"
            | "cartDeliveryAddressesRemove"
            | "cartDeliveryAddressesReplace"
            | "cartSelectedDeliveryOptionsUpdate" => DraftProxy::storefront_cart_mutation_resolver,
            "customerCreate"
            | "customerAccessTokenCreate"
            | "customerAccessTokenRenew"
            | "customerAccessTokenDelete"
            | "customerActivate"
            | "customerActivateByUrl"
            | "customerRecover"
            | "customerReset"
            | "customerResetByUrl"
            | "customerAccessTokenCreateWithMultipass"
            | "customerUpdate"
            | "customerAddressCreate"
            | "customerAddressUpdate"
            | "customerAddressDelete"
            | "customerDefaultAddressUpdate" => DraftProxy::storefront_customer_mutation_resolver,
            _ => return None,
        },
        OperationType::Subscription => return None,
    };
    Some(handler as NativeResolverHandler)
}

fn storefront_runtime_tests(operation_type: OperationType, name: &str) -> Vec<String> {
    if storefront_root_handler(operation_type, name).is_some() {
        vec!["tests/graphql_routes/storefront.rs".to_string()]
    } else {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn storefront_bindings_cover_known_handlers_in_every_executable_version() {
        let bindings = storefront_registry_bindings();
        for version in StorefrontApiVersion::ALL {
            for operation_type in [OperationType::Query, OperationType::Mutation] {
                for root_name in storefront_graphql::root_field_names(version, operation_type) {
                    if storefront_root_handler(operation_type, &root_name).is_none() {
                        continue;
                    }
                    assert!(bindings.iter().any(|binding| {
                        binding.entry.api_surface == ApiSurface::Storefront
                            && binding.entry.operation_type == operation_type
                            && binding.entry.name == root_name
                            && binding.entry.implemented
                            && binding.handler.is_some()
                    }));
                }
            }
        }
    }
}

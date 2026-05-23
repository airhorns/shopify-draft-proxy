use crate::graphql::OperationType;

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
    pub name: String,
    pub operation_type: OperationType,
    pub domain: CapabilityDomain,
    pub execution: CapabilityExecution,
    pub implemented: bool,
    pub match_names: Vec<String>,
    pub runtime_tests: Vec<String>,
    pub support_notes: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalDispatchRoot {
    pub name: &'static str,
    pub operation_type: OperationType,
    pub domain: CapabilityDomain,
    pub execution: CapabilityExecution,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationCapability {
    pub domain: CapabilityDomain,
    pub execution: CapabilityExecution,
    pub operation_name: Option<String>,
}

pub fn local_dispatch_roots() -> &'static [LocalDispatchRoot] {
    &LOCAL_DISPATCH_ROOTS
}

pub fn local_dispatch_root(
    operation_type: OperationType,
    domain: CapabilityDomain,
    execution: CapabilityExecution,
    root_field: &str,
) -> Option<&'static LocalDispatchRoot> {
    LOCAL_DISPATCH_ROOTS.iter().find(|root| {
        root.operation_type == operation_type
            && root.domain == domain
            && root.execution == execution
            && root.name == root_field
    })
}

pub fn default_registry() -> Vec<OperationRegistryEntry> {
    vec![
        OperationRegistryEntry {
            name: "product".to_string(),
            operation_type: OperationType::Query,
            domain: CapabilityDomain::Products,
            execution: CapabilityExecution::OverlayRead,
            implemented: true,
            match_names: strings(&["product", "Product"]),
            runtime_tests: strings(&["tests/graphql_routes.rs"]),
            support_notes: None,
        },
        OperationRegistryEntry {
            name: "products".to_string(),
            operation_type: OperationType::Query,
            domain: CapabilityDomain::Products,
            execution: CapabilityExecution::OverlayRead,
            implemented: true,
            match_names: strings(&["products", "Products"]),
            runtime_tests: strings(&["tests/graphql_routes.rs"]),
            support_notes: None,
        },
        OperationRegistryEntry {
            name: "productsCount".to_string(),
            operation_type: OperationType::Query,
            domain: CapabilityDomain::Products,
            execution: CapabilityExecution::OverlayRead,
            implemented: true,
            match_names: strings(&["productsCount", "ProductsCount"]),
            runtime_tests: strings(&["tests/graphql_routes.rs"]),
            support_notes: None,
        },
        OperationRegistryEntry {
            name: "productByIdentifier".to_string(),
            operation_type: OperationType::Query,
            domain: CapabilityDomain::Products,
            execution: CapabilityExecution::OverlayRead,
            implemented: true,
            match_names: strings(&["productByIdentifier", "ProductByIdentifier"]),
            runtime_tests: strings(&["tests/graphql_routes.rs"]),
            support_notes: None,
        },
        OperationRegistryEntry {
            name: "productCreate".to_string(),
            operation_type: OperationType::Mutation,
            domain: CapabilityDomain::Products,
            execution: CapabilityExecution::StageLocally,
            implemented: true,
            match_names: strings(&["productCreate", "ProductCreate"]),
            runtime_tests: strings(&["tests/graphql_routes.rs"]),
            support_notes: None,
        },
        OperationRegistryEntry {
            name: "productUpdate".to_string(),
            operation_type: OperationType::Mutation,
            domain: CapabilityDomain::Products,
            execution: CapabilityExecution::StageLocally,
            implemented: true,
            match_names: strings(&["productUpdate", "ProductUpdate"]),
            runtime_tests: strings(&["tests/graphql_routes.rs"]),
            support_notes: None,
        },
        OperationRegistryEntry {
            name: "productDelete".to_string(),
            operation_type: OperationType::Mutation,
            domain: CapabilityDomain::Products,
            execution: CapabilityExecution::StageLocally,
            implemented: true,
            match_names: strings(&["productDelete", "ProductDelete"]),
            runtime_tests: strings(&["tests/graphql_routes.rs"]),
            support_notes: None,
        },
        OperationRegistryEntry {
            name: "productChangeStatus".to_string(),
            operation_type: OperationType::Mutation,
            domain: CapabilityDomain::Products,
            execution: CapabilityExecution::StageLocally,
            implemented: true,
            match_names: strings(&["productChangeStatus", "ProductChangeStatus"]),
            runtime_tests: strings(&["tests/graphql_routes.rs"]),
            support_notes: None,
        },
        OperationRegistryEntry {
            name: "productVariantCreate".to_string(),
            operation_type: OperationType::Mutation,
            domain: CapabilityDomain::Products,
            execution: CapabilityExecution::StageLocally,
            implemented: true,
            match_names: strings(&["productVariantCreate", "ProductVariantCreate"]),
            runtime_tests: strings(&["tests/graphql_routes.rs"]),
            support_notes: None,
        },
        OperationRegistryEntry {
            name: "productVariantUpdate".to_string(),
            operation_type: OperationType::Mutation,
            domain: CapabilityDomain::Products,
            execution: CapabilityExecution::StageLocally,
            implemented: true,
            match_names: strings(&["productVariantUpdate", "ProductVariantUpdate"]),
            runtime_tests: strings(&["tests/graphql_routes.rs"]),
            support_notes: None,
        },
        OperationRegistryEntry {
            name: "productVariantDelete".to_string(),
            operation_type: OperationType::Mutation,
            domain: CapabilityDomain::Products,
            execution: CapabilityExecution::StageLocally,
            implemented: true,
            match_names: strings(&["productVariantDelete", "ProductVariantDelete"]),
            runtime_tests: strings(&["tests/graphql_routes.rs"]),
            support_notes: None,
        },
        OperationRegistryEntry {
            name: "tagsAdd".to_string(),
            operation_type: OperationType::Mutation,
            domain: CapabilityDomain::Products,
            execution: CapabilityExecution::StageLocally,
            implemented: true,
            match_names: strings(&["tagsAdd"]),
            runtime_tests: strings(&["tests/graphql_routes.rs"]),
            support_notes: None,
        },
        OperationRegistryEntry {
            name: "tagsRemove".to_string(),
            operation_type: OperationType::Mutation,
            domain: CapabilityDomain::Products,
            execution: CapabilityExecution::StageLocally,
            implemented: true,
            match_names: strings(&["tagsRemove"]),
            runtime_tests: strings(&["tests/graphql_routes.rs"]),
            support_notes: None,
        },
        saved_search_query("automaticDiscountSavedSearches"),
        saved_search_query("codeDiscountSavedSearches"),
        saved_search_query("collectionSavedSearches"),
        saved_search_query("customerSavedSearches"),
        saved_search_query("discountRedeemCodeSavedSearches"),
        saved_search_query("draftOrderSavedSearches"),
        saved_search_query("fileSavedSearches"),
        saved_search_query("orderSavedSearches"),
        saved_search_query("productSavedSearches"),
        OperationRegistryEntry {
            name: "savedSearchCreate".to_string(),
            operation_type: OperationType::Mutation,
            domain: CapabilityDomain::SavedSearches,
            execution: CapabilityExecution::StageLocally,
            implemented: true,
            match_names: strings(&["savedSearchCreate", "SavedSearchCreate"]),
            runtime_tests: strings(&["tests/graphql_routes.rs"]),
            support_notes: None,
        },
        OperationRegistryEntry {
            name: "app".to_string(),
            operation_type: OperationType::Query,
            domain: CapabilityDomain::Apps,
            execution: CapabilityExecution::OverlayRead,
            implemented: false,
            match_names: strings(&["app", "App"]),
            runtime_tests: Vec::new(),
            support_notes: None,
        },
    ]
}

pub fn implemented_entries(registry: &[OperationRegistryEntry]) -> Vec<&OperationRegistryEntry> {
    registry.iter().filter(|entry| entry.implemented).collect()
}

pub fn find_entry<'a>(
    registry: &'a [OperationRegistryEntry],
    operation_type: OperationType,
    names: &[Option<&str>],
) -> Option<&'a OperationRegistryEntry> {
    names
        .iter()
        .filter_map(|name| name.and_then(nonempty))
        .find_map(|candidate| {
            registry.iter().find(|entry| {
                entry.operation_type == operation_type
                    && entry.match_names.iter().any(|name| name == candidate)
            })
        })
}

pub fn operation_capability(
    registry: &[OperationRegistryEntry],
    operation_type: OperationType,
    root_field: Option<&str>,
) -> OperationCapability {
    let names = [root_field];
    match find_entry(registry, operation_type, &names).filter(|entry| entry.implemented) {
        Some(entry) => OperationCapability {
            domain: entry.domain,
            execution: entry.execution,
            operation_name: root_field
                .filter(|name| !name.is_empty())
                .map(str::to_string),
        },
        None => OperationCapability {
            domain: CapabilityDomain::Unknown,
            execution: CapabilityExecution::Passthrough,
            operation_name: root_field
                .filter(|name| !name.is_empty())
                .map(str::to_string),
        },
    }
}

fn saved_search_query(name: &str) -> OperationRegistryEntry {
    OperationRegistryEntry {
        name: name.to_string(),
        operation_type: OperationType::Query,
        domain: CapabilityDomain::SavedSearches,
        execution: CapabilityExecution::OverlayRead,
        implemented: true,
        match_names: match_names_with_pascal(name),
        runtime_tests: strings(&["tests/graphql_routes.rs"]),
        support_notes: None,
    }
}

const LOCAL_DISPATCH_ROOTS: [LocalDispatchRoot; 23] = [
    local_query("product", CapabilityDomain::Products),
    local_query("products", CapabilityDomain::Products),
    local_query("productsCount", CapabilityDomain::Products),
    local_query("productByIdentifier", CapabilityDomain::Products),
    local_mutation("productCreate", CapabilityDomain::Products),
    local_mutation("productUpdate", CapabilityDomain::Products),
    local_mutation("productDelete", CapabilityDomain::Products),
    local_mutation("productChangeStatus", CapabilityDomain::Products),
    local_mutation("productVariantCreate", CapabilityDomain::Products),
    local_mutation("productVariantUpdate", CapabilityDomain::Products),
    local_mutation("productVariantDelete", CapabilityDomain::Products),
    local_mutation("tagsAdd", CapabilityDomain::Products),
    local_mutation("tagsRemove", CapabilityDomain::Products),
    local_query(
        "automaticDiscountSavedSearches",
        CapabilityDomain::SavedSearches,
    ),
    local_query("codeDiscountSavedSearches", CapabilityDomain::SavedSearches),
    local_query("collectionSavedSearches", CapabilityDomain::SavedSearches),
    local_query("customerSavedSearches", CapabilityDomain::SavedSearches),
    local_query(
        "discountRedeemCodeSavedSearches",
        CapabilityDomain::SavedSearches,
    ),
    local_query("draftOrderSavedSearches", CapabilityDomain::SavedSearches),
    local_query("fileSavedSearches", CapabilityDomain::SavedSearches),
    local_query("orderSavedSearches", CapabilityDomain::SavedSearches),
    local_query("productSavedSearches", CapabilityDomain::SavedSearches),
    local_mutation("savedSearchCreate", CapabilityDomain::SavedSearches),
];

const fn local_query(name: &'static str, domain: CapabilityDomain) -> LocalDispatchRoot {
    LocalDispatchRoot {
        name,
        operation_type: OperationType::Query,
        domain,
        execution: CapabilityExecution::OverlayRead,
    }
}

const fn local_mutation(name: &'static str, domain: CapabilityDomain) -> LocalDispatchRoot {
    LocalDispatchRoot {
        name,
        operation_type: OperationType::Mutation,
        domain,
        execution: CapabilityExecution::StageLocally,
    }
}

fn strings(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_string()).collect()
}

fn match_names_with_pascal(name: &str) -> Vec<String> {
    let mut pascal = name.to_string();
    if let Some(first) = pascal.get_mut(0..1) {
        first.make_ascii_uppercase();
    }
    vec![name.to_string(), pascal]
}

fn nonempty(value: &str) -> Option<&str> {
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}

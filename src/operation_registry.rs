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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapabilityExecution {
    OverlayRead,
    StageLocally,
    Passthrough,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationCapability {
    pub domain: CapabilityDomain,
    pub execution: CapabilityExecution,
    pub operation_name: Option<String>,
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
            runtime_tests: strings(&["test/parity_test.gleam"]),
            support_notes: None,
        },
        OperationRegistryEntry {
            name: "products".to_string(),
            operation_type: OperationType::Query,
            domain: CapabilityDomain::Products,
            execution: CapabilityExecution::OverlayRead,
            implemented: true,
            match_names: strings(&["products", "Products"]),
            runtime_tests: strings(&["test/parity_test.gleam"]),
            support_notes: None,
        },
        OperationRegistryEntry {
            name: "productsCount".to_string(),
            operation_type: OperationType::Query,
            domain: CapabilityDomain::Products,
            execution: CapabilityExecution::OverlayRead,
            implemented: true,
            match_names: strings(&["productsCount", "ProductsCount"]),
            runtime_tests: strings(&["test/parity_test.gleam"]),
            support_notes: None,
        },
        OperationRegistryEntry {
            name: "productCreate".to_string(),
            operation_type: OperationType::Mutation,
            domain: CapabilityDomain::Products,
            execution: CapabilityExecution::StageLocally,
            implemented: true,
            match_names: strings(&["productCreate", "ProductCreate"]),
            runtime_tests: strings(&["test/parity_test.gleam"]),
            support_notes: None,
        },
        OperationRegistryEntry {
            name: "productUpdate".to_string(),
            operation_type: OperationType::Mutation,
            domain: CapabilityDomain::Products,
            execution: CapabilityExecution::StageLocally,
            implemented: true,
            match_names: strings(&["productUpdate", "ProductUpdate"]),
            runtime_tests: strings(&["test/parity_test.gleam"]),
            support_notes: None,
        },
        OperationRegistryEntry {
            name: "productDelete".to_string(),
            operation_type: OperationType::Mutation,
            domain: CapabilityDomain::Products,
            execution: CapabilityExecution::StageLocally,
            implemented: true,
            match_names: strings(&["productDelete", "ProductDelete"]),
            runtime_tests: strings(&["test/parity_test.gleam"]),
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

fn strings(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_string()).collect()
}

fn nonempty(value: &str) -> Option<&str> {
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}

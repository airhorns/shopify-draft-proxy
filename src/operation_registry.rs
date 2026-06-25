use crate::graphql::OperationType;
use serde_json::{json, Map, Value};

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationCapability {
    pub domain: CapabilityDomain,
    pub execution: CapabilityExecution,
    pub operation_name: Option<String>,
}

pub fn default_registry() -> Vec<OperationRegistryEntry> {
    crate::operation_registry_data::default_registry_entries()
}

pub fn default_registry_json_value() -> Value {
    registry_json_value(&default_registry())
}

pub fn registry_json_value(registry: &[OperationRegistryEntry]) -> Value {
    Value::Array(registry.iter().map(registry_entry_json_value).collect())
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
    let Some(field) = root_field.filter(|name| !name.is_empty()) else {
        return OperationCapability {
            domain: CapabilityDomain::Unknown,
            execution: CapabilityExecution::Passthrough,
            operation_name: None,
        };
    };
    let operation_name = Some(field.to_string());
    let local_entry = find_entry(registry, operation_type, &[Some(field)])
        .filter(|entry| entry.implemented && entry.name == field);

    match local_entry {
        Some(entry) => OperationCapability {
            domain: entry.domain,
            execution: entry.execution,
            operation_name,
        },
        None => OperationCapability {
            domain: CapabilityDomain::Unknown,
            execution: CapabilityExecution::Passthrough,
            operation_name,
        },
    }
}

fn registry_entry_json_value(entry: &OperationRegistryEntry) -> Value {
    let mut object = Map::new();
    object.insert("name".to_string(), json!(entry.name));
    object.insert("type".to_string(), json!(entry.operation_type.keyword()));
    object.insert("domain".to_string(), json!(entry.domain.registry_name()));
    object.insert(
        "execution".to_string(),
        json!(entry.execution.registry_name()),
    );
    object.insert("implemented".to_string(), json!(entry.implemented));
    object.insert("matchNames".to_string(), json!(entry.match_names));
    object.insert("runtimeTests".to_string(), json!(entry.runtime_tests));
    if let Some(support_notes) = &entry.support_notes {
        object.insert("supportNotes".to_string(), json!(support_notes));
    }
    Value::Object(object)
}

fn nonempty(value: &str) -> Option<&str> {
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}

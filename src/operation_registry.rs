use crate::graphql::OperationType;
use serde_json::{json, Map, Value};
use std::collections::BTreeSet;
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
    pub match_names: Vec<String>,
    pub runtime_tests: Vec<String>,
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
    let mut registry = crate::operation_registry_data::default_registry_entries();
    registry.extend(storefront_registry_entries());
    debug_assert_default_registry_local_routing_contract(&registry);
    registry
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
            && entry.match_names.iter().any(|name| name == field)
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
    object.insert("matchNames".to_string(), json!(entry.match_names));
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

fn storefront_registry_entries() -> Vec<OperationRegistryEntry> {
    let inventory = storefront_root_inventory_json("2026-04");
    let mut entries = Vec::new();
    let mut seen = BTreeSet::new();
    entries.extend(storefront_registry_entries_for_roots(
        inventory
            .pointer("/roots/query")
            .and_then(Value::as_array)
            .into_iter()
            .flatten(),
        OperationType::Query,
        &mut seen,
    ));
    entries.extend(storefront_registry_entries_for_roots(
        inventory
            .pointer("/roots/mutation")
            .and_then(Value::as_array)
            .into_iter()
            .flatten(),
        OperationType::Mutation,
        &mut seen,
    ));
    entries
}

fn storefront_registry_entries_for_roots<'a>(
    roots: impl Iterator<Item = &'a Value>,
    operation_type: OperationType,
    seen: &mut BTreeSet<(&'static str, String)>,
) -> Vec<OperationRegistryEntry> {
    roots
        .filter_map(|root| root.get("name").and_then(Value::as_str))
        .filter(|name| seen.insert((operation_type.keyword(), (*name).to_string())))
        .map(|name| OperationRegistryEntry {
            api_surface: ApiSurface::Storefront,
            name: name.to_string(),
            operation_type,
            domain: CapabilityDomain::Storefront,
            implemented: false,
            match_names: default_match_names(name),
            runtime_tests: Vec::new(),
        })
        .collect()
}

fn storefront_root_inventory_json(api_version: &str) -> Value {
    let raw = match api_version {
        "2026-04" => {
            include_str!("../config/storefront-graphql/2026-04/root-inventory.json")
        }
        _ => panic!(
            "unsupported Storefront API version has no captured root inventory: {api_version}"
        ),
    };
    serde_json::from_str(raw).expect("checked-in Storefront root inventory should be valid JSON")
}

fn default_match_names(name: &str) -> Vec<String> {
    let mut chars = name.chars();
    let mut capitalized = String::with_capacity(name.len());
    if let Some(first) = chars.next() {
        capitalized.extend(first.to_uppercase());
        capitalized.push_str(chars.as_str());
    }
    vec![name.to_string(), capitalized]
}

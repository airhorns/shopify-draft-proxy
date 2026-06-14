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
    let names = [root_field];
    // Capability routing keys on whether a concrete local dispatch root exists for the
    // resolved registry entry, NOT on `entry.implemented`. The `implemented` flag describes
    // the much larger set of operations the proxy handles locally (including the document-gated
    // special-case handlers earlier in dispatch), while only `LOCAL_DISPATCH_ROOTS` reach the
    // uniform table dispatch below. Keeping these separate means broadening `implemented` can
    // never route an operation into the table-dispatch `501` arms: anything without a dispatch
    // root falls through to Unknown/Passthrough (and thus to upstream passthrough), never an error.
    let dispatch_root = root_field
        .filter(|name| !name.is_empty())
        .and_then(|field| {
            find_entry(registry, operation_type, &names).and_then(|entry| {
                local_dispatch_root(operation_type, entry.domain, entry.execution, field)
            })
        });
    match dispatch_root {
        Some(root) => OperationCapability {
            domain: root.domain,
            execution: root.execution,
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

fn registry_entry_json_value(entry: &OperationRegistryEntry) -> Value {
    let mut object = Map::new();
    object.insert("name".to_string(), json!(entry.name));
    object.insert(
        "type".to_string(),
        json!(operation_type_registry_name(entry.operation_type)),
    );
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

const LOCAL_DISPATCH_ROOTS: [LocalDispatchRoot; 76] = [
    local_query("product", CapabilityDomain::Products),
    local_query("products", CapabilityDomain::Products),
    local_query("productsCount", CapabilityDomain::Products),
    local_query("productByIdentifier", CapabilityDomain::Products),
    local_query("productVariant", CapabilityDomain::Products),
    local_query("inventoryItem", CapabilityDomain::Products),
    local_query("inventoryItems", CapabilityDomain::Products),
    local_query("inventoryLevel", CapabilityDomain::Products),
    local_query("inventoryProperties", CapabilityDomain::Products),
    local_query("inventoryTransfer", CapabilityDomain::Products),
    local_query("inventoryTransfers", CapabilityDomain::Products),
    local_mutation("productCreate", CapabilityDomain::Products),
    local_mutation("productUpdate", CapabilityDomain::Products),
    local_mutation("productDelete", CapabilityDomain::Products),
    local_mutation("productChangeStatus", CapabilityDomain::Products),
    local_mutation("productVariantCreate", CapabilityDomain::Products),
    local_mutation("productVariantUpdate", CapabilityDomain::Products),
    local_mutation("productVariantDelete", CapabilityDomain::Products),
    local_mutation("tagsAdd", CapabilityDomain::Products),
    local_mutation("tagsRemove", CapabilityDomain::Products),
    local_mutation("inventoryAdjustQuantities", CapabilityDomain::Products),
    local_mutation("inventorySetQuantities", CapabilityDomain::Products),
    local_mutation("inventoryMoveQuantities", CapabilityDomain::Products),
    local_mutation("inventoryTransferCreate", CapabilityDomain::Products),
    local_mutation(
        "inventoryTransferCreateAsReadyToShip",
        CapabilityDomain::Products,
    ),
    local_mutation(
        "inventoryTransferMarkAsReadyToShip",
        CapabilityDomain::Products,
    ),
    local_mutation("inventoryTransferSetItems", CapabilityDomain::Products),
    local_mutation("inventoryTransferRemoveItems", CapabilityDomain::Products),
    local_mutation("inventoryTransferCancel", CapabilityDomain::Products),
    local_mutation("inventoryTransferDelete", CapabilityDomain::Products),
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
    local_query("bulkOperation", CapabilityDomain::BulkOperations),
    local_query("bulkOperations", CapabilityDomain::BulkOperations),
    local_query("currentBulkOperation", CapabilityDomain::BulkOperations),
    local_query("files", CapabilityDomain::Media),
    local_mutation("stagedUploadsCreate", CapabilityDomain::Media),
    local_mutation("fileAcknowledgeUpdateFailed", CapabilityDomain::Media),
    local_mutation("fileCreate", CapabilityDomain::Media),
    local_mutation("fileUpdate", CapabilityDomain::Media),
    local_mutation("fileDelete", CapabilityDomain::Media),
    local_query("metaobject", CapabilityDomain::Metaobjects),
    local_query("metaobjectByHandle", CapabilityDomain::Metaobjects),
    local_query("metaobjects", CapabilityDomain::Metaobjects),
    local_query("metaobjectDefinition", CapabilityDomain::Metaobjects),
    local_query("metaobjectDefinitionByType", CapabilityDomain::Metaobjects),
    local_query("metaobjectDefinitions", CapabilityDomain::Metaobjects),
    local_mutation("metaobjectCreate", CapabilityDomain::Metaobjects),
    local_mutation("metaobjectDelete", CapabilityDomain::Metaobjects),
    local_mutation("metaobjectDefinitionCreate", CapabilityDomain::Metaobjects),
    local_mutation("metaobjectDefinitionDelete", CapabilityDomain::Metaobjects),
    local_mutation("metafieldDefinitionCreate", CapabilityDomain::Metafields),
    local_mutation("metafieldDefinitionUpdate", CapabilityDomain::Metafields),
    local_mutation("metafieldDefinitionDelete", CapabilityDomain::Metafields),
    local_mutation(
        "standardMetafieldDefinitionEnable",
        CapabilityDomain::Metafields,
    ),
    local_query("giftCard", CapabilityDomain::GiftCards),
    local_query("giftCards", CapabilityDomain::GiftCards),
    local_query("giftCardsCount", CapabilityDomain::GiftCards),
    local_query("giftCardConfiguration", CapabilityDomain::GiftCards),
    local_mutation("giftCardCreate", CapabilityDomain::GiftCards),
    local_mutation("giftCardUpdate", CapabilityDomain::GiftCards),
    local_mutation("giftCardCredit", CapabilityDomain::GiftCards),
    local_mutation("giftCardDebit", CapabilityDomain::GiftCards),
    local_mutation("giftCardDeactivate", CapabilityDomain::GiftCards),
    local_mutation(
        "giftCardSendNotificationToCustomer",
        CapabilityDomain::GiftCards,
    ),
    local_mutation(
        "giftCardSendNotificationToRecipient",
        CapabilityDomain::GiftCards,
    ),
    local_mutation("locationAdd", CapabilityDomain::StoreProperties),
    local_mutation("locationActivate", CapabilityDomain::StoreProperties),
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

fn nonempty(value: &str) -> Option<&str> {
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}

fn operation_type_registry_name(operation_type: OperationType) -> &'static str {
    match operation_type {
        OperationType::Query => "query",
        OperationType::Mutation => "mutation",
        OperationType::Subscription => "subscription",
    }
}

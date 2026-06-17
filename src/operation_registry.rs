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
    LOCAL_DISPATCH_ROOTS
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
    // Capability routing keys on the explicit local dispatch table. The `implemented`
    // flag is kept aligned with this table by tests so local support cannot drift into
    // hidden special-case routing.
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

const LOCAL_DISPATCH_ROOTS: &[LocalDispatchRoot] = &[
    local_query("product", CapabilityDomain::Products),
    local_query("productByIdentifier", CapabilityDomain::Products),
    local_query("productOperation", CapabilityDomain::Products),
    local_query("products", CapabilityDomain::Products),
    local_query("productsCount", CapabilityDomain::Products),
    local_query("node", CapabilityDomain::AdminPlatform),
    local_query("nodes", CapabilityDomain::AdminPlatform),
    local_query("job", CapabilityDomain::AdminPlatform),
    local_query("domain", CapabilityDomain::AdminPlatform),
    local_query("backupRegion", CapabilityDomain::AdminPlatform),
    local_mutation("backupRegionUpdate", CapabilityDomain::AdminPlatform),
    local_mutation("flowGenerateSignature", CapabilityDomain::AdminPlatform),
    local_mutation("flowTriggerReceive", CapabilityDomain::AdminPlatform),
    local_query("currentAppInstallation", CapabilityDomain::Apps),
    local_mutation("appPurchaseOneTimeCreate", CapabilityDomain::Apps),
    local_mutation("appSubscriptionCreate", CapabilityDomain::Apps),
    local_mutation("appSubscriptionCancel", CapabilityDomain::Apps),
    local_mutation("appSubscriptionLineItemUpdate", CapabilityDomain::Apps),
    local_mutation("appSubscriptionTrialExtend", CapabilityDomain::Apps),
    local_mutation("appUsageRecordCreate", CapabilityDomain::Apps),
    local_mutation("appRevokeAccessScopes", CapabilityDomain::Apps),
    local_mutation("appUninstall", CapabilityDomain::Apps),
    local_mutation("delegateAccessTokenCreate", CapabilityDomain::Apps),
    local_mutation("delegateAccessTokenDestroy", CapabilityDomain::Apps),
    local_query("productVariant", CapabilityDomain::Products),
    local_query("inventoryItem", CapabilityDomain::Products),
    local_query("inventoryItems", CapabilityDomain::Products),
    local_query("inventoryLevel", CapabilityDomain::Products),
    local_query("inventoryProperties", CapabilityDomain::Products),
    local_query("inventoryTransfer", CapabilityDomain::Products),
    local_query("inventoryTransfers", CapabilityDomain::Products),
    local_query("collection", CapabilityDomain::StoreProperties),
    local_query("location", CapabilityDomain::StoreProperties),
    local_query("locationByIdentifier", CapabilityDomain::StoreProperties),
    local_query("company", CapabilityDomain::B2b),
    local_query("companyLocation", CapabilityDomain::B2b),
    local_query("cashTrackingSession", CapabilityDomain::Payments),
    local_query("cashTrackingSessions", CapabilityDomain::Payments),
    local_query("customerPaymentMethod", CapabilityDomain::Payments),
    local_query("paymentCustomization", CapabilityDomain::Payments),
    local_query("paymentCustomizations", CapabilityDomain::Payments),
    local_query("validation", CapabilityDomain::Functions),
    local_query("validations", CapabilityDomain::Functions),
    local_query("cartTransforms", CapabilityDomain::Functions),
    local_query("shopifyFunction", CapabilityDomain::Functions),
    local_query("shopifyFunctions", CapabilityDomain::Functions),
    local_query("pointOfSaleDevice", CapabilityDomain::Payments),
    local_query("dispute", CapabilityDomain::Payments),
    local_query("disputes", CapabilityDomain::Payments),
    local_query("disputeEvidence", CapabilityDomain::Payments),
    local_query("shopPayPaymentRequestReceipt", CapabilityDomain::Payments),
    local_query("shopPayPaymentRequestReceipts", CapabilityDomain::Payments),
    local_query("shopifyPaymentsAccount", CapabilityDomain::Payments),
    local_query("customer", CapabilityDomain::Customers),
    local_query("event", CapabilityDomain::Events),
    local_query("events", CapabilityDomain::Events),
    local_query("eventsCount", CapabilityDomain::Events),
    local_query("customers", CapabilityDomain::Customers),
    local_query("customersCount", CapabilityDomain::Customers),
    local_query("customerByIdentifier", CapabilityDomain::Customers),
    local_mutation("customerCreate", CapabilityDomain::Customers),
    local_mutation("customerUpdate", CapabilityDomain::Customers),
    local_mutation("customerDelete", CapabilityDomain::Customers),
    local_mutation("customerMerge", CapabilityDomain::Customers),
    local_mutation("customerSet", CapabilityDomain::Customers),
    local_mutation("companyCreate", CapabilityDomain::B2b),
    local_mutation("companyUpdate", CapabilityDomain::B2b),
    local_mutation("companyLocationCreate", CapabilityDomain::B2b),
    local_mutation("companyLocationUpdate", CapabilityDomain::B2b),
    local_mutation("companyLocationAssignAddress", CapabilityDomain::B2b),
    local_mutation("companyLocationTaxSettingsUpdate", CapabilityDomain::B2b),
    local_mutation("companyAssignCustomerAsContact", CapabilityDomain::B2b),
    local_mutation("companyContactCreate", CapabilityDomain::B2b),
    local_mutation("companyContactDelete", CapabilityDomain::B2b),
    local_mutation("companyContactsDelete", CapabilityDomain::B2b),
    local_mutation("companyContactRemoveFromCompany", CapabilityDomain::B2b),
    local_mutation("companyAssignMainContact", CapabilityDomain::B2b),
    local_mutation("companyRevokeMainContact", CapabilityDomain::B2b),
    local_mutation("companyDelete", CapabilityDomain::B2b),
    local_mutation("companiesDelete", CapabilityDomain::B2b),
    local_mutation("companyAddressDelete", CapabilityDomain::B2b),
    local_mutation("companyLocationsDelete", CapabilityDomain::B2b),
    local_mutation("tagsAdd", CapabilityDomain::Products),
    local_mutation("tagsRemove", CapabilityDomain::Products),
    local_mutation("productCreate", CapabilityDomain::Products),
    local_mutation("productUpdate", CapabilityDomain::Products),
    local_mutation("productDelete", CapabilityDomain::Products),
    local_mutation("productSet", CapabilityDomain::Products),
    local_mutation("productDuplicate", CapabilityDomain::Products),
    local_mutation("productBundleCreate", CapabilityDomain::Products),
    local_mutation("productBundleUpdate", CapabilityDomain::Products),
    local_mutation("productChangeStatus", CapabilityDomain::Products),
    local_mutation("productCreateMedia", CapabilityDomain::Products),
    local_mutation("productUpdateMedia", CapabilityDomain::Products),
    local_mutation("productDeleteMedia", CapabilityDomain::Products),
    local_mutation("productReorderMedia", CapabilityDomain::Products),
    local_mutation("locationAdd", CapabilityDomain::StoreProperties),
    local_mutation("locationEdit", CapabilityDomain::StoreProperties),
    local_mutation("locationActivate", CapabilityDomain::StoreProperties),
    local_mutation("locationDeactivate", CapabilityDomain::StoreProperties),
    local_mutation("publishablePublish", CapabilityDomain::StoreProperties),
    local_mutation(
        "publishablePublishToCurrentChannel",
        CapabilityDomain::StoreProperties,
    ),
    local_mutation("publishableUnpublish", CapabilityDomain::StoreProperties),
    local_mutation(
        "publishableUnpublishToCurrentChannel",
        CapabilityDomain::StoreProperties,
    ),
    local_query("productSavedSearches", CapabilityDomain::SavedSearches),
    local_query("collectionSavedSearches", CapabilityDomain::SavedSearches),
    local_query("customerSavedSearches", CapabilityDomain::SavedSearches),
    local_query("orderSavedSearches", CapabilityDomain::SavedSearches),
    local_query("draftOrderSavedSearches", CapabilityDomain::SavedSearches),
    local_query("fileSavedSearches", CapabilityDomain::SavedSearches),
    local_query("codeDiscountSavedSearches", CapabilityDomain::SavedSearches),
    local_query(
        "automaticDiscountSavedSearches",
        CapabilityDomain::SavedSearches,
    ),
    local_query(
        "discountRedeemCodeSavedSearches",
        CapabilityDomain::SavedSearches,
    ),
    local_mutation("savedSearchCreate", CapabilityDomain::SavedSearches),
    local_mutation("savedSearchUpdate", CapabilityDomain::SavedSearches),
    local_mutation("savedSearchDelete", CapabilityDomain::SavedSearches),
    local_query("theme", CapabilityDomain::OnlineStore),
    local_query("themes", CapabilityDomain::OnlineStore),
    local_query("scriptTag", CapabilityDomain::OnlineStore),
    local_query("scriptTags", CapabilityDomain::OnlineStore),
    local_query("webPixel", CapabilityDomain::OnlineStore),
    local_query("serverPixel", CapabilityDomain::OnlineStore),
    local_query("mobilePlatformApplication", CapabilityDomain::OnlineStore),
    local_query("mobilePlatformApplications", CapabilityDomain::OnlineStore),
    local_mutation("themeCreate", CapabilityDomain::OnlineStore),
    local_mutation("themeUpdate", CapabilityDomain::OnlineStore),
    local_mutation("themeDelete", CapabilityDomain::OnlineStore),
    local_mutation("themePublish", CapabilityDomain::OnlineStore),
    local_mutation("themeFilesCopy", CapabilityDomain::OnlineStore),
    local_mutation("themeFilesUpsert", CapabilityDomain::OnlineStore),
    local_mutation("themeFilesDelete", CapabilityDomain::OnlineStore),
    local_mutation("scriptTagCreate", CapabilityDomain::OnlineStore),
    local_mutation("scriptTagUpdate", CapabilityDomain::OnlineStore),
    local_mutation("scriptTagDelete", CapabilityDomain::OnlineStore),
    local_mutation("webPixelCreate", CapabilityDomain::OnlineStore),
    local_mutation("webPixelUpdate", CapabilityDomain::OnlineStore),
    local_mutation("serverPixelCreate", CapabilityDomain::OnlineStore),
    local_mutation(
        "eventBridgeServerPixelUpdate",
        CapabilityDomain::OnlineStore,
    ),
    local_mutation("pubSubServerPixelUpdate", CapabilityDomain::OnlineStore),
    local_mutation("storefrontAccessTokenCreate", CapabilityDomain::OnlineStore),
    local_mutation(
        "mobilePlatformApplicationCreate",
        CapabilityDomain::OnlineStore,
    ),
    local_mutation(
        "mobilePlatformApplicationUpdate",
        CapabilityDomain::OnlineStore,
    ),
    local_query("bulkOperation", CapabilityDomain::BulkOperations),
    local_query("bulkOperations", CapabilityDomain::BulkOperations),
    local_query("currentBulkOperation", CapabilityDomain::BulkOperations),
    local_mutation("bulkOperationRunQuery", CapabilityDomain::BulkOperations),
    local_mutation("bulkOperationRunMutation", CapabilityDomain::BulkOperations),
    local_mutation("bulkOperationCancel", CapabilityDomain::BulkOperations),
    local_mutation("productVariantsBulkCreate", CapabilityDomain::Products),
    local_mutation("productVariantsBulkUpdate", CapabilityDomain::Products),
    local_mutation("productVariantsBulkDelete", CapabilityDomain::Products),
    local_mutation("productVariantsBulkReorder", CapabilityDomain::Products),
    local_mutation("collectionCreate", CapabilityDomain::Products),
    local_mutation("collectionUpdate", CapabilityDomain::Products),
    local_mutation("collectionDelete", CapabilityDomain::Products),
    local_mutation("collectionAddProducts", CapabilityDomain::Products),
    local_mutation("collectionAddProductsV2", CapabilityDomain::Products),
    local_mutation("collectionRemoveProducts", CapabilityDomain::Products),
    local_mutation("collectionReorderProducts", CapabilityDomain::Products),
    local_mutation("productVariantCreate", CapabilityDomain::Products),
    local_mutation("productVariantUpdate", CapabilityDomain::Products),
    local_mutation("productVariantDelete", CapabilityDomain::Products),
    local_mutation("productOptionsCreate", CapabilityDomain::Products),
    local_mutation("productOptionUpdate", CapabilityDomain::Products),
    local_mutation("productOptionsDelete", CapabilityDomain::Products),
    local_mutation("productOptionsReorder", CapabilityDomain::Products),
    local_mutation("inventoryAdjustQuantities", CapabilityDomain::Products),
    local_mutation("inventorySetQuantities", CapabilityDomain::Products),
    local_mutation("inventoryMoveQuantities", CapabilityDomain::Products),
    local_mutation("inventoryActivate", CapabilityDomain::Products),
    local_mutation("inventoryDeactivate", CapabilityDomain::Products),
    local_mutation("inventoryBulkToggleActivation", CapabilityDomain::Products),
    local_mutation("inventoryItemUpdate", CapabilityDomain::Products),
    local_mutation("inventoryTransferCreate", CapabilityDomain::Products),
    local_mutation(
        "inventoryTransferCreateAsReadyToShip",
        CapabilityDomain::Products,
    ),
    local_mutation("inventoryTransferSetItems", CapabilityDomain::Products),
    local_mutation("inventoryTransferRemoveItems", CapabilityDomain::Products),
    local_mutation(
        "inventoryTransferMarkAsReadyToShip",
        CapabilityDomain::Products,
    ),
    local_mutation("inventoryTransferEdit", CapabilityDomain::Products),
    local_mutation("inventoryTransferDuplicate", CapabilityDomain::Products),
    local_mutation("inventoryTransferCancel", CapabilityDomain::Products),
    local_mutation("inventoryTransferDelete", CapabilityDomain::Products),
    local_mutation("publicationCreate", CapabilityDomain::Products),
    local_mutation("publicationUpdate", CapabilityDomain::Products),
    local_mutation("publicationDelete", CapabilityDomain::Products),
    local_mutation("productFeedCreate", CapabilityDomain::Products),
    local_mutation("productFullSync", CapabilityDomain::Products),
    local_mutation(
        "bulkProductResourceFeedbackCreate",
        CapabilityDomain::Products,
    ),
    local_mutation("shopResourceFeedbackCreate", CapabilityDomain::Products),
    local_mutation("metafieldsSet", CapabilityDomain::Products),
    local_mutation("metafieldsDelete", CapabilityDomain::Products),
    local_query("sellingPlanGroup", CapabilityDomain::Products),
    local_query("sellingPlanGroups", CapabilityDomain::Products),
    local_mutation("sellingPlanGroupCreate", CapabilityDomain::Products),
    local_mutation("sellingPlanGroupUpdate", CapabilityDomain::Products),
    local_mutation("sellingPlanGroupDelete", CapabilityDomain::Products),
    local_mutation("sellingPlanGroupAddProducts", CapabilityDomain::Products),
    local_mutation("sellingPlanGroupRemoveProducts", CapabilityDomain::Products),
    local_mutation(
        "sellingPlanGroupAddProductVariants",
        CapabilityDomain::Products,
    ),
    local_mutation(
        "sellingPlanGroupRemoveProductVariants",
        CapabilityDomain::Products,
    ),
    local_mutation("productJoinSellingPlanGroups", CapabilityDomain::Products),
    local_mutation("productLeaveSellingPlanGroups", CapabilityDomain::Products),
    local_mutation(
        "productVariantJoinSellingPlanGroups",
        CapabilityDomain::Products,
    ),
    local_mutation(
        "productVariantLeaveSellingPlanGroups",
        CapabilityDomain::Products,
    ),
    local_query("metafieldDefinition", CapabilityDomain::Metafields),
    local_query("metafieldDefinitions", CapabilityDomain::Metafields),
    local_mutation("metafieldDefinitionCreate", CapabilityDomain::Metafields),
    local_mutation("metafieldDefinitionUpdate", CapabilityDomain::Metafields),
    local_mutation("metafieldDefinitionDelete", CapabilityDomain::Metafields),
    local_mutation("metafieldDefinitionPin", CapabilityDomain::Metafields),
    local_mutation("metafieldDefinitionUnpin", CapabilityDomain::Metafields),
    local_mutation(
        "standardMetafieldDefinitionEnable",
        CapabilityDomain::Metafields,
    ),
    local_query("metaobject", CapabilityDomain::Metaobjects),
    local_query("metaobjectByHandle", CapabilityDomain::Metaobjects),
    local_query("metaobjects", CapabilityDomain::Metaobjects),
    local_query("metaobjectDefinition", CapabilityDomain::Metaobjects),
    local_query("metaobjectDefinitionByType", CapabilityDomain::Metaobjects),
    local_query("metaobjectDefinitions", CapabilityDomain::Metaobjects),
    local_mutation("metaobjectCreate", CapabilityDomain::Metaobjects),
    local_mutation("metaobjectUpdate", CapabilityDomain::Metaobjects),
    local_mutation("metaobjectUpsert", CapabilityDomain::Metaobjects),
    local_mutation("metaobjectDelete", CapabilityDomain::Metaobjects),
    local_mutation("metaobjectDefinitionCreate", CapabilityDomain::Metaobjects),
    local_mutation("metaobjectDefinitionUpdate", CapabilityDomain::Metaobjects),
    local_mutation("metaobjectDefinitionDelete", CapabilityDomain::Metaobjects),
    local_query("order", CapabilityDomain::Orders),
    local_mutation("paymentCustomizationActivation", CapabilityDomain::Payments),
    local_mutation("paymentCustomizationCreate", CapabilityDomain::Payments),
    local_mutation("paymentCustomizationDelete", CapabilityDomain::Payments),
    local_mutation("paymentCustomizationUpdate", CapabilityDomain::Payments),
    local_mutation(
        "customerPaymentMethodCreateFromDuplicationData",
        CapabilityDomain::Payments,
    ),
    local_mutation(
        "customerPaymentMethodCreditCardCreate",
        CapabilityDomain::Payments,
    ),
    local_mutation(
        "customerPaymentMethodCreditCardUpdate",
        CapabilityDomain::Payments,
    ),
    local_mutation(
        "customerPaymentMethodGetDuplicationData",
        CapabilityDomain::Payments,
    ),
    local_mutation(
        "customerPaymentMethodGetUpdateUrl",
        CapabilityDomain::Payments,
    ),
    local_mutation(
        "customerPaymentMethodPaypalBillingAgreementCreate",
        CapabilityDomain::Payments,
    ),
    local_mutation(
        "customerPaymentMethodPaypalBillingAgreementUpdate",
        CapabilityDomain::Payments,
    ),
    local_mutation(
        "customerPaymentMethodRemoteCreate",
        CapabilityDomain::Payments,
    ),
    local_mutation("customerPaymentMethodRevoke", CapabilityDomain::Payments),
    local_mutation("orderCapture", CapabilityDomain::Payments),
    local_mutation("orderCreateMandatePayment", CapabilityDomain::Payments),
    local_mutation("paymentReminderSend", CapabilityDomain::Payments),
    local_mutation("paymentTermsCreate", CapabilityDomain::Payments),
    local_mutation("paymentTermsDelete", CapabilityDomain::Payments),
    local_mutation("paymentTermsUpdate", CapabilityDomain::Payments),
    local_mutation("transactionVoid", CapabilityDomain::Payments),
    local_mutation("validationCreate", CapabilityDomain::Functions),
    local_mutation("validationUpdate", CapabilityDomain::Functions),
    local_mutation("validationDelete", CapabilityDomain::Functions),
    local_mutation("cartTransformCreate", CapabilityDomain::Functions),
    local_mutation("cartTransformDelete", CapabilityDomain::Functions),
    local_mutation("taxAppConfigure", CapabilityDomain::Functions),
    local_query("fulfillmentOrder", CapabilityDomain::ShippingFulfillments),
    local_mutation(
        "fulfillmentOrderMove",
        CapabilityDomain::ShippingFulfillments,
    ),
    local_mutation(
        "fulfillmentOrderOpen",
        CapabilityDomain::ShippingFulfillments,
    ),
    local_mutation(
        "fulfillmentOrderReportProgress",
        CapabilityDomain::ShippingFulfillments,
    ),
    local_mutation(
        "fulfillmentOrdersSetFulfillmentDeadline",
        CapabilityDomain::ShippingFulfillments,
    ),
    local_query("fulfillmentService", CapabilityDomain::ShippingFulfillments),
    local_mutation(
        "fulfillmentServiceCreate",
        CapabilityDomain::ShippingFulfillments,
    ),
    local_mutation(
        "fulfillmentServiceDelete",
        CapabilityDomain::ShippingFulfillments,
    ),
    local_mutation(
        "fulfillmentServiceUpdate",
        CapabilityDomain::ShippingFulfillments,
    ),
    local_query("carrierService", CapabilityDomain::ShippingFulfillments),
    local_query("carrierServices", CapabilityDomain::ShippingFulfillments),
    local_mutation(
        "carrierServiceCreate",
        CapabilityDomain::ShippingFulfillments,
    ),
    local_mutation(
        "carrierServiceDelete",
        CapabilityDomain::ShippingFulfillments,
    ),
    local_mutation(
        "carrierServiceUpdate",
        CapabilityDomain::ShippingFulfillments,
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
    local_query(
        "deliveryPromiseSettings",
        CapabilityDomain::ShippingFulfillments,
    ),
    local_query("deliverySettings", CapabilityDomain::ShippingFulfillments),
    local_mutation(
        "shippingPackageDelete",
        CapabilityDomain::ShippingFulfillments,
    ),
    local_mutation(
        "shippingPackageMakeDefault",
        CapabilityDomain::ShippingFulfillments,
    ),
    local_mutation(
        "shippingPackageUpdate",
        CapabilityDomain::ShippingFulfillments,
    ),
    local_mutation("orderCreate", CapabilityDomain::Orders),
    local_mutation("orderCancel", CapabilityDomain::Orders),
    local_mutation("orderCustomerSet", CapabilityDomain::Orders),
    local_mutation("orderCustomerRemove", CapabilityDomain::Orders),
    local_query("orders", CapabilityDomain::Orders),
    local_query("ordersCount", CapabilityDomain::Orders),
    local_query("abandonment", CapabilityDomain::Orders),
    local_query("return", CapabilityDomain::Orders),
    local_query("draftOrder", CapabilityDomain::Orders),
    local_query("reverseDelivery", CapabilityDomain::ShippingFulfillments),
    local_query(
        "reverseFulfillmentOrder",
        CapabilityDomain::ShippingFulfillments,
    ),
    local_mutation(
        "abandonmentUpdateActivitiesDeliveryStatuses",
        CapabilityDomain::Orders,
    ),
    local_mutation("draftOrderCreate", CapabilityDomain::Orders),
    local_mutation("draftOrderComplete", CapabilityDomain::Orders),
    local_mutation("draftOrderInvoiceSend", CapabilityDomain::Orders),
    local_mutation("draftOrderBulkAddTags", CapabilityDomain::Orders),
    local_mutation("draftOrderBulkRemoveTags", CapabilityDomain::Orders),
    local_mutation("orderEditBegin", CapabilityDomain::Orders),
    local_mutation("orderEditCommit", CapabilityDomain::Orders),
    local_mutation("orderMarkAsPaid", CapabilityDomain::Orders),
    local_mutation("refundCreate", CapabilityDomain::Orders),
    local_mutation("returnCreate", CapabilityDomain::Orders),
    local_mutation("returnRequest", CapabilityDomain::Orders),
    local_mutation("returnApproveRequest", CapabilityDomain::Orders),
    local_mutation("returnDeclineRequest", CapabilityDomain::Orders),
    local_mutation("returnCancel", CapabilityDomain::Orders),
    local_mutation("returnClose", CapabilityDomain::Orders),
    local_mutation("returnReopen", CapabilityDomain::Orders),
    local_mutation("removeFromReturn", CapabilityDomain::Orders),
    local_mutation("returnProcess", CapabilityDomain::Orders),
    local_mutation(
        "reverseDeliveryCreateWithShipping",
        CapabilityDomain::ShippingFulfillments,
    ),
    local_mutation(
        "reverseDeliveryShippingUpdate",
        CapabilityDomain::ShippingFulfillments,
    ),
    local_mutation(
        "reverseFulfillmentOrderDispose",
        CapabilityDomain::ShippingFulfillments,
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
    local_query("discountNode", CapabilityDomain::Discounts),
    local_query("discountNodes", CapabilityDomain::Discounts),
    local_query("discountNodesCount", CapabilityDomain::Discounts),
    local_query("discountNode", CapabilityDomain::Discounts),
    local_query("codeDiscountNode", CapabilityDomain::Discounts),
    local_query("codeDiscountNodeByCode", CapabilityDomain::Discounts),
    local_query(
        "discountRedeemCodeBulkCreation",
        CapabilityDomain::Discounts,
    ),
    local_query("automaticDiscountNodes", CapabilityDomain::Discounts),
    local_query("automaticDiscountNode", CapabilityDomain::Discounts),
    local_mutation("discountCodeBasicCreate", CapabilityDomain::Discounts),
    local_mutation("discountCodeBasicUpdate", CapabilityDomain::Discounts),
    local_mutation("discountCodeBxgyCreate", CapabilityDomain::Discounts),
    local_mutation("discountCodeBxgyUpdate", CapabilityDomain::Discounts),
    local_mutation(
        "discountCodeFreeShippingCreate",
        CapabilityDomain::Discounts,
    ),
    local_mutation(
        "discountCodeFreeShippingUpdate",
        CapabilityDomain::Discounts,
    ),
    local_mutation("discountCodeActivate", CapabilityDomain::Discounts),
    local_mutation("discountCodeDeactivate", CapabilityDomain::Discounts),
    local_mutation("discountCodeDelete", CapabilityDomain::Discounts),
    local_mutation("discountRedeemCodeBulkAdd", CapabilityDomain::Discounts),
    local_mutation(
        "discountCodeRedeemCodeBulkDelete",
        CapabilityDomain::Discounts,
    ),
    local_mutation("discountAutomaticBasicCreate", CapabilityDomain::Discounts),
    local_mutation("discountAutomaticBasicUpdate", CapabilityDomain::Discounts),
    local_mutation("discountAutomaticBxgyCreate", CapabilityDomain::Discounts),
    local_mutation("discountAutomaticBxgyUpdate", CapabilityDomain::Discounts),
    local_mutation(
        "discountAutomaticFreeShippingCreate",
        CapabilityDomain::Discounts,
    ),
    local_mutation(
        "discountAutomaticFreeShippingUpdate",
        CapabilityDomain::Discounts,
    ),
    local_mutation("discountAutomaticActivate", CapabilityDomain::Discounts),
    local_mutation("discountAutomaticDeactivate", CapabilityDomain::Discounts),
    local_mutation("discountAutomaticDelete", CapabilityDomain::Discounts),
    local_query("marketingActivities", CapabilityDomain::Marketing),
    local_query("marketingActivity", CapabilityDomain::Marketing),
    local_query("marketingEvent", CapabilityDomain::Marketing),
    local_query("marketingEvents", CapabilityDomain::Marketing),
    local_mutation("marketingActivityCreate", CapabilityDomain::Marketing),
    local_mutation("marketingActivityUpdate", CapabilityDomain::Marketing),
    local_mutation(
        "marketingActivityCreateExternal",
        CapabilityDomain::Marketing,
    ),
    local_mutation(
        "marketingActivityUpdateExternal",
        CapabilityDomain::Marketing,
    ),
    local_mutation(
        "marketingActivityUpsertExternal",
        CapabilityDomain::Marketing,
    ),
    local_mutation(
        "marketingActivityDeleteExternal",
        CapabilityDomain::Marketing,
    ),
    local_mutation(
        "marketingActivitiesDeleteAllExternal",
        CapabilityDomain::Marketing,
    ),
    local_mutation("marketingEngagementCreate", CapabilityDomain::Marketing),
    local_mutation("marketingEngagementsDelete", CapabilityDomain::Marketing),
    local_query("webhookSubscription", CapabilityDomain::Webhooks),
    local_query("webhookSubscriptions", CapabilityDomain::Webhooks),
    local_query("webhookSubscriptionsCount", CapabilityDomain::Webhooks),
    local_mutation(
        "eventBridgeWebhookSubscriptionCreate",
        CapabilityDomain::Webhooks,
    ),
    local_mutation(
        "eventBridgeWebhookSubscriptionUpdate",
        CapabilityDomain::Webhooks,
    ),
    local_mutation(
        "pubSubWebhookSubscriptionCreate",
        CapabilityDomain::Webhooks,
    ),
    local_mutation(
        "pubSubWebhookSubscriptionUpdate",
        CapabilityDomain::Webhooks,
    ),
    local_mutation("webhookSubscriptionCreate", CapabilityDomain::Webhooks),
    local_mutation("webhookSubscriptionUpdate", CapabilityDomain::Webhooks),
    local_mutation("webhookSubscriptionDelete", CapabilityDomain::Webhooks),
    local_query("segment", CapabilityDomain::Segments),
    local_query("segments", CapabilityDomain::Segments),
    local_query("segmentsCount", CapabilityDomain::Segments),
    local_query("customerSegmentMembersQuery", CapabilityDomain::Segments),
    local_mutation(
        "customerSegmentMembersQueryCreate",
        CapabilityDomain::Segments,
    ),
    local_mutation("segmentCreate", CapabilityDomain::Segments),
    local_mutation("segmentUpdate", CapabilityDomain::Segments),
    local_mutation("segmentDelete", CapabilityDomain::Segments),
    local_query("market", CapabilityDomain::Markets),
    local_query("markets", CapabilityDomain::Markets),
    local_query("catalog", CapabilityDomain::Markets),
    local_query("catalogs", CapabilityDomain::Markets),
    local_query("priceList", CapabilityDomain::Markets),
    local_query("priceLists", CapabilityDomain::Markets),
    local_mutation("priceListCreate", CapabilityDomain::Markets),
    local_mutation("priceListUpdate", CapabilityDomain::Markets),
    local_mutation("priceListDelete", CapabilityDomain::Markets),
    local_mutation("priceListFixedPricesAdd", CapabilityDomain::Markets),
    local_mutation("priceListFixedPricesUpdate", CapabilityDomain::Markets),
    local_mutation("priceListFixedPricesDelete", CapabilityDomain::Markets),
    local_mutation(
        "priceListFixedPricesByProductUpdate",
        CapabilityDomain::Markets,
    ),
    local_mutation("quantityPricingByVariantUpdate", CapabilityDomain::Markets),
    local_mutation("quantityRulesAdd", CapabilityDomain::Markets),
    local_mutation("quantityRulesDelete", CapabilityDomain::Markets),
    local_query("webPresences", CapabilityDomain::Markets),
    local_query("marketLocalizableResource", CapabilityDomain::Markets),
    local_query("marketLocalizableResources", CapabilityDomain::Markets),
    local_mutation("marketCreate", CapabilityDomain::Markets),
    local_mutation("marketUpdate", CapabilityDomain::Markets),
    local_mutation("marketDelete", CapabilityDomain::Markets),
    local_mutation("catalogCreate", CapabilityDomain::Markets),
    local_mutation("catalogUpdate", CapabilityDomain::Markets),
    local_mutation("catalogContextUpdate", CapabilityDomain::Markets),
    local_mutation("catalogDelete", CapabilityDomain::Markets),
    local_mutation("webPresenceCreate", CapabilityDomain::Markets),
    local_mutation("webPresenceUpdate", CapabilityDomain::Markets),
    local_mutation("webPresenceDelete", CapabilityDomain::Markets),
    local_mutation("marketLocalizationsRegister", CapabilityDomain::Markets),
    local_mutation("marketLocalizationsRemove", CapabilityDomain::Markets),
    local_query("availableLocales", CapabilityDomain::Localization),
    local_query("shopLocales", CapabilityDomain::Localization),
    local_query("translatableResource", CapabilityDomain::Localization),
    local_query("translatableResources", CapabilityDomain::Localization),
    local_query("translatableResourcesByIds", CapabilityDomain::Localization),
    local_mutation("shopLocaleEnable", CapabilityDomain::Localization),
    local_mutation("shopLocaleUpdate", CapabilityDomain::Localization),
    local_mutation("shopLocaleDisable", CapabilityDomain::Localization),
    local_mutation("translationsRegister", CapabilityDomain::Localization),
    local_mutation("translationsRemove", CapabilityDomain::Localization),
    local_query("files", CapabilityDomain::Media),
    local_mutation("stagedUploadsCreate", CapabilityDomain::Media),
    local_mutation("fileAcknowledgeUpdateFailed", CapabilityDomain::Media),
    local_mutation("fileCreate", CapabilityDomain::Media),
    local_mutation("fileUpdate", CapabilityDomain::Media),
    local_mutation("fileDelete", CapabilityDomain::Media),
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

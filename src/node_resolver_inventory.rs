use serde_json::{json, Value};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeResolverBehavior {
    ProjectLocalRecord,
    ReturnKnownNull,
}

/// Stable executable loader identifier. The public inventory and the runtime
/// node registry share this key, so documentation cannot drift from routing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeLoaderKey {
    App,
    B2b,
    BackupRegion,
    CartTransform,
    Collection,
    Customer,
    CustomerAddress,
    CustomerPaymentMethod,
    CustomerSegmentMembersQuery,
    DeliveryCustomization,
    Discount,
    FulfillmentConstraintRule,
    FulfillmentReturn,
    GiftCard,
    GiftCardTransaction,
    Inventory,
    KnownNull,
    Location,
    Media,
    Metaobject,
    OnlineStore,
    Order,
    Product,
    ProductDeleteOperation,
    ProductFeed,
    ProductOperation,
    ProductVariant,
    Segment,
    ShopifyFunction,
    ShopProperty,
    StoreCredit,
    TaxAppConfiguration,
    Validation,
    Abandonment,
}

impl NodeResolverBehavior {
    fn registry_name(self) -> &'static str {
        match self {
            Self::ProjectLocalRecord => "project-local-record",
            Self::ReturnKnownNull => "return-known-null",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NodeResolverInventoryEntry {
    pub type_name: &'static str,
    pub resolver: &'static str,
    pub behavior: NodeResolverBehavior,
}

impl NodeResolverInventoryEntry {
    pub fn loader_key(self) -> NodeLoaderKey {
        match self.resolver {
            "DraftProxy::app_node_value_by_id" => NodeLoaderKey::App,
            "DraftProxy::b2b_node_value_by_id" => NodeLoaderKey::B2b,
            "DraftProxy::collection_json_with_publication_fields" => NodeLoaderKey::Collection,
            "DraftProxy::customer_node_value_by_id" => NodeLoaderKey::Customer,
            "DraftProxy::customer_address_node_value_by_id" => NodeLoaderKey::CustomerAddress,
            "DraftProxy::customer_payment_method_node_value_by_id" => {
                NodeLoaderKey::CustomerPaymentMethod
            }
            "DraftProxy::discount_node_value_by_id" => NodeLoaderKey::Discount,
            "DraftProxy::fulfillment_return_node_value_by_id" => NodeLoaderKey::FulfillmentReturn,
            "DraftProxy::gift_card_node_value_by_id" => NodeLoaderKey::GiftCard,
            "DraftProxy::gift_card_transaction_node_value_by_id" => {
                NodeLoaderKey::GiftCardTransaction
            }
            "DraftProxy::inventory_node_value_by_id" => NodeLoaderKey::Inventory,
            "DraftProxy::metaobject_node_value_by_id" => NodeLoaderKey::Metaobject,
            "DraftProxy::online_store_content_node_value" => NodeLoaderKey::OnlineStore,
            "DraftProxy::product_json_with_variants_and_currency_context" => NodeLoaderKey::Product,
            "DraftProxy::product_delete_operation_value_by_id" => {
                NodeLoaderKey::ProductDeleteOperation
            }
            "DraftProxy::product_operation_json" => NodeLoaderKey::ProductOperation,
            "DraftProxy::product_tail_feed_node_value" => NodeLoaderKey::ProductFeed,
            "DraftProxy::product_variant_by_id_value" => NodeLoaderKey::ProductVariant,
            "DraftProxy::shop_property_node_value_by_id" => NodeLoaderKey::ShopProperty,
            "DraftProxy::store_credit_node_value_by_id" => NodeLoaderKey::StoreCredit,
            "local_node_value::is_safe_no_data_node_gid" => NodeLoaderKey::KnownNull,
            "DraftProxy::delivery_customization_node_value_by_id" => {
                NodeLoaderKey::DeliveryCustomization
            }
            "DraftProxy::order_node_value_by_id" => NodeLoaderKey::Order,
            "DraftProxy::shopify_function_node_value_by_id" => NodeLoaderKey::ShopifyFunction,
            "DraftProxy::abandonment_node_value_by_id" => NodeLoaderKey::Abandonment,
            "DraftProxy::local_node_value_by_id" => match self.type_name {
                "CartTransform" => NodeLoaderKey::CartTransform,
                "CustomerSegmentMembersQuery" => NodeLoaderKey::CustomerSegmentMembersQuery,
                "FulfillmentConstraintRule" => NodeLoaderKey::FulfillmentConstraintRule,
                "Location" => NodeLoaderKey::Location,
                "MarketRegionCountry" => NodeLoaderKey::BackupRegion,
                "ExternalVideo" | "GenericFile" | "MediaImage" | "Model3d" | "Video" => {
                    NodeLoaderKey::Media
                }
                "Segment" => NodeLoaderKey::Segment,
                "TaxAppConfiguration" => NodeLoaderKey::TaxAppConfiguration,
                "Validation" => NodeLoaderKey::Validation,
                other => panic!("node resolver inventory has no executable loader for {other}"),
            },
            other => panic!("unknown node resolver inventory target {other}"),
        }
    }
}

const DEFAULT_NODE_RESOLVER_INVENTORY: &[NodeResolverInventoryEntry] = &[
    entry(
        "Abandonment",
        "DraftProxy::abandonment_node_value_by_id",
        NodeResolverBehavior::ProjectLocalRecord,
    ),
    entry(
        "App",
        "DraftProxy::app_node_value_by_id",
        NodeResolverBehavior::ProjectLocalRecord,
    ),
    entry(
        "AppInstallation",
        "DraftProxy::app_node_value_by_id",
        NodeResolverBehavior::ProjectLocalRecord,
    ),
    entry(
        "AppPurchaseOneTime",
        "DraftProxy::app_node_value_by_id",
        NodeResolverBehavior::ProjectLocalRecord,
    ),
    entry(
        "AppSubscription",
        "DraftProxy::app_node_value_by_id",
        NodeResolverBehavior::ProjectLocalRecord,
    ),
    entry(
        "AppUsageRecord",
        "DraftProxy::app_node_value_by_id",
        NodeResolverBehavior::ProjectLocalRecord,
    ),
    entry(
        "Article",
        "DraftProxy::online_store_content_node_value",
        NodeResolverBehavior::ProjectLocalRecord,
    ),
    entry(
        "Blog",
        "DraftProxy::online_store_content_node_value",
        NodeResolverBehavior::ProjectLocalRecord,
    ),
    entry(
        "CartTransform",
        "DraftProxy::local_node_value_by_id",
        NodeResolverBehavior::ProjectLocalRecord,
    ),
    entry(
        "CashTrackingSession",
        "local_node_value::is_safe_no_data_node_gid",
        NodeResolverBehavior::ReturnKnownNull,
    ),
    entry(
        "Collection",
        "DraftProxy::collection_json_with_publication_fields",
        NodeResolverBehavior::ProjectLocalRecord,
    ),
    entry(
        "Comment",
        "DraftProxy::online_store_content_node_value",
        NodeResolverBehavior::ProjectLocalRecord,
    ),
    entry(
        "Company",
        "DraftProxy::b2b_node_value_by_id",
        NodeResolverBehavior::ProjectLocalRecord,
    ),
    entry(
        "CompanyAddress",
        "DraftProxy::b2b_node_value_by_id",
        NodeResolverBehavior::ProjectLocalRecord,
    ),
    entry(
        "CompanyContact",
        "DraftProxy::b2b_node_value_by_id",
        NodeResolverBehavior::ProjectLocalRecord,
    ),
    entry(
        "CompanyContactRole",
        "DraftProxy::b2b_node_value_by_id",
        NodeResolverBehavior::ProjectLocalRecord,
    ),
    entry(
        "CompanyContactRoleAssignment",
        "DraftProxy::b2b_node_value_by_id",
        NodeResolverBehavior::ProjectLocalRecord,
    ),
    entry(
        "CompanyLocation",
        "DraftProxy::b2b_node_value_by_id",
        NodeResolverBehavior::ProjectLocalRecord,
    ),
    entry(
        "Customer",
        "DraftProxy::customer_node_value_by_id",
        NodeResolverBehavior::ProjectLocalRecord,
    ),
    entry(
        "CustomerPaymentMethod",
        "DraftProxy::customer_payment_method_node_value_by_id",
        NodeResolverBehavior::ProjectLocalRecord,
    ),
    entry(
        "CustomerSegmentMembersQuery",
        "DraftProxy::local_node_value_by_id",
        NodeResolverBehavior::ProjectLocalRecord,
    ),
    entry(
        "DeliveryCustomization",
        "DraftProxy::delivery_customization_node_value_by_id",
        NodeResolverBehavior::ProjectLocalRecord,
    ),
    entry(
        "DiscountAutomaticNode",
        "DraftProxy::discount_node_value_by_id",
        NodeResolverBehavior::ProjectLocalRecord,
    ),
    entry(
        "DiscountCodeNode",
        "DraftProxy::discount_node_value_by_id",
        NodeResolverBehavior::ProjectLocalRecord,
    ),
    entry(
        "ExternalVideo",
        "DraftProxy::local_node_value_by_id",
        NodeResolverBehavior::ProjectLocalRecord,
    ),
    entry(
        "Fulfillment",
        "DraftProxy::fulfillment_return_node_value_by_id",
        NodeResolverBehavior::ProjectLocalRecord,
    ),
    entry(
        "FulfillmentConstraintRule",
        "DraftProxy::local_node_value_by_id",
        NodeResolverBehavior::ProjectLocalRecord,
    ),
    entry(
        "FulfillmentEvent",
        "DraftProxy::fulfillment_return_node_value_by_id",
        NodeResolverBehavior::ProjectLocalRecord,
    ),
    entry(
        "FulfillmentHold",
        "DraftProxy::fulfillment_return_node_value_by_id",
        NodeResolverBehavior::ProjectLocalRecord,
    ),
    entry(
        "FulfillmentLineItem",
        "DraftProxy::fulfillment_return_node_value_by_id",
        NodeResolverBehavior::ProjectLocalRecord,
    ),
    entry(
        "FulfillmentOrder",
        "DraftProxy::fulfillment_return_node_value_by_id",
        NodeResolverBehavior::ProjectLocalRecord,
    ),
    entry(
        "FulfillmentOrderLineItem",
        "DraftProxy::fulfillment_return_node_value_by_id",
        NodeResolverBehavior::ProjectLocalRecord,
    ),
    entry(
        "GenericFile",
        "DraftProxy::local_node_value_by_id",
        NodeResolverBehavior::ProjectLocalRecord,
    ),
    entry(
        "GiftCard",
        "DraftProxy::gift_card_node_value_by_id",
        NodeResolverBehavior::ProjectLocalRecord,
    ),
    entry(
        "GiftCardCreditTransaction",
        "DraftProxy::gift_card_transaction_node_value_by_id",
        NodeResolverBehavior::ProjectLocalRecord,
    ),
    entry(
        "GiftCardDebitTransaction",
        "DraftProxy::gift_card_transaction_node_value_by_id",
        NodeResolverBehavior::ProjectLocalRecord,
    ),
    entry(
        "InventoryAdjustmentGroup",
        "DraftProxy::inventory_node_value_by_id",
        NodeResolverBehavior::ProjectLocalRecord,
    ),
    entry(
        "InventoryItem",
        "DraftProxy::inventory_node_value_by_id",
        NodeResolverBehavior::ProjectLocalRecord,
    ),
    entry(
        "InventoryLevel",
        "DraftProxy::inventory_node_value_by_id",
        NodeResolverBehavior::ProjectLocalRecord,
    ),
    entry(
        "InventoryQuantity",
        "DraftProxy::inventory_node_value_by_id",
        NodeResolverBehavior::ProjectLocalRecord,
    ),
    entry(
        "InventoryShipment",
        "DraftProxy::inventory_node_value_by_id",
        NodeResolverBehavior::ProjectLocalRecord,
    ),
    entry(
        "InventoryShipmentLineItem",
        "DraftProxy::inventory_node_value_by_id",
        NodeResolverBehavior::ProjectLocalRecord,
    ),
    entry(
        "InventoryTransfer",
        "DraftProxy::inventory_node_value_by_id",
        NodeResolverBehavior::ProjectLocalRecord,
    ),
    entry(
        "InventoryTransferLineItem",
        "DraftProxy::inventory_node_value_by_id",
        NodeResolverBehavior::ProjectLocalRecord,
    ),
    entry(
        "Location",
        "DraftProxy::local_node_value_by_id",
        NodeResolverBehavior::ProjectLocalRecord,
    ),
    entry(
        "MailingAddress",
        "DraftProxy::customer_address_node_value_by_id",
        NodeResolverBehavior::ProjectLocalRecord,
    ),
    entry(
        "MarketRegionCountry",
        "DraftProxy::local_node_value_by_id",
        NodeResolverBehavior::ProjectLocalRecord,
    ),
    entry(
        "MediaImage",
        "DraftProxy::local_node_value_by_id",
        NodeResolverBehavior::ProjectLocalRecord,
    ),
    entry(
        "Metaobject",
        "DraftProxy::metaobject_node_value_by_id",
        NodeResolverBehavior::ProjectLocalRecord,
    ),
    entry(
        "MetaobjectDefinition",
        "DraftProxy::metaobject_node_value_by_id",
        NodeResolverBehavior::ProjectLocalRecord,
    ),
    entry(
        "Model3d",
        "DraftProxy::local_node_value_by_id",
        NodeResolverBehavior::ProjectLocalRecord,
    ),
    entry(
        "Order",
        "DraftProxy::order_node_value_by_id",
        NodeResolverBehavior::ProjectLocalRecord,
    ),
    entry(
        "Page",
        "DraftProxy::online_store_content_node_value",
        NodeResolverBehavior::ProjectLocalRecord,
    ),
    entry(
        "PointOfSaleDevice",
        "local_node_value::is_safe_no_data_node_gid",
        NodeResolverBehavior::ReturnKnownNull,
    ),
    entry(
        "Product",
        "DraftProxy::product_json_with_variants_and_currency_context",
        NodeResolverBehavior::ProjectLocalRecord,
    ),
    entry(
        "ProductBundleOperation",
        "DraftProxy::product_operation_json",
        NodeResolverBehavior::ProjectLocalRecord,
    ),
    entry(
        "ProductDeleteOperation",
        "DraftProxy::product_delete_operation_value_by_id",
        NodeResolverBehavior::ProjectLocalRecord,
    ),
    entry(
        "ProductDuplicateOperation",
        "DraftProxy::product_operation_json",
        NodeResolverBehavior::ProjectLocalRecord,
    ),
    entry(
        "ProductFeed",
        "DraftProxy::product_tail_feed_node_value",
        NodeResolverBehavior::ProjectLocalRecord,
    ),
    entry(
        "ProductSetOperation",
        "DraftProxy::product_operation_json",
        NodeResolverBehavior::ProjectLocalRecord,
    ),
    entry(
        "ProductVariant",
        "DraftProxy::product_variant_by_id_value",
        NodeResolverBehavior::ProjectLocalRecord,
    ),
    entry(
        "Return",
        "DraftProxy::fulfillment_return_node_value_by_id",
        NodeResolverBehavior::ProjectLocalRecord,
    ),
    entry(
        "ReturnLineItem",
        "DraftProxy::fulfillment_return_node_value_by_id",
        NodeResolverBehavior::ProjectLocalRecord,
    ),
    entry(
        "ReturnableFulfillment",
        "DraftProxy::fulfillment_return_node_value_by_id",
        NodeResolverBehavior::ProjectLocalRecord,
    ),
    entry(
        "ReverseDelivery",
        "DraftProxy::fulfillment_return_node_value_by_id",
        NodeResolverBehavior::ProjectLocalRecord,
    ),
    entry(
        "ReverseDeliveryLineItem",
        "DraftProxy::fulfillment_return_node_value_by_id",
        NodeResolverBehavior::ProjectLocalRecord,
    ),
    entry(
        "ReverseFulfillmentOrder",
        "DraftProxy::fulfillment_return_node_value_by_id",
        NodeResolverBehavior::ProjectLocalRecord,
    ),
    entry(
        "ReverseFulfillmentOrderLineItem",
        "DraftProxy::fulfillment_return_node_value_by_id",
        NodeResolverBehavior::ProjectLocalRecord,
    ),
    entry(
        "Segment",
        "DraftProxy::local_node_value_by_id",
        NodeResolverBehavior::ProjectLocalRecord,
    ),
    entry(
        "ShopAddress",
        "DraftProxy::shop_property_node_value_by_id",
        NodeResolverBehavior::ProjectLocalRecord,
    ),
    entry(
        "ShopPolicy",
        "DraftProxy::shop_property_node_value_by_id",
        NodeResolverBehavior::ProjectLocalRecord,
    ),
    entry(
        "ShopifyFunction",
        "DraftProxy::shopify_function_node_value_by_id",
        NodeResolverBehavior::ProjectLocalRecord,
    ),
    entry(
        "ShopifyPaymentsDispute",
        "local_node_value::is_safe_no_data_node_gid",
        NodeResolverBehavior::ReturnKnownNull,
    ),
    entry(
        "StoreCreditAccount",
        "DraftProxy::store_credit_node_value_by_id",
        NodeResolverBehavior::ProjectLocalRecord,
    ),
    entry(
        "StoreCreditAccountCreditTransaction",
        "DraftProxy::store_credit_node_value_by_id",
        NodeResolverBehavior::ProjectLocalRecord,
    ),
    entry(
        "StoreCreditAccountDebitRevertTransaction",
        "DraftProxy::store_credit_node_value_by_id",
        NodeResolverBehavior::ProjectLocalRecord,
    ),
    entry(
        "StoreCreditAccountDebitTransaction",
        "DraftProxy::store_credit_node_value_by_id",
        NodeResolverBehavior::ProjectLocalRecord,
    ),
    entry(
        "StoreCreditAccountTransaction",
        "DraftProxy::store_credit_node_value_by_id",
        NodeResolverBehavior::ProjectLocalRecord,
    ),
    entry(
        "TaxAppConfiguration",
        "DraftProxy::local_node_value_by_id",
        NodeResolverBehavior::ProjectLocalRecord,
    ),
    entry(
        "UnverifiedReturnLineItem",
        "DraftProxy::fulfillment_return_node_value_by_id",
        NodeResolverBehavior::ProjectLocalRecord,
    ),
    entry(
        "Validation",
        "DraftProxy::local_node_value_by_id",
        NodeResolverBehavior::ProjectLocalRecord,
    ),
    entry(
        "Video",
        "DraftProxy::local_node_value_by_id",
        NodeResolverBehavior::ProjectLocalRecord,
    ),
];

const fn entry(
    type_name: &'static str,
    resolver: &'static str,
    behavior: NodeResolverBehavior,
) -> NodeResolverInventoryEntry {
    NodeResolverInventoryEntry {
        type_name,
        resolver,
        behavior,
    }
}

pub fn default_node_resolver_inventory() -> &'static [NodeResolverInventoryEntry] {
    DEFAULT_NODE_RESOLVER_INVENTORY
}

pub fn default_node_resolver_inventory_json_value() -> Value {
    Value::Array(
        default_node_resolver_inventory()
            .iter()
            .map(node_resolver_inventory_entry_json_value)
            .collect(),
    )
}

fn node_resolver_inventory_entry_json_value(entry: &NodeResolverInventoryEntry) -> Value {
    json!({
        "typeName": entry.type_name,
        "resolver": entry.resolver,
        "behavior": entry.behavior.registry_name(),
    })
}

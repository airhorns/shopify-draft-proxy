use serde_json::{json, Value};

use crate::proxy::{DraftProxy, Request};

pub(crate) type NodeLoader = fn(&DraftProxy, &str, Option<&Request>) -> NodeLoadState<EntityRef>;

/// Concrete Storefront `Node` types backed by the local discovery model. Keep
/// this beside the Admin loader inventory so schema reachability and runtime
/// entity loading cannot silently treat every captured Storefront implementor
/// as locally materializable.
pub(crate) const STOREFRONT_NODE_TYPE_NAMES: &[&str] = &[
    "Article",
    "Blog",
    "Collection",
    "Location",
    "Menu",
    "Metaobject",
    "Page",
    "Product",
    "ProductVariant",
];

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct EntityRef {
    pub type_name: String,
    pub id: String,
    pub value: Value,
}

impl EntityRef {
    pub(crate) fn new(type_name: impl Into<String>, id: &str, mut value: Value) -> Self {
        let type_name = type_name.into();
        if let Some(object) = value.as_object_mut() {
            object
                .entry("__typename".to_string())
                .or_insert_with(|| json!(&type_name));
            object.entry("id".to_string()).or_insert_with(|| json!(id));
        }
        Self {
            type_name,
            id: id.to_string(),
            value,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum NodeLoadState<T = Value> {
    Found(T),
    KnownMissing,
    NeedsHydration,
    UnsupportedType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeResolverBehavior {
    ProjectLocalRecord,
    ReturnKnownNull,
}

impl NodeResolverBehavior {
    fn registry_name(self) -> &'static str {
        match self {
            Self::ProjectLocalRecord => "project-local-record",
            Self::ReturnKnownNull => "return-known-null",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct NodeResolverInventoryEntry {
    pub type_name: &'static str,
    pub resolver: &'static str,
    pub behavior: NodeResolverBehavior,
    pub(crate) loader: NodeLoader,
}

macro_rules! node_entry {
    ($type_name:literal, $resolver:literal, $behavior:expr, $loader:ident $(,)?) => {
        node_entry_with_loader(
            $type_name,
            $resolver,
            $behavior,
            crate::proxy::node_registry::$loader,
        )
    };
}

const DEFAULT_NODE_RESOLVER_INVENTORY: &[NodeResolverInventoryEntry] = &[
    node_entry!(
        "Abandonment",
        "Store::staged.abandonments",
        NodeResolverBehavior::ProjectLocalRecord,
        load_abandonment,
    ),
    node_entry!(
        "App",
        "DraftProxy::app_node_value_by_id",
        NodeResolverBehavior::ProjectLocalRecord,
        load_app,
    ),
    node_entry!(
        "AppInstallation",
        "DraftProxy::app_node_value_by_id",
        NodeResolverBehavior::ProjectLocalRecord,
        load_app,
    ),
    node_entry!(
        "AppPurchaseOneTime",
        "DraftProxy::app_node_value_by_id",
        NodeResolverBehavior::ProjectLocalRecord,
        load_app,
    ),
    node_entry!(
        "AppSubscription",
        "DraftProxy::app_node_value_by_id",
        NodeResolverBehavior::ProjectLocalRecord,
        load_app,
    ),
    node_entry!(
        "AppUsageRecord",
        "DraftProxy::app_node_value_by_id",
        NodeResolverBehavior::ProjectLocalRecord,
        load_app,
    ),
    node_entry!(
        "Article",
        "DraftProxy::online_store_content_node_value",
        NodeResolverBehavior::ProjectLocalRecord,
        load_online_store,
    ),
    node_entry!(
        "Blog",
        "DraftProxy::online_store_content_node_value",
        NodeResolverBehavior::ProjectLocalRecord,
        load_online_store,
    ),
    node_entry!(
        "CartTransform",
        "node_registry::load_cart_transform",
        NodeResolverBehavior::ProjectLocalRecord,
        load_cart_transform,
    ),
    node_entry!(
        "CashTrackingSession",
        "NodeLoadState::KnownMissing",
        NodeResolverBehavior::ReturnKnownNull,
        load_known_null,
    ),
    node_entry!(
        "Collection",
        "DraftProxy::collection_canonical_value_by_id",
        NodeResolverBehavior::ProjectLocalRecord,
        load_collection,
    ),
    node_entry!(
        "Comment",
        "DraftProxy::online_store_content_node_value",
        NodeResolverBehavior::ProjectLocalRecord,
        load_online_store,
    ),
    node_entry!(
        "Company",
        "DraftProxy::b2b_node_value_by_id",
        NodeResolverBehavior::ProjectLocalRecord,
        load_b2b,
    ),
    node_entry!(
        "CompanyAddress",
        "DraftProxy::b2b_node_value_by_id",
        NodeResolverBehavior::ProjectLocalRecord,
        load_b2b,
    ),
    node_entry!(
        "CompanyContact",
        "DraftProxy::b2b_node_value_by_id",
        NodeResolverBehavior::ProjectLocalRecord,
        load_b2b,
    ),
    node_entry!(
        "CompanyContactRole",
        "DraftProxy::b2b_node_value_by_id",
        NodeResolverBehavior::ProjectLocalRecord,
        load_b2b,
    ),
    node_entry!(
        "CompanyContactRoleAssignment",
        "DraftProxy::b2b_node_value_by_id",
        NodeResolverBehavior::ProjectLocalRecord,
        load_b2b,
    ),
    node_entry!(
        "CompanyLocation",
        "DraftProxy::b2b_node_value_by_id",
        NodeResolverBehavior::ProjectLocalRecord,
        load_b2b,
    ),
    node_entry!(
        "Customer",
        "DraftProxy::customer_node_value_by_id",
        NodeResolverBehavior::ProjectLocalRecord,
        load_customer,
    ),
    node_entry!(
        "CustomerPaymentMethod",
        "DraftProxy::customer_payment_method_node_value_by_id",
        NodeResolverBehavior::ProjectLocalRecord,
        load_customer_payment_method,
    ),
    node_entry!(
        "CustomerSegmentMembersQuery",
        "Store::staged.customer_segment_member_queries",
        NodeResolverBehavior::ProjectLocalRecord,
        load_customer_segment_members_query,
    ),
    node_entry!(
        "DeliveryCustomization",
        "DraftProxy::delivery_customization_node_value_by_id",
        NodeResolverBehavior::ProjectLocalRecord,
        load_delivery_customization,
    ),
    node_entry!(
        "DeliveryPromiseParticipant",
        "DraftProxy::delivery_promise_node_value_by_id",
        NodeResolverBehavior::ProjectLocalRecord,
        load_delivery_promise,
    ),
    node_entry!(
        "DeliveryPromiseProvider",
        "DraftProxy::delivery_promise_node_value_by_id",
        NodeResolverBehavior::ProjectLocalRecord,
        load_delivery_promise,
    ),
    node_entry!(
        "DiscountAutomaticNode",
        "DraftProxy::discount_node_value_by_id",
        NodeResolverBehavior::ProjectLocalRecord,
        load_discount,
    ),
    node_entry!(
        "DiscountCodeNode",
        "DraftProxy::discount_node_value_by_id",
        NodeResolverBehavior::ProjectLocalRecord,
        load_discount,
    ),
    node_entry!(
        "ExternalVideo",
        "Store::staged.media_files",
        NodeResolverBehavior::ProjectLocalRecord,
        load_media,
    ),
    node_entry!(
        "Fulfillment",
        "DraftProxy::fulfillment_return_node_value_by_id",
        NodeResolverBehavior::ProjectLocalRecord,
        load_fulfillment_return,
    ),
    node_entry!(
        "FulfillmentConstraintRule",
        "node_registry::load_fulfillment_constraint_rule",
        NodeResolverBehavior::ProjectLocalRecord,
        load_fulfillment_constraint_rule,
    ),
    node_entry!(
        "FulfillmentEvent",
        "DraftProxy::fulfillment_return_node_value_by_id",
        NodeResolverBehavior::ProjectLocalRecord,
        load_fulfillment_return,
    ),
    node_entry!(
        "FulfillmentHold",
        "DraftProxy::fulfillment_return_node_value_by_id",
        NodeResolverBehavior::ProjectLocalRecord,
        load_fulfillment_return,
    ),
    node_entry!(
        "FulfillmentLineItem",
        "DraftProxy::fulfillment_return_node_value_by_id",
        NodeResolverBehavior::ProjectLocalRecord,
        load_fulfillment_return,
    ),
    node_entry!(
        "FulfillmentOrder",
        "DraftProxy::fulfillment_return_node_value_by_id",
        NodeResolverBehavior::ProjectLocalRecord,
        load_fulfillment_return,
    ),
    node_entry!(
        "FulfillmentOrderLineItem",
        "DraftProxy::fulfillment_return_node_value_by_id",
        NodeResolverBehavior::ProjectLocalRecord,
        load_fulfillment_return,
    ),
    node_entry!(
        "GenericFile",
        "Store::staged.media_files",
        NodeResolverBehavior::ProjectLocalRecord,
        load_media,
    ),
    node_entry!(
        "GiftCard",
        "DraftProxy::gift_card_node_value_by_id",
        NodeResolverBehavior::ProjectLocalRecord,
        load_gift_card,
    ),
    node_entry!(
        "GiftCardCreditTransaction",
        "DraftProxy::gift_card_transaction_node_value_by_id",
        NodeResolverBehavior::ProjectLocalRecord,
        load_gift_card_transaction,
    ),
    node_entry!(
        "GiftCardDebitTransaction",
        "DraftProxy::gift_card_transaction_node_value_by_id",
        NodeResolverBehavior::ProjectLocalRecord,
        load_gift_card_transaction,
    ),
    node_entry!(
        "InventoryAdjustmentGroup",
        "DraftProxy::inventory_node_value_by_id",
        NodeResolverBehavior::ProjectLocalRecord,
        load_inventory,
    ),
    node_entry!(
        "InventoryItem",
        "DraftProxy::inventory_item_canonical_value",
        NodeResolverBehavior::ProjectLocalRecord,
        load_inventory_item,
    ),
    node_entry!(
        "InventoryLevel",
        "DraftProxy::inventory_level_value_by_id",
        NodeResolverBehavior::ProjectLocalRecord,
        load_inventory_level,
    ),
    node_entry!(
        "InventoryQuantity",
        "DraftProxy::inventory_node_value_by_id",
        NodeResolverBehavior::ProjectLocalRecord,
        load_inventory,
    ),
    node_entry!(
        "InventoryShipment",
        "DraftProxy::inventory_node_value_by_id",
        NodeResolverBehavior::ProjectLocalRecord,
        load_inventory,
    ),
    node_entry!(
        "InventoryShipmentLineItem",
        "DraftProxy::inventory_node_value_by_id",
        NodeResolverBehavior::ProjectLocalRecord,
        load_inventory,
    ),
    node_entry!(
        "InventoryTransfer",
        "DraftProxy::inventory_node_value_by_id",
        NodeResolverBehavior::ProjectLocalRecord,
        load_inventory,
    ),
    node_entry!(
        "InventoryTransferLineItem",
        "DraftProxy::inventory_node_value_by_id",
        NodeResolverBehavior::ProjectLocalRecord,
        load_inventory,
    ),
    node_entry!(
        "Location",
        "DraftProxy::location_for_read",
        NodeResolverBehavior::ProjectLocalRecord,
        load_location,
    ),
    node_entry!(
        "MailingAddress",
        "DraftProxy::customer_address_node_value_by_id",
        NodeResolverBehavior::ProjectLocalRecord,
        load_customer_address,
    ),
    node_entry!(
        "MarketRegionCountry",
        "Store::staged.backup_region",
        NodeResolverBehavior::ProjectLocalRecord,
        load_backup_region,
    ),
    node_entry!(
        "MediaImage",
        "Store::staged.media_files",
        NodeResolverBehavior::ProjectLocalRecord,
        load_media,
    ),
    node_entry!(
        "Metaobject",
        "DraftProxy::metaobject_node_value_by_id",
        NodeResolverBehavior::ProjectLocalRecord,
        load_metaobject,
    ),
    node_entry!(
        "MetaobjectDefinition",
        "DraftProxy::metaobject_node_value_by_id",
        NodeResolverBehavior::ProjectLocalRecord,
        load_metaobject,
    ),
    node_entry!(
        "Model3d",
        "Store::staged.media_files",
        NodeResolverBehavior::ProjectLocalRecord,
        load_media,
    ),
    node_entry!(
        "Order",
        "DraftProxy::order_node_value_by_id",
        NodeResolverBehavior::ProjectLocalRecord,
        load_order,
    ),
    node_entry!(
        "Page",
        "DraftProxy::online_store_content_node_value",
        NodeResolverBehavior::ProjectLocalRecord,
        load_online_store,
    ),
    node_entry!(
        "PointOfSaleDevice",
        "NodeLoadState::KnownMissing",
        NodeResolverBehavior::ReturnKnownNull,
        load_known_null,
    ),
    node_entry!(
        "Product",
        "DraftProxy::product_canonical_value",
        NodeResolverBehavior::ProjectLocalRecord,
        load_product,
    ),
    node_entry!(
        "ProductBundleOperation",
        "DraftProxy::product_operation_value_by_id",
        NodeResolverBehavior::ProjectLocalRecord,
        load_product_operation,
    ),
    node_entry!(
        "ProductDeleteOperation",
        "DraftProxy::product_operation_value_by_id",
        NodeResolverBehavior::ProjectLocalRecord,
        load_product_operation,
    ),
    node_entry!(
        "ProductDuplicateOperation",
        "DraftProxy::product_operation_value_by_id",
        NodeResolverBehavior::ProjectLocalRecord,
        load_product_operation,
    ),
    node_entry!(
        "ProductFeed",
        "DraftProxy::product_feed_canonical_value",
        NodeResolverBehavior::ProjectLocalRecord,
        load_product_feed,
    ),
    node_entry!(
        "ProductSetOperation",
        "DraftProxy::product_operation_value_by_id",
        NodeResolverBehavior::ProjectLocalRecord,
        load_product_operation,
    ),
    node_entry!(
        "ProductVariant",
        "DraftProxy::product_variant_canonical_value",
        NodeResolverBehavior::ProjectLocalRecord,
        load_product_variant,
    ),
    node_entry!(
        "Return",
        "DraftProxy::fulfillment_return_node_value_by_id",
        NodeResolverBehavior::ProjectLocalRecord,
        load_fulfillment_return,
    ),
    node_entry!(
        "ReturnLineItem",
        "DraftProxy::fulfillment_return_node_value_by_id",
        NodeResolverBehavior::ProjectLocalRecord,
        load_fulfillment_return,
    ),
    node_entry!(
        "ReturnableFulfillment",
        "DraftProxy::fulfillment_return_node_value_by_id",
        NodeResolverBehavior::ProjectLocalRecord,
        load_fulfillment_return,
    ),
    node_entry!(
        "ReverseDelivery",
        "DraftProxy::fulfillment_return_node_value_by_id",
        NodeResolverBehavior::ProjectLocalRecord,
        load_fulfillment_return,
    ),
    node_entry!(
        "ReverseDeliveryLineItem",
        "DraftProxy::fulfillment_return_node_value_by_id",
        NodeResolverBehavior::ProjectLocalRecord,
        load_fulfillment_return,
    ),
    node_entry!(
        "ReverseFulfillmentOrder",
        "DraftProxy::fulfillment_return_node_value_by_id",
        NodeResolverBehavior::ProjectLocalRecord,
        load_fulfillment_return,
    ),
    node_entry!(
        "ReverseFulfillmentOrderLineItem",
        "DraftProxy::fulfillment_return_node_value_by_id",
        NodeResolverBehavior::ProjectLocalRecord,
        load_fulfillment_return,
    ),
    node_entry!(
        "Segment",
        "Store::segment_by_id",
        NodeResolverBehavior::ProjectLocalRecord,
        load_segment,
    ),
    node_entry!(
        "ShopAddress",
        "DraftProxy::shop_property_node_value_by_id",
        NodeResolverBehavior::ProjectLocalRecord,
        load_shop_property,
    ),
    node_entry!(
        "ShopPolicy",
        "DraftProxy::shop_property_node_value_by_id",
        NodeResolverBehavior::ProjectLocalRecord,
        load_shop_property,
    ),
    node_entry!(
        "ShopifyFunction",
        "Store::effective.function_metadata",
        NodeResolverBehavior::ProjectLocalRecord,
        load_shopify_function,
    ),
    node_entry!(
        "ShopifyPaymentsDispute",
        "NodeLoadState::KnownMissing",
        NodeResolverBehavior::ReturnKnownNull,
        load_known_null,
    ),
    node_entry!(
        "StoreCreditAccount",
        "DraftProxy::store_credit_node_value_by_id",
        NodeResolverBehavior::ProjectLocalRecord,
        load_store_credit,
    ),
    node_entry!(
        "StoreCreditAccountCreditTransaction",
        "DraftProxy::store_credit_node_value_by_id",
        NodeResolverBehavior::ProjectLocalRecord,
        load_store_credit,
    ),
    node_entry!(
        "StoreCreditAccountDebitRevertTransaction",
        "DraftProxy::store_credit_node_value_by_id",
        NodeResolverBehavior::ProjectLocalRecord,
        load_store_credit,
    ),
    node_entry!(
        "StoreCreditAccountDebitTransaction",
        "DraftProxy::store_credit_node_value_by_id",
        NodeResolverBehavior::ProjectLocalRecord,
        load_store_credit,
    ),
    node_entry!(
        "StoreCreditAccountTransaction",
        "DraftProxy::store_credit_node_value_by_id",
        NodeResolverBehavior::ProjectLocalRecord,
        load_store_credit,
    ),
    node_entry!(
        "TaxAppConfiguration",
        "Store::staged.tax_app_configuration",
        NodeResolverBehavior::ProjectLocalRecord,
        load_tax_app_configuration,
    ),
    node_entry!(
        "TaxonomyCategory",
        "DraftProxy::taxonomy_category_node_value",
        NodeResolverBehavior::ProjectLocalRecord,
        load_taxonomy_category,
    ),
    node_entry!(
        "UnverifiedReturnLineItem",
        "DraftProxy::fulfillment_return_node_value_by_id",
        NodeResolverBehavior::ProjectLocalRecord,
        load_fulfillment_return,
    ),
    node_entry!(
        "Validation",
        "node_registry::load_validation",
        NodeResolverBehavior::ProjectLocalRecord,
        load_validation,
    ),
    node_entry!(
        "Video",
        "Store::staged.media_files",
        NodeResolverBehavior::ProjectLocalRecord,
        load_media,
    ),
];

const fn node_entry_with_loader(
    type_name: &'static str,
    resolver: &'static str,
    behavior: NodeResolverBehavior,
    loader: NodeLoader,
) -> NodeResolverInventoryEntry {
    NodeResolverInventoryEntry {
        type_name,
        resolver,
        behavior,
        loader,
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

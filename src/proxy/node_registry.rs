use super::*;

use crate::node_resolver_inventory::{default_node_resolver_inventory, EntityRef, NodeLoadState};

fn observed_node_values(body: &Value) -> Vec<Value> {
    let mut nodes = body
        .pointer("/data/nodes")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .cloned()
        .collect::<Vec<_>>();
    if let Some(node) = body.pointer("/data/node").filter(|node| node.is_object()) {
        nodes.push(node.clone());
    }
    for pointer in ["/data/productByIdentifier", "/data/productByHandle"] {
        if let Some(node) = body.pointer(pointer).filter(|node| node.is_object()) {
            nodes.push(node.clone());
        }
    }
    nodes
}

impl DraftProxy {
    pub(in crate::proxy) fn local_node_query_data(
        &self,
        fields: &[RootFieldSelection],
        allow_unknown_null: bool,
        request: Option<&Request>,
    ) -> Option<Value> {
        let mut missing_required = false;
        let data = root_payload_json(fields, |field| {
            let value = match field.name.as_str() {
                "node" => {
                    let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
                    match self.node_load_state(&id, request) {
                        NodeLoadState::Found(entity) => entity.value,
                        NodeLoadState::KnownMissing => Value::Null,
                        NodeLoadState::NeedsHydration | NodeLoadState::UnsupportedType
                            if allow_unknown_null =>
                        {
                            Value::Null
                        }
                        NodeLoadState::NeedsHydration | NodeLoadState::UnsupportedType => {
                            missing_required = true;
                            return None;
                        }
                    }
                }
                "nodes" => Value::Array(
                    field
                        .arguments
                        .get("ids")
                        .map(resolved_string_list)
                        .unwrap_or_default()
                        .into_iter()
                        .map(|id| match self.node_load_state(&id, request) {
                            NodeLoadState::Found(entity) => Some(entity.value),
                            NodeLoadState::KnownMissing => Some(Value::Null),
                            NodeLoadState::NeedsHydration | NodeLoadState::UnsupportedType
                                if allow_unknown_null =>
                            {
                                Some(Value::Null)
                            }
                            NodeLoadState::NeedsHydration | NodeLoadState::UnsupportedType => None,
                        })
                        .collect::<Option<Vec<_>>>()
                        .unwrap_or_else(|| {
                            missing_required = true;
                            Vec::new()
                        }),
                ),
                _ => return None,
            };
            Some(value)
        });
        (!missing_required).then_some(data)
    }

    pub(in crate::proxy) fn node_query_data_with_upstream_fallback(
        &self,
        fields: &[RootFieldSelection],
        upstream_body: &Value,
        request: Option<&Request>,
    ) -> Value {
        root_payload_json(fields, |field| {
            let upstream = upstream_body
                .get("data")
                .and_then(Value::as_object)
                .and_then(|data| data.get(&field.response_key));
            match field.name.as_str() {
                "node" => {
                    let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
                    Some(match self.node_load_state(&id, request) {
                        NodeLoadState::Found(entity) => entity.value,
                        NodeLoadState::KnownMissing => Value::Null,
                        NodeLoadState::NeedsHydration | NodeLoadState::UnsupportedType => {
                            upstream.cloned().unwrap_or(Value::Null)
                        }
                    })
                }
                "nodes" => {
                    let upstream_nodes = upstream.and_then(Value::as_array);
                    let values = field
                        .arguments
                        .get("ids")
                        .map(resolved_string_list)
                        .unwrap_or_default()
                        .into_iter()
                        .enumerate()
                        .map(|(index, id)| match self.node_load_state(&id, request) {
                            NodeLoadState::Found(entity) => entity.value,
                            NodeLoadState::KnownMissing => Value::Null,
                            NodeLoadState::NeedsHydration | NodeLoadState::UnsupportedType => {
                                upstream_nodes
                                    .and_then(|nodes| nodes.get(index))
                                    .cloned()
                                    .unwrap_or(Value::Null)
                            }
                        })
                        .collect();
                    Some(Value::Array(values))
                }
                _ => upstream.cloned(),
            }
        })
    }

    pub(in crate::proxy) fn local_node_value_by_id(
        &self,
        id: &str,
        selection: &[SelectedField],
    ) -> Option<Value> {
        self.local_node_value_by_id_with_request(id, selection, None)
    }

    fn local_node_value_by_id_with_request(
        &self,
        id: &str,
        selection: &[SelectedField],
        request: Option<&Request>,
    ) -> Option<Value> {
        match self.node_load_state(id, request) {
            NodeLoadState::Found(entity) => Some(selected_json(&entity.value, selection)),
            NodeLoadState::KnownMissing => Some(Value::Null),
            NodeLoadState::NeedsHydration | NodeLoadState::UnsupportedType => None,
        }
    }

    fn node_load_state(&self, id: &str, request: Option<&Request>) -> NodeLoadState<EntityRef> {
        registered_node_value(self, id, request)
    }

    pub(in crate::proxy) fn observe_nodes_response(&mut self, response: &Response) {
        self.observe_nodes_data(&response.body);
    }

    pub(in crate::proxy) fn observe_nodes_data(&mut self, body: &Value) {
        let nodes = observed_node_values(body);
        for node in &nodes {
            self.observe_node_response_value(node);
        }
        for node in nodes {
            let id = node.get("id").and_then(Value::as_str).unwrap_or_default();
            if is_shopify_gid_of_type(id, "Collection") {
                self.stage_collection_from_observed_json(&node);
            }
        }
    }

    fn observe_node_response_value(&mut self, node: &Value) {
        let id = node.get("id").and_then(Value::as_str).unwrap_or_default();
        if is_shopify_gid_of_type(id, "Product") {
            self.store.stage_observed_product_json(node);
            if let Some(product_id) = node.get("id").and_then(Value::as_str) {
                for variant in node
                    .get("variants")
                    .and_then(|connection| connection.get("nodes"))
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                {
                    let mut variant_value = variant.clone();
                    if let Some(object) = variant_value.as_object_mut() {
                        object.insert("productId".to_string(), json!(product_id));
                    }
                    if let Some(mut variant) =
                        product_variant_state_from_observed_json(&variant_value)
                    {
                        variant.product_id = product_id.to_string();
                        self.store.stage_product_variant(variant);
                    }
                }
            }
        } else if is_shopify_gid_of_type(id, "Collection") {
            self.stage_collection_from_observed_json(node);
        } else if is_shopify_gid_of_type(id, "ProductVariant") {
            if let Some(variant) = product_variant_state_from_observed_json(node) {
                self.store.stage_product_variant(variant);
            }
            if let Some(product) = node.get("product").and_then(product_state_from_json) {
                self.store.stage_observed_product(product);
            }
        } else if is_shopify_gid_of_type(id, "InventoryItem") {
            self.observe_inventory_item_node(node);
        } else if is_shopify_gid_of_type(id, "InventoryLevel") {
            self.observe_inventory_level_node(node);
        } else if shopify_gid_resource_type(id) == Some("Location") {
            self.merge_staged_location(node, &[]);
        } else if matches!(
            shopify_gid_resource_type(id),
            Some("ShopAddress" | "ShopPolicy")
        ) {
            self.observe_shop_property_node(node);
        }
    }

    pub(in crate::proxy) fn app_node_value_by_id(
        &self,
        id: &str,
        request: Option<&Request>,
    ) -> Option<Value> {
        for (app_id, installation) in &self.store.staged.installed_apps {
            if app_installation_id(installation).as_deref() == Some(id) {
                if self.store.staged.uninstalled_app_ids.contains(app_id) {
                    return Some(Value::Null);
                }
                let revoked_access_scopes = self
                    .store
                    .staged
                    .revoked_app_access_scopes
                    .get(app_id)
                    .cloned()
                    .unwrap_or_default();
                return Some(current_app_installation_node_value(
                    installation,
                    &self.store.staged.app_subscriptions,
                    &self.store.staged.app_one_time_purchases,
                    &revoked_access_scopes,
                ));
            }
            if installation.pointer("/app/id").and_then(Value::as_str) == Some(id) {
                return installation.get("app").cloned();
            }
        }
        if let Some(request) = request {
            let app_id = request_app_gid(request);
            let installation = current_app_installation_from_request(request);
            if app_installation_id(&installation).as_deref() == Some(id) {
                if self.store.staged.uninstalled_app_ids.contains(&app_id) {
                    return Some(Value::Null);
                }
                let revoked_access_scopes = self
                    .store
                    .staged
                    .revoked_app_access_scopes
                    .get(&app_id)
                    .cloned()
                    .unwrap_or_default();
                return Some(current_app_installation_node_value(
                    &installation,
                    &self.store.staged.app_subscriptions,
                    &self.store.staged.app_one_time_purchases,
                    &revoked_access_scopes,
                ));
            }
            if installation.pointer("/app/id").and_then(Value::as_str) == Some(id) {
                return installation.get("app").cloned();
            }
        }
        self.store
            .staged
            .app_subscriptions
            .get(id)
            .cloned()
            .or_else(|| self.store.staged.app_one_time_purchases.get(id).cloned())
            .or_else(|| self.find_staged_app_usage_record(id))
    }
}

fn current_app_installation_node_value(
    installation: &Value,
    subscriptions: &BTreeMap<String, Value>,
    one_time_purchases: &BTreeMap<String, Value>,
    revoked_access_scopes: &BTreeSet<String>,
) -> Value {
    let mut value = installation.clone();
    value["__typename"] = json!("AppInstallation");
    if let Some(id) = app_installation_id(installation) {
        value["id"] = json!(id);
    }
    if !subscriptions.is_empty() {
        let all = subscriptions.values().cloned().collect::<Vec<_>>();
        value["activeSubscriptions"] = Value::Array(
            all.iter()
                .filter(|subscription| subscription["status"] == "ACTIVE")
                .cloned()
                .collect(),
        );
        value["allSubscriptions"] = connection_json(all);
    } else if value.get("activeSubscriptions").is_none() {
        value["activeSubscriptions"] = Value::Array(Vec::new());
    }
    if !one_time_purchases.is_empty() {
        value["oneTimePurchases"] = connection_json(one_time_purchases.values().cloned().collect());
    }
    value["accessScopes"] = Value::Array(
        installation
            .get("accessScopes")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter(|scope| {
                scope
                    .get("handle")
                    .and_then(Value::as_str)
                    .is_none_or(|handle| !revoked_access_scopes.contains(handle))
            })
            .cloned()
            .collect(),
    );
    value
}

pub(in crate::proxy) fn registered_node_value(
    proxy: &DraftProxy,
    id: &str,
    request: Option<&Request>,
) -> NodeLoadState<EntityRef> {
    // Shopify market-region IDs use the nested `Market/Region/...` shape even
    // though the GraphQL runtime type is `MarketRegionCountry`. Resolve that
    // exceptional identity shape at the registry boundary so domain loaders
    // remain keyed by real GraphQL type names.
    let resource_type = if id.starts_with("gid://shopify/Market/Region/") {
        "MarketRegionCountry"
    } else {
        let Some(resource_type) = shopify_gid_resource_type(id) else {
            return NodeLoadState::UnsupportedType;
        };
        resource_type
    };
    let Some(registration) = default_node_resolver_inventory()
        .iter()
        .find(|registration| registration.type_name == resource_type)
    else {
        return NodeLoadState::UnsupportedType;
    };
    match (registration.loader)(proxy, id, request) {
        NodeLoadState::Found(entity) => {
            debug_assert_eq!(entity.type_name, resource_type);
            debug_assert_eq!(entity.id, id);
            NodeLoadState::Found(entity)
        }
        NodeLoadState::KnownMissing => NodeLoadState::KnownMissing,
        NodeLoadState::NeedsHydration => NodeLoadState::NeedsHydration,
        NodeLoadState::UnsupportedType => NodeLoadState::UnsupportedType,
    }
}

pub(crate) fn load_app(
    proxy: &DraftProxy,
    id: &str,
    request: Option<&Request>,
) -> NodeLoadState<EntityRef> {
    let Some(type_name) = registered_gid_type(
        id,
        &[
            "App",
            "AppInstallation",
            "AppPurchaseOneTime",
            "AppSubscription",
            "AppUsageRecord",
        ],
    ) else {
        return NodeLoadState::UnsupportedType;
    };
    entity_load_state(type_name, id, proxy.app_node_value_by_id(id, request))
}

pub(crate) fn load_online_store(
    proxy: &DraftProxy,
    id: &str,
    _request: Option<&Request>,
) -> NodeLoadState<EntityRef> {
    let Some(type_name) = registered_gid_type(id, &["Article", "Blog", "Comment", "Page"]) else {
        return NodeLoadState::UnsupportedType;
    };
    entity_load_state(type_name, id, proxy.online_store_content_node_value(id))
}

macro_rules! simple_loader {
    ($name:ident, $method:ident, [$($type_name:literal),+ $(,)?]) => {
        pub(crate) fn $name(
            proxy: &DraftProxy,
            id: &str,
            _request: Option<&Request>,
        ) -> NodeLoadState<EntityRef> {
            let Some(type_name) = registered_gid_type(id, &[$($type_name),+]) else {
                return NodeLoadState::UnsupportedType;
            };
            entity_load_state(type_name, id, proxy.$method(id))
        }
    };
}

simple_loader!(
    load_b2b,
    b2b_node_value_by_id,
    [
        "Company",
        "CompanyAddress",
        "CompanyContact",
        "CompanyContactRole",
        "CompanyContactRoleAssignment",
        "CompanyLocation",
    ]
);
simple_loader!(load_customer, customer_node_value_by_id, ["Customer"]);
simple_loader!(
    load_customer_address,
    customer_address_node_value_by_id,
    ["MailingAddress"]
);
simple_loader!(
    load_customer_payment_method,
    customer_payment_method_node_value_by_id,
    ["CustomerPaymentMethod"]
);
simple_loader!(
    load_store_credit,
    store_credit_node_value_by_id,
    [
        "StoreCreditAccount",
        "StoreCreditAccountCreditTransaction",
        "StoreCreditAccountDebitRevertTransaction",
        "StoreCreditAccountDebitTransaction",
        "StoreCreditAccountTransaction",
    ]
);
simple_loader!(
    load_discount,
    discount_node_value_by_id,
    ["DiscountAutomaticNode", "DiscountCodeNode"]
);
simple_loader!(
    load_fulfillment_return,
    fulfillment_return_node_value_by_id,
    [
        "Fulfillment",
        "FulfillmentEvent",
        "FulfillmentHold",
        "FulfillmentLineItem",
        "FulfillmentOrder",
        "FulfillmentOrderLineItem",
        "Return",
        "ReturnLineItem",
        "ReturnableFulfillment",
        "ReverseDelivery",
        "ReverseDeliveryLineItem",
        "ReverseFulfillmentOrder",
        "ReverseFulfillmentOrderLineItem",
        "UnverifiedReturnLineItem",
    ]
);
simple_loader!(load_gift_card, gift_card_node_value_by_id, ["GiftCard"]);
simple_loader!(
    load_gift_card_transaction,
    gift_card_transaction_node_value_by_id,
    ["GiftCardCreditTransaction", "GiftCardDebitTransaction"]
);
simple_loader!(
    load_inventory,
    inventory_node_value_by_id,
    [
        "InventoryAdjustmentGroup",
        "InventoryQuantity",
        "InventoryShipment",
        "InventoryShipmentLineItem",
        "InventoryTransfer",
        "InventoryTransferLineItem",
    ]
);
simple_loader!(
    load_metaobject,
    metaobject_node_value_by_id,
    ["Metaobject", "MetaobjectDefinition"]
);
simple_loader!(
    load_shop_property,
    shop_property_node_value_by_id,
    ["ShopAddress", "ShopPolicy"]
);

fn registered_gid_type(id: &str, allowed: &'static [&'static str]) -> Option<&'static str> {
    let resource_type = shopify_gid_resource_type(id)?;
    allowed
        .iter()
        .copied()
        .find(|candidate| *candidate == resource_type)
}

fn entity_load_state(
    type_name: &'static str,
    id: &str,
    value: Option<Value>,
) -> NodeLoadState<EntityRef> {
    match value {
        Some(value) if value.is_null() => NodeLoadState::KnownMissing,
        Some(value) => NodeLoadState::Found(EntityRef::new(type_name, id, value)),
        None => NodeLoadState::NeedsHydration,
    }
}

pub(crate) fn load_product(
    proxy: &DraftProxy,
    id: &str,
    _request: Option<&Request>,
) -> NodeLoadState<EntityRef> {
    if proxy.store.product_is_tombstoned(id) {
        return NodeLoadState::KnownMissing;
    }
    proxy
        .store
        .product_by_id(id)
        .map(|product| {
            NodeLoadState::Found(EntityRef::new(
                "Product",
                id,
                proxy.product_canonical_value(product),
            ))
        })
        .unwrap_or(NodeLoadState::NeedsHydration)
}

pub(crate) fn load_collection(
    proxy: &DraftProxy,
    id: &str,
    _request: Option<&Request>,
) -> NodeLoadState<EntityRef> {
    if proxy.store.collection_is_deleted(id) {
        return NodeLoadState::KnownMissing;
    }
    let value = proxy.collection_canonical_value_by_id(id);
    if value.is_null() {
        NodeLoadState::NeedsHydration
    } else {
        NodeLoadState::Found(EntityRef::new("Collection", id, value))
    }
}

pub(crate) fn load_product_variant(
    proxy: &DraftProxy,
    id: &str,
    _request: Option<&Request>,
) -> NodeLoadState<EntityRef> {
    if proxy.store.staged.product_variants.is_tombstoned(id) {
        return NodeLoadState::KnownMissing;
    }
    let value = proxy
        .store
        .product_variant_by_id(id)
        .map(|variant| proxy.product_variant_canonical_value(variant))
        .or_else(|| {
            proxy
                .owner_has_metafield_local_effects(id)
                .then(|| json!({ "__typename": "ProductVariant", "id": id }))
        });
    value.map_or(NodeLoadState::NeedsHydration, |value| {
        NodeLoadState::Found(EntityRef::new("ProductVariant", id, value))
    })
}

pub(crate) fn load_inventory_item(
    proxy: &DraftProxy,
    id: &str,
    _request: Option<&Request>,
) -> NodeLoadState<EntityRef> {
    if !proxy.inventory_item_exists(id) {
        return NodeLoadState::NeedsHydration;
    }
    NodeLoadState::Found(EntityRef::new(
        "InventoryItem",
        id,
        proxy.inventory_item_canonical_value(id),
    ))
}

pub(crate) fn load_inventory_level(
    proxy: &DraftProxy,
    id: &str,
    _request: Option<&Request>,
) -> NodeLoadState<EntityRef> {
    let value = proxy.inventory_level_value_by_id(id);
    if value.is_null() {
        NodeLoadState::NeedsHydration
    } else {
        NodeLoadState::Found(EntityRef::new("InventoryLevel", id, value))
    }
}

pub(crate) fn load_location(
    proxy: &DraftProxy,
    id: &str,
    _request: Option<&Request>,
) -> NodeLoadState<EntityRef> {
    if proxy.store.staged.locations.is_tombstoned(id) {
        return NodeLoadState::KnownMissing;
    }
    proxy
        .location_for_read(id)
        .map_or(NodeLoadState::NeedsHydration, |value| {
            NodeLoadState::Found(EntityRef::new("Location", id, value))
        })
}

pub(crate) fn load_delivery_customization(
    proxy: &DraftProxy,
    id: &str,
    _request: Option<&Request>,
) -> NodeLoadState<EntityRef> {
    if proxy.store.staged.delivery_customizations.is_tombstoned(id) {
        return NodeLoadState::KnownMissing;
    }
    proxy
        .store
        .staged
        .delivery_customizations
        .get(id)
        .cloned()
        .map(|mut value| {
            value["errorHistory"] = Value::Null;
            value["metafieldDefinitions"] = connection_json(Vec::new());
            NodeLoadState::Found(EntityRef::new("DeliveryCustomization", id, value))
        })
        .unwrap_or(NodeLoadState::NeedsHydration)
}

pub(crate) fn load_delivery_promise(
    proxy: &DraftProxy,
    id: &str,
    _request: Option<&Request>,
) -> NodeLoadState<EntityRef> {
    let Some(type_name) = registered_gid_type(
        id,
        &["DeliveryPromiseParticipant", "DeliveryPromiseProvider"],
    ) else {
        return NodeLoadState::UnsupportedType;
    };
    entity_load_state(type_name, id, proxy.delivery_promise_node_value_by_id(id))
}

pub(crate) fn load_product_feed(
    proxy: &DraftProxy,
    id: &str,
    _request: Option<&Request>,
) -> NodeLoadState<EntityRef> {
    if proxy.store.product_feed_is_tombstoned(id) {
        return NodeLoadState::KnownMissing;
    }
    proxy
        .product_feed_canonical_value(id)
        .map_or(NodeLoadState::NeedsHydration, |value| {
            NodeLoadState::Found(EntityRef::new("ProductFeed", id, value))
        })
}
pub(crate) fn load_product_operation(
    proxy: &DraftProxy,
    id: &str,
    _request: Option<&Request>,
) -> NodeLoadState<EntityRef> {
    let Some(type_name) = shopify_gid_resource_type(id).and_then(|type_name| match type_name {
        "ProductBundleOperation" => Some("ProductBundleOperation"),
        "ProductDeleteOperation" => Some("ProductDeleteOperation"),
        "ProductDuplicateOperation" => Some("ProductDuplicateOperation"),
        "ProductSetOperation" => Some("ProductSetOperation"),
        _ => None,
    }) else {
        return NodeLoadState::UnsupportedType;
    };
    proxy
        .product_operation_value_by_id(id)
        .map_or(NodeLoadState::NeedsHydration, |value| {
            NodeLoadState::Found(EntityRef::new(type_name, id, value))
        })
}

pub(crate) fn load_segment(
    proxy: &DraftProxy,
    id: &str,
    _request: Option<&Request>,
) -> NodeLoadState<EntityRef> {
    if proxy.store.staged.segments.is_tombstoned(id) {
        return NodeLoadState::KnownMissing;
    }
    proxy
        .store
        .segment_by_id(id)
        .cloned()
        .map_or(NodeLoadState::NeedsHydration, |value| {
            NodeLoadState::Found(EntityRef::new("Segment", id, value))
        })
}

pub(crate) fn load_customer_segment_members_query(
    proxy: &DraftProxy,
    id: &str,
    _request: Option<&Request>,
) -> NodeLoadState<EntityRef> {
    proxy
        .store
        .staged
        .customer_segment_member_queries
        .get(id)
        .cloned()
        .map_or(NodeLoadState::NeedsHydration, |value| {
            NodeLoadState::Found(EntityRef::new("CustomerSegmentMembersQuery", id, value))
        })
}

pub(crate) fn load_abandonment(
    proxy: &DraftProxy,
    id: &str,
    _request: Option<&Request>,
) -> NodeLoadState<EntityRef> {
    proxy
        .store
        .staged
        .abandonments
        .get(id)
        .cloned()
        .map_or(NodeLoadState::NeedsHydration, |value| {
            NodeLoadState::Found(EntityRef::new("Abandonment", id, value))
        })
}

pub(crate) fn load_order(
    proxy: &DraftProxy,
    id: &str,
    _request: Option<&Request>,
) -> NodeLoadState<EntityRef> {
    if proxy.store.staged.orders.is_tombstoned(id) {
        return NodeLoadState::KnownMissing;
    }
    proxy
        .staged_order_record_for_id(id)
        .map(|order| {
            NodeLoadState::Found(EntityRef::new(
                "Order",
                id,
                proxy.order_with_return_status_value(&order),
            ))
        })
        .unwrap_or(NodeLoadState::NeedsHydration)
}

pub(crate) fn load_backup_region(
    proxy: &DraftProxy,
    id: &str,
    _request: Option<&Request>,
) -> NodeLoadState<EntityRef> {
    let region = &proxy.store.staged.backup_region;
    if region.get("id").and_then(Value::as_str) != Some(id) {
        return NodeLoadState::NeedsHydration;
    }
    NodeLoadState::Found(EntityRef::new("MarketRegionCountry", id, region.clone()))
}

pub(crate) fn load_shopify_function(
    proxy: &DraftProxy,
    id: &str,
    _request: Option<&Request>,
) -> NodeLoadState<EntityRef> {
    proxy
        .store
        .staged
        .function_metadata
        .get(id)
        .or_else(|| proxy.store.base.function_metadata.get(id))
        .cloned()
        .map_or(NodeLoadState::NeedsHydration, |value| {
            NodeLoadState::Found(EntityRef::new("ShopifyFunction", id, value))
        })
}

pub(crate) fn load_validation(
    proxy: &DraftProxy,
    id: &str,
    _request: Option<&Request>,
) -> NodeLoadState<EntityRef> {
    if proxy
        .store
        .staged
        .deleted_function_validation_ids
        .contains(id)
    {
        return NodeLoadState::KnownMissing;
    }
    proxy
        .store
        .staged
        .function_validations
        .get(id)
        .or_else(|| proxy.store.base.function_validations.get(id))
        .or_else(|| {
            proxy
                .store
                .staged
                .function_validation
                .as_ref()
                .filter(|record| record.get("id").and_then(Value::as_str) == Some(id))
        })
        .map(|record| {
            NodeLoadState::Found(EntityRef::new(
                "Validation",
                id,
                validation_record_value(record),
            ))
        })
        .unwrap_or(NodeLoadState::NeedsHydration)
}

pub(crate) fn load_cart_transform(
    proxy: &DraftProxy,
    id: &str,
    _request: Option<&Request>,
) -> NodeLoadState<EntityRef> {
    if proxy
        .store
        .staged
        .deleted_function_cart_transform_ids
        .contains(id)
    {
        return NodeLoadState::KnownMissing;
    }
    proxy
        .store
        .staged
        .function_cart_transforms
        .get(id)
        .or_else(|| proxy.store.base.function_cart_transforms.get(id))
        .or_else(|| {
            proxy
                .store
                .staged
                .function_cart_transform
                .as_ref()
                .filter(|record| record.get("id").and_then(Value::as_str) == Some(id))
        })
        .map(|record| {
            NodeLoadState::Found(EntityRef::new(
                "CartTransform",
                id,
                cart_transform_record_value(record),
            ))
        })
        .unwrap_or(NodeLoadState::NeedsHydration)
}

pub(crate) fn load_fulfillment_constraint_rule(
    proxy: &DraftProxy,
    id: &str,
    _request: Option<&Request>,
) -> NodeLoadState<EntityRef> {
    if proxy
        .store
        .staged
        .deleted_function_fulfillment_constraint_rule_ids
        .contains(id)
    {
        return NodeLoadState::KnownMissing;
    }
    proxy
        .store
        .staged
        .function_fulfillment_constraint_rules
        .get(id)
        .or_else(|| {
            proxy
                .store
                .base
                .function_fulfillment_constraint_rules
                .get(id)
        })
        .map(|record| {
            NodeLoadState::Found(EntityRef::new(
                "FulfillmentConstraintRule",
                id,
                fulfillment_constraint_rule_record_value(record),
            ))
        })
        .unwrap_or(NodeLoadState::NeedsHydration)
}

pub(crate) fn load_tax_app_configuration(
    proxy: &DraftProxy,
    id: &str,
    _request: Option<&Request>,
) -> NodeLoadState<EntityRef> {
    proxy
        .store
        .staged
        .tax_app_configuration
        .as_ref()
        .filter(|configuration| configuration["id"].as_str() == Some(id))
        .cloned()
        .map_or(NodeLoadState::NeedsHydration, |value| {
            NodeLoadState::Found(EntityRef::new("TaxAppConfiguration", id, value))
        })
}

pub(crate) fn load_media(
    proxy: &DraftProxy,
    id: &str,
    _request: Option<&Request>,
) -> NodeLoadState<EntityRef> {
    if proxy.store.staged.media_files.is_tombstoned(id) {
        return NodeLoadState::KnownMissing;
    }
    let Some(type_name) = shopify_gid_resource_type(id).and_then(|type_name| match type_name {
        "ExternalVideo" => Some("ExternalVideo"),
        "GenericFile" => Some("GenericFile"),
        "MediaImage" => Some("MediaImage"),
        "Model3d" => Some("Model3d"),
        "Video" => Some("Video"),
        _ => None,
    }) else {
        return NodeLoadState::UnsupportedType;
    };
    proxy
        .store
        .staged
        .media_files
        .get(id)
        .cloned()
        .map_or(NodeLoadState::NeedsHydration, |value| {
            NodeLoadState::Found(EntityRef::new(type_name, id, value))
        })
}

pub(crate) fn load_known_null(
    _proxy: &DraftProxy,
    _id: &str,
    _request: Option<&Request>,
) -> NodeLoadState<EntityRef> {
    NodeLoadState::KnownMissing
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loader_type_names_are_unique() {
        let mut type_names = default_node_resolver_inventory()
            .iter()
            .map(|registration| registration.type_name)
            .collect::<Vec<_>>();
        let original_len = type_names.len();
        type_names.sort_unstable();
        type_names.dedup();
        assert_eq!(type_names.len(), original_len);
        for registration in default_node_resolver_inventory() {
            let _loader = registration.loader;
        }
    }
}

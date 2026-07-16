use super::*;

use crate::node_resolver_inventory::{default_node_resolver_inventory, NodeResolverBehavior};

#[derive(Debug, Clone, PartialEq)]
pub(in crate::proxy) enum NodeLoadState {
    Found(Value),
    KnownMissing,
    NeedsHydration,
    UnsupportedType,
}

fn observed_node_values(response: &Response) -> Vec<Value> {
    let mut nodes = response
        .body
        .pointer("/data/nodes")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .cloned()
        .collect::<Vec<_>>();
    if let Some(node) = response
        .body
        .pointer("/data/node")
        .filter(|node| node.is_object())
    {
        nodes.push(node.clone());
    }
    for pointer in ["/data/productByIdentifier", "/data/productByHandle"] {
        if let Some(node) = response
            .body
            .pointer(pointer)
            .filter(|node| node.is_object())
        {
            nodes.push(node.clone());
        }
    }
    nodes
}

pub(in crate::proxy) fn local_node_value(
    id: &str,
    selection: &[SelectedField],
    backup_region: Option<&Value>,
) -> Option<Value> {
    if is_safe_no_data_node_gid(id) {
        return Some(Value::Null);
    }
    if let Some(region) = backup_region {
        if region.get("id").and_then(Value::as_str) == Some(id) {
            return Some(selected_json(region, selection));
        }
    }
    None
}

fn is_safe_no_data_node_gid(id: &str) -> bool {
    [
        "gid://shopify/CashTrackingSession/",
        "gid://shopify/PointOfSaleDevice/",
        "gid://shopify/ShopifyPaymentsDispute/",
    ]
    .iter()
    .any(|prefix| id.starts_with(prefix))
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
                    match self.node_load_state(&id, &field.selection, request) {
                        NodeLoadState::Found(value) => value,
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
                        .map(
                            |id| match self.node_load_state(&id, &field.selection, request) {
                                NodeLoadState::Found(value) => Some(value),
                                NodeLoadState::KnownMissing => Some(Value::Null),
                                NodeLoadState::NeedsHydration | NodeLoadState::UnsupportedType
                                    if allow_unknown_null =>
                                {
                                    Some(Value::Null)
                                }
                                NodeLoadState::NeedsHydration | NodeLoadState::UnsupportedType => {
                                    None
                                }
                            },
                        )
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
                    Some(match self.node_load_state(&id, &field.selection, request) {
                        NodeLoadState::Found(value) => value,
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
                        .map(|(index, id)| {
                            match self.node_load_state(&id, &field.selection, request) {
                                NodeLoadState::Found(value) => value,
                                NodeLoadState::KnownMissing => Value::Null,
                                NodeLoadState::NeedsHydration | NodeLoadState::UnsupportedType => {
                                    upstream_nodes
                                        .and_then(|nodes| nodes.get(index))
                                        .cloned()
                                        .unwrap_or(Value::Null)
                                }
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
        match self.node_load_state(id, selection, request) {
            NodeLoadState::Found(value) => Some(value),
            NodeLoadState::KnownMissing => Some(Value::Null),
            NodeLoadState::NeedsHydration | NodeLoadState::UnsupportedType => None,
        }
    }

    fn node_load_state(
        &self,
        id: &str,
        selection: &[SelectedField],
        request: Option<&Request>,
    ) -> NodeLoadState {
        registered_node_value(self, id, selection, request)
    }

    pub(in crate::proxy) fn observe_nodes_response(&mut self, response: &Response) {
        let nodes = observed_node_values(response);
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
        selection: &[SelectedField],
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
                return Some(current_app_installation_json(
                    installation,
                    &self.store.staged.app_subscriptions,
                    &self.store.staged.app_one_time_purchases,
                    &revoked_access_scopes,
                    selection,
                ));
            }
            if installation.pointer("/app/id").and_then(Value::as_str) == Some(id) {
                return installation
                    .get("app")
                    .map(|app| selected_json(app, selection));
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
                return Some(current_app_installation_json(
                    &installation,
                    &self.store.staged.app_subscriptions,
                    &self.store.staged.app_one_time_purchases,
                    &revoked_access_scopes,
                    selection,
                ));
            }
            if installation.pointer("/app/id").and_then(Value::as_str) == Some(id) {
                return installation
                    .get("app")
                    .map(|app| selected_json(app, selection));
            }
        }
        self.store
            .staged
            .app_subscriptions
            .get(id)
            .map(|subscription| {
                selected_json(
                    subscription,
                    &selected_fields_named(
                        selection,
                        &["__typename", "id", "status", "trialDays", "lineItems"],
                    ),
                )
            })
            .or_else(|| {
                self.store
                    .staged
                    .app_one_time_purchases
                    .get(id)
                    .map(|purchase| {
                        selected_json(
                            purchase,
                            &selected_fields_named(
                                selection,
                                &["id", "name", "status", "test", "price"],
                            ),
                        )
                    })
            })
            .or_else(|| {
                self.find_staged_app_usage_record(id).map(|usage_record| {
                    selected_json(
                        &usage_record,
                        &selected_fields_named(
                            selection,
                            &["id", "description", "price", "subscriptionLineItem"],
                        ),
                    )
                })
            })
    }
}

pub(in crate::proxy) fn registered_node_value(
    proxy: &DraftProxy,
    id: &str,
    selection: &[SelectedField],
    request: Option<&Request>,
) -> NodeLoadState {
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
    match (registration.loader)(proxy, id, selection, request) {
        Some(value) if value.is_null() => NodeLoadState::KnownMissing,
        Some(value) => NodeLoadState::Found(value),
        None if registration.behavior == NodeResolverBehavior::ReturnKnownNull => {
            NodeLoadState::KnownMissing
        }
        None => NodeLoadState::NeedsHydration,
    }
}

pub(crate) fn load_app(
    proxy: &DraftProxy,
    id: &str,
    selection: &[SelectedField],
    request: Option<&Request>,
) -> Option<Value> {
    proxy.app_node_value_by_id(id, selection, request)
}

pub(crate) fn load_online_store(
    proxy: &DraftProxy,
    id: &str,
    selection: &[SelectedField],
    _request: Option<&Request>,
) -> Option<Value> {
    proxy.online_store_content_node_value(id, selection)
}

macro_rules! simple_loader {
    ($name:ident, $method:ident) => {
        pub(crate) fn $name(
            proxy: &DraftProxy,
            id: &str,
            selection: &[SelectedField],
            _request: Option<&Request>,
        ) -> Option<Value> {
            proxy.$method(id, selection)
        }
    };
}

simple_loader!(load_b2b, b2b_node_value_by_id);
simple_loader!(load_customer, customer_node_value_by_id);
simple_loader!(load_customer_address, customer_address_node_value_by_id);
simple_loader!(
    load_customer_payment_method,
    customer_payment_method_node_value_by_id
);
simple_loader!(load_store_credit, store_credit_node_value_by_id);
simple_loader!(load_discount, discount_node_value_by_id);
simple_loader!(load_fulfillment_return, fulfillment_return_node_value_by_id);
simple_loader!(load_gift_card, gift_card_node_value_by_id);
simple_loader!(
    load_gift_card_transaction,
    gift_card_transaction_node_value_by_id
);
simple_loader!(load_inventory, inventory_node_value_by_id);
simple_loader!(load_metaobject, metaobject_node_value_by_id);
simple_loader!(load_shop_property, shop_property_node_value_by_id);

pub(crate) fn load_product(
    proxy: &DraftProxy,
    id: &str,
    selection: &[SelectedField],
    _request: Option<&Request>,
) -> Option<Value> {
    if proxy.store.product_is_tombstoned(id) {
        return Some(Value::Null);
    }
    let product = proxy.store.product_by_id(id)?;
    let variants = proxy.store.product_variants_for_product(id);
    Some(proxy.product_json_with_variants_and_currency_context(
        product,
        &variants,
        selection,
        &proxy.store.shop_currency_code(),
    ))
}

pub(crate) fn load_collection(
    proxy: &DraftProxy,
    id: &str,
    selection: &[SelectedField],
    _request: Option<&Request>,
) -> Option<Value> {
    if proxy.store.collection_is_deleted(id) {
        return Some(Value::Null);
    }
    proxy
        .store
        .collection_by_id(id)
        .map(|collection| proxy.collection_json_with_publication_fields(collection, selection))
}

pub(crate) fn load_product_variant(
    proxy: &DraftProxy,
    id: &str,
    selection: &[SelectedField],
    _request: Option<&Request>,
) -> Option<Value> {
    let value = proxy.product_variant_by_id_value(id, selection);
    (!value.is_null()).then_some(value)
}

pub(crate) fn load_location(
    proxy: &DraftProxy,
    id: &str,
    selection: &[SelectedField],
    _request: Option<&Request>,
) -> Option<Value> {
    if proxy.store.staged.locations.is_tombstoned(id) {
        return Some(Value::Null);
    }
    proxy
        .location_for_read(id)
        .map(|location| selected_json(&location, selection))
}

pub(crate) fn load_delivery_customization(
    proxy: &DraftProxy,
    id: &str,
    selection: &[SelectedField],
    request: Option<&Request>,
) -> Option<Value> {
    if proxy.store.staged.delivery_customizations.is_tombstoned(id) {
        return Some(Value::Null);
    }
    let customization = proxy.store.staged.delivery_customizations.get(id)?;
    let api_client_id = request.and_then(request_app_namespace_api_client_id);
    Some(selected_delivery_customization_json(
        customization,
        selection,
        api_client_id.as_deref(),
    ))
}

simple_loader!(load_product_feed, product_tail_feed_node_value);
simple_loader!(
    load_product_delete_operation,
    product_delete_operation_value_by_id
);

pub(crate) fn load_product_operation(
    proxy: &DraftProxy,
    id: &str,
    selection: &[SelectedField],
    _request: Option<&Request>,
) -> Option<Value> {
    Some(
        proxy
            .store
            .staged
            .product_operations
            .get(id)
            .map(|operation| proxy.product_operation_json(operation, selection))
            .unwrap_or(Value::Null),
    )
}

pub(crate) fn load_segment(
    proxy: &DraftProxy,
    id: &str,
    selection: &[SelectedField],
    _request: Option<&Request>,
) -> Option<Value> {
    proxy
        .store
        .segment_by_id(id)
        .map(|record| selected_json(record, selection))
}

pub(crate) fn load_customer_segment_members_query(
    proxy: &DraftProxy,
    id: &str,
    selection: &[SelectedField],
    _request: Option<&Request>,
) -> Option<Value> {
    proxy
        .store
        .staged
        .customer_segment_member_queries
        .get(id)
        .map(|record| selected_json(record, selection))
}

pub(crate) fn load_abandonment(
    proxy: &DraftProxy,
    id: &str,
    selection: &[SelectedField],
    _request: Option<&Request>,
) -> Option<Value> {
    proxy
        .store
        .staged
        .abandonments
        .get(id)
        .map(|record| selected_json(record, selection))
}

pub(crate) fn load_order(
    proxy: &DraftProxy,
    id: &str,
    selection: &[SelectedField],
    _request: Option<&Request>,
) -> Option<Value> {
    if proxy.store.staged.orders.is_tombstoned(id) {
        return Some(Value::Null);
    }
    proxy
        .staged_order_record_for_id(id)
        .map(|order| proxy.selected_order_with_return_status(&order, selection))
}

pub(crate) fn load_backup_region(
    proxy: &DraftProxy,
    id: &str,
    selection: &[SelectedField],
    _request: Option<&Request>,
) -> Option<Value> {
    local_node_value(id, selection, Some(&proxy.store.staged.backup_region))
}

pub(crate) fn load_shopify_function(
    proxy: &DraftProxy,
    id: &str,
    selection: &[SelectedField],
    _request: Option<&Request>,
) -> Option<Value> {
    proxy
        .store
        .staged
        .function_metadata
        .get(id)
        .or_else(|| proxy.store.base.function_metadata.get(id))
        .map(|record| selected_json(record, selection))
}

pub(crate) fn load_validation(
    proxy: &DraftProxy,
    id: &str,
    selection: &[SelectedField],
    _request: Option<&Request>,
) -> Option<Value> {
    if proxy
        .store
        .staged
        .deleted_function_validation_ids
        .contains(id)
    {
        return Some(Value::Null);
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
            selected_json(
                &validation_record_for_selection(record, selection),
                selection,
            )
        })
}

pub(crate) fn load_cart_transform(
    proxy: &DraftProxy,
    id: &str,
    selection: &[SelectedField],
    _request: Option<&Request>,
) -> Option<Value> {
    if proxy
        .store
        .staged
        .deleted_function_cart_transform_ids
        .contains(id)
    {
        return Some(Value::Null);
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
            selected_json(
                &cart_transform_record_for_selection(record, selection),
                selection,
            )
        })
}

pub(crate) fn load_fulfillment_constraint_rule(
    proxy: &DraftProxy,
    id: &str,
    selection: &[SelectedField],
    _request: Option<&Request>,
) -> Option<Value> {
    if proxy
        .store
        .staged
        .deleted_function_fulfillment_constraint_rule_ids
        .contains(id)
    {
        return Some(Value::Null);
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
            selected_json(
                &fulfillment_constraint_rule_record_for_selection(record, selection),
                selection,
            )
        })
}

pub(crate) fn load_tax_app_configuration(
    proxy: &DraftProxy,
    id: &str,
    selection: &[SelectedField],
    _request: Option<&Request>,
) -> Option<Value> {
    proxy
        .store
        .staged
        .tax_app_configuration
        .as_ref()
        .filter(|configuration| configuration["id"].as_str() == Some(id))
        .map(|configuration| selected_json(configuration, selection))
}

pub(crate) fn load_media(
    proxy: &DraftProxy,
    id: &str,
    selection: &[SelectedField],
    _request: Option<&Request>,
) -> Option<Value> {
    if proxy.store.staged.media_files.is_tombstoned(id) {
        return Some(Value::Null);
    }
    proxy
        .store
        .staged
        .media_files
        .get(id)
        .map(|file| selected_json(file, selection))
}

pub(crate) fn load_known_null(
    _proxy: &DraftProxy,
    id: &str,
    selection: &[SelectedField],
    _request: Option<&Request>,
) -> Option<Value> {
    local_node_value(id, selection, None)
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

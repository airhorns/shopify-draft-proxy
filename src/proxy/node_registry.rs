use super::*;

type NodeLoader = fn(&DraftProxy, &str, &[SelectedField], Option<&Request>) -> Option<Value>;

use crate::node_resolver_inventory::{default_node_resolver_inventory, NodeLoaderKey};

pub(in crate::proxy) fn registered_node_value(
    proxy: &DraftProxy,
    id: &str,
    selection: &[SelectedField],
    request: Option<&Request>,
) -> Option<Option<Value>> {
    // Shopify market-region IDs use the nested `Market/Region/...` shape even
    // though the GraphQL runtime type is `MarketRegionCountry`. Resolve that
    // exceptional identity shape at the registry boundary so domain loaders
    // remain keyed by real GraphQL type names.
    let resource_type = if id.starts_with("gid://shopify/Market/Region/") {
        "MarketRegionCountry"
    } else {
        shopify_gid_resource_type(id)?
    };
    let loader: NodeLoader = match default_node_resolver_inventory()
        .iter()
        .find(|registration| registration.type_name == resource_type)?
        .loader_key()
    {
        NodeLoaderKey::App => load_app,
        NodeLoaderKey::B2b => load_b2b,
        NodeLoaderKey::BackupRegion => load_backup_region,
        NodeLoaderKey::CartTransform => load_cart_transform,
        NodeLoaderKey::Collection => load_collection,
        NodeLoaderKey::Customer => load_customer,
        NodeLoaderKey::CustomerAddress => load_customer_address,
        NodeLoaderKey::CustomerPaymentMethod => load_customer_payment_method,
        NodeLoaderKey::CustomerSegmentMembersQuery => load_customer_segment_members_query,
        NodeLoaderKey::DeliveryCustomization => load_delivery_customization,
        NodeLoaderKey::Discount => load_discount,
        NodeLoaderKey::FulfillmentConstraintRule => load_fulfillment_constraint_rule,
        NodeLoaderKey::FulfillmentReturn => load_fulfillment_return,
        NodeLoaderKey::GiftCard => load_gift_card,
        NodeLoaderKey::GiftCardTransaction => load_gift_card_transaction,
        NodeLoaderKey::Inventory => load_inventory,
        NodeLoaderKey::KnownNull => load_known_null,
        NodeLoaderKey::Location => load_location,
        NodeLoaderKey::Media => load_media,
        NodeLoaderKey::Metaobject => load_metaobject,
        NodeLoaderKey::OnlineStore => load_online_store,
        NodeLoaderKey::Order => load_order,
        NodeLoaderKey::Product => load_product,
        NodeLoaderKey::ProductDeleteOperation => load_product_delete_operation,
        NodeLoaderKey::ProductFeed => load_product_feed,
        NodeLoaderKey::ProductOperation => load_product_operation,
        NodeLoaderKey::ProductVariant => load_product_variant,
        NodeLoaderKey::Segment => load_segment,
        NodeLoaderKey::ShopifyFunction => load_shopify_function,
        NodeLoaderKey::ShopProperty => load_shop_property,
        NodeLoaderKey::StoreCredit => load_store_credit,
        NodeLoaderKey::TaxAppConfiguration => load_tax_app_configuration,
        NodeLoaderKey::Validation => load_validation,
        NodeLoaderKey::Abandonment => load_abandonment,
    };
    Some(loader(proxy, id, selection, request))
}

fn load_app(
    proxy: &DraftProxy,
    id: &str,
    selection: &[SelectedField],
    request: Option<&Request>,
) -> Option<Value> {
    proxy.app_node_value_by_id(id, selection, request)
}

fn load_online_store(
    proxy: &DraftProxy,
    id: &str,
    selection: &[SelectedField],
    _request: Option<&Request>,
) -> Option<Value> {
    proxy.online_store_content_node_value(id, selection)
}

macro_rules! simple_loader {
    ($name:ident, $method:ident) => {
        fn $name(
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

fn load_product(
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

fn load_collection(
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

fn load_product_variant(
    proxy: &DraftProxy,
    id: &str,
    selection: &[SelectedField],
    _request: Option<&Request>,
) -> Option<Value> {
    let value = proxy.product_variant_by_id_value(id, selection);
    (!value.is_null()).then_some(value)
}

fn load_location(
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

fn load_delivery_customization(
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

fn load_product_operation(
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

fn load_segment(
    proxy: &DraftProxy,
    id: &str,
    selection: &[SelectedField],
    _request: Option<&Request>,
) -> Option<Value> {
    proxy
        .store
        .staged
        .segments
        .get(id)
        .map(|record| selected_json(record, selection))
}

fn load_customer_segment_members_query(
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

fn load_abandonment(
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

fn load_order(
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

fn load_backup_region(
    proxy: &DraftProxy,
    id: &str,
    selection: &[SelectedField],
    _request: Option<&Request>,
) -> Option<Value> {
    super::dispatch::local_node_value(id, selection, Some(&proxy.store.staged.backup_region))
}

fn load_shopify_function(
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

fn load_validation(
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

fn load_cart_transform(
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

fn load_fulfillment_constraint_rule(
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

fn load_tax_app_configuration(
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

fn load_media(
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

fn load_known_null(
    _proxy: &DraftProxy,
    id: &str,
    selection: &[SelectedField],
    _request: Option<&Request>,
) -> Option<Value> {
    super::dispatch::local_node_value(id, selection, None)
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
            let _ = registration.loader_key();
        }
    }
}

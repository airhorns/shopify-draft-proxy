use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::graphql::{
    nested_root_field_path_selection, nested_root_field_selection, parse_operation,
    parsed_document, root_field_arguments, root_field_response_key, root_field_selection,
    root_fields, variable_definition_info, OperationType, RawArgumentValue, ResolvedValue,
    RootFieldSelection, SelectedField, SourceLocation,
};
use crate::operation_registry::{
    default_registry, local_dispatch_root, operation_capability, CapabilityDomain,
    CapabilityExecution, OperationRegistryEntry,
};

pub const DEFAULT_BULK_OPERATION_RUN_MUTATION_MAX_INPUT_FILE_SIZE_BYTES: u64 = 104_857_600;
const RUST_STATE_DUMP_SCHEMA: &str = "shopify-draft-proxy-rust-state/v1";
const LOCAL_APP_SUBSCRIPTION_ACTIVATION_ID: &str = "gid://shopify/AppSubscription/expected";
const LOCAL_APP_PURCHASE_ONE_TIME_ID: &str = "gid://shopify/AppPurchaseOneTime/expected";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReadMode {
    Snapshot,
    LiveHybrid,
    Live,
}

impl ReadMode {
    fn as_json_str(&self) -> &'static str {
        match self {
            Self::Snapshot => "snapshot",
            Self::LiveHybrid => "live-hybrid",
            Self::Live => "passthrough",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UnsupportedMutationMode {
    Passthrough,
    Reject,
}

impl UnsupportedMutationMode {
    fn as_json_str(&self) -> &'static str {
        match self {
            Self::Passthrough => "passthrough",
            Self::Reject => "reject",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub read_mode: ReadMode,
    pub unsupported_mutation_mode: Option<UnsupportedMutationMode>,
    pub bulk_operation_run_mutation_max_input_file_size_bytes: Option<u64>,
    pub port: u16,
    pub shopify_admin_origin: String,
    pub snapshot_path: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            read_mode: ReadMode::Snapshot,
            unsupported_mutation_mode: Some(UnsupportedMutationMode::Passthrough),
            bulk_operation_run_mutation_max_input_file_size_bytes: Some(
                DEFAULT_BULK_OPERATION_RUN_MUTATION_MAX_INPUT_FILE_SIZE_BYTES,
            ),
            port: 4000,
            shopify_admin_origin: "https://shopify.com".to_string(),
            snapshot_path: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Request {
    pub method: String,
    pub path: String,
    pub headers: BTreeMap<String, String>,
    pub body: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Response {
    pub status: u16,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub headers: BTreeMap<String, String>,
    pub body: Value,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProductRecord {
    pub id: String,
    pub created_at: String,
    pub updated_at: String,
    pub title: String,
    pub handle: String,
    pub status: String,
    pub description_html: String,
    pub vendor: String,
    pub product_type: String,
    pub tags: Vec<String>,
    pub template_suffix: String,
    pub seo_title: String,
    pub seo_description: String,
    pub total_inventory: i64,
    pub tracks_inventory: bool,
    pub media: Vec<Value>,
    pub variants: Vec<Value>,
    pub collections: Vec<Value>,
    pub extra_fields: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProductVariantRecord {
    pub id: String,
    pub product_id: String,
    pub title: String,
    pub sku: String,
    pub barcode: Option<String>,
    pub price: String,
    pub compare_at_price: Option<String>,
    pub taxable: bool,
    pub inventory_policy: String,
    pub inventory_quantity: i64,
    pub selected_options: Vec<ProductVariantSelectedOption>,
    pub inventory_item: ProductVariantInventoryItem,
    pub extra_fields: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProductVariantSelectedOption {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProductVariantInventoryItem {
    pub id: String,
    pub tracked: bool,
    pub requires_shipping: bool,
    pub extra_fields: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct SavedSearchRecord {
    id: String,
    name: String,
    query: String,
    resource_type: String,
}

#[derive(Clone, Default)]
struct Store {
    base: BaseState,
    staged: StagedState,
}

#[derive(Clone, Default)]
struct BaseState {
    products: OrderedRecords<ProductRecord>,
    product_variants: OrderedRecords<ProductVariantRecord>,
    saved_searches: OrderedRecords<SavedSearchRecord>,
    shop: Value,
    publication_ids: BTreeSet<String>,
    publication_count: Option<usize>,
    available_locales: BTreeMap<String, String>,
    shop_locales: BTreeMap<String, Value>,
}

#[derive(Clone)]
struct StagedState {
    products: StagedRecords<ProductRecord>,
    product_variants: StagedRecords<ProductVariantRecord>,
    saved_searches: StagedRecords<SavedSearchRecord>,
    product_search_tags: BTreeMap<String, BTreeSet<String>>,
    shipping_packages: BTreeMap<String, Value>,
    deleted_shipping_package_ids: BTreeSet<String>,
    customers: BTreeMap<String, Value>,
    deleted_customer_ids: BTreeSet<String>,
    customer_orders: BTreeMap<String, Vec<Value>>,
    merged_customer_ids: BTreeMap<String, String>,
    customer_merge_requests: BTreeMap<String, Value>,
    customer_data_erasure_requests: BTreeMap<String, Value>,
    taggable_resources: BTreeMap<String, Value>,
    carrier_services: BTreeMap<String, Value>,
    deleted_carrier_service_ids: BTreeSet<String>,
    app_subscriptions: BTreeMap<String, Value>,
    app_one_time_purchases: BTreeMap<String, Value>,
    revoked_app_access_scopes: BTreeSet<String>,
    app_uninstalled: bool,
    delegate_access_tokens: BTreeMap<String, Value>,
    customer_segment_member_queries: BTreeMap<String, Value>,
    fulfillment_services: BTreeMap<String, Value>,
    fulfillment_service_locations: BTreeMap<String, Value>,
    deleted_fulfillment_service_ids: BTreeSet<String>,
    deleted_fulfillment_service_location_ids: BTreeSet<String>,
    observed_shipping_locations: BTreeMap<String, Value>,
    observed_shipping_location_order: Vec<String>,
    locations: BTreeMap<String, Value>,
    location_order: Vec<String>,
    location_limit_reached: bool,
    segments: BTreeMap<String, Value>,
    collections: BTreeMap<String, Value>,
    fulfillment_order_deadlines: BTreeMap<String, String>,
    bulk_operations: BTreeMap<String, Value>,
    bulk_operation_staged_uploads: BTreeMap<String, Option<u64>>,
    discounts: BTreeMap<String, Value>,
    discount_code_index: BTreeMap<String, String>,
    deleted_discount_ids: BTreeSet<String>,
    discount_bulk_operations: BTreeMap<String, Value>,
    discount_redeem_code_bulk_creations: BTreeMap<String, Value>,
    timestamp_discounts: BTreeMap<String, Value>,
    gift_cards: BTreeMap<String, Value>,
    markets: BTreeMap<String, Value>,
    catalogs: BTreeMap<String, Value>,
    price_lists: BTreeMap<String, Value>,
    web_presences: BTreeMap<String, Value>,
    publication_ids: BTreeSet<String>,
    created_publication_ids: BTreeSet<String>,
    shop_locales: BTreeMap<String, Value>,
    localization_translations: Vec<Value>,
    marketing_activities: BTreeMap<String, Value>,
    deleted_marketing_activity_ids: BTreeSet<String>,
    marketing_delete_all_external: bool,
    webhook_subscriptions: BTreeMap<String, Value>,
    b2b_companies: BTreeMap<String, Value>,
    b2b_locations: BTreeMap<String, Value>,
    b2b_contacts: BTreeMap<String, Value>,
    deleted_b2b_contact_ids: BTreeSet<String>,
    b2b_contact_roles: BTreeMap<String, Value>,
    b2b_contact_role_assignments: BTreeMap<String, Value>,
    deleted_b2b_contact_role_assignment_ids: BTreeSet<String>,
    next_b2b_company_id: u64,
    next_b2b_contact_id: u64,
    next_b2b_contact_role_assignment_id: u64,
    inventory_levels: BTreeMap<(String, String), BTreeMap<String, i64>>,
    inventory_quantity_updated_at: BTreeMap<(String, String, String), String>,
    next_inventory_quantity_timestamp: u64,
    inventory_transfers: BTreeMap<String, InventoryTransferRecord>,
    inventory_shipments: BTreeMap<String, InventoryShipmentRecord>,
    metaobject_definitions: BTreeMap<String, Value>,
    deleted_metaobject_definition_ids: BTreeSet<String>,
    metaobjects: BTreeMap<String, Value>,
    deleted_metaobject_ids: BTreeSet<String>,
    url_redirects: BTreeMap<String, Value>,
    url_redirect_order: Vec<String>,
    product_option_linked_metaobject_definition_ids: BTreeSet<String>,
    app_metafields: BTreeMap<(String, String, String), Value>,
    owner_metafields: BTreeMap<String, Vec<Value>>,
    deleted_owner_metafields: BTreeSet<(String, String, String)>,
    metafield_definitions: BTreeMap<(String, String), Value>,
    media_files: BTreeMap<String, Value>,
    deleted_media_file_ids: BTreeSet<String>,
    online_store_integrations: BTreeMap<String, Value>,
    product_set_updated: bool,
    product_delete_operations: BTreeMap<String, String>,
    selling_plan_group_downstream_step: usize,
    mandate_payment_keys: BTreeSet<String>,
    payment_terms: BTreeMap<String, Value>,
    payment_terms_owner_index: BTreeMap<String, String>,
    payment_reminder_schedule_ids: BTreeSet<String>,
    payment_customizations: BTreeMap<String, Value>,
    customer_payment_methods: BTreeMap<String, Value>,
    customer_payment_method_customer_index: BTreeMap<String, Vec<String>>,
    next_customer_payment_method_id: u64,
    abandonments: BTreeMap<String, Value>,
    orders: BTreeMap<String, Value>,
    deleted_order_ids: BTreeSet<String>,
    draft_orders: BTreeMap<String, Value>,
    returns: BTreeMap<String, Value>,
    returns_by_order: BTreeMap<String, Vec<String>>,
    reverse_deliveries: BTreeMap<String, Value>,
    reverse_fulfillment_orders: BTreeMap<String, Value>,
    next_order_id: u64,
    next_draft_order_id: u64,
    next_return_id: u64,
    next_return_line_item_id: u64,
    next_reverse_delivery_id: u64,
    next_reverse_delivery_line_item_id: u64,
    next_reverse_fulfillment_order_id: u64,
    next_reverse_fulfillment_order_line_item_id: u64,
    draft_order_tags: BTreeMap<String, Vec<String>>,
    next_draft_order_bulk_tag_job_id: u64,
    draft_order_complete_gateway_create_count: usize,
    order_customer_orders: BTreeMap<String, Value>,
    order_customer_cancelled_ids: BTreeSet<String>,
    order_customer_b2b_order_ids: BTreeSet<String>,
    order_customer_contact_customer_ids: BTreeSet<String>,
    next_order_customer_order_id: u64,
    order_edit_existing_order: Option<Value>,
    order_edit_existing_calculated_order: Option<Value>,
    order_payment_next_transaction_id: u64,
    order_edit_existing_mode: Option<String>,
    function_validation: Option<Value>,
    function_cart_transform: Option<Value>,
    function_validations: BTreeMap<String, Value>,
    function_validation_order: Vec<String>,
    function_cart_transforms: BTreeMap<String, Value>,
    function_cart_transform_order: Vec<String>,
    code_basic_lifecycle_status: Option<String>,
    free_shipping_code_status: Option<String>,
    free_shipping_automatic_status: Option<String>,
    redeem_code_bulk_live_added: bool,
    redeem_code_bulk_live_deleted_seed: bool,
    backup_region: Value,
    flow_signatures: Vec<Value>,
    flow_trigger_receipts: Vec<Value>,
}

#[derive(Clone)]
struct InventoryTransferRecord {
    id: String,
    name: String,
    status: String,
    origin_location_id: String,
    destination_location_id: String,
    line_items: Vec<InventoryTransferLineItemRecord>,
}

#[derive(Clone)]
struct InventoryTransferLineItemRecord {
    id: String,
    inventory_item_id: String,
    quantity: i64,
}

#[derive(Clone)]
struct InventoryShipmentRecord {
    id: String,
    name: String,
    status: String,
    transfer_id: Option<String>,
    movement_id: Option<String>,
    tracking: Option<InventoryShipmentTrackingRecord>,
    line_items: Vec<InventoryShipmentLineItemRecord>,
}

#[derive(Clone, Default)]
struct InventoryShipmentTrackingRecord {
    tracking_number: Option<String>,
    company: Option<String>,
    tracking_url: Option<String>,
    arrives_at: Option<String>,
}

#[derive(Clone)]
struct InventoryShipmentLineItemRecord {
    id: String,
    inventory_item_id: String,
    transfer_line_item_id: Option<String>,
    quantity: i64,
    accepted_quantity: i64,
    rejected_quantity: i64,
}

#[derive(Clone)]
struct OrderedRecords<T> {
    records: BTreeMap<String, T>,
    order: Vec<String>,
}

#[derive(Clone)]
struct StagedRecords<T> {
    records: BTreeMap<String, T>,
    order: Vec<String>,
    tombstones: BTreeSet<String>,
}

impl<T> Default for OrderedRecords<T> {
    fn default() -> Self {
        Self {
            records: BTreeMap::new(),
            order: Vec::new(),
        }
    }
}

impl<T> Default for StagedRecords<T> {
    fn default() -> Self {
        Self {
            records: BTreeMap::new(),
            order: Vec::new(),
            tombstones: BTreeSet::new(),
        }
    }
}

impl<T> OrderedRecords<T> {
    fn replace_ordered<I>(&mut self, records: I)
    where
        I: IntoIterator<Item = (String, T)>,
    {
        self.records.clear();
        self.order.clear();
        for (id, record) in records {
            self.insert(id, record);
        }
    }

    fn replace_with_order(&mut self, records: BTreeMap<String, T>, order: Vec<String>) {
        self.records = records;
        self.order = normalized_order(self.records.keys(), order);
    }

    fn insert(&mut self, id: String, record: T) {
        if !self.records.contains_key(&id) {
            self.order.push(id.clone());
        }
        self.records.insert(id, record);
    }

    fn get(&self, id: &str) -> Option<&T> {
        self.records.get(id)
    }

    fn ordered_values(&self) -> Vec<&T> {
        self.order
            .iter()
            .filter_map(|id| self.records.get(id))
            .collect()
    }
}

impl<T> StagedRecords<T> {
    fn replace_with_order(&mut self, records: BTreeMap<String, T>, order: Vec<String>) {
        self.records = records;
        self.order = normalized_order(self.records.keys(), order);
    }

    fn stage(&mut self, id: String, record: T) {
        self.tombstones.remove(&id);
        if !self.records.contains_key(&id) {
            self.order.push(id.clone());
        }
        self.records.insert(id, record);
    }

    fn remove_staged(&mut self, id: &str) -> Option<T> {
        self.order.retain(|ordered_id| ordered_id != id);
        self.records.remove(id)
    }

    fn tombstone(&mut self, id: String) {
        self.tombstones.insert(id);
    }

    fn get(&self, id: &str) -> Option<&T> {
        self.records.get(id)
    }

    fn contains_staged(&self, id: &str) -> bool {
        self.records.contains_key(id)
    }

    fn is_tombstoned(&self, id: &str) -> bool {
        self.tombstones.contains(id)
    }
}

impl Default for StagedState {
    fn default() -> Self {
        Self {
            products: StagedRecords::default(),
            product_variants: StagedRecords::default(),
            saved_searches: StagedRecords::default(),
            product_search_tags: BTreeMap::new(),
            shipping_packages: BTreeMap::new(),
            deleted_shipping_package_ids: BTreeSet::new(),
            customers: BTreeMap::new(),
            deleted_customer_ids: BTreeSet::new(),
            customer_orders: BTreeMap::new(),
            merged_customer_ids: BTreeMap::new(),
            customer_merge_requests: BTreeMap::new(),
            customer_data_erasure_requests: BTreeMap::new(),
            taggable_resources: BTreeMap::new(),
            carrier_services: BTreeMap::new(),
            deleted_carrier_service_ids: BTreeSet::new(),
            app_subscriptions: BTreeMap::new(),
            app_one_time_purchases: BTreeMap::new(),
            revoked_app_access_scopes: BTreeSet::new(),
            app_uninstalled: false,
            delegate_access_tokens: BTreeMap::new(),
            customer_segment_member_queries: BTreeMap::new(),
            fulfillment_services: BTreeMap::new(),
            fulfillment_service_locations: BTreeMap::new(),
            deleted_fulfillment_service_ids: BTreeSet::new(),
            deleted_fulfillment_service_location_ids: BTreeSet::new(),
            observed_shipping_locations: BTreeMap::new(),
            observed_shipping_location_order: Vec::new(),
            locations: BTreeMap::new(),
            location_order: Vec::new(),
            location_limit_reached: false,
            segments: BTreeMap::new(),
            collections: BTreeMap::new(),
            fulfillment_order_deadlines: BTreeMap::new(),
            bulk_operations: BTreeMap::new(),
            bulk_operation_staged_uploads: BTreeMap::new(),
            discounts: BTreeMap::new(),
            discount_code_index: BTreeMap::new(),
            deleted_discount_ids: BTreeSet::new(),
            discount_bulk_operations: BTreeMap::new(),
            discount_redeem_code_bulk_creations: BTreeMap::new(),
            timestamp_discounts: BTreeMap::new(),
            gift_cards: BTreeMap::new(),
            markets: BTreeMap::new(),
            catalogs: BTreeMap::new(),
            price_lists: BTreeMap::new(),
            web_presences: BTreeMap::new(),
            publication_ids: BTreeSet::new(),
            created_publication_ids: BTreeSet::new(),
            shop_locales: BTreeMap::new(),
            localization_translations: Vec::new(),
            marketing_activities: BTreeMap::new(),
            deleted_marketing_activity_ids: BTreeSet::new(),
            marketing_delete_all_external: false,
            webhook_subscriptions: BTreeMap::new(),
            b2b_companies: BTreeMap::new(),
            b2b_locations: BTreeMap::new(),
            b2b_contacts: BTreeMap::new(),
            deleted_b2b_contact_ids: BTreeSet::new(),
            b2b_contact_roles: BTreeMap::new(),
            b2b_contact_role_assignments: BTreeMap::new(),
            deleted_b2b_contact_role_assignment_ids: BTreeSet::new(),
            next_b2b_company_id: 1,
            next_b2b_contact_id: 1,
            next_b2b_contact_role_assignment_id: 1,
            inventory_levels: BTreeMap::new(),
            inventory_quantity_updated_at: BTreeMap::new(),
            next_inventory_quantity_timestamp: 0,
            inventory_transfers: BTreeMap::new(),
            inventory_shipments: BTreeMap::new(),
            metaobject_definitions: BTreeMap::new(),
            deleted_metaobject_definition_ids: BTreeSet::new(),
            metaobjects: BTreeMap::new(),
            deleted_metaobject_ids: BTreeSet::new(),
            url_redirects: BTreeMap::new(),
            url_redirect_order: Vec::new(),
            product_option_linked_metaobject_definition_ids: BTreeSet::new(),
            app_metafields: BTreeMap::new(),
            owner_metafields: BTreeMap::new(),
            deleted_owner_metafields: BTreeSet::new(),
            metafield_definitions: BTreeMap::new(),
            media_files: BTreeMap::new(),
            deleted_media_file_ids: BTreeSet::new(),
            online_store_integrations: BTreeMap::new(),
            product_set_updated: false,
            product_delete_operations: BTreeMap::new(),
            selling_plan_group_downstream_step: 0,
            mandate_payment_keys: BTreeSet::new(),
            payment_terms: BTreeMap::new(),
            payment_terms_owner_index: BTreeMap::new(),
            payment_reminder_schedule_ids: BTreeSet::new(),
            payment_customizations: BTreeMap::new(),
            customer_payment_methods: BTreeMap::new(),
            customer_payment_method_customer_index: BTreeMap::new(),
            next_customer_payment_method_id: 1,
            abandonments: BTreeMap::new(),
            orders: BTreeMap::new(),
            deleted_order_ids: BTreeSet::new(),
            draft_orders: BTreeMap::new(),
            returns: BTreeMap::new(),
            returns_by_order: BTreeMap::new(),
            reverse_deliveries: BTreeMap::new(),
            reverse_fulfillment_orders: BTreeMap::new(),
            next_order_id: 1,
            next_draft_order_id: 1,
            next_return_id: 2,
            next_return_line_item_id: 1,
            next_reverse_delivery_id: 8,
            next_reverse_delivery_line_item_id: 7,
            next_reverse_fulfillment_order_id: 5,
            next_reverse_fulfillment_order_line_item_id: 4,
            draft_order_tags: BTreeMap::new(),
            next_draft_order_bulk_tag_job_id: 1,
            draft_order_complete_gateway_create_count: 0,
            order_customer_orders: BTreeMap::new(),
            order_customer_cancelled_ids: BTreeSet::new(),
            order_customer_b2b_order_ids: BTreeSet::new(),
            order_customer_contact_customer_ids: BTreeSet::new(),
            next_order_customer_order_id: 1,
            order_edit_existing_order: None,
            order_edit_existing_calculated_order: None,
            order_payment_next_transaction_id: 3,
            order_edit_existing_mode: None,
            function_validation: None,
            function_cart_transform: None,
            function_validations: BTreeMap::new(),
            function_validation_order: Vec::new(),
            function_cart_transforms: BTreeMap::new(),
            function_cart_transform_order: Vec::new(),
            code_basic_lifecycle_status: None,
            free_shipping_code_status: None,
            free_shipping_automatic_status: None,
            redeem_code_bulk_live_added: false,
            redeem_code_bulk_live_deleted_seed: false,
            backup_region: backup_region_country("CA")
                .expect("default backup region country must be captured"),
            flow_signatures: Vec::new(),
            flow_trigger_receipts: Vec::new(),
        }
    }
}

fn effective_get<'a, T>(
    base: &'a OrderedRecords<T>,
    staged: &'a StagedRecords<T>,
    id: &str,
) -> Option<&'a T> {
    if staged.is_tombstoned(id) {
        return None;
    }
    staged.get(id).or_else(|| base.get(id))
}

fn effective_records<T: Clone>(base: &OrderedRecords<T>, staged: &StagedRecords<T>) -> Vec<T> {
    let mut records = Vec::new();
    for (id, record) in base
        .order
        .iter()
        .filter_map(|id| base.records.get(id).map(|record| (id.as_str(), record)))
    {
        if staged.is_tombstoned(id) || staged.contains_staged(id) {
            continue;
        }
        records.push(record.clone());
    }
    for (id, record) in staged
        .order
        .iter()
        .filter_map(|id| staged.records.get(id).map(|record| (id.as_str(), record)))
    {
        if staged.is_tombstoned(id) {
            continue;
        }
        records.push(record.clone());
    }
    records
}

fn effective_count<T>(base: &OrderedRecords<T>, staged: &StagedRecords<T>) -> usize {
    base.records
        .keys()
        .filter(|id| !staged.is_tombstoned(id) && !staged.contains_staged(id))
        .count()
        + staged
            .records
            .keys()
            .filter(|id| !staged.is_tombstoned(id))
            .count()
}

fn normalized_order<'a, I>(record_ids: I, order: Vec<String>) -> Vec<String>
where
    I: IntoIterator<Item = &'a String>,
{
    let ids = record_ids.into_iter().cloned().collect::<BTreeSet<_>>();
    let mut normalized = Vec::new();
    for id in order {
        if ids.contains(&id) && !normalized.contains(&id) {
            normalized.push(id);
        }
    }
    for id in ids {
        if !normalized.contains(&id) {
            normalized.push(id);
        }
    }
    normalized
}

fn default_shop_json() -> Value {
    json!({
        "id": "gid://shopify/Shop/92891250994",
        "name": "harry-test-heelo",
        "myshopifyDomain": "harry-test-heelo.myshopify.com",
        "currencyCode": "USD"
    })
}

impl Store {
    fn with_default_baseline() -> Self {
        let mut store = Self::default();
        store.base.shop = default_shop_json();
        store.base.available_locales = default_available_locales();
        store.base.shop_locales.insert(
            "en".to_string(),
            json!({
                "locale": "en",
                "name": "English",
                "primary": true,
                "published": true,
                "marketWebPresences": [{
                    "id": "gid://shopify/MarketWebPresence/62842765618",
                    "subfolderSuffix": null
                }]
            }),
        );
        store
    }

    fn clear_staged(&mut self) {
        self.staged = StagedState::default();
    }

    fn replace_base_products(&mut self, products: Vec<ProductRecord>) {
        self.base.products.replace_ordered(
            products
                .into_iter()
                .map(|product| (product.id.clone(), product)),
        );
    }

    fn replace_base_products_map_with_order(
        &mut self,
        products: BTreeMap<String, ProductRecord>,
        order: Vec<String>,
    ) {
        self.base.products.replace_with_order(products, order);
    }

    fn replace_base_product_variants_map_with_order(
        &mut self,
        variants: BTreeMap<String, ProductVariantRecord>,
        order: Vec<String>,
    ) {
        self.base
            .product_variants
            .replace_with_order(variants, order);
    }

    fn replace_staged_products_map_with_order(
        &mut self,
        products: BTreeMap<String, ProductRecord>,
        order: Vec<String>,
    ) {
        self.staged.products.replace_with_order(products, order);
    }

    fn replace_staged_product_variants_map_with_order(
        &mut self,
        variants: BTreeMap<String, ProductVariantRecord>,
        order: Vec<String>,
    ) {
        self.staged
            .product_variants
            .replace_with_order(variants, order);
    }

    fn replace_base_saved_searches_map_with_order(
        &mut self,
        saved_searches: BTreeMap<String, SavedSearchRecord>,
        order: Vec<String>,
    ) {
        self.base
            .saved_searches
            .replace_with_order(saved_searches, order);
    }

    fn replace_staged_saved_searches_map_with_order(
        &mut self,
        saved_searches: BTreeMap<String, SavedSearchRecord>,
        order: Vec<String>,
    ) {
        self.staged
            .saved_searches
            .replace_with_order(saved_searches, order);
    }

    fn replace_product_tombstones(&mut self, ids: BTreeSet<String>) {
        self.staged.products.tombstones = ids;
    }

    fn replace_product_variant_tombstones(&mut self, ids: BTreeSet<String>) {
        self.staged.product_variants.tombstones = ids;
    }

    fn replace_saved_search_tombstones(&mut self, ids: BTreeSet<String>) {
        self.staged.saved_searches.tombstones = ids;
    }

    fn stage_created_publication_id(&mut self, id: String) {
        self.staged.created_publication_ids.insert(id.clone());
        self.staged.publication_ids.insert(id);
    }

    fn effective_publication_count(&self) -> usize {
        let base_count = self
            .base
            .publication_count
            .unwrap_or(self.base.publication_ids.len());
        base_count
            + self
                .staged
                .created_publication_ids
                .iter()
                .filter(|id| !self.base.publication_ids.contains(*id))
                .count()
    }

    fn effective_shop(&self) -> Value {
        let mut shop = self.base.shop.clone();
        shop["publicationCount"] = json!(self.effective_publication_count());
        shop
    }

    fn product_by_id(&self, id: &str) -> Option<&ProductRecord> {
        effective_get(&self.base.products, &self.staged.products, id)
    }

    fn product_by_handle(&self, handle: &str) -> Option<&ProductRecord> {
        self.staged
            .products
            .order
            .iter()
            .filter(|id| !self.staged.products.is_tombstoned(id))
            .filter_map(|id| self.staged.products.get(id))
            .find(|product| product.handle == handle)
            .or_else(|| {
                self.base
                    .products
                    .order
                    .iter()
                    .filter(|id| {
                        !self.staged.products.is_tombstoned(id)
                            && !self.staged.products.contains_staged(id)
                    })
                    .filter_map(|id| self.base.products.get(id))
                    .find(|product| product.handle == handle)
            })
    }

    fn products(&self) -> Vec<ProductRecord> {
        effective_records(&self.base.products, &self.staged.products)
    }

    fn product_count(&self) -> usize {
        effective_count(&self.base.products, &self.staged.products)
    }

    fn has_product_state(&self) -> bool {
        !self.base.products.records.is_empty()
            || !self.staged.products.records.is_empty()
            || !self.staged.products.tombstones.is_empty()
    }

    fn has_product(&self, id: &str) -> bool {
        self.product_by_id(id).is_some()
    }

    fn stage_product(&mut self, product: ProductRecord) {
        self.staged.products.stage(product.id.clone(), product);
    }

    fn stage_observed_product(&mut self, product: ProductRecord) {
        let merged = match self.product_by_id(&product.id).cloned() {
            Some(existing) => merge_observed_product(existing, product),
            None => product,
        };
        self.stage_product(merged);
    }

    fn stage_observed_product_json(&mut self, value: &Value) {
        if let Some(product) = product_state_from_json(value) {
            self.stage_observed_product(product);
        }
    }

    fn stage_collection_membership(&mut self, collection: Value, products: Vec<ProductRecord>) {
        let Some(collection_id) = collection
            .get("id")
            .and_then(Value::as_str)
            .map(str::to_string)
        else {
            return;
        };

        let mut normalized_products = Vec::new();
        for mut product in products {
            upsert_minimal_collection(&mut product.collections, &collection);
            normalized_products.push(product);
        }

        let product_nodes = normalized_products
            .iter()
            .map(product_summary_json)
            .collect::<Vec<_>>();
        let mut collection_record = self
            .staged
            .collections
            .get(&collection_id)
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        if let Some(observed) = collection.as_object() {
            for (key, value) in observed {
                if key == "products"
                    && value
                        .get("nodes")
                        .and_then(Value::as_array)
                        .map(Vec::is_empty)
                        .unwrap_or(false)
                    && collection_record.contains_key("products")
                {
                    continue;
                }
                collection_record.insert(key.clone(), value.clone());
            }
        }
        if !product_nodes.is_empty() || !collection_record.contains_key("products") {
            collection_record.insert(
                "products".to_string(),
                connection_json(product_nodes.clone()),
            );
        }
        collection_record
            .entry("defaultProducts".to_string())
            .or_insert_with(|| connection_json(product_nodes.clone()));
        collection_record
            .entry("manualProducts".to_string())
            .or_insert_with(|| connection_json(product_nodes));
        collection_record.insert(
            "productsCount".to_string(),
            json!({"count": normalized_products.len(), "precision": "EXACT"}),
        );
        self.staged
            .collections
            .insert(collection_id, Value::Object(collection_record));

        for product in normalized_products {
            self.stage_observed_product(product);
        }
    }

    fn delete_product(&mut self, id: &str) {
        self.staged.products.remove_staged(id);
        self.staged.products.tombstone(id.to_string());
    }

    fn product_staged_or_base(&self, id: &str) -> Option<ProductRecord> {
        self.product_by_id(id).cloned()
    }

    fn product_variant_by_id(&self, id: &str) -> Option<&ProductVariantRecord> {
        effective_get(
            &self.base.product_variants,
            &self.staged.product_variants,
            id,
        )
    }

    fn product_variant_by_inventory_item_id(
        &self,
        inventory_item_id: &str,
    ) -> Option<&ProductVariantRecord> {
        self.staged
            .product_variants
            .order
            .iter()
            .filter(|id| !self.staged.product_variants.is_tombstoned(id))
            .filter_map(|id| self.staged.product_variants.get(id))
            .find(|variant| variant.inventory_item.id == inventory_item_id)
            .or_else(|| {
                self.base
                    .product_variants
                    .order
                    .iter()
                    .filter(|id| {
                        !self.staged.product_variants.is_tombstoned(id)
                            && !self.staged.product_variants.contains_staged(id)
                    })
                    .filter_map(|id| self.base.product_variants.get(id))
                    .find(|variant| variant.inventory_item.id == inventory_item_id)
            })
    }

    fn product_variants_for_product(&self, product_id: &str) -> Vec<ProductVariantRecord> {
        effective_records(&self.base.product_variants, &self.staged.product_variants)
            .into_iter()
            .filter(|variant| variant.product_id == product_id)
            .collect()
    }

    fn stage_product_variant(&mut self, variant: ProductVariantRecord) {
        self.staged
            .product_variants
            .stage(variant.id.clone(), variant);
    }

    fn delete_product_variant(&mut self, id: &str) -> bool {
        let existed = self.product_variant_by_id(id).is_some();
        self.staged.product_variants.remove_staged(id);
        if existed {
            self.staged.product_variants.tombstone(id.to_string());
        }
        existed
    }

    fn saved_search_base_with_defaults(
        &self,
        resource_type: &str,
    ) -> OrderedRecords<SavedSearchRecord> {
        let mut base = OrderedRecords::default();
        for record in default_saved_searches(resource_type) {
            base.insert(record.id.clone(), record);
        }
        for record in self.base.saved_searches.ordered_values() {
            if record.resource_type == resource_type {
                base.insert(record.id.clone(), record.clone());
            }
        }
        base
    }

    fn saved_search_by_id(&self, id: &str) -> Option<SavedSearchRecord> {
        if self.staged.saved_searches.is_tombstoned(id) {
            return None;
        }
        self.staged
            .saved_searches
            .get(id)
            .cloned()
            .or_else(|| self.base.saved_searches.get(id).cloned())
            .or_else(|| default_saved_search_by_id(id))
    }

    fn saved_searches_for_resource(&self, resource_type: &str) -> Vec<SavedSearchRecord> {
        let base = self.saved_search_base_with_defaults(resource_type);
        effective_records(&base, &self.staged.saved_searches)
            .into_iter()
            .filter(|record| record.resource_type == resource_type)
            .collect()
    }

    fn stage_saved_search(&mut self, record: SavedSearchRecord) {
        self.staged.saved_searches.stage(record.id.clone(), record);
    }

    fn delete_saved_search(&mut self, id: &str) -> bool {
        let had_staged = self.staged.saved_searches.remove_staged(id).is_some();
        let has_base =
            self.base.saved_searches.get(id).is_some() || default_saved_search_by_id(id).is_some();
        if has_base {
            self.staged.saved_searches.tombstone(id.to_string());
        }
        had_staged || has_base
    }
}

type ProxyTransport = Arc<dyn Fn(Request) -> Response + Send + Sync>;

type CommitTransport = ProxyTransport;
type UpstreamTransport = ProxyTransport;

pub(in crate::proxy) struct OrdersLocalLogOutcome<'a> {
    status: &'a str,
    notes: &'a str,
}

pub(in crate::proxy) struct MutationOutcome {
    response: Response,
    log_drafts: Vec<LogDraft>,
}

pub(in crate::proxy) struct MutationFieldOutcome {
    value: Value,
    log_draft: Option<LogDraft>,
}

pub(in crate::proxy) struct LogDraft {
    root_field: String,
    staged_resource_ids: Vec<String>,
    status: String,
    capability_domain: String,
    capability_execution: String,
    notes: String,
}

impl MutationOutcome {
    fn response(response: Response) -> Self {
        Self {
            response,
            log_drafts: Vec::new(),
        }
    }

    fn staged(response: Response, log_draft: LogDraft) -> Self {
        Self {
            response,
            log_drafts: vec![log_draft],
        }
    }

    fn with_log_drafts(response: Response, log_drafts: Vec<LogDraft>) -> Self {
        Self {
            response,
            log_drafts,
        }
    }
}

impl MutationFieldOutcome {
    fn unlogged(value: Value) -> Self {
        Self {
            value,
            log_draft: None,
        }
    }

    fn staged(value: Value, log_draft: LogDraft) -> Self {
        Self {
            value,
            log_draft: Some(log_draft),
        }
    }
}

impl LogDraft {
    fn staged(
        root_field: impl Into<String>,
        domain: &'static str,
        staged_resource_ids: Vec<String>,
    ) -> Self {
        Self {
            root_field: root_field.into(),
            staged_resource_ids,
            status: "staged".to_string(),
            capability_domain: domain.to_string(),
            capability_execution: "stage-locally".to_string(),
            notes: "Supported mutation staged locally; commit replays the original raw mutation."
                .to_string(),
        }
    }
}

fn default_commit_transport(_request: Request) -> Response {
    json_error(501, "No Rust commit transport configured")
}

fn default_upstream_transport(_request: Request) -> Response {
    json_error(502, "No Rust upstream transport configured")
}

#[derive(Clone)]
pub struct DraftProxy {
    config: Config,
    log_entries: Vec<Value>,
    registry: Vec<OperationRegistryEntry>,
    store: Store,
    next_synthetic_id: u64,
    commit_transport: CommitTransport,
    upstream_transport: UpstreamTransport,
}

mod admin_shipping_gift_cards;
mod app_shipping_helpers;
mod b2b_customers;
mod commit;
mod connection;
mod core;
mod discounts;
mod dispatch;
mod localization_markets_catalogs;
mod marketing_webhooks_inventory;
mod markets_online_inventory;
mod media_products_saved_searches;
mod metafield_metaobject_definitions;
mod metafields_orders_payments;
mod metaobjects;
mod online_store_orders_payments;
mod product_helpers;
mod product_options;
mod resolved_values;
mod resource_ids;
mod routing;
mod schema_validation;
mod selection;

#[allow(unused_imports)]
pub(in crate::proxy) use self::admin_shipping_gift_cards::*;
#[allow(unused_imports)]
pub(in crate::proxy) use self::app_shipping_helpers::*;
#[allow(unused_imports)]
pub(in crate::proxy) use self::b2b_customers::*;
#[allow(unused_imports)]
pub(in crate::proxy) use self::commit::*;
#[allow(unused_imports)]
pub(in crate::proxy) use self::connection::*;
#[allow(unused_imports)]
pub(in crate::proxy) use self::core::*;
#[allow(unused_imports)]
pub(in crate::proxy) use self::discounts::*;
#[allow(unused_imports)]
pub(in crate::proxy) use self::dispatch::*;
#[allow(unused_imports)]
pub(in crate::proxy) use self::localization_markets_catalogs::*;
#[allow(unused_imports)]
pub(in crate::proxy) use self::marketing_webhooks_inventory::*;
#[allow(unused_imports)]
pub(in crate::proxy) use self::markets_online_inventory::*;
#[allow(unused_imports)]
pub(in crate::proxy) use self::media_products_saved_searches::*;
#[allow(unused_imports)]
pub(in crate::proxy) use self::metafield_metaobject_definitions::*;
#[allow(unused_imports)]
pub(in crate::proxy) use self::metafields_orders_payments::*;
#[allow(unused_imports)]
pub(in crate::proxy) use self::metaobjects::*;
#[allow(unused_imports)]
pub(in crate::proxy) use self::online_store_orders_payments::*;
#[allow(unused_imports)]
pub(in crate::proxy) use self::product_helpers::*;
#[allow(unused_imports)]
pub(in crate::proxy) use self::product_options::*;
#[allow(unused_imports)]
pub(in crate::proxy) use self::resolved_values::*;
#[allow(unused_imports)]
pub(in crate::proxy) use self::resource_ids::*;
#[allow(unused_imports)]
pub(in crate::proxy) use self::routing::*;
#[allow(unused_imports)]
pub(in crate::proxy) use self::schema_validation::*;
#[allow(unused_imports)]
pub(in crate::proxy) use self::selection::*;

#[cfg(test)]
mod store_tests {
    use super::*;

    fn product(id: &str, title: &str, handle: &str) -> ProductRecord {
        ProductRecord {
            id: id.to_string(),
            created_at: default_product_timestamp(id),
            updated_at: default_product_timestamp(id),
            title: title.to_string(),
            handle: handle.to_string(),
            status: "ACTIVE".to_string(),
            description_html: String::new(),
            vendor: String::new(),
            product_type: String::new(),
            tags: Vec::new(),
            template_suffix: String::new(),
            seo_title: String::new(),
            seo_description: String::new(),
            total_inventory: 0,
            tracks_inventory: false,
            variants: Vec::new(),
            media: Vec::new(),
            collections: Vec::new(),
            extra_fields: BTreeMap::new(),
        }
    }

    fn saved_search(id: &str, name: &str, resource_type: &str) -> SavedSearchRecord {
        SavedSearchRecord {
            id: id.to_string(),
            name: name.to_string(),
            query: "tag:promo".to_string(),
            resource_type: resource_type.to_string(),
        }
    }

    fn snapshot_proxy() -> DraftProxy {
        DraftProxy::new(Config {
            read_mode: ReadMode::Snapshot,
            unsupported_mutation_mode: None,
            bulk_operation_run_mutation_max_input_file_size_bytes: None,
            port: 0,
            shopify_admin_origin: "https://shopify.com".to_string(),
            snapshot_path: None,
        })
    }

    fn request(method: &str, path: &str, body: &str) -> Request {
        Request {
            method: method.to_string(),
            path: path.to_string(),
            headers: BTreeMap::new(),
            body: body.to_string(),
        }
    }

    fn graphql_request(query: &str, variables: Value) -> Request {
        request(
            "POST",
            "/admin/api/2025-01/graphql.json",
            &json!({
                "query": query,
                "variables": variables
            })
            .to_string(),
        )
    }

    #[test]
    fn store_effective_products_stage_overrides_base_and_tombstones() {
        let mut store = Store::default();
        store.replace_base_products(vec![
            product("gid://shopify/Product/base-1", "Base one", "base-one"),
            product("gid://shopify/Product/base-2", "Base two", "base-two"),
        ]);

        store.stage_product(product(
            "gid://shopify/Product/base-1",
            "Updated one",
            "updated-one",
        ));
        store.stage_product(product(
            "gid://shopify/Product/new",
            "New product",
            "new-product",
        ));
        store.delete_product("gid://shopify/Product/base-2");

        assert_eq!(
            store
                .product_by_id("gid://shopify/Product/base-1")
                .unwrap()
                .title,
            "Updated one"
        );
        assert!(store
            .product_by_id("gid://shopify/Product/base-2")
            .is_none());
        assert_eq!(
            store
                .product_by_handle("new-product")
                .map(|record| record.id.as_str()),
            Some("gid://shopify/Product/new")
        );
        assert_eq!(
            store
                .products()
                .iter()
                .map(|record| record.id.as_str())
                .collect::<Vec<_>>(),
            vec!["gid://shopify/Product/base-1", "gid://shopify/Product/new"]
        );
        assert_eq!(store.product_count(), 2);
    }

    #[test]
    fn store_saved_searches_overlay_defaults_base_and_tombstones_in_order() {
        let mut store = Store::default();
        store.replace_base_saved_searches_map_with_order(
            BTreeMap::from([(
                "gid://shopify/SavedSearch/base".to_string(),
                saved_search("gid://shopify/SavedSearch/base", "Base products", "PRODUCT"),
            )]),
            vec!["gid://shopify/SavedSearch/base".to_string()],
        );

        store.stage_saved_search(saved_search(
            "gid://shopify/SavedSearch/base",
            "Updated base products",
            "PRODUCT",
        ));
        store.stage_saved_search(saved_search(
            "gid://shopify/SavedSearch/new",
            "New products",
            "PRODUCT",
        ));
        assert!(store.delete_saved_search("gid://shopify/SavedSearch/base"));

        assert!(store
            .saved_search_by_id("gid://shopify/SavedSearch/base")
            .is_none());
        assert_eq!(
            store
                .saved_searches_for_resource("PRODUCT")
                .iter()
                .map(|record| record.id.as_str())
                .collect::<Vec<_>>(),
            vec!["gid://shopify/SavedSearch/new"]
        );
    }

    #[test]
    fn store_clear_staged_resets_overlays_and_tombstones_without_dropping_base() {
        let mut store = Store::default();
        store.replace_base_products(vec![product(
            "gid://shopify/Product/base",
            "Base product",
            "base-product",
        )]);
        store.stage_product(product(
            "gid://shopify/Product/base",
            "Updated product",
            "updated-product",
        ));
        store.delete_product("gid://shopify/Product/base");

        store.clear_staged();

        assert_eq!(
            store
                .product_by_id("gid://shopify/Product/base")
                .unwrap()
                .title,
            "Base product"
        );
        assert!(store.staged.products.records.is_empty());
        assert!(store.staged.products.tombstones.is_empty());
    }

    #[test]
    fn store_dump_restore_round_trips_order_and_tombstones() {
        let mut proxy = snapshot_proxy().with_base_products(vec![
            product("gid://shopify/Product/base-1", "Base one", "base-one"),
            product("gid://shopify/Product/base-2", "Base two", "base-two"),
        ]);
        proxy.store.stage_product(product(
            "gid://shopify/Product/base-1",
            "Updated one",
            "updated-one",
        ));
        proxy.store.stage_product(product(
            "gid://shopify/Product/new",
            "New product",
            "new-product",
        ));
        proxy.store.delete_product("gid://shopify/Product/base-2");
        proxy.store.stage_saved_search(saved_search(
            "gid://shopify/SavedSearch/new",
            "New products",
            "PRODUCT",
        ));

        let dump = proxy.process_request(request(
            "POST",
            "/__meta/dump",
            &json!({ "createdAt": "2026-05-23T00:00:00.000Z" }).to_string(),
        ));
        assert_eq!(
            dump.body["state"]["baseState"]["productOrder"],
            json!([
                "gid://shopify/Product/base-1",
                "gid://shopify/Product/base-2"
            ])
        );
        assert_eq!(
            dump.body["state"]["stagedState"]["productOrder"],
            json!(["gid://shopify/Product/base-1", "gid://shopify/Product/new"])
        );
        assert_eq!(
            dump.body["state"]["stagedState"]["deletedProductIds"],
            json!(["gid://shopify/Product/base-2"])
        );

        let mut restored = snapshot_proxy();
        let restore =
            restored.process_request(request("POST", "/__meta/restore", &dump.body.to_string()));
        assert_eq!(restore.status, 200);
        assert_eq!(
            restored
                .store
                .products()
                .iter()
                .map(|record| record.id.as_str())
                .collect::<Vec<_>>(),
            vec!["gid://shopify/Product/base-1", "gid://shopify/Product/new"]
        );
        assert_eq!(
            restored.store.saved_searches_for_resource("PRODUCT")[0].id,
            "gid://shopify/SavedSearch/new"
        );
    }

    #[test]
    fn product_downstream_read_uses_staged_store_instead_of_operation_name_fixture() {
        let mut proxy = snapshot_proxy();
        let create = proxy.process_request(graphql_request(
            r#"
            mutation ProductCreateParityPlan($product: ProductInput!) {
              productCreate(product: $product) {
                product {
                  id
                }
                userErrors {
                  field
                  message
                }
              }
            }
            "#,
            json!({
                "product": {
                    "title": "Store backed product",
                    "handle": "store-backed-product",
                    "vendor": "Hermes",
                    "productType": "Proof",
                    "tags": ["local", "store"],
                    "seo": {
                        "title": "Store SEO",
                        "description": "Projected from store"
                    }
                }
            }),
        ));
        let id = create.body["data"]["productCreate"]["product"]["id"]
            .as_str()
            .expect("productCreate should return a staged product id")
            .to_string();

        let read = proxy.process_request(graphql_request(
            r#"
            query ProductDetailRead($id: ID!) {
              product(id: $id) {
                id
                title
                handle
                vendor
                productType
                tags
                totalInventory
                tracksInventory
                onlineStorePreviewUrl
                category {
                  id
                  fullName
                }
                seo {
                  title
                  description
                }
                variants(first: 2) {
                  nodes {
                    id
                  }
                  pageInfo {
                    hasNextPage
                    hasPreviousPage
                    startCursor
                    endCursor
                  }
                }
                metafield(namespace: "custom", key: "material") {
                  value
                }
              }
            }
            "#,
            json!({ "id": id }),
        ));

        assert_eq!(read.status, 200);
        assert_eq!(read.body["data"]["product"]["id"], json!(id));
        assert_eq!(
            read.body["data"]["product"]["title"],
            json!("Store backed product")
        );
        assert_eq!(
            read.body["data"]["product"]["handle"],
            json!("store-backed-product")
        );
        assert_eq!(read.body["data"]["product"]["vendor"], json!("Hermes"));
        assert_eq!(read.body["data"]["product"]["productType"], json!("Proof"));
        assert_eq!(
            read.body["data"]["product"]["tags"],
            json!(["local", "store"])
        );
        assert_eq!(read.body["data"]["product"]["totalInventory"], json!(0));
        assert_eq!(
            read.body["data"]["product"]["tracksInventory"],
            json!(false)
        );
        assert_eq!(
            read.body["data"]["product"]["onlineStorePreviewUrl"],
            Value::Null
        );
        assert_eq!(read.body["data"]["product"]["category"], Value::Null);
        assert_eq!(
            read.body["data"]["product"]["seo"],
            json!({ "title": "Store SEO", "description": "Projected from store" })
        );
        assert_eq!(
            read.body["data"]["product"]["variants"]["pageInfo"],
            json!({
                "hasNextPage": false,
                "hasPreviousPage": false,
                "startCursor": null,
                "endCursor": null
            })
        );
        assert_eq!(read.body["data"]["product"]["variants"]["nodes"], json!([]));
        assert_eq!(read.body["data"]["product"]["metafield"], Value::Null);
    }

    #[test]
    fn product_read_passthroughs_in_live_hybrid_when_there_is_no_local_overlay_state() {
        let upstream_body = json!({
            "data": {
                "product": {
                    "id": "gid://shopify/Product/upstream",
                    "title": "Upstream product"
                }
            }
        });
        let mut proxy = DraftProxy::new(Config {
            read_mode: ReadMode::LiveHybrid,
            unsupported_mutation_mode: Some(UnsupportedMutationMode::Passthrough),
            bulk_operation_run_mutation_max_input_file_size_bytes: None,
            port: 0,
            shopify_admin_origin: "https://shopify.com".to_string(),
            snapshot_path: None,
        })
        .with_upstream_transport({
            let upstream_body = upstream_body.clone();
            move |_| ok_json(upstream_body.clone())
        });

        let response = proxy.process_request(graphql_request(
            r#"
            query ProductDetailRead($id: ID!) {
              product(id: $id) {
                id
                title
              }
            }
            "#,
            json!({ "id": "gid://shopify/Product/upstream" }),
        ));

        assert_eq!(response.status, 200);
        assert_eq!(response.body, upstream_body);
    }

    #[test]
    fn product_variant_downstream_read_uses_staged_variant_state() {
        let mut proxy = snapshot_proxy();

        let create_product = proxy.process_request(graphql_request(
            r#"
            mutation ProductVariantUpdateSetupProduct($product: ProductCreateInput!) {
              productCreate(product: $product) {
                product {
                  id
                }
                userErrors {
                  field
                  message
                }
              }
            }
            "#,
            json!({
                "product": {
                    "title": "Store Variant Product",
                    "status": "ACTIVE"
                }
            }),
        ));
        let product_id = create_product.body["data"]["productCreate"]["product"]["id"]
            .as_str()
            .expect("product create should return product id")
            .to_string();

        let create_variant = proxy.process_request(graphql_request(
            r#"
            mutation ProductVariantUpdateSetupVariant($input: ProductVariantInput!) {
              productVariantCreate(input: $input) {
                productVariant {
                  id
                }
                userErrors {
                  field
                  message
                }
              }
            }
            "#,
            json!({
                "input": {
                    "productId": product_id,
                    "title": "Store Red",
                    "sku": "STORE-DRAFT",
                    "inventoryItem": {
                        "tracked": false,
                        "requiresShipping": true
                    }
                }
            }),
        ));
        let variant_id = create_variant.body["data"]["productVariantCreate"]["productVariant"]
            ["id"]
            .as_str()
            .expect("variant create should return variant id")
            .to_string();

        let update = proxy.process_request(graphql_request(
            r#"
            mutation ProductVariantUpdateParityPlan($input: ProductVariantInput!) {
              productVariantUpdate(input: $input) {
                product {
                  id
                  totalInventory
                  tracksInventory
                  variants(first: 10) {
                    nodes {
                      id
                      title
                      sku
                    }
                  }
                }
                productVariant {
                  id
                  title
                  sku
                  barcode
                  selectedOptions {
                    name
                    value
                  }
                  inventoryItem {
                    id
                    tracked
                    requiresShipping
                  }
                }
                userErrors {
                  field
                  message
                }
              }
            }
            "#,
            json!({
                "input": {
                    "id": variant_id,
                    "title": "Store Red",
                    "sku": "STORE-RED",
                    "barcode": "store-barcode",
                    "selectedOptions": [{ "name": "Color", "value": "Store Red" }],
                    "inventoryItem": {
                        "tracked": true,
                        "requiresShipping": false
                    }
                }
            }),
        ));
        assert_eq!(
            update.body["data"]["productVariantUpdate"]["product"]["id"],
            json!(product_id)
        );

        let read = proxy.process_request(graphql_request(
            r#"
            query ProductVariantUpdateDownstreamRead($id: ID!, $query: String!) {
              product(id: $id) {
                id
                totalInventory
                tracksInventory
                variants(first: 10) {
                  nodes {
                    id
                    title
                    sku
                    barcode
                    selectedOptions {
                      name
                      value
                    }
                    inventoryItem {
                      id
                      tracked
                      requiresShipping
                    }
                  }
                }
              }
              products(first: 10, query: $query) {
                nodes {
                  id
                }
              }
              skuCount: productsCount(query: $query) {
                count
                precision
              }
            }
            "#,
            json!({ "id": product_id, "query": "sku:STORE-RED" }),
        ));

        assert_eq!(read.status, 200);
        assert_eq!(read.body["data"]["product"]["id"], json!(product_id));
        assert_eq!(read.body["data"]["product"]["tracksInventory"], json!(true));
        assert_eq!(
            read.body["data"]["product"]["variants"]["nodes"][0]["title"],
            json!("Store Red")
        );
        assert_eq!(
            read.body["data"]["product"]["variants"]["nodes"][0]["sku"],
            json!("STORE-RED")
        );
        assert_eq!(
            read.body["data"]["product"]["variants"]["nodes"][0]["inventoryItem"]
                ["requiresShipping"],
            json!(false)
        );
        assert_eq!(
            read.body["data"]["products"]["nodes"],
            json!([{ "id": product_id }])
        );
        assert_eq!(
            read.body["data"]["skuCount"],
            json!({ "count": 1, "precision": "EXACT" })
        );
    }

    #[test]
    fn collection_downstream_read_uses_observed_passthrough_membership_state() {
        let upstream_body = json!({
            "data": {
                "collectionAddProducts": {
                    "collection": {
                        "id": "gid://shopify/Collection/store-backed",
                        "title": "Store Backed Collection",
                        "handle": "store-backed-collection",
                        "products": {
                            "nodes": [
                                {
                                    "id": "gid://shopify/Product/first",
                                    "title": "First Product",
                                    "handle": "first-product"
                                },
                                {
                                    "id": "gid://shopify/Product/second",
                                    "title": "Second Product",
                                    "handle": "second-product"
                                }
                            ],
                            "pageInfo": {
                                "hasNextPage": false,
                                "hasPreviousPage": false
                            }
                        }
                    },
                    "userErrors": []
                }
            }
        });
        let mut proxy = DraftProxy::new(Config {
            read_mode: ReadMode::LiveHybrid,
            unsupported_mutation_mode: Some(UnsupportedMutationMode::Passthrough),
            bulk_operation_run_mutation_max_input_file_size_bytes: None,
            port: 0,
            shopify_admin_origin: "https://shopify.com".to_string(),
            snapshot_path: None,
        })
        .with_upstream_transport({
            let upstream_body = upstream_body.clone();
            move |_| ok_json(upstream_body.clone())
        });

        let mutation = proxy.process_request(graphql_request(
            r#"
            mutation CollectionAddProductsParityPlan($id: ID!, $productIds: [ID!]!) {
              collectionAddProducts(id: $id, productIds: $productIds) {
                collection {
                  id
                  title
                  handle
                  products(first: 10) {
                    nodes {
                      id
                      title
                      handle
                    }
                    pageInfo {
                      hasNextPage
                      hasPreviousPage
                    }
                  }
                }
                userErrors {
                  field
                  message
                }
              }
            }
            "#,
            json!({
                "id": "gid://shopify/Collection/store-backed",
                "productIds": ["gid://shopify/Product/first", "gid://shopify/Product/second"]
            }),
        ));
        assert_eq!(mutation.status, 200);

        let read = proxy.process_request(graphql_request(
            r#"
            query CollectionAddProductsDownstream($collectionId: ID!, $firstProductId: ID!, $secondProductId: ID!) {
              collection(id: $collectionId) {
                id
                title
                handle
                products(first: 10) {
                  nodes {
                    id
                    title
                    handle
                  }
                  pageInfo {
                    hasNextPage
                    hasPreviousPage
                  }
                }
              }
              first: product(id: $firstProductId) {
                id
                collections(first: 10) {
                  nodes {
                    id
                    title
                    handle
                  }
                }
              }
              second: product(id: $secondProductId) {
                id
                collections(first: 10) {
                  nodes {
                    id
                    title
                    handle
                  }
                }
              }
            }
            "#,
            json!({
                "collectionId": "gid://shopify/Collection/store-backed",
                "firstProductId": "gid://shopify/Product/first",
                "secondProductId": "gid://shopify/Product/second"
            }),
        ));

        assert_eq!(read.status, 200);
        assert_eq!(
            read.body["data"]["collection"]["products"]["nodes"],
            json!([
                {
                    "id": "gid://shopify/Product/first",
                    "title": "First Product",
                    "handle": "first-product"
                },
                {
                    "id": "gid://shopify/Product/second",
                    "title": "Second Product",
                    "handle": "second-product"
                }
            ])
        );
        assert_eq!(
            read.body["data"]["first"]["collections"]["nodes"],
            json!([
                {
                    "id": "gid://shopify/Collection/store-backed",
                    "title": "Store Backed Collection",
                    "handle": "store-backed-collection"
                }
            ])
        );
        assert_eq!(
            read.body["data"]["second"]["collections"]["nodes"],
            read.body["data"]["first"]["collections"]["nodes"]
        );
    }
}

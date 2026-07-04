use std::{
    collections::{btree_map, BTreeMap, BTreeSet},
    sync::Arc,
};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::graphql::{
    parse_operation, parsed_document, primary_root_field, root_field_arguments, root_fields,
    variable_definition_info, OperationType, RawArgumentValue, ResolvedValue, RootFieldSelection,
    SelectedField, SourceLocation,
};
use crate::operation_registry::{
    default_registry, operation_capability, CapabilityDomain, CapabilityExecution,
    OperationRegistryEntry,
};

pub const DEFAULT_BULK_OPERATION_RUN_MUTATION_MAX_INPUT_FILE_SIZE_BYTES: u64 = 104_857_600;
pub(in crate::proxy) const METAFIELDS_SET_INPUT_LIMIT: usize = 25;
const RUST_STATE_DUMP_SCHEMA: &str = "shopify-draft-proxy-rust-state/v1";

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

fn primary_root_response_parts(
    query: &str,
    variables: &BTreeMap<String, ResolvedValue>,
    default_response_key: impl FnOnce() -> String,
) -> (String, Vec<SelectedField>, BTreeMap<String, ResolvedValue>) {
    primary_root_field(query, variables)
        .map(|field| (field.response_key, field.selection, field.arguments))
        .unwrap_or_else(|| (default_response_key(), Vec::new(), BTreeMap::new()))
}

fn primary_root_response_selection(
    query: &str,
    variables: &BTreeMap<String, ResolvedValue>,
    default_response_key: impl FnOnce() -> String,
) -> (String, Vec<SelectedField>) {
    primary_root_field(query, variables)
        .map(|field| (field.response_key, field.selection))
        .unwrap_or_else(|| (default_response_key(), Vec::new()))
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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub media_ids: Vec<String>,
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
struct SellingPlanRecord {
    id: String,
    name: String,
    description: String,
    options: Vec<String>,
    position: i64,
    category: String,
    created_at: String,
    billing_policy: Value,
    delivery_policy: Value,
    inventory_policy: Value,
    pricing_policies: Vec<Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct SellingPlanGroupRecord {
    id: String,
    app_id: Option<String>,
    name: String,
    merchant_code: String,
    description: String,
    options: Vec<String>,
    position: i64,
    created_at: String,
    selling_plans: Vec<SellingPlanRecord>,
    product_ids: Vec<String>,
    product_variant_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct ProductOperationRecord {
    id: String,
    kind: ProductOperationKind,
    product_id: Option<String>,
    new_product_id: Option<String>,
    user_errors: Vec<Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
enum ProductOperationKind {
    Set,
    Duplicate,
    Bundle,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct SavedSearchRecord {
    id: String,
    name: String,
    query: String,
    resource_type: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct ShopPolicyRecord {
    id: String,
    policy_type: String,
    title: String,
    body: String,
    url: String,
    created_at: String,
    updated_at: String,
    translations: Vec<Value>,
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
    shop_policies: OrderedRecords<ShopPolicyRecord>,
    gift_cards: BTreeMap<String, Value>,
    gift_card_configuration: Option<Value>,
    shop: Value,
    publication_ids: BTreeSet<String>,
    publication_count: Option<usize>,
    available_locales: BTreeMap<String, String>,
    shop_locales: BTreeMap<String, Value>,
    localization_product_ids: BTreeSet<String>,
}

type MetafieldDefinitionKey = (String, String, String);

#[derive(Clone)]
struct StagedState {
    products: StagedRecords<ProductRecord>,
    product_variants: StagedRecords<ProductVariantRecord>,
    product_feeds: StagedRecords<Value>,
    selling_plan_groups: StagedRecords<SellingPlanGroupRecord>,
    saved_searches: StagedRecords<SavedSearchRecord>,
    shop_policies: StagedRecords<ShopPolicyRecord>,
    shipping_packages: StagedRecords<Value>,
    customers: StagedRecords<Value>,
    customer_addresses: BTreeMap<String, Value>,
    customer_address_order: BTreeMap<String, Vec<String>>,
    customer_address_owners: BTreeMap<String, String>,
    customer_orders: BTreeMap<String, Vec<Value>>,
    merged_customer_ids: BTreeMap<String, String>,
    customer_merge_requests: BTreeMap<String, Value>,
    customer_data_erasure_requests: BTreeMap<String, Value>,
    locally_created_customer_ids: BTreeSet<String>,
    // Store-wide total customer count baseline reported by `customersCount`.
    // The live shop's total is store-specific and cannot be reconstructed from
    // the handful of customers a scenario stages, so a scenario seeds the
    // recorded baseline and the read resolver reports `base - deletions` so the
    // count tracks merges/deletes generically. `None` falls back to the legacy
    // default for scenarios that never seed a baseline.
    customers_count_base: Option<u64>,
    store_credit_accounts: StagedRecords<Value>,
    store_credit_transactions: BTreeMap<String, Value>,
    store_credit_transaction_order: Vec<String>,
    next_store_credit_account_id: u64,
    next_store_credit_transaction_id: u64,
    taggable_resources: BTreeMap<String, Value>,
    carrier_services: StagedRecords<Value>,
    installed_apps: BTreeMap<String, Value>,
    app_subscriptions: BTreeMap<String, Value>,
    app_one_time_purchases: BTreeMap<String, Value>,
    revoked_app_access_scopes: BTreeMap<String, BTreeSet<String>>,
    uninstalled_app_ids: BTreeSet<String>,
    delegate_access_tokens: BTreeMap<String, Value>,
    customer_segment_member_queries: BTreeMap<String, Value>,
    fulfillment_services: StagedRecords<Value>,
    fulfillment_service_locations: StagedRecords<Value>,
    delivery_profiles: StagedRecords<Value>,
    observed_shipping_locations: BTreeMap<String, Value>,
    observed_shipping_location_order: Vec<String>,
    locations: StagedRecords<Value>,
    location_limit_reached: bool,
    segments: BTreeMap<String, Value>,
    // Recorded segment-catalog read baselines, keyed by root field name
    // (`segments` / `segmentsCount` / `segmentFilters` / `segmentFilterSuggestions`
    // / `segmentValueSuggestions` / `segmentMigrations`). These roots expose
    // Shopify-internal catalog/derived data whose opaque pagination cursors encode
    // backend-private values (microsecond timestamps, customer ids) that cannot be
    // reconstructed from arbitrary store state, so a scenario seeds the recorded
    // connection values and the read resolver projects the requested selection over
    // them. Empty for every scenario that does not seed a catalog, leaving the
    // generic staged-segment read path untouched.
    segment_catalog: BTreeMap<String, Value>,
    collections: StagedRecords<Value>,
    collection_jobs: BTreeMap<String, Value>,
    fulfillment_order_deadlines: BTreeMap<String, String>,
    bulk_operations: BTreeMap<String, Value>,
    bulk_operation_staged_uploads: BTreeMap<String, Option<u64>>,
    bulk_operation_results: BTreeMap<String, String>,
    discounts: StagedRecords<Value>,
    discount_code_index: BTreeMap<String, String>,
    discount_redeem_code_bulk_creations: BTreeMap<String, Value>,
    gift_cards: BTreeMap<String, Value>,
    markets: BTreeMap<String, Value>,
    catalogs: BTreeMap<String, Value>,
    price_lists: BTreeMap<String, Value>,
    web_presences: BTreeMap<String, Value>,
    publication_ids: BTreeSet<String>,
    created_publication_ids: BTreeSet<String>,
    // Full publication records staged this scenario, keyed by publication gid.
    // Seeded from `seedPublications` (base/default publications) and extended by
    // `publicationCreate`. Drives the local `publication`/`channel`/`channels`/
    // `publicationsCount`/`publishedProductsCount` roots without upstream replay.
    // Empty for every scenario that does not seed publications, leaving the
    // existing passthrough behavior for those roots untouched.
    publications: BTreeMap<String, Value>,
    // Resource gid (Product/Collection) -> set of publication gids the resource
    // is published on. Seeded from `seedProducts`/`seedCollections`
    // `publicationIds` and mutated by `publishablePublish`/`publishableUnpublish`.
    // Drives `publishedOnPublication`, `resourcePublicationsCount`,
    // `publicationCount`, and per-publication membership counts.
    resource_publications: BTreeMap<String, BTreeSet<String>>,
    shop_locales: BTreeMap<String, Value>,
    localization_translations: Vec<Value>,
    // Market-localizable resources observed from a cold upstream read or mutation
    // preflight: resourceId -> the resource's `marketLocalizableContent` array. The
    // presence of a key records that the resource exists (so register/remove resolve
    // RESOURCE_NOT_FOUND for never-observed ids); the stored content keys+digests drive
    // INVALID_KEY_FOR_MODEL / digest validation without fabricating field metadata.
    localization_resources: BTreeMap<String, Value>,
    // True once a localization mutation has cleared staged state (locale disable,
    // translation remove). Keeps the proxy authoritative for localization reads so a
    // now-empty staged set is not mistaken for a cold cache that must forward upstream.
    localization_dirty: bool,
    marketing_activities: StagedRecords<Value>,
    marketing_delete_all_external: bool,
    webhook_subscriptions: BTreeMap<String, Value>,
    b2b_companies: BTreeMap<String, Value>,
    b2b_locations: StagedRecords<Value>,
    b2b_contacts: BTreeMap<String, Value>,
    b2b_contact_roles: BTreeMap<String, Value>,
    b2b_role_assignments: BTreeMap<String, Value>,
    b2b_staff_assignments: BTreeMap<String, Value>,
    next_b2b_company_id: u64,
    inventory_levels: BTreeMap<(String, String), BTreeMap<String, i64>>,
    inventory_level_order: Vec<(String, String)>,
    inventory_level_ids: BTreeMap<(String, String), String>,
    // Opaque Relay pagination cursors for InventoryLevel connection edges, keyed by
    // the level's gid. These tokens encode Shopify's internal row ids and cannot be
    // reconstructed from store state, so they are seeded from recorded captures and
    // replayed when the inventory-level connection renderer projects edges/pageInfo.
    inventory_level_cursors: BTreeMap<String, String>,
    inactive_inventory_levels: BTreeSet<(String, String)>,
    inventory_quantity_updated_at: BTreeMap<(String, String, String), String>,
    next_inventory_quantity_timestamp: u64,
    inventory_transfers: BTreeMap<String, InventoryTransferRecord>,
    inventory_shipments: BTreeMap<String, InventoryShipmentRecord>,
    metaobject_definitions: StagedRecords<Value>,
    metaobjects: StagedRecords<Value>,
    url_redirects: BTreeMap<String, Value>,
    url_redirect_order: Vec<String>,
    linked_product_option_metaobject_sets: Vec<BTreeSet<String>>,
    product_option_linked_metaobject_definition_ids: BTreeSet<String>,
    owner_metafields: BTreeMap<String, Vec<Value>>,
    deleted_owner_metafields: BTreeSet<(String, String, String)>,
    metafield_definitions: BTreeMap<MetafieldDefinitionKey, Value>,
    metafield_reference_ids: BTreeSet<String>,
    media_files: StagedRecords<Value>,
    online_store_integrations: BTreeMap<String, Value>,
    online_store_blogs: BTreeMap<String, Value>,
    online_store_blog_order: Vec<String>,
    deleted_online_store_blog_ids: BTreeSet<String>,
    online_store_blogs_count_base: Option<usize>,
    online_store_pages: BTreeMap<String, Value>,
    online_store_page_order: Vec<String>,
    deleted_online_store_page_ids: BTreeSet<String>,
    online_store_pages_count_base: Option<usize>,
    online_store_articles: BTreeMap<String, Value>,
    online_store_article_order: Vec<String>,
    deleted_online_store_article_ids: BTreeSet<String>,
    online_store_comments: BTreeMap<String, Value>,
    online_store_comment_order: Vec<String>,
    deleted_online_store_comment_ids: BTreeSet<String>,
    product_operations: BTreeMap<String, ProductOperationRecord>,
    product_delete_operations: BTreeMap<String, String>,
    mandate_payment_keys: BTreeSet<String>,
    payment_terms: BTreeMap<String, Value>,
    payment_terms_owner_index: BTreeMap<String, String>,
    payment_reminder_schedule_ids: BTreeSet<String>,
    payment_customizations: BTreeMap<String, Value>,
    customer_payment_methods: BTreeMap<String, Value>,
    customer_payment_method_customer_index: BTreeMap<String, Vec<String>>,
    next_customer_payment_method_id: u64,
    abandonments: BTreeMap<String, Value>,
    orders: StagedRecords<Value>,
    draft_orders: BTreeMap<String, Value>,
    returns: BTreeMap<String, Value>,
    returns_by_order: BTreeMap<String, Vec<String>>,
    reverse_deliveries: BTreeMap<String, Value>,
    reverse_fulfillment_orders: BTreeMap<String, Value>,
    next_refund_id: u64,
    next_refund_line_item_id: u64,
    next_order_id: u64,
    next_draft_order_id: u64,
    draft_order_tags: BTreeMap<String, Vec<String>>,
    next_draft_order_bulk_tag_job_id: u64,
    order_customer_orders: BTreeMap<String, Value>,
    order_customer_cancelled_ids: BTreeSet<String>,
    order_customer_b2b_order_ids: BTreeSet<String>,
    order_customer_contact_customer_ids: BTreeSet<String>,
    next_order_customer_order_id: u64,
    order_edit_existing_order: Option<Value>,
    order_edit_existing_calculated_order: Option<Value>,
    order_edit_existing_calculated_order_id: Option<String>,
    order_edit_existing_session_order_id: Option<String>,
    order_edit_money_bag_calculated_order_ids: BTreeMap<String, String>,
    order_payment_next_transaction_id: u64,
    order_edit_existing_mode: Option<String>,
    /// Catalog of product variants an order-edit `orderEditAddVariant` can
    /// resolve against (variant id -> {title, sku, price, currencyCode}). Seeded
    /// from a scenario's `seedOrderEditVariants` recording input so the edit
    /// engine builds added calculated line items from store state instead of
    /// echoing the recorded response. Round-tripped through dump/restore.
    order_edit_variant_catalog: Value,
    /// Identity attributed to an order-edit commit event (the "<who> edited this
    /// order." message author). Seeded per scenario; defaults empty.
    order_edit_author: Option<String>,
    function_validation: Option<Value>,
    function_cart_transform: Option<Value>,
    function_metadata: BTreeMap<String, Value>,
    function_metadata_order: Vec<String>,
    function_validations: BTreeMap<String, Value>,
    function_validation_order: Vec<String>,
    function_cart_transforms: BTreeMap<String, Value>,
    function_cart_transform_order: Vec<String>,
    function_fulfillment_constraint_rules: BTreeMap<String, Value>,
    function_fulfillment_constraint_rule_order: Vec<String>,
    // True once any function lifecycle (validation / cart-transform) has been
    // staged this session. Distinguishes a post-delete local read (serve the
    // empty local result) from a cold read with no local backing (forward to
    // the upstream so function ownership metadata reflects real installs).
    functions_dirty: bool,
    backup_region: Value,
    flow_signatures: Vec<Value>,
    flow_trigger_receipts: Vec<Value>,

    b2b_contact_role_assignments: BTreeMap<String, Value>,
    deleted_b2b_contact_ids: BTreeSet<String>,
    deleted_b2b_contact_role_assignment_ids: BTreeSet<String>,
    next_b2b_contact_id: u64,
    next_b2b_contact_role_assignment_id: u64,
}

#[derive(Clone, Serialize, Deserialize)]
struct InventoryTransferRecord {
    id: String,
    name: String,
    status: String,
    origin_location_id: String,
    destination_location_id: String,
    line_items: Vec<InventoryTransferLineItemRecord>,
}

#[derive(Clone, Serialize, Deserialize)]
struct InventoryTransferLineItemRecord {
    id: String,
    inventory_item_id: String,
    quantity: i64,
}

#[derive(Clone, Serialize, Deserialize)]
struct InventoryShipmentRecord {
    id: String,
    name: String,
    status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    transfer_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    movement_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    tracking: Option<InventoryShipmentTrackingRecord>,
    line_items: Vec<InventoryShipmentLineItemRecord>,
}

#[derive(Clone, Serialize, Deserialize)]
struct InventoryShipmentLineItemRecord {
    id: String,
    inventory_item_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    transfer_line_item_id: Option<String>,
    quantity: i64,
    accepted_quantity: i64,
    rejected_quantity: i64,
}

#[derive(Clone, Serialize, Deserialize)]
struct InventoryShipmentTrackingRecord {
    tracking_number: Option<String>,
    company: Option<String>,
    tracking_url: Option<String>,
    arrives_at: Option<String>,
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

    fn replace_tombstones(&mut self, ids: BTreeSet<String>) {
        self.tombstones = ids;
    }

    fn replace_with_order_and_tombstones(
        &mut self,
        records: BTreeMap<String, T>,
        order: Vec<String>,
        tombstones: BTreeSet<String>,
    ) {
        self.replace_with_order(records, order);
        self.replace_tombstones(tombstones);
    }

    fn stage(&mut self, id: String, record: T) -> Option<T> {
        self.tombstones.remove(&id);
        if !self.records.contains_key(&id) {
            self.order.push(id.clone());
        }
        self.records.insert(id, record)
    }

    fn insert(&mut self, id: String, record: T) -> Option<T> {
        self.stage(id, record)
    }

    fn remove_staged(&mut self, id: &str) -> Option<T> {
        self.order.retain(|ordered_id| ordered_id != id);
        self.records.remove(id)
    }

    fn tombstone(&mut self, id: String) {
        self.tombstones.insert(id);
    }

    fn tombstone_staged(&mut self, id: &str) -> bool {
        let existed = self.remove_staged(id).is_some();
        if existed {
            self.tombstone(id.to_string());
        }
        existed
    }

    fn get(&self, id: &str) -> Option<&T> {
        if self.is_tombstoned(id) {
            return None;
        }
        self.records.get(id)
    }

    fn get_mut(&mut self, id: &str) -> Option<&mut T> {
        if self.is_tombstoned(id) {
            return None;
        }
        self.records.get_mut(id)
    }

    fn contains_key(&self, id: &str) -> bool {
        !self.is_tombstoned(id) && self.records.contains_key(id)
    }

    fn contains_staged(&self, id: &str) -> bool {
        self.records.contains_key(id)
    }

    fn is_tombstoned(&self, id: &str) -> bool {
        self.tombstones.contains(id)
    }

    fn remove(&mut self, id: &str) -> Option<T> {
        self.remove_staged(id)
    }

    fn entry(&mut self, id: String) -> btree_map::Entry<'_, String, T> {
        self.tombstones.remove(&id);
        if !self.records.contains_key(&id) {
            self.order.push(id.clone());
        }
        self.records.entry(id)
    }

    fn iter(&self) -> impl Iterator<Item = (&String, &T)> {
        self.records
            .iter()
            .filter(|(id, _)| !self.tombstones.contains(*id))
    }

    fn values(&self) -> impl Iterator<Item = &T> {
        self.iter().map(|(_, record)| record)
    }

    fn is_empty(&self) -> bool {
        self.records.is_empty() && self.tombstones.is_empty()
    }

    fn len(&self) -> usize {
        self.records
            .keys()
            .filter(|id| !self.tombstones.contains(*id))
            .count()
    }
}

impl<'a, T> IntoIterator for &'a StagedRecords<T> {
    type Item = (&'a String, &'a T);
    type IntoIter = btree_map::Iter<'a, String, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.records.iter()
    }
}

impl Default for StagedState {
    fn default() -> Self {
        Self {
            products: StagedRecords::default(),
            product_variants: StagedRecords::default(),
            product_feeds: StagedRecords::default(),
            selling_plan_groups: StagedRecords::default(),
            saved_searches: StagedRecords::default(),
            shop_policies: StagedRecords::default(),
            shipping_packages: StagedRecords::default(),
            customers: StagedRecords::default(),
            customer_addresses: BTreeMap::new(),
            customer_address_order: BTreeMap::new(),
            customer_address_owners: BTreeMap::new(),
            customer_orders: BTreeMap::new(),
            merged_customer_ids: BTreeMap::new(),
            customer_merge_requests: BTreeMap::new(),
            customer_data_erasure_requests: BTreeMap::new(),
            locally_created_customer_ids: BTreeSet::new(),
            customers_count_base: None,
            store_credit_accounts: StagedRecords::default(),
            store_credit_transactions: BTreeMap::new(),
            store_credit_transaction_order: Vec::new(),
            next_store_credit_account_id: 1,
            next_store_credit_transaction_id: 1,
            taggable_resources: BTreeMap::new(),
            carrier_services: StagedRecords::default(),
            installed_apps: BTreeMap::new(),
            app_subscriptions: BTreeMap::new(),
            app_one_time_purchases: BTreeMap::new(),
            revoked_app_access_scopes: BTreeMap::new(),
            uninstalled_app_ids: BTreeSet::new(),
            delegate_access_tokens: BTreeMap::new(),
            customer_segment_member_queries: BTreeMap::new(),
            fulfillment_services: StagedRecords::default(),
            fulfillment_service_locations: StagedRecords::default(),
            delivery_profiles: StagedRecords::default(),
            observed_shipping_locations: BTreeMap::new(),
            observed_shipping_location_order: Vec::new(),
            locations: StagedRecords::default(),
            location_limit_reached: false,
            segments: BTreeMap::new(),
            segment_catalog: BTreeMap::new(),
            collections: StagedRecords::default(),
            collection_jobs: BTreeMap::new(),
            fulfillment_order_deadlines: BTreeMap::new(),
            bulk_operations: BTreeMap::new(),
            bulk_operation_staged_uploads: BTreeMap::new(),
            bulk_operation_results: BTreeMap::new(),
            discounts: StagedRecords::default(),
            discount_code_index: BTreeMap::new(),
            discount_redeem_code_bulk_creations: BTreeMap::new(),
            gift_cards: BTreeMap::new(),
            markets: BTreeMap::new(),
            catalogs: BTreeMap::new(),
            price_lists: BTreeMap::new(),
            web_presences: BTreeMap::new(),
            publication_ids: BTreeSet::new(),
            created_publication_ids: BTreeSet::new(),
            publications: BTreeMap::new(),
            resource_publications: BTreeMap::new(),
            shop_locales: BTreeMap::new(),
            localization_translations: Vec::new(),
            localization_resources: BTreeMap::new(),
            localization_dirty: false,
            marketing_activities: StagedRecords::default(),
            marketing_delete_all_external: false,
            webhook_subscriptions: BTreeMap::new(),
            b2b_companies: BTreeMap::new(),
            b2b_locations: StagedRecords::default(),
            b2b_contacts: BTreeMap::new(),
            b2b_contact_roles: BTreeMap::new(),
            b2b_role_assignments: BTreeMap::new(),
            b2b_staff_assignments: BTreeMap::new(),
            next_b2b_company_id: 1,
            inventory_levels: BTreeMap::new(),
            inventory_level_order: Vec::new(),
            inventory_level_ids: BTreeMap::new(),
            inventory_level_cursors: BTreeMap::new(),
            inactive_inventory_levels: BTreeSet::new(),
            inventory_quantity_updated_at: BTreeMap::new(),
            next_inventory_quantity_timestamp: 0,
            inventory_transfers: BTreeMap::new(),
            inventory_shipments: BTreeMap::new(),
            metaobject_definitions: StagedRecords::default(),
            metaobjects: StagedRecords::default(),
            url_redirects: BTreeMap::new(),
            url_redirect_order: Vec::new(),
            linked_product_option_metaobject_sets: Vec::new(),
            product_option_linked_metaobject_definition_ids: BTreeSet::new(),
            owner_metafields: BTreeMap::new(),
            deleted_owner_metafields: BTreeSet::new(),
            metafield_definitions: BTreeMap::new(),
            metafield_reference_ids: BTreeSet::new(),
            media_files: StagedRecords::default(),
            online_store_integrations: BTreeMap::new(),
            online_store_blogs: BTreeMap::new(),
            online_store_blog_order: Vec::new(),
            deleted_online_store_blog_ids: BTreeSet::new(),
            online_store_blogs_count_base: None,
            online_store_pages: BTreeMap::new(),
            online_store_page_order: Vec::new(),
            deleted_online_store_page_ids: BTreeSet::new(),
            online_store_pages_count_base: None,
            online_store_articles: BTreeMap::new(),
            online_store_article_order: Vec::new(),
            deleted_online_store_article_ids: BTreeSet::new(),
            online_store_comments: BTreeMap::new(),
            online_store_comment_order: Vec::new(),
            deleted_online_store_comment_ids: BTreeSet::new(),
            product_operations: BTreeMap::new(),
            product_delete_operations: BTreeMap::new(),
            mandate_payment_keys: BTreeSet::new(),
            payment_terms: BTreeMap::new(),
            payment_terms_owner_index: BTreeMap::new(),
            payment_reminder_schedule_ids: BTreeSet::new(),
            payment_customizations: BTreeMap::new(),
            customer_payment_methods: BTreeMap::new(),
            customer_payment_method_customer_index: BTreeMap::new(),
            next_customer_payment_method_id: 1,
            abandonments: BTreeMap::new(),
            orders: StagedRecords::default(),
            draft_orders: BTreeMap::new(),
            returns: BTreeMap::new(),
            returns_by_order: BTreeMap::new(),
            reverse_deliveries: BTreeMap::new(),
            reverse_fulfillment_orders: BTreeMap::new(),
            next_refund_id: 1,
            next_refund_line_item_id: 1,
            next_order_id: 1,
            next_draft_order_id: 1,
            draft_order_tags: BTreeMap::new(),
            next_draft_order_bulk_tag_job_id: 1,
            order_customer_orders: BTreeMap::new(),
            order_customer_cancelled_ids: BTreeSet::new(),
            order_customer_b2b_order_ids: BTreeSet::new(),
            order_customer_contact_customer_ids: BTreeSet::new(),
            next_order_customer_order_id: 1,
            order_edit_existing_order: None,
            order_edit_existing_calculated_order: None,
            order_edit_existing_calculated_order_id: None,
            order_edit_existing_session_order_id: None,
            order_edit_money_bag_calculated_order_ids: BTreeMap::new(),
            order_payment_next_transaction_id: 3,
            order_edit_existing_mode: None,
            order_edit_variant_catalog: Value::Object(serde_json::Map::new()),
            order_edit_author: None,
            function_validation: None,
            function_cart_transform: None,
            function_metadata: BTreeMap::new(),
            function_metadata_order: Vec::new(),
            function_validations: BTreeMap::new(),
            function_validation_order: Vec::new(),
            function_cart_transforms: BTreeMap::new(),
            function_cart_transform_order: Vec::new(),
            function_fulfillment_constraint_rules: BTreeMap::new(),
            function_fulfillment_constraint_rule_order: Vec::new(),
            functions_dirty: false,
            backup_region: Value::Null,
            flow_signatures: Vec::new(),
            flow_trigger_receipts: Vec::new(),

            b2b_contact_role_assignments: BTreeMap::new(),
            deleted_b2b_contact_ids: BTreeSet::new(),
            deleted_b2b_contact_role_assignment_ids: BTreeSet::new(),
            next_b2b_contact_id: 1,
            next_b2b_contact_role_assignment_id: 1,
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

fn effective_find<'a, T, F>(
    base: &'a OrderedRecords<T>,
    staged: &'a StagedRecords<T>,
    mut predicate: F,
) -> Option<&'a T>
where
    F: FnMut(&T) -> bool,
{
    staged
        .order
        .iter()
        .filter_map(|id| staged.get(id))
        .find(|record| predicate(*record))
        .or_else(|| {
            base.order
                .iter()
                .filter(|id| !staged.is_tombstoned(id) && !staged.contains_staged(id))
                .filter_map(|id| base.get(id))
                .find(|record| predicate(*record))
        })
}

fn effective_records<T: Clone>(base: &OrderedRecords<T>, staged: &StagedRecords<T>) -> Vec<T> {
    let mut records = Vec::new();
    for (id, record) in base
        .order
        .iter()
        .filter_map(|id| base.records.get(id).map(|record| (id.as_str(), record)))
    {
        if staged.is_tombstoned(id) {
            continue;
        }
        // An updated base record overrides in place, preserving its original
        // position — Shopify does not reorder a record on update. Only staged
        // records with no base counterpart are appended at the end below.
        if let Some(staged_record) = staged.get(id) {
            records.push(staged_record.clone());
        } else {
            records.push(record.clone());
        }
    }
    for (id, record) in staged
        .order
        .iter()
        .filter_map(|id| staged.records.get(id).map(|record| (id.as_str(), record)))
    {
        if staged.is_tombstoned(id) || base.records.contains_key(id) {
            continue;
        }
        records.push(record.clone());
    }
    records
}

fn product_variant_position(variant: &ProductVariantRecord) -> Option<i64> {
    variant.extra_fields.get("position").and_then(Value::as_i64)
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

fn collect_domain_records(domains: &mut BTreeMap<String, Value>, value: Option<&Value>) {
    let Some(value) = value else {
        return;
    };
    if let Some(domain) = normalized_domain_record(value) {
        if let Some(id) = domain.get("id").and_then(Value::as_str) {
            domains.insert(id.to_string(), domain);
        }
        return;
    }
    if let Some(nodes) = value.get("nodes").and_then(Value::as_array) {
        for node in nodes {
            collect_domain_records(domains, Some(node));
        }
    }
    if let Some(edges) = value.get("edges").and_then(Value::as_array) {
        for edge in edges {
            collect_domain_records(domains, edge.get("node"));
        }
    }
    if let Some(values) = value.as_array() {
        for value in values {
            collect_domain_records(domains, Some(value));
        }
    }
}

fn normalized_domain_record(value: &Value) -> Option<Value> {
    let id = value.get("id").and_then(Value::as_str)?;
    let host = value
        .get("host")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| {
            value
                .get("url")
                .and_then(Value::as_str)
                .and_then(domain_host_from_url)
        })?;
    let url = value
        .get("url")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| format!("https://{host}"));
    let ssl_enabled = value
        .get("sslEnabled")
        .and_then(Value::as_bool)
        .unwrap_or_else(|| url.starts_with("https://"));
    Some(json!({
        "id": id,
        "host": host,
        "url": url,
        "sslEnabled": ssl_enabled
    }))
}

fn domain_host_from_url(url: &str) -> Option<String> {
    let without_scheme = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .unwrap_or(url);
    without_scheme
        .split('/')
        .next()
        .map(str::trim)
        .filter(|host| !host.is_empty())
        .map(str::to_string)
}

impl Store {
    fn with_default_baseline() -> Self {
        let mut store = Self::default();
        store.base.available_locales = default_available_locales();
        store.base.shop_locales.insert(
            "en".to_string(),
            json!({
                "locale": "en",
                "name": "English",
                "primary": true,
                "published": true,
                "marketWebPresences": []
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

    fn has_known_publication_catalog(&self) -> bool {
        self.base.publication_count.is_some()
            || !self.base.publication_ids.is_empty()
            || !self.staged.publication_ids.is_empty()
            || !self.staged.publications.is_empty()
    }

    fn has_publication_id(&self, id: &str) -> bool {
        self.base.publication_ids.contains(id)
            || self.staged.publication_ids.contains(id)
            || self.staged.publications.contains_key(id)
    }

    fn publication_id_for_channel_id(&self, channel_id: &str) -> Option<String> {
        self.staged
            .publications
            .iter()
            .find_map(|(id, record)| {
                let matches = record
                    .get("channel")
                    .and_then(|channel| channel.get("id"))
                    .and_then(Value::as_str)
                    == Some(channel_id);
                matches.then(|| id.clone())
            })
            .or_else(|| {
                let suffix = resource_id_path_tail(channel_id);
                let publication_id = shopify_gid("Publication", suffix);
                self.has_publication_id(&publication_id)
                    .then_some(publication_id)
            })
    }

    pub(in crate::proxy) fn effective_shop(&self) -> Value {
        let mut shop = self.base.shop.clone();
        shop["publicationCount"] = json!(self.effective_publication_count());
        shop["shopPolicies"] = Value::Array(
            self.shop_policies()
                .into_iter()
                .map(|policy| shop_policy_record_json(&policy))
                .collect(),
        );
        shop
    }

    pub(in crate::proxy) fn shop_currency_code(&self) -> String {
        self.base
            .shop
            .get("currencyCode")
            .and_then(Value::as_str)
            .filter(|currency| !currency.is_empty())
            .unwrap_or("USD")
            .to_string()
    }

    fn shop_money_format(&self) -> Option<String> {
        self.base
            .shop
            .pointer("/currencyFormats/moneyFormat")
            .and_then(Value::as_str)
            .filter(|format| !format.is_empty())
            .map(str::to_string)
    }

    fn shop_policy_by_id(&self, id: &str) -> Option<&ShopPolicyRecord> {
        effective_get(&self.base.shop_policies, &self.staged.shop_policies, id)
    }

    fn shop_policy_by_type(&self, policy_type: &str) -> Option<&ShopPolicyRecord> {
        effective_find(
            &self.base.shop_policies,
            &self.staged.shop_policies,
            |policy| policy.policy_type == policy_type,
        )
    }

    fn shop_policies(&self) -> Vec<ShopPolicyRecord> {
        effective_records(&self.base.shop_policies, &self.staged.shop_policies)
    }

    fn stage_shop_policy(&mut self, policy: ShopPolicyRecord) {
        self.staged.shop_policies.stage(policy.id.clone(), policy);
    }

    fn domain_by_id(&self, id: &str) -> Option<Value> {
        if id.is_empty() {
            return None;
        }
        self.effective_domains()
            .into_iter()
            .find(|domain| domain.get("id").and_then(Value::as_str) == Some(id))
    }

    fn effective_domains(&self) -> Vec<Value> {
        let mut domains = BTreeMap::<String, Value>::new();
        collect_domain_records(&mut domains, self.base.shop.get("primaryDomain"));
        collect_domain_records(&mut domains, self.base.shop.get("domains"));
        for web_presence in self.staged.web_presences.values() {
            collect_domain_records(&mut domains, web_presence.get("domain"));
        }
        domains.into_values().collect()
    }

    fn product_by_id(&self, id: &str) -> Option<&ProductRecord> {
        effective_get(&self.base.products, &self.staged.products, id)
    }

    fn product_by_handle(&self, handle: &str) -> Option<&ProductRecord> {
        effective_find(&self.base.products, &self.staged.products, |product| {
            product.handle == handle
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

    fn has_collection_state(&self) -> bool {
        !self.staged.collections.is_empty() || !self.staged.collection_jobs.is_empty()
    }

    fn product_feed_by_id(&self, id: &str) -> Option<&Value> {
        self.staged.product_feeds.get(id)
    }

    fn product_feeds(&self) -> Vec<Value> {
        self.staged.product_feeds.values().cloned().collect()
    }

    fn has_product_feed_state(&self) -> bool {
        !self.staged.product_feeds.is_empty()
    }

    fn product_feed_is_tombstoned(&self, id: &str) -> bool {
        self.staged.product_feeds.is_tombstoned(id)
    }

    fn stage_product_feed(&mut self, feed: Value) {
        if let Some(id) = feed.get("id").and_then(Value::as_str) {
            self.staged.product_feeds.stage(id.to_string(), feed);
        }
    }

    fn delete_product_feed(&mut self, id: &str) -> bool {
        self.staged.product_feeds.tombstone_staged(id)
    }

    fn has_product(&self, id: &str) -> bool {
        self.product_by_id(id).is_some()
    }

    /// True only when the product id has been locally deleted (tombstoned).
    /// Distinct from a product that is merely absent from the snapshot seed:
    /// the proxy never seeds every real product, so absence is not proof the
    /// product does not exist upstream. Only an id the proxy itself deleted is
    /// known-missing.
    fn product_is_tombstoned(&self, id: &str) -> bool {
        self.staged.products.is_tombstoned(id)
    }

    fn has_localization_product(&self, id: &str) -> bool {
        !self.staged.products.is_tombstoned(id)
            && (self.has_product(id) || self.base.localization_product_ids.contains(id))
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

    fn collection_by_id(&self, id: &str) -> Option<&Value> {
        self.staged.collections.get(id)
    }

    /// True when the collection id has been locally deleted (tombstoned). Unlike a
    /// never-seen collection, a tombstoned one must be served from local state
    /// (collection: null) for read-after-delete rather than forwarded upstream.
    fn collection_is_deleted(&self, id: &str) -> bool {
        self.staged.collections.is_tombstoned(id)
    }

    fn stage_collection(&mut self, collection: Value) {
        if let Some(id) = collection.get("id").and_then(Value::as_str) {
            self.staged.collections.insert(id.to_string(), collection);
        }
    }

    fn delete_collection(&mut self, id: &str) -> bool {
        let existed = self.staged.collections.tombstone_staged(id);
        for product in self.products() {
            if product
                .collections
                .iter()
                .any(|collection| collection.get("id").and_then(Value::as_str) == Some(id))
            {
                let mut updated = product;
                updated
                    .collections
                    .retain(|collection| collection.get("id").and_then(Value::as_str) != Some(id));
                self.stage_product(updated);
            }
        }
        existed
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

    fn product_variants(&self) -> Vec<ProductVariantRecord> {
        effective_records(&self.base.product_variants, &self.staged.product_variants)
    }

    fn has_product_variant_reference(&self, variant_id: &str) -> bool {
        self.product_variant_by_id(variant_id).is_some()
            || self.fixed_price_variant_lookup(variant_id).is_some()
    }

    /// Resolve a variant id to its `(variant_json, product)` by scanning the
    /// embedded variant nodes of effective products. The fixed-price preflight
    /// stages products with their variants under `ProductRecord.variants` (raw
    /// JSON observed from upstream), not as separate `ProductVariantRecord`s, so
    /// this is the lookup path used by the price-list fixed-price handlers.
    fn fixed_price_variant_lookup(&self, variant_id: &str) -> Option<(Value, ProductRecord)> {
        if variant_id.is_empty() {
            return None;
        }
        for product in self.products() {
            if let Some(variant) = product
                .variants
                .iter()
                .find(|variant| variant.get("id").and_then(Value::as_str) == Some(variant_id))
            {
                return Some((variant.clone(), product.clone()));
            }
        }
        None
    }

    /// The embedded variant nodes for a product id, used to expand by-product
    /// fixed-price mutations into per-variant rows.
    fn fixed_price_variants_for_product(&self, product_id: &str) -> Vec<Value> {
        self.product_by_id(product_id)
            .map(|product| product.variants.clone())
            .unwrap_or_default()
    }

    fn product_variant_by_inventory_item_id(
        &self,
        inventory_item_id: &str,
    ) -> Option<&ProductVariantRecord> {
        effective_find(
            &self.base.product_variants,
            &self.staged.product_variants,
            |variant| variant.inventory_item.id == inventory_item_id,
        )
    }

    fn product_variants_for_product(&self, product_id: &str) -> Vec<ProductVariantRecord> {
        let mut variants =
            effective_records(&self.base.product_variants, &self.staged.product_variants)
                .into_iter()
                .filter(|variant| variant.product_id == product_id)
                .collect::<Vec<_>>();
        if variants.len() > 1
            && variants
                .iter()
                .all(|variant| product_variant_position(variant).is_some())
        {
            let mut indexed = variants.into_iter().enumerate().collect::<Vec<_>>();
            indexed.sort_by(|left, right| {
                product_variant_position(&left.1)
                    .cmp(&product_variant_position(&right.1))
                    .then(left.0.cmp(&right.0))
            });
            variants = indexed.into_iter().map(|(_, variant)| variant).collect();
        }
        variants
    }

    fn product_media_by_id(&self, product_id: &str, media_id: &str) -> Option<Value> {
        self.product_by_id(product_id).and_then(|product| {
            product
                .media
                .iter()
                .find(|media| media.get("id").and_then(Value::as_str) == Some(media_id))
                .cloned()
        })
    }

    fn stage_product_variant(&mut self, variant: ProductVariantRecord) {
        self.staged
            .product_variants
            .stage(variant.id.clone(), variant);
    }

    fn compact_product_variant_positions(&mut self, product_id: &str) {
        let variants = self.product_variants_for_product(product_id);
        let mut positioned_variants = variants
            .into_iter()
            .enumerate()
            .map(|(index, variant)| {
                let position = variant
                    .extra_fields
                    .get("position")
                    .and_then(|value| value.as_i64().or_else(|| value.as_u64().map(|v| v as i64)))
                    .unwrap_or((index + 1) as i64);
                (position, index, variant)
            })
            .collect::<Vec<_>>();
        positioned_variants.sort_by(|left, right| left.0.cmp(&right.0).then(left.1.cmp(&right.1)));

        let ordered_ids = positioned_variants
            .iter()
            .map(|(_, _, variant)| variant.id.clone())
            .collect::<Vec<_>>();
        let mut positions_by_id = BTreeMap::new();
        for (index, (_, _, mut variant)) in positioned_variants.into_iter().enumerate() {
            let position = index + 1;
            variant
                .extra_fields
                .insert("position".to_string(), json!(position));
            positions_by_id.insert(variant.id.clone(), position);
            self.stage_product_variant(variant);
        }

        let ordered_id_set = ordered_ids.iter().cloned().collect::<BTreeSet<_>>();
        let mut reordered = Vec::new();
        let mut inserted_product_block = false;
        for id in self.staged.product_variants.order.iter() {
            if ordered_id_set.contains(id) {
                if !inserted_product_block {
                    reordered.extend(ordered_ids.iter().cloned());
                    inserted_product_block = true;
                }
            } else {
                reordered.push(id.clone());
            }
        }
        if !inserted_product_block {
            reordered.extend(ordered_ids.iter().cloned());
        }
        self.staged.product_variants.order =
            normalized_order(self.staged.product_variants.records.keys(), reordered);

        if let Some(mut product) = self.product_by_id(product_id).cloned() {
            let mut changed = false;
            for variant in &mut product.variants {
                let Some(id) = variant.get("id").and_then(Value::as_str) else {
                    continue;
                };
                let Some(position) = positions_by_id.get(id) else {
                    continue;
                };
                if variant.get("position").and_then(Value::as_u64) != Some(*position as u64) {
                    variant["position"] = json!(position);
                    changed = true;
                }
            }
            if changed {
                self.stage_product(product);
            }
        }
    }

    /// Detach the given media ids from product/variant owner state. Removes the
    /// ids from each product's `media` nodes and from each variant's `media_ids`.
    /// When `only_products` is `Some`, the removal is scoped to those product ids
    /// (fileUpdate `referencesToRemove`); `None` applies to all owners
    /// (fileDelete cascade). Only owners that actually change are re-staged.
    fn clear_media_ids(&mut self, media_ids: &[String], only_products: Option<&[String]>) {
        if media_ids.is_empty() {
            return;
        }
        let removes = |id: &str| media_ids.iter().any(|m| m == id);
        let in_scope = |product_id: &str| {
            only_products.is_none_or(|filter| filter.iter().any(|p| p == product_id))
        };
        for mut product in self.products() {
            if !in_scope(&product.id) {
                continue;
            }
            let before = product.media.len();
            product.media.retain(|node| {
                node.get("id")
                    .and_then(Value::as_str)
                    .map(|id| !removes(id))
                    .unwrap_or(true)
            });
            if product.media.len() != before {
                self.stage_product(product);
            }
        }
        for mut variant in
            effective_records(&self.base.product_variants, &self.staged.product_variants)
        {
            if !in_scope(&variant.product_id) {
                continue;
            }
            let before = variant.media_ids.len();
            variant.media_ids.retain(|id| !removes(id));
            if variant.media_ids.len() != before {
                self.stage_product_variant(variant);
            }
        }
    }

    fn delete_product_variant(&mut self, id: &str) -> bool {
        let product_id = self
            .product_variant_by_id(id)
            .map(|variant| variant.product_id.clone());
        let existed = product_id.is_some();
        self.staged.product_variants.remove_staged(id);
        if existed {
            self.staged.product_variants.tombstone(id.to_string());
        }
        // Drop the variant from the owning product's embedded observed variants
        // list as well, otherwise the connection fallback would resurrect it:
        // the fallback surfaces observed variants that lack a staged record, and
        // a just-deleted variant has neither a staged record nor a tombstone the
        // fallback is aware of.
        if let Some(product_id) = product_id {
            if let Some(mut product) = self.product_by_id(&product_id).cloned() {
                let before = product.variants.len();
                product
                    .variants
                    .retain(|variant| variant.get("id").and_then(Value::as_str) != Some(id));
                if product.variants.len() != before {
                    self.stage_product(product);
                }
            }
            self.compact_product_variant_positions(&product_id);
        }
        existed
    }

    fn reorder_product_variants(
        &mut self,
        product_id: &str,
        ordered_ids: &[String],
    ) -> Vec<ProductVariantRecord> {
        let variants = self.product_variants_for_product(product_id);
        let mut by_id = variants
            .iter()
            .cloned()
            .map(|variant| (variant.id.clone(), variant))
            .collect::<BTreeMap<_, _>>();
        let product_variant_ids = by_id.keys().cloned().collect::<BTreeSet<_>>();
        let mut staged_order = Vec::new();

        for id in ordered_ids {
            if product_variant_ids.contains(id) && !staged_order.contains(id) {
                staged_order.push(id.clone());
            }
        }
        for variant in variants {
            if !staged_order.contains(&variant.id) {
                staged_order.push(variant.id.clone());
            }
        }

        let mut reordered_variants = Vec::new();
        for id in staged_order.iter().cloned() {
            if let Some(mut variant) = by_id.remove(&id) {
                variant
                    .extra_fields
                    .insert("position".to_string(), json!(reordered_variants.len() + 1));
                self.staged.product_variants.stage(id, variant.clone());
                reordered_variants.push(variant);
            }
        }
        self.staged.product_variants.order =
            normalized_order(self.staged.product_variants.records.keys(), staged_order);
        reordered_variants
    }

    fn move_product_variants_to_positions(
        &mut self,
        product_id: &str,
        moves: &[(String, i64, usize)],
    ) -> Vec<ProductVariantRecord> {
        let mut ordered_ids = self
            .product_variants_for_product(product_id)
            .into_iter()
            .map(|variant| variant.id)
            .collect::<Vec<_>>();
        let mut sorted_moves = moves.to_vec();
        sorted_moves.sort_by(|left, right| left.1.cmp(&right.1).then(left.2.cmp(&right.2)));
        for (variant_id, position, _) in sorted_moves {
            ordered_ids.retain(|id| id != &variant_id);
            let insert_at = if position <= 1 {
                0
            } else {
                (position - 1) as usize
            };
            ordered_ids.insert(insert_at.min(ordered_ids.len()), variant_id);
        }
        self.reorder_product_variants(product_id, &ordered_ids)
    }

    fn selling_plan_group_by_id(&self, id: &str) -> Option<&SellingPlanGroupRecord> {
        self.staged.selling_plan_groups.get(id)
    }

    fn selling_plan_groups(&self) -> Vec<SellingPlanGroupRecord> {
        self.staged
            .selling_plan_groups
            .order
            .iter()
            .filter(|id| !self.staged.selling_plan_groups.is_tombstoned(id))
            .filter_map(|id| self.staged.selling_plan_groups.get(id).cloned())
            .collect()
    }

    fn stage_selling_plan_group(&mut self, group: SellingPlanGroupRecord) {
        self.staged
            .selling_plan_groups
            .stage(group.id.clone(), group);
    }

    fn delete_selling_plan_group(&mut self, id: &str) -> bool {
        let had_staged = self.staged.selling_plan_groups.remove_staged(id).is_some();
        if had_staged {
            self.staged.selling_plan_groups.tombstone(id.to_string());
        }
        had_staged
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

    fn failed(
        root_field: impl Into<String>,
        domain: &'static str,
        notes: impl Into<String>,
    ) -> Self {
        Self {
            root_field: root_field.into(),
            staged_resource_ids: Vec::new(),
            status: "failed".to_string(),
            capability_domain: domain.to_string(),
            capability_execution: "stage-locally".to_string(),
            notes: notes.into(),
        }
    }
}

fn default_commit_transport(_request: Request) -> Response {
    json_error(501, "No Rust commit transport configured")
}

fn default_upstream_transport(_request: Request) -> Response {
    json_error(502, "No Rust upstream transport configured")
}

type RuntimeClock = Arc<dyn Fn() -> time::OffsetDateTime + Send + Sync>;

fn default_runtime_clock() -> time::OffsetDateTime {
    time::OffsetDateTime::now_utc()
}

#[derive(Clone)]
pub struct DraftProxy {
    config: Config,
    log_entries: Vec<Value>,
    registry: Vec<OperationRegistryEntry>,
    store: Store,
    next_synthetic_id: u64,
    /// Per-scenario cache of the upstream shop's `shop.features.sellsSubscriptions`
    /// capability. Populated lazily by forwarding a `DraftProxyShopSubscriptionCapability`
    /// probe the first time a discount mutation touches subscription/recurring fields.
    /// Intentionally NOT part of the dump/restore snapshot so it survives
    /// `restoreState` between a scenario's targets; it is reset on `/__meta/reset`,
    /// which the parity runner issues at the start of every scenario.
    shop_sells_subscriptions: Option<bool>,
    clock: RuntimeClock,
    last_mutation_timestamp: Option<time::OffsetDateTime>,
    commit_transport: CommitTransport,
    upstream_transport: UpstreamTransport,
}

mod admin_shipping_gift_cards;
mod app_shipping_helpers;
mod b2b_customers;
mod civil_date;
mod commit;
mod connection;
mod core;
mod discounts;
mod dispatch;
mod functions;
mod json_helpers;
mod localization_markets_catalogs;
mod market_unsupported_country_regions;
mod marketing_webhooks_inventory;
mod markets_catalog_helpers;
mod media_products_saved_searches;
mod metafield_metaobject_definitions;
mod metafields_orders_payments;
mod metaobjects;
mod money;
mod online_store_content;
mod online_store_orders_payments;
mod privacy;
mod product_helpers;
mod product_operations;
mod product_options;
mod resolved_values;
mod resource_ids;
mod routing;
mod scalar_helpers;
mod schema_validation;
mod selection;
mod selling_plans;
mod store_properties;

pub(in crate::proxy) use self::admin_shipping_gift_cards::*;
pub(in crate::proxy) use self::app_shipping_helpers::*;
pub(in crate::proxy) use self::b2b_customers::*;
pub(in crate::proxy) use self::civil_date::*;
pub(in crate::proxy) use self::connection::*;
pub(in crate::proxy) use self::discounts::*;
pub(in crate::proxy) use self::functions::*;
pub(in crate::proxy) use self::json_helpers::*;
pub(in crate::proxy) use self::localization_markets_catalogs::*;
pub(in crate::proxy) use self::marketing_webhooks_inventory::*;
pub(in crate::proxy) use self::markets_catalog_helpers::*;
pub(in crate::proxy) use self::media_products_saved_searches::*;
pub(in crate::proxy) use self::metafield_metaobject_definitions::*;
pub(in crate::proxy) use self::metafields_orders_payments::*;
pub(in crate::proxy) use self::money::*;
pub(in crate::proxy) use self::online_store_orders_payments::*;
pub(in crate::proxy) use self::product_helpers::*;
pub(in crate::proxy) use self::product_operations::*;
pub(in crate::proxy) use self::product_options::*;
pub(in crate::proxy) use self::resolved_values::*;
pub(in crate::proxy) use self::resource_ids::*;
pub(in crate::proxy) use self::routing::*;
#[allow(unused_imports)]
pub(in crate::proxy) use self::scalar_helpers::*;
pub(in crate::proxy) use self::schema_validation::*;
pub(in crate::proxy) use self::selection::*;
pub(in crate::proxy) use self::store_properties::*;

#[cfg(test)]
mod store_tests {
    use super::*;

    fn product(id: &str, title: &str, handle: &str) -> ProductRecord {
        ProductRecord {
            id: id.to_string(),
            created_at: default_product_timestamp(),
            updated_at: default_product_timestamp(),
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
        store.base.saved_searches.replace_with_order(
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
        proxy.store.staged.locations.insert(
            "gid://shopify/Location/live".to_string(),
            json!({"id": "gid://shopify/Location/live", "name": "Live location"}),
        );
        proxy.store.staged.locations.insert(
            "gid://shopify/Location/deleted".to_string(),
            json!({"id": "gid://shopify/Location/deleted", "name": "Deleted location"}),
        );
        proxy
            .store
            .staged
            .locations
            .tombstone_staged("gid://shopify/Location/deleted");
        proxy.store.staged.delivery_profiles.insert(
            "gid://shopify/DeliveryProfile/live".to_string(),
            json!({"id": "gid://shopify/DeliveryProfile/live", "name": "Live profile"}),
        );
        proxy.store.staged.delivery_profiles.insert(
            "gid://shopify/DeliveryProfile/deleted".to_string(),
            json!({"id": "gid://shopify/DeliveryProfile/deleted", "name": "Deleted profile"}),
        );
        proxy
            .store
            .staged
            .delivery_profiles
            .tombstone_staged("gid://shopify/DeliveryProfile/deleted");
        proxy.store.staged.store_credit_accounts.insert(
            "gid://shopify/StoreCreditAccount/1".to_string(),
            json!({"id": "gid://shopify/StoreCreditAccount/1"}),
        );
        proxy.store.staged.b2b_locations.insert(
            "gid://shopify/CompanyLocation/1".to_string(),
            json!({"id": "gid://shopify/CompanyLocation/1"}),
        );
        proxy.store.staged.customers.insert(
            "gid://shopify/Customer/deleted".to_string(),
            json!({"id": "gid://shopify/Customer/deleted"}),
        );
        proxy
            .store
            .staged
            .customers
            .tombstone_staged("gid://shopify/Customer/deleted");
        proxy.store.staged.collections.insert(
            "gid://shopify/Collection/deleted".to_string(),
            json!({"id": "gid://shopify/Collection/deleted"}),
        );
        proxy
            .store
            .staged
            .collections
            .tombstone_staged("gid://shopify/Collection/deleted");

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
        assert_eq!(
            dump.body["state"]["stagedState"]["locationOrder"],
            json!(["gid://shopify/Location/live"])
        );
        assert_eq!(
            dump.body["state"]["stagedState"]["deletedLocationIds"],
            json!(["gid://shopify/Location/deleted"])
        );
        assert_eq!(
            dump.body["state"]["stagedState"]["deliveryProfileOrder"],
            json!(["gid://shopify/DeliveryProfile/live"])
        );
        assert_eq!(
            dump.body["state"]["stagedState"]["deletedDeliveryProfileIds"],
            json!(["gid://shopify/DeliveryProfile/deleted"])
        );
        assert_eq!(
            dump.body["state"]["stagedState"]["storeCreditAccountOrder"],
            json!(["gid://shopify/StoreCreditAccount/1"])
        );
        assert_eq!(
            dump.body["state"]["stagedState"]["b2bLocationOrder"],
            json!(["gid://shopify/CompanyLocation/1"])
        );
        assert_eq!(
            dump.body["state"]["stagedState"]["deletedCustomerIds"],
            json!(["gid://shopify/Customer/deleted"])
        );
        assert_eq!(
            dump.body["state"]["stagedState"]["deletedCollectionIds"],
            json!(["gid://shopify/Collection/deleted"])
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
        assert_eq!(
            restored
                .store
                .staged
                .locations
                .get("gid://shopify/Location/live"),
            Some(&json!({"id": "gid://shopify/Location/live", "name": "Live location"}))
        );
        assert!(restored
            .store
            .staged
            .locations
            .is_tombstoned("gid://shopify/Location/deleted"));
        assert!(restored
            .store
            .staged
            .delivery_profiles
            .is_tombstoned("gid://shopify/DeliveryProfile/deleted"));
        assert!(restored
            .store
            .staged
            .customers
            .is_tombstoned("gid://shopify/Customer/deleted"));
        assert!(restored
            .store
            .staged
            .collections
            .is_tombstoned("gid://shopify/Collection/deleted"));
        assert_eq!(
            restored.store.staged.store_credit_accounts.order,
            vec!["gid://shopify/StoreCreditAccount/1"]
        );
        assert_eq!(
            restored.store.staged.b2b_locations.order,
            vec!["gid://shopify/CompanyLocation/1"]
        );
    }

    #[test]
    fn state_version_header_advances_on_mutation_and_holds_on_reads() {
        let mut proxy = snapshot_proxy();

        let version_of = |response: &Response| {
            response
                .headers
                .get("x-sdp-state-version")
                .cloned()
                .expect("every response should carry x-sdp-state-version")
        };

        let baseline = proxy.process_request(Request {
            method: "GET".to_string(),
            path: "/__meta/health".to_string(),
            headers: BTreeMap::new(),
            body: String::new(),
        });
        let baseline_version = version_of(&baseline);

        let create = proxy.process_request(graphql_request(
            r#"
            mutation ProductCreate($product: ProductInput!) {
              productCreate(product: $product) {
                product { id }
                userErrors { field message }
              }
            }
            "#,
            json!({ "product": { "title": "Versioned", "handle": "versioned" } }),
        ));
        let after_create = version_of(&create);
        assert_ne!(
            after_create, baseline_version,
            "a staged mutation must advance the state version"
        );

        // A pure read must not advance the version, so embedders skip persisting.
        let read = proxy.process_request(Request {
            method: "GET".to_string(),
            path: "/__meta/state".to_string(),
            headers: BTreeMap::new(),
            body: String::new(),
        });
        assert_eq!(
            version_of(&read),
            after_create,
            "reads must leave the state version unchanged"
        );

        // Reset returns the version to its pristine baseline.
        let reset = proxy.process_request(Request {
            method: "POST".to_string(),
            path: "/__meta/reset".to_string(),
            headers: BTreeMap::new(),
            body: String::new(),
        });
        assert_eq!(
            version_of(&reset),
            baseline_version,
            "reset must return the state version to baseline"
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
                "startCursor": "gid://shopify/ProductVariant/2?shopify-draft-proxy=synthetic",
                "endCursor": "gid://shopify/ProductVariant/2?shopify-draft-proxy=synthetic"
            })
        );
        assert_eq!(
            read.body["data"]["product"]["variants"]["nodes"],
            json!([{ "id": "gid://shopify/ProductVariant/2?shopify-draft-proxy=synthetic" }])
        );
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
    fn top_level_collections_reflect_staged_collection_lifecycle() {
        let mut proxy = snapshot_proxy();

        let first = proxy.process_request(graphql_request(
            r#"
            mutation CollectionLifecycleCreateFirst($input: CollectionInput!) {
              collectionCreate(input: $input) {
                collection {
                  id
                  title
                  handle
                }
                userErrors {
                  field
                  message
                }
              }
            }
            "#,
            json!({ "input": { "title": "Alpha Collection", "handle": "alpha-collection" } }),
        ));
        assert_eq!(first.status, 200);
        let first_id = first.body["data"]["collectionCreate"]["collection"]["id"]
            .as_str()
            .expect("first collection should have an id")
            .to_string();

        let second = proxy.process_request(graphql_request(
            r#"
            mutation CollectionLifecycleCreateSecond($input: CollectionInput!) {
              collectionCreate(input: $input) {
                collection {
                  id
                }
                userErrors {
                  field
                  message
                }
              }
            }
            "#,
            json!({ "input": { "title": "Beta Collection", "handle": "beta-collection" } }),
        ));
        assert_eq!(second.status, 200);
        let second_id = second.body["data"]["collectionCreate"]["collection"]["id"]
            .as_str()
            .expect("second collection should have an id")
            .to_string();

        let initial_read = proxy.process_request(graphql_request(
            r#"
            query CollectionLifecycleInitialRead($titleQuery: String!, $handleQuery: String!) {
              titleMatches: collections(first: 10, query: $titleQuery, sortKey: TITLE) {
                nodes {
                  id
                  title
                  handle
                  updatedAt
                }
                pageInfo {
                  hasNextPage
                  hasPreviousPage
                  startCursor
                  endCursor
                }
              }
              handleMatches: collections(first: 10, query: $handleQuery) {
                nodes {
                  id
                  title
                  handle
                }
              }
              titleCount: collectionsCount(query: $titleQuery) {
                count
                precision
              }
            }
            "#,
            json!({
                "titleQuery": "title:Alpha*",
                "handleQuery": "handle:alpha-collection"
            }),
        ));
        assert_eq!(initial_read.status, 200);
        assert_eq!(
            initial_read.body["data"]["titleMatches"]["nodes"],
            json!([{
                "id": first_id,
                "title": "Alpha Collection",
                "handle": "alpha-collection",
                "updatedAt": "2024-01-01T00:00:01.000Z"
            }])
        );
        assert_eq!(
            initial_read.body["data"]["titleMatches"]["pageInfo"],
            json!({
                "hasNextPage": false,
                "hasPreviousPage": false,
                "startCursor": first_id,
                "endCursor": first_id
            })
        );
        assert_eq!(
            initial_read.body["data"]["handleMatches"]["nodes"],
            json!([{
                "id": first_id,
                "title": "Alpha Collection",
                "handle": "alpha-collection"
            }])
        );
        assert_eq!(
            initial_read.body["data"]["titleCount"],
            json!({ "count": 1, "precision": "EXACT" })
        );

        let update = proxy.process_request(graphql_request(
            r#"
            mutation CollectionLifecycleUpdate($input: CollectionInput!) {
              collectionUpdate(input: $input) {
                collection {
                  id
                  title
                  handle
                  updatedAt
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
                    "id": first_id,
                    "title": "Gamma Collection",
                    "handle": "alpha-collection-updated"
                }
            }),
        ));
        assert_eq!(update.status, 200);
        assert_eq!(
            update.body["data"]["collectionUpdate"]["collection"]["updatedAt"],
            json!("2024-01-01T00:00:03.000Z")
        );

        let update_read = proxy.process_request(graphql_request(
            r#"
            query CollectionLifecycleUpdatedRead($oldTitleQuery: String!, $oldHandleQuery: String!, $newHandleQuery: String!) {
              oldTitleMatches: collections(first: 10, query: $oldTitleQuery) {
                nodes {
                  id
                }
              }
              oldHandleMatches: collections(first: 10, query: $oldHandleQuery) {
                nodes {
                  id
                }
              }
              newHandleMatches: collections(first: 10, query: $newHandleQuery) {
                nodes {
                  id
                  title
                  handle
                  updatedAt
                }
              }
            }
            "#,
            json!({
                "oldTitleQuery": "title:Alpha*",
                "oldHandleQuery": "handle:alpha-collection",
                "newHandleQuery": "handle:alpha-collection-updated"
            }),
        ));
        assert_eq!(update_read.status, 200);
        assert_eq!(
            update_read.body["data"]["oldTitleMatches"]["nodes"],
            json!([])
        );
        assert_eq!(
            update_read.body["data"]["oldHandleMatches"]["nodes"],
            json!([])
        );
        assert_eq!(
            update_read.body["data"]["newHandleMatches"]["nodes"],
            json!([{
                "id": first_id,
                "title": "Gamma Collection",
                "handle": "alpha-collection-updated",
                "updatedAt": "2024-01-01T00:00:03.000Z"
            }])
        );

        let delete = proxy.process_request(graphql_request(
            r#"
            mutation CollectionLifecycleDelete($input: CollectionDeleteInput!) {
              collectionDelete(input: $input) {
                deletedCollectionId
                userErrors {
                  field
                  message
                }
              }
            }
            "#,
            json!({ "input": { "id": second_id } }),
        ));
        assert_eq!(delete.status, 200);
        assert_eq!(
            delete.body["data"]["collectionDelete"]["deletedCollectionId"],
            json!(second_id)
        );

        let delete_read = proxy.process_request(graphql_request(
            r#"
            query CollectionLifecycleDeleteRead {
              collections(first: 10) {
                nodes {
                  id
                  title
                }
              }
              collectionsCount {
                count
                precision
              }
            }
            "#,
            json!({}),
        ));
        assert_eq!(delete_read.status, 200);
        assert_eq!(
            delete_read.body["data"]["collections"]["nodes"],
            json!([{ "id": first_id, "title": "Gamma Collection" }])
        );
        assert_eq!(
            delete_read.body["data"]["collectionsCount"],
            json!({ "count": 1, "precision": "EXACT" })
        );
    }

    #[test]
    fn top_level_collections_honor_sort_reverse_cursors_and_limited_counts() {
        let mut proxy = snapshot_proxy();
        let mut ids = Vec::new();
        for (title, handle) in [
            ("Bravo Collection", "bravo-collection"),
            ("Alpha Collection", "alpha-collection"),
            ("Charlie Collection", "charlie-collection"),
        ] {
            let create = proxy.process_request(graphql_request(
                r#"
                mutation CollectionConnectionCreate($input: CollectionInput!) {
                  collectionCreate(input: $input) {
                    collection {
                      id
                    }
                    userErrors {
                      field
                      message
                    }
                  }
                }
                "#,
                json!({ "input": { "title": title, "handle": handle } }),
            ));
            assert_eq!(create.status, 200);
            ids.push(
                create.body["data"]["collectionCreate"]["collection"]["id"]
                    .as_str()
                    .expect("collection should have id")
                    .to_string(),
            );
        }

        let first_page = proxy.process_request(graphql_request(
            r#"
            query CollectionConnectionFirstPage {
              collections(first: 2) {
                edges {
                  cursor
                  node {
                    id
                    title
                  }
                }
                pageInfo {
                  hasNextPage
                  hasPreviousPage
                  startCursor
                  endCursor
                }
              }
              collectionsCount(limit: 2) {
                count
                precision
              }
            }
            "#,
            json!({}),
        ));
        assert_eq!(first_page.status, 200);
        assert_eq!(
            first_page.body["data"]["collections"]["edges"],
            json!([
                { "cursor": ids[0], "node": { "id": ids[0], "title": "Bravo Collection" } },
                { "cursor": ids[1], "node": { "id": ids[1], "title": "Alpha Collection" } }
            ])
        );
        assert_eq!(
            first_page.body["data"]["collections"]["pageInfo"],
            json!({
                "hasNextPage": true,
                "hasPreviousPage": false,
                "startCursor": ids[0],
                "endCursor": ids[1]
            })
        );
        assert_eq!(
            first_page.body["data"]["collectionsCount"],
            json!({ "count": 2, "precision": "AT_LEAST" })
        );

        let after_page = proxy.process_request(graphql_request(
            r#"
            query CollectionConnectionAfter($after: String!) {
              collections(first: 2, after: $after) {
                nodes {
                  id
                  title
                }
                pageInfo {
                  hasNextPage
                  hasPreviousPage
                  startCursor
                  endCursor
                }
              }
            }
            "#,
            json!({ "after": ids[1] }),
        ));
        assert_eq!(after_page.status, 200);
        assert_eq!(
            after_page.body["data"]["collections"]["nodes"],
            json!([{ "id": ids[2], "title": "Charlie Collection" }])
        );
        assert_eq!(
            after_page.body["data"]["collections"]["pageInfo"],
            json!({
                "hasNextPage": false,
                "hasPreviousPage": true,
                "startCursor": ids[2],
                "endCursor": ids[2]
            })
        );

        let title_reverse = proxy.process_request(graphql_request(
            r#"
            query CollectionConnectionTitleReverse {
              collections(first: 3, sortKey: TITLE, reverse: true) {
                nodes {
                  title
                }
              }
            }
            "#,
            json!({}),
        ));
        assert_eq!(title_reverse.status, 200);
        assert_eq!(
            title_reverse.body["data"]["collections"]["nodes"],
            json!([
                { "title": "Charlie Collection" },
                { "title": "Bravo Collection" },
                { "title": "Alpha Collection" }
            ])
        );

        let update = proxy.process_request(graphql_request(
            r#"
            mutation CollectionConnectionUpdate($input: CollectionInput!) {
              collectionUpdate(input: $input) {
                collection {
                  id
                  updatedAt
                }
                userErrors {
                  field
                  message
                }
              }
            }
            "#,
            json!({ "input": { "id": ids[1], "title": "Alpha Updated Collection" } }),
        ));
        assert_eq!(update.status, 200);

        let updated_filter = proxy.process_request(graphql_request(
            r#"
            query CollectionConnectionUpdatedFilter($query: String!) {
              collections(first: 10, query: $query, sortKey: UPDATED_AT, reverse: true) {
                nodes {
                  id
                  title
                  updatedAt
                }
              }
            }
            "#,
            json!({ "query": "updated_at:>=2024-01-01T00:00:03.000Z" }),
        ));
        assert_eq!(updated_filter.status, 200);
        assert_eq!(
            updated_filter.body["data"]["collections"]["nodes"],
            json!([
                {
                    "id": ids[1],
                    "title": "Alpha Updated Collection",
                    "updatedAt": "2024-01-01T00:00:04.000Z"
                },
                {
                    "id": ids[2],
                    "title": "Charlie Collection",
                    "updatedAt": "2024-01-01T00:00:03.000Z"
                }
            ])
        );
    }

    #[test]
    fn top_level_collections_live_hybrid_overlays_observed_upstream_state() {
        let upstream_body = json!({
            "data": {
                "collections": {
                    "nodes": [
                        {
                            "id": "gid://shopify/Collection/901",
                            "title": "Local Staged Collection",
                            "handle": "local-staged-collection",
                            "updatedAt": "2024-01-01T00:00:00.000Z",
                            "products": { "nodes": [] }
                        },
                        {
                            "id": "gid://shopify/Collection/900",
                            "title": "Upstream Base Collection",
                            "handle": "upstream-base-collection",
                            "updatedAt": "2024-01-01T00:00:00.000Z",
                            "products": { "nodes": [] }
                        }
                    ],
                    "pageInfo": {
                        "hasNextPage": false,
                        "hasPreviousPage": false,
                        "startCursor": "gid://shopify/Collection/900",
                        "endCursor": "gid://shopify/Collection/900"
                    }
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

        let create = proxy.process_request(graphql_request(
            r#"
            mutation CollectionLiveHybridCreate($input: CollectionInput!) {
              collectionCreate(input: $input) {
                collection {
                  id
                }
                userErrors {
                  field
                  message
                }
              }
            }
            "#,
            json!({ "input": { "title": "Local Staged Collection", "handle": "local-staged-collection" } }),
        ));
        assert_eq!(create.status, 200);
        let staged_id = create.body["data"]["collectionCreate"]["collection"]["id"]
            .as_str()
            .expect("staged collection should have id")
            .to_string();

        let read = proxy.process_request(graphql_request(
            r#"
            query CollectionLiveHybridRead {
              collections(first: 10, sortKey: TITLE) {
                nodes {
                  id
                  title
                  handle
                }
              }
            }
            "#,
            json!({}),
        ));
        assert_eq!(read.status, 200);
        assert_eq!(
            read.body["data"]["collections"]["nodes"],
            json!([
                {
                    "id": staged_id,
                    "title": "Local Staged Collection",
                    "handle": "local-staged-collection"
                },
                {
                    "id": "gid://shopify/Collection/900",
                    "title": "Upstream Base Collection",
                    "handle": "upstream-base-collection"
                }
            ])
        );
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
        let updated_variant = read.body["data"]["product"]["variants"]["nodes"]
            .as_array()
            .and_then(|variants| {
                variants
                    .iter()
                    .find(|variant| variant.get("id") == Some(&json!(variant_id)))
            })
            .expect("updated variant should be present in product variants");
        assert_eq!(updated_variant["title"], json!("Store Red"));
        assert_eq!(updated_variant["sku"], json!("STORE-RED"));
        assert_eq!(
            updated_variant["inventoryItem"]["requiresShipping"],
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
        let mut proxy = snapshot_proxy().with_base_products(vec![
            ProductRecord {
                id: "gid://shopify/Product/first".to_string(),
                title: "First Product".to_string(),
                handle: "first-product".to_string(),
                status: "ACTIVE".to_string(),
                ..ProductRecord::default()
            },
            ProductRecord {
                id: "gid://shopify/Product/second".to_string(),
                title: "Second Product".to_string(),
                handle: "second-product".to_string(),
                status: "ACTIVE".to_string(),
                ..ProductRecord::default()
            },
        ]);

        let create = proxy.process_request(graphql_request(
            r#"
            mutation CollectionCreateForDownstreamRead($input: CollectionInput!) {
              collectionCreate(input: $input) {
                collection {
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
                    "title": "Store Backed Collection",
                    "handle": "store-backed-collection",
                    "sortOrder": "MANUAL"
                }
            }),
        ));
        assert_eq!(create.status, 200);
        assert_eq!(
            create.body["data"]["collectionCreate"]["userErrors"],
            json!([])
        );
        let collection_id = create.body["data"]["collectionCreate"]["collection"]["id"]
            .as_str()
            .expect("collection create should return id")
            .to_string();

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
                "id": collection_id,
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
                "collectionId": collection_id,
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
                    "id": collection_id,
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

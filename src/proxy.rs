use std::{
    cell::RefCell,
    collections::{btree_map, BTreeMap, BTreeSet},
    sync::Arc,
};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::graphql::{
    parse_operation, parse_operation_with_variables,
    parse_operation_with_variables_and_operation_name, parsed_document, root_field_arguments,
    root_fields, selected_operation, selected_operation_query, variable_definition_info,
    variables_with_operation_defaults, OperationSelectionError, OperationType, RawArgumentValue,
    ResolvedValue, RootFieldSelection, SelectedField, SourceLocation,
};
use crate::node_resolver_inventory::{EntityRef, NodeLoadState};
use crate::operation_registry::{
    default_registry, operation_capability_for_surface, ApiSurface, CapabilityDomain,
    CapabilityExecution, OperationRegistryEntry,
};
use crate::resolver_registry::ResolverRegistry;
pub(in crate::proxy) use crate::resolver_registry::{
    FieldResolverRegistration, FieldResolverTypePolicy, LocalResolverMode,
    MutationLogDraft as LogDraft, OperationRootInvocation, ResolverOutcome, RootInvocation,
};

pub const DEFAULT_BULK_OPERATION_RUN_MUTATION_MAX_INPUT_FILE_SIZE_BYTES: u64 = 104_857_600;
pub(in crate::proxy) const METAFIELDS_SET_INPUT_LIMIT: usize = 25;
pub(in crate::proxy) const API_CLIENT_ID_HEADER: &str = "x-shopify-draft-proxy-api-client-id";
pub(in crate::proxy) const ACCESS_SCOPES_HEADER: &str = "x-shopify-draft-proxy-access-scopes";
const RUST_STATE_DUMP_SCHEMA: &str = "shopify-draft-proxy-rust-state/v1";
const OBSERVED_COLLECTION_BASELINE_FIELD: &str = "__shopifyDraftProxyObservedCollectionBaseline";

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

pub(in crate::proxy) struct UnsupportedOperationDispatch<'a> {
    pub request: &'a Request,
    pub query: &'a str,
    pub variables: &'a BTreeMap<String, ResolvedValue>,
    pub operation_type: OperationType,
    pub root_fields: &'a [String],
    pub root_field: &'a str,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Response {
    pub status: u16,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub headers: BTreeMap<String, String>,
    pub body: Value,
}

impl DraftProxy {
    pub fn record_bulk_operation_staged_upload_body(
        &mut self,
        staged_upload_path: &str,
        body: String,
    ) -> bool {
        let registered = self
            .store
            .staged
            .bulk_operation_staged_uploads
            .contains_key(staged_upload_path);
        self.store
            .staged
            .bulk_operation_staged_upload_bodies
            .insert(staged_upload_path.to_string(), body);
        registered
    }
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
    #[serde(default)]
    cursor: Option<String>,
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
    #[serde(default)]
    cursor: Option<String>,
    app_id: Option<String>,
    name: String,
    merchant_code: String,
    description: String,
    options: Vec<String>,
    position: i64,
    created_at: String,
    #[serde(default)]
    updated_at: String,
    selling_plans: Vec<SellingPlanRecord>,
    product_ids: Vec<String>,
    product_variant_ids: Vec<String>,
    #[serde(default)]
    product_cursors: BTreeMap<String, String>,
    #[serde(default)]
    product_variant_cursors: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct StorefrontCartAttributeRecord {
    key: String,
    value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct StorefrontCartRecord {
    internal_id: String,
    sequence: u64,
    created_at: String,
    updated_at: String,
    note: Option<String>,
    attributes: Vec<StorefrontCartAttributeRecord>,
    #[serde(default)]
    buyer_identity: StorefrontCartBuyerIdentityRecord,
    #[serde(default)]
    discount_codes: Vec<String>,
    #[serde(default)]
    applied_gift_cards: Vec<StorefrontCartAppliedGiftCardRecord>,
    #[serde(default)]
    metafields: Vec<StorefrontCartMetafieldRecord>,
    #[serde(default)]
    delivery_addresses: Vec<StorefrontCartDeliveryAddressRecord>,
    #[serde(default)]
    selected_delivery_options: BTreeMap<String, String>,
    #[serde(default)]
    delivery_warning_lines: Vec<StorefrontCartLineRecord>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
struct StorefrontCartDeliveryAddressFields {
    first_name: Option<String>,
    last_name: Option<String>,
    company: Option<String>,
    address1: Option<String>,
    address2: Option<String>,
    city: Option<String>,
    province_code: Option<String>,
    country_code: Option<String>,
    zip: Option<String>,
    phone: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct StorefrontCartDeliveryAddressRecord {
    sequence: u64,
    selected: bool,
    one_time_use: bool,
    fields: StorefrontCartDeliveryAddressFields,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
struct StorefrontCartBuyerIdentityRecord {
    country_code: Option<String>,
    email: Option<String>,
    phone: Option<String>,
    customer_id: Option<String>,
    company_location_id: Option<String>,
    delivery_address_preferences: Vec<Value>,
    preferences: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct StorefrontCartAppliedGiftCardRecord {
    sequence: u64,
    gift_card_id: String,
    code: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct StorefrontCartMetafieldRecord {
    sequence: u64,
    namespace: String,
    key: String,
    value: String,
    metafield_type: String,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct StorefrontCartLineRecord {
    internal_id: String,
    sequence: u64,
    cart_internal_id: String,
    merchandise_id: String,
    quantity: i64,
    attributes: Vec<StorefrontCartAttributeRecord>,
    selling_plan_id: Option<String>,
    out_of_stock_warning: bool,
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
    #[serde(default)]
    cursor: Option<String>,
    name: String,
    query: String,
    resource_type: String,
}

#[derive(Clone)]
struct ResourceStore<T> {
    base: OrderedRecords<T>,
    staged: StagedRecords<T>,
}

impl<T> Default for ResourceStore<T> {
    fn default() -> Self {
        Self {
            base: OrderedRecords::default(),
            staged: StagedRecords::default(),
        }
    }
}

impl<T> ResourceStore<T> {
    fn clear_staged(&mut self) {
        self.staged = StagedRecords::default();
    }

    fn get(&self, id: &str) -> Option<&T> {
        effective_get(&self.base, &self.staged, id)
    }

    fn find(&self, predicate: impl FnMut(&T) -> bool) -> Option<&T> {
        effective_find(&self.base, &self.staged, predicate)
    }

    fn count(&self) -> usize {
        effective_count(&self.base, &self.staged)
    }

    fn has_state(&self) -> bool {
        !self.base.records.is_empty()
            || !self.staged.records.is_empty()
            || !self.staged.tombstones.is_empty()
    }
}

impl<T: Clone> ResourceStore<T> {
    fn records(&self) -> Vec<T> {
        effective_records(&self.base, &self.staged)
    }
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

#[derive(Clone)]
struct Store {
    base: BaseState,
    staged: StagedState,
    products: ResourceStore<ProductRecord>,
    product_variants: ResourceStore<ProductVariantRecord>,
    saved_searches: ResourceStore<SavedSearchRecord>,
    shop_policies: ResourceStore<ShopPolicyRecord>,
}

#[derive(Clone, Default)]
struct BaseState {
    delivery_profiles: OrderedRecords<Value>,
    delivery_promise_providers: OrderedRecords<Value>,
    delivery_promise_provider_complete_location_ids: BTreeSet<String>,
    delivery_promise_participants: OrderedRecords<Value>,
    delivery_promise_participant_baseline_orders: BTreeMap<String, Vec<String>>,
    delivery_promise_participant_cursor_ids: BTreeMap<String, BTreeMap<String, String>>,
    delivery_promise_participant_complete_scopes: BTreeSet<String>,
    delivery_promise_participant_next_cursors: BTreeMap<String, String>,
    delivery_promise_participant_previous_cursors: BTreeMap<String, String>,
    delivery_promise_complete_node_ids: BTreeSet<String>,
    orders: OrderedRecords<Value>,
    order_count_baselines: BTreeMap<String, Value>,
    draft_orders: OrderedRecords<Value>,
    draft_order_count_baselines: BTreeMap<String, Value>,
    discounts: OrderedRecords<Value>,
    discount_count_baselines: BTreeMap<String, Value>,
    marketing_activities: OrderedRecords<Value>,
    marketing_events: OrderedRecords<Value>,
    segments: OrderedRecords<Value>,
    segment_name_ids: BTreeMap<String, BTreeSet<String>>,
    segment_complete_name_probes: BTreeSet<String>,
    segment_known_missing_ids: BTreeSet<String>,
    segment_count_baseline: Option<Value>,
    segment_catalog_complete: bool,
    customer_segment_member_queries: BTreeMap<String, Value>,
    customer_segment_member_query_known_missing_ids: BTreeSet<String>,
    bulk_operations: OrderedRecords<Value>,
    bulk_operations_observed: bool,
    locations: OrderedRecords<Value>,
    inventory_levels: BTreeMap<(String, String), BTreeMap<String, i64>>,
    inventory_level_order: Vec<(String, String)>,
    inventory_level_ids: BTreeMap<(String, String), String>,
    inventory_level_cursors: BTreeMap<String, String>,
    inventory_item_cursors: BTreeMap<String, String>,
    inventory_items_catalog_hydrated: bool,
    inactive_inventory_levels: BTreeSet<(String, String)>,
    inventory_quantity_updated_at: BTreeMap<(String, String, String), String>,
    gift_cards: BTreeMap<String, Value>,
    gift_card_configuration: Option<Value>,
    gift_card_complete_queries: BTreeSet<String>,
    shop: Value,
    storefront_shop: Value,
    storefront_localizations: BTreeMap<String, Value>,
    storefront_product_tags: Value,
    storefront_product_types: Value,
    storefront_payment_settings: Value,
    storefront_locations: OrderedRecords<Value>,
    storefront_location_cursors: BTreeMap<String, String>,
    storefront_public_api_versions: Vec<Value>,
    storefront_menus: OrderedRecords<Value>,
    publication_ids: BTreeSet<String>,
    publication_count: Option<usize>,
    available_locales: BTreeMap<String, String>,
    shop_locales: BTreeMap<String, Value>,
    localization_product_ids: BTreeSet<String>,
    function_metadata: BTreeMap<String, Value>,
    function_metadata_order: Vec<String>,
    function_metadata_catalog_hydrated: bool,
    function_metadata_hydrated_api_types: BTreeSet<String>,
    function_validations: BTreeMap<String, Value>,
    function_validation_order: Vec<String>,
    function_validations_catalog_hydrated: bool,
    function_cart_transforms: BTreeMap<String, Value>,
    function_cart_transform_order: Vec<String>,
    function_cart_transforms_catalog_hydrated: bool,
    function_fulfillment_constraint_rules: BTreeMap<String, Value>,
    function_fulfillment_constraint_rule_order: Vec<String>,
    function_fulfillment_constraint_rules_catalog_hydrated: bool,
    metafield_definitions: BTreeMap<MetafieldDefinitionKey, Value>,
    metafield_definition_owner_catalogs: BTreeSet<String>,
    metafield_definition_namespaces: BTreeSet<(String, String)>,
    inventory_transfers: OrderedRecords<InventoryTransferRecord>,
    b2b_companies: OrderedRecords<Value>,
    b2b_company_count_baselines: BTreeMap<String, Value>,
    b2b_locations: OrderedRecords<Value>,
    b2b_contacts: OrderedRecords<Value>,
    b2b_contact_roles: OrderedRecords<Value>,
    b2b_role_assignments: OrderedRecords<Value>,
    b2b_staff_assignments: OrderedRecords<Value>,
    b2b_staff_member_ids: BTreeSet<String>,
}

type MetafieldDefinitionKey = (String, String, String);

#[derive(Clone, Default)]
struct StagedState {
    product_feeds: StagedRecords<Value>,
    selling_plan_groups: StagedRecords<SellingPlanGroupRecord>,
    selling_plan_groups_overlay_dirty: bool,
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
    storefront_customer_email_index: BTreeMap<String, String>,
    storefront_customer_access_tokens: BTreeMap<String, Value>,
    next_storefront_customer_access_token_id: u64,
    next_storefront_customer_reset_token_id: u64,
    storefront_carts: BTreeMap<String, StorefrontCartRecord>,
    storefront_cart_order: Vec<String>,
    storefront_cart_lines: BTreeMap<String, StorefrontCartLineRecord>,
    storefront_cart_line_order: BTreeMap<String, Vec<String>>,
    next_storefront_cart_id: u64,
    next_storefront_cart_line_id: u64,
    next_storefront_cart_applied_gift_card_id: u64,
    next_storefront_cart_metafield_id: u64,
    next_storefront_cart_delivery_address_id: u64,
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
    delivery_promise_providers: StagedRecords<Value>,
    delivery_promise_participants: StagedRecords<Value>,
    observed_shipping_locations: BTreeMap<String, Value>,
    observed_shipping_location_order: Vec<String>,
    locations: StagedRecords<Value>,
    location_limit_reached: bool,
    delivery_customizations: StagedRecords<Value>,
    segments: StagedRecords<Value>,
    collections: StagedRecords<Value>,
    deleted_collection_handles: BTreeSet<String>,
    collection_jobs: BTreeMap<String, Value>,
    fulfillment_order_deadlines: BTreeMap<String, String>,
    fulfillment_order_cursors: BTreeMap<String, BTreeMap<String, String>>,
    bulk_operations: StagedRecords<Value>,
    bulk_operation_staged_uploads: BTreeMap<String, Option<u64>>,
    bulk_operation_staged_upload_bodies: BTreeMap<String, String>,
    bulk_operation_results: BTreeMap<String, String>,
    discounts: StagedRecords<Value>,
    discount_code_index: BTreeMap<String, String>,
    discount_redeem_code_bulk_creations: BTreeMap<String, Value>,
    gift_cards: BTreeMap<String, Value>,
    markets: BTreeMap<String, Value>,
    deleted_market_ids: BTreeSet<String>,
    catalogs: BTreeMap<String, Value>,
    created_catalog_ids: BTreeSet<String>,
    price_lists: BTreeMap<String, Value>,
    web_presences: BTreeMap<String, Value>,
    markets_hydrated_scopes: BTreeSet<String>,
    markets_upstream_counts: BTreeMap<String, Value>,
    markets_dirty_families: BTreeSet<String>,
    publication_ids: BTreeSet<String>,
    created_publication_ids: BTreeSet<String>,
    // Full publication records staged this scenario, keyed by publication gid.
    // Seeded from `seedPublications` (base/default publications) and extended by
    // `publicationCreate`. Drives the local `publication`/`channel`/`channels`/
    // `publicationsCount`/`publishedProductsCount` roots without upstream replay.
    // Empty for every scenario that does not seed publications, leaving the
    // existing passthrough behavior for those roots untouched.
    publications: BTreeMap<String, Value>,
    // Current app publication resolved from `currentAppInstallation.publication`
    // by current-channel publishable mutations in live-hybrid mode. The separate
    // resolved flag distinguishes "not looked up yet" from "looked up and Shopify
    // returned no current publication".
    current_channel_publication_id: Option<String>,
    current_channel_publication_resolved: bool,
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
    marketing_delete_all_external_app_ids: BTreeSet<String>,
    webhook_subscriptions: BTreeMap<String, Value>,
    b2b_companies: BTreeMap<String, Value>,
    deleted_b2b_company_ids: BTreeSet<String>,
    b2b_locations: StagedRecords<Value>,
    b2b_contacts: BTreeMap<String, Value>,
    b2b_contact_roles: BTreeMap<String, Value>,
    b2b_role_assignments: BTreeMap<String, Value>,
    b2b_staff_assignments: BTreeMap<String, Value>,
    deleted_b2b_staff_assignment_ids: BTreeSet<String>,
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
    active_inventory_levels: BTreeSet<(String, String)>,
    inventory_quantity_updated_at: BTreeMap<(String, String, String), String>,
    next_inventory_quantity_timestamp: u64,
    inventory_adjustment_groups: BTreeMap<String, Value>,
    inventory_transfers: StagedRecords<InventoryTransferRecord>,
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
    deleted_metafield_definitions: BTreeSet<MetafieldDefinitionKey>,
    metafield_reference_ids: BTreeSet<String>,
    media_files: StagedRecords<Value>,
    media_file_cursors: BTreeMap<String, String>,
    locally_created_media_file_ids: BTreeSet<String>,
    media_files_overlay_dirty: bool,
    media_ready_on_read: BTreeSet<String>,
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
    deleted_payment_customization_ids: BTreeSet<String>,
    payment_customization_catalog_hydrated: bool,
    customer_payment_methods: BTreeMap<String, Value>,
    customer_payment_method_customer_index: BTreeMap<String, Vec<String>>,
    next_customer_payment_method_id: u64,
    abandonments: BTreeMap<String, Value>,
    orders: StagedRecords<Value>,
    draft_orders: StagedRecords<Value>,
    returns: BTreeMap<String, Value>,
    returns_by_order: BTreeMap<String, Vec<String>>,
    reverse_deliveries: BTreeMap<String, Value>,
    reverse_fulfillment_orders: BTreeMap<String, Value>,
    next_refund_id: u64,
    next_refund_line_item_id: u64,
    next_order_id: u64,
    next_order_number: u64,
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
    deleted_function_validation_ids: BTreeSet<String>,
    function_cart_transforms: BTreeMap<String, Value>,
    function_cart_transform_order: Vec<String>,
    deleted_function_cart_transform_ids: BTreeSet<String>,
    function_fulfillment_constraint_rules: BTreeMap<String, Value>,
    function_fulfillment_constraint_rule_order: Vec<String>,
    deleted_function_fulfillment_constraint_rule_ids: BTreeSet<String>,
    tax_app_configuration: Option<Value>,
    function_validations_dirty: bool,
    function_cart_transforms_dirty: bool,
    function_fulfillment_constraint_rules_dirty: bool,
    // True once any function lifecycle (validation / cart-transform) has been
    // staged this session. Distinguishes a post-delete local read (serve the
    // empty local result) from a cold read with no local backing (forward to
    // the upstream so function ownership metadata reflects real installs).
    functions_dirty: bool,
    available_backup_regions: BTreeMap<String, Value>,
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
    #[serde(default)]
    created_at: String,
    status: String,
    origin_location_id: String,
    destination_location_id: String,
    #[serde(default)]
    tags: Vec<String>,
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

    fn values_mut(&mut self) -> impl Iterator<Item = &mut T> {
        let tombstones = &self.tombstones;
        self.records
            .iter_mut()
            .filter(move |(id, _)| !tombstones.contains(*id))
            .map(|(_, record)| record)
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

impl StagedState {
    fn new_session() -> Self {
        Self {
            // Most staged collections use their Rust defaults; session counters
            // intentionally start at Shopify-like first synthetic IDs.
            next_store_credit_account_id: 1,
            next_store_credit_transaction_id: 1,
            next_b2b_company_id: 1,
            next_customer_payment_method_id: 1,
            next_refund_id: 1,
            next_refund_line_item_id: 1,
            next_order_id: 1,
            next_order_number: 1,
            next_draft_order_id: 1,
            next_draft_order_bulk_tag_job_id: 1,
            next_order_customer_order_id: 1,
            order_payment_next_transaction_id: 3,
            order_edit_variant_catalog: Value::Object(serde_json::Map::new()),
            next_b2b_contact_id: 1,
            next_b2b_contact_role_assignment_id: 1,
            next_storefront_customer_access_token_id: 1,
            next_storefront_customer_reset_token_id: 1,
            next_storefront_cart_id: 1,
            next_storefront_cart_line_id: 1,
            next_storefront_cart_applied_gift_card_id: 1,
            next_storefront_cart_metafield_id: 1,
            next_storefront_cart_delivery_address_id: 1,
            ..Default::default()
        }
    }
}

impl Default for Store {
    fn default() -> Self {
        Self {
            base: BaseState::default(),
            staged: StagedState::new_session(),
            products: ResourceStore::default(),
            product_variants: ResourceStore::default(),
            saved_searches: ResourceStore::default(),
            shop_policies: ResourceStore::default(),
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

fn draft_order_records_have_same_logical_create(left: &Value, right: &Value) -> bool {
    let Some(left_email) = left.get("email").and_then(Value::as_str) else {
        return false;
    };
    if left_email.is_empty() || right.get("email").and_then(Value::as_str) != Some(left_email) {
        return false;
    }
    let Some(left_tags) = string_set_from_array(left.get("tags")) else {
        return false;
    };
    let Some(right_tags) = string_set_from_array(right.get("tags")) else {
        return false;
    };
    !left_tags.is_empty() && left_tags == right_tags
}

fn string_set_from_array(value: Option<&Value>) -> Option<BTreeSet<String>> {
    value?
        .as_array()?
        .iter()
        .map(|entry| entry.as_str().map(str::to_string))
        .collect::<Option<BTreeSet<_>>>()
}

fn saved_search_semantic_key(record: &SavedSearchRecord) -> (String, String, String) {
    (
        record.resource_type.clone(),
        record.name.clone(),
        record.query.clone(),
    )
}

fn effective_saved_search_records(
    base: &OrderedRecords<SavedSearchRecord>,
    staged: &StagedRecords<SavedSearchRecord>,
) -> Vec<SavedSearchRecord> {
    let mut records = Vec::new();
    let mut observed_keys = BTreeSet::new();
    for (id, record) in base
        .order
        .iter()
        .filter_map(|id| base.records.get(id).map(|record| (id.as_str(), record)))
    {
        if staged.is_tombstoned(id) {
            continue;
        }
        let effective = staged.get(id).unwrap_or(record);
        observed_keys.insert(saved_search_semantic_key(effective));
        records.push(effective.clone());
    }
    for (id, record) in staged
        .order
        .iter()
        .filter_map(|id| staged.records.get(id).map(|record| (id.as_str(), record)))
    {
        if staged.is_tombstoned(id) || base.records.contains_key(id) {
            continue;
        }
        if !observed_keys.insert(saved_search_semantic_key(record)) {
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

fn normalized_collection_handle(handle: &str) -> Option<String> {
    let handle = handle.trim();
    (!handle.is_empty()).then(|| handle.to_string())
}

fn merge_json_values(target: &mut Value, observed: &Value) {
    match (target, observed) {
        (Value::Object(target), Value::Object(observed)) => {
            for (key, value) in observed {
                match target.get_mut(key) {
                    Some(existing) => merge_json_values(existing, value),
                    None => {
                        target.insert(key.clone(), value.clone());
                    }
                }
            }
        }
        (target, observed) => {
            *target = observed.clone();
        }
    }
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
        self.staged = StagedState::new_session();
        self.products.clear_staged();
        self.product_variants.clear_staged();
        self.saved_searches.clear_staged();
        self.shop_policies.clear_staged();
    }

    fn replace_base_products(&mut self, products: Vec<ProductRecord>) {
        self.products.base.replace_ordered(
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

    fn has_known_publication_ids(&self) -> bool {
        !self.base.publication_ids.is_empty()
            || !self.staged.publication_ids.is_empty()
            || !self.staged.publications.is_empty()
    }

    fn has_publication_id(&self, id: &str) -> bool {
        self.base.publication_ids.contains(id)
            || self.staged.publication_ids.contains(id)
            || self.staged.publications.contains_key(id)
    }

    fn current_publication_ids(&self) -> Vec<&str> {
        if self.staged.current_channel_publication_resolved {
            return self
                .staged
                .current_channel_publication_id
                .as_deref()
                .into_iter()
                .collect();
        }

        Vec::new()
    }

    fn resource_is_published_on_current_publication(&self, resource_id: &str) -> bool {
        let publication_ids = self.current_publication_ids();
        self.staged
            .resource_publications
            .get(resource_id)
            .is_some_and(|publications| {
                publication_ids
                    .iter()
                    .any(|publication_id| publications.contains(*publication_id))
            })
    }

    fn product_is_published_on_current_publication(&self, product: &ProductRecord) -> bool {
        if product.status != "ACTIVE" {
            return false;
        }

        let publication_ids = self.current_publication_ids();
        publication_ids
            .iter()
            .any(|publication_id| product_is_published_on_publication(product, publication_id))
            || self
                .staged
                .resource_publications
                .get(&product.id)
                .is_some_and(|publications| {
                    publication_ids
                        .iter()
                        .any(|publication_id| publications.contains(*publication_id))
                })
    }

    fn product_is_published_on_known_publication(&self, product: &ProductRecord) -> bool {
        if product.status != "ACTIVE" || !self.has_known_publication_catalog() {
            return false;
        }

        self.staged
            .resource_publications
            .get(&product.id)
            .is_some_and(|publications| publications.iter().any(|id| self.has_publication_id(id)))
    }

    fn publication_id_for_channel_id(&self, channel_id: &str) -> Option<String> {
        self.staged.publications.iter().find_map(|(id, record)| {
            let matches = record
                .get("channel")
                .and_then(|channel| channel.get("id"))
                .and_then(Value::as_str)
                == Some(channel_id);
            matches.then(|| id.clone())
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
        self.observed_shop_currency_code().unwrap_or_default()
    }

    pub(in crate::proxy) fn observed_shop_currency_code(&self) -> Option<String> {
        self.base
            .shop
            .get("currencyCode")
            .and_then(Value::as_str)
            .filter(|currency| !currency.is_empty())
            .map(str::to_string)
    }

    pub(in crate::proxy) fn shop_taxes_included(&self) -> Option<bool> {
        self.base.shop.get("taxesIncluded").and_then(Value::as_bool)
    }

    pub(in crate::proxy) fn shop_duties_included(&self) -> Option<bool> {
        self.base
            .shop
            .get("dutiesIncluded")
            .and_then(Value::as_bool)
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
        self.shop_policies.get(id)
    }

    fn shop_policy_by_type(&self, policy_type: &str) -> Option<&ShopPolicyRecord> {
        self.shop_policies
            .find(|policy| policy.policy_type == policy_type)
    }

    fn shop_policies(&self) -> Vec<ShopPolicyRecord> {
        self.shop_policies.records()
    }

    fn stage_shop_policy(&mut self, policy: ShopPolicyRecord) {
        self.shop_policies.staged.stage(policy.id.clone(), policy);
    }

    fn observed_order_by_id(&self, id: &str) -> Option<&Value> {
        effective_get(&self.base.orders, &self.staged.orders, id)
    }

    fn effective_orders(&self) -> Vec<Value> {
        effective_records(&self.base.orders, &self.staged.orders)
    }

    fn segment_by_id(&self, id: &str) -> Option<&Value> {
        effective_get(&self.base.segments, &self.staged.segments, id)
    }

    fn effective_segment_count(&self) -> usize {
        if let Some(base_count) = self
            .base
            .segment_count_baseline
            .as_ref()
            .and_then(|count| count.get("count"))
            .and_then(Value::as_u64)
        {
            let mut count = base_count as usize;
            for id in &self.staged.segments.tombstones {
                if self.base.segments.records.contains_key(id) {
                    count = count.saturating_sub(1);
                }
            }
            for id in self.staged.segments.records.keys() {
                if !self.base.segments.records.contains_key(id)
                    && !self.staged.segments.is_tombstoned(id)
                {
                    count = count.saturating_add(1);
                }
            }
            return count;
        }
        self.base
            .segments
            .records
            .keys()
            .filter(|id| !self.staged.segments.is_tombstoned(id))
            .count()
            + self
                .staged
                .segments
                .records
                .keys()
                .filter(|id| !self.base.segments.records.contains_key(*id))
                .filter(|id| !self.staged.segments.is_tombstoned(id))
                .count()
    }

    fn observe_base_segment(&mut self, segment: Value) {
        let Some(id) = segment
            .get("id")
            .and_then(Value::as_str)
            .map(str::to_string)
        else {
            return;
        };
        if self.staged.segments.is_tombstoned(&id) || self.staged.segments.contains_staged(&id) {
            return;
        }
        if let Some(previous_name) = self
            .base
            .segments
            .get(&id)
            .and_then(|segment| segment.get("name"))
            .and_then(Value::as_str)
            .map(str::to_string)
        {
            if let Some(ids) = self.base.segment_name_ids.get_mut(&previous_name) {
                ids.remove(&id);
                if ids.is_empty() {
                    self.base.segment_name_ids.remove(&previous_name);
                }
            }
        }
        if let Some(name) = segment.get("name").and_then(Value::as_str) {
            self.base
                .segment_name_ids
                .entry(name.to_string())
                .or_default()
                .insert(id.clone());
        }
        self.base.segment_known_missing_ids.remove(&id);
        self.base.segments.insert(id, segment);
    }

    fn rebuild_segment_name_index(&mut self) {
        self.base.segment_name_ids.clear();
        for (id, segment) in &self.base.segments.records {
            let Some(name) = segment.get("name").and_then(Value::as_str) else {
                continue;
            };
            self.base
                .segment_name_ids
                .entry(name.to_string())
                .or_default()
                .insert(id.clone());
        }
    }

    fn customer_segment_member_query_by_id(&self, id: &str) -> Option<&Value> {
        self.staged
            .customer_segment_member_queries
            .get(id)
            .or_else(|| self.base.customer_segment_member_queries.get(id))
    }

    fn observe_base_customer_segment_member_query(&mut self, record: Value) {
        let Some(id) = record.get("id").and_then(Value::as_str).map(str::to_string) else {
            return;
        };
        if self
            .staged
            .customer_segment_member_queries
            .contains_key(&id)
        {
            return;
        }
        self.base
            .customer_segment_member_query_known_missing_ids
            .remove(&id);
        self.base.customer_segment_member_queries.insert(id, record);
    }

    fn observe_base_order(&mut self, order: Value) {
        let Some(id) = order.get("id").and_then(Value::as_str).map(str::to_string) else {
            return;
        };
        if self.staged.orders.is_tombstoned(&id) || self.staged.orders.contains_staged(&id) {
            return;
        }
        self.base.orders.insert(id, order);
    }

    fn observe_order_count_baseline(&mut self, key: String, count: Value) {
        self.base.order_count_baselines.insert(key, count);
    }

    fn order_count_baseline(&self, key: &str) -> Option<&Value> {
        self.base.order_count_baselines.get(key)
    }

    fn observed_draft_order_by_id(&self, id: &str) -> Option<&Value> {
        effective_get(&self.base.draft_orders, &self.staged.draft_orders, id)
    }

    fn effective_draft_orders(&self) -> Vec<Value> {
        let mut records = Vec::new();
        for (id, record) in self.base.draft_orders.order.iter().filter_map(|id| {
            self.base
                .draft_orders
                .records
                .get(id)
                .map(|record| (id.as_str(), record))
        }) {
            if self.staged.draft_orders.is_tombstoned(id) {
                continue;
            }
            if let Some(staged_record) = self.staged.draft_orders.get(id) {
                records.push(staged_record.clone());
            } else if self
                .staged_draft_order_logical_duplicate_for_base(id, record)
                .is_none()
            {
                records.push(record.clone());
            }
        }
        for (id, record) in self.staged.draft_orders.order.iter().filter_map(|id| {
            self.staged
                .draft_orders
                .records
                .get(id)
                .map(|record| (id.as_str(), record))
        }) {
            if self.staged.draft_orders.is_tombstoned(id)
                || self.base.draft_orders.records.contains_key(id)
            {
                continue;
            }
            records.push(record.clone());
        }
        records
    }

    fn observe_base_draft_order(&mut self, draft_order: Value) {
        let Some(id) = draft_order
            .get("id")
            .and_then(Value::as_str)
            .map(str::to_string)
        else {
            return;
        };
        if self.staged.draft_orders.is_tombstoned(&id)
            || self.staged.draft_orders.contains_staged(&id)
        {
            return;
        }
        self.base.draft_orders.insert(id, draft_order);
    }

    fn observe_draft_order_count_baseline(&mut self, key: String, count: Value) {
        self.base.draft_order_count_baselines.insert(key, count);
    }

    fn draft_order_count_baseline(&self, key: &str) -> Option<&Value> {
        self.base.draft_order_count_baselines.get(key)
    }

    fn base_draft_order_logical_duplicate_for_staged(
        &self,
        staged_id: &str,
        staged_draft_order: &Value,
    ) -> Option<&Value> {
        self.base
            .draft_orders
            .records
            .iter()
            .filter(|(base_id, _)| base_id.as_str() != staged_id)
            .filter(|(base_id, _)| !self.staged.draft_orders.is_tombstoned(base_id))
            .map(|(_, base_draft_order)| base_draft_order)
            .find(|base_draft_order| {
                draft_order_records_have_same_logical_create(base_draft_order, staged_draft_order)
            })
    }

    fn staged_draft_order_logical_duplicate_for_base(
        &self,
        base_id: &str,
        base_draft_order: &Value,
    ) -> Option<&Value> {
        self.staged
            .draft_orders
            .records
            .iter()
            .filter(|(staged_id, _)| staged_id.as_str() != base_id)
            .filter(|(staged_id, _)| !self.staged.draft_orders.is_tombstoned(staged_id))
            .map(|(_, staged_draft_order)| staged_draft_order)
            .find(|staged_draft_order| {
                draft_order_records_have_same_logical_create(base_draft_order, staged_draft_order)
            })
    }

    fn observe_discount_count_baseline(&mut self, key: String, count: Value) {
        self.base.discount_count_baselines.insert(key, count);
    }

    fn inventory_transfer_by_id(&self, id: &str) -> Option<&InventoryTransferRecord> {
        effective_get(
            &self.base.inventory_transfers,
            &self.staged.inventory_transfers,
            id,
        )
    }

    fn inventory_transfers(&self) -> Vec<InventoryTransferRecord> {
        effective_records(
            &self.base.inventory_transfers,
            &self.staged.inventory_transfers,
        )
    }

    fn inventory_transfer_count(&self) -> usize {
        effective_count(
            &self.base.inventory_transfers,
            &self.staged.inventory_transfers,
        )
    }

    fn has_inventory_transfer_state(&self) -> bool {
        !self.base.inventory_transfers.records.is_empty()
            || !self.staged.inventory_transfers.is_empty()
    }

    fn has_base_inventory_transfer_state(&self) -> bool {
        !self.base.inventory_transfers.records.is_empty()
    }

    fn observe_base_inventory_transfer(&mut self, transfer: InventoryTransferRecord) {
        if self.staged.inventory_transfers.is_tombstoned(&transfer.id)
            || self
                .staged
                .inventory_transfers
                .contains_staged(&transfer.id)
        {
            return;
        }
        self.base
            .inventory_transfers
            .insert(transfer.id.clone(), transfer);
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
        self.products.get(id)
    }

    fn product_by_handle(&self, handle: &str) -> Option<&ProductRecord> {
        self.products.find(|product| product.handle == handle)
    }

    fn products(&self) -> Vec<ProductRecord> {
        self.products.records()
    }

    fn product_count(&self) -> usize {
        self.products.count()
    }

    fn has_product_state(&self) -> bool {
        self.products.has_state()
    }

    fn has_collection_state(&self) -> bool {
        !self.staged.collections.is_empty()
            || !self.staged.deleted_collection_handles.is_empty()
            || !self.staged.collection_jobs.is_empty()
    }

    fn product_feed_by_id(&self, id: &str) -> Option<&Value> {
        self.staged.product_feeds.get(id)
    }

    fn product_feeds(&self) -> Vec<Value> {
        self.staged.product_feeds.values().cloned().collect()
    }

    fn marketing_activity_by_id(&self, id: &str) -> Option<&Value> {
        effective_get(
            &self.base.marketing_activities,
            &self.staged.marketing_activities,
            id,
        )
    }

    fn marketing_activities(&self) -> Vec<Value> {
        effective_records(
            &self.base.marketing_activities,
            &self.staged.marketing_activities,
        )
    }

    fn marketing_events(&self) -> Vec<Value> {
        let mut events = Vec::new();
        let mut seen_event_ids = BTreeSet::new();
        let mut hidden_event_ids = BTreeSet::new();

        for (id, activity) in self
            .base
            .marketing_activities
            .records
            .iter()
            .chain(self.staged.marketing_activities.records.iter())
        {
            if self.staged.marketing_activities.is_tombstoned(id) {
                if let Some(event_id) = activity["marketingEvent"]["id"].as_str() {
                    hidden_event_ids.insert(event_id.to_string());
                }
            }
        }

        for activity in self.marketing_activities() {
            let event = &activity["marketingEvent"];
            if event.is_null() {
                continue;
            }
            let Some(event_id) = event["id"].as_str() else {
                continue;
            };
            if seen_event_ids.insert(event_id.to_string()) {
                let mut event = event.clone();
                if let Some(base_event) = self.base.marketing_events.get(event_id) {
                    let mut merged = base_event.clone();
                    merge_json_values(&mut merged, &event);
                    event = merged;
                }
                events.push(event);
            }
        }

        for event in self.base.marketing_events.ordered_values() {
            let Some(event_id) = event["id"].as_str() else {
                continue;
            };
            if hidden_event_ids.contains(event_id) || !seen_event_ids.insert(event_id.to_string()) {
                continue;
            }
            events.push(event.clone());
        }

        events
    }

    fn marketing_event_by_id(&self, id: &str) -> Option<Value> {
        self.marketing_events()
            .into_iter()
            .find(|event| event["id"].as_str() == Some(id))
    }

    fn has_marketing_overlay_state(&self) -> bool {
        !self.staged.marketing_activities.is_empty()
            || self.staged.marketing_delete_all_external
            || !self.staged.marketing_delete_all_external_app_ids.is_empty()
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
        self.products.staged.is_tombstoned(id)
    }

    fn has_localization_product(&self, id: &str) -> bool {
        !self.products.staged.is_tombstoned(id)
            && (self.has_product(id) || self.base.localization_product_ids.contains(id))
    }

    fn stage_product(&mut self, product: ProductRecord) {
        self.products.staged.stage(product.id.clone(), product);
    }

    fn stage_observed_product(&mut self, product: ProductRecord) {
        // Upstream reads describe base state and must never resurrect a local
        // read-after-delete tombstone. This guard belongs at the observation
        // boundary so every hydration path preserves staged deletion
        // precedence, even when a mixed node batch has not cached this ID yet.
        if self.product_is_tombstoned(&product.id) {
            return;
        }
        let merged = match self.product_by_id(&product.id).cloned() {
            Some(existing) => merge_observed_product(existing, product),
            None => product,
        };
        self.stage_product(merged);
    }

    fn observe_base_product(&mut self, product: ProductRecord) {
        if self.product_is_tombstoned(&product.id) {
            return;
        }
        let merged = self
            .products
            .base
            .get(&product.id)
            .cloned()
            .map(|existing| merge_observed_product(existing, product.clone()))
            .unwrap_or(product);
        self.products.base.insert(merged.id.clone(), merged);
    }

    fn stage_observed_product_json(&mut self, value: &Value) {
        if let Some(product) = product_state_from_json(value) {
            self.stage_observed_product(product);
        }
    }

    /// Merge an upstream catalog row into baseline state without touching the
    /// staged overlay. Bulk exports hydrate selected fields incrementally, so
    /// preserve any baseline fields learned by earlier reads when the current
    /// query did not select them.
    fn observe_base_product_json(&mut self, value: &Value) {
        let Some(id) = value.get("id").and_then(Value::as_str) else {
            return;
        };
        let mut merged = self
            .products
            .base
            .get(id)
            .map(product_state_json)
            .unwrap_or_else(|| json!({}));
        let (Some(merged), Some(observed)) = (merged.as_object_mut(), value.as_object()) else {
            return;
        };
        if let Some(extra_fields) = merged
            .remove("extraFields")
            .and_then(|extra_fields| extra_fields.as_object().cloned())
        {
            merged.extend(extra_fields);
        }
        merged.extend(observed.clone());
        let Some(product) = product_state_from_json(&Value::Object(merged.clone())) else {
            return;
        };
        self.products.base.insert(product.id.clone(), product);
    }

    fn observe_base_product_variant_json(&mut self, value: &Value, product_id: &str) {
        let Some(id) = value.get("id").and_then(Value::as_str) else {
            return;
        };
        let mut observed = value.clone();
        let Some(observed_object) = observed.as_object_mut() else {
            return;
        };
        observed_object.insert("productId".to_string(), json!(product_id));
        let mut merged = self
            .product_variants
            .base
            .get(id)
            .map(product_variant_state_json)
            .unwrap_or_else(|| json!({}));
        let (Some(merged), Some(observed)) = (merged.as_object_mut(), observed.as_object()) else {
            return;
        };
        merged.extend(observed.clone());
        let Some(variant) =
            product_variant_state_from_observed_json(&Value::Object(merged.clone()))
        else {
            return;
        };
        self.product_variants
            .base
            .insert(variant.id.clone(), variant);
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
        let preserve_observed_products = collection
            .get(OBSERVED_COLLECTION_BASELINE_FIELD)
            .and_then(Value::as_bool)
            .unwrap_or(false)
            && collection.get("products").is_some();
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
        if !preserve_observed_products
            && (!product_nodes.is_empty() || !collection_record.contains_key("products"))
        {
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
        if !preserve_observed_products || !collection_record.contains_key("productsCount") {
            collection_record.insert(
                "productsCount".to_string(),
                count_object(normalized_products.len()),
            );
        }
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

    fn collection_by_handle(&self, handle: &str) -> Option<&Value> {
        let handle = handle.trim();
        if handle.is_empty() || self.collection_handle_is_deleted(handle) {
            return None;
        }
        self.staged
            .collections
            .values()
            .find(|collection| collection.get("handle").and_then(Value::as_str) == Some(handle))
    }

    /// True when the collection id has been locally deleted (tombstoned). Unlike a
    /// never-seen collection, a tombstoned one must be served from local state
    /// (collection: null) for read-after-delete rather than forwarded upstream.
    fn collection_is_deleted(&self, id: &str) -> bool {
        self.staged.collections.is_tombstoned(id)
    }

    fn collection_handle_is_deleted(&self, handle: &str) -> bool {
        normalized_collection_handle(handle)
            .is_some_and(|handle| self.staged.deleted_collection_handles.contains(&handle))
    }

    fn delete_collection_handle(&mut self, handle: &str) {
        if let Some(handle) = normalized_collection_handle(handle) {
            self.staged.deleted_collection_handles.insert(handle);
        }
    }

    fn stage_collection(&mut self, collection: Value) {
        if let Some(id) = collection.get("id").and_then(Value::as_str) {
            if let Some(handle) = collection.get("handle").and_then(Value::as_str) {
                if let Some(handle) = normalized_collection_handle(handle) {
                    self.staged.deleted_collection_handles.remove(&handle);
                }
            }
            self.staged.collections.insert(id.to_string(), collection);
        }
    }

    fn delete_collection(&mut self, id: &str) -> bool {
        let deleted_handle = self
            .staged
            .collections
            .get(id)
            .and_then(|collection| collection.get("handle"))
            .and_then(Value::as_str)
            .and_then(normalized_collection_handle);
        let existed = self.staged.collections.tombstone_staged(id);
        if existed {
            if let Some(handle) = deleted_handle {
                self.staged.deleted_collection_handles.insert(handle);
            }
        }
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
        self.products.staged.remove_staged(id);
        self.products.staged.tombstone(id.to_string());
    }

    fn product_staged_or_base(&self, id: &str) -> Option<ProductRecord> {
        self.product_by_id(id).cloned()
    }

    fn product_variant_by_id(&self, id: &str) -> Option<&ProductVariantRecord> {
        self.product_variants.get(id)
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
        self.product_variants
            .find(|variant| variant.inventory_item.id == inventory_item_id)
    }

    fn product_variants_for_product(&self, product_id: &str) -> Vec<ProductVariantRecord> {
        let mut variants = self
            .product_variants
            .records()
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
        self.product_variants
            .staged
            .stage(variant.id.clone(), variant);
    }

    fn observe_base_product_variant(&mut self, variant: ProductVariantRecord) {
        if self.product_variants.staged.is_tombstoned(&variant.id) {
            return;
        }
        self.product_variants
            .base
            .insert(variant.id.clone(), variant);
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
        for id in self.product_variants.staged.order.iter() {
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
        self.product_variants.staged.order =
            normalized_order(self.product_variants.staged.records.keys(), reordered);

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
        for mut variant in self.product_variants.records() {
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
        self.product_variants.staged.remove_staged(id);
        if existed {
            self.product_variants.staged.tombstone(id.to_string());
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
                self.product_variants.staged.stage(id, variant.clone());
                reordered_variants.push(variant);
            }
        }
        self.product_variants.staged.order =
            normalized_order(self.product_variants.staged.records.keys(), staged_order);
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
        self.staged.selling_plan_groups_overlay_dirty = true;
        self.staged
            .selling_plan_groups
            .stage(group.id.clone(), group);
    }

    fn observe_selling_plan_group(&mut self, group: SellingPlanGroupRecord) {
        self.staged
            .selling_plan_groups
            .stage(group.id.clone(), group);
    }

    fn delete_selling_plan_group(&mut self, id: &str) -> bool {
        let had_staged = self.staged.selling_plan_groups.remove_staged(id).is_some();
        if had_staged {
            self.staged.selling_plan_groups.tombstone(id.to_string());
            self.staged.selling_plan_groups_overlay_dirty = true;
        }
        had_staged
    }

    fn saved_search_base_with_defaults(
        &self,
        resource_type: &str,
    ) -> OrderedRecords<SavedSearchRecord> {
        let mut base = OrderedRecords::default();
        let has_base_records = self.has_base_saved_searches_for_resource(resource_type);
        if !has_base_records {
            for record in default_saved_searches(resource_type) {
                base.insert(record.id.clone(), record);
            }
        }
        for record in self.saved_searches.base.ordered_values() {
            if record.resource_type == resource_type {
                base.insert(record.id.clone(), record.clone());
            }
        }
        base
    }

    fn has_base_saved_searches_for_resource(&self, resource_type: &str) -> bool {
        self.saved_searches
            .base
            .ordered_values()
            .iter()
            .any(|record| record.resource_type == resource_type)
    }

    fn saved_search_by_id(&self, id: &str) -> Option<SavedSearchRecord> {
        if self.saved_searches.staged.is_tombstoned(id) {
            return None;
        }
        self.saved_searches
            .staged
            .get(id)
            .cloned()
            .or_else(|| self.saved_searches.base.get(id).cloned())
            .or_else(|| {
                let record = default_saved_search_by_id(id)?;
                (!self.has_base_saved_searches_for_resource(&record.resource_type))
                    .then_some(record)
            })
    }

    fn saved_searches_for_resource(&self, resource_type: &str) -> Vec<SavedSearchRecord> {
        let base = self.saved_search_base_with_defaults(resource_type);
        effective_saved_search_records(&base, &self.saved_searches.staged)
            .into_iter()
            .filter(|record| record.resource_type == resource_type)
            .collect()
    }

    fn has_saved_search_overlay(&self, resource_type: &str) -> bool {
        self.saved_searches
            .staged
            .records
            .values()
            .any(|record| record.resource_type == resource_type)
            || self.saved_searches.staged.tombstones.iter().any(|id| {
                self.saved_searches
                    .base
                    .get(id)
                    .is_some_and(|record| record.resource_type == resource_type)
                    || default_saved_search_by_id(id)
                        .is_some_and(|record| record.resource_type == resource_type)
            })
    }

    fn stage_saved_search(&mut self, record: SavedSearchRecord) {
        self.saved_searches.staged.stage(record.id.clone(), record);
    }

    fn delete_saved_search(&mut self, id: &str) -> bool {
        let had_staged = self.saved_searches.staged.remove_staged(id).is_some();
        let has_default = default_saved_search_by_id(id)
            .map(|record| !self.has_base_saved_searches_for_resource(&record.resource_type))
            .unwrap_or(false);
        let has_base = self.saved_searches.base.get(id).is_some() || has_default;
        if has_base {
            self.saved_searches.staged.tombstone(id.to_string());
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

pub(in crate::proxy) struct MutationFieldOutcome {
    value: Value,
    log_draft: Option<LogDraft>,
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

fn default_commit_transport(_request: Request) -> Response {
    json_error(501, "No Rust commit transport configured")
}

fn default_upstream_transport(_request: Request) -> Response {
    json_error(502, "No Rust upstream transport configured")
}

type RuntimeClock = Arc<dyn Fn() -> time::OffsetDateTime + Send + Sync>;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct RequestEntityCacheKey {
    api_surface: ApiSurface,
    api_version: String,
    /// Storefront identity and visibility can vary with `@inContext`; Admin
    /// entities use `None`. Keeping the context in the key prevents a future
    /// multi-context execution from reusing the wrong surface projection.
    context: Option<String>,
    id: String,
}

impl RequestEntityCacheKey {
    fn admin(api_version: &str, id: &str) -> Self {
        Self {
            api_surface: ApiSurface::Admin,
            api_version: api_version.to_string(),
            context: None,
            id: id.to_string(),
        }
    }

    fn storefront(api_version: &str, id: &str, context: String) -> Self {
        Self {
            api_surface: ApiSurface::Storefront,
            api_version: api_version.to_string(),
            context: Some(context),
            id: id.to_string(),
        }
    }
}

type RequestEntityCache = RefCell<BTreeMap<RequestEntityCacheKey, NodeLoadState<EntityRef>>>;

#[derive(Clone)]
struct RequestNodeHydration {
    response: Response,
    upstream_response_keys: BTreeSet<String>,
}

#[derive(Clone, Default)]
struct ExecutionSession {
    api_surface: Option<ApiSurface>,
    api_version: Option<String>,
    mutation_log_start: Option<usize>,
    discount_refs_preflighted: bool,
    owner_metafield_hydrated_ids: BTreeSet<String>,
    upstream_query_response: Option<Response>,
    upstream_query_data: Option<Value>,
    upstream_query_selections: BTreeMap<String, Vec<SelectedField>>,
    localization_context_preflighted: bool,
    markets_query_preflighted: bool,
    node_hydration: Option<RequestNodeHydration>,
    owner_metafield_read_ids: BTreeSet<String>,
    owner_metafield_missing_ids: BTreeSet<String>,
    entity_cache: RequestEntityCache,
}

impl ExecutionSession {
    fn admin(version: crate::admin_graphql::AdminApiVersion) -> Self {
        Self {
            api_surface: Some(ApiSurface::Admin),
            api_version: Some(version.as_str().to_string()),
            ..Self::default()
        }
    }

    fn storefront(version: crate::storefront_graphql::StorefrontApiVersion) -> Self {
        Self {
            api_surface: Some(ApiSurface::Storefront),
            api_version: Some(version.as_str().to_string()),
            ..Self::default()
        }
    }

    fn api_version(&self, api_surface: ApiSurface) -> &str {
        assert_eq!(
            self.api_surface,
            Some(api_surface),
            "GraphQL entity loading crossed execution surfaces",
        );
        self.api_version
            .as_deref()
            .expect("GraphQL entity loading requires an active execution session")
    }
}

fn default_runtime_clock() -> time::OffsetDateTime {
    time::OffsetDateTime::now_utc()
}

#[derive(Clone)]
pub struct DraftProxy {
    config: Config,
    log_entries: Vec<Value>,
    registry: ResolverRegistry,
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
    /// All GraphQL-execution transients live behind one request-lifetime
    /// boundary. A new value replaces this session before every Admin or
    /// Storefront schema execution; it is never dumped or restored.
    execution_session: ExecutionSession,
    commit_transport: CommitTransport,
    upstream_transport: UpstreamTransport,
    storefront_upstream_transport: UpstreamTransport,
}

mod admin_shipping_gift_cards;
mod app_shipping_helpers;
mod b2b_customers;
mod civil_date;
mod commit;
mod connection;
mod core;
mod discounts;
mod functions;
mod graphql_error_compat;
mod graphql_runtime;
mod json_helpers;
mod localization_markets_catalogs;
mod market_unsupported_country_regions;
mod marketing_webhooks_inventory;
mod markets_catalog_data;
mod markets_catalog_helpers;
mod media_products_saved_searches;
mod metafield_metaobject_definitions;
mod metafields_orders_payments;
mod metaobjects;
mod money;
pub(crate) mod node_registry;
mod online_store_content;
mod orders_payments_fulfillment;
mod phone;
mod privacy;
mod product_helpers;
mod product_operations;
mod product_options;
mod resolved_values;
mod resource_ids;
mod routing;
mod scalar_helpers;
mod search;
mod selling_plans;
mod store_properties;
mod storefront;
mod storefront_cart;
mod storefront_graphql_runtime;
mod url_redirects;
mod validation_helpers;

pub(in crate::proxy) use self::admin_shipping_gift_cards::*;
pub(in crate::proxy) use self::app_shipping_helpers::*;
pub(in crate::proxy) use self::b2b_customers::*;
pub(in crate::proxy) use self::civil_date::*;
pub(in crate::proxy) use self::connection::*;
pub(in crate::proxy) use self::functions::*;
pub(crate) use self::graphql_runtime::{
    field_resolver_registrations, field_resolver_type_policies,
};
pub(in crate::proxy) use self::graphql_runtime::{
    graphql_error_outcome, resolver_http_error_outcome, resolver_outcome_from_upstream_response,
    root_field_errors_from_json,
};
pub(in crate::proxy) use self::json_helpers::*;
pub(in crate::proxy) use self::localization_markets_catalogs::*;
pub(in crate::proxy) use self::marketing_webhooks_inventory::*;
pub(in crate::proxy) use self::markets_catalog_data::*;
pub(in crate::proxy) use self::markets_catalog_helpers::*;
pub(in crate::proxy) use self::media_products_saved_searches::*;
pub(in crate::proxy) use self::metafield_metaobject_definitions::*;
pub(in crate::proxy) use self::metafields_orders_payments::*;
pub(in crate::proxy) use self::metaobjects::metaobject_cursor;
pub(in crate::proxy) use self::money::*;
pub(in crate::proxy) use self::orders_payments_fulfillment::*;
pub(in crate::proxy) use self::phone::*;
pub(in crate::proxy) use self::product_helpers::*;
pub(in crate::proxy) use self::product_options::*;
pub(in crate::proxy) use self::resolved_values::*;
pub(in crate::proxy) use self::resource_ids::*;
pub(in crate::proxy) use self::routing::*;
pub(in crate::proxy) use self::scalar_helpers::*;
pub(in crate::proxy) use self::store_properties::*;
pub(in crate::proxy) use self::validation_helpers::*;

#[cfg(test)]
mod upstream_guard_tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn graphql_request(query: String, api_surface: ApiSurface) -> Request {
        let path = match api_surface {
            ApiSurface::Admin => "/admin/api/2026-04/graphql.json",
            ApiSurface::Storefront => "/api/2026-04/graphql.json",
        };
        Request {
            method: "POST".to_string(),
            path: path.to_string(),
            headers: BTreeMap::new(),
            body: json!({ "query": query }).to_string(),
        }
    }

    #[test]
    fn guarded_upstream_transport_blocks_every_implemented_stage_locally_mutation_root() {
        let mutation_roots = default_registry()
            .into_iter()
            .filter(|entry| entry.implemented && entry.operation_type == OperationType::Mutation)
            .collect::<Vec<_>>();
        assert!(
            !mutation_roots.is_empty(),
            "default registry should expose implemented mutation roots"
        );

        for entry in mutation_roots {
            let forwarded = Arc::new(AtomicUsize::new(0));
            let transport = super::core::guarded_upstream_transport({
                let forwarded = Arc::clone(&forwarded);
                move |_| {
                    forwarded.fetch_add(1, Ordering::SeqCst);
                    ok_json(json!({ "data": { "unexpected": true } }))
                }
            });
            let response = transport(graphql_request(
                format!(
                    "mutation UpstreamSafetyRegression {{ {} {{ __typename }} }}",
                    entry.name
                ),
                entry.api_surface,
            ));

            assert_eq!(
                forwarded.load(Ordering::SeqCst),
                0,
                "{} was forwarded through the upstream transport",
                entry.name
            );
            assert_eq!(response.status, 400, "{} should fail closed", entry.name);
            let message = response.body["errors"][0]["message"]
                .as_str()
                .unwrap_or_default();
            assert!(
                message.contains(&entry.name),
                "blocked response for {} should name the root: {message}",
                entry.name
            );
        }
    }

    #[test]
    fn guarded_upstream_transport_allows_hydration_queries_and_unknown_mutations() {
        let forwarded = Arc::new(AtomicUsize::new(0));
        let transport = super::core::guarded_upstream_transport({
            let forwarded = Arc::clone(&forwarded);
            move |_| {
                forwarded.fetch_add(1, Ordering::SeqCst);
                ok_json(json!({ "data": { "ok": true } }))
            }
        });

        let query_response = transport(graphql_request(
            r#"query HydrateOrderForLocalMutation { order(id: "gid://shopify/Order/1") { id } }"#
                .to_string(),
            ApiSurface::Admin,
        ));
        assert_eq!(query_response.status, 200);
        assert_eq!(forwarded.load(Ordering::SeqCst), 1);

        let unknown_mutation_response = transport(graphql_request(
            "mutation UnsupportedMutationPassthrough { definitelyUnknownRoot { id } }".to_string(),
            ApiSurface::Admin,
        ));
        assert_eq!(unknown_mutation_response.status, 200);
        assert_eq!(forwarded.load(Ordering::SeqCst), 2);
    }
}

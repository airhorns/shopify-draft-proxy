use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::graphql::{
    nested_root_field_path_selection, nested_root_field_selection, parse_operation,
    root_field_arguments, root_field_response_key, root_field_selection, root_fields,
    OperationType, ResolvedValue, RootFieldSelection, SelectedField,
};
use crate::operation_registry::{
    default_registry, operation_capability, CapabilityDomain, CapabilityExecution,
    OperationRegistryEntry,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProductRecord {
    pub id: String,
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
    saved_searches: OrderedRecords<SavedSearchRecord>,
}

#[derive(Clone)]
struct StagedState {
    products: StagedRecords<ProductRecord>,
    saved_searches: StagedRecords<SavedSearchRecord>,
    product_search_tags: BTreeMap<String, BTreeSet<String>>,
    shipping_packages: BTreeMap<String, Value>,
    deleted_shipping_package_ids: BTreeSet<String>,
    customers: BTreeMap<String, Value>,
    deleted_customer_ids: BTreeSet<String>,
    customer_orders: BTreeMap<String, Vec<Value>>,
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
    segments: BTreeMap<String, Value>,
    collections: BTreeMap<String, Value>,
    fulfillment_order_deadlines: BTreeMap<String, String>,
    bulk_operations: BTreeMap<String, Value>,
    timestamp_discounts: BTreeMap<String, Value>,
    gift_cards: BTreeMap<String, Value>,
    markets: BTreeMap<String, Value>,
    catalogs: BTreeMap<String, Value>,
    price_lists: BTreeMap<String, Value>,
    web_presences: BTreeMap<String, Value>,
    shop_locales: BTreeMap<String, Value>,
    localization_translations: Vec<Value>,
    marketing_activities: BTreeMap<String, Value>,
    deleted_marketing_activity_ids: BTreeSet<String>,
    marketing_delete_all_external: bool,
    webhook_subscriptions: BTreeMap<String, Value>,
    b2b_companies: BTreeMap<String, Value>,
    b2b_locations: BTreeMap<String, Value>,
    next_b2b_company_id: u64,
    inventory_levels: BTreeMap<(String, String), BTreeMap<String, i64>>,
    metaobjects: BTreeMap<String, Value>,
    deleted_metaobject_ids: BTreeSet<String>,
    app_metafields: BTreeMap<(String, String, String), Value>,
    owner_metafields: BTreeMap<String, Vec<Value>>,
    metafield_definitions: BTreeMap<(String, String), Value>,
    media_files: BTreeMap<String, Value>,
    deleted_media_file_ids: BTreeSet<String>,
    online_store_integrations: BTreeMap<String, Value>,
    product_set_updated: bool,
    product_option_fixture: Option<String>,
    product_metafields_fixture: Option<String>,
    product_delete_operations: BTreeMap<String, String>,
    selling_plan_group_downstream_step: usize,
    return_status: Option<String>,
    recorded_return_statuses: BTreeMap<String, String>,
    mandate_payment_keys: BTreeSet<String>,
    payment_terms_ids: BTreeSet<String>,
    payment_reminder_schedule_ids: BTreeSet<String>,
    payment_customizations: BTreeMap<String, Value>,
    draft_orders: BTreeMap<String, Value>,
    next_draft_order_id: u64,
    draft_order_tags: BTreeMap<String, Vec<String>>,
    next_draft_order_bulk_tag_job_id: u64,
    draft_order_complete_gateway_create_count: usize,
    order_customer_orders: BTreeMap<String, Value>,
    order_customer_cancelled_ids: BTreeSet<String>,
    order_customer_b2b_order_ids: BTreeSet<String>,
    order_customer_contact_customer_ids: BTreeSet<String>,
    next_order_customer_order_id: u64,
    order_payment_transaction_state: Option<String>,
    order_edit_existing_mode: Option<String>,
    function_validation: Option<Value>,
    function_cart_transform: Option<Value>,
    code_basic_lifecycle_status: Option<String>,
    free_shipping_code_status: Option<String>,
    free_shipping_automatic_status: Option<String>,
    redeem_code_bulk_live_added: bool,
    redeem_code_bulk_live_deleted_seed: bool,
    backup_region: Value,
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
            saved_searches: StagedRecords::default(),
            product_search_tags: BTreeMap::new(),
            shipping_packages: BTreeMap::new(),
            deleted_shipping_package_ids: BTreeSet::new(),
            customers: BTreeMap::new(),
            deleted_customer_ids: BTreeSet::new(),
            customer_orders: BTreeMap::new(),
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
            segments: BTreeMap::new(),
            collections: BTreeMap::new(),
            fulfillment_order_deadlines: BTreeMap::new(),
            bulk_operations: BTreeMap::new(),
            timestamp_discounts: BTreeMap::new(),
            gift_cards: BTreeMap::new(),
            markets: BTreeMap::new(),
            catalogs: BTreeMap::new(),
            price_lists: BTreeMap::new(),
            web_presences: BTreeMap::new(),
            shop_locales: BTreeMap::new(),
            localization_translations: Vec::new(),
            marketing_activities: BTreeMap::new(),
            deleted_marketing_activity_ids: BTreeSet::new(),
            marketing_delete_all_external: false,
            webhook_subscriptions: BTreeMap::new(),
            b2b_companies: BTreeMap::new(),
            b2b_locations: BTreeMap::new(),
            next_b2b_company_id: 1,
            inventory_levels: BTreeMap::new(),
            metaobjects: BTreeMap::new(),
            deleted_metaobject_ids: BTreeSet::new(),
            app_metafields: BTreeMap::new(),
            owner_metafields: BTreeMap::new(),
            metafield_definitions: BTreeMap::new(),
            media_files: BTreeMap::new(),
            deleted_media_file_ids: BTreeSet::new(),
            online_store_integrations: BTreeMap::new(),
            product_set_updated: false,
            product_option_fixture: None,
            product_metafields_fixture: None,
            product_delete_operations: BTreeMap::new(),
            selling_plan_group_downstream_step: 0,
            return_status: None,
            recorded_return_statuses: BTreeMap::new(),
            mandate_payment_keys: BTreeSet::new(),
            payment_terms_ids: BTreeSet::new(),
            payment_reminder_schedule_ids: BTreeSet::new(),
            payment_customizations: BTreeMap::new(),
            draft_orders: BTreeMap::new(),
            next_draft_order_id: 1,
            draft_order_tags: BTreeMap::new(),
            next_draft_order_bulk_tag_job_id: 1,
            draft_order_complete_gateway_create_count: 0,
            order_customer_orders: BTreeMap::new(),
            order_customer_cancelled_ids: BTreeSet::new(),
            order_customer_b2b_order_ids: BTreeSet::new(),
            order_customer_contact_customer_ids: BTreeSet::new(),
            next_order_customer_order_id: 1,
            order_payment_transaction_state: None,
            order_edit_existing_mode: None,
            function_validation: None,
            function_cart_transform: None,
            code_basic_lifecycle_status: None,
            free_shipping_code_status: None,
            free_shipping_automatic_status: None,
            redeem_code_bulk_live_added: false,
            redeem_code_bulk_live_deleted_seed: false,
            backup_region: backup_region_country("CA"),
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

impl Store {
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

    fn replace_staged_products_map_with_order(
        &mut self,
        products: BTreeMap<String, ProductRecord>,
        order: Vec<String>,
    ) {
        self.staged.products.replace_with_order(products, order);
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

    fn replace_saved_search_tombstones(&mut self, ids: BTreeSet<String>) {
        self.staged.saved_searches.tombstones = ids;
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

    fn has_product(&self, id: &str) -> bool {
        self.product_by_id(id).is_some()
    }

    fn stage_product(&mut self, product: ProductRecord) {
        self.staged.products.stage(product.id.clone(), product);
    }

    fn delete_product(&mut self, id: &str) {
        self.staged.products.remove_staged(id);
        self.staged.products.tombstone(id.to_string());
    }

    fn product_staged_or_base(&self, id: &str) -> Option<ProductRecord> {
        self.product_by_id(id).cloned()
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
mod core;
mod discounts;
mod dispatch;
mod localization_markets_catalogs;
mod marketing_webhooks_inventory;
mod markets_online_inventory;
mod media_products_saved_searches;
mod metafields_orders_payments;
mod online_store_orders_payments;
mod product_helpers;
mod routing;

#[allow(unused_imports)]
pub(in crate::proxy) use self::admin_shipping_gift_cards::*;
#[allow(unused_imports)]
pub(in crate::proxy) use self::app_shipping_helpers::*;
#[allow(unused_imports)]
pub(in crate::proxy) use self::b2b_customers::*;
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
pub(in crate::proxy) use self::metafields_orders_payments::*;
#[allow(unused_imports)]
pub(in crate::proxy) use self::online_store_orders_payments::*;
#[allow(unused_imports)]
pub(in crate::proxy) use self::product_helpers::*;
#[allow(unused_imports)]
pub(in crate::proxy) use self::routing::*;

#[cfg(test)]
mod store_tests {
    use super::*;

    fn product(id: &str, title: &str, handle: &str) -> ProductRecord {
        ProductRecord {
            id: id.to_string(),
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
}

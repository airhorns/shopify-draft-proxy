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
    base_products: BTreeMap<String, ProductRecord>,
    staged_products: BTreeMap<String, ProductRecord>,
    staged_product_search_tags: BTreeMap<String, BTreeSet<String>>,
    staged_deleted_product_ids: BTreeSet<String>,
    staged_saved_searches: BTreeMap<String, SavedSearchRecord>,
    staged_deleted_saved_search_ids: BTreeSet<String>,
    staged_shipping_packages: BTreeMap<String, Value>,
    staged_deleted_shipping_package_ids: BTreeSet<String>,
    staged_customers: BTreeMap<String, Value>,
    staged_deleted_customer_ids: BTreeSet<String>,
    staged_customer_orders: BTreeMap<String, Vec<Value>>,
    staged_carrier_services: BTreeMap<String, Value>,
    staged_deleted_carrier_service_ids: BTreeSet<String>,
    staged_app_subscriptions: BTreeMap<String, Value>,
    staged_app_one_time_purchases: BTreeMap<String, Value>,
    revoked_app_access_scopes: BTreeSet<String>,
    app_uninstalled: bool,
    staged_delegate_access_tokens: BTreeMap<String, Value>,
    staged_customer_segment_member_queries: BTreeMap<String, Value>,
    staged_fulfillment_services: BTreeMap<String, Value>,
    staged_fulfillment_service_locations: BTreeMap<String, Value>,
    staged_deleted_fulfillment_service_ids: BTreeSet<String>,
    staged_deleted_fulfillment_service_location_ids: BTreeSet<String>,
    staged_segments: BTreeMap<String, Value>,
    staged_collections: BTreeMap<String, Value>,
    staged_fulfillment_order_deadlines: BTreeMap<String, String>,
    staged_bulk_operations: BTreeMap<String, Value>,
    staged_timestamp_discounts: BTreeMap<String, Value>,
    staged_gift_cards: BTreeMap<String, Value>,
    staged_markets: BTreeMap<String, Value>,
    staged_catalogs: BTreeMap<String, Value>,
    staged_price_lists: BTreeMap<String, Value>,
    staged_web_presences: BTreeMap<String, Value>,
    staged_shop_locales: BTreeMap<String, Value>,
    staged_localization_translations: Vec<Value>,
    staged_marketing_activities: BTreeMap<String, Value>,
    staged_deleted_marketing_activity_ids: BTreeSet<String>,
    staged_marketing_delete_all_external: bool,
    staged_webhook_subscriptions: BTreeMap<String, Value>,
    staged_b2b_companies: BTreeMap<String, Value>,
    staged_b2b_locations: BTreeMap<String, Value>,
    next_b2b_company_id: u64,
    staged_inventory_levels: BTreeMap<(String, String), BTreeMap<String, i64>>,
    staged_metaobjects: BTreeMap<String, Value>,
    staged_deleted_metaobject_ids: BTreeSet<String>,
    staged_app_metafields: BTreeMap<(String, String, String), Value>,
    staged_owner_metafields: BTreeMap<String, Vec<Value>>,
    staged_metafield_definitions: BTreeMap<(String, String), Value>,
    staged_media_files: BTreeMap<String, Value>,
    staged_deleted_media_file_ids: BTreeSet<String>,
    staged_online_store_integrations: BTreeMap<String, Value>,
    staged_product_set_updated: bool,
    staged_product_option_fixture: Option<String>,
    staged_product_metafields_fixture: Option<String>,
    staged_product_delete_operations: BTreeMap<String, String>,
    staged_selling_plan_group_downstream_step: usize,
    staged_return_status: Option<String>,
    staged_recorded_return_statuses: BTreeMap<String, String>,
    staged_mandate_payment_keys: BTreeSet<String>,
    staged_payment_terms_ids: BTreeSet<String>,
    staged_payment_reminder_schedule_ids: BTreeSet<String>,
    staged_payment_customizations: BTreeMap<String, Value>,
    staged_draft_orders: BTreeMap<String, Value>,
    next_draft_order_id: u64,
    staged_draft_order_tags: BTreeMap<String, Vec<String>>,
    next_draft_order_bulk_tag_job_id: u64,
    staged_draft_order_complete_gateway_create_count: usize,
    staged_order_customer_orders: BTreeMap<String, Value>,
    staged_order_customer_cancelled_ids: BTreeSet<String>,
    staged_order_customer_b2b_order_ids: BTreeSet<String>,
    staged_order_customer_contact_customer_ids: BTreeSet<String>,
    next_order_customer_order_id: u64,
    staged_order_payment_transaction_state: Option<String>,
    staged_order_edit_existing_mode: Option<String>,
    staged_function_validation: Option<Value>,
    staged_function_cart_transform: Option<Value>,
    staged_code_basic_lifecycle_status: Option<String>,
    staged_free_shipping_code_status: Option<String>,
    staged_free_shipping_automatic_status: Option<String>,
    staged_redeem_code_bulk_live_added: bool,
    staged_redeem_code_bulk_live_deleted_seed: bool,
    backup_region: Value,
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
